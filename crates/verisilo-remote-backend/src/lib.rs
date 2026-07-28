//! VeriSilo V0.9 self-hosted remote browser environment controller prototype.
//!
//! The crate deliberately contains no HTTP client, cloud account, VM driver,
//! shell runner, generic command field, or caller-selected filesystem path.
//! A production adapter must implement [`RemoteTransport`] with ordinary PKI
//! validation *and* the configured certificate/SPKI pin, then report the
//! authenticated peer pin back to this controller for a second comparison.
//! The controller is executable against an adapter and is tested offline with
//! a deterministic transport.

pub mod agent;
pub mod transport;

use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_PAIRING_TOKEN_LIFETIME_MS: u64 = 5 * 60 * 1_000;
pub const MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS: u64 = 60 * 1_000;
pub const MAX_CLOCK_SKEW_MS: u64 = 2 * 60 * 1_000;
pub const MAX_EVIDENCE_AGE_MS: u64 = 2 * 60 * 1_000;
pub const MAX_LOG_ENTRIES: u16 = 200;
pub const MAX_REPLAY_WINDOW_ENTRIES: usize = 4_096;
pub const MAX_STORED_BINDINGS: usize = 10_000;
pub const MAX_AUTOMATION_AUTHORIZATIONS_PER_SILO: usize = 128;
pub const REMOTE_ORPHAN_NOTICE: &str = "The local binding was force-detached without authenticated remote deletion. The remote environment and billable resources may still be running; contact the self-hosted operator using this receipt.";
const MAX_TEXT_BYTES: usize = 1_024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointOwnership {
    UserSelfHosted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TlsPinKind {
    CertificateSha256,
    SpkiSha256,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsPin {
    pub kind: TlsPinKind,
    pub sha256: String,
}

impl TlsPin {
    pub fn validate(&self) -> Result<(), RemoteBackendError> {
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.sha256.bytes().all(|byte| byte == b'0')
        {
            return Err(RemoteBackendError::InvalidEndpoint(
                "TLS pin must be a non-zero, lowercase SHA-256 hex digest.".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TlsPinRotationOperation {
    AuthorizeTlsPinRotation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsPinRotationAuthorizationBody {
    pub operation: TlsPinRotationOperation,
    pub client_credential_id: Uuid,
    pub pairing_token_id: Uuid,
    pub new_pin: TlsPin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsPinRotationAuthorizationRequestEnvelope {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub nonce: String,
    pub sequence: u64,
    pub sent_at_unix_ms: u64,
    pub body: TlsPinRotationAuthorizationBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsPinRotationPairingClaim {
    pub challenge: String,
    pub server_id: Uuid,
    pub old_client_credential_id: Uuid,
    pub authorization_request_id: Uuid,
    pub authorization_request_nonce: String,
    pub authorization_request_sequence: u64,
    pub authorization_response_sequence: u64,
    pub authorization_expires_at_unix_ms: u64,
    pub pairing_token_id: Uuid,
    pub new_pin: TlsPin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TlsPinRotationAuthorizationResponseBody {
    Success {
        server_id: Uuid,
        client_credential_id: Uuid,
        pairing_token_id: Uuid,
        new_pin: TlsPin,
        challenge: String,
        authorization_expires_at_unix_ms: u64,
    },
    Rejected {
        code: RejectionCode,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsPinRotationAuthorizationResponseEnvelope {
    pub protocol_version: u16,
    pub response_id: Uuid,
    pub in_reply_to: Uuid,
    pub nonce: String,
    pub sent_at_unix_ms: u64,
    pub sequence: u64,
    pub body: TlsPinRotationAuthorizationResponseBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteEndpoint {
    pub ownership: EndpointOwnership,
    pub origin: String,
    pub pin: TlsPin,
}

impl RemoteEndpoint {
    pub fn validate(&self) -> Result<(), RemoteBackendError> {
        self.pin.validate()?;
        let url = Url::parse(&self.origin).map_err(|error| {
            RemoteBackendError::InvalidEndpoint(format!("Endpoint is not a URL: {error}"))
        })?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(RemoteBackendError::InvalidEndpoint(
                "Endpoint must be a credential-free HTTPS origin with no path, query, or fragment."
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum RemoteOperation {
    Create,
    Start,
    Stop,
    Pause,
    Snapshot,
    Destroy,
    ConfigureNetwork,
    Health,
    Logs,
}

impl RemoteOperation {
    pub const ALL: [Self; 9] = [
        Self::Create,
        Self::Start,
        Self::Stop,
        Self::Pause,
        Self::Snapshot,
        Self::Destroy,
        Self::ConfigureNetwork,
        Self::Health,
        Self::Logs,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "availability",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CapabilityAvailability {
    Available,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteCapability {
    pub operation: RemoteOperation,
    pub availability: CapabilityAvailability,
}

fn validate_capabilities(capabilities: &[RemoteCapability]) -> Result<(), RemoteBackendError> {
    if capabilities.len() != RemoteOperation::ALL.len() {
        return Err(RemoteBackendError::Protocol(
            "Capability response must describe exactly nine operations.".to_owned(),
        ));
    }
    for operation in RemoteOperation::ALL {
        if capabilities
            .iter()
            .filter(|capability| capability.operation == operation)
            .count()
            != 1
        {
            return Err(RemoteBackendError::Protocol(format!(
                "Capability response must describe {operation:?} exactly once."
            )));
        }
    }
    for capability in capabilities {
        if let CapabilityAvailability::Unavailable { reason } = &capability.availability {
            validate_text("unavailable reason", reason)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "mode",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RemoteNetworkPolicy {
    Direct,
    FixedProxy { required: bool, policy_id: Uuid },
}

impl RemoteNetworkPolicy {
    fn requires_proxy(&self) -> bool {
        matches!(self, Self::FixedProxy { required: true, .. })
    }

    fn policy_id(&self) -> Option<Uuid> {
        match self {
            Self::FixedProxy { policy_id, .. } => Some(*policy_id),
            Self::Direct => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OperationBody {
    Create {
        network: RemoteNetworkPolicy,
        ttl_seconds: u64,
        cost_acknowledged: bool,
    },
    Start {
        binding_id: Uuid,
        remote_environment_id: Uuid,
    },
    Stop {
        binding_id: Uuid,
        remote_environment_id: Uuid,
    },
    Pause {
        binding_id: Uuid,
        remote_environment_id: Uuid,
    },
    Snapshot {
        binding_id: Uuid,
        remote_environment_id: Uuid,
    },
    Destroy {
        binding_id: Uuid,
        remote_environment_id: Uuid,
        confirm_destroy: bool,
    },
    ConfigureNetwork {
        binding_id: Uuid,
        remote_environment_id: Uuid,
        network: RemoteNetworkPolicy,
    },
    Health {
        binding_id: Uuid,
        remote_environment_id: Uuid,
    },
    Logs {
        binding_id: Uuid,
        remote_environment_id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<Uuid>,
        limit: u16,
    },
}

impl OperationBody {
    fn operation(&self) -> RemoteOperation {
        match self {
            Self::Create { .. } => RemoteOperation::Create,
            Self::Start { .. } => RemoteOperation::Start,
            Self::Stop { .. } => RemoteOperation::Stop,
            Self::Pause { .. } => RemoteOperation::Pause,
            Self::Snapshot { .. } => RemoteOperation::Snapshot,
            Self::Destroy { .. } => RemoteOperation::Destroy,
            Self::ConfigureNetwork { .. } => RemoteOperation::ConfigureNetwork,
            Self::Health { .. } => RemoteOperation::Health,
            Self::Logs { .. } => RemoteOperation::Logs,
        }
    }

    fn validate(&self) -> Result<(), RemoteBackendError> {
        match self {
            Self::Create {
                ttl_seconds,
                cost_acknowledged,
                ..
            } => {
                if !cost_acknowledged {
                    return Err(RemoteBackendError::InvalidRequest(
                        "Create requires a separate, explicit cost acknowledgement.".to_owned(),
                    ));
                }
                if !(60..=agent::MAX_ENVIRONMENT_TTL_SECONDS).contains(ttl_seconds) {
                    return Err(RemoteBackendError::InvalidRequest(format!(
                        "Environment TTL must be between 60 and {} seconds.",
                        agent::MAX_ENVIRONMENT_TTL_SECONDS
                    )));
                }
                Ok(())
            }
            Self::Logs { limit, .. } => {
                if *limit == 0 || *limit > MAX_LOG_ENTRIES {
                    return Err(RemoteBackendError::LimitExceeded(
                        "Log limit must be between 1 and 200.".to_owned(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationRequestEnvelope {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub nonce: String,
    pub sequence: u64,
    pub sent_at_unix_ms: u64,
    pub silo_id: Uuid,
    pub body: OperationBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingRequestBody {
    pub operation: PairingOperation,
    pub approved_by_user: bool,
    pub pairing_token_id: Uuid,
    pub pairing_token: String,
    pub pairing_token_expires_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_pin_rotation: Option<TlsPinRotationPairingClaim>,
}

impl Drop for PairingRequestBody {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.pairing_token.zeroize();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PairingOperation {
    Pair,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingRequestEnvelope {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub nonce: String,
    pub sent_at_unix_ms: u64,
    pub body: PairingRequestBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingApproval {
    pub approved_by_user: bool,
    pub pairing_token_id: Uuid,
    pub pairing_token: String,
    pub pairing_token_expires_at_unix_ms: u64,
}

impl Drop for PairingApproval {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.pairing_token.zeroize();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuestEvidenceSource {
    GuestAgent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCheckState {
    Verified,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyEvidenceState {
    NotRequired,
    Enforced,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyEvidence {
    pub state: ProxyEvidenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExitEvidence {
    pub state: EvidenceCheckState,
    pub public_addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DnsEvidence {
    pub state: EvidenceCheckState,
    pub resolvers: Vec<String>,
    pub leak_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebRtcEvidence {
    pub state: EvidenceCheckState,
    pub observed_candidates: Vec<String>,
    pub leak_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuestHealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuestHealthEvidence {
    pub state: GuestHealthState,
    pub agent_version: String,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuestEvidence {
    pub protocol_version: u16,
    pub evidence_id: Uuid,
    pub binding_id: Uuid,
    pub remote_environment_id: Uuid,
    pub source: GuestEvidenceSource,
    pub sequence: u64,
    pub observed_at_unix_ms: u64,
    pub proxy: ProxyEvidence,
    pub exit: ExitEvidence,
    pub dns: DnsEvidence,
    pub web_rtc: WebRtcEvidence,
    pub health: GuestHealthEvidence,
}

impl GuestEvidence {
    fn validate(
        &self,
        binding_id: Uuid,
        remote_environment_id: Uuid,
        now_ms: u64,
    ) -> Result<(), RemoteBackendError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(RemoteBackendError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: self.protocol_version,
            });
        }
        if self.binding_id != binding_id || self.remote_environment_id != remote_environment_id {
            return Err(RemoteBackendError::BindingMismatch);
        }
        if self.sequence == 0 {
            return Err(RemoteBackendError::Protocol(
                "Guest evidence sequence must be positive.".to_owned(),
            ));
        }
        validate_fresh_timestamp(self.observed_at_unix_ms, now_ms, MAX_EVIDENCE_AGE_MS)?;
        validate_string_list("exit addresses", &self.exit.public_addresses, 16, 128)?;
        validate_string_list("DNS resolvers", &self.dns.resolvers, 16, 128)?;
        validate_string_list(
            "WebRTC candidates",
            &self.web_rtc.observed_candidates,
            32,
            128,
        )?;
        validate_text("guest agent version", &self.health.agent_version)?;
        validate_string_list("health checks", &self.health.checks, 32, MAX_TEXT_BYTES)?;
        Ok(())
    }

    fn validates_required_proxy(&self, network: &RemoteNetworkPolicy) -> bool {
        if !network.requires_proxy() {
            return true;
        }
        self.proxy.state == ProxyEvidenceState::Enforced
            && self.proxy.policy_id == network.policy_id()
            && self.exit.state == EvidenceCheckState::Verified
            && !self.exit.public_addresses.is_empty()
            && self.dns.state == EvidenceCheckState::Verified
            && !self.dns.leak_detected
            && self.web_rtc.state == EvidenceCheckState::Verified
            && !self.web_rtc.leak_detected
            && self.health.state == GuestHealthState::Healthy
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteResultState {
    Created,
    Started,
    Stopped,
    Paused,
    SnapshotCreated,
    Destroyed,
    NetworkConfigured,
    Healthy,
    LogsReturned,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteLogEntry {
    pub sequence: u64,
    pub observed_at_unix_ms: u64,
    pub level: RemoteLogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationResult {
    pub operation: RemoteOperation,
    pub silo_id: Uuid,
    pub binding_id: Uuid,
    pub remote_environment_id: Uuid,
    pub server_id: Uuid,
    pub last_activity_at_unix_ms: u64,
    pub state: RemoteResultState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<agent::VolumeAttestation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<GuestEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<Vec<RemoteLogEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_proof: Option<agent::DeletionProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// Direct variants intentionally preserve the public typed wire-contract API.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OperationResponseBody {
    Success {
        result: OperationResult,
    },
    Unavailable {
        operation: RemoteOperation,
        reason: String,
    },
    Rejected {
        code: RejectionCode,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RejectionCode {
    NotPaired,
    Unauthorized,
    InvalidState,
    InvalidRequest,
    StaleRequest,
    Replay,
    LimitExceeded,
    ProxyUnverified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationResponseEnvelope {
    pub protocol_version: u16,
    pub response_id: Uuid,
    pub in_reply_to: Uuid,
    pub nonce: String,
    pub sent_at_unix_ms: u64,
    pub sequence: u64,
    pub body: OperationResponseBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PairingResponseBody {
    Success {
        server_id: Uuid,
        client_credential_id: Uuid,
        node: agent::NodeDisclosure,
        client_credential: String,
        credential_expires_at_unix_ms: u64,
        capabilities: Vec<RemoteCapability>,
    },
    Rejected {
        code: PairingRejectionCode,
        message: String,
    },
}

impl Drop for PairingResponseBody {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Self::Success {
            client_credential, ..
        } = self
        {
            client_credential.zeroize();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PairingRejectionCode {
    ApprovalRequired,
    TokenExpired,
    TokenInvalid,
    Replay,
    LimitExceeded,
    RotationAuthorizationInvalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingResponseEnvelope {
    pub protocol_version: u16,
    pub response_id: Uuid,
    pub in_reply_to: Uuid,
    pub nonce: String,
    pub sent_at_unix_ms: u64,
    pub sequence: u64,
    pub body: PairingResponseBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// Direct variants intentionally preserve the public typed wire-contract API.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AgentControlResponseBody {
    Success {
        response: agent::AgentResponse,
    },
    Unavailable {
        reason: String,
    },
    Rejected {
        code: RejectionCode,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentResponseEnvelope {
    pub protocol_version: u16,
    pub response_id: Uuid,
    pub in_reply_to: Uuid,
    pub nonce: String,
    pub sent_at_unix_ms: u64,
    pub sequence: u64,
    pub body: AgentControlResponseBody,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentControlOperation {
    OpenHumanSession,
    CloseHumanSession,
    GrantAutomation,
    RevokeAutomation,
    OpenScreen,
    SendInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentInteractionReceipt {
    pub operation: AgentControlOperation,
    pub observed_at_unix_ms: u64,
    pub response: agent::AgentResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InteractivePrincipal {
    HumanSession { authorization_id: Uuid },
    Automation { authorization_id: Uuid },
}

/// Inputs to a concrete HTTPS adapter. `credential` is deliberately outside
/// the JSON body so protocol logs can redact it without parsing request data.
pub struct TransportRequest<'a> {
    pub endpoint: &'a RemoteEndpoint,
    pub credential: Option<&'a str>,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct TransportResponse {
    /// True only after normal hostname/chain/validity TLS verification.
    pub tls_validated: bool,
    /// The certificate or SPKI digest actually observed by the adapter.
    pub peer_pin: TlsPin,
    pub payload: Vec<u8>,
}

pub trait RemoteTransport {
    fn exchange(
        &mut self,
        request: TransportRequest<'_>,
    ) -> Result<TransportResponse, RemoteBackendError>;
}

pub trait Clock {
    fn now_unix_ms(&self) -> u64;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SiloBinding {
    pub silo_id: Uuid,
    pub binding_id: Uuid,
    pub remote_environment_id: Uuid,
    pub server_id: Uuid,
    pub endpoint: RemoteEndpoint,
    pub network: RemoteNetworkPolicy,
    pub volume: agent::VolumeAttestation,
    pub last_activity_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_session: Option<agent::SessionAuthorization>,
    #[serde(default)]
    pub automation_authorizations: Vec<agent::AutomationAuthorization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_screen_channel: Option<agent::ScreenChannel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_interaction: Option<AgentInteractionReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_evidence: Option<GuestEvidence>,
}

/// Durable local audit evidence for disaster recovery when authenticated
/// remote deletion cannot be completed. This is deliberately not a deletion
/// proof: it records only that the local binding was force-detached and that
/// the remote environment may still exist, run and incur charges.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteOrphanReceipt {
    pub receipt_id: Uuid,
    pub silo_id: Uuid,
    pub binding_id: Uuid,
    pub remote_environment_id: Uuid,
    pub server_id: Uuid,
    pub endpoint: RemoteEndpoint,
    pub detached_at_unix_ms: u64,
    pub notice: String,
}

impl RemoteOrphanReceipt {
    pub fn validate(&self) -> Result<(), RemoteBackendError> {
        self.endpoint.validate()?;
        if self.receipt_id == Uuid::nil()
            || self.silo_id == Uuid::nil()
            || self.binding_id == Uuid::nil()
            || self.remote_environment_id == Uuid::nil()
            || self.server_id == Uuid::nil()
            || self.detached_at_unix_ms == 0
            || self.notice != REMOTE_ORPHAN_NOTICE
        {
            return Err(RemoteBackendError::Store(
                "Remote orphan receipt has invalid identities, time, or risk notice.".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Persistence boundary for stable Silo-to-remote-environment bindings.
/// Production implementations must make `insert_if_absent` atomic and store
/// the state locally; no vendor service is assumed.
pub trait BindingStore {
    fn get(&self, silo_id: Uuid) -> Result<Option<SiloBinding>, RemoteBackendError>;
    fn insert_if_absent(&mut self, binding: SiloBinding) -> Result<(), RemoteBackendError>;
    fn update(&mut self, binding: SiloBinding) -> Result<(), RemoteBackendError>;
    fn remove(&mut self, silo_id: Uuid) -> Result<(), RemoteBackendError>;
}

#[derive(Debug, Default)]
pub struct MemoryBindingStore {
    bindings: HashMap<Uuid, SiloBinding>,
}

impl MemoryBindingStore {
    pub fn from_bindings(bindings: Vec<SiloBinding>) -> Result<Self, RemoteBackendError> {
        if bindings.len() > MAX_STORED_BINDINGS {
            return Err(RemoteBackendError::LimitExceeded(format!(
                "Binding snapshot exceeds {MAX_STORED_BINDINGS} entries."
            )));
        }
        let mut store = Self::default();
        for binding in bindings {
            validate_binding_snapshot(&binding)?;
            if store.bindings.insert(binding.silo_id, binding).is_some() {
                return Err(RemoteBackendError::Store(
                    "Binding snapshot contains duplicate Silo IDs.".to_owned(),
                ));
            }
        }
        Ok(store)
    }

    pub fn snapshot(&self) -> Vec<SiloBinding> {
        let mut bindings = self.bindings.values().cloned().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.silo_id);
        bindings
    }
}

fn validate_binding_snapshot(binding: &SiloBinding) -> Result<(), RemoteBackendError> {
    binding.endpoint.validate()?;
    if binding.silo_id == Uuid::nil()
        || binding.binding_id == Uuid::nil()
        || binding.remote_environment_id == Uuid::nil()
        || binding.server_id == Uuid::nil()
        || binding.last_activity_at_unix_ms == 0
        || !binding.volume.encrypted
        || binding.volume.key_custody != agent::KeyCustody::UserControlled
        || binding.volume.volume_id == Uuid::nil()
        || binding.volume.key_id == Uuid::nil()
        || matches!(
            binding.network,
            RemoteNetworkPolicy::FixedProxy {
                policy_id,
                ..
            } if policy_id == Uuid::nil()
        )
    {
        return Err(RemoteBackendError::Store(
            "Binding snapshot has invalid IDs, network policy, or encrypted-volume custody."
                .to_owned(),
        ));
    }

    if let Some(authorization) = &binding.human_session {
        validate_session_authorization_shape(authorization, binding)?;
    }
    if binding.automation_authorizations.len() > MAX_AUTOMATION_AUTHORIZATIONS_PER_SILO {
        return Err(RemoteBackendError::LimitExceeded(format!(
            "A Silo may retain at most {MAX_AUTOMATION_AUTHORIZATIONS_PER_SILO} automation authorizations."
        )));
    }
    let mut automation_ids = HashSet::new();
    for authorization in &binding.automation_authorizations {
        validate_automation_authorization_shape(authorization, binding)?;
        if !automation_ids.insert(authorization.authorization_id) {
            return Err(RemoteBackendError::Store(
                "Binding snapshot contains duplicate automation authorization IDs.".to_owned(),
            ));
        }
    }
    if let Some(channel) = &binding.last_screen_channel {
        validate_screen_channel_shape(channel, binding)?;
        let known_authorization = binding.human_session.as_ref().is_some_and(|authorization| {
            authorization.authorization_id == channel.authorization_id
        }) || binding
            .automation_authorizations
            .iter()
            .any(|authorization| authorization.authorization_id == channel.authorization_id);
        if !known_authorization {
            return Err(RemoteBackendError::Store(
                "Stored screen channel references an unknown authorization.".to_owned(),
            ));
        }
    }
    if let Some(receipt) = &binding.last_interaction {
        validate_interaction_receipt_shape(receipt, binding)?;
    }
    if binding.last_evidence.as_ref().is_some_and(|evidence| {
        evidence.binding_id != binding.binding_id
            || evidence.remote_environment_id != binding.remote_environment_id
    }) {
        return Err(RemoteBackendError::Store(
            "Stored guest evidence does not match the binding.".to_owned(),
        ));
    }
    Ok(())
}

impl BindingStore for MemoryBindingStore {
    fn get(&self, silo_id: Uuid) -> Result<Option<SiloBinding>, RemoteBackendError> {
        Ok(self.bindings.get(&silo_id).cloned())
    }

    fn insert_if_absent(&mut self, binding: SiloBinding) -> Result<(), RemoteBackendError> {
        if self.bindings.contains_key(&binding.silo_id) {
            return Err(RemoteBackendError::SiloAlreadyBound(binding.silo_id));
        }
        self.bindings.insert(binding.silo_id, binding);
        Ok(())
    }

    fn update(&mut self, binding: SiloBinding) -> Result<(), RemoteBackendError> {
        if !self.bindings.contains_key(&binding.silo_id) {
            return Err(RemoteBackendError::SiloNotBound(binding.silo_id));
        }
        self.bindings.insert(binding.silo_id, binding);
        Ok(())
    }

    fn remove(&mut self, silo_id: Uuid) -> Result<(), RemoteBackendError> {
        self.bindings.remove(&silo_id);
        Ok(())
    }
}

#[derive(Debug)]
struct PairingState {
    server_id: Uuid,
    client_credential_id: Uuid,
    node: agent::NodeDisclosure,
    credential: Zeroizing<String>,
    credential_expires_at_unix_ms: u64,
    capabilities: Vec<RemoteCapability>,
    last_client_sequence: u64,
    last_server_sequence: u64,
}

/// Serializable pairing material intended for an encrypted local store.
///
/// Callers must never expose this structure to a webview or logs. The desktop
/// keeps it inside the encrypted Vault payload and only reconstructs a backend
/// while the Vault is unlocked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingSnapshot {
    pub server_id: Uuid,
    pub client_credential_id: Uuid,
    pub node: agent::NodeDisclosure,
    pub client_credential: String,
    pub credential_expires_at_unix_ms: u64,
    pub capabilities: Vec<RemoteCapability>,
    #[serde(default)]
    pub last_client_sequence: u64,
    pub last_server_sequence: u64,
}

impl Drop for PairingSnapshot {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.client_credential.zeroize();
    }
}

/// Complete state needed to reconstruct the controller without changing a
/// Silo's stable remote binding or resetting replay counters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteBackendSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing: Option<PairingSnapshot>,
    #[serde(default)]
    pub used_pairing_token_ids: Vec<Uuid>,
    #[serde(default)]
    pub bindings: Vec<SiloBinding>,
}

#[derive(Debug, Default)]
struct ReplayWindow {
    seen: HashMap<String, u64>,
}

impl ReplayWindow {
    fn check_and_record(&mut self, nonce: &str, now_ms: u64) -> Result<(), RemoteBackendError> {
        validate_nonce(nonce)?;
        self.seen
            .retain(|_, recorded_at| now_ms.saturating_sub(*recorded_at) <= MAX_CLOCK_SKEW_MS * 2);
        if self.seen.contains_key(nonce) {
            return Err(RemoteBackendError::ReplayDetected);
        }
        if self.seen.len() >= MAX_REPLAY_WINDOW_ENTRIES {
            return Err(RemoteBackendError::LimitExceeded(
                "Response replay window is full.".to_owned(),
            ));
        }
        self.seen.insert(nonce.to_owned(), now_ms);
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum RemoteBackendError {
    #[error("Invalid remote endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("Invalid remote request: {0}")]
    InvalidRequest(String),
    #[error("Remote protocol error: {0}")]
    Protocol(String),
    #[error("Remote protocol version mismatch: expected {expected}, received {actual}")]
    VersionMismatch { expected: u16, actual: u16 },
    #[error("Remote transport did not validate ordinary TLS")]
    TlsNotValidated,
    #[error("Remote TLS certificate/public-key pin did not match")]
    PinMismatch,
    #[error("Pairing requires an explicit user approval")]
    PairingApprovalRequired,
    #[error("Pairing token is expired or exceeds the five-minute lifetime")]
    PairingTokenLifetime,
    #[error("Pairing token has already been used locally")]
    PairingTokenReplay,
    #[error("Remote backend is already paired")]
    AlreadyPaired,
    #[error("Remote backend is not paired")]
    NotPaired,
    #[error("Remote credential expired")]
    CredentialExpired,
    #[error("Replay detected in remote response")]
    ReplayDetected,
    #[error("Stale or future remote message timestamp")]
    StaleMessage,
    #[error("Remote response sequence did not advance")]
    SequenceReplay,
    #[error("Remote operation {operation:?} is unavailable: {reason}")]
    Unavailable {
        operation: RemoteOperation,
        reason: String,
    },
    #[error("Remote request was rejected ({code:?}): {message}")]
    Rejected {
        code: RejectionCode,
        message: String,
    },
    #[error("Remote pairing was rejected ({code:?}): {message}")]
    PairingRejected {
        code: PairingRejectionCode,
        message: String,
    },
    #[error("Silo {0} already has a stable remote binding")]
    SiloAlreadyBound(Uuid),
    #[error("Silo {0} has no remote binding")]
    SiloNotBound(Uuid),
    #[error("Remote response does not match the Silo binding")]
    BindingMismatch,
    #[error("TLS pin rotation must keep the existing HTTPS origin")]
    RotationOriginMismatch,
    #[error("TLS pin rotation requires a different certificate/public-key pin")]
    RotationPinUnchanged,
    #[error("TLS pin rotation authorization expired before the new pin was contacted")]
    RotationAuthorizationExpired,
    #[error("TLS pin rotation reached a different server: expected {expected}, received {actual}")]
    RotationServerMismatch { expected: Uuid, actual: Uuid },
    #[error("Force detach requires both local-removal and remote-orphan risk confirmations")]
    ForceDetachConfirmationRequired,
    #[error("Required proxy guest evidence is missing, stale, failed, or leaked")]
    RequiredProxyUnverified,
    #[error("Remote provider reported the operation as blocked")]
    RemoteBlocked,
    #[error("Remote interactive provider is unavailable: {0}")]
    InteractiveUnavailable(String),
    #[error("Remote interactive request was rejected ({code:?}): {message}")]
    InteractiveRejected {
        code: RejectionCode,
        message: String,
    },
    #[error("Remote message exceeds a protocol limit: {0}")]
    LimitExceeded(String),
    #[error("Remote JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Remote transport error: {0}")]
    Transport(String),
    #[error("Remote binding store error: {0}")]
    Store(String),
}

pub struct RemoteEnvironmentBackend<T, S = MemoryBindingStore, C = SystemClock> {
    endpoint: RemoteEndpoint,
    transport: T,
    store: S,
    clock: C,
    pairing: Option<PairingState>,
    used_pairing_tokens: HashSet<Uuid>,
    response_replay: ReplayWindow,
}

impl<T: RemoteTransport> RemoteEnvironmentBackend<T, MemoryBindingStore, SystemClock> {
    pub fn with_memory_store(
        endpoint: RemoteEndpoint,
        transport: T,
    ) -> Result<Self, RemoteBackendError> {
        Self::new(
            endpoint,
            transport,
            MemoryBindingStore::default(),
            SystemClock,
        )
    }
}

impl<T: RemoteTransport, C: Clock> RemoteEnvironmentBackend<T, MemoryBindingStore, C> {
    pub fn from_snapshot(
        endpoint: RemoteEndpoint,
        transport: T,
        clock: C,
        snapshot: RemoteBackendSnapshot,
    ) -> Result<Self, RemoteBackendError> {
        if snapshot.used_pairing_token_ids.len() > MAX_REPLAY_WINDOW_ENTRIES {
            return Err(RemoteBackendError::LimitExceeded(format!(
                "Pairing-token replay ledger exceeds {MAX_REPLAY_WINDOW_ENTRIES} entries."
            )));
        }
        let store = MemoryBindingStore::from_bindings(snapshot.bindings)?;
        let mut backend = Self::new(endpoint, transport, store, clock)?;
        backend.restore_used_pairing_tokens(snapshot.used_pairing_token_ids)?;
        if let Some(pairing) = snapshot.pairing {
            backend.restore_pairing(pairing)?;
        }
        Ok(backend)
    }

    pub fn export_snapshot(&self) -> RemoteBackendSnapshot {
        RemoteBackendSnapshot {
            pairing: self.pairing_snapshot(),
            used_pairing_token_ids: self.used_pairing_token_ids(),
            bindings: self.store.snapshot(),
        }
    }

    /// Re-pairs through a replacement pin at the same HTTPS origin, then
    /// swaps the endpoint, credential and every stable binding together in
    /// memory. Callers must persist [`Self::export_snapshot`] atomically.
    ///
    /// Any failure leaves the old endpoint, pairing and bindings intact. The
    /// attempted one-time token ID is still recorded in the replay ledger.
    pub fn rotate_tls_pin(
        &mut self,
        new_endpoint: RemoteEndpoint,
        approval: PairingApproval,
    ) -> Result<Uuid, RemoteBackendError> {
        let rotation_claim = self.begin_tls_pin_rotation(&new_endpoint, &approval)?;
        self.finish_tls_pin_rotation(new_endpoint, &approval, rotation_claim)
    }

    /// Validates the candidate, consumes its token ID locally and obtains a
    /// short-lived authorization over the old pinned TLS channel with the old
    /// bearer credential. No request reaches the new pin in this phase.
    pub fn begin_tls_pin_rotation(
        &mut self,
        new_endpoint: &RemoteEndpoint,
        approval: &PairingApproval,
    ) -> Result<TlsPinRotationPairingClaim, RemoteBackendError> {
        self.reserve_pairing_token(approval)?;
        new_endpoint.validate()?;

        let now_ms = self.clock.now_unix_ms();
        let old_pairing = self.pairing.as_ref().ok_or(RemoteBackendError::NotPaired)?;
        if old_pairing.credential_expires_at_unix_ms <= now_ms {
            return Err(RemoteBackendError::CredentialExpired);
        }
        let expected_server_id = old_pairing.server_id;
        if !same_https_origin(&self.endpoint, new_endpoint) {
            return Err(RemoteBackendError::RotationOriginMismatch);
        }
        if self.endpoint.pin == new_endpoint.pin {
            return Err(RemoteBackendError::RotationPinUnchanged);
        }

        let bindings = self.store.snapshot();
        if bindings.iter().any(|binding| {
            binding.endpoint != self.endpoint || binding.server_id != expected_server_id
        }) {
            return Err(RemoteBackendError::BindingMismatch);
        }

        self.authorize_tls_pin_rotation(new_endpoint, approval)
    }

    /// Completes a previously authorized rotation through the new pin. The
    /// endpoint, new pairing and every binding are swapped together only after
    /// the Agent consumes the exact authorization claim and returns the same
    /// server identity.
    pub fn finish_tls_pin_rotation(
        &mut self,
        new_endpoint: RemoteEndpoint,
        approval: &PairingApproval,
        rotation_claim: TlsPinRotationPairingClaim,
    ) -> Result<Uuid, RemoteBackendError> {
        new_endpoint.validate()?;
        let old_pairing = self.pairing.as_ref().ok_or(RemoteBackendError::NotPaired)?;
        if old_pairing.credential_expires_at_unix_ms <= self.clock.now_unix_ms() {
            return Err(RemoteBackendError::CredentialExpired);
        }
        let expected_server_id = old_pairing.server_id;
        if !same_https_origin(&self.endpoint, &new_endpoint) {
            return Err(RemoteBackendError::RotationOriginMismatch);
        }
        if self.endpoint.pin == new_endpoint.pin {
            return Err(RemoteBackendError::RotationPinUnchanged);
        }
        if !self
            .used_pairing_tokens
            .contains(&approval.pairing_token_id)
            || rotation_claim.server_id != expected_server_id
            || rotation_claim.old_client_credential_id != old_pairing.client_credential_id
            || rotation_claim.pairing_token_id != approval.pairing_token_id
            || rotation_claim.new_pin != new_endpoint.pin
        {
            return Err(RemoteBackendError::BindingMismatch);
        }
        if rotation_claim.authorization_expires_at_unix_ms <= self.clock.now_unix_ms() {
            return Err(RemoteBackendError::RotationAuthorizationExpired);
        }

        let mut bindings = self.store.snapshot();
        if bindings.iter().any(|binding| {
            binding.endpoint != self.endpoint || binding.server_id != expected_server_id
        }) {
            return Err(RemoteBackendError::BindingMismatch);
        }
        let new_pairing = self.exchange_pairing(&new_endpoint, approval, Some(rotation_claim))?;
        if new_pairing.server_id != expected_server_id {
            return Err(RemoteBackendError::RotationServerMismatch {
                expected: expected_server_id,
                actual: new_pairing.server_id,
            });
        }

        for binding in &mut bindings {
            binding.endpoint = new_endpoint.clone();
            // Authorization IDs are bearer capabilities. A new application
            // credential must never inherit grants issued to the old one.
            binding.human_session = None;
            binding.automation_authorizations.clear();
            binding.last_screen_channel = None;
        }
        let new_store = MemoryBindingStore::from_bindings(bindings)?;

        self.endpoint = new_endpoint;
        self.store = new_store;
        self.pairing = Some(new_pairing);
        Ok(expected_server_id)
    }
}

impl<T: RemoteTransport, S: BindingStore, C: Clock> RemoteEnvironmentBackend<T, S, C> {
    pub fn new(
        endpoint: RemoteEndpoint,
        transport: T,
        store: S,
        clock: C,
    ) -> Result<Self, RemoteBackendError> {
        endpoint.validate()?;
        Ok(Self {
            endpoint,
            transport,
            store,
            clock,
            pairing: None,
            used_pairing_tokens: HashSet::new(),
            response_replay: ReplayWindow::default(),
        })
    }

    pub fn endpoint(&self) -> &RemoteEndpoint {
        &self.endpoint
    }

    pub fn binding(&self, silo_id: Uuid) -> Result<Option<SiloBinding>, RemoteBackendError> {
        self.store.get(silo_id)
    }

    pub fn capabilities(&self) -> Option<&[RemoteCapability]> {
        self.pairing
            .as_ref()
            .map(|pairing| pairing.capabilities.as_slice())
    }

    pub fn pairing_snapshot(&self) -> Option<PairingSnapshot> {
        self.pairing.as_ref().map(|pairing| PairingSnapshot {
            server_id: pairing.server_id,
            client_credential_id: pairing.client_credential_id,
            node: pairing.node.clone(),
            client_credential: pairing.credential.to_string(),
            credential_expires_at_unix_ms: pairing.credential_expires_at_unix_ms,
            capabilities: pairing.capabilities.clone(),
            last_client_sequence: pairing.last_client_sequence,
            last_server_sequence: pairing.last_server_sequence,
        })
    }

    pub fn restore_pairing(&mut self, snapshot: PairingSnapshot) -> Result<(), RemoteBackendError> {
        if self.pairing.is_some() {
            return Err(RemoteBackendError::AlreadyPaired);
        }
        if !valid_secret_token(&snapshot.client_credential, 32, 512) {
            return Err(RemoteBackendError::Protocol(
                "Stored client credential must be 32-512 base64url characters.".to_owned(),
            ));
        }
        if snapshot.server_id == Uuid::nil() || snapshot.client_credential_id == Uuid::nil() {
            return Err(RemoteBackendError::Protocol(
                "Stored server and client credential IDs must be non-nil.".to_owned(),
            ));
        }
        validate_capabilities(&snapshot.capabilities)?;
        snapshot
            .node
            .validate()
            .map_err(|error| RemoteBackendError::Protocol(error.to_string()))?;
        if snapshot.credential_expires_at_unix_ms == 0 || snapshot.last_server_sequence == 0 {
            return Err(RemoteBackendError::Protocol(
                "Stored pairing expiry and server sequence must be positive.".to_owned(),
            ));
        }
        self.pairing = Some(PairingState {
            server_id: snapshot.server_id,
            client_credential_id: snapshot.client_credential_id,
            node: snapshot.node.clone(),
            credential: Zeroizing::new(snapshot.client_credential.clone()),
            credential_expires_at_unix_ms: snapshot.credential_expires_at_unix_ms,
            capabilities: snapshot.capabilities.clone(),
            last_client_sequence: snapshot.last_client_sequence,
            last_server_sequence: snapshot.last_server_sequence,
        });
        Ok(())
    }

    pub fn revoke_pairing(&mut self) {
        self.pairing = None;
        self.response_replay = ReplayWindow::default();
    }

    pub fn used_pairing_token_ids(&self) -> Vec<Uuid> {
        let mut ids = self.used_pairing_tokens.iter().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    pub fn restore_used_pairing_tokens(
        &mut self,
        ids: Vec<Uuid>,
    ) -> Result<(), RemoteBackendError> {
        if ids.len() > MAX_REPLAY_WINDOW_ENTRIES {
            return Err(RemoteBackendError::LimitExceeded(format!(
                "Pairing-token replay ledger exceeds {MAX_REPLAY_WINDOW_ENTRIES} entries."
            )));
        }
        let input_len = ids.len();
        let unique = ids.into_iter().collect::<HashSet<_>>();
        if unique.len() != input_len {
            return Err(RemoteBackendError::Store(
                "Pairing-token replay ledger contains duplicate IDs.".to_owned(),
            ));
        }
        self.used_pairing_tokens = unique;
        Ok(())
    }

    /// Validates and consumes a one-time pairing token locally without doing
    /// network I/O. Desktop callers use this to durably reserve the token
    /// before a pin-rotation exchange, so a failed exchange or final commit
    /// cannot make the token reusable.
    pub fn reserve_pairing_token(
        &mut self,
        approval: &PairingApproval,
    ) -> Result<(), RemoteBackendError> {
        validate_pairing_approval(approval, self.clock.now_unix_ms())?;
        if self
            .used_pairing_tokens
            .contains(&approval.pairing_token_id)
        {
            return Err(RemoteBackendError::PairingTokenReplay);
        }
        if self.used_pairing_tokens.len() >= MAX_REPLAY_WINDOW_ENTRIES {
            return Err(RemoteBackendError::LimitExceeded(format!(
                "Pairing-token replay ledger may retain at most {MAX_REPLAY_WINDOW_ENTRIES} entries."
            )));
        }
        self.used_pairing_tokens.insert(approval.pairing_token_id);
        Ok(())
    }

    pub fn pair(&mut self, approval: PairingApproval) -> Result<Uuid, RemoteBackendError> {
        if self.pairing.is_some() {
            return Err(RemoteBackendError::AlreadyPaired);
        }
        self.reserve_pairing_token(&approval)?;
        let endpoint = self.endpoint.clone();
        let pairing = self.exchange_pairing(&endpoint, &approval, None)?;
        let server_id = pairing.server_id;
        self.pairing = Some(pairing);
        Ok(server_id)
    }

    fn exchange_pairing(
        &mut self,
        endpoint: &RemoteEndpoint,
        approval: &PairingApproval,
        tls_pin_rotation: Option<TlsPinRotationPairingClaim>,
    ) -> Result<PairingState, RemoteBackendError> {
        let now_ms = self.clock.now_unix_ms();
        let request_id = Uuid::new_v4();
        let request = PairingRequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            nonce: new_nonce(),
            sent_at_unix_ms: now_ms,
            body: PairingRequestBody {
                operation: PairingOperation::Pair,
                approved_by_user: approval.approved_by_user,
                pairing_token_id: approval.pairing_token_id,
                pairing_token: approval.pairing_token.clone(),
                pairing_token_expires_at_unix_ms: approval.pairing_token_expires_at_unix_ms,
                tls_pin_rotation,
            },
        };
        let payload = Zeroizing::new(encode_json(&request)?);
        let response = self.transport.exchange(TransportRequest {
            endpoint,
            credential: None,
            payload: payload.as_slice(),
        })?;
        self.validate_transport_response_for_endpoint(endpoint, &response)?;
        let response_payload = Zeroizing::new(response.payload);
        let response: PairingResponseEnvelope = decode_json(response_payload.as_slice())?;
        self.validate_pairing_response_metadata(
            response.protocol_version,
            response.in_reply_to,
            request_id,
            &response.nonce,
            response.sent_at_unix_ms,
            response.sequence,
        )?;

        match &response.body {
            PairingResponseBody::Success {
                server_id,
                client_credential_id,
                node,
                client_credential,
                credential_expires_at_unix_ms,
                capabilities,
            } => {
                validate_capabilities(capabilities)?;
                node.validate()
                    .map_err(|error| RemoteBackendError::Protocol(error.to_string()))?;
                if !valid_secret_token(client_credential, 32, 512) {
                    return Err(RemoteBackendError::Protocol(
                        "Client credential must be 32-512 base64url characters.".to_owned(),
                    ));
                }
                if *server_id == Uuid::nil() || *client_credential_id == Uuid::nil() {
                    return Err(RemoteBackendError::Protocol(
                        "Server and client credential IDs must be non-nil.".to_owned(),
                    ));
                }
                if *credential_expires_at_unix_ms <= now_ms {
                    return Err(RemoteBackendError::CredentialExpired);
                }
                Ok(PairingState {
                    server_id: *server_id,
                    client_credential_id: *client_credential_id,
                    node: node.clone(),
                    credential: Zeroizing::new(client_credential.clone()),
                    credential_expires_at_unix_ms: *credential_expires_at_unix_ms,
                    capabilities: capabilities.clone(),
                    last_client_sequence: 0,
                    last_server_sequence: response.sequence,
                })
            }
            PairingResponseBody::Rejected { code, message } => {
                validate_text("pairing rejection", message)?;
                Err(RemoteBackendError::PairingRejected {
                    code: code.clone(),
                    message: message.clone(),
                })
            }
        }
    }

    fn authorize_tls_pin_rotation(
        &mut self,
        new_endpoint: &RemoteEndpoint,
        approval: &PairingApproval,
    ) -> Result<TlsPinRotationPairingClaim, RemoteBackendError> {
        let expected_server_id = self
            .pairing
            .as_ref()
            .ok_or(RemoteBackendError::NotPaired)?
            .server_id;
        let (now_ms, sequence, client_credential_id, credential) =
            self.reserve_authenticated_request()?;
        let request_id = Uuid::new_v4();
        let request = TlsPinRotationAuthorizationRequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            nonce: new_nonce(),
            sequence,
            sent_at_unix_ms: now_ms,
            body: TlsPinRotationAuthorizationBody {
                operation: TlsPinRotationOperation::AuthorizeTlsPinRotation,
                client_credential_id,
                pairing_token_id: approval.pairing_token_id,
                new_pin: new_endpoint.pin.clone(),
            },
        };
        let authorization_request_nonce = request.nonce.clone();
        let payload = encode_json(&request)?;
        let response = self.transport.exchange(TransportRequest {
            endpoint: &self.endpoint,
            credential: Some(credential.as_str()),
            payload: &payload,
        })?;
        self.validate_transport_response(&response)?;
        let response: TlsPinRotationAuthorizationResponseEnvelope = decode_json(&response.payload)?;
        let authorization_response_sequence = response.sequence;
        self.validate_response_metadata(
            response.protocol_version,
            response.in_reply_to,
            request_id,
            &response.nonce,
            response.sent_at_unix_ms,
            response.sequence,
        )?;
        match response.body {
            TlsPinRotationAuthorizationResponseBody::Success {
                server_id,
                client_credential_id: response_credential_id,
                pairing_token_id,
                new_pin,
                challenge,
                authorization_expires_at_unix_ms,
            } => {
                validate_nonce(&challenge)?;
                let lifetime = authorization_expires_at_unix_ms
                    .checked_sub(now_ms)
                    .ok_or(RemoteBackendError::StaleMessage)?;
                if server_id != expected_server_id
                    || response_credential_id != client_credential_id
                    || pairing_token_id != approval.pairing_token_id
                    || new_pin != new_endpoint.pin
                    || lifetime == 0
                    || lifetime > MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS
                {
                    return Err(RemoteBackendError::BindingMismatch);
                }
                Ok(TlsPinRotationPairingClaim {
                    challenge,
                    server_id,
                    old_client_credential_id: client_credential_id,
                    authorization_request_id: request_id,
                    authorization_request_nonce,
                    authorization_request_sequence: sequence,
                    authorization_response_sequence,
                    authorization_expires_at_unix_ms,
                    pairing_token_id,
                    new_pin,
                })
            }
            TlsPinRotationAuthorizationResponseBody::Rejected { code, message } => {
                validate_text("TLS pin rotation authorization rejection", &message)?;
                Err(RemoteBackendError::Rejected { code, message })
            }
        }
    }

    /// Removes only the local stable binding after two explicit disaster-
    /// recovery confirmations. It does not contact the Agent and cannot
    /// produce a deletion proof.
    pub fn force_detach_binding(
        &mut self,
        silo_id: Uuid,
        confirm_local_detach: bool,
        acknowledge_remote_orphan_risk: bool,
    ) -> Result<RemoteOrphanReceipt, RemoteBackendError> {
        if !confirm_local_detach || !acknowledge_remote_orphan_risk {
            return Err(RemoteBackendError::ForceDetachConfirmationRequired);
        }
        let binding = self
            .store
            .get(silo_id)?
            .ok_or(RemoteBackendError::SiloNotBound(silo_id))?;
        let receipt = RemoteOrphanReceipt {
            receipt_id: Uuid::new_v4(),
            silo_id: binding.silo_id,
            binding_id: binding.binding_id,
            remote_environment_id: binding.remote_environment_id,
            server_id: binding.server_id,
            endpoint: binding.endpoint.clone(),
            detached_at_unix_ms: self.clock.now_unix_ms(),
            notice: REMOTE_ORPHAN_NOTICE.to_owned(),
        };
        receipt.validate()?;
        self.store.remove(silo_id)?;
        Ok(receipt)
    }

    pub fn create(
        &mut self,
        silo_id: Uuid,
        network: RemoteNetworkPolicy,
        ttl_seconds: u64,
        cost_acknowledged: bool,
    ) -> Result<OperationResult, RemoteBackendError> {
        if self.store.get(silo_id)?.is_some() {
            return Err(RemoteBackendError::SiloAlreadyBound(silo_id));
        }
        let result = self.send_operation(
            silo_id,
            OperationBody::Create {
                network: network.clone(),
                ttl_seconds,
                cost_acknowledged,
            },
        )?;
        let server_id = self
            .pairing
            .as_ref()
            .ok_or(RemoteBackendError::NotPaired)?
            .server_id;
        let binding = SiloBinding {
            silo_id,
            binding_id: result.binding_id,
            remote_environment_id: result.remote_environment_id,
            server_id,
            endpoint: self.endpoint.clone(),
            network: network.clone(),
            volume: result.volume.clone().ok_or_else(|| {
                RemoteBackendError::Protocol(
                    "Create result did not include an encrypted volume attestation.".to_owned(),
                )
            })?,
            last_activity_at_unix_ms: result.last_activity_at_unix_ms,
            human_session: None,
            automation_authorizations: Vec::new(),
            last_screen_channel: None,
            last_interaction: None,
            last_evidence: result.evidence.clone(),
        };
        self.store.insert_if_absent(binding.clone())?;
        if result.state == RemoteResultState::Blocked {
            return Err(RemoteBackendError::RemoteBlocked);
        }
        self.require_proxy_evidence(&binding, self.clock.now_unix_ms())?;
        Ok(result)
    }

    pub fn start(&mut self, silo_id: Uuid) -> Result<OperationResult, RemoteBackendError> {
        let binding = self.require_binding(silo_id)?;
        self.require_proxy_evidence(&binding, self.clock.now_unix_ms())?;
        let result =
            self.send_bound_operation(&binding, RemoteOperation::Start, None, None, None)?;
        self.update_evidence_and_enforce(binding, &result)
    }

    pub fn stop(&mut self, silo_id: Uuid) -> Result<OperationResult, RemoteBackendError> {
        let binding = self.require_binding(silo_id)?;
        let result =
            self.send_bound_operation(&binding, RemoteOperation::Stop, None, None, None)?;
        self.update_optional_evidence(binding, &result)?;
        Ok(result)
    }

    pub fn pause(&mut self, silo_id: Uuid) -> Result<OperationResult, RemoteBackendError> {
        let binding = self.require_binding(silo_id)?;
        let result =
            self.send_bound_operation(&binding, RemoteOperation::Pause, None, None, None)?;
        self.update_optional_evidence(binding, &result)?;
        Ok(result)
    }

    pub fn snapshot(&mut self, silo_id: Uuid) -> Result<OperationResult, RemoteBackendError> {
        let binding = self.require_binding(silo_id)?;
        let result =
            self.send_bound_operation(&binding, RemoteOperation::Snapshot, None, None, None)?;
        self.update_optional_evidence(binding, &result)?;
        Ok(result)
    }

    pub fn destroy(
        &mut self,
        silo_id: Uuid,
        confirm_destroy: bool,
    ) -> Result<OperationResult, RemoteBackendError> {
        let binding = self.require_binding(silo_id)?;
        let result = self.send_bound_operation(
            &binding,
            RemoteOperation::Destroy,
            None,
            Some(confirm_destroy),
            None,
        )?;
        self.store.remove(silo_id)?;
        Ok(result)
    }

    pub fn configure_network(
        &mut self,
        silo_id: Uuid,
        network: RemoteNetworkPolicy,
    ) -> Result<OperationResult, RemoteBackendError> {
        let mut binding = self.require_binding(silo_id)?;
        let result = self.send_bound_operation(
            &binding,
            RemoteOperation::ConfigureNetwork,
            Some(network.clone()),
            None,
            None,
        )?;
        self.ensure_evidence_advances(&binding, &result)?;
        binding.network = network;
        binding.last_evidence = result.evidence.clone();
        binding.last_activity_at_unix_ms = result.last_activity_at_unix_ms;
        // Persist the new policy even when verification fails so a later start
        // cannot fall back to the old policy or lose track of remote state.
        self.store.update(binding.clone())?;
        if result.state == RemoteResultState::Blocked {
            return Err(RemoteBackendError::RemoteBlocked);
        }
        self.require_proxy_evidence(&binding, self.clock.now_unix_ms())?;
        Ok(result)
    }

    pub fn health(&mut self, silo_id: Uuid) -> Result<OperationResult, RemoteBackendError> {
        let binding = self.require_binding(silo_id)?;
        let result =
            self.send_bound_operation(&binding, RemoteOperation::Health, None, None, None)?;
        self.update_evidence_and_enforce(binding, &result)
    }

    pub fn logs(
        &mut self,
        silo_id: Uuid,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<OperationResult, RemoteBackendError> {
        let mut binding = self.require_binding(silo_id)?;
        let result = self.send_bound_operation(
            &binding,
            RemoteOperation::Logs,
            None,
            None,
            Some((cursor, limit)),
        )?;
        binding.last_activity_at_unix_ms = result.last_activity_at_unix_ms;
        self.store.update(binding)?;
        Ok(result)
    }

    pub fn open_human_session(
        &mut self,
        silo_id: Uuid,
        lifetime_seconds: u64,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        if !(60..=agent::MAX_HUMAN_SESSION_SECONDS).contains(&lifetime_seconds) {
            return Err(RemoteBackendError::InvalidRequest(
                "Human session lifetime must be between 60 seconds and eight hours.".to_owned(),
            ));
        }
        let mut binding = self.require_binding(silo_id)?;
        let response = self.send_agent_command(
            agent::PrincipalKind::ControlPlane,
            None,
            agent::AgentCommand::OpenHumanSession {
                silo_id,
                lifetime_seconds,
            },
        )?;
        let authorization = match &response {
            agent::AgentResponse::HumanSession { authorization } => authorization.clone(),
            _ => {
                return Err(RemoteBackendError::Protocol(
                    "openHumanSession returned the wrong Agent response type.".to_owned(),
                ))
            }
        };
        validate_session_authorization(&authorization, &binding, self.clock.now_unix_ms(), false)?;
        if authorization.expires_at_unix_ms - authorization.issued_at_unix_ms
            != lifetime_seconds * 1_000
        {
            return Err(RemoteBackendError::Protocol(
                "Human-session authorization lifetime differs from the user-approved request."
                    .to_owned(),
            ));
        }
        binding.human_session = Some(authorization);
        self.persist_interaction(binding, AgentControlOperation::OpenHumanSession, response)
    }

    pub fn close_human_session(
        &mut self,
        silo_id: Uuid,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        let mut binding = self.require_binding(silo_id)?;
        let stored = binding
            .human_session
            .as_ref()
            .ok_or_else(|| {
                RemoteBackendError::InvalidRequest(
                    "No human session authorization is stored for this Silo.".to_owned(),
                )
            })?
            .clone();
        let now_ms = self.clock.now_unix_ms();
        if stored.revoked || stored.expires_at_unix_ms <= now_ms {
            return Err(RemoteBackendError::InvalidRequest(
                "The human session is already revoked or expired.".to_owned(),
            ));
        }
        let response = self.send_agent_command(
            agent::PrincipalKind::HumanSession,
            Some(stored.authorization_id),
            agent::AgentCommand::CloseHumanSession { silo_id },
        )?;
        let authorization = match &response {
            agent::AgentResponse::HumanSession { authorization } => authorization.clone(),
            _ => {
                return Err(RemoteBackendError::Protocol(
                    "closeHumanSession returned the wrong Agent response type.".to_owned(),
                ))
            }
        };
        validate_session_authorization(&authorization, &binding, now_ms, true)?;
        if authorization.authorization_id != stored.authorization_id {
            return Err(RemoteBackendError::BindingMismatch);
        }
        binding.human_session = Some(authorization);
        self.persist_interaction(binding, AgentControlOperation::CloseHumanSession, response)
    }

    pub fn grant_automation(
        &mut self,
        silo_id: Uuid,
        lifetime_seconds: u64,
        scopes: Vec<agent::AutomationScope>,
        approved_by_user: bool,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        if !approved_by_user
            || !(60..=agent::MAX_AUTOMATION_SECONDS).contains(&lifetime_seconds)
            || scopes.is_empty()
            || scopes.len() > 2
            || scopes.iter().copied().collect::<HashSet<_>>().len() != scopes.len()
        {
            return Err(RemoteBackendError::InvalidRequest(
                "Automation requires explicit approval, unique scopes and a 60-3600 second lifetime."
                    .to_owned(),
            ));
        }
        let mut binding = self.require_binding(silo_id)?;
        let requested_scopes = scopes.iter().copied().collect::<HashSet<_>>();
        let response = self.send_agent_command(
            agent::PrincipalKind::ControlPlane,
            None,
            agent::AgentCommand::GrantAutomation {
                silo_id,
                lifetime_seconds,
                scopes,
                approved_by_user,
            },
        )?;
        let authorization = match &response {
            agent::AgentResponse::Automation { authorization } => authorization.clone(),
            _ => {
                return Err(RemoteBackendError::Protocol(
                    "grantAutomation returned the wrong Agent response type.".to_owned(),
                ))
            }
        };
        validate_automation_authorization(
            &authorization,
            &binding,
            self.clock.now_unix_ms(),
            false,
        )?;
        if authorization.expires_at_unix_ms - authorization.issued_at_unix_ms
            != lifetime_seconds * 1_000
            || authorization.scopes.iter().copied().collect::<HashSet<_>>() != requested_scopes
        {
            return Err(RemoteBackendError::Protocol(
                "Automation authorization differs from the user-approved lifetime or scope set."
                    .to_owned(),
            ));
        }
        if let Some(stored) = binding
            .automation_authorizations
            .iter_mut()
            .find(|stored| stored.authorization_id == authorization.authorization_id)
        {
            *stored = authorization;
        } else {
            if binding.automation_authorizations.len() >= MAX_AUTOMATION_AUTHORIZATIONS_PER_SILO {
                return Err(RemoteBackendError::LimitExceeded(format!(
                    "A Silo may retain at most {MAX_AUTOMATION_AUTHORIZATIONS_PER_SILO} automation authorizations."
                )));
            }
            binding.automation_authorizations.push(authorization);
        }
        self.persist_interaction(binding, AgentControlOperation::GrantAutomation, response)
    }

    pub fn revoke_automation(
        &mut self,
        silo_id: Uuid,
        authorization_id: Uuid,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        let mut binding = self.require_binding(silo_id)?;
        let index = binding
            .automation_authorizations
            .iter()
            .position(|authorization| authorization.authorization_id == authorization_id)
            .ok_or_else(|| {
                RemoteBackendError::InvalidRequest(
                    "The selected automation authorization is not stored for this Silo.".to_owned(),
                )
            })?;
        let response = self.send_agent_command(
            agent::PrincipalKind::ControlPlane,
            None,
            agent::AgentCommand::RevokeAutomation {
                silo_id,
                authorization_id,
            },
        )?;
        let authorization = match &response {
            agent::AgentResponse::Automation { authorization } => authorization.clone(),
            _ => {
                return Err(RemoteBackendError::Protocol(
                    "revokeAutomation returned the wrong Agent response type.".to_owned(),
                ))
            }
        };
        validate_automation_authorization(
            &authorization,
            &binding,
            self.clock.now_unix_ms(),
            true,
        )?;
        if authorization.authorization_id != authorization_id {
            return Err(RemoteBackendError::BindingMismatch);
        }
        binding.automation_authorizations[index] = authorization;
        self.persist_interaction(binding, AgentControlOperation::RevokeAutomation, response)
    }

    pub fn open_screen(
        &mut self,
        silo_id: Uuid,
        principal: InteractivePrincipal,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        let mut binding = self.require_binding(silo_id)?;
        let (kind, authorization_id, authorization_expires_at_unix_ms) =
            authorize_interactive_principal(
                &binding,
                &principal,
                self.clock.now_unix_ms(),
                agent::AutomationScope::ReadScreen,
                false,
            )?;
        let response = self.send_agent_command(
            kind,
            Some(authorization_id),
            agent::AgentCommand::OpenScreen { silo_id },
        )?;
        let channel = match &response {
            agent::AgentResponse::Screen { channel } => channel.clone(),
            _ => {
                return Err(RemoteBackendError::Protocol(
                    "openScreen returned the wrong Agent response type.".to_owned(),
                ))
            }
        };
        validate_screen_channel(
            &channel,
            &binding,
            authorization_id,
            authorization_expires_at_unix_ms,
            self.clock.now_unix_ms(),
        )?;
        binding.last_screen_channel = Some(channel);
        self.persist_interaction(binding, AgentControlOperation::OpenScreen, response)
    }

    pub fn send_input(
        &mut self,
        silo_id: Uuid,
        principal: InteractivePrincipal,
        events: Vec<agent::InputEvent>,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        validate_input_events(&events)?;
        let binding = self.require_binding(silo_id)?;
        let (kind, authorization_id, _) = authorize_interactive_principal(
            &binding,
            &principal,
            self.clock.now_unix_ms(),
            agent::AutomationScope::SendInput,
            true,
        )?;
        let expected_count = events.len();
        let response = self.send_agent_command(
            kind,
            Some(authorization_id),
            agent::AgentCommand::SendInput { silo_id, events },
        )?;
        match &response {
            agent::AgentResponse::InputAccepted { event_count }
                if *event_count == expected_count => {}
            _ => {
                return Err(RemoteBackendError::Protocol(
                    "sendInput returned the wrong Agent response type or event count.".to_owned(),
                ))
            }
        }
        self.persist_interaction(binding, AgentControlOperation::SendInput, response)
    }

    fn persist_interaction(
        &mut self,
        mut binding: SiloBinding,
        operation: AgentControlOperation,
        response: agent::AgentResponse,
    ) -> Result<AgentInteractionReceipt, RemoteBackendError> {
        let receipt = AgentInteractionReceipt {
            operation,
            observed_at_unix_ms: self.clock.now_unix_ms(),
            response,
        };
        binding.last_interaction = Some(receipt.clone());
        self.store.update(binding)?;
        Ok(receipt)
    }

    fn require_binding(&self, silo_id: Uuid) -> Result<SiloBinding, RemoteBackendError> {
        let binding = self
            .store
            .get(silo_id)?
            .ok_or(RemoteBackendError::SiloNotBound(silo_id))?;
        let pairing = self.pairing.as_ref().ok_or(RemoteBackendError::NotPaired)?;
        if binding.server_id != pairing.server_id || binding.endpoint != self.endpoint {
            return Err(RemoteBackendError::BindingMismatch);
        }
        Ok(binding)
    }

    fn send_bound_operation(
        &mut self,
        binding: &SiloBinding,
        operation: RemoteOperation,
        network: Option<RemoteNetworkPolicy>,
        confirm_destroy: Option<bool>,
        logs: Option<(Option<Uuid>, u16)>,
    ) -> Result<OperationResult, RemoteBackendError> {
        let bound = || (binding.binding_id, binding.remote_environment_id);
        let body = match operation {
            RemoteOperation::Create => {
                return Err(RemoteBackendError::Protocol(
                    "Create is not a bound operation.".to_owned(),
                ))
            }
            RemoteOperation::Start => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::Start {
                    binding_id,
                    remote_environment_id,
                }
            }
            RemoteOperation::Stop => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::Stop {
                    binding_id,
                    remote_environment_id,
                }
            }
            RemoteOperation::Pause => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::Pause {
                    binding_id,
                    remote_environment_id,
                }
            }
            RemoteOperation::Snapshot => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::Snapshot {
                    binding_id,
                    remote_environment_id,
                }
            }
            RemoteOperation::Destroy => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::Destroy {
                    binding_id,
                    remote_environment_id,
                    confirm_destroy: confirm_destroy.unwrap_or(false),
                }
            }
            RemoteOperation::ConfigureNetwork => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::ConfigureNetwork {
                    binding_id,
                    remote_environment_id,
                    network: network.ok_or_else(|| {
                        RemoteBackendError::InvalidRequest(
                            "configureNetwork requires a network policy.".to_owned(),
                        )
                    })?,
                }
            }
            RemoteOperation::Health => {
                let (binding_id, remote_environment_id) = bound();
                OperationBody::Health {
                    binding_id,
                    remote_environment_id,
                }
            }
            RemoteOperation::Logs => {
                let (binding_id, remote_environment_id) = bound();
                let (cursor, limit) = logs.ok_or_else(|| {
                    RemoteBackendError::InvalidRequest("logs requires a bounded limit.".to_owned())
                })?;
                OperationBody::Logs {
                    binding_id,
                    remote_environment_id,
                    cursor,
                    limit,
                }
            }
        };
        let result = self.send_operation(binding.silo_id, body)?;
        if result.binding_id != binding.binding_id
            || result.remote_environment_id != binding.remote_environment_id
            || result.server_id != binding.server_id
        {
            return Err(RemoteBackendError::BindingMismatch);
        }
        if operation == RemoteOperation::Destroy {
            let Some(proof) = result.deletion_proof.as_ref() else {
                return Err(RemoteBackendError::BindingMismatch);
            };
            if proof.volume_id != binding.volume.volume_id
                || !agent::deletion_resources_are_bound(
                    &proof.resource_deletions,
                    binding.remote_environment_id,
                    binding.volume.volume_id,
                    binding.volume.key_id,
                )
            {
                return Err(RemoteBackendError::BindingMismatch);
            }
        }
        Ok(result)
    }

    fn send_operation(
        &mut self,
        silo_id: Uuid,
        body: OperationBody,
    ) -> Result<OperationResult, RemoteBackendError> {
        body.validate()?;
        let operation = body.operation();
        // Destroy with `confirmDestroy: false` is a non-mutating recovery
        // query. The Agent alone knows whether the environment is already
        // deleted, so local capability negotiation must not block retrieval
        // of an existing durable proof. A live environment still fails closed
        // on the Agent unless explicit confirmation and provider capability
        // are both present.
        if operation != RemoteOperation::Destroy {
            self.require_available(operation)?;
        }
        let (now_ms, sequence, _credential_id, credential) =
            self.reserve_authenticated_request()?;
        let request_id = Uuid::new_v4();
        let request = OperationRequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            nonce: new_nonce(),
            sequence,
            sent_at_unix_ms: now_ms,
            silo_id,
            body,
        };
        let payload = encode_json(&request)?;
        let response = self.transport.exchange(TransportRequest {
            endpoint: &self.endpoint,
            credential: Some(credential.as_str()),
            payload: &payload,
        })?;
        self.validate_transport_response(&response)?;
        let response: OperationResponseEnvelope = decode_json(&response.payload)?;
        self.validate_response_metadata(
            response.protocol_version,
            response.in_reply_to,
            request_id,
            &response.nonce,
            response.sent_at_unix_ms,
            response.sequence,
        )?;

        match response.body {
            OperationResponseBody::Success { result } => {
                self.validate_result(&result, operation, silo_id, now_ms)?;
                Ok(result)
            }
            OperationResponseBody::Unavailable {
                operation: response_operation,
                reason,
            } => {
                validate_text("unavailable reason", &reason)?;
                if response_operation != operation {
                    return Err(RemoteBackendError::Protocol(
                        "Unavailable response names a different operation.".to_owned(),
                    ));
                }
                Err(RemoteBackendError::Unavailable { operation, reason })
            }
            OperationResponseBody::Rejected { code, message } => {
                validate_text("rejection message", &message)?;
                Err(RemoteBackendError::Rejected { code, message })
            }
        }
    }

    fn send_agent_command(
        &mut self,
        principal_kind: agent::PrincipalKind,
        authorization_id: Option<Uuid>,
        command: agent::AgentCommand,
    ) -> Result<agent::AgentResponse, RemoteBackendError> {
        let (now_ms, sequence, credential_id, credential) = self.reserve_authenticated_request()?;
        let request_id = Uuid::new_v4();
        let request = agent::AgentRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            nonce: new_nonce(),
            sequence,
            sent_at_unix_ms: now_ms,
            principal: agent::Principal {
                kind: principal_kind,
                credential_id,
                authorization_id,
            },
            command,
        };
        let payload = encode_json(&request)?;
        let response = self.transport.exchange(TransportRequest {
            endpoint: &self.endpoint,
            credential: Some(credential.as_str()),
            payload: &payload,
        })?;
        self.validate_transport_response(&response)?;
        let response: AgentResponseEnvelope = decode_json(&response.payload)?;
        self.validate_response_metadata(
            response.protocol_version,
            response.in_reply_to,
            request_id,
            &response.nonce,
            response.sent_at_unix_ms,
            response.sequence,
        )?;
        match response.body {
            AgentControlResponseBody::Success { response } => Ok(response),
            AgentControlResponseBody::Unavailable { reason } => {
                validate_text("interactive unavailable reason", &reason)?;
                Err(RemoteBackendError::InteractiveUnavailable(reason))
            }
            AgentControlResponseBody::Rejected { code, message } => {
                validate_text("interactive rejection", &message)?;
                Err(RemoteBackendError::InteractiveRejected { code, message })
            }
        }
    }

    fn reserve_authenticated_request(
        &mut self,
    ) -> Result<(u64, u64, Uuid, Zeroizing<String>), RemoteBackendError> {
        let now_ms = self.clock.now_unix_ms();
        let pairing = self.pairing.as_mut().ok_or(RemoteBackendError::NotPaired)?;
        if pairing.credential_expires_at_unix_ms <= now_ms {
            return Err(RemoteBackendError::CredentialExpired);
        }
        let sequence = pairing.last_client_sequence.checked_add(1).ok_or_else(|| {
            RemoteBackendError::LimitExceeded(
                "Client request sequence is exhausted; explicit re-pairing is required.".to_owned(),
            )
        })?;
        // Reserve before serialization/transport. A failed or uncertain
        // delivery therefore consumes a sequence instead of allowing reuse.
        pairing.last_client_sequence = sequence;
        Ok((
            now_ms,
            sequence,
            pairing.client_credential_id,
            Zeroizing::new(pairing.credential.to_string()),
        ))
    }

    fn require_available(&self, operation: RemoteOperation) -> Result<(), RemoteBackendError> {
        let pairing = self.pairing.as_ref().ok_or(RemoteBackendError::NotPaired)?;
        let capability = pairing
            .capabilities
            .iter()
            .find(|capability| capability.operation == operation)
            .ok_or_else(|| {
                RemoteBackendError::Protocol(format!("Missing capability for {operation:?}."))
            })?;
        match &capability.availability {
            CapabilityAvailability::Available => Ok(()),
            CapabilityAvailability::Unavailable { reason } => {
                Err(RemoteBackendError::Unavailable {
                    operation,
                    reason: reason.clone(),
                })
            }
        }
    }

    fn validate_transport_response(
        &self,
        response: &TransportResponse,
    ) -> Result<(), RemoteBackendError> {
        self.validate_transport_response_for_endpoint(&self.endpoint, response)
    }

    fn validate_transport_response_for_endpoint(
        &self,
        endpoint: &RemoteEndpoint,
        response: &TransportResponse,
    ) -> Result<(), RemoteBackendError> {
        if !response.tls_validated {
            return Err(RemoteBackendError::TlsNotValidated);
        }
        if response.peer_pin != endpoint.pin {
            return Err(RemoteBackendError::PinMismatch);
        }
        if response.payload.len() > MAX_MESSAGE_BYTES {
            return Err(RemoteBackendError::LimitExceeded(format!(
                "response is {} bytes; maximum is {MAX_MESSAGE_BYTES}",
                response.payload.len()
            )));
        }
        Ok(())
    }

    fn validate_pairing_response_metadata(
        &mut self,
        protocol_version: u16,
        in_reply_to: Uuid,
        request_id: Uuid,
        nonce: &str,
        sent_at_unix_ms: u64,
        sequence: u64,
    ) -> Result<(), RemoteBackendError> {
        if protocol_version != PROTOCOL_VERSION {
            return Err(RemoteBackendError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: protocol_version,
            });
        }
        if in_reply_to != request_id {
            return Err(RemoteBackendError::Protocol(
                "Response inReplyTo does not match the request.".to_owned(),
            ));
        }
        let now_ms = self.clock.now_unix_ms();
        validate_fresh_timestamp(sent_at_unix_ms, now_ms, MAX_CLOCK_SKEW_MS)?;
        self.response_replay.check_and_record(nonce, now_ms)?;
        if sequence == 0 {
            return Err(RemoteBackendError::SequenceReplay);
        }
        Ok(())
    }

    fn validate_response_metadata(
        &mut self,
        protocol_version: u16,
        in_reply_to: Uuid,
        request_id: Uuid,
        nonce: &str,
        sent_at_unix_ms: u64,
        sequence: u64,
    ) -> Result<(), RemoteBackendError> {
        if protocol_version != PROTOCOL_VERSION {
            return Err(RemoteBackendError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: protocol_version,
            });
        }
        if in_reply_to != request_id {
            return Err(RemoteBackendError::Protocol(
                "Response inReplyTo does not match the request.".to_owned(),
            ));
        }
        let now_ms = self.clock.now_unix_ms();
        validate_fresh_timestamp(sent_at_unix_ms, now_ms, MAX_CLOCK_SKEW_MS)?;
        self.response_replay.check_and_record(nonce, now_ms)?;
        if sequence == 0 {
            return Err(RemoteBackendError::SequenceReplay);
        }
        if let Some(pairing) = &mut self.pairing {
            if sequence <= pairing.last_server_sequence {
                return Err(RemoteBackendError::SequenceReplay);
            }
            pairing.last_server_sequence = sequence;
        }
        Ok(())
    }

    fn validate_result(
        &self,
        result: &OperationResult,
        operation: RemoteOperation,
        silo_id: Uuid,
        now_ms: u64,
    ) -> Result<(), RemoteBackendError> {
        if result.operation != operation || result.silo_id != silo_id {
            return Err(RemoteBackendError::Protocol(
                "Operation result does not match request operation/Silo.".to_owned(),
            ));
        }
        let expected_server_id = self
            .pairing
            .as_ref()
            .ok_or(RemoteBackendError::NotPaired)?
            .server_id;
        if result.server_id != expected_server_id
            || result.server_id == Uuid::nil()
            || result.last_activity_at_unix_ms == 0
            || result.last_activity_at_unix_ms > now_ms.saturating_add(MAX_CLOCK_SKEW_MS)
        {
            return Err(RemoteBackendError::BindingMismatch);
        }
        let state_ok = matches!(
            (operation, &result.state),
            (
                RemoteOperation::Create,
                RemoteResultState::Created | RemoteResultState::Blocked
            ) | (
                RemoteOperation::Start,
                RemoteResultState::Started | RemoteResultState::Blocked
            ) | (RemoteOperation::Stop, RemoteResultState::Stopped)
                | (RemoteOperation::Pause, RemoteResultState::Paused)
                | (
                    RemoteOperation::Snapshot,
                    RemoteResultState::SnapshotCreated
                )
                | (RemoteOperation::Destroy, RemoteResultState::Destroyed)
                | (
                    RemoteOperation::ConfigureNetwork,
                    RemoteResultState::NetworkConfigured | RemoteResultState::Blocked
                )
                | (
                    RemoteOperation::Health,
                    RemoteResultState::Healthy | RemoteResultState::Blocked
                )
                | (RemoteOperation::Logs, RemoteResultState::LogsReturned)
        );
        if !state_ok {
            return Err(RemoteBackendError::Protocol(
                "Operation result state does not match the operation.".to_owned(),
            ));
        }
        let evidence_required = matches!(
            operation,
            RemoteOperation::Create
                | RemoteOperation::Start
                | RemoteOperation::ConfigureNetwork
                | RemoteOperation::Health
        );
        if evidence_required && result.evidence.is_none() {
            return Err(RemoteBackendError::Protocol(
                "Guest evidence is required for this operation.".to_owned(),
            ));
        }
        if let Some(evidence) = &result.evidence {
            evidence.validate(result.binding_id, result.remote_environment_id, now_ms)?;
        }
        match (&operation, &result.volume) {
            (RemoteOperation::Create, Some(volume))
                if volume.encrypted
                    && volume.key_custody == agent::KeyCustody::UserControlled
                    && volume.volume_id != Uuid::nil()
                    && volume.key_id != Uuid::nil() => {}
            (RemoteOperation::Create, _) => {
                return Err(RemoteBackendError::Protocol(
                    "Create must attest a non-nil, user-controlled encrypted volume.".to_owned(),
                ))
            }
            (_, Some(_)) => {
                return Err(RemoteBackendError::Protocol(
                    "Only create results may contain a volume attestation.".to_owned(),
                ))
            }
            (_, None) => {}
        }
        match (&operation, &result.deletion_proof) {
            (RemoteOperation::Destroy, Some(proof)) => {
                validate_deletion_proof(proof, result)?;
            }
            (RemoteOperation::Destroy, None) => {
                return Err(RemoteBackendError::Protocol(
                    "Destroy must include a bound deletion proof.".to_owned(),
                ))
            }
            (_, Some(_)) => {
                return Err(RemoteBackendError::Protocol(
                    "Only destroy results may contain a deletion proof.".to_owned(),
                ))
            }
            (_, None) => {}
        }
        if operation != RemoteOperation::Logs
            && (result.logs.is_some() || result.next_cursor.is_some())
        {
            return Err(RemoteBackendError::Protocol(
                "Only logs responses may contain log fields.".to_owned(),
            ));
        }
        if let Some(logs) = &result.logs {
            if logs.len() > usize::from(MAX_LOG_ENTRIES) {
                return Err(RemoteBackendError::LimitExceeded(
                    "Remote returned more than 200 log entries.".to_owned(),
                ));
            }
            for log in logs {
                if log.sequence == 0 {
                    return Err(RemoteBackendError::Protocol(
                        "Log sequence must be positive.".to_owned(),
                    ));
                }
                validate_fresh_timestamp(log.observed_at_unix_ms, now_ms, MAX_EVIDENCE_AGE_MS)?;
                validate_text("log message", &log.message)?;
            }
        }
        Ok(())
    }

    fn update_optional_evidence(
        &mut self,
        mut binding: SiloBinding,
        result: &OperationResult,
    ) -> Result<(), RemoteBackendError> {
        if let Some(evidence) = &result.evidence {
            if binding
                .last_evidence
                .as_ref()
                .is_some_and(|previous| evidence.sequence <= previous.sequence)
            {
                return Err(RemoteBackendError::SequenceReplay);
            }
            binding.last_evidence = Some(evidence.clone());
        }
        binding.last_activity_at_unix_ms = result.last_activity_at_unix_ms;
        self.store.update(binding)?;
        Ok(())
    }

    fn update_evidence_and_enforce(
        &mut self,
        mut binding: SiloBinding,
        result: &OperationResult,
    ) -> Result<OperationResult, RemoteBackendError> {
        self.ensure_evidence_advances(&binding, result)?;
        binding.last_evidence = result.evidence.clone();
        binding.last_activity_at_unix_ms = result.last_activity_at_unix_ms;
        self.store.update(binding.clone())?;
        if result.state == RemoteResultState::Blocked {
            return Err(RemoteBackendError::RemoteBlocked);
        }
        self.require_proxy_evidence(&binding, self.clock.now_unix_ms())?;
        Ok(result.clone())
    }

    fn ensure_evidence_advances(
        &self,
        binding: &SiloBinding,
        result: &OperationResult,
    ) -> Result<(), RemoteBackendError> {
        if let (Some(previous), Some(current)) = (&binding.last_evidence, &result.evidence) {
            if current.sequence <= previous.sequence {
                return Err(RemoteBackendError::SequenceReplay);
            }
        }
        Ok(())
    }

    fn require_proxy_evidence(
        &self,
        binding: &SiloBinding,
        now_ms: u64,
    ) -> Result<(), RemoteBackendError> {
        if !binding.network.requires_proxy() {
            return Ok(());
        }
        let evidence = binding
            .last_evidence
            .as_ref()
            .ok_or(RemoteBackendError::RequiredProxyUnverified)?;
        evidence
            .validate(binding.binding_id, binding.remote_environment_id, now_ms)
            .map_err(|_| RemoteBackendError::RequiredProxyUnverified)?;
        if !evidence.validates_required_proxy(&binding.network) {
            return Err(RemoteBackendError::RequiredProxyUnverified);
        }
        Ok(())
    }
}

fn validate_session_authorization_shape(
    authorization: &agent::SessionAuthorization,
    binding: &SiloBinding,
) -> Result<(), RemoteBackendError> {
    let lifetime_ms = authorization
        .expires_at_unix_ms
        .checked_sub(authorization.issued_at_unix_ms);
    if authorization.authorization_id == Uuid::nil()
        || authorization.silo_id != binding.silo_id
        || authorization.remote_environment_id != binding.remote_environment_id
        || !lifetime_ms.is_some_and(|lifetime_ms| {
            (60_000..=agent::MAX_HUMAN_SESSION_SECONDS * 1_000).contains(&lifetime_ms)
        })
    {
        return Err(RemoteBackendError::Protocol(
            "Human-session authorization is not bound to this environment or has an invalid lifetime."
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_session_authorization(
    authorization: &agent::SessionAuthorization,
    binding: &SiloBinding,
    now_ms: u64,
    expected_revoked: bool,
) -> Result<(), RemoteBackendError> {
    validate_session_authorization_shape(authorization, binding)?;
    if authorization.revoked != expected_revoked
        || (!expected_revoked
            && (authorization.expires_at_unix_ms <= now_ms
                || authorization.issued_at_unix_ms.abs_diff(now_ms) > MAX_CLOCK_SKEW_MS))
    {
        return Err(RemoteBackendError::Protocol(
            "Human-session authorization has an invalid revocation or freshness state.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_automation_authorization_shape(
    authorization: &agent::AutomationAuthorization,
    binding: &SiloBinding,
) -> Result<(), RemoteBackendError> {
    let lifetime_ms = authorization
        .expires_at_unix_ms
        .checked_sub(authorization.issued_at_unix_ms);
    let unique_scopes = authorization.scopes.iter().copied().collect::<HashSet<_>>();
    if authorization.authorization_id == Uuid::nil()
        || authorization.silo_id != binding.silo_id
        || authorization.remote_environment_id != binding.remote_environment_id
        || !authorization.approved_by_user
        || authorization.scopes.is_empty()
        || authorization.scopes.len() > 2
        || unique_scopes.len() != authorization.scopes.len()
        || !lifetime_ms.is_some_and(|lifetime_ms| {
            (60_000..=agent::MAX_AUTOMATION_SECONDS * 1_000).contains(&lifetime_ms)
        })
    {
        return Err(RemoteBackendError::Protocol(
            "Automation authorization is unapproved, unbound, duplicated, or has an invalid lifetime."
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_automation_authorization(
    authorization: &agent::AutomationAuthorization,
    binding: &SiloBinding,
    now_ms: u64,
    expected_revoked: bool,
) -> Result<(), RemoteBackendError> {
    validate_automation_authorization_shape(authorization, binding)?;
    if authorization.revoked != expected_revoked
        || (!expected_revoked
            && (authorization.expires_at_unix_ms <= now_ms
                || authorization.issued_at_unix_ms.abs_diff(now_ms) > MAX_CLOCK_SKEW_MS))
    {
        return Err(RemoteBackendError::Protocol(
            "Automation authorization has an invalid revocation or freshness state.".to_owned(),
        ));
    }
    Ok(())
}

fn authorize_interactive_principal(
    binding: &SiloBinding,
    principal: &InteractivePrincipal,
    now_ms: u64,
    required_scope: agent::AutomationScope,
    enforce_human_priority: bool,
) -> Result<(agent::PrincipalKind, Uuid, u64), RemoteBackendError> {
    match principal {
        InteractivePrincipal::HumanSession { authorization_id } => {
            let authorization = binding
                .human_session
                .as_ref()
                .filter(|authorization| authorization.authorization_id == *authorization_id)
                .ok_or_else(|| RemoteBackendError::InteractiveRejected {
                    code: RejectionCode::Unauthorized,
                    message: "Human-session authorization is not stored for this Silo.".to_owned(),
                })?;
            validate_session_authorization_shape(authorization, binding)?;
            if authorization.revoked || authorization.expires_at_unix_ms <= now_ms {
                return Err(RemoteBackendError::InteractiveRejected {
                    code: RejectionCode::Unauthorized,
                    message: "Human-session authorization is revoked or expired.".to_owned(),
                });
            }
            Ok((
                agent::PrincipalKind::HumanSession,
                *authorization_id,
                authorization.expires_at_unix_ms,
            ))
        }
        InteractivePrincipal::Automation { authorization_id } => {
            if enforce_human_priority
                && binding.human_session.as_ref().is_some_and(|authorization| {
                    !authorization.revoked && authorization.expires_at_unix_ms > now_ms
                })
            {
                return Err(RemoteBackendError::InteractiveRejected {
                    code: RejectionCode::Unauthorized,
                    message: "Automation input is suspended while a human session is active."
                        .to_owned(),
                });
            }
            let authorization = binding
                .automation_authorizations
                .iter()
                .find(|authorization| authorization.authorization_id == *authorization_id)
                .ok_or_else(|| RemoteBackendError::InteractiveRejected {
                    code: RejectionCode::Unauthorized,
                    message: "Automation authorization is not stored for this Silo.".to_owned(),
                })?;
            validate_automation_authorization_shape(authorization, binding)?;
            if authorization.revoked
                || authorization.expires_at_unix_ms <= now_ms
                || !authorization.scopes.contains(&required_scope)
            {
                return Err(RemoteBackendError::InteractiveRejected {
                    code: RejectionCode::Unauthorized,
                    message:
                        "Automation authorization is revoked, expired, or lacks the required scope."
                            .to_owned(),
                });
            }
            Ok((
                agent::PrincipalKind::Automation,
                *authorization_id,
                authorization.expires_at_unix_ms,
            ))
        }
    }
}

fn validate_screen_channel_shape(
    channel: &agent::ScreenChannel,
    binding: &SiloBinding,
) -> Result<(), RemoteBackendError> {
    if channel.channel_id == Uuid::nil()
        || channel.authorization_id == Uuid::nil()
        || channel.remote_environment_id != binding.remote_environment_id
        || channel.expires_at_unix_ms == 0
    {
        return Err(RemoteBackendError::Protocol(
            "Screen channel is not bound to the selected environment and authorization.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_screen_channel(
    channel: &agent::ScreenChannel,
    binding: &SiloBinding,
    authorization_id: Uuid,
    authorization_expires_at_unix_ms: u64,
    now_ms: u64,
) -> Result<(), RemoteBackendError> {
    validate_screen_channel_shape(channel, binding)?;
    if channel.authorization_id != authorization_id
        || channel.expires_at_unix_ms <= now_ms
        || channel.expires_at_unix_ms > authorization_expires_at_unix_ms
        || channel.expires_at_unix_ms
            > now_ms.saturating_add(agent::MAX_HUMAN_SESSION_SECONDS * 1_000)
    {
        return Err(RemoteBackendError::Protocol(
            "Screen channel authorization or expiry does not match the request.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_input_events(events: &[agent::InputEvent]) -> Result<(), RemoteBackendError> {
    if events.is_empty() || events.len() > agent::MAX_INPUT_EVENTS {
        return Err(RemoteBackendError::LimitExceeded(
            "Input batch must contain 1 to 128 events.".to_owned(),
        ));
    }
    for event in events {
        match event {
            agent::InputEvent::Key { code, .. }
                if code.is_empty()
                    || code.len() > 40
                    || !code
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
            {
                return Err(RemoteBackendError::InvalidRequest(
                    "Key code is invalid.".to_owned(),
                ));
            }
            agent::InputEvent::PointerMove { x, y } if *x > 16_384 || *y > 16_384 => {
                return Err(RemoteBackendError::InvalidRequest(
                    "Pointer coordinate exceeds negotiated bounds.".to_owned(),
                ));
            }
            agent::InputEvent::Text { value }
                if value.trim() != value
                    || value.is_empty()
                    || value.len() > 512
                    || value.chars().any(|character| {
                        character.is_control() && character != '\n' && character != '\t'
                    }) =>
            {
                return Err(RemoteBackendError::InvalidRequest(
                    "Input text is empty, oversized, padded, or contains unsupported controls."
                        .to_owned(),
                ));
            }
            agent::InputEvent::Key { .. }
            | agent::InputEvent::PointerMove { .. }
            | agent::InputEvent::PointerButton { .. }
            | agent::InputEvent::Text { .. } => {}
        }
    }
    Ok(())
}

fn validate_interaction_receipt_shape(
    receipt: &AgentInteractionReceipt,
    binding: &SiloBinding,
) -> Result<(), RemoteBackendError> {
    if receipt.observed_at_unix_ms == 0 {
        return Err(RemoteBackendError::Protocol(
            "Stored interaction receipt has no observation time.".to_owned(),
        ));
    }
    match (receipt.operation, &receipt.response) {
        (
            AgentControlOperation::OpenHumanSession | AgentControlOperation::CloseHumanSession,
            agent::AgentResponse::HumanSession { authorization },
        ) => validate_session_authorization_shape(authorization, binding),
        (
            AgentControlOperation::GrantAutomation | AgentControlOperation::RevokeAutomation,
            agent::AgentResponse::Automation { authorization },
        ) => validate_automation_authorization_shape(authorization, binding),
        (AgentControlOperation::OpenScreen, agent::AgentResponse::Screen { channel }) => {
            validate_screen_channel_shape(channel, binding)
        }
        (AgentControlOperation::SendInput, agent::AgentResponse::InputAccepted { event_count })
            if (1..=agent::MAX_INPUT_EVENTS).contains(event_count) =>
        {
            Ok(())
        }
        _ => Err(RemoteBackendError::Protocol(
            "Stored interaction receipt operation and response do not match.".to_owned(),
        )),
    }
}

fn validate_deletion_proof(
    proof: &agent::DeletionProof,
    result: &OperationResult,
) -> Result<(), RemoteBackendError> {
    let key_id = proof
        .resource_deletions
        .iter()
        .find(|resource| resource.kind == agent::DeletionResourceKind::EphemeralKey)
        .and_then(|resource| resource.resource_id);
    if proof.proof_id == Uuid::nil()
        || proof.provider_receipt_id == Uuid::nil()
        || proof.volume_id == Uuid::nil()
        || proof.silo_id != result.silo_id
        || proof.binding_id != result.binding_id
        || proof.remote_environment_id != result.remote_environment_id
        || proof.deleted_at_unix_ms == 0
        || proof.deleted_at_unix_ms != result.last_activity_at_unix_ms
        || !key_id.is_some_and(|key_id| {
            agent::deletion_resources_are_bound(
                &proof.resource_deletions,
                proof.remote_environment_id,
                proof.volume_id,
                key_id,
            )
        })
    {
        return Err(RemoteBackendError::Protocol(
            "Deletion proof does not contain the stable bound resource dispositions.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_pairing_approval(
    approval: &PairingApproval,
    now_ms: u64,
) -> Result<(), RemoteBackendError> {
    if !approval.approved_by_user {
        return Err(RemoteBackendError::PairingApprovalRequired);
    }
    if !valid_secret_token(&approval.pairing_token, 32, 256) {
        return Err(RemoteBackendError::InvalidRequest(
            "Pairing token must be 32-256 base64url characters.".to_owned(),
        ));
    }
    if approval.pairing_token_id == Uuid::nil() {
        return Err(RemoteBackendError::InvalidRequest(
            "Pairing token ID must be non-nil.".to_owned(),
        ));
    }
    let lifetime = approval
        .pairing_token_expires_at_unix_ms
        .checked_sub(now_ms)
        .ok_or(RemoteBackendError::PairingTokenLifetime)?;
    if lifetime == 0 || lifetime > MAX_PAIRING_TOKEN_LIFETIME_MS {
        return Err(RemoteBackendError::PairingTokenLifetime);
    }
    Ok(())
}

fn same_https_origin(left: &RemoteEndpoint, right: &RemoteEndpoint) -> bool {
    let Ok(left) = Url::parse(&left.origin) else {
        return false;
    };
    let Ok(right) = Url::parse(&right.origin) else {
        return false;
    };
    left.origin().ascii_serialization() == right.origin().ascii_serialization()
}

fn valid_secret_token(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RemoteBackendError> {
    let payload = serde_json::to_vec(value)?;
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(RemoteBackendError::LimitExceeded(format!(
            "request is {} bytes; maximum is {MAX_MESSAGE_BYTES}",
            payload.len()
        )));
    }
    Ok(payload)
}

pub fn decode_strict_json<T: DeserializeOwned>(payload: &[u8]) -> Result<T, RemoteBackendError> {
    decode_json(payload)
}

fn decode_json<T: DeserializeOwned>(payload: &[u8]) -> Result<T, RemoteBackendError> {
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(RemoteBackendError::LimitExceeded(format!(
            "message is {} bytes; maximum is {MAX_MESSAGE_BYTES}",
            payload.len()
        )));
    }
    Ok(serde_json::from_slice(payload)?)
}

fn validate_fresh_timestamp(
    sent_at_unix_ms: u64,
    now_ms: u64,
    allowed_skew_ms: u64,
) -> Result<(), RemoteBackendError> {
    if sent_at_unix_ms.abs_diff(now_ms) > allowed_skew_ms {
        return Err(RemoteBackendError::StaleMessage);
    }
    Ok(())
}

fn validate_nonce(nonce: &str) -> Result<(), RemoteBackendError> {
    if !(32..=128).contains(&nonce.len())
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(RemoteBackendError::Protocol(
            "Nonce must be 32-128 base64url characters.".to_owned(),
        ));
    }
    Ok(())
}

fn new_nonce() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn validate_text(label: &str, value: &str) -> Result<(), RemoteBackendError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(RemoteBackendError::LimitExceeded(format!(
            "{label} must contain 1-{MAX_TEXT_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_string_list(
    label: &str,
    values: &[String],
    max_items: usize,
    max_item_bytes: usize,
) -> Result<(), RemoteBackendError> {
    if values.len() > max_items
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > max_item_bytes)
    {
        return Err(RemoteBackendError::LimitExceeded(format!(
            "{label} exceeds its item or byte limit"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::Value;

    use super::*;

    const NOW_MS: u64 = 1_785_196_800_000;

    #[derive(Debug, Clone, Copy)]
    struct FixedClock;

    impl Clock for FixedClock {
        fn now_unix_ms(&self) -> u64 {
            NOW_MS
        }
    }

    struct MockTransport {
        pin: TlsPin,
        tls_validated: bool,
        sequence: u64,
        repeated_nonce: bool,
        bad_proxy_evidence: bool,
        omit_deletion_proof: bool,
        wrong_deletion_volume: bool,
        authorization_lifetime_extension_seconds: u64,
        expand_automation_scopes: bool,
        screen_lifetime_seconds: u64,
        protocol_version: u16,
        client_credential: String,
        server_id: Uuid,
        next_pairing_server_id: Option<Uuid>,
        transport_error: bool,
        reject_pin_rotation_authorization: bool,
        unavailable: HashSet<RemoteOperation>,
        binding_id: Uuid,
        remote_environment_id: Uuid,
        current_policy: RemoteNetworkPolicy,
        requests: Vec<Value>,
    }

    impl MockTransport {
        fn new(pin: TlsPin) -> Self {
            Self {
                pin,
                tls_validated: true,
                sequence: 0,
                repeated_nonce: false,
                bad_proxy_evidence: false,
                omit_deletion_proof: false,
                wrong_deletion_volume: false,
                authorization_lifetime_extension_seconds: 0,
                expand_automation_scopes: false,
                screen_lifetime_seconds: 60,
                protocol_version: PROTOCOL_VERSION,
                client_credential: "credential_abcdefghijklmnopqrstuvwxyz0123456789".to_owned(),
                server_id: server_id(),
                next_pairing_server_id: None,
                transport_error: false,
                reject_pin_rotation_authorization: false,
                unavailable: HashSet::new(),
                binding_id: Uuid::parse_str("6b8a9da2-13e7-4f69-90cb-860f8d02e510").unwrap(),
                remote_environment_id: Uuid::parse_str("2d931510-d99f-494a-8c67-87feb05e1594")
                    .unwrap(),
                current_policy: RemoteNetworkPolicy::Direct,
                requests: Vec::new(),
            }
        }

        fn capabilities(&self) -> Vec<RemoteCapability> {
            RemoteOperation::ALL
                .into_iter()
                .map(|operation| RemoteCapability {
                    operation,
                    availability: if self.unavailable.contains(&operation) {
                        CapabilityAvailability::Unavailable {
                            reason: "Self-hosted provider does not implement this operation."
                                .to_owned(),
                        }
                    } else {
                        CapabilityAvailability::Available
                    },
                })
                .collect()
        }

        fn nonce(&self) -> String {
            if self.repeated_nonce {
                "repeated_response_nonce_0000000000".to_owned()
            } else {
                format!("response_nonce_{:032}", self.sequence)
            }
        }

        fn evidence(&self) -> GuestEvidence {
            let (proxy_state, policy_id) = match &self.current_policy {
                RemoteNetworkPolicy::Direct => (ProxyEvidenceState::NotRequired, None),
                RemoteNetworkPolicy::FixedProxy { policy_id, .. } => {
                    if self.bad_proxy_evidence {
                        (ProxyEvidenceState::Failed, Some(*policy_id))
                    } else {
                        (ProxyEvidenceState::Enforced, Some(*policy_id))
                    }
                }
            };
            GuestEvidence {
                protocol_version: PROTOCOL_VERSION,
                evidence_id: Uuid::new_v4(),
                binding_id: self.binding_id,
                remote_environment_id: self.remote_environment_id,
                source: GuestEvidenceSource::GuestAgent,
                sequence: self.sequence,
                observed_at_unix_ms: NOW_MS,
                proxy: ProxyEvidence {
                    state: proxy_state,
                    policy_id,
                },
                exit: ExitEvidence {
                    state: EvidenceCheckState::Verified,
                    public_addresses: vec!["203.0.113.7".to_owned()],
                },
                dns: DnsEvidence {
                    state: EvidenceCheckState::Verified,
                    resolvers: vec!["192.0.2.53".to_owned()],
                    leak_detected: false,
                },
                web_rtc: WebRtcEvidence {
                    state: EvidenceCheckState::Verified,
                    observed_candidates: vec!["relay 203.0.113.7".to_owned()],
                    leak_detected: self.bad_proxy_evidence,
                },
                health: GuestHealthEvidence {
                    state: GuestHealthState::Healthy,
                    agent_version: "0.9.0-prototype".to_owned(),
                    checks: vec!["guest agent owns browser process".to_owned()],
                },
            }
        }
    }

    impl RemoteTransport for MockTransport {
        fn exchange(
            &mut self,
            request: TransportRequest<'_>,
        ) -> Result<TransportResponse, RemoteBackendError> {
            assert_eq!(request.endpoint.origin, endpoint().origin);
            assert!(request.payload.len() <= MAX_MESSAGE_BYTES);
            if self.transport_error {
                return Err(RemoteBackendError::Transport(
                    "simulated transport failure".to_owned(),
                ));
            }
            let value: Value = serde_json::from_slice(request.payload)?;
            self.requests.push(value.clone());
            self.sequence += 1;
            let request_id = Uuid::parse_str(value["requestId"].as_str().unwrap()).unwrap();
            let operation = value["body"]["operation"].as_str();
            let response_pin = self.pin.clone();

            let payload = if operation == Some("pair") {
                assert!(request.credential.is_none());
                serde_json::to_vec(&PairingResponseEnvelope {
                    protocol_version: self.protocol_version,
                    response_id: Uuid::new_v4(),
                    in_reply_to: request_id,
                    nonce: self.nonce(),
                    sent_at_unix_ms: NOW_MS,
                    sequence: self.sequence,
                    body: PairingResponseBody::Success {
                        server_id: self.next_pairing_server_id.unwrap_or(self.server_id),
                        client_credential_id: client_credential_id(),
                        node: node_disclosure(),
                        client_credential: self.client_credential.clone(),
                        credential_expires_at_unix_ms: NOW_MS + 86_400_000,
                        capabilities: self.capabilities(),
                    },
                })?
            } else if operation == Some("authorize_tls_pin_rotation") {
                assert!(request.credential.is_some());
                assert!(value["body"].get("pairingToken").is_none());
                let request: TlsPinRotationAuthorizationRequestEnvelope =
                    serde_json::from_slice(request.payload)?;
                let body = if self.reject_pin_rotation_authorization {
                    TlsPinRotationAuthorizationResponseBody::Rejected {
                        code: RejectionCode::Unauthorized,
                        message: "The old credential was rejected.".to_owned(),
                    }
                } else {
                    let body = TlsPinRotationAuthorizationResponseBody::Success {
                        server_id: self.server_id,
                        client_credential_id: client_credential_id(),
                        pairing_token_id: request.body.pairing_token_id,
                        new_pin: request.body.new_pin.clone(),
                        challenge: format!("rotation_challenge_{:032}", self.sequence),
                        authorization_expires_at_unix_ms: NOW_MS
                            + MAX_TLS_PIN_ROTATION_AUTHORIZATION_LIFETIME_MS,
                    };
                    self.pin = request.body.new_pin;
                    body
                };
                serde_json::to_vec(&TlsPinRotationAuthorizationResponseEnvelope {
                    protocol_version: self.protocol_version,
                    response_id: Uuid::new_v4(),
                    in_reply_to: request_id,
                    nonce: self.nonce(),
                    sent_at_unix_ms: NOW_MS,
                    sequence: self.sequence,
                    body,
                })?
            } else if value.get("command").is_some() {
                assert!(request.credential.is_some());
                let request: agent::AgentRequest = serde_json::from_slice(request.payload)?;
                assert_eq!(request.principal.credential_id, client_credential_id());
                let response = match request.command {
                    agent::AgentCommand::OpenHumanSession {
                        silo_id,
                        lifetime_seconds,
                    } => agent::AgentResponse::HumanSession {
                        authorization: agent::SessionAuthorization {
                            authorization_id: Uuid::new_v4(),
                            silo_id,
                            remote_environment_id: self.remote_environment_id,
                            issued_at_unix_ms: NOW_MS,
                            expires_at_unix_ms: NOW_MS
                                + (lifetime_seconds
                                    + self.authorization_lifetime_extension_seconds)
                                    * 1_000,
                            revoked: false,
                        },
                    },
                    agent::AgentCommand::CloseHumanSession { silo_id } => {
                        agent::AgentResponse::HumanSession {
                            authorization: agent::SessionAuthorization {
                                authorization_id: request.principal.authorization_id.unwrap(),
                                silo_id,
                                remote_environment_id: self.remote_environment_id,
                                issued_at_unix_ms: NOW_MS,
                                expires_at_unix_ms: NOW_MS + 60_000,
                                revoked: true,
                            },
                        }
                    }
                    agent::AgentCommand::GrantAutomation {
                        silo_id,
                        lifetime_seconds,
                        mut scopes,
                        approved_by_user,
                    } => {
                        if self.expand_automation_scopes
                            && !scopes.contains(&agent::AutomationScope::SendInput)
                        {
                            scopes.push(agent::AutomationScope::SendInput);
                        }
                        agent::AgentResponse::Automation {
                            authorization: agent::AutomationAuthorization {
                                authorization_id: Uuid::new_v4(),
                                silo_id,
                                remote_environment_id: self.remote_environment_id,
                                issued_at_unix_ms: NOW_MS,
                                expires_at_unix_ms: NOW_MS
                                    + (lifetime_seconds
                                        + self.authorization_lifetime_extension_seconds)
                                        * 1_000,
                                scopes,
                                approved_by_user,
                                revoked: false,
                            },
                        }
                    }
                    agent::AgentCommand::RevokeAutomation {
                        silo_id,
                        authorization_id,
                    } => agent::AgentResponse::Automation {
                        authorization: agent::AutomationAuthorization {
                            authorization_id,
                            silo_id,
                            remote_environment_id: self.remote_environment_id,
                            issued_at_unix_ms: NOW_MS,
                            expires_at_unix_ms: NOW_MS + 60_000,
                            scopes: vec![
                                agent::AutomationScope::ReadScreen,
                                agent::AutomationScope::SendInput,
                            ],
                            approved_by_user: true,
                            revoked: true,
                        },
                    },
                    agent::AgentCommand::OpenScreen { .. } => agent::AgentResponse::Screen {
                        channel: agent::ScreenChannel {
                            channel_id: Uuid::new_v4(),
                            remote_environment_id: self.remote_environment_id,
                            authorization_id: request.principal.authorization_id.unwrap(),
                            expires_at_unix_ms: NOW_MS + self.screen_lifetime_seconds * 1_000,
                            transport: agent::ScreenTransport::AuthenticatedEncryptedStream,
                        },
                    },
                    agent::AgentCommand::SendInput { events, .. } => {
                        agent::AgentResponse::InputAccepted {
                            event_count: events.len(),
                        }
                    }
                    _ => panic!("lifecycle Agent command is not expected in this mock"),
                };
                serde_json::to_vec(&AgentResponseEnvelope {
                    protocol_version: self.protocol_version,
                    response_id: Uuid::new_v4(),
                    in_reply_to: request_id,
                    nonce: self.nonce(),
                    sent_at_unix_ms: NOW_MS,
                    sequence: self.sequence,
                    body: AgentControlResponseBody::Success { response },
                })?
            } else {
                assert!(request.credential.is_some());
                let request: OperationRequestEnvelope = serde_json::from_slice(request.payload)?;
                let requested_operation = request.body.operation();
                if let OperationBody::Create { network, .. }
                | OperationBody::ConfigureNetwork { network, .. } = &request.body
                {
                    self.current_policy = network.clone();
                }
                let state = match requested_operation {
                    RemoteOperation::Create => RemoteResultState::Created,
                    RemoteOperation::Start => RemoteResultState::Started,
                    RemoteOperation::Stop => RemoteResultState::Stopped,
                    RemoteOperation::Pause => RemoteResultState::Paused,
                    RemoteOperation::Snapshot => RemoteResultState::SnapshotCreated,
                    RemoteOperation::Destroy => RemoteResultState::Destroyed,
                    RemoteOperation::ConfigureNetwork => RemoteResultState::NetworkConfigured,
                    RemoteOperation::Health => RemoteResultState::Healthy,
                    RemoteOperation::Logs => RemoteResultState::LogsReturned,
                };
                let evidence = matches!(
                    requested_operation,
                    RemoteOperation::Create
                        | RemoteOperation::Start
                        | RemoteOperation::ConfigureNetwork
                        | RemoteOperation::Health
                )
                .then(|| self.evidence());
                let logs = (requested_operation == RemoteOperation::Logs).then(|| {
                    vec![RemoteLogEntry {
                        sequence: 1,
                        observed_at_unix_ms: NOW_MS,
                        level: RemoteLogLevel::Info,
                        message: "guest lifecycle event".to_owned(),
                    }]
                });
                let volume =
                    (requested_operation == RemoteOperation::Create).then(volume_attestation);
                let deletion_proof = (requested_operation == RemoteOperation::Destroy
                    && !self.omit_deletion_proof)
                    .then(|| {
                        let volume_id = if self.wrong_deletion_volume {
                            Uuid::new_v4()
                        } else {
                            volume_attestation().volume_id
                        };
                        agent::DeletionProof {
                            proof_id: Uuid::new_v4(),
                            silo_id: request.silo_id,
                            binding_id: self.binding_id,
                            remote_environment_id: self.remote_environment_id,
                            volume_id,
                            provider_receipt_id: Uuid::new_v4(),
                            resource_deletions: vec![
                                agent::ResourceDeletionItem {
                                    kind: agent::DeletionResourceKind::ComputeInstance,
                                    resource_id: Some(self.remote_environment_id),
                                    status: agent::DeletionResourceStatus::Deleted,
                                },
                                agent::ResourceDeletionItem {
                                    kind: agent::DeletionResourceKind::PersistentVolume,
                                    resource_id: Some(volume_id),
                                    status: agent::DeletionResourceStatus::Deleted,
                                },
                                agent::ResourceDeletionItem {
                                    kind: agent::DeletionResourceKind::Snapshot,
                                    resource_id: None,
                                    status: agent::DeletionResourceStatus::NotApplicable,
                                },
                                agent::ResourceDeletionItem {
                                    kind: agent::DeletionResourceKind::EphemeralKey,
                                    resource_id: Some(volume_attestation().key_id),
                                    status: agent::DeletionResourceStatus::Deleted,
                                },
                            ],
                            deleted_at_unix_ms: NOW_MS,
                            reason: agent::DeletionReason::UserConfirmed,
                        }
                    });
                serde_json::to_vec(&OperationResponseEnvelope {
                    protocol_version: self.protocol_version,
                    response_id: Uuid::new_v4(),
                    in_reply_to: request_id,
                    nonce: self.nonce(),
                    sent_at_unix_ms: NOW_MS,
                    sequence: self.sequence,
                    body: OperationResponseBody::Success {
                        result: OperationResult {
                            operation: requested_operation,
                            silo_id: request.silo_id,
                            binding_id: self.binding_id,
                            remote_environment_id: self.remote_environment_id,
                            server_id: self.server_id,
                            last_activity_at_unix_ms: NOW_MS,
                            state,
                            volume,
                            evidence,
                            logs,
                            next_cursor: None,
                            deletion_proof,
                        },
                    },
                })?
            };
            Ok(TransportResponse {
                tls_validated: self.tls_validated,
                peer_pin: response_pin,
                payload,
            })
        }
    }

    fn pin() -> TlsPin {
        TlsPin {
            kind: TlsPinKind::SpkiSha256,
            sha256: "a".repeat(64),
        }
    }

    fn endpoint() -> RemoteEndpoint {
        RemoteEndpoint {
            ownership: EndpointOwnership::UserSelfHosted,
            origin: "https://browser.example.test:8443".to_owned(),
            pin: pin(),
        }
    }

    fn rotated_endpoint() -> RemoteEndpoint {
        RemoteEndpoint {
            ownership: EndpointOwnership::UserSelfHosted,
            origin: endpoint().origin,
            pin: TlsPin {
                kind: TlsPinKind::SpkiSha256,
                sha256: "b".repeat(64),
            },
        }
    }

    fn silo_id() -> Uuid {
        Uuid::parse_str("0f8fad5b-d9cb-469f-a165-70867728950e").unwrap()
    }

    fn client_credential_id() -> Uuid {
        Uuid::parse_str("08d739b0-95bb-424e-8517-380b24337696").unwrap()
    }

    fn server_id() -> Uuid {
        Uuid::parse_str("d9428888-122b-11e1-b85c-61cd3cbb3210").unwrap()
    }

    fn policy_id() -> Uuid {
        Uuid::parse_str("73c16720-9a53-4e4f-a6c1-4c34bc02d638").unwrap()
    }

    fn node_disclosure() -> agent::NodeDisclosure {
        agent::NodeDisclosure {
            node_id: Uuid::parse_str("8944b7ee-bc5e-4a90-a850-02e030e17ac0").unwrap(),
            ownership: agent::NodeOwnership::UserSelfHosted,
            operator_label: "Example self-hosted operator".to_owned(),
            data_region: "user-selected-region".to_owned(),
            key_custody: agent::KeyCustody::UserControlled,
            cost: agent::CostDisclosure {
                currency: "USD".to_owned(),
                estimated_micros_per_hour: 250_000,
                notice: "Compute, storage and network usage are billed by the user's provider."
                    .to_owned(),
            },
        }
    }

    fn volume_attestation() -> agent::VolumeAttestation {
        agent::VolumeAttestation {
            encrypted: true,
            key_custody: agent::KeyCustody::UserControlled,
            volume_id: Uuid::parse_str("2aa5be56-dfca-4b0f-9379-2e6086b1b440").unwrap(),
            key_id: Uuid::parse_str("6c5a4aa4-496d-4ada-955c-c4277bb437f0").unwrap(),
        }
    }

    fn approval() -> PairingApproval {
        PairingApproval {
            approved_by_user: true,
            pairing_token_id: Uuid::parse_str("748b1e8d-05c6-49df-90e7-850dd30d1a1c").unwrap(),
            pairing_token: "pairing_token_abcdefghijklmnopqrstuvwxyz".to_owned(),
            pairing_token_expires_at_unix_ms: NOW_MS + MAX_PAIRING_TOKEN_LIFETIME_MS,
        }
    }

    fn rotation_approval() -> PairingApproval {
        PairingApproval {
            approved_by_user: true,
            pairing_token_id: Uuid::new_v4(),
            pairing_token: "rotation_token_abcdefghijklmnopqrstuvwxyz".to_owned(),
            pairing_token_expires_at_unix_ms: NOW_MS + MAX_PAIRING_TOKEN_LIFETIME_MS,
        }
    }

    fn backend(
        transport: MockTransport,
    ) -> RemoteEnvironmentBackend<MockTransport, MemoryBindingStore, FixedClock> {
        RemoteEnvironmentBackend::new(
            endpoint(),
            transport,
            MemoryBindingStore::default(),
            FixedClock,
        )
        .unwrap()
    }

    #[test]
    fn endpoint_requires_https_origin_and_nonzero_pin() {
        for origin in [
            "http://browser.example.test",
            "https://user:secret@browser.example.test",
            "https://browser.example.test/api",
        ] {
            let mut candidate = endpoint();
            candidate.origin = origin.to_owned();
            assert!(matches!(
                candidate.validate(),
                Err(RemoteBackendError::InvalidEndpoint(_))
            ));
        }
        let mut candidate = endpoint();
        candidate.pin.sha256 = "0".repeat(64);
        assert!(candidate.validate().is_err());
    }

    #[test]
    fn pairing_is_explicit_short_lived_and_capabilities_are_complete() {
        let mut backend = backend(MockTransport::new(pin()));
        let mut denied = approval();
        denied.approved_by_user = false;
        assert!(matches!(
            backend.pair(denied),
            Err(RemoteBackendError::PairingApprovalRequired)
        ));
        let mut too_long = approval();
        too_long.pairing_token_expires_at_unix_ms += 1;
        assert!(matches!(
            backend.pair(too_long),
            Err(RemoteBackendError::PairingTokenLifetime)
        ));
        backend.pair(approval()).unwrap();
        assert_eq!(backend.capabilities().unwrap().len(), 9);
        let snapshot = backend.pairing_snapshot().unwrap();
        assert_eq!(snapshot.node, node_disclosure());
        assert_eq!(snapshot.node.cost.estimated_micros_per_hour, 250_000);
    }

    #[test]
    fn pairing_rejects_a_non_base64url_client_credential() {
        let mut transport = MockTransport::new(pin());
        transport.client_credential = format!("{}:", "a".repeat(31));
        let mut backend = backend(transport);
        assert!(matches!(
            backend.pair(approval()),
            Err(RemoteBackendError::Protocol(_))
        ));
    }

    #[test]
    fn tls_pin_rotation_atomically_updates_every_binding_and_clears_authorizations() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let second_silo_id = Uuid::new_v4();
        backend
            .create(second_silo_id, RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let human = backend.open_human_session(silo_id(), 600).unwrap();
        let human_id = match human.response {
            agent::AgentResponse::HumanSession { authorization } => authorization.authorization_id,
            _ => panic!("expected human authorization"),
        };
        backend
            .grant_automation(
                silo_id(),
                300,
                vec![agent::AutomationScope::ReadScreen],
                true,
            )
            .unwrap();
        backend
            .open_screen(
                silo_id(),
                InteractivePrincipal::HumanSession {
                    authorization_id: human_id,
                },
            )
            .unwrap();

        let old_server_id = backend.pairing_snapshot().unwrap().server_id;
        backend.transport.client_credential =
            "rotated_credential_abcdefghijklmnopqrstuvwxyz0123456789".to_owned();
        let approval = rotation_approval();
        let token_id = approval.pairing_token_id;
        assert_eq!(
            backend
                .rotate_tls_pin(rotated_endpoint(), approval)
                .unwrap(),
            old_server_id
        );

        let snapshot = backend.export_snapshot();
        assert_eq!(backend.endpoint(), &rotated_endpoint());
        assert_eq!(snapshot.pairing.as_ref().unwrap().server_id, old_server_id);
        assert!(snapshot
            .pairing
            .as_ref()
            .unwrap()
            .client_credential
            .starts_with("rotated_credential_"));
        assert!(snapshot.used_pairing_token_ids.contains(&token_id));
        assert_eq!(snapshot.bindings.len(), 2);
        assert!(snapshot.bindings.iter().all(|binding| {
            binding.endpoint == rotated_endpoint()
                && binding.server_id == old_server_id
                && binding.human_session.is_none()
                && binding.automation_authorizations.is_empty()
                && binding.last_screen_channel.is_none()
        }));
        let authorization_request =
            &backend.transport.requests[backend.transport.requests.len() - 2];
        assert_eq!(
            authorization_request["body"]["operation"],
            "authorize_tls_pin_rotation"
        );
        assert!(authorization_request["body"].get("pairingToken").is_none());
        assert_eq!(
            backend.transport.requests.last().unwrap()["body"]["operation"],
            "pair"
        );
    }

    #[test]
    fn rejected_old_pin_authorization_never_contacts_the_new_endpoint() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        backend.transport.reject_pin_rotation_authorization = true;
        let requests_before = backend.transport.requests.len();
        let old_endpoint = backend.endpoint().clone();
        let old_bindings = backend.export_snapshot().bindings;
        let rotation = rotation_approval();
        let token_id = rotation.pairing_token_id;

        assert!(matches!(
            backend.rotate_tls_pin(rotated_endpoint(), rotation),
            Err(RemoteBackendError::Rejected {
                code: RejectionCode::Unauthorized,
                ..
            })
        ));
        assert_eq!(backend.transport.requests.len(), requests_before + 1);
        assert_eq!(
            backend.transport.requests[requests_before]["body"]["operation"],
            "authorize_tls_pin_rotation"
        );
        assert_eq!(backend.endpoint(), &old_endpoint);
        assert_eq!(backend.export_snapshot().bindings, old_bindings);
        assert!(backend.used_pairing_token_ids().contains(&token_id));
    }

    #[test]
    fn tls_pin_rotation_rejects_other_server_and_origin_without_changing_control_state() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let before_endpoint = backend.endpoint().clone();
        let before_pairing = backend.pairing_snapshot().unwrap();
        let before_bindings = backend.export_snapshot().bindings;

        let origin_approval = rotation_approval();
        let origin_token_id = origin_approval.pairing_token_id;
        let mut other_origin = rotated_endpoint();
        other_origin.origin = "https://other.example.test:8443".to_owned();
        assert!(matches!(
            backend.rotate_tls_pin(other_origin, origin_approval),
            Err(RemoteBackendError::RotationOriginMismatch)
        ));
        assert_eq!(backend.endpoint(), &before_endpoint);
        assert_eq!(backend.pairing_snapshot().unwrap(), before_pairing);
        assert_eq!(backend.export_snapshot().bindings, before_bindings);
        assert!(backend.used_pairing_token_ids().contains(&origin_token_id));

        backend.transport.next_pairing_server_id = Some(Uuid::new_v4());
        let server_approval = rotation_approval();
        let server_token_id = server_approval.pairing_token_id;
        let requests_before_server_mismatch = backend.transport.requests.len();
        assert!(matches!(
            backend.rotate_tls_pin(rotated_endpoint(), server_approval),
            Err(RemoteBackendError::RotationServerMismatch { .. })
        ));
        assert_eq!(backend.endpoint(), &before_endpoint);
        let after_failed_pairing = backend.pairing_snapshot().unwrap();
        assert_eq!(after_failed_pairing.server_id, before_pairing.server_id);
        assert_eq!(
            after_failed_pairing.client_credential_id,
            before_pairing.client_credential_id
        );
        assert_eq!(
            after_failed_pairing.client_credential,
            before_pairing.client_credential
        );
        assert_eq!(
            after_failed_pairing.credential_expires_at_unix_ms,
            before_pairing.credential_expires_at_unix_ms
        );
        assert_eq!(backend.export_snapshot().bindings, before_bindings);
        assert!(backend.used_pairing_token_ids().contains(&server_token_id));
        assert_eq!(
            backend.transport.requests.len(),
            requests_before_server_mismatch + 2
        );
        assert_eq!(
            backend.transport.requests[requests_before_server_mismatch]["body"]["operation"],
            "authorize_tls_pin_rotation"
        );
        assert_eq!(
            backend.transport.requests[requests_before_server_mismatch + 1]["body"]["operation"],
            "pair"
        );
    }

    #[test]
    fn failed_or_expired_rotation_keeps_old_state_and_burns_token_against_replay() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let before_endpoint = backend.endpoint().clone();
        let before_pairing = backend.pairing_snapshot().unwrap();
        let before_bindings = backend.export_snapshot().bindings;

        backend.transport.transport_error = true;
        let failed_approval = rotation_approval();
        let replay_approval = failed_approval.clone();
        let failed_token_id = failed_approval.pairing_token_id;
        assert!(matches!(
            backend.rotate_tls_pin(rotated_endpoint(), failed_approval),
            Err(RemoteBackendError::Transport(_))
        ));
        assert!(matches!(
            backend.rotate_tls_pin(rotated_endpoint(), replay_approval),
            Err(RemoteBackendError::PairingTokenReplay)
        ));
        assert_eq!(backend.endpoint(), &before_endpoint);
        let after_transport_failure = backend.pairing_snapshot().unwrap();
        assert_eq!(after_transport_failure.server_id, before_pairing.server_id);
        assert_eq!(
            after_transport_failure.client_credential_id,
            before_pairing.client_credential_id
        );
        assert_eq!(
            after_transport_failure.client_credential,
            before_pairing.client_credential
        );
        assert_eq!(
            after_transport_failure.credential_expires_at_unix_ms,
            before_pairing.credential_expires_at_unix_ms
        );
        assert_eq!(
            after_transport_failure.last_client_sequence,
            before_pairing.last_client_sequence + 1
        );
        assert_eq!(backend.export_snapshot().bindings, before_bindings);
        assert!(backend.used_pairing_token_ids().contains(&failed_token_id));

        let mut expired_snapshot = backend.export_snapshot();
        expired_snapshot
            .pairing
            .as_mut()
            .unwrap()
            .credential_expires_at_unix_ms = NOW_MS;
        let mut expired = RemoteEnvironmentBackend::from_snapshot(
            before_endpoint.clone(),
            MockTransport::new(rotated_endpoint().pin.clone()),
            FixedClock,
            expired_snapshot,
        )
        .unwrap();
        let expired_approval = rotation_approval();
        let expired_token_id = expired_approval.pairing_token_id;
        assert!(matches!(
            expired.rotate_tls_pin(rotated_endpoint(), expired_approval),
            Err(RemoteBackendError::CredentialExpired)
        ));
        assert_eq!(expired.endpoint(), &before_endpoint);
        assert_eq!(
            expired
                .pairing_snapshot()
                .unwrap()
                .credential_expires_at_unix_ms,
            NOW_MS
        );
        assert_eq!(expired.export_snapshot().bindings, before_bindings);
        assert!(expired.used_pairing_token_ids().contains(&expired_token_id));
    }

    #[test]
    fn force_detach_is_double_confirmed_local_only_and_never_a_deletion_proof() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let requests_before = backend.transport.requests.len();

        assert!(matches!(
            backend.force_detach_binding(silo_id(), true, false),
            Err(RemoteBackendError::ForceDetachConfirmationRequired)
        ));
        assert!(backend.binding(silo_id()).unwrap().is_some());

        let receipt = backend.force_detach_binding(silo_id(), true, true).unwrap();
        assert!(backend.binding(silo_id()).unwrap().is_none());
        assert_eq!(backend.transport.requests.len(), requests_before);
        assert_eq!(receipt.silo_id, silo_id());
        assert_eq!(receipt.server_id, server_id());
        assert_eq!(receipt.notice, REMOTE_ORPHAN_NOTICE);
        let serialized = serde_json::to_value(receipt).unwrap();
        assert!(serialized.get("deletionProof").is_none());
        assert!(serialized.get("providerReceiptId").is_none());
        assert!(serialized.get("volumeKeyDestroyed").is_none());
    }

    #[test]
    fn all_nine_typed_operations_execute_without_command_or_path_input() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        let silo_id = silo_id();
        backend
            .create(silo_id, RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        backend.start(silo_id).unwrap();
        backend.stop(silo_id).unwrap();
        backend.pause(silo_id).unwrap();
        backend.snapshot(silo_id).unwrap();
        backend
            .configure_network(
                silo_id,
                RemoteNetworkPolicy::FixedProxy {
                    required: true,
                    policy_id: policy_id(),
                },
            )
            .unwrap();
        backend.health(silo_id).unwrap();
        assert_eq!(
            backend.logs(silo_id, None, 20).unwrap().logs.unwrap().len(),
            1
        );
        backend.destroy(silo_id, true).unwrap();
        assert!(backend.binding(silo_id).unwrap().is_none());

        for request in &backend.transport.requests {
            let serialized = request.to_string();
            for forbidden in ["command", "shell", "args", "path"] {
                assert!(!serialized.contains(forbidden));
            }
        }
        let create = &backend.transport.requests[1]["body"];
        assert_eq!(create["ttlSeconds"], 600);
        assert_eq!(create["costAcknowledged"], true);
        let sequences = backend
            .transport
            .requests
            .iter()
            .skip(1)
            .map(|request| request["sequence"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(sequences, (1..=9).collect::<Vec<_>>());
    }

    #[test]
    fn client_sequence_survives_snapshot_restore_and_continues_monotonically() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let snapshot = backend.export_snapshot();
        assert_eq!(snapshot.pairing.as_ref().unwrap().last_client_sequence, 1);

        let mut transport = MockTransport::new(pin());
        transport.sequence = snapshot.pairing.as_ref().unwrap().last_server_sequence;
        let mut restored =
            RemoteEnvironmentBackend::from_snapshot(endpoint(), transport, FixedClock, snapshot)
                .unwrap();
        restored.start(silo_id()).unwrap();
        assert_eq!(restored.transport.requests[0]["sequence"], 2);
        assert_eq!(restored.pairing_snapshot().unwrap().last_client_sequence, 2);
    }

    #[test]
    fn destroy_requires_proof_for_the_original_encrypted_volume() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let bound_volume = backend
            .binding(silo_id())
            .unwrap()
            .unwrap()
            .volume
            .volume_id;

        backend.transport.omit_deletion_proof = true;
        assert!(matches!(
            backend.destroy(silo_id(), true),
            Err(RemoteBackendError::Protocol(_))
        ));
        assert!(backend.binding(silo_id()).unwrap().is_some());

        backend.transport.omit_deletion_proof = false;
        backend.transport.wrong_deletion_volume = true;
        assert!(matches!(
            backend.destroy(silo_id(), true),
            Err(RemoteBackendError::BindingMismatch)
        ));
        assert!(backend.binding(silo_id()).unwrap().is_some());

        backend.transport.wrong_deletion_volume = false;
        let destroyed = backend.destroy(silo_id(), true).unwrap();
        let proof = destroyed.deletion_proof.unwrap();
        assert_eq!(proof.volume_id, bound_volume);
        assert!(agent::deletion_resources_are_bound(
            &proof.resource_deletions,
            proof.remote_environment_id,
            proof.volume_id,
            volume_attestation().key_id,
        ));
        assert!(backend.binding(silo_id()).unwrap().is_none());
    }

    #[test]
    fn stable_silo_binding_rejects_a_second_create() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        assert!(matches!(
            backend.create(silo_id(), RemoteNetworkPolicy::Direct, 600, true),
            Err(RemoteBackendError::SiloAlreadyBound(id)) if id == silo_id()
        ));
    }

    #[test]
    fn human_and_explicit_automation_control_is_bound_persisted_and_human_priority_wins() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();

        let human = backend.open_human_session(silo_id(), 600).unwrap();
        let human_id = match human.response {
            agent::AgentResponse::HumanSession { authorization } => {
                assert!(!authorization.revoked);
                authorization.authorization_id
            }
            _ => panic!("expected human-session response"),
        };
        let automation = backend
            .grant_automation(
                silo_id(),
                300,
                vec![
                    agent::AutomationScope::ReadScreen,
                    agent::AutomationScope::SendInput,
                ],
                true,
            )
            .unwrap();
        let automation_id = match automation.response {
            agent::AgentResponse::Automation { authorization } => {
                assert!(authorization.approved_by_user);
                authorization.authorization_id
            }
            _ => panic!("expected automation response"),
        };

        let screen = backend
            .open_screen(
                silo_id(),
                InteractivePrincipal::HumanSession {
                    authorization_id: human_id,
                },
            )
            .unwrap();
        assert!(matches!(
            screen.response,
            agent::AgentResponse::Screen {
                channel: agent::ScreenChannel {
                    transport: agent::ScreenTransport::AuthenticatedEncryptedStream,
                    ..
                }
            }
        ));

        let request_count = backend.transport.requests.len();
        assert!(matches!(
            backend.send_input(
                silo_id(),
                InteractivePrincipal::Automation {
                    authorization_id: automation_id,
                },
                vec![agent::InputEvent::Text {
                    value: "blocked while human active".to_owned(),
                }],
            ),
            Err(RemoteBackendError::InteractiveRejected {
                code: RejectionCode::Unauthorized,
                ..
            })
        ));
        assert_eq!(backend.transport.requests.len(), request_count);

        backend.close_human_session(silo_id()).unwrap();
        let input = backend
            .send_input(
                silo_id(),
                InteractivePrincipal::Automation {
                    authorization_id: automation_id,
                },
                vec![agent::InputEvent::Text {
                    value: "accepted after human closes".to_owned(),
                }],
            )
            .unwrap();
        assert!(matches!(
            input.response,
            agent::AgentResponse::InputAccepted { event_count: 1 }
        ));

        let snapshot = backend.export_snapshot();
        let binding = &snapshot.bindings[0];
        assert!(binding.human_session.as_ref().unwrap().revoked);
        assert_eq!(binding.automation_authorizations.len(), 1);
        assert_eq!(
            binding
                .last_screen_channel
                .as_ref()
                .unwrap()
                .authorization_id,
            human_id
        );
        assert!(matches!(
            binding.last_interaction.as_ref().unwrap().response,
            agent::AgentResponse::InputAccepted { event_count: 1 }
        ));

        backend.revoke_automation(silo_id(), automation_id).unwrap();
        let request_count = backend.transport.requests.len();
        assert!(matches!(
            backend.open_screen(
                silo_id(),
                InteractivePrincipal::Automation {
                    authorization_id: automation_id,
                },
            ),
            Err(RemoteBackendError::InteractiveRejected {
                code: RejectionCode::Unauthorized,
                ..
            })
        ));
        assert_eq!(backend.transport.requests.len(), request_count);
    }

    #[test]
    fn agent_cannot_broaden_an_interactive_grant_or_outlive_its_authorization() {
        let mut backend = backend(MockTransport::new(pin()));
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();

        backend.transport.authorization_lifetime_extension_seconds = 1;
        assert!(matches!(
            backend.open_human_session(silo_id(), 600),
            Err(RemoteBackendError::Protocol(_))
        ));
        assert!(backend
            .binding(silo_id())
            .unwrap()
            .unwrap()
            .human_session
            .is_none());

        backend.transport.authorization_lifetime_extension_seconds = 0;
        let human = backend.open_human_session(silo_id(), 600).unwrap();
        let human_id = match human.response {
            agent::AgentResponse::HumanSession { authorization } => authorization.authorization_id,
            _ => panic!("expected human-session response"),
        };

        backend.transport.expand_automation_scopes = true;
        assert!(matches!(
            backend.grant_automation(
                silo_id(),
                300,
                vec![agent::AutomationScope::ReadScreen],
                true,
            ),
            Err(RemoteBackendError::Protocol(_))
        ));
        assert!(backend
            .binding(silo_id())
            .unwrap()
            .unwrap()
            .automation_authorizations
            .is_empty());

        backend.transport.screen_lifetime_seconds = 601;
        assert!(matches!(
            backend.open_screen(
                silo_id(),
                InteractivePrincipal::HumanSession {
                    authorization_id: human_id,
                },
            ),
            Err(RemoteBackendError::Protocol(_))
        ));
        assert!(backend
            .binding(silo_id())
            .unwrap()
            .unwrap()
            .last_screen_channel
            .is_none());
    }

    #[test]
    fn required_proxy_failure_is_persisted_and_fails_closed_before_start() {
        let mut transport = MockTransport::new(pin());
        transport.bad_proxy_evidence = true;
        let mut backend = backend(transport);
        backend.pair(approval()).unwrap();
        assert!(matches!(
            backend.create(
                silo_id(),
                RemoteNetworkPolicy::FixedProxy {
                    required: true,
                    policy_id: policy_id(),
                },
                600,
                true,
            ),
            Err(RemoteBackendError::RequiredProxyUnverified)
        ));
        assert!(backend.binding(silo_id()).unwrap().is_some());
        let request_count = backend.transport.requests.len();
        assert!(matches!(
            backend.start(silo_id()),
            Err(RemoteBackendError::RequiredProxyUnverified)
        ));
        assert_eq!(backend.transport.requests.len(), request_count);
    }

    #[test]
    fn negotiated_unavailable_is_not_a_successful_noop() {
        let mut transport = MockTransport::new(pin());
        transport.unavailable.insert(RemoteOperation::Pause);
        let mut backend = backend(transport);
        backend.pair(approval()).unwrap();
        backend
            .create(silo_id(), RemoteNetworkPolicy::Direct, 600, true)
            .unwrap();
        let request_count = backend.transport.requests.len();
        assert!(matches!(
            backend.pause(silo_id()),
            Err(RemoteBackendError::Unavailable {
                operation: RemoteOperation::Pause,
                ..
            })
        ));
        assert_eq!(backend.transport.requests.len(), request_count);
    }

    #[test]
    fn transport_requires_normal_tls_and_exact_pin() {
        let mut no_tls = MockTransport::new(pin());
        no_tls.tls_validated = false;
        assert!(matches!(
            backend(no_tls).pair(approval()),
            Err(RemoteBackendError::TlsNotValidated)
        ));

        let mut wrong_peer = MockTransport::new(pin());
        wrong_peer.pin.sha256 = "b".repeat(64);
        let mut backend = backend(wrong_peer);
        assert!(matches!(
            backend.pair(approval()),
            Err(RemoteBackendError::PinMismatch)
        ));
    }

    #[test]
    fn replay_and_protocol_version_are_rejected() {
        let mut transport = MockTransport::new(pin());
        transport.repeated_nonce = true;
        let mut replay_backend = backend(transport);
        replay_backend.pair(approval()).unwrap();
        assert!(matches!(
            replay_backend.create(silo_id(), RemoteNetworkPolicy::Direct, 600, true),
            Err(RemoteBackendError::ReplayDetected)
        ));

        let mut transport = MockTransport::new(pin());
        transport.protocol_version = 2;
        assert!(matches!(
            backend(transport).pair(approval()),
            Err(RemoteBackendError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: 2
            })
        ));
    }

    #[test]
    fn strict_decode_rejects_unknown_fields_and_oversize_messages() {
        let unknown = br#"{
          "protocolVersion":1,
          "requestId":"0d3f7ba5-f545-4aa4-836a-99f6af661775",
          "nonce":"nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
          "sequence":1,
          "sentAtUnixMs":1785196800000,
          "siloId":"0f8fad5b-d9cb-469f-a165-70867728950e",
          "body":{"operation":"start","bindingId":"6b8a9da2-13e7-4f69-90cb-860f8d02e510","remoteEnvironmentId":"2d931510-d99f-494a-8c67-87feb05e1594","command":"sh -c anything"}
        }"#;
        assert!(matches!(
            decode_strict_json::<OperationRequestEnvelope>(unknown),
            Err(RemoteBackendError::Json(_))
        ));
        assert!(matches!(
            decode_strict_json::<OperationRequestEnvelope>(&vec![b' '; MAX_MESSAGE_BYTES + 1]),
            Err(RemoteBackendError::LimitExceeded(_))
        ));
    }
}
