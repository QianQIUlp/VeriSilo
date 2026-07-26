use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command},
    time::Duration,
};

use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    NetworkProfile, RuntimeActivation, RuntimeEvidenceState, RuntimeNetworkEvidence, RuntimeState,
    Silo,
};
use crate::{
    mihomo,
    proxy_relay::ProxyRelay,
    vault::{MihomoControllerAuthentication, ProxyAuthentication},
};

#[derive(Debug, Default)]
pub struct RuntimeManager {
    child: Option<Child>,
    activation: Option<RuntimeActivation>,
    proxy_relay: Option<ProxyRelay>,
}

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("已有一个 VeriSilo 浏览器环境正在运行；关闭后才能启动另一个 Silo。")]
    AnotherSiloRunning,
    #[error("检测到受管 Silo 的浏览器锁；VeriSilo 不会删除锁或强制结束浏览器。")]
    ProfileInUse,
    #[error("代理启动前检查失败：{0}")]
    ProxyPreflight(String),
    #[error("网络配置无效：{0}")]
    InvalidNetwork(String),
    #[error("无法启动所选浏览器：{0}")]
    Spawn(#[from] std::io::Error),
    #[error("无法启动禁止直连回退的本机代理中继：{0}")]
    ProxyRelay(String),
    #[error("无法绑定所选 Mihomo 节点：{0}")]
    Mihomo(String),
}

impl RuntimeManager {
    pub fn activation(&mut self) -> RuntimeActivation {
        self.refresh();
        self.activation
            .clone()
            .unwrap_or_else(RuntimeActivation::idle)
    }

    pub fn is_active(&mut self, silo_id: Uuid) -> bool {
        self.refresh();
        self.activation
            .as_ref()
            .and_then(|activation| activation.active_silo_id)
            .is_some_and(|active_silo_id| active_silo_id == silo_id)
    }

    pub fn launch(
        &mut self,
        silo: &Silo,
        managed_profile_directories: &[PathBuf],
        proxy_authentication: Option<ProxyAuthentication>,
        mihomo_authentication: Option<MihomoControllerAuthentication>,
    ) -> Result<RuntimeActivation, LauncherError> {
        self.refresh();
        if self
            .activation
            .as_ref()
            .and_then(|activation| activation.active_silo_id)
            .is_some()
        {
            return Err(LauncherError::AnotherSiloRunning);
        }

        silo.network_profile
            .validate()
            .map_err(|error| LauncherError::InvalidNetwork(error.to_string()))?;

        let mut network_evidence = RuntimeNetworkEvidence::configured(
            &silo.network_profile,
            proxy_authentication.is_some(),
        );
        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Preflight,
            updated_at: Utc::now(),
            message: Some("正在检查浏览器目录、代理端点和本次网络绑定。".to_owned()),
            network_evidence: Some(network_evidence.clone()),
        });

        if managed_profile_directories
            .iter()
            .any(|directory| profile_in_use(directory))
        {
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some("另一个受管 Silo 的浏览器目录正在使用中。".to_owned()),
                network_evidence: Some(network_evidence),
            });
            return Err(LauncherError::ProfileInUse);
        }

        if let Some(binding) = silo.network_profile.external_mihomo_binding() {
            if let Err(error) = mihomo::apply_binding(binding, mihomo_authentication.as_ref()) {
                network_evidence.controller_binding = RuntimeEvidenceState::Failed;
                let error = LauncherError::Mihomo(error.to_string());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    network_evidence: Some(network_evidence),
                });
                return Err(error);
            }
            network_evidence.controller_binding = RuntimeEvidenceState::Verified;
        }

        if let Err(error) = preflight_proxy(
            &silo.network_profile,
            proxy_authentication.as_ref(),
            &mut network_evidence,
        ) {
            network_evidence.endpoint = RuntimeEvidenceState::Failed;
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                network_evidence: Some(network_evidence),
            });
            return Err(error);
        }

        let use_proxy_relay = ProxyRelay::supports(&silo.network_profile)
            && (silo.network_profile.requires_proxy()
                || proxy_authentication.is_some()
                || silo.network_profile.external_mihomo_binding().is_some());
        let proxy_relay = use_proxy_relay
            .then(|| ProxyRelay::start(&silo.network_profile, proxy_authentication))
            .transpose()
            .map_err(|error| {
                network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                let launcher_error = LauncherError::ProxyRelay(error.to_string());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(launcher_error.to_string()),
                    network_evidence: Some(network_evidence.clone()),
                });
                launcher_error
            })?;

        if let Some(relay) = proxy_relay.as_ref() {
            let upstream = network_evidence
                .endpoint_label
                .as_deref()
                .unwrap_or("proxy");
            network_evidence.endpoint_label = Some(format!(
                "{}:{} → {upstream}",
                relay.endpoint().host,
                relay.endpoint().port
            ));
        }

        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Launching,
            updated_at: Utc::now(),
            message: Some("正在用独立数据目录和已检查的网络路径启动浏览器。".to_owned()),
            network_evidence: Some(network_evidence.clone()),
        });

        let proxy_override = proxy_relay
            .as_ref()
            .map(|relay| (relay.endpoint().host.as_str(), relay.endpoint().port));
        let child = match Command::new(&silo.browser.executable_path)
            .args(silo.launch_arguments_with_proxy_override(proxy_override))
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(format!("无法启动所选浏览器：{error}")),
                    network_evidence: Some({
                        network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                        network_evidence
                    }),
                });
                return Err(LauncherError::Spawn(error));
            }
        };

        network_evidence.browser_routing = RuntimeEvidenceState::Applied;
        let activation = RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Running,
            updated_at: Utc::now(),
            message: Some(
                "Silo 正在运行。请在这个 Silo 的 Companion 中主动验证实际出口、DNS 证据和 WebRTC 路径。"
                    .to_owned(),
            ),
            network_evidence: Some(network_evidence),
        };
        self.child = Some(child);
        self.proxy_relay = proxy_relay;
        self.activation = Some(activation.clone());
        Ok(activation)
    }

    fn refresh(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };

        if child.try_wait().ok().flatten().is_some() {
            let network_evidence = self
                .activation
                .as_ref()
                .and_then(|activation| activation.network_evidence.clone());
            self.child = None;
            self.proxy_relay = None;
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Stopped,
                updated_at: Utc::now(),
                message: Some("受管浏览器进程已退出。".to_owned()),
                network_evidence,
            });
        }
    }
}

pub fn profile_in_use(profile_directory: &Path) -> bool {
    ["SingletonLock", "SingletonCookie", "SingletonSocket"]
        .iter()
        .any(|name| fs::symlink_metadata(profile_directory.join(name)).is_ok())
}

fn preflight_proxy(
    profile: &NetworkProfile,
    authentication: Option<&ProxyAuthentication>,
    evidence: &mut RuntimeNetworkEvidence,
) -> Result<(), LauncherError> {
    match profile {
        NetworkProfile::Direct { .. } => Ok(()),
        NetworkProfile::FixedProxy {
            proxy_required,
            host,
            port,
            ..
        } if *proxy_required || authentication.is_some() => {
            if ProxyRelay::supports(profile) {
                let preflight = ProxyRelay::preflight_upstream(profile, authentication)
                    .map_err(|error| LauncherError::ProxyPreflight(error.to_string()))?;
                evidence.endpoint = RuntimeEvidenceState::Reachable;
                if authentication.is_some() {
                    evidence.authentication = if preflight.authentication_verified {
                        RuntimeEvidenceState::Verified
                    } else {
                        RuntimeEvidenceState::Configured
                    };
                }
                return Ok(());
            }
            let socket = (host.trim_matches(['[', ']']), *port)
                .to_socket_addrs()
                .map_err(|error| LauncherError::ProxyPreflight(error.to_string()))?
                .next()
                .ok_or_else(|| {
                    LauncherError::ProxyPreflight("代理主机没有解析到可连接地址。".to_owned())
                })?;
            TcpStream::connect_timeout(&socket, Duration::from_secs(3))
                .map_err(|error| LauncherError::ProxyPreflight(error.to_string()))?;
            evidence.endpoint = RuntimeEvidenceState::Reachable;
            Ok(())
        }
        NetworkProfile::FixedProxy { .. } => {
            evidence.endpoint = RuntimeEvidenceState::NotRequested;
            Ok(())
        }
        NetworkProfile::Pac {
            proxy_required: true,
            ..
        } => Err(LauncherError::ProxyPreflight(
            "PAC 当前没有可证明无 DIRECT 回退的启动前出口检查，因此不能启用“必须代理”。".to_owned(),
        )),
        NetworkProfile::Pac { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
    use uuid::Uuid;

    use super::RuntimeManager;
    use crate::domain::{
        BrowserDescriptor, BrowserKind, NetworkProfile, RuntimeState, Silo, SCHEMA_VERSION,
    };

    fn test_silo(network_profile: NetworkProfile) -> Silo {
        Silo {
            id: Uuid::new_v4(),
            schema_version: SCHEMA_VERSION,
            name: "test".to_owned(),
            color: "#4f46e5".to_owned(),
            browser: BrowserDescriptor {
                kind: BrowserKind::Chrome,
                executable_path: "C:\\does-not-exist\\chrome.exe".to_owned(),
                version: None,
            },
            profile_directory: std::env::temp_dir()
                .join(format!("verisilo-launcher-test-{}", Uuid::new_v4()))
                .to_string_lossy()
                .to_string(),
            network_profile,
            seed_reference: Uuid::new_v4(),
            created_at: Utc::now(),
            archived_at: None,
        }
    }

    #[test]
    fn failed_proxy_preflight_does_not_leave_a_silo_active() {
        let mut runtime = RuntimeManager::default();
        let silo = test_silo(NetworkProfile::Pac {
            proxy_required: true,
            pac_url: "https://example.test/proxy.pac".to_owned(),
        });

        assert!(runtime
            .launch(
                &silo,
                &[std::path::PathBuf::from(&silo.profile_directory)],
                None,
                None,
            )
            .is_err());
        let activation = runtime.activation();
        assert!(activation.active_silo_id.is_none());
        assert!(matches!(activation.state, RuntimeState::Failed));
    }

    #[test]
    fn failed_browser_spawn_does_not_leave_a_silo_active() {
        let mut runtime = RuntimeManager::default();
        let silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });

        assert!(runtime
            .launch(
                &silo,
                &[std::path::PathBuf::from(&silo.profile_directory)],
                None,
                None,
            )
            .is_err());
        let activation = runtime.activation();
        assert!(activation.active_silo_id.is_none());
        assert!(matches!(activation.state, RuntimeState::Failed));
    }

    #[test]
    fn a_lock_in_another_managed_silo_blocks_a_new_launch() {
        let mut runtime = RuntimeManager::default();
        let silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });
        let locked_silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });
        let locked_directory = std::path::PathBuf::from(&locked_silo.profile_directory);
        fs::create_dir_all(&locked_directory).expect("create locked profile directory");
        fs::write(locked_directory.join("SingletonLock"), []).expect("create profile lock");

        assert!(runtime
            .launch(
                &silo,
                &[
                    std::path::PathBuf::from(&silo.profile_directory),
                    locked_directory.clone(),
                ],
                None,
                None,
            )
            .is_err());
        let activation = runtime.activation();
        assert!(activation.active_silo_id.is_none());
        assert!(matches!(activation.state, RuntimeState::Failed));

        fs::remove_dir_all(locked_directory).expect("remove locked profile directory");
    }
}
