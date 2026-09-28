//! Public contract for same-user, same-Vault Managed Silo cold backups.
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedSiloBackupInput {
    pub destination_path: String,
    pub passphrase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedSiloBackupSourceInput {
    pub source_path: String,
    pub passphrase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedSiloRestoreInput {
    pub source_path: String,
    pub passphrase: String,
    /// Digest returned by inspection, binding approval to the reviewed archive.
    pub expected_archive_sha256: String,
    pub confirm_overwrite: bool,
}

impl Drop for ManagedSiloBackupInput {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}
impl Drop for ManagedSiloBackupSourceInput {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}
impl Drop for ManagedSiloRestoreInput {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSiloBackupReceipt {
    pub silo_id: Uuid,
    pub destination_path: String,
    pub bytes: u64,
    pub profile_bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSiloBackupInspection {
    pub silo_id: Uuid,
    pub silo_name: String,
    pub created_at: String,
    pub artifact_id: String,
    pub artifact_sha256: String,
    pub engine_version: String,
    pub profile_bytes: u64,
    pub file_count: u64,
    pub archive_sha256: String,
    /// Human-readable policy summary; credentials never leave the backend.
    pub network_summary: String,
}

#[cfg(test)]
mod tests {
    use super::ManagedSiloRestoreInput;

    #[test]
    fn restore_contract_requires_reviewed_archive_and_explicit_confirmation() {
        let mut input = serde_json::json!({
            "sourcePath": "backup.vsilo",
            "passphrase": "test-backup-passphrase"
        });
        assert!(serde_json::from_value::<ManagedSiloRestoreInput>(input.clone()).is_err());
        input["expectedArchiveSha256"] = serde_json::json!("a".repeat(64));
        assert!(serde_json::from_value::<ManagedSiloRestoreInput>(input.clone()).is_err());
        input["confirmOverwrite"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ManagedSiloRestoreInput>(input.clone()).is_ok());
        input["newSiloId"] = serde_json::json!(uuid::Uuid::new_v4());
        assert!(serde_json::from_value::<ManagedSiloRestoreInput>(input).is_err());
    }
}
