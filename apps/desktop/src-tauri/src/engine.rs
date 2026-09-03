use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration as StdDuration, Instant},
};

use base64::{
    engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::{DateTime, Duration, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{BrowserDescriptor, BrowserKind, NetworkProfile, ProxyScheme};

pub const ENGINE_CONTRACT_VERSION: u32 = 1;
pub const ENGINE_BOOTSTRAP_VERSION: u32 = 1;
pub const ENGINE_BOOTSTRAP_ACK_VERSION: u32 = 1;
pub const ENGINE_RUNTIME_RECEIPT_VERSION: u32 = 1;
pub const MAX_ENGINE_BOOTSTRAP_BYTES: usize = 256 * 1024;
pub const MAX_ENGINE_BOOTSTRAP_ACK_BYTES: usize = 16 * 1024;
pub const MAX_ENGINE_RUNTIME_RECEIPT_BYTES: usize = 32 * 1024;
pub const CAMOUFOX_HOST_PACKAGE_SCHEMA_VERSION: u32 = 3;
pub const CAMOUFOX_ARTIFACT_SCHEMA_V3: &str = "verisilo-camoufox-resolved-identity/v3";
pub const CAMOUFOX_ARTIFACT_SCHEMA_V5: &str = "verisilo-camoufox-resolved-identity/v5";
pub const CAMOUFOX_ARTIFACT_SCHEMA_V6: &str = "verisilo-camoufox-resolved-identity/v6";
pub const CAMOUFOX_ARTIFACT_SCHEMA: &str = CAMOUFOX_ARTIFACT_SCHEMA_V3;
pub const CAMOUFOX_HOST_PROTOCOL: &str = "verisilo-camoufox-host/v1";
pub const CAMOUFOX_HOST_ENTRYPOINT_KIND: &str = "camoufox-host-v1";
pub const CAMOUFOX_BROWSER_TREE_SCHEMA: &str = "verisilo-camoufox-browser-tree-manifest/v1";
pub const CAMOUFOX_HOST_VERSION: &str = "0.1.0";
pub const MAX_CAMOUFOX_HOST_FRAME_BYTES: usize = 32 * 1024;
pub const CAMOUFOX_PROVISION_TIMEOUT: StdDuration = StdDuration::from_secs(120);
pub const CAMOUFOX_FORMAL_V3_ENGINE_VERSION: &str = "152.0.4-beta.28";
pub const CAMOUFOX_FORMAL_V3_BROWSER_RELEASE: &str = "v152.0.4-beta.28";
pub const CAMOUFOX_FORMAL_V3_ENGINE_REVISION: &str =
    "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3";
pub const CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256: &str =
    "8a3ef192e02cfb955bd3f9bcf71b009bd89f78e758e522b7cf373c6a0d988cbb";
pub const CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SIZE_BYTES: u64 = 493_496_137;
pub const CAMOUFOX_FORMAL_V3_BROWSER_EXECUTABLE_SHA256: &str =
    "c5535c7ca64c1ed5096238d4267f4445203fd8d57b6da7760f6717dc9804b49e";
pub const CAMOUFOX_PACKAGE_ASSET_LOCK_SCHEMA: &str = "verisilo-camoufox-package-asset/v1";
pub const CAMOUFOX_PACKAGE_ASSET_KIND: &str = "self-built";
pub const CAMOUFOX_PACKAGE_ASSET_PLATFORM: &str = "windows-x86_64";
pub const CAMOUFOX_PACKAGE_ASSET_EVIDENCE_CLASS: &str = "compiled-not-runtime-verified";
pub const CAMOUFOX_PACKAGE_PYTHON_PACKAGE: &str = "camoufox==0.5.4";
const ENGINE_PACKAGE_MANIFEST: &str = "engine-package.json";
const ENGINE_STATE_SCHEMA_VERSION: u32 = 1;
const MAX_ENGINE_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_CAMOUFOX_BROWSER_TREE_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CAMOUFOX_PACKAGE_TREE_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENGINE_EXECUTABLE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_ENGINE_SIGNATURE_BYTES: usize = 48 * 1024;
const MAX_SESSION_TOKEN_LIFETIME_MINUTES: i64 = 60;
pub const DEFAULT_SESSION_TOKEN_LIFETIME_MINUTES: i64 = 30;
const WINDOWS_X64_PLATFORM: &str = "windows-x64";
const CMS_SHA256_ALGORITHM: &str = "cms-detached-sha256";
const SESSION_TOKEN_DOMAIN: &[u8] = b"VeriSilo engine session token v1\0";
const SESSION_TOKEN_ID_DOMAIN: &[u8] = b"VeriSilo engine session token id v1\0";
const ENGINE_BOOTSTRAP_FRAME_HEADER_BYTES: usize = 4;
const ENGINE_RUNTIME_RECEIPT_LIFETIME_SECONDS: i64 = 30;
const ENGINE_RUNTIME_RECEIPT_CLOCK_SKEW_SECONDS: i64 = 5;
const MAX_RETAINED_ENGINE_FALLBACK_RECEIPTS: usize = 128;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Engine capability is unavailable: {0}")]
    CapabilityUnavailable(String),
    #[error("Engine identity template is inconsistent: {0}")]
    InvalidIdentityTemplate(String),
    #[error("Engine package is invalid: {0}")]
    InvalidPackage(String),
    #[error("Engine package verification is unavailable or failed: {0}")]
    VerificationUnavailable(String),
    #[error("Engine adapter is emergency-disabled: {0}")]
    EmergencyDisabled(String),
    #[error("Engine state transition is invalid: {0}")]
    InvalidTransition(String),
    #[error("Engine path is unsafe: {0}")]
    UnsafePath(String),
    #[error("Engine I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Engine manifest serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Engine bootstrap envelope is invalid: {0}")]
    InvalidBootstrap(String),
    #[error("Engine runtime receipt is invalid: {0}")]
    InvalidRuntimeReceipt(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum EngineAdapterId {
    StockChrome,
    StockEdge,
    ControlledChromium,
    Camoufox,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EngineTransport {
    Stock,
    NativeBootstrapV1,
    CamoufoxHostJsonlV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CamoufoxArtifactBindingV1 {
    pub artifact_id: String,
    pub artifact_file_sha256: String,
    pub schema: String,
}

impl CamoufoxArtifactBindingV1 {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !valid_artifact_id(&self.artifact_id)
            || !is_lower_hex(&self.artifact_file_sha256, 64)
            || !matches!(
                self.schema.as_str(),
                CAMOUFOX_ARTIFACT_SCHEMA_V3
                    | CAMOUFOX_ARTIFACT_SCHEMA_V5
                    | CAMOUFOX_ARTIFACT_SCHEMA_V6
            )
        {
            return Err(EngineError::InvalidIdentityTemplate(
                "identity_artifact_unavailable: Camoufox Artifact binding is not a strict v3/v6 ID/raw-SHA binding"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CamoufoxHostRoots {
    pub artifact_root: PathBuf,
    pub profile_root: PathBuf,
    pub state_root: PathBuf,
}

impl CamoufoxHostRoots {
    fn validate(&self) -> Result<(), EngineError> {
        for (label, root) in [
            ("artifact", &self.artifact_root),
            ("profile", &self.profile_root),
            ("state", &self.state_root),
        ] {
            validate_host_root(root, label)?;
        }
        Ok(())
    }
}

/// Per-Silo adapter selection. The tagged variants deliberately expose no
/// executable path, command-line argument, bootstrap URL, or arbitrary
/// adapter string. Existing Silo records deserialize to `Stock` through the
/// default on `Silo::engine`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "adapter",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SiloEngineConfig {
    Stock,
    ControlledChromium {
        identity_template: IdentityTemplate,
        fallback_rules: Vec<SiteFallbackRule>,
    },
    Camoufox {
        /// Legacy schema-8 records may deserialize these fields, but current
        /// managed records leave them absent. Camoufox runtime authority is
        /// the signed Artifact binding below, never a template or fallback.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity_template: Option<IdentityTemplate>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fallback_rules: Vec<SiteFallbackRule>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact_binding: Option<CamoufoxArtifactBindingV1>,
    },
}

impl Default for SiloEngineConfig {
    fn default() -> Self {
        Self::Stock
    }
}

impl SiloEngineConfig {
    pub fn adapter_id(&self, stock_kind: &BrowserKind) -> EngineAdapterId {
        match self {
            Self::Stock => match stock_kind {
                BrowserKind::Chrome => EngineAdapterId::StockChrome,
                BrowserKind::Edge => EngineAdapterId::StockEdge,
            },
            Self::ControlledChromium { .. } => EngineAdapterId::ControlledChromium,
            Self::Camoufox { .. } => EngineAdapterId::Camoufox,
        }
    }

    pub fn is_stock(&self) -> bool {
        matches!(self, Self::Stock)
    }

    pub fn identity_template(&self) -> Option<&IdentityTemplate> {
        match self {
            Self::Stock => None,
            Self::ControlledChromium {
                identity_template, ..
            } => Some(identity_template),
            Self::Camoufox { .. } => None,
        }
    }

    pub fn fallback_rules(&self) -> &[SiteFallbackRule] {
        match self {
            Self::Stock => &[],
            Self::ControlledChromium { fallback_rules, .. } => fallback_rules,
            Self::Camoufox { .. } => &[],
        }
    }

    pub fn validate(&self, proxy_required: bool) -> Result<(), EngineError> {
        match self {
            Self::Stock => Ok(()),
            Self::ControlledChromium {
                identity_template,
                fallback_rules,
            } => {
                validate_identity_template(identity_template, BrowserFamily::Chromium, None)?;
                validate_fallback_rules(fallback_rules)?;
                if identity_template.network.proxy_required != proxy_required {
                    return Err(EngineError::InvalidIdentityTemplate(
                        "identity template network.proxyRequired must match the Silo network policy"
                            .to_owned(),
                    ));
                }
                Ok(())
            }
            Self::Camoufox {
                identity_template,
                fallback_rules,
                artifact_binding,
            } => {
                // These fields are accepted only for migration decoding. A
                // current record must be binding-only; legacy records are
                // intentionally unavailable until a native Artifact is bound.
                if identity_template.is_some() || !fallback_rules.is_empty() {
                    return Err(EngineError::InvalidIdentityTemplate(
                        "Camoufox records cannot retain an identity template or fallback rules"
                            .to_owned(),
                    ));
                }
                if let Some(binding) = artifact_binding {
                    binding.validate()?;
                }
                Ok(())
            }
        }
    }

    pub fn profile_directory(&self, stock_profile_directory: &Path) -> PathBuf {
        match self {
            Self::Stock => stock_profile_directory.to_path_buf(),
            Self::ControlledChromium { .. } => stock_profile_directory
                .join("engines")
                .join("controlled-chromium"),
            Self::Camoufox { .. } => stock_profile_directory.join("engines").join("camoufox"),
        }
    }

    pub fn all_profile_directories(stock_profile_directory: &Path) -> [PathBuf; 3] {
        [
            stock_profile_directory.to_path_buf(),
            stock_profile_directory
                .join("engines")
                .join("controlled-chromium"),
            stock_profile_directory.join("engines").join("camoufox"),
        ]
    }

    pub fn camoufox_artifact_binding(&self) -> Option<&CamoufoxArtifactBindingV1> {
        match self {
            Self::Camoufox {
                artifact_binding, ..
            } => artifact_binding.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineChannel {
    Stable,
    Experimental,
    Development,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EngineCapabilityId {
    ProfileIsolation,
    LaunchNetwork,
    IdentityTemplate,
    UaUaCh,
    LanguageTimezone,
    Screen,
    Canvas,
    Webgl,
    Fonts,
    MediaDevices,
    RequestHeaders,
    Window,
    Iframe,
    DedicatedWorker,
    TlsClientHello,
    Quic,
    SiteFallback,
}

impl EngineCapabilityId {
    pub const ALL: [Self; 17] = [
        Self::ProfileIsolation,
        Self::LaunchNetwork,
        Self::IdentityTemplate,
        Self::UaUaCh,
        Self::LanguageTimezone,
        Self::Screen,
        Self::Canvas,
        Self::Webgl,
        Self::Fonts,
        Self::MediaDevices,
        Self::RequestHeaders,
        Self::Window,
        Self::Iframe,
        Self::DedicatedWorker,
        Self::TlsClientHello,
        Self::Quic,
        Self::SiteFallback,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineCapabilityAvailability {
    Supported,
    Experimental,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineCapabilityOperation {
    NotConfigured,
    Configured,
    Applied,
    Verified,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineCapabilityState {
    pub id: EngineCapabilityId,
    pub availability: EngineCapabilityAvailability,
    pub operation: EngineCapabilityOperation,
    pub reason: String,
    pub verified_at: Option<DateTime<Utc>>,
    pub evidence: Vec<String>,
}

impl EngineCapabilityState {
    fn declared(
        id: EngineCapabilityId,
        availability: EngineCapabilityAvailability,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id,
            availability,
            operation: EngineCapabilityOperation::NotConfigured,
            reason: reason.into(),
            verified_at: None,
            evidence: Vec::new(),
        }
    }

    pub fn transition(
        &mut self,
        target: EngineCapabilityOperation,
        evidence: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        if self.availability == EngineCapabilityAvailability::Unavailable
            && !matches!(
                target,
                EngineCapabilityOperation::NotConfigured | EngineCapabilityOperation::Failed
            )
        {
            return Err(EngineError::InvalidTransition(
                "unavailable capabilities cannot be configured, applied, or verified".to_owned(),
            ));
        }
        let allowed = matches!(
            (self.operation, target),
            (
                EngineCapabilityOperation::NotConfigured,
                EngineCapabilityOperation::Configured
            ) | (
                EngineCapabilityOperation::Configured,
                EngineCapabilityOperation::Applied
            ) | (
                EngineCapabilityOperation::Applied,
                EngineCapabilityOperation::Verified
            ) | (
                EngineCapabilityOperation::Configured
                    | EngineCapabilityOperation::Applied
                    | EngineCapabilityOperation::Verified,
                EngineCapabilityOperation::NotConfigured
            ) | (_, EngineCapabilityOperation::Failed)
        );
        if !allowed {
            return Err(EngineError::InvalidTransition(format!(
                "cannot move {:?} from {:?} to {:?}",
                self.id, self.operation, target
            )));
        }
        validate_evidence_entries(&evidence)?;
        if matches!(
            target,
            EngineCapabilityOperation::Applied
                | EngineCapabilityOperation::Verified
                | EngineCapabilityOperation::Failed
        ) && evidence.is_empty()
        {
            return Err(EngineError::InvalidTransition(
                "applied, verified, and failed states require direct evidence".to_owned(),
            ));
        }
        if target == EngineCapabilityOperation::NotConfigured
            && matches!(
                self.operation,
                EngineCapabilityOperation::Applied | EngineCapabilityOperation::Verified
            )
            && evidence.is_empty()
        {
            return Err(EngineError::InvalidTransition(
                "restoring an applied capability requires restore evidence".to_owned(),
            ));
        }
        self.operation = target;
        self.verified_at = (target == EngineCapabilityOperation::Verified).then_some(now);
        self.evidence = evidence;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineDescriptor {
    pub contract_version: u32,
    pub id: EngineAdapterId,
    pub adapter_version: String,
    pub engine_version: String,
    pub channel: EngineChannel,
    pub browser_family: BrowserFamily,
    pub platform: String,
    pub externally_packaged: bool,
    pub emergency_disabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFamily {
    Chromium,
    Firefox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineNegotiation {
    pub adapter: EngineDescriptor,
    pub capabilities: Vec<EngineCapabilityState>,
    pub accepted: Vec<EngineCapabilityId>,
    pub rejected: Vec<EngineCapabilityId>,
}

#[derive(Debug, Clone)]
pub struct EngineLaunchRequest {
    pub silo_id: Option<Uuid>,
    pub session_id: Uuid,
    pub profile_directory: PathBuf,
    pub network_profile: NetworkProfile,
    pub identity: Option<IdentityTemplate>,
    pub derived_token: Option<DerivedIdentityToken>,
    pub fallback_rules: Vec<SiteFallbackRule>,
    pub camoufox_artifact_binding: Option<CamoufoxArtifactBindingV1>,
    pub camoufox_roots: Option<CamoufoxHostRoots>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineLaunchPlan {
    pub adapter: EngineDescriptor,
    pub transport: EngineTransport,
    pub executable_path: PathBuf,
    pub arguments: Vec<String>,
    pub profile_directory: PathBuf,
    pub shell: bool,
    pub capabilities: Vec<EngineCapabilityState>,
    pub identity_delivery: Option<IdentityDeliveryRequirement>,
    pub control: Option<EngineControlPlan>,
    pub camoufox_host: Option<CamoufoxHostLaunch>,
    pub package_verification: Option<EngineLaunchPackageVerification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CamoufoxHostLaunch {
    pub protocol: String,
    pub host_version: String,
    pub platform: String,
    pub artifact_id: String,
    pub artifact_file_sha256: String,
    pub profile_id: String,
    pub browser_release: String,
    pub browser_asset_sha256: String,
    pub browser_tree_manifest_path: PathBuf,
    pub browser_tree_manifest_sha256: String,
    #[serde(skip)]
    pub browser_proxy_server: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CamoufoxProvisionResult {
    pub artifact_id: String,
    pub artifact_file_sha256: String,
    pub schema: String,
    pub raw_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxProvisionResponse {
    ok: bool,
    #[serde(default)]
    result: Option<CamoufoxProvisionResponseResult>,
    #[serde(default)]
    error: Option<CamoufoxProvisionResponseError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxProvisionResponseResult {
    #[serde(rename = "artifactId")]
    artifact_id: String,
    #[serde(rename = "artifactFileSha256")]
    artifact_file_sha256: String,
    schema: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxProvisionResponseError {
    #[allow(dead_code)]
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineLaunchPackageVerification {
    pub verifier_id: String,
    pub artifact_sha256: String,
    pub digest_verified: bool,
    pub signature_verified: bool,
    pub package_manifest_sha256: String,
    pub package_tree_sha256: Option<String>,
    pub host_sha256: String,
    pub signer_certificate_sha256: String,
    pub engine_revision: Option<String>,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityDeliveryRequirement {
    pub token_id: Uuid,
    pub delivery: IdentityDelivery,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentityDelivery {
    SecureStdinBeforeNavigation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineControlPhase {
    Observe,
    Apply,
    Verify,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineControlPlan {
    pub session_id: Uuid,
    pub template_id: Uuid,
    pub phases: [EngineControlPhase; 4],
    pub capabilities: Vec<EngineCapabilityState>,
    pub site_fallback: SiteFallbackPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SiteFallbackPolicy {
    pub default_action: SiteFallbackAction,
    pub rules: Vec<SiteFallbackRule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SiteFallbackAction {
    RestoreExperimentalControls,
    RestoreThenReload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SiteFallbackRule {
    pub site_pattern: String,
    pub disable_capabilities: Vec<EngineCapabilityId>,
    pub action: SiteFallbackAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineCapabilityEvidence {
    pub id: EngineCapabilityId,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineControlPhaseReceipt {
    pub phase: EngineControlPhase,
    pub recorded_at: DateTime<Utc>,
    pub capabilities: Vec<EngineCapabilityEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SiteFallbackReceipt {
    pub site: String,
    pub matched_pattern: String,
    pub action: SiteFallbackAction,
    pub restored_at: DateTime<Utc>,
    pub capabilities: Vec<EngineCapabilityEvidence>,
}

/// Wire-only runtime receipt payload. The outer frame carries all launch
/// bindings and a short validity window; accepted receipts are copied into the
/// sanitized phase/fallback records above, so token identifiers are never
/// exposed through RuntimeActivation or persisted runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineRuntimeReceipt {
    Phase(EngineRuntimePhaseReceipt),
    SiteFallback(EngineRuntimeSiteFallbackReceipt),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineRuntimePhaseReceipt {
    pub phase: EngineControlPhase,
    pub capabilities: Vec<EngineCapabilityEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineRuntimeSiteFallbackReceipt {
    pub site: String,
    pub matched_pattern: String,
    pub action: SiteFallbackAction,
    pub capabilities: Vec<EngineCapabilityEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineRuntimeReceiptFrame {
    pub receipt_version: u32,
    pub contract_version: u32,
    pub adapter_id: EngineAdapterId,
    pub silo_id: Uuid,
    pub session_id: Uuid,
    pub token_id: Uuid,
    pub package: EngineBootstrapPackageBinding,
    pub sequence: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub receipt: EngineRuntimeReceipt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineControlExecution {
    pub session_id: Uuid,
    pub template_id: Uuid,
    pub next_phase: Option<EngineControlPhase>,
    pub capabilities: Vec<EngineCapabilityState>,
    pub site_fallback: SiteFallbackPolicy,
    pub phase_receipts: Vec<EngineControlPhaseReceipt>,
    pub fallback_receipts: Vec<SiteFallbackReceipt>,
}

impl EngineControlExecution {
    pub fn from_plan(plan: EngineControlPlan) -> Self {
        Self {
            session_id: plan.session_id,
            template_id: plan.template_id,
            next_phase: Some(EngineControlPhase::Observe),
            capabilities: plan.capabilities,
            site_fallback: plan.site_fallback,
            phase_receipts: Vec::new(),
            fallback_receipts: Vec::new(),
        }
    }

    pub fn record_phase(
        &mut self,
        phase: EngineControlPhase,
        evidence: Vec<EngineCapabilityEvidence>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        if self.next_phase != Some(phase) {
            return Err(EngineError::InvalidTransition(format!(
                "expected {:?}, received {phase:?}",
                self.next_phase
            )));
        }
        let target_ids = self
            .capabilities
            .iter()
            .filter_map(|capability| {
                let included = match phase {
                    EngineControlPhase::Observe | EngineControlPhase::Apply => {
                        capability.operation == EngineCapabilityOperation::Configured
                    }
                    EngineControlPhase::Verify => {
                        capability.operation == EngineCapabilityOperation::Applied
                    }
                    EngineControlPhase::Restore => matches!(
                        capability.operation,
                        EngineCapabilityOperation::Configured
                            | EngineCapabilityOperation::Applied
                            | EngineCapabilityOperation::Verified
                    ),
                };
                included.then_some(capability.id)
            })
            .collect::<BTreeSet<_>>();
        let evidence_by_id = validate_phase_evidence(&evidence, &target_ids)?;

        if phase != EngineControlPhase::Observe {
            let target_operation = match phase {
                EngineControlPhase::Apply => EngineCapabilityOperation::Applied,
                EngineControlPhase::Verify => EngineCapabilityOperation::Verified,
                EngineControlPhase::Restore => EngineCapabilityOperation::NotConfigured,
                EngineControlPhase::Observe => unreachable!(),
            };
            for capability in &mut self.capabilities {
                if target_ids.contains(&capability.id) {
                    capability.transition(
                        target_operation,
                        evidence_by_id
                            .get(&capability.id)
                            .cloned()
                            .unwrap_or_default(),
                        now,
                    )?;
                }
            }
        }

        self.phase_receipts.push(EngineControlPhaseReceipt {
            phase,
            recorded_at: now,
            capabilities: evidence,
        });
        self.next_phase = match phase {
            EngineControlPhase::Observe => Some(EngineControlPhase::Apply),
            EngineControlPhase::Apply => Some(EngineControlPhase::Verify),
            EngineControlPhase::Verify => Some(EngineControlPhase::Restore),
            EngineControlPhase::Restore => None,
        };
        Ok(())
    }

    /// Applies one already-bound wire receipt transactionally. In particular,
    /// a controlled process cannot choose its own fallback rule/action: the
    /// claimed match must equal the desktop's most-specific policy match.
    pub fn apply_runtime_receipt(
        &mut self,
        receipt: EngineRuntimeReceipt,
        recorded_at: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let mut candidate = self.clone();
        match receipt {
            EngineRuntimeReceipt::Phase(receipt) => {
                candidate.record_phase(receipt.phase, receipt.capabilities, recorded_at)?
            }
            EngineRuntimeReceipt::SiteFallback(receipt) => {
                if candidate.next_phase != Some(EngineControlPhase::Restore) {
                    return Err(EngineError::InvalidTransition(
                        "site fallback receipts are accepted only after initial verification"
                            .to_owned(),
                    ));
                }
                let accepted = candidate
                    .apply_site_fallback(&receipt.site, receipt.capabilities, recorded_at)?
                    .ok_or_else(|| {
                        EngineError::InvalidTransition(
                            "runtime fallback did not match a configured desktop rule".to_owned(),
                        )
                    })?;
                if accepted.site != receipt.site
                    || accepted.matched_pattern != receipt.matched_pattern
                    || accepted.action != receipt.action
                {
                    return Err(EngineError::InvalidTransition(
                        "runtime fallback rule or action does not match desktop policy".to_owned(),
                    ));
                }
            }
        }
        *self = candidate;
        Ok(())
    }

    pub fn launch_evidence_complete(&self) -> bool {
        self.next_phase == Some(EngineControlPhase::Restore)
            && self.phase_receipts.len() == 3
            && self.phase_receipts.iter().map(|receipt| receipt.phase).eq([
                EngineControlPhase::Observe,
                EngineControlPhase::Apply,
                EngineControlPhase::Verify,
            ])
            && self.capabilities.iter().all(|capability| {
                capability.availability == EngineCapabilityAvailability::Unavailable
                    || matches!(
                        capability.operation,
                        EngineCapabilityOperation::NotConfigured
                            | EngineCapabilityOperation::Verified
                    )
            })
    }

    pub fn restore_complete(&self) -> bool {
        self.next_phase.is_none()
            && self
                .phase_receipts
                .last()
                .is_some_and(|receipt| receipt.phase == EngineControlPhase::Restore)
            && self.capabilities.iter().all(|capability| {
                !matches!(
                    capability.operation,
                    EngineCapabilityOperation::Configured
                        | EngineCapabilityOperation::Applied
                        | EngineCapabilityOperation::Verified
                )
            })
    }

    pub fn fail_active_capabilities(&mut self, reason: &str, now: DateTime<Utc>) {
        for capability in &mut self.capabilities {
            if matches!(
                capability.operation,
                EngineCapabilityOperation::Configured
                    | EngineCapabilityOperation::Applied
                    | EngineCapabilityOperation::Verified
            ) {
                let _ = capability.transition(
                    EngineCapabilityOperation::Failed,
                    vec![reason.to_owned()],
                    now,
                );
            }
        }
    }

    pub fn apply_site_fallback(
        &mut self,
        site: &str,
        evidence: Vec<EngineCapabilityEvidence>,
        now: DateTime<Utc>,
    ) -> Result<Option<SiteFallbackReceipt>, EngineError> {
        if self.fallback_receipts.len() >= MAX_RETAINED_ENGINE_FALLBACK_RECEIPTS {
            return Err(EngineError::InvalidTransition(
                "runtime fallback receipt retention limit was reached".to_owned(),
            ));
        }
        if !matches!(
            self.next_phase,
            Some(EngineControlPhase::Verify | EngineControlPhase::Restore)
        ) {
            return Err(EngineError::InvalidTransition(
                "site fallback is valid only after controls have been applied".to_owned(),
            ));
        }
        let normalized_site = normalize_runtime_host(site)?;
        let Some(rule) = self
            .site_fallback
            .rules
            .iter()
            .filter(|rule| site_pattern_matches(&rule.site_pattern, &normalized_site))
            .max_by_key(|rule| rule.site_pattern.len())
            .cloned()
        else {
            return Ok(None);
        };
        let target_ids = rule
            .disable_capabilities
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let evidence_by_id = validate_phase_evidence(&evidence, &target_ids)?;
        for capability in &mut self.capabilities {
            if target_ids.contains(&capability.id) {
                if !matches!(
                    capability.operation,
                    EngineCapabilityOperation::Applied | EngineCapabilityOperation::Verified
                ) {
                    return Err(EngineError::InvalidTransition(format!(
                        "fallback target {:?} is not currently applied",
                        capability.id
                    )));
                }
                capability.transition(
                    EngineCapabilityOperation::NotConfigured,
                    evidence_by_id
                        .get(&capability.id)
                        .cloned()
                        .unwrap_or_default(),
                    now,
                )?;
            }
        }
        let receipt = SiteFallbackReceipt {
            site: normalized_site,
            matched_pattern: rule.site_pattern,
            action: rule.action,
            restored_at: now,
            capabilities: evidence,
        };
        self.fallback_receipts.push(receipt.clone());
        Ok(Some(receipt))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityTemplate {
    pub schema_version: u32,
    pub template_id: Uuid,
    pub os: IdentityOperatingSystem,
    pub browser: IdentityBrowser,
    pub languages: IdentityLanguages,
    pub timezone: String,
    pub screen: IdentityScreen,
    pub render: IdentityRender,
    pub fonts: IdentityFonts,
    pub media: IdentityMedia,
    pub network: IdentityNetwork,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityOperatingSystem {
    pub family: String,
    pub version: String,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityBrowser {
    pub family: BrowserFamily,
    pub major_version: u16,
    pub user_agent: String,
    pub ua_ch: Option<IdentityUaCh>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityUaCh {
    pub brands: Vec<IdentityUaChBrand>,
    pub platform: String,
    pub platform_version: String,
    pub architecture: String,
    pub bitness: String,
    pub mobile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityUaChBrand {
    pub brand: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityLanguages {
    pub primary: String,
    pub accepted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityScreen {
    pub width: u32,
    pub height: u32,
    pub available_width: u32,
    pub available_height: u32,
    pub device_pixel_ratio: f64,
    pub color_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityRender {
    pub canvas: CanvasMode,
    pub web_gl_vendor: Option<String>,
    pub web_gl_renderer: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CanvasMode {
    Native,
    Normalized,
    Controlled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityFonts {
    pub families: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityMedia {
    pub microphones: u8,
    pub cameras: u8,
    pub speakers: u8,
    pub labels_exposed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityNetwork {
    pub proxy_required: bool,
    pub country_code: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub desired_quic: DesiredQuic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesiredQuic {
    BrowserDefault,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityDerivationContext {
    pub silo_id: Uuid,
    pub seed_reference: Uuid,
    pub template_id: Uuid,
    pub session_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct DerivedIdentityToken {
    pub token_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for DerivedIdentityToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DerivedIdentityToken")
            .field("token_id", &self.token_id)
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl Drop for DerivedIdentityToken {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

impl DerivedIdentityToken {
    fn into_bootstrap_token(mut self) -> EngineBootstrapToken {
        EngineBootstrapToken {
            token_id: self.token_id,
            opaque_token: std::mem::take(&mut self.token),
            expires_at: self.expires_at,
        }
    }
}

/// Native-only token derivation from the 32-byte seed held by the unlocked
/// Vault. HMAC-SHA-256 gives deterministic, domain-separated session output;
/// the seed itself is never serialized into a launch plan or bootstrap frame.
pub struct VaultSeedIdentityTokenDeriver {
    seed: Zeroizing<[u8; 32]>,
}

impl VaultSeedIdentityTokenDeriver {
    pub fn new(seed: &[u8]) -> Result<Self, EngineError> {
        let seed: [u8; 32] = seed.try_into().map_err(|_| {
            EngineError::VerificationUnavailable(
                "Vault identity seed must contain exactly 32 bytes".to_owned(),
            )
        })?;
        Ok(Self {
            seed: Zeroizing::new(seed),
        })
    }
}

impl std::fmt::Debug for VaultSeedIdentityTokenDeriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultSeedIdentityTokenDeriver")
            .field("seed", &"[REDACTED]")
            .finish()
    }
}

impl IdentityTokenDeriver for VaultSeedIdentityTokenDeriver {
    fn derive_session_token(
        &self,
        context: &IdentityDerivationContext,
    ) -> Result<DerivedIdentityToken, EngineError> {
        validate_derivation_context(context, Utc::now())?;
        let encoded_context = encode_derivation_context(context);

        let mut token_input =
            Vec::with_capacity(SESSION_TOKEN_DOMAIN.len() + encoded_context.len());
        token_input.extend_from_slice(SESSION_TOKEN_DOMAIN);
        token_input.extend_from_slice(&encoded_context);
        let mut token_digest = hmac_sha256(self.seed.as_ref(), &token_input);

        let mut id_input =
            Vec::with_capacity(SESSION_TOKEN_ID_DOMAIN.len() + encoded_context.len());
        id_input.extend_from_slice(SESSION_TOKEN_ID_DOMAIN);
        id_input.extend_from_slice(&encoded_context);
        let mut id_digest = hmac_sha256(self.seed.as_ref(), &id_input);
        let mut token_id_bytes = [0_u8; 16];
        token_id_bytes.copy_from_slice(&id_digest[..16]);
        token_id_bytes[6] = (token_id_bytes[6] & 0x0f) | 0x40;
        token_id_bytes[8] = (token_id_bytes[8] & 0x3f) | 0x80;

        let token = URL_SAFE_NO_PAD.encode(token_digest);
        token_digest.zeroize();
        id_digest.zeroize();
        token_input.zeroize();
        id_input.zeroize();

        Ok(DerivedIdentityToken {
            token_id: Uuid::from_bytes(token_id_bytes),
            token,
            expires_at: context.expires_at,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineBootstrapToken {
    pub token_id: Uuid,
    pub opaque_token: String,
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for EngineBootstrapToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EngineBootstrapToken")
            .field("token_id", &self.token_id)
            .field("opaque_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl Drop for EngineBootstrapToken {
    fn drop(&mut self) {
        self.opaque_token.zeroize();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineBootstrapPackageBinding {
    pub engine_version: String,
    pub artifact_sha256: String,
    pub verifier_id: String,
    pub verified_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineBootstrapEnvelope {
    pub bootstrap_version: u32,
    pub contract_version: u32,
    pub adapter_id: EngineAdapterId,
    pub silo_id: Uuid,
    pub session_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub package: EngineBootstrapPackageBinding,
    pub identity: IdentityTemplate,
    pub control: EngineControlPlan,
    pub token: EngineBootstrapToken,
}

impl std::fmt::Debug for EngineBootstrapEnvelope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EngineBootstrapEnvelope")
            .field("bootstrap_version", &self.bootstrap_version)
            .field("contract_version", &self.contract_version)
            .field("adapter_id", &self.adapter_id)
            .field("silo_id", &self.silo_id)
            .field("session_id", &self.session_id)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("package", &self.package)
            .field("template_id", &self.identity.template_id)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl EngineBootstrapEnvelope {
    pub fn for_launch(
        silo_id: Uuid,
        issued_at: DateTime<Utc>,
        plan: &EngineLaunchPlan,
        identity: IdentityTemplate,
        token: DerivedIdentityToken,
    ) -> Result<Self, EngineError> {
        let control = plan.control.clone().ok_or_else(|| {
            EngineError::InvalidBootstrap(
                "external launch plan has no constrained control plan".to_owned(),
            )
        })?;
        let delivery = plan.identity_delivery.as_ref().ok_or_else(|| {
            EngineError::InvalidBootstrap(
                "external launch plan has no secure identity delivery requirement".to_owned(),
            )
        })?;
        if delivery.delivery != IdentityDelivery::SecureStdinBeforeNavigation
            || delivery.token_id != token.token_id
            || delivery.expires_at != token.expires_at
        {
            return Err(EngineError::InvalidBootstrap(
                "launch plan and derived token delivery metadata do not match".to_owned(),
            ));
        }
        let package_verification = plan.package_verification.as_ref().ok_or_else(|| {
            EngineError::InvalidBootstrap(
                "external launch plan has no per-launch package verification".to_owned(),
            )
        })?;
        if !plan.adapter.externally_packaged
            || plan.adapter.contract_version != ENGINE_CONTRACT_VERSION
            || !package_verification.digest_verified
            || !package_verification.signature_verified
            || !valid_text(&package_verification.verifier_id, 100)
            || package_verification.verified_at > Utc::now() + Duration::seconds(5)
            || package_verification.verified_at < Utc::now() - Duration::minutes(1)
        {
            return Err(EngineError::InvalidBootstrap(
                "external bootstrap requires a freshly digest- and signature-verified package"
                    .to_owned(),
            ));
        }
        let token_expires_at = token.expires_at;
        let envelope = Self {
            bootstrap_version: ENGINE_BOOTSTRAP_VERSION,
            contract_version: ENGINE_CONTRACT_VERSION,
            adapter_id: plan.adapter.id,
            silo_id,
            session_id: control.session_id,
            issued_at,
            expires_at: token_expires_at,
            package: EngineBootstrapPackageBinding {
                engine_version: plan.adapter.engine_version.clone(),
                artifact_sha256: package_verification.artifact_sha256.clone(),
                verifier_id: package_verification.verifier_id.clone(),
                verified_at: package_verification.verified_at,
            },
            identity,
            control,
            token: token.into_bootstrap_token(),
        };
        envelope.validate_at(Utc::now(), Some(plan.adapter.id))?;
        Ok(envelope)
    }

    pub fn validate_at(
        &self,
        now: DateTime<Utc>,
        expected_adapter: Option<EngineAdapterId>,
    ) -> Result<(), EngineError> {
        if self.bootstrap_version != ENGINE_BOOTSTRAP_VERSION
            || self.contract_version != ENGINE_CONTRACT_VERSION
            || !matches!(
                self.adapter_id,
                EngineAdapterId::ControlledChromium | EngineAdapterId::Camoufox
            )
            || expected_adapter.is_some_and(|adapter| adapter != self.adapter_id)
            || self.control.session_id != self.session_id
            || self.control.template_id != self.identity.template_id
            || self.token.expires_at != self.expires_at
            || !valid_engine_version(&self.package.engine_version)
            || !is_lower_hex(&self.package.artifact_sha256, 64)
            || !valid_text(&self.package.verifier_id, 100)
            || self.package.verified_at > now + Duration::seconds(5)
            || self.package.verified_at < now - Duration::minutes(1)
        {
            return Err(EngineError::InvalidBootstrap(
                "bootstrap version, adapter, session, template, or expiry binding is invalid"
                    .to_owned(),
            ));
        }
        let expected_family = match self.adapter_id {
            EngineAdapterId::ControlledChromium => BrowserFamily::Chromium,
            EngineAdapterId::Camoufox => BrowserFamily::Firefox,
            EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => unreachable!(),
        };
        validate_identity_template(
            &self.identity,
            expected_family,
            version_major(&self.package.engine_version),
        )?;
        validate_fallback_rules(&self.control.site_fallback.rules)?;
        let expected_phases = [
            EngineControlPhase::Observe,
            EngineControlPhase::Apply,
            EngineControlPhase::Verify,
            EngineControlPhase::Restore,
        ];
        if self.control.phases != expected_phases
            || self.control.capabilities.len() > EngineCapabilityId::ALL.len()
            || self.control.site_fallback.default_action
                != SiteFallbackAction::RestoreExperimentalControls
        {
            return Err(EngineError::InvalidBootstrap(
                "bootstrap control phases or capability bounds are invalid".to_owned(),
            ));
        }
        validate_derivation_context(
            &IdentityDerivationContext {
                silo_id: self.silo_id,
                seed_reference: Uuid::nil(),
                template_id: self.identity.template_id,
                session_id: self.session_id,
                issued_at: self.issued_at,
                expires_at: self.expires_at,
            },
            now,
        )?;
        validate_derived_token(
            &DerivedIdentityToken {
                token_id: self.token.token_id,
                token: self.token.opaque_token.clone(),
                expires_at: self.token.expires_at,
            },
            now,
        )
    }
}

#[derive(Debug, Clone)]
pub struct EngineBootstrapAckExpectation {
    adapter_id: EngineAdapterId,
    silo_id: Uuid,
    session_id: Uuid,
    token_id: Uuid,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    package: EngineBootstrapPackageBinding,
}

impl From<&EngineBootstrapEnvelope> for EngineBootstrapAckExpectation {
    fn from(envelope: &EngineBootstrapEnvelope) -> Self {
        Self {
            adapter_id: envelope.adapter_id,
            silo_id: envelope.silo_id,
            session_id: envelope.session_id,
            token_id: envelope.token.token_id,
            issued_at: envelope.issued_at,
            expires_at: envelope.expires_at,
            package: envelope.package.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineBootstrapAckStatus {
    BootstrapApplied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineBootstrapAck {
    pub ack_version: u32,
    pub contract_version: u32,
    pub adapter_id: EngineAdapterId,
    pub silo_id: Uuid,
    pub session_id: Uuid,
    pub token_id: Uuid,
    pub package: EngineBootstrapPackageBinding,
    pub status: EngineBootstrapAckStatus,
    pub accepted_at: DateTime<Utc>,
}

impl EngineBootstrapAck {
    pub fn accepted(envelope: &EngineBootstrapEnvelope, accepted_at: DateTime<Utc>) -> Self {
        Self {
            ack_version: ENGINE_BOOTSTRAP_ACK_VERSION,
            contract_version: ENGINE_CONTRACT_VERSION,
            adapter_id: envelope.adapter_id,
            silo_id: envelope.silo_id,
            session_id: envelope.session_id,
            token_id: envelope.token.token_id,
            package: envelope.package.clone(),
            status: EngineBootstrapAckStatus::BootstrapApplied,
            accepted_at,
        }
    }

    pub fn validate(
        &self,
        expected: &EngineBootstrapAckExpectation,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        if self.ack_version != ENGINE_BOOTSTRAP_ACK_VERSION
            || self.contract_version != ENGINE_CONTRACT_VERSION
            || self.adapter_id != expected.adapter_id
            || self.silo_id != expected.silo_id
            || self.session_id != expected.session_id
            || self.token_id != expected.token_id
            || self.package.engine_version != expected.package.engine_version
            || self.package.artifact_sha256 != expected.package.artifact_sha256
            || self.package.verifier_id != expected.package.verifier_id
            || self.package.verified_at != expected.package.verified_at
            || self.status != EngineBootstrapAckStatus::BootstrapApplied
            || self.accepted_at < expected.issued_at - Duration::seconds(5)
            || self.accepted_at > now + Duration::seconds(5)
            || self.accepted_at >= expected.expires_at
        {
            return Err(EngineError::InvalidBootstrap(
                "bootstrap ACK does not match its adapter, package, session, token reference, or lifetime"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

pub struct EngineRuntimeReceiptExpectation {
    adapter_id: EngineAdapterId,
    silo_id: Uuid,
    session_id: Uuid,
    token_id: Uuid,
    bootstrap_issued_at: DateTime<Utc>,
    package: EngineBootstrapPackageBinding,
    opaque_token: Zeroizing<String>,
}

impl From<&EngineBootstrapEnvelope> for EngineRuntimeReceiptExpectation {
    fn from(envelope: &EngineBootstrapEnvelope) -> Self {
        Self {
            adapter_id: envelope.adapter_id,
            silo_id: envelope.silo_id,
            session_id: envelope.session_id,
            token_id: envelope.token.token_id,
            bootstrap_issued_at: envelope.issued_at,
            package: envelope.package.clone(),
            opaque_token: Zeroizing::new(envelope.token.opaque_token.clone()),
        }
    }
}

impl EngineRuntimeReceiptFrame {
    pub fn for_envelope(
        envelope: &EngineBootstrapEnvelope,
        sequence: u64,
        issued_at: DateTime<Utc>,
        receipt: EngineRuntimeReceipt,
    ) -> Self {
        Self {
            receipt_version: ENGINE_RUNTIME_RECEIPT_VERSION,
            contract_version: ENGINE_CONTRACT_VERSION,
            adapter_id: envelope.adapter_id,
            silo_id: envelope.silo_id,
            session_id: envelope.session_id,
            token_id: envelope.token.token_id,
            package: envelope.package.clone(),
            sequence,
            issued_at,
            expires_at: issued_at + Duration::seconds(ENGINE_RUNTIME_RECEIPT_LIFETIME_SECONDS),
            receipt,
        }
    }

    pub fn validate(
        &self,
        expected: &EngineRuntimeReceiptExpectation,
        expected_sequence: u64,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        if self.receipt_version != ENGINE_RUNTIME_RECEIPT_VERSION
            || self.contract_version != ENGINE_CONTRACT_VERSION
            || self.adapter_id != expected.adapter_id
            || self.silo_id != expected.silo_id
            || self.session_id != expected.session_id
            || self.token_id != expected.token_id
            || self.package != expected.package
            || self.sequence != expected_sequence
            || self.sequence == 0
            || self.issued_at
                < expected.bootstrap_issued_at
                    - Duration::seconds(ENGINE_RUNTIME_RECEIPT_CLOCK_SKEW_SECONDS)
            || self.expires_at <= self.issued_at
            || self.expires_at
                > self.issued_at + Duration::seconds(ENGINE_RUNTIME_RECEIPT_LIFETIME_SECONDS)
            || now < self.issued_at - Duration::seconds(ENGINE_RUNTIME_RECEIPT_CLOCK_SKEW_SECONDS)
            || now > self.expires_at + Duration::seconds(ENGINE_RUNTIME_RECEIPT_CLOCK_SKEW_SECONDS)
        {
            return Err(EngineError::InvalidRuntimeReceipt(
                "receipt version, launch binding, sequence, or validity window is invalid"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

pub fn write_engine_bootstrap_ack_frame<W: Write>(
    writer: &mut W,
    ack: &EngineBootstrapAck,
) -> Result<(), EngineError> {
    let payload = serde_json::to_vec(ack)?;
    if payload.is_empty() || payload.len() > MAX_ENGINE_BOOTSTRAP_ACK_BYTES {
        return Err(EngineError::InvalidBootstrap(
            "serialized bootstrap ACK is empty or exceeds 16 KiB".to_owned(),
        ));
    }
    let payload_length = u32::try_from(payload.len()).map_err(|_| {
        EngineError::InvalidBootstrap("bootstrap ACK length does not fit u32".to_owned())
    })?;
    writer.write_all(&payload_length.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn read_engine_bootstrap_ack_frame<R: Read>(
    reader: &mut R,
    expected: &EngineBootstrapAckExpectation,
    now: DateTime<Utc>,
) -> Result<EngineBootstrapAck, EngineError> {
    let mut header = [0_u8; ENGINE_BOOTSTRAP_FRAME_HEADER_BYTES];
    reader.read_exact(&mut header)?;
    let payload_length = u32::from_be_bytes(header) as usize;
    if payload_length == 0 || payload_length > MAX_ENGINE_BOOTSTRAP_ACK_BYTES {
        return Err(EngineError::InvalidBootstrap(
            "bootstrap ACK length is zero or exceeds 16 KiB".to_owned(),
        ));
    }
    let mut payload = Zeroizing::new(vec![0_u8; payload_length]);
    reader.read_exact(payload.as_mut_slice())?;
    let ack: EngineBootstrapAck = serde_json::from_slice(payload.as_slice())?;
    ack.validate(expected, now)?;
    Ok(ack)
}

pub fn write_engine_runtime_receipt_frame<W: Write>(
    writer: &mut W,
    receipt: &EngineRuntimeReceiptFrame,
) -> Result<(), EngineError> {
    let payload = Zeroizing::new(serde_json::to_vec(receipt)?);
    if payload.is_empty() || payload.len() > MAX_ENGINE_RUNTIME_RECEIPT_BYTES {
        return Err(EngineError::InvalidRuntimeReceipt(
            "serialized runtime receipt is empty or exceeds 32 KiB".to_owned(),
        ));
    }
    let payload_length = u32::try_from(payload.len()).map_err(|_| {
        EngineError::InvalidRuntimeReceipt("runtime receipt length does not fit u32".to_owned())
    })?;
    writer.write_all(&payload_length.to_be_bytes())?;
    writer.write_all(payload.as_slice())?;
    writer.flush()?;
    Ok(())
}

pub fn read_engine_runtime_receipt_frame<R: Read>(
    reader: &mut R,
    expected: &EngineRuntimeReceiptExpectation,
    expected_sequence: u64,
    now: DateTime<Utc>,
) -> Result<EngineRuntimeReceiptFrame, EngineError> {
    let mut header = [0_u8; ENGINE_BOOTSTRAP_FRAME_HEADER_BYTES];
    reader.read_exact(&mut header)?;
    let payload_length = u32::from_be_bytes(header) as usize;
    if payload_length == 0 || payload_length > MAX_ENGINE_RUNTIME_RECEIPT_BYTES {
        return Err(EngineError::InvalidRuntimeReceipt(
            "runtime receipt length is zero or exceeds 32 KiB".to_owned(),
        ));
    }
    let mut payload = Zeroizing::new(vec![0_u8; payload_length]);
    reader.read_exact(payload.as_mut_slice())?;
    if payload
        .windows(expected.opaque_token.len())
        .any(|window| window == expected.opaque_token.as_bytes())
    {
        return Err(EngineError::InvalidRuntimeReceipt(
            "runtime receipt payload reflected forbidden bootstrap secret material".to_owned(),
        ));
    }
    let receipt: EngineRuntimeReceiptFrame = serde_json::from_slice(payload.as_slice())?;
    receipt.validate(expected, expected_sequence, now)?;
    Ok(receipt)
}

pub fn write_engine_bootstrap_frame<W: Write>(
    writer: &mut W,
    envelope: &EngineBootstrapEnvelope,
) -> Result<(), EngineError> {
    envelope.validate_at(Utc::now(), Some(envelope.adapter_id))?;
    let payload = Zeroizing::new(serde_json::to_vec(envelope)?);
    if payload.is_empty() || payload.len() > MAX_ENGINE_BOOTSTRAP_BYTES {
        return Err(EngineError::InvalidBootstrap(format!(
            "serialized bootstrap must contain 1 to {MAX_ENGINE_BOOTSTRAP_BYTES} bytes"
        )));
    }
    let payload_length = u32::try_from(payload.len()).map_err(|_| {
        EngineError::InvalidBootstrap("bootstrap frame length does not fit u32".to_owned())
    })?;
    writer.write_all(&payload_length.to_be_bytes())?;
    writer.write_all(payload.as_slice())?;
    writer.flush()?;
    Ok(())
}

pub fn read_engine_bootstrap_frame<R: Read>(
    reader: &mut R,
    expected_adapter: EngineAdapterId,
    now: DateTime<Utc>,
) -> Result<EngineBootstrapEnvelope, EngineError> {
    let mut header = [0_u8; ENGINE_BOOTSTRAP_FRAME_HEADER_BYTES];
    reader.read_exact(&mut header)?;
    let payload_length = u32::from_be_bytes(header) as usize;
    if payload_length == 0 || payload_length > MAX_ENGINE_BOOTSTRAP_BYTES {
        return Err(EngineError::InvalidBootstrap(
            "bootstrap frame length is zero or exceeds the protocol ceiling".to_owned(),
        ));
    }
    let mut payload = Zeroizing::new(vec![0_u8; payload_length]);
    reader.read_exact(payload.as_mut_slice())?;
    let mut trailing = [0_u8; 1];
    if reader.read(&mut trailing)? != 0 {
        return Err(EngineError::InvalidBootstrap(
            "bootstrap stream contains trailing bytes".to_owned(),
        ));
    }
    let envelope: EngineBootstrapEnvelope = serde_json::from_slice(payload.as_slice())?;
    envelope.validate_at(now, Some(expected_adapter))?;
    Ok(envelope)
}

pub trait IdentityTokenDeriver: Send + Sync {
    fn derive_session_token(
        &self,
        context: &IdentityDerivationContext,
    ) -> Result<DerivedIdentityToken, EngineError>;
}

pub struct UnavailableIdentityTokenDeriver;

impl IdentityTokenDeriver for UnavailableIdentityTokenDeriver {
    fn derive_session_token(
        &self,
        _context: &IdentityDerivationContext,
    ) -> Result<DerivedIdentityToken, EngineError> {
        Err(EngineError::VerificationUnavailable(
            "no Vault-backed session token deriver is installed".to_owned(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageRequest {
    pub package_root: PathBuf,
    pub expected_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageManifest {
    pub schema_version: u32,
    pub engine_id: EngineAdapterId,
    pub engine_version: String,
    pub channel: EngineChannel,
    pub platform: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub executable_relative_path: String,
    pub artifact_sha256: String,
    pub signature: EnginePackageSignature,
    pub capabilities: Vec<EngineCapabilityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<EnginePackageEntrypoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_manifest: Option<EnginePackageTreeBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_tree_manifest: Option<EnginePackageTreeBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_asset_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageEntrypoint {
    pub kind: String,
    pub relative_path: String,
    pub protocol: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageTreeBinding {
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageTreeManifest {
    pub schema: String,
    pub entries: Vec<EnginePackageTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageTreeEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineBrowserTreeManifest {
    pub schema: String,
    pub tree_root_label: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub entries: Vec<EngineBrowserTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineBrowserTreeEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct EnginePackageVerification {
    pub verifier_id: String,
    pub digest_verified: bool,
    pub signature_verified: bool,
    pub package_manifest_sha256: String,
    pub package_tree_sha256: Option<String>,
    pub host_sha256: String,
    pub signer_certificate_sha256: String,
    pub engine_revision: Option<String>,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineTrustedSignerPolicy {
    pub schema_version: u32,
    pub signers: Vec<EngineTrustedSigner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineTrustedSigner {
    pub certificate_sha256: String,
    pub publisher: String,
}

pub trait EnginePackageVerifier: Send + Sync {
    fn verify(
        &self,
        manifest_bytes: &[u8],
        executable_path: &Path,
        manifest: &EnginePackageManifest,
    ) -> Result<EnginePackageVerification, EngineError>;
}

pub struct WindowsProductionEnginePackageVerifier {
    trusted_signers: BTreeMap<String, String>,
}

impl WindowsProductionEnginePackageVerifier {
    pub fn from_embedded_policy() -> Result<Self, EngineError> {
        let mut verifier =
            Self::from_policy_bytes(include_bytes!("../resources/engine-trusted-signers.json"))?;
        if let Some(build_pins) = option_env!("VERISILO_ENGINE_SIGNER_SHA256") {
            for pin in build_pins
                .split(',')
                .map(str::trim)
                .filter(|pin| !pin.is_empty())
            {
                if !is_lower_hex(pin, 64) {
                    return Err(EngineError::VerificationUnavailable(
                        "VERISILO_ENGINE_SIGNER_SHA256 contains an invalid certificate pin"
                            .to_owned(),
                    ));
                }
                verifier
                    .trusted_signers
                    .entry(pin.to_owned())
                    .or_insert_with(|| "release-build signer".to_owned());
            }
        }
        Ok(verifier)
    }

    pub fn from_policy_bytes(policy_bytes: &[u8]) -> Result<Self, EngineError> {
        let policy: EngineTrustedSignerPolicy = serde_json::from_slice(policy_bytes)?;
        if policy.schema_version != 1 || policy.signers.len() > 16 {
            return Err(EngineError::VerificationUnavailable(
                "engine signer policy schema or signer count is invalid".to_owned(),
            ));
        }
        let mut trusted_signers = BTreeMap::new();
        for signer in policy.signers {
            if !is_lower_hex(&signer.certificate_sha256, 64)
                || !valid_text(&signer.publisher, 200)
                || trusted_signers
                    .insert(signer.certificate_sha256, signer.publisher)
                    .is_some()
            {
                return Err(EngineError::VerificationUnavailable(
                    "engine signer policy contains an invalid or duplicate signer".to_owned(),
                ));
            }
        }
        Ok(Self { trusted_signers })
    }
}

impl EnginePackageVerifier for WindowsProductionEnginePackageVerifier {
    fn verify(
        &self,
        _manifest_bytes: &[u8],
        executable_path: &Path,
        manifest: &EnginePackageManifest,
    ) -> Result<EnginePackageVerification, EngineError> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (executable_path, manifest);
            Err(EngineError::VerificationUnavailable(
                "the production signed-manifest verifier is available only on Windows".to_owned(),
            ))
        }

        #[cfg(target_os = "windows")]
        {
            let _publisher = self
                .trusted_signers
                .get(&manifest.signature.key_id)
                .ok_or_else(|| {
                    EngineError::VerificationUnavailable(
                        "manifest signer certificate is not pinned by this build".to_owned(),
                    )
                })?;
            let artifact_digest = sha256_file(executable_path)?;
            if hex_lower(&artifact_digest) != manifest.artifact_sha256 {
                return Err(EngineError::VerificationUnavailable(
                    "engine executable SHA-256 does not match the signed manifest".to_owned(),
                ));
            }
            let signature = BASE64_STANDARD
                .decode(manifest.signature.value.as_bytes())
                .map_err(|_| {
                    EngineError::VerificationUnavailable(
                        "manifest CMS signature is not canonical base64".to_owned(),
                    )
                })?;
            if signature.is_empty() || signature.len() > MAX_ENGINE_SIGNATURE_BYTES {
                return Err(EngineError::VerificationUnavailable(
                    "manifest CMS signature has an invalid size".to_owned(),
                ));
            }
            let payload = manifest_signing_payload(manifest)?;
            let signer_certificate = verify_windows_detached_cms_sha256(&payload, &signature)?;
            let signer_digest = hex_lower(&sha256_bytes(&signer_certificate));
            if signer_digest != manifest.signature.key_id {
                return Err(EngineError::VerificationUnavailable(
                    "CMS signer certificate does not match manifest.signature.keyId".to_owned(),
                ));
            }
            Ok(EnginePackageVerification {
                verifier_id: format!("windows-crypt32-cms-sha256-v1:{}", &signer_digest[..12]),
                digest_verified: true,
                signature_verified: true,
                package_manifest_sha256: String::new(),
                package_tree_sha256: None,
                host_sha256: manifest.artifact_sha256.clone(),
                signer_certificate_sha256: signer_digest,
                engine_revision: None,
                verified_at: Utc::now(),
            })
        }
    }
}

pub struct UnavailableEnginePackageVerifier;

impl EnginePackageVerifier for UnavailableEnginePackageVerifier {
    fn verify(
        &self,
        _manifest_bytes: &[u8],
        _executable_path: &Path,
        _manifest: &EnginePackageManifest,
    ) -> Result<EnginePackageVerification, EngineError> {
        Err(EngineError::VerificationUnavailable(
            "no production package hash/signature verifier is installed".to_owned(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedEnginePackage {
    pub package_root: PathBuf,
    pub manifest_path: PathBuf,
    pub executable_path: PathBuf,
    pub manifest: EnginePackageManifest,
    pub verification: EnginePackageVerification,
}

impl VerifiedEnginePackage {
    fn entrypoint(&self) -> Option<&EnginePackageEntrypoint> {
        self.manifest.entrypoint.as_ref()
    }

    fn browser_tree_manifest(&self) -> Option<&EnginePackageTreeBinding> {
        self.manifest.browser_tree_manifest.as_ref()
    }

    fn host_version(&self) -> Result<String, EngineError> {
        self.manifest.host_version.clone().ok_or_else(|| {
            EngineError::InvalidPackage("Camoufox package Host version is missing".to_owned())
        })
    }

    fn browser_asset_sha256(&self) -> Result<String, EngineError> {
        self.manifest.browser_asset_sha256.clone().ok_or_else(|| {
            EngineError::InvalidPackage(
                "Camoufox package browser asset SHA-256 is missing".to_owned(),
            )
        })
    }

    fn browser_release(&self) -> Result<String, EngineError> {
        self.manifest.browser_release.clone().ok_or_else(|| {
            EngineError::InvalidPackage(
                "Camoufox package browser release binding is missing".to_owned(),
            )
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMaintenanceReceipt {
    pub action: EngineMaintenanceAction,
    pub adapter_id: EngineAdapterId,
    pub engine_version: String,
    pub verifier_id: String,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineMaintenanceAction {
    Install,
    Update,
    Rollback,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineHealth {
    pub state: EngineHealthState,
    pub checked_at: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineHealthState {
    Healthy,
    Degraded,
    Unavailable,
    EmergencyDisabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredEnginePackage {
    package_root: PathBuf,
    engine_version: String,
}

impl StoredEnginePackage {
    fn from_verified(package: &VerifiedEnginePackage) -> Self {
        Self {
            package_root: package.package_root.clone(),
            engine_version: package.manifest.engine_version.clone(),
        }
    }

    fn request(&self) -> EnginePackageRequest {
        EnginePackageRequest {
            package_root: self.package_root.clone(),
            expected_version: self.engine_version.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedEngineState {
    schema_version: u32,
    adapter_id: EngineAdapterId,
    active_package: Option<StoredEnginePackage>,
    previous_package: Option<StoredEnginePackage>,
    emergency_disabled: bool,
    emergency_reason: Option<String>,
    updated_at: DateTime<Utc>,
}

impl PersistedEngineState {
    fn empty(adapter_id: EngineAdapterId) -> Self {
        Self {
            schema_version: ENGINE_STATE_SCHEMA_VERSION,
            adapter_id,
            active_package: None,
            previous_package: None,
            emergency_disabled: false,
            emergency_reason: None,
            updated_at: Utc::now(),
        }
    }

    fn validate(&self, adapter_id: EngineAdapterId) -> Result<(), EngineError> {
        if self.schema_version != ENGINE_STATE_SCHEMA_VERSION || self.adapter_id != adapter_id {
            return Err(EngineError::InvalidPackage(
                "persisted engine state schema or adapter identity is invalid".to_owned(),
            ));
        }
        validate_emergency_change(self.emergency_disabled, self.emergency_reason.as_deref())?;
        if !self.emergency_disabled && self.emergency_reason.is_some() {
            return Err(EngineError::InvalidPackage(
                "enabled engine state cannot retain an emergency reason".to_owned(),
            ));
        }
        for package in [&self.active_package, &self.previous_package]
            .into_iter()
            .flatten()
        {
            if !package.package_root.is_absolute() || !valid_engine_version(&package.engine_version)
            {
                return Err(EngineError::InvalidPackage(
                    "persisted engine package reference is invalid".to_owned(),
                ));
            }
            ensure_clean_components(&package.package_root, true)?;
        }
        if self.active_package.is_none() && self.previous_package.is_some() {
            return Err(EngineError::InvalidPackage(
                "persisted rollback state requires an active package".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct EngineStateStore {
    root: PathBuf,
    state_path: PathBuf,
}

impl EngineStateStore {
    fn new(root: PathBuf, adapter_id: EngineAdapterId) -> Result<Self, EngineError> {
        if !root.is_absolute() {
            return Err(EngineError::UnsafePath(
                "engine state root must be absolute".to_owned(),
            ));
        }
        ensure_clean_components(&root, true)?;
        ensure_no_link_or_reparse(&root)?;
        if root.exists() {
            secure_existing_directory(&root)?;
        }
        let file_name = match adapter_id {
            EngineAdapterId::ControlledChromium => "controlled-chromium-state.json",
            EngineAdapterId::Camoufox => "camoufox-state.json",
            EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => {
                return Err(EngineError::InvalidPackage(
                    "stock adapters do not persist external package state".to_owned(),
                ));
            }
        };
        Ok(Self {
            state_path: root.join(file_name),
            root,
        })
    }

    fn load(&self, adapter_id: EngineAdapterId) -> Result<PersistedEngineState, EngineError> {
        match fs::symlink_metadata(&self.state_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PersistedEngineState::empty(adapter_id));
            }
            Err(error) => return Err(EngineError::Io(error)),
            Ok(metadata) if metadata_is_link_or_reparse(&metadata) => {
                return Err(EngineError::UnsafePath(
                    "persisted engine state cannot be a link or reparse point".to_owned(),
                ));
            }
            Ok(_) => {}
        }
        let state_path = secure_existing_file(&self.state_path)?;
        if fs::symlink_metadata(&state_path)?.len() > MAX_ENGINE_MANIFEST_BYTES {
            return Err(EngineError::InvalidPackage(
                "persisted engine state exceeds 64 KiB".to_owned(),
            ));
        }
        let state: PersistedEngineState = serde_json::from_slice(&fs::read(state_path)?)?;
        state.validate(adapter_id)?;
        Ok(state)
    }

    fn persist(&self, state: &PersistedEngineState) -> Result<(), EngineError> {
        state.validate(state.adapter_id)?;
        ensure_no_link_or_reparse(&self.root)?;
        fs::create_dir_all(&self.root)?;
        let canonical_root = secure_existing_directory(&self.root)?;
        let state_path =
            canonical_root.join(self.state_path.file_name().ok_or_else(|| {
                EngineError::UnsafePath("state path has no file name".to_owned())
            })?);
        match fs::symlink_metadata(&state_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EngineError::Io(error)),
            Ok(metadata) if metadata_is_link_or_reparse(&metadata) => {
                return Err(EngineError::UnsafePath(
                    "persisted engine state cannot be replaced through a link or reparse point"
                        .to_owned(),
                ));
            }
            Ok(_) => {
                secure_existing_file(&state_path)?;
            }
        }
        let temporary_path = canonical_root.join(format!(
            ".{}.{}.tmp",
            state_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("engine-state"),
            Uuid::new_v4().simple()
        ));
        let bytes = serde_json::to_vec_pretty(state)?;
        if bytes.len() as u64 > MAX_ENGINE_MANIFEST_BYTES {
            return Err(EngineError::InvalidPackage(
                "persisted engine state exceeds 64 KiB".to_owned(),
            ));
        }
        let result = (|| -> Result<(), EngineError> {
            let mut temporary = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)?;
            temporary.write_all(&bytes)?;
            temporary.sync_all()?;
            drop(temporary);
            atomic_replace_file(&temporary_path, &state_path)?;
            sync_directory(&canonical_root)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}

pub trait EngineAdapter: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;
    fn negotiate(&self, requested: &[EngineCapabilityId]) -> EngineNegotiation;
    fn install(
        &mut self,
        request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError>;
    fn update(
        &mut self,
        request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError>;
    fn launch_plan(&self, request: &EngineLaunchRequest) -> Result<EngineLaunchPlan, EngineError>;
    fn camoufox_host_launch_plan(
        &self,
        _request: &EngineLaunchRequest,
        _package: &VerifiedEnginePackage,
    ) -> Result<EngineLaunchPlan, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "this engine adapter has no Camoufox Host launch plan".to_owned(),
        ))
    }
    fn health(&self) -> EngineHealth;
    fn rollback(&mut self) -> Result<EngineMaintenanceReceipt, EngineError>;
    fn set_emergency_disabled(
        &mut self,
        disabled: bool,
        reason: Option<String>,
    ) -> Result<(), EngineError>;
    fn validate_identity_template(&self, template: &IdentityTemplate) -> Result<(), EngineError>;
    fn derive_identity_token(
        &self,
        context: &IdentityDerivationContext,
        deriver: &dyn IdentityTokenDeriver,
    ) -> Result<DerivedIdentityToken, EngineError>;
    fn control_plan(
        &self,
        session_id: Uuid,
        template: &IdentityTemplate,
        rules: &[SiteFallbackRule],
    ) -> Result<EngineControlPlan, EngineError>;
}

pub fn production_engine_adapter(
    config: &SiloEngineConfig,
    stock_browser: BrowserDescriptor,
) -> Result<Box<dyn EngineAdapter>, EngineError> {
    production_engine_adapter_for_silo(config, Some(stock_browser))
}

/// Builds an adapter without inventing a stock browser descriptor for a
/// managed Camoufox Silo. Stock adapters require `Some`; external adapters
/// deliberately ignore the descriptor and accept `None`.
pub fn production_engine_adapter_for_silo(
    config: &SiloEngineConfig,
    stock_browser: Option<BrowserDescriptor>,
) -> Result<Box<dyn EngineAdapter>, EngineError> {
    match config {
        SiloEngineConfig::Stock => Ok(Box::new(StockChromiumAdapter::new(
            stock_browser.ok_or_else(|| {
                EngineError::InvalidPackage(
                    "stock adapter requires a browser descriptor".to_owned(),
                )
            })?,
        ))),
        SiloEngineConfig::ControlledChromium { .. } => Ok(Box::new(
            ExternalPackageEngineAdapter::production_prototype(
                EngineAdapterId::ControlledChromium,
            )?,
        )),
        SiloEngineConfig::Camoufox { .. } => Ok(Box::new(
            ExternalPackageEngineAdapter::production_prototype(EngineAdapterId::Camoufox)?,
        )),
    }
}

pub struct StockChromiumAdapter {
    browser: BrowserDescriptor,
    emergency_disabled: bool,
    emergency_reason: Option<String>,
}

impl StockChromiumAdapter {
    pub fn new(browser: BrowserDescriptor) -> Self {
        Self {
            browser,
            emergency_disabled: false,
            emergency_reason: None,
        }
    }

    fn adapter_id(&self) -> EngineAdapterId {
        match self.browser.kind {
            BrowserKind::Chrome => EngineAdapterId::StockChrome,
            BrowserKind::Edge => EngineAdapterId::StockEdge,
        }
    }

    fn capability_catalog(&self) -> Vec<EngineCapabilityState> {
        EngineCapabilityId::ALL
            .into_iter()
            .map(|id| match id {
                EngineCapabilityId::ProfileIsolation => EngineCapabilityState::declared(
                    id,
                    EngineCapabilityAvailability::Supported,
                    "Stock Chromium supports an explicit, independent user-data-dir.",
                ),
                EngineCapabilityId::SiteFallback => EngineCapabilityState::declared(
                    id,
                    EngineCapabilityAvailability::Unavailable,
                    "Stock mode does not participate in controlled-engine site fallback; external launch failures remain fail-closed.",
                ),
                EngineCapabilityId::LaunchNetwork => EngineCapabilityState::declared(
                    id,
                    EngineCapabilityAvailability::Unavailable,
                    "Network launch configuration belongs to the existing Silo launcher, not this adapter.",
                ),
                EngineCapabilityId::TlsClientHello | EngineCapabilityId::Quic => {
                    EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Unavailable,
                        "Stock Chrome/Edge does not expose engine-level protocol control or direct protocol evidence.",
                    )
                }
                _ => EngineCapabilityState::declared(
                    id,
                    EngineCapabilityAvailability::Unavailable,
                    "Stock Chrome/Edge is observed as shipped and does not claim identity-signal control.",
                ),
            })
            .collect()
    }
}

impl EngineAdapter for StockChromiumAdapter {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            contract_version: ENGINE_CONTRACT_VERSION,
            id: self.adapter_id(),
            adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
            engine_version: self
                .browser
                .version
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
            channel: EngineChannel::Stable,
            browser_family: BrowserFamily::Chromium,
            platform: WINDOWS_X64_PLATFORM.to_owned(),
            externally_packaged: false,
            emergency_disabled: self.emergency_disabled,
        }
    }

    fn negotiate(&self, requested: &[EngineCapabilityId]) -> EngineNegotiation {
        negotiate_capabilities(self.descriptor(), self.capability_catalog(), requested)
    }

    fn install(
        &mut self,
        _request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "stock browsers are installed and updated by their vendor".to_owned(),
        ))
    }

    fn update(
        &mut self,
        _request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "stock browser updates are not managed by VeriSilo".to_owned(),
        ))
    }

    fn launch_plan(&self, request: &EngineLaunchRequest) -> Result<EngineLaunchPlan, EngineError> {
        ensure_not_disabled(self.emergency_disabled, self.emergency_reason.as_deref())?;
        if request.identity.is_some() || request.derived_token.is_some() {
            return Err(EngineError::CapabilityUnavailable(
                "stock Chrome/Edge cannot apply a VeriSilo identity template".to_owned(),
            ));
        }
        validate_fallback_rules(&request.fallback_rules)?;
        let executable_path = secure_existing_file(Path::new(&self.browser.executable_path))?;
        let profile_directory = validate_launch_profile_path(&request.profile_directory)?;
        let profile_argument = path_argument("--user-data-dir=", &profile_directory)?;
        let mut capabilities = self.capability_catalog();
        set_configured(&mut capabilities, EngineCapabilityId::ProfileIsolation)?;
        Ok(EngineLaunchPlan {
            adapter: self.descriptor(),
            transport: EngineTransport::Stock,
            executable_path,
            arguments: vec![
                profile_argument,
                "--no-first-run".to_owned(),
                "--no-default-browser-check".to_owned(),
                // Keep cloud account data from crossing otherwise-independent
                // stock Chrome/Edge profiles. OS-level SSO remains outside this
                // process-level boundary and is reported separately in the UI.
                "--disable-sync".to_owned(),
            ],
            profile_directory,
            shell: false,
            capabilities,
            identity_delivery: None,
            control: None,
            camoufox_host: None,
            package_verification: None,
        })
    }

    fn health(&self) -> EngineHealth {
        if self.emergency_disabled {
            return health(
                EngineHealthState::EmergencyDisabled,
                self.emergency_reason
                    .clone()
                    .unwrap_or_else(|| "Stock adapter was disabled explicitly.".to_owned()),
            );
        }
        match secure_existing_file(Path::new(&self.browser.executable_path)) {
            Ok(_) if self.browser.version.is_some() => health(
                EngineHealthState::Healthy,
                "Browser executable and version are present.",
            ),
            Ok(_) => health(
                EngineHealthState::Degraded,
                "Browser executable exists, but its version has not been observed.",
            ),
            Err(error) => health(EngineHealthState::Unavailable, error.to_string()),
        }
    }

    fn rollback(&mut self) -> Result<EngineMaintenanceReceipt, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "stock browser rollback is not managed by VeriSilo".to_owned(),
        ))
    }

    fn set_emergency_disabled(
        &mut self,
        disabled: bool,
        reason: Option<String>,
    ) -> Result<(), EngineError> {
        validate_emergency_change(disabled, reason.as_deref())?;
        self.emergency_disabled = disabled;
        self.emergency_reason = disabled.then(|| reason.unwrap_or_default());
        Ok(())
    }

    fn validate_identity_template(&self, _template: &IdentityTemplate) -> Result<(), EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "stock Chrome/Edge does not apply identity templates".to_owned(),
        ))
    }

    fn derive_identity_token(
        &self,
        _context: &IdentityDerivationContext,
        _deriver: &dyn IdentityTokenDeriver,
    ) -> Result<DerivedIdentityToken, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "stock Chrome/Edge does not consume identity session tokens".to_owned(),
        ))
    }

    fn control_plan(
        &self,
        _session_id: Uuid,
        _template: &IdentityTemplate,
        _rules: &[SiteFallbackRule],
    ) -> Result<EngineControlPlan, EngineError> {
        Err(EngineError::CapabilityUnavailable(
            "stock Chrome/Edge has no VeriSilo identity control cycle".to_owned(),
        ))
    }
}

pub struct ExternalPackageEngineAdapter {
    id: EngineAdapterId,
    verifier: Arc<dyn EnginePackageVerifier>,
    state_store: Option<EngineStateStore>,
    active_package: Option<StoredEnginePackage>,
    previous_package: Option<StoredEnginePackage>,
    emergency_disabled: bool,
    emergency_reason: Option<String>,
}

impl ExternalPackageEngineAdapter {
    pub fn production_prototype(id: EngineAdapterId) -> Result<Self, EngineError> {
        let verifier = Arc::new(WindowsProductionEnginePackageVerifier::from_embedded_policy()?);
        let state_store = production_state_store(id)?;
        let state = state_store
            .as_ref()
            .map(|store| store.load(id))
            .transpose()?
            .unwrap_or_else(|| PersistedEngineState::empty(id));
        Self::new(id, verifier, state_store, state)
    }

    pub fn with_verifier(
        id: EngineAdapterId,
        verifier: Arc<dyn EnginePackageVerifier>,
    ) -> Result<Self, EngineError> {
        Self::new(id, verifier, None, PersistedEngineState::empty(id))
    }

    pub fn with_verifier_and_state(
        id: EngineAdapterId,
        verifier: Arc<dyn EnginePackageVerifier>,
        state_root: PathBuf,
    ) -> Result<Self, EngineError> {
        let state_store = EngineStateStore::new(state_root, id)?;
        let state = state_store.load(id)?;
        Self::new(id, verifier, Some(state_store), state)
    }

    fn new(
        id: EngineAdapterId,
        verifier: Arc<dyn EnginePackageVerifier>,
        state_store: Option<EngineStateStore>,
        state: PersistedEngineState,
    ) -> Result<Self, EngineError> {
        if !matches!(
            id,
            EngineAdapterId::ControlledChromium | EngineAdapterId::Camoufox
        ) {
            return Err(EngineError::InvalidPackage(
                "external package adapter supports only controlled-chromium or camoufox".to_owned(),
            ));
        }
        state.validate(id)?;
        Ok(Self {
            id,
            verifier,
            state_store,
            active_package: state.active_package,
            previous_package: state.previous_package,
            emergency_disabled: state.emergency_disabled,
            emergency_reason: state.emergency_reason,
        })
    }

    fn family(&self) -> BrowserFamily {
        match self.id {
            EngineAdapterId::ControlledChromium => BrowserFamily::Chromium,
            EngineAdapterId::Camoufox => BrowserFamily::Firefox,
            EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => unreachable!(),
        }
    }

    fn capability_catalog(
        &self,
        package: Option<&VerifiedEnginePackage>,
    ) -> Vec<EngineCapabilityState> {
        let declared = package
            .map(|package| {
                package
                    .manifest
                    .capabilities
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        EngineCapabilityId::ALL
            .into_iter()
            .map(|id| {
                if id == EngineCapabilityId::ProfileIsolation {
                    return EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Supported,
                        "The adapter always launches an explicit per-Silo profile directory.",
                    );
                }
                if self.id == EngineAdapterId::Camoufox
                    && id == EngineCapabilityId::SiteFallback
                {
                    return EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Unavailable,
                        "Camoufox Host v1 has no site fallback implementation; fallback rules remain empty.",
                    );
                }
                if matches!(id, EngineCapabilityId::TlsClientHello | EngineCapabilityId::Quic) {
                    return EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Unavailable,
                        "No signed controlled build plus direct ClientHello/QUIC observation evidence is configured.",
                    );
                }
                if id == EngineCapabilityId::LaunchNetwork {
                    return EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Unavailable,
                        "Network routing remains owned by the Silo launcher and requires separate runtime evidence.",
                    );
                }
                if package.is_none() {
                    return EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Unavailable,
                        "No verified local engine package is active.",
                    );
                }
                if declared.contains(&id) && external_capability_allowed(self.id, id) {
                    EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Experimental,
                        "The verified package declares this experimental surface; runtime verification is still required.",
                    )
                } else {
                    EngineCapabilityState::declared(
                        id,
                        EngineCapabilityAvailability::Unavailable,
                        "The package does not declare this capability, or the adapter forbids it.",
                    )
                }
            })
            .collect()
    }

    fn verify_package(
        &self,
        request: &EnginePackageRequest,
    ) -> Result<VerifiedEnginePackage, EngineError> {
        load_and_verify_package(self.id, request, self.verifier.as_ref())
    }

    fn active_verified_package(&self) -> Result<VerifiedEnginePackage, EngineError> {
        let package = self.active_package.as_ref().ok_or_else(|| {
            EngineError::CapabilityUnavailable("no verified engine package is installed".to_owned())
        })?;
        self.verify_package(&package.request())
    }

    /// Runs the signed Camoufox Host's bounded provisioning mode. The seed is
    /// sent only through the child's length-prefixed stdin frame; it is never
    /// placed in command-line arguments, persisted by this adapter, or logged.
    pub fn provision_camoufox_artifact(
        &self,
        roots: &CamoufoxHostRoots,
        preset: &str,
        seed: &[u8; 32],
        proxy_server: Option<&str>,
    ) -> Result<CamoufoxProvisionResult, EngineError> {
        if self.id != EngineAdapterId::Camoufox {
            return Err(EngineError::CapabilityUnavailable(
                "only the Camoufox adapter can provision a Camoufox Artifact".to_owned(),
            ));
        }
        if !matches!(
            preset,
            "balanced-en-us" | "balanced-zh-cn" | "balanced-de-de" | "match-fixed-proxy"
        ) {
            return Err(EngineError::InvalidIdentityTemplate(
                "managed identity preset is not one of the four fixed values".to_owned(),
            ));
        }
        if preset == "match-fixed-proxy" && proxy_server.is_none() {
            return Err(EngineError::CapabilityUnavailable(
                "match-fixed-proxy provisioning requires a proxy endpoint".to_owned(),
            ));
        }
        if preset != "match-fixed-proxy" && proxy_server.is_some() {
            return Err(EngineError::InvalidPackage(
                "a proxy endpoint is accepted only by match-fixed-proxy provisioning".to_owned(),
            ));
        }
        roots.validate()?;
        for root in [&roots.artifact_root, &roots.profile_root, &roots.state_root] {
            fs::create_dir_all(root)?;
        }
        let package = self.active_verified_package()?;
        let browser_tree = package.browser_tree_manifest().ok_or_else(|| {
            EngineError::InvalidPackage(
                "Camoufox package is missing its browser tree manifest binding".to_owned(),
            )
        })?;
        let browser_tree_path = secure_package_member(
            &package.package_root,
            Path::new(&browser_tree.relative_path),
            PathKind::File,
        )?;
        let asset_lock_path = secure_package_member(
            &package.package_root,
            Path::new("runtime-asset-lock.json"),
            PathKind::File,
        )?;
        let browser_root = secure_package_member(
            &package.package_root,
            Path::new("browser"),
            PathKind::Directory,
        )?;
        let package_root = secure_existing_directory(&package.package_root)?;
        let argument_values = [
            ("--artifact-root", roots.artifact_root.as_path()),
            ("--profile-root", roots.profile_root.as_path()),
            ("--state-root", roots.state_root.as_path()),
            ("--tree-manifest", browser_tree_path.as_path()),
            ("--asset-lock", asset_lock_path.as_path()),
            ("--browser-root", browser_root.as_path()),
            ("--package-root", package_root.as_path()),
        ];
        let mut command = Command::new(&package.executable_path);
        for (flag, value) in argument_values {
            command.arg(flag).arg(value);
        }
        command.arg("--provision-artifact");
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
        Self::configure_camoufox_host_process(&mut command);
        let mut child = command.spawn()?;
        let request = match proxy_server {
            Some(proxy_server) => serde_json::json!({
                "seed": BASE64_STANDARD.encode(seed),
                "preset": preset,
                "proxyServer": proxy_server,
            }),
            None => serde_json::json!({
                "seed": BASE64_STANDARD.encode(seed),
                "preset": preset,
            }),
        };
        let payload = serde_json::to_vec(&request)?;
        if payload.is_empty() || payload.len() > 4 * 1024 {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EngineError::InvalidIdentityTemplate(
                "Camoufox provisioning request exceeds its 4 KiB bound".to_owned(),
            ));
        }
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::InvalidBootstrap("Host stdin is unavailable".to_owned()))
            .and_then(|mut stdin| {
                stdin.write_all(&(payload.len() as u32).to_be_bytes())?;
                stdin.write_all(&payload)?;
                stdin.flush()?;
                Ok(())
            });
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let deadline = Instant::now() + CAMOUFOX_PROVISION_TIMEOUT;
        loop {
            match child.try_wait()? {
                Some(_) => break,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(EngineError::VerificationUnavailable(
                        "Camoufox Artifact provisioning exceeded its 120 second bound".to_owned(),
                    ));
                }
                None => thread::sleep(StdDuration::from_millis(25)),
            }
        }
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .ok_or_else(|| EngineError::InvalidBootstrap("Host stdout is unavailable".to_owned()))?
            .read_to_end(&mut output)?;
        if output.len() > 8 * 1024 {
            return Err(EngineError::InvalidBootstrap(
                "Camoufox provisioning response exceeds its bound".to_owned(),
            ));
        }
        let response: CamoufoxProvisionResponse = strict_json_from_slice(&output)?;
        if !response.ok {
            return Err(EngineError::VerificationUnavailable(
                response
                    .error
                    .map(|error| error.message)
                    .unwrap_or_else(|| "Camoufox Artifact provisioning failed".to_owned()),
            ));
        }
        let result = response.result.ok_or_else(|| {
            EngineError::InvalidBootstrap(
                "Camoufox provisioning succeeded without an Artifact result".to_owned(),
            )
        })?;
        if !valid_artifact_id(&result.artifact_id)
            || !is_lower_hex(&result.artifact_file_sha256, 64)
            || !matches!(
                result.schema.as_str(),
                CAMOUFOX_ARTIFACT_SCHEMA_V5 | CAMOUFOX_ARTIFACT_SCHEMA_V6
            )
        {
            return Err(EngineError::InvalidIdentityTemplate(
                "Camoufox provisioning returned an invalid Artifact binding".to_owned(),
            ));
        }
        let artifact_path = secure_package_member(
            &roots.artifact_root,
            Path::new(&format!("{}.json", result.artifact_id)),
            PathKind::File,
        )?;
        let raw = fs::read(&artifact_path)?;
        if raw.len() > 4 * 1024 * 1024 || sha256_hex_bytes(&raw) != result.artifact_file_sha256 {
            return Err(EngineError::VerificationUnavailable(
                "Camoufox provisioning Artifact bytes do not match its returned SHA-256".to_owned(),
            ));
        }
        let sidecar_path = secure_package_member(
            &roots.artifact_root,
            Path::new(&format!("{}.json.sha256", result.artifact_id)),
            PathKind::File,
        )?;
        let expected_sidecar = format!(
            "{}  {}.json\n",
            result.artifact_file_sha256, result.artifact_id
        );
        if fs::read(&sidecar_path)? != expected_sidecar.as_bytes() {
            return Err(EngineError::VerificationUnavailable(
                "Camoufox provisioning Artifact sidecar is missing or does not match the raw bytes"
                    .to_owned(),
            ));
        }
        let value: serde_json::Value = strict_json_from_slice(&raw)?;
        let object = value.as_object().ok_or_else(|| {
            EngineError::InvalidIdentityTemplate(
                "Camoufox Artifact must be a JSON object".to_owned(),
            )
        })?;
        if object.get("artifactId").and_then(serde_json::Value::as_str)
            != Some(result.artifact_id.as_str())
            || object.get("schema").and_then(serde_json::Value::as_str)
                != Some(result.schema.as_str())
        {
            return Err(EngineError::InvalidIdentityTemplate(
                "Camoufox Artifact identity fields do not match the Host result".to_owned(),
            ));
        }
        let raw_json = String::from_utf8(raw).map_err(|_| {
            EngineError::InvalidIdentityTemplate(
                "Camoufox Artifact JSON is not valid UTF-8".to_owned(),
            )
        })?;
        Ok(CamoufoxProvisionResult {
            artifact_id: result.artifact_id,
            artifact_file_sha256: result.artifact_file_sha256,
            schema: result.schema,
            raw_json,
        })
    }

    /// Keep the bundled Host on the PyInstaller console bootloader so its
    /// length-prefixed provisioning response remains on redirected stdout.
    /// Windows only needs the console hidden; the stdio pipes above stay
    /// intact for both provisioning and the JSONL runtime protocol.
    #[cfg(target_os = "windows")]
    const CAMOUFOX_HOST_PROCESS_CREATION_FLAGS: u32 = 0x0800_0000; // CREATE_NO_WINDOW

    #[cfg(target_os = "windows")]
    fn configure_camoufox_host_process(command: &mut Command) {
        use std::os::windows::process::CommandExt;

        command.creation_flags(Self::CAMOUFOX_HOST_PROCESS_CREATION_FLAGS);
    }

    #[cfg(not(target_os = "windows"))]
    fn configure_camoufox_host_process(_command: &mut Command) {}

    /// Verifies and activates the package shipped in the application
    /// resources. Same-version resources are reloaded after verification;
    /// newer resources replace the active package, while downgrades fail
    /// closed and require the explicit rollback path.
    pub fn ensure_builtin_package(
        &mut self,
        package_root: &Path,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        let package_root = secure_existing_directory(package_root)?;
        let manifest_path = secure_package_member(
            &package_root,
            Path::new(ENGINE_PACKAGE_MANIFEST),
            PathKind::File,
        )?;
        let manifest_bytes = fs::read(&manifest_path)?;
        if manifest_bytes.len() as u64 > MAX_ENGINE_MANIFEST_BYTES {
            return Err(EngineError::InvalidPackage(
                "engine manifest exceeds 64 KiB".to_owned(),
            ));
        }
        let manifest: EnginePackageManifest = strict_json_from_slice(&manifest_bytes)?;
        if manifest.engine_id != self.id {
            return Err(EngineError::InvalidPackage(
                "built-in package engine identity does not match the adapter".to_owned(),
            ));
        }
        let request = EnginePackageRequest {
            package_root,
            expected_version: manifest.engine_version,
        };
        let package = self.verify_package(&request)?;
        let installed = StoredEnginePackage::from_verified(&package);
        let previous = self.active_package.clone();
        if let Some(current) = &previous {
            self.verify_package(&current.request())?;
            match compare_engine_versions(&installed.engine_version, &current.engine_version) {
                Some(std::cmp::Ordering::Less) => {
                    return Err(EngineError::InvalidPackage(
                        "built-in package is older than the active verified package; use rollback explicitly"
                            .to_owned(),
                    ));
                }
                Some(std::cmp::Ordering::Equal)
                    if current.package_root == installed.package_root =>
                {
                    return Ok(maintenance_receipt(
                        EngineMaintenanceAction::Install,
                        &package,
                    ));
                }
                Some(std::cmp::Ordering::Equal) => {
                    self.persist_state(
                        Some(installed.clone()),
                        None,
                        self.emergency_disabled,
                        self.emergency_reason.clone(),
                    )?;
                    self.active_package = Some(installed);
                    self.previous_package = None;
                    return Ok(maintenance_receipt(
                        EngineMaintenanceAction::Update,
                        &package,
                    ));
                }
                _ => {}
            }
        }
        let action = if previous.is_some() {
            EngineMaintenanceAction::Update
        } else {
            EngineMaintenanceAction::Install
        };
        let previous_for_state = previous.clone();
        self.persist_state(
            Some(installed.clone()),
            previous,
            self.emergency_disabled,
            self.emergency_reason.clone(),
        )?;
        self.active_package = Some(installed);
        self.previous_package = previous_for_state;
        Ok(maintenance_receipt(action, &package))
    }

    fn persist_state(
        &self,
        active_package: Option<StoredEnginePackage>,
        previous_package: Option<StoredEnginePackage>,
        emergency_disabled: bool,
        emergency_reason: Option<String>,
    ) -> Result<(), EngineError> {
        if let Some(store) = &self.state_store {
            store.persist(&PersistedEngineState {
                schema_version: ENGINE_STATE_SCHEMA_VERSION,
                adapter_id: self.id,
                active_package,
                previous_package,
                emergency_disabled,
                emergency_reason,
                updated_at: Utc::now(),
            })?;
        }
        Ok(())
    }
}

impl EngineAdapter for ExternalPackageEngineAdapter {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            contract_version: ENGINE_CONTRACT_VERSION,
            id: self.id,
            adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
            engine_version: self
                .active_package
                .as_ref()
                .map(|package| package.engine_version.clone())
                .unwrap_or_else(|| "not-installed".to_owned()),
            channel: EngineChannel::Experimental,
            browser_family: self.family(),
            platform: WINDOWS_X64_PLATFORM.to_owned(),
            externally_packaged: true,
            emergency_disabled: self.emergency_disabled,
        }
    }

    fn negotiate(&self, requested: &[EngineCapabilityId]) -> EngineNegotiation {
        let verified = self.active_verified_package().ok();
        negotiate_capabilities(
            self.descriptor(),
            self.capability_catalog(verified.as_ref()),
            requested,
        )
    }

    fn install(
        &mut self,
        request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        ensure_not_disabled(self.emergency_disabled, self.emergency_reason.as_deref())?;
        if self.active_package.is_some() {
            return Err(EngineError::InvalidPackage(
                "an engine is already installed; use update for a new version".to_owned(),
            ));
        }
        let package = self.verify_package(request)?;
        let receipt = maintenance_receipt(EngineMaintenanceAction::Install, &package);
        let installed = StoredEnginePackage::from_verified(&package);
        self.persist_state(
            Some(installed.clone()),
            None,
            self.emergency_disabled,
            self.emergency_reason.clone(),
        )?;
        self.active_package = Some(installed);
        self.previous_package = None;
        Ok(receipt)
    }

    fn update(
        &mut self,
        request: &EnginePackageRequest,
    ) -> Result<EngineMaintenanceReceipt, EngineError> {
        ensure_not_disabled(self.emergency_disabled, self.emergency_reason.as_deref())?;
        let current = self.active_package.clone().ok_or_else(|| {
            EngineError::CapabilityUnavailable(
                "install an engine package before updating".to_owned(),
            )
        })?;
        self.verify_package(&current.request())?;
        if compare_engine_versions(&request.expected_version, &current.engine_version)
            != Some(std::cmp::Ordering::Greater)
        {
            return Err(EngineError::InvalidPackage(
                "update requires a strictly newer pinned semantic version; use rollback to downgrade"
                    .to_owned(),
            ));
        }
        let package = self.verify_package(request)?;
        let receipt = maintenance_receipt(EngineMaintenanceAction::Update, &package);
        let installed = StoredEnginePackage::from_verified(&package);
        self.persist_state(
            Some(installed.clone()),
            Some(current.clone()),
            self.emergency_disabled,
            self.emergency_reason.clone(),
        )?;
        self.active_package = Some(installed);
        self.previous_package = Some(current);
        Ok(receipt)
    }

    fn launch_plan(&self, request: &EngineLaunchRequest) -> Result<EngineLaunchPlan, EngineError> {
        ensure_not_disabled(self.emergency_disabled, self.emergency_reason.as_deref())?;
        let package = self.active_verified_package()?;
        if self.id == EngineAdapterId::Camoufox {
            // Camoufox identity comes from the Vault Artifact binding. It is
            // deliberately not accepted as a template or sent over stdin.
            return self.camoufox_host_launch_plan(request, &package);
        }
        let identity = request.identity.as_ref().ok_or_else(|| {
            EngineError::InvalidIdentityTemplate(
                "external controlled engines require an explicit constrained identity template"
                    .to_owned(),
            )
        })?;
        self.validate_identity_template(identity)?;
        validate_fallback_rules(&request.fallback_rules)?;

        let token = request.derived_token.as_ref().ok_or_else(|| {
            EngineError::VerificationUnavailable(
                "a short-lived session token must be derived before launch".to_owned(),
            )
        })?;
        validate_derived_token(token, Utc::now())?;
        let profile_directory = validate_launch_profile_path(&request.profile_directory)?;
        let mut capabilities = self.capability_catalog(Some(&package));
        set_configured(&mut capabilities, EngineCapabilityId::ProfileIsolation)?;
        for id in identity_capabilities() {
            if capabilities.iter().any(|capability| {
                capability.id == id
                    && capability.availability != EngineCapabilityAvailability::Unavailable
            }) {
                set_configured(&mut capabilities, id)?;
            }
        }
        let control = self.control_plan(request.session_id, identity, &request.fallback_rules)?;
        let arguments = match self.id {
            EngineAdapterId::ControlledChromium => vec![
                path_argument("--user-data-dir=", &profile_directory)?,
                "--no-first-run".to_owned(),
                "--no-default-browser-check".to_owned(),
                "--verisilo-control-channel=stdio-v1".to_owned(),
            ],
            EngineAdapterId::Camoufox => vec![
                "--profile".to_owned(),
                path_string(&profile_directory)?,
                "--no-remote".to_owned(),
                "--verisilo-control-channel=stdio-v1".to_owned(),
            ],
            EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => unreachable!(),
        };
        Ok(EngineLaunchPlan {
            adapter: self.descriptor(),
            transport: EngineTransport::NativeBootstrapV1,
            executable_path: package.executable_path,
            arguments,
            profile_directory,
            shell: false,
            capabilities,
            identity_delivery: Some(IdentityDeliveryRequirement {
                token_id: token.token_id,
                delivery: IdentityDelivery::SecureStdinBeforeNavigation,
                expires_at: token.expires_at,
            }),
            control: Some(control),
            camoufox_host: None,
            package_verification: Some(EngineLaunchPackageVerification {
                verifier_id: package.verification.verifier_id,
                artifact_sha256: package.manifest.artifact_sha256,
                digest_verified: package.verification.digest_verified,
                signature_verified: package.verification.signature_verified,
                package_manifest_sha256: package.verification.package_manifest_sha256,
                package_tree_sha256: package.verification.package_tree_sha256,
                host_sha256: package.verification.host_sha256,
                signer_certificate_sha256: package.verification.signer_certificate_sha256,
                engine_revision: package.verification.engine_revision,
                verified_at: package.verification.verified_at,
            }),
        })
    }

    fn camoufox_host_launch_plan(
        &self,
        request: &EngineLaunchRequest,
        package: &VerifiedEnginePackage,
    ) -> Result<EngineLaunchPlan, EngineError> {
        let binding = request
            .camoufox_artifact_binding
            .as_ref()
            .ok_or_else(|| {
                EngineError::InvalidIdentityTemplate(
                    "identity_artifact_unavailable: Camoufox launch requires an explicit Artifact ID and raw SHA binding"
                        .to_owned(),
                )
        })?;
        binding.validate()?;
        if request.derived_token.is_some() {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox Host v1 does not accept a Vault-derived token".to_owned(),
            ));
        }
        if !matches!(
            &request.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            } | NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                ..
            }
        ) {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox Host v1 only supports Direct(false) or required FixedProxy HTTP/SOCKS5 profiles"
                    .to_owned(),
            ));
        }
        if matches!(
            &request.network_profile,
            NetworkProfile::FixedProxy {
                proxy_required: true,
                ..
            }
        ) && binding.schema != CAMOUFOX_ARTIFACT_SCHEMA_V6
        {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox required FixedProxy launches require an Artifact/Policy v6 network-bound binding"
                    .to_owned(),
            ));
        }
        if !request.fallback_rules.is_empty() {
            return Err(EngineError::CapabilityUnavailable(
                "Camoufox Host v1 has no site fallback implementation; fallback rules must be empty"
                    .to_owned(),
            ));
        }
        let silo_id = request.silo_id.ok_or_else(|| {
            EngineError::InvalidIdentityTemplate(
                "Camoufox Host launch requires the owning Silo ID to derive profileId".to_owned(),
            )
        })?;
        let roots = request.camoufox_roots.as_ref().ok_or_else(|| {
            EngineError::UnsafePath(
                "Camoufox Host roots must be supplied by the desktop-owned root resolver"
                    .to_owned(),
            )
        })?;
        roots.validate()?;
        let entrypoint = package.entrypoint().ok_or_else(|| {
            EngineError::InvalidPackage(
                "Camoufox package is missing its v3 Host entrypoint binding".to_owned(),
            )
        })?;
        let browser_tree_manifest = package.browser_tree_manifest().ok_or_else(|| {
            EngineError::InvalidPackage(
                "Camoufox package is missing its v3 browser tree manifest binding".to_owned(),
            )
        })?;
        let browser_tree_manifest_path = secure_package_member(
            &package.package_root,
            Path::new(&browser_tree_manifest.relative_path),
            PathKind::File,
        )?;
        let asset_lock_path = secure_package_member(
            &package.package_root,
            Path::new("runtime-asset-lock.json"),
            PathKind::File,
        )?;
        let browser_root = secure_package_member(
            &package.package_root,
            Path::new("browser"),
            PathKind::Directory,
        )?;
        let package_root = secure_existing_directory(&package.package_root)?;
        let profile_id = camoufox_profile_id(silo_id);
        let profile_directory = roots.profile_root.join(&profile_id);
        validate_launch_profile_path(&profile_directory)?;
        let mut capabilities = self.capability_catalog(Some(package));
        set_configured(&mut capabilities, EngineCapabilityId::ProfileIsolation)?;
        for id in identity_capabilities() {
            if capabilities.iter().any(|capability| {
                capability.id == id
                    && capability.availability != EngineCapabilityAvailability::Unavailable
            }) {
                set_configured(&mut capabilities, id)?;
            }
        }
        let arguments = vec![
            "--artifact-root".to_owned(),
            path_string(&roots.artifact_root)?,
            "--profile-root".to_owned(),
            path_string(&roots.profile_root)?,
            "--state-root".to_owned(),
            path_string(&roots.state_root)?,
            "--package-root".to_owned(),
            path_string(&package_root)?,
            "--asset-lock".to_owned(),
            path_string(&asset_lock_path)?,
            "--browser-root".to_owned(),
            path_string(&browser_root)?,
            "--tree-manifest".to_owned(),
            path_string(&browser_tree_manifest_path)?,
        ];
        Ok(EngineLaunchPlan {
            adapter: self.descriptor(),
            transport: EngineTransport::CamoufoxHostJsonlV1,
            executable_path: package.executable_path.clone(),
            arguments,
            profile_directory,
            shell: false,
            capabilities,
            identity_delivery: None,
            control: None,
            camoufox_host: Some(CamoufoxHostLaunch {
                protocol: entrypoint.protocol.clone(),
                host_version: package.host_version()?,
                platform: package.manifest.platform.clone(),
                artifact_id: binding.artifact_id.clone(),
                artifact_file_sha256: binding.artifact_file_sha256.clone(),
                profile_id,
                browser_release: package.browser_release()?,
                browser_asset_sha256: package.browser_asset_sha256()?,
                browser_tree_manifest_path,
                browser_tree_manifest_sha256: browser_tree_manifest.sha256.clone(),
                browser_proxy_server: None,
            }),
            package_verification: Some(EngineLaunchPackageVerification {
                verifier_id: package.verification.verifier_id.clone(),
                artifact_sha256: package.manifest.artifact_sha256.clone(),
                digest_verified: package.verification.digest_verified,
                signature_verified: package.verification.signature_verified,
                package_manifest_sha256: package.verification.package_manifest_sha256.clone(),
                package_tree_sha256: package.verification.package_tree_sha256.clone(),
                host_sha256: package.verification.host_sha256.clone(),
                signer_certificate_sha256: package.verification.signer_certificate_sha256.clone(),
                engine_revision: package.verification.engine_revision.clone(),
                verified_at: package.verification.verified_at,
            }),
        })
    }

    fn health(&self) -> EngineHealth {
        if self.emergency_disabled {
            return health(
                EngineHealthState::EmergencyDisabled,
                self.emergency_reason
                    .clone()
                    .unwrap_or_else(|| "Engine was emergency-disabled.".to_owned()),
            );
        }
        match self.active_verified_package() {
            Ok(_) => health(
                EngineHealthState::Healthy,
                "Local package paths, locked version, digest, and signature were reverified.",
            ),
            Err(error) => health(EngineHealthState::Unavailable, error.to_string()),
        }
    }

    fn rollback(&mut self) -> Result<EngineMaintenanceReceipt, EngineError> {
        ensure_not_disabled(self.emergency_disabled, self.emergency_reason.as_deref())?;
        let active = self.active_package.clone().ok_or_else(|| {
            EngineError::CapabilityUnavailable("no active package is installed".to_owned())
        })?;
        self.verify_package(&active.request())?;
        let previous = self.previous_package.clone().ok_or_else(|| {
            EngineError::CapabilityUnavailable(
                "no verified rollback package is available".to_owned(),
            )
        })?;
        let reverified = self.verify_package(&previous.request())?;
        let receipt = maintenance_receipt(EngineMaintenanceAction::Rollback, &reverified);
        self.persist_state(
            Some(previous.clone()),
            Some(active.clone()),
            self.emergency_disabled,
            self.emergency_reason.clone(),
        )?;
        self.active_package = Some(previous);
        self.previous_package = Some(active);
        Ok(receipt)
    }

    fn set_emergency_disabled(
        &mut self,
        disabled: bool,
        reason: Option<String>,
    ) -> Result<(), EngineError> {
        validate_emergency_change(disabled, reason.as_deref())?;
        if !disabled && self.active_package.is_some() {
            self.active_verified_package()?;
        }
        let emergency_reason = disabled.then(|| reason.unwrap_or_default());
        self.persist_state(
            self.active_package.clone(),
            self.previous_package.clone(),
            disabled,
            emergency_reason.clone(),
        )?;
        self.emergency_disabled = disabled;
        self.emergency_reason = emergency_reason;
        Ok(())
    }

    fn validate_identity_template(&self, template: &IdentityTemplate) -> Result<(), EngineError> {
        let package = self.active_verified_package()?;
        let package_major = version_major(&package.manifest.engine_version);
        validate_identity_template(template, self.family(), package_major)?;
        let declared = package
            .manifest
            .capabilities
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut required = [
            EngineCapabilityId::IdentityTemplate,
            EngineCapabilityId::UaUaCh,
            EngineCapabilityId::LanguageTimezone,
            EngineCapabilityId::Screen,
            EngineCapabilityId::Fonts,
            EngineCapabilityId::MediaDevices,
            EngineCapabilityId::RequestHeaders,
            EngineCapabilityId::Window,
            EngineCapabilityId::Iframe,
            EngineCapabilityId::DedicatedWorker,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        if self.id == EngineAdapterId::ControlledChromium {
            required.insert(EngineCapabilityId::SiteFallback);
        }
        if template.render.canvas != CanvasMode::Native {
            required.insert(EngineCapabilityId::Canvas);
        }
        if template.render.web_gl_vendor.is_some() {
            required.insert(EngineCapabilityId::Webgl);
        }
        if !required.is_subset(&declared) {
            return Err(EngineError::CapabilityUnavailable(
                "verified package does not declare every surface required by the identity template"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    fn derive_identity_token(
        &self,
        context: &IdentityDerivationContext,
        deriver: &dyn IdentityTokenDeriver,
    ) -> Result<DerivedIdentityToken, EngineError> {
        ensure_not_disabled(self.emergency_disabled, self.emergency_reason.as_deref())?;
        validate_derivation_context(context, Utc::now())?;
        let token = deriver.derive_session_token(context)?;
        if token.expires_at != context.expires_at {
            return Err(EngineError::VerificationUnavailable(
                "derived token expiry does not match the requested session scope".to_owned(),
            ));
        }
        validate_derived_token(&token, Utc::now())?;
        Ok(token)
    }

    fn control_plan(
        &self,
        session_id: Uuid,
        template: &IdentityTemplate,
        rules: &[SiteFallbackRule],
    ) -> Result<EngineControlPlan, EngineError> {
        self.validate_identity_template(template)?;
        validate_fallback_rules(rules)?;
        let package = self.active_verified_package()?;
        let mut capabilities = self.capability_catalog(Some(&package));
        for rule in rules {
            if rule.disable_capabilities.iter().any(|id| {
                !capabilities.iter().any(|capability| {
                    capability.id == *id
                        && capability.availability != EngineCapabilityAvailability::Unavailable
                })
            }) {
                return Err(EngineError::CapabilityUnavailable(
                    "site fallback cannot target a capability unavailable in the verified package"
                        .to_owned(),
                ));
            }
        }
        for capability in &mut capabilities {
            if capability.availability != EngineCapabilityAvailability::Unavailable
                && (capability.id == EngineCapabilityId::ProfileIsolation
                    || identity_capabilities().contains(&capability.id))
            {
                capability.transition(
                    EngineCapabilityOperation::Configured,
                    Vec::new(),
                    Utc::now(),
                )?;
            }
        }
        Ok(EngineControlPlan {
            session_id,
            template_id: template.template_id,
            phases: [
                EngineControlPhase::Observe,
                EngineControlPhase::Apply,
                EngineControlPhase::Verify,
                EngineControlPhase::Restore,
            ],
            capabilities,
            site_fallback: SiteFallbackPolicy {
                default_action: SiteFallbackAction::RestoreExperimentalControls,
                rules: rules.to_vec(),
            },
        })
    }
}

fn negotiate_capabilities(
    adapter: EngineDescriptor,
    capabilities: Vec<EngineCapabilityState>,
    requested: &[EngineCapabilityId],
) -> EngineNegotiation {
    let requested = requested.iter().copied().collect::<BTreeSet<_>>();
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for id in requested {
        if capabilities.iter().any(|capability| {
            capability.id == id
                && capability.availability != EngineCapabilityAvailability::Unavailable
        }) {
            accepted.push(id);
        } else {
            rejected.push(id);
        }
    }
    EngineNegotiation {
        adapter,
        capabilities,
        accepted,
        rejected,
    }
}

fn external_capability_allowed(adapter: EngineAdapterId, id: EngineCapabilityId) -> bool {
    let common = matches!(
        id,
        EngineCapabilityId::IdentityTemplate
            | EngineCapabilityId::UaUaCh
            | EngineCapabilityId::LanguageTimezone
            | EngineCapabilityId::Screen
            | EngineCapabilityId::Canvas
            | EngineCapabilityId::Webgl
            | EngineCapabilityId::Fonts
            | EngineCapabilityId::MediaDevices
            | EngineCapabilityId::RequestHeaders
            | EngineCapabilityId::Window
            | EngineCapabilityId::Iframe
            | EngineCapabilityId::DedicatedWorker
    );
    (common
        && matches!(
            adapter,
            EngineAdapterId::ControlledChromium | EngineAdapterId::Camoufox
        ))
        || (id == EngineCapabilityId::SiteFallback
            && adapter == EngineAdapterId::ControlledChromium)
}

fn identity_capabilities() -> [EngineCapabilityId; 12] {
    [
        EngineCapabilityId::IdentityTemplate,
        EngineCapabilityId::UaUaCh,
        EngineCapabilityId::LanguageTimezone,
        EngineCapabilityId::Screen,
        EngineCapabilityId::Canvas,
        EngineCapabilityId::Webgl,
        EngineCapabilityId::Fonts,
        EngineCapabilityId::MediaDevices,
        EngineCapabilityId::RequestHeaders,
        EngineCapabilityId::Window,
        EngineCapabilityId::Iframe,
        EngineCapabilityId::DedicatedWorker,
    ]
}

fn set_configured(
    capabilities: &mut [EngineCapabilityState],
    id: EngineCapabilityId,
) -> Result<(), EngineError> {
    let capability = capabilities
        .iter_mut()
        .find(|capability| capability.id == id)
        .ok_or_else(|| EngineError::CapabilityUnavailable(format!("{id:?} is not declared")))?;
    capability.transition(
        EngineCapabilityOperation::Configured,
        Vec::new(),
        Utc::now(),
    )
}

fn maintenance_receipt(
    action: EngineMaintenanceAction,
    package: &VerifiedEnginePackage,
) -> EngineMaintenanceReceipt {
    EngineMaintenanceReceipt {
        action,
        adapter_id: package.manifest.engine_id,
        engine_version: package.manifest.engine_version.clone(),
        verifier_id: package.verification.verifier_id.clone(),
        verified_at: package.verification.verified_at,
    }
}

fn health(state: EngineHealthState, message: impl Into<String>) -> EngineHealth {
    EngineHealth {
        state,
        checked_at: Utc::now(),
        message: message.into(),
    }
}

fn validate_emergency_change(disabled: bool, reason: Option<&str>) -> Result<(), EngineError> {
    if disabled && reason.is_none_or(|reason| !valid_text(reason, 300)) {
        return Err(EngineError::EmergencyDisabled(
            "emergency disable requires a bounded reason".to_owned(),
        ));
    }
    Ok(())
}

fn ensure_not_disabled(disabled: bool, reason: Option<&str>) -> Result<(), EngineError> {
    if disabled {
        return Err(EngineError::EmergencyDisabled(
            reason
                .unwrap_or("engine disabled without a reason")
                .to_owned(),
        ));
    }
    Ok(())
}

fn load_and_verify_package(
    adapter_id: EngineAdapterId,
    request: &EnginePackageRequest,
    verifier: &dyn EnginePackageVerifier,
) -> Result<VerifiedEnginePackage, EngineError> {
    if !valid_engine_version(&request.expected_version) {
        return Err(EngineError::InvalidPackage(
            "expected version is not a pinned semantic version".to_owned(),
        ));
    }
    let package_root = secure_existing_directory(&request.package_root)?;
    let manifest_path = secure_package_member(
        &package_root,
        Path::new(ENGINE_PACKAGE_MANIFEST),
        PathKind::File,
    )?;
    let metadata = fs::symlink_metadata(&manifest_path)?;
    if metadata.len() > MAX_ENGINE_MANIFEST_BYTES {
        return Err(EngineError::InvalidPackage(
            "engine manifest exceeds 64 KiB".to_owned(),
        ));
    }
    let manifest_bytes = fs::read(&manifest_path)?;
    let manifest: EnginePackageManifest = strict_json_from_slice(&manifest_bytes)?;
    validate_package_manifest(adapter_id, &request.expected_version, &manifest)?;
    let executable_relative = if adapter_id == EngineAdapterId::Camoufox {
        Path::new(
            &manifest
                .entrypoint
                .as_ref()
                .ok_or_else(|| {
                    EngineError::InvalidPackage(
                        "Camoufox package is missing its Host entrypoint".to_owned(),
                    )
                })?
                .relative_path,
        )
    } else {
        Path::new(&manifest.executable_relative_path)
    };
    let executable_path =
        secure_package_member(&package_root, executable_relative, PathKind::File)?;
    let executable_size = fs::symlink_metadata(&executable_path)?.len();
    if executable_size == 0 || executable_size > MAX_ENGINE_EXECUTABLE_BYTES {
        return Err(EngineError::InvalidPackage(
            "engine executable is empty or exceeds the 4 GiB package ceiling".to_owned(),
        ));
    }
    if adapter_id == EngineAdapterId::Camoufox {
        verify_camoufox_package_tree(&package_root, &manifest)?;
        verify_camoufox_package_layout(&package_root, &manifest)?;
    }
    let mut verification = verifier.verify(&manifest_bytes, &executable_path, &manifest)?;
    verification.package_manifest_sha256 = sha256_hex_bytes(&manifest_bytes);
    verification.package_tree_sha256 = manifest
        .tree_manifest
        .as_ref()
        .map(|binding| binding.sha256.clone());
    verification.host_sha256 = manifest.artifact_sha256.clone();
    verification.engine_revision = (adapter_id == EngineAdapterId::Camoufox)
        .then_some(CAMOUFOX_FORMAL_V3_ENGINE_REVISION.to_owned());
    let now = Utc::now();
    if !verification.digest_verified
        || !verification.signature_verified
        || !is_lower_hex(&verification.package_manifest_sha256, 64)
        || !is_lower_hex(&verification.host_sha256, 64)
        || !is_lower_hex(&verification.signer_certificate_sha256, 64)
        || verification
            .package_tree_sha256
            .as_deref()
            .is_some_and(|sha| !is_lower_hex(sha, 64))
        || !valid_text(&verification.verifier_id, 100)
        || verification.verified_at > now + Duration::seconds(5)
        || verification.verified_at < now - Duration::minutes(1)
    {
        return Err(EngineError::VerificationUnavailable(
            "package verifier did not prove both digest and signature".to_owned(),
        ));
    }
    Ok(VerifiedEnginePackage {
        package_root,
        manifest_path,
        executable_path,
        manifest,
        verification,
    })
}

fn validate_package_manifest(
    adapter_id: EngineAdapterId,
    expected_version: &str,
    manifest: &EnginePackageManifest,
) -> Result<(), EngineError> {
    let expected_executable = match adapter_id {
        EngineAdapterId::ControlledChromium => Some("bin/chromium.exe"),
        EngineAdapterId::Camoufox => None,
        EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => {
            return Err(EngineError::InvalidPackage(
                "stock adapters do not consume external packages".to_owned(),
            ));
        }
    };
    if manifest.engine_id != adapter_id
        || manifest.engine_version != expected_version
        || !valid_engine_version(&manifest.engine_version)
        || !version_major(&manifest.engine_version)
            .is_some_and(|major| (100..=999).contains(&major))
        || manifest.channel != EngineChannel::Experimental
        || manifest.platform != WINDOWS_X64_PLATFORM
        || !is_lower_hex(&manifest.artifact_sha256, 64)
        || manifest.signature.algorithm != CMS_SHA256_ALGORITHM
        || !is_lower_hex(&manifest.signature.key_id, 64)
        || !valid_signature_value(&manifest.signature.value)
        || manifest.capabilities.len() > EngineCapabilityId::ALL.len()
    {
        return Err(EngineError::InvalidPackage(
            "manifest identity, version, platform, path, digest, or signature metadata is invalid"
                .to_owned(),
        ));
    }
    match adapter_id {
        EngineAdapterId::ControlledChromium => {
            if manifest.schema_version != 2
                || manifest.executable_relative_path != expected_executable.expect("chromium")
                || manifest.entrypoint.is_some()
                || manifest.tree_manifest.is_some()
                || manifest.browser_tree_manifest.is_some()
                || manifest.host_version.is_some()
                || manifest.browser_asset_sha256.is_some()
                || manifest.browser_release.is_some()
            {
                return Err(EngineError::InvalidPackage(
                    "Controlled Chromium packages must retain the strict schema-v2 native entrypoint contract"
                        .to_owned(),
                ));
            }
        }
        EngineAdapterId::Camoufox => {
            if manifest.schema_version == 2 {
                return Err(EngineError::InvalidPackage(
                    "unsupported entrypoint/schema: Camoufox schema v2 packages cannot be treated as Host entrypoints"
                        .to_owned(),
                ));
            }
            if manifest.schema_version != CAMOUFOX_HOST_PACKAGE_SCHEMA_VERSION
                || !manifest.executable_relative_path.is_empty()
                || manifest.engine_version != CAMOUFOX_FORMAL_V3_ENGINE_VERSION
                || manifest.host_version.as_deref() != Some(CAMOUFOX_HOST_VERSION)
                || manifest.browser_release.as_deref() != Some(CAMOUFOX_FORMAL_V3_BROWSER_RELEASE)
                || manifest.browser_asset_sha256.as_deref()
                    != Some(CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256)
            {
                return Err(EngineError::InvalidPackage(
                    "Camoufox packages require the strict schema-v3 Host entrypoint contract"
                        .to_owned(),
                ));
            }
            let entrypoint = manifest.entrypoint.as_ref().ok_or_else(|| {
                EngineError::InvalidPackage(
                    "Camoufox schema-v3 package is missing entrypoint binding".to_owned(),
                )
            })?;
            let tree_manifest = manifest.tree_manifest.as_ref().ok_or_else(|| {
                EngineError::InvalidPackage(
                    "Camoufox schema-v3 package is missing package tree binding".to_owned(),
                )
            })?;
            let browser_tree_manifest =
                manifest.browser_tree_manifest.as_ref().ok_or_else(|| {
                    EngineError::InvalidPackage(
                        "Camoufox schema-v3 package is missing browser tree manifest binding"
                            .to_owned(),
                    )
                })?;
            if entrypoint.kind != CAMOUFOX_HOST_ENTRYPOINT_KIND
                || entrypoint.protocol != CAMOUFOX_HOST_PROTOCOL
                || !is_lower_hex(&entrypoint.sha256, 64)
                || manifest.artifact_sha256 != entrypoint.sha256
                || !is_lower_hex(&tree_manifest.sha256, 64)
                || !is_lower_hex(&browser_tree_manifest.sha256, 64)
                || entrypoint.relative_path != "host/camoufox-host.exe"
                || tree_manifest.relative_path != "package-tree.json"
                || browser_tree_manifest.relative_path != "browser-tree-manifest.json"
                || entrypoint.relative_path == tree_manifest.relative_path
                || entrypoint.relative_path == browser_tree_manifest.relative_path
                || tree_manifest.relative_path == browser_tree_manifest.relative_path
                || manifest
                    .host_version
                    .as_deref()
                    .is_none_or(|version| !valid_text(version, 64))
                || manifest
                    .browser_asset_sha256
                    .as_deref()
                    .is_none_or(|sha| !is_lower_hex(sha, 64))
                || manifest
                    .browser_release
                    .as_deref()
                    .is_none_or(|release| release != format!("v{}", manifest.engine_version))
            {
                return Err(EngineError::InvalidPackage(
                    "Camoufox schema-v3 Host, protocol, tree, or asset binding is invalid"
                        .to_owned(),
                ));
            }
        }
        EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => unreachable!(),
    }
    let capability_set = manifest
        .capabilities
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let required_capabilities = match adapter_id {
        EngineAdapterId::ControlledChromium => [
            EngineCapabilityId::IdentityTemplate,
            EngineCapabilityId::SiteFallback,
        ]
        .as_slice(),
        EngineAdapterId::Camoufox => [EngineCapabilityId::IdentityTemplate].as_slice(),
        EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => unreachable!(),
    };
    if capability_set.len() != manifest.capabilities.len()
        || capability_set
            .iter()
            .any(|id| !external_capability_allowed(adapter_id, *id))
        || required_capabilities
            .iter()
            .any(|id| !capability_set.contains(id))
    {
        return Err(EngineError::InvalidPackage(
            "manifest capabilities are duplicated or exceed the adapter ceiling".to_owned(),
        ));
    }
    Ok(())
}

fn valid_artifact_id(value: &str) -> bool {
    let Some(suffix) = value.strip_prefix("identity-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix.len() <= 64
        && suffix.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && suffix
            .chars()
            .next()
            .is_some_and(|character| character != '-')
}

fn camoufox_profile_id(silo_id: Uuid) -> String {
    format!("silo-{}", silo_id.simple())
}

fn valid_package_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && ensure_clean_components(Path::new(value), false).is_ok()
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn valid_browser_tree_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && ensure_clean_components(Path::new(value), false).is_ok()
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
        && value
            .split('/')
            .next()
            .is_none_or(|component| !component.contains(':'))
}

fn validate_host_root(root: &Path, label: &str) -> Result<(), EngineError> {
    if !root.is_absolute() {
        return Err(EngineError::UnsafePath(format!(
            "Camoufox Host {label} root must be absolute"
        )));
    }
    ensure_clean_components(root, true)?;
    ensure_no_link_or_reparse(root)?;
    let text = path_string(root)?;
    if text.len() > 4096 {
        return Err(EngineError::UnsafePath(format!(
            "Camoufox Host {label} root is too long"
        )));
    }
    Ok(())
}

fn verify_camoufox_package_tree(
    package_root: &Path,
    manifest: &EnginePackageManifest,
) -> Result<(), EngineError> {
    let binding = manifest.tree_manifest.as_ref().ok_or_else(|| {
        EngineError::InvalidPackage("Camoufox package tree binding is missing".to_owned())
    })?;
    let tree_path = secure_package_member(
        package_root,
        Path::new(&binding.relative_path),
        PathKind::File,
    )?;
    let tree_bytes = fs::read(&tree_path)?;
    validate_camoufox_package_tree_size(&tree_bytes)?;
    if hex_lower(&sha256_bytes(&tree_bytes)) != binding.sha256 {
        return Err(EngineError::VerificationUnavailable(
            "Camoufox package tree manifest SHA-256 does not match its signed binding".to_owned(),
        ));
    }
    let tree: EnginePackageTreeManifest = strict_json_from_slice(&tree_bytes)?;
    if tree.schema != "verisilo-camoufox-host-package-tree/v1"
        || tree.entries.is_empty()
        || tree.entries.len() > 65_536
    {
        return Err(EngineError::InvalidPackage(
            "Camoufox package tree manifest schema or entry count is invalid".to_owned(),
        ));
    }
    let mut expected = BTreeMap::new();
    for entry in tree.entries {
        if !valid_package_relative_path(&entry.path)
            || !is_lower_hex(&entry.sha256, 64)
            || expected.insert(entry.path.clone(), entry.sha256).is_some()
        {
            return Err(EngineError::InvalidPackage(
                "Camoufox package tree manifest contains an invalid or duplicate member".to_owned(),
            ));
        }
    }
    let entrypoint = manifest.entrypoint.as_ref().ok_or_else(|| {
        EngineError::InvalidPackage("Camoufox package entrypoint is missing".to_owned())
    })?;
    if expected.get(&entrypoint.relative_path) != Some(&entrypoint.sha256) {
        return Err(EngineError::InvalidPackage(
            "Camoufox package tree does not bind the Host entrypoint".to_owned(),
        ));
    }
    let browser_tree_binding = manifest.browser_tree_manifest.as_ref().ok_or_else(|| {
        EngineError::InvalidPackage(
            "Camoufox package is missing its browser tree manifest binding".to_owned(),
        )
    })?;
    if expected.get(&browser_tree_binding.relative_path) != Some(&browser_tree_binding.sha256) {
        return Err(EngineError::InvalidPackage(
            "Camoufox package tree does not bind the browser tree manifest member".to_owned(),
        ));
    }
    let browser_tree_path = secure_package_member(
        package_root,
        Path::new(&browser_tree_binding.relative_path),
        PathKind::File,
    )?;
    let browser_tree_bytes = fs::read(&browser_tree_path)?;
    if browser_tree_bytes.len() as u64 > MAX_CAMOUFOX_BROWSER_TREE_MANIFEST_BYTES
        || hex_lower(&sha256_bytes(&browser_tree_bytes)) != browser_tree_binding.sha256
    {
        return Err(EngineError::VerificationUnavailable(
            "Camoufox browser tree manifest bytes do not match their signed binding".to_owned(),
        ));
    }
    validate_camoufox_browser_tree_manifest(&browser_tree_bytes)?;
    let mut actual = BTreeMap::new();
    collect_package_files(
        package_root,
        package_root,
        &binding.relative_path,
        &mut actual,
    )?;
    if actual != expected {
        return Err(EngineError::VerificationUnavailable(
            "Camoufox package tree has a missing, extra, or changed member".to_owned(),
        ));
    }
    Ok(())
}

fn verify_camoufox_package_layout(
    package_root: &Path,
    manifest: &EnginePackageManifest,
) -> Result<(), EngineError> {
    for (relative, kind) in [
        ("runtime-asset-lock.json", PathKind::File),
        ("browser-tree-manifest.json", PathKind::File),
        ("package-tree.json", PathKind::File),
        ("host/camoufox-host.exe", PathKind::File),
        ("host/verisilo-camoufox-supervisor.exe", PathKind::File),
        ("host/probe/probe.html", PathKind::File),
        ("browser", PathKind::Directory),
    ] {
        secure_package_member(package_root, Path::new(relative), kind)?;
    }
    let asset_lock_path = secure_package_member(
        package_root,
        Path::new("runtime-asset-lock.json"),
        PathKind::File,
    )?;
    let asset_lock_bytes = fs::read(&asset_lock_path)?;
    if asset_lock_bytes.len() as u64 > MAX_ENGINE_MANIFEST_BYTES {
        return Err(EngineError::InvalidPackage(
            "Camoufox runtime asset lock exceeds 64 KiB".to_owned(),
        ));
    }
    let asset_lock: serde_json::Value = strict_json_from_slice(&asset_lock_bytes)?;
    let object = asset_lock.as_object().ok_or_else(|| {
        EngineError::InvalidPackage("Camoufox runtime asset lock must be an object".to_owned())
    })?;
    let exact_string = |field: &str, expected: &str| {
        object.get(field).and_then(serde_json::Value::as_str) == Some(expected)
    };
    let exact_u64 = |field: &str, expected: u64| {
        object.get(field).and_then(serde_json::Value::as_u64) == Some(expected)
    };
    let sha256_field = |field: &str, expected: &str| {
        object.get(field).and_then(serde_json::Value::as_str) == Some(expected)
    };
    if !exact_string("schema", CAMOUFOX_PACKAGE_ASSET_LOCK_SCHEMA)
        || !exact_string("assetKind", CAMOUFOX_PACKAGE_ASSET_KIND)
        || object.get("verified") != Some(&serde_json::Value::Bool(false))
        || !exact_string("evidenceClass", CAMOUFOX_PACKAGE_ASSET_EVIDENCE_CLASS)
        || !exact_string("package", "camoufox")
        || !exact_string("release", CAMOUFOX_FORMAL_V3_BROWSER_RELEASE)
        || !exact_string("platform", CAMOUFOX_PACKAGE_ASSET_PLATFORM)
        || !exact_string("pythonPackage", CAMOUFOX_PACKAGE_PYTHON_PACKAGE)
        || !exact_string("engineRevision", CAMOUFOX_FORMAL_V3_ENGINE_REVISION)
        || !exact_string("executableRelativePath", "camoufox.exe")
        || !sha256_field("sha256", CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256)
        || !sha256_field(
            "browserExecutableSha256",
            CAMOUFOX_FORMAL_V3_BROWSER_EXECUTABLE_SHA256,
        )
        || !exact_u64("sizeBytes", CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SIZE_BYTES)
    {
        return Err(EngineError::VerificationUnavailable(
            "Camoufox runtime asset lock is not the pinned Formal-v3 RC1 asset".to_owned(),
        ));
    }
    if manifest
        .entrypoint
        .as_ref()
        .map(|entrypoint| entrypoint.relative_path.as_str())
        != Some("host/camoufox-host.exe")
        || manifest
            .tree_manifest
            .as_ref()
            .map(|binding| binding.relative_path.as_str())
            != Some("package-tree.json")
        || manifest
            .browser_tree_manifest
            .as_ref()
            .map(|binding| binding.relative_path.as_str())
            != Some("browser-tree-manifest.json")
    {
        return Err(EngineError::InvalidPackage(
            "Camoufox package member paths are not the fixed RC1 layout".to_owned(),
        ));
    }
    Ok(())
}

fn validate_camoufox_package_tree_size(bytes: &[u8]) -> Result<(), EngineError> {
    if bytes.len() as u64 > MAX_CAMOUFOX_PACKAGE_TREE_MANIFEST_BYTES {
        return Err(EngineError::InvalidPackage(
            "Camoufox package tree manifest exceeds 4 MiB".to_owned(),
        ));
    }
    Ok(())
}

fn validate_camoufox_browser_tree_manifest(bytes: &[u8]) -> Result<(), EngineError> {
    let tree: EngineBrowserTreeManifest = strict_json_from_slice(bytes)?;
    if tree.schema != CAMOUFOX_BROWSER_TREE_SCHEMA
        || !valid_text(&tree.tree_root_label, 512)
        || tree.file_count != tree.entries.len() as u64
        || tree.entries.is_empty()
        || tree.entries.len() > 65_536
    {
        return Err(EngineError::InvalidPackage(
            "Camoufox browser tree manifest schema or summary is invalid".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for entry in tree.entries {
        if !valid_browser_tree_relative_path(&entry.path)
            || !is_lower_hex(&entry.sha256, 64)
            || !seen.insert(entry.path.to_ascii_lowercase())
        {
            return Err(EngineError::InvalidPackage(
                "Camoufox browser tree manifest contains an invalid or duplicate member".to_owned(),
            ));
        }
        total_bytes = total_bytes.checked_add(entry.size).ok_or_else(|| {
            EngineError::InvalidPackage(
                "Camoufox browser tree manifest byte count overflowed".to_owned(),
            )
        })?;
    }
    if total_bytes != tree.total_bytes {
        return Err(EngineError::InvalidPackage(
            "Camoufox browser tree manifest totalBytes does not match its entries".to_owned(),
        ));
    }
    Ok(())
}

fn collect_package_files(
    root: &Path,
    current: &Path,
    tree_manifest_relative: &str,
    actual: &mut BTreeMap<String, String>,
) -> Result<(), EngineError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(EngineError::UnsafePath(format!(
                "Camoufox package tree rejects a link or reparse point: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_package_files(root, &path, tree_manifest_relative, actual)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(EngineError::UnsafePath(format!(
                "Camoufox package tree contains a non-regular member: {}",
                path.display()
            )));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| EngineError::UnsafePath("package tree member escaped root".to_owned()))?
            .to_str()
            .ok_or_else(|| {
                EngineError::UnsafePath("package tree member is not Unicode".to_owned())
            })?
            .replace('\\', "/");
        if relative == ENGINE_PACKAGE_MANIFEST || relative == tree_manifest_relative {
            continue;
        }
        if !valid_package_relative_path(&relative)
            || actual
                .insert(relative, hex_lower(&sha256_file(&path)?))
                .is_some()
        {
            return Err(EngineError::InvalidPackage(
                "Camoufox package tree contains an invalid or duplicate member".to_owned(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn strict_json_from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, EngineError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictJsonValue::deserialize(&mut deserializer)
        .map_err(|error| EngineError::InvalidPackage(format!("strict JSON parse failed: {error}")))?
        .0;
    deserializer.end().map_err(|error| {
        EngineError::InvalidPackage(format!("strict JSON contains trailing data: {error}"))
    })?;
    serde_json::from_value(value).map_err(EngineError::Serialization)
}

struct StrictJsonValue(serde_json::Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StrictJsonVisitor;

        impl<'de> serde::de::Visitor<'de> for StrictJsonVisitor {
            type Value = serde_json::Value;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a strict JSON value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Bool(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Number(value.into()))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Number(value.into()))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(serde_json::Value::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(serde_json::Value::String(value))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Null)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Null)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
                    values.push(value.0);
                }
                Ok(serde_json::Value::Array(values))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut object = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if object.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON field: {key}"
                        )));
                    }
                    let value = map.next_value::<StrictJsonValue>()?;
                    object.insert(key, value.0);
                }
                Ok(serde_json::Value::Object(object))
            }
        }

        deserializer
            .deserialize_any(StrictJsonVisitor)
            .map(StrictJsonValue)
    }
}

#[derive(Clone, Copy)]
enum PathKind {
    File,
    Directory,
}

fn secure_existing_file(path: &Path) -> Result<PathBuf, EngineError> {
    secure_existing_path(path, PathKind::File)
}

fn secure_existing_directory(path: &Path) -> Result<PathBuf, EngineError> {
    secure_existing_path(path, PathKind::Directory)
}

fn secure_existing_path(path: &Path, kind: PathKind) -> Result<PathBuf, EngineError> {
    if !path.is_absolute() {
        return Err(EngineError::UnsafePath(
            "engine paths must be absolute".to_owned(),
        ));
    }
    ensure_clean_components(path, true)?;
    ensure_no_link_or_reparse(path)?;
    let canonical = path.canonicalize()?;
    let metadata = fs::symlink_metadata(&canonical)?;
    let expected_kind = match kind {
        PathKind::File => metadata.file_type().is_file(),
        PathKind::Directory => metadata.file_type().is_dir(),
    };
    if !expected_kind || metadata_is_link_or_reparse(&metadata) {
        return Err(EngineError::UnsafePath(
            "engine path has the wrong type or is a link/reparse point".to_owned(),
        ));
    }
    Ok(canonical)
}

fn secure_package_member(
    root: &Path,
    relative: &Path,
    kind: PathKind,
) -> Result<PathBuf, EngineError> {
    if relative.is_absolute() || relative.as_os_str().to_string_lossy().contains('\\') {
        return Err(EngineError::UnsafePath(
            "package members must use normalized forward-slash relative paths".to_owned(),
        ));
    }
    ensure_clean_components(relative, false)?;
    let candidate = root.join(relative);
    let canonical_candidate = secure_existing_path(&candidate, kind)?;
    let canonical_root = root.canonicalize()?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(EngineError::UnsafePath(
            "package member escapes its package root".to_owned(),
        ));
    }
    Ok(canonical_candidate)
}

fn ensure_clean_components(path: &Path, allow_root: bool) -> Result<(), EngineError> {
    for component in path.components() {
        let allowed = match component {
            Component::Prefix(_) | Component::RootDir => allow_root,
            Component::Normal(_) => true,
            Component::CurDir | Component::ParentDir => false,
        };
        if !allowed {
            return Err(EngineError::UnsafePath(
                "path contains traversal, a prefix, or a redundant component".to_owned(),
            ));
        }
    }
    Ok(())
}

fn ensure_no_link_or_reparse(path: &Path) -> Result<(), EngineError> {
    for ancestor in path.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(EngineError::Io(error)),
        };
        if metadata_is_link_or_reparse(&metadata) {
            return Err(EngineError::UnsafePath(format!(
                "link or reparse point rejected: {}",
                ancestor.display()
            )));
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
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(target_os = "windows"))]
    false
}

fn validate_launch_profile_path(path: &Path) -> Result<PathBuf, EngineError> {
    if !path.is_absolute() {
        return Err(EngineError::UnsafePath(
            "profile directory must be absolute".to_owned(),
        ));
    }
    ensure_clean_components(path, true)?;
    ensure_no_link_or_reparse(path)?;
    let text = path_string(path)?;
    if text.len() > 4_096 {
        return Err(EngineError::UnsafePath(
            "profile directory path is too long".to_owned(),
        ));
    }
    Ok(path.to_path_buf())
}

fn path_string(path: &Path) -> Result<String, EngineError> {
    path.to_str()
        .filter(|value| !value.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| EngineError::UnsafePath("path is not valid bounded Unicode".to_owned()))
}

fn path_argument(prefix: &str, path: &Path) -> Result<String, EngineError> {
    Ok(format!("{prefix}{}", path_string(path)?))
}

fn validate_identity_template(
    template: &IdentityTemplate,
    expected_family: BrowserFamily,
    expected_major: Option<u16>,
) -> Result<(), EngineError> {
    if template.schema_version != 1
        || template.os.family != "windows"
        || !matches!(template.os.version.as_str(), "10" | "11")
        || template.os.architecture != "x64"
        || template.browser.family != expected_family
        || !(100..=999).contains(&template.browser.major_version)
        || expected_major.is_some_and(|major| major != template.browser.major_version)
        || !valid_text(&template.browser.user_agent, 512)
        || !template.browser.user_agent.contains("Windows NT 10.0")
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "OS, browser family, package major version, or user-agent is inconsistent".to_owned(),
        ));
    }
    let marker = match expected_family {
        BrowserFamily::Chromium => ["Chrome/", "Edg/"]
            .into_iter()
            .find_map(|marker| ua_major(&template.browser.user_agent, marker)),
        BrowserFamily::Firefox => ua_major(&template.browser.user_agent, "Firefox/"),
    };
    if marker != Some(template.browser.major_version) {
        return Err(EngineError::InvalidIdentityTemplate(
            "user-agent major version does not match browser.majorVersion".to_owned(),
        ));
    }

    match expected_family {
        BrowserFamily::Chromium => {
            let ua_ch = template.browser.ua_ch.as_ref().ok_or_else(|| {
                EngineError::InvalidIdentityTemplate(
                    "Chromium templates require coordinated UA-CH".to_owned(),
                )
            })?;
            let chromium_brand_matches = ua_ch.brands.iter().any(|brand| {
                brand.brand == "Chromium"
                    && brand.version == template.browser.major_version.to_string()
            });
            if !chromium_brand_matches
                || ua_ch.brands.is_empty()
                || ua_ch.brands.len() > 8
                || ua_ch.platform != "Windows"
                || ua_ch.architecture != "x86"
                || ua_ch.bitness != "64"
                || ua_ch.mobile
                || !valid_numeric_version(&ua_ch.platform_version)
                || ua_ch
                    .brands
                    .iter()
                    .any(|brand| !valid_text(&brand.brand, 64) || !is_decimal(&brand.version, 3))
            {
                return Err(EngineError::InvalidIdentityTemplate(
                    "Chromium UA-CH must be a matching 64-bit non-mobile Windows declaration"
                        .to_owned(),
                ));
            }
        }
        BrowserFamily::Firefox if template.browser.ua_ch.is_some() => {
            return Err(EngineError::InvalidIdentityTemplate(
                "Camoufox prototype does not accept Chromium UA-CH data".to_owned(),
            ));
        }
        BrowserFamily::Firefox => {}
    }

    if template.languages.accepted.is_empty()
        || template.languages.accepted.len() > 8
        || template.languages.accepted[0] != template.languages.primary
        || !valid_language_tag(&template.languages.primary)
        || template
            .languages
            .accepted
            .iter()
            .any(|language| !valid_language_tag(language))
        || lowercase_unique(&template.languages.accepted).len() != template.languages.accepted.len()
        || !valid_timezone(&template.timezone)
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "language order, uniqueness, or timezone is invalid".to_owned(),
        ));
    }

    if !(800..=16_384).contains(&template.screen.width)
        || !(600..=16_384).contains(&template.screen.height)
        || template.screen.available_width < 640
        || template.screen.available_height < 480
        || template.screen.available_width > template.screen.width
        || template.screen.available_height > template.screen.height
        || !template.screen.device_pixel_ratio.is_finite()
        || !(0.5..=8.0).contains(&template.screen.device_pixel_ratio)
        || !matches!(template.screen.color_depth, 24 | 30 | 32)
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "screen dimensions, pixel ratio, or color depth is invalid".to_owned(),
        ));
    }

    if template.render.web_gl_vendor.is_some() != template.render.web_gl_renderer.is_some()
        || !valid_optional_text(&template.render.web_gl_vendor, 160)
        || !valid_optional_text(&template.render.web_gl_renderer, 300)
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "WebGL vendor and renderer must be bounded and configured together".to_owned(),
        ));
    }
    if template.fonts.families.is_empty()
        || template.fonts.families.len() > 64
        || template
            .fonts
            .families
            .iter()
            .any(|font| !valid_text(font, 100))
        || lowercase_unique(&template.fonts.families).len() != template.fonts.families.len()
        || !template
            .fonts
            .families
            .iter()
            .any(|font| font == "Segoe UI")
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "Windows font declarations must be unique and include Segoe UI".to_owned(),
        ));
    }
    if template.media.microphones > 16
        || template.media.cameras > 16
        || template.media.speakers > 16
        || (template.media.labels_exposed
            && template.media.microphones + template.media.cameras + template.media.speakers == 0)
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "media device count and label exposure are inconsistent".to_owned(),
        ));
    }
    if template
        .network
        .country_code
        .as_ref()
        .is_some_and(|country| {
            country.len() != 2
                || !country
                    .chars()
                    .all(|character| character.is_ascii_uppercase())
        })
        || template
            .network
            .timezone
            .as_ref()
            .is_some_and(|timezone| timezone != &template.timezone)
        || template
            .network
            .locale
            .as_ref()
            .is_some_and(|locale| !locale.eq_ignore_ascii_case(&template.languages.primary))
        || template.network.desired_quic != DesiredQuic::BrowserDefault
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "network country, timezone, locale, or unsupported QUIC control contradicts the browser template"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_derivation_context(
    context: &IdentityDerivationContext,
    now: DateTime<Utc>,
) -> Result<(), EngineError> {
    let lifetime = context.expires_at.signed_duration_since(context.issued_at);
    if context.issued_at > now + Duration::seconds(5)
        || context.expires_at <= now
        || lifetime <= Duration::zero()
        || lifetime > Duration::minutes(MAX_SESSION_TOKEN_LIFETIME_MINUTES)
    {
        return Err(EngineError::InvalidIdentityTemplate(
            "identity derivation context is not a current session scope of at most one hour"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_derived_token(
    token: &DerivedIdentityToken,
    now: DateTime<Utc>,
) -> Result<(), EngineError> {
    if token.expires_at <= now
        || token.expires_at > now + Duration::minutes(MAX_SESSION_TOKEN_LIFETIME_MINUTES)
        || token.token.len() < 32
        || token.token.len() > 128
        || !token
            .token
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(EngineError::VerificationUnavailable(
            "derived identity token is malformed or outside its short session lifetime".to_owned(),
        ));
    }
    Ok(())
}

fn encode_derivation_context(context: &IdentityDerivationContext) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(16 * 4 + 24);
    encoded.extend_from_slice(context.silo_id.as_bytes());
    encoded.extend_from_slice(context.seed_reference.as_bytes());
    encoded.extend_from_slice(context.template_id.as_bytes());
    encoded.extend_from_slice(context.session_id.as_bytes());
    encoded.extend_from_slice(&context.issued_at.timestamp().to_be_bytes());
    encoded.extend_from_slice(&context.issued_at.timestamp_subsec_nanos().to_be_bytes());
    encoded.extend_from_slice(&context.expires_at.timestamp().to_be_bytes());
    encoded.extend_from_slice(&context.expires_at.timestamp_subsec_nanos().to_be_bytes());
    encoded
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_BYTES: usize = 64;
    let mut key_block = [0_u8; BLOCK_BYTES];
    if key.len() > BLOCK_BYTES {
        let mut digest = sha256_bytes(key);
        key_block[..digest.len()].copy_from_slice(&digest);
        digest.zeroize();
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK_BYTES];
    let mut outer_pad = [0x5c_u8; BLOCK_BYTES];
    for index in 0..BLOCK_BYTES {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let mut inner = Sha256State::new();
    inner.update(&inner_pad);
    inner.update(message);
    let mut inner_digest = inner.finalize();
    let mut outer = Sha256State::new();
    outer.update(&outer_pad);
    outer.update(&inner_digest);
    let output = outer.finalize();
    key_block.zeroize();
    inner_pad.zeroize();
    outer_pad.zeroize();
    inner_digest.zeroize();
    output
}

fn validate_fallback_rules(rules: &[SiteFallbackRule]) -> Result<(), EngineError> {
    if rules.len() > 100 {
        return Err(EngineError::InvalidIdentityTemplate(
            "site fallback policy exceeds 100 rules".to_owned(),
        ));
    }
    let mut sites = BTreeSet::new();
    for rule in rules {
        let capabilities = rule
            .disable_capabilities
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if !valid_site_pattern(&rule.site_pattern)
            || !sites.insert(rule.site_pattern.to_ascii_lowercase())
            || capabilities.is_empty()
            || capabilities.len() != rule.disable_capabilities.len()
            || capabilities.len() > 16
            || capabilities
                .iter()
                .any(|capability| !identity_capabilities().contains(capability))
            || rule.action != SiteFallbackAction::RestoreThenReload
        {
            return Err(EngineError::InvalidIdentityTemplate(
                "site fallback rules must use unique host patterns and experimental controls"
                    .to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_evidence_entries(evidence: &[String]) -> Result<(), EngineError> {
    let unique = evidence.iter().collect::<BTreeSet<_>>();
    if evidence.len() > 16
        || unique.len() != evidence.len()
        || evidence.iter().any(|entry| !valid_text(entry, 512))
    {
        return Err(EngineError::InvalidTransition(
            "control evidence must contain at most 16 unique bounded entries".to_owned(),
        ));
    }
    Ok(())
}

fn validate_phase_evidence(
    evidence: &[EngineCapabilityEvidence],
    expected: &BTreeSet<EngineCapabilityId>,
) -> Result<BTreeMap<EngineCapabilityId, Vec<String>>, EngineError> {
    let mut by_id = BTreeMap::new();
    for item in evidence {
        validate_evidence_entries(&item.evidence)?;
        if item.evidence.is_empty() || by_id.insert(item.id, item.evidence.clone()).is_some() {
            return Err(EngineError::InvalidTransition(
                "each phase capability requires exactly one non-empty evidence set".to_owned(),
            ));
        }
    }
    if by_id.keys().copied().collect::<BTreeSet<_>>() != *expected {
        return Err(EngineError::InvalidTransition(
            "phase evidence must cover exactly the capabilities in that phase".to_owned(),
        ));
    }
    Ok(by_id)
}

fn normalize_runtime_host(site: &str) -> Result<String, EngineError> {
    if site.trim() != site || site.contains('*') || !valid_site_pattern(site) {
        return Err(EngineError::InvalidIdentityTemplate(
            "runtime fallback site must be a normalized host without a scheme, port, or wildcard"
                .to_owned(),
        ));
    }
    Ok(site.to_ascii_lowercase())
}

fn site_pattern_matches(pattern: &str, site: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    if let Some(suffix) = pattern.strip_prefix("*.") {
        site.len() > suffix.len()
            && site.ends_with(suffix)
            && site.as_bytes()[site.len() - suffix.len() - 1] == b'.'
    } else {
        site == pattern
    }
}

fn valid_site_pattern(value: &str) -> bool {
    let host = value.strip_prefix("*.").unwrap_or(value);
    !host.is_empty()
        && value.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphanumeric())
                && label
                    .chars()
                    .last()
                    .is_some_and(|character| character.is_ascii_alphanumeric())
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
}

fn ua_major(user_agent: &str, marker: &str) -> Option<u16> {
    let start = user_agent.find(marker)? + marker.len();
    let digits = user_agent[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn version_major(version: &str) -> Option<u16> {
    parse_engine_version(version)?.core[0].try_into().ok()
}

fn valid_engine_version(version: &str) -> bool {
    parse_engine_version(version).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEngineVersion {
    core: [u32; 3],
    prerelease: Option<Vec<String>>,
}

fn parse_engine_version(version: &str) -> Option<ParsedEngineVersion> {
    if version.is_empty() || version.len() > 64 || !version.is_ascii() {
        return None;
    }
    let mut build_split = version.split('+');
    let version_without_build = build_split.next()?;
    let build = build_split.next();
    if build_split.next().is_some()
        || build.is_some_and(|build| !valid_semver_identifiers(build, false))
    {
        return None;
    }
    let (core, prerelease) = version_without_build
        .split_once('-')
        .map_or((version_without_build, None), |(core, prerelease)| {
            (core, Some(prerelease))
        });
    if prerelease.is_some_and(|value| !valid_semver_identifiers(value, true)) {
        return None;
    }
    let core_parts = core.split('.').collect::<Vec<_>>();
    if core_parts.len() != 3
        || core_parts.iter().any(|part| {
            part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.chars().all(|character| character.is_ascii_digit())
        })
    {
        return None;
    }
    let core = [
        core_parts[0].parse().ok()?,
        core_parts[1].parse().ok()?,
        core_parts[2].parse().ok()?,
    ];
    Some(ParsedEngineVersion {
        core,
        prerelease: prerelease.map(|value| value.split('.').map(str::to_owned).collect()),
    })
}

fn valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                && (!reject_numeric_leading_zero
                    || !identifier
                        .chars()
                        .all(|character| character.is_ascii_digit())
                    || identifier.len() == 1
                    || !identifier.starts_with('0'))
        })
}

fn compare_engine_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let left = parse_engine_version(left)?;
    let right = parse_engine_version(right)?;
    let core_order = left.core.cmp(&right.core);
    if core_order != std::cmp::Ordering::Equal {
        return Some(core_order);
    }
    match (&left.prerelease, &right.prerelease) {
        (None, None) => Some(std::cmp::Ordering::Equal),
        (None, Some(_)) => Some(std::cmp::Ordering::Greater),
        (Some(_), None) => Some(std::cmp::Ordering::Less),
        (Some(left), Some(right)) => {
            for (left, right) in left.iter().zip(right) {
                let left_numeric = left.parse::<u64>().ok();
                let right_numeric = right.parse::<u64>().ok();
                let order = match (left_numeric, right_numeric) {
                    (Some(left), Some(right)) => left.cmp(&right),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => left.cmp(right),
                };
                if order != std::cmp::Ordering::Equal {
                    return Some(order);
                }
            }
            Some(left.len().cmp(&right.len()))
        }
    }
}

fn valid_numeric_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    !parts.is_empty()
        && parts.len() <= 4
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

fn valid_language_tag(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    matches!(parts.len(), 1 | 2)
        && matches!(parts[0].len(), 2 | 3)
        && parts[0]
            .chars()
            .all(|character| character.is_ascii_alphabetic())
        && (parts.len() == 1
            || (parts[1].len() == 2
                && parts[1]
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())))
}

fn valid_timezone(value: &str) -> bool {
    value == "UTC"
        || (valid_text(value, 80)
            && value.contains('/')
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '/' | '_' | '+' | '-' | '.')
            }))
}

fn lowercase_unique(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn valid_optional_text(value: &Option<String>, maximum: usize) -> bool {
    value
        .as_ref()
        .is_none_or(|value| valid_text(value, maximum))
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn is_decimal(value: &str, maximum_digits: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_digits
        && value.chars().all(|character| character.is_ascii_digit())
}

fn is_lower_hex(value: &str, exact_length: usize) -> bool {
    value.len() == exact_length
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

fn valid_signature_value(value: &str) -> bool {
    (256..=60_000).contains(&value.len())
        && value.len() % 4 == 0
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
        })
        && value.find('=').is_none_or(|position| {
            value[position..].len() <= 2
                && value[position..].chars().all(|character| character == '=')
        })
}

#[cfg(any(target_os = "windows", test))]
fn manifest_signing_payload(manifest: &EnginePackageManifest) -> Result<Vec<u8>, EngineError> {
    let domain: &[u8] = match manifest.schema_version {
        2 => b"VeriSilo engine package manifest v2\0",
        CAMOUFOX_HOST_PACKAGE_SCHEMA_VERSION => b"VeriSilo engine package manifest v3\0",
        _ => {
            return Err(EngineError::InvalidPackage(
                "unsupported engine package manifest schema for signing payload".to_owned(),
            ))
        }
    };
    let mut unsigned = manifest.clone();
    unsigned.signature.value.clear();
    let encoded = serde_json::to_vec(&unsigned)?;
    let mut payload = Vec::with_capacity(domain.len() + encoded.len());
    payload.extend_from_slice(domain);
    payload.extend_from_slice(&encoded);
    Ok(payload)
}

fn sha256_file(path: &Path) -> Result<[u8; 32], EngineError> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() == 0 || before.len() > MAX_ENGINE_EXECUTABLE_BYTES {
        return Err(EngineError::InvalidPackage(
            "engine artifact is empty, not a file, or exceeds 4 GiB".to_owned(),
        ));
    }
    let before_modified = before.modified().ok();
    let mut state = Sha256State::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.checked_add(read as u64).ok_or_else(|| {
            EngineError::InvalidPackage("engine artifact length overflow".to_owned())
        })?;
        if total > MAX_ENGINE_EXECUTABLE_BYTES {
            return Err(EngineError::InvalidPackage(
                "engine artifact exceeds 4 GiB while hashing".to_owned(),
            ));
        }
        state.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    if total != before.len()
        || after.len() != before.len()
        || (before_modified.is_some() && after.modified().ok() != before_modified)
    {
        return Err(EngineError::VerificationUnavailable(
            "engine artifact changed while its SHA-256 was computed".to_owned(),
        ));
    }
    Ok(state.finalize())
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut state = Sha256State::new();
    state.update(bytes);
    state.finalize()
}

pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    hex_lower(&sha256_bytes(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 0x0f) as usize] as char);
    }
    value
}

struct Sha256State {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    message_len: u64,
}

impl Sha256State {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffer_len: 0,
            message_len: 0,
        }
    }

    fn update(&mut self, mut bytes: &[u8]) {
        self.message_len = self
            .message_len
            .checked_add(bytes.len() as u64)
            .expect("SHA-256 input length is bounded by the package ceiling");
        if self.buffer_len != 0 {
            let needed = 64 - self.buffer_len;
            let copied = needed.min(bytes.len());
            self.buffer[self.buffer_len..self.buffer_len + copied]
                .copy_from_slice(&bytes[..copied]);
            self.buffer_len += copied;
            bytes = &bytes[copied..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                sha256_compress(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while bytes.len() >= 64 {
            let block: &[u8; 64] = bytes[..64]
                .try_into()
                .expect("the SHA-256 block has an exact length");
            sha256_compress(&mut self.state, block);
            bytes = &bytes[64..];
        }
        self.buffer[..bytes.len()].copy_from_slice(bytes);
        self.buffer_len = bytes.len();
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self
            .message_len
            .checked_mul(8)
            .expect("SHA-256 input length is bounded by the package ceiling");
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            self.buffer[self.buffer_len..].fill(0);
            let block = self.buffer;
            sha256_compress(&mut self.state, &block);
            self.buffer = [0; 64];
        } else {
            self.buffer[self.buffer_len..56].fill(0);
        }
        self.buffer[56..].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        sha256_compress(&mut self.state, &block);
        let mut digest = [0_u8; 32];
        for (chunk, value) in digest.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&value.to_be_bytes());
        }
        digest
    }
}

fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut schedule = [0_u32; 64];
    for (index, chunk) in block.chunks_exact(4).enumerate() {
        schedule[index] = u32::from_be_bytes(chunk.try_into().expect("four-byte SHA word"));
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ ((!e) & g);
        let temporary1 = h
            .wrapping_add(sum1)
            .wrapping_add(choice)
            .wrapping_add(K[index])
            .wrapping_add(schedule[index]);
        let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temporary2 = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temporary1);
        d = c;
        c = b;
        b = a;
        a = temporary1.wrapping_add(temporary2);
    }
    for (value, added) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *value = value.wrapping_add(added);
    }
}

fn production_state_store(
    adapter_id: EngineAdapterId,
) -> Result<Option<EngineStateStore>, EngineError> {
    #[cfg(target_os = "windows")]
    {
        let root = crate::domain::app_data_root_path()
            .map_err(|error| EngineError::VerificationUnavailable(error.to_string()))?
            .join("engine-state");
        EngineStateStore::new(root, adapter_id).map(Some)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = adapter_id;
        Ok(None)
    }
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), EngineError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), EngineError> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let existing = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            replacement.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        return Err(EngineError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn sync_directory(path: &Path) -> Result<(), EngineError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn sync_directory(_path: &Path) -> Result<(), EngineError> {
    // MoveFileExW(MOVEFILE_WRITE_THROUGH) flushes the atomic replacement on Windows.
    Ok(())
}

#[cfg(target_os = "windows")]
fn verify_windows_detached_cms_sha256(
    payload: &[u8],
    signature: &[u8],
) -> Result<Vec<u8>, EngineError> {
    use std::{
        ffi::{c_char, c_void, CStr},
        mem, ptr, slice,
    };

    const X509_ASN_ENCODING: u32 = 0x0000_0001;
    const PKCS_7_ASN_ENCODING: u32 = 0x0001_0000;
    const ENCODING: u32 = X509_ASN_ENCODING | PKCS_7_ASN_ENCODING;
    const CMSG_DETACHED_FLAG: u32 = 0x0000_0004;
    const CMSG_SIGNER_COUNT_PARAM: u32 = 5;
    const CMSG_SIGNER_HASH_ALGORITHM_PARAM: u32 = 8;
    const SHA256_OID: &str = "2.16.840.1.101.3.4.2.1";
    const CODE_SIGNING_EKU_OID: &str = "1.3.6.1.5.5.7.3.3";

    #[repr(C)]
    struct CryptDataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[repr(C)]
    struct CryptAlgorithmIdentifier {
        psz_obj_id: *mut c_char,
        parameters: CryptDataBlob,
    }

    #[repr(C)]
    struct CryptVerifyMessagePara {
        cb_size: u32,
        dw_msg_and_cert_encoding_type: u32,
        h_crypt_prov: usize,
        pfn_get_signer_certificate: *mut c_void,
        pv_get_arg: *mut c_void,
    }

    #[repr(C)]
    struct CertContext {
        dw_cert_encoding_type: u32,
        pb_cert_encoded: *const u8,
        cb_cert_encoded: u32,
        p_cert_info: *mut c_void,
        h_cert_store: *mut c_void,
    }

    #[repr(C)]
    struct CertEnhKeyUsage {
        c_usage_identifier: u32,
        rgpsz_usage_identifier: *mut *mut c_char,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptMsgOpenToDecode(
            encoding: u32,
            flags: u32,
            message_type: u32,
            crypt_prov: usize,
            recipient_info: *const c_void,
            stream_info: *const c_void,
        ) -> *mut c_void;
        fn CryptMsgUpdate(
            message: *mut c_void,
            data: *const u8,
            data_len: u32,
            final_block: i32,
        ) -> i32;
        fn CryptMsgGetParam(
            message: *mut c_void,
            param_type: u32,
            index: u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
        fn CryptMsgClose(message: *mut c_void) -> i32;
        fn CryptVerifyDetachedMessageSignature(
            verify_para: *const CryptVerifyMessagePara,
            signer_index: u32,
            detached_signature: *const u8,
            detached_signature_len: u32,
            content_count: u32,
            content: *const *const u8,
            content_len: *const u32,
            signer_certificate: *mut *const CertContext,
        ) -> i32;
        fn CertVerifyTimeValidity(time: *const c_void, cert_info: *const c_void) -> i32;
        fn CertGetEnhancedKeyUsage(
            cert_context: *const CertContext,
            flags: u32,
            usage: *mut CertEnhKeyUsage,
            usage_len: *mut u32,
        ) -> i32;
        fn CertFreeCertificateContext(cert_context: *const CertContext) -> i32;
    }

    fn crypto_error(context: &str) -> EngineError {
        EngineError::VerificationUnavailable(format!(
            "{context}: {}",
            std::io::Error::last_os_error()
        ))
    }

    fn aligned_storage(byte_len: u32) -> Vec<usize> {
        vec![0; (byte_len as usize).div_ceil(mem::size_of::<usize>())]
    }

    if payload.is_empty()
        || payload.len() > u32::MAX as usize
        || signature.is_empty()
        || signature.len() > MAX_ENGINE_SIGNATURE_BYTES
    {
        return Err(EngineError::VerificationUnavailable(
            "detached CMS input size is invalid".to_owned(),
        ));
    }

    let message = unsafe {
        CryptMsgOpenToDecode(ENCODING, CMSG_DETACHED_FLAG, 0, 0, ptr::null(), ptr::null())
    };
    if message.is_null() {
        return Err(crypto_error("failed to open detached CMS message"));
    }
    let algorithm_result = (|| -> Result<(), EngineError> {
        if unsafe { CryptMsgUpdate(message, signature.as_ptr(), signature.len() as u32, 1) } == 0 {
            return Err(crypto_error("failed to decode detached CMS message"));
        }
        let mut signer_count = 0_u32;
        let mut signer_count_len = mem::size_of::<u32>() as u32;
        if unsafe {
            CryptMsgGetParam(
                message,
                CMSG_SIGNER_COUNT_PARAM,
                0,
                (&mut signer_count as *mut u32).cast(),
                &mut signer_count_len,
            )
        } == 0
        {
            return Err(crypto_error("failed to read CMS signer count"));
        }
        if signer_count != 1 {
            return Err(EngineError::VerificationUnavailable(
                "engine manifests require exactly one CMS signer".to_owned(),
            ));
        }
        let mut algorithm_len = 0_u32;
        unsafe {
            CryptMsgGetParam(
                message,
                CMSG_SIGNER_HASH_ALGORITHM_PARAM,
                0,
                ptr::null_mut(),
                &mut algorithm_len,
            );
        }
        if algorithm_len < mem::size_of::<CryptAlgorithmIdentifier>() as u32
            || algorithm_len > 4_096
        {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer hash algorithm metadata is invalid".to_owned(),
            ));
        }
        let mut storage = aligned_storage(algorithm_len);
        if unsafe {
            CryptMsgGetParam(
                message,
                CMSG_SIGNER_HASH_ALGORITHM_PARAM,
                0,
                storage.as_mut_ptr().cast(),
                &mut algorithm_len,
            )
        } == 0
        {
            return Err(crypto_error("failed to read CMS hash algorithm"));
        }
        let algorithm = unsafe { &*(storage.as_ptr().cast::<CryptAlgorithmIdentifier>()) };
        if algorithm.psz_obj_id.is_null()
            || unsafe { CStr::from_ptr(algorithm.psz_obj_id) }.to_bytes() != SHA256_OID.as_bytes()
        {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer must use SHA-256".to_owned(),
            ));
        }
        Ok(())
    })();
    unsafe {
        CryptMsgClose(message);
    }
    algorithm_result?;

    let verify_para = CryptVerifyMessagePara {
        cb_size: mem::size_of::<CryptVerifyMessagePara>() as u32,
        dw_msg_and_cert_encoding_type: ENCODING,
        h_crypt_prov: 0,
        pfn_get_signer_certificate: ptr::null_mut(),
        pv_get_arg: ptr::null_mut(),
    };
    let content = [payload.as_ptr()];
    let content_len = [payload.len() as u32];
    let mut signer_certificate: *const CertContext = ptr::null();
    if unsafe {
        CryptVerifyDetachedMessageSignature(
            &verify_para,
            0,
            signature.as_ptr(),
            signature.len() as u32,
            1,
            content.as_ptr(),
            content_len.as_ptr(),
            &mut signer_certificate,
        )
    } == 0
    {
        return Err(crypto_error("detached CMS signature verification failed"));
    }
    if signer_certificate.is_null() {
        return Err(EngineError::VerificationUnavailable(
            "detached CMS verification returned no signer certificate".to_owned(),
        ));
    }
    let certificate_result = (|| -> Result<Vec<u8>, EngineError> {
        let certificate = unsafe { &*signer_certificate };
        if certificate.p_cert_info.is_null()
            || unsafe { CertVerifyTimeValidity(ptr::null(), certificate.p_cert_info) } != 0
        {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer certificate is not currently valid".to_owned(),
            ));
        }

        let mut usage_len = 0_u32;
        unsafe {
            CertGetEnhancedKeyUsage(signer_certificate, 0, ptr::null_mut(), &mut usage_len);
        }
        if usage_len < mem::size_of::<CertEnhKeyUsage>() as u32 || usage_len > 16_384 {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer certificate has no bounded enhanced-key-usage extension".to_owned(),
            ));
        }
        let mut usage_storage = aligned_storage(usage_len);
        if unsafe {
            CertGetEnhancedKeyUsage(
                signer_certificate,
                0,
                usage_storage.as_mut_ptr().cast(),
                &mut usage_len,
            )
        } == 0
        {
            return Err(crypto_error("failed to read CMS signer certificate EKU"));
        }
        let usage = unsafe { &*(usage_storage.as_ptr().cast::<CertEnhKeyUsage>()) };
        if usage.c_usage_identifier == 0
            || usage.c_usage_identifier > 64
            || usage.rgpsz_usage_identifier.is_null()
        {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer certificate must explicitly declare the code-signing EKU".to_owned(),
            ));
        }
        let identifiers = unsafe {
            slice::from_raw_parts(
                usage.rgpsz_usage_identifier,
                usage.c_usage_identifier as usize,
            )
        };
        let code_signing = identifiers.iter().any(|identifier| {
            !identifier.is_null()
                && unsafe { CStr::from_ptr(*identifier) }.to_bytes()
                    == CODE_SIGNING_EKU_OID.as_bytes()
        });
        if !code_signing {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer certificate lacks the code-signing EKU".to_owned(),
            ));
        }

        if certificate.pb_cert_encoded.is_null()
            || certificate.cb_cert_encoded == 0
            || certificate.cb_cert_encoded > 64 * 1024
        {
            return Err(EngineError::VerificationUnavailable(
                "CMS signer certificate encoding is invalid".to_owned(),
            ));
        }
        Ok(unsafe {
            slice::from_raw_parts(
                certificate.pb_cert_encoded,
                certificate.cb_cert_encoded as usize,
            )
            .to_vec()
        })
    })();
    unsafe {
        CertFreeCertificateContext(signer_certificate);
    }
    certificate_result
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, io::Cursor, path::Path, sync::Arc};

    use chrono::{Duration, Utc};
    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::domain::{BrowserDescriptor, BrowserKind, NetworkProfile, ProxyScheme};

    use super::{
        hex_lower, sha256_bytes, sha256_file, BrowserFamily, CamoufoxArtifactBindingV1,
        CamoufoxHostRoots, CanvasMode, DerivedIdentityToken, DesiredQuic, EngineAdapter,
        EngineAdapterId, EngineBrowserTreeEntry, EngineBrowserTreeManifest,
        EngineCapabilityAvailability, EngineCapabilityEvidence, EngineCapabilityId,
        EngineCapabilityOperation, EngineChannel, EngineControlExecution, EngineControlPhase,
        EngineError, EngineLaunchRequest, EngineMaintenanceAction, EnginePackageEntrypoint,
        EnginePackageManifest, EnginePackageRequest, EnginePackageSignature,
        EnginePackageTreeBinding, EnginePackageTreeEntry, EnginePackageTreeManifest,
        EnginePackageVerification, EnginePackageVerifier, EngineTransport,
        ExternalPackageEngineAdapter, IdentityBrowser, IdentityDerivationContext, IdentityFonts,
        IdentityLanguages, IdentityMedia, IdentityNetwork, IdentityOperatingSystem, IdentityRender,
        IdentityScreen, IdentityTemplate, IdentityTokenDeriver, IdentityUaCh, IdentityUaChBrand,
        SiteFallbackAction, SiteFallbackRule, StockChromiumAdapter,
        UnavailableIdentityTokenDeriver, WindowsProductionEnginePackageVerifier,
        CAMOUFOX_ARTIFACT_SCHEMA, CAMOUFOX_ARTIFACT_SCHEMA_V6, CAMOUFOX_BROWSER_TREE_SCHEMA,
        CAMOUFOX_HOST_ENTRYPOINT_KIND, CAMOUFOX_HOST_PROTOCOL,
    };

    struct TestPackageVerifier;

    impl EnginePackageVerifier for TestPackageVerifier {
        fn verify(
            &self,
            _manifest_bytes: &[u8],
            executable_path: &Path,
            manifest: &EnginePackageManifest,
        ) -> Result<EnginePackageVerification, EngineError> {
            if !executable_path.is_file()
                || hex_lower(&sha256_file(executable_path)?) != manifest.artifact_sha256
            {
                return Err(EngineError::VerificationUnavailable(
                    "test digest mismatch".to_owned(),
                ));
            }
            if manifest.signature.key_id != "1".repeat(64)
                || manifest.signature.value != "A".repeat(256)
            {
                return Err(EngineError::VerificationUnavailable(
                    "test signer mismatch".to_owned(),
                ));
            }
            Ok(EnginePackageVerification {
                verifier_id: "test-only-verifier".to_owned(),
                digest_verified: true,
                signature_verified: true,
                package_manifest_sha256: String::new(),
                package_tree_sha256: None,
                host_sha256: String::new(),
                signer_certificate_sha256: manifest.signature.key_id.clone(),
                engine_revision: None,
                verified_at: Utc::now(),
            })
        }
    }

    struct DeterministicTestDeriver;

    impl IdentityTokenDeriver for DeterministicTestDeriver {
        fn derive_session_token(
            &self,
            context: &IdentityDerivationContext,
        ) -> Result<DerivedIdentityToken, EngineError> {
            Ok(DerivedIdentityToken {
                token_id: context.session_id,
                token: context.session_id.simple().to_string(),
                expires_at: context.expires_at,
            })
        }
    }

    #[test]
    fn stock_adapter_builds_a_shell_free_profile_plan_without_control_claims() {
        let root = test_root("stock");
        fs::create_dir_all(&root).expect("root");
        let executable = root.join("chrome.exe");
        fs::write(&executable, b"fixture").expect("browser fixture");
        let adapter = StockChromiumAdapter::new(BrowserDescriptor {
            kind: BrowserKind::Chrome,
            executable_path: executable.to_string_lossy().into_owned(),
            version: Some("150.0.0".to_owned()),
        });
        let plan = adapter
            .launch_plan(&EngineLaunchRequest {
                silo_id: None,
                session_id: Uuid::new_v4(),
                profile_directory: root.join("profile"),
                network_profile: NetworkProfile::Direct {
                    proxy_required: false,
                },
                identity: None,
                derived_token: None,
                fallback_rules: Vec::new(),
                camoufox_artifact_binding: None,
                camoufox_roots: None,
            })
            .expect("stock plan");
        assert!(!plan.shell);
        assert!(plan.arguments[0].starts_with("--user-data-dir="));
        assert!(plan.arguments.contains(&"--disable-sync".to_owned()));
        assert!(plan.control.is_none());
        assert_capability(
            &plan.capabilities,
            EngineCapabilityId::ProfileIsolation,
            EngineCapabilityAvailability::Supported,
            EngineCapabilityOperation::Configured,
        );
        assert_capability(
            &plan.capabilities,
            EngineCapabilityId::Canvas,
            EngineCapabilityAvailability::Unavailable,
            EngineCapabilityOperation::NotConfigured,
        );
        assert_capability(
            &plan.capabilities,
            EngineCapabilityId::TlsClientHello,
            EngineCapabilityAvailability::Unavailable,
            EngineCapabilityOperation::NotConfigured,
        );
        assert!(matches!(
            adapter.launch_plan(&EngineLaunchRequest {
                silo_id: None,
                session_id: Uuid::new_v4(),
                profile_directory: root.join("profile-with-token"),
                network_profile: NetworkProfile::Direct {
                    proxy_required: false,
                },
                identity: None,
                derived_token: Some(DerivedIdentityToken {
                    token_id: Uuid::new_v4(),
                    token: "x".repeat(43),
                    expires_at: Utc::now() + Duration::minutes(30),
                }),
                fallback_rules: Vec::new(),
                camoufox_artifact_binding: None,
                camoufox_roots: None,
            }),
            Err(EngineError::CapabilityUnavailable(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_verifier_fails_closed_without_a_pinned_windows_signer() {
        let root = create_package(
            "no-verifier",
            "150.0.0",
            EngineAdapterId::ControlledChromium,
        );
        let mut adapter =
            ExternalPackageEngineAdapter::production_prototype(EngineAdapterId::ControlledChromium)
                .expect("prototype");
        let error = adapter
            .install(&EnginePackageRequest {
                package_root: root.clone(),
                expected_version: "150.0.0".to_owned(),
            })
            .expect_err("unavailable platform or missing signer pin must fail");
        assert!(matches!(error, EngineError::VerificationUnavailable(_)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_v3_package_binds_host_transport_artifact_and_tree() {
        let root = test_root("camoufox-v3");
        let host_directory = root.join("host");
        fs::create_dir_all(&host_directory).expect("Host directory");
        let host_path = host_directory.join("camoufox-host.exe");
        fs::write(&host_path, b"fake Host entrypoint").expect("Host entrypoint");
        let host_sha = hex_lower(&sha256_file(&host_path).expect("Host digest"));
        let browser_tree = EngineBrowserTreeManifest {
            schema: CAMOUFOX_BROWSER_TREE_SCHEMA.to_owned(),
            tree_root_label: "fake-camoufox-browser".to_owned(),
            file_count: 1,
            total_bytes: 1,
            entries: vec![EngineBrowserTreeEntry {
                path: "camoufox.exe".to_owned(),
                size: 1,
                sha256: "4".repeat(64),
            }],
        };
        let browser_tree_bytes = serde_json::to_vec(&browser_tree).expect("browser tree JSON");
        let browser_tree_path = root.join("browser-tree-manifest.json");
        fs::write(&browser_tree_path, &browser_tree_bytes).expect("browser tree manifest");
        let browser_tree_sha = hex_lower(&sha256_bytes(&browser_tree_bytes));
        let browser_root = root.join("browser");
        fs::create_dir_all(&browser_root).expect("browser root");
        let browser_executable = browser_root.join("camoufox.exe");
        fs::write(&browser_executable, b"fake browser executable").expect("browser asset");
        let supervisor_path = host_directory.join("verisilo-camoufox-supervisor.exe");
        fs::write(&supervisor_path, b"fake supervisor").expect("supervisor");
        let probe_directory = host_directory.join("probe");
        fs::create_dir_all(&probe_directory).expect("probe directory");
        let probe_path = probe_directory.join("probe.html");
        fs::write(&probe_path, b"<!doctype html>").expect("probe");
        fs::write(
            root.join("runtime-asset-lock.json"),
            serde_json::json!({
                "schema": super::CAMOUFOX_PACKAGE_ASSET_LOCK_SCHEMA,
                "assetKind": super::CAMOUFOX_PACKAGE_ASSET_KIND,
                "verified": false,
                "evidenceClass": super::CAMOUFOX_PACKAGE_ASSET_EVIDENCE_CLASS,
                "package": "camoufox",
                "release": super::CAMOUFOX_FORMAL_V3_BROWSER_RELEASE,
                "platform": super::CAMOUFOX_PACKAGE_ASSET_PLATFORM,
                "pythonPackage": super::CAMOUFOX_PACKAGE_PYTHON_PACKAGE,
                "engineRevision": super::CAMOUFOX_FORMAL_V3_ENGINE_REVISION,
                "executableRelativePath": "camoufox.exe",
                "sha256": super::CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256,
                "browserExecutableSha256": super::CAMOUFOX_FORMAL_V3_BROWSER_EXECUTABLE_SHA256,
                "sizeBytes": super::CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SIZE_BYTES,
            })
            .to_string(),
        )
        .expect("asset lock");
        let tree = EnginePackageTreeManifest {
            schema: "verisilo-camoufox-host-package-tree/v1".to_owned(),
            entries: vec![
                EnginePackageTreeEntry {
                    path: "browser-tree-manifest.json".to_owned(),
                    sha256: browser_tree_sha.clone(),
                },
                EnginePackageTreeEntry {
                    path: "host/camoufox-host.exe".to_owned(),
                    sha256: host_sha.clone(),
                },
                EnginePackageTreeEntry {
                    path: "runtime-asset-lock.json".to_owned(),
                    sha256: hex_lower(
                        &sha256_file(&root.join("runtime-asset-lock.json"))
                            .expect("asset lock digest"),
                    ),
                },
                EnginePackageTreeEntry {
                    path: "host/verisilo-camoufox-supervisor.exe".to_owned(),
                    sha256: hex_lower(&sha256_file(&supervisor_path).expect("supervisor digest")),
                },
                EnginePackageTreeEntry {
                    path: "host/probe/probe.html".to_owned(),
                    sha256: hex_lower(&sha256_file(&probe_path).expect("probe digest")),
                },
                EnginePackageTreeEntry {
                    path: "browser/camoufox.exe".to_owned(),
                    sha256: hex_lower(&sha256_file(&browser_executable).expect("browser digest")),
                },
            ],
        };
        let tree_bytes = serde_json::to_vec(&tree).expect("tree JSON");
        let tree_path = root.join("package-tree.json");
        fs::write(&tree_path, &tree_bytes).expect("tree manifest");
        let tree_sha = hex_lower(&sha256_bytes(&tree_bytes));
        let manifest = EnginePackageManifest {
            schema_version: 3,
            engine_id: EngineAdapterId::Camoufox,
            engine_version: "152.0.4-beta.28".to_owned(),
            channel: EngineChannel::Experimental,
            platform: "windows-x64".to_owned(),
            executable_relative_path: String::new(),
            artifact_sha256: host_sha.clone(),
            signature: EnginePackageSignature {
                algorithm: super::CMS_SHA256_ALGORITHM.to_owned(),
                key_id: "1".repeat(64),
                value: "A".repeat(256),
            },
            capabilities: super::identity_capabilities().into_iter().collect(),
            entrypoint: Some(EnginePackageEntrypoint {
                kind: CAMOUFOX_HOST_ENTRYPOINT_KIND.to_owned(),
                relative_path: "host/camoufox-host.exe".to_owned(),
                protocol: CAMOUFOX_HOST_PROTOCOL.to_owned(),
                sha256: host_sha.clone(),
            }),
            tree_manifest: Some(EnginePackageTreeBinding {
                relative_path: "package-tree.json".to_owned(),
                sha256: tree_sha,
            }),
            browser_tree_manifest: Some(EnginePackageTreeBinding {
                relative_path: "browser-tree-manifest.json".to_owned(),
                sha256: browser_tree_sha,
            }),
            host_version: Some("0.1.0".to_owned()),
            browser_asset_sha256: Some(super::CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256.to_owned()),
            browser_release: Some("v152.0.4-beta.28".to_owned()),
        };
        fs::write(
            root.join("engine-package.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
        )
        .expect("manifest");

        let mut adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::Camoufox,
            Arc::new(TestPackageVerifier),
        )
        .expect("Camoufox adapter");
        adapter
            .install(&EnginePackageRequest {
                package_root: root.clone(),
                expected_version: "152.0.4-beta.28".to_owned(),
            })
            .expect("install v3 Host package");
        let template = firefox_template(152);
        let silo_id = Uuid::new_v4();
        let roots = CamoufoxHostRoots {
            artifact_root: root.join("runtime-artifacts"),
            profile_root: root.join("runtime-profiles"),
            state_root: root.join("runtime-state"),
        };
        for path in [&roots.artifact_root, &roots.profile_root, &roots.state_root] {
            fs::create_dir_all(path).expect("Host root");
        }
        let request = EngineLaunchRequest {
            silo_id: Some(silo_id),
            session_id: Uuid::new_v4(),
            profile_directory: root.join("legacy-profile"),
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
            identity: Some(template.clone()),
            derived_token: None,
            fallback_rules: Vec::new(),
            camoufox_artifact_binding: Some(CamoufoxArtifactBindingV1 {
                artifact_id: "identity-camoufox-m3".to_owned(),
                artifact_file_sha256: "3".repeat(64),
                schema: CAMOUFOX_ARTIFACT_SCHEMA.to_owned(),
            }),
            camoufox_roots: Some(roots.clone()),
        };
        let plan = adapter.launch_plan(&request).expect("Host launch plan");
        for network_profile in [
            NetworkProfile::FixedProxy {
                proxy_required: false,
                scheme: ProxyScheme::Socks5,
                host: "127.0.0.1".to_owned(),
                port: 1,
                bypass_list: Vec::new(),
                credential_reference: None,
                external_mihomo: None,
            },
            NetworkProfile::Pac {
                proxy_required: false,
                pac_url: "http://127.0.0.1/pac".to_owned(),
            },
        ] {
            let mut non_direct_request = request.clone();
            non_direct_request.network_profile = network_profile;
            let error = adapter
                .launch_plan(&non_direct_request)
                .expect_err("Camoufox engine plan must reject unsupported network profiles");
            assert!(matches!(
                error,
                EngineError::CapabilityUnavailable(message)
                    if message.contains("only supports Direct(false) or required FixedProxy")
            ));
        }
        assert_eq!(plan.transport, EngineTransport::CamoufoxHostJsonlV1);
        let expected_browser_tree_path = fs::canonicalize(&browser_tree_path)
            .expect("canonical browser tree path")
            .to_string_lossy()
            .into_owned();
        assert!(plan.arguments.windows(2).any(|window| {
            window
                == [
                    "--tree-manifest".to_owned(),
                    expected_browser_tree_path.clone(),
                ]
        }));
        assert!(plan.identity_delivery.is_none());
        assert!(plan.control.is_none());
        assert!(plan.camoufox_host.is_some());
        assert_eq!(
            plan.capabilities
                .iter()
                .find(|capability| capability.id == EngineCapabilityId::SiteFallback)
                .expect("Host fallback capability")
                .availability,
            EngineCapabilityAvailability::Unavailable
        );
        assert!(plan
            .arguments
            .iter()
            .all(|argument| !argument.contains("token") && !argument.contains("seed")));
        assert_eq!(
            plan.camoufox_host
                .as_ref()
                .expect("Host binding")
                .profile_id,
            format!("silo-{}", silo_id.simple())
        );
        assert_eq!(
            plan.camoufox_host
                .as_ref()
                .expect("Host browser release binding")
                .browser_release,
            "v152.0.4-beta.28"
        );

        let required_profile = |scheme| NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme,
            host: "fp3-upstream.example".to_owned(),
            port: 1080,
            bypass_list: Vec::new(),
            credential_reference: Some(Uuid::nil()),
            external_mihomo: None,
        };
        let mut required_v3 = request.clone();
        required_v3.network_profile = required_profile(ProxyScheme::Http);
        required_v3
            .identity
            .as_mut()
            .expect("identity")
            .network
            .proxy_required = true;
        assert!(matches!(
            adapter.launch_plan(&required_v3),
            Err(EngineError::CapabilityUnavailable(message))
                if message.contains("Artifact/Policy v6")
        ));
        for scheme in [ProxyScheme::Http, ProxyScheme::Socks5] {
            let mut required_v6 = required_v3.clone();
            required_v6.network_profile = required_profile(scheme);
            required_v6
                .camoufox_artifact_binding
                .as_mut()
                .expect("Artifact binding")
                .schema = CAMOUFOX_ARTIFACT_SCHEMA_V6.to_owned();
            let required_plan = adapter
                .launch_plan(&required_v6)
                .expect("required proxy v6 Host plan");
            let serialized = serde_json::to_string(&required_plan).expect("Host plan JSON");
            assert!(!serialized.contains("fp3-upstream.example"));
            assert!(!serialized.contains(&Uuid::nil().to_string()));
            assert!(!serialized.contains("browserProxyServer"));
            assert!(required_plan
                .camoufox_host
                .as_ref()
                .is_some_and(|binding| binding.browser_proxy_server.is_none()));
        }
        let mut missing_binding = request.clone();
        missing_binding.camoufox_artifact_binding = None;
        assert!(matches!(
            adapter.launch_plan(&missing_binding),
            Err(EngineError::InvalidIdentityTemplate(_))
        ));
        let mut fallback_request = request.clone();
        fallback_request.fallback_rules = vec![SiteFallbackRule {
            site_pattern: "*.example.test".to_owned(),
            disable_capabilities: vec![EngineCapabilityId::Canvas],
            action: SiteFallbackAction::RestoreThenReload,
        }];
        assert!(matches!(
            adapter.launch_plan(&fallback_request),
            Err(EngineError::CapabilityUnavailable(_))
        ));

        let mut token_request = request.clone();
        token_request.derived_token = Some(DerivedIdentityToken {
            token_id: Uuid::new_v4(),
            token: "TOKEN-SENTINEL-MUST-NOT-ENTER-CAMOUFOX".to_owned(),
            expires_at: Utc::now() + Duration::minutes(5),
        });
        assert!(matches!(
            adapter.launch_plan(&token_request),
            Err(EngineError::CapabilityUnavailable(message))
                if message.contains("does not accept")
        ));

        let mut misleading_manifest = manifest.clone();
        misleading_manifest
            .capabilities
            .push(EngineCapabilityId::SiteFallback);
        assert!(super::validate_package_manifest(
            EngineAdapterId::Camoufox,
            "152.0.4-beta.28",
            &misleading_manifest,
        )
        .is_err());

        fs::write(&host_path, b"tampered Host entrypoint").expect("tamper Host");
        assert!(adapter.launch_plan(&request).is_err());
        fs::write(&host_path, b"fake Host entrypoint").expect("restore Host");
        fs::write(&browser_tree_path, b"tampered browser tree").expect("tamper browser tree");
        assert!(adapter.launch_plan(&request).is_err());
        fs::write(&browser_tree_path, &browser_tree_bytes).expect("restore browser tree");
        let extra_path = root.join("extra.bin");
        fs::write(&extra_path, b"unexpected package member").expect("extra member");
        assert!(adapter.launch_plan(&request).is_err());
        fs::remove_file(&extra_path).expect("remove extra member");
        let tree_bytes = fs::read(&tree_path).expect("tree bytes");
        fs::write(&tree_path, b"{\"schema\":\"tampered\"}\n").expect("tamper tree");
        assert!(adapter.launch_plan(&request).is_err());
        fs::write(&tree_path, tree_bytes).expect("restore tree");

        let old_root = create_package(
            "camoufox-v2-rejected",
            "152.0.4-beta.28",
            EngineAdapterId::Camoufox,
        );
        let mut old_adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::Camoufox,
            Arc::new(TestPackageVerifier),
        )
        .expect("old Camoufox adapter");
        let old_error = old_adapter
            .install(&EnginePackageRequest {
                package_root: old_root.clone(),
                expected_version: "152.0.4-beta.28".to_owned(),
            })
            .expect_err("v2 Camoufox package must be rejected");
        assert!(matches!(old_error, EngineError::InvalidPackage(_)));
        let _ = fs::remove_dir_all(old_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verified_external_package_supports_launch_update_rollback_and_disable() {
        let root_150 = create_package("engine-150", "150.0.0", EngineAdapterId::ControlledChromium);
        let root_151 = create_package("engine-151", "151.0.0", EngineAdapterId::ControlledChromium);
        let mut adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
        )
        .expect("adapter");
        let install = adapter
            .install(&EnginePackageRequest {
                package_root: root_150.clone(),
                expected_version: "150.0.0".to_owned(),
            })
            .expect("install");
        assert_eq!(install.action, EngineMaintenanceAction::Install);

        let template = chromium_template(150);
        let context = derivation_context(template.template_id);
        let token = adapter
            .derive_identity_token(&context, &DeterministicTestDeriver)
            .expect("token");
        let mut request = EngineLaunchRequest {
            silo_id: Some(context.silo_id),
            session_id: context.session_id,
            profile_directory: root_150.join("profile"),
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
            identity: Some(template.clone()),
            derived_token: Some(token),
            fallback_rules: vec![SiteFallbackRule {
                site_pattern: "*.example.test".to_owned(),
                disable_capabilities: vec![EngineCapabilityId::Canvas],
                action: SiteFallbackAction::RestoreThenReload,
            }],
            camoufox_artifact_binding: None,
            camoufox_roots: None,
        };
        let token_secret = request
            .derived_token
            .as_ref()
            .expect("request token")
            .token
            .clone();
        let plan = adapter
            .launch_plan(&request)
            .expect("controlled launch plan");
        assert!(!plan.shell);
        assert!(plan.identity_delivery.is_some());
        assert!(plan.package_verification.is_some());
        assert!(plan
            .arguments
            .iter()
            .all(|argument| !argument.contains(&token_secret)));
        assert_eq!(
            plan.control.as_ref().expect("control").phases,
            [
                super::EngineControlPhase::Observe,
                super::EngineControlPhase::Apply,
                super::EngineControlPhase::Verify,
                super::EngineControlPhase::Restore,
            ]
        );
        assert_capability(
            &plan.capabilities,
            EngineCapabilityId::Canvas,
            EngineCapabilityAvailability::Experimental,
            EngineCapabilityOperation::Configured,
        );
        assert_capability(
            &plan.capabilities,
            EngineCapabilityId::Quic,
            EngineCapabilityAvailability::Unavailable,
            EngineCapabilityOperation::NotConfigured,
        );

        let envelope = super::EngineBootstrapEnvelope::for_launch(
            context.silo_id,
            context.issued_at,
            &plan,
            request.identity.take().expect("request identity"),
            request.derived_token.take().expect("request token"),
        )
        .expect("bootstrap envelope");
        let debug = format!("{envelope:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(&token_secret));
        let mut framed = Vec::new();
        super::write_engine_bootstrap_frame(&mut framed, &envelope).expect("bootstrap frame");
        assert!(framed.len() <= super::MAX_ENGINE_BOOTSTRAP_BYTES + 4);
        let parsed = super::read_engine_bootstrap_frame(
            &mut Cursor::new(framed.as_slice()),
            EngineAdapterId::ControlledChromium,
            Utc::now(),
        )
        .expect("strict bootstrap parse");
        assert_eq!(parsed.session_id, context.session_id);
        assert_eq!(parsed.identity.template_id, template.template_id);
        let ack = super::EngineBootstrapAck::accepted(&parsed, Utc::now());
        let mut ack_frame = Vec::new();
        super::write_engine_bootstrap_ack_frame(&mut ack_frame, &ack).expect("ACK frame");
        assert!(!String::from_utf8_lossy(&ack_frame).contains(&token_secret));
        let expectation = super::EngineBootstrapAckExpectation::from(&parsed);
        let parsed_ack = super::read_engine_bootstrap_ack_frame(
            &mut Cursor::new(ack_frame),
            &expectation,
            Utc::now(),
        )
        .expect("bound ACK");
        assert_eq!(
            parsed_ack.status,
            super::EngineBootstrapAckStatus::BootstrapApplied
        );
        let mut wrong_ack = ack;
        wrong_ack.session_id = Uuid::new_v4();
        let mut wrong_frame = Vec::new();
        super::write_engine_bootstrap_ack_frame(&mut wrong_frame, &wrong_ack)
            .expect("wrong ACK frame");
        assert!(super::read_engine_bootstrap_ack_frame(
            &mut Cursor::new(wrong_frame),
            &expectation,
            Utc::now(),
        )
        .is_err());

        let receipt = super::EngineRuntimeReceiptFrame::for_envelope(
            &parsed,
            1,
            Utc::now(),
            super::EngineRuntimeReceipt::Phase(super::EngineRuntimePhaseReceipt {
                phase: EngineControlPhase::Observe,
                capabilities: Vec::new(),
            }),
        );
        let mut receipt_frame = Vec::new();
        super::write_engine_runtime_receipt_frame(&mut receipt_frame, &receipt)
            .expect("runtime receipt frame");
        assert!(!String::from_utf8_lossy(&receipt_frame).contains(&token_secret));
        let receipt_expectation = super::EngineRuntimeReceiptExpectation::from(&parsed);
        let parsed_receipt = super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(receipt_frame.clone()),
            &receipt_expectation,
            1,
            Utc::now(),
        )
        .expect("bound runtime receipt");
        assert_eq!(parsed_receipt.sequence, 1);
        assert!(super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(receipt_frame.clone()),
            &receipt_expectation,
            2,
            Utc::now(),
        )
        .is_err());
        let mut wrong_token_receipt = receipt.clone();
        wrong_token_receipt.token_id = Uuid::new_v4();
        let mut wrong_token_frame = Vec::new();
        super::write_engine_runtime_receipt_frame(&mut wrong_token_frame, &wrong_token_receipt)
            .expect("wrong token receipt frame");
        assert!(super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(wrong_token_frame),
            &receipt_expectation,
            1,
            Utc::now(),
        )
        .is_err());
        let mut wrong_package_receipt = receipt.clone();
        wrong_package_receipt.package.artifact_sha256 = "b".repeat(64);
        let mut wrong_package_frame = Vec::new();
        super::write_engine_runtime_receipt_frame(&mut wrong_package_frame, &wrong_package_receipt)
            .expect("wrong package receipt frame");
        assert!(super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(wrong_package_frame),
            &receipt_expectation,
            1,
            Utc::now(),
        )
        .is_err());
        let reflected_secret_receipt = super::EngineRuntimeReceiptFrame::for_envelope(
            &parsed,
            1,
            Utc::now(),
            super::EngineRuntimeReceipt::Phase(super::EngineRuntimePhaseReceipt {
                phase: EngineControlPhase::Observe,
                capabilities: vec![EngineCapabilityEvidence {
                    id: EngineCapabilityId::Canvas,
                    evidence: vec![token_secret.clone()],
                }],
            }),
        );
        let mut reflected_secret_frame = Vec::new();
        super::write_engine_runtime_receipt_frame(
            &mut reflected_secret_frame,
            &reflected_secret_receipt,
        )
        .expect("reflected secret receipt frame");
        assert!(super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(reflected_secret_frame),
            &receipt_expectation,
            1,
            Utc::now(),
        )
        .is_err());

        let receipt_payload_length =
            u32::from_be_bytes(receipt_frame[..4].try_into().expect("receipt header"));
        let mut receipt_value: Value =
            serde_json::from_slice(&receipt_frame[4..4 + receipt_payload_length as usize])
                .expect("receipt json");
        receipt_value["receipt"]["unexpected"] = json!(true);
        let receipt_payload = serde_json::to_vec(&receipt_value).expect("mutated receipt");
        let mut unknown_receipt_frame = (receipt_payload.len() as u32).to_be_bytes().to_vec();
        unknown_receipt_frame.extend_from_slice(&receipt_payload);
        assert!(super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(unknown_receipt_frame),
            &receipt_expectation,
            1,
            Utc::now(),
        )
        .is_err());

        let oversized_receipt_frame = ((super::MAX_ENGINE_RUNTIME_RECEIPT_BYTES + 1) as u32)
            .to_be_bytes()
            .to_vec();
        assert!(super::read_engine_runtime_receipt_frame(
            &mut Cursor::new(oversized_receipt_frame),
            &receipt_expectation,
            1,
            Utc::now(),
        )
        .is_err());

        let payload_length = u32::from_be_bytes(framed[..4].try_into().expect("frame header"));
        let mut value: Value = serde_json::from_slice(&framed[4..4 + payload_length as usize])
            .expect("bootstrap json");
        value["unexpected"] = json!(true);
        let mutated_payload = serde_json::to_vec(&value).expect("mutated bootstrap");
        let mut unknown_field_frame = (mutated_payload.len() as u32).to_be_bytes().to_vec();
        unknown_field_frame.extend_from_slice(&mutated_payload);
        assert!(super::read_engine_bootstrap_frame(
            &mut Cursor::new(unknown_field_frame),
            EngineAdapterId::ControlledChromium,
            Utc::now(),
        )
        .is_err());

        let mut trailing_frame = framed.clone();
        trailing_frame.push(0);
        assert!(super::read_engine_bootstrap_frame(
            &mut Cursor::new(trailing_frame),
            EngineAdapterId::ControlledChromium,
            Utc::now(),
        )
        .is_err());
        let oversized = ((super::MAX_ENGINE_BOOTSTRAP_BYTES + 1) as u32)
            .to_be_bytes()
            .to_vec();
        assert!(super::read_engine_bootstrap_frame(
            &mut Cursor::new(oversized),
            EngineAdapterId::ControlledChromium,
            Utc::now(),
        )
        .is_err());

        let update = adapter
            .update(&EnginePackageRequest {
                package_root: root_151.clone(),
                expected_version: "151.0.0".to_owned(),
            })
            .expect("update");
        assert_eq!(update.action, EngineMaintenanceAction::Update);
        assert_eq!(adapter.descriptor().engine_version, "151.0.0");
        let rollback = adapter.rollback().expect("rollback");
        assert_eq!(rollback.action, EngineMaintenanceAction::Rollback);
        assert_eq!(adapter.descriptor().engine_version, "150.0.0");

        adapter
            .set_emergency_disabled(true, Some("package advisory".to_owned()))
            .expect("disable");
        assert!(matches!(
            adapter.launch_plan(&EngineLaunchRequest {
                silo_id: None,
                session_id: Uuid::new_v4(),
                profile_directory: root_150.join("profile-2"),
                network_profile: NetworkProfile::Direct {
                    proxy_required: false,
                },
                identity: Some(template),
                derived_token: None,
                fallback_rules: Vec::new(),
                camoufox_artifact_binding: None,
                camoufox_roots: None,
            }),
            Err(EngineError::EmergencyDisabled(_))
        ));
        let _ = fs::remove_dir_all(root_150);
        let _ = fs::remove_dir_all(root_151);
    }

    #[test]
    fn package_state_persists_update_rollback_and_emergency_disable() {
        let state_root = test_root("persistent-state");
        let root_150 = create_package(
            "persistent-engine-150",
            "150.0.0",
            EngineAdapterId::ControlledChromium,
        );
        let root_151 = create_package(
            "persistent-engine-151",
            "151.0.0",
            EngineAdapterId::ControlledChromium,
        );
        {
            let mut adapter = ExternalPackageEngineAdapter::with_verifier_and_state(
                EngineAdapterId::ControlledChromium,
                Arc::new(TestPackageVerifier),
                state_root.clone(),
            )
            .expect("persistent adapter");
            adapter
                .install(&EnginePackageRequest {
                    package_root: root_150.clone(),
                    expected_version: "150.0.0".to_owned(),
                })
                .expect("persistent install");
            adapter
                .update(&EnginePackageRequest {
                    package_root: root_151.clone(),
                    expected_version: "151.0.0".to_owned(),
                })
                .expect("persistent update");
        }
        {
            let mut adapter = ExternalPackageEngineAdapter::with_verifier_and_state(
                EngineAdapterId::ControlledChromium,
                Arc::new(TestPackageVerifier),
                state_root.clone(),
            )
            .expect("reloaded adapter");
            assert_eq!(adapter.descriptor().engine_version, "151.0.0");
            adapter.rollback().expect("persistent rollback");
            assert_eq!(adapter.descriptor().engine_version, "150.0.0");
            adapter
                .set_emergency_disabled(true, Some("revoked build".to_owned()))
                .expect("persist emergency disable");
        }
        let adapter = ExternalPackageEngineAdapter::with_verifier_and_state(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
            state_root.clone(),
        )
        .expect("disabled adapter reload");
        assert!(adapter.descriptor().emergency_disabled);
        assert_eq!(
            adapter.health().state,
            super::EngineHealthState::EmergencyDisabled
        );
        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(root_150);
        let _ = fs::remove_dir_all(root_151);
    }

    #[test]
    fn lifecycle_reverification_detects_tampering_and_blocks_update() {
        let root_150 = create_package(
            "tamper-active-150",
            "150.0.0",
            EngineAdapterId::ControlledChromium,
        );
        let root_151 = create_package(
            "tamper-update-151",
            "151.0.0",
            EngineAdapterId::ControlledChromium,
        );
        let mut adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
        )
        .expect("adapter");
        adapter
            .install(&EnginePackageRequest {
                package_root: root_150.clone(),
                expected_version: "150.0.0".to_owned(),
            })
            .expect("install");
        fs::write(root_150.join("bin/chromium.exe"), b"tampered engine")
            .expect("tamper executable");
        assert_eq!(
            adapter.health().state,
            super::EngineHealthState::Unavailable
        );
        assert!(matches!(
            adapter.update(&EnginePackageRequest {
                package_root: root_151.clone(),
                expected_version: "151.0.0".to_owned(),
            }),
            Err(EngineError::VerificationUnavailable(_))
        ));
        adapter
            .set_emergency_disabled(true, Some("tamper detected".to_owned()))
            .expect("safety disable must remain available after tamper");
        assert!(matches!(
            adapter.set_emergency_disabled(false, None),
            Err(EngineError::VerificationUnavailable(_))
        ));
        let _ = fs::remove_dir_all(root_150);
        let _ = fs::remove_dir_all(root_151);
    }

    #[test]
    fn package_rejects_unknown_fields_version_platform_and_traversal() {
        for (label, mutation) in [
            (
                "unknown",
                Box::new(|manifest: &mut Value| manifest["unexpected"] = json!(true))
                    as Box<dyn Fn(&mut Value)>,
            ),
            (
                "platform",
                Box::new(|manifest: &mut Value| manifest["platform"] = json!("linux-x64")),
            ),
            (
                "traversal",
                Box::new(|manifest: &mut Value| {
                    manifest["executableRelativePath"] = json!("../chromium.exe")
                }),
            ),
        ] {
            let root = create_package(label, "150.0.0", EngineAdapterId::ControlledChromium);
            mutate_manifest(&root, mutation);
            let mut adapter = ExternalPackageEngineAdapter::with_verifier(
                EngineAdapterId::ControlledChromium,
                Arc::new(TestPackageVerifier),
            )
            .expect("adapter");
            assert!(adapter
                .install(&EnginePackageRequest {
                    package_root: root.clone(),
                    expected_version: "150.0.0".to_owned(),
                })
                .is_err());
            let _ = fs::remove_dir_all(root);
        }

        let root = create_package("version", "150.0.0", EngineAdapterId::ControlledChromium);
        let mut adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
        )
        .expect("adapter");
        assert!(adapter
            .install(&EnginePackageRequest {
                package_root: root.clone(),
                expected_version: "151.0.0".to_owned(),
            })
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_rejects_wrong_hash_signer_and_signature() {
        for (label, mutation) in [
            (
                "wrong-hash",
                Box::new(|manifest: &mut Value| manifest["artifactSha256"] = json!("0".repeat(64)))
                    as Box<dyn Fn(&mut Value)>,
            ),
            (
                "wrong-signer",
                Box::new(|manifest: &mut Value| {
                    manifest["signature"]["keyId"] = json!("2".repeat(64))
                }),
            ),
            (
                "wrong-signature",
                Box::new(|manifest: &mut Value| {
                    manifest["signature"]["value"] = json!("B".repeat(256))
                }),
            ),
        ] {
            let root = create_package(label, "150.0.0", EngineAdapterId::ControlledChromium);
            mutate_manifest(&root, mutation);
            let mut adapter = ExternalPackageEngineAdapter::with_verifier(
                EngineAdapterId::ControlledChromium,
                Arc::new(TestPackageVerifier),
            )
            .expect("adapter");
            assert!(matches!(
                adapter.install(&EnginePackageRequest {
                    package_root: root.clone(),
                    expected_version: "150.0.0".to_owned(),
                }),
                Err(EngineError::VerificationUnavailable(_))
            ));
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn persisted_state_rejects_unknown_fields() {
        let state_root = test_root("unknown-state");
        let package_root = create_package(
            "unknown-state-package",
            "150.0.0",
            EngineAdapterId::ControlledChromium,
        );
        {
            let mut adapter = ExternalPackageEngineAdapter::with_verifier_and_state(
                EngineAdapterId::ControlledChromium,
                Arc::new(TestPackageVerifier),
                state_root.clone(),
            )
            .expect("persistent adapter");
            adapter
                .install(&EnginePackageRequest {
                    package_root: package_root.clone(),
                    expected_version: "150.0.0".to_owned(),
                })
                .expect("install");
        }
        let state_path = state_root.join("controlled-chromium-state.json");
        let mut state: Value =
            serde_json::from_slice(&fs::read(&state_path).expect("state bytes")).expect("state");
        state["unexpected"] = json!(true);
        fs::write(
            &state_path,
            serde_json::to_vec_pretty(&state).expect("state json"),
        )
        .expect("mutate state");
        assert!(ExternalPackageEngineAdapter::with_verifier_and_state(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
            state_root.clone(),
        )
        .is_err());
        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(package_root);
    }

    #[cfg(unix)]
    #[test]
    fn persisted_state_rejects_dangling_and_external_symlinks() {
        use std::os::unix::fs::symlink;

        for (label, target_exists) in [("external", true), ("dangling", false)] {
            let state_root = test_root(&format!("symlink-state-{label}"));
            fs::create_dir_all(&state_root).expect("state root");
            let target = test_root(&format!("symlink-target-{label}"));
            if target_exists {
                fs::write(&target, b"{}").expect("external state fixture");
            }
            symlink(&target, state_root.join("controlled-chromium-state.json"))
                .expect("state symlink");
            assert!(matches!(
                ExternalPackageEngineAdapter::with_verifier_and_state(
                    EngineAdapterId::ControlledChromium,
                    Arc::new(TestPackageVerifier),
                    state_root.clone(),
                ),
                Err(EngineError::UnsafePath(_))
            ));
            let _ = fs::remove_dir_all(state_root);
            let _ = fs::remove_file(target);
        }
    }

    #[cfg(unix)]
    #[test]
    fn package_rejects_symlinked_executable() {
        use std::os::unix::fs::symlink;

        let root = create_package("symlink", "150.0.0", EngineAdapterId::ControlledChromium);
        let executable = root.join("bin/chromium.exe");
        fs::remove_file(&executable).expect("remove fixture");
        let outside = test_root("outside-executable");
        fs::write(&outside, b"outside").expect("outside fixture");
        symlink(&outside, &executable).expect("symlink");
        let mut adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
        )
        .expect("adapter");
        assert!(matches!(
            adapter.install(&EnginePackageRequest {
                package_root: root.clone(),
                expected_version: "150.0.0".to_owned(),
            }),
            Err(EngineError::UnsafePath(_))
        ));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(outside);
    }

    #[test]
    fn constrained_identity_rejects_mismatch_and_default_deriver_is_unavailable() {
        let root = create_package("identity", "150.0.0", EngineAdapterId::ControlledChromium);
        let mut adapter = ExternalPackageEngineAdapter::with_verifier(
            EngineAdapterId::ControlledChromium,
            Arc::new(TestPackageVerifier),
        )
        .expect("adapter");
        adapter
            .install(&EnginePackageRequest {
                package_root: root.clone(),
                expected_version: "150.0.0".to_owned(),
            })
            .expect("install");
        let mut invalid = chromium_template(150);
        invalid.timezone = "America/New_York".to_owned();
        assert!(matches!(
            adapter.validate_identity_template(&invalid),
            Err(EngineError::InvalidIdentityTemplate(_))
        ));
        assert!(matches!(
            adapter.derive_identity_token(
                &derivation_context(chromium_template(150).template_id),
                &UnavailableIdentityTokenDeriver
            ),
            Err(EngineError::VerificationUnavailable(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn derived_identity_token_debug_output_is_redacted() {
        let secret = "session-secret-value-that-must-never-be-logged";
        let token = DerivedIdentityToken {
            token_id: Uuid::new_v4(),
            token: secret.to_owned(),
            expires_at: Utc::now() + Duration::minutes(30),
        };
        let debug = format!("{token:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(secret));
    }

    #[test]
    fn vault_seed_tokens_are_deterministic_domain_separated_and_redacted() {
        let seed = [0x5a_u8; 32];
        let deriver = super::VaultSeedIdentityTokenDeriver::new(&seed).expect("seed deriver");
        let context = derivation_context(Uuid::new_v4());
        let first = deriver
            .derive_session_token(&context)
            .expect("first session token");
        let second = deriver
            .derive_session_token(&context)
            .expect("second session token");
        assert_eq!(first.token_id, second.token_id);
        assert_eq!(first.token, second.token);
        assert_eq!(first.token.len(), 43);

        let mut other_session = context.clone();
        other_session.session_id = Uuid::new_v4();
        let separated = deriver
            .derive_session_token(&other_session)
            .expect("domain-separated token");
        assert_ne!(first.token_id, separated.token_id);
        assert_ne!(first.token, separated.token);

        let debug = format!("{deriver:?} {first:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(&first.token));
        assert!(!debug.contains(&hex_lower(&seed)));
    }

    #[test]
    fn silo_engine_selection_is_strict_and_profiles_never_overlap() {
        let template = chromium_template(150);
        let controlled = super::SiloEngineConfig::ControlledChromium {
            identity_template: template.clone(),
            fallback_rules: vec![SiteFallbackRule {
                site_pattern: "*.example.test".to_owned(),
                disable_capabilities: vec![EngineCapabilityId::Canvas],
                action: SiteFallbackAction::RestoreThenReload,
            }],
        };
        controlled.validate(true).expect("valid controlled config");
        assert!(controlled.validate(false).is_err());

        let root = Path::new("C:/VeriSilo/silos/example/browser-data");
        let profiles = super::SiloEngineConfig::all_profile_directories(root);
        assert_eq!(profiles.iter().collect::<BTreeSet<_>>().len(), 3);
        assert_eq!(
            controlled.profile_directory(root),
            root.join("engines").join("controlled-chromium")
        );
        assert_eq!(
            super::SiloEngineConfig::Camoufox {
                identity_template: Some(template.clone()),
                fallback_rules: Vec::new(),
                artifact_binding: None,
            }
            .profile_directory(root),
            root.join("engines").join("camoufox")
        );

        let encoded = serde_json::to_value(&controlled).expect("engine config json");
        assert_eq!(encoded["adapter"], "controlled-chromium");
        assert!(encoded.get("identityTemplate").is_some());
        assert!(serde_json::from_value::<super::SiloEngineConfig>(json!({
            "adapter": "arbitrary-adapter",
            "arguments": ["--unsafe"]
        }))
        .is_err());
        assert!(serde_json::from_value::<super::SiloEngineConfig>(json!({
            "adapter": "controlled-chromium",
            "identityTemplate": encoded["identityTemplate"].clone()
        }))
        .is_err());
        let wrong_family = super::SiloEngineConfig::Camoufox {
            identity_template: Some(template),
            fallback_rules: Vec::new(),
            artifact_binding: None,
        };
        assert!(wrong_family.validate(true).is_err());
    }

    #[test]
    fn control_execution_requires_ordered_evidence_and_restores_site_fallback() {
        let now = Utc::now();
        let mut profile = super::EngineCapabilityState::declared(
            EngineCapabilityId::ProfileIsolation,
            EngineCapabilityAvailability::Supported,
            "fixture",
        );
        profile
            .transition(EngineCapabilityOperation::Configured, Vec::new(), now)
            .expect("configure profile");
        let mut canvas = super::EngineCapabilityState::declared(
            EngineCapabilityId::Canvas,
            EngineCapabilityAvailability::Experimental,
            "fixture",
        );
        canvas
            .transition(EngineCapabilityOperation::Configured, Vec::new(), now)
            .expect("configure canvas");
        let plan = super::EngineControlPlan {
            session_id: Uuid::new_v4(),
            template_id: Uuid::new_v4(),
            phases: [
                EngineControlPhase::Observe,
                EngineControlPhase::Apply,
                EngineControlPhase::Verify,
                EngineControlPhase::Restore,
            ],
            capabilities: vec![profile, canvas],
            site_fallback: super::SiteFallbackPolicy {
                default_action: SiteFallbackAction::RestoreExperimentalControls,
                rules: vec![SiteFallbackRule {
                    site_pattern: "*.example.test".to_owned(),
                    disable_capabilities: vec![EngineCapabilityId::Canvas],
                    action: SiteFallbackAction::RestoreThenReload,
                }],
            },
        };
        let mut execution = EngineControlExecution::from_plan(plan);
        assert!(execution
            .record_phase(EngineControlPhase::Apply, Vec::new(), now)
            .is_err());

        let observe_evidence =
            evidence_for_operation(&execution, EngineCapabilityOperation::Configured, "base");
        execution
            .record_phase(EngineControlPhase::Observe, observe_evidence, now)
            .expect("observe");
        let apply_evidence =
            evidence_for_operation(&execution, EngineCapabilityOperation::Configured, "apply");
        execution
            .record_phase(
                EngineControlPhase::Apply,
                apply_evidence,
                now + Duration::seconds(1),
            )
            .expect("apply");
        let verify_evidence =
            evidence_for_operation(&execution, EngineCapabilityOperation::Applied, "verify");
        execution
            .record_phase(
                EngineControlPhase::Verify,
                verify_evidence,
                now + Duration::seconds(2),
            )
            .expect("verify");
        let fallback = execution
            .apply_site_fallback(
                "login.example.test",
                vec![EngineCapabilityEvidence {
                    id: EngineCapabilityId::Canvas,
                    evidence: vec!["compatibility probe requested restore".to_owned()],
                }],
                now + Duration::seconds(3),
            )
            .expect("fallback")
            .expect("matching fallback");
        assert_eq!(fallback.action, SiteFallbackAction::RestoreThenReload);
        let restore_evidence =
            evidence_for_operation(&execution, EngineCapabilityOperation::Verified, "restore");
        execution
            .record_phase(
                EngineControlPhase::Restore,
                restore_evidence,
                now + Duration::seconds(4),
            )
            .expect("restore");
        assert!(execution.next_phase.is_none());
        assert_eq!(execution.phase_receipts.len(), 4);
        assert_eq!(execution.fallback_receipts.len(), 1);
        assert!(execution
            .capabilities
            .iter()
            .all(|capability| capability.operation == EngineCapabilityOperation::NotConfigured));
    }

    #[test]
    fn sha256_and_manifest_signing_payload_are_deterministic() {
        assert_eq!(
            hex_lower(&sha256_bytes(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_lower(&sha256_bytes(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_lower(&super::hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        let root = create_package(
            "canonical-payload",
            "150.0.0",
            EngineAdapterId::ControlledChromium,
        );
        let manifest: EnginePackageManifest =
            serde_json::from_slice(&fs::read(root.join("engine-package.json")).expect("manifest"))
                .expect("manifest json");
        let payload = super::manifest_signing_payload(&manifest).expect("payload");
        let mut signature_changed = manifest.clone();
        signature_changed.signature.value = "B".repeat(256);
        assert_eq!(
            payload,
            super::manifest_signing_payload(&signature_changed).expect("payload")
        );
        let mut metadata_changed = manifest;
        metadata_changed.engine_version = "151.0.0".to_owned();
        assert_ne!(
            payload,
            super::manifest_signing_payload(&metadata_changed).expect("payload")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_manifest_signing_payload_matches_rc1_field_order() {
        let manifest = EnginePackageManifest {
            schema_version: super::CAMOUFOX_HOST_PACKAGE_SCHEMA_VERSION,
            engine_id: EngineAdapterId::Camoufox,
            engine_version: super::CAMOUFOX_FORMAL_V3_ENGINE_VERSION.to_owned(),
            channel: EngineChannel::Experimental,
            platform: super::WINDOWS_X64_PLATFORM.to_owned(),
            executable_relative_path: String::new(),
            artifact_sha256: "a".repeat(64),
            signature: EnginePackageSignature {
                algorithm: super::CMS_SHA256_ALGORITHM.to_owned(),
                key_id: "b".repeat(64),
                value: "C".repeat(256),
            },
            capabilities: vec![EngineCapabilityId::IdentityTemplate],
            entrypoint: Some(EnginePackageEntrypoint {
                kind: CAMOUFOX_HOST_ENTRYPOINT_KIND.to_owned(),
                relative_path: "host/camoufox-host.exe".to_owned(),
                protocol: CAMOUFOX_HOST_PROTOCOL.to_owned(),
                sha256: "c".repeat(64),
            }),
            tree_manifest: Some(EnginePackageTreeBinding {
                relative_path: "package-tree.json".to_owned(),
                sha256: "d".repeat(64),
            }),
            browser_tree_manifest: Some(EnginePackageTreeBinding {
                relative_path: "browser-tree-manifest.json".to_owned(),
                sha256: "e".repeat(64),
            }),
            host_version: Some(super::CAMOUFOX_HOST_VERSION.to_owned()),
            browser_release: Some(super::CAMOUFOX_FORMAL_V3_BROWSER_RELEASE.to_owned()),
            browser_asset_sha256: Some(super::CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256.to_owned()),
        };
        let payload = super::manifest_signing_payload(&manifest).expect("RC1 signing payload");
        let encoded = String::from_utf8(payload).expect("UTF-8 signing payload");
        let host_position = encoded.find("\"hostVersion\"").expect("hostVersion");
        let release_position = encoded.find("\"browserRelease\"").expect("browserRelease");
        let asset_position = encoded
            .find("\"browserAssetSha256\"")
            .expect("browserAssetSha256");
        assert!(host_position < release_position && release_position < asset_position);
        assert!(encoded.ends_with(&format!(
            "\"browserAssetSha256\":\"{}\"}}",
            super::CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256
        )));
    }

    #[test]
    fn camoufox_package_tree_has_an_independent_four_mib_ceiling() {
        let medium = vec![b' '; 64 * 1024 + 1];
        assert!(super::validate_camoufox_package_tree_size(&medium).is_ok());
        let oversized = vec![b' '; 4 * 1024 * 1024 + 1];
        assert!(super::validate_camoufox_package_tree_size(&oversized).is_err());
    }

    #[test]
    fn trusted_signer_policy_rejects_unknown_and_duplicate_entries() {
        WindowsProductionEnginePackageVerifier::from_policy_bytes(
            br#"{"schemaVersion":1,"signers":[]}"#,
        )
        .expect("empty policy is fail-closed but structurally valid");
        assert!(WindowsProductionEnginePackageVerifier::from_policy_bytes(
            br#"{"schemaVersion":1,"signers":[],"unknown":true}"#,
        )
        .is_err());
        let pin = "1".repeat(64);
        let duplicate = json!({
            "schemaVersion": 1,
            "signers": [
                { "certificateSha256": pin, "publisher": "Publisher A" },
                { "certificateSha256": "1".repeat(64), "publisher": "Publisher B" }
            ]
        });
        assert!(WindowsProductionEnginePackageVerifier::from_policy_bytes(
            &serde_json::to_vec(&duplicate).expect("policy")
        )
        .is_err());
    }

    #[test]
    fn semantic_version_comparison_rejects_downgrade_and_leading_zero() {
        assert_eq!(
            super::compare_engine_versions("151.0.0", "150.9.9"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            super::compare_engine_versions("150.0.0-rc.2", "150.0.0-rc.1"),
            Some(std::cmp::Ordering::Greater)
        );
        assert!(!super::valid_engine_version("150.01.0"));
        assert!(!super::valid_engine_version("150.0.0-01"));
    }

    fn evidence_for_operation(
        execution: &EngineControlExecution,
        operation: EngineCapabilityOperation,
        label: &str,
    ) -> Vec<EngineCapabilityEvidence> {
        execution
            .capabilities
            .iter()
            .filter(|capability| capability.operation == operation)
            .map(|capability| EngineCapabilityEvidence {
                id: capability.id,
                evidence: vec![format!("{label}:{:?}", capability.id)],
            })
            .collect()
    }

    fn create_package(label: &str, version: &str, id: EngineAdapterId) -> std::path::PathBuf {
        let root = test_root(label);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("package bin");
        let executable = match id {
            EngineAdapterId::ControlledChromium => "chromium.exe",
            EngineAdapterId::Camoufox => "camoufox.exe",
            EngineAdapterId::StockChrome | EngineAdapterId::StockEdge => unreachable!(),
        };
        let executable_path = bin.join(executable);
        fs::write(&executable_path, b"engine fixture").expect("engine executable");
        let manifest = EnginePackageManifest {
            schema_version: 2,
            engine_id: id,
            engine_version: version.to_owned(),
            channel: EngineChannel::Experimental,
            platform: "windows-x64".to_owned(),
            executable_relative_path: format!("bin/{executable}"),
            artifact_sha256: hex_lower(&sha256_file(&executable_path).expect("fixture digest")),
            signature: super::EnginePackageSignature {
                algorithm: super::CMS_SHA256_ALGORITHM.to_owned(),
                key_id: "1".repeat(64),
                value: "A".repeat(256),
            },
            capabilities: super::identity_capabilities()
                .into_iter()
                .chain([EngineCapabilityId::SiteFallback])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            entrypoint: None,
            tree_manifest: None,
            browser_tree_manifest: None,
            host_version: None,
            browser_asset_sha256: None,
            browser_release: None,
        };
        fs::write(
            root.join("engine-package.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest json"),
        )
        .expect("manifest");
        root
    }

    fn mutate_manifest(root: &Path, mutation: Box<dyn Fn(&mut Value)>) {
        let path = root.join("engine-package.json");
        let mut value: Value =
            serde_json::from_slice(&fs::read(&path).expect("read manifest")).expect("json");
        mutation(&mut value);
        fs::write(path, serde_json::to_vec_pretty(&value).expect("json")).expect("write");
    }

    fn chromium_template(major: u16) -> IdentityTemplate {
        IdentityTemplate {
            schema_version: 1,
            template_id: Uuid::new_v4(),
            os: IdentityOperatingSystem {
                family: "windows".to_owned(),
                version: "11".to_owned(),
                architecture: "x64".to_owned(),
            },
            browser: IdentityBrowser {
                family: BrowserFamily::Chromium,
                major_version: major,
                user_agent: format!(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/{major}.0.0.0 Safari/537.36"
                ),
                ua_ch: Some(IdentityUaCh {
                    brands: vec![IdentityUaChBrand {
                        brand: "Chromium".to_owned(),
                        version: major.to_string(),
                    }],
                    platform: "Windows".to_owned(),
                    platform_version: "15.0.0".to_owned(),
                    architecture: "x86".to_owned(),
                    bitness: "64".to_owned(),
                    mobile: false,
                }),
            },
            languages: IdentityLanguages {
                primary: "zh-CN".to_owned(),
                accepted: vec!["zh-CN".to_owned(), "en-US".to_owned()],
            },
            timezone: "Asia/Singapore".to_owned(),
            screen: IdentityScreen {
                width: 1920,
                height: 1080,
                available_width: 1920,
                available_height: 1040,
                device_pixel_ratio: 1.0,
                color_depth: 24,
            },
            render: IdentityRender {
                canvas: CanvasMode::Controlled,
                web_gl_vendor: Some("Google Inc. (NVIDIA)".to_owned()),
                web_gl_renderer: Some("ANGLE (NVIDIA)".to_owned()),
            },
            fonts: IdentityFonts {
                families: vec!["Segoe UI".to_owned(), "Arial".to_owned()],
            },
            media: IdentityMedia {
                microphones: 1,
                cameras: 1,
                speakers: 1,
                labels_exposed: false,
            },
            network: IdentityNetwork {
                proxy_required: true,
                country_code: Some("SG".to_owned()),
                timezone: Some("Asia/Singapore".to_owned()),
                locale: Some("zh-CN".to_owned()),
                desired_quic: DesiredQuic::BrowserDefault,
            },
        }
    }

    fn firefox_template(major: u16) -> IdentityTemplate {
        let mut template = chromium_template(major);
        template.browser.family = BrowserFamily::Firefox;
        template.browser.user_agent = format!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:{major}.0) Gecko/20100101 Firefox/{major}.0"
        );
        template.browser.ua_ch = None;
        template.network.proxy_required = false;
        template
    }

    fn derivation_context(template_id: Uuid) -> IdentityDerivationContext {
        let issued_at = Utc::now();
        IdentityDerivationContext {
            silo_id: Uuid::new_v4(),
            seed_reference: Uuid::new_v4(),
            template_id,
            session_id: Uuid::new_v4(),
            issued_at,
            expires_at: issued_at + Duration::minutes(30),
        }
    }

    fn assert_capability(
        capabilities: &[super::EngineCapabilityState],
        id: EngineCapabilityId,
        availability: EngineCapabilityAvailability,
        operation: EngineCapabilityOperation,
    ) {
        let capability = capabilities
            .iter()
            .find(|capability| capability.id == id)
            .expect("capability");
        assert_eq!(capability.availability, availability);
        assert_eq!(capability.operation, operation);
    }

    fn test_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("verisilo-engine-{label}-{}", Uuid::new_v4()))
    }
}
