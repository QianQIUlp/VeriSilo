use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use uuid::Uuid;
use verisilo_remote_backend::{
    CapabilityAvailability as RemoteCapabilityAvailability, MemoryBindingStore,
    OperationResult as RemoteOperationResult, RemoteBackendSnapshot, RemoteEndpoint,
    RemoteOperation, RemoteOrphanReceipt,
};
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{
    inspect_browser_executable, validate_silo_name, BrowserDescriptor, BrowserVerification,
    BrowserVerificationState, CreateSiloInput, ExternalMihomoBinding, NetworkProfile, ProxyScheme,
    Silo, SiloStorageUsage, UpdateSiloEngineInput, UpdateSiloInput, UpdateSiloNetworkInput,
    VaultLockState, VaultStatus, SCHEMA_VERSION,
};
use crate::native_host::{validate_network_evidence_inbox_entry, NativeNetworkEvidenceInboxEntry};

const AUTO_LOCK_MINUTES: i64 = 15;
const VAULT_FILE_NAME: &str = "vault.json";
const VAULT_ENVELOPE_VERSION: u32 = 2;
const VAULT_DATA_SCHEMA_VERSION: u32 = 7;
const MAX_NETWORK_EVIDENCE_RECORDS: usize = 1_000;
const MAX_NETWORK_EVIDENCE_PER_SILO: usize = 100;
const MAX_REMOTE_BINDINGS: usize = 10_000;
const MAX_REMOTE_OPERATION_RESULTS: usize = 10_000;
const MAX_USED_PAIRING_TOKENS: usize = 4_096;
const MAX_REMOTE_ORPHAN_RECEIPTS: usize = 10_000;
const KDF_MEMORY_KIB: u32 = 19_456;
const KDF_ITERATIONS: u32 = 2;
const KDF_PARALLELISM: u32 = 1;
const MAX_VAULT_BACKUP_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Default)]
pub struct VaultRuntime {
    unlocked: Option<UnlockedVault>,
    #[cfg(test)]
    test_now: Option<chrono::DateTime<Utc>>,
}

struct UnlockedVault {
    /// Random data-encryption key. It encrypts only the vault payload, never
    /// browser-owned profile files.
    dek: Zeroizing<[u8; 32]>,
    /// Argon2id-derived key used only to wrap the DEK.
    kek: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    data: VaultData,
    auto_lock_at: chrono::DateTime<Utc>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VaultData {
    schema_version: u32,
    #[serde(
        serialize_with = "serialize_vault_silos",
        deserialize_with = "deserialize_vault_silos"
    )]
    silos: Vec<Silo>,
    seed_material: HashMap<Uuid, String>,
    #[serde(default)]
    proxy_credentials: HashMap<Uuid, StoredProxyCredential>,
    #[serde(default)]
    mihomo_controller_secrets: HashMap<Uuid, StoredMihomoControllerSecret>,
    /// User-initiated, sanitized observations imported from Native Messaging.
    /// This lives inside the encrypted Vault payload; it is evidence of an
    /// observed request, not proof that DNS/WebRTC/QUIC were fully controlled.
    #[serde(default)]
    network_evidence: Vec<NativeNetworkEvidenceInboxEntry>,
    /// Self-hosted endpoint, application credential, replay ledger and stable
    /// Silo bindings. The complete structure is serialized only inside the
    /// encrypted Vault payload.
    #[serde(default)]
    remote_control_plane: RemoteVaultState,
}

/// Vault schemas 1–7 persist `NetworkProfile` fields with their historical
/// snake_case Rust names. The public Tauri/domain JSON stays camelCase. These
/// explicit DTOs keep both contracts strict without rewriting arbitrary JSON
/// keys or accepting mixed wire shapes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultSiloRef<'a> {
    id: Uuid,
    schema_version: u32,
    name: &'a str,
    color: &'a str,
    browser: &'a BrowserDescriptor,
    profile_directory: &'a str,
    network_profile: VaultNetworkProfileRef<'a>,
    engine: &'a crate::engine::SiloEngineConfig,
    seed_reference: Uuid,
    created_at: &'a chrono::DateTime<Utc>,
    archived_at: Option<&'a chrono::DateTime<Utc>>,
}

impl<'a> From<&'a Silo> for VaultSiloRef<'a> {
    fn from(silo: &'a Silo) -> Self {
        Self {
            id: silo.id,
            schema_version: silo.schema_version,
            name: &silo.name,
            color: &silo.color,
            browser: &silo.browser,
            profile_directory: &silo.profile_directory,
            network_profile: VaultNetworkProfileRef::from(&silo.network_profile),
            engine: &silo.engine,
            seed_reference: silo.seed_reference,
            created_at: &silo.created_at,
            archived_at: silo.archived_at.as_ref(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VaultSiloRecord {
    id: Uuid,
    schema_version: u32,
    name: String,
    color: String,
    browser: BrowserDescriptor,
    profile_directory: String,
    network_profile: VaultNetworkProfile,
    #[serde(default)]
    engine: crate::engine::SiloEngineConfig,
    seed_reference: Uuid,
    created_at: chrono::DateTime<Utc>,
    archived_at: Option<chrono::DateTime<Utc>>,
}

impl From<VaultSiloRecord> for Silo {
    fn from(silo: VaultSiloRecord) -> Self {
        Self {
            id: silo.id,
            schema_version: silo.schema_version,
            name: silo.name,
            color: silo.color,
            browser: silo.browser,
            profile_directory: silo.profile_directory,
            network_profile: silo.network_profile.into(),
            engine: silo.engine,
            seed_reference: silo.seed_reference,
            created_at: silo.created_at,
            archived_at: silo.archived_at,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "mode")]
enum VaultNetworkProfileRef<'a> {
    #[serde(rename = "direct")]
    Direct { proxy_required: bool },
    #[serde(rename = "fixed_proxy")]
    FixedProxy {
        proxy_required: bool,
        scheme: &'a ProxyScheme,
        host: &'a str,
        port: u16,
        bypass_list: &'a [String],
        #[serde(skip_serializing_if = "Option::is_none")]
        credential_reference: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        external_mihomo: Option<&'a ExternalMihomoBinding>,
    },
    #[serde(rename = "pac")]
    Pac {
        proxy_required: bool,
        pac_url: &'a str,
    },
}

impl<'a> From<&'a NetworkProfile> for VaultNetworkProfileRef<'a> {
    fn from(profile: &'a NetworkProfile) -> Self {
        match profile {
            NetworkProfile::Direct { proxy_required } => Self::Direct {
                proxy_required: *proxy_required,
            },
            NetworkProfile::FixedProxy {
                proxy_required,
                scheme,
                host,
                port,
                bypass_list,
                credential_reference,
                external_mihomo,
            } => Self::FixedProxy {
                proxy_required: *proxy_required,
                scheme,
                host,
                port: *port,
                bypass_list,
                credential_reference: *credential_reference,
                external_mihomo: external_mihomo.as_ref(),
            },
            NetworkProfile::Pac {
                proxy_required,
                pac_url,
            } => Self::Pac {
                proxy_required: *proxy_required,
                pac_url,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "mode", deny_unknown_fields)]
enum VaultNetworkProfile {
    #[serde(rename = "direct")]
    Direct { proxy_required: bool },
    #[serde(rename = "fixed_proxy")]
    FixedProxy {
        proxy_required: bool,
        scheme: ProxyScheme,
        host: String,
        port: u16,
        bypass_list: Vec<String>,
        #[serde(default)]
        credential_reference: Option<Uuid>,
        #[serde(default)]
        external_mihomo: Option<ExternalMihomoBinding>,
    },
    #[serde(rename = "pac")]
    Pac {
        proxy_required: bool,
        pac_url: String,
    },
}

impl From<VaultNetworkProfile> for NetworkProfile {
    fn from(profile: VaultNetworkProfile) -> Self {
        match profile {
            VaultNetworkProfile::Direct { proxy_required } => Self::Direct { proxy_required },
            VaultNetworkProfile::FixedProxy {
                proxy_required,
                scheme,
                host,
                port,
                bypass_list,
                credential_reference,
                external_mihomo,
            } => Self::FixedProxy {
                proxy_required,
                scheme,
                host,
                port,
                bypass_list,
                credential_reference,
                external_mihomo,
            },
            VaultNetworkProfile::Pac {
                proxy_required,
                pac_url,
            } => Self::Pac {
                proxy_required,
                pac_url,
            },
        }
    }
}

fn serialize_vault_silos<S>(silos: &[Silo], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    silos
        .iter()
        .map(VaultSiloRef::from)
        .collect::<Vec<_>>()
        .serialize(serializer)
}

fn deserialize_vault_silos<'de, D>(deserializer: D) -> Result<Vec<Silo>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<VaultSiloRecord>::deserialize(deserializer)
        .map(|silos| silos.into_iter().map(Silo::from).collect())
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RemoteVaultState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint: Option<RemoteEndpoint>,
    #[serde(default)]
    pub(crate) backend: RemoteBackendSnapshot,
    #[serde(default)]
    pub(crate) last_results: HashMap<Uuid, RemoteOperationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pairing_revoked_at: Option<chrono::DateTime<Utc>>,
    /// Permanent local audit trail for force-detached bindings. Receipts may
    /// outlive their local Silo and never imply authenticated remote deletion.
    #[serde(default)]
    pub(crate) orphan_receipts: Vec<RemoteOrphanReceipt>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredProxyCredential {
    username: String,
    password: String,
}

impl Drop for StoredProxyCredential {
    fn drop(&mut self) {
        self.username.zeroize();
        self.password.zeroize();
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMihomoControllerSecret {
    secret: String,
}

impl Drop for StoredMihomoControllerSecret {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

#[derive(Clone)]
pub struct ProxyAuthentication {
    username: Zeroizing<String>,
    password: Zeroizing<String>,
}

#[derive(Clone)]
pub struct MihomoControllerAuthentication {
    secret: Zeroizing<String>,
}

impl MihomoControllerAuthentication {
    pub(crate) fn new(secret: String) -> Self {
        Self {
            secret: Zeroizing::new(secret),
        }
    }

    pub fn secret(&self) -> &str {
        self.secret.as_str()
    }
}

impl ProxyAuthentication {
    pub(crate) fn new(username: String, password: String) -> Self {
        Self {
            username: Zeroizing::new(username),
            password: Zeroizing::new(password),
        }
    }

    pub fn username(&self) -> &str {
        self.username.as_str()
    }

    pub fn password(&self) -> &str {
        self.password.as_str()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VaultEnvelope {
    version: u32,
    kdf: String,
    salt: String,
    #[serde(default)]
    wrap_nonce: Option<String>,
    #[serde(default)]
    wrapped_dek: Option<String>,
    nonce: String,
    ciphertext: String,
}

struct OpenedEnvelope {
    dek: Zeroizing<[u8; 32]>,
    kek: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    data: VaultData,
    needs_migration: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultBackupReceipt {
    pub destination_path: String,
    pub bytes: u64,
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("The local vault already exists.")]
    AlreadyInitialized,
    #[error("The local vault does not exist.")]
    NotInitialized,
    #[error("The vault is locked.")]
    Locked,
    #[error("The vault could not be decrypted. Check the passphrase.")]
    InvalidPassphrase,
    #[error("The vault data is invalid or uses an unsupported version.")]
    InvalidData,
    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Cryptographic setup failed.")]
    CryptographicSetup,
    #[error("Silo not found.")]
    SiloNotFound,
    #[error("A Silo cannot be archived while it is running.")]
    SiloRunning,
    #[error("The Silo browser profile is locked by a browser process.")]
    SiloProfileInUse,
    #[error("Browser verification failed: {0}")]
    BrowserVerification(String),
    #[error("The stored Silo profile path is outside VeriSilo's managed directory.")]
    UnmanagedProfile,
    #[error("Permanent deletion requires explicit confirmation.")]
    PermanentDeleteNotConfirmed,
    #[error("Destroy the Silo's remote environment before permanently deleting the Silo.")]
    SiloRemoteBound,
    #[error("Restoring over an existing Vault requires explicit confirmation.")]
    RestoreOverwriteNotConfirmed,
    #[error("The selected backup destination already exists.")]
    BackupDestinationExists,
    #[error("The selected Vault backup is too large.")]
    BackupTooLarge,
    #[error("Silo input is invalid: {0}")]
    InvalidSilo(String),
}

impl VaultRuntime {
    pub fn status(&mut self, root: &Path) -> VaultStatus {
        // A Windows replacement interrupted between removing the destination
        // and renaming the temporary envelope must still look like a locked
        // Vault, never a fresh/uninitialized installation.
        let _ = recover_interrupted_write(root);
        self.expire_if_needed();
        match &self.unlocked {
            Some(unlocked) => VaultStatus {
                state: VaultLockState::Unlocked,
                auto_lock_at: Some(unlocked.auto_lock_at),
            },
            None if vault_path(root).is_file()
                || vault_path(root).with_extension("bak").is_file() =>
            {
                VaultStatus {
                    state: VaultLockState::Locked,
                    auto_lock_at: None,
                }
            }
            None => VaultStatus {
                state: VaultLockState::Uninitialized,
                auto_lock_at: None,
            },
        }
    }

    pub fn initialize(&mut self, root: &Path, passphrase: &str) -> Result<(), VaultError> {
        validate_passphrase(passphrase)?;
        recover_interrupted_write(root)?;
        if vault_path(root).exists() {
            return Err(VaultError::AlreadyInitialized);
        }

        let salt = random_bytes::<16>();
        let kek = derive_key(passphrase, &salt)?;
        let data = VaultData {
            schema_version: VAULT_DATA_SCHEMA_VERSION,
            silos: Vec::new(),
            seed_material: HashMap::new(),
            proxy_credentials: HashMap::new(),
            mihomo_controller_secrets: HashMap::new(),
            network_evidence: Vec::new(),
            remote_control_plane: RemoteVaultState::default(),
        };
        let unlocked = UnlockedVault {
            dek: Zeroizing::new(random_bytes()),
            kek,
            salt,
            data,
            auto_lock_at: self.auto_lock_time(),
        };
        self.unlocked = Some(unlocked);
        if let Err(error) = self.persist(root) {
            self.lock();
            return Err(error);
        }
        Ok(())
    }

    pub fn unlock(&mut self, root: &Path, passphrase: &str) -> Result<(), VaultError> {
        validate_passphrase(passphrase)?;
        recover_interrupted_write(root)?;
        let raw = fs::read(vault_path(root)).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VaultError::NotInitialized,
            _ => VaultError::Filesystem(error),
        })?;
        let opened = open_envelope(&raw, passphrase)?;
        let needs_migration = opened.needs_migration;

        self.unlocked = Some(UnlockedVault {
            dek: opened.dek,
            kek: opened.kek,
            salt: opened.salt,
            data: opened.data,
            auto_lock_at: self.auto_lock_time(),
        });
        if needs_migration {
            if let Err(error) = self.persist(root) {
                self.lock();
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn lock(&mut self) {
        self.unlocked = None;
    }

    pub fn change_passphrase(
        &mut self,
        root: &Path,
        current_passphrase: &str,
        new_passphrase: &str,
    ) -> Result<(), VaultError> {
        validate_passphrase(current_passphrase)?;
        validate_passphrase(new_passphrase)?;
        recover_interrupted_write(root)?;

        let raw = fs::read(vault_path(root)).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VaultError::NotInitialized,
            _ => VaultError::Filesystem(error),
        })?;
        let current = open_envelope(&raw, current_passphrase)?;
        {
            let unlocked = self.unlocked_mut_for_activity()?;
            if current.dek.as_ref() != unlocked.dek.as_ref() {
                return Err(VaultError::InvalidData);
            }
        }

        let new_salt = random_bytes::<16>();
        let new_kek = derive_key(new_passphrase, &new_salt)?;
        let (old_salt, old_kek) = {
            let unlocked = self.unlocked_mut_without_activity()?;
            let old = (unlocked.salt, unlocked.kek.clone());
            unlocked.salt = new_salt;
            unlocked.kek = new_kek;
            old
        };

        if let Err(error) = self.persist(root) {
            if let Some(unlocked) = self.unlocked.as_mut() {
                unlocked.salt = old_salt;
                unlocked.kek = old_kek;
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn backup(
        &mut self,
        root: &Path,
        destination_path: &Path,
    ) -> Result<VaultBackupReceipt, VaultError> {
        self.record_activity()?;
        if destination_path.exists() {
            return Err(VaultError::BackupDestinationExists);
        }

        // Regenerate the envelope from the in-memory state so a backup never
        // copies a partially written or externally corrupted Vault file.
        self.persist(root)?;
        let raw = fs::read(vault_path(root))?;
        let envelope: VaultEnvelope =
            serde_json::from_slice(&raw).map_err(|_| VaultError::InvalidData)?;
        if envelope.version != VAULT_ENVELOPE_VERSION || envelope.kdf != "argon2id" {
            return Err(VaultError::InvalidData);
        }
        atomic_write_new(destination_path, &raw)?;
        Ok(VaultBackupReceipt {
            destination_path: destination_path.to_string_lossy().to_string(),
            bytes: raw.len() as u64,
        })
    }

    pub fn restore(
        &mut self,
        root: &Path,
        source_path: &Path,
        passphrase: &str,
        confirm_overwrite: bool,
    ) -> Result<(), VaultError> {
        validate_passphrase(passphrase)?;
        let metadata = fs::metadata(source_path)?;
        if !metadata.is_file() {
            return Err(VaultError::InvalidData);
        }
        if metadata.len() > MAX_VAULT_BACKUP_BYTES {
            return Err(VaultError::BackupTooLarge);
        }
        let raw = fs::read(source_path)?;
        let mut opened = open_envelope(&raw, passphrase)?;

        recover_interrupted_write(root)?;
        if vault_path(root).exists() && !confirm_overwrite {
            return Err(VaultError::RestoreOverwriteNotConfirmed);
        }

        // A backup contains metadata, not browser-owned Profile files. Always
        // rebase restored paths to this installation's managed root instead of
        // trusting an absolute path recorded on another machine.
        for silo in &mut opened.data.silos {
            silo.profile_directory = root
                .join("silos")
                .join(silo.id.to_string())
                .join("browser-data")
                .to_string_lossy()
                .to_string();
        }
        for silo in &opened.data.silos {
            fs::create_dir_all(&silo.profile_directory)?;
        }

        let previous = self.unlocked.take();
        let auto_lock_at = self.auto_lock_time();
        self.unlocked = Some(UnlockedVault {
            dek: opened.dek,
            kek: opened.kek,
            salt: opened.salt,
            data: opened.data,
            auto_lock_at,
        });
        // Persisting from the validated in-memory representation both performs
        // an atomic replacement and normalizes legacy envelopes to version 2.
        if let Err(error) = self.persist(root) {
            self.unlocked = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn list_silos(&mut self) -> Result<Vec<Silo>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        Ok(unlocked.data.silos.clone())
    }

    pub fn list_active_silos(&mut self) -> Result<Vec<Silo>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        Ok(unlocked
            .data
            .silos
            .iter()
            .filter(|silo| silo.archived_at.is_none())
            .cloned()
            .collect())
    }

    pub fn list_archived_silos(&mut self) -> Result<Vec<Silo>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        Ok(unlocked
            .data
            .silos
            .iter()
            .filter(|silo| silo.archived_at.is_some())
            .cloned()
            .collect())
    }

    pub fn get_silo(&mut self, silo_id: Uuid) -> Result<Silo, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id && silo.archived_at.is_none())
            .cloned()
            .ok_or(VaultError::SiloNotFound)
    }

    pub fn managed_profile_directories(&mut self) -> Result<Vec<PathBuf>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        Ok(unlocked
            .data
            .silos
            .iter()
            .flat_map(Silo::all_engine_profile_directories)
            .collect())
    }

    pub fn identity_seed_for_silo(
        &mut self,
        silo_id: Uuid,
    ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        let seed_reference = unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id && silo.archived_at.is_none())
            .ok_or(VaultError::SiloNotFound)?
            .seed_reference;
        let decoded = Zeroizing::new(
            unlocked
                .data
                .seed_material
                .get(&seed_reference)
                .and_then(|value| STANDARD_NO_PAD.decode(value.as_bytes()).ok())
                .ok_or(VaultError::InvalidData)?,
        );
        let seed: [u8; 32] = decoded
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::InvalidData)?;
        Ok(Zeroizing::new(seed))
    }

    pub fn proxy_authentication_for_silo(
        &mut self,
        silo_id: Uuid,
    ) -> Result<Option<ProxyAuthentication>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        let credential_reference = unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id && silo.archived_at.is_none())
            .ok_or(VaultError::SiloNotFound)?
            .network_profile
            .credential_reference();
        let Some(reference) = credential_reference else {
            return Ok(None);
        };
        let credential = unlocked
            .data
            .proxy_credentials
            .get(&reference)
            .ok_or(VaultError::InvalidData)?;
        Ok(Some(ProxyAuthentication::new(
            credential.username.clone(),
            credential.password.clone(),
        )))
    }

    pub fn mihomo_controller_authentication_for_silo(
        &mut self,
        silo_id: Uuid,
    ) -> Result<Option<MihomoControllerAuthentication>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        let secret_reference = unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id && silo.archived_at.is_none())
            .ok_or(VaultError::SiloNotFound)?
            .network_profile
            .mihomo_controller_secret_reference();
        let Some(reference) = secret_reference else {
            return Ok(None);
        };
        let stored = unlocked
            .data
            .mihomo_controller_secrets
            .get(&reference)
            .ok_or(VaultError::InvalidData)?;
        Ok(Some(MihomoControllerAuthentication::new(
            stored.secret.clone(),
        )))
    }

    pub fn silo_profile_directory(&mut self, silo_id: Uuid) -> Result<PathBuf, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id)
            .map(|silo| PathBuf::from(&silo.profile_directory))
            .ok_or(VaultError::SiloNotFound)
    }

    fn silo_has_browser_lock(&mut self, silo_id: Uuid) -> Result<bool, VaultError> {
        let silo = self.silo_by_id(silo_id)?;
        Ok(silo
            .all_engine_profile_directories()
            .iter()
            .any(|directory| profile_has_browser_lock(directory)))
    }

    pub fn create_silo(&mut self, root: &Path, input: CreateSiloInput) -> Result<Silo, VaultError> {
        input
            .validate()
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        // Refuse sensitive state changes before creating even an empty profile directory.
        self.record_activity()?;
        let CreateSiloInput {
            name,
            color,
            browser_kind,
            executable_path,
            mut network_profile,
            engine,
            proxy_credentials,
            mihomo_controller_secret,
        } = input;
        let browser_inspection =
            inspect_browser_executable(&browser_kind, Path::new(&executable_path))
                .map_err(|error| VaultError::BrowserVerification(error.to_string()))?;
        let silo_id = Uuid::new_v4();
        let seed_reference = Uuid::new_v4();
        let profile_directory = root
            .join("silos")
            .join(silo_id.to_string())
            .join("browser-data");
        fs::create_dir_all(&profile_directory)?;

        let stored_proxy_credential = proxy_credentials.map(|credentials| {
            let reference = Uuid::new_v4();
            network_profile
                .set_credential_reference(reference)
                .expect("validated fixed proxy accepts a credential reference");
            (
                reference,
                StoredProxyCredential {
                    username: credentials.username,
                    password: credentials.password,
                },
            )
        });
        let stored_mihomo_controller_secret = mihomo_controller_secret.map(|controller_secret| {
            let reference = Uuid::new_v4();
            network_profile
                .set_mihomo_controller_secret_reference(reference)
                .expect("validated external Mihomo binding accepts a secret reference");
            (
                reference,
                StoredMihomoControllerSecret {
                    secret: controller_secret.secret,
                },
            )
        });

        let silo = Silo {
            id: silo_id,
            schema_version: SCHEMA_VERSION,
            name: name.trim().to_owned(),
            color,
            browser: crate::domain::BrowserDescriptor {
                kind: browser_kind,
                executable_path: browser_inspection.resolved_path,
                version: Some(browser_inspection.version),
            },
            profile_directory: profile_directory.to_string_lossy().to_string(),
            network_profile,
            engine,
            seed_reference,
            created_at: Utc::now(),
            archived_at: None,
        };

        let seed = STANDARD_NO_PAD.encode(random_bytes::<32>());
        let prospective_data = {
            let unlocked = self.unlocked_mut_without_activity()?;
            let mut data = unlocked.data.clone();
            data.seed_material.insert(seed_reference, seed);
            if let Some((reference, credentials)) = stored_proxy_credential {
                data.proxy_credentials.insert(reference, credentials);
            }
            if let Some((reference, secret)) = stored_mihomo_controller_secret {
                data.mihomo_controller_secrets.insert(reference, secret);
            }
            data.silos.push(silo.clone());
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        Ok(silo)
    }

    pub fn update_silo(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        input: UpdateSiloInput,
        is_active: bool,
    ) -> Result<Silo, VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        input
            .validate()
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }
        let browser_inspection =
            inspect_browser_executable(&input.browser_kind, Path::new(&input.executable_path))
                .map_err(|error| VaultError::BrowserVerification(error.to_string()))?;
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            silo.name = input.name.trim().to_owned();
            silo.color = input.color;
            silo.browser.kind = input.browser_kind;
            silo.browser.executable_path = browser_inspection.resolved_path;
            silo.browser.version = Some(browser_inspection.version);
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        self.silo_by_id(silo_id)
    }

    /// Atomically updates editable metadata and, when requested, replaces the
    /// complete network configuration. Browser inspection and every input
    /// validation finish before a single encrypted Vault commit; callers never
    /// observe a half-updated Silo.
    pub fn update_silo_configuration(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        input: UpdateSiloInput,
        network_input: Option<UpdateSiloNetworkInput>,
        engine_input: Option<UpdateSiloEngineInput>,
        is_active: bool,
    ) -> Result<Silo, VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        input
            .validate()
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        if let Some(network) = network_input.as_ref() {
            network
                .validate()
                .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        }
        let current = self.silo_by_id(silo_id)?;
        let effective_network = network_input
            .as_ref()
            .map(|network| &network.network_profile)
            .unwrap_or(&current.network_profile);
        if let Some(engine) = engine_input.as_ref() {
            engine
                .validate(effective_network)
                .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        } else if network_input.is_some() {
            UpdateSiloEngineInput {
                engine: current.engine,
            }
            .validate(effective_network)
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        }
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }
        let browser_inspection =
            inspect_browser_executable(&input.browser_kind, Path::new(&input.executable_path))
                .map_err(|error| VaultError::BrowserVerification(error.to_string()))?;

        let prepared_network = network_input.map(|network_input| {
            let UpdateSiloNetworkInput {
                mut network_profile,
                proxy_credentials,
                mihomo_controller_secret,
            } = network_input;
            let proxy = proxy_credentials.map(|credentials| {
                let reference = Uuid::new_v4();
                network_profile
                    .set_credential_reference(reference)
                    .expect("validated network profile accepts proxy credentials");
                (
                    reference,
                    StoredProxyCredential {
                        username: credentials.username,
                        password: credentials.password,
                    },
                )
            });
            let mihomo = mihomo_controller_secret.map(|controller_secret| {
                let reference = Uuid::new_v4();
                network_profile
                    .set_mihomo_controller_secret_reference(reference)
                    .expect("validated external Mihomo binding accepts a controller secret");
                (
                    reference,
                    StoredMihomoControllerSecret {
                        secret: controller_secret.secret,
                    },
                )
            });
            (network_profile, proxy, mihomo)
        });

        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            silo.name = input.name.trim().to_owned();
            silo.color = input.color;
            silo.browser.kind = input.browser_kind;
            silo.browser.executable_path = browser_inspection.resolved_path;
            silo.browser.version = Some(browser_inspection.version);
            if let Some(engine) = engine_input {
                silo.engine = engine.engine;
            }

            if let Some((network_profile, proxy, mihomo)) = prepared_network {
                let old_proxy_reference = silo.network_profile.credential_reference();
                let old_mihomo_reference =
                    silo.network_profile.mihomo_controller_secret_reference();
                silo.network_profile = network_profile;
                if let Some((reference, credentials)) = proxy {
                    data.proxy_credentials.insert(reference, credentials);
                }
                if let Some((reference, secret)) = mihomo {
                    data.mihomo_controller_secrets.insert(reference, secret);
                }
                remove_unreferenced_secrets(&mut data, old_proxy_reference, old_mihomo_reference);
            }
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        self.silo_by_id(silo_id)
    }

    /// User-triggered acceptance of an in-place browser version update. A
    /// changed canonical path is never adopted here; selecting a different
    /// executable requires the explicit Silo update flow.
    pub fn recheck_silo_browser(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        is_active: bool,
    ) -> Result<BrowserVerification, VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }
        let descriptor = self.silo_by_id(silo_id)?.browser;
        let inspection =
            inspect_browser_executable(&descriptor.kind, Path::new(&descriptor.executable_path))
                .map_err(|error| VaultError::BrowserVerification(error.to_string()))?;
        if !browser_paths_match(&descriptor.executable_path, &inspection.resolved_path) {
            return Err(VaultError::BrowserVerification(
                "The canonical browser path changed; select the intended browser explicitly in Silo settings."
                    .to_owned(),
            ));
        }
        let actual_version = inspection.version;
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            data.silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?
                .browser
                .version = Some(actual_version.clone());
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        Ok(BrowserVerification {
            state: BrowserVerificationState::Verified,
            expected_kind: descriptor.kind,
            expected_version: Some(actual_version.clone()),
            actual_version: Some(actual_version.clone()),
            executable_path: inspection.resolved_path,
            checked_at: Utc::now(),
            message: format!("已由用户确认浏览器版本基线 {actual_version}。"),
        })
    }

    pub fn rename_silo(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        name: &str,
        is_active: bool,
    ) -> Result<Silo, VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        validate_silo_name(name).map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            silo.name = name.trim().to_owned();
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        self.silo_by_id(silo_id)
    }

    pub fn update_silo_network(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        input: UpdateSiloNetworkInput,
        is_active: bool,
    ) -> Result<Silo, VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        input
            .validate()
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        UpdateSiloEngineInput {
            engine: self.silo_by_id(silo_id)?.engine,
        }
        .validate(&input.network_profile)
        .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }

        let UpdateSiloNetworkInput {
            mut network_profile,
            proxy_credentials,
            mihomo_controller_secret,
        } = input;
        let new_proxy_credential = proxy_credentials.map(|credentials| {
            let reference = Uuid::new_v4();
            network_profile
                .set_credential_reference(reference)
                .expect("validated network profile accepts proxy credentials");
            (
                reference,
                StoredProxyCredential {
                    username: credentials.username,
                    password: credentials.password,
                },
            )
        });
        let new_mihomo_secret = mihomo_controller_secret.map(|controller_secret| {
            let reference = Uuid::new_v4();
            network_profile
                .set_mihomo_controller_secret_reference(reference)
                .expect("validated external Mihomo binding accepts a controller secret");
            (
                reference,
                StoredMihomoControllerSecret {
                    secret: controller_secret.secret,
                },
            )
        });

        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            let old_proxy_reference = silo.network_profile.credential_reference();
            let old_mihomo_reference = silo.network_profile.mihomo_controller_secret_reference();
            silo.network_profile = network_profile;

            if let Some((reference, credentials)) = new_proxy_credential {
                data.proxy_credentials.insert(reference, credentials);
            }
            if let Some((reference, secret)) = new_mihomo_secret {
                data.mihomo_controller_secrets.insert(reference, secret);
            }
            remove_unreferenced_secrets(&mut data, old_proxy_reference, old_mihomo_reference);
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        self.silo_by_id(silo_id)
    }

    pub fn update_silo_engine(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        input: UpdateSiloEngineInput,
        is_active: bool,
    ) -> Result<Silo, VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        let network_profile = self.silo_by_id(silo_id)?.network_profile;
        input
            .validate(&network_profile)
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            data.silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?
                .engine = input.engine;
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        self.silo_by_id(silo_id)
    }

    pub fn archive_silo(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        is_active: bool,
    ) -> Result<(), VaultError> {
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            if silo.archived_at.is_none() {
                silo.archived_at = Some(Utc::now());
            }
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        Ok(())
    }

    pub fn restore_archived_silo(
        &mut self,
        root: &Path,
        silo_id: Uuid,
    ) -> Result<Silo, VaultError> {
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            silo.archived_at = None;
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        self.silo_by_id(silo_id)
    }

    pub fn delete_silo(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        is_active: bool,
        confirm_permanent: bool,
    ) -> Result<(), VaultError> {
        if !confirm_permanent {
            return Err(VaultError::PermanentDeleteNotConfirmed);
        }
        if is_active {
            return Err(VaultError::SiloRunning);
        }
        if self
            .unlocked_mut_for_activity()?
            .data
            .remote_control_plane
            .backend
            .bindings
            .iter()
            .any(|binding| binding.silo_id == silo_id)
        {
            return Err(VaultError::SiloRemoteBound);
        }

        let profile_directory = self.silo_profile_directory(silo_id)?;
        let managed_directory = verified_managed_silo_directory(root, silo_id, &profile_directory)?;
        if self.silo_has_browser_lock(silo_id)? {
            return Err(VaultError::SiloProfileInUse);
        }

        let prospective_data = {
            let unlocked = self.unlocked_mut_without_activity()?;
            let mut data = unlocked.data.clone();
            let index = data
                .silos
                .iter()
                .position(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            let removed = data.silos.remove(index);
            data.seed_material.remove(&removed.seed_reference);
            data.network_evidence
                .retain(|entry| entry.silo_id != removed.id);
            data.remote_control_plane.last_results.remove(&removed.id);
            remove_unreferenced_secrets(
                &mut data,
                removed.network_profile.credential_reference(),
                removed.network_profile.mihomo_controller_secret_reference(),
            );
            data
        };

        let quarantine = root.join("silos").join(format!(".deleting-{silo_id}"));
        let moved_to_quarantine = if managed_directory.exists() {
            ensure_tree_has_no_links_or_reparse_points(&managed_directory)?;
            if fs::symlink_metadata(&quarantine).is_ok() {
                return Err(VaultError::UnmanagedProfile);
            }
            fs::rename(&managed_directory, &quarantine)?;
            true
        } else {
            false
        };

        if let Err(error) = self.persist_data(root, &prospective_data) {
            if moved_to_quarantine {
                let _ = fs::rename(&quarantine, &managed_directory);
            }
            return Err(error);
        }
        self.unlocked_mut_without_activity()?.data = prospective_data;
        if moved_to_quarantine {
            ensure_tree_has_no_links_or_reparse_points(&quarantine)?;
            fs::remove_dir_all(quarantine)?;
        }
        Ok(())
    }

    pub fn silo_storage_usage(
        &mut self,
        root: &Path,
        silo_id: Uuid,
    ) -> Result<SiloStorageUsage, VaultError> {
        let profile_directory = self.silo_profile_directory(silo_id)?;
        verified_managed_silo_directory(root, silo_id, &profile_directory)?;
        let bytes = if profile_directory.exists() {
            directory_size_without_links(&profile_directory)?
        } else {
            0
        };
        Ok(SiloStorageUsage {
            silo_id,
            profile_directory: profile_directory.to_string_lossy().to_string(),
            bytes,
        })
    }

    pub fn import_network_evidence(
        &mut self,
        root: &Path,
        entries: Vec<NativeNetworkEvidenceInboxEntry>,
    ) -> Result<usize, VaultError> {
        if entries.is_empty() {
            return Ok(0);
        }

        let prospective_data = {
            let unlocked = self.unlocked_mut_without_activity()?;
            let mut data = unlocked.data.clone();
            let silo_ids = data
                .silos
                .iter()
                .map(|silo| silo.id)
                .collect::<HashSet<_>>();
            let mut evidence_ids = data
                .network_evidence
                .iter()
                .map(|entry| entry.evidence_id)
                .collect::<HashSet<_>>();
            let mut request_ids = data
                .network_evidence
                .iter()
                .map(|entry| entry.request_id)
                .collect::<HashSet<_>>();
            let mut newly_seen = HashSet::new();

            for entry in entries {
                validate_network_evidence_inbox_entry(&entry)
                    .map_err(|_| VaultError::InvalidData)?;
                if !silo_ids.contains(&entry.silo_id) {
                    continue;
                }
                if evidence_ids.contains(&entry.evidence_id)
                    || request_ids.contains(&entry.request_id)
                {
                    continue;
                }
                evidence_ids.insert(entry.evidence_id);
                request_ids.insert(entry.request_id);
                newly_seen.insert(entry.evidence_id);
                data.network_evidence.push(entry);
            }

            data.network_evidence
                .sort_by_key(|entry| std::cmp::Reverse(entry.received_at));
            let mut retained_per_silo = HashMap::<Uuid, usize>::new();
            data.network_evidence.retain(|entry| {
                let retained = retained_per_silo.entry(entry.silo_id).or_default();
                if *retained >= MAX_NETWORK_EVIDENCE_PER_SILO {
                    return false;
                }
                *retained += 1;
                true
            });
            data.network_evidence.truncate(MAX_NETWORK_EVIDENCE_RECORDS);
            let retained_imported = data
                .network_evidence
                .iter()
                .filter(|entry| newly_seen.contains(&entry.evidence_id))
                .count();
            (data, retained_imported)
        };

        let (prospective_data, imported) = prospective_data;
        if imported > 0 {
            self.persist_data(root, &prospective_data)?;
            self.unlocked_mut_without_activity()?.data = prospective_data;
        }
        Ok(imported)
    }

    pub fn list_network_evidence(
        &mut self,
        silo_id: Option<Uuid>,
    ) -> Result<Vec<NativeNetworkEvidenceInboxEntry>, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        let mut evidence = unlocked
            .data
            .network_evidence
            .iter()
            .filter(|entry| silo_id.is_none_or(|id| entry.silo_id == id))
            .cloned()
            .collect::<Vec<_>>();
        evidence.sort_by_key(|entry| std::cmp::Reverse(entry.received_at));
        Ok(evidence)
    }

    pub fn clear_network_evidence(
        &mut self,
        root: &Path,
        silo_id: Uuid,
        confirm_clear: bool,
    ) -> Result<usize, VaultError> {
        if !confirm_clear {
            return Err(VaultError::PermanentDeleteNotConfirmed);
        }
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            if !unlocked.data.silos.iter().any(|silo| silo.id == silo_id) {
                return Err(VaultError::SiloNotFound);
            }
            let mut data = unlocked.data.clone();
            let before = data.network_evidence.len();
            data.network_evidence
                .retain(|entry| entry.silo_id != silo_id);
            (data, before)
        };
        let (prospective_data, before) = prospective_data;
        let removed = before.saturating_sub(prospective_data.network_evidence.len());
        if removed > 0 {
            self.persist_data(root, &prospective_data)?;
            self.unlocked_mut_without_activity()?.data = prospective_data;
        }
        Ok(removed)
    }

    pub(crate) fn remote_control_plane(&mut self) -> Result<RemoteVaultState, VaultError> {
        Ok(self
            .unlocked_without_activity()?
            .data
            .remote_control_plane
            .clone())
    }

    /// Atomically replaces the complete encrypted remote-control state. The
    /// caller performs a remote exchange while holding the Vault guard, then
    /// commits the new replay counters, credentials, bindings, evidence and
    /// result together so a partial local update is never observable.
    pub(crate) fn persist_remote_control_plane(
        &mut self,
        root: &Path,
        remote: RemoteVaultState,
    ) -> Result<(), VaultError> {
        let prospective_data = {
            let unlocked = self.unlocked_mut_for_activity()?;
            let mut data = unlocked.data.clone();
            data.remote_control_plane = remote;
            validate_remote_control_plane(&data)?;
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut_without_activity()?.data = prospective_data;
        Ok(())
    }

    fn silo_by_id(&mut self, silo_id: Uuid) -> Result<Silo, VaultError> {
        let unlocked = self.unlocked_without_activity()?;
        unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id)
            .cloned()
            .ok_or(VaultError::SiloNotFound)
    }

    /// Confirms that the Vault is still unlocked without treating the access
    /// as activity. Status, list and presentation-only reads must use this
    /// path so a visible desktop window cannot keep key material resident.
    fn unlocked_without_activity(&mut self) -> Result<&UnlockedVault, VaultError> {
        self.expire_if_needed();
        self.unlocked.as_ref().ok_or(VaultError::Locked)
    }

    /// Mutable counterpart for background state ingestion. Mutation alone is
    /// not proof of user activity (for example, importing a Native Host inbox
    /// during the periodic desktop status refresh).
    fn unlocked_mut_without_activity(&mut self) -> Result<&mut UnlockedVault, VaultError> {
        self.expire_if_needed();
        self.unlocked.as_mut().ok_or(VaultError::Locked)
    }

    /// Records one explicit user or sensitive operation against the
    /// authoritative Rust deadline. Callers must choose this deliberately;
    /// merely reading unlocked state never renews the deadline.
    pub(crate) fn record_activity(&mut self) -> Result<(), VaultError> {
        let now = self.now();
        self.expire_if_needed_at(now);
        let unlocked = self.unlocked.as_mut().ok_or(VaultError::Locked)?;
        unlocked.auto_lock_at = now + Duration::minutes(AUTO_LOCK_MINUTES);
        Ok(())
    }

    fn unlocked_mut_for_activity(&mut self) -> Result<&mut UnlockedVault, VaultError> {
        self.record_activity()?;
        self.unlocked.as_mut().ok_or(VaultError::Locked)
    }

    fn expire_if_needed(&mut self) {
        let now = self.now();
        self.expire_if_needed_at(now);
    }

    fn expire_if_needed_at(&mut self, now: chrono::DateTime<Utc>) {
        if self
            .unlocked
            .as_ref()
            .is_some_and(|unlocked| now >= unlocked.auto_lock_at)
        {
            self.unlocked = None;
        }
    }

    fn now(&self) -> chrono::DateTime<Utc> {
        #[cfg(test)]
        if let Some(now) = self.test_now {
            return now;
        }
        Utc::now()
    }

    fn auto_lock_time(&self) -> chrono::DateTime<Utc> {
        self.now() + Duration::minutes(AUTO_LOCK_MINUTES)
    }

    #[cfg(test)]
    fn set_test_now(&mut self, now: chrono::DateTime<Utc>) {
        self.test_now = Some(now);
    }

    fn persist(&self, root: &Path) -> Result<(), VaultError> {
        let unlocked = self.unlocked.as_ref().ok_or(VaultError::Locked)?;
        self.persist_data(root, &unlocked.data)
    }

    fn persist_data(&self, root: &Path, data: &VaultData) -> Result<(), VaultError> {
        recover_interrupted_write(root)?;
        let unlocked = self.unlocked.as_ref().ok_or(VaultError::Locked)?;
        let plaintext = Zeroizing::new(serde_json::to_vec(data)?);
        let nonce = random_bytes::<12>();
        let cipher = Aes256Gcm::new_from_slice(unlocked.dek.as_ref())
            .map_err(|_| VaultError::CryptographicSetup)?;
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
            .map_err(|_| VaultError::CryptographicSetup)?;
        let wrap_nonce = random_bytes::<12>();
        let wrapping_cipher = Aes256Gcm::new_from_slice(unlocked.kek.as_ref())
            .map_err(|_| VaultError::CryptographicSetup)?;
        let wrapped_dek = wrapping_cipher
            .encrypt(Nonce::from_slice(&wrap_nonce), unlocked.dek.as_ref())
            .map_err(|_| VaultError::CryptographicSetup)?;
        let envelope = VaultEnvelope {
            version: VAULT_ENVELOPE_VERSION,
            kdf: "argon2id".to_owned(),
            salt: STANDARD_NO_PAD.encode(unlocked.salt),
            wrap_nonce: Some(STANDARD_NO_PAD.encode(wrap_nonce)),
            wrapped_dek: Some(STANDARD_NO_PAD.encode(wrapped_dek)),
            nonce: STANDARD_NO_PAD.encode(nonce),
            ciphertext: STANDARD_NO_PAD.encode(ciphertext),
        };
        let encoded = serde_json::to_vec_pretty(&envelope)?;
        atomic_write(&vault_path(root), &encoded)
    }
}

fn remove_unreferenced_secrets(
    data: &mut VaultData,
    proxy_reference: Option<Uuid>,
    mihomo_reference: Option<Uuid>,
) {
    if let Some(reference) = proxy_reference {
        let still_used = data
            .silos
            .iter()
            .any(|silo| silo.network_profile.credential_reference() == Some(reference));
        if !still_used {
            data.proxy_credentials.remove(&reference);
        }
    }
    if let Some(reference) = mihomo_reference {
        let still_used = data.silos.iter().any(|silo| {
            silo.network_profile.mihomo_controller_secret_reference() == Some(reference)
        });
        if !still_used {
            data.mihomo_controller_secrets.remove(&reference);
        }
    }
}

fn verified_managed_silo_directory(
    root: &Path,
    silo_id: Uuid,
    profile_directory: &Path,
) -> Result<PathBuf, VaultError> {
    let silos_root = root.join("silos");
    let managed_directory = silos_root.join(silo_id.to_string());
    let expected_profile = managed_directory.join("browser-data");
    if profile_directory != expected_profile {
        return Err(VaultError::UnmanagedProfile);
    }

    for path in [&silos_root, &managed_directory, &expected_profile] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata_is_link_or_reparse(&metadata) => {
                return Err(VaultError::UnmanagedProfile)
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(VaultError::Filesystem(error)),
        }
    }
    Ok(managed_directory)
}

fn profile_has_browser_lock(profile_directory: &Path) -> bool {
    ["SingletonLock", "SingletonCookie", "SingletonSocket"]
        .iter()
        .any(|name| fs::symlink_metadata(profile_directory.join(name)).is_ok())
}

fn browser_paths_match(stored: &str, resolved: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        stored.eq_ignore_ascii_case(resolved)
    }
    #[cfg(not(target_os = "windows"))]
    {
        stored == resolved
    }
}

fn directory_size_without_links(path: &Path) -> Result<u64, VaultError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        bytes = bytes.saturating_add(directory_size_without_links(&entry?.path())?);
    }
    Ok(bytes)
}

fn ensure_tree_has_no_links_or_reparse_points(path: &Path) -> Result<(), VaultError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) {
        return Err(VaultError::UnmanagedProfile);
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            ensure_tree_has_no_links_or_reparse_points(&entry?.path())?;
        }
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;

        // FILE_ATTRIBUTE_REPARSE_POINT covers junctions and other reparse
        // points that are not always reported as Rust symbolic links.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

const VAULT_DATA_FIELDS_V1: &[&str] = &["schemaVersion", "silos", "seedMaterial"];
const VAULT_DATA_FIELDS_V2: &[&str] =
    &["schemaVersion", "silos", "seedMaterial", "proxyCredentials"];
const VAULT_DATA_FIELDS_V3: &[&str] = &[
    "schemaVersion",
    "silos",
    "seedMaterial",
    "proxyCredentials",
    "mihomoControllerSecrets",
];
const VAULT_DATA_FIELDS_V4: &[&str] = &[
    "schemaVersion",
    "silos",
    "seedMaterial",
    "proxyCredentials",
    "mihomoControllerSecrets",
    "networkEvidence",
];
const VAULT_DATA_FIELDS_V5_TO_V7: &[&str] = &[
    "schemaVersion",
    "silos",
    "seedMaterial",
    "proxyCredentials",
    "mihomoControllerSecrets",
    "networkEvidence",
    "remoteControlPlane",
];

fn expected_vault_data_fields(schema_version: u32) -> Option<&'static [&'static str]> {
    match schema_version {
        1 => Some(VAULT_DATA_FIELDS_V1),
        2 => Some(VAULT_DATA_FIELDS_V2),
        3 => Some(VAULT_DATA_FIELDS_V3),
        4 => Some(VAULT_DATA_FIELDS_V4),
        5 | 6 | VAULT_DATA_SCHEMA_VERSION => Some(VAULT_DATA_FIELDS_V5_TO_V7),
        _ => None,
    }
}

fn object_with_known_fields<'a>(
    value: &'a serde_json::Value,
    allowed: &[&str],
    required: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, VaultError> {
    let object = value.as_object().ok_or(VaultError::InvalidData)?;
    if object
        .keys()
        .any(|field| !allowed.contains(&field.as_str()))
        || required.iter().any(|field| !object.contains_key(*field))
    {
        return Err(VaultError::InvalidData);
    }
    Ok(object)
}

fn validate_silo_json_shape(value: &serde_json::Value) -> Result<(), VaultError> {
    const SILO_FIELDS: &[&str] = &[
        "id",
        "schemaVersion",
        "name",
        "color",
        "browser",
        "profileDirectory",
        "networkProfile",
        "engine",
        "seedReference",
        "createdAt",
        "archivedAt",
    ];
    const SILO_REQUIRED_FIELDS: &[&str] = &[
        "id",
        "schemaVersion",
        "name",
        "color",
        "browser",
        "profileDirectory",
        "networkProfile",
        "seedReference",
        "createdAt",
        "archivedAt",
    ];
    const BROWSER_FIELDS: &[&str] = &["kind", "executablePath", "version"];
    // `NetworkProfile` is an internally tagged enum whose historical Vault
    // representation used Rust field names. Keep that exact shape for stored
    // data even though surrounding Silo fields are camelCase.
    const DIRECT_FIELDS: &[&str] = &["mode", "proxy_required"];
    const FIXED_PROXY_FIELDS: &[&str] = &[
        "mode",
        "proxy_required",
        "scheme",
        "host",
        "port",
        "bypass_list",
        "credential_reference",
        "external_mihomo",
    ];
    const FIXED_PROXY_REQUIRED_FIELDS: &[&str] = &[
        "mode",
        "proxy_required",
        "scheme",
        "host",
        "port",
        "bypass_list",
    ];
    const PAC_FIELDS: &[&str] = &["mode", "proxy_required", "pac_url"];
    const MIHOMO_FIELDS: &[&str] = &[
        "controllerUrl",
        "selectorGroup",
        "nodeName",
        "controllerSecretReference",
    ];
    const MIHOMO_REQUIRED_FIELDS: &[&str] = &["controllerUrl", "selectorGroup", "nodeName"];

    let silo = object_with_known_fields(value, SILO_FIELDS, SILO_REQUIRED_FIELDS)?;
    object_with_known_fields(
        silo.get("browser").ok_or(VaultError::InvalidData)?,
        BROWSER_FIELDS,
        BROWSER_FIELDS,
    )?;
    let network_value = silo.get("networkProfile").ok_or(VaultError::InvalidData)?;
    let network = network_value.as_object().ok_or(VaultError::InvalidData)?;
    match network.get("mode").and_then(serde_json::Value::as_str) {
        Some("direct") => {
            object_with_known_fields(network_value, DIRECT_FIELDS, DIRECT_FIELDS)?;
        }
        Some("fixed_proxy") => {
            let network = object_with_known_fields(
                network_value,
                FIXED_PROXY_FIELDS,
                FIXED_PROXY_REQUIRED_FIELDS,
            )?;
            if let Some(binding) = network.get("external_mihomo") {
                object_with_known_fields(binding, MIHOMO_FIELDS, MIHOMO_REQUIRED_FIELDS)?;
            }
        }
        Some("pac") => {
            object_with_known_fields(network_value, PAC_FIELDS, PAC_FIELDS)?;
        }
        _ => return Err(VaultError::InvalidData),
    }
    Ok(())
}

fn deserialize_vault_data(plaintext: &[u8]) -> Result<(VaultData, u32), VaultError> {
    let value: serde_json::Value =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::InvalidData)?;
    let object = value.as_object().ok_or(VaultError::InvalidData)?;
    let schema_version = object
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or(VaultError::InvalidData)?;
    let expected_fields =
        expected_vault_data_fields(schema_version).ok_or(VaultError::InvalidData)?;
    object_with_known_fields(&value, expected_fields, expected_fields)?;

    let silos = object
        .get("silos")
        .and_then(serde_json::Value::as_array)
        .ok_or(VaultError::InvalidData)?;
    for silo in silos {
        validate_silo_json_shape(silo)?;
    }

    if schema_version >= 5 {
        const REMOTE_FIELDS_BEFORE_ORPHANS: &[&str] =
            &["endpoint", "backend", "lastResults", "pairingRevokedAt"];
        const REMOTE_FIELDS_CURRENT: &[&str] = &[
            "endpoint",
            "backend",
            "lastResults",
            "pairingRevokedAt",
            "orphanReceipts",
        ];
        const REMOTE_REQUIRED_FIELDS: &[&str] = &["backend", "lastResults"];
        const REMOTE_REQUIRED_FIELDS_CURRENT: &[&str] =
            &["backend", "lastResults", "orphanReceipts"];
        let remote = object
            .get("remoteControlPlane")
            .ok_or(VaultError::InvalidData)?;
        let remote = if schema_version < VAULT_DATA_SCHEMA_VERSION {
            object_with_known_fields(remote, REMOTE_FIELDS_BEFORE_ORPHANS, REMOTE_REQUIRED_FIELDS)?
        } else {
            object_with_known_fields(
                remote,
                REMOTE_FIELDS_CURRENT,
                REMOTE_REQUIRED_FIELDS_CURRENT,
            )?
        };
        if schema_version < 6 {
            let results = remote
                .get("lastResults")
                .and_then(serde_json::Value::as_object)
                .ok_or(VaultError::InvalidData)?;
            if results.values().any(|result| {
                result
                    .as_object()
                    .is_some_and(|result| result.contains_key("deletionProof"))
            }) {
                return Err(VaultError::InvalidData);
            }
        }
    }

    let data = serde_json::from_value(value).map_err(|_| VaultError::InvalidData)?;
    Ok((data, schema_version))
}

fn open_envelope(raw: &[u8], passphrase: &str) -> Result<OpenedEnvelope, VaultError> {
    let envelope: VaultEnvelope =
        serde_json::from_slice(raw).map_err(|_| VaultError::InvalidData)?;
    if !matches!(envelope.version, 1 | VAULT_ENVELOPE_VERSION) || envelope.kdf != "argon2id" {
        return Err(VaultError::InvalidData);
    }

    let salt = decode_fixed::<16>(&envelope.salt).ok_or(VaultError::InvalidData)?;
    let kek = derive_key(passphrase, &salt)?;
    let ciphertext = STANDARD_NO_PAD
        .decode(envelope.ciphertext.as_bytes())
        .map_err(|_| VaultError::InvalidData)?;
    let nonce = decode_fixed::<12>(&envelope.nonce).ok_or(VaultError::InvalidData)?;

    let migrated_from_legacy = envelope.version == 1;
    let dek = if migrated_from_legacy {
        // Version 1 used the password-derived key directly for data encryption.
        // A fresh random DEK is generated solely for the migrated version 2 envelope.
        Zeroizing::new(random_bytes())
    } else {
        let wrap_nonce = envelope
            .wrap_nonce
            .as_deref()
            .and_then(decode_fixed::<12>)
            .ok_or(VaultError::InvalidData)?;
        let wrapped_dek = envelope
            .wrapped_dek
            .as_deref()
            .map(|value| STANDARD_NO_PAD.decode(value.as_bytes()))
            .transpose()
            .map_err(|_| VaultError::InvalidData)?
            .ok_or(VaultError::InvalidData)?;
        let wrapping_cipher =
            Aes256Gcm::new_from_slice(kek.as_ref()).map_err(|_| VaultError::CryptographicSetup)?;
        let raw_dek = wrapping_cipher
            .decrypt(Nonce::from_slice(&wrap_nonce), wrapped_dek.as_ref())
            .map_err(|_| VaultError::InvalidPassphrase)?;
        let raw_dek: [u8; 32] = raw_dek.try_into().map_err(|_| VaultError::InvalidData)?;
        Zeroizing::new(raw_dek)
    };

    let data_key = if migrated_from_legacy {
        kek.as_ref()
    } else {
        dek.as_ref()
    };
    let cipher = Aes256Gcm::new_from_slice(data_key).map_err(|_| VaultError::CryptographicSetup)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| VaultError::InvalidPassphrase)?,
    );
    let (mut data, source_schema_version) = deserialize_vault_data(plaintext.as_ref())?;
    let migrated_data_schema = match source_schema_version {
        1..=6 => {
            data.schema_version = VAULT_DATA_SCHEMA_VERSION;
            true
        }
        VAULT_DATA_SCHEMA_VERSION => false,
        _ => return Err(VaultError::InvalidData),
    };
    validate_vault_data(&data)?;

    Ok(OpenedEnvelope {
        dek,
        kek,
        salt,
        data,
        needs_migration: migrated_from_legacy || migrated_data_schema,
    })
}

fn validate_vault_data(data: &VaultData) -> Result<(), VaultError> {
    if data.silos.len() > 10_000
        || data.seed_material.len() > 10_000
        || data.proxy_credentials.len() > 10_000
        || data.mihomo_controller_secrets.len() > 10_000
        || data.network_evidence.len() > MAX_NETWORK_EVIDENCE_RECORDS
    {
        return Err(VaultError::InvalidData);
    }
    if data.proxy_credentials.values().any(|credentials| {
        credentials.username.trim().is_empty()
            || credentials.username.len() > 512
            || credentials.password.len() > 1_024
            || credentials.username.chars().any(char::is_control)
            || credentials.password.chars().any(char::is_control)
    }) || data
        .mihomo_controller_secrets
        .values()
        .any(|stored| stored.secret.len() > 1_024 || stored.secret.chars().any(char::is_control))
    {
        return Err(VaultError::InvalidData);
    }

    let mut silo_ids = HashSet::new();
    let mut seed_references = HashSet::new();
    for silo in &data.silos {
        if silo.schema_version != SCHEMA_VERSION
            || !silo_ids.insert(silo.id)
            || !seed_references.insert(silo.seed_reference)
            || validate_silo_name(&silo.name).is_err()
            || silo.color.len() != 7
            || !silo.color.starts_with('#')
            || !silo
                .color
                .chars()
                .skip(1)
                .all(|value| value.is_ascii_hexdigit())
            || silo.browser.executable_path.is_empty()
            || silo.browser.executable_path.len() > 32_768
            || silo.browser.executable_path.chars().any(char::is_control)
            || silo
                .browser
                .version
                .as_ref()
                .is_some_and(|version| version.is_empty() || version.len() > 512)
            || silo.profile_directory.is_empty()
            || silo.profile_directory.len() > 32_768
            || silo.network_profile.validate().is_err()
            || silo.validate_engine().is_err()
        {
            return Err(VaultError::InvalidData);
        }

        let seed = data
            .seed_material
            .get(&silo.seed_reference)
            .and_then(|value| STANDARD_NO_PAD.decode(value.as_bytes()).ok())
            .ok_or(VaultError::InvalidData)?;
        if seed.len() != 32 {
            return Err(VaultError::InvalidData);
        }
        if silo
            .network_profile
            .credential_reference()
            .is_some_and(|reference| !data.proxy_credentials.contains_key(&reference))
            || silo
                .network_profile
                .mihomo_controller_secret_reference()
                .is_some_and(|reference| !data.mihomo_controller_secrets.contains_key(&reference))
        {
            return Err(VaultError::InvalidData);
        }
    }

    let mut evidence_ids = HashSet::new();
    let mut evidence_request_ids = HashSet::new();
    let mut evidence_per_silo = HashMap::<Uuid, usize>::new();
    for entry in &data.network_evidence {
        let count = evidence_per_silo.entry(entry.silo_id).or_default();
        *count += 1;
        if !silo_ids.contains(&entry.silo_id)
            || !evidence_ids.insert(entry.evidence_id)
            || !evidence_request_ids.insert(entry.request_id)
            || *count > MAX_NETWORK_EVIDENCE_PER_SILO
            || validate_network_evidence_inbox_entry(entry).is_err()
        {
            return Err(VaultError::InvalidData);
        }
    }
    validate_remote_control_plane(data)
}

fn validate_remote_control_plane(data: &VaultData) -> Result<(), VaultError> {
    let remote = &data.remote_control_plane;
    if remote.backend.bindings.len() > MAX_REMOTE_BINDINGS
        || remote.last_results.len() > MAX_REMOTE_OPERATION_RESULTS
        || remote.backend.used_pairing_token_ids.len() > MAX_USED_PAIRING_TOKENS
        || remote.orphan_receipts.len() > MAX_REMOTE_ORPHAN_RECEIPTS
    {
        return Err(VaultError::InvalidData);
    }
    if let Some(endpoint) = &remote.endpoint {
        endpoint.validate().map_err(|_| VaultError::InvalidData)?;
    }
    if let Some(pairing) = &remote.backend.pairing {
        if remote.endpoint.is_none()
            || pairing.server_id == Uuid::nil()
            || pairing.client_credential_id == Uuid::nil()
            || !(32..=512).contains(&pairing.client_credential.len())
            || !pairing
                .client_credential
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            || pairing.credential_expires_at_unix_ms == 0
            || pairing.last_server_sequence == 0
            || pairing.capabilities.len() != RemoteOperation::ALL.len()
        {
            return Err(VaultError::InvalidData);
        }
        pairing
            .node
            .validate()
            .map_err(|_| VaultError::InvalidData)?;
        for operation in RemoteOperation::ALL {
            if pairing
                .capabilities
                .iter()
                .filter(|capability| capability.operation == operation)
                .count()
                != 1
            {
                return Err(VaultError::InvalidData);
            }
        }
        if pairing.capabilities.iter().any(|capability| {
            matches!(
                &capability.availability,
                RemoteCapabilityAvailability::Unavailable { reason }
                    if reason.trim().is_empty() || reason.len() > 1_024
            )
        }) {
            return Err(VaultError::InvalidData);
        }
    }

    let silo_ids = data
        .silos
        .iter()
        .map(|silo| silo.id)
        .collect::<HashSet<_>>();
    if !remote.backend.bindings.is_empty() && remote.endpoint.is_none() {
        return Err(VaultError::InvalidData);
    }
    MemoryBindingStore::from_bindings(remote.backend.bindings.clone())
        .map_err(|_| VaultError::InvalidData)?;
    let mut bound_silos = HashSet::new();
    for binding in &remote.backend.bindings {
        if !silo_ids.contains(&binding.silo_id)
            || !bound_silos.insert(binding.silo_id)
            || binding.endpoint.validate().is_err()
            || remote
                .endpoint
                .as_ref()
                .is_none_or(|endpoint| endpoint != &binding.endpoint)
            || remote
                .backend
                .pairing
                .as_ref()
                .is_some_and(|pairing| pairing.server_id != binding.server_id)
            || !binding.volume.encrypted
            || binding.volume.key_custody
                != verisilo_remote_backend::agent::KeyCustody::UserControlled
            || binding.volume.volume_id == Uuid::nil()
            || binding.volume.key_id == Uuid::nil()
        {
            return Err(VaultError::InvalidData);
        }
    }
    let unique_tokens = remote
        .backend
        .used_pairing_token_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    if unique_tokens.len() != remote.backend.used_pairing_token_ids.len() {
        return Err(VaultError::InvalidData);
    }
    let mut orphan_receipt_ids = HashSet::new();
    let mut orphan_binding_ids = HashSet::new();
    let mut orphan_remote_identities = HashSet::new();
    for receipt in &remote.orphan_receipts {
        if !orphan_receipt_ids.insert(receipt.receipt_id)
            || !orphan_binding_ids.insert(receipt.binding_id)
            || !orphan_remote_identities.insert((receipt.server_id, receipt.remote_environment_id))
            || receipt.validate().is_err()
        {
            return Err(VaultError::InvalidData);
        }
    }
    for (silo_id, result) in &remote.last_results {
        if *silo_id != result.silo_id
            || !silo_ids.contains(silo_id)
            || result.server_id == Uuid::nil()
            || result.last_activity_at_unix_ms == 0
            || remote
                .backend
                .pairing
                .as_ref()
                .is_some_and(|pairing| pairing.server_id != result.server_id)
            || result.logs.as_ref().is_some_and(|logs| logs.len() > 200)
        {
            return Err(VaultError::InvalidData);
        }
        let volume_valid = match (&result.operation, &result.volume) {
            (RemoteOperation::Create, Some(volume)) => {
                volume.encrypted
                    && volume.key_custody
                        == verisilo_remote_backend::agent::KeyCustody::UserControlled
                    && volume.volume_id != Uuid::nil()
                    && volume.key_id != Uuid::nil()
            }
            (RemoteOperation::Create, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        let deletion_valid = match (&result.operation, &result.deletion_proof) {
            (RemoteOperation::Destroy, Some(proof)) => {
                let key_id = proof
                    .resource_deletions
                    .iter()
                    .find(|resource| {
                        resource.kind
                            == verisilo_remote_backend::agent::DeletionResourceKind::EphemeralKey
                    })
                    .and_then(|resource| resource.resource_id);
                proof.proof_id != Uuid::nil()
                    && proof.provider_receipt_id != Uuid::nil()
                    && proof.volume_id != Uuid::nil()
                    && proof.silo_id == result.silo_id
                    && proof.binding_id == result.binding_id
                    && proof.remote_environment_id == result.remote_environment_id
                    && proof.deleted_at_unix_ms == result.last_activity_at_unix_ms
                    && key_id.is_some_and(|key_id| {
                        verisilo_remote_backend::agent::deletion_resources_are_bound(
                            &proof.resource_deletions,
                            proof.remote_environment_id,
                            proof.volume_id,
                            key_id,
                        )
                    })
            }
            (RemoteOperation::Destroy, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        if !volume_valid || !deletion_valid {
            return Err(VaultError::InvalidData);
        }
    }
    Ok(())
}

fn validate_passphrase(passphrase: &str) -> Result<(), VaultError> {
    if passphrase.chars().count() < 12 {
        return Err(VaultError::InvalidSilo(
            "Vault passphrase must contain at least 12 characters.".to_owned(),
        ));
    }
    Ok(())
}

fn derive_key(passphrase: &str, salt: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_PARALLELISM, Some(32))
        .map_err(|_| VaultError::CryptographicSetup)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|_| VaultError::CryptographicSetup)?;
    Ok(key)
}

fn random_bytes<const SIZE: usize>() -> [u8; SIZE] {
    let mut bytes = [0_u8; SIZE];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn decode_fixed<const SIZE: usize>(value: &str) -> Option<[u8; SIZE]> {
    let decoded = STANDARD_NO_PAD.decode(value.as_bytes()).ok()?;
    decoded.try_into().ok()
}

fn vault_path(root: &Path) -> PathBuf {
    root.join(VAULT_FILE_NAME)
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), VaultError> {
    let temporary_path = path.with_extension("tmp");
    let mut temporary = fs::File::create(&temporary_path)?;
    temporary.write_all(contents)?;
    temporary.sync_all()?;
    drop(temporary);

    replace_file(&temporary_path, path)?;
    Ok(())
}

fn atomic_write_new(path: &Path, contents: &[u8]) -> Result<(), VaultError> {
    if path.exists() {
        return Err(VaultError::BackupDestinationExists);
    }
    let temporary_path = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let mut temporary = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)?;
    temporary.write_all(contents)?;
    temporary.sync_all()?;
    drop(temporary);

    // A hard link publishes the fully synced file without ever replacing an
    // existing destination. Both paths are in the same directory/filesystem.
    if let Err(error) = fs::hard_link(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        if path.exists() {
            return Err(VaultError::BackupDestinationExists);
        }
        return Err(VaultError::Filesystem(error));
    }
    fs::remove_file(temporary_path)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file(temporary_path: &Path, destination_path: &Path) -> Result<(), VaultError> {
    fs::rename(temporary_path, destination_path)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_file(temporary_path: &Path, destination_path: &Path) -> Result<(), VaultError> {
    // Windows does not replace an existing destination with std::fs::rename.
    // Keep a same-directory recovery copy until the new encrypted envelope is in place.
    let backup_path = destination_path.with_extension("bak");
    if destination_path.exists() {
        fs::copy(destination_path, &backup_path)?;
        fs::remove_file(destination_path)?;
    }
    if let Err(error) = fs::rename(temporary_path, destination_path) {
        if backup_path.exists() {
            let _ = fs::rename(&backup_path, destination_path);
        }
        return Err(VaultError::Filesystem(error));
    }
    if backup_path.exists() {
        fs::remove_file(backup_path)?;
    }
    Ok(())
}

fn recover_interrupted_write(root: &Path) -> Result<(), VaultError> {
    let destination_path = vault_path(root);
    let backup_path = destination_path.with_extension("bak");
    if !destination_path.exists() && backup_path.exists() {
        fs::rename(backup_path, destination_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
    use chrono::{Duration, Utc};
    use uuid::Uuid;
    use verisilo_remote_backend::{
        agent::{
            AutomationAuthorization, AutomationScope, CostDisclosure, DeletionProof,
            DeletionReason, DeletionResourceKind, DeletionResourceStatus, KeyCustody,
            NodeDisclosure, NodeOwnership, ResourceDeletionItem, ScreenChannel, ScreenTransport,
            SessionAuthorization, VolumeAttestation,
        },
        AgentControlOperation, AgentInteractionReceipt, CapabilityAvailability, EndpointOwnership,
        OperationResult, PairingSnapshot, RemoteCapability, RemoteNetworkPolicy, RemoteOperation,
        RemoteOrphanReceipt, RemoteResultState, SiloBinding, TlsPin, TlsPinKind,
        REMOTE_ORPHAN_NOTICE,
    };

    use super::{
        derive_key, open_envelope, RemoteVaultState, VaultData, VaultEnvelope, VaultError,
        VaultRuntime, VAULT_DATA_SCHEMA_VERSION,
    };
    use crate::domain::{
        BrowserKind, CreateSiloInput, NetworkProfile, ProxyCredentialsInput, ProxyScheme,
        UpdateSiloInput, UpdateSiloNetworkInput, VaultLockState, SCHEMA_VERSION,
    };
    use crate::native_host::{
        NativeDnsObservation, NativeDnsState, NativeDnssecState, NativeNetworkCheckResult,
        NativeNetworkEvidenceCoverage, NativeNetworkEvidenceInboxEntry,
        NativeReputationObservation, NativeReputationState, NETWORK_REPUTATION_EXPLANATION,
        PROTOCOL_VERSION,
    };

    fn temporary_root() -> std::path::PathBuf {
        env::temp_dir().join(format!("verisilo-vault-test-{}", Uuid::new_v4()))
    }

    fn create_test_browser(root: &std::path::Path) -> std::path::PathBuf {
        let browser = root.join("chrome.exe");
        fs::write(&browser, []).expect("create test browser file");
        fs::write(
            browser.with_extension("version-output"),
            "Google Chrome 126.0.6478.127\n",
        )
        .expect("create browser version harness");
        browser
    }

    fn evidence_entry(
        silo_id: Uuid,
        request_id: Uuid,
        received_at: chrono::DateTime<Utc>,
    ) -> NativeNetworkEvidenceInboxEntry {
        NativeNetworkEvidenceInboxEntry {
            schema_version: 1,
            protocol_version: PROTOCOL_VERSION,
            evidence_id: Uuid::new_v4(),
            request_id,
            silo_id,
            runtime_id: Uuid::new_v4(),
            received_at,
            expires_at: received_at + Duration::minutes(10),
            coverage: NativeNetworkEvidenceCoverage {
                trigger: "user_initiated".to_owned(),
                transport: "companion_extension_fetch".to_owned(),
                ip: "third_party_https_observation".to_owned(),
                public_dns: "public_doh_answer_comparison".to_owned(),
                actual_dns_path: "not_observed".to_owned(),
                web_rtc: "not_observed".to_owned(),
                quic: "not_observed".to_owned(),
            },
            result: NativeNetworkCheckResult {
                schema_version: 1,
                checked_at: received_at,
                ip: None,
                dns: NativeDnsObservation {
                    state: NativeDnsState::Failed,
                    dnssec: NativeDnssecState::Unavailable,
                    query_name: "example.com".to_owned(),
                    providers: Vec::new(),
                },
                reputation: NativeReputationObservation {
                    state: NativeReputationState::NotScored,
                    explanation: NETWORK_REPUTATION_EXPLANATION.to_owned(),
                },
                errors: vec!["No useful result.".to_owned()],
            },
        }
    }

    fn create_direct_silo(
        root: &std::path::Path,
        vault: &mut VaultRuntime,
        name: &str,
    ) -> crate::domain::Silo {
        let browser = create_test_browser(root);
        vault
            .create_silo(
                root,
                CreateSiloInput {
                    name: name.to_owned(),
                    color: "#4f46e5".to_owned(),
                    browser_kind: BrowserKind::Chrome,
                    executable_path: browser.to_string_lossy().to_string(),
                    network_profile: NetworkProfile::Direct {
                        proxy_required: false,
                    },
                    engine: Default::default(),
                    proxy_credentials: None,
                    mihomo_controller_secret: None,
                },
            )
            .expect("create direct Silo")
    }

    fn minimal_remote_state(silo_id: Uuid) -> RemoteVaultState {
        let endpoint = verisilo_remote_backend::RemoteEndpoint {
            ownership: EndpointOwnership::UserSelfHosted,
            origin: "https://remote.example.test:8443".to_owned(),
            pin: TlsPin {
                kind: TlsPinKind::SpkiSha256,
                sha256: "a".repeat(64),
            },
        };
        let server_id = Uuid::new_v4();
        RemoteVaultState {
            endpoint: Some(endpoint.clone()),
            backend: verisilo_remote_backend::RemoteBackendSnapshot {
                pairing: Some(PairingSnapshot {
                    server_id,
                    client_credential_id: Uuid::new_v4(),
                    node: NodeDisclosure {
                        node_id: Uuid::new_v4(),
                        ownership: NodeOwnership::UserSelfHosted,
                        operator_label: "Vault rotation test operator".to_owned(),
                        data_region: "test-region".to_owned(),
                        key_custody: KeyCustody::UserControlled,
                        cost: CostDisclosure {
                            currency: "USD".to_owned(),
                            estimated_micros_per_hour: 100_000,
                            notice: "Test resources may incur cost".to_owned(),
                        },
                    },
                    client_credential: "remote_credential_abcdefghijklmnopqrstuvwxyz0123456789"
                        .to_owned(),
                    credential_expires_at_unix_ms: 1_900_000_000_000,
                    capabilities: RemoteOperation::ALL
                        .into_iter()
                        .map(|operation| RemoteCapability {
                            operation,
                            availability: CapabilityAvailability::Available,
                        })
                        .collect(),
                    last_client_sequence: 7,
                    last_server_sequence: 8,
                }),
                used_pairing_token_ids: Vec::new(),
                bindings: vec![SiloBinding {
                    silo_id,
                    binding_id: Uuid::new_v4(),
                    remote_environment_id: Uuid::new_v4(),
                    server_id,
                    endpoint,
                    network: RemoteNetworkPolicy::Direct,
                    volume: VolumeAttestation {
                        encrypted: true,
                        key_custody: KeyCustody::UserControlled,
                        volume_id: Uuid::new_v4(),
                        key_id: Uuid::new_v4(),
                    },
                    last_activity_at_unix_ms: 1_800_000_000_000,
                    human_session: None,
                    automation_authorizations: Vec::new(),
                    last_screen_channel: None,
                    last_interaction: None,
                    last_evidence: None,
                }],
            },
            last_results: Default::default(),
            pairing_revoked_at: None,
            orphan_receipts: Vec::new(),
        }
    }

    const SCHEMA_FIXTURE_PASSPHRASE: &str = "schema fixture passphrase is long enough";
    const ROTATED_SCHEMA_FIXTURE_PASSPHRASE: &str =
        "rotated schema fixture passphrase is long enough";
    const FIXTURE_PROXY_USERNAME: &str = "fixture-proxy-user";
    const FIXTURE_PROXY_PASSWORD: &str = "fixture-proxy-password";
    const FIXTURE_MIHOMO_SECRET: &str = "fixture-mihomo-secret";

    fn fixture_silo_id() -> Uuid {
        Uuid::from_u128(0x11111111_2222_4333_8444_555555555555)
    }

    fn fixture_seed_reference() -> Uuid {
        Uuid::from_u128(0x21111111_2222_4333_8444_555555555555)
    }

    fn fixture_proxy_reference() -> Uuid {
        Uuid::from_u128(0x31111111_2222_4333_8444_555555555555)
    }

    fn fixture_mihomo_reference() -> Uuid {
        Uuid::from_u128(0x41111111_2222_4333_8444_555555555555)
    }

    fn remote_state_for_schema(silo_id: Uuid, schema_version: u32) -> RemoteVaultState {
        let mut remote = minimal_remote_state(silo_id);
        let binding = remote.backend.bindings[0].clone();
        let (operation, state, deletion_proof) = if schema_version >= 6 {
            let deleted_at_unix_ms = 1_800_000_100_000;
            let proof = DeletionProof {
                proof_id: Uuid::from_u128(0x51111111_2222_4333_8444_555555555555),
                silo_id,
                binding_id: binding.binding_id,
                remote_environment_id: binding.remote_environment_id,
                volume_id: binding.volume.volume_id,
                provider_receipt_id: Uuid::from_u128(0x61111111_2222_4333_8444_555555555555),
                resource_deletions: vec![
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::ComputeInstance,
                        resource_id: Some(binding.remote_environment_id),
                        status: DeletionResourceStatus::Deleted,
                    },
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::PersistentVolume,
                        resource_id: Some(binding.volume.volume_id),
                        status: DeletionResourceStatus::Deleted,
                    },
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::Snapshot,
                        resource_id: None,
                        status: DeletionResourceStatus::NotApplicable,
                    },
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::EphemeralKey,
                        resource_id: Some(binding.volume.key_id),
                        status: DeletionResourceStatus::Deleted,
                    },
                ],
                deleted_at_unix_ms,
                reason: DeletionReason::TtlExpired,
            };
            (
                RemoteOperation::Destroy,
                RemoteResultState::Destroyed,
                Some(proof),
            )
        } else {
            (RemoteOperation::Stop, RemoteResultState::Stopped, None)
        };
        remote.last_results.insert(
            silo_id,
            OperationResult {
                operation,
                silo_id,
                binding_id: binding.binding_id,
                remote_environment_id: binding.remote_environment_id,
                server_id: binding.server_id,
                last_activity_at_unix_ms: deletion_proof
                    .as_ref()
                    .map_or(binding.last_activity_at_unix_ms, |proof| {
                        proof.deleted_at_unix_ms
                    }),
                state,
                volume: None,
                evidence: None,
                logs: None,
                next_cursor: None,
                deletion_proof,
            },
        );
        remote
    }

    fn schema_fixture_payload(schema_version: u32, root: &std::path::Path) -> serde_json::Value {
        assert!((1..=VAULT_DATA_SCHEMA_VERSION).contains(&schema_version));
        let silo_id = fixture_silo_id();
        let seed_reference = fixture_seed_reference();
        let proxy_reference = fixture_proxy_reference();
        let mihomo_reference = fixture_mihomo_reference();
        let network_profile = match schema_version {
            1 => serde_json::json!({
                "mode": "direct",
                "proxy_required": false
            }),
            2 => serde_json::json!({
                "mode": "fixed_proxy",
                "proxy_required": true,
                "scheme": "http",
                "host": "proxy.example.test",
                "port": 8080,
                "bypass_list": [],
                "credential_reference": proxy_reference
            }),
            _ => serde_json::json!({
                "mode": "fixed_proxy",
                "proxy_required": true,
                "scheme": "socks5",
                "host": "127.0.0.1",
                "port": 1080,
                "bypass_list": [],
                "credential_reference": proxy_reference,
                "external_mihomo": {
                    "controllerUrl": "http://127.0.0.1:9090/",
                    "selectorGroup": "GLOBAL",
                    "nodeName": "fixture-node",
                    "controllerSecretReference": mihomo_reference
                }
            }),
        };
        let profile_directory = root
            .join("silos")
            .join(silo_id.to_string())
            .join("browser-data")
            .to_string_lossy()
            .to_string();
        let mut payload = serde_json::json!({
            "schemaVersion": schema_version,
            "silos": [{
                "id": silo_id,
                "schemaVersion": SCHEMA_VERSION,
                "name": format!("schema {schema_version} fixture"),
                "color": "#2457d6",
                "browser": {
                    "kind": "chrome",
                    "executablePath": root.join("fixture-chrome.exe").to_string_lossy(),
                    "version": "126.0.6478.127"
                },
                "profileDirectory": profile_directory,
                "networkProfile": network_profile,
                "seedReference": seed_reference,
                "createdAt": "2026-07-20T12:00:00Z",
                "archivedAt": null
            }],
            "seedMaterial": {
                (seed_reference.to_string()): STANDARD_NO_PAD.encode([0xa5_u8; 32])
            }
        });
        let object = payload.as_object_mut().expect("fixture payload object");
        if schema_version >= 2 {
            object.insert(
                "proxyCredentials".to_owned(),
                serde_json::json!({
                    (proxy_reference.to_string()): {
                        "username": FIXTURE_PROXY_USERNAME,
                        "password": FIXTURE_PROXY_PASSWORD
                    }
                }),
            );
        }
        if schema_version >= 3 {
            object.insert(
                "mihomoControllerSecrets".to_owned(),
                serde_json::json!({
                    (mihomo_reference.to_string()): { "secret": FIXTURE_MIHOMO_SECRET }
                }),
            );
        }
        if schema_version >= 4 {
            let received_at = chrono::DateTime::parse_from_rfc3339("2026-07-20T12:05:00Z")
                .expect("fixture timestamp")
                .with_timezone(&Utc);
            let mut evidence = serde_json::to_value(evidence_entry(
                silo_id,
                Uuid::from_u128(0x71111111_2222_4333_8444_555555555555),
                received_at,
            ))
            .expect("serialize evidence fixture");
            let evidence = evidence.as_object_mut().expect("evidence fixture object");
            evidence.remove("runtimeId");
            evidence.insert("protocolVersion".to_owned(), serde_json::json!(1));
            object.insert(
                "networkEvidence".to_owned(),
                serde_json::Value::Array(vec![serde_json::Value::Object(evidence.clone())]),
            );
        }
        if schema_version >= 5 {
            let mut remote = serde_json::to_value(remote_state_for_schema(silo_id, schema_version))
                .expect("serialize remote fixture");
            if schema_version < VAULT_DATA_SCHEMA_VERSION {
                remote
                    .as_object_mut()
                    .expect("remote fixture object")
                    .remove("orphanReceipts");
            }
            object.insert("remoteControlPlane".to_owned(), remote);
        }
        payload
    }

    fn encrypted_schema_fixture(
        schema_version: u32,
        payload: &serde_json::Value,
        passphrase: &str,
    ) -> Vec<u8> {
        let salt = [schema_version as u8; 16];
        let kek = derive_key(passphrase, &salt).expect("derive fixture KEK");
        let dek = [schema_version as u8 + 0x20; 32];
        let nonce = [schema_version as u8 + 0x40; 12];
        let wrap_nonce = [schema_version as u8 + 0x60; 12];
        let cipher = Aes256Gcm::new_from_slice(&dek).expect("create fixture data cipher");
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                serde_json::to_vec(payload)
                    .expect("serialize fixture payload")
                    .as_ref(),
            )
            .expect("encrypt fixture payload");
        let wrapping_cipher =
            Aes256Gcm::new_from_slice(kek.as_ref()).expect("create fixture wrapping cipher");
        let wrapped_dek = wrapping_cipher
            .encrypt(Nonce::from_slice(&wrap_nonce), dek.as_ref())
            .expect("wrap fixture DEK");
        serde_json::to_vec(&VaultEnvelope {
            version: 2,
            kdf: "argon2id".to_owned(),
            salt: STANDARD_NO_PAD.encode(salt),
            wrap_nonce: Some(STANDARD_NO_PAD.encode(wrap_nonce)),
            wrapped_dek: Some(STANDARD_NO_PAD.encode(wrapped_dek)),
            nonce: STANDARD_NO_PAD.encode(nonce),
            ciphertext: STANDARD_NO_PAD.encode(ciphertext),
        })
        .expect("serialize fixture envelope")
    }

    fn assert_schema_fixture_semantics(
        vault: &mut VaultRuntime,
        schema_version: u32,
        expected_root: &std::path::Path,
    ) {
        let silo_id = fixture_silo_id();
        let silos = vault.list_silos().expect("list migrated Silos");
        assert_eq!(silos.len(), 1, "schema {schema_version}");
        let silo = &silos[0];
        assert_eq!(silo.id, silo_id, "schema {schema_version}");
        assert_eq!(silo.name, format!("schema {schema_version} fixture"));
        assert!(silo.engine.is_stock(), "schema {schema_version}");
        assert_eq!(
            silo.profile_directory,
            expected_root
                .join("silos")
                .join(silo_id.to_string())
                .join("browser-data")
                .to_string_lossy(),
            "schema {schema_version}"
        );
        match (&silo.network_profile, schema_version) {
            (NetworkProfile::Direct { proxy_required }, 1) => assert!(!proxy_required),
            (
                NetworkProfile::FixedProxy {
                    credential_reference,
                    external_mihomo,
                    ..
                },
                2,
            ) => {
                assert_eq!(*credential_reference, Some(fixture_proxy_reference()));
                assert!(external_mihomo.is_none());
            }
            (
                NetworkProfile::FixedProxy {
                    credential_reference,
                    external_mihomo,
                    ..
                },
                3..=6,
            ) => {
                assert_eq!(*credential_reference, Some(fixture_proxy_reference()));
                assert_eq!(
                    external_mihomo
                        .as_ref()
                        .and_then(|binding| binding.controller_secret_reference),
                    Some(fixture_mihomo_reference())
                );
            }
            _ => panic!("schema {schema_version} network profile changed"),
        }
        assert_eq!(
            vault
                .identity_seed_for_silo(silo_id)
                .expect("restore identity seed")
                .as_ref(),
            &[0xa5_u8; 32],
            "schema {schema_version}"
        );
        let proxy = vault
            .proxy_authentication_for_silo(silo_id)
            .expect("restore proxy authentication");
        if schema_version >= 2 {
            let proxy = proxy.expect("schema with proxy credential");
            assert_eq!(proxy.username(), FIXTURE_PROXY_USERNAME);
            assert_eq!(proxy.password(), FIXTURE_PROXY_PASSWORD);
        } else {
            assert!(proxy.is_none());
        }
        let mihomo = vault
            .mihomo_controller_authentication_for_silo(silo_id)
            .expect("restore Mihomo authentication");
        if schema_version >= 3 {
            assert_eq!(
                mihomo.expect("schema with Mihomo secret").secret(),
                FIXTURE_MIHOMO_SECRET
            );
        } else {
            assert!(mihomo.is_none());
        }
        let evidence = vault
            .list_network_evidence(Some(silo_id))
            .expect("restore network evidence");
        if schema_version >= 4 {
            assert_eq!(evidence.len(), 1);
            assert_eq!(evidence[0].protocol_version, 1);
            assert!(evidence[0].runtime_id.is_nil());
            assert_eq!(evidence[0].coverage.actual_dns_path, "not_observed");
            assert!(!serde_json::to_string(&evidence[0])
                .expect("serialize migrated evidence")
                .contains("verified"));
        } else {
            assert!(evidence.is_empty());
        }
        let remote = vault.remote_control_plane().expect("restore remote state");
        if schema_version >= 5 {
            assert_eq!(
                remote
                    .endpoint
                    .as_ref()
                    .map(|endpoint| endpoint.origin.as_str()),
                Some("https://remote.example.test:8443")
            );
            assert_eq!(remote.backend.bindings.len(), 1);
            let result = remote
                .last_results
                .get(&silo_id)
                .expect("restore remote operation result");
            if schema_version >= 6 {
                assert_eq!(result.state, RemoteResultState::Destroyed);
                assert_eq!(
                    result.deletion_proof.as_ref().map(|proof| &proof.reason),
                    Some(&DeletionReason::TtlExpired)
                );
            } else {
                assert_eq!(result.state, RemoteResultState::Stopped);
                assert!(result.deletion_proof.is_none());
            }
        } else {
            assert!(remote.endpoint.is_none());
            assert!(remote.backend.bindings.is_empty());
            assert!(remote.last_results.is_empty());
        }
        assert!(remote.orphan_receipts.is_empty());
    }

    fn assert_open_rejected(raw: &[u8], passphrase: &str) -> VaultError {
        match open_envelope(raw, passphrase) {
            Ok(_) => panic!("invalid Vault fixture unexpectedly opened"),
            Err(error) => error,
        }
    }

    fn auto_lock_test_start() -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-07-28T12:00:00Z")
            .expect("valid auto-lock test timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn vault_locks_at_the_original_deadline_without_background_reads() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let start = auto_lock_test_start();
        let mut vault = VaultRuntime::default();
        vault.set_test_now(start);
        vault
            .initialize(&root, "an auto lock passphrase long enough")
            .expect("initialize Vault");

        vault.set_test_now(start + Duration::minutes(15));
        assert!(matches!(vault.status(&root).state, VaultLockState::Locked));

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn background_status_and_read_only_refreshes_do_not_extend_auto_lock() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let start = auto_lock_test_start();
        let deadline = start + Duration::minutes(15);
        let mut vault = VaultRuntime::default();
        vault.set_test_now(start);
        vault
            .initialize(&root, "an auto lock passphrase long enough")
            .expect("initialize Vault");
        let silo = create_direct_silo(&root, &mut vault, "auto-lock fixture");

        for elapsed_seconds in (30..15 * 60).step_by(30) {
            vault.set_test_now(start + Duration::seconds(elapsed_seconds));
            assert!(matches!(
                vault.status(&root).state,
                VaultLockState::Unlocked
            ));
            assert_eq!(vault.list_silos().expect("list all Silos").len(), 1);
            assert_eq!(
                vault.list_active_silos().expect("list active Silos").len(),
                1
            );
            assert!(vault
                .list_archived_silos()
                .expect("list archived Silos")
                .is_empty());
            assert!(vault
                .list_network_evidence(None)
                .expect("list network evidence")
                .is_empty());
            vault
                .silo_storage_usage(&root, silo.id)
                .expect("read Silo storage usage");
            vault
                .remote_control_plane()
                .expect("read remote control status");
            assert_eq!(vault.status(&root).auto_lock_at, Some(deadline));
        }

        vault.set_test_now(deadline);
        assert!(matches!(vault.status(&root).state, VaultLockState::Locked));
        let rejected_backup = root.join("locked-backup.json");
        assert!(matches!(
            vault.backup(&root, &rejected_backup),
            Err(VaultError::Locked)
        ));
        assert!(!rejected_backup.exists());

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn explicit_sensitive_operation_renews_auto_lock_once() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let start = auto_lock_test_start();
        let activity_at = start + Duration::minutes(14) + Duration::seconds(30);
        let renewed_deadline = activity_at + Duration::minutes(15);
        let mut vault = VaultRuntime::default();
        vault.set_test_now(start);
        vault
            .initialize(&root, "an auto lock passphrase long enough")
            .expect("initialize Vault");

        vault.set_test_now(activity_at);
        vault
            .backup(&root, &root.join("user-requested-backup.json"))
            .expect("explicit backup activity");
        assert_eq!(vault.status(&root).auto_lock_at, Some(renewed_deadline));

        vault.set_test_now(start + Duration::minutes(15));
        assert!(matches!(
            vault.status(&root).state,
            VaultLockState::Unlocked
        ));
        vault.set_test_now(renewed_deadline);
        assert!(matches!(vault.status(&root).state, VaultLockState::Locked));

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn vault_round_trip_rejects_an_incorrect_passphrase() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let passphrase = "a passphrase that is long enough";
        let mut vault = VaultRuntime::default();

        vault
            .initialize(&root, passphrase)
            .expect("initialize encrypted vault");
        let envelope: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("vault.json")).expect("read encrypted vault"),
        )
        .expect("parse vault envelope");
        assert_eq!(envelope["version"], 2);
        assert!(envelope["wrappedDek"].as_str().is_some());
        assert!(envelope["ciphertext"].as_str().is_some());
        assert!(matches!(
            vault.status(&root).state,
            VaultLockState::Unlocked
        ));
        vault.lock();
        assert!(vault.unlock(&root, "an incorrect long passphrase").is_err());
        vault
            .unlock(&root, passphrase)
            .expect("unlock encrypted vault");
        assert!(matches!(
            vault.status(&root).state,
            VaultLockState::Unlocked
        ));

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn interrupted_windows_style_replacement_is_reported_and_recovered_as_locked() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let passphrase = "a passphrase that is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize Vault");
        vault.lock();
        fs::rename(root.join("vault.json"), root.join("vault.bak"))
            .expect("simulate interrupted replacement");

        assert!(matches!(vault.status(&root).state, VaultLockState::Locked));
        assert!(root.join("vault.json").is_file());
        vault
            .unlock(&root, passphrase)
            .expect("unlock recovered Vault");

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn legacy_vaults_are_rewrapped_with_a_random_dek_on_unlock() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let passphrase = "a passphrase that is long enough";
        let salt = [7_u8; 16];
        let legacy_key = derive_key(passphrase, &salt).expect("derive legacy key");
        let nonce = [9_u8; 12];
        let plaintext = serde_json::to_vec(&VaultData {
            schema_version: VAULT_DATA_SCHEMA_VERSION,
            silos: Vec::new(),
            seed_material: Default::default(),
            proxy_credentials: Default::default(),
            mihomo_controller_secrets: Default::default(),
            network_evidence: Vec::new(),
            remote_control_plane: RemoteVaultState::default(),
        })
        .expect("serialize legacy vault data");
        let cipher =
            Aes256Gcm::new_from_slice(legacy_key.as_ref()).expect("create legacy vault cipher");
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
            .expect("encrypt legacy vault data");
        let legacy_envelope = VaultEnvelope {
            version: 1,
            kdf: "argon2id".to_owned(),
            salt: STANDARD_NO_PAD.encode(salt),
            wrap_nonce: None,
            wrapped_dek: None,
            nonce: STANDARD_NO_PAD.encode(nonce),
            ciphertext: STANDARD_NO_PAD.encode(ciphertext),
        };
        fs::write(
            root.join("vault.json"),
            serde_json::to_vec(&legacy_envelope).expect("serialize legacy envelope"),
        )
        .expect("write legacy vault");

        let mut vault = VaultRuntime::default();
        vault
            .unlock(&root, passphrase)
            .expect("unlock and migrate legacy vault");

        let migrated: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("vault.json")).expect("read migrated vault"),
        )
        .expect("parse migrated envelope");
        assert_eq!(migrated["version"], 2);
        assert!(migrated["wrappedDek"].as_str().is_some());

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn every_supported_data_schema_migrates_atomically_and_reopens_without_semantic_loss() {
        for schema_version in 1..=6 {
            let root = temporary_root().join(format!("schema-{schema_version}"));
            fs::create_dir_all(&root).expect("create schema fixture root");
            let payload = schema_fixture_payload(schema_version, &root);
            let raw = encrypted_schema_fixture(schema_version, &payload, SCHEMA_FIXTURE_PASSPHRASE);
            let raw_text = String::from_utf8_lossy(&raw);
            for sensitive in [
                FIXTURE_PROXY_USERNAME,
                FIXTURE_PROXY_PASSWORD,
                FIXTURE_MIHOMO_SECRET,
                "schema 6 fixture",
                "remote_credential_abcdefghijklmnopqrstuvwxyz0123456789",
            ] {
                assert!(!raw_text.contains(sensitive), "schema {schema_version}");
            }
            fs::write(root.join("vault.json"), &raw).expect("write encrypted schema fixture");

            let wrong_passphrase_error =
                assert_open_rejected(&raw, "incorrect schema fixture passphrase long enough");
            assert!(matches!(
                wrong_passphrase_error,
                VaultError::InvalidPassphrase
            ));
            let error_text = wrong_passphrase_error.to_string();
            assert!(!error_text.contains(SCHEMA_FIXTURE_PASSPHRASE));
            assert!(!error_text.contains(FIXTURE_PROXY_PASSWORD));
            assert_eq!(
                fs::read(root.join("vault.json")).expect("read rejected fixture"),
                raw,
                "schema {schema_version}"
            );

            let mut vault = VaultRuntime::default();
            vault
                .unlock(&root, SCHEMA_FIXTURE_PASSPHRASE)
                .unwrap_or_else(|error| panic!("migrate schema {schema_version}: {error}"));
            assert_schema_fixture_semantics(&mut vault, schema_version, &root);

            let migrated = fs::read(root.join("vault.json")).expect("read migrated Vault");
            assert_ne!(migrated, raw, "schema {schema_version}");
            assert!(!root.join("vault.tmp").exists(), "schema {schema_version}");
            assert!(!root.join("vault.bak").exists(), "schema {schema_version}");
            let opened = open_envelope(&migrated, SCHEMA_FIXTURE_PASSPHRASE)
                .unwrap_or_else(|error| panic!("open schema {schema_version} migration: {error}"));
            assert_eq!(opened.data.schema_version, VAULT_DATA_SCHEMA_VERSION);
            assert!(!opened.needs_migration);

            vault.lock();
            vault
                .unlock(&root, SCHEMA_FIXTURE_PASSPHRASE)
                .unwrap_or_else(|error| panic!("reopen schema {schema_version}: {error}"));
            assert_schema_fixture_semantics(&mut vault, schema_version, &root);
            assert_eq!(
                fs::read(root.join("vault.json")).expect("read stable migration"),
                migrated,
                "current schema reopen must not rewrite schema {schema_version} migration"
            );

            fs::remove_dir_all(root).expect("remove schema fixture root");
        }
    }

    #[test]
    fn encrypted_backup_restore_migrates_every_supported_schema_and_rotation_remains_stable() {
        for schema_version in 1..=6 {
            let source_root = temporary_root().join(format!("backup-source-{schema_version}"));
            let destination_root =
                temporary_root().join(format!("backup-destination-{schema_version}"));
            fs::create_dir_all(&source_root).expect("create backup source root");
            fs::create_dir_all(&destination_root).expect("create backup destination root");
            let payload = schema_fixture_payload(schema_version, &source_root);
            let backup_bytes =
                encrypted_schema_fixture(schema_version, &payload, SCHEMA_FIXTURE_PASSPHRASE);
            let backup_path = source_root.join(format!("schema-{schema_version}-backup.json"));
            fs::write(&backup_path, &backup_bytes).expect("write encrypted legacy backup");

            let mut restored = VaultRuntime::default();
            restored
                .restore(
                    &destination_root,
                    &backup_path,
                    SCHEMA_FIXTURE_PASSPHRASE,
                    false,
                )
                .unwrap_or_else(|error| {
                    panic!("restore and migrate schema {schema_version}: {error}")
                });
            assert_schema_fixture_semantics(&mut restored, schema_version, &destination_root);
            let migrated =
                fs::read(destination_root.join("vault.json")).expect("read restored migration");
            assert_ne!(migrated, backup_bytes, "schema {schema_version}");
            assert_eq!(
                open_envelope(&migrated, SCHEMA_FIXTURE_PASSPHRASE)
                    .expect("open restored migration")
                    .data
                    .schema_version,
                VAULT_DATA_SCHEMA_VERSION
            );

            if schema_version == 6 {
                restored
                    .change_passphrase(
                        &destination_root,
                        SCHEMA_FIXTURE_PASSPHRASE,
                        ROTATED_SCHEMA_FIXTURE_PASSPHRASE,
                    )
                    .expect("rotate migrated Vault passphrase");
                restored.lock();
                assert!(matches!(
                    restored.unlock(&destination_root, SCHEMA_FIXTURE_PASSPHRASE),
                    Err(VaultError::InvalidPassphrase)
                ));
                restored
                    .unlock(&destination_root, ROTATED_SCHEMA_FIXTURE_PASSPHRASE)
                    .expect("unlock rotated migrated Vault");
                assert_schema_fixture_semantics(&mut restored, schema_version, &destination_root);

                let current_backup = destination_root.join("current-encrypted-backup.json");
                restored
                    .backup(&destination_root, &current_backup)
                    .expect("back up rotated migrated Vault");
                let current_backup_text =
                    String::from_utf8_lossy(&fs::read(&current_backup).expect("read backup"))
                        .into_owned();
                for sensitive in [
                    FIXTURE_PROXY_PASSWORD,
                    FIXTURE_MIHOMO_SECRET,
                    "remote_credential_abcdefghijklmnopqrstuvwxyz0123456789",
                ] {
                    assert!(!current_backup_text.contains(sensitive));
                }
                let second_restore_root = temporary_root().join("rotated-backup-restore");
                fs::create_dir_all(&second_restore_root).expect("create second restore root");
                let mut second_restore = VaultRuntime::default();
                second_restore
                    .restore(
                        &second_restore_root,
                        &current_backup,
                        ROTATED_SCHEMA_FIXTURE_PASSPHRASE,
                        false,
                    )
                    .expect("restore rotated current backup");
                assert_schema_fixture_semantics(
                    &mut second_restore,
                    schema_version,
                    &second_restore_root,
                );
                second_restore.lock();
                second_restore
                    .unlock(&second_restore_root, ROTATED_SCHEMA_FIXTURE_PASSPHRASE)
                    .expect("reopen restored rotated backup");
                assert_schema_fixture_semantics(
                    &mut second_restore,
                    schema_version,
                    &second_restore_root,
                );
                fs::remove_dir_all(second_restore_root).expect("remove second restore root");
            } else {
                restored.lock();
                restored
                    .unlock(&destination_root, SCHEMA_FIXTURE_PASSPHRASE)
                    .expect("reopen restored migration");
                assert_schema_fixture_semantics(&mut restored, schema_version, &destination_root);
            }

            fs::remove_dir_all(source_root).expect("remove backup source root");
            fs::remove_dir_all(destination_root).expect("remove backup destination root");
        }
    }

    #[test]
    fn schema_field_matrix_rejects_omissions_unknowns_and_downgrades() {
        let root = temporary_root();
        for schema_version in 1..=6 {
            let payload = schema_fixture_payload(schema_version, &root);
            let expected_fields = super::expected_vault_data_fields(schema_version)
                .expect("supported schema field set");
            assert!(super::deserialize_vault_data(
                &serde_json::to_vec(&payload).expect("serialize valid payload")
            )
            .is_ok());
            for field in expected_fields {
                let mut missing = payload.clone();
                missing
                    .as_object_mut()
                    .expect("fixture object")
                    .remove(*field);
                assert!(
                    super::deserialize_vault_data(
                        &serde_json::to_vec(&missing).expect("serialize missing-field payload")
                    )
                    .is_err(),
                    "schema {schema_version} accepted missing {field}"
                );
            }

            let mut unknown = payload.clone();
            unknown
                .as_object_mut()
                .expect("fixture object")
                .insert("futureCriticalState".to_owned(), serde_json::json!(true));
            assert!(super::deserialize_vault_data(
                &serde_json::to_vec(&unknown).expect("serialize unknown-field payload")
            )
            .is_err());

            let mut unknown_silo = payload.clone();
            unknown_silo["silos"][0]["futureCredentialReference"] =
                serde_json::json!(Uuid::new_v4());
            assert!(super::deserialize_vault_data(
                &serde_json::to_vec(&unknown_silo).expect("serialize unknown Silo field")
            )
            .is_err());
        }

        let mut downgraded = schema_fixture_payload(6, &root);
        downgraded["schemaVersion"] = serde_json::json!(1);
        let downgraded = encrypted_schema_fixture(6, &downgraded, SCHEMA_FIXTURE_PASSPHRASE);
        assert!(matches!(
            assert_open_rejected(&downgraded, SCHEMA_FIXTURE_PASSPHRASE),
            VaultError::InvalidData
        ));

        let mut schema_five_with_future_deletion = schema_fixture_payload(6, &root);
        schema_five_with_future_deletion["schemaVersion"] = serde_json::json!(5);
        let schema_five_with_future_deletion = encrypted_schema_fixture(
            5,
            &schema_five_with_future_deletion,
            SCHEMA_FIXTURE_PASSPHRASE,
        );
        assert!(matches!(
            assert_open_rejected(&schema_five_with_future_deletion, SCHEMA_FIXTURE_PASSPHRASE),
            VaultError::InvalidData
        ));

        for unsupported_version in [0, 8, u32::MAX] {
            let mut payload = schema_fixture_payload(6, &root);
            payload["schemaVersion"] = serde_json::json!(unsupported_version);
            let raw = encrypted_schema_fixture(6, &payload, SCHEMA_FIXTURE_PASSPHRASE);
            assert!(matches!(
                assert_open_rejected(&raw, SCHEMA_FIXTURE_PASSPHRASE),
                VaultError::InvalidData
            ));
        }
    }

    #[test]
    fn vault_network_profile_wire_stays_strict_while_public_json_is_camel_case() {
        let root = temporary_root();
        for schema_version in 1..=VAULT_DATA_SCHEMA_VERSION {
            let payload = schema_fixture_payload(schema_version, &root);
            let encoded = serde_json::to_vec(&payload).expect("serialize schema fixture");
            let (mut data, parsed_schema) =
                super::deserialize_vault_data(&encoded).expect("parse strict Vault wire");
            assert_eq!(parsed_schema, schema_version);

            let public_profile =
                serde_json::to_value(&data.silos[0].network_profile).expect("public profile JSON");
            let public = public_profile.as_object().expect("public profile object");
            assert!(public.contains_key("proxyRequired"));
            assert!(!public.contains_key("proxy_required"));
            if public_profile["mode"] == "fixed_proxy" {
                assert!(public.contains_key("bypassList"));
                assert!(!public.contains_key("bypass_list"));
                assert!(!public.contains_key("credential_reference"));
                assert!(!public.contains_key("external_mihomo"));
            }
            let public_round_trip: NetworkProfile =
                serde_json::from_value(public_profile.clone()).expect("round-trip public profile");
            assert_eq!(
                serde_json::to_value(public_round_trip).expect("serialize public round-trip"),
                public_profile
            );

            let credentials =
                serde_json::to_value(&data.proxy_credentials).expect("credential snapshot");
            let evidence = serde_json::to_value(&data.network_evidence).expect("evidence snapshot");
            data.schema_version = VAULT_DATA_SCHEMA_VERSION;
            let persisted = serde_json::to_value(&data).expect("serialize current Vault wire");
            let persisted_profile = persisted["silos"][0]["networkProfile"]
                .as_object()
                .expect("persisted profile object");
            assert!(persisted_profile.contains_key("proxy_required"));
            assert!(!persisted_profile.contains_key("proxyRequired"));
            if persisted["silos"][0]["networkProfile"]["mode"] == "fixed_proxy" {
                assert!(persisted_profile.contains_key("bypass_list"));
                assert!(!persisted_profile.contains_key("bypassList"));
                assert!(!persisted_profile.contains_key("credentialReference"));
                assert!(!persisted_profile.contains_key("externalMihomo"));
            }
            let persisted_bytes =
                serde_json::to_vec(&persisted).expect("serialize current persisted payload");
            let (persisted_data, persisted_schema) =
                super::deserialize_vault_data(&persisted_bytes).expect("reopen current Vault wire");
            assert_eq!(persisted_schema, VAULT_DATA_SCHEMA_VERSION);
            assert_eq!(
                serde_json::to_value(&persisted_data.proxy_credentials)
                    .expect("reopened credentials"),
                credentials
            );
            assert_eq!(
                serde_json::to_value(&persisted_data.network_evidence).expect("reopened evidence"),
                evidence
            );

            let original_required = payload["silos"][0]["networkProfile"]["proxy_required"].clone();
            let mut duplicate_spelling = payload.clone();
            duplicate_spelling["silos"][0]["networkProfile"]
                .as_object_mut()
                .expect("duplicate profile object")
                .insert("proxyRequired".to_owned(), original_required.clone());
            assert!(super::deserialize_vault_data(
                &serde_json::to_vec(&duplicate_spelling).expect("serialize duplicate spelling")
            )
            .is_err());

            let mut camel_spelling = payload;
            let camel_profile = camel_spelling["silos"][0]["networkProfile"]
                .as_object_mut()
                .expect("camel profile object");
            camel_profile.remove("proxy_required");
            camel_profile.insert("proxyRequired".to_owned(), original_required);
            assert!(super::deserialize_vault_data(
                &serde_json::to_vec(&camel_spelling).expect("serialize camel Vault spelling")
            )
            .is_err());
        }
    }

    #[test]
    fn envelope_metadata_ciphertext_and_unknown_fields_fail_closed() {
        let root = temporary_root();
        let payload = schema_fixture_payload(6, &root);
        let raw = encrypted_schema_fixture(6, &payload, SCHEMA_FIXTURE_PASSPHRASE);
        let envelope: serde_json::Value =
            serde_json::from_slice(&raw).expect("parse encrypted fixture envelope");

        let mut unknown = envelope.clone();
        unknown
            .as_object_mut()
            .expect("envelope object")
            .insert("futureKdfParameters".to_owned(), serde_json::json!({}));
        assert!(matches!(
            assert_open_rejected(
                &serde_json::to_vec(&unknown).expect("serialize unknown envelope"),
                SCHEMA_FIXTURE_PASSPHRASE
            ),
            VaultError::InvalidData
        ));

        for field in [
            "version",
            "kdf",
            "salt",
            "wrapNonce",
            "wrappedDek",
            "nonce",
            "ciphertext",
        ] {
            let mut missing = envelope.clone();
            missing
                .as_object_mut()
                .expect("envelope object")
                .remove(field);
            assert_open_rejected(
                &serde_json::to_vec(&missing).expect("serialize missing metadata"),
                SCHEMA_FIXTURE_PASSPHRASE,
            );
        }

        let mutations = [
            ("version", serde_json::json!(1)),
            ("version", serde_json::json!(3)),
            ("kdf", serde_json::json!("argon2i")),
            (
                "salt",
                serde_json::json!(STANDARD_NO_PAD.encode([0x91_u8; 16])),
            ),
            (
                "wrapNonce",
                serde_json::json!(STANDARD_NO_PAD.encode([0x92_u8; 12])),
            ),
            (
                "wrappedDek",
                serde_json::json!(STANDARD_NO_PAD.encode([0x93_u8; 48])),
            ),
            (
                "nonce",
                serde_json::json!(STANDARD_NO_PAD.encode([0x94_u8; 12])),
            ),
            (
                "ciphertext",
                serde_json::json!(STANDARD_NO_PAD.encode([0x95_u8; 48])),
            ),
        ];
        for (field, replacement) in mutations {
            let mut tampered = envelope.clone();
            tampered[field] = replacement;
            let error = assert_open_rejected(
                &serde_json::to_vec(&tampered).expect("serialize tampered envelope"),
                SCHEMA_FIXTURE_PASSPHRASE,
            );
            let error_text = error.to_string();
            assert!(!error_text.contains(SCHEMA_FIXTURE_PASSPHRASE));
            assert!(!error_text.contains(FIXTURE_PROXY_PASSWORD));
            assert!(!error_text.contains(FIXTURE_MIHOMO_SECRET));
        }
    }

    #[test]
    fn locked_vault_does_not_create_a_profile_directory() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let browser = create_test_browser(&root);
        let mut vault = VaultRuntime::default();
        let input = CreateSiloInput {
            name: "locked".to_owned(),
            color: "#4f46e5".to_owned(),
            browser_kind: BrowserKind::Chrome,
            executable_path: browser.to_string_lossy().to_string(),
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
            engine: Default::default(),
            proxy_credentials: None,
            mihomo_controller_secret: None,
        };

        assert!(vault.create_silo(&root, input).is_err());
        assert!(!root.join("silos").exists());

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn proxy_credentials_are_encrypted_and_resolved_only_while_unlocked() {
        use crate::domain::{ProxyCredentialsInput, ProxyScheme};

        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let browser = create_test_browser(&root);
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, "a passphrase that is long enough")
            .expect("initialize vault");

        let silo = vault
            .create_silo(
                &root,
                CreateSiloInput {
                    name: "authenticated proxy".to_owned(),
                    color: "#4f46e5".to_owned(),
                    browser_kind: BrowserKind::Chrome,
                    executable_path: browser.to_string_lossy().to_string(),
                    network_profile: NetworkProfile::FixedProxy {
                        proxy_required: true,
                        scheme: ProxyScheme::Socks5,
                        host: "proxy.example.test".to_owned(),
                        port: 1080,
                        bypass_list: Vec::new(),
                        credential_reference: None,
                        external_mihomo: None,
                    },
                    engine: Default::default(),
                    proxy_credentials: Some(ProxyCredentialsInput {
                        username: "alice".to_owned(),
                        password: "vault-only-secret".to_owned(),
                    }),
                    mihomo_controller_secret: None,
                },
            )
            .expect("create authenticated Silo");

        let raw_vault = fs::read(root.join("vault.json")).expect("read encrypted vault");
        assert!(!String::from_utf8_lossy(&raw_vault).contains("vault-only-secret"));
        assert!(silo.network_profile.credential_reference().is_some());
        let authentication = vault
            .proxy_authentication_for_silo(silo.id)
            .expect("read credential reference")
            .expect("credential exists");
        assert_eq!(authentication.username(), "alice");
        assert_eq!(authentication.password(), "vault-only-secret");
        let credential_reference = silo
            .network_profile
            .credential_reference()
            .expect("credential reference");
        vault
            .update_silo_network(
                &root,
                silo.id,
                UpdateSiloNetworkInput {
                    network_profile: NetworkProfile::Direct {
                        proxy_required: false,
                    },
                    proxy_credentials: None,
                    mihomo_controller_secret: None,
                },
                false,
            )
            .expect("clear proxy credentials with network replacement");
        assert!(!vault
            .unlocked
            .as_ref()
            .expect("unlocked Vault")
            .data
            .proxy_credentials
            .contains_key(&credential_reference));
        assert!(vault
            .proxy_authentication_for_silo(silo.id)
            .expect("resolve cleared authentication")
            .is_none());

        vault.lock();
        assert!(vault.proxy_authentication_for_silo(silo.id).is_err());
        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn mihomo_controller_secrets_are_encrypted_and_reference_only() {
        use crate::domain::{ExternalMihomoBinding, MihomoControllerSecretInput, ProxyScheme};

        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let browser = create_test_browser(&root);
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, "a passphrase that is long enough")
            .expect("initialize vault");
        let silo = vault
            .create_silo(
                &root,
                CreateSiloInput {
                    name: "mihomo node".to_owned(),
                    color: "#4f46e5".to_owned(),
                    browser_kind: BrowserKind::Chrome,
                    executable_path: browser.to_string_lossy().to_string(),
                    network_profile: NetworkProfile::FixedProxy {
                        proxy_required: true,
                        scheme: ProxyScheme::Socks5,
                        host: "127.0.0.1".to_owned(),
                        port: 7890,
                        bypass_list: Vec::new(),
                        credential_reference: None,
                        external_mihomo: Some(ExternalMihomoBinding {
                            controller_url: "http://127.0.0.1:9090/".to_owned(),
                            selector_group: "GLOBAL".to_owned(),
                            node_name: "Tokyo 01".to_owned(),
                            controller_secret_reference: None,
                        }),
                    },
                    engine: Default::default(),
                    proxy_credentials: None,
                    mihomo_controller_secret: Some(MihomoControllerSecretInput {
                        secret: "controller-vault-secret".to_owned(),
                    }),
                },
            )
            .expect("create Mihomo-bound Silo");

        let raw_vault = fs::read(root.join("vault.json")).expect("read encrypted vault");
        assert!(!String::from_utf8_lossy(&raw_vault).contains("controller-vault-secret"));
        assert!(silo
            .network_profile
            .mihomo_controller_secret_reference()
            .is_some());
        let authentication = vault
            .mihomo_controller_authentication_for_silo(silo.id)
            .expect("read controller secret reference")
            .expect("controller secret exists");
        assert_eq!(authentication.secret(), "controller-vault-secret");

        vault.lock();
        assert!(vault
            .mihomo_controller_authentication_for_silo(silo.id)
            .is_err());
        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn silo_updates_preserve_identity_and_archive_state_can_be_restored() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let passphrase = "a passphrase that is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize vault");
        let original = create_direct_silo(&root, &mut vault, "original");
        let edge = root.join("msedge.exe");
        fs::write(&edge, []).expect("create Edge test browser");
        fs::write(
            edge.with_extension("version-output"),
            "Microsoft Edge 126.0.2592.87\n",
        )
        .expect("create Edge version output");
        let seed_before = vault
            .unlocked
            .as_ref()
            .expect("unlocked vault")
            .data
            .seed_material
            .get(&original.seed_reference)
            .cloned()
            .expect("seed exists");

        assert!(matches!(
            vault.update_silo(
                &root,
                original.id,
                UpdateSiloInput {
                    name: "must not apply while running".to_owned(),
                    color: "#16a34a".to_owned(),
                    browser_kind: BrowserKind::Edge,
                    executable_path: edge.to_string_lossy().to_string(),
                },
                true,
            ),
            Err(VaultError::SiloRunning)
        ));

        let updated = vault
            .update_silo(
                &root,
                original.id,
                UpdateSiloInput {
                    name: "updated".to_owned(),
                    color: "#16a34a".to_owned(),
                    browser_kind: BrowserKind::Edge,
                    executable_path: edge.to_string_lossy().to_string(),
                },
                false,
            )
            .expect("update Silo metadata");
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.profile_directory, original.profile_directory);
        assert_eq!(updated.created_at, original.created_at);
        assert_eq!(updated.seed_reference, original.seed_reference);
        assert_eq!(updated.name, "updated");
        assert_eq!(
            vault
                .unlocked
                .as_ref()
                .expect("unlocked vault")
                .data
                .seed_material
                .get(&updated.seed_reference),
            Some(&seed_before)
        );

        let renamed = vault
            .rename_silo(&root, original.id, "renamed", false)
            .expect("rename Silo");
        assert_eq!(renamed.name, "renamed");
        assert_eq!(renamed.id, original.id);
        let profile = std::path::PathBuf::from(&original.profile_directory);
        fs::write(profile.join("SingletonLock"), []).expect("create profile lock");
        assert!(matches!(
            vault.rename_silo(&root, original.id, "blocked", false),
            Err(VaultError::SiloProfileInUse)
        ));
        fs::remove_file(profile.join("SingletonLock")).expect("remove profile lock");

        vault
            .archive_silo(&root, original.id, false)
            .expect("archive Silo");
        let first_archived_at = vault.list_archived_silos().expect("archive list")[0].archived_at;
        vault
            .archive_silo(&root, original.id, false)
            .expect("archive is idempotent");
        assert_eq!(
            vault.list_archived_silos().expect("archive list")[0].archived_at,
            first_archived_at
        );
        assert!(vault.list_active_silos().expect("active list").is_empty());
        assert_eq!(vault.list_archived_silos().expect("archive list").len(), 1);
        let restored = vault
            .restore_archived_silo(&root, original.id)
            .expect("restore archived Silo");
        assert!(restored.archived_at.is_none());
        assert!(vault
            .restore_archived_silo(&root, original.id)
            .expect("restore is idempotent")
            .archived_at
            .is_none());
        assert_eq!(vault.list_active_silos().expect("active list").len(), 1);
        assert!(vault
            .list_archived_silos()
            .expect("archive list")
            .is_empty());

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn network_replacement_keeps_identity_and_replaces_encrypted_credentials() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, "a passphrase that is long enough")
            .expect("initialize vault");
        let original = create_direct_silo(&root, &mut vault, "network update");

        let updated = vault
            .update_silo_network(
                &root,
                original.id,
                UpdateSiloNetworkInput {
                    network_profile: NetworkProfile::FixedProxy {
                        proxy_required: true,
                        scheme: ProxyScheme::Socks5,
                        host: "proxy.example.test".to_owned(),
                        port: 1080,
                        bypass_list: Vec::new(),
                        credential_reference: None,
                        external_mihomo: None,
                    },
                    proxy_credentials: Some(ProxyCredentialsInput {
                        username: "alice".to_owned(),
                        password: "updated-secret".to_owned(),
                    }),
                    mihomo_controller_secret: None,
                },
                false,
            )
            .expect("replace network profile");
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.profile_directory, original.profile_directory);
        assert_eq!(updated.created_at, original.created_at);
        assert_eq!(updated.seed_reference, original.seed_reference);
        let authentication = vault
            .proxy_authentication_for_silo(original.id)
            .expect("resolve authentication")
            .expect("authentication exists");
        assert_eq!(authentication.password(), "updated-secret");
        assert!(!String::from_utf8_lossy(
            &fs::read(root.join("vault.json")).expect("read encrypted vault")
        )
        .contains("updated-secret"));

        fs::write(
            std::path::Path::new(&original.profile_directory).join("SingletonLock"),
            [],
        )
        .expect("create profile lock");
        assert!(matches!(
            vault.update_silo_network(
                &root,
                original.id,
                UpdateSiloNetworkInput {
                    network_profile: NetworkProfile::Direct {
                        proxy_required: false,
                    },
                    proxy_credentials: None,
                    mihomo_controller_secret: None,
                },
                false,
            ),
            Err(VaultError::SiloProfileInUse)
        ));

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn permanent_delete_is_confirmed_locked_and_confined_to_the_managed_directory() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, "a passphrase that is long enough")
            .expect("initialize vault");
        let silo = create_direct_silo(&root, &mut vault, "delete me");
        let profile = std::path::PathBuf::from(&silo.profile_directory);
        fs::write(profile.join("content.bin"), [1_u8, 2, 3, 4, 5, 6])
            .expect("write profile content");
        let usage = vault
            .silo_storage_usage(&root, silo.id)
            .expect("measure profile");
        assert_eq!(usage.bytes, 6);

        assert!(matches!(
            vault.delete_silo(&root, silo.id, false, false),
            Err(VaultError::PermanentDeleteNotConfirmed)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let link_target = root.join("outside-link-target");
            fs::create_dir_all(&link_target).expect("create link target");
            let profile_link = profile.join("outside-link");
            symlink(&link_target, &profile_link).expect("create profile symlink");
            assert!(matches!(
                vault.delete_silo(&root, silo.id, false, true),
                Err(VaultError::UnmanagedProfile)
            ));
            assert!(link_target.exists());
            fs::remove_file(profile_link).expect("remove profile symlink");
        }
        fs::write(profile.join("SingletonLock"), []).expect("create profile lock");
        assert!(matches!(
            vault.delete_silo(&root, silo.id, false, true),
            Err(VaultError::SiloProfileInUse)
        ));
        fs::remove_file(profile.join("SingletonLock")).expect("remove profile lock");

        let unrelated = root.join("default-profile");
        fs::create_dir_all(&unrelated).expect("create unrelated profile");
        fs::write(unrelated.join("keep.txt"), b"keep").expect("write unrelated data");
        {
            let unlocked = vault.unlocked.as_mut().expect("unlocked vault");
            unlocked
                .data
                .silos
                .iter_mut()
                .find(|candidate| candidate.id == silo.id)
                .expect("Silo exists")
                .profile_directory = unrelated.to_string_lossy().to_string();
        }
        assert!(matches!(
            vault.delete_silo(&root, silo.id, false, true),
            Err(VaultError::UnmanagedProfile)
        ));
        assert!(unrelated.join("keep.txt").exists());
        vault
            .unlocked
            .as_mut()
            .expect("unlocked vault")
            .data
            .silos
            .iter_mut()
            .find(|candidate| candidate.id == silo.id)
            .expect("Silo exists")
            .profile_directory = silo.profile_directory.clone();

        vault
            .delete_silo(&root, silo.id, false, true)
            .expect("delete managed Silo");
        assert!(!root.join("silos").join(silo.id.to_string()).exists());
        assert!(unrelated.join("keep.txt").exists());
        assert!(vault.list_silos().expect("list Silo metadata").is_empty());

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn passphrase_change_rewraps_the_same_dek_and_invalidates_the_old_passphrase() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let old_passphrase = "the old passphrase is long enough";
        let new_passphrase = "the new passphrase is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, old_passphrase)
            .expect("initialize vault");
        create_direct_silo(&root, &mut vault, "preserved");
        let before = fs::read(root.join("vault.json")).expect("read original envelope");
        let old_dek = open_envelope(&before, old_passphrase)
            .expect("open original envelope")
            .dek
            .to_vec();

        vault
            .change_passphrase(&root, old_passphrase, new_passphrase)
            .expect("change passphrase");
        let after = fs::read(root.join("vault.json")).expect("read rewrapped envelope");
        let new_dek = open_envelope(&after, new_passphrase)
            .expect("open rewrapped envelope")
            .dek
            .to_vec();
        assert_eq!(old_dek, new_dek);
        assert_ne!(
            serde_json::from_slice::<serde_json::Value>(&before).expect("old envelope")["salt"],
            serde_json::from_slice::<serde_json::Value>(&after).expect("new envelope")["salt"]
        );

        vault.lock();
        assert!(matches!(
            vault.unlock(&root, old_passphrase),
            Err(VaultError::InvalidPassphrase)
        ));
        vault
            .unlock(&root, new_passphrase)
            .expect("unlock with new passphrase");
        assert_eq!(vault.list_silos().expect("preserved Silo").len(), 1);

        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn encrypted_backup_restore_validates_passphrase_and_requires_overwrite_confirmation() {
        let source_root = temporary_root();
        let destination_root = temporary_root();
        fs::create_dir_all(&source_root).expect("create source root");
        fs::create_dir_all(&destination_root).expect("create destination root");
        let passphrase = "a backup passphrase that is long enough";
        let mut source = VaultRuntime::default();
        source
            .initialize(&source_root, passphrase)
            .expect("initialize source Vault");
        create_direct_silo(&source_root, &mut source, "from backup");
        let backup_path = source_root.join("verisilo-backup.json");
        let receipt = source
            .backup(&source_root, &backup_path)
            .expect("create encrypted backup");
        assert_eq!(receipt.bytes, fs::metadata(&backup_path).unwrap().len());
        assert!(!String::from_utf8_lossy(&fs::read(&backup_path).unwrap()).contains("from backup"));

        let mut destination = VaultRuntime::default();
        destination
            .initialize(
                &destination_root,
                "an existing passphrase that is long enough",
            )
            .expect("initialize destination Vault");
        create_direct_silo(&destination_root, &mut destination, "existing");
        let existing = fs::read(destination_root.join("vault.json")).expect("read destination");
        assert!(matches!(
            destination.restore(&destination_root, &backup_path, passphrase, false),
            Err(VaultError::RestoreOverwriteNotConfirmed)
        ));
        assert_eq!(
            fs::read(destination_root.join("vault.json")).expect("unchanged destination"),
            existing
        );
        assert!(matches!(
            destination.restore(
                &destination_root,
                &backup_path,
                "an incorrect backup passphrase",
                true,
            ),
            Err(VaultError::InvalidPassphrase)
        ));

        destination
            .restore(&destination_root, &backup_path, passphrase, true)
            .expect("restore encrypted backup");
        let restored = destination.list_silos().expect("list restored Silo");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "from backup");
        assert_eq!(
            restored[0].profile_directory,
            destination_root
                .join("silos")
                .join(restored[0].id.to_string())
                .join("browser-data")
                .to_string_lossy()
        );

        fs::remove_dir_all(source_root).expect("remove source root");
        fs::remove_dir_all(destination_root).expect("remove destination root");
    }

    #[test]
    fn network_evidence_is_deduplicated_bounded_and_persists_encrypted() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let passphrase = "an evidence passphrase that is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize Vault");
        let silo = create_direct_silo(&root, &mut vault, "evidence history");
        let now = Utc::now();
        let first_request_id = Uuid::new_v4();
        let mut entries = (0..105)
            .map(|index| {
                evidence_entry(
                    silo.id,
                    if index == 0 {
                        first_request_id
                    } else {
                        Uuid::new_v4()
                    },
                    now - Duration::seconds(index),
                )
            })
            .collect::<Vec<_>>();
        entries.push(evidence_entry(
            silo.id,
            first_request_id,
            now + Duration::seconds(1),
        ));

        assert_eq!(
            vault
                .import_network_evidence(&root, entries)
                .expect("import evidence"),
            100
        );
        let listed = vault
            .list_network_evidence(Some(silo.id))
            .expect("list evidence");
        assert_eq!(listed.len(), 100);
        assert!(listed
            .windows(2)
            .all(|pair| pair[0].received_at >= pair[1].received_at));
        assert_eq!(
            listed
                .iter()
                .filter(|entry| entry.request_id == first_request_id)
                .count(),
            1
        );
        let encrypted = fs::read(root.join("vault.json")).expect("read encrypted Vault");
        assert!(!String::from_utf8_lossy(&encrypted).contains("evidence history"));

        vault.lock();
        vault
            .unlock(&root, passphrase)
            .expect("unlock persisted Vault");
        assert_eq!(
            vault
                .list_network_evidence(Some(silo.id))
                .expect("list persisted evidence")
                .len(),
            100
        );
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn network_evidence_clear_and_silo_delete_remove_only_the_bound_history() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, "an evidence passphrase that is long enough")
            .expect("initialize Vault");
        let first = create_direct_silo(&root, &mut vault, "first");
        let second = create_direct_silo(&root, &mut vault, "second");
        vault
            .import_network_evidence(
                &root,
                vec![
                    evidence_entry(first.id, Uuid::new_v4(), Utc::now()),
                    evidence_entry(second.id, Uuid::new_v4(), Utc::now()),
                ],
            )
            .expect("import evidence");

        assert!(vault
            .clear_network_evidence(&root, first.id, false)
            .is_err());
        assert_eq!(
            vault
                .clear_network_evidence(&root, first.id, true)
                .expect("clear first history"),
            1
        );
        assert!(vault
            .list_network_evidence(Some(first.id))
            .expect("first history")
            .is_empty());
        assert_eq!(
            vault
                .list_network_evidence(Some(second.id))
                .expect("second history")
                .len(),
            1
        );

        vault
            .delete_silo(&root, second.id, false, true)
            .expect("delete second Silo");
        assert!(vault
            .list_network_evidence(None)
            .expect("remaining history")
            .is_empty());
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn schema_three_vault_without_new_fields_migrates_to_current_schema() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let passphrase = "a migration passphrase that is long enough";
        let salt = [3_u8; 16];
        let kek = derive_key(passphrase, &salt).expect("derive KEK");
        let dek = [4_u8; 32];
        let nonce = [5_u8; 12];
        let wrap_nonce = [6_u8; 12];
        let legacy_data = serde_json::json!({
            "schemaVersion": 3,
            "silos": [],
            "seedMaterial": {},
            "proxyCredentials": {},
            "mihomoControllerSecrets": {}
        });
        let cipher = Aes256Gcm::new_from_slice(&dek).expect("create data cipher");
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                serde_json::to_vec(&legacy_data)
                    .expect("serialize legacy data")
                    .as_ref(),
            )
            .expect("encrypt legacy data");
        let wrapping_cipher = Aes256Gcm::new_from_slice(kek.as_ref()).expect("create wrap cipher");
        let wrapped_dek = wrapping_cipher
            .encrypt(Nonce::from_slice(&wrap_nonce), dek.as_ref())
            .expect("wrap DEK");
        let envelope = VaultEnvelope {
            version: 2,
            kdf: "argon2id".to_owned(),
            salt: STANDARD_NO_PAD.encode(salt),
            wrap_nonce: Some(STANDARD_NO_PAD.encode(wrap_nonce)),
            wrapped_dek: Some(STANDARD_NO_PAD.encode(wrapped_dek)),
            nonce: STANDARD_NO_PAD.encode(nonce),
            ciphertext: STANDARD_NO_PAD.encode(ciphertext),
        };
        fs::write(
            root.join("vault.json"),
            serde_json::to_vec(&envelope).expect("serialize envelope"),
        )
        .expect("write legacy Vault");

        let mut vault = VaultRuntime::default();
        vault
            .unlock(&root, passphrase)
            .expect("migrate schema three");
        assert!(vault
            .list_network_evidence(None)
            .expect("empty migrated history")
            .is_empty());
        let migrated = open_envelope(
            &fs::read(root.join("vault.json")).expect("read migrated Vault"),
            passphrase,
        )
        .expect("open migrated Vault");
        assert_eq!(migrated.data.schema_version, VAULT_DATA_SCHEMA_VERSION);
        assert!(migrated.data.network_evidence.is_empty());
        assert!(migrated.data.remote_control_plane.endpoint.is_none());
        assert!(migrated
            .data
            .remote_control_plane
            .backend
            .bindings
            .is_empty());
        assert!(migrated
            .data
            .remote_control_plane
            .orphan_receipts
            .is_empty());
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn remote_pairing_binding_and_results_are_encrypted_and_require_unlock() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let passphrase = "a remote state passphrase that is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize Vault");
        let silo = create_direct_silo(&root, &mut vault, "remote bound");
        let endpoint = verisilo_remote_backend::RemoteEndpoint {
            ownership: EndpointOwnership::UserSelfHosted,
            origin: "https://remote.example.test:8443".to_owned(),
            pin: TlsPin {
                kind: TlsPinKind::SpkiSha256,
                sha256: "a".repeat(64),
            },
        };
        let server_id = Uuid::new_v4();
        let client_credential_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let remote_environment_id = Uuid::new_v4();
        let human_authorization_id = Uuid::new_v4();
        let automation_authorization_id = Uuid::new_v4();
        let screen_channel_id = Uuid::new_v4();
        let issued_at_unix_ms = 1_800_000_000_000;
        let credential = "remote_credential_abcdefghijklmnopqrstuvwxyz0123456789".to_owned();
        let capabilities = RemoteOperation::ALL
            .into_iter()
            .map(|operation| RemoteCapability {
                operation,
                availability: CapabilityAvailability::Available,
            })
            .collect();
        let mut remote = RemoteVaultState {
            endpoint: Some(endpoint.clone()),
            backend: verisilo_remote_backend::RemoteBackendSnapshot {
                pairing: Some(PairingSnapshot {
                    server_id,
                    client_credential_id,
                    node: NodeDisclosure {
                        node_id: Uuid::new_v4(),
                        ownership: NodeOwnership::UserSelfHosted,
                        operator_label: "Vault test operator".to_owned(),
                        data_region: "test-region".to_owned(),
                        key_custody: KeyCustody::UserControlled,
                        cost: CostDisclosure {
                            currency: "USD".to_owned(),
                            estimated_micros_per_hour: 100_000,
                            notice: "Test infrastructure cost notice".to_owned(),
                        },
                    },
                    client_credential: credential.clone(),
                    credential_expires_at_unix_ms: 1_900_000_000_000,
                    capabilities,
                    last_client_sequence: 7,
                    last_server_sequence: 4,
                }),
                used_pairing_token_ids: vec![Uuid::new_v4()],
                bindings: vec![SiloBinding {
                    silo_id: silo.id,
                    binding_id,
                    remote_environment_id,
                    server_id,
                    endpoint,
                    network: RemoteNetworkPolicy::Direct,
                    volume: VolumeAttestation {
                        encrypted: true,
                        key_custody: KeyCustody::UserControlled,
                        volume_id: Uuid::new_v4(),
                        key_id: Uuid::new_v4(),
                    },
                    last_activity_at_unix_ms: issued_at_unix_ms,
                    human_session: Some(SessionAuthorization {
                        authorization_id: human_authorization_id,
                        silo_id: silo.id,
                        remote_environment_id,
                        issued_at_unix_ms,
                        expires_at_unix_ms: issued_at_unix_ms + 600_000,
                        revoked: false,
                    }),
                    automation_authorizations: vec![AutomationAuthorization {
                        authorization_id: automation_authorization_id,
                        silo_id: silo.id,
                        remote_environment_id,
                        issued_at_unix_ms,
                        expires_at_unix_ms: issued_at_unix_ms + 300_000,
                        scopes: vec![AutomationScope::ReadScreen, AutomationScope::SendInput],
                        approved_by_user: true,
                        revoked: false,
                    }],
                    last_screen_channel: Some(ScreenChannel {
                        channel_id: screen_channel_id,
                        remote_environment_id,
                        authorization_id: human_authorization_id,
                        expires_at_unix_ms: issued_at_unix_ms + 60_000,
                        transport: ScreenTransport::AuthenticatedEncryptedStream,
                    }),
                    last_interaction: Some(AgentInteractionReceipt {
                        operation: AgentControlOperation::OpenScreen,
                        observed_at_unix_ms: issued_at_unix_ms,
                        response: verisilo_remote_backend::agent::AgentResponse::Screen {
                            channel: ScreenChannel {
                                channel_id: screen_channel_id,
                                remote_environment_id,
                                authorization_id: human_authorization_id,
                                expires_at_unix_ms: issued_at_unix_ms + 60_000,
                                transport: ScreenTransport::AuthenticatedEncryptedStream,
                            },
                        },
                    }),
                    last_evidence: None,
                }],
            },
            last_results: Default::default(),
            pairing_revoked_at: None,
            orphan_receipts: Vec::new(),
        };
        remote.last_results.insert(
            silo.id,
            OperationResult {
                operation: RemoteOperation::Stop,
                silo_id: silo.id,
                binding_id,
                remote_environment_id,
                server_id,
                last_activity_at_unix_ms: issued_at_unix_ms,
                state: RemoteResultState::Stopped,
                volume: None,
                evidence: None,
                logs: None,
                next_cursor: None,
                deletion_proof: None,
            },
        );
        vault
            .persist_remote_control_plane(&root, remote)
            .expect("persist remote state");

        let envelope = fs::read(root.join("vault.json")).expect("read encrypted Vault");
        let envelope_text = String::from_utf8_lossy(&envelope);
        assert!(!envelope_text.contains(&credential));
        assert!(!envelope_text.contains("remote.example.test"));
        assert!(matches!(
            vault.delete_silo(&root, silo.id, false, true),
            Err(VaultError::SiloRemoteBound)
        ));

        vault.lock();
        assert!(matches!(
            vault.remote_control_plane(),
            Err(VaultError::Locked)
        ));
        vault.unlock(&root, passphrase).expect("unlock Vault");
        let restored = vault.remote_control_plane().expect("restore remote state");
        assert_eq!(
            restored.endpoint.as_ref().unwrap().origin,
            "https://remote.example.test:8443"
        );
        assert_eq!(restored.backend.bindings.len(), 1);
        assert_eq!(
            restored.backend.bindings[0]
                .human_session
                .as_ref()
                .unwrap()
                .authorization_id,
            human_authorization_id
        );
        assert_eq!(
            restored.backend.bindings[0]
                .automation_authorizations
                .first()
                .unwrap()
                .authorization_id,
            automation_authorization_id
        );
        assert_eq!(
            restored.backend.bindings[0]
                .last_screen_channel
                .as_ref()
                .unwrap()
                .channel_id,
            screen_channel_id
        );
        assert_eq!(
            restored.last_results[&silo.id].state,
            RemoteResultState::Stopped
        );

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn failed_final_rotation_persist_keeps_old_state_and_durable_token_reservation() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let passphrase = "a rotation persistence passphrase long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize Vault");
        let silo = create_direct_silo(&root, &mut vault, "rotation persistence");
        let original = minimal_remote_state(silo.id);
        vault
            .persist_remote_control_plane(&root, original.clone())
            .expect("persist original remote state");

        let token_id = Uuid::new_v4();
        let mut reserved = original.clone();
        reserved.backend.used_pairing_token_ids.push(token_id);
        reserved
            .backend
            .pairing
            .as_mut()
            .unwrap()
            .last_client_sequence += 1;
        reserved
            .backend
            .pairing
            .as_mut()
            .unwrap()
            .last_server_sequence += 1;
        vault
            .persist_remote_control_plane(&root, reserved.clone())
            .expect("persist replay reservation");

        let mut candidate = reserved.clone();
        let mut new_endpoint = candidate.endpoint.clone().unwrap();
        new_endpoint.pin.sha256 = "b".repeat(64);
        candidate.endpoint = Some(new_endpoint.clone());
        candidate
            .backend
            .pairing
            .as_mut()
            .unwrap()
            .client_credential_id = Uuid::new_v4();
        candidate
            .backend
            .pairing
            .as_mut()
            .unwrap()
            .client_credential =
            "rotated_credential_abcdefghijklmnopqrstuvwxyz0123456789".to_owned();
        for binding in &mut candidate.backend.bindings {
            binding.endpoint = new_endpoint.clone();
        }

        // Make only the final destination unavailable. persist_remote_control_plane
        // must not mutate the unlocked in-memory state before atomic_write
        // succeeds.
        let moved_root = root.with_extension("rotation-persist-test");
        fs::rename(&root, &moved_root).expect("temporarily move Vault root");
        fs::write(&root, b"not a directory").expect("block Vault root path");
        let persist_result = vault.persist_remote_control_plane(&root, candidate);
        fs::remove_file(&root).expect("remove blocking file");
        fs::rename(&moved_root, &root).expect("restore Vault root");
        assert!(persist_result.is_err());

        let current = vault.remote_control_plane().expect("read current state");
        assert_eq!(current.endpoint, original.endpoint);
        assert_eq!(current.backend.pairing, reserved.backend.pairing);
        assert_eq!(current.backend.bindings, original.backend.bindings);
        assert_eq!(current.backend.used_pairing_token_ids, vec![token_id]);

        vault.lock();
        vault
            .unlock(&root, passphrase)
            .expect("reopen durable Vault");
        let durable = vault.remote_control_plane().expect("read durable state");
        assert_eq!(durable.endpoint, original.endpoint);
        assert_eq!(durable.backend.pairing, reserved.backend.pairing);
        assert_eq!(durable.backend.bindings, original.backend.bindings);
        assert_eq!(durable.backend.used_pairing_token_ids, vec![token_id]);

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn old_ttl_provider_receipt_persists_after_authenticated_binding_recovery() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let passphrase = "a TTL recovery passphrase that is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize Vault");
        let silo = create_direct_silo(&root, &mut vault, "expired remote");
        let mut remote = minimal_remote_state(silo.id);
        let binding = remote.backend.bindings.remove(0);
        let deleted_at_unix_ms = 1_700_000_000_000;
        let proof = DeletionProof {
            proof_id: Uuid::new_v4(),
            silo_id: binding.silo_id,
            binding_id: binding.binding_id,
            remote_environment_id: binding.remote_environment_id,
            volume_id: binding.volume.volume_id,
            provider_receipt_id: Uuid::new_v4(),
            resource_deletions: vec![
                ResourceDeletionItem {
                    kind: DeletionResourceKind::ComputeInstance,
                    resource_id: Some(binding.remote_environment_id),
                    status: DeletionResourceStatus::Deleted,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::PersistentVolume,
                    resource_id: Some(binding.volume.volume_id),
                    status: DeletionResourceStatus::Deleted,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::Snapshot,
                    resource_id: None,
                    status: DeletionResourceStatus::NotApplicable,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::EphemeralKey,
                    resource_id: Some(binding.volume.key_id),
                    status: DeletionResourceStatus::Deleted,
                },
            ],
            deleted_at_unix_ms,
            reason: DeletionReason::TtlExpired,
        };
        let result = OperationResult {
            operation: RemoteOperation::Destroy,
            silo_id: binding.silo_id,
            binding_id: binding.binding_id,
            remote_environment_id: binding.remote_environment_id,
            server_id: binding.server_id,
            last_activity_at_unix_ms: deleted_at_unix_ms,
            state: RemoteResultState::Destroyed,
            volume: None,
            evidence: None,
            logs: None,
            next_cursor: None,
            deletion_proof: Some(proof.clone()),
        };
        remote.last_results.insert(silo.id, result.clone());

        let mut forged_server = remote.clone();
        forged_server
            .last_results
            .get_mut(&silo.id)
            .unwrap()
            .server_id = Uuid::new_v4();
        assert!(matches!(
            vault.persist_remote_control_plane(&root, forged_server),
            Err(VaultError::InvalidData)
        ));
        let mut duplicate_resource = remote.clone();
        let forged_proof = duplicate_resource
            .last_results
            .get_mut(&silo.id)
            .unwrap()
            .deletion_proof
            .as_mut()
            .unwrap();
        forged_proof.resource_deletions[2] = forged_proof.resource_deletions[0].clone();
        assert!(matches!(
            vault.persist_remote_control_plane(&root, duplicate_resource),
            Err(VaultError::InvalidData)
        ));

        vault
            .persist_remote_control_plane(&root, remote)
            .expect("persist recovered TTL receipt");
        vault.lock();
        vault.unlock(&root, passphrase).expect("reopen Vault");
        let reopened = vault.remote_control_plane().unwrap();
        assert!(reopened.backend.bindings.is_empty());
        assert_eq!(reopened.last_results.get(&silo.id), Some(&result));
        assert_eq!(
            reopened
                .last_results
                .get(&silo.id)
                .unwrap()
                .deletion_proof
                .as_ref()
                .unwrap()
                .reason,
            DeletionReason::TtlExpired
        );

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn orphan_receipt_survives_permanent_local_silo_deletion_without_deletion_proof() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test root");
        let passphrase = "an orphan receipt passphrase that is long enough";
        let mut vault = VaultRuntime::default();
        vault
            .initialize(&root, passphrase)
            .expect("initialize Vault");
        let silo = create_direct_silo(&root, &mut vault, "orphaned remote");
        let mut remote = minimal_remote_state(silo.id);
        let binding = remote.backend.bindings.remove(0);
        let receipt = RemoteOrphanReceipt {
            receipt_id: Uuid::new_v4(),
            silo_id: binding.silo_id,
            binding_id: binding.binding_id,
            remote_environment_id: binding.remote_environment_id,
            server_id: binding.server_id,
            endpoint: binding.endpoint,
            detached_at_unix_ms: u64::try_from(Utc::now().timestamp_millis()).unwrap(),
            notice: REMOTE_ORPHAN_NOTICE.to_owned(),
        };
        remote.orphan_receipts.push(receipt.clone());
        let mut duplicate_binding = receipt.clone();
        duplicate_binding.receipt_id = Uuid::new_v4();
        duplicate_binding.remote_environment_id = Uuid::new_v4();
        remote.orphan_receipts.push(duplicate_binding);
        assert!(matches!(
            vault.persist_remote_control_plane(&root, remote.clone()),
            Err(VaultError::InvalidData)
        ));
        remote.orphan_receipts.pop();

        let mut duplicate_remote_identity = receipt.clone();
        duplicate_remote_identity.receipt_id = Uuid::new_v4();
        duplicate_remote_identity.binding_id = Uuid::new_v4();
        remote.orphan_receipts.push(duplicate_remote_identity);
        assert!(matches!(
            vault.persist_remote_control_plane(&root, remote.clone()),
            Err(VaultError::InvalidData)
        ));
        remote.orphan_receipts.pop();

        vault
            .persist_remote_control_plane(&root, remote)
            .expect("persist orphan receipt");

        vault
            .delete_silo(&root, silo.id, false, true)
            .expect("permanently delete detached local Silo");
        assert!(matches!(
            vault.get_silo(silo.id),
            Err(VaultError::SiloNotFound)
        ));
        let after_delete = vault.remote_control_plane().expect("read audit trail");
        assert_eq!(after_delete.orphan_receipts, vec![receipt.clone()]);
        assert!(after_delete.backend.bindings.is_empty());
        let serialized = serde_json::to_value(&after_delete.orphan_receipts[0]).unwrap();
        assert!(serialized.get("deletionProof").is_none());

        vault.lock();
        vault.unlock(&root, passphrase).expect("reopen Vault");
        assert_eq!(
            vault
                .remote_control_plane()
                .expect("restore audit trail")
                .orphan_receipts,
            vec![receipt.clone()]
        );

        vault
            .change_passphrase(&root, passphrase, ROTATED_SCHEMA_FIXTURE_PASSPHRASE)
            .expect("rotate Vault containing orphan receipt");
        let backup_path = root.join("orphan-receipt-backup.json");
        vault
            .backup(&root, &backup_path)
            .expect("back up Vault containing orphan receipt");
        let backup_text =
            String::from_utf8_lossy(&fs::read(&backup_path).expect("read backup")).into_owned();
        assert!(!backup_text.contains(&receipt.notice));
        assert!(!backup_text.contains(&receipt.endpoint.origin));

        let restored_root = temporary_root();
        fs::create_dir_all(&restored_root).expect("create orphan receipt restore root");
        let mut restored = VaultRuntime::default();
        restored
            .restore(
                &restored_root,
                &backup_path,
                ROTATED_SCHEMA_FIXTURE_PASSPHRASE,
                false,
            )
            .expect("restore Vault containing orphan receipt");
        assert_eq!(
            restored
                .remote_control_plane()
                .expect("read restored audit trail")
                .orphan_receipts,
            vec![receipt.clone()]
        );
        restored.lock();
        restored
            .unlock(&restored_root, ROTATED_SCHEMA_FIXTURE_PASSPHRASE)
            .expect("reopen restored orphan receipt Vault");
        assert_eq!(
            restored
                .remote_control_plane()
                .expect("reopen restored audit trail")
                .orphan_receipts,
            vec![receipt]
        );

        fs::remove_dir_all(restored_root).expect("remove orphan receipt restore root");
        fs::remove_dir_all(root).expect("remove test root");
    }
}
