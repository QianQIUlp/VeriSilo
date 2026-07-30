use std::{
    env, fs,
    io::{self, Read, Write},
    net::IpAddr,
    path::Path,
};

#[cfg(any(target_os = "windows", test))]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    app_data_root, RuntimeActivation, RuntimeEvidenceState, RuntimeNetworkEvidence,
    RuntimeNetworkProvider, RuntimeState, VaultLockState, VaultStatus,
};

pub const PROTOCOL_VERSION: u32 = 2;
pub const RUNTIME_STATUS_SNAPSHOT_FILE: &str = "native-runtime-status.json";
const RUNTIME_STATUS_SCHEMA_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024;
const SNAPSHOT_MAX_AGE_SECONDS: i64 = 45;
const SNAPSHOT_CLOCK_SKEW_SECONDS: i64 = 5;
const DEVELOPMENT_ALLOWLIST_FILE: &str = "native-host-development-allowlist.json";
const EVIDENCE_INBOX_DIRECTORY: &str = "native-evidence-inbox";
const EVIDENCE_ENTRY_SCHEMA_VERSION: u32 = 1;
const MAX_EVIDENCE_ENTRY_BYTES: u64 = 16 * 1024;
const MAX_EVIDENCE_INBOX_ENTRIES: usize = 32;
const EVIDENCE_ENTRY_TTL_SECONDS: i64 = 10 * 60;
const NETWORK_CHECK_MAX_AGE_SECONDS: i64 = 5 * 60;
pub(crate) const NETWORK_REPUTATION_EXPLANATION: &str =
    "未查询商业信誉库或黑名单；运营商与机房线索不能代表 IP 一定干净或一定有风险。";
#[cfg(any(target_os = "windows", test))]
const HOST_EXECUTABLE_NAME: &str = "verisilo-native-host.exe";
#[cfg(any(target_os = "windows", test))]
const DESKTOP_EXECUTABLE_NAME: &str = "verisilo.exe";

#[derive(Debug, Error)]
pub enum NativeHostError {
    #[error("Native host caller origin is missing or unauthorized.")]
    UnauthorizedOrigin,
    #[error("Native host message is invalid.")]
    InvalidMessage,
    #[error("Native host message exceeds the maximum size.")]
    MessageTooLarge,
    #[error("Native host runtime snapshot is unavailable or invalid.")]
    InvalidSnapshot,
    #[error("VeriSilo desktop executable is unavailable.")]
    DesktopUnavailable,
    #[error("Native network evidence was rejected.")]
    EvidenceRejected,
    #[error("Native network evidence inbox is full.")]
    EvidenceInboxFull,
    #[error("Native host I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Native host serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum NativeRequest {
    Handshake {
        protocol_version: u32,
        request_id: Uuid,
    },
    GetRuntimeStatus {
        protocol_version: u32,
        request_id: Uuid,
    },
    OpenDesktop {
        protocol_version: u32,
        request_id: Uuid,
    },
    SubmitNetworkEvidence {
        protocol_version: u32,
        request_id: Uuid,
        silo_id: Uuid,
        runtime_id: Uuid,
        network_check: Box<NativeNetworkCheckResult>,
        coverage: Box<NativeNetworkEvidenceCoverage>,
    },
}

impl NativeRequest {
    fn protocol_version(&self) -> u32 {
        match self {
            Self::Handshake {
                protocol_version, ..
            }
            | Self::GetRuntimeStatus {
                protocol_version, ..
            }
            | Self::OpenDesktop {
                protocol_version, ..
            }
            | Self::SubmitNetworkEvidence {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    fn request_id(&self) -> Uuid {
        match self {
            Self::Handshake { request_id, .. }
            | Self::GetRuntimeStatus { request_id, .. }
            | Self::OpenDesktop { request_id, .. }
            | Self::SubmitNetworkEvidence { request_id, .. } => *request_id,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum NativeResponse {
    HandshakeAck {
        protocol_version: u32,
        request_id: Uuid,
        product: &'static str,
    },
    RuntimeStatus {
        protocol_version: u32,
        request_id: Uuid,
        snapshot_written_at: DateTime<Utc>,
        activation: SnapshotActivation,
        vault: SnapshotVault,
    },
    DesktopOpened {
        protocol_version: u32,
        request_id: Uuid,
    },
    EvidenceAccepted {
        protocol_version: u32,
        request_id: Uuid,
        evidence_id: Uuid,
        accepted_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    },
    Error {
        protocol_version: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<Uuid>,
        code: &'static str,
        message: &'static str,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DevelopmentAllowlist {
    allowed_extension_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeStatusSnapshot {
    schema_version: u32,
    protocol_version: u32,
    written_at: DateTime<Utc>,
    activation: SnapshotActivation,
    vault: SnapshotVault,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotActivation {
    active_silo_id: Option<Uuid>,
    state: SnapshotRuntimeState,
    updated_at: DateTime<Utc>,
    network_evidence: Option<SnapshotNetworkEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotRuntimeState {
    Idle,
    Preflight,
    Launching,
    Running,
    VerificationFailed,
    RecoveryRequired,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotNetworkEvidence {
    runtime_id: Uuid,
    evidence_id: Uuid,
    observed_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    provenance: SnapshotNetworkEvidenceProvenance,
    provider: SnapshotNetworkProvider,
    configuration: SnapshotEvidenceState,
    controller_binding: SnapshotEvidenceState,
    endpoint: SnapshotEvidenceState,
    authentication: SnapshotEvidenceState,
    authentication_provenance: SnapshotNetworkEvidenceProvenance,
    browser_routing: SnapshotEvidenceState,
    exit: SnapshotEvidenceState,
    dns: SnapshotEvidenceState,
    web_rtc: SnapshotEvidenceState,
    safeguards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotNetworkProvider {
    Direct,
    FixedProxy,
    ExternalMihomo,
    Pac,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotNetworkEvidenceProvenance {
    DesktopControlPlane,
    ExtensionAsserted,
    RelayObserved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotEvidenceState {
    NotApplicable,
    NotRequested,
    Configured,
    Reachable,
    Applied,
    Observed,
    Verified,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotVault {
    state: SnapshotVaultState,
    auto_lock_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotVaultState {
    Uninitialized,
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeNetworkEvidenceCoverage {
    pub trigger: String,
    pub transport: String,
    pub ip: String,
    pub public_dns: String,
    pub actual_dns_path: String,
    pub web_rtc: String,
    pub quic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeNetworkCheckResult {
    pub schema_version: u32,
    pub checked_at: DateTime<Utc>,
    pub ip: Option<NativeIpExitObservation>,
    pub dns: NativeDnsObservation,
    pub reputation: NativeReputationObservation,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeIpExitObservation {
    pub address: String,
    pub version: NativeIpVersion,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub asn: Option<String>,
    pub organization: Option<String>,
    pub isp: Option<String>,
    pub timezone: Option<String>,
    pub network_hint: NativeNetworkHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NativeIpVersion {
    #[serde(rename = "IPv4")]
    Ipv4,
    #[serde(rename = "IPv6")]
    Ipv6,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeNetworkHint {
    CloudOrHosting,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeDnsObservation {
    pub state: NativeDnsState,
    pub dnssec: NativeDnssecState,
    pub query_name: String,
    pub providers: Vec<NativeDnsProviderObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeDnsState {
    Consistent,
    Different,
    ResolverError,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeDnssecState {
    Validated,
    NotValidated,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeDnsProviderObservation {
    pub provider: NativeDnsProvider,
    pub status: u16,
    pub dnssec_authenticated: bool,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeDnsProvider {
    Cloudflare,
    Google,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeReputationObservation {
    pub state: NativeReputationState,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReputationState {
    NotScored,
}

/// A short-lived, user-initiated observation accepted from Companion. This is
/// an inbox transport record, never proof that an isolation capability was
/// verified.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeNetworkEvidenceInboxEntry {
    pub schema_version: u32,
    pub protocol_version: u32,
    pub evidence_id: Uuid,
    pub request_id: Uuid,
    pub silo_id: Uuid,
    #[serde(default)]
    pub runtime_id: Uuid,
    pub received_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub coverage: NativeNetworkEvidenceCoverage,
    pub result: NativeNetworkCheckResult,
}

/// Writes the only desktop state that Native Messaging may expose. This DTO
/// deliberately omits user-facing runtime messages, proxy endpoint labels,
/// Silo metadata, secrets, and all browser-owned state.
pub fn write_runtime_status_snapshot(
    root: &Path,
    activation: &RuntimeActivation,
    vault: &VaultStatus,
) -> Result<(), NativeHostError> {
    fs::create_dir_all(root)?;
    let snapshot = RuntimeStatusSnapshot {
        schema_version: RUNTIME_STATUS_SCHEMA_VERSION,
        protocol_version: PROTOCOL_VERSION,
        written_at: Utc::now(),
        activation: SnapshotActivation::from(activation),
        vault: SnapshotVault::from(vault),
    };
    validate_snapshot(&snapshot)?;
    let payload = serde_json::to_vec(&snapshot)?;
    if payload.len() as u64 > MAX_SNAPSHOT_BYTES
        || contains_sensitive_key(&serde_json::to_value(&snapshot)?)
    {
        return Err(NativeHostError::InvalidSnapshot);
    }

    let target = root.join(RUNTIME_STATUS_SNAPSHOT_FILE);
    let temporary = root.join(format!(
        ".{RUNTIME_STATUS_SNAPSHOT_FILE}.{}.tmp",
        Uuid::new_v4()
    ));
    fs::write(&temporary, payload)?;
    match fs::remove_file(&target) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(NativeHostError::Io(error));
        }
    }
    fs::rename(&temporary, &target)?;
    Ok(())
}

pub fn clear_runtime_status_snapshot(root: &Path) -> Result<(), NativeHostError> {
    match fs::remove_file(root.join(RUNTIME_STATUS_SNAPSHOT_FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(NativeHostError::Io(error)),
    }
}

/// Drains evidence only while the same Silo is still active and the Vault is
/// unlocked. Invalid, expired, malformed, and no-longer-authorized entries are
/// deleted instead of being returned to a persistence layer.
pub fn drain_network_evidence_inbox(
    root: &Path,
) -> Result<Vec<NativeNetworkEvidenceInboxEntry>, NativeHostError> {
    collect_network_evidence_inbox(root, true)
}

/// Reads authorized entries without deleting them. The desktop uses this
/// two-phase form so a Vault persistence failure cannot destroy the only copy
/// of an observation before it has reached encrypted history.
pub fn read_network_evidence_inbox(
    root: &Path,
) -> Result<Vec<NativeNetworkEvidenceInboxEntry>, NativeHostError> {
    collect_network_evidence_inbox(root, false)
}

pub fn acknowledge_network_evidence_inbox(
    root: &Path,
    entries: &[NativeNetworkEvidenceInboxEntry],
) -> Result<(), NativeHostError> {
    let inbox = root.join(EVIDENCE_INBOX_DIRECTORY);
    validate_evidence_inbox_directory(&inbox)?;
    for entry in entries {
        let path = inbox.join(format!("evidence-{}.json", entry.evidence_id));
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(NativeHostError::Io(error)),
        }
    }
    Ok(())
}

fn collect_network_evidence_inbox(
    root: &Path,
    remove_valid_entries: bool,
) -> Result<Vec<NativeNetworkEvidenceInboxEntry>, NativeHostError> {
    let inbox = root.join(EVIDENCE_INBOX_DIRECTORY);
    validate_evidence_inbox_directory(&inbox)?;
    let entries = match fs::read_dir(&inbox) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(NativeHostError::Io(error)),
    };
    let now = Utc::now();
    let snapshot = read_runtime_status_snapshot(root).ok();
    let mut drained = Vec::new();

    for directory_entry in entries.flatten() {
        let path = directory_entry.path();
        let file_name = match directory_entry.file_name().into_string() {
            Ok(file_name) => file_name,
            Err(_) => {
                remove_inbox_file(&path);
                continue;
            }
        };
        let file_type = match directory_entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                remove_inbox_file(&path);
                continue;
            }
        };
        if file_name.starts_with(".pending-") && file_name.ends_with(".tmp") {
            if pending_file_is_stale(&path, now) {
                remove_inbox_file(&path);
            }
            continue;
        }
        if !file_type.is_file() {
            if file_type.is_symlink() {
                remove_inbox_file(&path);
            }
            continue;
        }

        let Some(file_evidence_id) = evidence_id_from_file_name(&file_name) else {
            remove_inbox_file(&path);
            continue;
        };
        let Ok(entry) = read_evidence_entry(&path) else {
            remove_inbox_file(&path);
            continue;
        };
        let authorized = snapshot.as_ref().is_some_and(|snapshot| {
            snapshot_authorizes_runtime(snapshot, entry.silo_id, entry.runtime_id, now)
        });
        if entry.evidence_id != file_evidence_id
            || !authorized
            || validate_live_evidence_entry(&entry, now).is_err()
        {
            remove_inbox_file(&path);
            continue;
        }

        if !remove_valid_entries || fs::remove_file(&path).is_ok() {
            drained.push(entry);
        }
    }
    drained.sort_by_key(|entry| entry.received_at);
    Ok(drained)
}

fn validate_evidence_inbox_directory(inbox: &Path) -> Result<(), NativeHostError> {
    match fs::symlink_metadata(inbox) {
        Ok(metadata) if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() => {
            Err(NativeHostError::EvidenceRejected)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(NativeHostError::Io(error)),
    }
}

fn accept_network_evidence(
    request_id: Uuid,
    silo_id: Uuid,
    runtime_id: Uuid,
    coverage: NativeNetworkEvidenceCoverage,
    result: NativeNetworkCheckResult,
) -> Result<NativeNetworkEvidenceInboxEntry, NativeHostError> {
    let root = app_data_root().map_err(|_| NativeHostError::EvidenceRejected)?;
    accept_network_evidence_at(&root, request_id, silo_id, runtime_id, coverage, result)
}

fn accept_network_evidence_at(
    root: &Path,
    request_id: Uuid,
    silo_id: Uuid,
    runtime_id: Uuid,
    coverage: NativeNetworkEvidenceCoverage,
    result: NativeNetworkCheckResult,
) -> Result<NativeNetworkEvidenceInboxEntry, NativeHostError> {
    let snapshot =
        read_runtime_status_snapshot(root).map_err(|_| NativeHostError::EvidenceRejected)?;
    let now = Utc::now();
    if !snapshot_authorizes_runtime(&snapshot, silo_id, runtime_id, now) {
        return Err(NativeHostError::EvidenceRejected);
    }
    validate_coverage(&coverage)?;
    validate_network_check(&result, now)?;

    let inbox = root.join(EVIDENCE_INBOX_DIRECTORY);
    fs::create_dir_all(&inbox)?;
    validate_evidence_inbox_directory(&inbox)?;
    if prune_and_count_evidence_entries(&inbox, &snapshot, now)? >= MAX_EVIDENCE_INBOX_ENTRIES {
        return Err(NativeHostError::EvidenceInboxFull);
    }

    let evidence_id = Uuid::new_v4();
    let entry = NativeNetworkEvidenceInboxEntry {
        schema_version: EVIDENCE_ENTRY_SCHEMA_VERSION,
        protocol_version: PROTOCOL_VERSION,
        evidence_id,
        request_id,
        silo_id,
        runtime_id,
        received_at: now,
        expires_at: now + Duration::seconds(EVIDENCE_ENTRY_TTL_SECONDS),
        coverage,
        result,
    };
    validate_live_evidence_entry(&entry, now)?;
    let payload = serde_json::to_vec(&entry)?;
    if payload.len() as u64 > MAX_EVIDENCE_ENTRY_BYTES
        || contains_sensitive_key(&serde_json::to_value(&entry)?)
    {
        return Err(NativeHostError::EvidenceRejected);
    }

    let temporary = inbox.join(format!(".pending-{}.tmp", Uuid::new_v4()));
    let destination = inbox.join(format!("evidence-{evidence_id}.json"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    if let Err(error) = file.write_all(&payload).and_then(|()| file.sync_all()) {
        drop(file);
        remove_inbox_file(&temporary);
        return Err(NativeHostError::Io(error));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary, &destination) {
        remove_inbox_file(&temporary);
        return Err(NativeHostError::Io(error));
    }

    if prune_and_count_evidence_entries(&inbox, &snapshot, now)? > MAX_EVIDENCE_INBOX_ENTRIES {
        remove_inbox_file(&destination);
        return Err(NativeHostError::EvidenceInboxFull);
    }
    Ok(entry)
}

fn prune_and_count_evidence_entries(
    inbox: &Path,
    snapshot: &RuntimeStatusSnapshot,
    now: DateTime<Utc>,
) -> Result<usize, NativeHostError> {
    let mut valid_count = 0;
    for directory_entry in fs::read_dir(inbox)?.flatten() {
        let path = directory_entry.path();
        let file_name = match directory_entry.file_name().into_string() {
            Ok(file_name) => file_name,
            Err(_) => {
                remove_inbox_file(&path);
                continue;
            }
        };
        if file_name.starts_with(".pending-") && file_name.ends_with(".tmp") {
            if pending_file_is_stale(&path, now) {
                remove_inbox_file(&path);
            }
            continue;
        }
        let file_type = match directory_entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                remove_inbox_file(&path);
                continue;
            }
        };
        if !file_type.is_file() {
            if file_type.is_symlink() {
                remove_inbox_file(&path);
            }
            continue;
        }
        let Some(file_evidence_id) = evidence_id_from_file_name(&file_name) else {
            remove_inbox_file(&path);
            continue;
        };
        let Ok(entry) = read_evidence_entry(&path) else {
            remove_inbox_file(&path);
            continue;
        };
        if entry.evidence_id != file_evidence_id
            || !snapshot_authorizes_runtime(snapshot, entry.silo_id, entry.runtime_id, now)
            || validate_live_evidence_entry(&entry, now).is_err()
        {
            remove_inbox_file(&path);
            continue;
        }
        valid_count += 1;
    }
    Ok(valid_count)
}

fn read_evidence_entry(path: &Path) -> Result<NativeNetworkEvidenceInboxEntry, NativeHostError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_EVIDENCE_ENTRY_BYTES {
        return Err(NativeHostError::EvidenceRejected);
    }
    let payload = fs::read(path)?;
    let raw: Value = serde_json::from_slice(&payload)?;
    if contains_sensitive_key(&raw) {
        return Err(NativeHostError::EvidenceRejected);
    }
    serde_json::from_value(raw).map_err(NativeHostError::Serialization)
}

/// Validates the immutable entry schema and internal time/coverage/result
/// coherence without consulting a runtime snapshot or filesystem path. Vault
/// consumers must still enforce Silo existence, deduplication, and history
/// limits before persistence.
pub fn validate_network_evidence_inbox_entry(
    entry: &NativeNetworkEvidenceInboxEntry,
) -> Result<(), NativeHostError> {
    if entry.schema_version != EVIDENCE_ENTRY_SCHEMA_VERSION
        || !matches!(entry.protocol_version, 1 | PROTOCOL_VERSION)
        || entry.expires_at != entry.received_at + Duration::seconds(EVIDENCE_ENTRY_TTL_SECONDS)
    {
        return Err(NativeHostError::EvidenceRejected);
    }
    validate_coverage(&entry.coverage)?;
    validate_network_check(&entry.result, entry.received_at)
}

pub fn network_evidence_has_public_ip_observation(entry: &NativeNetworkEvidenceInboxEntry) -> bool {
    validate_network_evidence_inbox_entry(entry).is_ok() && entry.result.ip.is_some()
}

fn validate_live_evidence_entry(
    entry: &NativeNetworkEvidenceInboxEntry,
    now: DateTime<Utc>,
) -> Result<(), NativeHostError> {
    validate_network_evidence_inbox_entry(entry)?;
    if entry.protocol_version != PROTOCOL_VERSION
        || entry.runtime_id.is_nil()
        || entry.received_at > now + Duration::seconds(SNAPSHOT_CLOCK_SKEW_SECONDS)
        || entry.expires_at <= now
    {
        return Err(NativeHostError::EvidenceRejected);
    }
    Ok(())
}

fn validate_coverage(coverage: &NativeNetworkEvidenceCoverage) -> Result<(), NativeHostError> {
    if coverage.trigger != "user_initiated"
        || coverage.transport != "companion_extension_fetch"
        || coverage.ip != "third_party_https_observation"
        || coverage.public_dns != "public_doh_answer_comparison"
        || coverage.actual_dns_path != "not_observed"
        || coverage.web_rtc != "not_observed"
        || coverage.quic != "not_observed"
    {
        return Err(NativeHostError::EvidenceRejected);
    }
    Ok(())
}

fn validate_network_check(
    result: &NativeNetworkCheckResult,
    reference_time: DateTime<Utc>,
) -> Result<(), NativeHostError> {
    let age = reference_time.signed_duration_since(result.checked_at);
    if result.schema_version != 1
        || age > Duration::seconds(NETWORK_CHECK_MAX_AGE_SECONDS)
        || age < Duration::seconds(-SNAPSHOT_CLOCK_SKEW_SECONDS)
        || result.errors.len() > 10
        || result.errors.iter().any(|error| !valid_text(error, 300))
    {
        return Err(NativeHostError::EvidenceRejected);
    }

    if let Some(ip) = &result.ip {
        let parsed_address = ip
            .address
            .parse::<IpAddr>()
            .map_err(|_| NativeHostError::EvidenceRejected)?;
        let version_matches = matches!(
            (&ip.version, parsed_address),
            (NativeIpVersion::Ipv4, IpAddr::V4(_)) | (NativeIpVersion::Ipv6, IpAddr::V6(_))
        );
        if !valid_text(&ip.address, 64)
            || !version_matches
            || !is_public_observation_ip(parsed_address)
            || !valid_optional_text(&ip.country, 100)
            || !valid_optional_text(&ip.country_code, 8)
            || !valid_optional_text(&ip.region, 120)
            || !valid_optional_text(&ip.city, 120)
            || !valid_optional_text(&ip.organization, 160)
            || !valid_optional_text(&ip.isp, 160)
            || !valid_optional_text(&ip.timezone, 80)
            || ip.asn.as_ref().is_some_and(|asn| {
                asn.len() < 3
                    || asn.len() > 12
                    || !asn.starts_with("AS")
                    || !asn[2..].chars().all(|character| character.is_ascii_digit())
            })
        {
            return Err(NativeHostError::EvidenceRejected);
        }
    }

    if result.dns.query_name != "example.com" || result.dns.providers.len() > 2 {
        return Err(NativeHostError::EvidenceRejected);
    }
    let mut provider_names = result
        .dns
        .providers
        .iter()
        .map(|provider| provider.provider.clone())
        .collect::<Vec<_>>();
    provider_names.sort();
    provider_names.dedup();
    if provider_names.len() != result.dns.providers.len() {
        return Err(NativeHostError::EvidenceRejected);
    }
    for provider in &result.dns.providers {
        if provider.addresses.len() > 16 {
            return Err(NativeHostError::EvidenceRejected);
        }
        let mut normalized_addresses = provider.addresses.clone();
        normalized_addresses.sort();
        normalized_addresses.dedup();
        if normalized_addresses != provider.addresses
            || provider
                .addresses
                .iter()
                .any(|address| !matches!(address.parse::<IpAddr>(), Ok(IpAddr::V4(_))))
        {
            return Err(NativeHostError::EvidenceRejected);
        }
    }

    if result.dns.state != expected_dns_state(&result.dns.providers)
        || result.dns.dnssec != expected_dnssec_state(&result.dns.providers)
        || !matches!(result.reputation.state, NativeReputationState::NotScored)
        || result.reputation.explanation != NETWORK_REPUTATION_EXPLANATION
    {
        return Err(NativeHostError::EvidenceRejected);
    }
    Ok(())
}

fn is_public_observation_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [first, second, third, _] = address.octets();
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_documentation()
                || address.is_unspecified()
                || address.is_multicast()
                || first == 0
                || (first == 100 && (64..=127).contains(&second))
                || (first == 192 && second == 0 && third == 0)
                || (first == 198 && (18..=19).contains(&second))
                || first >= 240)
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            // Accept only ordinary global-unicast space and reject the
            // documentation prefix. This deliberately fails closed for
            // special-purpose and IPv4-mapped addresses.
            segments[0] & 0xe000 == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

fn expected_dns_state(providers: &[NativeDnsProviderObservation]) -> NativeDnsState {
    if providers.is_empty() {
        NativeDnsState::Failed
    } else if providers.len() == 1 {
        NativeDnsState::Partial
    } else if providers.iter().any(|provider| provider.status != 0) {
        NativeDnsState::ResolverError
    } else if !providers[0].addresses.is_empty() && providers[0].addresses == providers[1].addresses
    {
        NativeDnsState::Consistent
    } else {
        NativeDnsState::Different
    }
}

fn expected_dnssec_state(providers: &[NativeDnsProviderObservation]) -> NativeDnssecState {
    if providers.is_empty() {
        NativeDnssecState::Unavailable
    } else if providers.len() == 1 {
        NativeDnssecState::Partial
    } else if providers
        .iter()
        .all(|provider| provider.dnssec_authenticated)
    {
        NativeDnssecState::Validated
    } else {
        NativeDnssecState::NotValidated
    }
}

fn snapshot_authorizes_runtime(
    snapshot: &RuntimeStatusSnapshot,
    silo_id: Uuid,
    runtime_id: Uuid,
    now: DateTime<Utc>,
) -> bool {
    matches!(snapshot.vault.state, SnapshotVaultState::Unlocked)
        && snapshot
            .vault
            .auto_lock_at
            .is_some_and(|deadline| deadline > now)
        && matches!(snapshot.activation.state, SnapshotRuntimeState::Running)
        && snapshot.activation.active_silo_id == Some(silo_id)
        && snapshot
            .activation
            .network_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.runtime_id == runtime_id)
}

fn valid_optional_text(value: &Option<String>, maximum_length: usize) -> bool {
    value
        .as_ref()
        .is_none_or(|value| valid_text(value, maximum_length))
}

fn valid_text(value: &str, maximum_length: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum_length
        && !value.chars().any(char::is_control)
}

fn evidence_id_from_file_name(file_name: &str) -> Option<Uuid> {
    Uuid::parse_str(file_name.strip_prefix("evidence-")?.strip_suffix(".json")?).ok()
}

fn pending_file_is_stale(path: &Path, now: DateTime<Utc>) -> bool {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(DateTime::<Utc>::from)
        .map(|modified| now.signed_duration_since(modified) > Duration::minutes(1))
        .unwrap_or(true)
}

fn remove_inbox_file(path: &Path) {
    let _ = fs::remove_file(path);
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;

        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn run_native_host() -> Result<(), NativeHostError> {
    let origin = env::args()
        .nth(1)
        .ok_or(NativeHostError::UnauthorizedOrigin)?;
    if !is_allowed_origin(&origin) {
        return Err(NativeHostError::UnauthorizedOrigin);
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let payload = match read_frame(&mut reader) {
            Ok(Some(payload)) => payload,
            Ok(None) => return Ok(()),
            Err(error) => {
                let _ = write_frame(
                    &mut writer,
                    &NativeResponse::Error {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: None,
                        code: "invalid_message",
                        message: "Native host rejected an invalid message.",
                    },
                );
                return Err(error);
            }
        };

        let raw: Value = match serde_json::from_slice(&payload) {
            Ok(value) if !contains_sensitive_key(&value) => value,
            _ => {
                write_frame(
                    &mut writer,
                    &NativeResponse::Error {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: None,
                        code: "invalid_message",
                        message: "Sensitive browser or Vault state is not accepted by VeriSilo Native Messaging.",
                    },
                )?;
                continue;
            }
        };

        let request: NativeRequest = match serde_json::from_value::<NativeRequest>(raw) {
            Ok(request) => request,
            Err(_) => {
                write_frame(
                    &mut writer,
                    &NativeResponse::Error {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: None,
                        code: "invalid_message",
                        message: "Native host rejected an unknown or malformed message.",
                    },
                )?;
                continue;
            }
        };

        if request.protocol_version() != PROTOCOL_VERSION {
            write_frame(
                &mut writer,
                &NativeResponse::Error {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: Some(request.request_id()),
                    code: "unsupported_protocol",
                    message: "Native host and Companion protocol versions do not match.",
                },
            )?;
            continue;
        }

        let response = handle_request(request);
        write_frame(&mut writer, &response)?;
    }
}

fn handle_request(request: NativeRequest) -> NativeResponse {
    match request {
        NativeRequest::Handshake { request_id, .. } => NativeResponse::HandshakeAck {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            product: "VeriSilo",
        },
        NativeRequest::GetRuntimeStatus { request_id, .. } => {
            match load_runtime_status_snapshot() {
                Ok(snapshot) => NativeResponse::RuntimeStatus {
                    protocol_version: PROTOCOL_VERSION,
                    request_id,
                    snapshot_written_at: snapshot.written_at,
                    activation: snapshot.activation,
                    vault: snapshot.vault,
                },
                Err(_) => NativeResponse::Error {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: Some(request_id),
                    code: "unavailable",
                    message: "A fresh, non-sensitive desktop runtime snapshot is unavailable.",
                },
            }
        }
        NativeRequest::OpenDesktop { request_id, .. } => match open_desktop_application() {
            Ok(()) => NativeResponse::DesktopOpened {
                protocol_version: PROTOCOL_VERSION,
                request_id,
            },
            Err(_) => NativeResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                request_id: Some(request_id),
                code: "desktop_unavailable",
                message: "The installed VeriSilo desktop application could not be opened.",
            },
        },
        NativeRequest::SubmitNetworkEvidence {
            request_id,
            silo_id,
            runtime_id,
            network_check,
            coverage,
            ..
        } => {
            match accept_network_evidence(
                request_id,
                silo_id,
                runtime_id,
                *coverage,
                *network_check,
            ) {
                Ok(entry) => NativeResponse::EvidenceAccepted {
                    protocol_version: PROTOCOL_VERSION,
                    request_id,
                    evidence_id: entry.evidence_id,
                    accepted_at: entry.received_at,
                    expires_at: entry.expires_at,
                },
                Err(NativeHostError::EvidenceInboxFull) => NativeResponse::Error {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: Some(request_id),
                    code: "evidence_inbox_full",
                    message: "The temporary desktop evidence inbox is full.",
                },
                Err(_) => NativeResponse::Error {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: Some(request_id),
                    code: "evidence_rejected",
                    message: "Network evidence did not match the active unlocked Silo.",
                },
            }
        }
    }
}

fn load_runtime_status_snapshot() -> Result<RuntimeStatusSnapshot, NativeHostError> {
    let root = app_data_root().map_err(|_| NativeHostError::InvalidSnapshot)?;
    read_runtime_status_snapshot(&root)
}

fn read_runtime_status_snapshot(root: &Path) -> Result<RuntimeStatusSnapshot, NativeHostError> {
    let path = root.join(RUNTIME_STATUS_SNAPSHOT_FILE);
    let metadata = fs::metadata(&path).map_err(|_| NativeHostError::InvalidSnapshot)?;
    if !metadata.is_file() || metadata.len() > MAX_SNAPSHOT_BYTES {
        return Err(NativeHostError::InvalidSnapshot);
    }
    let payload = fs::read(path)?;
    let raw: Value = serde_json::from_slice(&payload)?;
    if contains_sensitive_key(&raw) {
        return Err(NativeHostError::InvalidSnapshot);
    }
    let snapshot: RuntimeStatusSnapshot = serde_json::from_value(raw)?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot(snapshot: &RuntimeStatusSnapshot) -> Result<(), NativeHostError> {
    if snapshot.schema_version != RUNTIME_STATUS_SCHEMA_VERSION
        || snapshot.protocol_version != PROTOCOL_VERSION
    {
        return Err(NativeHostError::InvalidSnapshot);
    }

    let now = Utc::now();
    let age = now.signed_duration_since(snapshot.written_at);
    if age > Duration::seconds(SNAPSHOT_MAX_AGE_SECONDS)
        || age < Duration::seconds(-SNAPSHOT_CLOCK_SKEW_SECONDS)
        || snapshot.activation.updated_at
            > snapshot.written_at + Duration::seconds(SNAPSHOT_CLOCK_SKEW_SECONDS)
    {
        return Err(NativeHostError::InvalidSnapshot);
    }

    if matches!(&snapshot.vault.state, SnapshotVaultState::Unlocked)
        && snapshot
            .vault
            .auto_lock_at
            .as_ref()
            .is_none_or(|auto_lock_at| auto_lock_at <= &now)
    {
        return Err(NativeHostError::InvalidSnapshot);
    }
    if !matches!(&snapshot.vault.state, SnapshotVaultState::Unlocked)
        && snapshot.vault.auto_lock_at.is_some()
    {
        return Err(NativeHostError::InvalidSnapshot);
    }

    if matches!(
        &snapshot.activation.state,
        SnapshotRuntimeState::Preflight
            | SnapshotRuntimeState::Launching
            | SnapshotRuntimeState::Running
    ) && snapshot.activation.active_silo_id.is_none()
    {
        return Err(NativeHostError::InvalidSnapshot);
    }

    if snapshot
        .activation
        .network_evidence
        .as_ref()
        .is_some_and(|evidence| {
            evidence.safeguards.len() > 12
                || evidence.safeguards.iter().any(|value| {
                    value.is_empty() || value.len() > 160 || value.chars().any(char::is_control)
                })
        })
    {
        return Err(NativeHostError::InvalidSnapshot);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_desktop_application() -> Result<(), NativeHostError> {
    let current_executable = env::current_exe().map_err(|_| NativeHostError::DesktopUnavailable)?;
    let executable = desktop_executable_from_host_path(&current_executable)?;
    Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| NativeHostError::DesktopUnavailable)
}

#[cfg(not(target_os = "windows"))]
fn open_desktop_application() -> Result<(), NativeHostError> {
    Err(NativeHostError::DesktopUnavailable)
}

#[cfg(any(target_os = "windows", test))]
fn desktop_executable_from_host_path(host_path: &Path) -> Result<PathBuf, NativeHostError> {
    if !host_path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(HOST_EXECUTABLE_NAME))
    {
        return Err(NativeHostError::DesktopUnavailable);
    }
    let canonical_host = host_path
        .canonicalize()
        .map_err(|_| NativeHostError::DesktopUnavailable)?;
    let canonical_directory = canonical_host
        .parent()
        .ok_or(NativeHostError::DesktopUnavailable)?;
    let desktop = canonical_directory.join(DESKTOP_EXECUTABLE_NAME);
    let canonical_desktop = desktop
        .canonicalize()
        .map_err(|_| NativeHostError::DesktopUnavailable)?;
    if !canonical_desktop.is_file() || canonical_desktop.parent() != Some(canonical_directory) {
        return Err(NativeHostError::DesktopUnavailable);
    }
    Ok(canonical_desktop)
}

fn is_allowed_origin(origin: &str) -> bool {
    let Some(extension_id) = extension_id_from_origin(origin) else {
        return false;
    };
    allowed_extension_ids()
        .iter()
        .any(|allowed_id| allowed_id == &extension_id)
}

fn extension_id_from_origin(origin: &str) -> Option<String> {
    let extension_id = origin
        .strip_prefix("chrome-extension://")?
        .strip_suffix('/')?;
    is_valid_extension_id(extension_id).then(|| extension_id.to_owned())
}

fn is_valid_extension_id(value: &str) -> bool {
    value.len() == 32
        && value
            .chars()
            .all(|character| matches!(character, 'a'..='p'))
}

fn allowed_extension_ids() -> Vec<String> {
    let mut ids = [
        option_env!("VERISILO_CHROME_EXTENSION_ID"),
        option_env!("VERISILO_EDGE_EXTENSION_ID"),
    ]
    .into_iter()
    .flatten()
    .filter(|id| is_valid_extension_id(id))
    .map(str::to_owned)
    .collect::<Vec<_>>();

    if cfg!(debug_assertions) {
        ids.extend(load_development_extension_ids());
    }
    ids.sort();
    ids.dedup();
    ids
}

fn load_development_extension_ids() -> Vec<String> {
    let mut ids = env::var("VERISILO_DEV_EXTENSION_IDS")
        .ok()
        .into_iter()
        .flat_map(|values| {
            values
                .split(',')
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|id| is_valid_extension_id(id))
        .collect::<Vec<_>>();

    if let Ok(root) = app_data_root() {
        if let Ok(raw) = fs::read(root.join(DEVELOPMENT_ALLOWLIST_FILE)) {
            if let Ok(allowlist) = serde_json::from_slice::<DevelopmentAllowlist>(&raw) {
                ids.extend(
                    allowlist
                        .allowed_extension_ids
                        .into_iter()
                        .filter(|id| is_valid_extension_id(id)),
                );
            }
        }
    }
    ids
}

fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, NativeHostError> {
    let mut length_bytes = [0_u8; 4];
    match reader.read_exact(&mut length_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(NativeHostError::Io(error)),
    }
    let length = u32::from_ne_bytes(length_bytes) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(NativeHostError::MessageTooLarge);
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_frame(writer: &mut impl Write, response: &NativeResponse) -> Result<(), NativeHostError> {
    let payload = serde_json::to_vec(response)?;
    let length = u32::try_from(payload.len()).map_err(|_| NativeHostError::MessageTooLarge)?;
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

fn contains_sensitive_key(value: &Value) -> bool {
    const FORBIDDEN_KEYWORDS: [&str; 15] = [
        "authorization",
        "browserdata",
        "cachestorage",
        "cookie",
        "credential",
        "indexeddb",
        "localstorage",
        "password",
        "passphrase",
        "profiledata",
        "secret",
        "seed",
        "sessionstorage",
        "token",
        "vaultdata",
    ];
    match value {
        Value::Array(values) => values.iter().any(contains_sensitive_key),
        Value::Object(values) => values.iter().any(|(key, nested)| {
            let normalized = key.to_ascii_lowercase();
            FORBIDDEN_KEYWORDS
                .iter()
                .any(|keyword| normalized.contains(keyword))
                || contains_sensitive_key(nested)
        }),
        _ => false,
    }
}

impl From<&RuntimeActivation> for SnapshotActivation {
    fn from(activation: &RuntimeActivation) -> Self {
        Self {
            active_silo_id: activation.active_silo_id,
            state: match &activation.state {
                RuntimeState::Idle => SnapshotRuntimeState::Idle,
                RuntimeState::Preflight => SnapshotRuntimeState::Preflight,
                RuntimeState::Launching => SnapshotRuntimeState::Launching,
                RuntimeState::Running => SnapshotRuntimeState::Running,
                RuntimeState::VerificationFailed => SnapshotRuntimeState::VerificationFailed,
                RuntimeState::RecoveryRequired => SnapshotRuntimeState::RecoveryRequired,
                RuntimeState::Stopped => SnapshotRuntimeState::Stopped,
                RuntimeState::Failed => SnapshotRuntimeState::Failed,
            },
            updated_at: activation.updated_at,
            network_evidence: activation
                .network_evidence
                .as_ref()
                .map(SnapshotNetworkEvidence::from),
        }
    }
}

impl From<&RuntimeNetworkEvidence> for SnapshotNetworkEvidence {
    fn from(evidence: &RuntimeNetworkEvidence) -> Self {
        Self {
            runtime_id: evidence.runtime_id,
            evidence_id: evidence.evidence_id,
            observed_at: evidence.observed_at,
            expires_at: evidence.expires_at,
            provenance: match &evidence.provenance {
                crate::domain::RuntimeNetworkEvidenceProvenance::DesktopControlPlane => {
                    SnapshotNetworkEvidenceProvenance::DesktopControlPlane
                }
                crate::domain::RuntimeNetworkEvidenceProvenance::ExtensionAsserted => {
                    SnapshotNetworkEvidenceProvenance::ExtensionAsserted
                }
                crate::domain::RuntimeNetworkEvidenceProvenance::RelayObserved => {
                    SnapshotNetworkEvidenceProvenance::RelayObserved
                }
            },
            provider: match &evidence.provider {
                RuntimeNetworkProvider::Direct => SnapshotNetworkProvider::Direct,
                RuntimeNetworkProvider::FixedProxy => SnapshotNetworkProvider::FixedProxy,
                RuntimeNetworkProvider::ExternalMihomo => SnapshotNetworkProvider::ExternalMihomo,
                RuntimeNetworkProvider::Pac => SnapshotNetworkProvider::Pac,
            },
            configuration: SnapshotEvidenceState::from(&evidence.configuration),
            controller_binding: SnapshotEvidenceState::from(&evidence.controller_binding),
            endpoint: SnapshotEvidenceState::from(&evidence.endpoint),
            authentication: SnapshotEvidenceState::from(&evidence.authentication),
            authentication_provenance: match &evidence.authentication_provenance {
                crate::domain::RuntimeNetworkEvidenceProvenance::DesktopControlPlane => {
                    SnapshotNetworkEvidenceProvenance::DesktopControlPlane
                }
                crate::domain::RuntimeNetworkEvidenceProvenance::ExtensionAsserted => {
                    SnapshotNetworkEvidenceProvenance::ExtensionAsserted
                }
                crate::domain::RuntimeNetworkEvidenceProvenance::RelayObserved => {
                    SnapshotNetworkEvidenceProvenance::RelayObserved
                }
            },
            browser_routing: SnapshotEvidenceState::from(&evidence.browser_routing),
            exit: SnapshotEvidenceState::from(&evidence.exit),
            dns: SnapshotEvidenceState::from(&evidence.dns),
            web_rtc: SnapshotEvidenceState::from(&evidence.web_rtc),
            safeguards: evidence.safeguards.clone(),
        }
    }
}

impl From<&RuntimeEvidenceState> for SnapshotEvidenceState {
    fn from(state: &RuntimeEvidenceState) -> Self {
        match state {
            RuntimeEvidenceState::NotApplicable => Self::NotApplicable,
            RuntimeEvidenceState::NotRequested => Self::NotRequested,
            RuntimeEvidenceState::Configured => Self::Configured,
            RuntimeEvidenceState::Reachable => Self::Reachable,
            RuntimeEvidenceState::Applied => Self::Applied,
            RuntimeEvidenceState::Observed => Self::Observed,
            RuntimeEvidenceState::Verified => Self::Verified,
            RuntimeEvidenceState::Failed => Self::Failed,
            RuntimeEvidenceState::Unavailable => Self::Unavailable,
        }
    }
}

impl From<&VaultStatus> for SnapshotVault {
    fn from(vault: &VaultStatus) -> Self {
        Self {
            state: match &vault.state {
                VaultLockState::Uninitialized => SnapshotVaultState::Uninitialized,
                VaultLockState::Locked => SnapshotVaultState::Locked,
                VaultLockState::Unlocked => SnapshotVaultState::Unlocked,
            },
            auto_lock_at: vault.auto_lock_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor};

    use chrono::{Duration, Utc};
    use serde_json::json;

    use crate::domain::{
        RuntimeActivation, RuntimeNetworkEvidence, RuntimeState, VaultLockState, VaultStatus,
    };

    use super::{
        accept_network_evidence_at, acknowledge_network_evidence_inbox, contains_sensitive_key,
        desktop_executable_from_host_path, drain_network_evidence_inbox, extension_id_from_origin,
        network_evidence_has_public_ip_observation, read_frame, read_network_evidence_inbox,
        read_runtime_status_snapshot, validate_live_evidence_entry,
        validate_network_evidence_inbox_entry, write_runtime_status_snapshot, NativeDnsObservation,
        NativeDnsState, NativeDnssecState, NativeHostError, NativeIpExitObservation,
        NativeIpVersion, NativeNetworkCheckResult, NativeNetworkEvidenceCoverage,
        NativeNetworkHint, NativeReputationObservation, NativeReputationState, NativeRequest,
        NativeResponse, EVIDENCE_INBOX_DIRECTORY, MAX_MESSAGE_BYTES,
        NETWORK_REPUTATION_EXPLANATION, PROTOCOL_VERSION, RUNTIME_STATUS_SNAPSHOT_FILE,
    };

    #[test]
    fn parses_only_valid_extension_origins() {
        assert_eq!(
            extension_id_from_origin("chrome-extension://abcdefghijklmnopabcdefghijklmnop/"),
            Some("abcdefghijklmnopabcdefghijklmnop".to_owned())
        );
        assert_eq!(extension_id_from_origin("https://example.test/"), None);
        assert_eq!(
            extension_id_from_origin("chrome-extension://ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP/"),
            None
        );
    }

    #[test]
    fn rejects_browser_and_vault_secret_keys() {
        for value in [
            json!({ "nested": { "cookieValue": "x" } }),
            json!({ "proxyCredential": "x" }),
            json!({ "vaultData": "x" }),
            json!({ "refreshToken": "x" }),
        ] {
            assert!(contains_sensitive_key(&value));
        }
    }

    #[test]
    fn rejects_unknown_request_fields() {
        let request = json!({
            "type": "handshake",
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
            "unexpected": true
        });
        assert!(serde_json::from_value::<NativeRequest>(request).is_err());
    }

    #[test]
    fn protocol_errors_omit_an_unknown_request_id_instead_of_serializing_null() {
        let response = NativeResponse::Error {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            code: "invalid_message",
            message: "rejected",
        };
        let serialized = serde_json::to_value(response).expect("serialize protocol error");
        assert!(serialized.get("requestId").is_none());
    }

    #[test]
    fn rejects_oversized_native_messages_before_allocation() {
        let frame = ((MAX_MESSAGE_BYTES as u32) + 1).to_ne_bytes().to_vec();
        let error = read_frame(&mut Cursor::new(frame)).expect_err("reject oversized frame");
        assert!(matches!(error, NativeHostError::MessageTooLarge));
    }

    #[test]
    fn snapshot_round_trip_omits_messages_and_endpoint_labels() {
        let root = test_root("snapshot-round-trip");
        let activation = RuntimeActivation {
            active_silo_id: None,
            state: RuntimeState::Idle,
            updated_at: Utc::now(),
            message: Some("must not cross the bridge".to_owned()),
            browser_verification: None,
            engine_evidence: None,
            network_evidence: Some(RuntimeNetworkEvidence {
                endpoint_label: Some("private-proxy.example:1080".to_owned()),
                ..RuntimeNetworkEvidence::configured(
                    &crate::domain::NetworkProfile::Direct {
                        proxy_required: false,
                    },
                    false,
                )
            }),
        };
        let vault = VaultStatus {
            state: VaultLockState::Locked,
            auto_lock_at: None,
        };
        write_runtime_status_snapshot(&root, &activation, &vault).expect("write snapshot");
        let parsed = read_runtime_status_snapshot(&root).expect("read snapshot");
        assert!(parsed.activation.network_evidence.is_some());
        let raw = fs::read_to_string(root.join(RUNTIME_STATUS_SNAPSHOT_FILE)).expect("snapshot");
        assert!(!raw.contains("must not cross"));
        assert!(!raw.contains("private-proxy"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_snapshot_fails_closed() {
        let root = test_root("stale-snapshot");
        fs::create_dir_all(&root).expect("create root");
        let snapshot = json!({
            "schemaVersion": 1,
            "protocolVersion": PROTOCOL_VERSION,
            "writtenAt": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
            "activation": {
                "activeSiloId": null,
                "state": "idle",
                "updatedAt": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
                "networkEvidence": null
            },
            "vault": { "state": "locked", "autoLockAt": null }
        });
        fs::write(
            root.join(RUNTIME_STATUS_SNAPSHOT_FILE),
            serde_json::to_vec(&snapshot).expect("serialize"),
        )
        .expect("write");
        assert!(matches!(
            read_runtime_status_snapshot(&root),
            Err(NativeHostError::InvalidSnapshot)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_wake_path_is_fixed_to_the_host_directory() {
        let root = test_root("desktop-path");
        fs::create_dir_all(&root).expect("create root");
        let host = root.join("verisilo-native-host.exe");
        let desktop = root.join("verisilo.exe");
        fs::write(&host, b"host").expect("host fixture");
        fs::write(&desktop, b"desktop").expect("desktop fixture");
        assert_eq!(
            desktop_executable_from_host_path(&host).expect("fixed desktop path"),
            desktop.canonicalize().expect("canonical desktop")
        );
        assert!(desktop_executable_from_host_path(&root.join("renamed-host.exe")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn accepts_and_drains_evidence_only_for_the_active_unlocked_silo() {
        let root = test_root("evidence-round-trip");
        let silo_id = uuid::Uuid::new_v4();
        let runtime_id = uuid::Uuid::new_v4();
        publish_running_snapshot(&root, silo_id, runtime_id, true);

        let wrong_silo = accept_network_evidence_at(
            &root,
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            runtime_id,
            test_coverage(),
            test_network_check(),
        );
        assert!(matches!(wrong_silo, Err(NativeHostError::EvidenceRejected)));
        let wrong_runtime = accept_network_evidence_at(
            &root,
            uuid::Uuid::new_v4(),
            silo_id,
            uuid::Uuid::new_v4(),
            test_coverage(),
            test_network_check(),
        );
        assert!(matches!(
            wrong_runtime,
            Err(NativeHostError::EvidenceRejected)
        ));

        let accepted = accept_network_evidence_at(
            &root,
            uuid::Uuid::new_v4(),
            silo_id,
            runtime_id,
            test_coverage(),
            test_network_check(),
        )
        .expect("accept matching evidence");
        validate_network_evidence_inbox_entry(&accepted).expect("validate accepted entry");
        let mut legacy_value = serde_json::to_value(&accepted).expect("serialize legacy fixture");
        let legacy_object = legacy_value.as_object_mut().expect("legacy entry object");
        legacy_object.remove("runtimeId");
        legacy_object.insert("protocolVersion".to_owned(), serde_json::json!(1));
        let legacy_entry: super::NativeNetworkEvidenceInboxEntry =
            serde_json::from_value(legacy_value).expect("read legacy Vault evidence");
        validate_network_evidence_inbox_entry(&legacy_entry)
            .expect("legacy evidence remains readable for Vault migration");
        assert!(validate_live_evidence_entry(&legacy_entry, Utc::now()).is_err());
        let mut public_ip = accepted.clone();
        public_ip.result.ip = Some(NativeIpExitObservation {
            address: "8.8.8.8".to_owned(),
            version: NativeIpVersion::Ipv4,
            country: None,
            country_code: None,
            region: None,
            city: None,
            asn: None,
            organization: None,
            isp: None,
            timezone: None,
            network_hint: NativeNetworkHint::Unknown,
        });
        assert!(network_evidence_has_public_ip_observation(&public_ip));
        public_ip
            .result
            .ip
            .as_mut()
            .expect("IP observation")
            .address = "127.0.0.1".to_owned();
        assert!(!network_evidence_has_public_ip_observation(&public_ip));
        let pending = read_network_evidence_inbox(&root).expect("read without deleting evidence");
        assert_eq!(pending.len(), 1);
        assert_eq!(
            read_network_evidence_inbox(&root)
                .expect("read evidence again")
                .len(),
            1
        );
        acknowledge_network_evidence_inbox(&root, &pending)
            .expect("acknowledge persisted evidence");
        assert!(read_network_evidence_inbox(&root)
            .expect("read acknowledged inbox")
            .is_empty());

        let accepted = accept_network_evidence_at(
            &root,
            uuid::Uuid::new_v4(),
            silo_id,
            runtime_id,
            test_coverage(),
            test_network_check(),
        )
        .expect("accept drain fixture");
        let drained = drain_network_evidence_inbox(&root).expect("drain evidence");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].evidence_id, accepted.evidence_id);
        assert_eq!(drained[0].silo_id, silo_id);
        assert!(drain_network_evidence_inbox(&root)
            .expect("second drain")
            .is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_locked_vault_and_deletes_malformed_inbox_files() {
        let root = test_root("evidence-rejection");
        let silo_id = uuid::Uuid::new_v4();
        let runtime_id = uuid::Uuid::new_v4();
        publish_running_snapshot(&root, silo_id, runtime_id, false);
        let rejected = accept_network_evidence_at(
            &root,
            uuid::Uuid::new_v4(),
            silo_id,
            runtime_id,
            test_coverage(),
            test_network_check(),
        );
        assert!(matches!(rejected, Err(NativeHostError::EvidenceRejected)));

        let inbox = root.join(EVIDENCE_INBOX_DIRECTORY);
        fs::create_dir_all(&inbox).expect("create inbox");
        let malformed = inbox.join("not-host-generated.json");
        fs::write(&malformed, b"{}").expect("write malformed entry");
        assert!(drain_network_evidence_inbox(&root)
            .expect("drain malformed")
            .is_empty());
        assert!(!malformed.exists());
        let _ = fs::remove_dir_all(root);
    }

    fn publish_running_snapshot(
        root: &std::path::Path,
        silo_id: uuid::Uuid,
        runtime_id: uuid::Uuid,
        unlocked: bool,
    ) {
        let mut network_evidence = RuntimeNetworkEvidence::configured(
            &crate::domain::NetworkProfile::Direct {
                proxy_required: false,
            },
            false,
        );
        network_evidence.runtime_id = runtime_id;
        let activation = RuntimeActivation {
            active_silo_id: Some(silo_id),
            state: RuntimeState::Running,
            updated_at: Utc::now(),
            message: None,
            browser_verification: None,
            engine_evidence: None,
            network_evidence: Some(network_evidence),
        };
        let vault = VaultStatus {
            state: if unlocked {
                VaultLockState::Unlocked
            } else {
                VaultLockState::Locked
            },
            auto_lock_at: unlocked.then(|| Utc::now() + Duration::minutes(15)),
        };
        write_runtime_status_snapshot(root, &activation, &vault).expect("publish snapshot");
    }

    fn test_coverage() -> NativeNetworkEvidenceCoverage {
        NativeNetworkEvidenceCoverage {
            trigger: "user_initiated".to_owned(),
            transport: "companion_extension_fetch".to_owned(),
            ip: "third_party_https_observation".to_owned(),
            public_dns: "public_doh_answer_comparison".to_owned(),
            actual_dns_path: "not_observed".to_owned(),
            web_rtc: "not_observed".to_owned(),
            quic: "not_observed".to_owned(),
        }
    }

    fn test_network_check() -> NativeNetworkCheckResult {
        NativeNetworkCheckResult {
            schema_version: 1,
            checked_at: Utc::now(),
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
        }
    }

    fn test_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("verisilo-{label}-{}", uuid::Uuid::new_v4()))
    }
}
