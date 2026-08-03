//! Transport-independent core for a user-operated VeriSilo remote Agent.
//!
//! A concrete daemon must authenticate its TLS transport before calling this
//! core. The core has no shell, executable, argument-list, filesystem-path, VM
//! image, or arbitrary URL input.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    CapabilityAvailability, Clock, GuestEvidence, RemoteCapability, RemoteNetworkPolicy,
    RemoteOperation, MAX_CLOCK_SKEW_MS, MAX_MESSAGE_BYTES, MAX_REPLAY_WINDOW_ENTRIES,
    PROTOCOL_VERSION,
};

pub const MAX_ENVIRONMENT_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const MAX_HUMAN_SESSION_SECONDS: u64 = 8 * 60 * 60;
pub const MAX_AUTOMATION_SECONDS: u64 = 60 * 60;
pub const MAX_INPUT_EVENTS: usize = 128;
pub const MAX_ACTIVITY_ENTRIES: usize = 2_000;
const MAX_SCREEN_CHANNEL_LIFETIME_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeOwnership {
    UserSelfHosted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyCustody {
    UserControlled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CostDisclosure {
    pub currency: String,
    pub estimated_micros_per_hour: u64,
    pub notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeDisclosure {
    pub node_id: Uuid,
    pub ownership: NodeOwnership,
    pub operator_label: String,
    pub data_region: String,
    pub key_custody: KeyCustody,
    pub cost: CostDisclosure,
}

impl NodeDisclosure {
    pub fn validate(&self) -> Result<(), AgentError> {
        if self.node_id == Uuid::nil() {
            return Err(AgentError::InvalidRequest(
                "Node identity must be a non-zero UUID.".to_owned(),
            ));
        }
        bounded_text("operator label", &self.operator_label, 1, 120)?;
        bounded_text("data region", &self.data_region, 2, 120)?;
        bounded_text("cost notice", &self.cost.notice, 1, 500)?;
        if self.cost.currency.len() != 3
            || !self
                .cost
                .currency
                .bytes()
                .all(|byte| byte.is_ascii_uppercase())
        {
            return Err(AgentError::InvalidRequest(
                "Cost currency must be a three-letter uppercase code.".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentState {
    /// Durable create intent written before the provider is called. This state
    /// is never returned as a successful create response; startup recovery
    /// destroys it before serving requests.
    Provisioning,
    Created,
    Running,
    Stopped,
    Paused,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VolumeAttestation {
    pub encrypted: bool,
    pub key_custody: KeyCustody,
    pub volume_id: Uuid,
    pub key_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentRecord {
    pub silo_id: Uuid,
    pub binding_id: Uuid,
    pub remote_environment_id: Uuid,
    pub node_id: Uuid,
    pub state: EnvironmentState,
    pub network: RemoteNetworkPolicy,
    pub volume: VolumeAttestation,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub last_activity_at_unix_ms: u64,
    pub deletion_proof_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeletionReason {
    UserConfirmed,
    TtlExpired,
    ProviderPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DeletionResourceKind {
    ComputeInstance,
    PersistentVolume,
    Snapshot,
    EphemeralKey,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeletionResourceStatus {
    Deleted,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceDeletionItem {
    pub kind: DeletionResourceKind,
    pub resource_id: Option<Uuid>,
    pub status: DeletionResourceStatus,
}

pub const REQUIRED_DELETION_RESOURCE_KINDS: [DeletionResourceKind; 4] = [
    DeletionResourceKind::ComputeInstance,
    DeletionResourceKind::PersistentVolume,
    DeletionResourceKind::Snapshot,
    DeletionResourceKind::EphemeralKey,
];

/// A deletion receipt is complete only when it has one typed disposition for
/// every resource class. Stable environment, volume and key identifiers are
/// checked exactly; the optional snapshot entry must say `not_applicable`
/// rather than disappearing from the proof.
pub fn deletion_resources_are_bound(
    resources: &[ResourceDeletionItem],
    remote_environment_id: Uuid,
    volume_id: Uuid,
    key_id: Uuid,
) -> bool {
    if remote_environment_id == Uuid::nil()
        || volume_id == Uuid::nil()
        || key_id == Uuid::nil()
        || resources.len() != REQUIRED_DELETION_RESOURCE_KINDS.len()
        || resources
            .iter()
            .map(|resource| resource.kind)
            .collect::<HashSet<_>>()
            .len()
            != resources.len()
    {
        return false;
    }

    REQUIRED_DELETION_RESOURCE_KINDS.iter().all(|kind| {
        let Some(resource) = resources.iter().find(|resource| resource.kind == *kind) else {
            return false;
        };
        match (resource.kind, resource.status, resource.resource_id) {
            (
                DeletionResourceKind::ComputeInstance,
                DeletionResourceStatus::Deleted,
                Some(resource_id),
            ) => resource_id == remote_environment_id,
            (
                DeletionResourceKind::PersistentVolume,
                DeletionResourceStatus::Deleted,
                Some(resource_id),
            ) => resource_id == volume_id,
            (
                DeletionResourceKind::EphemeralKey,
                DeletionResourceStatus::Deleted,
                Some(resource_id),
            ) => resource_id == key_id,
            (
                DeletionResourceKind::Snapshot,
                DeletionResourceStatus::Deleted,
                Some(resource_id),
            ) => resource_id != Uuid::nil(),
            (DeletionResourceKind::Snapshot, DeletionResourceStatus::NotApplicable, None) => true,
            _ => false,
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDeletionReceipt {
    pub receipt_id: Uuid,
    pub remote_environment_id: Uuid,
    pub volume_id: Uuid,
    pub resource_deletions: Vec<ResourceDeletionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeletionProof {
    pub proof_id: Uuid,
    pub silo_id: Uuid,
    pub binding_id: Uuid,
    pub remote_environment_id: Uuid,
    pub volume_id: Uuid,
    pub provider_receipt_id: Uuid,
    pub resource_deletions: Vec<ResourceDeletionItem>,
    pub deleted_at_unix_ms: u64,
    pub reason: DeletionReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AutomationScope {
    ReadScreen,
    SendInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionAuthorization {
    pub authorization_id: Uuid,
    pub silo_id: Uuid,
    pub remote_environment_id: Uuid,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationAuthorization {
    pub authorization_id: Uuid,
    pub silo_id: Uuid,
    pub remote_environment_id: Uuid,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub scopes: Vec<AutomationScope>,
    pub approved_by_user: bool,
    pub revoked: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    ControlPlane,
    HumanSession,
    Automation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Principal {
    pub kind: PrincipalKind,
    pub credential_id: Uuid,
    pub authorization_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InputEvent {
    Key {
        code: String,
        pressed: bool,
    },
    PointerMove {
        x: u32,
        y: u32,
    },
    PointerButton {
        button: PointerButton,
        pressed: bool,
    },
    Text {
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AgentCommand {
    Create {
        silo_id: Uuid,
        binding_id: Uuid,
        remote_environment_id: Uuid,
        ttl_seconds: u64,
        network: RemoteNetworkPolicy,
        cost_acknowledged: bool,
    },
    Start {
        silo_id: Uuid,
    },
    Stop {
        silo_id: Uuid,
    },
    Pause {
        silo_id: Uuid,
    },
    Snapshot {
        silo_id: Uuid,
    },
    Destroy {
        silo_id: Uuid,
        confirm_destroy: bool,
    },
    ConfigureNetwork {
        silo_id: Uuid,
        network: RemoteNetworkPolicy,
    },
    Health {
        silo_id: Uuid,
    },
    Logs {
        silo_id: Uuid,
        limit: u16,
    },
    OpenHumanSession {
        silo_id: Uuid,
        lifetime_seconds: u64,
    },
    CloseHumanSession {
        silo_id: Uuid,
    },
    GrantAutomation {
        silo_id: Uuid,
        lifetime_seconds: u64,
        scopes: Vec<AutomationScope>,
        approved_by_user: bool,
    },
    RevokeAutomation {
        silo_id: Uuid,
        authorization_id: Uuid,
    },
    OpenScreen {
        silo_id: Uuid,
    },
    SendInput {
        silo_id: Uuid,
        events: Vec<InputEvent>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRequest {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub nonce: String,
    pub sequence: u64,
    pub sent_at_unix_ms: u64,
    pub principal: Principal,
    pub command: AgentCommand,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScreenTransport {
    AuthenticatedEncryptedStream,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenChannel {
    pub channel_id: Uuid,
    pub remote_environment_id: Uuid,
    pub authorization_id: Uuid,
    pub expires_at_unix_ms: u64,
    pub transport: ScreenTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityEntry {
    pub activity_id: Uuid,
    pub silo_id: Uuid,
    pub principal: PrincipalKind,
    pub operation: String,
    pub accepted: bool,
    pub occurred_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// This enum mirrors a frozen tagged wire contract. Keeping variants direct
// avoids making Box part of the public Rust API; serde still enforces bounds.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AgentResponse {
    Environment {
        record: EnvironmentRecord,
        evidence: Option<GuestEvidence>,
    },
    Deleted {
        proof: DeletionProof,
    },
    HumanSession {
        authorization: SessionAuthorization,
    },
    Automation {
        authorization: AutomationAuthorization,
    },
    Screen {
        channel: ScreenChannel,
    },
    InputAccepted {
        event_count: usize,
    },
    Logs {
        entries: Vec<String>,
        last_activity_at_unix_ms: u64,
    },
}

#[derive(Debug, Clone)]
pub struct ProvisionReceipt {
    pub volume: VolumeAttestation,
    pub evidence: GuestEvidence,
}

#[derive(Debug, Clone, Default)]
pub struct LifecycleReceipt {
    pub evidence: Option<GuestEvidence>,
    pub logs: Vec<String>,
}

/// Concrete providers receive only UUID-bound typed operations.
///
/// `create` and `destroy` form one recovery boundary keyed by
/// `remote_environment_id`. A provider must make `destroy` idempotent and able
/// to remove (or confirm the absence of) a partially-created environment when
/// the record is still [`EnvironmentState::Provisioning`]. In that state the
/// volume and key IDs can be nil because the process may have stopped before a
/// provisioning receipt was durably journaled.
pub trait AgentProvider {
    fn capabilities(&self) -> Vec<RemoteCapability>;
    fn create(&mut self, record: &EnvironmentRecord) -> Result<ProvisionReceipt, AgentError>;
    fn lifecycle(
        &mut self,
        operation: RemoteOperation,
        record: &EnvironmentRecord,
        log_limit: Option<u16>,
    ) -> Result<LifecycleReceipt, AgentError>;
    fn destroy(
        &mut self,
        record: &EnvironmentRecord,
    ) -> Result<ProviderDeletionReceipt, AgentError>;
    fn open_screen(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        expires_at_unix_ms: u64,
    ) -> Result<ScreenChannel, AgentError>;
    fn send_input(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        events: &[InputEvent],
    ) -> Result<(), AgentError>;
}

/// Production implementations must make replay claims and record updates
/// atomic and durable.
pub trait AgentStore {
    fn claim_request(
        &mut self,
        principal_id: Uuid,
        request_id: Uuid,
        nonce: &str,
        sequence: u64,
    ) -> Result<(), AgentError>;
    fn environment_ids(&self) -> Vec<Uuid>;
    fn environment(&self, silo_id: Uuid) -> Option<EnvironmentRecord>;
    fn insert_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError>;
    fn update_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError>;
    fn human_session(&self, silo_id: Uuid) -> Option<SessionAuthorization>;
    fn set_human_session(&mut self, authorization: SessionAuthorization) -> Result<(), AgentError>;
    fn automation(&self, authorization_id: Uuid) -> Option<AutomationAuthorization>;
    fn set_automation(&mut self, authorization: AutomationAuthorization) -> Result<(), AgentError>;
    /// Atomically records the deleted environment state and its proof. A
    /// durable implementation must not expose one without the other.
    fn commit_deletion(
        &mut self,
        record: EnvironmentRecord,
        proof: DeletionProof,
    ) -> Result<(), AgentError>;
    fn deletion_proof(&self, proof_id: Uuid) -> Option<DeletionProof>;
    fn append_activity(&mut self, activity: ActivityEntry) -> Result<(), AgentError>;
    fn activities(&self, silo_id: Uuid) -> Vec<ActivityEntry>;
}

#[derive(Default)]
pub struct MemoryAgentStore {
    environments: HashMap<Uuid, EnvironmentRecord>,
    human_sessions: HashMap<Uuid, SessionAuthorization>,
    automation: HashMap<Uuid, AutomationAuthorization>,
    proofs: HashMap<Uuid, DeletionProof>,
    request_ids: HashSet<Uuid>,
    nonces: HashSet<String>,
    sequences: HashMap<Uuid, u64>,
    activity: Vec<ActivityEntry>,
}

impl AgentStore for MemoryAgentStore {
    fn claim_request(
        &mut self,
        principal_id: Uuid,
        request_id: Uuid,
        nonce: &str,
        sequence: u64,
    ) -> Result<(), AgentError> {
        if self.request_ids.len() >= MAX_REPLAY_WINDOW_ENTRIES {
            return Err(AgentError::LimitExceeded(
                "Replay ledger is full; durable pruning is required.".to_owned(),
            ));
        }
        let last = self.sequences.get(&principal_id).copied().unwrap_or(0);
        if sequence <= last || self.request_ids.contains(&request_id) || self.nonces.contains(nonce)
        {
            return Err(AgentError::Replay);
        }
        self.request_ids.insert(request_id);
        self.nonces.insert(nonce.to_owned());
        self.sequences.insert(principal_id, sequence);
        Ok(())
    }

    fn environment_ids(&self) -> Vec<Uuid> {
        self.environments.keys().copied().collect()
    }

    fn environment(&self, silo_id: Uuid) -> Option<EnvironmentRecord> {
        self.environments.get(&silo_id).cloned()
    }

    fn insert_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError> {
        if self.environments.contains_key(&record.silo_id) {
            return Err(AgentError::Conflict(
                "A remote environment already exists for this Silo.".to_owned(),
            ));
        }
        self.environments.insert(record.silo_id, record);
        Ok(())
    }

    fn update_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError> {
        if !self.environments.contains_key(&record.silo_id) {
            return Err(AgentError::NotFound);
        }
        self.environments.insert(record.silo_id, record);
        Ok(())
    }

    fn human_session(&self, silo_id: Uuid) -> Option<SessionAuthorization> {
        self.human_sessions.get(&silo_id).cloned()
    }

    fn set_human_session(&mut self, authorization: SessionAuthorization) -> Result<(), AgentError> {
        self.human_sessions
            .insert(authorization.silo_id, authorization);
        Ok(())
    }

    fn automation(&self, authorization_id: Uuid) -> Option<AutomationAuthorization> {
        self.automation.get(&authorization_id).cloned()
    }

    fn set_automation(&mut self, authorization: AutomationAuthorization) -> Result<(), AgentError> {
        self.automation
            .insert(authorization.authorization_id, authorization);
        Ok(())
    }

    fn commit_deletion(
        &mut self,
        record: EnvironmentRecord,
        proof: DeletionProof,
    ) -> Result<(), AgentError> {
        if !self.environments.contains_key(&record.silo_id)
            || record.deletion_proof_id != Some(proof.proof_id)
            || record.state != EnvironmentState::Deleted
            || record.silo_id != proof.silo_id
            || record.binding_id != proof.binding_id
            || record.remote_environment_id != proof.remote_environment_id
            || record.volume.volume_id != proof.volume_id
        {
            return Err(AgentError::InvalidState(
                "Deletion record and proof do not form one atomic state transition.".to_owned(),
            ));
        }
        self.environments.insert(record.silo_id, record);
        self.proofs.insert(proof.proof_id, proof);
        Ok(())
    }

    fn deletion_proof(&self, proof_id: Uuid) -> Option<DeletionProof> {
        self.proofs.get(&proof_id).cloned()
    }

    fn append_activity(&mut self, activity: ActivityEntry) -> Result<(), AgentError> {
        if self.activity.len() == MAX_ACTIVITY_ENTRIES {
            self.activity.remove(0);
        }
        self.activity.push(activity);
        Ok(())
    }

    fn activities(&self, silo_id: Uuid) -> Vec<ActivityEntry> {
        self.activity
            .iter()
            .filter(|entry| entry.silo_id == silo_id)
            .cloned()
            .collect()
    }
}

pub struct AgentCore<P, S, C> {
    node: NodeDisclosure,
    provider: P,
    store: S,
    clock: C,
}

impl<P: AgentProvider, S: AgentStore, C: Clock> AgentCore<P, S, C> {
    pub fn new(node: NodeDisclosure, provider: P, store: S, clock: C) -> Result<Self, AgentError> {
        node.validate()?;
        validate_capabilities(&provider.capabilities())?;
        let mut core = Self {
            node,
            provider,
            store,
            clock,
        };
        core.recover_incomplete_creates()?;
        Ok(core)
    }

    pub fn node(&self) -> &NodeDisclosure {
        &self.node
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn execute(&mut self, request: AgentRequest) -> Result<AgentResponse, AgentError> {
        self.validate_request(&request)?;
        let now = self.clock.now_unix_ms();
        self.store.claim_request(
            request.principal.credential_id,
            request.request_id,
            &request.nonce,
            request.sequence,
        )?;
        let silo_id = command_silo_id(&request.command);
        let label = command_label(&request.command);
        let result = self.execute_authorized(&request.principal, request.command, now);
        self.store.append_activity(ActivityEntry {
            activity_id: Uuid::new_v4(),
            silo_id,
            principal: request.principal.kind,
            operation: label.to_owned(),
            accepted: result.is_ok(),
            occurred_at_unix_ms: now,
        })?;
        result
    }

    pub fn sweep_expired(&mut self) -> Vec<Result<DeletionProof, AgentError>> {
        let now = self.clock.now_unix_ms();
        self.store
            .environment_ids()
            .into_iter()
            .filter_map(|silo_id| {
                self.store
                    .environment(silo_id)
                    .filter(|record| {
                        record.state != EnvironmentState::Deleted
                            && record.expires_at_unix_ms <= now
                    })
                    .map(|record| {
                        let silo_id = record.silo_id;
                        let result = self.destroy_record(record, DeletionReason::TtlExpired, now);
                        let activity_result = self.store.append_activity(ActivityEntry {
                            activity_id: Uuid::new_v4(),
                            silo_id,
                            principal: PrincipalKind::ControlPlane,
                            operation: "destroy".to_owned(),
                            accepted: result.is_ok(),
                            occurred_at_unix_ms: now,
                        });
                        match activity_result {
                            Ok(()) => result,
                            Err(error) => Err(error),
                        }
                    })
            })
            .collect()
    }

    fn validate_request(&self, request: &AgentRequest) -> Result<(), AgentError> {
        if request.protocol_version != PROTOCOL_VERSION {
            return Err(AgentError::VersionMismatch);
        }
        if request.sequence == 0
            || request.nonce.len() < 32
            || request.nonce.len() > 128
            || !request
                .nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(AgentError::InvalidRequest(
                "Nonce or sequence is invalid.".to_owned(),
            ));
        }
        if request.sent_at_unix_ms.abs_diff(self.clock.now_unix_ms()) > MAX_CLOCK_SKEW_MS {
            return Err(AgentError::Stale);
        }
        if serde_json::to_vec(request)?.len() > MAX_MESSAGE_BYTES {
            return Err(AgentError::LimitExceeded(
                "Agent request exceeds 64 KiB.".to_owned(),
            ));
        }
        match request.principal.kind {
            PrincipalKind::ControlPlane if request.principal.authorization_id.is_none() => Ok(()),
            PrincipalKind::HumanSession | PrincipalKind::Automation
                if request.principal.authorization_id.is_some() =>
            {
                Ok(())
            }
            _ => Err(AgentError::Unauthorized(
                "Principal kind and authorization ID do not agree.".to_owned(),
            )),
        }
    }

    fn execute_authorized(
        &mut self,
        principal: &Principal,
        command: AgentCommand,
        now: u64,
    ) -> Result<AgentResponse, AgentError> {
        match command {
            AgentCommand::Create {
                silo_id,
                binding_id,
                remote_environment_id,
                ttl_seconds,
                network,
                cost_acknowledged,
            } => self.create(
                principal,
                silo_id,
                binding_id,
                remote_environment_id,
                ttl_seconds,
                network,
                cost_acknowledged,
                now,
            ),
            AgentCommand::Destroy {
                silo_id,
                confirm_destroy,
            } => {
                require_control(principal)?;
                if let Some(record) = self.store.environment(silo_id) {
                    if record.state == EnvironmentState::Deleted {
                        let proof = record
                            .deletion_proof_id
                            .and_then(|proof_id| self.store.deletion_proof(proof_id))
                            .filter(|proof| {
                                proof.silo_id == record.silo_id
                                    && proof.binding_id == record.binding_id
                                    && proof.remote_environment_id == record.remote_environment_id
                                    && proof.volume_id == record.volume.volume_id
                                    && proof.deleted_at_unix_ms == record.last_activity_at_unix_ms
                                    && deletion_resources_are_bound(
                                        &proof.resource_deletions,
                                        record.remote_environment_id,
                                        record.volume.volume_id,
                                        record.volume.key_id,
                                    )
                            })
                            .ok_or_else(|| {
                                AgentError::InvalidState(
                                    "Deleted environment is missing its bound deletion proof."
                                        .to_owned(),
                                )
                            })?;
                        return Ok(AgentResponse::Deleted { proof });
                    }
                }
                if !confirm_destroy {
                    return Err(AgentError::InvalidRequest(
                        "Destroy requires explicit confirmation unless the environment is already deleted."
                            .to_owned(),
                    ));
                }
                let record = self.record(silo_id, now, true)?;
                self.destroy_record(record, DeletionReason::UserConfirmed, now)
                    .map(|proof| AgentResponse::Deleted { proof })
            }
            AgentCommand::OpenHumanSession {
                silo_id,
                lifetime_seconds,
            } => self.open_human_session(principal, silo_id, lifetime_seconds, now),
            AgentCommand::CloseHumanSession { silo_id } => {
                let mut authorization = authorize_human(&self.store, principal, silo_id, now)?;
                authorization.revoked = true;
                self.store.set_human_session(authorization.clone())?;
                Ok(AgentResponse::HumanSession { authorization })
            }
            AgentCommand::GrantAutomation {
                silo_id,
                lifetime_seconds,
                scopes,
                approved_by_user,
            } => self.grant_automation(
                principal,
                silo_id,
                lifetime_seconds,
                scopes,
                approved_by_user,
                now,
            ),
            AgentCommand::RevokeAutomation {
                silo_id,
                authorization_id,
            } => {
                require_control(principal)?;
                let mut authorization = self
                    .store
                    .automation(authorization_id)
                    .filter(|item| item.silo_id == silo_id)
                    .ok_or(AgentError::NotFound)?;
                authorization.revoked = true;
                self.store.set_automation(authorization.clone())?;
                Ok(AgentResponse::Automation { authorization })
            }
            AgentCommand::OpenScreen { silo_id } => {
                let record = self.record(silo_id, now, false)?;
                let (authorization_id, authorization_expires_at_unix_ms) = authorize_scope(
                    &self.store,
                    principal,
                    silo_id,
                    now,
                    AutomationScope::ReadScreen,
                )?;
                let requested_expires_at_unix_ms = authorization_expires_at_unix_ms
                    .min(now.saturating_add(MAX_SCREEN_CHANNEL_LIFETIME_MS));
                let channel = self.provider.open_screen(
                    &record,
                    authorization_id,
                    requested_expires_at_unix_ms,
                )?;
                if channel.remote_environment_id != record.remote_environment_id
                    || channel.authorization_id != authorization_id
                    || channel.expires_at_unix_ms <= now
                    || channel.expires_at_unix_ms > requested_expires_at_unix_ms
                {
                    return Err(AgentError::Provider(
                        "Provider returned a mismatched or out-of-bounds screen channel."
                            .to_owned(),
                    ));
                }
                Ok(AgentResponse::Screen { channel })
            }
            AgentCommand::SendInput { silo_id, events } => {
                validate_input(&events)?;
                let record = self.record(silo_id, now, false)?;
                let authorization_id = authorize_input(&self.store, principal, silo_id, now)?;
                self.provider
                    .send_input(&record, authorization_id, &events)?;
                Ok(AgentResponse::InputAccepted {
                    event_count: events.len(),
                })
            }
            other => {
                require_control(principal)?;
                self.lifecycle(other, now)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create(
        &mut self,
        principal: &Principal,
        silo_id: Uuid,
        binding_id: Uuid,
        remote_environment_id: Uuid,
        ttl_seconds: u64,
        network: RemoteNetworkPolicy,
        cost_acknowledged: bool,
        now: u64,
    ) -> Result<AgentResponse, AgentError> {
        require_control(principal)?;
        if silo_id == Uuid::nil()
            || binding_id == Uuid::nil()
            || remote_environment_id == Uuid::nil()
        {
            return Err(AgentError::InvalidRequest(
                "Create identifiers must be non-zero UUIDs.".to_owned(),
            ));
        }
        if !cost_acknowledged {
            return Err(AgentError::Unauthorized(
                "The user must acknowledge the node cost disclosure.".to_owned(),
            ));
        }
        if !(60..=MAX_ENVIRONMENT_TTL_SECONDS).contains(&ttl_seconds) {
            return Err(AgentError::InvalidRequest(
                "Environment TTL must be between one minute and 30 days.".to_owned(),
            ));
        }
        if self.store.environment(silo_id).is_some() {
            return Err(AgentError::Conflict(
                "A remote environment already exists for this Silo.".to_owned(),
            ));
        }
        ensure_capability(&self.provider.capabilities(), RemoteOperation::Create)?;
        // Provisioning is not safe unless the same fixed provider can
        // compensate a partial create.
        ensure_capability(&self.provider.capabilities(), RemoteOperation::Destroy)?;
        let mut record = EnvironmentRecord {
            silo_id,
            binding_id,
            remote_environment_id,
            node_id: self.node.node_id,
            state: EnvironmentState::Provisioning,
            network,
            volume: VolumeAttestation {
                encrypted: false,
                key_custody: KeyCustody::UserControlled,
                volume_id: Uuid::nil(),
                key_id: Uuid::nil(),
            },
            created_at_unix_ms: now,
            expires_at_unix_ms: now + ttl_seconds * 1_000,
            last_activity_at_unix_ms: now,
            deletion_proof_id: None,
        };
        // Write intent before the external mutation. A crash from this point
        // through activation leaves a durable, UUID-bound recovery record.
        self.store.insert_environment(record.clone())?;
        let receipt = match self.provider.create(&record) {
            Ok(receipt) => receipt,
            Err(error) => {
                return Err(self.fail_create_with_compensation(record, error, now));
            }
        };

        // Journal provider-assigned resource IDs while the state is still
        // Provisioning. Invalid attestation is retained only long enough to
        // drive and prove compensating deletion.
        record.volume = receipt.volume.clone();
        if let Err(error) = self.store.update_environment(record.clone()) {
            return Err(self.fail_create_with_compensation(record, error, now));
        }
        if !provisioned_volume_is_valid(&receipt.volume) {
            let error = AgentError::Provider(
                "Provider did not attest a user-controlled encrypted volume.".to_owned(),
            );
            return Err(self.fail_create_with_compensation(record, error, now));
        }
        if let Err(error) = validate_evidence(&receipt.evidence, &record, now) {
            return Err(self.fail_create_with_compensation(record, error, now));
        }

        // This is the activation commit. If its durability is uncertain we do
        // not destroy: the disk contains either a recoverable Provisioning
        // record or the complete Created record, so the resource is never
        // untracked.
        record.state = EnvironmentState::Created;
        self.store.update_environment(record.clone())?;
        Ok(AgentResponse::Environment {
            record,
            evidence: Some(receipt.evidence),
        })
    }

    fn open_human_session(
        &mut self,
        principal: &Principal,
        silo_id: Uuid,
        lifetime_seconds: u64,
        now: u64,
    ) -> Result<AgentResponse, AgentError> {
        require_control(principal)?;
        if !(60..=MAX_HUMAN_SESSION_SECONDS).contains(&lifetime_seconds) {
            return Err(AgentError::InvalidRequest(
                "Human session lifetime must be one minute to eight hours.".to_owned(),
            ));
        }
        let record = self.record(silo_id, now, false)?;
        let authorization = SessionAuthorization {
            authorization_id: Uuid::new_v4(),
            silo_id,
            remote_environment_id: record.remote_environment_id,
            issued_at_unix_ms: now,
            expires_at_unix_ms: now + lifetime_seconds * 1_000,
            revoked: false,
        };
        self.store.set_human_session(authorization.clone())?;
        Ok(AgentResponse::HumanSession { authorization })
    }

    fn grant_automation(
        &mut self,
        principal: &Principal,
        silo_id: Uuid,
        lifetime_seconds: u64,
        scopes: Vec<AutomationScope>,
        approved_by_user: bool,
        now: u64,
    ) -> Result<AgentResponse, AgentError> {
        require_control(principal)?;
        if !approved_by_user
            || !(60..=MAX_AUTOMATION_SECONDS).contains(&lifetime_seconds)
            || scopes.is_empty()
            || scopes.len() > 2
            || scopes.iter().copied().collect::<HashSet<_>>().len() != scopes.len()
        {
            return Err(AgentError::InvalidRequest(
                "Automation requires explicit approval, unique scopes, and a one-minute to one-hour lifetime."
                    .to_owned(),
            ));
        }
        let record = self.record(silo_id, now, false)?;
        let authorization = AutomationAuthorization {
            authorization_id: Uuid::new_v4(),
            silo_id,
            remote_environment_id: record.remote_environment_id,
            issued_at_unix_ms: now,
            expires_at_unix_ms: now + lifetime_seconds * 1_000,
            scopes,
            approved_by_user,
            revoked: false,
        };
        self.store.set_automation(authorization.clone())?;
        Ok(AgentResponse::Automation { authorization })
    }

    fn lifecycle(&mut self, command: AgentCommand, now: u64) -> Result<AgentResponse, AgentError> {
        let silo_id = command_silo_id(&command);
        let mut record = self.record(silo_id, now, false)?;
        let (operation, network, log_limit) = match command {
            AgentCommand::Start { .. } => (RemoteOperation::Start, None, None),
            AgentCommand::Stop { .. } => (RemoteOperation::Stop, None, None),
            AgentCommand::Pause { .. } => (RemoteOperation::Pause, None, None),
            AgentCommand::Snapshot { .. } => (RemoteOperation::Snapshot, None, None),
            AgentCommand::ConfigureNetwork { network, .. } => {
                (RemoteOperation::ConfigureNetwork, Some(network), None)
            }
            AgentCommand::Health { .. } => (RemoteOperation::Health, None, None),
            AgentCommand::Logs { limit, .. } if (1..=200).contains(&limit) => {
                (RemoteOperation::Logs, None, Some(limit))
            }
            AgentCommand::Logs { .. } => {
                return Err(AgentError::LimitExceeded(
                    "Log limit must be between 1 and 200.".to_owned(),
                ))
            }
            _ => {
                return Err(AgentError::InvalidRequest(
                    "Command is not a lifecycle operation.".to_owned(),
                ))
            }
        };
        ensure_capability(&self.provider.capabilities(), operation)?;
        if let Some(network) = network {
            record.network = network;
        }
        let receipt = self.provider.lifecycle(operation, &record, log_limit)?;
        if operation == RemoteOperation::Logs {
            let limit = usize::from(log_limit.unwrap_or(0));
            if receipt.logs.len() > limit || receipt.logs.iter().any(|entry| entry.len() > 1_024) {
                return Err(AgentError::Provider(
                    "Provider logs exceeded negotiated limits.".to_owned(),
                ));
            }
            record.last_activity_at_unix_ms = now;
            self.store.update_environment(record)?;
            return Ok(AgentResponse::Logs {
                entries: receipt.logs,
                last_activity_at_unix_ms: now,
            });
        }
        if matches!(
            operation,
            RemoteOperation::Start | RemoteOperation::ConfigureNetwork | RemoteOperation::Health
        ) {
            let evidence = receipt.evidence.as_ref().ok_or_else(|| {
                AgentError::Provider("Provider omitted guest evidence.".to_owned())
            })?;
            validate_evidence(evidence, &record, now)?;
        }
        record.state = match operation {
            RemoteOperation::Start => EnvironmentState::Running,
            RemoteOperation::Stop => EnvironmentState::Stopped,
            RemoteOperation::Pause => EnvironmentState::Paused,
            _ => record.state,
        };
        record.last_activity_at_unix_ms = now;
        self.store.update_environment(record.clone())?;
        Ok(AgentResponse::Environment {
            record,
            evidence: receipt.evidence,
        })
    }

    fn record(
        &self,
        silo_id: Uuid,
        now: u64,
        allow_expired: bool,
    ) -> Result<EnvironmentRecord, AgentError> {
        let record = self
            .store
            .environment(silo_id)
            .ok_or(AgentError::NotFound)?;
        match record.state {
            EnvironmentState::Provisioning => {
                return Err(AgentError::InvalidState(
                    "Environment provisioning recovery is incomplete.".to_owned(),
                ));
            }
            EnvironmentState::Deleted => {
                return Err(AgentError::InvalidState(
                    "Environment has already been deleted.".to_owned(),
                ));
            }
            _ => {}
        }
        if !allow_expired && record.expires_at_unix_ms <= now {
            return Err(AgentError::Expired);
        }
        Ok(record)
    }

    fn destroy_record(
        &mut self,
        mut record: EnvironmentRecord,
        reason: DeletionReason,
        now: u64,
    ) -> Result<DeletionProof, AgentError> {
        ensure_capability(&self.provider.capabilities(), RemoteOperation::Destroy)?;
        let receipt = self.provider.destroy(&record)?;
        hydrate_recovered_resource_ids(&mut record, &receipt);
        if receipt.receipt_id == Uuid::nil()
            || receipt.remote_environment_id != record.remote_environment_id
            || receipt.volume_id != record.volume.volume_id
            || !deletion_resources_are_bound(
                &receipt.resource_deletions,
                record.remote_environment_id,
                record.volume.volume_id,
                record.volume.key_id,
            )
        {
            return Err(AgentError::Provider(
                "Deletion receipt does not contain one bound typed disposition for compute, volume, snapshot and ephemeral key."
                    .to_owned(),
            ));
        }
        let proof = DeletionProof {
            proof_id: Uuid::new_v4(),
            silo_id: record.silo_id,
            binding_id: record.binding_id,
            remote_environment_id: record.remote_environment_id,
            volume_id: record.volume.volume_id,
            provider_receipt_id: receipt.receipt_id,
            resource_deletions: receipt.resource_deletions,
            deleted_at_unix_ms: now,
            reason,
        };
        record.state = EnvironmentState::Deleted;
        record.deletion_proof_id = Some(proof.proof_id);
        record.last_activity_at_unix_ms = now;
        self.store.commit_deletion(record, proof.clone())?;
        Ok(proof)
    }

    fn recover_incomplete_creates(&mut self) -> Result<(), AgentError> {
        let now = self.clock.now_unix_ms();
        let records = self
            .store
            .environment_ids()
            .into_iter()
            .filter_map(|silo_id| self.store.environment(silo_id))
            .filter(|record| record.state == EnvironmentState::Provisioning)
            .collect::<Vec<_>>();
        for record in records {
            if record.silo_id == Uuid::nil()
                || record.binding_id == Uuid::nil()
                || record.remote_environment_id == Uuid::nil()
                || record.node_id != self.node.node_id
            {
                return Err(AgentError::InvalidState(
                    "Provisioning recovery record is not bound to this Agent node.".to_owned(),
                ));
            }
            self.destroy_record(record, DeletionReason::ProviderPolicy, now)?;
        }
        Ok(())
    }

    fn fail_create_with_compensation(
        &mut self,
        record: EnvironmentRecord,
        original_error: AgentError,
        now: u64,
    ) -> AgentError {
        match self.destroy_record(record, DeletionReason::ProviderPolicy, now) {
            Ok(_) => original_error,
            Err(_) => AgentError::Provider(
                "Create failed and compensating deletion is still pending; restart is required."
                    .to_owned(),
            ),
        }
    }
}

fn provisioned_volume_is_valid(volume: &VolumeAttestation) -> bool {
    volume.encrypted
        && volume.key_custody == KeyCustody::UserControlled
        && volume.volume_id != Uuid::nil()
        && volume.key_id != Uuid::nil()
}

fn hydrate_recovered_resource_ids(
    record: &mut EnvironmentRecord,
    receipt: &ProviderDeletionReceipt,
) {
    if record.state != EnvironmentState::Provisioning {
        return;
    }
    if record.volume.volume_id == Uuid::nil() {
        record.volume.volume_id = receipt.volume_id;
    }
    if record.volume.key_id == Uuid::nil() {
        if let Some(key_id) = receipt.resource_deletions.iter().find_map(|resource| {
            (resource.kind == DeletionResourceKind::EphemeralKey
                && resource.status == DeletionResourceStatus::Deleted)
                .then_some(resource.resource_id)
                .flatten()
        }) {
            record.volume.key_id = key_id;
        }
    }
}

fn validate_capabilities(capabilities: &[RemoteCapability]) -> Result<(), AgentError> {
    if capabilities.len() != RemoteOperation::ALL.len()
        || RemoteOperation::ALL.iter().any(|operation| {
            capabilities
                .iter()
                .filter(|item| item.operation == *operation)
                .count()
                != 1
        })
    {
        return Err(AgentError::Provider(
            "Provider must describe each of nine lifecycle operations exactly once.".to_owned(),
        ));
    }
    Ok(())
}

fn ensure_capability(
    capabilities: &[RemoteCapability],
    operation: RemoteOperation,
) -> Result<(), AgentError> {
    match capabilities
        .iter()
        .find(|item| item.operation == operation)
        .map(|item| &item.availability)
    {
        Some(CapabilityAvailability::Available) => Ok(()),
        Some(CapabilityAvailability::Unavailable { reason }) => {
            Err(AgentError::Unavailable(reason.clone()))
        }
        None => Err(AgentError::Provider(
            "Provider capability is missing.".to_owned(),
        )),
    }
}

fn validate_evidence(
    evidence: &GuestEvidence,
    record: &EnvironmentRecord,
    now: u64,
) -> Result<(), AgentError> {
    evidence
        .validate(record.binding_id, record.remote_environment_id, now)
        .map_err(|error| AgentError::Provider(error.to_string()))?;
    if !evidence.validates_required_proxy(&record.network) {
        return Err(AgentError::Provider(
            "Required proxy evidence failed closed.".to_owned(),
        ));
    }
    Ok(())
}

fn require_control(principal: &Principal) -> Result<(), AgentError> {
    if principal.kind == PrincipalKind::ControlPlane {
        Ok(())
    } else {
        Err(AgentError::Unauthorized(
            "Only the control-plane credential may change lifecycle.".to_owned(),
        ))
    }
}

fn authorize_human(
    store: &impl AgentStore,
    principal: &Principal,
    silo_id: Uuid,
    now: u64,
) -> Result<SessionAuthorization, AgentError> {
    let authorization = store.human_session(silo_id).ok_or(AgentError::NotFound)?;
    if principal.kind != PrincipalKind::HumanSession
        || principal.authorization_id != Some(authorization.authorization_id)
        || authorization.revoked
        || authorization.expires_at_unix_ms <= now
    {
        return Err(AgentError::Unauthorized(
            "Human session is mismatched, revoked, or expired.".to_owned(),
        ));
    }
    Ok(authorization)
}

fn authorize_automation(
    store: &impl AgentStore,
    principal: &Principal,
    silo_id: Uuid,
    now: u64,
) -> Result<AutomationAuthorization, AgentError> {
    let authorization_id = principal.authorization_id.ok_or_else(|| {
        AgentError::Unauthorized("Automation authorization ID is missing.".to_owned())
    })?;
    let authorization = store
        .automation(authorization_id)
        .filter(|item| item.silo_id == silo_id)
        .ok_or(AgentError::NotFound)?;
    if principal.kind != PrincipalKind::Automation
        || !authorization.approved_by_user
        || authorization.revoked
        || authorization.expires_at_unix_ms <= now
    {
        return Err(AgentError::Unauthorized(
            "Automation grant is mismatched, unapproved, revoked, or expired.".to_owned(),
        ));
    }
    Ok(authorization)
}

fn authorize_scope(
    store: &impl AgentStore,
    principal: &Principal,
    silo_id: Uuid,
    now: u64,
    scope: AutomationScope,
) -> Result<(Uuid, u64), AgentError> {
    if principal.kind == PrincipalKind::HumanSession {
        let authorization = authorize_human(store, principal, silo_id, now)?;
        return Ok((
            authorization.authorization_id,
            authorization.expires_at_unix_ms,
        ));
    }
    let authorization = authorize_automation(store, principal, silo_id, now)?;
    if !authorization.scopes.contains(&scope) {
        return Err(AgentError::Unauthorized(
            "Automation grant lacks the requested scope.".to_owned(),
        ));
    }
    Ok((
        authorization.authorization_id,
        authorization.expires_at_unix_ms,
    ))
}

fn authorize_input(
    store: &impl AgentStore,
    principal: &Principal,
    silo_id: Uuid,
    now: u64,
) -> Result<Uuid, AgentError> {
    if principal.kind == PrincipalKind::Automation
        && store
            .human_session(silo_id)
            .is_some_and(|session| !session.revoked && session.expires_at_unix_ms > now)
    {
        return Err(AgentError::Unauthorized(
            "Automation input is suspended while a human session is active.".to_owned(),
        ));
    }
    authorize_scope(store, principal, silo_id, now, AutomationScope::SendInput)
        .map(|(authorization_id, _)| authorization_id)
}

fn validate_input(events: &[InputEvent]) -> Result<(), AgentError> {
    if events.is_empty() || events.len() > MAX_INPUT_EVENTS {
        return Err(AgentError::LimitExceeded(
            "Input batch must contain 1 to 128 events.".to_owned(),
        ));
    }
    for event in events {
        match event {
            InputEvent::Key { code, .. }
                if code.is_empty()
                    || code.len() > 40
                    || !code
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
            {
                return Err(AgentError::InvalidRequest(
                    "Key code is invalid.".to_owned(),
                ))
            }
            InputEvent::PointerMove { x, y } if *x > 16_384 || *y > 16_384 => {
                return Err(AgentError::InvalidRequest(
                    "Pointer coordinate exceeds negotiated bounds.".to_owned(),
                ))
            }
            InputEvent::Text { value } => {
                bounded_text("input text", value, 1, 512)?;
                if value.chars().any(|character| {
                    character.is_control() && character != '\n' && character != '\t'
                }) {
                    return Err(AgentError::InvalidRequest(
                        "Input text contains unsupported control characters.".to_owned(),
                    ));
                }
            }
            InputEvent::Key { .. }
            | InputEvent::PointerMove { .. }
            | InputEvent::PointerButton { .. } => {}
        }
    }
    Ok(())
}

fn command_silo_id(command: &AgentCommand) -> Uuid {
    match command {
        AgentCommand::Create { silo_id, .. }
        | AgentCommand::Start { silo_id }
        | AgentCommand::Stop { silo_id }
        | AgentCommand::Pause { silo_id }
        | AgentCommand::Snapshot { silo_id }
        | AgentCommand::Destroy { silo_id, .. }
        | AgentCommand::ConfigureNetwork { silo_id, .. }
        | AgentCommand::Health { silo_id }
        | AgentCommand::Logs { silo_id, .. }
        | AgentCommand::OpenHumanSession { silo_id, .. }
        | AgentCommand::CloseHumanSession { silo_id }
        | AgentCommand::GrantAutomation { silo_id, .. }
        | AgentCommand::RevokeAutomation { silo_id, .. }
        | AgentCommand::OpenScreen { silo_id }
        | AgentCommand::SendInput { silo_id, .. } => *silo_id,
    }
}

fn command_label(command: &AgentCommand) -> &'static str {
    match command {
        AgentCommand::Create { .. } => "create",
        AgentCommand::Start { .. } => "start",
        AgentCommand::Stop { .. } => "stop",
        AgentCommand::Pause { .. } => "pause",
        AgentCommand::Snapshot { .. } => "snapshot",
        AgentCommand::Destroy { .. } => "destroy",
        AgentCommand::ConfigureNetwork { .. } => "configure_network",
        AgentCommand::Health { .. } => "health",
        AgentCommand::Logs { .. } => "logs",
        AgentCommand::OpenHumanSession { .. } => "open_human_session",
        AgentCommand::CloseHumanSession { .. } => "close_human_session",
        AgentCommand::GrantAutomation { .. } => "grant_automation",
        AgentCommand::RevokeAutomation { .. } => "revoke_automation",
        AgentCommand::OpenScreen { .. } => "open_screen",
        AgentCommand::SendInput { .. } => "send_input",
    }
}

fn bounded_text(label: &str, value: &str, min: usize, max: usize) -> Result<(), AgentError> {
    if value.trim() != value || value.len() < min || value.len() > max {
        return Err(AgentError::InvalidRequest(format!(
            "{label} must contain {min} to {max} bytes without surrounding whitespace."
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Remote Agent protocol version mismatch")]
    VersionMismatch,
    #[error("Remote Agent request is stale")]
    Stale,
    #[error("Remote Agent replay detected")]
    Replay,
    #[error("Remote Agent request is invalid: {0}")]
    InvalidRequest(String),
    #[error("Remote Agent request is unauthorized: {0}")]
    Unauthorized(String),
    #[error("Remote Agent operation is unavailable: {0}")]
    Unavailable(String),
    #[error("Remote Agent provider failed: {0}")]
    Provider(String),
    #[error("Remote Agent state conflict: {0}")]
    Conflict(String),
    #[error("Remote Agent record not found")]
    NotFound,
    #[error("Remote Agent environment expired")]
    Expired,
    #[error("Remote Agent state is invalid: {0}")]
    InvalidState(String),
    #[error("Remote Agent limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("Remote Agent durable store failed: {0}")]
    Store(String),
    #[error("Remote Agent JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DnsEvidence, EvidenceCheckState, ExitEvidence, GuestEvidenceSource, GuestHealthEvidence,
        GuestHealthState, ProxyEvidence, ProxyEvidenceState, WebRtcEvidence,
    };

    #[derive(Clone, Copy)]
    struct FixedClock(u64);

    impl Clock for FixedClock {
        fn now_unix_ms(&self) -> u64 {
            self.0
        }
    }

    struct FailingActivationStore {
        inner: MemoryAgentStore,
        update_calls: usize,
        fail_on_update: usize,
    }

    impl FailingActivationStore {
        fn new(fail_on_update: usize) -> Self {
            Self {
                inner: MemoryAgentStore::default(),
                update_calls: 0,
                fail_on_update,
            }
        }
    }

    impl AgentStore for FailingActivationStore {
        fn claim_request(
            &mut self,
            principal_id: Uuid,
            request_id: Uuid,
            nonce: &str,
            sequence: u64,
        ) -> Result<(), AgentError> {
            self.inner
                .claim_request(principal_id, request_id, nonce, sequence)
        }

        fn environment_ids(&self) -> Vec<Uuid> {
            self.inner.environment_ids()
        }

        fn environment(&self, silo_id: Uuid) -> Option<EnvironmentRecord> {
            self.inner.environment(silo_id)
        }

        fn insert_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError> {
            self.inner.insert_environment(record)
        }

        fn update_environment(&mut self, record: EnvironmentRecord) -> Result<(), AgentError> {
            self.update_calls += 1;
            if self.update_calls == self.fail_on_update {
                return Err(AgentError::Store(
                    "Injected activation persistence failure.".to_owned(),
                ));
            }
            self.inner.update_environment(record)
        }

        fn human_session(&self, silo_id: Uuid) -> Option<SessionAuthorization> {
            self.inner.human_session(silo_id)
        }

        fn set_human_session(
            &mut self,
            authorization: SessionAuthorization,
        ) -> Result<(), AgentError> {
            self.inner.set_human_session(authorization)
        }

        fn automation(&self, authorization_id: Uuid) -> Option<AutomationAuthorization> {
            self.inner.automation(authorization_id)
        }

        fn set_automation(
            &mut self,
            authorization: AutomationAuthorization,
        ) -> Result<(), AgentError> {
            self.inner.set_automation(authorization)
        }

        fn commit_deletion(
            &mut self,
            record: EnvironmentRecord,
            proof: DeletionProof,
        ) -> Result<(), AgentError> {
            self.inner.commit_deletion(record, proof)
        }

        fn deletion_proof(&self, proof_id: Uuid) -> Option<DeletionProof> {
            self.inner.deletion_proof(proof_id)
        }

        fn append_activity(&mut self, activity: ActivityEntry) -> Result<(), AgentError> {
            self.inner.append_activity(activity)
        }

        fn activities(&self, silo_id: Uuid) -> Vec<ActivityEntry> {
            self.inner.activities(silo_id)
        }
    }

    struct FakeProvider {
        omit_volume: bool,
        deletion_key_destroyed: bool,
        screen_expiry_extension_ms: u64,
    }

    impl FakeProvider {
        fn evidence(record: &EnvironmentRecord) -> GuestEvidence {
            GuestEvidence {
                protocol_version: PROTOCOL_VERSION,
                evidence_id: Uuid::new_v4(),
                binding_id: record.binding_id,
                remote_environment_id: record.remote_environment_id,
                source: GuestEvidenceSource::GuestAgent,
                sequence: 1,
                observed_at_unix_ms: 1_000_000,
                proxy: ProxyEvidence {
                    state: ProxyEvidenceState::NotRequired,
                    policy_id: None,
                },
                exit: ExitEvidence {
                    state: EvidenceCheckState::Verified,
                    public_addresses: vec!["203.0.113.10".to_owned()],
                },
                dns: DnsEvidence {
                    state: EvidenceCheckState::Verified,
                    resolvers: vec!["resolver.example".to_owned()],
                    leak_detected: false,
                },
                web_rtc: WebRtcEvidence {
                    state: EvidenceCheckState::Verified,
                    observed_candidates: Vec::new(),
                    leak_detected: false,
                },
                health: GuestHealthEvidence {
                    state: GuestHealthState::Healthy,
                    agent_version: "0.9.0".to_owned(),
                    checks: vec!["provider".to_owned()],
                },
            }
        }
    }

    impl AgentProvider for FakeProvider {
        fn capabilities(&self) -> Vec<RemoteCapability> {
            RemoteOperation::ALL
                .into_iter()
                .map(|operation| RemoteCapability {
                    operation,
                    availability: CapabilityAvailability::Available,
                })
                .collect()
        }

        fn create(&mut self, record: &EnvironmentRecord) -> Result<ProvisionReceipt, AgentError> {
            if self.omit_volume {
                return Ok(ProvisionReceipt {
                    volume: VolumeAttestation {
                        encrypted: false,
                        key_custody: KeyCustody::UserControlled,
                        volume_id: Uuid::nil(),
                        key_id: Uuid::nil(),
                    },
                    evidence: Self::evidence(record),
                });
            }
            Ok(ProvisionReceipt {
                volume: VolumeAttestation {
                    encrypted: true,
                    key_custody: KeyCustody::UserControlled,
                    volume_id: Uuid::new_v4(),
                    key_id: Uuid::new_v4(),
                },
                evidence: Self::evidence(record),
            })
        }

        fn lifecycle(
            &mut self,
            operation: RemoteOperation,
            record: &EnvironmentRecord,
            _log_limit: Option<u16>,
        ) -> Result<LifecycleReceipt, AgentError> {
            let logs = if operation == RemoteOperation::Logs {
                vec!["bounded".to_owned()]
            } else {
                Vec::new()
            };
            Ok(LifecycleReceipt {
                evidence: Some(Self::evidence(record)),
                logs,
            })
        }

        fn destroy(
            &mut self,
            record: &EnvironmentRecord,
        ) -> Result<ProviderDeletionReceipt, AgentError> {
            // A real provider resolves these IDs from the stable remote
            // environment ID when recovering a pre-receipt create intent.
            let volume_id = if record.volume.volume_id == Uuid::nil() {
                Uuid::from_u128(10)
            } else {
                record.volume.volume_id
            };
            let key_id = if record.volume.key_id == Uuid::nil() {
                Uuid::from_u128(11)
            } else {
                record.volume.key_id
            };
            let key_status = if self.deletion_key_destroyed {
                DeletionResourceStatus::Deleted
            } else {
                DeletionResourceStatus::NotApplicable
            };
            Ok(ProviderDeletionReceipt {
                receipt_id: Uuid::new_v4(),
                remote_environment_id: record.remote_environment_id,
                volume_id,
                resource_deletions: vec![
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::ComputeInstance,
                        resource_id: Some(record.remote_environment_id),
                        status: DeletionResourceStatus::Deleted,
                    },
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::PersistentVolume,
                        resource_id: Some(volume_id),
                        status: DeletionResourceStatus::Deleted,
                    },
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::Snapshot,
                        resource_id: None,
                        status: DeletionResourceStatus::NotApplicable,
                    },
                    ResourceDeletionItem {
                        kind: DeletionResourceKind::EphemeralKey,
                        resource_id: self.deletion_key_destroyed.then_some(key_id),
                        status: key_status,
                    },
                ],
            })
        }

        fn open_screen(
            &mut self,
            record: &EnvironmentRecord,
            authorization_id: Uuid,
            expires_at_unix_ms: u64,
        ) -> Result<ScreenChannel, AgentError> {
            Ok(ScreenChannel {
                channel_id: Uuid::new_v4(),
                remote_environment_id: record.remote_environment_id,
                authorization_id,
                expires_at_unix_ms: expires_at_unix_ms
                    .saturating_add(self.screen_expiry_extension_ms),
                transport: ScreenTransport::AuthenticatedEncryptedStream,
            })
        }

        fn send_input(
            &mut self,
            _record: &EnvironmentRecord,
            _authorization_id: Uuid,
            _events: &[InputEvent],
        ) -> Result<(), AgentError> {
            Ok(())
        }
    }

    fn node() -> NodeDisclosure {
        NodeDisclosure {
            node_id: Uuid::new_v4(),
            ownership: NodeOwnership::UserSelfHosted,
            operator_label: "My node".to_owned(),
            data_region: "SG".to_owned(),
            key_custody: KeyCustody::UserControlled,
            cost: CostDisclosure {
                currency: "USD".to_owned(),
                estimated_micros_per_hour: 100_000,
                notice: "Billed by your own provider.".to_owned(),
            },
        }
    }

    #[test]
    fn typed_deletion_resources_reject_missing_duplicate_unknown_and_unbound_items() {
        let remote_environment_id = Uuid::new_v4();
        let volume_id = Uuid::new_v4();
        let key_id = Uuid::new_v4();
        let resources = vec![
            ResourceDeletionItem {
                kind: DeletionResourceKind::ComputeInstance,
                resource_id: Some(remote_environment_id),
                status: DeletionResourceStatus::Deleted,
            },
            ResourceDeletionItem {
                kind: DeletionResourceKind::PersistentVolume,
                resource_id: Some(volume_id),
                status: DeletionResourceStatus::Deleted,
            },
            ResourceDeletionItem {
                kind: DeletionResourceKind::Snapshot,
                resource_id: None,
                status: DeletionResourceStatus::NotApplicable,
            },
            ResourceDeletionItem {
                kind: DeletionResourceKind::EphemeralKey,
                resource_id: Some(key_id),
                status: DeletionResourceStatus::Deleted,
            },
        ];
        assert!(deletion_resources_are_bound(
            &resources,
            remote_environment_id,
            volume_id,
            key_id,
        ));

        assert!(!deletion_resources_are_bound(
            &resources[..3],
            remote_environment_id,
            volume_id,
            key_id,
        ));
        let mut duplicate = resources.clone();
        duplicate[2] = duplicate[0].clone();
        assert!(!deletion_resources_are_bound(
            &duplicate,
            remote_environment_id,
            volume_id,
            key_id,
        ));
        let mut wrong_key = resources.clone();
        wrong_key[3].resource_id = Some(Uuid::new_v4());
        assert!(!deletion_resources_are_bound(
            &wrong_key,
            remote_environment_id,
            volume_id,
            key_id,
        ));
        let mut not_applicable_with_id = resources.clone();
        not_applicable_with_id[2].resource_id = Some(Uuid::new_v4());
        assert!(!deletion_resources_are_bound(
            &not_applicable_with_id,
            remote_environment_id,
            volume_id,
            key_id,
        ));

        let valid = serde_json::to_value(&resources[0]).unwrap();
        let mut unknown_kind = valid.clone();
        unknown_kind["kind"] = serde_json::json!("unknown_kind");
        assert!(serde_json::from_value::<ResourceDeletionItem>(unknown_kind).is_err());
        let mut unknown_status = valid;
        unknown_status["status"] = serde_json::json!("unknown_status");
        assert!(serde_json::from_value::<ResourceDeletionItem>(unknown_status).is_err());
    }

    fn control() -> Principal {
        Principal {
            kind: PrincipalKind::ControlPlane,
            credential_id: Uuid::new_v4(),
            authorization_id: None,
        }
    }

    fn request(principal: Principal, sequence: u64, command: AgentCommand) -> AgentRequest {
        let request_id = Uuid::new_v4();
        AgentRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            nonce: request_id.simple().to_string(),
            sequence,
            sent_at_unix_ms: 1_000_000,
            principal,
            command,
        }
    }

    fn create(core: &mut AgentCore<FakeProvider, MemoryAgentStore, FixedClock>, silo_id: Uuid) {
        core.execute(request(
            control(),
            1,
            AgentCommand::Create {
                silo_id,
                binding_id: Uuid::new_v4(),
                remote_environment_id: Uuid::new_v4(),
                ttl_seconds: 600,
                network: RemoteNetworkPolicy::Direct,
                cost_acknowledged: true,
            },
        ))
        .expect("create");
    }

    fn grant_read_screen(
        core: &mut AgentCore<FakeProvider, MemoryAgentStore, FixedClock>,
        silo_id: Uuid,
    ) -> AutomationAuthorization {
        match core
            .execute(request(
                control(),
                1,
                AgentCommand::GrantAutomation {
                    silo_id,
                    lifetime_seconds: 600,
                    scopes: vec![AutomationScope::ReadScreen],
                    approved_by_user: true,
                },
            ))
            .expect("grant screen automation")
        {
            AgentResponse::Automation { authorization } => authorization,
            _ => panic!("wrong response"),
        }
    }

    fn authorization_principal(kind: PrincipalKind, authorization_id: Uuid) -> Principal {
        Principal {
            kind,
            credential_id: Uuid::new_v4(),
            authorization_id: Some(authorization_id),
        }
    }

    #[test]
    fn create_rejects_an_unencrypted_volume() {
        let provider = FakeProvider {
            omit_volume: true,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        assert!(matches!(
            core.execute(request(
                control(),
                1,
                AgentCommand::Create {
                    silo_id,
                    binding_id: Uuid::new_v4(),
                    remote_environment_id: Uuid::new_v4(),
                    ttl_seconds: 600,
                    network: RemoteNetworkPolicy::Direct,
                    cost_acknowledged: true,
                }
            )),
            Err(AgentError::Provider(_))
        ));
        let record = core.store().environment(silo_id).unwrap();
        assert_eq!(record.state, EnvironmentState::Deleted);
        assert!(record
            .deletion_proof_id
            .and_then(|proof_id| core.store().deletion_proof(proof_id))
            .is_some());
    }

    #[test]
    fn create_rejects_unbound_identifiers_before_writing_provisioning_intent() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .unwrap();
        assert!(matches!(
            core.execute(request(
                control(),
                1,
                AgentCommand::Create {
                    silo_id: Uuid::nil(),
                    binding_id: Uuid::nil(),
                    remote_environment_id: Uuid::nil(),
                    ttl_seconds: 600,
                    network: RemoteNetworkPolicy::Direct,
                    cost_acknowledged: true,
                },
            )),
            Err(AgentError::InvalidRequest(_))
        ));
        assert!(core.store().environment_ids().is_empty());
    }

    #[test]
    fn startup_recovers_a_create_that_stopped_before_receipt_journaling() {
        let node = node();
        let silo_id = Uuid::new_v4();
        let mut store = MemoryAgentStore::default();
        store
            .insert_environment(EnvironmentRecord {
                silo_id,
                binding_id: Uuid::new_v4(),
                remote_environment_id: Uuid::new_v4(),
                node_id: node.node_id,
                state: EnvironmentState::Provisioning,
                network: RemoteNetworkPolicy::Direct,
                volume: VolumeAttestation {
                    encrypted: false,
                    key_custody: KeyCustody::UserControlled,
                    volume_id: Uuid::nil(),
                    key_id: Uuid::nil(),
                },
                created_at_unix_ms: 1_000_000,
                expires_at_unix_ms: 1_600_000,
                last_activity_at_unix_ms: 1_000_000,
                deletion_proof_id: None,
            })
            .unwrap();
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };

        let core = AgentCore::new(node, provider, store, FixedClock(1_000_001)).unwrap();
        let record = core.store().environment(silo_id).unwrap();
        assert_eq!(record.state, EnvironmentState::Deleted);
        let proof = core
            .store()
            .deletion_proof(record.deletion_proof_id.unwrap())
            .unwrap();
        assert_eq!(proof.reason, DeletionReason::ProviderPolicy);
        assert_eq!(proof.volume_id, Uuid::from_u128(10));
        assert!(deletion_resources_are_bound(
            &proof.resource_deletions,
            record.remote_environment_id,
            Uuid::from_u128(10),
            Uuid::from_u128(11),
        ));
    }

    #[test]
    fn activation_persistence_failure_remains_journaled_and_restart_compensates() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        // Update 1 journals provider IDs while still Provisioning. Update 2 is
        // the final Created activation commit and is forced to fail.
        let mut core = AgentCore::new(
            node(),
            provider,
            FailingActivationStore::new(2),
            FixedClock(1_000_000),
        )
        .unwrap();
        let silo_id = Uuid::new_v4();
        let result = core.execute(request(
            control(),
            1,
            AgentCommand::Create {
                silo_id,
                binding_id: Uuid::new_v4(),
                remote_environment_id: Uuid::new_v4(),
                ttl_seconds: 600,
                network: RemoteNetworkPolicy::Direct,
                cost_acknowledged: true,
            },
        ));
        assert!(matches!(result, Err(AgentError::Store(_))));
        assert_eq!(
            core.store().environment(silo_id).unwrap().state,
            EnvironmentState::Provisioning
        );

        let AgentCore {
            node,
            provider,
            store,
            clock,
        } = core;
        let recovered = AgentCore::new(node, provider, store, clock).unwrap();
        let record = recovered.store().environment(silo_id).unwrap();
        assert_eq!(record.state, EnvironmentState::Deleted);
        assert!(record
            .deletion_proof_id
            .and_then(|proof_id| recovered.store().deletion_proof(proof_id))
            .is_some());
    }

    #[test]
    fn ttl_sweep_records_a_bound_deletion_proof() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        create(&mut core, silo_id);
        core.clock = FixedClock(1_700_000);
        let result = core.sweep_expired();
        assert_eq!(result.len(), 1);
        let proof = result.into_iter().next().expect("entry").expect("proof");
        assert_eq!(proof.reason, DeletionReason::TtlExpired);
        assert!(deletion_resources_are_bound(
            &proof.resource_deletions,
            proof.remote_environment_id,
            proof.volume_id,
            core.store().environment(silo_id).unwrap().volume.key_id,
        ));
        assert!(core.store().deletion_proof(proof.proof_id).is_some());
        assert!(core
            .store()
            .activities(silo_id)
            .iter()
            .any(|entry| entry.operation == "destroy" && entry.accepted));

        let mut destroy_request = request(
            control(),
            1,
            AgentCommand::Destroy {
                silo_id,
                confirm_destroy: false,
            },
        );
        destroy_request.sent_at_unix_ms = 1_700_000;
        let returned = core
            .execute(destroy_request)
            .expect("destroy after TTL returns the existing proof");
        match returned {
            AgentResponse::Deleted { proof: returned } => assert_eq!(returned, proof),
            _ => panic!("wrong response"),
        }
    }

    #[test]
    fn automation_is_explicitly_scoped_and_yields_to_a_human_session() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        create(&mut core, silo_id);
        let automation = match core
            .execute(request(
                control(),
                1,
                AgentCommand::GrantAutomation {
                    silo_id,
                    lifetime_seconds: 600,
                    scopes: vec![AutomationScope::SendInput],
                    approved_by_user: true,
                },
            ))
            .expect("automation")
        {
            AgentResponse::Automation { authorization } => authorization,
            _ => panic!("wrong response"),
        };
        core.execute(request(
            control(),
            1,
            AgentCommand::OpenHumanSession {
                silo_id,
                lifetime_seconds: 600,
            },
        ))
        .expect("human");
        let automation_principal = Principal {
            kind: PrincipalKind::Automation,
            credential_id: Uuid::new_v4(),
            authorization_id: Some(automation.authorization_id),
        };
        assert!(matches!(
            core.execute(request(
                automation_principal,
                1,
                AgentCommand::SendInput {
                    silo_id,
                    events: vec![InputEvent::Key {
                        code: "KeyA".to_owned(),
                        pressed: true,
                    }],
                }
            )),
            Err(AgentError::Unauthorized(_))
        ));
    }

    #[test]
    fn screen_channel_is_capped_by_near_expiry_human_session() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        create(&mut core, silo_id);
        let authorization = match core
            .execute(request(
                control(),
                1,
                AgentCommand::OpenHumanSession {
                    silo_id,
                    lifetime_seconds: 60,
                },
            ))
            .expect("open human session")
        {
            AgentResponse::HumanSession { authorization } => authorization,
            _ => panic!("wrong response"),
        };
        let now = 1_059_000;
        core.clock = FixedClock(now);
        let mut open = request(
            authorization_principal(PrincipalKind::HumanSession, authorization.authorization_id),
            1,
            AgentCommand::OpenScreen { silo_id },
        );
        open.sent_at_unix_ms = now;

        let channel = match core.execute(open).expect("open screen") {
            AgentResponse::Screen { channel } => channel,
            _ => panic!("wrong response"),
        };
        assert_eq!(channel.expires_at_unix_ms, authorization.expires_at_unix_ms);
        assert_eq!(channel.expires_at_unix_ms - now, 1_000);
    }

    #[test]
    fn screen_channel_keeps_five_minute_lifetime_for_normal_automation_grant() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        create(&mut core, silo_id);
        let authorization = grant_read_screen(&mut core, silo_id);

        let channel = match core
            .execute(request(
                authorization_principal(PrincipalKind::Automation, authorization.authorization_id),
                1,
                AgentCommand::OpenScreen { silo_id },
            ))
            .expect("open screen")
        {
            AgentResponse::Screen { channel } => channel,
            _ => panic!("wrong response"),
        };
        assert_eq!(
            channel.expires_at_unix_ms,
            1_000_000 + MAX_SCREEN_CHANNEL_LIFETIME_MS
        );
        assert!(channel.expires_at_unix_ms < authorization.expires_at_unix_ms);
    }

    #[test]
    fn screen_channel_rejects_provider_expiry_beyond_requested_deadline() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 1,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        create(&mut core, silo_id);
        let authorization = grant_read_screen(&mut core, silo_id);

        assert!(matches!(
            core.execute(request(
                authorization_principal(PrincipalKind::Automation, authorization.authorization_id,),
                1,
                AgentCommand::OpenScreen { silo_id },
            )),
            Err(AgentError::Provider(_))
        ));
    }

    #[test]
    fn replay_unknown_fields_and_input_limits_are_rejected() {
        let provider = FakeProvider {
            omit_volume: false,
            deletion_key_destroyed: true,
            screen_expiry_extension_ms: 0,
        };
        let mut core = AgentCore::new(
            node(),
            provider,
            MemoryAgentStore::default(),
            FixedClock(1_000_000),
        )
        .expect("agent");
        let silo_id = Uuid::new_v4();
        let envelope = request(
            control(),
            1,
            AgentCommand::Create {
                silo_id,
                binding_id: Uuid::new_v4(),
                remote_environment_id: Uuid::new_v4(),
                ttl_seconds: 600,
                network: RemoteNetworkPolicy::Direct,
                cost_acknowledged: true,
            },
        );
        core.execute(envelope.clone()).expect("first");
        assert!(matches!(core.execute(envelope), Err(AgentError::Replay)));

        let mut value =
            serde_json::to_value(request(control(), 1, AgentCommand::Health { silo_id }))
                .expect("json");
        value
            .as_object_mut()
            .expect("object")
            .insert("shell".to_owned(), serde_json::json!("whoami"));
        assert!(serde_json::from_value::<AgentRequest>(value).is_err());
        assert!(validate_input(&vec![
            InputEvent::Key {
                code: "KeyA".to_owned(),
                pressed: true,
            };
            MAX_INPUT_EVENTS + 1
        ])
        .is_err());
    }
}
