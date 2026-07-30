//! Crash-safe authentication state for the user-operated Remote Agent.
//!
//! Pairing tokens and control-plane credentials are generated locally from
//! operating-system randomness. Only domain-separated SHA-256 digests are
//! persisted. The plaintext values exist only in zeroizing process memory and
//! are returned once to the operator or pairing client. Request replay claims
//! and the globally increasing response sequence are committed in the same
//! state file so a restart cannot reset either security boundary.

use std::{
    collections::HashSet,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use verisilo_remote_backend::{
    TlsPin, TlsPinRotationPairingClaim, MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS,
};
use zeroize::Zeroizing;

const AUTH_SCHEMA_VERSION: u32 = 1;
const MAX_AUTH_STATE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CREDENTIALS: usize = 64;
const MAX_PAIRING_REPLAY_CLAIMS: usize = 4_096;
const MAX_OPERATION_REPLAY_CLAIMS: usize = 4_096;
const MAX_PIN_ROTATION_AUTHORIZATIONS: usize = 64;
const MIN_SECRET_LIFETIME_MS: u64 = 1_000;
pub const MAX_PAIRING_TOKEN_LIFETIME_MS: u64 = 5 * 60 * 1_000;
pub const MAX_CONTROL_CREDENTIAL_LIFETIME_MS: u64 = 24 * 60 * 60 * 1_000;
const PAIRING_HASH_DOMAIN: &[u8] = b"verisilo-remote-auth-v1\0pairing-token\0";
const CREDENTIAL_HASH_DOMAIN: &[u8] = b"verisilo-remote-auth-v1\0control-credential\0";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedAuthState {
    schema_version: u32,
    server_id: Uuid,
    response_sequence: u64,
    pairing_token: Option<PairingTokenRecord>,
    credentials: Vec<CredentialRecord>,
    pairing_claims: Vec<PairingReplayClaim>,
    operation_claims: Vec<OperationReplayClaim>,
    #[serde(default)]
    pin_rotation_authorizations: Vec<PinRotationAuthorizationRecord>,
}

impl PersistedAuthState {
    fn new() -> Self {
        Self {
            schema_version: AUTH_SCHEMA_VERSION,
            server_id: Uuid::new_v4(),
            response_sequence: 0,
            pairing_token: None,
            credentials: Vec::new(),
            pairing_claims: Vec::new(),
            operation_claims: Vec::new(),
            pin_rotation_authorizations: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairingTokenRecord {
    token_id: Uuid,
    digest_sha256: String,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialRecord {
    credential_id: Uuid,
    digest_sha256: String,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    revoked: bool,
    last_request_sequence: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairingReplayClaim {
    request_id: Uuid,
    nonce: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationReplayClaim {
    credential_id: Uuid,
    request_id: Uuid,
    nonce: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PinRotationAuthorizationRecord {
    challenge: String,
    server_id: Uuid,
    credential_id: Uuid,
    authorization_request_id: Uuid,
    authorization_request_nonce: String,
    authorization_request_sequence: u64,
    authorization_response_sequence: u64,
    pairing_token_id: Uuid,
    new_pin: TlsPin,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}

/// A pairing token whose secret must be shown to the operator exactly once.
pub struct IssuedPairingToken {
    token_id: Uuid,
    secret: Zeroizing<String>,
    expires_at_unix_ms: u64,
}

impl IssuedPairingToken {
    pub fn token_id(&self) -> Uuid {
        self.token_id
    }

    pub fn secret(&self) -> &str {
        self.secret.as_str()
    }

    pub fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }
}

impl std::fmt::Debug for IssuedPairingToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssuedPairingToken")
            .field("token_id", &self.token_id)
            .field("secret", &"[REDACTED]")
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .finish()
    }
}

/// A newly paired control credential and the response sequence reserved by the
/// same durable transaction.
pub struct PairingGrant {
    pub credential_id: Uuid,
    pub credential: Zeroizing<String>,
    pub credential_expires_at_unix_ms: u64,
    pub response_sequence: u64,
}

impl std::fmt::Debug for PairingGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PairingGrant")
            .field("credential_id", &self.credential_id)
            .field("credential", &"[REDACTED]")
            .field(
                "credential_expires_at_unix_ms",
                &self.credential_expires_at_unix_ms,
            )
            .field("response_sequence", &self.response_sequence)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticatedOperation {
    pub credential_id: Uuid,
    pub response_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRotationAuthorizationGrant {
    pub credential_id: Uuid,
    pub response_sequence: u64,
    pub challenge: String,
    pub expires_at_unix_ms: u64,
}

/// Exclusive, process-local handle to the authentication state file.
pub struct AuthStore {
    path: PathBuf,
    state: PersistedAuthState,
    _lock_file: File,
    poisoned: bool,
}

impl AuthStore {
    /// Opens or creates an auth state file at a canonical absolute path.
    ///
    /// The parent directory must already exist. On Unix the state and lock
    /// files are forced to mode `0600`, and an advisory exclusive lock prevents
    /// two daemon processes from racing state transitions.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AuthStoreError> {
        let path = validate_state_path(path.into())?;
        let lock_path = sibling_with_suffix(&path, ".lock");
        reject_symlink_if_present(&lock_path)?;
        let lock_file = open_lock_file(&lock_path)?;
        acquire_exclusive_lock(&lock_file)?;
        recover_interrupted_write(&path)?;

        let state = if path.exists() {
            read_state_file(&path)?
        } else {
            PersistedAuthState::new()
        };
        validate_state(&state)?;

        let store = Self {
            path,
            state,
            _lock_file: lock_file,
            poisoned: false,
        };
        if !store.path.exists() {
            store.persist(&store.state)?;
        }
        Ok(store)
    }

    pub fn server_id(&self) -> Uuid {
        self.state.server_id
    }

    pub fn current_response_sequence(&self) -> u64 {
        self.state.response_sequence
    }

    /// Replaces any previous unconsumed pairing token.
    pub fn issue_pairing_token(
        &mut self,
        now_unix_ms: u64,
        lifetime_ms: u64,
    ) -> Result<IssuedPairingToken, AuthStoreError> {
        validate_lifetime(lifetime_ms, MAX_PAIRING_TOKEN_LIFETIME_MS, "pairing token")?;
        let expires_at_unix_ms = now_unix_ms
            .checked_add(lifetime_ms)
            .ok_or(AuthStoreError::InvalidLifetime("pairing token"))?;
        let token_id = Uuid::new_v4();
        let secret = new_high_entropy_secret();
        let digest_sha256 = hash_secret_hex(PAIRING_HASH_DOMAIN, secret.as_bytes());

        self.mutate(|state| {
            state.pairing_token = Some(PairingTokenRecord {
                token_id,
                digest_sha256,
                issued_at_unix_ms: now_unix_ms,
                expires_at_unix_ms,
            });
            state.pin_rotation_authorizations.clear();
            Ok(())
        })?;

        Ok(IssuedPairingToken {
            token_id,
            secret,
            expires_at_unix_ms,
        })
    }

    /// Atomically consumes the one-time pairing token, records pairing replay
    /// identifiers, creates a hashed control credential, and reserves the
    /// response sequence returned with the pairing success.
    #[allow(clippy::too_many_arguments)]
    pub fn redeem_pairing_token(
        &mut self,
        request_id: Uuid,
        nonce: &str,
        token_id: Uuid,
        token_secret: &str,
        advertised_expires_at_unix_ms: u64,
        now_unix_ms: u64,
        credential_lifetime_ms: u64,
    ) -> Result<PairingGrant, AuthStoreError> {
        self.redeem_pairing_token_inner(
            request_id,
            nonce,
            token_id,
            token_secret,
            advertised_expires_at_unix_ms,
            now_unix_ms,
            credential_lifetime_ms,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn redeem_pairing_token_for_rotation(
        &mut self,
        request_id: Uuid,
        nonce: &str,
        token_id: Uuid,
        token_secret: &str,
        advertised_expires_at_unix_ms: u64,
        now_unix_ms: u64,
        credential_lifetime_ms: u64,
        rotation: &TlsPinRotationPairingClaim,
    ) -> Result<PairingGrant, AuthStoreError> {
        self.redeem_pairing_token_inner(
            request_id,
            nonce,
            token_id,
            token_secret,
            advertised_expires_at_unix_ms,
            now_unix_ms,
            credential_lifetime_ms,
            Some(rotation),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn redeem_pairing_token_inner(
        &mut self,
        request_id: Uuid,
        nonce: &str,
        token_id: Uuid,
        token_secret: &str,
        advertised_expires_at_unix_ms: u64,
        now_unix_ms: u64,
        credential_lifetime_ms: u64,
        rotation: Option<&TlsPinRotationPairingClaim>,
    ) -> Result<PairingGrant, AuthStoreError> {
        validate_request_identity(request_id, nonce)?;
        validate_secret(token_secret)?;
        validate_lifetime(
            credential_lifetime_ms,
            MAX_CONTROL_CREDENTIAL_LIFETIME_MS,
            "control credential",
        )?;

        let credential_expires_at_unix_ms = now_unix_ms
            .checked_add(credential_lifetime_ms)
            .ok_or(AuthStoreError::InvalidLifetime("control credential"))?;
        let supplied_digest = hash_secret(PAIRING_HASH_DOMAIN, token_secret.as_bytes());
        let credential_id = Uuid::new_v4();
        let credential = new_high_entropy_secret();
        let credential_digest = hash_secret_hex(CREDENTIAL_HASH_DOMAIN, credential.as_bytes());
        let mut prospective = self.state.clone();

        if prospective
            .pairing_claims
            .iter()
            .any(|claim| claim.request_id == request_id || claim.nonce == nonce)
        {
            return Err(AuthStoreError::Replay);
        }
        let token = prospective
            .pairing_token
            .as_ref()
            .ok_or(AuthStoreError::PairingTokenInvalid)?;
        if token.expires_at_unix_ms <= now_unix_ms {
            return Err(AuthStoreError::PairingTokenExpired);
        }
        let expected_digest = decode_digest(&token.digest_sha256)?;
        if token.token_id != token_id
            || token.expires_at_unix_ms != advertised_expires_at_unix_ms
            || !constant_time_eq(&expected_digest, &supplied_digest)
        {
            return Err(AuthStoreError::PairingTokenInvalid);
        }
        let pending_rotation = prospective
            .pin_rotation_authorizations
            .iter()
            .find(|authorization| authorization.pairing_token_id == token_id);
        match (pending_rotation, rotation) {
            (None, None) => {}
            (Some(authorization), Some(claim))
                if authorization.expires_at_unix_ms > now_unix_ms
                    && claim.server_id == prospective.server_id
                    && claim.server_id == authorization.server_id
                    && claim.pairing_token_id == token_id
                    && claim.challenge == authorization.challenge
                    && claim.old_client_credential_id == authorization.credential_id
                    && claim.authorization_request_id == authorization.authorization_request_id
                    && claim.authorization_request_nonce
                        == authorization.authorization_request_nonce
                    && claim.authorization_request_sequence
                        == authorization.authorization_request_sequence
                    && claim.authorization_response_sequence
                        == authorization.authorization_response_sequence
                    && claim.authorization_expires_at_unix_ms
                        == authorization.expires_at_unix_ms
                    && claim.new_pin == authorization.new_pin
                    && prospective.credentials.iter().any(|credential| {
                        credential.credential_id == authorization.credential_id
                            && !credential.revoked
                            && credential.expires_at_unix_ms > now_unix_ms
                    }) => {}
            _ => return Err(AuthStoreError::PinRotationAuthorizationInvalid),
        }

        prospective
            .credentials
            .retain(|item| !item.revoked && item.expires_at_unix_ms > now_unix_ms);
        let active_credential_ids = prospective
            .credentials
            .iter()
            .map(|credential| credential.credential_id)
            .collect::<HashSet<_>>();
        prospective
            .operation_claims
            .retain(|claim| active_credential_ids.contains(&claim.credential_id));
        if prospective.credentials.len() >= MAX_CREDENTIALS {
            return Err(AuthStoreError::CredentialLimitReached);
        }

        let response_sequence = increment_response_sequence(&mut prospective)?;
        prospective.pairing_token = None;
        prospective
            .pin_rotation_authorizations
            .retain(|authorization| authorization.pairing_token_id != token_id);
        if prospective.pairing_claims.len() == MAX_PAIRING_REPLAY_CLAIMS {
            // A consumed token can never become valid again. Keeping the most
            // recent bounded window improves diagnostics without allowing
            // normal, operator-approved pairings to permanently lock the
            // service after 4,096 rotations.
            prospective.pairing_claims.remove(0);
        }
        prospective.pairing_claims.push(PairingReplayClaim {
            request_id,
            nonce: nonce.to_owned(),
        });
        prospective.credentials.push(CredentialRecord {
            credential_id,
            digest_sha256: credential_digest,
            issued_at_unix_ms: now_unix_ms,
            expires_at_unix_ms: credential_expires_at_unix_ms,
            revoked: false,
            last_request_sequence: 0,
        });
        self.commit(prospective)?;

        Ok(PairingGrant {
            credential_id,
            credential,
            credential_expires_at_unix_ms,
            response_sequence,
        })
    }

    /// Atomically authenticates the old bearer, consumes its request sequence,
    /// verifies the referenced unconsumed pairing token ID, and persists one
    /// short-lived challenge bound to the proposed new pin.
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_pin_rotation(
        &mut self,
        bearer: &str,
        request_id: Uuid,
        nonce: &str,
        request_sequence: u64,
        expected_credential_id: Uuid,
        pairing_token_id: Uuid,
        new_pin: &TlsPin,
        now_unix_ms: u64,
    ) -> Result<PinRotationAuthorizationGrant, AuthStoreError> {
        validate_secret(bearer)?;
        validate_request_identity(request_id, nonce)?;
        new_pin
            .validate()
            .map_err(|_| AuthStoreError::InvalidRequestIdentity)?;
        if request_sequence == 0
            || expected_credential_id == Uuid::nil()
            || pairing_token_id == Uuid::nil()
        {
            return Err(AuthStoreError::InvalidRequestIdentity);
        }

        let supplied_digest = hash_secret(CREDENTIAL_HASH_DOMAIN, bearer.as_bytes());
        let mut prospective = self.state.clone();
        let credential_index = matching_credential_index(&prospective, &supplied_digest)?;
        let credential = &prospective.credentials[credential_index];
        if credential.revoked {
            return Err(AuthStoreError::CredentialRevoked);
        }
        if credential.expires_at_unix_ms <= now_unix_ms {
            return Err(AuthStoreError::CredentialExpired);
        }
        if request_sequence <= credential.last_request_sequence
            || prospective
                .operation_claims
                .iter()
                .any(|claim| claim.request_id == request_id || claim.nonce == nonce)
        {
            return Err(AuthStoreError::Replay);
        }
        let token = prospective
            .pairing_token
            .as_ref()
            .ok_or(AuthStoreError::PairingTokenInvalid)?;
        if token.token_id != pairing_token_id || token.expires_at_unix_ms <= now_unix_ms {
            return Err(AuthStoreError::PairingTokenInvalid);
        }

        let credential_id = credential.credential_id;
        if credential_id != expected_credential_id {
            return Err(AuthStoreError::CredentialInvalid);
        }
        prospective.credentials[credential_index].last_request_sequence = request_sequence;
        if prospective.operation_claims.len() == MAX_OPERATION_REPLAY_CLAIMS {
            prospective.operation_claims.remove(0);
        }
        prospective.operation_claims.push(OperationReplayClaim {
            credential_id,
            request_id,
            nonce: nonce.to_owned(),
        });
        prospective
            .pin_rotation_authorizations
            .retain(|authorization| {
                authorization.expires_at_unix_ms > now_unix_ms
                    && authorization.pairing_token_id != pairing_token_id
            });
        if prospective.pin_rotation_authorizations.len() >= MAX_PIN_ROTATION_AUTHORIZATIONS {
            return Err(AuthStoreError::StateLimitExceeded);
        }
        let challenge = fresh_nonce();
        let expires_at_unix_ms = now_unix_ms
            .checked_add(MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS)
            .ok_or(AuthStoreError::InvalidLifetime(
                "pin rotation authorization",
            ))?;
        let response_sequence = increment_response_sequence(&mut prospective)?;
        let server_id = prospective.server_id;
        prospective
            .pin_rotation_authorizations
            .push(PinRotationAuthorizationRecord {
                challenge: challenge.clone(),
                server_id,
                credential_id,
                authorization_request_id: request_id,
                authorization_request_nonce: nonce.to_owned(),
                authorization_request_sequence: request_sequence,
                authorization_response_sequence: response_sequence,
                pairing_token_id,
                new_pin: new_pin.clone(),
                issued_at_unix_ms: now_unix_ms,
                expires_at_unix_ms,
            });
        self.commit(prospective)?;
        Ok(PinRotationAuthorizationGrant {
            credential_id,
            response_sequence,
            challenge,
            expires_at_unix_ms,
        })
    }

    /// Authenticates and atomically consumes an operation request identity and
    /// client sequence. The returned response sequence is already durable, so
    /// a rejected binding or unavailable capability cannot make the client
    /// sequence reusable.
    pub fn authenticate_operation(
        &mut self,
        bearer: &str,
        request_id: Uuid,
        nonce: &str,
        request_sequence: u64,
        now_unix_ms: u64,
    ) -> Result<AuthenticatedOperation, AuthStoreError> {
        validate_secret(bearer)?;
        validate_request_identity(request_id, nonce)?;
        if request_sequence == 0 {
            return Err(AuthStoreError::InvalidRequestIdentity);
        }
        let supplied_digest = hash_secret(CREDENTIAL_HASH_DOMAIN, bearer.as_bytes());
        let mut prospective = self.state.clone();
        let credential_index = matching_credential_index(&prospective, &supplied_digest)?;
        let credential = &prospective.credentials[credential_index];
        if credential.revoked {
            return Err(AuthStoreError::CredentialRevoked);
        }
        if credential.expires_at_unix_ms <= now_unix_ms {
            return Err(AuthStoreError::CredentialExpired);
        }
        if request_sequence <= credential.last_request_sequence
            || prospective
                .operation_claims
                .iter()
                .any(|claim| claim.request_id == request_id || claim.nonce == nonce)
        {
            return Err(AuthStoreError::Replay);
        }

        let credential_id = credential.credential_id;
        prospective.credentials[credential_index].last_request_sequence = request_sequence;
        if prospective.operation_claims.len() == MAX_OPERATION_REPLAY_CLAIMS {
            // `last_request_sequence` is the authoritative, persistent replay
            // barrier for each credential. Evicting the oldest request-id/
            // nonce diagnostic claim cannot make an old request executable,
            // because its sequence remains <= the credential's high-water
            // mark. This keeps normal traffic from exhausting the service.
            prospective.operation_claims.remove(0);
        }
        prospective.operation_claims.push(OperationReplayClaim {
            credential_id,
            request_id,
            nonce: nonce.to_owned(),
        });
        let response_sequence = increment_response_sequence(&mut prospective)?;
        self.commit(prospective)?;
        Ok(AuthenticatedOperation {
            credential_id,
            response_sequence,
        })
    }

    /// Performs a read-only bearer check. Request routing should normally use
    /// [`Self::authenticate_operation`] so replay and sequence state is claimed.
    pub fn authenticate(&self, bearer: &str, now_unix_ms: u64) -> Result<Uuid, AuthStoreError> {
        validate_secret(bearer)?;
        let supplied_digest = hash_secret(CREDENTIAL_HASH_DOMAIN, bearer.as_bytes());
        let index = matching_credential_index(&self.state, &supplied_digest)?;
        let credential = &self.state.credentials[index];
        if credential.revoked {
            return Err(AuthStoreError::CredentialRevoked);
        }
        if credential.expires_at_unix_ms <= now_unix_ms {
            return Err(AuthStoreError::CredentialExpired);
        }
        Ok(credential.credential_id)
    }

    pub fn has_active_credentials(&self, now_unix_ms: u64) -> bool {
        self.state
            .credentials
            .iter()
            .any(|item| !item.revoked && item.expires_at_unix_ms > now_unix_ms)
    }

    pub fn revoke_credential(&mut self, credential_id: Uuid) -> Result<(), AuthStoreError> {
        self.mutate(|state| {
            let credential = state
                .credentials
                .iter_mut()
                .find(|item| item.credential_id == credential_id)
                .ok_or(AuthStoreError::CredentialInvalid)?;
            credential.revoked = true;
            Ok(())
        })
    }

    pub fn revoke_all_credentials(&mut self) -> Result<usize, AuthStoreError> {
        let mut prospective = self.state.clone();
        let mut revoked = 0;
        for credential in &mut prospective.credentials {
            if !credential.revoked {
                credential.revoked = true;
                revoked += 1;
            }
        }
        self.commit(prospective)?;
        Ok(revoked)
    }

    /// Reserves the next global response sequence for a protocol rejection
    /// that did not authenticate an operation request.
    pub fn reserve_response_sequence(&mut self) -> Result<u64, AuthStoreError> {
        let mut prospective = self.state.clone();
        let sequence = increment_response_sequence(&mut prospective)?;
        self.commit(prospective)?;
        Ok(sequence)
    }

    fn mutate(
        &mut self,
        change: impl FnOnce(&mut PersistedAuthState) -> Result<(), AuthStoreError>,
    ) -> Result<(), AuthStoreError> {
        let mut prospective = self.state.clone();
        change(&mut prospective)?;
        self.commit(prospective)
    }

    fn commit(&mut self, prospective: PersistedAuthState) -> Result<(), AuthStoreError> {
        if self.poisoned {
            return Err(AuthStoreError::StatePoisoned);
        }
        validate_state(&prospective)?;
        if let Err(error) = self.persist(&prospective) {
            // A rename may have succeeded even if a later chmod/directory-fsync
            // failed. Continuing with the old in-memory sequence could then
            // reuse a response number. Fail closed until restart reloads the
            // authoritative disk image.
            self.poisoned = true;
            return Err(error);
        }
        self.state = prospective;
        Ok(())
    }

    fn persist(&self, state: &PersistedAuthState) -> Result<(), AuthStoreError> {
        let raw = serde_json::to_vec(state).map_err(AuthStoreError::Json)?;
        if raw.len() as u64 > MAX_AUTH_STATE_BYTES {
            return Err(AuthStoreError::StateLimitExceeded);
        }
        let temporary = sibling_with_suffix(&self.path, ".tmp");
        reject_symlink_if_present(&temporary)?;
        if temporary.exists() {
            fs::remove_file(&temporary).map_err(AuthStoreError::Io)?;
        }

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(AuthStoreError::Io)?;
        if let Err(error) = file.write_all(&raw).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&temporary);
            return Err(AuthStoreError::Io(error));
        }
        drop(file);

        #[cfg(unix)]
        {
            fs::rename(&temporary, &self.path).map_err(AuthStoreError::Io)?;
            set_mode_0600(&self.path)?;
            sync_parent(&self.path)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = fs::remove_file(&temporary);
            Err(AuthStoreError::UnsupportedPlatform)
        }
    }
}

#[derive(Debug, Error)]
pub enum AuthStoreError {
    #[error("authentication state path must be canonical and absolute")]
    InvalidPath,
    #[error("authentication state path is a symbolic link or non-regular file")]
    UnsafeFile,
    #[error("another Remote Agent process already holds the authentication state")]
    AlreadyLocked,
    #[error("authentication state is not supported on this platform")]
    UnsupportedPlatform,
    #[error("authentication state I/O failed")]
    Io(#[source] std::io::Error),
    #[error("authentication state JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("authentication state schema or invariant is invalid")]
    InvalidState,
    #[error("authentication state durability is uncertain; restart is required")]
    StatePoisoned,
    #[error("authentication state exceeds its fixed size or item limit")]
    StateLimitExceeded,
    #[error("{0} lifetime is outside the allowed bound")]
    InvalidLifetime(&'static str),
    #[error("pairing token is invalid")]
    PairingTokenInvalid,
    #[error("pairing token has expired")]
    PairingTokenExpired,
    #[error("TLS pin rotation authorization is invalid, expired, or already consumed")]
    PinRotationAuthorizationInvalid,
    #[error("request replay detected")]
    Replay,
    #[error("control credential limit reached")]
    CredentialLimitReached,
    #[error("control credential is invalid")]
    CredentialInvalid,
    #[error("control credential has expired")]
    CredentialExpired,
    #[error("control credential was revoked")]
    CredentialRevoked,
    #[error("request identity is invalid")]
    InvalidRequestIdentity,
    #[error("global response sequence is exhausted")]
    ResponseSequenceExhausted,
}

/// Generates a 122-bit response nonce using the UUID crate's OS-backed V4 RNG.
pub(crate) fn fresh_nonce() -> String {
    Uuid::new_v4().simple().to_string()
}

fn new_high_entropy_secret() -> Zeroizing<String> {
    let mut value = String::with_capacity(64);
    value.push_str(&Uuid::new_v4().simple().to_string());
    value.push_str(&Uuid::new_v4().simple().to_string());
    Zeroizing::new(value)
}

fn validate_lifetime(
    lifetime_ms: u64,
    maximum_ms: u64,
    label: &'static str,
) -> Result<(), AuthStoreError> {
    if !(MIN_SECRET_LIFETIME_MS..=maximum_ms).contains(&lifetime_ms) {
        return Err(AuthStoreError::InvalidLifetime(label));
    }
    Ok(())
}

fn validate_secret(secret: &str) -> Result<(), AuthStoreError> {
    if !(32..=512).contains(&secret.len())
        || !secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(AuthStoreError::CredentialInvalid);
    }
    Ok(())
}

fn validate_request_identity(request_id: Uuid, nonce: &str) -> Result<(), AuthStoreError> {
    if request_id == Uuid::nil()
        || !(32..=128).contains(&nonce.len())
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(AuthStoreError::InvalidRequestIdentity);
    }
    Ok(())
}

fn increment_response_sequence(state: &mut PersistedAuthState) -> Result<u64, AuthStoreError> {
    state.response_sequence = state
        .response_sequence
        .checked_add(1)
        .ok_or(AuthStoreError::ResponseSequenceExhausted)?;
    Ok(state.response_sequence)
}

fn matching_credential_index(
    state: &PersistedAuthState,
    supplied_digest: &[u8; 32],
) -> Result<usize, AuthStoreError> {
    if state.credentials.is_empty() {
        return Err(AuthStoreError::CredentialInvalid);
    }
    let mut matching = None;
    for (index, credential) in state.credentials.iter().enumerate() {
        let expected = decode_digest(&credential.digest_sha256)?;
        let equal = constant_time_eq(&expected, supplied_digest);
        if equal {
            matching = Some(index);
        }
    }
    matching.ok_or(AuthStoreError::CredentialInvalid)
}

fn hash_secret(domain: &[u8], secret: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(secret);
    hasher.finalize().into()
}

fn hash_secret_hex(domain: &[u8], secret: &[u8]) -> String {
    encode_digest(&hash_secret(domain, secret))
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0_u8;
    for index in 0..32 {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

fn encode_digest(digest: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn decode_digest(value: &str) -> Result<[u8; 32], AuthStoreError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AuthStoreError::InvalidState);
    }
    let mut digest = [0_u8; 32];
    for (index, slot) in digest.iter_mut().enumerate() {
        let high = hex_nibble(value.as_bytes()[index * 2])?;
        let low = hex_nibble(value.as_bytes()[index * 2 + 1])?;
        *slot = (high << 4) | low;
    }
    Ok(digest)
}

fn hex_nibble(value: u8) -> Result<u8, AuthStoreError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(AuthStoreError::InvalidState),
    }
}

fn validate_state(state: &PersistedAuthState) -> Result<(), AuthStoreError> {
    if state.schema_version != AUTH_SCHEMA_VERSION
        || state.server_id == Uuid::nil()
        || state.credentials.len() > MAX_CREDENTIALS
        || state.pairing_claims.len() > MAX_PAIRING_REPLAY_CLAIMS
        || state.operation_claims.len() > MAX_OPERATION_REPLAY_CLAIMS
        || state.pin_rotation_authorizations.len() > MAX_PIN_ROTATION_AUTHORIZATIONS
    {
        return Err(AuthStoreError::InvalidState);
    }
    if let Some(token) = &state.pairing_token {
        if token.token_id == Uuid::nil()
            || token.expires_at_unix_ms <= token.issued_at_unix_ms
            || token.expires_at_unix_ms - token.issued_at_unix_ms > MAX_PAIRING_TOKEN_LIFETIME_MS
            || decode_digest(&token.digest_sha256).is_err()
        {
            return Err(AuthStoreError::InvalidState);
        }
    }

    for (index, credential) in state.credentials.iter().enumerate() {
        if credential.credential_id == Uuid::nil()
            || credential.expires_at_unix_ms <= credential.issued_at_unix_ms
            || credential.expires_at_unix_ms - credential.issued_at_unix_ms
                > MAX_CONTROL_CREDENTIAL_LIFETIME_MS
            || decode_digest(&credential.digest_sha256).is_err()
            || state.credentials[index + 1..].iter().any(|other| {
                other.credential_id == credential.credential_id
                    || other.digest_sha256 == credential.digest_sha256
            })
        {
            return Err(AuthStoreError::InvalidState);
        }
    }

    for (index, claim) in state.pairing_claims.iter().enumerate() {
        if validate_request_identity(claim.request_id, &claim.nonce).is_err()
            || state.pairing_claims[index + 1..]
                .iter()
                .any(|other| other.request_id == claim.request_id || other.nonce == claim.nonce)
        {
            return Err(AuthStoreError::InvalidState);
        }
    }
    for (index, claim) in state.operation_claims.iter().enumerate() {
        if validate_request_identity(claim.request_id, &claim.nonce).is_err()
            || !state
                .credentials
                .iter()
                .any(|item| item.credential_id == claim.credential_id)
            || state.operation_claims[index + 1..]
                .iter()
                .any(|other| other.request_id == claim.request_id || other.nonce == claim.nonce)
        {
            return Err(AuthStoreError::InvalidState);
        }
    }
    for (index, authorization) in state.pin_rotation_authorizations.iter().enumerate() {
        if validate_request_identity(Uuid::from_u128(1), &authorization.challenge).is_err()
            || authorization.server_id != state.server_id
            || authorization.credential_id == Uuid::nil()
            || validate_request_identity(
                authorization.authorization_request_id,
                &authorization.authorization_request_nonce,
            )
            .is_err()
            || authorization.authorization_request_sequence == 0
            || authorization.authorization_response_sequence == 0
            || authorization.authorization_response_sequence > state.response_sequence
            || authorization.pairing_token_id == Uuid::nil()
            || authorization.new_pin.validate().is_err()
            || authorization.expires_at_unix_ms <= authorization.issued_at_unix_ms
            || authorization.expires_at_unix_ms - authorization.issued_at_unix_ms
                > MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS
            || !state
                .credentials
                .iter()
                .any(|credential| credential.credential_id == authorization.credential_id)
            || state
                .pairing_token
                .as_ref()
                .is_none_or(|token| token.token_id != authorization.pairing_token_id)
            || !state.operation_claims.iter().any(|claim| {
                claim.credential_id == authorization.credential_id
                    && claim.request_id == authorization.authorization_request_id
                    && claim.nonce == authorization.authorization_request_nonce
            })
            || state.pin_rotation_authorizations[index + 1..]
                .iter()
                .any(|other| {
                    other.challenge == authorization.challenge
                        || other.pairing_token_id == authorization.pairing_token_id
                })
        {
            return Err(AuthStoreError::InvalidState);
        }
    }
    Ok(())
}

fn validate_state_path(path: PathBuf) -> Result<PathBuf, AuthStoreError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AuthStoreError::InvalidPath);
    }
    let parent = path.parent().ok_or(AuthStoreError::InvalidPath)?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| AuthStoreError::InvalidPath)?;
    if canonical_parent != parent {
        return Err(AuthStoreError::InvalidPath);
    }
    reject_symlink_if_present(&path)?;
    Ok(path)
}

fn reject_symlink_if_present(path: &Path) -> Result<(), AuthStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AuthStoreError::UnsafeFile),
        Ok(metadata) if !metadata.is_file() => Err(AuthStoreError::UnsafeFile),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AuthStoreError::Io(error)),
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn open_lock_file(path: &Path) -> Result<File, AuthStoreError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(AuthStoreError::Io)?;
    set_mode_0600(path)?;
    Ok(file)
}

#[cfg(unix)]
fn acquire_exclusive_lock(file: &File) -> Result<(), AuthStoreError> {
    use std::os::{raw::c_int, unix::io::AsRawFd};

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;
    extern "C" {
        fn flock(file_descriptor: c_int, operation: c_int) -> c_int;
    }

    // SAFETY: `file` owns a live descriptor for the duration of this call and
    // `flock` does not retain the pointer or access Rust memory.
    let result = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if result == 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        if matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::PermissionDenied
        ) {
            Err(AuthStoreError::AlreadyLocked)
        } else {
            Err(AuthStoreError::Io(error))
        }
    }
}

#[cfg(not(unix))]
fn acquire_exclusive_lock(_file: &File) -> Result<(), AuthStoreError> {
    Err(AuthStoreError::UnsupportedPlatform)
}

fn recover_interrupted_write(path: &Path) -> Result<(), AuthStoreError> {
    let temporary = sibling_with_suffix(path, ".tmp");
    reject_symlink_if_present(&temporary)?;
    if path.exists() {
        if temporary.exists() {
            fs::remove_file(temporary).map_err(AuthStoreError::Io)?;
        }
        return Ok(());
    }
    if temporary.exists() {
        let state = read_state_file(&temporary)?;
        validate_state(&state)?;
        #[cfg(unix)]
        {
            fs::rename(&temporary, path).map_err(AuthStoreError::Io)?;
            set_mode_0600(path)?;
            sync_parent(path)?;
        }
        #[cfg(not(unix))]
        return Err(AuthStoreError::UnsupportedPlatform);
    }
    Ok(())
}

fn read_state_file(path: &Path) -> Result<PersistedAuthState, AuthStoreError> {
    reject_symlink_if_present(path)?;
    let metadata = fs::metadata(path).map_err(AuthStoreError::Io)?;
    if !metadata.is_file() || metadata.len() > MAX_AUTH_STATE_BYTES {
        return Err(AuthStoreError::UnsafeFile);
    }
    set_mode_0600(path)?;
    let mut file = File::open(path).map_err(AuthStoreError::Io)?;
    let mut raw = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take(MAX_AUTH_STATE_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(AuthStoreError::Io)?;
    if raw.len() as u64 > MAX_AUTH_STATE_BYTES {
        return Err(AuthStoreError::StateLimitExceeded);
    }
    let state = serde_json::from_slice(&raw).map_err(AuthStoreError::Json)?;
    validate_state(&state)?;
    Ok(state)
}

#[cfg(unix)]
fn set_mode_0600(path: &Path) -> Result<(), AuthStoreError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path).map_err(AuthStoreError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AuthStoreError::UnsafeFile);
    }
    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o777 != 0o600 {
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions).map_err(AuthStoreError::Io)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_mode_0600(_path: &Path) -> Result<(), AuthStoreError> {
    Err(AuthStoreError::UnsupportedPlatform)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), AuthStoreError> {
    let parent = path.parent().ok_or(AuthStoreError::InvalidPath)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(AuthStoreError::Io)
}
