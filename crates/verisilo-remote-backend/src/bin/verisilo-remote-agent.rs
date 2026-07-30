//! Operator entry point for the V0.9 self-hosted Remote Agent.
//!
//! The hardened auth-store implementation in this release targets Unix/Linux:
//! it relies on `0600`, `flock`, same-directory atomic rename, and directory
//! fsync. Non-Unix builds fail closed at startup instead of pretending to offer
//! equivalent persistence. This daemon provides the HTTPS control plane only;
//! it does not claim a WAN/VM/browser/media-stream end-to-end deployment.

#[cfg(unix)]
#[allow(dead_code)]
#[path = "../agent_service.rs"]
mod agent_service;
#[cfg(unix)]
#[allow(dead_code)]
#[path = "../auth_store.rs"]
mod auth_store;
#[cfg(unix)]
#[path = "../durable_store.rs"]
mod durable_store;
#[cfg(unix)]
#[allow(dead_code)]
#[path = "../https_server.rs"]
mod https_server;
#[cfg(unix)]
#[path = "../provider_bridge.rs"]
mod provider_bridge;

#[cfg(unix)]
use std::{
    env,
    ffi::OsString,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use thiserror::Error;
#[cfg(unix)]
use uuid::Uuid;

#[cfg(unix)]
use verisilo_remote_backend::{
    agent::{
        AgentCore, AgentError, AgentProvider, EnvironmentRecord, InputEvent, LifecycleReceipt,
        ProviderDeletionReceipt, ProvisionReceipt, ScreenChannel,
    },
    RemoteCapability, RemoteOperation, SystemClock,
};

#[cfg(unix)]
use agent_service::{AgentService, ServiceResponse};
#[cfg(unix)]
use auth_store::AuthStore;
#[cfg(unix)]
use durable_store::DurableAgentStore;
#[cfg(unix)]
use https_server::{
    load_configuration, load_tls_configuration, serve_https, ApplicationResponse,
    JsonRequestHandler, LocalProviderConfiguration,
};
#[cfg(unix)]
use provider_bridge::{ProviderBridgeConfiguration, StdioAgentProvider};

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("verisilo-remote-agent: V0.9 secure persistence is supported only on Unix/Linux");
    std::process::ExitCode::FAILURE
}

#[cfg(unix)]
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("verisilo-remote-agent: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(unix)]
fn run() -> Result<(), OperatorError> {
    match parse_arguments()? {
        OperatorCommand::InitToken {
            configuration_path,
            lifetime_seconds,
        } => init_token(&configuration_path, lifetime_seconds),
        OperatorCommand::RevokeAll { configuration_path } => revoke_all(&configuration_path),
        OperatorCommand::Serve { configuration_path } => serve(&configuration_path),
    }
}

#[cfg(unix)]
fn init_token(configuration_path: &Path, lifetime_seconds: u64) -> Result<(), OperatorError> {
    if !std::io::stdout().is_terminal() {
        return Err(OperatorError::TerminalRequired);
    }
    let configuration =
        load_configuration(configuration_path).map_err(|_| OperatorError::ConfigurationInvalid)?;
    let lifetime_ms = lifetime_seconds
        .checked_mul(1_000)
        .filter(|value| *value <= auth_store::MAX_PAIRING_TOKEN_LIFETIME_MS)
        .ok_or(OperatorError::ArgumentsInvalid)?;
    let mut auth =
        AuthStore::open(configuration.auth_state_path).map_err(|_| OperatorError::StateFailed)?;
    let token = auth
        .issue_pairing_token(now_unix_ms(), lifetime_ms)
        .map_err(|_| OperatorError::StateFailed)?;

    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "pairing_token_id={}", token.token_id())
        .and_then(|_| {
            writeln!(
                output,
                "pairing_token_expires_at_unix_ms={}",
                token.expires_at_unix_ms()
            )
        })
        // The plaintext secret occurs exactly once in operator output.
        .and_then(|_| writeln!(output, "pairing_token={}", token.secret()))
        .and_then(|_| output.flush())
        .map_err(|_| OperatorError::TerminalWriteFailed)
}

#[cfg(unix)]
fn revoke_all(configuration_path: &Path) -> Result<(), OperatorError> {
    let configuration =
        load_configuration(configuration_path).map_err(|_| OperatorError::ConfigurationInvalid)?;
    let mut auth =
        AuthStore::open(configuration.auth_state_path).map_err(|_| OperatorError::StateFailed)?;
    let revoked = auth
        .revoke_all_credentials()
        .map_err(|_| OperatorError::StateFailed)?;
    println!("revoked_credentials={revoked}");
    Ok(())
}

#[cfg(unix)]
fn serve(configuration_path: &Path) -> Result<(), OperatorError> {
    let configuration =
        load_configuration(configuration_path).map_err(|_| OperatorError::ConfigurationInvalid)?;
    let tls = load_tls_configuration(&configuration)
        .map_err(|_| OperatorError::TlsConfigurationFailed)?;
    let auth = AuthStore::open(configuration.auth_state_path.clone())
        .map_err(|_| OperatorError::StateFailed)?;
    let store = DurableAgentStore::open(configuration.agent_state_path.clone())
        .map_err(|_| OperatorError::StateFailed)?;
    let capabilities = configuration.provider.capabilities().to_vec();
    let provider = RuntimeProvider::from_configuration(configuration.provider.clone())?;
    let core = AgentCore::new(configuration.node.clone(), provider, store, SystemClock)
        .map_err(|_| OperatorError::StartupFailed)?;
    let service = AgentService::new(
        auth,
        core,
        configuration.node.clone(),
        capabilities,
        SystemClock,
        configuration
            .credential_lifetime_ms()
            .map_err(|_| OperatorError::ConfigurationInvalid)?,
    )
    .map_err(|_| OperatorError::StartupFailed)?;
    let mut handler = ServiceHandler { service };
    serve_https(&configuration, tls, &mut handler).map_err(|_| OperatorError::ListenerFailed)
}

#[cfg(unix)]
struct ServiceHandler {
    service: RuntimeService,
}

#[cfg(unix)]
type RuntimeService =
    AgentService<AgentCore<RuntimeProvider, DurableAgentStore, SystemClock>, SystemClock>;

#[cfg(unix)]
impl JsonRequestHandler for ServiceHandler {
    fn handle_json(&mut self, bearer: Option<&str>, body: &[u8]) -> ApplicationResponse {
        let response: ServiceResponse = self.service.route(bearer, body);
        let (status_code, body) = response.into_parts();
        ApplicationResponse::new(status_code, body)
    }

    fn maintenance_tick(&mut self) {
        self.service.maintenance_tick();
    }
}

#[cfg(unix)]
enum RuntimeProvider {
    Stdio(StdioAgentProvider),
    Unavailable(UnavailableAgentProvider),
}

#[cfg(unix)]
impl RuntimeProvider {
    fn from_configuration(
        configuration: LocalProviderConfiguration,
    ) -> Result<Self, OperatorError> {
        match configuration {
            LocalProviderConfiguration::Stdio {
                executable_path,
                executable_sha256,
                capabilities,
            } => StdioAgentProvider::new(ProviderBridgeConfiguration {
                executable_path,
                executable_sha256,
                capabilities,
            })
            .map(Self::Stdio)
            .map_err(|_| OperatorError::ProviderFailed),
            LocalProviderConfiguration::Unavailable { capabilities } => {
                Ok(Self::Unavailable(UnavailableAgentProvider { capabilities }))
            }
        }
    }
}

#[cfg(unix)]
impl AgentProvider for RuntimeProvider {
    fn capabilities(&self) -> Vec<RemoteCapability> {
        match self {
            Self::Stdio(provider) => provider.capabilities(),
            Self::Unavailable(provider) => provider.capabilities(),
        }
    }

    fn create(&mut self, record: &EnvironmentRecord) -> Result<ProvisionReceipt, AgentError> {
        match self {
            Self::Stdio(provider) => provider.create(record),
            Self::Unavailable(provider) => provider.create(record),
        }
    }

    fn lifecycle(
        &mut self,
        operation: RemoteOperation,
        record: &EnvironmentRecord,
        log_limit: Option<u16>,
    ) -> Result<LifecycleReceipt, AgentError> {
        match self {
            Self::Stdio(provider) => provider.lifecycle(operation, record, log_limit),
            Self::Unavailable(provider) => provider.lifecycle(operation, record, log_limit),
        }
    }

    fn destroy(
        &mut self,
        record: &EnvironmentRecord,
    ) -> Result<ProviderDeletionReceipt, AgentError> {
        match self {
            Self::Stdio(provider) => provider.destroy(record),
            Self::Unavailable(provider) => provider.destroy(record),
        }
    }

    fn open_screen(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        expires_at_unix_ms: u64,
    ) -> Result<ScreenChannel, AgentError> {
        match self {
            Self::Stdio(provider) => {
                provider.open_screen(record, authorization_id, expires_at_unix_ms)
            }
            Self::Unavailable(provider) => {
                provider.open_screen(record, authorization_id, expires_at_unix_ms)
            }
        }
    }

    fn send_input(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        events: &[InputEvent],
    ) -> Result<(), AgentError> {
        match self {
            Self::Stdio(provider) => provider.send_input(record, authorization_id, events),
            Self::Unavailable(provider) => provider.send_input(record, authorization_id, events),
        }
    }
}

#[cfg(unix)]
struct UnavailableAgentProvider {
    capabilities: Vec<RemoteCapability>,
}

#[cfg(unix)]
impl UnavailableAgentProvider {
    fn unavailable<T>() -> Result<T, AgentError> {
        Err(AgentError::Unavailable(
            "No local provider artifact is configured.".to_owned(),
        ))
    }
}

#[cfg(unix)]
impl AgentProvider for UnavailableAgentProvider {
    fn capabilities(&self) -> Vec<RemoteCapability> {
        self.capabilities.clone()
    }

    fn create(&mut self, _record: &EnvironmentRecord) -> Result<ProvisionReceipt, AgentError> {
        Self::unavailable()
    }

    fn lifecycle(
        &mut self,
        _operation: RemoteOperation,
        _record: &EnvironmentRecord,
        _log_limit: Option<u16>,
    ) -> Result<LifecycleReceipt, AgentError> {
        Self::unavailable()
    }

    fn destroy(
        &mut self,
        _record: &EnvironmentRecord,
    ) -> Result<ProviderDeletionReceipt, AgentError> {
        Self::unavailable()
    }

    fn open_screen(
        &mut self,
        _record: &EnvironmentRecord,
        _authorization_id: Uuid,
        _expires_at_unix_ms: u64,
    ) -> Result<ScreenChannel, AgentError> {
        Self::unavailable()
    }

    fn send_input(
        &mut self,
        _record: &EnvironmentRecord,
        _authorization_id: Uuid,
        _events: &[InputEvent],
    ) -> Result<(), AgentError> {
        Self::unavailable()
    }
}

#[cfg(unix)]
enum OperatorCommand {
    InitToken {
        configuration_path: PathBuf,
        lifetime_seconds: u64,
    },
    RevokeAll {
        configuration_path: PathBuf,
    },
    Serve {
        configuration_path: PathBuf,
    },
}

#[cfg(unix)]
fn parse_arguments() -> Result<OperatorCommand, OperatorError> {
    let mut arguments = env::args_os().skip(1);
    let command = arguments.next().ok_or(OperatorError::ArgumentsInvalid)?;
    let mut configuration_path = None;
    let mut lifetime_seconds = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--config") if configuration_path.is_none() => {
                configuration_path = Some(PathBuf::from(
                    arguments.next().ok_or(OperatorError::ArgumentsInvalid)?,
                ));
            }
            Some("--lifetime-seconds") if lifetime_seconds.is_none() => {
                let raw: OsString = arguments.next().ok_or(OperatorError::ArgumentsInvalid)?;
                let parsed = raw
                    .to_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| (30..=300).contains(value))
                    .ok_or(OperatorError::ArgumentsInvalid)?;
                lifetime_seconds = Some(parsed);
            }
            _ => return Err(OperatorError::ArgumentsInvalid),
        }
    }
    let configuration_path = configuration_path.ok_or(OperatorError::ArgumentsInvalid)?;
    match command.to_str() {
        Some("init-token") => Ok(OperatorCommand::InitToken {
            configuration_path,
            lifetime_seconds: lifetime_seconds.unwrap_or(300),
        }),
        Some("revoke-all") if lifetime_seconds.is_none() => {
            Ok(OperatorCommand::RevokeAll { configuration_path })
        }
        Some("serve") if lifetime_seconds.is_none() => {
            Ok(OperatorCommand::Serve { configuration_path })
        }
        _ => Err(OperatorError::ArgumentsInvalid),
    }
}

#[cfg(unix)]
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(unix)]
#[derive(Debug, Error)]
enum OperatorError {
    #[error("arguments are invalid; use init-token|revoke-all|serve --config /absolute/path")]
    ArgumentsInvalid,
    #[error("init-token requires an interactive terminal to avoid redirecting its secret")]
    TerminalRequired,
    #[error("could not write the one-time pairing token to the terminal")]
    TerminalWriteFailed,
    #[error("the strict local server configuration is invalid")]
    ConfigurationInvalid,
    #[error("the TLS certificate chain or private key is invalid")]
    TlsConfigurationFailed,
    #[error("the durable local state could not be opened or updated")]
    StateFailed,
    #[error("the configured fixed provider could not be verified")]
    ProviderFailed,
    #[error("the Remote Agent service could not initialize")]
    StartupFailed,
    #[error("the TLS listener stopped")]
    ListenerFailed,
}
