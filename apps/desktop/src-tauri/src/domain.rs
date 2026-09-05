use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::engine::{
    EngineAdapterId, EngineCapabilityState, EngineControlExecution, EngineControlPhaseReceipt,
    SiloEngineConfig, SiteFallbackReceipt,
};

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserKind {
    Chrome,
    Edge,
}

impl BrowserKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Edge => "Microsoft Edge",
        }
    }

    fn known_relative_paths(&self) -> &'static [&'static str] {
        match self {
            Self::Chrome => &[
                "Google\\Chrome\\Application\\chrome.exe",
                "Google\\Chrome Beta\\Application\\chrome.exe",
            ],
            Self::Edge => &["Microsoft\\Edge\\Application\\msedge.exe"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDescriptor {
    pub kind: BrowserKind,
    pub executable_path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SiloExecutionTarget {
    Local,
    Wsl { distribution: String },
    Remote { endpoint_origin: String },
}

impl Default for SiloExecutionTarget {
    fn default() -> Self {
        Self::Local
    }
}

impl SiloExecutionTarget {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Local => Ok(()),
            Self::Wsl { distribution } => {
                if distribution.trim().is_empty()
                    || distribution.len() > 128
                    || distribution.chars().any(char::is_control)
                {
                    return Err(DomainError::InvalidSilo(
                        "The WSL distribution must be non-empty, at most 128 bytes, and contain no control characters."
                            .to_owned(),
                    ));
                }
                Ok(())
            }
            Self::Remote { endpoint_origin } => {
                if endpoint_origin.trim() != endpoint_origin
                    || endpoint_origin.is_empty()
                    || endpoint_origin.len() > 2_048
                    || endpoint_origin.chars().any(char::is_control)
                {
                    return Err(DomainError::InvalidSilo(
                        "The remote endpoint origin is empty, too long, or contains unsafe characters."
                            .to_owned(),
                    ));
                }
                let parsed = Url::parse(endpoint_origin).map_err(|_| {
                    DomainError::InvalidSilo(
                        "The remote endpoint must be a valid HTTPS origin.".to_owned(),
                    )
                })?;
                let authority_and_suffix = endpoint_origin
                    .find("://")
                    .and_then(|separator| endpoint_origin.get(separator + 3..))
                    .ok_or_else(|| {
                        DomainError::InvalidSilo(
                            "The remote endpoint must be a valid HTTPS origin.".to_owned(),
                        )
                    })?;
                let suffix_start = authority_and_suffix
                    .find(['/', '?', '#'])
                    .unwrap_or(authority_and_suffix.len());
                let (raw_authority, raw_suffix) = authority_and_suffix.split_at(suffix_start);
                if parsed.scheme() != "https"
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || parsed.path() != "/"
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
                    || raw_authority.contains('@')
                    || !matches!(raw_suffix, "" | "/")
                {
                    return Err(DomainError::InvalidSilo(
                        "The remote endpoint must be an HTTPS origin without credentials, a path, query, or fragment."
                            .to_owned(),
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn validate_network_input(
        &self,
        network_profile: &NetworkProfile,
        proxy_credentials: Option<&ProxyCredentialsInput>,
        mihomo_controller_secret: Option<&MihomoControllerSecretInput>,
    ) -> Result<(), DomainError> {
        validate_network_replacement(network_profile, proxy_credentials, mihomo_controller_secret)?;
        let wsl_network_supported = match network_profile {
            NetworkProfile::Direct { proxy_required } => !proxy_required,
            NetworkProfile::FixedProxy {
                scheme: ProxyScheme::Socks5,
                host,
                port,
                bypass_list,
                credential_reference: None,
                external_mihomo: None,
                ..
            } => host == "127.0.0.1" && *port > 0 && bypass_list.is_empty(),
            _ => false,
        };
        match self {
            Self::Local => Ok(()),
            Self::Wsl { .. }
                if proxy_credentials.is_none()
                    && mihomo_controller_secret.is_none()
                    && wsl_network_supported =>
            {
                Ok(())
            }
            Self::Wsl { .. } => Err(DomainError::InvalidNetwork(
                "A WSL Silo currently accepts direct access or a credential-free loopback SOCKS5 endpoint inside that Linux environment."
                    .to_owned(),
            )),
            Self::Remote { .. } => Err(DomainError::InvalidNetwork(
                "Remote Silo network changes are unavailable until the remote browser identity runtime is verified."
                    .to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCandidate {
    pub kind: BrowserKind,
    pub display_name: String,
    pub executable_path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerification {
    pub state: BrowserVerificationState,
    pub expected_kind: BrowserKind,
    pub expected_version: Option<String>,
    pub actual_version: Option<String>,
    pub executable_path: String,
    pub checked_at: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserVerificationState {
    Verified,
    BaselineMissing,
    VersionDrift,
    Missing,
    PathChanged,
    KindMismatch,
    PublisherMismatch,
    ProbeFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserInspection {
    pub resolved_path: String,
    pub version: String,
}

#[derive(Debug, Error)]
pub enum BrowserVerificationError {
    #[error("所选浏览器可执行文件不存在或不是普通文件。")]
    Missing,
    #[error("所选浏览器路径解析失败：{0}")]
    Path(std::io::Error),
    #[error("所选文件名与浏览器类型不一致。")]
    FilenameMismatch,
    #[error("浏览器 --version 检查失败：{0}")]
    Probe(String),
    #[error("浏览器 --version 输出与所选 Chrome/Edge 类型不一致。")]
    KindMismatch,
    #[error("Windows Authenticode 发布者与所选 Chrome/Edge 类型不一致。")]
    PublisherMismatch,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
pub enum WindowsSystemTool {
    PowerShell,
    Tasklist,
    Wsl,
    WindowsSandbox,
    Certutil,
}

#[cfg(target_os = "windows")]
pub fn trusted_windows_system_tool(tool: WindowsSystemTool) -> Result<PathBuf, DomainError> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }

    let mut buffer = vec![0_u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        return Err(DomainError::InvalidSilo(
            "Windows system directory could not be resolved safely.".to_owned(),
        ));
    }
    let system_directory =
        PathBuf::from(String::from_utf16(&buffer[..length as usize]).map_err(|_| {
            DomainError::InvalidSilo("Windows system directory is not valid UTF-16.".to_owned())
        })?);
    let candidate = match tool {
        WindowsSystemTool::PowerShell => system_directory
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
        WindowsSystemTool::Tasklist => system_directory.join("tasklist.exe"),
        WindowsSystemTool::Wsl => system_directory.join("wsl.exe"),
        WindowsSystemTool::WindowsSandbox => system_directory.join("WindowsSandbox.exe"),
        WindowsSystemTool::Certutil => system_directory.join("certutil.exe"),
    };
    let canonical_system = fs::canonicalize(&system_directory)?;
    let candidate_metadata = fs::symlink_metadata(&candidate)?;
    let canonical_candidate = fs::canonicalize(&candidate)?;
    if !canonical_candidate.starts_with(&canonical_system)
        || !candidate_metadata.is_file()
        || candidate_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(DomainError::InvalidSilo(
            "Windows system tool path is outside System32, missing, or a reparse point.".to_owned(),
        ));
    }
    // `std::fs::canonicalize` returns an extended-length (`\\?\`) path on Windows.
    // Windows PowerShell 5.1 cannot initialize reliably when launched through that spelling,
    // so preserve the fixed System32 path after validating it against the canonical root.
    Ok(candidate)
}

/// Console-subsystem children (PowerShell, wsl.exe, browser `--version`)
/// otherwise flash a visible window. Keep redirected stdio; hide the window.
#[cfg(target_os = "windows")]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn hide_windows_console(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = command;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NetworkProfile {
    #[serde(rename = "direct")]
    Direct { proxy_required: bool },
    #[serde(rename = "fixed_proxy")]
    FixedProxy {
        proxy_required: bool,
        scheme: ProxyScheme,
        host: String,
        port: u16,
        bypass_list: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_reference: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        external_mihomo: Option<ExternalMihomoBinding>,
    },
    #[serde(rename = "pac")]
    Pac {
        proxy_required: bool,
        pac_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalMihomoBinding {
    pub controller_url: String,
    pub selector_group: String,
    pub node_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_secret_reference: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyScheme {
    Http,
    Https,
    Socks4,
    Socks5,
}

impl ProxyScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks4 => "socks4",
            Self::Socks5 => "socks5",
        }
    }
}

impl NetworkProfile {
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Direct { proxy_required } if *proxy_required => Err(DomainError::InvalidNetwork(
                "A direct profile cannot require a proxy.".to_owned(),
            )),
            Self::Direct { .. } => Ok(()),
            Self::FixedProxy {
                proxy_required,
                scheme,
                host,
                port,
                bypass_list,
                credential_reference,
                external_mihomo,
                ..
            } => {
                if host.trim().is_empty()
                    || host.len() > 253
                    || !host.chars().all(|character| {
                        character.is_ascii_alphanumeric() || ".:-[]".contains(character)
                    })
                {
                    return Err(DomainError::InvalidNetwork(
                        "Proxy host contains unsupported characters.".to_owned(),
                    ));
                }

                if *port == 0 {
                    return Err(DomainError::InvalidNetwork(
                        "Proxy port must be between 1 and 65535.".to_owned(),
                    ));
                }

                if bypass_list.len() > 100
                    || bypass_list.iter().any(|entry| {
                        entry.trim().is_empty()
                            || entry.len() > 255
                            || entry.contains(';')
                            || entry.chars().any(char::is_control)
                    })
                {
                    return Err(DomainError::InvalidNetwork(
                        "Proxy bypass entries must be short, non-empty values without separators."
                            .to_owned(),
                    ));
                }

                if self.requires_proxy() && !bypass_list.is_empty() {
                    return Err(DomainError::InvalidNetwork(
                        "A required proxy profile cannot contain direct bypass rules.".to_owned(),
                    ));
                }

                if credential_reference.is_some()
                    && !matches!(scheme, ProxyScheme::Http | ProxyScheme::Socks5)
                {
                    return Err(DomainError::InvalidNetwork(
                        "Stored proxy credentials require an HTTP or SOCKS5 upstream.".to_owned(),
                    ));
                }

                if let Some(binding) = external_mihomo {
                    if !*proxy_required
                        || !matches!(scheme, ProxyScheme::Socks5)
                        || !is_loopback_host(host)
                    {
                        return Err(DomainError::InvalidNetwork(
                            "An external Mihomo binding requires a fail-closed loopback SOCKS5 endpoint."
                                .to_owned(),
                        ));
                    }
                    validate_mihomo_binding(binding)?;
                }

                Ok(())
            }
            Self::Pac {
                proxy_required: true,
                ..
            } => Err(DomainError::InvalidNetwork(
                "A PAC profile cannot guarantee fail-closed proxy routing because PAC rules may return DIRECT."
                    .to_owned(),
            )),
            Self::Pac { pac_url, .. } => {
                if pac_url.len() > 2_048 {
                    return Err(DomainError::InvalidNetwork(
                        "PAC URL is too long.".to_owned(),
                    ));
                }
                let parsed = Url::parse(pac_url)
                    .map_err(|_| DomainError::InvalidNetwork("PAC URL is invalid.".to_owned()))?;
                if !matches!(parsed.scheme(), "http" | "https") {
                    return Err(DomainError::InvalidNetwork(
                        "PAC URL must use HTTPS or HTTP.".to_owned(),
                    ));
                }
                if !parsed.username().is_empty() || parsed.password().is_some() {
                    return Err(DomainError::InvalidNetwork(
                        "PAC URL must not include credentials.".to_owned(),
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn requires_proxy(&self) -> bool {
        match self {
            Self::Direct { proxy_required }
            | Self::FixedProxy { proxy_required, .. }
            | Self::Pac { proxy_required, .. } => *proxy_required,
        }
    }

    pub fn launch_arguments(&self) -> Vec<OsString> {
        self.launch_arguments_with_proxy_override(None)
    }

    pub fn launch_arguments_with_proxy_override(
        &self,
        proxy_override: Option<(&str, u16)>,
    ) -> Vec<OsString> {
        match self {
            Self::Direct { .. } => vec![OsString::from("--no-proxy-server")],
            Self::FixedProxy {
                scheme,
                host,
                port,
                bypass_list,
                ..
            } => {
                let (launch_scheme, launch_host, launch_port) = proxy_override
                    .map(|(override_host, override_port)| ("socks5", override_host, override_port))
                    .unwrap_or((scheme.as_str(), host.as_str(), *port));
                let launch_authority_host =
                    if launch_host.contains(':') && !launch_host.starts_with('[') {
                        format!("[{launch_host}]")
                    } else {
                        launch_host.to_owned()
                    };
                let mut arguments = vec![OsString::from(format!(
                    "--proxy-server={launch_scheme}://{launch_authority_host}:{launch_port}"
                ))];
                if !bypass_list.is_empty() {
                    arguments.push(OsString::from(format!(
                        "--proxy-bypass-list={}",
                        bypass_list.join(";")
                    )));
                }
                if self.requires_proxy() {
                    arguments.push(OsString::from("--proxy-bypass-list=<-loopback>"));
                    arguments.push(OsString::from(format!(
                        "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE {launch_host}"
                    )));
                    arguments.push(OsString::from("--disable-quic"));
                    arguments.push(OsString::from(
                        "--webrtc-ip-handling-policy=disable_non_proxied_udp",
                    ));
                }
                arguments
            }
            Self::Pac { pac_url, .. } => vec![OsString::from(format!("--proxy-pac-url={pac_url}"))],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Silo {
    pub id: Uuid,
    pub schema_version: u32,
    pub name: String,
    pub color: String,
    /// Standard and WSL Silos carry a browser descriptor. Managed Camoufox
    /// Silos deliberately keep this null: the signed package is the browser
    /// authority and there is no user-selected stock executable to verify.
    pub browser: Option<BrowserDescriptor>,
    #[serde(default)]
    pub execution_target: SiloExecutionTarget,
    pub profile_directory: String,
    pub network_profile: NetworkProfile,
    #[serde(default)]
    pub engine: SiloEngineConfig,
    pub seed_reference: Uuid,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub identity_locked_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSiloInput {
    pub name: String,
    pub color: String,
    pub browser_kind: BrowserKind,
    pub executable_path: String,
    #[serde(default)]
    pub execution_target: SiloExecutionTarget,
    pub network_profile: NetworkProfile,
    #[serde(default)]
    pub engine: SiloEngineConfig,
    #[serde(default)]
    pub proxy_credentials: Option<ProxyCredentialsInput>,
    #[serde(default)]
    pub mihomo_controller_secret: Option<MihomoControllerSecretInput>,
}

/// Locale starting points for a managed identity. The Host still owns the
/// resolved Artifact; these names only pick language/timezone defaults.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedIdentityPreset {
    BalancedEnUs,
    BalancedZhCn,
    BalancedDeDe,
    MatchFixedProxy,
}

impl ManagedIdentityPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BalancedEnUs => "balanced-en-us",
            Self::BalancedZhCn => "balanced-zh-cn",
            Self::BalancedDeDe => "balanced-de-de",
            Self::MatchFixedProxy => "match-fixed-proxy",
        }
    }

    pub fn requires_proxy(self) -> bool {
        matches!(self, Self::MatchFixedProxy)
    }

    pub fn locale(self) -> Option<&'static str> {
        match self {
            Self::BalancedEnUs => Some("en-US"),
            Self::BalancedZhCn => Some("zh-CN"),
            Self::BalancedDeDe => Some("de-DE"),
            Self::MatchFixedProxy => None,
        }
    }

    pub fn from_locale(locale: &str) -> Self {
        match locale {
            "zh-CN" => Self::BalancedZhCn,
            "de-DE" => Self::BalancedDeDe,
            _ => Self::BalancedEnUs,
        }
    }
}

fn default_managed_screen_width() -> u32 {
    1280
}

fn default_managed_screen_height() -> u32 {
    800
}

fn is_supported_gpu_preset(gpu: &str) -> bool {
    matches!(
        gpu,
        "auto"
            | "nvidia-rtx-3060"
            | "nvidia-rtx-3070"
            | "nvidia-rtx-4060"
            | "nvidia-rtx-4070"
            | "nvidia-rtx-4080"
            | "nvidia-gtx-1660"
            | "amd-rx-6600"
            | "amd-rx-7600"
            | "amd-rx-7800xt"
            | "intel-uhd-770"
    )
}

fn is_supported_managed_timezone(timezone: &str) -> bool {
    matches!(
        timezone,
        "Asia/Shanghai"
            | "Asia/Hong_Kong"
            | "Asia/Tokyo"
            | "Asia/Singapore"
            | "Europe/London"
            | "Europe/Berlin"
            | "Europe/Paris"
            | "America/New_York"
            | "America/Chicago"
            | "America/Los_Angeles"
            | "UTC"
    )
}

/// Website-visible identity facts projected from a stored Artifact.
/// Seeds, font lists, and raw Artifact JSON stay in the Vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedIdentityPreview {
    pub user_agent: String,
    pub language: String,
    pub timezone: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub hardware_concurrency: u32,
    pub webgl_vendor: String,
    pub webgl_renderer: String,
    pub platform: String,
    pub country_code: Option<String>,
    pub public_address: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub network_bound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedIdentityIntent {
    pub identity_preset: ManagedIdentityPreset,
    #[serde(default)]
    pub follow_network_exit: bool,
    #[serde(default = "default_managed_screen_width")]
    pub screen_width: u32,
    #[serde(default = "default_managed_screen_height")]
    pub screen_height: u32,
    #[serde(default)]
    pub hardware_concurrency: Option<u32>,
    #[serde(default)]
    pub gpu_preset: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
}

impl ManagedIdentityIntent {
    pub fn validate(&self, has_proxy: bool) -> Result<(), DomainError> {
        if !(800..=7680).contains(&self.screen_width) || !(600..=4320).contains(&self.screen_height)
        {
            return Err(DomainError::InvalidNetwork(
                "Managed identity screen size is out of range.".to_owned(),
            ));
        }
        if let Some(cores) = self.hardware_concurrency {
            if ![2, 4, 6, 8, 12, 16].contains(&cores) {
                return Err(DomainError::InvalidNetwork(
                    "Managed identity hardware concurrency is not a supported core count."
                        .to_owned(),
                ));
            }
        }
        if let Some(gpu) = self.gpu_preset.as_deref() {
            if !is_supported_gpu_preset(gpu) {
                return Err(DomainError::InvalidNetwork(
                    "Managed identity GPU profile is not supported.".to_owned(),
                ));
            }
        }
        if let Some(timezone) = self.timezone.as_deref() {
            if !is_supported_managed_timezone(timezone) {
                return Err(DomainError::InvalidNetwork(
                    "Managed identity timezone is not supported.".to_owned(),
                ));
            }
        }
        if self.identity_preset.requires_proxy() && !has_proxy {
            return Err(DomainError::InvalidNetwork(
                "跟随出口的身份需要配置必须代理。".to_owned(),
            ));
        }
        if self.follow_network_exit && !has_proxy {
            return Err(DomainError::InvalidNetwork(
                "跟随出口的身份需要配置必须代理。".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn follow_network_exit(&self, has_proxy: bool) -> bool {
        has_proxy
            && (self.follow_network_exit || self.identity_preset.requires_proxy())
    }

    pub fn host_preset(&self) -> ManagedIdentityPreset {
        if self.identity_preset.requires_proxy() {
            ManagedIdentityPreset::MatchFixedProxy
        } else {
            self.identity_preset
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateManagedSiloInput {
    pub name: String,
    pub color: String,
    #[serde(rename = "identityPreset")]
    pub identity_preset: ManagedIdentityPreset,
    #[serde(default)]
    pub follow_network_exit: bool,
    #[serde(default = "default_managed_screen_width")]
    pub screen_width: u32,
    #[serde(default = "default_managed_screen_height")]
    pub screen_height: u32,
    #[serde(default)]
    pub hardware_concurrency: Option<u32>,
    #[serde(default)]
    pub gpu_preset: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    pub network_profile: NetworkProfile,
    #[serde(default)]
    pub proxy_credentials: Option<ProxyCredentialsInput>,
    #[serde(default)]
    pub mihomo_controller_secret: Option<MihomoControllerSecretInput>,
}

impl CreateManagedSiloInput {
    pub fn identity_intent(&self) -> ManagedIdentityIntent {
        ManagedIdentityIntent {
            identity_preset: self.identity_preset,
            follow_network_exit: self.follow_network_exit,
            screen_width: self.screen_width,
            screen_height: self.screen_height,
            hardware_concurrency: self.hardware_concurrency,
            gpu_preset: self.gpu_preset.clone(),
            timezone: self.timezone.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        validate_silo_appearance(&self.name, &self.color)?;
        validate_network_replacement(
            &self.network_profile,
            self.proxy_credentials.as_ref(),
            self.mihomo_controller_secret.as_ref(),
        )?;
        let has_proxy = matches!(
            self.network_profile,
            NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                ..
            }
        );
        let direct = matches!(
            self.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            }
        );
        if !direct && !has_proxy {
            return Err(DomainError::InvalidNetwork(
                "Managed identity browsers accept Direct or a required HTTP/SOCKS5 proxy, including a local Clash/Mihomo binding.".to_owned(),
            ));
        }
        if direct && (self.proxy_credentials.is_some() || self.mihomo_controller_secret.is_some())
        {
            return Err(DomainError::InvalidNetwork(
                "Direct managed identity browsers cannot carry proxy credentials.".to_owned(),
            ));
        }
        self.identity_intent().validate(has_proxy)?;
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateManagedIdentityInput {
    pub identity_preset: ManagedIdentityPreset,
    #[serde(default)]
    pub follow_network_exit: bool,
    #[serde(default = "default_managed_screen_width")]
    pub screen_width: u32,
    #[serde(default = "default_managed_screen_height")]
    pub screen_height: u32,
    #[serde(default)]
    pub hardware_concurrency: Option<u32>,
    #[serde(default)]
    pub gpu_preset: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub rotate_seed: bool,
}

impl UpdateManagedIdentityInput {
    pub fn identity_intent(&self) -> ManagedIdentityIntent {
        ManagedIdentityIntent {
            identity_preset: self.identity_preset,
            follow_network_exit: self.follow_network_exit,
            screen_width: self.screen_width,
            screen_height: self.screen_height,
            hardware_concurrency: self.hardware_concurrency,
            gpu_preset: self.gpu_preset.clone(),
            timezone: self.timezone.clone(),
        }
    }
}

/// Editable Silo metadata. Identity-bearing fields intentionally do not appear
/// here: a caller cannot replace the UUID, seed, profile path, creation time or
/// archived state through an update.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSiloInput {
    pub name: String,
    pub color: String,
    pub browser_kind: BrowserKind,
    pub executable_path: String,
}

/// A complete replacement for a Silo's network configuration. Optional
/// secrets are plaintext inputs only at the Tauri boundary and are converted
/// to opaque Vault references before the Silo is persisted.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSiloNetworkInput {
    pub network_profile: NetworkProfile,
    #[serde(default)]
    pub proxy_credentials: Option<ProxyCredentialsInput>,
    #[serde(default)]
    pub mihomo_controller_secret: Option<MihomoControllerSecretInput>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSiloEngineInput {
    pub engine: SiloEngineConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiloStorageUsage {
    pub silo_id: Uuid,
    pub profile_directory: String,
    pub bytes: u64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyCredentialsInput {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MihomoControllerSecretInput {
    pub secret: String,
}

impl CreateSiloInput {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_silo_appearance(&self.name, &self.color)?;
        self.execution_target.validate()?;
        if !self.engine.is_stock() {
            return Err(DomainError::InvalidEngine(
                "Managed browser Silos must be created through the managed-browser command."
                    .to_owned(),
            ));
        }
        if self.execution_target.is_local() {
            validate_browser_executable(&self.executable_path)?;
        } else if !self.engine.is_stock() {
            return Err(DomainError::InvalidEngine(
                "WSL and remote execution currently accept only the fixed stock guest engine."
                    .to_owned(),
            ));
        }
        self.execution_target.validate_network_input(
            &self.network_profile,
            self.proxy_credentials.as_ref(),
            self.mihomo_controller_secret.as_ref(),
        )?;
        validate_engine_configuration(&self.engine, &self.network_profile)
    }
}

impl UpdateSiloInput {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_silo_metadata(&self.name, &self.color, &self.executable_path)
    }

    pub fn validate_managed_metadata(&self) -> Result<(), DomainError> {
        validate_silo_appearance(&self.name, &self.color)
    }

    pub fn validate_for_execution_target(
        &self,
        execution_target: &SiloExecutionTarget,
    ) -> Result<(), DomainError> {
        validate_silo_appearance(&self.name, &self.color)?;
        if execution_target.is_local() {
            validate_browser_executable(&self.executable_path)?;
        }
        Ok(())
    }
}

impl UpdateSiloNetworkInput {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_network_replacement(
            &self.network_profile,
            self.proxy_credentials.as_ref(),
            self.mihomo_controller_secret.as_ref(),
        )
    }

    pub fn validate_for_execution_target(
        &self,
        execution_target: &SiloExecutionTarget,
    ) -> Result<(), DomainError> {
        execution_target.validate_network_input(
            &self.network_profile,
            self.proxy_credentials.as_ref(),
            self.mihomo_controller_secret.as_ref(),
        )
    }
}

impl UpdateSiloEngineInput {
    pub fn validate(&self, network_profile: &NetworkProfile) -> Result<(), DomainError> {
        validate_engine_configuration(&self.engine, network_profile)
    }
}

fn validate_engine_configuration(
    engine: &SiloEngineConfig,
    network_profile: &NetworkProfile,
) -> Result<(), DomainError> {
    engine
        .validate(network_profile.requires_proxy())
        .map_err(|error| DomainError::InvalidEngine(error.to_string()))
}

pub(crate) fn validate_silo_name(name: &str) -> Result<(), DomainError> {
    if name.trim().is_empty() || name.chars().count() > 64 {
        return Err(DomainError::InvalidSilo(
            "Silo name must contain 1–64 characters.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_silo_metadata(
    name: &str,
    color: &str,
    executable_path: &str,
) -> Result<(), DomainError> {
    validate_silo_appearance(name, color)?;
    validate_browser_executable(executable_path)
}

fn validate_silo_appearance(name: &str, color: &str) -> Result<(), DomainError> {
    validate_silo_name(name)?;
    if !is_hex_color(color) {
        return Err(DomainError::InvalidSilo(
            "Silo color must be a six-digit hex color.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_browser_executable(executable_path: &str) -> Result<(), DomainError> {
    if !Path::new(executable_path).is_file() {
        return Err(DomainError::InvalidSilo(
            "The selected browser executable does not exist.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_network_replacement(
    network_profile: &NetworkProfile,
    proxy_credentials: Option<&ProxyCredentialsInput>,
    mihomo_controller_secret: Option<&MihomoControllerSecretInput>,
) -> Result<(), DomainError> {
    network_profile.validate()?;
    if network_profile.credential_reference().is_some() {
        return Err(DomainError::InvalidNetwork(
            "Network updates cannot supply an existing credential reference.".to_owned(),
        ));
    }
    if network_profile
        .mihomo_controller_secret_reference()
        .is_some()
    {
        return Err(DomainError::InvalidNetwork(
            "Network updates cannot supply an existing Mihomo controller secret reference."
                .to_owned(),
        ));
    }

    if let Some(credentials) = proxy_credentials {
        if !matches!(network_profile, NetworkProfile::FixedProxy { .. }) {
            return Err(DomainError::InvalidNetwork(
                "Proxy credentials require a fixed proxy profile.".to_owned(),
            ));
        }
        if credentials.username.trim().is_empty()
            || credentials.username.len() > 512
            || credentials.password.len() > 1_024
            || credentials.username.chars().any(char::is_control)
            || credentials.password.chars().any(char::is_control)
        {
            return Err(DomainError::InvalidNetwork(
                "Proxy credentials are empty, too long, or contain control characters.".to_owned(),
            ));
        }
        if !matches!(
            network_profile,
            NetworkProfile::FixedProxy {
                scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                ..
            }
        ) {
            return Err(DomainError::InvalidNetwork(
                "Automatic proxy authentication currently supports HTTP and SOCKS5. Use an external Mihomo endpoint for other authenticated protocols."
                    .to_owned(),
            ));
        }
    }

    if let Some(controller_secret) = mihomo_controller_secret {
        if network_profile.external_mihomo_binding().is_none() {
            return Err(DomainError::InvalidNetwork(
                "A Mihomo controller secret requires an external Mihomo binding.".to_owned(),
            ));
        }
        if controller_secret.secret.len() > 1_024
            || controller_secret.secret.chars().any(char::is_control)
        {
            return Err(DomainError::InvalidNetwork(
                "The Mihomo controller secret is too long or contains control characters."
                    .to_owned(),
            ));
        }
    }

    Ok(())
}

impl NetworkProfile {
    pub fn credential_reference(&self) -> Option<Uuid> {
        match self {
            Self::FixedProxy {
                credential_reference,
                ..
            } => *credential_reference,
            Self::Direct { .. } | Self::Pac { .. } => None,
        }
    }

    pub fn external_mihomo_binding(&self) -> Option<&ExternalMihomoBinding> {
        match self {
            Self::FixedProxy {
                external_mihomo, ..
            } => external_mihomo.as_ref(),
            Self::Direct { .. } | Self::Pac { .. } => None,
        }
    }

    pub fn mihomo_controller_secret_reference(&self) -> Option<Uuid> {
        self.external_mihomo_binding()
            .and_then(|binding| binding.controller_secret_reference)
    }

    pub fn set_credential_reference(&mut self, reference: Uuid) -> Result<(), DomainError> {
        match self {
            Self::FixedProxy {
                credential_reference,
                ..
            } => {
                *credential_reference = Some(reference);
                Ok(())
            }
            Self::Direct { .. } | Self::Pac { .. } => Err(DomainError::InvalidNetwork(
                "Only fixed proxies can reference credentials.".to_owned(),
            )),
        }
    }

    pub fn set_mihomo_controller_secret_reference(
        &mut self,
        reference: Uuid,
    ) -> Result<(), DomainError> {
        let Some(binding) = (match self {
            Self::FixedProxy {
                external_mihomo, ..
            } => external_mihomo.as_mut(),
            Self::Direct { .. } | Self::Pac { .. } => None,
        }) else {
            return Err(DomainError::InvalidNetwork(
                "Only an external Mihomo binding can reference a controller secret.".to_owned(),
            ));
        };
        binding.controller_secret_reference = Some(reference);
        Ok(())
    }
}

impl Silo {
    pub fn adapter_id(&self) -> EngineAdapterId {
        match &self.engine {
            SiloEngineConfig::Stock => self
                .browser
                .as_ref()
                .map(|browser| self.engine.adapter_id(&browser.kind))
                .expect("validated stock Silo must carry a browser descriptor"),
            _ => self.engine.adapter_id(&BrowserKind::Chrome),
        }
    }

    pub fn browser_descriptor(&self) -> Result<&BrowserDescriptor, DomainError> {
        self.browser.as_ref().ok_or_else(|| {
            DomainError::InvalidSilo(
                "This Silo has no stock browser descriptor; the managed engine owns its browser."
                    .to_owned(),
            )
        })
    }

    pub fn engine_profile_directory(&self) -> PathBuf {
        self.engine
            .profile_directory(Path::new(&self.profile_directory))
    }

    pub fn all_engine_profile_directories(&self) -> [PathBuf; 3] {
        SiloEngineConfig::all_profile_directories(Path::new(&self.profile_directory))
    }

    pub fn validate_engine(&self) -> Result<(), DomainError> {
        validate_engine_configuration(&self.engine, &self.network_profile)
    }

    pub fn launch_arguments(&self) -> Vec<OsString> {
        self.launch_arguments_with_proxy_override(None)
    }

    pub fn launch_arguments_with_proxy_override(
        &self,
        proxy_override: Option<(&str, u16)>,
    ) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from(format!("--user-data-dir={}", self.profile_directory)),
            OsString::from("--no-first-run"),
            OsString::from("--no-default-browser-check"),
            // A dedicated profile keeps site storage separate. Disabling browser
            // sync additionally prevents that isolated profile from importing or
            // exporting browser data through a signed-in Chrome/Edge account.
            OsString::from("--disable-sync"),
        ];
        arguments.extend(
            self.network_profile
                .launch_arguments_with_proxy_override(proxy_override),
        );
        arguments
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivation {
    pub active_silo_id: Option<Uuid>,
    pub state: RuntimeState,
    pub updated_at: DateTime<Utc>,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_verification: Option<BrowserVerification>,
    pub engine_evidence: Option<RuntimeEngineEvidence>,
    pub network_evidence: Option<RuntimeNetworkEvidence>,
}

/// Separates configuration, process launch, package authenticity, bootstrap
/// delivery, and runtime identity verification. A verified package never sets
/// `verified_adapter`; that field requires direct runtime protocol evidence.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePackageVerification {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEngineEvidence {
    pub configured_adapter: EngineAdapterId,
    pub launched_adapter: Option<EngineAdapterId>,
    pub verified_adapter: Option<EngineAdapterId>,
    pub package_verification: RuntimeEvidenceState,
    pub package_verification_details: Option<RuntimePackageVerification>,
    pub bootstrap_delivery: RuntimeEvidenceState,
    pub host_launch: RuntimeEvidenceState,
    pub runtime_receipts: RuntimeEvidenceState,
    pub restore_receipt: RuntimeEvidenceState,
    pub capabilities: Vec<EngineCapabilityState>,
    pub phase_receipts: Vec<EngineControlPhaseReceipt>,
    pub fallback_receipts: Vec<SiteFallbackReceipt>,
}

impl RuntimeEngineEvidence {
    pub fn configured(adapter: EngineAdapterId, externally_packaged: bool) -> Self {
        let camoufox_host = adapter == EngineAdapterId::Camoufox;
        let native_control = externally_packaged && !camoufox_host;
        Self {
            configured_adapter: adapter,
            launched_adapter: None,
            verified_adapter: None,
            package_verification: if externally_packaged {
                RuntimeEvidenceState::NotRequested
            } else {
                RuntimeEvidenceState::NotApplicable
            },
            package_verification_details: None,
            bootstrap_delivery: if native_control {
                RuntimeEvidenceState::NotRequested
            } else {
                RuntimeEvidenceState::NotApplicable
            },
            host_launch: if camoufox_host {
                RuntimeEvidenceState::NotRequested
            } else {
                RuntimeEvidenceState::NotApplicable
            },
            runtime_receipts: if native_control {
                RuntimeEvidenceState::NotRequested
            } else {
                RuntimeEvidenceState::NotApplicable
            },
            restore_receipt: if native_control {
                RuntimeEvidenceState::NotRequested
            } else {
                RuntimeEvidenceState::NotApplicable
            },
            capabilities: Vec::new(),
            phase_receipts: Vec::new(),
            fallback_receipts: Vec::new(),
        }
    }

    pub fn sync_control_execution(&mut self, execution: &EngineControlExecution) {
        self.capabilities.clone_from(&execution.capabilities);
        self.phase_receipts.clone_from(&execution.phase_receipts);
        self.fallback_receipts
            .clone_from(&execution.fallback_receipts);
    }
}

impl RuntimeActivation {
    pub fn idle() -> Self {
        Self {
            active_silo_id: None,
            state: RuntimeState::Idle,
            updated_at: Utc::now(),
            message: None,
            browser_verification: None,
            engine_evidence: None,
            network_evidence: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNetworkEvidence {
    pub runtime_id: Uuid,
    pub evidence_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub provenance: RuntimeNetworkEvidenceProvenance,
    pub provider: RuntimeNetworkProvider,
    pub configuration: RuntimeEvidenceState,
    pub controller_binding: RuntimeEvidenceState,
    pub endpoint: RuntimeEvidenceState,
    pub authentication: RuntimeEvidenceState,
    pub authentication_provenance: RuntimeNetworkEvidenceProvenance,
    pub browser_routing: RuntimeEvidenceState,
    pub exit: RuntimeEvidenceState,
    pub dns: RuntimeEvidenceState,
    pub web_rtc: RuntimeEvidenceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_label: Option<String>,
    pub safeguards: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeNetworkProvider {
    Direct,
    FixedProxy,
    ExternalMihomo,
    Pac,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeNetworkEvidenceProvenance {
    DesktopControlPlane,
    ExtensionAsserted,
    RelayObserved,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEvidenceState {
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

impl RuntimeNetworkEvidence {
    pub fn configured(profile: &NetworkProfile, has_authentication: bool) -> Self {
        match profile {
            NetworkProfile::Direct { .. } => Self {
                runtime_id: Uuid::new_v4(),
                evidence_id: Uuid::new_v4(),
                observed_at: Utc::now(),
                expires_at: None,
                provenance: RuntimeNetworkEvidenceProvenance::DesktopControlPlane,
                provider: RuntimeNetworkProvider::Direct,
                configuration: RuntimeEvidenceState::Configured,
                controller_binding: RuntimeEvidenceState::NotApplicable,
                endpoint: RuntimeEvidenceState::NotApplicable,
                authentication: RuntimeEvidenceState::NotApplicable,
                authentication_provenance: RuntimeNetworkEvidenceProvenance::DesktopControlPlane,
                browser_routing: RuntimeEvidenceState::NotRequested,
                exit: RuntimeEvidenceState::NotRequested,
                dns: RuntimeEvidenceState::NotRequested,
                web_rtc: RuntimeEvidenceState::NotRequested,
                endpoint_label: None,
                safeguards: Vec::new(),
            },
            NetworkProfile::FixedProxy {
                host,
                port,
                external_mihomo,
                ..
            } => Self {
                runtime_id: Uuid::new_v4(),
                evidence_id: Uuid::new_v4(),
                observed_at: Utc::now(),
                expires_at: None,
                provenance: RuntimeNetworkEvidenceProvenance::DesktopControlPlane,
                provider: if external_mihomo.is_some() {
                    RuntimeNetworkProvider::ExternalMihomo
                } else {
                    RuntimeNetworkProvider::FixedProxy
                },
                configuration: RuntimeEvidenceState::Configured,
                controller_binding: if external_mihomo.is_some() {
                    RuntimeEvidenceState::Configured
                } else {
                    RuntimeEvidenceState::NotApplicable
                },
                endpoint: RuntimeEvidenceState::NotRequested,
                authentication: if has_authentication {
                    RuntimeEvidenceState::Configured
                } else {
                    RuntimeEvidenceState::NotApplicable
                },
                authentication_provenance: RuntimeNetworkEvidenceProvenance::DesktopControlPlane,
                browser_routing: RuntimeEvidenceState::NotRequested,
                exit: RuntimeEvidenceState::NotRequested,
                dns: RuntimeEvidenceState::NotRequested,
                web_rtc: RuntimeEvidenceState::NotRequested,
                endpoint_label: Some(format!("{host}:{port}")),
                safeguards: if profile.requires_proxy() {
                    vec![
                        "no_direct_fallback".to_owned(),
                        "browser_dns_through_proxy".to_owned(),
                        "quic_disabled".to_owned(),
                        "non_proxied_webrtc_udp_disabled".to_owned(),
                    ]
                } else {
                    Vec::new()
                },
            },
            NetworkProfile::Pac { .. } => Self {
                runtime_id: Uuid::new_v4(),
                evidence_id: Uuid::new_v4(),
                observed_at: Utc::now(),
                expires_at: None,
                provenance: RuntimeNetworkEvidenceProvenance::DesktopControlPlane,
                provider: RuntimeNetworkProvider::Pac,
                configuration: RuntimeEvidenceState::Configured,
                controller_binding: RuntimeEvidenceState::NotApplicable,
                endpoint: RuntimeEvidenceState::Unavailable,
                authentication: RuntimeEvidenceState::NotApplicable,
                authentication_provenance: RuntimeNetworkEvidenceProvenance::DesktopControlPlane,
                browser_routing: RuntimeEvidenceState::NotRequested,
                exit: RuntimeEvidenceState::NotRequested,
                dns: RuntimeEvidenceState::NotRequested,
                web_rtc: RuntimeEvidenceState::NotRequested,
                endpoint_label: None,
                safeguards: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Idle,
    Preflight,
    Launching,
    Running,
    VerificationFailed,
    RecoveryRequired,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub state: VaultLockState,
    pub auto_lock_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultLockState {
    Uninitialized,
    Locked,
    Unlocked,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Invalid Silo: {0}")]
    InvalidSilo(String),
    #[error("Invalid network profile: {0}")]
    InvalidNetwork(String),
    #[error("Invalid engine configuration: {0}")]
    InvalidEngine(String),
    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
}

pub const DEFAULT_VAULT_NAME: &str = "default";
pub const VAULT_NAME_ENV: &str = "VERISILO_VAULT_NAME";

pub fn validate_vault_name(value: &str) -> Result<String, DomainError> {
    let valid = (1..=32).contains(&value.len())
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| byte.is_ascii_lowercase() || byte.is_ascii_digit() || (index > 0 && matches!(byte, b'-' | b'_')));
    if !valid {
        return Err(DomainError::InvalidSilo(
            "Vault name must be 1..32 lowercase letters, digits, '-' or '_'.".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

pub fn active_vault_name() -> Result<String, DomainError> {
    match std::env::var(VAULT_NAME_ENV) {
        Ok(value) => validate_vault_name(&value),
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_VAULT_NAME.to_owned()),
        Err(_) => Err(DomainError::InvalidSilo(
            "Vault name is not valid UTF-8.".to_owned(),
        )),
    }
}

pub fn select_vault_name(value: &str) -> Result<(), DomainError> {
    let value = validate_vault_name(value)?;
    std::env::set_var(VAULT_NAME_ENV, value);
    Ok(())
}

fn default_app_data_root_path() -> Result<PathBuf, DomainError> {
    if cfg!(target_os = "windows") {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .ok_or_else(|| {
                DomainError::InvalidSilo(
                    "Windows application data directory is unavailable.".to_owned(),
                )
            })?;
        Ok(windows_app_data_root(&base))
    } else {
        Ok(std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .ok_or_else(|| {
                DomainError::InvalidSilo("Application data directory is unavailable.".to_owned())
            })?
            .join("VeriSilo"))
    }
}

fn vault_root(default_root: PathBuf, vault_name: &str) -> PathBuf {
    if vault_name == DEFAULT_VAULT_NAME {
        default_root
    } else {
        default_root.join("vaults").join(vault_name)
    }
}

pub(crate) fn app_data_root_path() -> Result<PathBuf, DomainError> {
    let default_root = default_app_data_root_path()?;
    let vault_name = active_vault_name()?;
    Ok(vault_root(default_root, &vault_name))
}

pub fn available_vault_names() -> Result<Vec<String>, DomainError> {
    let mut names = vec![DEFAULT_VAULT_NAME.to_owned()];
    let entries = match fs::read_dir(default_app_data_root_path()?.join("vaults")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(error) => return Err(DomainError::Filesystem(error)),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if validate_vault_name(name).is_ok() {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names[1..].sort_unstable();
    Ok(names)
}

pub fn app_data_root() -> Result<PathBuf, DomainError> {
    let root = app_data_root_path()?;
    if cfg!(target_os = "windows") && active_vault_name()? == DEFAULT_VAULT_NAME {
        let base = root.parent().ok_or_else(|| {
            DomainError::InvalidSilo(
                "Windows application data directory is unavailable.".to_owned(),
            )
        })?;
        migrate_legacy_app_data(base)?;
    }
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn windows_app_data_root(base: &Path) -> PathBuf {
    base.join("io.verisilo.app")
}

fn migrate_legacy_app_data(base: &Path) -> Result<(), DomainError> {
    let current = windows_app_data_root(base);
    let legacy = base.join("VeriSilo");
    let legacy_metadata = match fs::symlink_metadata(&legacy) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(DomainError::Filesystem(error)),
    };
    if metadata_is_link_or_reparse(&legacy_metadata) || !legacy_metadata.is_dir() {
        return Err(DomainError::InvalidSilo(
            "Legacy VeriSilo application data is redirected or not a directory; migration stopped without changing it."
                .to_owned(),
        ));
    }
    // Current-user NSIS installs into %LOCALAPPDATA%\VeriSilo. That is the
    // application directory, not a legacy Vault. Leave it alone.
    if windows_verisilo_dir_is_current_user_install(&legacy) {
        return Ok(());
    }
    match fs::symlink_metadata(&current) {
        Ok(metadata) if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() => {
            return Err(DomainError::InvalidSilo(
                "RC1 application data path is redirected or not a directory; legacy migration stopped."
                    .to_owned(),
            ))
        }
        Ok(_) => {
            return Err(DomainError::InvalidSilo(
                "Both legacy and RC1 application data directories exist; migration stopped without merging or overwriting either directory."
                    .to_owned(),
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(DomainError::Filesystem(error)),
    }
    fs::rename(&legacy, &current).map_err(|error| {
        DomainError::InvalidSilo(format!(
            "Legacy application data could not be moved atomically to the RC1 path: {error}"
        ))
    })?;
    Ok(())
}

fn windows_verisilo_dir_is_current_user_install(path: &Path) -> bool {
    path.join("verisilo.exe").is_file() || path.join("uninstall.exe").is_file()
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

pub fn discover_browsers() -> Vec<BrowserCandidate> {
    let mut candidates = Vec::new();
    for kind in [BrowserKind::Chrome, BrowserKind::Edge] {
        let mut paths = Vec::new();
        for variable in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(variable) {
                for relative in kind.known_relative_paths() {
                    paths.push(PathBuf::from(&base).join(relative));
                }
            }
        }

        paths.sort();
        paths.dedup();
        for path in paths.into_iter().filter(|candidate| candidate.is_file()) {
            if let Ok(inspection) = inspect_browser_executable(&kind, &path) {
                candidates.push(BrowserCandidate {
                    display_name: kind.display_name().to_owned(),
                    version: Some(inspection.version),
                    executable_path: inspection.resolved_path,
                    kind: kind.clone(),
                });
            }
        }
    }
    candidates
}

pub fn inspect_browser_executable(
    kind: &BrowserKind,
    executable_path: &Path,
) -> Result<BrowserInspection, BrowserVerificationError> {
    if !executable_path.is_file() {
        return Err(BrowserVerificationError::Missing);
    }
    let resolved_path =
        fs::canonicalize(executable_path).map_err(BrowserVerificationError::Path)?;
    let filename = resolved_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let expected_filename = match kind {
        BrowserKind::Chrome => "chrome.exe",
        BrowserKind::Edge => "msedge.exe",
    };
    #[cfg(target_os = "windows")]
    let filename_matches = filename.eq_ignore_ascii_case(expected_filename);
    #[cfg(not(target_os = "windows"))]
    let filename_matches = filename.eq_ignore_ascii_case(expected_filename)
        || filename.eq_ignore_ascii_case(expected_filename.trim_end_matches(".exe"));
    if !filename_matches {
        return Err(BrowserVerificationError::FilenameMismatch);
    }

    let prefix = match kind {
        BrowserKind::Chrome => "Google Chrome ",
        BrowserKind::Edge => "Microsoft Edge ",
    };
    let output = browser_identity_output(kind, prefix, &resolved_path)?;
    let version = output
        .strip_prefix(prefix)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
                })
        })
        .ok_or(BrowserVerificationError::KindMismatch)?;
    Ok(BrowserInspection {
        resolved_path: resolved_path.to_string_lossy().to_string(),
        version: version.to_owned(),
    })
}

pub fn verify_browser_descriptor(descriptor: &BrowserDescriptor) -> BrowserVerification {
    let checked_at = Utc::now();
    let executable_path = descriptor.executable_path.clone();
    let inspection = match inspect_browser_executable(
        &descriptor.kind,
        Path::new(&descriptor.executable_path),
    ) {
        Ok(inspection) => inspection,
        Err(error) => {
            let state = match &error {
                BrowserVerificationError::Missing => BrowserVerificationState::Missing,
                BrowserVerificationError::KindMismatch
                | BrowserVerificationError::FilenameMismatch => {
                    BrowserVerificationState::KindMismatch
                }
                BrowserVerificationError::PublisherMismatch => {
                    BrowserVerificationState::PublisherMismatch
                }
                BrowserVerificationError::Path(_) | BrowserVerificationError::Probe(_) => {
                    BrowserVerificationState::ProbeFailed
                }
            };
            return BrowserVerification {
                state,
                expected_kind: descriptor.kind.clone(),
                expected_version: descriptor.version.clone(),
                actual_version: None,
                executable_path,
                checked_at,
                message: error.to_string(),
            };
        }
    };

    if !paths_match(&descriptor.executable_path, &inspection.resolved_path) {
        return BrowserVerification {
            state: BrowserVerificationState::PathChanged,
            expected_kind: descriptor.kind.clone(),
            expected_version: descriptor.version.clone(),
            actual_version: Some(inspection.version),
            executable_path: inspection.resolved_path,
            checked_at,
            message: "浏览器路径解析结果已变化；为避免启动被替换的程序，本次已拒绝启动。"
                .to_owned(),
        };
    }

    let (state, message) = match descriptor.version.as_deref() {
        None => (
            BrowserVerificationState::BaselineMissing,
            "该 Silo 尚无已确认的浏览器版本基线；请先执行显式浏览器重新检查。".to_owned(),
        ),
        Some(expected) if expected != inspection.version => (
            BrowserVerificationState::VersionDrift,
            format!(
                "浏览器版本已从 {expected} 变为 {}；请显式重新检查后再启动。",
                inspection.version
            ),
        ),
        Some(_) => (
            BrowserVerificationState::Verified,
            format!(
                "已核验 {} {} 的路径、类型、版本和发布者基线。",
                descriptor.kind.display_name(),
                inspection.version
            ),
        ),
    };
    BrowserVerification {
        state,
        expected_kind: descriptor.kind.clone(),
        expected_version: descriptor.version.clone(),
        actual_version: Some(inspection.version),
        executable_path: inspection.resolved_path,
        checked_at,
        message,
    }
}

fn browser_identity_output(
    _kind: &BrowserKind,
    prefix: &str,
    executable_path: &Path,
) -> Result<String, BrowserVerificationError> {
    #[cfg(test)]
    if executable_path.with_extension("version-output").is_file() {
        return browser_version_output(executable_path);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(version) = windows_file_product_version(executable_path) {
            return Ok(format!("{prefix}{version}"));
        }
    }
    let _ = prefix;
    browser_version_output(executable_path)
}

fn browser_version_output(executable_path: &Path) -> Result<String, BrowserVerificationError> {
    #[cfg(test)]
    {
        let fixture = executable_path.with_extension("version-output");
        if fixture.is_file() {
            return fs::read_to_string(fixture)
                .map(|output| output.trim().to_owned())
                .map_err(|error| BrowserVerificationError::Probe(error.to_string()));
        }
    }
    let mut command = Command::new(executable_path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_windows_console(&mut command);
    let child = command
        .spawn()
        .map_err(|error| BrowserVerificationError::Probe(error.to_string()))?;
    let output = wait_child_output(child, Duration::from_secs(5))?;
    if !output.status.success() {
        return Err(BrowserVerificationError::Probe(format!(
            "进程返回 {}。",
            output.status
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| BrowserVerificationError::Probe("输出不是 UTF-8。".to_owned()))?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|_| BrowserVerificationError::Probe("输出不是 UTF-8。".to_owned()))?;
    let output = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    if output.is_empty() || output.len() > 512 || output.chars().any(char::is_control) {
        return Err(BrowserVerificationError::Probe(
            "输出为空、过长或包含控制字符。".to_owned(),
        ));
    }
    Ok(output.to_owned())
}

fn wait_child_output(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, BrowserVerificationError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BrowserVerificationError::Probe(
                    "检查超时；进程已结束且浏览器未启动。".to_owned(),
                ));
            }
            Err(error) => return Err(BrowserVerificationError::Probe(error.to_string())),
        }
    }
    child
        .wait_with_output()
        .map_err(|error| BrowserVerificationError::Probe(error.to_string()))
}

fn paths_match(stored: &str, resolved: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        stored.eq_ignore_ascii_case(resolved)
    }
    #[cfg(not(target_os = "windows"))]
    {
        stored == resolved
    }
}

#[cfg(target_os = "windows")]
fn windows_file_product_version(path: &Path) -> Result<String, BrowserVerificationError> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "version")]
    extern "system" {
        fn GetFileVersionInfoSizeW(filename: *const u16, handle: *mut u32) -> u32;
        fn GetFileVersionInfoW(filename: *const u16, handle: u32, len: u32, data: *mut u8) -> i32;
        fn VerQueryValueW(
            block: *const u8,
            sub_block: *const u16,
            buffer: *mut *mut u8,
            len: *mut u32,
        ) -> i32;
    }

    #[repr(C)]
    struct VsFixedFileInfo {
        signature: u32,
        struc_version: u32,
        file_version_ms: u32,
        file_version_ls: u32,
        product_version_ms: u32,
        product_version_ls: u32,
    }

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let mut handle = 0_u32;
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle) };
    if size == 0 {
        return Err(BrowserVerificationError::Probe(
            "Windows file version resource is missing.".to_owned(),
        ));
    }
    let mut buffer = vec![0_u8; size as usize];
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, buffer.as_mut_ptr()) } == 0 {
        return Err(BrowserVerificationError::Probe(
            "Windows file version resource could not be read.".to_owned(),
        ));
    }
    let root: Vec<u16> = "\\\0".encode_utf16().collect();
    let mut info_ptr: *mut u8 = std::ptr::null_mut();
    let mut info_len = 0_u32;
    let queried = unsafe {
        VerQueryValueW(
            buffer.as_ptr(),
            root.as_ptr(),
            &mut info_ptr,
            &mut info_len,
        )
    };
    if queried == 0
        || info_ptr.is_null()
        || (info_len as usize) < std::mem::size_of::<VsFixedFileInfo>()
    {
        return Err(BrowserVerificationError::Probe(
            "Windows file version resource is incomplete.".to_owned(),
        ));
    }
    let info = unsafe { &*(info_ptr as *const VsFixedFileInfo) };
    if info.signature != 0xFEEF_04BD {
        return Err(BrowserVerificationError::Probe(
            "Windows file version signature is invalid.".to_owned(),
        ));
    }
    let version = format!(
        "{}.{}.{}.{}",
        info.product_version_ms >> 16,
        info.product_version_ms & 0xFFFF,
        info.product_version_ls >> 16,
        info.product_version_ls & 0xFFFF
    );
    if version.len() > 128
        || !version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
    {
        return Err(BrowserVerificationError::Probe(
            "Windows product version metadata is invalid.".to_owned(),
        ));
    }
    Ok(version)
}

fn is_hex_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color
            .chars()
            .skip(1)
            .all(|character| character.is_ascii_hexdigit())
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    matches!(normalized.as_str(), "127.0.0.1" | "::1")
}

fn is_allowed_clash_pipe(host: &str) -> bool {
    host == "verge-mihomo"
}

fn validate_mihomo_binding(binding: &ExternalMihomoBinding) -> Result<(), DomainError> {
    if binding.controller_url.len() > 2_048 {
        return Err(DomainError::InvalidNetwork(
            "The Mihomo controller URL is too long.".to_owned(),
        ));
    }
    let controller = Url::parse(&binding.controller_url).map_err(|_| {
        DomainError::InvalidNetwork("The Mihomo controller URL is invalid.".to_owned())
    })?;
    let controller_host = controller.host_str().unwrap_or_default();
    let pipe_controller = cfg!(windows)
        && controller.scheme() == "pipe"
        && is_allowed_clash_pipe(controller_host)
        && controller.port().is_none()
        && controller.username().is_empty()
        && controller.password().is_none()
        && (controller.path() == "/" || controller.path().is_empty())
        && controller.query().is_none()
        && controller.fragment().is_none();
    let loopback_http = controller.scheme() == "http"
        && is_loopback_host(controller_host)
        && controller.port().is_some()
        && controller.username().is_empty()
        && controller.password().is_none()
        && controller.path() == "/"
        && controller.query().is_none()
        && controller.fragment().is_none();
    if !pipe_controller && !loopback_http {
        return Err(DomainError::InvalidNetwork(
            "The Mihomo controller must be a loopback HTTP URL such as http://127.0.0.1:9090/, or the Clash Verge kernel pipe."
                .to_owned(),
        ));
    }
    if binding.selector_group.trim().is_empty()
        || binding.selector_group.chars().count() > 128
        || binding.selector_group.chars().any(char::is_control)
        || binding.node_name.trim().is_empty()
        || binding.node_name.chars().count() > 256
        || binding.node_name.chars().any(char::is_control)
    {
        return Err(DomainError::InvalidNetwork(
            "Mihomo selector and node names must be short, non-empty values without control characters."
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    #[cfg(target_os = "windows")]
    use super::{trusted_windows_system_tool, WindowsSystemTool};
    use super::{
        validate_vault_name, vault_root, verify_browser_descriptor, windows_app_data_root,
        BrowserDescriptor, BrowserKind, BrowserVerificationState, CreateManagedSiloInput,
        ManagedIdentityPreset, NetworkProfile, ProxyScheme, Silo, SiloExecutionTarget,
    };

    #[test]
    fn named_vaults_are_validated_and_isolated_below_the_default_root() {
        let root = std::path::PathBuf::from("data");
        assert_eq!(vault_root(root.clone(), "default"), root);
        assert_eq!(
            vault_root(root, "agent_1"),
            std::path::PathBuf::from("data/vaults/agent_1")
        );
        assert!(validate_vault_name("agent_1").is_ok());
        assert!(validate_vault_name("Agent").is_err());
        assert!(validate_vault_name("../agent").is_err());
    }

    fn browser_fixture(output: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("verisilo-browser-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create browser fixture root");
        let executable = root.join("chrome.exe");
        fs::write(&executable, []).expect("create browser fixture");
        fs::write(executable.with_extension("version-output"), output)
            .expect("write version fixture");
        fs::canonicalize(executable).expect("canonical browser fixture")
    }

    #[test]
    fn browser_discovery_does_not_spawn_powershell() {
        let source = include_str!("domain.rs");
        let start = source
            .find("pub fn inspect_browser_executable(")
            .expect("inspect_browser_executable");
        let body = source
            .get(start..start.saturating_add(2500))
            .expect("inspect body");
        assert!(source.contains("fn windows_file_product_version("));
        assert!(body.contains("browser_identity_output"));
        assert!(!body.contains("Command::new"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_console_child_processes_request_create_no_window() {
        assert_eq!(
            super::CREATE_NO_WINDOW,
            0x0800_0000,
            "CREATE_NO_WINDOW must hide PowerShell and --version consoles without discarding redirected stdio"
        );
        let mut command = std::process::Command::new("powershell.exe");
        super::hide_windows_console(&mut command);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn trusted_windows_system_tools_return_spawnable_dos_paths() {
        let powershell = trusted_windows_system_tool(WindowsSystemTool::PowerShell)
            .expect("resolve trusted Windows PowerShell");
        let rendered = powershell.to_string_lossy();

        assert!(powershell.is_file());
        assert!(
            !rendered.starts_with(r"\\?\"),
            "trusted tools must not be returned with an extended-length prefix: {rendered}"
        );

    }

    #[test]
    fn browser_verification_reports_actual_version_and_drift_explicitly() {
        let executable = browser_fixture("Google Chrome 126.0.6478.127\n");
        let mut descriptor = BrowserDescriptor {
            kind: BrowserKind::Chrome,
            executable_path: executable.to_string_lossy().to_string(),
            version: Some("126.0.6478.127".to_owned()),
        };
        let verified = verify_browser_descriptor(&descriptor);
        assert_eq!(verified.state, BrowserVerificationState::Verified);
        assert_eq!(verified.actual_version.as_deref(), Some("126.0.6478.127"));

        descriptor.version = Some("125.0.0.0".to_owned());
        let drift = verify_browser_descriptor(&descriptor);
        assert_eq!(drift.state, BrowserVerificationState::VersionDrift);
        assert_eq!(drift.expected_version.as_deref(), Some("125.0.0.0"));
        assert_eq!(drift.actual_version.as_deref(), Some("126.0.6478.127"));
        fs::remove_dir_all(executable.parent().expect("fixture parent"))
            .expect("remove browser fixture");
    }

    #[test]
    fn legacy_silo_without_engine_deserializes_as_stock() {
        let silo: Silo = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "schemaVersion": 1,
            "name": "legacy",
            "color": "#4f46e5",
            "browser": {
                "kind": "chrome",
                "executablePath": "C:/Program Files/Google/Chrome/Application/chrome.exe",
                "version": "150.0.0.0"
            },
            "profileDirectory": "C:/VeriSilo/silos/legacy/browser-data",
            "networkProfile": { "mode": "direct", "proxyRequired": false },
            "seedReference": Uuid::new_v4(),
            "createdAt": "2026-07-28T00:00:00Z",
            "archivedAt": null
        }))
        .expect("legacy Silo");
        assert!(silo.engine.is_stock());
        assert_eq!(silo.execution_target, SiloExecutionTarget::Local);
        assert!(silo.identity_locked_at.is_none());
    }

    #[test]
    fn execution_target_wire_shape_is_strict_and_explicit() {
        assert_eq!(
            serde_json::to_value(SiloExecutionTarget::Local).expect("serialize local target"),
            serde_json::json!({ "kind": "local" })
        );
        assert_eq!(
            serde_json::to_value(SiloExecutionTarget::Wsl {
                distribution: "Ubuntu-24.04".to_owned(),
            })
            .expect("serialize WSL target"),
            serde_json::json!({
                "kind": "wsl",
                "distribution": "Ubuntu-24.04"
            })
        );
        assert_eq!(
            serde_json::to_value(SiloExecutionTarget::Remote {
                endpoint_origin: "https://remote.example.test:8443".to_owned(),
            })
            .expect("serialize remote target"),
            serde_json::json!({
                "kind": "remote",
                "endpointOrigin": "https://remote.example.test:8443"
            })
        );
        assert!(
            serde_json::from_value::<SiloExecutionTarget>(serde_json::json!({
                "kind": "wsl",
                "distribution": "Ubuntu",
                "futureField": true
            }))
            .is_err()
        );
    }

    #[test]
    fn execution_target_validation_rejects_ambiguous_runtime_addresses() {
        assert!(SiloExecutionTarget::Wsl {
            distribution: "Ubuntu-24.04".to_owned(),
        }
        .validate()
        .is_ok());
        for distribution in [
            "".to_owned(),
            "   ".to_owned(),
            "Ubuntu\n".to_owned(),
            "x".repeat(129),
        ] {
            assert!(SiloExecutionTarget::Wsl { distribution }
                .validate()
                .is_err());
        }

        assert!(SiloExecutionTarget::Remote {
            endpoint_origin: "https://remote.example.test:8443".to_owned(),
        }
        .validate()
        .is_ok());
        for endpoint_origin in [
            "http://remote.example.test".to_owned(),
            "https://user:password@remote.example.test".to_owned(),
            "https://@remote.example.test".to_owned(),
            "https://remote.example.test/runtime".to_owned(),
            "https://remote.example.test/.".to_owned(),
            "https://remote.example.test?tenant=one".to_owned(),
            "https://remote.example.test/#fragment".to_owned(),
        ] {
            assert!(SiloExecutionTarget::Remote { endpoint_origin }
                .validate()
                .is_err());
        }
    }

    #[test]
    fn silo_launch_keeps_site_data_local_and_disables_account_sync() {
        let silo: Silo = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "schemaVersion": 1,
            "name": "isolated",
            "color": "#4f46e5",
            "browser": {
                "kind": "edge",
                "executablePath": "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
                "version": "150.0.0.0"
            },
            "profileDirectory": "C:/VeriSilo/silos/isolated/browser-data",
            "networkProfile": { "mode": "direct", "proxyRequired": false },
            "seedReference": Uuid::new_v4(),
            "createdAt": "2026-07-28T00:00:00Z",
            "archivedAt": null
        }))
        .expect("isolated Silo");

        let arguments = silo
            .launch_arguments()
            .into_iter()
            .map(|argument| argument.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(arguments
            .contains(&"--user-data-dir=C:/VeriSilo/silos/isolated/browser-data".to_owned()));
        assert!(arguments.contains(&"--disable-sync".to_owned()));
    }

    #[test]
    fn browser_verification_rejects_a_kind_disguise() {
        let executable = browser_fixture("Microsoft Edge 126.0.0.0\n");
        let descriptor = BrowserDescriptor {
            kind: BrowserKind::Chrome,
            executable_path: executable.to_string_lossy().to_string(),
            version: Some("126.0.0.0".to_owned()),
        };
        let verification = verify_browser_descriptor(&descriptor);
        assert_eq!(verification.state, BrowserVerificationState::KindMismatch);
        fs::remove_dir_all(executable.parent().expect("fixture parent"))
            .expect("remove browser fixture");
    }

    #[test]
    fn direct_profiles_cannot_require_a_proxy() {
        let profile = NetworkProfile::Direct {
            proxy_required: true,
        };
        assert!(profile.validate().is_err());
    }

    #[test]
    fn fixed_proxy_arguments_are_constructed_without_shell_interpolation() {
        let profile = NetworkProfile::FixedProxy {
            proxy_required: false,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 1080,
            bypass_list: vec!["localhost".to_owned()],
            credential_reference: None,
            external_mihomo: None,
        };
        let arguments = profile
            .launch_arguments()
            .into_iter()
            .map(|argument| argument.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            arguments,
            vec![
                "--proxy-server=socks5://127.0.0.1:1080".to_owned(),
                "--proxy-bypass-list=localhost".to_owned(),
            ]
        );
    }

    #[test]
    fn proxy_ports_and_pac_credentials_are_rejected() {
        assert!(NetworkProfile::FixedProxy {
            proxy_required: false,
            scheme: ProxyScheme::Http,
            host: "127.0.0.1".to_owned(),
            port: 0,
            bypass_list: vec![],
            credential_reference: None,
            external_mihomo: None,
        }
        .validate()
        .is_err());
        assert!(NetworkProfile::Pac {
            proxy_required: false,
            pac_url: "https://user:pass@example.test/proxy.pac".to_owned(),
        }
        .validate()
        .is_err());
        assert!(NetworkProfile::Pac {
            proxy_required: true,
            pac_url: "https://example.test/proxy.pac".to_owned(),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn required_proxy_arguments_block_local_dns_quic_and_non_proxy_webrtc_udp() {
        let profile = NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 7890,
            bypass_list: vec![],
            credential_reference: None,
            external_mihomo: None,
        };
        let arguments = profile
            .launch_arguments()
            .into_iter()
            .map(|argument| argument.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(arguments.contains(&"--proxy-server=socks5://127.0.0.1:7890".to_owned()));
        assert!(arguments
            .contains(&"--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE 127.0.0.1".to_owned()));
        assert!(arguments.contains(&"--proxy-bypass-list=<-loopback>".to_owned()));
        assert!(arguments.contains(&"--disable-quic".to_owned()));
        assert!(
            arguments.contains(&"--webrtc-ip-handling-policy=disable_non_proxied_udp".to_owned())
        );
        assert!(arguments
            .iter()
            .all(|argument| !argument.contains("direct://")));
    }

    #[test]
    fn managed_identity_presets_have_the_exact_rc1_wire_names() {
        let presets = [
            (ManagedIdentityPreset::BalancedEnUs, "balanced-en-us"),
            (ManagedIdentityPreset::BalancedZhCn, "balanced-zh-cn"),
            (ManagedIdentityPreset::BalancedDeDe, "balanced-de-de"),
            (ManagedIdentityPreset::MatchFixedProxy, "match-fixed-proxy"),
        ];
        for (preset, expected) in presets {
            assert_eq!(
                serde_json::to_string(&preset).expect("preset JSON"),
                format!("\"{expected}\"")
            );
            assert_eq!(
                serde_json::from_str::<ManagedIdentityPreset>(&format!("\"{expected}\""))
                    .expect("preset decode"),
                preset
            );
        }
        assert!(serde_json::from_str::<ManagedIdentityPreset>("\"balanced\"").is_err());
        assert!(serde_json::from_str::<ManagedIdentityPreset>("\"privacy\"").is_err());
    }

    #[test]
    fn managed_identity_accepts_locale_with_required_proxy() {
        let direct = NetworkProfile::Direct {
            proxy_required: false,
        };
        let fixed = NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 7890,
            bypass_list: Vec::new(),
            credential_reference: None,
            external_mihomo: None,
        };
        for preset in [
            ManagedIdentityPreset::BalancedEnUs,
            ManagedIdentityPreset::BalancedZhCn,
            ManagedIdentityPreset::BalancedDeDe,
        ] {
            assert!(CreateManagedSiloInput {
                name: "managed".to_owned(),
                color: "#4f46e5".to_owned(),
                identity_preset: preset,
                follow_network_exit: false,
                screen_width: 1280,
                screen_height: 800,
                hardware_concurrency: None,
                gpu_preset: None,
                timezone: None,
                network_profile: direct.clone(),
                proxy_credentials: None,
                mihomo_controller_secret: None,
            }
            .validate()
            .is_ok());
            assert!(CreateManagedSiloInput {
                name: "managed".to_owned(),
                color: "#4f46e5".to_owned(),
                identity_preset: preset,
                follow_network_exit: true,
                screen_width: 1920,
                screen_height: 1080,
                hardware_concurrency: Some(8),
                gpu_preset: Some("nvidia-rtx-3060".to_owned()),
                timezone: Some("Asia/Tokyo".to_owned()),
                network_profile: fixed.clone(),
                proxy_credentials: None,
                mihomo_controller_secret: None,
            }
            .validate()
            .is_ok());
        }
        assert!(CreateManagedSiloInput {
            name: "managed".to_owned(),
            color: "#4f46e5".to_owned(),
            identity_preset: ManagedIdentityPreset::MatchFixedProxy,
            follow_network_exit: true,
            screen_width: 1280,
            screen_height: 800,
            hardware_concurrency: None,
            gpu_preset: None,
            timezone: None,
            network_profile: direct,
            proxy_credentials: None,
            mihomo_controller_secret: None,
        }
        .validate()
        .is_err());
        assert!(CreateManagedSiloInput {
            name: "managed".to_owned(),
            color: "#4f46e5".to_owned(),
            identity_preset: ManagedIdentityPreset::MatchFixedProxy,
            follow_network_exit: true,
            screen_width: 1280,
            screen_height: 800,
            hardware_concurrency: None,
            gpu_preset: None,
            timezone: None,
            network_profile: fixed,
            proxy_credentials: None,
            mihomo_controller_secret: None,
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn windows_app_data_root_prefers_rc1_path_without_orphaning_legacy_vaults() {
        let base = std::env::temp_dir().join(format!("verisilo-app-data-{}", Uuid::new_v4()));
        let current = base.join("io.verisilo.app");
        let legacy = base.join("VeriSilo");
        assert_eq!(windows_app_data_root(&base), current.clone());
        fs::create_dir_all(&legacy).expect("legacy app data");
        fs::write(legacy.join("vault.json"), b"encrypted").expect("legacy Vault");
        assert_eq!(windows_app_data_root(&base), current);
        super::migrate_legacy_app_data(&base).expect("atomically migrate legacy app data");
        assert!(current.join("vault.json").is_file());
        assert!(!legacy.exists());
        fs::create_dir_all(&legacy).expect("conflicting legacy app data");
        assert!(super::migrate_legacy_app_data(&base).is_err());
        fs::remove_dir_all(base).expect("remove app-data fixture");
    }

    #[test]
    fn windows_current_user_install_dir_is_not_migrated_as_legacy_vault() {
        let base = std::env::temp_dir().join(format!("verisilo-install-{}", Uuid::new_v4()));
        let current = base.join("io.verisilo.app");
        let install = base.join("VeriSilo");
        fs::create_dir_all(&install).expect("install dir");
        fs::write(install.join("verisilo.exe"), b"exe").expect("installed exe");
        fs::create_dir_all(&current).expect("rc1 data");
        super::migrate_legacy_app_data(&base)
            .expect("install directory must not be treated as a Vault");
        assert!(install.join("verisilo.exe").is_file());
        assert!(current.is_dir());
        assert!(!current.join("verisilo.exe").exists());
        fs::remove_dir_all(base).expect("remove install fixture");
    }
}
