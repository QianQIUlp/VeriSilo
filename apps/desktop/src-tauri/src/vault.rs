use std::{
    collections::HashMap,
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
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{CreateSiloInput, Silo, VaultLockState, VaultStatus, SCHEMA_VERSION};

const AUTO_LOCK_MINUTES: i64 = 15;
const VAULT_FILE_NAME: &str = "vault.json";
const VAULT_ENVELOPE_VERSION: u32 = 2;
const VAULT_DATA_SCHEMA_VERSION: u32 = 3;
const KDF_MEMORY_KIB: u32 = 19_456;
const KDF_ITERATIONS: u32 = 2;
const KDF_PARALLELISM: u32 = 1;

#[derive(Default)]
pub struct VaultRuntime {
    unlocked: Option<UnlockedVault>,
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
#[serde(rename_all = "camelCase")]
struct VaultData {
    schema_version: u32,
    silos: Vec<Silo>,
    seed_material: HashMap<Uuid, String>,
    #[serde(default)]
    proxy_credentials: HashMap<Uuid, StoredProxyCredential>,
    #[serde(default)]
    mihomo_controller_secrets: HashMap<Uuid, StoredMihomoControllerSecret>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
    #[error("Silo input is invalid: {0}")]
    InvalidSilo(String),
}

impl VaultRuntime {
    pub fn status(&mut self, root: &Path) -> VaultStatus {
        self.expire_if_needed();
        match &self.unlocked {
            Some(unlocked) => VaultStatus {
                state: VaultLockState::Unlocked,
                auto_lock_at: Some(unlocked.auto_lock_at),
            },
            None if vault_path(root).is_file() => VaultStatus {
                state: VaultLockState::Locked,
                auto_lock_at: None,
            },
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
        };
        let unlocked = UnlockedVault {
            dek: Zeroizing::new(random_bytes()),
            kek,
            salt,
            data,
            auto_lock_at: auto_lock_time(),
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
        let envelope: VaultEnvelope =
            serde_json::from_slice(&raw).map_err(|_| VaultError::InvalidData)?;
        if !matches!(envelope.version, 1 | VAULT_ENVELOPE_VERSION) || envelope.kdf != "argon2id" {
            return Err(VaultError::InvalidData);
        }

        let salt = decode_fixed::<16>(&envelope.salt).ok_or(VaultError::InvalidData)?;
        let kek = derive_key(passphrase, &salt)?;
        let ciphertext = STANDARD_NO_PAD
            .decode(envelope.ciphertext.as_bytes())
            .map_err(|_| VaultError::InvalidData)?;
        let nonce = decode_fixed::<12>(&envelope.nonce).ok_or(VaultError::InvalidData)?;

        let (dek, migrated_from_legacy) = if envelope.version == VAULT_ENVELOPE_VERSION {
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
            let wrapping_cipher = Aes256Gcm::new_from_slice(kek.as_ref())
                .map_err(|_| VaultError::CryptographicSetup)?;
            let raw_dek = wrapping_cipher
                .decrypt(Nonce::from_slice(&wrap_nonce), wrapped_dek.as_ref())
                .map_err(|_| VaultError::InvalidPassphrase)?;
            let raw_dek: [u8; 32] = raw_dek.try_into().map_err(|_| VaultError::InvalidData)?;
            (Zeroizing::new(raw_dek), false)
        } else {
            // Version 1 used the password-derived key directly for data encryption.
            // It is accepted solely to migrate an existing local vault on unlock.
            (Zeroizing::new(random_bytes()), true)
        };

        let data_key = if migrated_from_legacy {
            kek.as_ref()
        } else {
            dek.as_ref()
        };
        let cipher =
            Aes256Gcm::new_from_slice(data_key).map_err(|_| VaultError::CryptographicSetup)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
                .map_err(|_| VaultError::InvalidPassphrase)?,
        );
        let mut data: VaultData =
            serde_json::from_slice(plaintext.as_ref()).map_err(|_| VaultError::InvalidData)?;
        let migrated_data_schema = match data.schema_version {
            1 | 2 => {
                data.schema_version = VAULT_DATA_SCHEMA_VERSION;
                true
            }
            VAULT_DATA_SCHEMA_VERSION => false,
            _ => return Err(VaultError::InvalidData),
        };

        self.unlocked = Some(UnlockedVault {
            dek,
            kek,
            salt,
            data,
            auto_lock_at: auto_lock_time(),
        });
        if migrated_from_legacy || migrated_data_schema {
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

    pub fn list_silos(&mut self) -> Result<Vec<Silo>, VaultError> {
        let unlocked = self.unlocked_mut()?;
        Ok(unlocked.data.silos.clone())
    }

    pub fn get_silo(&mut self, silo_id: Uuid) -> Result<Silo, VaultError> {
        let unlocked = self.unlocked_mut()?;
        unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id && silo.archived_at.is_none())
            .cloned()
            .ok_or(VaultError::SiloNotFound)
    }

    pub fn managed_profile_directories(&mut self) -> Result<Vec<PathBuf>, VaultError> {
        let unlocked = self.unlocked_mut()?;
        Ok(unlocked
            .data
            .silos
            .iter()
            .map(|silo| PathBuf::from(&silo.profile_directory))
            .collect())
    }

    pub fn proxy_authentication_for_silo(
        &mut self,
        silo_id: Uuid,
    ) -> Result<Option<ProxyAuthentication>, VaultError> {
        let unlocked = self.unlocked_mut()?;
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
        let unlocked = self.unlocked_mut()?;
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
        let unlocked = self.unlocked_mut()?;
        unlocked
            .data
            .silos
            .iter()
            .find(|silo| silo.id == silo_id)
            .map(|silo| PathBuf::from(&silo.profile_directory))
            .ok_or(VaultError::SiloNotFound)
    }

    pub fn create_silo(&mut self, root: &Path, input: CreateSiloInput) -> Result<Silo, VaultError> {
        input
            .validate()
            .map_err(|error| VaultError::InvalidSilo(error.to_string()))?;
        // Refuse sensitive state changes before creating even an empty profile directory.
        self.unlocked_mut()?;
        let CreateSiloInput {
            name,
            color,
            browser_kind,
            executable_path,
            mut network_profile,
            proxy_credentials,
            mihomo_controller_secret,
        } = input;
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
                executable_path,
                version: None,
            },
            profile_directory: profile_directory.to_string_lossy().to_string(),
            network_profile,
            seed_reference,
            created_at: Utc::now(),
            archived_at: None,
        };

        let seed = STANDARD_NO_PAD.encode(random_bytes::<32>());
        let prospective_data = {
            let unlocked = self.unlocked_mut()?;
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
        self.unlocked_mut()?.data = prospective_data;
        Ok(silo)
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
            let unlocked = self.unlocked_mut()?;
            let mut data = unlocked.data.clone();
            let silo = data
                .silos
                .iter_mut()
                .find(|silo| silo.id == silo_id)
                .ok_or(VaultError::SiloNotFound)?;
            silo.archived_at = Some(Utc::now());
            data
        };
        self.persist_data(root, &prospective_data)?;
        self.unlocked_mut()?.data = prospective_data;
        Ok(())
    }

    fn unlocked_mut(&mut self) -> Result<&mut UnlockedVault, VaultError> {
        self.expire_if_needed();
        let unlocked = self.unlocked.as_mut().ok_or(VaultError::Locked)?;
        unlocked.auto_lock_at = auto_lock_time();
        Ok(unlocked)
    }

    fn expire_if_needed(&mut self) {
        if self
            .unlocked
            .as_ref()
            .is_some_and(|unlocked| Utc::now() >= unlocked.auto_lock_at)
        {
            self.unlocked = None;
        }
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

fn auto_lock_time() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::minutes(AUTO_LOCK_MINUTES)
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
    use uuid::Uuid;

    use super::{derive_key, VaultData, VaultEnvelope, VaultRuntime};
    use crate::domain::{
        BrowserKind, CreateSiloInput, NetworkProfile, VaultLockState, SCHEMA_VERSION,
    };

    fn temporary_root() -> std::path::PathBuf {
        env::temp_dir().join(format!("verisilo-vault-test-{}", Uuid::new_v4()))
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
    fn legacy_vaults_are_rewrapped_with_a_random_dek_on_unlock() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let passphrase = "a passphrase that is long enough";
        let salt = [7_u8; 16];
        let legacy_key = derive_key(passphrase, &salt).expect("derive legacy key");
        let nonce = [9_u8; 12];
        let plaintext = serde_json::to_vec(&VaultData {
            schema_version: SCHEMA_VERSION,
            silos: Vec::new(),
            seed_material: Default::default(),
            proxy_credentials: Default::default(),
            mihomo_controller_secrets: Default::default(),
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
    fn locked_vault_does_not_create_a_profile_directory() {
        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let browser = root.join("chrome.exe");
        fs::write(&browser, []).expect("create test browser file");
        let mut vault = VaultRuntime::default();
        let input = CreateSiloInput {
            name: "locked".to_owned(),
            color: "#4f46e5".to_owned(),
            browser_kind: BrowserKind::Chrome,
            executable_path: browser.to_string_lossy().to_string(),
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
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
        let browser = root.join("chrome.exe");
        fs::write(&browser, []).expect("create test browser file");
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

        vault.lock();
        assert!(vault.proxy_authentication_for_silo(silo.id).is_err());
        fs::remove_dir_all(root).expect("remove test vault directory");
    }

    #[test]
    fn mihomo_controller_secrets_are_encrypted_and_reference_only() {
        use crate::domain::{ExternalMihomoBinding, MihomoControllerSecretInput, ProxyScheme};

        let root = temporary_root();
        fs::create_dir_all(&root).expect("create test vault directory");
        let browser = root.join("chrome.exe");
        fs::write(&browser, []).expect("create test browser file");
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
}
