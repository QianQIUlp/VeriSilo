use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDescriptor {
    pub kind: BrowserKind,
    pub executable_path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCandidate {
    pub kind: BrowserKind,
    pub display_name: String,
    pub executable_path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
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
    pub browser: BrowserDescriptor,
    pub profile_directory: String,
    pub network_profile: NetworkProfile,
    pub seed_reference: Uuid,
    pub created_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSiloInput {
    pub name: String,
    pub color: String,
    pub browser_kind: BrowserKind,
    pub executable_path: String,
    pub network_profile: NetworkProfile,
    #[serde(default)]
    pub proxy_credentials: Option<ProxyCredentialsInput>,
    #[serde(default)]
    pub mihomo_controller_secret: Option<MihomoControllerSecretInput>,
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
        if self.name.trim().is_empty() || self.name.chars().count() > 64 {
            return Err(DomainError::InvalidSilo(
                "Silo name must contain 1–64 characters.".to_owned(),
            ));
        }

        if !is_hex_color(&self.color) {
            return Err(DomainError::InvalidSilo(
                "Silo color must be a six-digit hex color.".to_owned(),
            ));
        }

        let executable_path = Path::new(&self.executable_path);
        if !executable_path.is_file() {
            return Err(DomainError::InvalidSilo(
                "The selected browser executable does not exist.".to_owned(),
            ));
        }

        self.network_profile.validate()?;

        if self.network_profile.credential_reference().is_some() {
            return Err(DomainError::InvalidNetwork(
                "A new Silo cannot supply an existing credential reference.".to_owned(),
            ));
        }
        if self
            .network_profile
            .mihomo_controller_secret_reference()
            .is_some()
        {
            return Err(DomainError::InvalidNetwork(
                "A new Silo cannot supply an existing Mihomo controller secret reference."
                    .to_owned(),
            ));
        }

        if let Some(credentials) = &self.proxy_credentials {
            if !matches!(&self.network_profile, NetworkProfile::FixedProxy { .. }) {
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
                    "Proxy credentials are empty, too long, or contain control characters."
                        .to_owned(),
                ));
            }
            let supports_local_auth_relay = matches!(
                &self.network_profile,
                NetworkProfile::FixedProxy {
                    scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                    ..
                }
            );
            if !supports_local_auth_relay {
                return Err(DomainError::InvalidNetwork(
                    "Automatic proxy authentication currently supports HTTP and SOCKS5. Use an external Mihomo endpoint for other authenticated protocols."
                        .to_owned(),
                ));
            }
        }

        if let Some(controller_secret) = &self.mihomo_controller_secret {
            if self.network_profile.external_mihomo_binding().is_none() {
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
    pub network_evidence: Option<RuntimeNetworkEvidence>,
}

impl RuntimeActivation {
    pub fn idle() -> Self {
        Self {
            active_silo_id: None,
            state: RuntimeState::Idle,
            updated_at: Utc::now(),
            message: None,
            network_evidence: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNetworkEvidence {
    pub provider: RuntimeNetworkProvider,
    pub configuration: RuntimeEvidenceState,
    pub controller_binding: RuntimeEvidenceState,
    pub endpoint: RuntimeEvidenceState,
    pub authentication: RuntimeEvidenceState,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEvidenceState {
    NotApplicable,
    NotRequested,
    Configured,
    Reachable,
    Applied,
    Verified,
    Failed,
    Unavailable,
}

impl RuntimeNetworkEvidence {
    pub fn configured(profile: &NetworkProfile, has_authentication: bool) -> Self {
        match profile {
            NetworkProfile::Direct { .. } => Self {
                provider: RuntimeNetworkProvider::Direct,
                configuration: RuntimeEvidenceState::Configured,
                controller_binding: RuntimeEvidenceState::NotApplicable,
                endpoint: RuntimeEvidenceState::NotApplicable,
                authentication: RuntimeEvidenceState::NotApplicable,
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
                provider: RuntimeNetworkProvider::Pac,
                configuration: RuntimeEvidenceState::Configured,
                controller_binding: RuntimeEvidenceState::NotApplicable,
                endpoint: RuntimeEvidenceState::Unavailable,
                authentication: RuntimeEvidenceState::NotApplicable,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Idle,
    Preflight,
    Launching,
    Running,
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
    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
}

pub fn app_data_root() -> Result<PathBuf, DomainError> {
    let root = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .ok_or_else(|| {
                DomainError::InvalidSilo(
                    "Windows application data directory is unavailable.".to_owned(),
                )
            })?
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .ok_or_else(|| {
                DomainError::InvalidSilo("Application data directory is unavailable.".to_owned())
            })?
    }
    .join("VeriSilo");

    fs::create_dir_all(&root)?;
    Ok(root)
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
            candidates.push(BrowserCandidate {
                display_name: kind.display_name().to_owned(),
                version: browser_version(&path),
                executable_path: path.to_string_lossy().to_string(),
                kind: kind.clone(),
            });
        }
    }
    candidates
}

fn browser_version(executable_path: &Path) -> Option<String> {
    std::process::Command::new(executable_path)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
            } else {
                None
            }
        })
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
    if controller.scheme() != "http"
        || !is_loopback_host(controller_host)
        || controller.port().is_none()
        || !controller.username().is_empty()
        || controller.password().is_some()
        || controller.path() != "/"
        || controller.query().is_some()
        || controller.fragment().is_some()
    {
        return Err(DomainError::InvalidNetwork(
            "The Mihomo controller must be an explicit loopback HTTP URL such as http://127.0.0.1:9090/."
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
    use super::{NetworkProfile, ProxyScheme};

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
}
