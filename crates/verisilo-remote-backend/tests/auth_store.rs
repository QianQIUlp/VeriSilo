#![cfg(unix)]

#[allow(dead_code)]
#[path = "../src/auth_store.rs"]
mod auth_store;

use std::{fs, path::PathBuf};

use auth_store::{AuthStore, AuthStoreError};
use uuid::Uuid;
use verisilo_remote_backend::{
    TlsPin, TlsPinKind, TlsPinRotationPairingClaim, MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS,
};

const NOW: u64 = 1_800_000_000_000;

fn root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("verisilo-auth-test-{label}-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    root
}

fn nonce() -> String {
    Uuid::new_v4().simple().to_string()
}

fn pair(store: &mut AuthStore, now: u64) -> (Uuid, String, u64) {
    let token = store.issue_pairing_token(now, 300_000).unwrap();
    let request_id = Uuid::new_v4();
    let grant = store
        .redeem_pairing_token(
            request_id,
            &nonce(),
            token.token_id(),
            token.secret(),
            token.expires_at_unix_ms(),
            now,
            3_600_000,
        )
        .unwrap();
    (
        grant.credential_id,
        grant.credential.to_string(),
        grant.credential_expires_at_unix_ms,
    )
}

#[test]
fn server_id_and_response_sequence_survive_restart() {
    let root = root("restart");
    let path = root.join("auth.json");
    let server_id;
    {
        let mut store = AuthStore::open(path.clone()).unwrap();
        server_id = store.server_id();
        assert_eq!(store.current_response_sequence(), 0);
        assert_eq!(store.reserve_response_sequence().unwrap(), 1);
        assert_eq!(store.reserve_response_sequence().unwrap(), 2);
    }
    {
        let mut reopened = AuthStore::open(path).unwrap();
        assert_eq!(reopened.server_id(), server_id);
        assert_eq!(reopened.current_response_sequence(), 2);
        assert_eq!(reopened.reserve_response_sequence().unwrap(), 3);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pairing_token_is_one_time_and_plaintext_never_reaches_disk() {
    let root = root("one-time");
    let path = root.join("auth.json");
    let mut store = AuthStore::open(path.clone()).unwrap();
    let token = store.issue_pairing_token(NOW, 300_000).unwrap();
    let token_secret = token.secret().to_owned();
    let request_id = Uuid::new_v4();
    let request_nonce = nonce();
    let grant = store
        .redeem_pairing_token(
            request_id,
            &request_nonce,
            token.token_id(),
            token.secret(),
            token.expires_at_unix_ms(),
            NOW,
            3_600_000,
        )
        .unwrap();
    let credential = grant.credential.to_string();

    let persisted = fs::read_to_string(&path).unwrap();
    assert!(!persisted.contains(&token_secret));
    assert!(!persisted.contains(&credential));
    assert!(persisted.contains("digestSha256"));

    assert!(matches!(
        store.redeem_pairing_token(
            request_id,
            &request_nonce,
            token.token_id(),
            token.secret(),
            token.expires_at_unix_ms(),
            NOW,
            3_600_000,
        ),
        Err(AuthStoreError::Replay)
    ));
    assert!(matches!(
        store.redeem_pairing_token(
            Uuid::new_v4(),
            &nonce(),
            token.token_id(),
            token.secret(),
            token.expires_at_unix_ms(),
            NOW,
            3_600_000,
        ),
        Err(AuthStoreError::PairingTokenInvalid)
    ));
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn credential_authentication_replay_expiry_and_revocation_are_durable() {
    let root = root("credential");
    let path = root.join("auth.json");
    let credential_id;
    let credential;
    let expiry;
    {
        let mut store = AuthStore::open(path.clone()).unwrap();
        (credential_id, credential, expiry) = pair(&mut store, NOW);
        assert_eq!(store.authenticate(&credential, NOW).unwrap(), credential_id);
        let request_id = Uuid::new_v4();
        let request_nonce = nonce();
        let authenticated = store
            .authenticate_operation(&credential, request_id, &request_nonce, 1, NOW)
            .unwrap();
        assert_eq!(authenticated.credential_id, credential_id);
        assert!(matches!(
            store.authenticate_operation(&credential, request_id, &request_nonce, 1, NOW),
            Err(AuthStoreError::Replay)
        ));
    }
    {
        let mut reopened = AuthStore::open(path.clone()).unwrap();
        assert!(matches!(
            reopened.authenticate_operation(&credential, Uuid::new_v4(), &nonce(), 1, NOW),
            Err(AuthStoreError::Replay)
        ));
        assert!(matches!(
            reopened.authenticate(&credential, expiry),
            Err(AuthStoreError::CredentialExpired)
        ));
        reopened.revoke_credential(credential_id).unwrap();
    }
    {
        let reopened = AuthStore::open(path).unwrap();
        assert!(matches!(
            reopened.authenticate(&credential, NOW),
            Err(AuthStoreError::CredentialRevoked)
        ));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pin_rotation_authorization_is_durable_bound_short_lived_and_single_use() {
    let root = root("pin-rotation");
    let path = root.join("auth.json");
    let mut store = AuthStore::open(path.clone()).unwrap();
    let (credential_id, credential, _) = pair(&mut store, NOW);
    let token = store.issue_pairing_token(NOW, 300_000).unwrap();
    let pin = TlsPin {
        kind: TlsPinKind::SpkiSha256,
        sha256: "b".repeat(64),
    };
    let server_id = store.server_id();
    let authorization_request_id = Uuid::new_v4();
    let authorization_request_nonce = nonce();
    let authorization = store
        .authorize_pin_rotation(
            &credential,
            authorization_request_id,
            &authorization_request_nonce,
            1,
            credential_id,
            token.token_id(),
            &pin,
            NOW,
        )
        .unwrap();
    drop(store);

    let mut reopened = AuthStore::open(path.clone()).unwrap();
    let claim = TlsPinRotationPairingClaim {
        challenge: authorization.challenge.clone(),
        server_id,
        old_client_credential_id: credential_id,
        authorization_request_id,
        authorization_request_nonce,
        authorization_request_sequence: 1,
        authorization_response_sequence: authorization.response_sequence,
        authorization_expires_at_unix_ms: authorization.expires_at_unix_ms,
        pairing_token_id: token.token_id(),
        new_pin: pin.clone(),
    };
    assert!(matches!(
        reopened.redeem_pairing_token_for_rotation(
            Uuid::new_v4(),
            &nonce(),
            token.token_id(),
            token.secret(),
            token.expires_at_unix_ms(),
            NOW + MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS,
            3_600_000,
            &claim,
        ),
        Err(AuthStoreError::PinRotationAuthorizationInvalid)
    ));
    drop(reopened);
    drop(token);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_initial_write_is_recovered_and_files_are_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let root = root("recovery");
    let path = root.join("auth.json");
    let server_id;
    {
        let store = AuthStore::open(path.clone()).unwrap();
        server_id = store.server_id();
    }
    let temporary = PathBuf::from(format!("{}.tmp", path.display()));
    fs::copy(&path, &temporary).unwrap();
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_file(&path).unwrap();

    {
        let reopened = AuthStore::open(path.clone()).unwrap();
        assert_eq!(reopened.server_id(), server_id);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let lock = PathBuf::from(format!("{}.lock", path.display()));
        assert_eq!(
            fs::metadata(lock).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_open_is_rejected() {
    let root = root("lock");
    let path = root.join("auth.json");
    let first = AuthStore::open(path.clone()).unwrap();
    assert!(matches!(
        AuthStore::open(path),
        Err(AuthStoreError::AlreadyLocked)
    ));
    drop(first);
    fs::remove_dir_all(root).unwrap();
}
