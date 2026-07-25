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

use crate::domain::{NetworkProfile, RuntimeActivation, RuntimeState, Silo};

#[derive(Debug, Default)]
pub struct RuntimeManager {
    child: Option<Child>,
    activation: Option<RuntimeActivation>,
}

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("Another VeriSilo browser environment is already running. Close it before launching another Silo.")]
    AnotherSiloRunning,
    #[error("This Silo appears to be in use by a browser process. VeriSilo will not remove its profile lock.")]
    ProfileInUse,
    #[error("Proxy preflight failed: {0}")]
    ProxyPreflight(String),
    #[error("Network configuration is invalid: {0}")]
    InvalidNetwork(String),
    #[error("Could not start the selected browser: {0}")]
    Spawn(#[from] std::io::Error),
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

        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Preflight,
            updated_at: Utc::now(),
            message: Some("Checking the isolated browser profile before launch.".to_owned()),
        });

        if managed_profile_directories
            .iter()
            .any(|directory| profile_in_use(directory))
        {
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some("A managed Silo profile is already in use.".to_owned()),
            });
            return Err(LauncherError::ProfileInUse);
        }

        if let Err(error) = preflight_proxy(&silo.network_profile) {
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
            });
            return Err(error);
        }

        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Launching,
            updated_at: Utc::now(),
            message: Some("Launching the browser with a dedicated data directory.".to_owned()),
        });

        let child = match Command::new(&silo.browser.executable_path)
            .args(silo.launch_arguments())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(format!("Could not start the selected browser: {error}")),
                });
                return Err(LauncherError::Spawn(error));
            }
        };

        let activation = RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Running,
            updated_at: Utc::now(),
            message: Some("The browser is running in its own VeriSilo data directory.".to_owned()),
        };
        self.child = Some(child);
        self.activation = Some(activation.clone());
        Ok(activation)
    }

    fn refresh(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };

        if child.try_wait().ok().flatten().is_some() {
            self.child = None;
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Stopped,
                updated_at: Utc::now(),
                message: Some("The managed browser process exited.".to_owned()),
            });
        }
    }
}

pub fn profile_in_use(profile_directory: &Path) -> bool {
    ["SingletonLock", "SingletonCookie", "SingletonSocket"]
        .iter()
        .any(|name| fs::symlink_metadata(profile_directory.join(name)).is_ok())
}

fn preflight_proxy(profile: &NetworkProfile) -> Result<(), LauncherError> {
    match profile {
        NetworkProfile::Direct { .. } => Ok(()),
        NetworkProfile::FixedProxy {
            proxy_required,
            host,
            port,
            ..
        } if *proxy_required => {
            let address = format!("{host}:{port}");
            let socket = address
                .to_socket_addrs()
                .map_err(|error| LauncherError::ProxyPreflight(error.to_string()))?
                .next()
                .ok_or_else(|| LauncherError::ProxyPreflight("No proxy address could be resolved.".to_owned()))?;
            TcpStream::connect_timeout(&socket, Duration::from_secs(3))
                .map_err(|error| LauncherError::ProxyPreflight(error.to_string()))?;
            Ok(())
        }
        NetworkProfile::FixedProxy { .. } => Ok(()),
        NetworkProfile::Pac {
            proxy_required: true,
            ..
        } => Err(LauncherError::ProxyPreflight(
            "PAC profiles require an explicit browser exit test before they can be marked verified; launch with proxy-required disabled until such a test is configured.".to_owned(),
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
            .launch(&silo, &[std::path::PathBuf::from(&silo.profile_directory)])
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
            .launch(&silo, &[std::path::PathBuf::from(&silo.profile_directory)])
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
            )
            .is_err());
        let activation = runtime.activation();
        assert!(activation.active_silo_id.is_none());
        assert!(matches!(activation.state, RuntimeState::Failed));

        fs::remove_dir_all(locked_directory).expect("remove locked profile directory");
    }
}
