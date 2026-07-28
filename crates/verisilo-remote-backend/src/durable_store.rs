//! Atomic on-disk state for the self-hosted Agent.
//!
//! This store contains environment metadata, authorization records, replay
//! claims and deletion proofs. It deliberately does not contain pairing bearer
//! credentials or browser-profile bytes. Credentials are stored as hashes by
//! the HTTPS service; profile encryption belongs to the concrete provider.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use verisilo_remote_backend::{
    agent::{
        deletion_resources_are_bound, ActivityEntry, AgentError, AgentStore,
        AutomationAuthorization, DeletionProof, EnvironmentRecord, EnvironmentState,
        SessionAuthorization, MAX_ACTIVITY_ENTRIES,
    },
    MAX_REPLAY_WINDOW_ENTRIES,
};

const STORE_SCHEMA_VERSION: u32 = 1;
const MAX_STORE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ENVIRONMENTS: usize = 1_000;
const MAX_AUTHORIZATIONS: usize = 4_096;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedAgentState {
    schema_version: u32,
    environments: HashMap<Uuid, EnvironmentRecord>,
    human_sessions: HashMap<Uuid, SessionAuthorization>,
    automation: HashMap<Uuid, AutomationAuthorization>,
    proofs: HashMap<Uuid, DeletionProof>,
    request_ids: Vec<Uuid>,
    nonces: Vec<String>,
    sequences: HashMap<Uuid, u64>,
    activity: Vec<ActivityEntry>,
}

impl Default for PersistedAgentState {
    fn default() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            environments: HashMap::new(),
            human_sessions: HashMap::new(),
            automation: HashMap::new(),
            proofs: HashMap::new(),
            request_ids: Vec::new(),
            nonces: Vec::new(),
            sequences: HashMap::new(),
            activity: Vec::new(),
        }
    }
}

pub struct DurableAgentStore {
    path: PathBuf,
    state: PersistedAgentState,
    poisoned: bool,
}

impl DurableAgentStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AgentError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(store_error("Agent state path must be absolute."));
        }
        recover_interrupted_write(&path)?;
        let state = if path.exists() {
            let metadata = fs::metadata(&path).map_err(io_error)?;
            if !metadata.is_file() || metadata.len() > MAX_STORE_BYTES {
                return Err(store_error("Agent state is not a bounded regular file."));
            }
            let raw = fs::read(&path).map_err(io_error)?;
            serde_json::from_slice(&raw)
                .map_err(|error| store_error(format!("Agent state JSON is invalid: {error}")))?
        } else {
            PersistedAgentState::default()
        };
        validate_state(&state)?;
        let store = Self {
            path,
            state,
            poisoned: false,
        };
        if !store.path.exists() {
            store.persist(&store.state)?;
        }
        Ok(store)
    }

    fn mutate(
        &mut self,
        change: impl FnOnce(&mut PersistedAgentState) -> Result<(), AgentError>,
    ) -> Result<(), AgentError> {
        if self.poisoned {
            return Err(store_error(
                "Agent state durability is uncertain; restart is required.",
            ));
        }
        let mut prospective = self.state.clone();
        change(&mut prospective)?;
        validate_state(&prospective)?;
        if let Err(error) = self.persist(&prospective) {
            // The destination rename may have reached disk even if a later
            // directory fsync failed. Refuse further mutations until reopen so
            // replay counters or environment state cannot be reused from a
            // stale in-memory snapshot.
            self.poisoned = true;
            return Err(error);
        }
        self.state = prospective;
        Ok(())
    }

    fn persist(&self, state: &PersistedAgentState) -> Result<(), AgentError> {
        let raw = serde_json::to_vec(state)
            .map_err(|error| store_error(format!("Could not serialize Agent state: {error}")))?;
        if raw.len() as u64 > MAX_STORE_BYTES {
            return Err(store_error("Agent state exceeds 8 MiB."));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| store_error("Agent state path has no parent."))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let temporary = self.path.with_extension(format!("tmp-{}", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(io_error)?;
        if let Err(error) = file.write_all(&raw).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&temporary);
            return Err(io_error(error));
        }
        drop(file);
        if let Err(error) = replace_file(&temporary, &self.path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }
}

impl AgentStore for DurableAgentStore {
    fn claim_request(
        &mut self,
        principal_id: Uuid,
        request_id: Uuid,
        nonce: &str,
        sequence: u64,
    ) -> Result<(), AgentError> {
        self.mutate(|state| {
            let last = state.sequences.get(&principal_id).copied().unwrap_or(0);
            if sequence == 0
                || sequence <= last
                || state.request_ids.contains(&request_id)
                || state.nonces.iter().any(|seen| seen == nonce)
            {
                return Err(AgentError::Replay);
            }
            if state.request_ids.len() == MAX_REPLAY_WINDOW_ENTRIES {
                // The per-principal sequence high-water mark remains the
                // authoritative replay barrier. Keep only the most recent
                // request-id/nonce diagnostics so normal traffic cannot
                // permanently exhaust the Agent after 4,096 operations.
                state.request_ids.remove(0);
                state.nonces.remove(0);
            }
            state.request_ids.push(request_id);
            state.nonces.push(nonce.to_owned());
            state.sequences.insert(principal_id, sequence);
            Ok(())
        })
    }

    fn environment_ids(&self) -> Vec<Uuid> {
        self.state.environments.keys().copied().collect()
    }

    fn environment(&self, silo_id: Uuid) -> Option<EnvironmentRecord> {
        self.state.environments.get(&silo_id).cloned()
    }

    fn insert_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError> {
        self.mutate(|state| {
            if state.environments.contains_key(&record.silo_id) {
                return Err(AgentError::Conflict(
                    "A remote environment already exists for this Silo.".to_owned(),
                ));
            }
            state.environments.insert(record.silo_id, record);
            Ok(())
        })
    }

    fn update_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError> {
        self.mutate(|state| {
            if !state.environments.contains_key(&record.silo_id) {
                return Err(AgentError::NotFound);
            }
            state.environments.insert(record.silo_id, record);
            Ok(())
        })
    }

    fn human_session(&self, silo_id: Uuid) -> Option<SessionAuthorization> {
        self.state.human_sessions.get(&silo_id).cloned()
    }

    fn set_human_session(&mut self, authorization: SessionAuthorization) -> Result<(), AgentError> {
        self.mutate(|state| {
            state
                .human_sessions
                .insert(authorization.silo_id, authorization);
            Ok(())
        })
    }

    fn automation(&self, authorization_id: Uuid) -> Option<AutomationAuthorization> {
        self.state.automation.get(&authorization_id).cloned()
    }

    fn set_automation(&mut self, authorization: AutomationAuthorization) -> Result<(), AgentError> {
        self.mutate(|state| {
            state
                .automation
                .insert(authorization.authorization_id, authorization);
            Ok(())
        })
    }

    fn commit_deletion(
        &mut self,
        record: EnvironmentRecord,
        proof: DeletionProof,
    ) -> Result<(), AgentError> {
        self.mutate(|state| {
            if !state.environments.contains_key(&record.silo_id)
                || record.state != EnvironmentState::Deleted
                || record.deletion_proof_id != Some(proof.proof_id)
                || record.silo_id != proof.silo_id
                || record.binding_id != proof.binding_id
                || record.remote_environment_id != proof.remote_environment_id
                || record.volume.volume_id != proof.volume_id
            {
                return Err(AgentError::InvalidState(
                    "Deletion record and proof do not match.".to_owned(),
                ));
            }
            state.environments.insert(record.silo_id, record);
            state.proofs.insert(proof.proof_id, proof);
            Ok(())
        })
    }

    fn deletion_proof(&self, proof_id: Uuid) -> Option<DeletionProof> {
        self.state.proofs.get(&proof_id).cloned()
    }

    fn append_activity(&mut self, activity: ActivityEntry) -> Result<(), AgentError> {
        self.mutate(|state| {
            if state.activity.len() == MAX_ACTIVITY_ENTRIES {
                state.activity.remove(0);
            }
            state.activity.push(activity);
            Ok(())
        })
    }

    fn activities(&self, silo_id: Uuid) -> Vec<ActivityEntry> {
        self.state
            .activity
            .iter()
            .filter(|entry| entry.silo_id == silo_id)
            .cloned()
            .collect()
    }
}

fn validate_state(state: &PersistedAgentState) -> Result<(), AgentError> {
    if state.schema_version != STORE_SCHEMA_VERSION
        || state.environments.len() > MAX_ENVIRONMENTS
        || state.human_sessions.len() > MAX_ENVIRONMENTS
        || state.automation.len() > MAX_AUTHORIZATIONS
        || state.proofs.len() > MAX_ENVIRONMENTS
        || state.request_ids.len() > MAX_REPLAY_WINDOW_ENTRIES
        || state.nonces.len() > MAX_REPLAY_WINDOW_ENTRIES
        || state.request_ids.len() != state.nonces.len()
        || state.sequences.len() > MAX_AUTHORIZATIONS
        || state.activity.len() > MAX_ACTIVITY_ENTRIES
    {
        return Err(store_error(
            "Agent state exceeds its schema or item limits.",
        ));
    }
    if state.nonces.iter().any(|nonce| {
        !(32..=128).contains(&nonce.len())
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    }) || state.request_ids.iter().collect::<HashSet<_>>().len() != state.request_ids.len()
        || state.nonces.iter().collect::<HashSet<_>>().len() != state.nonces.len()
        || state.sequences.values().any(|sequence| *sequence == 0)
    {
        return Err(store_error("Agent replay state is malformed."));
    }
    for (silo_id, record) in &state.environments {
        if silo_id != &record.silo_id
            || record.binding_id == Uuid::nil()
            || record.remote_environment_id == Uuid::nil()
            || record.expires_at_unix_ms <= record.created_at_unix_ms
            || record.last_activity_at_unix_ms < record.created_at_unix_ms
        {
            return Err(store_error("Agent environment record is inconsistent."));
        }
        match (record.state, record.deletion_proof_id) {
            (EnvironmentState::Deleted, Some(proof_id)) => {
                let proof = state.proofs.get(&proof_id).ok_or_else(|| {
                    store_error("Deleted environment is missing its deletion proof.")
                })?;
                if proof.proof_id == Uuid::nil()
                    || proof.provider_receipt_id == Uuid::nil()
                    || proof.deleted_at_unix_ms == 0
                    || proof.silo_id != record.silo_id
                    || proof.binding_id != record.binding_id
                    || proof.remote_environment_id != record.remote_environment_id
                    || proof.volume_id != record.volume.volume_id
                    || proof.deleted_at_unix_ms != record.last_activity_at_unix_ms
                    || !deletion_resources_are_bound(
                        &proof.resource_deletions,
                        record.remote_environment_id,
                        record.volume.volume_id,
                        record.volume.key_id,
                    )
                {
                    return Err(store_error(
                        "Deletion proof does not match its environment.",
                    ));
                }
            }
            (EnvironmentState::Deleted, None) | (_, Some(_)) => {
                return Err(store_error("Environment deletion state is ambiguous."));
            }
            _ => {}
        }
    }
    if state
        .human_sessions
        .iter()
        .any(|(silo_id, authorization)| silo_id != &authorization.silo_id)
        || state
            .automation
            .iter()
            .any(|(authorization_id, authorization)| {
                authorization_id != &authorization.authorization_id
            })
        || state
            .proofs
            .iter()
            .any(|(proof_id, proof)| proof_id != &proof.proof_id)
    {
        return Err(store_error(
            "Agent authorization or proof index is inconsistent.",
        ));
    }
    Ok(())
}

fn replace_file(temporary: &Path, destination: &Path) -> Result<(), AgentError> {
    let backup = destination.with_extension("bak");
    if backup.exists() {
        fs::remove_file(&backup).map_err(io_error)?;
        sync_parent_directory(destination)?;
    }
    if destination.exists() {
        fs::rename(destination, &backup).map_err(io_error)?;
        sync_parent_directory(destination)?;
    }
    if let Err(error) = fs::rename(temporary, destination) {
        if backup.exists() && fs::rename(&backup, destination).is_ok() {
            let _ = sync_parent_directory(destination);
        }
        return Err(io_error(error));
    }
    // This is the commit point: both the new file contents and the directory
    // entry naming them have reached stable storage before in-memory state is
    // advanced.
    sync_parent_directory(destination)?;
    if backup.exists() {
        // The new destination is already durable. Backup cleanup is recoverable
        // and must not turn a committed state transition into an ambiguous
        // error for the caller.
        if fs::remove_file(backup).is_ok() {
            let _ = sync_parent_directory(destination);
        }
    }
    Ok(())
}

fn recover_interrupted_write(destination: &Path) -> Result<(), AgentError> {
    let backup = destination.with_extension("bak");
    if !destination.exists() && backup.exists() {
        fs::rename(backup, destination).map_err(io_error)?;
        sync_parent_directory(destination)?;
    } else if destination.exists() && backup.exists() {
        fs::remove_file(backup).map_err(io_error)?;
        sync_parent_directory(destination)?;
    }
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), AgentError> {
    let parent = path
        .parent()
        .ok_or_else(|| store_error("Agent state path has no parent."))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

fn io_error(error: std::io::Error) -> AgentError {
    store_error(format!("Filesystem error: {error}"))
}

fn store_error(message: impl Into<String>) -> AgentError {
    AgentError::Store(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use verisilo_remote_backend::{
        agent::{
            DeletionReason, DeletionResourceKind, DeletionResourceStatus, KeyCustody,
            ResourceDeletionItem, VolumeAttestation,
        },
        RemoteNetworkPolicy,
    };

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("verisilo-agent-store-{label}-{}", Uuid::new_v4()))
    }

    fn record(silo_id: Uuid) -> EnvironmentRecord {
        EnvironmentRecord {
            silo_id,
            binding_id: Uuid::new_v4(),
            remote_environment_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            state: EnvironmentState::Created,
            network: RemoteNetworkPolicy::Direct,
            volume: VolumeAttestation {
                encrypted: true,
                key_custody: KeyCustody::UserControlled,
                volume_id: Uuid::new_v4(),
                key_id: Uuid::new_v4(),
            },
            created_at_unix_ms: 1_000,
            expires_at_unix_ms: 10_000,
            last_activity_at_unix_ms: 1_000,
            deletion_proof_id: None,
        }
    }

    #[test]
    fn request_claim_and_environment_survive_reopen() {
        let root = root("reopen");
        let path = root.join("state.json");
        let silo_id = Uuid::new_v4();
        let principal_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let nonce = request_id.simple().to_string();
        {
            let mut store = DurableAgentStore::open(path.clone()).unwrap();
            store
                .claim_request(principal_id, request_id, &nonce, 1)
                .unwrap();
            store.insert_environment(record(silo_id)).unwrap();
        }
        let mut reopened = DurableAgentStore::open(path).unwrap();
        assert!(reopened.environment(silo_id).is_some());
        assert!(matches!(
            reopened.claim_request(principal_id, request_id, &nonce, 1),
            Err(AgentError::Replay)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deletion_record_and_proof_commit_together() {
        let root = root("deletion");
        let path = root.join("state.json");
        let mut store = DurableAgentStore::open(path.clone()).unwrap();
        let silo_id = Uuid::new_v4();
        let mut record = record(silo_id);
        store.insert_environment(record.clone()).unwrap();
        let proof = DeletionProof {
            proof_id: Uuid::new_v4(),
            silo_id,
            binding_id: record.binding_id,
            remote_environment_id: record.remote_environment_id,
            volume_id: record.volume.volume_id,
            provider_receipt_id: Uuid::new_v4(),
            resource_deletions: vec![
                ResourceDeletionItem {
                    kind: DeletionResourceKind::ComputeInstance,
                    resource_id: Some(record.remote_environment_id),
                    status: DeletionResourceStatus::Deleted,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::PersistentVolume,
                    resource_id: Some(record.volume.volume_id),
                    status: DeletionResourceStatus::Deleted,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::Snapshot,
                    resource_id: None,
                    status: DeletionResourceStatus::NotApplicable,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::EphemeralKey,
                    resource_id: Some(record.volume.key_id),
                    status: DeletionResourceStatus::Deleted,
                },
            ],
            deleted_at_unix_ms: 2_000,
            reason: DeletionReason::UserConfirmed,
        };
        record.state = EnvironmentState::Deleted;
        record.deletion_proof_id = Some(proof.proof_id);
        record.last_activity_at_unix_ms = 2_000;
        store.commit_deletion(record, proof.clone()).unwrap();
        drop(store);
        let reopened = DurableAgentStore::open(path).unwrap();
        assert_eq!(reopened.deletion_proof(proof.proof_id), Some(proof));
        assert_eq!(
            reopened.environment(silo_id).unwrap().state,
            EnvironmentState::Deleted
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_or_inconsistent_state_is_rejected() {
        let root = root("invalid");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.json");
        fs::write(
            &path,
            br#"{"schemaVersion":1,"environments":{},"humanSessions":{},"automation":{},"proofs":{},"requestIds":[],"nonces":[],"sequences":{},"activity":[],"shell":"bad"}"#,
        )
        .unwrap();
        assert!(matches!(
            DurableAgentStore::open(path),
            Err(AgentError::Store(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_backup_is_recovered_before_state_is_read() {
        let root = root("recovery");
        let path = root.join("state.json");
        let silo_id = Uuid::new_v4();
        {
            let mut store = DurableAgentStore::open(path.clone()).unwrap();
            store.insert_environment(record(silo_id)).unwrap();
        }
        let backup = path.with_extension("bak");
        fs::rename(&path, &backup).unwrap();
        let reopened = DurableAgentStore::open(path.clone()).unwrap();
        assert!(reopened.environment(silo_id).is_some());
        assert!(path.exists());
        assert!(!backup.exists());
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replay_diagnostics_roll_without_resetting_sequence_high_water_mark() {
        let root = root("rolling-replay");
        let path = root.join("state.json");
        let mut store = DurableAgentStore::open(path).unwrap();
        let principal_id = Uuid::new_v4();
        for index in 0..MAX_REPLAY_WINDOW_ENTRIES {
            store
                .state
                .request_ids
                .push(Uuid::from_u128(index as u128 + 1));
            store.state.nonces.push(format!("{index:032x}"));
        }
        store
            .state
            .sequences
            .insert(principal_id, MAX_REPLAY_WINDOW_ENTRIES as u64);
        store.persist(&store.state).unwrap();

        let next_request_id = Uuid::new_v4();
        let next_nonce = next_request_id.simple().to_string();
        store
            .claim_request(
                principal_id,
                next_request_id,
                &next_nonce,
                MAX_REPLAY_WINDOW_ENTRIES as u64 + 1,
            )
            .unwrap();
        assert_eq!(store.state.request_ids.len(), MAX_REPLAY_WINDOW_ENTRIES);
        assert!(!store.state.request_ids.contains(&Uuid::from_u128(1)));
        assert_eq!(
            store.state.sequences.get(&principal_id),
            Some(&(MAX_REPLAY_WINDOW_ENTRIES as u64 + 1))
        );
        let _ = fs::remove_dir_all(root);
    }
}
