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
    },
    #[serde(rename = "pac")]
    Pac {
        proxy_required: bool,
        pac_url: String,
    },
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
                host,
                port,
                bypass_list,
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
        match self {
            Self::Direct { .. } => vec![OsString::from("--no-proxy-server")],
            Self::FixedProxy {
                scheme,
                host,
                port,
                bypass_list,
                ..
            } => {
                let mut arguments = vec![OsString::from(format!(
                    "--proxy-server={}://{}:{}",
                    scheme.as_str(),
                    host,
                    port
                ))];
                if !bypass_list.is_empty() {
                    arguments.push(OsString::from(format!(
                        "--proxy-bypass-list={}",
                        bypass_list.join(";")
                    )));
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSiloInput {
    pub name: String,
    pub color: String,
    pub browser_kind: BrowserKind,
    pub executable_path: String,
    pub network_profile: NetworkProfile,
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

        self.network_profile.validate()
    }
}

impl Silo {
    pub fn launch_arguments(&self) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from(format!("--user-data-dir={}", self.profile_directory)),
            OsString::from("--no-first-run"),
            OsString::from("--no-default-browser-check"),
        ];
        arguments.extend(self.network_profile.launch_arguments());
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
}

impl RuntimeActivation {
    pub fn idle() -> Self {
        Self {
            active_silo_id: None,
            state: RuntimeState::Idle,
            updated_at: Utc::now(),
            message: None,
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
}
