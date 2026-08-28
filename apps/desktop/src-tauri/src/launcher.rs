use std::{
    ffi::OsString,
    fs,
    io::{BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    verify_browser_descriptor, BrowserVerificationState, NetworkProfile, RuntimeActivation,
    RuntimeEngineEvidence, RuntimeEvidenceState, RuntimeNetworkEvidence,
    RuntimeNetworkEvidenceProvenance, RuntimeState, Silo,
};
#[cfg(test)]
use crate::engine::EngineAdapter;
#[cfg(target_os = "windows")]
use crate::vault::chromium_profile_sentinel_exists;
use crate::{
    engine::{
        production_engine_adapter, read_engine_bootstrap_ack_frame,
        read_engine_runtime_receipt_frame, strict_json_from_slice, write_engine_bootstrap_frame,
        CamoufoxHostLaunch, CamoufoxHostRoots, EngineBootstrapAck, EngineBootstrapAckExpectation,
        EngineBootstrapEnvelope, EngineCapabilityId, EngineCapabilityOperation,
        EngineCapabilityState, EngineControlExecution, EngineHealthState, EngineLaunchPlan,
        EngineLaunchRequest, EngineRuntimeReceiptExpectation, EngineRuntimeReceiptFrame,
        EngineTransport, IdentityDerivationContext, IdentityTokenDeriver, CAMOUFOX_HOST_PROTOCOL,
        DEFAULT_SESSION_TOKEN_LIFETIME_MINUTES, MAX_CAMOUFOX_HOST_FRAME_BYTES,
    },
    mihomo,
    native_host::{
        network_evidence_has_public_ip_observation, validate_network_evidence_inbox_entry,
        NativeNetworkEvidenceInboxEntry,
    },
    proxy_relay::{ProxyRelay, RelayAuthenticationEvidence},
    vault::{
        profile_has_browser_lock, BrowserProfileLease, MihomoControllerAuthentication,
        ProxyAuthentication,
    },
};

const RUNTIME_RECORD_DIRECTORY: &str = "runtime";
const RUNTIME_RECORD_FILE: &str = "browser-session.json";
const ENGINE_BOOTSTRAP_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const ENGINE_INITIAL_RECEIPT_TIMEOUT: Duration = Duration::from_secs(5);
const ENGINE_EXIT_RECEIPT_GRACE: Duration = Duration::from_millis(100);
#[cfg(target_os = "windows")]
const STOCK_BROWSER_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(target_os = "windows")]
const STOCK_BROWSER_OWNERSHIP_STABILITY: Duration = Duration::from_millis(350);
const ENGINE_PROTOCOL_CHANNEL_CAPACITY: usize = 32;
const HTTP_AUTH_EVIDENCE_LOOKBACK_SECONDS: i64 = 15;
const EVIDENCE_CLOCK_SKEW_SECONDS: i64 = 5;
pub(crate) const RUNTIME_HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const M3_WI_REAL_HOST_ADAPTER_VERSION: &str = "m3-wi-test-only-real-host";
#[cfg(test)]
const M3_WI_REAL_HOST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct RuntimeManager {
    child: Option<Child>,
    activation: Option<RuntimeActivation>,
    proxy_relay: Option<ProxyRelay>,
    health_context: Option<RuntimeHealthContext>,
    engine_runtime: Option<EngineRuntimeProtocol>,
    profile_lease: Option<BrowserProfileLease>,
    #[cfg(target_os = "windows")]
    pending_stock_profile_release: Option<PathBuf>,
    record_path: Option<PathBuf>,
    record: Option<RuntimeRecord>,
    #[cfg(test)]
    test_engine_adapter: Option<Box<dyn EngineAdapter>>,
}

pub(crate) struct VaultRestoreRuntimePreparation {
    _private: (),
}

struct RuntimeHealthContext {
    silo: Silo,
    runtime_id: Uuid,
    compromised: bool,
    mihomo_authentication: Option<MihomoControllerAuthentication>,
    mihomo_guard: Option<mihomo::MihomoRuntimeGuard>,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeNetworkFailure {
    Relay,
    Controller,
    Endpoint,
    Configuration,
    ExitEvidence,
    RuntimeEvidence,
    Credentials,
}

enum EngineRuntimeProtocol {
    Native {
        receiver: mpsc::Receiver<EngineProtocolEvent>,
        execution: EngineControlExecution,
    },
    CamoufoxHost(Box<CamoufoxHostRuntime>),
}

impl EngineRuntimeProtocol {
    #[cfg(test)]
    fn native_execution(&self) -> Option<&EngineControlExecution> {
        match self {
            Self::Native { execution, .. } => Some(execution),
            Self::CamoufoxHost(_) => None,
        }
    }
}

struct CamoufoxHostRuntime {
    transport: CamoufoxHostTransport,
    session_id: String,
    binding: CamoufoxHostLaunch,
    observed_website_digest: Option<String>,
    evidence_class: String,
    closed_confirmed: bool,
    #[cfg(test)]
    real_host_integration: bool,
    #[cfg(test)]
    launch_surface: Option<Value>,
}

enum EngineProtocolEvent {
    Ack(Result<EngineBootstrapAck, String>),
    Receipt(Result<EngineRuntimeReceiptFrame, String>),
}

struct SpawnedEngine {
    child: Child,
    bootstrap_ack: Option<EngineBootstrapAck>,
    runtime: Option<EngineRuntimeProtocol>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostResponse {
    id: Option<String>,
    ok: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<CamoufoxHostError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostError {
    code: String,
    message: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostHello {
    protocol: String,
    host_version: String,
    #[serde(default)]
    python_version: Option<String>,
    artifact_root: String,
    profile_root: String,
    state_root: String,
    max_frame_bytes: usize,
    probe_port_policy: String,
    browser_release: String,
    asset_sha256: String,
    tree_manifest: String,
    tree_manifest_sha256: String,
    platform: String,
    state: String,
    verified: bool,
    evidence_class: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostLaunchResult {
    session_id: String,
    state: String,
    artifact_id: String,
    profile_id: String,
    artifact_file_sha256: String,
    #[serde(default)]
    browser_proxy_server: Option<String>,
    #[serde(default)]
    configured_identity_digest: Option<String>,
    #[serde(default)]
    observed_website_digest: Option<String>,
    #[serde(default)]
    boot_count_before: Option<u64>,
    #[serde(default)]
    boot_count_after: Option<u64>,
    #[serde(default)]
    spawn_seconds: Option<f64>,
    #[serde(default)]
    probe_seconds: Option<f64>,
    #[serde(default)]
    font_mode: Option<String>,
    #[serde(default)]
    managed_pids: Option<Vec<u32>>,
    #[serde(default)]
    cookie_evidence: Option<Value>,
    #[serde(default)]
    probe_port: Option<u16>,
    #[serde(default)]
    verified: Option<bool>,
    #[serde(default)]
    evidence_class: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostStatusResult {
    state: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    artifact_id: Option<String>,
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    artifact_file_sha256: Option<String>,
    #[serde(default)]
    browser_proxy_server: Option<String>,
    #[serde(default)]
    configured_identity_digest: Option<String>,
    #[serde(default)]
    observed_website_digest: Option<String>,
    #[serde(default)]
    exit_status: Option<i32>,
    #[serde(default)]
    exit_file_observed: Option<bool>,
    #[serde(default)]
    quarantine: Option<Value>,
    #[serde(default)]
    failure: Option<String>,
    #[serde(default)]
    context_close: Option<Value>,
    #[serde(default)]
    close_outcome: Option<Value>,
    #[serde(default)]
    verified: Option<bool>,
    #[serde(default)]
    evidence_class: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostCloseResult {
    session_id: String,
    state: String,
    #[serde(default)]
    exit_status: Option<i32>,
    #[serde(default)]
    exit_file_observed: Option<bool>,
    #[serde(default)]
    process_tree_exit: Option<Value>,
    #[serde(default)]
    cookie_sqlite: Option<Value>,
    #[serde(default)]
    context_close: Option<Value>,
    #[serde(default)]
    close_outcome: Option<Value>,
    #[serde(default)]
    quarantine: Option<Value>,
    #[serde(default)]
    close_seconds: Option<f64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostSelfCheck {
    argv_matches: Vec<String>,
    stderr_log_matches: Vec<String>,
    patterns_checked: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CamoufoxHostShutdownResult {
    state: String,
    #[serde(default)]
    sessions_closed: Option<u64>,
    self_check: CamoufoxHostSelfCheck,
}

struct CamoufoxHostTransport {
    stdin: Option<ChildStdin>,
    receiver: mpsc::Receiver<Result<CamoufoxHostResponse, String>>,
    next_request_id: u64,
    #[cfg(test)]
    wire_snapshot: Vec<Vec<u8>>,
}

impl CamoufoxHostTransport {
    fn attach(child: &mut Child) -> Result<Self, LauncherError> {
        let stdin = child.stdin.take().ok_or_else(|| {
            LauncherError::Bootstrap(
                "Camoufox Host child did not expose its exact stdin pipe".to_owned(),
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            LauncherError::Bootstrap(
                "Camoufox Host child did not expose its exact stdout pipe".to_owned(),
            )
        })?;
        let (sender, receiver) = mpsc::sync_channel(32);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_camoufox_host_frame(&mut reader) {
                    Ok(Some(frame)) => {
                        let response = strict_json_from_slice::<CamoufoxHostResponse>(&frame)
                            .map_err(|error| error.to_string());
                        let terminal = response.is_err();
                        if sender.send(response).is_err() || terminal {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = sender.send(Err(
                            "Camoufox Host stdout reached EOF before a response".to_owned(),
                        ));
                        break;
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            stdin: Some(stdin),
            receiver,
            next_request_id: 1,
            #[cfg(test)]
            wire_snapshot: Vec::new(),
        })
    }

    fn request(
        &mut self,
        command: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LauncherError> {
        let request_id = format!("m3-{}", self.next_request_id);
        self.next_request_id = self.next_request_id.checked_add(1).ok_or_else(|| {
            LauncherError::RuntimeReceipt("Camoufox Host request ID exhausted its bound".to_owned())
        })?;
        let body = serde_json::to_vec(&json!({
            "id": request_id,
            "command": command,
            "params": params,
        }))
        .map_err(|error| LauncherError::RuntimeReceipt(error.to_string()))?;
        if body.is_empty() || body.len() > MAX_CAMOUFOX_HOST_FRAME_BYTES {
            return Err(LauncherError::RuntimeReceipt(
                "Camoufox Host request exceeded the 32 KiB frame bound".to_owned(),
            ));
        }
        #[cfg(test)]
        self.wire_snapshot.push(body.clone());
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            LauncherError::RuntimeReceipt(
                "Camoufox Host stdin is closed for this exact child".to_owned(),
            )
        })?;
        stdin
            .write_all(&body)
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|error| {
                LauncherError::RuntimeReceipt(format!("Host request write failed: {error}"))
            })?;
        let response = self
            .receiver
            .recv_timeout(timeout)
            .map_err(|error| {
                LauncherError::RuntimeReceipt(format!(
                    "Camoufox Host response timeout/EOF: {error}"
                ))
            })?
            .map_err(LauncherError::RuntimeReceipt)?;
        if response.id.as_deref() != Some(request_id.as_str()) {
            return Err(LauncherError::RuntimeReceipt(
                "Camoufox Host response ID did not match the outstanding request".to_owned(),
            ));
        }
        if response.ok {
            if response.error.is_some() || response.result.is_none() {
                return Err(LauncherError::RuntimeReceipt(
                    "Camoufox Host success response has an invalid result/error shape".to_owned(),
                ));
            }
            Ok(response.result.expect("checked Host result"))
        } else {
            if response.result.is_some() || response.error.is_none() {
                return Err(LauncherError::RuntimeReceipt(
                    "Camoufox Host error response has an invalid result/error shape".to_owned(),
                ));
            }
            let error = response.error.expect("checked Host error");
            Err(LauncherError::RuntimeReceipt(format!(
                "Camoufox Host rejected {command}: {} ({})",
                error.message, error.code
            )))
        }
    }

    #[cfg(test)]
    fn close_exact_stdin(&mut self) {
        self.stdin = None;
    }
}

fn read_camoufox_host_frame<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::with_capacity(MAX_CAMOUFOX_HOST_FRAME_BYTES);
    let mut byte = [0_u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) if frame.is_empty() => return Ok(None),
            Ok(0) => return Err("Camoufox Host emitted a partial frame before EOF".to_owned()),
            Ok(_) => {
                if byte[0] == b'\n' {
                    return Ok(Some(frame));
                }
                frame.push(byte[0]);
                if frame.len() > MAX_CAMOUFOX_HOST_FRAME_BYTES {
                    return Err("Camoufox Host emitted a frame over 32 KiB".to_owned());
                }
            }
            Err(error) => return Err(format!("Camoufox Host stdout read failed: {error}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeRecord {
    silo_id: Uuid,
    pid: u32,
    started_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    state: RuntimeState,
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
    #[error("浏览器启动前核验失败：{0}")]
    BrowserVerification(String),
    #[error("浏览器启动后未能证明受管 Profile 的独立所有权：{0}")]
    BrowserStartup(String),
    #[error("浏览器引擎适配器拒绝启动：{0}")]
    Engine(String),
    #[error("受控引擎启动协议写入失败：{0}")]
    Bootstrap(String),
    #[error("受控引擎运行证据协议失败：{0}")]
    RuntimeReceipt(String),
    #[error("无法启动所选浏览器：{0}")]
    Spawn(#[from] std::io::Error),
    #[error("无法启动禁止直连回退的本机代理中继：{0}")]
    ProxyRelay(String),
    #[error("无法绑定所选 Mihomo 节点：{0}")]
    Mihomo(String),
}

impl RuntimeManager {
    pub fn open(root: &Path) -> Self {
        let record_path = root
            .join(RUNTIME_RECORD_DIRECTORY)
            .join(RUNTIME_RECORD_FILE);
        let (record, read_error) = match read_runtime_record(&record_path) {
            Ok(record) => (record, None),
            Err(error) => (None, Some(error)),
        };
        let activation = read_error.map(|error| RuntimeActivation {
            active_silo_id: None,
            state: RuntimeState::Failed,
            updated_at: Utc::now(),
            message: Some(format!(
                "最小运行记录无法读取，已拒绝静默恢复：{error}。未操作任何浏览器进程或 Profile 锁。"
            )),
            browser_verification: None,
            engine_evidence: None,
            network_evidence: None,
        }).or_else(|| record.as_ref().map(|record| {
            let stopped = record.state == RuntimeState::Stopped;
            RuntimeActivation {
                active_silo_id: (!stopped).then_some(record.silo_id),
                state: if stopped {
                    RuntimeState::Stopped
                } else {
                    RuntimeState::RecoveryRequired
                },
                updated_at: Utc::now(),
                message: Some(if stopped {
                    "最近一次持久化运行记录显示浏览器会话已停止。".to_owned()
                } else {
                    "检测到上次桌面会话的最小运行记录；解锁 Vault 后将结合进程与 Profile 锁解释状态。VeriSilo 不会强杀浏览器或删除锁。"
                        .to_owned()
                }),
                browser_verification: None,
                engine_evidence: None,
                network_evidence: None,
            }
        }));
        Self {
            record_path: Some(record_path),
            record,
            activation,
            ..Self::default()
        }
    }

    fn camoufox_host_roots(&self) -> Option<CamoufoxHostRoots> {
        let app_root = self.record_path.as_ref()?.parent()?.parent()?;
        let managed_root = app_root.join("camoufox");
        Some(CamoufoxHostRoots {
            artifact_root: managed_root.join("artifacts"),
            profile_root: managed_root.join("profiles"),
            state_root: managed_root.join("state"),
        })
    }

    #[cfg(test)]
    fn set_test_engine_adapter(&mut self, adapter: Box<dyn EngineAdapter>) {
        self.test_engine_adapter = Some(adapter);
    }

    /// Stop only a Camoufox Host child owned by this RuntimeManager. The Host
    /// close acknowledgement and the exact child exit are both required before
    /// releasing the Profile lease or publishing `stopped`.
    pub fn stop_managed_camoufox(
        &mut self,
        silo_id: Uuid,
    ) -> Result<RuntimeActivation, LauncherError> {
        self.refresh();
        if self
            .activation
            .as_ref()
            .and_then(|activation| activation.active_silo_id)
            != Some(silo_id)
        {
            return Err(LauncherError::InvalidNetwork(
                "the requested Silo is not the active local runtime".to_owned(),
            ));
        }

        let response_timeout = self
            .engine_runtime
            .as_ref()
            .and_then(|runtime| match runtime {
                EngineRuntimeProtocol::CamoufoxHost(host) => {
                    Some(camoufox_host_runtime_timeout(host))
                }
                EngineRuntimeProtocol::Native { .. } => None,
            })
            .unwrap_or(ENGINE_INITIAL_RECEIPT_TIMEOUT);

        let host_result = (|| -> Result<(), LauncherError> {
            let Some(EngineRuntimeProtocol::CamoufoxHost(host)) = self.engine_runtime.as_mut()
            else {
                return Err(LauncherError::RuntimeReceipt(
                    "the active Silo is not backed by a Camoufox Host session".to_owned(),
                ));
            };
            let session_id = host.session_id.clone();
            let binding = host.binding.clone();
            let close_value = host.transport.request(
                "close",
                json!({ "sessionId": session_id }),
                response_timeout,
            )?;
            let close: CamoufoxHostCloseResult =
                serde_json::from_value(close_value).map_err(|error| {
                    LauncherError::RuntimeReceipt(format!(
                        "invalid Camoufox Host close response: {error}"
                    ))
                })?;
            validate_camoufox_host_close(&close, &binding, &session_id)?;

            let shutdown_value = host
                .transport
                .request("shutdown", json!({}), response_timeout)?;
            let shutdown: CamoufoxHostShutdownResult = serde_json::from_value(shutdown_value)
                .map_err(|error| {
                    LauncherError::RuntimeReceipt(format!(
                        "invalid Camoufox Host shutdown response: {error}"
                    ))
                })?;
            validate_camoufox_host_shutdown(&shutdown)?;
            host.closed_confirmed = true;
            Ok(())
        })();
        if let Err(error) = host_result {
            self.mark_camoufox_host_failure(error.to_string(), true);
            return Err(error);
        }

        let deadline = Instant::now() + response_timeout;
        let exit_result = (|| -> Result<std::process::ExitStatus, LauncherError> {
            loop {
                let status = self
                    .child
                    .as_mut()
                    .ok_or_else(|| {
                        LauncherError::RuntimeReceipt(
                            "Camoufox Host child handle disappeared before exact exit confirmation"
                                .to_owned(),
                        )
                    })?
                    .try_wait()
                    .map_err(|error| LauncherError::RuntimeReceipt(error.to_string()))?;
                if let Some(status) = status {
                    break Ok(status);
                }
                if Instant::now() >= deadline {
                    break Err(LauncherError::RuntimeReceipt(
                        "Camoufox Host close acknowledged but the exact Host child did not exit within the bounded wait"
                            .to_owned(),
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
        })();
        let exit_status = match exit_result {
            Ok(status) => status,
            Err(error) => {
                self.mark_camoufox_host_failure(error.to_string(), true);
                return Err(error);
            }
        };
        if !exit_status.success() {
            let error = LauncherError::RuntimeReceipt(format!(
                "Camoufox Host exact child exited unsuccessfully: {exit_status}"
            ));
            self.mark_camoufox_host_failure(error.to_string(), true);
            return Err(error);
        }

        let runtime_binding = self
            .health_context
            .as_ref()
            .map(|context| (context.silo.id, context.runtime_id));
        if let Some((runtime_silo_id, runtime_id)) = runtime_binding {
            self.shutdown_relay_for_runtime(runtime_silo_id, runtime_id);
        }
        self.child = None;
        self.engine_runtime = None;
        self.profile_lease = None;
        self.health_context = None;
        let mut activation = self
            .activation
            .clone()
            .unwrap_or_else(RuntimeActivation::idle);
        activation.active_silo_id = None;
        activation.state = RuntimeState::Stopped;
        activation.updated_at = Utc::now();
        activation.message = Some(
            "Camoufox Host close, shutdown, process-tree exit, and exact child wait were confirmed; Profile ownership was released."
                .to_owned(),
        );
        if let Some(evidence) = activation.engine_evidence.as_mut() {
            evidence.host_launch = RuntimeEvidenceState::Observed;
            evidence.bootstrap_delivery = RuntimeEvidenceState::NotApplicable;
            evidence.runtime_receipts = RuntimeEvidenceState::NotApplicable;
            evidence.restore_receipt = RuntimeEvidenceState::NotApplicable;
            evidence.verified_adapter = None;
        }
        self.activation = Some(activation.clone());
        self.persist_current_record(RuntimeState::Stopped);
        Ok(activation)
    }

    fn mark_camoufox_host_failure(&mut self, message: String, persist_runtime_record: bool) {
        if let Some(activation) = self.activation.as_mut() {
            activation.state = RuntimeState::VerificationFailed;
            activation.updated_at = Utc::now();
            activation.message = Some(message.clone());
            if let Some(evidence) = activation.engine_evidence.as_mut() {
                evidence.host_launch = RuntimeEvidenceState::Failed;
                evidence.bootstrap_delivery = RuntimeEvidenceState::NotApplicable;
                evidence.runtime_receipts = RuntimeEvidenceState::NotApplicable;
                evidence.restore_receipt = RuntimeEvidenceState::NotApplicable;
                evidence.verified_adapter = None;
            }
        }
        let network_path_must_close = self.health_context.as_ref().is_some_and(|context| {
            context.silo.network_profile.requires_proxy()
                || expects_managed_relay(&context.silo.network_profile)
        });
        if network_path_must_close {
            self.fail_closed_network_path_with_persistence(
                format!("Camoufox Host runtime evidence failed: {message}"),
                RuntimeNetworkFailure::RuntimeEvidence,
                persist_runtime_record,
            );
        } else if persist_runtime_record {
            self.persist_current_record(RuntimeState::VerificationFailed);
        }
    }

    pub fn recorded_silo_id(&self) -> Option<Uuid> {
        self.record.as_ref().map(|record| record.silo_id)
    }

    pub fn needs_reconciliation(&self) -> bool {
        self.child.is_none()
            && self
                .record
                .as_ref()
                .is_some_and(|record| record.state != RuntimeState::Stopped)
    }

    fn stock_profile_release_pending(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.pending_stock_profile_release.is_some()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    /// Reconciles a previous desktop session without terminating a process or
    /// deleting a browser-owned Profile lock. PID alone is never treated as
    /// proof: a live process and Profile lock are considered together.
    pub fn reconcile_persisted(
        &mut self,
        silo: &Silo,
        mihomo_authentication: Option<MihomoControllerAuthentication>,
    ) -> RuntimeActivation {
        let Some(record) = self.record.as_ref() else {
            return self.activation();
        };
        if record.silo_id != silo.id {
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some(
                    "运行记录引用的 Silo 已不存在；记录已保留供审计，未操作任何进程或锁。"
                        .to_owned(),
                ),
                browser_verification: None,
                engine_evidence: None,
                network_evidence: None,
            });
            self.persist_current_record(RuntimeState::Failed);
            return self.activation.clone().expect("activation was set");
        }

        let configured_adapter = silo.engine.adapter_id(&silo.browser.kind);
        let process_alive = process_is_alive(record.pid);
        let profile_locked = profile_in_use(&silo.engine_profile_directory());
        let mut evidence =
            configured_network_evidence(&silo.network_profile, false, configured_adapter);
        let externally_packaged = !silo.engine.is_stock();
        let mut engine_evidence =
            RuntimeEngineEvidence::configured(configured_adapter, externally_packaged);
        let (mut state, active_silo_id, mut message) = match (process_alive, profile_locked) {
            (false, false) => (
                RuntimeState::Stopped,
                None,
                "上次记录的浏览器进程和 Profile 锁均已消失；会话已解释为停止。".to_owned(),
            ),
            (true, true) if expects_managed_relay(&silo.network_profile) => {
                evidence.browser_routing = RuntimeEvidenceState::Failed;
                evidence.endpoint = RuntimeEvidenceState::Failed;
                (
                    RuntimeState::VerificationFailed,
                    Some(silo.id),
                    "浏览器及 Profile 锁仍存在，但桌面重启已使受管 loopback relay 不可恢复；代理仍为 fail-closed，不会切换 DIRECT。请自行关闭浏览器后重新启动 Silo。VeriSilo 不会强制关闭它。"
                        .to_owned(),
                )
            }
            (true, true) => (
                RuntimeState::Running,
                Some(silo.id),
                "已结合存活 PID 与 Profile 锁恢复会话解释；该浏览器不是当前进程的子进程，VeriSilo 不声称能强制关闭它。"
                    .to_owned(),
            ),
            (true, false) => (
                RuntimeState::RecoveryRequired,
                Some(silo.id),
                "记录的 PID 仍存在但 Profile 锁缺失，可能是 PID 复用或浏览器尚未建立锁；状态需要用户重新检查，未操作进程。"
                    .to_owned(),
            ),
            (false, true) => (
                RuntimeState::RecoveryRequired,
                Some(silo.id),
                "记录的 PID 已消失但 Profile 锁仍存在；可能有浏览器接管了 Profile。VeriSilo 不会删除锁或强杀进程。"
                    .to_owned(),
            ),
        };
        if process_alive && profile_locked && !expects_managed_relay(&silo.network_profile) {
            if let Err(error) = preflight_proxy(&silo.network_profile, None, &mut evidence) {
                if silo.network_profile.requires_proxy() {
                    state = RuntimeState::VerificationFailed;
                    evidence.endpoint = RuntimeEvidenceState::Failed;
                    evidence.browser_routing = RuntimeEvidenceState::Failed;
                    message = format!(
                        "恢复检查无法回读 required proxy：{error}。浏览器配置不会改为 DIRECT；VeriSilo 不会强制关闭浏览器。"
                    );
                }
            }
        }
        if process_alive && profile_locked {
            engine_evidence.launched_adapter = Some(configured_adapter);
            if externally_packaged && configured_adapter != crate::engine::EngineAdapterId::Camoufox
            {
                engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Unavailable;
                engine_evidence.runtime_receipts = RuntimeEvidenceState::Unavailable;
                engine_evidence.restore_receipt = RuntimeEvidenceState::Unavailable;
            } else if configured_adapter == crate::engine::EngineAdapterId::Camoufox {
                engine_evidence.host_launch = RuntimeEvidenceState::Unavailable;
            }
            if state == RuntimeState::Running {
                if let Some(binding) = silo.network_profile.external_mihomo_binding() {
                    match mihomo::verify_binding(binding, mihomo_authentication.as_ref()) {
                        Ok(()) => evidence.controller_binding = RuntimeEvidenceState::Verified,
                        Err(error) => {
                            state = RuntimeState::VerificationFailed;
                            evidence.controller_binding = RuntimeEvidenceState::Failed;
                            message = format!(
                            "恢复时无法回读外部 Mihomo 绑定：{error}。required proxy 保持 fail-closed，不会切换 DIRECT 或后台轮换节点。"
                        );
                        }
                    }
                }
            }
        }
        if externally_packaged {
            match production_engine_adapter(&silo.engine, silo.browser.clone()) {
                Ok(adapter) if adapter.health().state == EngineHealthState::Healthy => {
                    engine_evidence.package_verification = RuntimeEvidenceState::Verified;
                }
                Ok(adapter) => {
                    engine_evidence.package_verification = RuntimeEvidenceState::Failed;
                    if process_alive && profile_locked {
                        state = RuntimeState::VerificationFailed;
                        message = format!(
                            "恢复时外部引擎包重新验证失败：{}。不会回退 stock 或操作该进程。",
                            adapter.health().message
                        );
                    }
                }
                Err(error) => {
                    engine_evidence.package_verification = RuntimeEvidenceState::Failed;
                    if process_alive && profile_locked {
                        state = RuntimeState::VerificationFailed;
                        message = format!(
                            "恢复时无法加载外部引擎适配器：{error}。不会回退 stock 或操作该进程。"
                        );
                    }
                }
            }
        }
        let runtime_id = evidence.runtime_id;
        let compromised = state == RuntimeState::VerificationFailed;
        if compromised {
            invalidate_network_evidence(
                &mut evidence,
                RuntimeNetworkFailure::RuntimeEvidence,
                Utc::now(),
            );
        }
        let mihomo_authentication = if compromised {
            None
        } else {
            mihomo_authentication
        };
        self.health_context = Some(RuntimeHealthContext {
            silo: silo.clone(),
            runtime_id,
            compromised,
            mihomo_authentication,
            mihomo_guard: None,
        });
        self.activation = Some(RuntimeActivation {
            active_silo_id,
            state: state.clone(),
            updated_at: Utc::now(),
            message: Some(message),
            browser_verification: silo
                .engine
                .is_stock()
                .then(|| verify_browser_descriptor(&silo.browser)),
            engine_evidence: Some(engine_evidence),
            network_evidence: Some(evidence),
        });
        self.persist_current_record(state);
        self.activation.clone().expect("activation was set")
    }

    pub fn activation(&mut self) -> RuntimeActivation {
        self.refresh();
        self.activation
            .clone()
            .unwrap_or_else(RuntimeActivation::idle)
    }

    pub(crate) fn prepare_for_vault_restore(&mut self) -> Option<VaultRestoreRuntimePreparation> {
        let activation = self.activation();
        (runtime_allows_vault_restore(&activation)
            && self.child.is_none()
            && self.proxy_relay.is_none()
            && self.health_context.is_none()
            && self.engine_runtime.is_none()
            && self.profile_lease.is_none()
            && !self.needs_reconciliation())
        .then_some(VaultRestoreRuntimePreparation { _private: () })
    }

    pub(crate) fn complete_successful_vault_restore(
        &mut self,
        _preparation: VaultRestoreRuntimePreparation,
    ) -> RuntimeActivation {
        // The preparation token is issued only while this mutex-owned runtime
        // is proven quiescent. A successful Vault replacement starts a new
        // ownership epoch, so no activation/evidence from the prior Vault may
        // remain reachable or be republished.
        self.proxy_relay = None;
        self.health_context = None;
        self.engine_runtime = None;
        self.profile_lease = None;
        #[cfg(target_os = "windows")]
        {
            self.pending_stock_profile_release = None;
        }
        self.record = None;
        if let Some(path) = self.record_path.as_ref() {
            // The Vault is already committed. Stale-record cleanup is
            // best-effort and must never roll the new Vault back.
            let _ = fs::remove_file(path);
        }
        let activation = RuntimeActivation::idle();
        self.activation = Some(activation.clone());
        activation
    }

    pub(crate) fn activation_for_watchdog(
        &mut self,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> RuntimeActivation {
        self.refresh_until(deadline, cancelled, false);
        self.activation
            .clone()
            .unwrap_or_else(RuntimeActivation::idle)
    }

    /// Revokes every decrypted runtime credential when the Vault locks. A
    /// browser that depended on the managed relay remains alive, but its
    /// loopback proxy disappears and the session becomes verification_failed;
    /// VeriSilo never rewrites the browser to DIRECT or force-kills it.
    pub fn revoke_secrets_for_vault_lock(&mut self) -> RuntimeActivation {
        self.refresh();
        let should_revoke = self.proxy_relay.is_some()
            || self
                .health_context
                .as_ref()
                .and_then(|context| context.mihomo_authentication.as_ref())
                .is_some();
        if should_revoke {
            self.fail_closed_network_path(
                "保险库已锁定，运行时代理凭据或 Mihomo Controller Secret 不再可用".to_owned(),
                RuntimeNetworkFailure::Credentials,
            );
        }
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

    /// Applies only the facts the Companion asserts it observed. A successful
    /// public IP request is recorded as a time-bounded observation, never as
    /// authenticated process or route verification. Public DoH answer
    /// comparison is retained in encrypted history, but it cannot verify the
    /// browser/OS resolver path; WebRTC and QUIC remain unobserved.
    pub fn apply_network_evidence(
        &mut self,
        entry: &NativeNetworkEvidenceInboxEntry,
    ) -> RuntimeActivation {
        self.refresh();
        let now = Utc::now();
        let Some(current_activation) = self.activation.as_ref() else {
            return RuntimeActivation::idle();
        };
        let current_runtime_matches = current_activation
            .network_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.runtime_id == entry.runtime_id);
        if current_activation.active_silo_id != Some(entry.silo_id)
            || !matches!(&current_activation.state, RuntimeState::Running)
            || !current_runtime_matches
            || validate_network_evidence_inbox_entry(entry).is_err()
            || entry.expires_at <= now
            || entry.received_at > now + ChronoDuration::seconds(EVIDENCE_CLOCK_SKEW_SECONDS)
        {
            return current_activation.clone();
        }
        let window_scoped_http_authentication = current_activation
            .network_evidence
            .as_ref()
            .is_some_and(|evidence| {
                self.health_context.as_ref().is_some_and(|context| {
                    context.silo.id == entry.silo_id
                        && context.runtime_id == entry.runtime_id
                        && is_window_scoped_http_authentication(
                            &context.silo.network_profile,
                            evidence,
                        )
                })
            });
        let relay_authentication =
            self.proxy_relay
                .as_ref()
                .map_or(RelayAuthenticationEvidence::None, |relay| {
                    relay.authentication_evidence(
                        entry.silo_id,
                        entry.runtime_id,
                        entry.result.checked_at
                            - ChronoDuration::seconds(HTTP_AUTH_EVIDENCE_LOOKBACK_SECONDS),
                        entry.result.checked_at,
                    )
                });
        let has_public_ip_observation = network_evidence_has_public_ip_observation(entry);
        let activation = self
            .activation
            .as_mut()
            .expect("current activation was checked above");
        if let Some(network_evidence) = activation.network_evidence.as_mut() {
            network_evidence.evidence_id = entry.evidence_id;
            network_evidence.observed_at = entry.result.checked_at;
            network_evidence.expires_at = Some(entry.expires_at);
            network_evidence.provenance = RuntimeNetworkEvidenceProvenance::ExtensionAsserted;
            network_evidence.exit = asserted_exit_state(has_public_ip_observation);
            network_evidence.dns = RuntimeEvidenceState::Unavailable;
            network_evidence.web_rtc = RuntimeEvidenceState::Unavailable;
            if window_scoped_http_authentication {
                reset_http_authentication_window(network_evidence);
                match relay_authentication {
                    RelayAuthenticationEvidence::Accepted if has_public_ip_observation => {
                        network_evidence.authentication = RuntimeEvidenceState::Verified;
                        network_evidence.authentication_provenance =
                            RuntimeNetworkEvidenceProvenance::RelayObserved;
                    }
                    RelayAuthenticationEvidence::Rejected => {
                        network_evidence.authentication = RuntimeEvidenceState::Failed;
                        network_evidence.authentication_provenance =
                            RuntimeNetworkEvidenceProvenance::RelayObserved;
                    }
                    RelayAuthenticationEvidence::Accepted | RelayAuthenticationEvidence::None => {}
                }
            }
        }
        activation.updated_at = Utc::now();
        activation.message = Some(match relay_authentication {
            RelayAuthenticationEvidence::Accepted if has_public_ip_observation =>
                "Companion 以 extension_asserted 声明观测到本次 Silo 的公网出口；同一 runtime 的受管 relay 在该检查窗口内另行记录了 HTTP Basic challenge、带凭据上游接受和已转发字节，因此代理认证记为 relay_observed / verified。两者是联合证据，不是独立可信的浏览器进程证明；出口仍只记 observed。".to_owned(),
            RelayAuthenticationEvidence::Rejected =>
                "同一 runtime 的受管 relay 在该检查窗口内观测到上游 HTTP 407，代理认证已记为 relay_observed / failed。Companion 回传仍只是 extension_asserted，不能独立证明浏览器进程或出口。".to_owned(),
            _ if has_public_ip_observation =>
                "Companion 扩展声明观测到本次 Silo 的公网出口；Native inbox 未做本机进程级认证，因此仅记为 extension_asserted / observed。没有同一 runtime、同一检查窗口内的 relay 认证收据时，HTTP 凭据状态不会升级。公共 DoH 只比较答案；实际 DNS 路径、WebRTC 与 QUIC 仍未观测。".to_owned(),
            _ =>
                "Companion 扩展回传检查失败声明；本次没有取得公网出口观测，也没有可联合使用的 HTTP 认证成功收据。实际 DNS 路径、WebRTC 与 QUIC 仍未观测。".to_owned(),
        });
        let terminal_failure = !has_public_ip_observation
            || relay_authentication == RelayAuthenticationEvidence::Rejected;
        if terminal_failure {
            let reason = if relay_authentication == RelayAuthenticationEvidence::Rejected {
                "同一 runtime 的受管 relay 观测到代理认证被拒绝"
            } else {
                "本次 Silo 出口观测失败"
            };
            self.fail_closed_network_path(
                format!("{reason}；旧的网络证据已失效"),
                RuntimeNetworkFailure::ExitEvidence,
            );
        }
        self.activation
            .clone()
            .unwrap_or_else(RuntimeActivation::idle)
    }

    pub fn launch(
        &mut self,
        silo: &Silo,
        managed_profile_directories: &[PathBuf],
        proxy_authentication: Option<ProxyAuthentication>,
        mihomo_authentication: Option<MihomoControllerAuthentication>,
    ) -> Result<RuntimeActivation, LauncherError> {
        self.launch_with_identity_deriver(
            silo,
            managed_profile_directories,
            proxy_authentication,
            mihomo_authentication,
            None,
        )
    }

    pub fn launch_with_identity_deriver(
        &mut self,
        silo: &Silo,
        managed_profile_directories: &[PathBuf],
        proxy_authentication: Option<ProxyAuthentication>,
        mihomo_authentication: Option<MihomoControllerAuthentication>,
        identity_deriver: Option<&dyn IdentityTokenDeriver>,
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

        let configured_adapter = silo.engine.adapter_id(&silo.browser.kind);
        let externally_packaged = !silo.engine.is_stock();
        let mut engine_evidence =
            RuntimeEngineEvidence::configured(configured_adapter, externally_packaged);

        if let Err(error) = silo.validate_engine() {
            let error = LauncherError::Engine(error.to_string());
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: None,
                engine_evidence: Some(engine_evidence),
                network_evidence: None,
            });
            return Err(error);
        }

        let browser_verification = if silo.engine.is_stock() {
            let verification = verify_browser_descriptor(&silo.browser);
            if verification.state != BrowserVerificationState::Verified {
                let error = LauncherError::BrowserVerification(verification.message.clone());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::VerificationFailed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: Some(verification),
                    engine_evidence: Some(engine_evidence),
                    network_evidence: None,
                });
                return Err(error);
            }
            Some(verification)
        } else {
            // External packages are reverified by their production adapter on
            // every plan. Running stock Authenticode/--version probes against
            // an unrelated fallback browser would be both misleading and a
            // source of accidental stock fallback.
            None
        };

        if let Err(error) = silo.network_profile.validate() {
            let error = LauncherError::InvalidNetwork(error.to_string());
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence.clone()),
                network_evidence: Some(configured_network_evidence(
                    &silo.network_profile,
                    proxy_authentication.is_some(),
                    configured_adapter,
                )),
            });
            return Err(error);
        }

        let mut network_evidence = configured_network_evidence(
            &silo.network_profile,
            proxy_authentication.is_some(),
            configured_adapter,
        );
        if configured_adapter == crate::engine::EngineAdapterId::Camoufox
            && !matches!(
                &silo.network_profile,
                NetworkProfile::Direct {
                    proxy_required: false
                } | NetworkProfile::FixedProxy {
                    proxy_required: true,
                    scheme: crate::domain::ProxyScheme::Http | crate::domain::ProxyScheme::Socks5,
                    ..
                }
            )
        {
            let error = LauncherError::InvalidNetwork(
                "Camoufox Host v1 only permits Direct(false) or required FixedProxy HTTP/SOCKS5 profiles"
                    .to_owned(),
            );
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Failed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence.clone()),
                network_evidence: Some(network_evidence),
            });
            return Err(error);
        }
        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Preflight,
            updated_at: Utc::now(),
            message: Some("正在检查浏览器目录、代理端点和本次网络绑定。".to_owned()),
            browser_verification: browser_verification.clone(),
            engine_evidence: Some(engine_evidence.clone()),
            network_evidence: Some(network_evidence.clone()),
        });

        let mut runtime_profile_directories = managed_profile_directories.to_vec();
        if configured_adapter == crate::engine::EngineAdapterId::Camoufox {
            if let Some(roots) = self.camoufox_host_roots() {
                runtime_profile_directories.push(
                    roots
                        .profile_root
                        .join(format!("silo-{}", silo.id.simple())),
                );
            }
        }
        let profile_lease = match BrowserProfileLease::acquire_for_runtime(
            &runtime_profile_directories,
            Path::new(&silo.profile_directory),
        ) {
            Ok(lease) => lease,
            Err(_) => {
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some("另一个受管 Silo 的浏览器目录正在使用中。".to_owned()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence.clone()),
                    network_evidence: Some(network_evidence),
                });
                return Err(LauncherError::ProfileInUse);
            }
        };

        // Resolve, reverify, derive, and serialize the immutable adapter plan
        // before touching Mihomo or starting a relay. A controlled failure is
        // terminal; it never falls back to stock.
        let mut production_adapter = || {
            production_engine_adapter(&silo.engine, silo.browser.clone()).map_err(|error| {
                let error = LauncherError::Engine(error.to_string());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::VerificationFailed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence.clone()),
                    network_evidence: Some(network_evidence.clone()),
                });
                error
            })
        };
        #[cfg(test)]
        let adapter = match self.test_engine_adapter.take() {
            Some(adapter) => adapter,
            None => production_adapter()?,
        };
        #[cfg(not(test))]
        let adapter = production_adapter()?;
        let session_id = Uuid::new_v4();
        let issued_at = Utc::now();
        let mut engine_request = EngineLaunchRequest {
            silo_id: Some(silo.id),
            session_id,
            profile_directory: silo.engine_profile_directory(),
            network_profile: silo.network_profile.clone(),
            identity: silo.engine.identity_template().cloned(),
            derived_token: None,
            fallback_rules: silo.engine.fallback_rules().to_vec(),
            camoufox_artifact_binding: silo.engine.camoufox_artifact_binding().cloned(),
            camoufox_roots: self.camoufox_host_roots(),
        };
        if configured_adapter != crate::engine::EngineAdapterId::Camoufox {
            if let Some(identity) = engine_request.identity.as_ref() {
                let deriver = identity_deriver.ok_or_else(|| {
                    let error = LauncherError::Engine(
                        "controlled engine launch requires an unlocked Vault seed deriver"
                            .to_owned(),
                    );
                    self.activation = Some(RuntimeActivation {
                        active_silo_id: None,
                        state: RuntimeState::VerificationFailed,
                        updated_at: Utc::now(),
                        message: Some(error.to_string()),
                        browser_verification: browser_verification.clone(),
                        engine_evidence: Some(engine_evidence.clone()),
                        network_evidence: Some(network_evidence.clone()),
                    });
                    error
                })?;
                let context = IdentityDerivationContext {
                    silo_id: silo.id,
                    seed_reference: silo.seed_reference,
                    template_id: identity.template_id,
                    session_id,
                    issued_at,
                    expires_at: issued_at
                        + ChronoDuration::minutes(DEFAULT_SESSION_TOKEN_LIFETIME_MINUTES),
                };
                engine_request.derived_token = Some(
                    adapter
                        .derive_identity_token(&context, deriver)
                        .map_err(|error| {
                            let error = LauncherError::Engine(error.to_string());
                            self.activation = Some(RuntimeActivation {
                                active_silo_id: None,
                                state: RuntimeState::VerificationFailed,
                                updated_at: Utc::now(),
                                message: Some(error.to_string()),
                                browser_verification: browser_verification.clone(),
                                engine_evidence: Some(engine_evidence.clone()),
                                network_evidence: Some(network_evidence.clone()),
                            });
                            error
                        })?,
                );
            }
        }
        let mut engine_plan = adapter.launch_plan(&engine_request).map_err(|error| {
            let error = LauncherError::Engine(error.to_string());
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::VerificationFailed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence.clone()),
                network_evidence: Some(network_evidence.clone()),
            });
            error
        })?;
        if engine_plan.package_verification.is_some() {
            engine_evidence.package_verification = RuntimeEvidenceState::Verified;
        }
        let bootstrap = (|| -> Result<Option<EngineBootstrapEnvelope>, LauncherError> {
            if engine_plan.identity_delivery.is_none() {
                return Ok(None);
            }
            let identity = engine_request.identity.take().ok_or_else(|| {
                LauncherError::Engine(
                    "controlled plan lost its constrained identity template".to_owned(),
                )
            })?;
            let token = engine_request.derived_token.take().ok_or_else(|| {
                LauncherError::Engine(
                    "controlled plan lost its short-lived identity token".to_owned(),
                )
            })?;
            EngineBootstrapEnvelope::for_launch(silo.id, issued_at, &engine_plan, identity, token)
                .map(Some)
                .map_err(|error| LauncherError::Engine(error.to_string()))
        })()
        .inspect_err(|error| {
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::VerificationFailed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence.clone()),
                network_evidence: Some(network_evidence.clone()),
            });
        })?;

        let mut mihomo_guard = None;
        if let Some(binding) = silo.network_profile.external_mihomo_binding() {
            if let Err(error) = mihomo::apply_binding(binding, mihomo_authentication.as_ref()) {
                network_evidence.controller_binding = RuntimeEvidenceState::Failed;
                let error = LauncherError::Mihomo(error.to_string());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence.clone()),
                    network_evidence: Some(network_evidence),
                });
                return Err(error);
            }
            let NetworkProfile::FixedProxy { host, port, .. } = &silo.network_profile else {
                unreachable!("validated external Mihomo binding uses a fixed proxy")
            };
            match mihomo::capture_runtime_guard(
                binding,
                host,
                *port,
                mihomo_authentication.as_ref(),
            ) {
                Ok(guard) => mihomo_guard = Some(guard),
                Err(error) => {
                    network_evidence.configuration = RuntimeEvidenceState::Failed;
                    network_evidence.controller_binding = RuntimeEvidenceState::Failed;
                    let error = LauncherError::Mihomo(error.to_string());
                    self.activation = Some(RuntimeActivation {
                        active_silo_id: None,
                        state: RuntimeState::Failed,
                        updated_at: Utc::now(),
                        message: Some(error.to_string()),
                        browser_verification: browser_verification.clone(),
                        engine_evidence: Some(engine_evidence.clone()),
                        network_evidence: Some(network_evidence),
                    });
                    return Err(error);
                }
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
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence.clone()),
                network_evidence: Some(network_evidence),
            });
            return Err(error);
        }

        let use_proxy_relay = ProxyRelay::supports(&silo.network_profile)
            && (silo.network_profile.requires_proxy()
                || proxy_authentication.is_some()
                || silo.network_profile.external_mihomo_binding().is_some());
        let proxy_relay = use_proxy_relay
            .then(|| {
                ProxyRelay::start(
                    &silo.network_profile,
                    silo.id,
                    network_evidence.runtime_id,
                    proxy_authentication,
                )
            })
            .transpose()
            .map_err(|error| {
                network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                let launcher_error = LauncherError::ProxyRelay(error.to_string());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(launcher_error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence.clone()),
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

        if configured_adapter == crate::engine::EngineAdapterId::Camoufox
            && matches!(
                &silo.network_profile,
                NetworkProfile::FixedProxy {
                    proxy_required: true,
                    ..
                }
            )
        {
            if let Err(error) = bind_camoufox_host_proxy(
                &mut engine_plan,
                &silo.network_profile,
                proxy_relay.as_ref(),
            ) {
                network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::VerificationFailed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence.clone()),
                    network_evidence: Some(network_evidence),
                });
                return Err(error);
            }
        }

        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Launching,
            updated_at: Utc::now(),
            message: Some("正在用独立数据目录和已检查的网络路径启动浏览器。".to_owned()),
            browser_verification: browser_verification.clone(),
            engine_evidence: Some(engine_evidence.clone()),
            network_evidence: Some(network_evidence.clone()),
        });

        let launch_arguments = if engine_plan.transport == EngineTransport::CamoufoxHostJsonlV1 {
            engine_plan
                .arguments
                .iter()
                .cloned()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        } else {
            let proxy_override = proxy_relay
                .as_ref()
                .map(|relay| (relay.endpoint().host.as_str(), relay.endpoint().port));
            let mut arguments = engine_plan
                .arguments
                .iter()
                .cloned()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>();
            arguments.extend(
                silo.network_profile
                    .launch_arguments_with_proxy_override(proxy_override),
            );
            arguments
        };
        let spawned = match spawn_engine_child(&engine_plan, &launch_arguments, bootstrap.as_ref())
        {
            Ok(spawned) => spawned,
            Err(error) => {
                let protocol_failure = matches!(
                    &error,
                    LauncherError::Bootstrap(_) | LauncherError::RuntimeReceipt(_)
                );
                if engine_plan.transport == EngineTransport::CamoufoxHostJsonlV1 {
                    engine_evidence.launched_adapter = Some(configured_adapter);
                    engine_evidence.host_launch = RuntimeEvidenceState::Failed;
                    engine_evidence.bootstrap_delivery = RuntimeEvidenceState::NotApplicable;
                    engine_evidence.runtime_receipts = RuntimeEvidenceState::NotApplicable;
                    engine_evidence.restore_receipt = RuntimeEvidenceState::NotApplicable;
                } else {
                    match &error {
                        LauncherError::Bootstrap(_) => {
                            engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Failed;
                            engine_evidence.runtime_receipts = RuntimeEvidenceState::NotRequested;
                        }
                        LauncherError::RuntimeReceipt(_) => {
                            engine_evidence.launched_adapter = Some(configured_adapter);
                            engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Verified;
                            engine_evidence.runtime_receipts = RuntimeEvidenceState::Failed;
                            engine_evidence.restore_receipt = RuntimeEvidenceState::Unavailable;
                        }
                        _ => {}
                    }
                }
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: if protocol_failure {
                        RuntimeState::VerificationFailed
                    } else {
                        RuntimeState::Failed
                    },
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence),
                    network_evidence: Some({
                        network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                        network_evidence
                    }),
                });
                return Err(error);
            }
        };
        drop(bootstrap);

        let SpawnedEngine {
            mut child,
            bootstrap_ack,
            runtime,
        } = spawned;

        #[cfg(target_os = "windows")]
        if silo.engine.is_stock() {
            if let Err(error) = verify_stock_browser_profile_ownership(
                &mut child,
                &silo.engine_profile_directory(),
                &engine_plan.executable_path,
            ) {
                network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::Failed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence),
                    network_evidence: Some(network_evidence),
                });
                return Err(error);
            }
        }

        let uses_native_bootstrap = engine_plan.transport == EngineTransport::NativeBootstrapV1;
        let uses_camoufox_host = engine_plan.transport == EngineTransport::CamoufoxHostJsonlV1;
        if uses_native_bootstrap && bootstrap_ack.is_none() {
            terminate_just_spawned_child(&mut child);
            engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Failed;
            network_evidence.browser_routing = RuntimeEvidenceState::Failed;
            let error = LauncherError::Bootstrap(
                "controlled engine launch returned without a bound ACK".to_owned(),
            );
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::VerificationFailed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence),
                network_evidence: Some(network_evidence),
            });
            return Err(error);
        }
        if uses_native_bootstrap && runtime.is_none() {
            terminate_just_spawned_child(&mut child);
            engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Verified;
            engine_evidence.runtime_receipts = RuntimeEvidenceState::Failed;
            engine_evidence.restore_receipt = RuntimeEvidenceState::Unavailable;
            network_evidence.browser_routing = RuntimeEvidenceState::Failed;
            let error = LauncherError::RuntimeReceipt(
                "controlled engine launch returned without a runtime receipt stream".to_owned(),
            );
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::VerificationFailed,
                updated_at: Utc::now(),
                message: Some(error.to_string()),
                browser_verification: browser_verification.clone(),
                engine_evidence: Some(engine_evidence),
                network_evidence: Some(network_evidence),
            });
            return Err(error);
        }

        let pid = child.id();
        engine_evidence.launched_adapter = Some(configured_adapter);
        if uses_native_bootstrap {
            engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Verified;
            let EngineRuntimeProtocol::Native { execution, .. } =
                runtime.as_ref().expect("checked controlled runtime stream")
            else {
                unreachable!("native transport must carry native runtime protocol")
            };
            engine_evidence.runtime_receipts = RuntimeEvidenceState::Verified;
            engine_evidence.restore_receipt = RuntimeEvidenceState::NotRequested;
            engine_evidence.verified_adapter = Some(configured_adapter);
            engine_evidence.sync_control_execution(execution);
        } else if uses_camoufox_host {
            let host_runtime = runtime.as_ref().and_then(|runtime| match runtime {
                EngineRuntimeProtocol::CamoufoxHost(host) => Some(host),
                EngineRuntimeProtocol::Native { .. } => None,
            });
            if host_runtime.is_none() {
                terminate_just_spawned_child(&mut child);
                engine_evidence.host_launch = RuntimeEvidenceState::Failed;
                network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                let error = LauncherError::RuntimeReceipt(
                    "Camoufox Host launch returned without its bound JSONL session".to_owned(),
                );
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::VerificationFailed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence),
                    network_evidence: Some(network_evidence),
                });
                return Err(error);
            }
            engine_evidence.host_launch = RuntimeEvidenceState::Observed;
            engine_evidence.bootstrap_delivery = RuntimeEvidenceState::NotApplicable;
            engine_evidence.runtime_receipts = RuntimeEvidenceState::NotApplicable;
            engine_evidence.restore_receipt = RuntimeEvidenceState::NotApplicable;
            engine_evidence.verified_adapter = None;
            apply_camoufox_host_capability_evidence(
                &mut engine_evidence,
                &engine_plan.capabilities,
                &host_runtime
                    .expect("checked Camoufox Host runtime")
                    .evidence_class,
            )
            .map_err(|error| {
                terminate_just_spawned_child(&mut child);
                let error = LauncherError::RuntimeReceipt(error);
                self.activation = Some(RuntimeActivation {
                    active_silo_id: None,
                    state: RuntimeState::VerificationFailed,
                    updated_at: Utc::now(),
                    message: Some(error.to_string()),
                    browser_verification: browser_verification.clone(),
                    engine_evidence: Some(engine_evidence.clone()),
                    network_evidence: Some({
                        network_evidence.browser_routing = RuntimeEvidenceState::Failed;
                        network_evidence.clone()
                    }),
                });
                error
            })?;
        }
        mark_browser_routing_applied(&silo.network_profile, &mut network_evidence);
        let started_at = Utc::now();
        let runtime_id = network_evidence.runtime_id;
        let activation = RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: RuntimeState::Running,
            updated_at: Utc::now(),
            message: Some(
                if configured_adapter == crate::engine::EngineAdapterId::Camoufox {
                    "Camoufox Silo 正在运行；Host 生命周期已建立，实际出口、Geo、DNS 与 WebRTC 仍需独立 runtime evidence。".to_owned()
                } else {
                    "Silo 正在运行。请在这个 Silo 的 Companion 中主动验证实际出口、DNS 证据和 WebRTC 路径。".to_owned()
                },
            ),
            browser_verification,
            engine_evidence: Some(engine_evidence),
            network_evidence: Some(network_evidence),
        };
        self.child = Some(child);
        self.profile_lease = Some(profile_lease);
        self.engine_runtime = runtime;
        self.proxy_relay = proxy_relay;
        self.health_context = Some(RuntimeHealthContext {
            silo: silo.clone(),
            runtime_id,
            compromised: false,
            mihomo_authentication,
            mihomo_guard,
        });
        self.record = Some(RuntimeRecord {
            silo_id: silo.id,
            pid,
            started_at,
            last_seen_at: started_at,
            state: RuntimeState::Running,
        });
        self.activation = Some(activation.clone());
        self.persist_current_record(RuntimeState::Running);
        Ok(activation)
    }

    pub fn recheck_active(
        &mut self,
        silo: &Silo,
        proxy_authentication: Option<&ProxyAuthentication>,
        mihomo_authentication: Option<&MihomoControllerAuthentication>,
    ) -> Result<RuntimeActivation, LauncherError> {
        self.refresh();
        if self
            .activation
            .as_ref()
            .and_then(|activation| activation.active_silo_id)
            != Some(silo.id)
        {
            return Err(LauncherError::InvalidNetwork(
                "该 Silo 当前没有可重新检查的活动或待恢复会话。".to_owned(),
            ));
        }
        if self.stock_profile_release_pending() {
            return Ok(self
                .activation
                .clone()
                .expect("pending stock Profile release has an activation"));
        }
        if self
            .health_context
            .as_ref()
            .is_some_and(|context| context.compromised)
        {
            return Ok(self
                .activation
                .clone()
                .expect("active runtime has activation"));
        }

        let configured_adapter = silo.engine.adapter_id(&silo.browser.kind);
        let browser_verification = silo
            .engine
            .is_stock()
            .then(|| verify_browser_descriptor(&silo.browser));
        let mut evidence = self
            .activation
            .as_ref()
            .and_then(|activation| activation.network_evidence.clone())
            .unwrap_or_else(|| {
                configured_network_evidence(
                    &silo.network_profile,
                    proxy_authentication.is_some(),
                    configured_adapter,
                )
            });
        let mut failures = Vec::new();
        let mut hard_failure = false;
        if let Some(verification) = browser_verification.as_ref() {
            if verification.state != BrowserVerificationState::Verified {
                hard_failure = true;
                failures.push(verification.message.clone());
            }
        }
        let externally_packaged = !silo.engine.is_stock();
        let mut engine_evidence = self
            .activation
            .as_ref()
            .and_then(|activation| activation.engine_evidence.clone())
            .unwrap_or_else(|| {
                RuntimeEngineEvidence::configured(configured_adapter, externally_packaged)
            });
        if externally_packaged {
            match production_engine_adapter(&silo.engine, silo.browser.clone()) {
                Ok(adapter) => {
                    let health = adapter.health();
                    if health.state == EngineHealthState::Healthy {
                        engine_evidence.package_verification = RuntimeEvidenceState::Verified;
                    } else {
                        hard_failure = true;
                        engine_evidence.package_verification = RuntimeEvidenceState::Failed;
                        failures.push(format!("外部引擎包重新验证失败：{}", health.message));
                    }
                }
                Err(error) => {
                    hard_failure = true;
                    engine_evidence.package_verification = RuntimeEvidenceState::Failed;
                    failures.push(format!("外部引擎适配器不可用：{error}"));
                }
            }
        }
        let expects_relay = expects_managed_relay(&silo.network_profile);
        if expects_relay
            && !self
                .proxy_relay
                .as_ref()
                .is_some_and(ProxyRelay::is_healthy)
        {
            evidence.browser_routing = RuntimeEvidenceState::Failed;
            evidence.endpoint = RuntimeEvidenceState::Failed;
            failures.push(
                "受管 loopback relay 不再可连接；浏览器的代理参数不会改为 DIRECT。".to_owned(),
            );
        }
        if let Some(binding) = silo.network_profile.external_mihomo_binding() {
            match mihomo::verify_binding(binding, mihomo_authentication) {
                Ok(()) => evidence.controller_binding = RuntimeEvidenceState::Verified,
                Err(error) => {
                    evidence.controller_binding = RuntimeEvidenceState::Failed;
                    failures.push(format!("Mihomo 绑定回读失败：{error}"));
                }
            }
        }
        if let Err(error) =
            preflight_proxy(&silo.network_profile, proxy_authentication, &mut evidence)
        {
            evidence.endpoint = RuntimeEvidenceState::Failed;
            failures.push(error.to_string());
        }

        let required_failure = silo.network_profile.requires_proxy() && !failures.is_empty();
        let state = if required_failure || hard_failure {
            RuntimeState::VerificationFailed
        } else {
            RuntimeState::Running
        };
        let message = if failures.is_empty() {
            "用户触发的运行时重新检查已完成；浏览器路径、受管 relay 和外部绑定（如有）均保持可回读。未进行后台节点轮换。"
                .to_owned()
        } else {
            format!(
                "用户触发的重新检查发现问题：{} VeriSilo 不会把浏览器切换到 DIRECT，也不声称能强制关闭浏览器。",
                failures.join("；")
            )
        };
        self.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo.id),
            state: state.clone(),
            updated_at: Utc::now(),
            message: Some(message),
            browser_verification,
            engine_evidence: Some(engine_evidence),
            network_evidence: Some(evidence),
        });
        self.persist_current_record(state);
        Ok(self.activation.clone().expect("activation was set"))
    }

    pub fn rebind_active_mihomo(
        &mut self,
        silo: &Silo,
        proxy_authentication: Option<&ProxyAuthentication>,
        mihomo_authentication: Option<&MihomoControllerAuthentication>,
    ) -> Result<RuntimeActivation, LauncherError> {
        self.refresh();
        if self
            .activation
            .as_ref()
            .and_then(|activation| activation.active_silo_id)
            != Some(silo.id)
        {
            return Err(LauncherError::InvalidNetwork(
                "该 Silo 当前没有可重新绑定的活动或待恢复会话。".to_owned(),
            ));
        }
        if self.stock_profile_release_pending() {
            return Err(LauncherError::InvalidNetwork(
                "受管浏览器进程已退出，但 Chromium Profile 锁仍未释放；在 Silo 回到空闲前不会重绑网络。"
                    .to_owned(),
            ));
        }
        if self
            .health_context
            .as_ref()
            .is_some_and(|context| context.compromised)
        {
            return Err(LauncherError::InvalidNetwork(
                "该 runtime 的网络路径已终止并关闭旧 relay；重绑不会重新打开旧端口。请正常关闭浏览器后明确重新启动 Silo。"
                    .to_owned(),
            ));
        }
        let binding = silo
            .network_profile
            .external_mihomo_binding()
            .ok_or_else(|| {
                LauncherError::InvalidNetwork("该 Silo 未配置外部 Mihomo 绑定。".to_owned())
            })?;
        mihomo::apply_binding(binding, mihomo_authentication)
            .map_err(|error| LauncherError::Mihomo(error.to_string()))?;
        if let Some(evidence) = self
            .activation
            .as_mut()
            .and_then(|activation| activation.network_evidence.as_mut())
        {
            reset_network_observation(evidence, Utc::now());
        }
        self.recheck_active(silo, proxy_authentication, mihomo_authentication)
    }

    fn shutdown_relay_for_runtime(&mut self, silo_id: Uuid, runtime_id: Uuid) -> bool {
        if !self
            .proxy_relay
            .as_ref()
            .is_some_and(|relay| relay.matches_runtime(silo_id, runtime_id))
        {
            return false;
        }
        let mut relay = self
            .proxy_relay
            .take()
            .expect("matching relay was checked above");
        relay.shutdown_for_runtime(silo_id, runtime_id)
    }

    fn fail_closed_network_path(&mut self, reason: String, failure: RuntimeNetworkFailure) {
        self.fail_closed_network_path_with_persistence(reason, failure, true);
    }

    fn fail_closed_network_path_with_persistence(
        &mut self,
        reason: String,
        failure: RuntimeNetworkFailure,
        persist_runtime_record: bool,
    ) {
        let stock_profile_release_pending = self.stock_profile_release_pending();
        let (silo_id, runtime_id, required, expected_relay, secret_revoked) = {
            let Some(context) = self.health_context.as_mut() else {
                return;
            };
            if context.compromised {
                return;
            }
            context.compromised = true;
            (
                context.silo.id,
                context.runtime_id,
                context.silo.network_profile.requires_proxy(),
                expects_managed_relay(&context.silo.network_profile),
                context.mihomo_authentication.take().is_some(),
            )
        };
        let relay_closed = self.shutdown_relay_for_runtime(silo_id, runtime_id);
        let now = Utc::now();
        if let Some(activation) = self.activation.as_mut() {
            activation.state = if stock_profile_release_pending {
                RuntimeState::RecoveryRequired
            } else {
                RuntimeState::VerificationFailed
            };
            activation.updated_at = now;
            if let Some(evidence) = activation.network_evidence.as_mut() {
                invalidate_network_evidence(evidence, failure, now);
            }
            let closure = if relay_closed {
                "exact-runtime relay listener、既有连接和内存凭据已撤销"
            } else if expected_relay {
                "该 exact-runtime relay 已不存在或身份不匹配；未操作其他 relay"
            } else {
                "此模式没有受管 relay 可关闭"
            };
            let policy = if required {
                "required proxy 已进入终止性 fail-closed；浏览器不会回退 DIRECT"
            } else if relay_closed {
                "非 required 的受管 relay 也按产品语义关闭；浏览器自身的可选代理回退不宣称 fail-closed"
            } else {
                "非 required 模式已诚实降级，旧 verified/observed 证据不再有效"
            };
            activation.message = Some(format!(
                "检测到运行网络路径失效：{reason}。{closure}；{policy}。状态刷新、复查和重绑都不会重开旧端口；请正常关闭浏览器后明确重新启动并验证。{}",
                if secret_revoked {
                    " Mihomo Controller Secret 已从该 runtime 内存撤销。"
                } else {
                    ""
                }
            ));
        }
        if persist_runtime_record {
            self.persist_current_record(if stock_profile_release_pending {
                RuntimeState::RecoveryRequired
            } else {
                RuntimeState::VerificationFailed
            });
        }
    }

    fn drain_engine_runtime_receipts(
        &mut self,
        wait: Option<Duration>,
        child_exited: bool,
        persist_runtime_record: bool,
    ) {
        let deadline = wait.map(|duration| Instant::now() + duration);
        let mut changed = false;
        let mut failure = None;
        let mut snapshot = None;
        if let Some(EngineRuntimeProtocol::Native {
            receiver,
            execution,
        }) = self.engine_runtime.as_mut()
        {
            loop {
                let event = if let Some(deadline) = deadline {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
                        Ok(event) => Some(event),
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            if !child_exited && !execution.restore_complete() {
                                failure = Some(
                                    "controlled engine runtime receipt channel closed while the process was still active"
                                        .to_owned(),
                                );
                            }
                            None
                        }
                    }
                } else {
                    match receiver.try_recv() {
                        Ok(event) => Some(event),
                        Err(mpsc::TryRecvError::Empty) => None,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            if !child_exited && !execution.restore_complete() {
                                failure = Some(
                                    "controlled engine runtime receipt channel closed while the process was still active"
                                        .to_owned(),
                                );
                            }
                            None
                        }
                    }
                };
                let Some(event) = event else {
                    break;
                };
                match event {
                    EngineProtocolEvent::Receipt(Ok(frame)) => {
                        if let Err(error) =
                            execution.apply_runtime_receipt(frame.receipt, frame.issued_at)
                        {
                            failure = Some(error.to_string());
                            break;
                        }
                        changed = true;
                    }
                    EngineProtocolEvent::Receipt(Err(error)) => {
                        failure = Some(error);
                        break;
                    }
                    EngineProtocolEvent::Ack(_) => {
                        failure = Some(
                            "controlled engine emitted a duplicate bootstrap ACK at runtime"
                                .to_owned(),
                        );
                        break;
                    }
                }
            }
            if let Some(reason) = failure.as_deref() {
                execution.fail_active_capabilities(
                    &format!("runtime receipt protocol failed: {reason}"),
                    Utc::now(),
                );
            }
            if changed || failure.is_some() {
                snapshot = Some(execution.clone());
            }
        }

        if let Some(execution) = snapshot.as_ref() {
            if let Some(evidence) = self
                .activation
                .as_mut()
                .and_then(|activation| activation.engine_evidence.as_mut())
            {
                evidence.sync_control_execution(execution);
                if execution.restore_complete() {
                    evidence.restore_receipt = RuntimeEvidenceState::Verified;
                }
            }
        }
        if let Some(reason) = failure {
            self.engine_runtime = None;
            if let Some(activation) = self.activation.as_mut() {
                activation.state = RuntimeState::VerificationFailed;
                activation.updated_at = Utc::now();
                activation.message = Some(format!(
                    "受控引擎运行证据通道拒绝了收据：{reason}。能力核验已 fail-closed；不会回退 stock，也不会操作其他浏览器进程。"
                ));
                if let Some(evidence) = activation.engine_evidence.as_mut() {
                    evidence.runtime_receipts = RuntimeEvidenceState::Failed;
                    evidence.restore_receipt = RuntimeEvidenceState::Unavailable;
                    evidence.verified_adapter = None;
                }
            }
            let network_path_must_close = self.health_context.as_ref().is_some_and(|context| {
                context.silo.network_profile.requires_proxy()
                    || expects_managed_relay(&context.silo.network_profile)
            });
            if network_path_must_close {
                self.fail_closed_network_path_with_persistence(
                    format!("受控引擎运行证据失效：{reason}"),
                    RuntimeNetworkFailure::RuntimeEvidence,
                    persist_runtime_record,
                );
            } else if persist_runtime_record {
                self.persist_current_record(RuntimeState::VerificationFailed);
            }
        }
    }

    fn probe_camoufox_host_status(&mut self, deadline: Instant, persist_runtime_record: bool) {
        let Some(EngineRuntimeProtocol::CamoufoxHost(host)) = self.engine_runtime.as_mut() else {
            return;
        };
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(ENGINE_INITIAL_RECEIPT_TIMEOUT);
        if timeout.is_zero() {
            return;
        }
        let session_id = host.session_id.clone();
        let binding = host.binding.clone();
        let result = host
            .transport
            .request("status", json!({ "sessionId": session_id }), timeout)
            .and_then(|value| {
                serde_json::from_value::<CamoufoxHostStatusResult>(value).map_err(|error| {
                    LauncherError::RuntimeReceipt(format!(
                        "invalid Camoufox Host watchdog status response: {error}"
                    ))
                })
            })
            .and_then(|status| {
                validate_camoufox_host_status_binding(&status, &session_id, &binding)
                    .map(|_| status)
            });
        match result {
            Ok(status) => {
                host.observed_website_digest = status.observed_website_digest;
            }
            Err(error) => {
                self.mark_camoufox_host_failure(
                    format!("Camoufox Host watchdog status failed closed: {error}"),
                    persist_runtime_record,
                );
            }
        }
        if persist_runtime_record
            && self
                .activation
                .as_ref()
                .is_some_and(|activation| activation.state == RuntimeState::Running)
        {
            self.persist_current_record(RuntimeState::Running);
        }
    }

    fn refresh(&mut self) {
        let cancelled = AtomicBool::new(false);
        self.refresh_until(
            Instant::now() + RUNTIME_HEALTH_PROBE_TIMEOUT,
            &cancelled,
            true,
        );
    }

    fn refresh_until(
        &mut self,
        deadline: Instant,
        cancelled: &AtomicBool,
        persist_runtime_record: bool,
    ) {
        #[cfg(target_os = "windows")]
        if self.child.is_none() && self.refresh_pending_stock_profile_release() {
            return;
        }

        let child_status = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten());
        let child_exited = child_status.is_some();
        let normal_exit = child_status.is_some_and(|status| status.success());
        self.drain_engine_runtime_receipts(
            child_exited.then_some(ENGINE_EXIT_RECEIPT_GRACE),
            child_exited,
            persist_runtime_record,
        );
        if !child_exited {
            self.probe_camoufox_host_status(deadline, persist_runtime_record);
        }
        if child_exited {
            let mut restore_missing = false;
            let mut restore_verified = false;
            let mut camoufox_host_exit_unconfirmed = false;
            if let Some(runtime) = self.engine_runtime.take() {
                match runtime {
                    EngineRuntimeProtocol::Native { mut execution, .. } => {
                        if !normal_exit || !execution.restore_complete() {
                            restore_missing = true;
                            execution.fail_active_capabilities(
                                "controlled engine did not exit normally with a bound Restore receipt",
                                Utc::now(),
                            );
                        }
                        if let Some(evidence) = self
                            .activation
                            .as_mut()
                            .and_then(|activation| activation.engine_evidence.as_mut())
                        {
                            evidence.sync_control_execution(&execution);
                            if restore_missing {
                                evidence.restore_receipt = RuntimeEvidenceState::Failed;
                                evidence.verified_adapter = None;
                            } else {
                                evidence.restore_receipt = RuntimeEvidenceState::Verified;
                                restore_verified = true;
                            }
                        }
                    }
                    EngineRuntimeProtocol::CamoufoxHost(host) => {
                        if !host.closed_confirmed {
                            camoufox_host_exit_unconfirmed = true;
                            if let Some(evidence) = self
                                .activation
                                .as_mut()
                                .and_then(|activation| activation.engine_evidence.as_mut())
                            {
                                evidence.host_launch = RuntimeEvidenceState::Failed;
                                evidence.verified_adapter = None;
                            }
                        }
                    }
                }
            } else if let Some(evidence) = self
                .activation
                .as_mut()
                .and_then(|activation| activation.engine_evidence.as_mut())
            {
                if evidence.runtime_receipts != RuntimeEvidenceState::NotApplicable
                    && evidence.restore_receipt != RuntimeEvidenceState::Verified
                {
                    restore_missing = true;
                    if evidence.restore_receipt == RuntimeEvidenceState::NotRequested {
                        evidence.restore_receipt = RuntimeEvidenceState::Unavailable;
                    }
                    evidence.verified_adapter = None;
                } else if evidence.restore_receipt == RuntimeEvidenceState::Verified {
                    restore_verified = true;
                }
            }
            let network_evidence = self
                .activation
                .as_ref()
                .and_then(|activation| activation.network_evidence.clone());
            let browser_verification = self
                .activation
                .as_ref()
                .and_then(|activation| activation.browser_verification.clone());
            let engine_evidence = self
                .activation
                .as_ref()
                .and_then(|activation| activation.engine_evidence.clone());
            let relay_binding = self
                .health_context
                .as_ref()
                .map(|context| (context.silo.id, context.runtime_id));
            if let Some((silo_id, runtime_id)) = relay_binding {
                self.shutdown_relay_for_runtime(silo_id, runtime_id);
            }
            self.child = None;
            if camoufox_host_exit_unconfirmed {
                let engine_evidence = self
                    .activation
                    .as_ref()
                    .and_then(|activation| activation.engine_evidence.clone());
                self.activation = Some(RuntimeActivation {
                    active_silo_id: relay_binding.map(|binding| binding.0),
                    state: RuntimeState::VerificationFailed,
                    updated_at: Utc::now(),
                    message: Some(
                        "Camoufox Host exited before close/shutdown and exact process-tree confirmation; profile ownership remains held fail-closed."
                            .to_owned(),
                    ),
                    browser_verification,
                    engine_evidence,
                    network_evidence,
                });
                if persist_runtime_record {
                    self.persist_current_record(RuntimeState::VerificationFailed);
                }
                return;
            }

            #[cfg(target_os = "windows")]
            if let Some((silo_id, profile_directory)) = self
                .health_context
                .as_ref()
                .filter(|context| {
                    context.silo.execution_target.is_local() && context.silo.engine.is_stock()
                })
                .map(|context| (context.silo.id, context.silo.engine_profile_directory()))
            {
                let pending_message = match chromium_profile_sentinel_exists(&profile_directory) {
                    Ok(false) => None,
                    Ok(true) => Some(
                        "受管浏览器进程已退出，但 Chromium Profile 锁仍存在；VeriSilo 将继续核对，且不会启动另一个 Silo 或删除浏览器锁。"
                            .to_owned(),
                    ),
                    Err(error) => Some(format!(
                        "受管浏览器进程已退出，但无法确认 Chromium Profile 锁是否已释放：{error}。VeriSilo 将保持锁定并继续核对。"
                    )),
                };
                if let Some(message) = pending_message {
                    self.pending_stock_profile_release = Some(profile_directory);
                    self.activation = Some(RuntimeActivation {
                        active_silo_id: Some(silo_id),
                        state: RuntimeState::RecoveryRequired,
                        updated_at: Utc::now(),
                        message: Some(message),
                        browser_verification,
                        engine_evidence,
                        network_evidence,
                    });
                    // Exact child exit is a terminal ownership transition. It
                    // must reach disk even when the watchdog observed it first.
                    self.persist_current_record(RuntimeState::RecoveryRequired);
                    return;
                }
            }

            #[cfg(target_os = "windows")]
            let stock_profile_release_completed =
                self.health_context.as_ref().is_some_and(|context| {
                    context.silo.execution_target.is_local() && context.silo.engine.is_stock()
                });
            #[cfg(not(target_os = "windows"))]
            let stock_profile_release_completed = false;
            self.profile_lease = None;
            self.health_context = None;
            self.activation = Some(RuntimeActivation {
                active_silo_id: None,
                state: RuntimeState::Stopped,
                updated_at: Utc::now(),
                message: Some(if restore_missing {
                    "受管浏览器进程已退出，但没有收到完整且绑定的 Restore 收据；不会声称能力已恢复，也未强制结束任何其他浏览器。"
                        .to_owned()
                } else if restore_verified {
                    "受管浏览器进程已退出；已接受退出前完整且绑定的 Restore 收据。".to_owned()
                } else {
                    "受管浏览器进程已退出。".to_owned()
                }),
                browser_verification,
                engine_evidence,
                network_evidence,
            });
            if persist_runtime_record || stock_profile_release_completed {
                self.persist_current_record(RuntimeState::Stopped);
            }
            return;
        }

        if self.child.is_none() {
            return;
        }

        if self
            .health_context
            .as_ref()
            .is_some_and(|context| context.compromised)
        {
            if persist_runtime_record {
                self.persist_current_record(RuntimeState::VerificationFailed);
            }
            return;
        }

        let now = Utc::now();
        let exit_evidence_expired = self
            .activation
            .as_ref()
            .and_then(|activation| activation.network_evidence.as_ref())
            .is_some_and(|evidence| {
                evidence
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= now)
                    && matches!(
                        evidence.exit,
                        RuntimeEvidenceState::Observed | RuntimeEvidenceState::Verified
                    )
            });
        if exit_evidence_expired {
            self.fail_closed_network_path_with_persistence(
                "已接受的 time-bounded Silo 出口证据已过期".to_owned(),
                RuntimeNetworkFailure::ExitEvidence,
                persist_runtime_record,
            );
            return;
        }

        if cancelled.load(Ordering::Acquire) {
            return;
        }

        let relay_failed = self.health_context.as_ref().is_some_and(|context| {
            expects_managed_relay(&context.silo.network_profile)
                && !self.proxy_relay.as_ref().is_some_and(|relay| {
                    relay.matches_runtime(context.silo.id, context.runtime_id)
                        && relay.is_healthy_until(deadline, cancelled)
                })
        });
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let endpoint_error = self.health_context.as_ref().and_then(|context| {
            self.proxy_relay.as_ref().and_then(|relay| {
                relay
                    .matches_runtime(context.silo.id, context.runtime_id)
                    .then(|| relay.verify_upstream_until(deadline, cancelled).err())
                    .flatten()
                    .map(|error| error.to_string())
            })
        });
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let mihomo_error = self.health_context.as_ref().and_then(|context| {
            let binding = context.silo.network_profile.external_mihomo_binding()?;
            let NetworkProfile::FixedProxy { host, port, .. } = &context.silo.network_profile
            else {
                return Some((
                    "external Mihomo runtime no longer has a fixed proxy endpoint".to_owned(),
                    RuntimeNetworkFailure::Configuration,
                ));
            };
            let result = context.mihomo_guard.as_ref().map_or_else(
                || Err(mihomo::MihomoError::ConfigurationDrift),
                |guard| {
                    mihomo::verify_runtime_guard_until(
                        guard,
                        binding,
                        host,
                        *port,
                        context.mihomo_authentication.as_ref(),
                        deadline,
                        cancelled,
                    )
                },
            );
            result.err().map(|error| {
                let failure = match &error {
                    mihomo::MihomoError::DirectFallbackPossible
                    | mihomo::MihomoError::UnsafeSelectedNode(_)
                    | mihomo::MihomoError::ProxyListenerMismatch
                    | mihomo::MihomoError::ConfigurationDrift => {
                        RuntimeNetworkFailure::Configuration
                    }
                    _ => RuntimeNetworkFailure::Controller,
                };
                (error.to_string(), failure)
            })
        });
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        if relay_failed || endpoint_error.is_some() || mihomo_error.is_some() {
            let (message, failure) = if relay_failed {
                (
                    "受管 loopback relay 健康检查失败".to_owned(),
                    RuntimeNetworkFailure::Relay,
                )
            } else if let Some(error) = endpoint_error {
                (
                    format!("代理/Mihomo 上游端点健康检查失败：{error}"),
                    RuntimeNetworkFailure::Endpoint,
                )
            } else {
                let (error, failure) = mihomo_error.expect("checked above");
                (format!("Mihomo runtime guard 无法回读：{error}"), failure)
            };
            self.fail_closed_network_path_with_persistence(
                message,
                failure,
                persist_runtime_record,
            );
        } else if persist_runtime_record
            && self
                .activation
                .as_ref()
                .is_some_and(|activation| activation.state == RuntimeState::Running)
        {
            self.persist_current_record(RuntimeState::Running);
        }
    }

    fn persist_current_record(&mut self, state: RuntimeState) {
        let Some(record) = self.record.as_mut() else {
            return;
        };
        record.last_seen_at = Utc::now();
        record.state = state;
        if let Some(path) = self.record_path.as_ref() {
            let _ = write_runtime_record(path, record);
        }
    }

    #[cfg(target_os = "windows")]
    fn refresh_pending_stock_profile_release(&mut self) -> bool {
        let Some(profile_directory) = self.pending_stock_profile_release.clone() else {
            return false;
        };
        match chromium_profile_sentinel_exists(&profile_directory) {
            Ok(false) => {
                self.pending_stock_profile_release = None;
                self.profile_lease = None;
                self.health_context = None;
                if let Some(activation) = self.activation.as_mut() {
                    activation.active_silo_id = None;
                    activation.state = RuntimeState::Stopped;
                    activation.updated_at = Utc::now();
                    activation.message = Some(
                        "受管浏览器进程与 Chromium Profile 锁均已释放；Silo 已回到空闲。"
                            .to_owned(),
                    );
                }
                self.persist_current_record(RuntimeState::Stopped);
            }
            Ok(true) => self.persist_current_record(RuntimeState::RecoveryRequired),
            Err(error) => {
                if let Some(activation) = self.activation.as_mut() {
                    activation.updated_at = Utc::now();
                    activation.message = Some(format!(
                        "无法确认 Chromium Profile 锁是否已释放：{error}。VeriSilo 将保持锁定并继续核对。"
                    ));
                }
                self.persist_current_record(RuntimeState::RecoveryRequired);
            }
        }
        true
    }
}

/// Vault replacement is safe only after the runtime has positively reached a
/// quiescent state. A missing active Silo identifier is not enough: corrupt or
/// incomplete recovery records deliberately use `None` while remaining
/// untrusted.
pub fn runtime_allows_vault_restore(activation: &RuntimeActivation) -> bool {
    activation.active_silo_id.is_none()
        && matches!(activation.state, RuntimeState::Idle | RuntimeState::Stopped)
}

pub fn profile_in_use(profile_directory: &Path) -> bool {
    profile_has_browser_lock(profile_directory)
}

pub fn managed_profiles_are_quiescent_for_vault_restore(profile_directories: &[PathBuf]) -> bool {
    profile_directories
        .iter()
        .all(|profile_directory| !profile_in_use(profile_directory))
}

fn asserted_exit_state(has_public_ip_observation: bool) -> RuntimeEvidenceState {
    if has_public_ip_observation {
        RuntimeEvidenceState::Observed
    } else {
        RuntimeEvidenceState::Failed
    }
}

fn configured_network_evidence(
    profile: &NetworkProfile,
    has_authentication: bool,
    configured_adapter: crate::engine::EngineAdapterId,
) -> RuntimeNetworkEvidence {
    let mut evidence = RuntimeNetworkEvidence::configured(profile, has_authentication);
    if configured_adapter == crate::engine::EngineAdapterId::Camoufox {
        evidence.safeguards.clear();
    }
    evidence
}

fn bind_camoufox_host_proxy(
    plan: &mut EngineLaunchPlan,
    profile: &NetworkProfile,
    relay: Option<&ProxyRelay>,
) -> Result<(), LauncherError> {
    if !matches!(
        profile,
        NetworkProfile::FixedProxy {
            proxy_required: true,
            ..
        }
    ) {
        return Ok(());
    }
    let relay = relay.ok_or_else(|| {
        LauncherError::ProxyRelay(
            "required Camoufox FixedProxy launch has no loopback relay".to_owned(),
        )
    })?;
    let endpoint = relay.endpoint();
    if endpoint.host != "127.0.0.1" || endpoint.port == 0 {
        return Err(LauncherError::ProxyRelay(
            "Camoufox Host relay endpoint is not canonical loopback SOCKS5".to_owned(),
        ));
    }
    let binding = plan.camoufox_host.as_mut().ok_or_else(|| {
        LauncherError::Engine(
            "required Camoufox FixedProxy launch has no typed Host binding".to_owned(),
        )
    })?;
    binding.browser_proxy_server = Some(format!("socks5://127.0.0.1:{}", endpoint.port));
    Ok(())
}

fn mark_browser_routing_applied(profile: &NetworkProfile, evidence: &mut RuntimeNetworkEvidence) {
    if !matches!(profile, NetworkProfile::Direct { .. }) {
        evidence.browser_routing = RuntimeEvidenceState::Applied;
    }
}

fn is_window_scoped_http_authentication(
    profile: &NetworkProfile,
    evidence: &RuntimeNetworkEvidence,
) -> bool {
    matches!(
        profile,
        NetworkProfile::FixedProxy {
            scheme: crate::domain::ProxyScheme::Http,
            ..
        }
    ) && evidence.authentication != RuntimeEvidenceState::NotApplicable
}

fn reset_http_authentication_window(evidence: &mut RuntimeNetworkEvidence) {
    if evidence.authentication != RuntimeEvidenceState::Applied {
        evidence.authentication = RuntimeEvidenceState::Configured;
    }
    evidence.authentication_provenance = RuntimeNetworkEvidenceProvenance::DesktopControlPlane;
}

fn reset_network_observation(evidence: &mut RuntimeNetworkEvidence, now: DateTime<Utc>) {
    evidence.evidence_id = Uuid::new_v4();
    evidence.observed_at = now;
    evidence.expires_at = None;
    evidence.provenance = RuntimeNetworkEvidenceProvenance::DesktopControlPlane;
    evidence.exit = RuntimeEvidenceState::NotRequested;
    evidence.dns = RuntimeEvidenceState::NotRequested;
    evidence.web_rtc = RuntimeEvidenceState::NotRequested;
}

fn invalidate_network_evidence(
    evidence: &mut RuntimeNetworkEvidence,
    failure: RuntimeNetworkFailure,
    now: DateTime<Utc>,
) {
    let downgrade = |state: &mut RuntimeEvidenceState| {
        if matches!(
            state,
            RuntimeEvidenceState::Reachable
                | RuntimeEvidenceState::Applied
                | RuntimeEvidenceState::Observed
                | RuntimeEvidenceState::Verified
        ) {
            *state = RuntimeEvidenceState::Unavailable;
        }
    };
    downgrade(&mut evidence.configuration);
    downgrade(&mut evidence.controller_binding);
    downgrade(&mut evidence.endpoint);
    downgrade(&mut evidence.authentication);
    downgrade(&mut evidence.browser_routing);
    downgrade(&mut evidence.exit);
    downgrade(&mut evidence.dns);
    downgrade(&mut evidence.web_rtc);

    evidence.evidence_id = Uuid::new_v4();
    evidence.observed_at = now;
    evidence.expires_at = None;
    evidence.provenance = RuntimeNetworkEvidenceProvenance::DesktopControlPlane;
    if evidence.authentication != RuntimeEvidenceState::Failed {
        evidence.authentication_provenance = RuntimeNetworkEvidenceProvenance::DesktopControlPlane;
    }
    evidence.browser_routing = RuntimeEvidenceState::Failed;
    evidence.exit = if matches!(failure, RuntimeNetworkFailure::ExitEvidence) {
        RuntimeEvidenceState::Failed
    } else {
        RuntimeEvidenceState::Unavailable
    };
    evidence.dns = RuntimeEvidenceState::Unavailable;
    evidence.web_rtc = RuntimeEvidenceState::Unavailable;

    match failure {
        RuntimeNetworkFailure::Relay | RuntimeNetworkFailure::Endpoint => {
            evidence.endpoint = RuntimeEvidenceState::Failed;
        }
        RuntimeNetworkFailure::Controller => {
            evidence.controller_binding = RuntimeEvidenceState::Failed;
            evidence.endpoint = RuntimeEvidenceState::Failed;
        }
        RuntimeNetworkFailure::Configuration => {
            evidence.configuration = RuntimeEvidenceState::Failed;
            evidence.controller_binding = RuntimeEvidenceState::Failed;
            evidence.endpoint = RuntimeEvidenceState::Failed;
        }
        RuntimeNetworkFailure::Credentials => {
            if evidence.authentication != RuntimeEvidenceState::NotApplicable {
                evidence.authentication = RuntimeEvidenceState::Unavailable;
            }
            if evidence.controller_binding != RuntimeEvidenceState::NotApplicable {
                evidence.controller_binding = RuntimeEvidenceState::Unavailable;
            }
        }
        RuntimeNetworkFailure::ExitEvidence | RuntimeNetworkFailure::RuntimeEvidence => {}
    }
}

fn spawn_engine_child(
    plan: &EngineLaunchPlan,
    arguments: &[OsString],
    bootstrap: Option<&EngineBootstrapEnvelope>,
) -> Result<SpawnedEngine, LauncherError> {
    if plan.transport == EngineTransport::CamoufoxHostJsonlV1 {
        if bootstrap.is_some() {
            return Err(LauncherError::Engine(
                "Camoufox Host transport cannot carry a native bootstrap envelope".to_owned(),
            ));
        }
        return spawn_camoufox_host(plan, arguments);
    }
    let mut child = spawn_engine_child_with(
        plan,
        arguments,
        bootstrap,
        |path, arguments, piped_stdin| {
            let mut command = Command::new(path);
            command.args(arguments);
            command.stdin(if piped_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            });
            if piped_stdin {
                command.stdout(Stdio::piped());
                // The bootstrap token is delivered only on stdin. Keep the
                // controlled process from reflecting it into inherited desktop
                // logs before the bound ACK has been accepted.
                command.stderr(Stdio::null());
            }
            command.spawn()
        },
        |stdin, envelope| {
            write_engine_bootstrap_frame(stdin, envelope)
                .map_err(|error| LauncherError::Bootstrap(error.to_string()))
        },
    )?;
    let Some(envelope) = bootstrap else {
        return Ok(SpawnedEngine {
            child,
            bootstrap_ack: None,
            runtime: None,
        });
    };
    let receiver = match start_engine_protocol_reader(&mut child, envelope) {
        Ok(receiver) => receiver,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let ack =
        match await_engine_bootstrap_ack_event(&mut child, &receiver, ENGINE_BOOTSTRAP_ACK_TIMEOUT)
        {
            Ok(ack) => ack,
            Err(error) => {
                terminate_just_spawned_child(&mut child);
                return Err(error);
            }
        };
    let execution = match await_initial_engine_receipts(
        &mut child,
        &receiver,
        &envelope.control,
        ENGINE_INITIAL_RECEIPT_TIMEOUT,
    ) {
        Ok(execution) => execution,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    Ok(SpawnedEngine {
        child,
        bootstrap_ack: Some(ack),
        runtime: Some(EngineRuntimeProtocol::Native {
            receiver,
            execution,
        }),
    })
}

fn camoufox_host_plan_timeout(_plan: &EngineLaunchPlan) -> Duration {
    #[cfg(test)]
    if _plan.adapter.adapter_version == M3_WI_REAL_HOST_ADAPTER_VERSION {
        return M3_WI_REAL_HOST_TIMEOUT;
    }
    ENGINE_INITIAL_RECEIPT_TIMEOUT
}

fn camoufox_host_runtime_timeout(_runtime: &CamoufoxHostRuntime) -> Duration {
    #[cfg(test)]
    if _runtime.real_host_integration {
        return M3_WI_REAL_HOST_TIMEOUT;
    }
    ENGINE_INITIAL_RECEIPT_TIMEOUT
}

fn spawn_camoufox_host(
    plan: &EngineLaunchPlan,
    arguments: &[OsString],
) -> Result<SpawnedEngine, LauncherError> {
    let binding = plan.camoufox_host.as_ref().ok_or_else(|| {
        LauncherError::Engine(
            "Camoufox Host transport has no typed artifact/profile/package binding".to_owned(),
        )
    })?;
    if plan.adapter.id != crate::engine::EngineAdapterId::Camoufox
        || binding.protocol != CAMOUFOX_HOST_PROTOCOL
        || plan.shell
    {
        return Err(LauncherError::Engine(
            "Camoufox Host transport has an invalid adapter, protocol, or shell boundary"
                .to_owned(),
        ));
    }
    let typed_arguments = plan
        .arguments
        .iter()
        .cloned()
        .map(OsString::from)
        .collect::<Vec<_>>();
    if arguments != typed_arguments {
        return Err(LauncherError::Engine(
            "Camoufox Host transport arguments must match the typed Host plan exactly".to_owned(),
        ));
    }
    let mut command = Command::new(&plan.executable_path);
    command.args(arguments);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    let mut child = command.spawn().map_err(LauncherError::Spawn)?;
    let mut transport = match CamoufoxHostTransport::attach(&mut child) {
        Ok(transport) => transport,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let response_timeout = camoufox_host_plan_timeout(plan);
    let hello_timeout = if response_timeout > ENGINE_BOOTSTRAP_ACK_TIMEOUT {
        response_timeout
    } else {
        ENGINE_BOOTSTRAP_ACK_TIMEOUT
    };
    let hello_value = match transport.request("hello", json!({}), hello_timeout) {
        Ok(value) => value,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let hello: CamoufoxHostHello = match serde_json::from_value(hello_value)
        .map_err(|error| {
            LauncherError::RuntimeReceipt(format!("invalid Camoufox Host hello: {error}"))
        })
        .and_then(|hello| validate_camoufox_host_hello(&hello, binding, arguments).map(|_| hello))
    {
        Ok(hello) => hello,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let mut launch_params = json!({
        "artifactId": binding.artifact_id,
        "profileId": binding.profile_id,
        "expectedArtifactFileSha256": binding.artifact_file_sha256,
    });
    if let Some(browser_proxy_server) = binding.browser_proxy_server.as_deref() {
        launch_params["browserProxyServer"] = json!(browser_proxy_server);
    }
    let launch_value = match transport.request("launch", launch_params, response_timeout) {
        Ok(value) => value,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let launch: CamoufoxHostLaunchResult = match serde_json::from_value(launch_value)
        .map_err(|error| {
            LauncherError::RuntimeReceipt(format!("invalid Camoufox Host launch response: {error}"))
        })
        .and_then(|launch| validate_camoufox_host_launch(&launch, binding).map(|_| launch))
    {
        Ok(launch) => launch,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let status_value = match transport.request(
        "status",
        json!({ "sessionId": launch.session_id }),
        response_timeout,
    ) {
        Ok(value) => value,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let status: CamoufoxHostStatusResult = match serde_json::from_value(status_value)
        .map_err(|error| {
            LauncherError::RuntimeReceipt(format!("invalid Camoufox Host status response: {error}"))
        })
        .and_then(|status| validate_camoufox_host_status(&status, &launch, binding).map(|_| status))
    {
        Ok(status) => status,
        Err(error) => {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    };
    let _ = status;
    Ok(SpawnedEngine {
        child,
        bootstrap_ack: None,
        runtime: Some(EngineRuntimeProtocol::CamoufoxHost(Box::new(
            CamoufoxHostRuntime {
                transport,
                session_id: launch.session_id,
                binding: binding.clone(),
                observed_website_digest: launch.observed_website_digest,
                evidence_class: launch
                    .evidence_class
                    .or(Some(hello.evidence_class))
                    .unwrap_or_else(|| "observed-on-this-host".to_owned()),
                closed_confirmed: false,
                #[cfg(test)]
                real_host_integration: plan.adapter.adapter_version
                    == M3_WI_REAL_HOST_ADAPTER_VERSION,
                #[cfg(test)]
                launch_surface: (plan.adapter.adapter_version == M3_WI_REAL_HOST_ADAPTER_VERSION)
                    .then(|| {
                        json!({
                            "integrationPath": "test-only-real-host",
                            "adapterVersion": plan.adapter.adapter_version,
                            "transport": "camoufox-host-jsonl-v1",
                            "executablePath": plan.executable_path.to_string_lossy(),
                            "arguments": arguments
                                .iter()
                                .map(|argument| argument.to_string_lossy().into_owned())
                                .collect::<Vec<_>>(),
                            "shell": plan.shell,
                            "packageVerification": plan
                                .package_verification
                                .as_ref()
                                .map(|_| "present"),
                        })
                    }),
            },
        ))),
    })
}

fn validate_camoufox_host_hello(
    hello: &CamoufoxHostHello,
    binding: &CamoufoxHostLaunch,
    arguments: &[OsString],
) -> Result<(), LauncherError> {
    #[cfg(test)]
    let probe_port_positions = arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| (argument == "--probe-port").then_some(index))
        .collect::<Vec<_>>();
    #[cfg(test)]
    let expected_probe_port_policy = match probe_port_positions.as_slice() {
        [] => "ephemeral",
        [position] => {
            let port = arguments
                .get(position + 1)
                .and_then(|value| value.to_str())
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|port| *port != 0)
                .ok_or_else(|| {
                    LauncherError::RuntimeReceipt(
                        "Camoufox Host --probe-port must be one non-zero u16 value".to_owned(),
                    )
                })?;
            let _ = port;
            "fixed"
        }
        _ => {
            return Err(LauncherError::RuntimeReceipt(
                "Camoufox Host plan contains duplicate --probe-port arguments".to_owned(),
            ))
        }
    };
    #[cfg(not(test))]
    let expected_probe_port_policy = "ephemeral";
    let expected_roots = ["--artifact-root", "--profile-root", "--state-root"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            arguments
                .iter()
                .position(|argument| argument == label)
                .and_then(|position| arguments.get(position + 1))
                .map(|value| value.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    LauncherError::RuntimeReceipt(format!(
                        "Camoufox Host arguments are missing {label}"
                    ))
                })
                .map(|root| (index, root))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if hello.protocol != binding.protocol
        || hello.host_version != binding.host_version
        || hello.platform != binding.platform
        || hello.browser_release != binding.browser_release
        || hello.asset_sha256 != binding.browser_asset_sha256
        || hello.tree_manifest_sha256 != binding.browser_tree_manifest_sha256
        || hello.max_frame_bytes != MAX_CAMOUFOX_HOST_FRAME_BYTES
        || hello.probe_port_policy != expected_probe_port_policy
        || hello.state != "idle"
        || hello.verified
        || hello.evidence_class != "observed-on-this-host"
        || canonical_host_path(&hello.tree_manifest)?
            != canonical_host_path(&binding.browser_tree_manifest_path.to_string_lossy())?
        || hello.artifact_root != expected_roots[0].1
        || hello.profile_root != expected_roots[1].1
        || hello.state_root != expected_roots[2].1
    {
        return Err(LauncherError::RuntimeReceipt(
            "Camoufox Host hello did not match protocol, version, platform, asset, tree, roots, or honest evidence bindings"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_camoufox_host_launch(
    launch: &CamoufoxHostLaunchResult,
    binding: &CamoufoxHostLaunch,
) -> Result<(), LauncherError> {
    if launch.state != "running"
        || launch.artifact_id != binding.artifact_id
        || launch.profile_id != binding.profile_id
        || launch.artifact_file_sha256 != binding.artifact_file_sha256
        || launch.browser_proxy_server.as_deref() != binding.browser_proxy_server.as_deref()
        || launch.observed_website_digest.is_none()
        || launch.verified != Some(false)
        || launch.evidence_class.as_deref() != Some("observed-on-this-host")
    {
        return Err(LauncherError::RuntimeReceipt(
            "Camoufox Host launch response did not prove the bound Artifact/profile or honest running state"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_camoufox_host_status(
    status: &CamoufoxHostStatusResult,
    launch: &CamoufoxHostLaunchResult,
    binding: &CamoufoxHostLaunch,
) -> Result<(), LauncherError> {
    validate_camoufox_host_status_binding(status, &launch.session_id, binding)
}

fn validate_camoufox_host_status_binding(
    status: &CamoufoxHostStatusResult,
    session_id: &str,
    binding: &CamoufoxHostLaunch,
) -> Result<(), LauncherError> {
    if status.state != "running"
        || status.session_id.as_deref() != Some(session_id)
        || status.artifact_id.as_deref() != Some(binding.artifact_id.as_str())
        || status.profile_id.as_deref() != Some(binding.profile_id.as_str())
        || status.artifact_file_sha256.as_deref() != Some(binding.artifact_file_sha256.as_str())
        || status.browser_proxy_server.as_deref() != binding.browser_proxy_server.as_deref()
        || status.observed_website_digest.is_none()
        || status.verified != Some(false)
        || status.evidence_class.as_deref() != Some("observed-on-this-host")
        || status.quarantine.is_some()
        || status.failure.is_some()
    {
        return Err(LauncherError::RuntimeReceipt(
            "Camoufox Host status response did not preserve the exact running binding".to_owned(),
        ));
    }
    Ok(())
}

fn validate_camoufox_host_close(
    close: &CamoufoxHostCloseResult,
    binding: &CamoufoxHostLaunch,
    session_id: &str,
) -> Result<(), LauncherError> {
    let tree_exited = close
        .process_tree_exit
        .as_ref()
        .and_then(|value| value.get("exited"))
        .and_then(Value::as_bool)
        == Some(true);
    let typed_close_is_clean = close
        .close_outcome
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        == Some("success")
        && close
            .close_outcome
            .as_ref()
            .and_then(|value| value.pointer("/contextClose/ctx/status"))
            .and_then(Value::as_str)
            == Some("success")
        && close
            .close_outcome
            .as_ref()
            .and_then(|value| value.pointer("/contextClose/page/status"))
            .is_some_and(|status| matches!(status.as_str(), Some("success") | Some("not_present")))
        && close
            .close_outcome
            .as_ref()
            .and_then(|value| value.pointer("/gracefulProcessExit/status"))
            .and_then(Value::as_str)
            == Some("success")
        && close
            .close_outcome
            .as_ref()
            .and_then(|value| value.pointer("/forcedJobCleanup/status"))
            .and_then(Value::as_str)
            == Some("not_needed");
    if close.session_id != session_id
        || close.state != "exited"
        || close.exit_status != Some(0)
        || close.exit_file_observed != Some(true)
        || !tree_exited
        || !typed_close_is_clean
        || close.quarantine.is_some()
    {
        return Err(LauncherError::RuntimeReceipt(format!(
            "Camoufox Host close did not confirm an exited, non-quarantined exact process tree for {}",
            binding.profile_id
        )));
    }
    Ok(())
}

fn validate_camoufox_host_shutdown(
    shutdown: &CamoufoxHostShutdownResult,
) -> Result<(), LauncherError> {
    if shutdown.state != "shutdown"
        || !shutdown.self_check.argv_matches.is_empty()
        || !shutdown.self_check.stderr_log_matches.is_empty()
    {
        return Err(LauncherError::RuntimeReceipt(
            "Camoufox Host shutdown self-check did not pass".to_owned(),
        ));
    }
    Ok(())
}

fn apply_camoufox_host_capability_evidence(
    evidence: &mut RuntimeEngineEvidence,
    plan: &[EngineCapabilityState],
    evidence_class: &str,
) -> Result<(), String> {
    let mut capabilities = plan.to_vec();
    let host_evidence = format!("camoufox-host/v1 running; evidenceClass={}", evidence_class);
    for capability in &mut capabilities {
        if capability.id == EngineCapabilityId::ProfileIsolation
            && capability.operation == EngineCapabilityOperation::Configured
        {
            capability
                .transition(
                    EngineCapabilityOperation::Applied,
                    vec![host_evidence.clone()],
                    Utc::now(),
                )
                .map_err(|error| error.to_string())?;
        }
    }
    evidence.capabilities = capabilities;
    Ok(())
}

fn canonical_host_path(value: &str) -> Result<PathBuf, LauncherError> {
    let path = PathBuf::from(value);
    path.canonicalize().map_err(|error| {
        LauncherError::RuntimeReceipt(format!(
            "Camoufox Host binding path is not resolvable: {error}"
        ))
    })
}

fn start_engine_protocol_reader(
    child: &mut Child,
    envelope: &EngineBootstrapEnvelope,
) -> Result<mpsc::Receiver<EngineProtocolEvent>, LauncherError> {
    let mut stdout = child.stdout.take().ok_or_else(|| {
        LauncherError::Bootstrap(
            "the newly spawned controlled engine has no piped protocol channel".to_owned(),
        )
    })?;
    let ack_expectation = EngineBootstrapAckExpectation::from(envelope);
    let receipt_expectation = EngineRuntimeReceiptExpectation::from(envelope);
    let (sender, receiver) = mpsc::sync_channel(ENGINE_PROTOCOL_CHANNEL_CAPACITY);
    // Dropping JoinHandle deliberately detaches the sole bounded reader. We
    // never join it on launch timeout or exit because a descendant may have
    // inherited stdout even after the exact controlled child is gone.
    let _reader = thread::spawn(move || {
        let ack = read_engine_bootstrap_ack_frame(&mut stdout, &ack_expectation, Utc::now())
            .map_err(|error| error.to_string());
        let accepted = ack.is_ok();
        if sender.send(EngineProtocolEvent::Ack(ack)).is_err() || !accepted {
            return;
        }
        let mut expected_sequence = 1_u64;
        loop {
            let receipt = read_engine_runtime_receipt_frame(
                &mut stdout,
                &receipt_expectation,
                expected_sequence,
                Utc::now(),
            )
            .map_err(|error| error.to_string());
            let accepted = receipt.is_ok();
            let terminal_restore = receipt.as_ref().is_ok_and(|frame| {
                matches!(
                    &frame.receipt,
                    crate::engine::EngineRuntimeReceipt::Phase(
                        crate::engine::EngineRuntimePhaseReceipt {
                            phase: crate::engine::EngineControlPhase::Restore,
                            ..
                        }
                    )
                )
            });
            if sender.send(EngineProtocolEvent::Receipt(receipt)).is_err()
                || !accepted
                || terminal_restore
            {
                return;
            }
            let Some(next_sequence) = expected_sequence.checked_add(1) else {
                let _ = sender.send(EngineProtocolEvent::Receipt(Err(
                    "runtime receipt sequence exhausted its bounded counter".to_owned(),
                )));
                return;
            };
            expected_sequence = next_sequence;
        }
    });
    Ok(receiver)
}

#[cfg(all(test, unix))]
fn await_engine_bootstrap_ack_with_timeout(
    child: &mut Child,
    envelope: &EngineBootstrapEnvelope,
    timeout: Duration,
) -> Result<EngineBootstrapAck, LauncherError> {
    let receiver = start_engine_protocol_reader(child, envelope)?;
    match await_engine_bootstrap_ack_event(child, &receiver, timeout) {
        Ok(ack) => Ok(ack),
        Err(error) => {
            terminate_just_spawned_child(child);
            Err(error)
        }
    }
}

fn await_engine_bootstrap_ack_event(
    child: &mut Child,
    receiver: &mpsc::Receiver<EngineProtocolEvent>,
    timeout: Duration,
) -> Result<EngineBootstrapAck, LauncherError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(LauncherError::Bootstrap(
                "controlled engine did not acknowledge bootstrap within the fixed timeout"
                    .to_owned(),
            ));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(EngineProtocolEvent::Ack(result)) => {
                return result.map_err(LauncherError::Bootstrap)
            }
            Ok(EngineProtocolEvent::Receipt(_)) => {
                return Err(LauncherError::Bootstrap(
                    "controlled engine emitted runtime evidence before its bootstrap ACK"
                        .to_owned(),
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(LauncherError::Bootstrap(
                    "controlled engine protocol channel closed before its bootstrap ACK".to_owned(),
                ))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if child.try_wait().map_err(LauncherError::Spawn)?.is_some() {
            return Err(LauncherError::Bootstrap(
                "controlled engine exited before its bootstrap ACK".to_owned(),
            ));
        }
    }
}

fn await_initial_engine_receipts(
    child: &mut Child,
    receiver: &mpsc::Receiver<EngineProtocolEvent>,
    plan: &crate::engine::EngineControlPlan,
    timeout: Duration,
) -> Result<EngineControlExecution, LauncherError> {
    let deadline = Instant::now() + timeout;
    let mut execution = EngineControlExecution::from_plan(plan.clone());
    while !execution.launch_evidence_complete() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(LauncherError::RuntimeReceipt(
                "controlled engine did not provide complete observe/apply/verify evidence within the fixed timeout"
                    .to_owned(),
            ));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(EngineProtocolEvent::Receipt(Ok(frame))) => execution
                .apply_runtime_receipt(frame.receipt, frame.issued_at)
                .map_err(|error| LauncherError::RuntimeReceipt(error.to_string()))?,
            Ok(EngineProtocolEvent::Receipt(Err(error))) => {
                return Err(LauncherError::RuntimeReceipt(error))
            }
            Ok(EngineProtocolEvent::Ack(_)) => {
                return Err(LauncherError::RuntimeReceipt(
                    "controlled engine emitted a duplicate bootstrap ACK".to_owned(),
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(LauncherError::RuntimeReceipt(
                    "controlled engine protocol channel closed before initial evidence completed"
                        .to_owned(),
                ))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if child.try_wait().map_err(LauncherError::Spawn)?.is_some() {
            return Err(LauncherError::RuntimeReceipt(
                "controlled engine exited before initial evidence completed".to_owned(),
            ));
        }
    }
    if child.try_wait().map_err(LauncherError::Spawn)?.is_some() {
        return Err(LauncherError::RuntimeReceipt(
            "controlled engine exited before launch verification completed".to_owned(),
        ));
    }
    Ok(execution)
}

fn spawn_engine_child_with<B, F, D>(
    plan: &EngineLaunchPlan,
    arguments: &[OsString],
    bootstrap: Option<&B>,
    spawn: F,
    deliver: D,
) -> Result<Child, LauncherError>
where
    F: FnOnce(&Path, &[OsString], bool) -> std::io::Result<Child>,
    D: FnOnce(&mut ChildStdin, &B) -> Result<(), LauncherError>,
{
    let requires_bootstrap = plan.identity_delivery.is_some();
    if requires_bootstrap != bootstrap.is_some() {
        return Err(LauncherError::Engine(
            "launch plan and secure-stdin bootstrap presence do not match".to_owned(),
        ));
    }
    let mut child = spawn(&plan.executable_path, arguments, requires_bootstrap)?;
    if let Some(envelope) = bootstrap {
        let delivery = child
            .stdin
            .take()
            .ok_or_else(|| {
                LauncherError::Bootstrap(
                    "the newly spawned controlled engine has no piped stdin".to_owned(),
                )
            })
            .and_then(|mut stdin| deliver(&mut stdin, envelope));
        if let Err(error) = delivery {
            terminate_just_spawned_child(&mut child);
            return Err(error);
        }
    }
    Ok(child)
}

fn terminate_just_spawned_child(child: &mut Child) {
    // This handle was returned by the immediately preceding spawn call. Never
    // enumerate, match, or terminate any other browser process.
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "windows")]
fn verify_stock_browser_profile_ownership(
    child: &mut Child,
    profile_directory: &Path,
    executable_path: &Path,
) -> Result<(), LauncherError> {
    #[cfg(test)]
    if executable_path.with_extension("version-output").is_file() {
        return Ok(());
    }
    let _ = executable_path;

    let deadline = Instant::now() + STOCK_BROWSER_STARTUP_TIMEOUT;
    let mut sentinel_observed_at = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(LauncherError::BrowserStartup(format!(
                    "newly spawned browser exited before ownership stabilized (status {status})"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                terminate_just_spawned_child(child);
                return Err(LauncherError::Spawn(error));
            }
        }

        match chromium_profile_sentinel_exists(profile_directory) {
            Ok(true) => {
                let observed_at = sentinel_observed_at.get_or_insert_with(Instant::now);
                if observed_at.elapsed() >= STOCK_BROWSER_OWNERSHIP_STABILITY {
                    return Ok(());
                }
            }
            Ok(false) => {
                sentinel_observed_at = None;
            }
            Err(error) => {
                terminate_just_spawned_child(child);
                return Err(LauncherError::BrowserStartup(format!(
                    "Chromium Profile sentinel probe failed closed: {error}"
                )));
            }
        }

        if Instant::now() >= deadline {
            terminate_just_spawned_child(child);
            return Err(LauncherError::BrowserStartup(
                "the exact child stayed alive but no stable Chromium Profile sentinel appeared"
                    .to_owned(),
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn expects_managed_relay(profile: &NetworkProfile) -> bool {
    ProxyRelay::supports(profile)
        && (profile.requires_proxy()
            || profile.credential_reference().is_some()
            || profile.external_mihomo_binding().is_some())
}

fn read_runtime_record(path: &Path) -> Result<Option<RuntimeRecord>, std::io::Error> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let record = serde_json::from_slice::<RuntimeRecord>(&raw)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    if record.pid == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "runtime record PID must be non-zero",
        ));
    }
    Ok(Some(record))
}

fn write_runtime_record(path: &Path, record: &RuntimeRecord) -> Result<(), std::io::Error> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "runtime record path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let encoded = serde_json::to_vec_pretty(record)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&encoded)?;
    file.sync_all()?;
    drop(file);
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)
}

#[cfg(target_os = "windows")]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    // Avoid tasklist.exe entirely: its output is locale/code-page dependent and
    // can be denied by application-control policy even when this process may
    // query the recorded PID directly.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut exit_code = 0_u32;
        let queried = GetExitCodeProcess(process, &mut exit_code) != 0;
        let _ = CloseHandle(process);
        queried && exit_code == STILL_ACTIVE as u32
    }
}

#[cfg(target_os = "linux")]
fn process_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn process_is_alive(_pid: u32) -> bool {
    false
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

#[cfg(all(test, target_os = "windows"))]
#[path = "launcher_m3_wi_windows_tests.rs"]
mod m3_wi_windows_tests;

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        io::Cursor,
        net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };

    #[cfg(unix)]
    use std::{
        io::{Read as _, Write as _},
        process::Stdio,
        sync::{mpsc, Mutex},
    };

    use chrono::{Duration as ChronoDuration, Utc};
    use uuid::Uuid;

    #[cfg(unix)]
    use super::spawn_engine_child_with;
    use super::{
        managed_profiles_are_quiescent_for_vault_restore, runtime_allows_vault_restore,
        write_runtime_record, RuntimeHealthContext, RuntimeManager, RuntimeRecord,
    };
    #[cfg(unix)]
    use crate::domain::ExternalMihomoBinding;
    use crate::domain::ProxyScheme;
    use crate::domain::{
        BrowserDescriptor, BrowserKind, NetworkProfile, RuntimeActivation, RuntimeEngineEvidence,
        RuntimeEvidenceState, RuntimeNetworkEvidence, RuntimeNetworkEvidenceProvenance,
        RuntimeState, Silo, SiloExecutionTarget, SCHEMA_VERSION,
    };
    use crate::engine::{
        BrowserFamily, CamoufoxArtifactBindingV1, CamoufoxHostLaunch, DerivedIdentityToken,
        EngineAdapter, EngineAdapterId, EngineBootstrapEnvelope, EngineCapabilityAvailability,
        EngineCapabilityId, EngineCapabilityOperation, EngineCapabilityState, EngineChannel,
        EngineControlPhase, EngineControlPlan, EngineDescriptor, EngineError, EngineHealth,
        EngineLaunchPackageVerification, EngineLaunchPlan, EngineLaunchRequest,
        EngineMaintenanceReceipt, EngineNegotiation, EnginePackageRequest, EngineTransport,
        IdentityDelivery, IdentityDeliveryRequirement, IdentityDerivationContext, IdentityTemplate,
        IdentityTokenDeriver, SiloEngineConfig, SiteFallbackAction, SiteFallbackPolicy,
        SiteFallbackRule, CAMOUFOX_ARTIFACT_SCHEMA, CAMOUFOX_ARTIFACT_SCHEMA_V6,
        ENGINE_CONTRACT_VERSION,
    };
    use crate::native_host::{
        NativeDnsObservation, NativeDnsState, NativeDnssecState, NativeIpExitObservation,
        NativeIpVersion, NativeNetworkCheckResult, NativeNetworkEvidenceCoverage,
        NativeNetworkEvidenceInboxEntry, NativeNetworkHint, NativeReputationObservation,
        NativeReputationState, NETWORK_REPUTATION_EXPLANATION, PROTOCOL_VERSION,
    };
    use crate::proxy_relay::{ProxyRelay, RelayAuthenticationEvidence};
    #[cfg(unix)]
    use crate::runtime_watchdog::RuntimeWatchdog;
    use crate::vault::{BrowserProfileLease, MihomoControllerAuthentication, ProxyAuthentication};

    fn test_silo(network_profile: NetworkProfile) -> Silo {
        let profile_directory =
            std::env::temp_dir().join(format!("verisilo-launcher-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&profile_directory).expect("create test Profile directory");
        let browser = profile_directory.join("chrome.exe");
        fs::write(&browser, []).expect("create test browser harness");
        fs::write(
            browser.with_extension("version-output"),
            "Google Chrome 126.0.6478.127\n",
        )
        .expect("create browser version output");
        Silo {
            id: Uuid::new_v4(),
            schema_version: SCHEMA_VERSION,
            name: "test".to_owned(),
            color: "#4f46e5".to_owned(),
            browser: BrowserDescriptor {
                kind: BrowserKind::Chrome,
                executable_path: fs::canonicalize(&browser)
                    .expect("canonical browser path")
                    .to_string_lossy()
                    .to_string(),
                version: Some("126.0.6478.127".to_owned()),
            },
            execution_target: SiloExecutionTarget::Local,
            profile_directory: profile_directory.to_string_lossy().to_string(),
            network_profile,
            engine: Default::default(),
            seed_reference: Uuid::new_v4(),
            created_at: Utc::now(),
            identity_locked_at: None,
            archived_at: None,
        }
    }

    fn http_runtime_manager() -> (RuntimeManager, Silo, Uuid, TcpListener) {
        let upstream = TcpListener::bind("127.0.0.1:0").expect("bind unused HTTP upstream");
        let silo = test_silo(NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: crate::domain::ProxyScheme::Http,
            host: "127.0.0.1".to_owned(),
            port: upstream.local_addr().expect("upstream address").port(),
            bypass_list: Vec::new(),
            credential_reference: Some(Uuid::new_v4()),
            external_mihomo: None,
        });
        let mut evidence = RuntimeNetworkEvidence::configured(&silo.network_profile, true);
        let runtime_id = evidence.runtime_id;
        evidence.browser_routing = RuntimeEvidenceState::Applied;
        let relay = ProxyRelay::start(
            &silo.network_profile,
            silo.id,
            runtime_id,
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "secret".to_owned(),
            )),
        )
        .expect("start receipt test relay");
        let runtime = RuntimeManager {
            activation: Some(RuntimeActivation {
                active_silo_id: Some(silo.id),
                state: RuntimeState::Running,
                updated_at: Utc::now(),
                message: None,
                browser_verification: None,
                engine_evidence: None,
                network_evidence: Some(evidence),
            }),
            proxy_relay: Some(relay),
            health_context: Some(RuntimeHealthContext {
                silo: silo.clone(),
                runtime_id,
                compromised: false,
                mihomo_authentication: None,
                mihomo_guard: None,
            }),
            ..RuntimeManager::default()
        };
        (runtime, silo, runtime_id, upstream)
    }

    #[cfg(unix)]
    const MIHOMO_GOOD_PROXIES: &str = r#"{"proxies":{"GLOBAL":{"type":"Selector","now":"Tokyo 01","all":["Tokyo 01"]},"Tokyo 01":{"type":"Socks5","alive":true}}}"#;
    #[cfg(unix)]
    const MIHOMO_DRIFTED_PROXIES: &str = r#"{"proxies":{"GLOBAL":{"type":"Selector","now":"DIRECT","all":["DIRECT","Tokyo 01"]},"DIRECT":{"type":"Direct","alive":true},"Tokyo 01":{"type":"Socks5","alive":true}}}"#;
    #[cfg(unix)]
    fn spawn_json_controller(bodies: Vec<String>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind fake Controller");
        let address = listener.local_addr().expect("Controller address");
        let worker = thread::spawn(move || {
            for body in bodies {
                let (mut stream, _) = listener.accept().expect("accept Controller request");
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0_u8; 1];
                    stream
                        .read_exact(&mut byte)
                        .expect("read Controller request");
                    request.push(byte[0]);
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("write Controller response");
            }
        });
        (format!("http://127.0.0.1:{}/", address.port()), worker)
    }

    #[cfg(unix)]
    fn spawn_socks_health_endpoint() -> (TcpListener, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind fake Mihomo SOCKS");
        let worker_listener = listener.try_clone().expect("clone fake SOCKS listener");
        let worker = thread::spawn(move || {
            let (mut stream, _) = worker_listener.accept().expect("accept health probe");
            let mut greeting = [0_u8; 3];
            stream
                .read_exact(&mut greeting)
                .expect("read health greeting");
            assert_eq!(greeting, [5, 1, 0]);
            stream.write_all(&[5, 0]).expect("accept health greeting");
        });
        (listener, worker)
    }

    #[cfg(unix)]
    enum MihomoRuntimeFixture {
        NodeDrift,
        ConfigDrift,
        ControllerExit,
    }

    #[cfg(unix)]
    fn mihomo_runtime_manager(
        fixture: MihomoRuntimeFixture,
    ) -> (
        RuntimeManager,
        Silo,
        u16,
        TcpListener,
        thread::JoinHandle<()>,
        thread::JoinHandle<()>,
    ) {
        let (upstream, upstream_worker) = spawn_socks_health_endpoint();
        let upstream_port = upstream.local_addr().expect("upstream address").port();
        let mut controller_bodies = vec![
            MIHOMO_GOOD_PROXIES.to_owned(),
            format!(
                "{{\"mode\":\"global\",\"socks-port\":{upstream_port},\"mixed-port\":0,\"allow-lan\":false}}"
            ),
        ];
        match fixture {
            MihomoRuntimeFixture::NodeDrift => {
                controller_bodies.push(MIHOMO_DRIFTED_PROXIES.to_owned());
            }
            MihomoRuntimeFixture::ConfigDrift => {
                controller_bodies.push(MIHOMO_GOOD_PROXIES.to_owned());
                controller_bodies.push(format!(
                    "{{\"mode\":\"global\",\"socks-port\":{upstream_port},\"mixed-port\":0,\"allow-lan\":true}}"
                ));
            }
            MihomoRuntimeFixture::ControllerExit => {}
        }
        let (controller_url, controller_worker) = spawn_json_controller(controller_bodies);
        let silo = test_silo(NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: upstream_port,
            bypass_list: Vec::new(),
            credential_reference: None,
            external_mihomo: Some(ExternalMihomoBinding {
                controller_url,
                selector_group: "GLOBAL".to_owned(),
                node_name: "Tokyo 01".to_owned(),
                controller_secret_reference: None,
            }),
        });
        let binding = silo
            .network_profile
            .external_mihomo_binding()
            .expect("Mihomo binding");
        let guard = crate::mihomo::capture_runtime_guard(binding, "127.0.0.1", upstream_port, None)
            .expect("capture safe Mihomo runtime");
        let mut evidence = RuntimeNetworkEvidence::configured(&silo.network_profile, false);
        let runtime_id = evidence.runtime_id;
        evidence.controller_binding = RuntimeEvidenceState::Verified;
        evidence.endpoint = RuntimeEvidenceState::Reachable;
        evidence.browser_routing = RuntimeEvidenceState::Applied;
        evidence.exit = RuntimeEvidenceState::Observed;
        evidence.expires_at = Some(Utc::now() + ChronoDuration::minutes(10));
        evidence.provenance = RuntimeNetworkEvidenceProvenance::ExtensionAsserted;
        let relay = ProxyRelay::start(&silo.network_profile, silo.id, runtime_id, None)
            .expect("start exact runtime relay");
        let old_port = relay.endpoint().port;
        let child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .spawn()
            .expect("spawn owned browser fixture");
        let runtime = RuntimeManager {
            child: Some(child),
            activation: Some(RuntimeActivation {
                active_silo_id: Some(silo.id),
                state: RuntimeState::Running,
                updated_at: Utc::now(),
                message: None,
                browser_verification: None,
                engine_evidence: None,
                network_evidence: Some(evidence),
            }),
            proxy_relay: Some(relay),
            health_context: Some(RuntimeHealthContext {
                silo: silo.clone(),
                runtime_id,
                compromised: false,
                mihomo_authentication: None,
                mihomo_guard: Some(guard),
            }),
            ..RuntimeManager::default()
        };
        (
            runtime,
            silo,
            old_port,
            upstream,
            controller_worker,
            upstream_worker,
        )
    }

    fn network_evidence_entry(
        silo_id: Uuid,
        runtime_id: Uuid,
        checked_at: chrono::DateTime<Utc>,
        has_public_ip: bool,
    ) -> NativeNetworkEvidenceInboxEntry {
        NativeNetworkEvidenceInboxEntry {
            schema_version: 1,
            protocol_version: PROTOCOL_VERSION,
            evidence_id: Uuid::new_v4(),
            request_id: Uuid::new_v4(),
            silo_id,
            runtime_id,
            received_at: checked_at,
            expires_at: checked_at + ChronoDuration::minutes(10),
            coverage: NativeNetworkEvidenceCoverage {
                trigger: "user_initiated".to_owned(),
                transport: "companion_extension_fetch".to_owned(),
                ip: "third_party_https_observation".to_owned(),
                public_dns: "public_doh_answer_comparison".to_owned(),
                actual_dns_path: "not_observed".to_owned(),
                web_rtc: "not_observed".to_owned(),
                quic: "not_observed".to_owned(),
            },
            result: NativeNetworkCheckResult {
                schema_version: 1,
                checked_at,
                ip: has_public_ip.then(|| NativeIpExitObservation {
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
                    latitude: None,
                    longitude: None,
                }),
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
                errors: Vec::new(),
            },
        }
    }

    #[cfg(unix)]
    fn external_spawn_plan(executable_path: PathBuf) -> EngineLaunchPlan {
        EngineLaunchPlan {
            adapter: EngineDescriptor {
                contract_version: ENGINE_CONTRACT_VERSION,
                id: EngineAdapterId::ControlledChromium,
                adapter_version: "0.1.0".to_owned(),
                engine_version: "150.0.0".to_owned(),
                channel: EngineChannel::Experimental,
                browser_family: BrowserFamily::Chromium,
                platform: "windows-x64".to_owned(),
                externally_packaged: true,
                emergency_disabled: false,
            },
            transport: EngineTransport::NativeBootstrapV1,
            executable_path,
            arguments: vec!["--verisilo-control-channel=stdio-v1".to_owned()],
            profile_directory: std::env::temp_dir().join("verisilo-controlled-profile"),
            shell: false,
            capabilities: Vec::new(),
            identity_delivery: Some(IdentityDeliveryRequirement {
                token_id: Uuid::new_v4(),
                delivery: IdentityDelivery::SecureStdinBeforeNavigation,
                expires_at: Utc::now() + chrono::Duration::minutes(30),
            }),
            control: None,
            camoufox_host: None,
            package_verification: None,
        }
    }

    fn protocol_fixture_with_behavior(
        behavior: &str,
    ) -> (EngineLaunchPlan, EngineBootstrapEnvelope, Vec<OsString>) {
        let now = Utc::now();
        let session_id = Uuid::new_v4();
        let template: IdentityTemplate = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "templateId": Uuid::new_v4(),
            "os": { "family": "windows", "version": "11", "architecture": "x64" },
            "browser": {
                "family": "chromium",
                "majorVersion": 150,
                "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/150.0.0.0 Safari/537.36",
                "uaCh": {
                    "brands": [{ "brand": "Chromium", "version": "150" }],
                    "platform": "Windows",
                    "platformVersion": "15.0.0",
                    "architecture": "x86",
                    "bitness": "64",
                    "mobile": false
                }
            },
            "languages": { "primary": "en-US", "accepted": ["en-US"] },
            "timezone": "UTC",
            "screen": {
                "width": 1920, "height": 1080,
                "availableWidth": 1920, "availableHeight": 1040,
                "devicePixelRatio": 1.0, "colorDepth": 24
            },
            "render": { "canvas": "native", "webGlVendor": null, "webGlRenderer": null },
            "fonts": { "families": ["Segoe UI"] },
            "media": { "microphones": 0, "cameras": 0, "speakers": 0, "labelsExposed": false },
            "network": {
                "proxyRequired": false, "countryCode": null,
                "timezone": "UTC", "locale": "en-US", "desiredQuic": "browser_default"
            }
        }))
        .expect("identity fixture");
        let configured_capability = |id, availability, reason: &str| EngineCapabilityState {
            id,
            availability,
            operation: EngineCapabilityOperation::Configured,
            reason: reason.to_owned(),
            verified_at: None,
            evidence: Vec::new(),
        };
        let control = EngineControlPlan {
            session_id,
            template_id: template.template_id,
            phases: [
                EngineControlPhase::Observe,
                EngineControlPhase::Apply,
                EngineControlPhase::Verify,
                EngineControlPhase::Restore,
            ],
            capabilities: vec![
                configured_capability(
                    EngineCapabilityId::ProfileIsolation,
                    EngineCapabilityAvailability::Supported,
                    "test profile boundary",
                ),
                configured_capability(
                    EngineCapabilityId::Canvas,
                    EngineCapabilityAvailability::Experimental,
                    "test canvas boundary",
                ),
            ],
            site_fallback: SiteFallbackPolicy {
                default_action: SiteFallbackAction::RestoreExperimentalControls,
                rules: vec![SiteFallbackRule {
                    site_pattern: "*.example.test".to_owned(),
                    disable_capabilities: vec![EngineCapabilityId::Canvas],
                    action: SiteFallbackAction::RestoreThenReload,
                }],
            },
        };
        let token = DerivedIdentityToken {
            token_id: Uuid::new_v4(),
            token: "x".repeat(43),
            expires_at: now + chrono::Duration::minutes(30),
        };
        let plan = EngineLaunchPlan {
            adapter: EngineDescriptor {
                contract_version: ENGINE_CONTRACT_VERSION,
                id: EngineAdapterId::ControlledChromium,
                adapter_version: "0.1.0".to_owned(),
                engine_version: "150.0.0".to_owned(),
                channel: EngineChannel::Experimental,
                browser_family: BrowserFamily::Chromium,
                platform: "windows-x64".to_owned(),
                externally_packaged: true,
                emergency_disabled: false,
            },
            transport: EngineTransport::NativeBootstrapV1,
            executable_path: PathBuf::from("node"),
            arguments: Vec::new(),
            profile_directory: std::env::temp_dir().join("verisilo-protocol-profile"),
            shell: false,
            capabilities: Vec::new(),
            identity_delivery: Some(IdentityDeliveryRequirement {
                token_id: token.token_id,
                delivery: IdentityDelivery::SecureStdinBeforeNavigation,
                expires_at: token.expires_at,
            }),
            control: Some(control),
            camoufox_host: None,
            package_verification: Some(EngineLaunchPackageVerification {
                verifier_id: "test-command-e2e".to_owned(),
                artifact_sha256: "a".repeat(64),
                digest_verified: true,
                signature_verified: true,
                verified_at: now,
            }),
        };
        let envelope =
            EngineBootstrapEnvelope::for_launch(Uuid::new_v4(), now, &plan, template, token)
                .expect("bootstrap fixture");
        let script = r#"
const chunks = [];
process.stdin.on('data', chunk => chunks.push(chunk));
process.stdin.on('end', () => {
  const frame = Buffer.concat(chunks);
  const size = frame.readUInt32BE(0);
  const bootstrap = JSON.parse(frame.subarray(4, 4 + size).toString('utf8'));
  const ack = {
    ackVersion: 1,
    contractVersion: bootstrap.contractVersion,
    adapterId: bootstrap.adapterId,
    siloId: bootstrap.siloId,
    sessionId: bootstrap.sessionId,
    tokenId: bootstrap.token.tokenId,
    package: bootstrap.package,
    status: 'bootstrap_applied',
    acceptedAt: new Date().toISOString()
  };
  const writeFrame = value => {
    const payload = Buffer.from(JSON.stringify(value));
    const header = Buffer.alloc(4);
    header.writeUInt32BE(payload.length, 0);
    process.stdout.write(Buffer.concat([header, payload]));
  };
  writeFrame(ack);
  let sequence = 1;
  const configured = bootstrap.control.capabilities
    .filter(capability => capability.operation === 'configured')
    .map(capability => capability.id);
  const receipt = (value, overrides = {}) => {
    const issuedAt = new Date();
    writeFrame({
      receiptVersion: 1,
      contractVersion: bootstrap.contractVersion,
      adapterId: bootstrap.adapterId,
      siloId: bootstrap.siloId,
      sessionId: bootstrap.sessionId,
      tokenId: bootstrap.token.tokenId,
      package: bootstrap.package,
      sequence: sequence++,
      issuedAt: issuedAt.toISOString(),
      expiresAt: new Date(issuedAt.getTime() + 10_000).toISOString(),
      receipt: value,
      ...overrides
    });
  };
  const phase = (name, ids = configured) => receipt({
    kind: 'phase',
    phase: name,
    capabilities: ids.map(id => ({ id, evidence: [`${name}:${id}`] }))
  });
  const behavior = process.argv[1];
  if (behavior === 'out_of_order') {
    phase('apply');
    return setInterval(() => {}, 1000);
  }
  if (behavior === 'missing_capability') {
    phase('observe', configured.slice(0, 1));
    return setInterval(() => {}, 1000);
  }
  if (behavior === 'duplicate_sequence') {
    phase('observe');
    sequence -= 1;
    phase('apply');
    return setInterval(() => {}, 1000);
  }
  if (behavior === 'wrong_binding') {
    const issuedAt = new Date();
    receipt({
      kind: 'phase', phase: 'observe',
      capabilities: configured.map(id => ({ id, evidence: [`observe:${id}`] }))
    }, { sessionId: '11111111-1111-4111-8111-111111111111' });
    return setInterval(() => {}, 1000);
  }
  if (behavior === 'oversized') {
    const header = Buffer.alloc(4);
    header.writeUInt32BE(32 * 1024 + 1, 0);
    process.stdout.write(header);
    return setInterval(() => {}, 1000);
  }
  if (behavior === 'early_exit') {
    return process.exit(0);
  }
  if (behavior === 'inherit_stdout_after_ack') {
    require('node:child_process').spawn('sh', ['-c', 'sleep 15'], {
      stdio: ['ignore', 1, 'ignore']
    });
    return process.exit(0);
  }
  phase('observe');
  phase('apply');
  phase('verify');
  if (behavior === 'forged_fallback') {
    receipt({
      kind: 'site_fallback',
      site: 'login.example.test',
      matchedPattern: 'evil.example.test',
      action: 'restore_then_reload',
      capabilities: [{ id: 'canvas', evidence: ['forged fallback'] }]
    });
    return setInterval(() => {}, 1000);
  }
  if (behavior === 'fallback_restore_exit') {
    receipt({
      kind: 'site_fallback',
      site: 'login.example.test',
      matchedPattern: '*.example.test',
      action: 'restore_then_reload',
      capabilities: [{ id: 'canvas', evidence: ['compatibility restore'] }]
    });
    phase('restore', ['profile_isolation']);
    return setTimeout(() => process.exit(0), 25);
  }
  if (behavior === 'restore_exit') {
    phase('restore');
    return setTimeout(() => process.exit(0), 25);
  }
  if (behavior === 'no_restore_exit') {
    return setTimeout(() => process.exit(0), 100);
  }
  setInterval(() => {}, 1000);
});
"#;
        (
            plan,
            envelope,
            vec![
                OsString::from("-e"),
                OsString::from(script),
                OsString::from(behavior),
            ],
        )
    }

    fn protocol_fixture() -> (EngineLaunchPlan, EngineBootstrapEnvelope, Vec<OsString>) {
        protocol_fixture_with_behavior("complete")
    }

    fn reserve_fake_controlled_engine_test() -> std::sync::MutexGuard<'static, ()> {
        static RESERVATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
        static NODE_WARMUP: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        let reservation = RESERVATION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        NODE_WARMUP.get_or_init(|| {
            let status = std::process::Command::new("node")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("start Node protocol fixture warmup");
            assert!(status.success(), "Node protocol fixture warmup failed");
        });
        reservation
    }

    fn fake_camoufox_host_fixture(mode: &str) -> (PathBuf, EngineLaunchPlan, Vec<OsString>) {
        let root =
            std::env::temp_dir().join(format!("verisilo-camoufox-host-fixture-{}", Uuid::new_v4()));
        let artifact_root = root.join("artifacts");
        let profile_root = root.join("profiles");
        let state_root = root.join("state");
        fs::create_dir_all(&artifact_root).expect("fake artifact root");
        fs::create_dir_all(&profile_root).expect("fake profile root");
        fs::create_dir_all(&state_root).expect("fake state root");
        let browser_tree_manifest_path = root.join("browser-tree-manifest.json");
        let browser_tree_manifest = br#"{"schema":"verisilo-camoufox-browser-tree-manifest/v1","treeRootLabel":"fake-camoufox","fileCount":1,"totalBytes":1,"entries":[{"path":"camoufox.exe","size":1,"sha256":"4444444444444444444444444444444444444444444444444444444444444444"}]}"#;
        fs::write(&browser_tree_manifest_path, browser_tree_manifest)
            .expect("fake browser tree manifest");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/camoufox/fake-host-v1.py");
        let platform = if cfg!(target_os = "windows") {
            "windows-x64"
        } else {
            "linux-x64"
        };
        let browser_proxy_server =
            matches!(mode, "proxy-required" | "proxy-mismatch" | "proxy-missing")
                .then(|| "socks5://127.0.0.1:43127".to_owned());
        let binding = CamoufoxHostLaunch {
            protocol: super::CAMOUFOX_HOST_PROTOCOL.to_owned(),
            host_version: "0.1.0".to_owned(),
            platform: platform.to_owned(),
            artifact_id: "identity-m3-fake".to_owned(),
            artifact_file_sha256: "a".repeat(64),
            profile_id: "silo-22222222222242228222222222222222".to_owned(),
            browser_release: "v152.0.4-beta.28".to_owned(),
            browser_asset_sha256: "b".repeat(64),
            browser_tree_manifest_path: browser_tree_manifest_path.clone(),
            browser_tree_manifest_sha256:
                "f5788711bf5361124b6be6265c882b9e1652d9aad368a7091bbdda683631aac2".to_owned(),
            browser_proxy_server,
        };
        let python = if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        };
        let mut arguments = vec![
            OsString::from("-u"),
            OsString::from(script.to_string_lossy().into_owned()),
            OsString::from("--artifact-root"),
            OsString::from(artifact_root.to_string_lossy().into_owned()),
            OsString::from("--profile-root"),
            OsString::from(profile_root.to_string_lossy().into_owned()),
            OsString::from("--state-root"),
            OsString::from(state_root.to_string_lossy().into_owned()),
            OsString::from("--tree-manifest"),
            OsString::from(browser_tree_manifest_path.to_string_lossy().into_owned()),
        ];
        if mode != "normal" {
            arguments.push(OsString::from("--mode"));
            arguments.push(OsString::from(mode));
        }
        let plan = EngineLaunchPlan {
            adapter: EngineDescriptor {
                contract_version: ENGINE_CONTRACT_VERSION,
                id: EngineAdapterId::Camoufox,
                adapter_version: "m3-test".to_owned(),
                engine_version: "152.0.4-beta.28".to_owned(),
                channel: EngineChannel::Experimental,
                browser_family: BrowserFamily::Firefox,
                platform: binding.platform.clone(),
                externally_packaged: true,
                emergency_disabled: false,
            },
            transport: EngineTransport::CamoufoxHostJsonlV1,
            executable_path: PathBuf::from(python),
            arguments: arguments
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect(),
            profile_directory: profile_root.join(&binding.profile_id),
            shell: false,
            capabilities: Vec::new(),
            identity_delivery: None,
            control: None,
            camoufox_host: Some(binding),
            package_verification: None,
        };
        (root, plan, arguments)
    }

    struct TestCamoufoxAdapter {
        executable_path: PathBuf,
        script: PathBuf,
        browser_tree_manifest_path: PathBuf,
        browser_tree_manifest_sha256: String,
        mode: String,
    }

    impl TestCamoufoxAdapter {
        fn test_descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                contract_version: ENGINE_CONTRACT_VERSION,
                id: EngineAdapterId::Camoufox,
                adapter_version: "m3-test-only".to_owned(),
                engine_version: "152.0.4-beta.28".to_owned(),
                channel: EngineChannel::Experimental,
                browser_family: BrowserFamily::Firefox,
                platform: if cfg!(target_os = "windows") {
                    "windows-x64".to_owned()
                } else {
                    "linux-x64".to_owned()
                },
                externally_packaged: true,
                emergency_disabled: false,
            }
        }
    }

    struct SentinelVaultDeriver {
        called: Arc<AtomicBool>,
    }

    impl IdentityTokenDeriver for SentinelVaultDeriver {
        fn derive_session_token(
            &self,
            context: &IdentityDerivationContext,
        ) -> Result<DerivedIdentityToken, EngineError> {
            self.called.store(true, Ordering::SeqCst);
            Ok(DerivedIdentityToken {
                token_id: context.session_id,
                token: "TOKEN-SENTINEL-FROM-VAULT-DERIVER".to_owned(),
                expires_at: context.expires_at,
            })
        }
    }

    impl EngineAdapter for TestCamoufoxAdapter {
        fn descriptor(&self) -> EngineDescriptor {
            self.test_descriptor()
        }

        fn negotiate(&self, _requested: &[EngineCapabilityId]) -> EngineNegotiation {
            panic!("test Camoufox adapter negotiation is not part of the launch seam")
        }

        fn install(
            &mut self,
            _request: &EnginePackageRequest,
        ) -> Result<EngineMaintenanceReceipt, EngineError> {
            Err(EngineError::CapabilityUnavailable(
                "test Camoufox adapter install is not part of the launch seam".to_owned(),
            ))
        }

        fn update(
            &mut self,
            _request: &EnginePackageRequest,
        ) -> Result<EngineMaintenanceReceipt, EngineError> {
            Err(EngineError::CapabilityUnavailable(
                "test Camoufox adapter update is not part of the launch seam".to_owned(),
            ))
        }

        fn launch_plan(
            &self,
            request: &EngineLaunchRequest,
        ) -> Result<EngineLaunchPlan, EngineError> {
            if request.derived_token.is_some() {
                return Err(EngineError::CapabilityUnavailable(
                    "test Camoufox adapter must never receive a Vault-derived token".to_owned(),
                ));
            }
            let roots = request.camoufox_roots.as_ref().ok_or_else(|| {
                EngineError::UnsafePath("test Camoufox launch is missing Host roots".to_owned())
            })?;
            let artifact_binding = request.camoufox_artifact_binding.as_ref().ok_or_else(|| {
                EngineError::InvalidIdentityTemplate(
                    "test Camoufox launch is missing Artifact binding".to_owned(),
                )
            })?;
            let silo_id = request.silo_id.ok_or_else(|| {
                EngineError::InvalidIdentityTemplate(
                    "test Camoufox launch is missing Silo ID".to_owned(),
                )
            })?;
            let profile_id = format!("silo-{}", silo_id.simple());
            let browser_tree_manifest_path = self.browser_tree_manifest_path.clone();
            let mut arguments = vec![
                "-u".to_owned(),
                self.script.to_string_lossy().into_owned(),
                "--artifact-root".to_owned(),
                roots.artifact_root.to_string_lossy().into_owned(),
                "--profile-root".to_owned(),
                roots.profile_root.to_string_lossy().into_owned(),
                "--state-root".to_owned(),
                roots.state_root.to_string_lossy().into_owned(),
                "--tree-manifest".to_owned(),
                browser_tree_manifest_path.to_string_lossy().into_owned(),
            ];
            if self.mode != "normal" {
                arguments.push("--mode".to_owned());
                arguments.push(self.mode.clone());
            }
            Ok(EngineLaunchPlan {
                adapter: self.test_descriptor(),
                transport: EngineTransport::CamoufoxHostJsonlV1,
                executable_path: self.executable_path.clone(),
                arguments,
                profile_directory: roots.profile_root.join(&profile_id),
                shell: false,
                capabilities: Vec::new(),
                identity_delivery: None,
                control: None,
                camoufox_host: Some(CamoufoxHostLaunch {
                    protocol: super::CAMOUFOX_HOST_PROTOCOL.to_owned(),
                    host_version: "0.1.0".to_owned(),
                    platform: self.test_descriptor().platform,
                    artifact_id: artifact_binding.artifact_id.clone(),
                    artifact_file_sha256: artifact_binding.artifact_file_sha256.clone(),
                    profile_id,
                    browser_release: "v152.0.4-beta.28".to_owned(),
                    browser_asset_sha256: "b".repeat(64),
                    browser_tree_manifest_path,
                    browser_tree_manifest_sha256: self.browser_tree_manifest_sha256.clone(),
                    browser_proxy_server: None,
                }),
                package_verification: Some(EngineLaunchPackageVerification {
                    verifier_id: "test-only-camoufox-host-verifier".to_owned(),
                    artifact_sha256: "c".repeat(64),
                    digest_verified: true,
                    signature_verified: true,
                    verified_at: Utc::now(),
                }),
            })
        }

        fn health(&self) -> EngineHealth {
            panic!("test Camoufox adapter health is not part of the launch seam")
        }

        fn rollback(&mut self) -> Result<EngineMaintenanceReceipt, EngineError> {
            Err(EngineError::CapabilityUnavailable(
                "test Camoufox adapter rollback is not part of the launch seam".to_owned(),
            ))
        }

        fn set_emergency_disabled(
            &mut self,
            _disabled: bool,
            _reason: Option<String>,
        ) -> Result<(), EngineError> {
            Err(EngineError::CapabilityUnavailable(
                "test Camoufox adapter emergency state is not part of the launch seam".to_owned(),
            ))
        }

        fn validate_identity_template(
            &self,
            _template: &IdentityTemplate,
        ) -> Result<(), EngineError> {
            Ok(())
        }

        fn derive_identity_token(
            &self,
            _context: &crate::engine::IdentityDerivationContext,
            _deriver: &dyn crate::engine::IdentityTokenDeriver,
        ) -> Result<DerivedIdentityToken, EngineError> {
            Err(EngineError::CapabilityUnavailable(
                "test Camoufox adapter does not derive a Host token".to_owned(),
            ))
        }

        fn control_plan(
            &self,
            _session_id: Uuid,
            _template: &IdentityTemplate,
            _rules: &[SiteFallbackRule],
        ) -> Result<EngineControlPlan, EngineError> {
            Err(EngineError::CapabilityUnavailable(
                "test Camoufox adapter control plan is not part of the Host seam".to_owned(),
            ))
        }
    }

    fn camoufox_test_identity_template(proxy_required: bool) -> IdentityTemplate {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "templateId": Uuid::new_v4(),
            "os": { "family": "windows", "version": "11", "architecture": "x64" },
            "browser": {
                "family": "firefox",
                "majorVersion": 152,
                "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:152.0) Gecko/20100101 Firefox/152.0",
                "uaCh": null
            },
            "languages": { "primary": "en-US", "accepted": ["en-US"] },
            "timezone": "UTC",
            "screen": {
                "width": 1920, "height": 1080,
                "availableWidth": 1920, "availableHeight": 1040,
                "devicePixelRatio": 1.0, "colorDepth": 24
            },
            "render": { "canvas": "native", "webGlVendor": null, "webGlRenderer": null },
            "fonts": { "families": ["Segoe UI"] },
            "media": { "microphones": 0, "cameras": 0, "speakers": 0, "labelsExposed": false },
            "network": {
                "proxyRequired": proxy_required, "countryCode": null,
                "timezone": "UTC", "locale": "en-US", "desiredQuic": "browser_default"
            }
        }))
        .expect("Camoufox test identity template")
    }

    fn camoufox_test_silo(network_profile: NetworkProfile) -> Silo {
        let mut silo = test_silo(network_profile.clone());
        silo.id = Uuid::parse_str("22222222-2222-4222-8222-222222222222")
            .expect("fixed Camoufox test Silo ID");
        silo.engine = SiloEngineConfig::Camoufox {
            identity_template: camoufox_test_identity_template(network_profile.requires_proxy()),
            fallback_rules: Vec::new(),
            artifact_binding: Some(CamoufoxArtifactBindingV1 {
                artifact_id: "identity-m3-fake".to_owned(),
                artifact_file_sha256: "a".repeat(64),
                schema: CAMOUFOX_ARTIFACT_SCHEMA.to_owned(),
            }),
        };
        silo
    }

    fn fake_camoufox_runtime_launch_fixture(
        mode: &str,
        network_profile: NetworkProfile,
    ) -> (PathBuf, RuntimeManager, Silo) {
        let requires_proxy = network_profile.requires_proxy();
        let mut silo = camoufox_test_silo(network_profile);
        if requires_proxy {
            let SiloEngineConfig::Camoufox {
                artifact_binding: Some(binding),
                ..
            } = &mut silo.engine
            else {
                unreachable!("Camoufox test Silo has an Artifact binding")
            };
            binding.schema = CAMOUFOX_ARTIFACT_SCHEMA_V6.to_owned();
        }
        let root = PathBuf::from(&silo.profile_directory);
        let managed_root = root.join("camoufox");
        fs::create_dir_all(root.join("camoufox").join("artifacts"))
            .expect("fake RuntimeManager artifact root");
        fs::create_dir_all(
            root.join("camoufox")
                .join("profiles")
                .join(format!("silo-{}", silo.id.simple())),
        )
        .expect("fake RuntimeManager profile root");
        fs::create_dir_all(managed_root.join("state")).expect("fake RuntimeManager state root");
        let browser_tree_manifest_path = root.join("browser-tree-manifest.json");
        let browser_tree_manifest = br#"{"schema":"verisilo-camoufox-browser-tree-manifest/v1","treeRootLabel":"fake-camoufox","fileCount":1,"totalBytes":1,"entries":[{"path":"camoufox.exe","size":1,"sha256":"4444444444444444444444444444444444444444444444444444444444444444"}]}"#;
        fs::write(&browser_tree_manifest_path, browser_tree_manifest)
            .expect("fake RuntimeManager browser tree");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/camoufox/fake-host-v1.py");
        let executable_path = PathBuf::from(if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        });
        let mut runtime = RuntimeManager::open(&root);
        runtime.set_test_engine_adapter(Box::new(TestCamoufoxAdapter {
            executable_path,
            script,
            browser_tree_manifest_path,
            browser_tree_manifest_sha256:
                "f5788711bf5361124b6be6265c882b9e1652d9aad368a7091bbdda683631aac2".to_owned(),
            mode: mode.to_owned(),
        }));
        (root, runtime, silo)
    }

    fn fake_camoufox_runtime_manager(mode: &str) -> (PathBuf, RuntimeManager, Uuid) {
        let (root, plan, arguments) = fake_camoufox_host_fixture(mode);
        fs::create_dir_all(&plan.profile_directory).expect("fake Host profile directory");
        let mut spawned =
            super::spawn_camoufox_host(&plan, &arguments).expect("fake Host RuntimeManager launch");
        let silo_id = Uuid::new_v4();
        let profile_lease = BrowserProfileLease::acquire_for_runtime(
            std::slice::from_ref(&plan.profile_directory),
            &plan.profile_directory,
        )
        .expect("fake Host profile lease");
        let mut evidence = RuntimeEngineEvidence::configured(EngineAdapterId::Camoufox, true);
        evidence.launched_adapter = Some(EngineAdapterId::Camoufox);
        evidence.package_verification = RuntimeEvidenceState::Verified;
        evidence.host_launch = RuntimeEvidenceState::Observed;
        evidence.bootstrap_delivery = RuntimeEvidenceState::NotApplicable;
        evidence.runtime_receipts = RuntimeEvidenceState::NotApplicable;
        evidence.restore_receipt = RuntimeEvidenceState::NotApplicable;
        let runtime = RuntimeManager {
            child: Some(spawned.child),
            activation: Some(RuntimeActivation {
                active_silo_id: Some(silo_id),
                state: RuntimeState::Running,
                updated_at: Utc::now(),
                message: None,
                browser_verification: None,
                engine_evidence: Some(evidence),
                network_evidence: None,
            }),
            engine_runtime: spawned.runtime.take(),
            profile_lease: Some(profile_lease),
            ..RuntimeManager::default()
        };
        (root, runtime, silo_id)
    }

    #[test]
    fn camoufox_host_hello_binds_typed_fixed_probe_port() {
        let root =
            std::env::temp_dir().join(format!("verisilo-camoufox-fixed-probe-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("fixed probe test root");
        let tree = root.join("browser-tree.json");
        fs::write(&tree, b"{}\n").expect("fixed probe tree path");
        let binding = CamoufoxHostLaunch {
            protocol: super::CAMOUFOX_HOST_PROTOCOL.to_owned(),
            host_version: "0.1.0".to_owned(),
            platform: "windows-x64".to_owned(),
            artifact_id: "identity-fixed-probe".to_owned(),
            artifact_file_sha256: "a".repeat(64),
            profile_id: "silo-fixed-probe".to_owned(),
            browser_release: "v152.0.4-beta.28".to_owned(),
            browser_asset_sha256: "b".repeat(64),
            browser_tree_manifest_path: tree.clone(),
            browser_tree_manifest_sha256: "c".repeat(64),
            browser_proxy_server: None,
        };
        let roots = [
            root.join("artifacts"),
            root.join("profiles"),
            root.join("state"),
        ];
        let arguments = vec![
            OsString::from("--artifact-root"),
            roots[0].as_os_str().to_owned(),
            OsString::from("--profile-root"),
            roots[1].as_os_str().to_owned(),
            OsString::from("--state-root"),
            roots[2].as_os_str().to_owned(),
            OsString::from("--tree-manifest"),
            tree.as_os_str().to_owned(),
            OsString::from("--probe-port"),
            OsString::from("43127"),
        ];
        let hello = super::CamoufoxHostHello {
            protocol: binding.protocol.clone(),
            host_version: binding.host_version.clone(),
            python_version: Some("3.12.11".to_owned()),
            artifact_root: roots[0].to_string_lossy().into_owned(),
            profile_root: roots[1].to_string_lossy().into_owned(),
            state_root: roots[2].to_string_lossy().into_owned(),
            max_frame_bytes: crate::engine::MAX_CAMOUFOX_HOST_FRAME_BYTES,
            probe_port_policy: "fixed".to_owned(),
            browser_release: binding.browser_release.clone(),
            asset_sha256: binding.browser_asset_sha256.clone(),
            tree_manifest: tree.to_string_lossy().into_owned(),
            tree_manifest_sha256: binding.browser_tree_manifest_sha256.clone(),
            platform: binding.platform.clone(),
            state: "idle".to_owned(),
            verified: false,
            evidence_class: "observed-on-this-host".to_owned(),
        };
        assert!(super::validate_camoufox_host_hello(&hello, &binding, &arguments).is_ok());
        let mut duplicated = arguments.clone();
        duplicated.extend([OsString::from("--probe-port"), OsString::from("43128")]);
        assert!(super::validate_camoufox_host_hello(&hello, &binding, &duplicated).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_host_running_applies_only_profile_binding() {
        let plan = EngineCapabilityId::ALL
            .into_iter()
            .map(|id| EngineCapabilityState {
                id,
                availability: if id == EngineCapabilityId::ProfileIsolation {
                    EngineCapabilityAvailability::Supported
                } else {
                    EngineCapabilityAvailability::Experimental
                },
                operation: EngineCapabilityOperation::Configured,
                reason: "configured test capability".to_owned(),
                verified_at: None,
                evidence: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut evidence = RuntimeEngineEvidence::configured(EngineAdapterId::Camoufox, true);

        super::apply_camoufox_host_capability_evidence(
            &mut evidence,
            &plan,
            "observed-on-this-host",
        )
        .expect("apply bound Camoufox Host evidence");

        for capability in evidence.capabilities {
            if capability.id == EngineCapabilityId::ProfileIsolation {
                assert_eq!(capability.operation, EngineCapabilityOperation::Applied);
                assert_eq!(
                    capability.evidence,
                    vec![
                        "camoufox-host/v1 running; evidenceClass=observed-on-this-host".to_owned()
                    ]
                );
            } else {
                assert_eq!(capability.operation, EngineCapabilityOperation::Configured);
                assert!(capability.evidence.is_empty());
            }
            assert!(capability.verified_at.is_none());
        }
    }

    #[test]
    fn fake_camoufox_host_jsonl_launch_close_shutdown_is_bound_and_secret_free() {
        let root =
            std::env::temp_dir().join(format!("verisilo-camoufox-host-test-{}", Uuid::new_v4()));
        let artifact_root = root.join("artifacts");
        let profile_root = root.join("profiles");
        let state_root = root.join("state");
        fs::create_dir_all(&artifact_root).expect("fake artifact root");
        fs::create_dir_all(&profile_root).expect("fake profile root");
        fs::create_dir_all(&state_root).expect("fake state root");
        let browser_tree_manifest_path = root.join("browser-tree-manifest.json");
        let browser_tree_manifest = br#"{"schema":"verisilo-camoufox-browser-tree-manifest/v1","treeRootLabel":"fake-camoufox","fileCount":1,"totalBytes":1,"entries":[{"path":"camoufox.exe","size":1,"sha256":"4444444444444444444444444444444444444444444444444444444444444444"}]}"#;
        fs::write(&browser_tree_manifest_path, browser_tree_manifest).expect("fake browser tree");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/camoufox/fake-host-v1.py");
        let platform = if cfg!(target_os = "windows") {
            "windows-x64"
        } else {
            "linux-x64"
        };
        let binding = CamoufoxHostLaunch {
            protocol: super::CAMOUFOX_HOST_PROTOCOL.to_owned(),
            host_version: "0.1.0".to_owned(),
            platform: platform.to_owned(),
            artifact_id: "identity-m3-fake".to_owned(),
            artifact_file_sha256: "a".repeat(64),
            profile_id: "silo-22222222222242228222222222222222".to_owned(),
            browser_release: "v152.0.4-beta.28".to_owned(),
            browser_asset_sha256: "b".repeat(64),
            browser_tree_manifest_path: browser_tree_manifest_path.clone(),
            browser_tree_manifest_sha256:
                "f5788711bf5361124b6be6265c882b9e1652d9aad368a7091bbdda683631aac2".to_owned(),
            browser_proxy_server: None,
        };
        let python = if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        };
        let arguments = vec![
            OsString::from("-u"),
            OsString::from(script.to_string_lossy().into_owned()),
            OsString::from("--artifact-root"),
            OsString::from(artifact_root.to_string_lossy().into_owned()),
            OsString::from("--profile-root"),
            OsString::from(profile_root.to_string_lossy().into_owned()),
            OsString::from("--state-root"),
            OsString::from(state_root.to_string_lossy().into_owned()),
            OsString::from("--tree-manifest"),
            OsString::from(browser_tree_manifest_path.to_string_lossy().into_owned()),
        ];
        for argument in &arguments {
            let argument = argument.to_string_lossy();
            assert!(!argument.contains("seed"));
            assert!(!argument.contains("token"));
            assert!(!argument.contains("secret"));
            assert!(!argument.contains("proxy"));
        }
        let plan = EngineLaunchPlan {
            adapter: EngineDescriptor {
                contract_version: ENGINE_CONTRACT_VERSION,
                id: EngineAdapterId::Camoufox,
                adapter_version: "m3-test".to_owned(),
                engine_version: binding.browser_release.clone(),
                channel: EngineChannel::Experimental,
                browser_family: BrowserFamily::Firefox,
                platform: binding.platform.clone(),
                externally_packaged: true,
                emergency_disabled: false,
            },
            transport: EngineTransport::CamoufoxHostJsonlV1,
            executable_path: PathBuf::from(python),
            arguments: arguments
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect(),
            profile_directory: profile_root.join(&binding.profile_id),
            shell: false,
            capabilities: Vec::new(),
            identity_delivery: None,
            control: None,
            camoufox_host: Some(binding.clone()),
            package_verification: None,
        };
        let mut spawned = super::spawn_camoufox_host(&plan, &arguments).expect("fake Host launch");
        {
            let super::EngineRuntimeProtocol::CamoufoxHost(host) =
                spawned.runtime.as_mut().expect("Host runtime")
            else {
                panic!("fake Host did not use the Host transport");
            };
            assert_eq!(host.session_id, "11111111-1111-4111-8111-111111111111");
            assert_eq!(host.binding.artifact_id, binding.artifact_id);
            let close_value = host
                .transport
                .request(
                    "close",
                    serde_json::json!({ "sessionId": host.session_id }),
                    super::ENGINE_INITIAL_RECEIPT_TIMEOUT,
                )
                .expect("fake Host close");
            let close: super::CamoufoxHostCloseResult =
                serde_json::from_value(close_value).expect("close response");
            super::validate_camoufox_host_close(&close, &binding, &host.session_id)
                .expect("close binding");
            host.closed_confirmed = true;
            let shutdown_value = host
                .transport
                .request(
                    "shutdown",
                    serde_json::json!({}),
                    super::ENGINE_INITIAL_RECEIPT_TIMEOUT,
                )
                .expect("fake Host shutdown");
            let shutdown: super::CamoufoxHostShutdownResult =
                serde_json::from_value(shutdown_value).expect("shutdown response");
            super::validate_camoufox_host_shutdown(&shutdown).expect("shutdown self-check");
            for wire in &host.transport.wire_snapshot {
                let wire = String::from_utf8_lossy(wire);
                assert!(!wire.contains("seed"));
                assert!(!wire.contains("token"));
                assert!(!wire.contains("secret"));
                assert!(!wire.contains("proxy"));
            }
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let status = loop {
            if let Some(status) = spawned.child.try_wait().expect("wait fake Host") {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fake Host child did not exit"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(status.success());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fake_camoufox_required_proxy_receipt_is_exact_and_fail_closed() {
        let (root, spawned_plan, arguments) = fake_camoufox_host_fixture("proxy-required");
        let mut spawned =
            super::spawn_camoufox_host(&spawned_plan, &arguments).expect("required proxy Host");
        let wire = match spawned.runtime.as_ref().expect("Host runtime") {
            super::EngineRuntimeProtocol::CamoufoxHost(host) => {
                host.transport.wire_snapshot.clone()
            }
            super::EngineRuntimeProtocol::Native { .. } => panic!("unexpected native Host runtime"),
        };
        let wire = String::from_utf8_lossy(&wire.concat()).into_owned();
        assert!(wire.contains("\"browserProxyServer\":\"socks5://127.0.0.1:43127"));
        assert!(!wire.contains("credential") && !wire.contains("secret"));
        super::terminate_just_spawned_child(&mut spawned.child);
        let _ = fs::remove_dir_all(root);

        for mode in ["proxy-mismatch", "proxy-missing"] {
            let (root, plan, arguments) = fake_camoufox_host_fixture(mode);
            let error = match super::spawn_camoufox_host(&plan, &arguments) {
                Ok(_) => panic!("proxy receipt mismatch must fail closed"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("bound Artifact/profile"));
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn camoufox_runtime_manager_binds_required_proxy_through_exact_relay() {
        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind HTTP upstream");
        let upstream_port = upstream.local_addr().expect("HTTP upstream address").port();
        let credential_reference = Uuid::new_v4();
        let profile = NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Http,
            host: "127.0.0.1".to_owned(),
            port: upstream_port,
            bypass_list: Vec::new(),
            credential_reference: Some(credential_reference),
            external_mihomo: None,
        };
        let (root, mut runtime, silo) = fake_camoufox_runtime_launch_fixture("normal", profile);
        let managed_profiles = vec![PathBuf::from(&silo.profile_directory)];
        let username = "FP3-PROXY-USERNAME-SENTINEL";
        let password = "FP3-PROXY-PASSWORD-SENTINEL";
        let activation = runtime
            .launch(
                &silo,
                &managed_profiles,
                Some(ProxyAuthentication::new(
                    username.to_owned(),
                    password.to_owned(),
                )),
                None,
            )
            .expect("required proxy Host launch through relay");
        let relay_port = runtime
            .proxy_relay
            .as_ref()
            .expect("required proxy relay")
            .endpoint()
            .port;
        let expected_proxy = format!("socks5://127.0.0.1:{relay_port}");
        let host = match runtime.engine_runtime.as_ref().expect("Host runtime") {
            super::EngineRuntimeProtocol::CamoufoxHost(host) => host,
            super::EngineRuntimeProtocol::Native { .. } => panic!("unexpected native runtime"),
        };
        assert_eq!(
            host.binding.browser_proxy_server.as_deref(),
            Some(expected_proxy.as_str())
        );
        let wire = String::from_utf8_lossy(&host.transport.wire_snapshot.concat()).into_owned();
        assert!(wire.contains(&expected_proxy));
        assert!(!wire.contains(&format!("127.0.0.1:{upstream_port}")));
        assert!(!wire.contains(username) && !wire.contains(password));
        assert!(!wire.contains(&credential_reference.to_string()));

        let evidence = activation
            .network_evidence
            .as_ref()
            .expect("network evidence");
        assert!(activation
            .message
            .as_deref()
            .is_some_and(|message| message.contains("独立 runtime evidence")));
        assert_eq!(evidence.browser_routing, RuntimeEvidenceState::Applied);
        assert_eq!(evidence.endpoint, RuntimeEvidenceState::Reachable);
        assert_eq!(evidence.exit, RuntimeEvidenceState::NotRequested);
        assert_eq!(evidence.dns, RuntimeEvidenceState::NotRequested);
        assert_eq!(evidence.web_rtc, RuntimeEvidenceState::NotRequested);
        assert!(evidence.safeguards.is_empty());
        let activation_surface = serde_json::to_string(&activation).expect("activation JSON");
        assert!(!activation_surface.contains(username));
        assert!(!activation_surface.contains(password));

        runtime
            .stop_managed_camoufox(silo.id)
            .expect("stop required proxy Host");
        assert!(runtime.proxy_relay.is_none());
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, relay_port)),
            Duration::from_millis(200),
        )
        .is_err());
        drop(upstream);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_required_proxy_host_failures_revoke_relay_and_cannot_recover() {
        for mode in ["status-proxy-mismatch", "desktop-close-eof"] {
            let upstream =
                TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind required proxy upstream");
            let profile = NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Http,
                host: "127.0.0.1".to_owned(),
                port: upstream.local_addr().expect("upstream address").port(),
                bypass_list: Vec::new(),
                credential_reference: Some(Uuid::new_v4()),
                external_mihomo: None,
            };
            let (root, mut runtime, silo) = fake_camoufox_runtime_launch_fixture(mode, profile);
            runtime
                .launch(
                    &silo,
                    &[PathBuf::from(&silo.profile_directory)],
                    Some(ProxyAuthentication::new(
                        "runtime-user".to_owned(),
                        "runtime-password".to_owned(),
                    )),
                    None,
                )
                .expect("required proxy Host launch");
            let relay_port = runtime
                .proxy_relay
                .as_ref()
                .expect("required proxy relay")
                .endpoint()
                .port;

            if mode == "desktop-close-eof" {
                let error = runtime
                    .stop_managed_camoufox(silo.id)
                    .expect_err("close receipt failure must fail closed");
                assert!(error.to_string().to_ascii_lowercase().contains("eof"));
            }
            let failure = wait_for_runtime_state(&mut runtime, RuntimeState::VerificationFailed);
            assert_eq!(
                failure
                    .network_evidence
                    .as_ref()
                    .expect("network evidence")
                    .browser_routing,
                RuntimeEvidenceState::Failed
            );
            assert!(runtime.proxy_relay.is_none());
            assert!(runtime
                .health_context
                .as_ref()
                .is_some_and(|context| context.compromised));
            assert!(TcpStream::connect_timeout(
                &SocketAddr::from((Ipv4Addr::LOCALHOST, relay_port)),
                Duration::from_millis(200),
            )
            .is_err());

            let rechecked = runtime
                .recheck_active(&silo, None, None)
                .expect("compromised runtime remains inspectable");
            assert_eq!(rechecked.state, RuntimeState::VerificationFailed);
            assert_eq!(
                rechecked
                    .network_evidence
                    .as_ref()
                    .expect("rechecked network evidence")
                    .browser_routing,
                RuntimeEvidenceState::Failed
            );

            if mode == "status-proxy-mismatch" {
                runtime
                    .stop_managed_camoufox(silo.id)
                    .expect("close live fake Host after fail-closed assertion");
            } else {
                wait_for_camoufox_failure_cleanup(&mut runtime);
            }
            drop(upstream);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn camoufox_host_transport_rejects_browser_network_argv() {
        let (root, plan, mut arguments) = fake_camoufox_host_fixture("normal");
        arguments.push(OsString::from("--no-proxy-server"));
        let error = super::spawn_camoufox_host(&plan, &arguments)
            .err()
            .expect("Host transport must reject Chromium/browser argv");
        assert!(error
            .to_string()
            .contains("arguments must match the typed Host plan exactly"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_secret_sentinels_are_absent_from_launch_surfaces() {
        const VAULT_SEED_SENTINEL: &str = "VAULT-SEED-SENTINEL-9d6f";
        const ARTIFACT_SEED_SENTINEL: &str = "ARTIFACT-SEED-SENTINEL-3a81";
        const TOKEN_SENTINEL: &str = "TOKEN-SENTINEL-6c44";
        const PROXY_SECRET_SENTINEL: &str = "PROXY-SECRET-SENTINEL-2e70";
        let sentinels = [
            VAULT_SEED_SENTINEL,
            ARTIFACT_SEED_SENTINEL,
            TOKEN_SENTINEL,
            PROXY_SECRET_SENTINEL,
        ];
        let (root, plan, arguments) = fake_camoufox_host_fixture("normal");
        fs::write(
            root.join("artifacts").join("identity-m3-fake.json"),
            format!("{{\"artifactSeed\":\"{ARTIFACT_SEED_SENTINEL}\"}}"),
        )
        .expect("sentinel artifact fixture");
        let mut spawned = super::spawn_camoufox_host(&plan, &arguments).expect("fake Host launch");
        let mut wire = Vec::new();
        if let Some(super::EngineRuntimeProtocol::CamoufoxHost(host)) = spawned.runtime.as_mut() {
            wire = host.transport.wire_snapshot.clone();
        }
        let evidence = RuntimeEngineEvidence::configured(EngineAdapterId::Camoufox, true);
        let runtime_record = RuntimeRecord {
            silo_id: Uuid::new_v4(),
            pid: 1234,
            started_at: Utc::now(),
            last_seen_at: Utc::now(),
            state: RuntimeState::Running,
        };
        let runtime_record_path = root.join("runtime-record.json");
        write_runtime_record(&runtime_record_path, &runtime_record)
            .expect("persisted runtime record");
        let persisted_runtime_record =
            fs::read_to_string(&runtime_record_path).expect("read persisted runtime record");
        let surfaces = format!(
            "argv={} plan={} wire={} evidence={} record={}",
            arguments
                .iter()
                .map(|argument| argument.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
            serde_json::to_string(&plan).expect("plan JSON"),
            String::from_utf8_lossy(&wire.concat()),
            serde_json::to_string(&evidence).expect("evidence JSON"),
            persisted_runtime_record,
        );
        for sentinel in sentinels {
            assert!(
                !surfaces.contains(sentinel),
                "Camoufox launch surface leaked sentinel {sentinel}"
            );
        }
        super::terminate_just_spawned_child(&mut spawned.child);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_host_jsonl_framing_and_quarantine_fail_closed() {
        let mut empty = Cursor::new(Vec::<u8>::new());
        assert!(matches!(
            super::read_camoufox_host_frame(&mut empty),
            Ok(None)
        ));

        let mut partial = Cursor::new(br#"{"id":"m3-1"}"#.to_vec());
        assert!(super::read_camoufox_host_frame(&mut partial).is_err());

        let mut oversized = Cursor::new(vec![b'x'; super::MAX_CAMOUFOX_HOST_FRAME_BYTES + 1]);
        assert!(super::read_camoufox_host_frame(&mut oversized)
            .expect_err("oversized Host frame")
            .contains("32 KiB"));

        let mut duplicate = Cursor::new(
            br#"{"id":"m3-1","id":"m3-2"}
"#
            .to_vec(),
        );
        let frame = super::read_camoufox_host_frame(&mut duplicate)
            .expect("duplicate frame read")
            .expect("duplicate frame");
        assert!(
            crate::engine::strict_json_from_slice::<super::CamoufoxHostResponse>(&frame).is_err()
        );

        let binding = CamoufoxHostLaunch {
            protocol: super::CAMOUFOX_HOST_PROTOCOL.to_owned(),
            host_version: "0.1.0".to_owned(),
            platform: "windows-x64".to_owned(),
            artifact_id: "identity-m3-fake".to_owned(),
            artifact_file_sha256: "a".repeat(64),
            profile_id: "silo-fake".to_owned(),
            browser_release: "v152.0.4-beta.28".to_owned(),
            browser_asset_sha256: "b".repeat(64),
            browser_tree_manifest_path: PathBuf::from("C:\\verisilo\\tree.json"),
            browser_tree_manifest_sha256: "c".repeat(64),
            browser_proxy_server: None,
        };
        let close = |tree_exited: bool, quarantine: Option<serde_json::Value>| {
            super::CamoufoxHostCloseResult {
                session_id: "session-fake".to_owned(),
                state: "exited".to_owned(),
                exit_status: Some(0),
                exit_file_observed: Some(true),
                process_tree_exit: Some(serde_json::json!({"exited": tree_exited})),
                cookie_sqlite: None,
                context_close: Some(serde_json::json!({
                    "page": {"status": "not_present"},
                    "ctx": {"status": "success"}
                })),
                close_outcome: Some(serde_json::json!({
                    "status": "success",
                    "contextClose": {
                        "page": {"status": "not_present"},
                        "ctx": {"status": "success"}
                    },
                    "gracefulProcessExit": {"status": "success"},
                    "forcedJobCleanup": {"status": "not_needed"},
                    "sqliteEvidence": {"status": "unavailable"}
                })),
                quarantine,
                close_seconds: None,
            }
        };
        assert!(
            super::validate_camoufox_host_close(&close(true, None), &binding, "session-fake")
                .is_ok()
        );
        assert!(
            super::validate_camoufox_host_close(&close(false, None), &binding, "session-fake")
                .is_err()
        );
        assert!(super::validate_camoufox_host_close(
            &close(true, Some(serde_json::json!({"reason":"quarantined"}))),
            &binding,
            "session-fake"
        )
        .is_err());
    }

    #[test]
    fn fake_camoufox_host_timeout_and_early_exit_fail_closed() {
        let root =
            std::env::temp_dir().join(format!("verisilo-camoufox-host-failure-{}", Uuid::new_v4()));
        let artifact_root = root.join("artifacts");
        let profile_root = root.join("profiles");
        let state_root = root.join("state");
        fs::create_dir_all(&artifact_root).expect("fake artifact root");
        fs::create_dir_all(&profile_root).expect("fake profile root");
        fs::create_dir_all(&state_root).expect("fake state root");
        let browser_tree_manifest_path = root.join("browser-tree-manifest.json");
        fs::write(&browser_tree_manifest_path, b"{\"schema\":\"fake\"}\n")
            .expect("fake browser tree");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/camoufox/fake-host-v1.py");
        let python = if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        };

        for (mode, expected) in [("timeout", "timeout"), ("eof", "eof")] {
            let arguments = vec![
                std::ffi::OsString::from("-u"),
                std::ffi::OsString::from(script.to_string_lossy().into_owned()),
                std::ffi::OsString::from("--artifact-root"),
                std::ffi::OsString::from(artifact_root.to_string_lossy().into_owned()),
                std::ffi::OsString::from("--profile-root"),
                std::ffi::OsString::from(profile_root.to_string_lossy().into_owned()),
                std::ffi::OsString::from("--state-root"),
                std::ffi::OsString::from(state_root.to_string_lossy().into_owned()),
                std::ffi::OsString::from("--tree-manifest"),
                std::ffi::OsString::from(browser_tree_manifest_path.to_string_lossy().into_owned()),
                std::ffi::OsString::from("--mode"),
                std::ffi::OsString::from(mode),
            ];
            let mut child = std::process::Command::new(python)
                .args(&arguments)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("start fake Host failure mode");
            let mut transport = super::CamoufoxHostTransport::attach(&mut child)
                .expect("attach fake Host failure mode");
            let error = transport
                .request(
                    "hello",
                    serde_json::json!({}),
                    std::time::Duration::from_millis(50),
                )
                .expect_err("failure mode must not return a Host response");
            assert!(error.to_string().to_ascii_lowercase().contains(expected));
            drop(transport);
            super::terminate_just_spawned_child(&mut child);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fake_camoufox_host_frozen_failure_matrix_fails_closed() {
        let modes = [
            "wrong-protocol",
            "wrong-host-version",
            "wrong-platform",
            "wrong-release",
            "wrong-asset",
            "wrong-tree",
            "wrong-root",
            "wrong-tree-path",
            "unknown-field",
            "duplicate-field",
            "invalid-utf8",
            "oversized",
            "partial-frame",
            "wrong-id",
            "out-of-order-id",
            "duplicate-id",
            "launch-artifact-mismatch",
            "launch-sha-mismatch",
            "launch-profile-mismatch",
            "launch-unknown-field",
            "profile-in-use",
            "profile-quarantined",
            "quarantined",
        ];
        for mode in modes {
            let (root, plan, arguments) = fake_camoufox_host_fixture(mode);
            let result = super::spawn_camoufox_host(&plan, &arguments);
            assert!(
                result.is_err(),
                "fake Host mode {mode} unexpectedly entered Running"
            );
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn fake_camoufox_host_stdin_eof_closes_exact_child() {
        let (root, plan, arguments) = fake_camoufox_host_fixture("normal");
        let mut command = std::process::Command::new(&plan.executable_path);
        command.args(&arguments);
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::null());
        let mut child = command.spawn().expect("start fake Host for stdin EOF");
        let mut transport = super::CamoufoxHostTransport::attach(&mut child)
            .expect("attach fake Host for stdin EOF");
        transport
            .request("hello", serde_json::json!({}), Duration::from_secs(1))
            .expect("hello before stdin EOF");
        drop(transport);
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait fake Host after stdin EOF") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "fake Host did not close on stdin EOF"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fake_camoufox_runtime_manager_preserves_evidence_and_releases_exact_ownership() {
        let (root, mut runtime, silo_id) = fake_camoufox_runtime_manager("normal");
        let activation = runtime
            .stop_managed_camoufox(silo_id)
            .expect("fake Host RuntimeManager stop");
        assert_eq!(activation.state, RuntimeState::Stopped);
        assert_eq!(activation.active_silo_id, None);
        let evidence = activation.engine_evidence.expect("Host evidence");
        assert_eq!(evidence.configured_adapter, EngineAdapterId::Camoufox);
        assert_eq!(evidence.launched_adapter, Some(EngineAdapterId::Camoufox));
        assert_eq!(evidence.verified_adapter, None);
        assert_eq!(evidence.host_launch, RuntimeEvidenceState::Observed);
        assert_eq!(
            evidence.bootstrap_delivery,
            RuntimeEvidenceState::NotApplicable
        );
        assert_eq!(
            evidence.runtime_receipts,
            RuntimeEvidenceState::NotApplicable
        );
        assert_eq!(
            evidence.restore_receipt,
            RuntimeEvidenceState::NotApplicable
        );
        assert!(evidence.phase_receipts.is_empty());
        assert!(evidence.fallback_receipts.is_empty());
        assert!(runtime.child.is_none());
        assert!(runtime.engine_runtime.is_none());
        assert!(runtime.profile_lease.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fake_camoufox_runtime_manager_keeps_ownership_on_uncertain_tree_exit() {
        let (root, mut runtime, silo_id) = fake_camoufox_runtime_manager("tree-exit-false");
        let error = runtime
            .stop_managed_camoufox(silo_id)
            .expect_err("uncertain process tree exit must fail closed");
        assert!(error.to_string().contains("process tree"));
        assert_eq!(runtime.activation().state, RuntimeState::VerificationFailed);
        assert!(runtime.profile_lease.is_some());
        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn camoufox_runtime_manager_rejects_unsupported_network_before_spawn() {
        let profiles = [
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
        ];
        for network_profile in profiles {
            let fixed_proxy = matches!(network_profile, NetworkProfile::FixedProxy { .. });
            let silo = camoufox_test_silo(network_profile);
            let root = PathBuf::from(&silo.profile_directory);
            let mut runtime = RuntimeManager::open(&root);
            let managed_profiles = vec![root.clone()];
            let proxy_secret = "PROXY-SECRET-SENTINEL-FROM-VAULT";
            let proxy_authentication = fixed_proxy.then(|| {
                ProxyAuthentication::new(
                    "PROXY-USERNAME-SENTINEL-FROM-VAULT".to_owned(),
                    proxy_secret.to_owned(),
                )
            });
            let error = runtime
                .launch(&silo, &managed_profiles, proxy_authentication, None)
                .expect_err("unsupported Camoufox network policy must fail before spawn");
            assert!(error
                .to_string()
                .contains("only permits Direct(false) or required FixedProxy"));
            assert!(runtime.child.is_none());
            let activation = runtime.activation();
            assert_eq!(activation.state, RuntimeState::Failed);
            let surfaces = format!(
                "error={} activation={}",
                error,
                serde_json::to_string(&activation).expect("activation JSON")
            );
            assert!(!surfaces.contains(proxy_secret));
            assert!(!surfaces.contains("PROXY-USERNAME-SENTINEL-FROM-VAULT"));
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn fake_camoufox_runtime_manager_launch_stop_composition_is_bound_and_secret_free() {
        let (root, mut runtime, silo) = fake_camoufox_runtime_launch_fixture(
            "normal",
            NetworkProfile::Direct {
                proxy_required: false,
            },
        );
        let managed_profiles = vec![PathBuf::from(&silo.profile_directory)];
        let deriver_called = Arc::new(AtomicBool::new(false));
        let deriver = SentinelVaultDeriver {
            called: Arc::clone(&deriver_called),
        };
        let activation = runtime
            .launch_with_identity_deriver(&silo, &managed_profiles, None, None, Some(&deriver))
            .expect("RuntimeManager must launch the fake Host through composition");
        assert!(!deriver_called.load(Ordering::SeqCst));
        let activation_surface = serde_json::to_string(&activation).expect("activation JSON");
        assert!(!activation_surface.contains("TOKEN-SENTINEL-FROM-VAULT-DERIVER"));
        if let Some(super::EngineRuntimeProtocol::CamoufoxHost(host)) =
            runtime.engine_runtime.as_ref()
        {
            let wire_bytes = host.transport.wire_snapshot.concat();
            let wire_surface = String::from_utf8_lossy(&wire_bytes);
            assert!(!wire_surface.contains("TOKEN-SENTINEL-FROM-VAULT-DERIVER"));
        }
        assert_eq!(activation.state, RuntimeState::Running);
        assert_eq!(activation.active_silo_id, Some(silo.id));
        assert!(runtime.child.is_some());
        assert!(runtime.engine_runtime.is_some());
        assert!(runtime.profile_lease.is_some());
        let network = activation.network_evidence.expect("Host network evidence");
        assert_eq!(network.browser_routing, RuntimeEvidenceState::NotRequested);
        let evidence = activation.engine_evidence.expect("Host evidence");
        assert_eq!(evidence.launched_adapter, Some(EngineAdapterId::Camoufox));
        assert_eq!(evidence.verified_adapter, None);
        assert_eq!(
            evidence.package_verification,
            RuntimeEvidenceState::Verified
        );
        assert_eq!(evidence.host_launch, RuntimeEvidenceState::Observed);
        assert_eq!(
            evidence.bootstrap_delivery,
            RuntimeEvidenceState::NotApplicable
        );
        assert_eq!(
            evidence.runtime_receipts,
            RuntimeEvidenceState::NotApplicable
        );
        assert_eq!(
            evidence.restore_receipt,
            RuntimeEvidenceState::NotApplicable
        );
        assert!(evidence.phase_receipts.is_empty());
        assert!(evidence.fallback_receipts.is_empty());

        let stopped = runtime
            .stop_managed_camoufox(silo.id)
            .expect("RuntimeManager must close the fake Host through composition");
        assert_eq!(stopped.state, RuntimeState::Stopped);
        assert!(stopped.active_silo_id.is_none());
        assert!(runtime.child.is_none());
        assert!(runtime.engine_runtime.is_none());
        assert!(runtime.profile_lease.is_none());
        assert_eq!(
            runtime.record.as_ref().map(|record| record.state.clone()),
            Some(RuntimeState::Stopped)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fake_camoufox_runtime_manager_active_session_failures_keep_ownership() {
        for mode in ["active-session-eof", "active-session-crash"] {
            let (root, mut runtime, silo) = fake_camoufox_runtime_launch_fixture(
                mode,
                NetworkProfile::Direct {
                    proxy_required: false,
                },
            );
            let managed_profiles = vec![PathBuf::from(&silo.profile_directory)];
            runtime
                .launch(&silo, &managed_profiles, None, None)
                .expect("fake Host active session launch");
            let owned_pid = runtime.child.as_ref().expect("owned Host child").id();
            let failure = wait_for_camoufox_failure_cleanup(&mut runtime);
            assert_eq!(failure.active_silo_id, Some(silo.id));
            assert!(runtime.child.is_none());
            assert!(runtime.engine_runtime.is_none());
            assert!(runtime.profile_lease.is_some());
            assert_eq!(
                runtime.record.as_ref().map(|record| record.state.clone()),
                Some(RuntimeState::VerificationFailed)
            );
            assert!(!super::process_is_alive(owned_pid));
            let _ = fs::remove_dir_all(root);
        }

        let (root, mut runtime, silo) = fake_camoufox_runtime_launch_fixture(
            "desktop-close-eof",
            NetworkProfile::Direct {
                proxy_required: false,
            },
        );
        let managed_profiles = vec![PathBuf::from(&silo.profile_directory)];
        runtime
            .launch(&silo, &managed_profiles, None, None)
            .expect("fake Host desktop-close launch");
        let owned_pid = runtime.child.as_ref().expect("owned Host child").id();
        let error = runtime
            .stop_managed_camoufox(silo.id)
            .expect_err("desktop close EOF must fail closed");
        assert!(error.to_string().to_ascii_lowercase().contains("eof"));
        let failure = wait_for_camoufox_failure_cleanup(&mut runtime);
        assert_eq!(failure.active_silo_id, Some(silo.id));
        assert!(runtime.profile_lease.is_some());
        assert!(!super::process_is_alive(owned_pid));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fake_controlled_command_completes_bound_bootstrap_ack_e2e() {
        let _reservation = reserve_fake_controlled_engine_test();
        let (plan, envelope, arguments) = protocol_fixture();
        let mut spawned = super::spawn_engine_child(&plan, &arguments, Some(&envelope))
            .expect("fake controlled process ACK");
        let ack = spawned.bootstrap_ack.expect("controlled ACK");
        assert_eq!(ack.session_id, envelope.session_id);
        assert_eq!(ack.token_id, envelope.token.token_id);
        assert_eq!(
            ack.package.artifact_sha256,
            envelope.package.artifact_sha256
        );
        assert!(spawned.runtime.as_ref().is_some_and(|runtime| matches!(
            runtime,
            super::EngineRuntimeProtocol::Native { execution, .. }
                if execution.launch_evidence_complete()
        )));
        assert_eq!(
            spawned
                .runtime
                .as_ref()
                .expect("runtime protocol")
                .native_execution()
                .expect("native runtime")
                .phase_receipts
                .len(),
            3
        );
        super::terminate_just_spawned_child(&mut spawned.child);
    }

    fn runtime_manager_for_spawned(
        envelope: &EngineBootstrapEnvelope,
        spawned: super::SpawnedEngine,
    ) -> RuntimeManager {
        let mut engine_evidence = RuntimeEngineEvidence::configured(envelope.adapter_id, true);
        engine_evidence.launched_adapter = Some(envelope.adapter_id);
        engine_evidence.verified_adapter = Some(envelope.adapter_id);
        engine_evidence.package_verification = RuntimeEvidenceState::Verified;
        engine_evidence.bootstrap_delivery = RuntimeEvidenceState::Verified;
        engine_evidence.runtime_receipts = RuntimeEvidenceState::Verified;
        if let Some(runtime) = spawned.runtime.as_ref() {
            if let Some(execution) = runtime.native_execution() {
                engine_evidence.sync_control_execution(execution);
            }
        }
        RuntimeManager {
            child: Some(spawned.child),
            activation: Some(RuntimeActivation {
                active_silo_id: Some(envelope.silo_id),
                state: RuntimeState::Running,
                updated_at: Utc::now(),
                message: None,
                browser_verification: None,
                engine_evidence: Some(engine_evidence),
                network_evidence: None,
            }),
            engine_runtime: spawned.runtime,
            ..RuntimeManager::default()
        }
    }

    fn wait_for_runtime_state(
        runtime: &mut RuntimeManager,
        expected: RuntimeState,
    ) -> RuntimeActivation {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let activation = runtime.activation();
            if activation.state == expected {
                return activation;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "runtime did not reach {expected:?}; last activation was {activation:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn wait_for_camoufox_failure_cleanup(runtime: &mut RuntimeManager) -> RuntimeActivation {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let activation = runtime.activation();
            if activation.state == RuntimeState::VerificationFailed
                && runtime.child.is_none()
                && runtime.engine_runtime.is_none()
            {
                return activation;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Camoufox failure did not reap the exact Host child while retaining ownership; last activation was {activation:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn fake_controlled_engine_rejects_out_of_order_duplicate_missing_and_wrong_bound_receipts() {
        let _reservation = reserve_fake_controlled_engine_test();
        for behavior in [
            "out_of_order",
            "duplicate_sequence",
            "missing_capability",
            "wrong_binding",
            "oversized",
            "early_exit",
        ] {
            let (plan, envelope, arguments) = protocol_fixture_with_behavior(behavior);
            match super::spawn_engine_child(&plan, &arguments, Some(&envelope)) {
                Err(super::LauncherError::RuntimeReceipt(_)) => {}
                Err(error) => panic!("behavior {behavior} returned the wrong error: {error}"),
                Ok(mut spawned) => {
                    super::terminate_just_spawned_child(&mut spawned.child);
                    panic!("behavior {behavior} did not fail closed");
                }
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn initial_receipt_exit_with_inherited_stdout_is_bounded() {
        let _reservation = reserve_fake_controlled_engine_test();
        let (plan, envelope, arguments) =
            protocol_fixture_with_behavior("inherit_stdout_after_ack");
        let started = std::time::Instant::now();
        match super::spawn_engine_child(&plan, &arguments, Some(&envelope)) {
            Err(super::LauncherError::RuntimeReceipt(_)) => {}
            Err(error) => panic!("inherited stdout returned the wrong error: {error}"),
            Ok(mut spawned) => {
                super::terminate_just_spawned_child(&mut spawned.child);
                panic!("inherited stdout did not fail closed");
            }
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(8),
            "launch failure waited {elapsed:?} for a descendant that inherited stdout"
        );
    }

    #[test]
    fn runtime_accepts_bound_fallback_and_restore_before_normal_exit() {
        let _reservation = reserve_fake_controlled_engine_test();
        let (plan, envelope, arguments) = protocol_fixture_with_behavior("fallback_restore_exit");
        let spawned = super::spawn_engine_child(&plan, &arguments, Some(&envelope))
            .expect("verified initial runtime receipts");
        let mut runtime = runtime_manager_for_spawned(&envelope, spawned);

        let activation = wait_for_runtime_state(&mut runtime, RuntimeState::Stopped);
        let evidence = activation.engine_evidence.expect("engine evidence");
        assert_eq!(activation.state, RuntimeState::Stopped);
        assert_eq!(evidence.restore_receipt, RuntimeEvidenceState::Verified);
        assert_eq!(evidence.fallback_receipts.len(), 1);
        assert!(evidence.capabilities.iter().all(|capability| !matches!(
            capability.operation,
            EngineCapabilityOperation::Configured
                | EngineCapabilityOperation::Applied
                | EngineCapabilityOperation::Verified
        )));
    }

    #[test]
    fn forged_runtime_fallback_is_transactionally_rejected() {
        let _reservation = reserve_fake_controlled_engine_test();
        let (plan, envelope, arguments) = protocol_fixture_with_behavior("forged_fallback");
        let spawned = super::spawn_engine_child(&plan, &arguments, Some(&envelope))
            .expect("verified initial runtime receipts");
        let mut runtime = runtime_manager_for_spawned(&envelope, spawned);

        let activation = wait_for_runtime_state(&mut runtime, RuntimeState::VerificationFailed);
        let evidence = activation.engine_evidence.expect("engine evidence");
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert_eq!(evidence.runtime_receipts, RuntimeEvidenceState::Failed);
        assert!(evidence.fallback_receipts.is_empty());
        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
    }

    #[test]
    fn normal_exit_without_restore_never_claims_restoration() {
        let _reservation = reserve_fake_controlled_engine_test();
        let (plan, envelope, arguments) = protocol_fixture_with_behavior("no_restore_exit");
        let spawned = super::spawn_engine_child(&plan, &arguments, Some(&envelope))
            .expect("verified initial runtime receipts");
        let mut runtime = runtime_manager_for_spawned(&envelope, spawned);

        let activation = wait_for_runtime_state(&mut runtime, RuntimeState::Stopped);
        let evidence = activation.engine_evidence.expect("engine evidence");
        assert_eq!(activation.state, RuntimeState::Stopped);
        assert!(matches!(
            evidence.restore_receipt,
            RuntimeEvidenceState::Failed | RuntimeEvidenceState::Unavailable
        ));
        assert_ne!(evidence.restore_receipt, RuntimeEvidenceState::Verified);
        assert!(evidence.verified_adapter.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn ack_timeout_does_not_join_a_reader_held_by_inherited_stdout() {
        let (_plan, envelope, _arguments) = protocol_fixture();
        let started = std::time::Instant::now();
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 2 & exit 0"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn inherited stdout fixture");
        let result = super::await_engine_bootstrap_ack_with_timeout(
            &mut child,
            &envelope,
            std::time::Duration::from_millis(50),
        );
        assert!(matches!(result, Err(super::LauncherError::Bootstrap(_))));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn external_spawn_boundary_receives_fixed_plan_and_piped_bootstrap() {
        let plan = external_spawn_plan(PathBuf::from("/verified/package/bin/chromium.exe"));
        let arguments = plan
            .arguments
            .iter()
            .cloned()
            .map(OsString::from)
            .collect::<Vec<_>>();
        let bootstrap = "native-only-bootstrap".to_owned();
        let mut saw_fixed_plan = false;
        let child = spawn_engine_child_with(
            &plan,
            &arguments,
            Some(&bootstrap),
            |path, received_arguments, piped_stdin| {
                saw_fixed_plan = path == PathBuf::from("/verified/package/bin/chromium.exe")
                    && received_arguments == arguments
                    && piped_stdin
                    && received_arguments
                        .iter()
                        .all(|argument| argument != "native-only-bootstrap");
                std::process::Command::new("sh")
                    .args(["-c", "cat"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
            },
            |stdin, payload| {
                stdin.write_all(payload.as_bytes()).map_err(|error| {
                    super::LauncherError::Bootstrap(format!("test delivery failed: {error}"))
                })
            },
        )
        .expect("spawn boundary");
        let output = child.wait_with_output().expect("collect piped bootstrap");
        assert!(saw_fixed_plan);
        assert_eq!(output.stdout, bootstrap.as_bytes());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn failed_bootstrap_delivery_terminates_only_the_new_child() {
        let plan = external_spawn_plan(PathBuf::from("/verified/package/bin/chromium.exe"));
        let bootstrap = "native-only-bootstrap".to_owned();
        let mut spawned_pid = None;
        let result = spawn_engine_child_with(
            &plan,
            &[OsString::from("--verisilo-control-channel=stdio-v1")],
            Some(&bootstrap),
            |_path, _arguments, _piped_stdin| {
                let child = std::process::Command::new("sh")
                    .args(["-c", "sleep 30"])
                    .stdin(Stdio::piped())
                    .spawn()?;
                spawned_pid = Some(child.id());
                Ok(child)
            },
            |_stdin, _payload| {
                Err(super::LauncherError::Bootstrap(
                    "injected write failure".to_owned(),
                ))
            },
        );
        assert!(matches!(result, Err(super::LauncherError::Bootstrap(_))));
        assert!(spawned_pid.is_some_and(|pid| !super::process_is_alive(pid)));
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
    fn browser_routing_evidence_is_not_applied_for_direct_profiles() {
        let direct = NetworkProfile::Direct {
            proxy_required: false,
        };
        let mut direct_evidence = RuntimeNetworkEvidence::configured(&direct, false);
        super::mark_browser_routing_applied(&direct, &mut direct_evidence);
        assert_eq!(
            direct_evidence.browser_routing,
            RuntimeEvidenceState::NotRequested
        );

        let fixed_proxy = NetworkProfile::FixedProxy {
            proxy_required: false,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 1,
            bypass_list: Vec::new(),
            credential_reference: None,
            external_mihomo: None,
        };
        let mut fixed_proxy_evidence = RuntimeNetworkEvidence::configured(&fixed_proxy, false);
        super::mark_browser_routing_applied(&fixed_proxy, &mut fixed_proxy_evidence);
        assert_eq!(
            fixed_proxy_evidence.browser_routing,
            RuntimeEvidenceState::Applied
        );
    }

    #[test]
    fn extension_asserted_exit_is_observed_never_verified_and_rebind_resets_it() {
        assert_eq!(
            super::asserted_exit_state(true),
            crate::domain::RuntimeEvidenceState::Observed
        );
        assert_ne!(
            super::asserted_exit_state(true),
            crate::domain::RuntimeEvidenceState::Verified
        );
        let mut evidence = crate::domain::RuntimeNetworkEvidence::configured(
            &NetworkProfile::Direct {
                proxy_required: false,
            },
            false,
        );
        evidence.exit = crate::domain::RuntimeEvidenceState::Observed;
        evidence.dns = crate::domain::RuntimeEvidenceState::Unavailable;
        evidence.web_rtc = crate::domain::RuntimeEvidenceState::Unavailable;
        let prior_id = evidence.evidence_id;
        super::reset_network_observation(&mut evidence, Utc::now());
        assert_ne!(evidence.evidence_id, prior_id);
        assert_eq!(
            evidence.exit,
            crate::domain::RuntimeEvidenceState::NotRequested
        );
        assert_eq!(
            evidence.dns,
            crate::domain::RuntimeEvidenceState::NotRequested
        );
        assert_eq!(
            evidence.web_rtc,
            crate::domain::RuntimeEvidenceState::NotRequested
        );
    }

    #[test]
    fn http_authentication_requires_current_relay_receipt_and_browser_observation() {
        let checked_at = Utc::now() - ChronoDuration::seconds(30);

        let (mut accepted_runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        accepted_runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Accepted,
                checked_at - ChronoDuration::seconds(1),
                true,
            );
        let accepted = accepted_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, true,
        ));
        let serialized = serde_json::to_string(&accepted).expect("serialize accepted activation");
        for sensitive in [
            "alice",
            "secret",
            "Proxy-Authorization",
            "connectionId",
            "relayId",
        ] {
            assert!(
                !serialized.contains(sensitive),
                "runtime evidence must not expose relay receipts or credentials"
            );
        }
        let accepted_evidence = accepted
            .network_evidence
            .as_ref()
            .expect("accepted evidence");
        assert_eq!(
            accepted_evidence.authentication,
            RuntimeEvidenceState::Verified
        );
        assert!(matches!(
            accepted_evidence.authentication_provenance,
            RuntimeNetworkEvidenceProvenance::RelayObserved
        ));
        assert!(matches!(
            accepted_evidence.provenance,
            RuntimeNetworkEvidenceProvenance::ExtensionAsserted
        ));
        let accepted_evidence_id = accepted_evidence.evidence_id;
        let unrelated = accepted_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id,
            Uuid::new_v4(),
            Utc::now(),
            true,
        ));
        let unrelated_evidence = unrelated.network_evidence.expect("unrelated evidence");
        assert_eq!(unrelated_evidence.evidence_id, accepted_evidence_id);
        assert_eq!(
            unrelated_evidence.authentication,
            RuntimeEvidenceState::Verified,
            "evidence for another runtime must not mutate the current runtime"
        );
        let next_checked_at = Utc::now();
        let next_window = accepted_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id,
            runtime_id,
            next_checked_at,
            true,
        ));
        let next_window_evidence = next_window.network_evidence.expect("next-window evidence");
        assert_eq!(
            next_window_evidence.authentication,
            RuntimeEvidenceState::Configured,
            "a prior relay-observed receipt must not remain verified in a later window"
        );
        assert!(matches!(
            next_window_evidence.authentication_provenance,
            RuntimeNetworkEvidenceProvenance::DesktopControlPlane
        ));

        let (mut no_browser_runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        no_browser_runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Accepted,
                checked_at - ChronoDuration::seconds(1),
                true,
            );
        let no_browser = no_browser_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, false,
        ));
        assert_eq!(
            no_browser
                .network_evidence
                .expect("no-browser evidence")
                .authentication,
            RuntimeEvidenceState::Configured
        );
    }

    #[test]
    fn prior_http_407_is_cleared_by_a_new_window_and_fresh_acceptance_can_reverify() {
        let checked_at = Utc::now();
        let (mut runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        let evidence = runtime
            .activation
            .as_mut()
            .and_then(|activation| activation.network_evidence.as_mut())
            .expect("network evidence");
        evidence.authentication = RuntimeEvidenceState::Failed;
        evidence.authentication_provenance = RuntimeNetworkEvidenceProvenance::RelayObserved;
        runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Rejected,
                checked_at - ChronoDuration::seconds(30),
                false,
            );

        let next_window = runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, true,
        ));
        let next_evidence = next_window.network_evidence.expect("next-window evidence");
        assert_eq!(
            next_evidence.authentication,
            RuntimeEvidenceState::Configured,
            "an HTTP 407 outside the current window must not remain failed"
        );
        assert!(matches!(
            next_evidence.authentication_provenance,
            RuntimeNetworkEvidenceProvenance::DesktopControlPlane
        ));

        let accepted_at = Utc::now();
        runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Accepted,
                accepted_at - ChronoDuration::seconds(1),
                true,
            );
        let accepted = runtime.apply_network_evidence(&network_evidence_entry(
            silo.id,
            runtime_id,
            accepted_at,
            true,
        ));
        let accepted_evidence = accepted.network_evidence.expect("accepted evidence");
        assert_eq!(
            accepted_evidence.authentication,
            RuntimeEvidenceState::Verified
        );
        assert!(matches!(
            accepted_evidence.authentication_provenance,
            RuntimeNetworkEvidenceProvenance::RelayObserved
        ));
    }

    #[test]
    fn http_407_receipt_marks_authentication_failed() {
        let checked_at = Utc::now();
        let (mut runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Rejected,
                checked_at - ChronoDuration::seconds(1),
                false,
            );
        let activation = runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, false,
        ));
        let evidence = activation.network_evidence.expect("rejected evidence");
        assert_eq!(evidence.authentication, RuntimeEvidenceState::Failed);
        assert!(matches!(
            evidence.authentication_provenance,
            RuntimeNetworkEvidenceProvenance::RelayObserved
        ));
    }

    #[test]
    fn stale_wrong_runtime_and_missing_relay_receipts_do_not_upgrade_http_authentication() {
        let checked_at = Utc::now();

        let (mut stale_runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        stale_runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Accepted,
                checked_at - ChronoDuration::seconds(30),
                true,
            );
        let stale = stale_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, true,
        ));
        assert_eq!(
            stale
                .network_evidence
                .expect("stale evidence")
                .authentication,
            RuntimeEvidenceState::Configured
        );

        let (mut no_bytes_runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        no_bytes_runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Accepted,
                checked_at - ChronoDuration::seconds(1),
                false,
            );
        let no_bytes = no_bytes_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, true,
        ));
        assert_eq!(
            no_bytes
                .network_evidence
                .expect("no-bytes evidence")
                .authentication,
            RuntimeEvidenceState::Configured
        );

        let (mut wrong_runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        wrong_runtime
            .proxy_relay
            .as_ref()
            .expect("relay")
            .inject_authentication_receipt_for_test(
                RelayAuthenticationEvidence::Accepted,
                checked_at - ChronoDuration::seconds(1),
                true,
            );
        let wrong = wrong_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id,
            Uuid::new_v4(),
            checked_at,
            true,
        ));
        let wrong_evidence = wrong.network_evidence.expect("wrong-runtime evidence");
        assert_eq!(wrong_evidence.runtime_id, runtime_id);
        assert_eq!(
            wrong_evidence.authentication,
            RuntimeEvidenceState::Configured
        );
        assert!(matches!(
            wrong_evidence.provenance,
            RuntimeNetworkEvidenceProvenance::DesktopControlPlane
        ));

        let (mut missing_runtime, silo, runtime_id, _upstream) = http_runtime_manager();
        let missing = missing_runtime.apply_network_evidence(&network_evidence_entry(
            silo.id, runtime_id, checked_at, true,
        ));
        assert_eq!(
            missing
                .network_evidence
                .expect("missing-receipt evidence")
                .authentication,
            RuntimeEvidenceState::Configured
        );
    }

    #[test]
    fn companion_http_windows_do_not_downgrade_socks5_preflight_verification() {
        let profile = NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: crate::domain::ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 1080,
            bypass_list: Vec::new(),
            credential_reference: Some(Uuid::new_v4()),
            external_mihomo: None,
        };
        let mut evidence = RuntimeNetworkEvidence::configured(&profile, true);
        evidence.authentication = RuntimeEvidenceState::Verified;
        assert!(!super::is_window_scoped_http_authentication(
            &profile, &evidence
        ));
        assert_eq!(
            evidence.authentication,
            RuntimeEvidenceState::Verified,
            "SOCKS5 authentication remains verified by its preflight handshake"
        );
        assert!(matches!(
            evidence.authentication_provenance,
            RuntimeNetworkEvidenceProvenance::DesktopControlPlane
        ));
    }

    #[test]
    fn optional_managed_relay_failure_closes_relay_and_clears_verified_evidence() {
        let (mut runtime, _silo, _runtime_id, _upstream) = http_runtime_manager();
        if let Some(context) = runtime.health_context.as_mut() {
            if let NetworkProfile::FixedProxy { proxy_required, .. } =
                &mut context.silo.network_profile
            {
                *proxy_required = false;
            }
        }
        if let Some(evidence) = runtime
            .activation
            .as_mut()
            .and_then(|activation| activation.network_evidence.as_mut())
        {
            evidence.authentication = RuntimeEvidenceState::Verified;
            evidence.authentication_provenance = RuntimeNetworkEvidenceProvenance::RelayObserved;
            evidence.exit = RuntimeEvidenceState::Observed;
        }

        runtime.fail_closed_network_path(
            "optional endpoint drift".to_owned(),
            super::RuntimeNetworkFailure::Endpoint,
        );

        let activation = runtime.activation.expect("activation remains available");
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(runtime.proxy_relay.is_none());
        assert!(activation
            .message
            .as_deref()
            .is_some_and(|message| message.contains("非 required")));
        let evidence = activation.network_evidence.expect("network evidence");
        assert_ne!(evidence.authentication, RuntimeEvidenceState::Verified);
        assert_ne!(evidence.exit, RuntimeEvidenceState::Observed);
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

    #[cfg(target_os = "windows")]
    #[test]
    fn stock_child_exit_waits_for_chromium_profile_release_before_stopping() {
        let silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });
        let profile = std::path::PathBuf::from(&silo.profile_directory);
        let profile_lease = crate::vault::BrowserProfileLease::acquire_for_runtime(
            &silo.all_engine_profile_directories(),
            &profile,
        )
        .expect("acquire runtime Profile lease");
        let chromium_lock = profile.join(crate::vault::CHROMIUM_PROFILE_SENTINEL_NAMES[0]);
        fs::write(&chromium_lock, []).expect("create Chromium Profile sentinel");

        let child = std::process::Command::new("cmd.exe")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("spawn completed stock child fixture");
        let pid = child.id();
        let evidence = RuntimeNetworkEvidence::configured(&silo.network_profile, false);
        let runtime_id = evidence.runtime_id;
        let silo_id = silo.id;
        let silo_for_recheck = silo.clone();
        let record_path = profile.join("runtime").join("browser-session.json");
        let now = Utc::now();
        let record = RuntimeRecord {
            silo_id,
            pid,
            started_at: now,
            last_seen_at: now,
            state: RuntimeState::Running,
        };
        write_runtime_record(&record_path, &record).expect("persist running stock record");
        let mut runtime = RuntimeManager {
            child: Some(child),
            activation: Some(RuntimeActivation {
                active_silo_id: Some(silo_id),
                state: RuntimeState::Running,
                updated_at: Utc::now(),
                message: None,
                browser_verification: None,
                engine_evidence: None,
                network_evidence: Some(evidence),
            }),
            health_context: Some(RuntimeHealthContext {
                silo,
                runtime_id,
                compromised: false,
                mihomo_authentication: None,
                mihomo_guard: None,
            }),
            profile_lease: Some(profile_lease),
            record_path: Some(record_path.clone()),
            record: Some(record),
            ..RuntimeManager::default()
        };
        while runtime
            .child
            .as_mut()
            .expect("fixture child")
            .try_wait()
            .expect("query fixture child")
            .is_none()
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let waiting = runtime.activation_for_watchdog(
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(waiting.state, RuntimeState::RecoveryRequired);
        assert_eq!(waiting.active_silo_id, Some(silo_id));
        assert!(runtime.profile_lease.is_some());
        assert_eq!(
            super::read_runtime_record(&record_path)
                .expect("read pending stock record")
                .expect("pending stock record exists")
                .state,
            RuntimeState::RecoveryRequired
        );

        let rechecked = runtime
            .recheck_active(&silo_for_recheck, None, None)
            .expect("pending stock Profile release remains recheckable");
        assert_eq!(rechecked.state, RuntimeState::RecoveryRequired);
        assert_eq!(rechecked.active_silo_id, Some(silo_id));
        assert!(runtime.profile_lease.is_some());

        fs::remove_file(&chromium_lock).expect("release Chromium Profile sentinel");
        let stopped = runtime.activation();
        assert_eq!(stopped.state, RuntimeState::Stopped);
        assert!(stopped.active_silo_id.is_none());
        assert!(runtime.profile_lease.is_none());
        assert_eq!(
            super::read_runtime_record(&record_path)
                .expect("read stopped stock record")
                .expect("stopped stock record exists")
                .state,
            RuntimeState::Stopped
        );

        let completed_child = std::process::Command::new("cmd.exe")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("spawn second completed stock child fixture");
        let second_evidence =
            RuntimeNetworkEvidence::configured(&silo_for_recheck.network_profile, false);
        runtime.child = Some(completed_child);
        runtime.activation = Some(RuntimeActivation {
            active_silo_id: Some(silo_id),
            state: RuntimeState::Running,
            updated_at: Utc::now(),
            message: None,
            browser_verification: None,
            engine_evidence: None,
            network_evidence: Some(second_evidence.clone()),
        });
        runtime.health_context = Some(RuntimeHealthContext {
            silo: silo_for_recheck.clone(),
            runtime_id: second_evidence.runtime_id,
            compromised: false,
            mihomo_authentication: None,
            mihomo_guard: None,
        });
        runtime.profile_lease = Some(
            crate::vault::BrowserProfileLease::acquire_for_runtime(
                &silo_for_recheck.all_engine_profile_directories(),
                &profile,
            )
            .expect("reacquire runtime Profile lease"),
        );
        runtime.persist_current_record(RuntimeState::Running);
        while runtime
            .child
            .as_mut()
            .expect("second fixture child")
            .try_wait()
            .expect("query second fixture child")
            .is_none()
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let stopped_by_watchdog = runtime.activation_for_watchdog(
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(stopped_by_watchdog.state, RuntimeState::Stopped);
        assert!(stopped_by_watchdog.active_silo_id.is_none());
        assert_eq!(
            super::read_runtime_record(&record_path)
                .expect("read watchdog-stopped stock record")
                .expect("watchdog-stopped stock record exists")
                .state,
            RuntimeState::Stopped
        );

        drop(runtime);
        fs::remove_dir_all(profile).expect("remove stock exit fixture");
    }

    #[test]
    fn unavailable_controlled_engine_fails_closed_without_stock_fallback() {
        let mut runtime = RuntimeManager::default();
        let mut silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });
        let (_plan, envelope, _arguments) = protocol_fixture();
        silo.engine = crate::engine::SiloEngineConfig::ControlledChromium {
            identity_template: envelope.identity,
            fallback_rules: Vec::new(),
        };

        assert!(runtime
            .launch(&silo, &silo.all_engine_profile_directories(), None, None,)
            .is_err());
        let activation = runtime.activation();
        assert!(activation.active_silo_id.is_none());
        assert!(matches!(
            activation.state,
            RuntimeState::VerificationFailed | RuntimeState::Failed
        ));
        let engine = activation.engine_evidence.expect("engine evidence");
        assert_eq!(
            engine.configured_adapter,
            EngineAdapterId::ControlledChromium
        );
        assert!(engine.launched_adapter.is_none());
        assert!(engine.verified_adapter.is_none());
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
        fs::write(
            locked_directory.join(crate::vault::CHROMIUM_PROFILE_SENTINEL_NAMES[0]),
            [],
        )
        .expect("create profile lock");

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

    #[test]
    fn runtime_record_contains_only_the_minimal_non_sensitive_fields() {
        let root =
            std::env::temp_dir().join(format!("verisilo-runtime-record-test-{}", Uuid::new_v4()));
        let path = root.join("runtime").join("browser-session.json");
        let now = Utc::now();
        let record = RuntimeRecord {
            silo_id: Uuid::new_v4(),
            pid: std::process::id(),
            started_at: now,
            last_seen_at: now,
            state: RuntimeState::Running,
        };
        write_runtime_record(&path, &record).expect("persist runtime record");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read runtime record"))
                .expect("parse runtime record");
        let mut keys = value
            .as_object()
            .expect("record object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["lastSeenAt", "pid", "siloId", "startedAt", "state"]
        );
        let raw = value.to_string().to_ascii_lowercase();
        assert!(!raw.contains("argument"));
        assert!(!raw.contains("credential"));
        fs::remove_dir_all(root).expect("remove runtime record fixture");
    }

    #[test]
    fn restart_recovery_combines_pid_and_profile_lock_without_deleting_the_lock() {
        let root =
            std::env::temp_dir().join(format!("verisilo-runtime-recovery-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create recovery root");
        let silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });
        let profile = std::path::PathBuf::from(&silo.profile_directory);
        let lock = profile.join(crate::vault::CHROMIUM_PROFILE_SENTINEL_NAMES[0]);
        fs::write(&lock, []).expect("create browser lock");
        let now = Utc::now();
        write_runtime_record(
            &root.join("runtime").join("browser-session.json"),
            &RuntimeRecord {
                silo_id: silo.id,
                pid: std::process::id(),
                started_at: now,
                last_seen_at: now,
                state: RuntimeState::Running,
            },
        )
        .expect("write recovery record");

        let mut runtime = RuntimeManager::open(&root);
        let activation = runtime.reconcile_persisted(&silo, None);
        assert_eq!(activation.active_silo_id, Some(silo.id));
        assert_eq!(activation.state, RuntimeState::Running);
        assert!(lock.exists(), "recovery must not delete the browser lock");

        fs::remove_dir_all(root).expect("remove recovery record root");
        fs::remove_dir_all(profile).expect("remove recovery Profile fixture");
    }

    #[test]
    fn vault_restore_requires_a_proven_quiescent_runtime_state() {
        for state in [RuntimeState::Idle, RuntimeState::Stopped] {
            let mut activation = RuntimeActivation::idle();
            activation.state = state;
            assert!(runtime_allows_vault_restore(&activation));
        }

        for state in [
            RuntimeState::Preflight,
            RuntimeState::Launching,
            RuntimeState::Running,
            RuntimeState::VerificationFailed,
            RuntimeState::RecoveryRequired,
            RuntimeState::Failed,
        ] {
            let mut activation = RuntimeActivation::idle();
            activation.state = state;
            assert!(!runtime_allows_vault_restore(&activation));
        }

        let mut active = RuntimeActivation::idle();
        active.state = RuntimeState::Stopped;
        active.active_silo_id = Some(Uuid::new_v4());
        assert!(!runtime_allows_vault_restore(&active));

        let mut idle = RuntimeManager::default();
        assert!(idle.prepare_for_vault_restore().is_some());

        let now = Utc::now();
        let mut unresolved = RuntimeManager {
            activation: Some(RuntimeActivation::idle()),
            record: Some(RuntimeRecord {
                silo_id: Uuid::new_v4(),
                pid: u32::MAX,
                started_at: now,
                last_seen_at: now,
                state: RuntimeState::Failed,
            }),
            ..RuntimeManager::default()
        };
        assert!(unresolved.prepare_for_vault_restore().is_none());

        let profile = std::env::temp_dir().join(format!(
            "verisilo-restore-profile-lock-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&profile).expect("create managed profile fixture");
        assert!(managed_profiles_are_quiescent_for_vault_restore(&[
            profile.clone()
        ]));
        fs::write(
            profile.join(crate::vault::CHROMIUM_PROFILE_SENTINEL_NAMES[0]),
            [],
        )
        .expect("create active profile lock");
        assert!(!managed_profiles_are_quiescent_for_vault_restore(&[
            profile.clone()
        ]));
        fs::remove_dir_all(profile).expect("remove managed profile fixture");
    }

    #[test]
    fn successful_vault_restore_starts_a_clean_runtime_ownership_epoch() {
        let root = std::env::temp_dir().join(format!(
            "verisilo-restore-runtime-reset-test-{}",
            Uuid::new_v4()
        ));
        let record_path = root.join("runtime").join("browser-session.json");
        let now = Utc::now();
        let record = RuntimeRecord {
            silo_id: Uuid::new_v4(),
            pid: u32::MAX,
            started_at: now,
            last_seen_at: now,
            state: RuntimeState::Stopped,
        };
        write_runtime_record(&record_path, &record).expect("persist stale stopped record");
        let silo = test_silo(NetworkProfile::Direct {
            proxy_required: false,
        });
        let mut stale_activation = RuntimeActivation::idle();
        stale_activation.state = RuntimeState::Stopped;
        stale_activation.message = Some("old Vault runtime detail".to_owned());
        stale_activation.browser_verification =
            Some(crate::domain::verify_browser_descriptor(&silo.browser));
        stale_activation.engine_evidence = Some(RuntimeEngineEvidence::configured(
            EngineAdapterId::StockChrome,
            false,
        ));
        stale_activation.network_evidence = Some(RuntimeNetworkEvidence::configured(
            &silo.network_profile,
            false,
        ));
        let mut runtime = RuntimeManager {
            activation: Some(stale_activation),
            record_path: Some(record_path.clone()),
            record: Some(record),
            ..RuntimeManager::default()
        };
        let preparation = runtime
            .prepare_for_vault_restore()
            .expect("stopped runtime is quiescent");

        let activation = runtime.complete_successful_vault_restore(preparation);

        assert_eq!(activation.state, RuntimeState::Idle);
        assert!(activation.active_silo_id.is_none());
        assert!(activation.message.is_none());
        assert!(activation.browser_verification.is_none());
        assert!(activation.engine_evidence.is_none());
        assert!(activation.network_evidence.is_none());
        assert!(runtime.record.is_none());
        assert!(!record_path.exists());

        fs::remove_dir_all(root).expect("remove runtime reset fixture");
        fs::remove_dir_all(silo.profile_directory).expect("remove stale Silo profile fixture");
    }

    #[test]
    fn required_proxy_recovery_is_verification_failed_after_relay_loss() {
        let root = std::env::temp_dir().join(format!(
            "verisilo-runtime-failclosed-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create recovery root");
        let silo = test_silo(NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: crate::domain::ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 9,
            bypass_list: Vec::new(),
            credential_reference: None,
            external_mihomo: None,
        });
        let profile = std::path::PathBuf::from(&silo.profile_directory);
        let lock = profile.join(crate::vault::CHROMIUM_PROFILE_SENTINEL_NAMES[0]);
        fs::write(&lock, []).expect("create browser lock");
        let now = Utc::now();
        write_runtime_record(
            &root.join("runtime").join("browser-session.json"),
            &RuntimeRecord {
                silo_id: silo.id,
                pid: std::process::id(),
                started_at: now,
                last_seen_at: now,
                state: RuntimeState::Running,
            },
        )
        .expect("write recovery record");

        let mut runtime = RuntimeManager::open(&root);
        let activation = runtime.reconcile_persisted(&silo, None);
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(activation
            .message
            .as_deref()
            .is_some_and(|message| message.contains("fail-closed")));
        assert!(lock.exists());

        fs::remove_dir_all(root).expect("remove recovery record root");
        fs::remove_dir_all(profile).expect("remove recovery Profile fixture");
    }

    #[cfg(unix)]
    #[test]
    fn mihomo_node_drift_atomically_closes_exact_relay_and_latches_recovery() {
        let (mut runtime, silo, old_port, _upstream, controller_worker, upstream_worker) =
            mihomo_runtime_manager(MihomoRuntimeFixture::NodeDrift);
        let launch_arguments = silo
            .network_profile
            .launch_arguments_with_proxy_override(Some(("127.0.0.1", old_port)))
            .iter()
            .map(|argument| argument.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(launch_arguments
            .iter()
            .any(|argument| argument == &format!("--proxy-server=socks5://127.0.0.1:{old_port}")));
        assert!(launch_arguments
            .iter()
            .all(|argument| !argument.to_ascii_lowercase().contains("direct://")));

        let activation = runtime.activation();
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(runtime.proxy_relay.is_none());
        assert!(runtime
            .health_context
            .as_ref()
            .is_some_and(|context| context.compromised));
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, old_port)),
            Duration::from_millis(200),
        )
        .is_err());
        let evidence = activation
            .network_evidence
            .expect("failed network evidence");
        assert_eq!(evidence.controller_binding, RuntimeEvidenceState::Failed);
        assert_eq!(evidence.browser_routing, RuntimeEvidenceState::Failed);
        assert_ne!(evidence.exit, RuntimeEvidenceState::Observed);
        assert_ne!(evidence.exit, RuntimeEvidenceState::Verified);

        let refreshed = runtime.activation();
        assert_eq!(refreshed.state, RuntimeState::VerificationFailed);
        assert!(runtime.proxy_relay.is_none());
        let explicitly_rechecked = runtime
            .recheck_active(&silo, None, None)
            .expect("explicit recheck reports the latched state");
        assert_eq!(explicitly_rechecked.state, RuntimeState::VerificationFailed);
        assert!(explicitly_rechecked
            .message
            .as_deref()
            .is_some_and(|message| message.contains("不会重开旧端口")));
        assert!(runtime.rebind_active_mihomo(&silo, None, None).is_err());
        assert!(runtime.child.as_mut().is_some_and(|child| child
            .try_wait()
            .ok()
            .flatten()
            .is_none()));

        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
        controller_worker.join().expect("Controller worker exits");
        upstream_worker
            .join()
            .expect("upstream health worker exits");
    }

    #[cfg(unix)]
    #[test]
    fn mihomo_controller_process_exit_closes_relay_without_killing_owned_browser() {
        let (mut runtime, _silo, old_port, _upstream, controller_worker, upstream_worker) =
            mihomo_runtime_manager(MihomoRuntimeFixture::ControllerExit);
        controller_worker
            .join()
            .expect("Controller fixture exits after launch guard");

        let activation = runtime.activation();
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(activation
            .message
            .as_deref()
            .is_some_and(|message| message.contains("Controller")));
        assert!(runtime.proxy_relay.is_none());
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, old_port)),
            Duration::from_millis(200),
        )
        .is_err());
        assert!(runtime.child.as_mut().is_some_and(|child| child
            .try_wait()
            .ok()
            .flatten()
            .is_none()));

        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
        upstream_worker
            .join()
            .expect("upstream health worker exits");
    }

    #[cfg(unix)]
    #[test]
    fn mihomo_config_drift_closes_old_listener_and_clears_configuration_evidence() {
        let (mut runtime, _silo, old_port, _upstream, controller_worker, upstream_worker) =
            mihomo_runtime_manager(MihomoRuntimeFixture::ConfigDrift);

        let activation = runtime.activation();
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(runtime.proxy_relay.is_none());
        let evidence = activation.network_evidence.expect("failed config evidence");
        assert_eq!(evidence.configuration, RuntimeEvidenceState::Failed);
        assert_ne!(evidence.exit, RuntimeEvidenceState::Observed);
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, old_port)),
            Duration::from_millis(200),
        )
        .is_err());

        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
        controller_worker.join().expect("Controller worker exits");
        upstream_worker
            .join()
            .expect("upstream health worker exits");
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_probe_timeout_closes_only_the_exact_runtime_relay() {
        let upstream =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind slow required SOCKS upstream");
        let upstream_port = upstream.local_addr().expect("upstream address").port();
        let (greeting_seen_tx, greeting_seen) = mpsc::channel();
        let (release_tx, release) = mpsc::channel();
        let upstream_worker = thread::spawn(move || {
            let (mut stream, _) = upstream.accept().expect("accept health probe");
            let mut greeting = [0_u8; 3];
            stream.read_exact(&mut greeting).expect("read greeting");
            assert_eq!(greeting, [5, 1, 2]);
            greeting_seen_tx.send(()).expect("report greeting");
            stream.write_all(&[5]).expect("write partial method reply");
            let _ = release.recv_timeout(Duration::from_secs(2));
        });
        let silo = test_silo(NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: upstream_port,
            bypass_list: Vec::new(),
            credential_reference: Some(Uuid::new_v4()),
            external_mihomo: None,
        });
        let mut evidence = RuntimeNetworkEvidence::configured(&silo.network_profile, true);
        let runtime_id = evidence.runtime_id;
        evidence.endpoint = RuntimeEvidenceState::Reachable;
        evidence.browser_routing = RuntimeEvidenceState::Applied;
        let relay = ProxyRelay::start(
            &silo.network_profile,
            silo.id,
            runtime_id,
            Some(ProxyAuthentication::new(
                "alice".to_owned(),
                "secret".to_owned(),
            )),
        )
        .expect("start exact runtime relay");
        let old_port = relay.endpoint().port;
        let child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .spawn()
            .expect("spawn owned browser fixture");
        let mut runtime = RuntimeManager {
            child: Some(child),
            activation: Some(RuntimeActivation {
                active_silo_id: Some(silo.id),
                state: RuntimeState::Running,
                updated_at: Utc::now(),
                message: None,
                browser_verification: None,
                engine_evidence: None,
                network_evidence: Some(evidence),
            }),
            proxy_relay: Some(relay),
            health_context: Some(RuntimeHealthContext {
                silo,
                runtime_id,
                compromised: false,
                mihomo_authentication: None,
                mihomo_guard: None,
            }),
            ..RuntimeManager::default()
        };
        let cancelled = AtomicBool::new(false);
        let started_at = Instant::now();

        let activation =
            runtime.activation_for_watchdog(started_at + Duration::from_millis(150), &cancelled);

        greeting_seen
            .recv_timeout(Duration::from_secs(1))
            .expect("watchdog reached the authenticated SOCKS handshake");
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(runtime.proxy_relay.is_none());
        assert!(runtime
            .health_context
            .as_ref()
            .is_some_and(|context| context.compromised && context.runtime_id == runtime_id));
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, old_port)),
            Duration::from_millis(200),
        )
        .is_err());
        assert!(runtime.child.as_mut().is_some_and(|child| child
            .try_wait()
            .ok()
            .flatten()
            .is_none()));
        assert!(started_at.elapsed() < Duration::from_secs(1));

        let _ = release_tx.send(());
        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
        upstream_worker.join().expect("slow upstream exits");
    }

    #[cfg(unix)]
    #[test]
    fn native_watchdog_detects_mihomo_drift_without_a_desktop_status_poll() {
        let (runtime, _silo, old_port, _upstream, controller_worker, upstream_worker) =
            mihomo_runtime_manager(MihomoRuntimeFixture::NodeDrift);
        let runtime = Arc::new(Mutex::new(runtime));
        let mut watchdog = RuntimeWatchdog::start(&runtime).expect("start native watchdog");

        watchdog.tick_and_wait();

        watchdog.shutdown();
        let mut runtime = runtime.lock().expect("runtime after watchdog tick");
        let activation = runtime
            .activation
            .as_ref()
            .expect("watchdog preserves terminal activation");
        assert_eq!(activation.state, RuntimeState::VerificationFailed);
        assert!(runtime.proxy_relay.is_none());
        assert!(runtime
            .health_context
            .as_ref()
            .is_some_and(|context| context.compromised));
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, old_port)),
            Duration::from_millis(200),
        )
        .is_err());
        assert!(runtime.child.as_mut().is_some_and(|child| child
            .try_wait()
            .ok()
            .flatten()
            .is_none()));

        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
        drop(runtime);
        controller_worker.join().expect("Controller worker exits");
        upstream_worker
            .join()
            .expect("upstream health worker exits");
    }

    #[cfg(unix)]
    #[test]
    fn native_watchdog_preserves_a_healthy_required_fixed_proxy() {
        let (mut runtime, _silo, _runtime_id, _upstream) = http_runtime_manager();
        runtime.child = Some(
            std::process::Command::new("sh")
                .args(["-c", "sleep 30"])
                .spawn()
                .expect("spawn owned browser fixture"),
        );
        let runtime = Arc::new(Mutex::new(runtime));
        let mut watchdog = RuntimeWatchdog::start(&runtime).expect("start native watchdog");

        watchdog.tick_and_wait();

        watchdog.shutdown();
        let mut runtime = runtime.lock().expect("runtime after watchdog tick");
        assert!(runtime
            .activation
            .as_ref()
            .is_some_and(|activation| activation.state == RuntimeState::Running));
        assert!(runtime.proxy_relay.is_some());
        assert!(runtime
            .health_context
            .as_ref()
            .is_some_and(|context| !context.compromised));

        if let Some(child) = runtime.child.as_mut() {
            super::terminate_just_spawned_child(child);
        }
        runtime.child = None;
    }

    #[test]
    fn vault_lock_revokes_runtime_mihomo_secret_without_stopping_a_process() {
        let mut runtime = RuntimeManager::default();
        let runtime_id = Uuid::new_v4();
        runtime.health_context = Some(RuntimeHealthContext {
            silo: test_silo(NetworkProfile::Direct {
                proxy_required: false,
            }),
            runtime_id,
            compromised: false,
            mihomo_authentication: Some(MihomoControllerAuthentication::new(
                "controller-secret".to_owned(),
            )),
            mihomo_guard: None,
        });
        runtime.activation = Some(RuntimeActivation {
            active_silo_id: None,
            state: RuntimeState::Idle,
            updated_at: Utc::now(),
            message: None,
            browser_verification: None,
            engine_evidence: None,
            network_evidence: None,
        });

        runtime.revoke_secrets_for_vault_lock();

        assert!(runtime
            .health_context
            .as_ref()
            .is_some_and(|context| context.mihomo_authentication.is_none()));
        assert!(runtime.child.is_none());
    }
}
