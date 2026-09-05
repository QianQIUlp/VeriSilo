use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use uuid::Uuid;

use crate::{
    domain::{hide_windows_console, ExternalMihomoBinding},
    vault::MihomoControllerAuthentication,
};

const CONTROLLER_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROLLER_IO_POLL: Duration = Duration::from_millis(100);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const COMMON_MIXED_PORTS: [u16; 4] = [7897, 7890, 7891, 7880];
const COMMON_CONTROLLER_PORTS: [u16; 3] = [9097, 9090, 9091];
const LOCAL_PROBE_TIMEOUT: Duration = Duration::from_millis(250);
const WINDOWS_CONTROLLER_PIPES: [&str; 1] = ["verge-mihomo"];
#[cfg(windows)]
const ERROR_PIPE_BUSY: i32 = 231;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MihomoControllerInput {
    pub controller_url: String,
    #[serde(default)]
    pub secret: String,
}

impl Drop for MihomoControllerInput {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoSnapshot {
    pub checked_at: String,
    pub groups: Vec<MihomoSelectorGroup>,
    pub providers: Vec<MihomoProvider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalClashProbe {
    pub mixed_port: Option<u16>,
    pub controller_url: Option<String>,
    pub snapshot: Option<MihomoSnapshot>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoProvider {
    pub name: String,
    pub vehicle_type: Option<String>,
    pub updated_at: Option<String>,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoSelectorGroup {
    pub name: String,
    pub selected: Option<String>,
    pub nodes: Vec<MihomoNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MihomoNode {
    pub name: String,
    pub proxy_type: Option<String>,
    pub delay_ms: Option<u64>,
    pub alive: Option<bool>,
}

/// Immutable, in-memory evidence for the exact external Mihomo instance and
/// listener accepted at launch. It is deliberately not serialized: recovery
/// after a desktop restart must perform a fresh launch instead of trusting an
/// old Controller/config snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct MihomoRuntimeGuard {
    controller: ControllerTarget,
    proxy_endpoint: SocketAddr,
    configuration: Value,
    pinned: Option<PinnedInbound>,
}

#[derive(Clone)]
pub struct PinnedInbound {
    pub name: String,
    pub port: u16,
    pub node_name: String,
    runtime: Arc<Mutex<IsolatedMihomoRuntime>>,
}

impl std::fmt::Debug for PinnedInbound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PinnedInbound")
            .field("name", &self.name)
            .field("port", &self.port)
            .field("node_name", &self.node_name)
            .field("isolated", &true)
            .finish()
    }
}

impl PartialEq for PinnedInbound {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.port == other.port && self.node_name == other.node_name
    }
}

impl Eq for PinnedInbound {}

#[derive(Debug)]
struct IsolatedMihomoRuntime {
    child: Option<Child>,
    root: PathBuf,
    #[cfg(windows)]
    job_handle: isize,
}

impl IsolatedMihomoRuntime {
    fn shutdown(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(windows)]
        if self.job_handle != 0 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.job_handle as _);
            }
            self.job_handle = 0;
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Drop for IsolatedMihomoRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl MihomoRuntimeGuard {
    pub fn proxy_endpoint(&self) -> SocketAddr {
        self.proxy_endpoint
    }

    pub fn pinned_inbound(&self) -> Option<&PinnedInbound> {
        self.pinned.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ControllerTarget {
    Tcp(SocketAddr),
    Pipe(String),
}

enum ControllerConn {
    Tcp(TcpStream),
    #[cfg(windows)]
    Pipe(File),
}

#[derive(Debug, Error)]
pub enum MihomoError {
    #[error("Mihomo Controller 只允许显式本机 HTTP 地址，例如 http://127.0.0.1:9097/。")]
    UnsafeController,
    #[error("{0} 是 Clash 给浏览器走流量的代理端口，不是读取代理组的控制端口。控制端口一般是 9097 或 9090。")]
    MixedPortUsedAsController(u16),
    #[error("Mihomo Controller Secret 过长或包含不支持的字符。")]
    InvalidSecret,
    #[error("本机没有可用的 Clash 控制口。Clash Verge 默认关闭 9097，请点「查找本机 Clash」或再点「读取代理组」，程序会走内核管道。")]
    ControllerUnreachable,
    #[error("无法连接本机 Mihomo Controller：{0}")]
    Io(#[from] io::Error),
    #[error("本机 Mihomo Controller 返回 HTTP {0}；请检查地址和 Secret。")]
    HttpStatus(u16),
    #[error("本机 Mihomo Controller 返回了无法识别的响应。")]
    InvalidResponse,
    #[error("没有找到 Mihomo 选择组“{0}”。")]
    SelectorNotFound(String),
    #[error("Mihomo 选择组“{group}”中不存在节点“{node}”。")]
    NodeNotFound { group: String, node: String },
    #[error("Mihomo 报告节点“{0}”当前不可用。")]
    NodeUnavailable(String),
    #[error("Mihomo 回读结果显示选择组“{group}”没有保持在节点“{node}”。")]
    SelectionNotApplied { group: String, node: String },
    #[error("Clash 当前是直连模式，所选节点不会生效。请在 Clash 里改回规则或全局模式后再启动。")]
    DirectFallbackPossible,
    #[error("Mihomo 选择的节点“{0}”是 DIRECT/REJECT 或无法证明为远端代理节点。")]
    UnsafeSelectedNode(String),
    #[error("Mihomo 配置没有把所选 loopback SOCKS5 端口绑定为 socks-port 或 mixed-port。")]
    ProxyListenerMismatch,
    #[error("Mihomo Controller 的运行中配置已漂移。")]
    ConfigurationDrift,
    #[error(
        "无法为这个 Silo 创建独立代理。请保持 Clash Verge 开启，并确认所选节点仍在当前配置中。"
    )]
    PinnedInboundUnsupported,
    #[error("无法把托管浏览器绑到所选 Clash 节点的独立入口。")]
    PinnedInboundFailed,
    #[error("无法读取 Clash Verge 当前配置，不能安全复制所选节点。")]
    IsolatedConfigUnavailable,
    #[error("Clash Verge 当前配置中没有可独立运行的节点“{0}”。")]
    IsolatedNodeUnavailable(String),
}

pub fn inspect_controller(input: &MihomoControllerInput) -> Result<MihomoSnapshot, MihomoError> {
    validate_secret(&input.secret)?;
    let target = parse_controller_target(&input.controller_url)?;
    if let ControllerTarget::Tcp(endpoint) = &target {
        if COMMON_MIXED_PORTS.contains(&endpoint.port()) {
            return Err(MihomoError::MixedPortUsedAsController(endpoint.port()));
        }
    }
    let body = controller_request(
        &input.controller_url,
        "GET",
        "/proxies",
        &input.secret,
        None,
    )?;
    let mut snapshot = parse_snapshot(&body)?;
    snapshot.controller_url = Some(input.controller_url.clone());
    Ok(snapshot)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClashDiagnose {
    pub mixed_port: Option<u16>,
    pub controller_url: Option<String>,
    pub mode: Option<String>,
    pub socks_port: Option<u64>,
    pub configured_mixed_port: Option<u64>,
    pub detail: String,
    pub groups: Vec<MihomoSelectorGroup>,
}

pub fn inspect_configs(controller_url: &str, secret: &str) -> Result<Value, MihomoError> {
    validate_secret(secret)?;
    let body = Zeroizing::new(controller_request(
        controller_url,
        "GET",
        "/configs",
        secret,
        None,
    )?);
    let mut configuration: Value =
        serde_json::from_slice(&body).map_err(|_| MihomoError::InvalidResponse)?;
    redact_configuration_secrets(&mut configuration);
    Ok(configuration)
}

pub fn diagnose_local_clash(secret: &str) -> ClashDiagnose {
    let probe = probe_local_clash(secret);
    let mut mode = None;
    let mut socks_port = None;
    let mut configured_mixed_port = None;
    let mut groups = Vec::new();
    if let Some(controller) = probe.controller_url.as_deref() {
        if let Ok(snapshot) = inspect_controller(&MihomoControllerInput {
            controller_url: controller.to_owned(),
            secret: secret.to_owned(),
        }) {
            groups = snapshot.groups;
        }
        if let Ok(configs) = inspect_configs(controller, secret) {
            mode = configs
                .get("mode")
                .and_then(Value::as_str)
                .map(str::to_owned);
            socks_port = configs.get("socks-port").and_then(Value::as_u64);
            configured_mixed_port = configs.get("mixed-port").and_then(Value::as_u64);
        }
    }
    ClashDiagnose {
        mixed_port: probe.mixed_port,
        controller_url: probe.controller_url,
        mode,
        socks_port,
        configured_mixed_port,
        detail: probe.detail,
        groups,
    }
}

pub fn probe_local_clash(_secret: &str) -> LocalClashProbe {
    let mixed_port = COMMON_MIXED_PORTS
        .into_iter()
        .find(|port| loopback_port_is_open(*port));
    let controller_url = discover_controller_url();
    match (mixed_port, controller_url.as_deref()) {
        (Some(mixed), Some(controller)) if controller.starts_with("pipe:") => LocalClashProbe {
            mixed_port: Some(mixed),
            controller_url: Some(controller.to_owned()),
            snapshot: None,
            detail: format!(
                "已找到 Clash Verge，代理端口 {mixed}。直接点「读取代理组」选线路，不用填 9097。"
            ),
        },
        (Some(mixed), Some(controller)) => LocalClashProbe {
            mixed_port: Some(mixed),
            controller_url: Some(controller.to_owned()),
            snapshot: None,
            detail: format!(
                "已找到本机代理 {mixed}，控制口 {}。要选线路再点「读取代理组」。",
                controller_label(controller)
            ),
        },
        (Some(mixed), None) => LocalClashProbe {
            mixed_port: Some(mixed),
            controller_url: None,
            snapshot: None,
            detail: format!(
                "已找到本机代理端口 {mixed}，但没有可读的控制口。Clash Verge 请保持开启后再点「读取代理组」；其他客户端请打开外部控制（9097 或 9090），不要把 {mixed} 填进控制口。"
            ),
        },
        (None, Some(controller)) => LocalClashProbe {
            mixed_port: None,
            controller_url: Some(controller.to_owned()),
            snapshot: None,
            detail: format!(
                "已找到 Clash 控制口 {}。请确认浏览器代理端口是 7897 或 7890。",
                controller_label(controller)
            ),
        },
        (None, None) => LocalClashProbe {
            mixed_port: None,
            controller_url: None,
            snapshot: None,
            detail: "没有在本机找到 Clash。请先打开 Clash Verge / Mihomo，代理端口一般是 7897 或 7890。"
                .to_owned(),
        },
    }
}

fn discover_controller_url() -> Option<String> {
    for port in COMMON_CONTROLLER_PORTS {
        if loopback_port_is_open(port) && controller_looks_like_clash(port) {
            return Some(format!("http://127.0.0.1:{port}/"));
        }
    }
    WINDOWS_CONTROLLER_PIPES
        .into_iter()
        .find(|name| pipe_looks_like_clash(name))
        .map(|name| format!("pipe://{name}/"))
}

fn controller_label(controller: &str) -> String {
    if let Some(name) = controller
        .strip_prefix("pipe://")
        .and_then(|value| value.strip_suffix('/'))
    {
        return format!("Clash Verge 内核管道（{name}）");
    }
    Url::parse(controller)
        .ok()
        .and_then(|url| url.port())
        .map(|port| port.to_string())
        .unwrap_or_else(|| controller.to_owned())
}

fn loopback_port_is_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        LOCAL_PROBE_TIMEOUT,
    )
    .is_ok()
}

fn controller_looks_like_clash(port: u16) -> bool {
    let endpoint = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&endpoint, LOCAL_PROBE_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(LOCAL_PROBE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(LOCAL_PROBE_TIMEOUT));
    let request =
        format!("GET /version HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    response_looks_like_clash(&mut stream, &request)
}

fn pipe_looks_like_clash(name: &str) -> bool {
    #[cfg(not(windows))]
    {
        let _ = name;
        false
    }
    #[cfg(windows)]
    {
        let Ok(mut stream) = connect_windows_pipe(name, LOCAL_PROBE_TIMEOUT) else {
            return false;
        };
        let request =
            "GET /version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".to_owned();
        response_looks_like_clash(&mut stream, &request)
    }
}

fn response_looks_like_clash(stream: &mut impl ReadWrite, request: &str) -> bool {
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let _ = stream.flush();
    let mut body = Vec::new();
    let mut buffer = [0_u8; 512];
    let deadline = Instant::now() + LOCAL_PROBE_TIMEOUT;
    while Instant::now() < deadline && body.len() < 2048 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => body.extend_from_slice(&buffer[..read]),
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&body).to_ascii_lowercase();
    text.contains("clash")
        || text.contains("mihomo")
        || text.contains("\"meta\"")
        || text.contains("\"hello\"")
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

pub fn pin_selected_inbound(
    binding: &ExternalMihomoBinding,
    authentication: Option<&MihomoControllerAuthentication>,
    silo_id: Uuid,
) -> Result<PinnedInbound, MihomoError> {
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;
    validate_available_node(binding, secret)?;
    pin_isolated_mihomo(binding, silo_id)
}

fn validate_available_node(
    binding: &ExternalMihomoBinding,
    secret: &str,
) -> Result<(), MihomoError> {
    let snapshot = controller_request(&binding.controller_url, "GET", "/proxies", secret, None)?;
    let snapshot = parse_snapshot(&snapshot)?;
    let group = snapshot
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .ok_or_else(|| MihomoError::SelectorNotFound(binding.selector_group.clone()))?;
    let node = group
        .nodes
        .iter()
        .find(|node| node.name == binding.node_name)
        .ok_or_else(|| MihomoError::NodeNotFound {
            group: binding.selector_group.clone(),
            node: binding.node_name.clone(),
        })?;
    if node.alive == Some(false) {
        return Err(MihomoError::NodeUnavailable(binding.node_name.clone()));
    }
    if is_unsafe_proxy_type(node.proxy_type.as_deref()) {
        return Err(MihomoError::UnsafeSelectedNode(binding.node_name.clone()));
    }
    Ok(())
}

pub fn verify_binding(
    binding: &ExternalMihomoBinding,
    authentication: Option<&MihomoControllerAuthentication>,
) -> Result<(), MihomoError> {
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;
    validate_available_node(binding, secret)
}

#[cfg(windows)]
fn pin_isolated_mihomo(
    binding: &ExternalMihomoBinding,
    silo_id: Uuid,
) -> Result<PinnedInbound, MihomoError> {
    if !matches!(
        parse_controller_target(&binding.controller_url)?,
        ControllerTarget::Pipe(ref name) if name == "verge-mihomo"
    ) {
        return Err(MihomoError::PinnedInboundUnsupported);
    }

    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or(MihomoError::IsolatedConfigUnavailable)?;
    let program_files = std::env::var_os("PROGRAMFILES")
        .map(PathBuf::from)
        .ok_or(MihomoError::IsolatedConfigUnavailable)?;
    let source_path = app_data
        .join("io.github.clash-verge-rev.clash-verge-rev")
        .join("clash-verge.yaml");
    let binary = program_files.join("Clash Verge").join("verge-mihomo.exe");
    if !source_path.is_file() || !binary.is_file() {
        return Err(MihomoError::IsolatedConfigUnavailable);
    }
    let source_metadata = fs::metadata(&source_path)?;
    if source_metadata.len() > 8 * 1024 * 1024 {
        return Err(MihomoError::IsolatedConfigUnavailable);
    }

    let source = Zeroizing::new(fs::read(source_path)?);
    let parsed: serde_yaml::Value =
        serde_yaml::from_slice(&source).map_err(|_| MihomoError::IsolatedConfigUnavailable)?;
    let port = allocate_loopback_port()?;
    let name = pinned_inbound_name(silo_id);
    let config = isolated_mihomo_config(&parsed, &binding.node_name, &name, port)?;
    let config_bytes = Zeroizing::new(
        serde_yaml::to_string(&config)
            .map_err(|_| MihomoError::IsolatedConfigUnavailable)?
            .into_bytes(),
    );

    let local_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(MihomoError::IsolatedConfigUnavailable)?;
    let root = local_data
        .join("io.verisilo.app")
        .join("runtime")
        .join("mihomo")
        .join(format!(
            "{}-{}",
            silo_id.as_simple(),
            Uuid::new_v4().as_simple()
        ));
    fs::create_dir_all(&root)?;
    let config_path = root.join("config.yaml");
    let result: Result<PinnedInbound, MihomoError> = (|| -> Result<_, MihomoError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config_path)?;
        file.write_all(&config_bytes)?;
        file.sync_all()?;
        drop(file);

        let mut command = Command::new(&binary);
        command
            .arg("-d")
            .arg(&root)
            .arg("-f")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_windows_console(&mut command);
        let mut child = command.spawn()?;
        let job_handle = match attach_kill_on_close_job(&child) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(MihomoError::Io(error));
            }
        };
        let runtime = Arc::new(Mutex::new(IsolatedMihomoRuntime {
            child: Some(child),
            root: root.clone(),
            job_handle,
        }));
        if let Err(error) = wait_for_socks_hello(port) {
            if let Ok(mut runtime) = runtime.lock() {
                runtime.shutdown();
            }
            return Err(error);
        }
        scrub_and_remove(&config_path, config_bytes.len());
        Ok(PinnedInbound {
            name,
            port,
            node_name: binding.node_name.clone(),
            runtime,
        })
    })();
    if result.is_err() {
        scrub_and_remove(&config_path, config_bytes.len());
        let _ = fs::remove_dir_all(&root);
    }
    result
}

#[cfg(not(windows))]
fn pin_isolated_mihomo(
    _binding: &ExternalMihomoBinding,
    _silo_id: Uuid,
) -> Result<PinnedInbound, MihomoError> {
    Err(MihomoError::PinnedInboundUnsupported)
}

fn isolated_mihomo_config(
    source: &serde_yaml::Value,
    node_name: &str,
    listener_name: &str,
    port: u16,
) -> Result<serde_yaml::Value, MihomoError> {
    use serde_yaml::{Mapping, Number, Value as Yaml};

    let source = source
        .as_mapping()
        .ok_or(MihomoError::IsolatedConfigUnavailable)?;
    let proxies = source
        .get("proxies")
        .and_then(Yaml::as_sequence)
        .ok_or_else(|| MihomoError::IsolatedNodeUnavailable(node_name.to_owned()))?;
    let mut by_name = HashMap::new();
    for proxy in proxies {
        if let Some(name) = proxy
            .as_mapping()
            .and_then(|proxy| proxy.get("name"))
            .and_then(Yaml::as_str)
        {
            by_name.insert(name.to_owned(), proxy);
        }
    }
    let mut selected = Vec::new();
    collect_proxy_dependencies(node_name, &by_name, &mut HashSet::new(), &mut selected)?;

    let mut listener = Mapping::new();
    listener.insert("name", Yaml::String(listener_name.to_owned()));
    listener.insert("type", Yaml::String("socks".to_owned()));
    listener.insert("listen", Yaml::String("127.0.0.1".to_owned()));
    listener.insert("port", Yaml::Number(Number::from(port)));
    listener.insert("udp", Yaml::Bool(true));
    listener.insert("proxy", Yaml::String(node_name.to_owned()));

    let mut root = Mapping::new();
    root.insert("allow-lan", Yaml::Bool(false));
    root.insert("ipv6", Yaml::Bool(false));
    root.insert("log-level", Yaml::String("silent".to_owned()));
    root.insert("proxies", Yaml::Sequence(selected));
    root.insert("listeners", Yaml::Sequence(vec![Yaml::Mapping(listener)]));
    Ok(Yaml::Mapping(root))
}

fn collect_proxy_dependencies<'a>(
    name: &str,
    proxies: &HashMap<String, &'a serde_yaml::Value>,
    visiting: &mut HashSet<String>,
    selected: &mut Vec<serde_yaml::Value>,
) -> Result<(), MihomoError> {
    if selected.iter().any(|proxy| {
        proxy
            .as_mapping()
            .and_then(|proxy| proxy.get("name"))
            .and_then(serde_yaml::Value::as_str)
            == Some(name)
    }) {
        return Ok(());
    }
    if !visiting.insert(name.to_owned()) {
        return Err(MihomoError::IsolatedNodeUnavailable(name.to_owned()));
    }
    let proxy = proxies
        .get(name)
        .ok_or_else(|| MihomoError::IsolatedNodeUnavailable(name.to_owned()))?;
    if let Some(dependency) = proxy
        .as_mapping()
        .and_then(|proxy| proxy.get("dialer-proxy"))
        .and_then(serde_yaml::Value::as_str)
    {
        collect_proxy_dependencies(dependency, proxies, visiting, selected)?;
    }
    selected.push((*proxy).clone());
    visiting.remove(name);
    Ok(())
}

fn scrub_and_remove(path: &Path, length: usize) {
    if let Ok(mut file) = OpenOptions::new().write(true).open(path) {
        let zeros = vec![0_u8; length.min(8 * 1024 * 1024)];
        let _ = file.write_all(&zeros);
        let _ = file.sync_all();
    }
    let _ = fs::remove_file(path);
}

#[cfg(windows)]
fn attach_kill_on_close_job(child: &Child) -> io::Result<isize> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
    };

    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            std::mem::size_of_val(&limits) as u32,
        )
    };
    let assigned = configured != 0
        && unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as _) } != 0;
    if !assigned {
        let error = io::Error::last_os_error();
        unsafe { CloseHandle(job) };
        return Err(error);
    }
    Ok(job as isize)
}

pub fn unpin_inbound(
    _binding: &ExternalMihomoBinding,
    _authentication: Option<&MihomoControllerAuthentication>,
    inbound: &PinnedInbound,
) -> Result<(), MihomoError> {
    inbound
        .runtime
        .lock()
        .map_err(|_| MihomoError::PinnedInboundFailed)?
        .shutdown();
    Ok(())
}

fn pinned_inbound_name(silo_id: Uuid) -> String {
    format!("verisilo-{}", silo_id.as_simple())
}

fn allocate_loopback_port() -> Result<u16, MihomoError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    if port == 0 {
        return Err(MihomoError::PinnedInboundFailed);
    }
    Ok(port)
}

fn wait_for_socks_hello(port: u16) -> Result<(), MihomoError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last_error = MihomoError::PinnedInboundFailed;
    while Instant::now() < deadline {
        match socks5_noauth_hello(port) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(last_error)
}

fn socks5_noauth_hello(port: u16) -> Result<(), MihomoError> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(200))?;
    stream.set_read_timeout(Some(Duration::from_millis(400)))?;
    stream.set_write_timeout(Some(Duration::from_millis(400)))?;
    stream.write_all(&[5, 1, 0])?;
    let mut reply = [0_u8; 2];
    stream.read_exact(&mut reply)?;
    if reply == [5, 0] {
        Ok(())
    } else {
        Err(MihomoError::PinnedInboundFailed)
    }
}

fn verify_pinned_inbound(
    _binding: &ExternalMihomoBinding,
    inbound: &PinnedInbound,
    _secret: &str,
) -> Result<(), MihomoError> {
    socks5_noauth_hello(inbound.port)?;
    let mut runtime = inbound
        .runtime
        .lock()
        .map_err(|_| MihomoError::PinnedInboundFailed)?;
    if runtime
        .child
        .as_mut()
        .ok_or(MihomoError::ConfigurationDrift)?
        .try_wait()?
        .is_some()
    {
        return Err(MihomoError::ConfigurationDrift);
    }
    Ok(())
}

/// Captures the exact Controller endpoint, proxy listener and redacted config
/// accepted for this required-proxy launch. The browser is fail-closed onto the
/// loopback relay; Clash may stay in rule mode and any selector group. Clash
/// `direct` mode is rejected because the selected node would not carry traffic.
pub fn capture_runtime_guard(
    binding: &ExternalMihomoBinding,
    proxy_host: &str,
    proxy_port: u16,
    authentication: Option<&MihomoControllerAuthentication>,
) -> Result<MihomoRuntimeGuard, MihomoError> {
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;
    let controller = parse_controller_target(&binding.controller_url)?;
    let proxy_endpoint = loopback_proxy_endpoint(proxy_host, proxy_port)?;
    let snapshot = read_binding_snapshot(binding, secret)?;
    validate_required_route(binding, &snapshot)?;
    let configuration = read_runtime_configuration(binding, proxy_endpoint, secret)?;
    Ok(MihomoRuntimeGuard {
        controller,
        proxy_endpoint,
        configuration,
        pinned: None,
    })
}

pub fn capture_pinned_runtime_guard(
    binding: &ExternalMihomoBinding,
    inbound: &PinnedInbound,
    authentication: Option<&MihomoControllerAuthentication>,
) -> Result<MihomoRuntimeGuard, MihomoError> {
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;
    let controller = parse_controller_target(&binding.controller_url)?;
    let proxy_endpoint = loopback_proxy_endpoint("127.0.0.1", inbound.port)?;
    let configuration = serde_json::json!({ "kind": "isolated-mihomo", "node": inbound.node_name });
    verify_pinned_inbound(binding, inbound, secret)?;
    Ok(MihomoRuntimeGuard {
        controller,
        proxy_endpoint,
        configuration,
        pinned: Some(inbound.clone()),
    })
}

/// Rechecks an existing guard without applying a node or configuration. Any
/// endpoint, authentication, selection, node-health or config mismatch is a
/// terminal result for the caller's exact relay; this function never repairs
/// or rotates the user's Mihomo process in the background.
pub fn verify_runtime_guard(
    guard: &MihomoRuntimeGuard,
    binding: &ExternalMihomoBinding,
    proxy_host: &str,
    proxy_port: u16,
    authentication: Option<&MihomoControllerAuthentication>,
) -> Result<(), MihomoError> {
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;
    let proxy_endpoint = loopback_proxy_endpoint(proxy_host, proxy_port)?;
    if proxy_endpoint != guard.proxy_endpoint {
        return Err(MihomoError::ConfigurationDrift);
    }
    if let Some(inbound) = guard.pinned.as_ref() {
        if binding.node_name != inbound.node_name {
            return Err(MihomoError::ConfigurationDrift);
        }
        return verify_pinned_inbound(binding, inbound, secret);
    }
    let controller = parse_controller_target(&binding.controller_url)?;
    if controller != guard.controller {
        return Err(MihomoError::ConfigurationDrift);
    }
    let snapshot = read_binding_snapshot(binding, secret)?;
    validate_required_route(binding, &snapshot)?;
    let configuration = if guard.pinned.is_some() {
        read_runtime_configuration_without_port(binding, secret)?
    } else {
        read_runtime_configuration(binding, proxy_endpoint, secret)?
    };
    if configuration != guard.configuration {
        return Err(MihomoError::ConfigurationDrift);
    }
    if let Some(inbound) = &guard.pinned {
        verify_pinned_inbound(binding, inbound, secret)?;
    }
    Ok(())
}

/// Deadline-aware variant used by the native runtime watchdog. Both
/// Controller reads share one absolute deadline, so reconnects and slow
/// trickle responses cannot reset the health-check budget.
pub(crate) fn verify_runtime_guard_until(
    guard: &MihomoRuntimeGuard,
    binding: &ExternalMihomoBinding,
    proxy_host: &str,
    proxy_port: u16,
    authentication: Option<&MihomoControllerAuthentication>,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<(), MihomoError> {
    ensure_probe_active(deadline, cancelled)?;
    let secret = authentication.map_or("", MihomoControllerAuthentication::secret);
    validate_secret(secret)?;
    let proxy_endpoint = loopback_proxy_endpoint(proxy_host, proxy_port)?;
    if proxy_endpoint != guard.proxy_endpoint {
        return Err(MihomoError::ConfigurationDrift);
    }
    if let Some(inbound) = guard.pinned.as_ref() {
        if binding.node_name != inbound.node_name {
            return Err(MihomoError::ConfigurationDrift);
        }
        ensure_probe_active(deadline, cancelled)?;
        return verify_pinned_inbound(binding, inbound, secret);
    }
    let controller = parse_controller_target(&binding.controller_url)?;
    if controller != guard.controller {
        return Err(MihomoError::ConfigurationDrift);
    }
    let snapshot = read_binding_snapshot_until(binding, secret, deadline, cancelled)?;
    validate_required_route(binding, &snapshot)?;
    ensure_probe_active(deadline, cancelled)?;
    let configuration = if guard.pinned.is_some() {
        read_runtime_configuration_without_port_until(binding, secret, deadline, cancelled)?
    } else {
        read_runtime_configuration_until(binding, proxy_endpoint, secret, deadline, cancelled)?
    };
    ensure_probe_active(deadline, cancelled)?;
    if configuration != guard.configuration {
        return Err(MihomoError::ConfigurationDrift);
    }
    if let Some(inbound) = &guard.pinned {
        ensure_probe_active(deadline, cancelled)?;
        verify_pinned_inbound(binding, inbound, secret)?;
    }
    Ok(())
}

fn read_binding_snapshot(
    binding: &ExternalMihomoBinding,
    secret: &str,
) -> Result<MihomoSnapshot, MihomoError> {
    let body = controller_request(&binding.controller_url, "GET", "/proxies", secret, None)?;
    let snapshot = parse_snapshot(&body)?;
    let selected = snapshot
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .and_then(|group| group.selected.as_deref());
    if selected != Some(binding.node_name.as_str()) {
        return Err(MihomoError::SelectionNotApplied {
            group: binding.selector_group.clone(),
            node: binding.node_name.clone(),
        });
    }
    Ok(snapshot)
}

fn read_binding_snapshot_until(
    binding: &ExternalMihomoBinding,
    secret: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<MihomoSnapshot, MihomoError> {
    let body = controller_request_until(
        &binding.controller_url,
        "GET",
        "/proxies",
        secret,
        None,
        deadline,
        cancelled,
    )?;
    ensure_probe_active(deadline, cancelled)?;
    let snapshot = parse_snapshot(&body)?;
    let selected = snapshot
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .and_then(|group| group.selected.as_deref());
    if selected != Some(binding.node_name.as_str()) {
        return Err(MihomoError::SelectionNotApplied {
            group: binding.selector_group.clone(),
            node: binding.node_name.clone(),
        });
    }
    Ok(snapshot)
}

fn validate_required_route(
    binding: &ExternalMihomoBinding,
    snapshot: &MihomoSnapshot,
) -> Result<(), MihomoError> {
    let node = snapshot
        .groups
        .iter()
        .find(|group| group.name == binding.selector_group)
        .and_then(|group| {
            group
                .nodes
                .iter()
                .find(|node| node.name == binding.node_name)
        })
        .ok_or_else(|| MihomoError::NodeNotFound {
            group: binding.selector_group.clone(),
            node: binding.node_name.clone(),
        })?;
    if node.alive == Some(false) {
        return Err(MihomoError::NodeUnavailable(binding.node_name.clone()));
    }
    if is_unsafe_proxy_type(node.proxy_type.as_deref()) {
        return Err(MihomoError::UnsafeSelectedNode(binding.node_name.clone()));
    }
    Ok(())
}

fn is_unsafe_proxy_type(proxy_type: Option<&str>) -> bool {
    let proxy_type = proxy_type.unwrap_or_default();
    proxy_type.is_empty()
        || matches!(
            proxy_type.to_ascii_lowercase().as_str(),
            "direct" | "reject" | "reject-drop" | "pass" | "compatible"
        )
}

fn read_runtime_configuration(
    binding: &ExternalMihomoBinding,
    proxy_endpoint: SocketAddr,
    secret: &str,
) -> Result<Value, MihomoError> {
    let configuration = read_runtime_configuration_without_port(binding, secret)?;
    let object = configuration
        .as_object()
        .ok_or(MihomoError::InvalidResponse)?;
    let expected_port = u64::from(proxy_endpoint.port());
    let listener_matches = ["socks-port", "mixed-port"]
        .iter()
        .any(|field| object.get(*field).and_then(Value::as_u64) == Some(expected_port));
    if !listener_matches {
        return Err(MihomoError::ProxyListenerMismatch);
    }
    Ok(configuration)
}

fn read_runtime_configuration_without_port(
    binding: &ExternalMihomoBinding,
    secret: &str,
) -> Result<Value, MihomoError> {
    let body = Zeroizing::new(controller_request(
        &binding.controller_url,
        "GET",
        "/configs",
        secret,
        None,
    )?);
    parse_runtime_configuration(&body)
}

fn parse_runtime_configuration(body: &[u8]) -> Result<Value, MihomoError> {
    let mut configuration: Value =
        serde_json::from_slice(body).map_err(|_| MihomoError::InvalidResponse)?;
    redact_configuration_secrets(&mut configuration);
    let object = configuration
        .as_object()
        .ok_or(MihomoError::InvalidResponse)?;
    reject_clash_direct_mode(object)?;
    Ok(configuration)
}

fn read_runtime_configuration_until(
    binding: &ExternalMihomoBinding,
    proxy_endpoint: SocketAddr,
    secret: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Value, MihomoError> {
    let body = controller_request_until(
        &binding.controller_url,
        "GET",
        "/configs",
        secret,
        None,
        deadline,
        cancelled,
    )?;
    ensure_probe_active(deadline, cancelled)?;
    let configuration = parse_runtime_configuration(&body)?;
    let object = configuration
        .as_object()
        .ok_or(MihomoError::InvalidResponse)?;
    let expected_port = u64::from(proxy_endpoint.port());
    let listener_matches = ["socks-port", "mixed-port"]
        .iter()
        .any(|field| object.get(*field).and_then(Value::as_u64) == Some(expected_port));
    if !listener_matches {
        return Err(MihomoError::ProxyListenerMismatch);
    }
    Ok(configuration)
}

fn read_runtime_configuration_without_port_until(
    binding: &ExternalMihomoBinding,
    secret: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Value, MihomoError> {
    let body = controller_request_until(
        &binding.controller_url,
        "GET",
        "/configs",
        secret,
        None,
        deadline,
        cancelled,
    )?;
    ensure_probe_active(deadline, cancelled)?;
    parse_runtime_configuration(&body)
}

fn reject_clash_direct_mode(object: &serde_json::Map<String, Value>) -> Result<(), MihomoError> {
    let mode = object
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("direct") {
        Err(MihomoError::DirectFallbackPossible)
    } else {
        Ok(())
    }
}

fn redact_configuration_secrets(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let key = key.to_ascii_lowercase();
                if key.contains("secret")
                    || key.contains("password")
                    || key.contains("token")
                    || key.contains("authorization")
                {
                    zeroize_json_strings(value);
                    *value = Value::String("<redacted>".to_owned());
                } else {
                    redact_configuration_secrets(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_configuration_secrets),
        _ => {}
    }
}

fn zeroize_json_strings(value: &mut Value) {
    match value {
        Value::String(value) => value.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(zeroize_json_strings),
        Value::Object(object) => object.values_mut().for_each(zeroize_json_strings),
        _ => {}
    }
}

fn loopback_proxy_endpoint(host: &str, port: u16) -> Result<SocketAddr, MihomoError> {
    let ip = host
        .trim()
        .trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .map_err(|_| MihomoError::ProxyListenerMismatch)?;
    if !ip.is_loopback() || port == 0 {
        return Err(MihomoError::ProxyListenerMismatch);
    }
    Ok(SocketAddr::new(ip, port))
}

fn validate_secret(secret: &str) -> Result<(), MihomoError> {
    if secret.len() > 1_024 || secret.chars().any(char::is_control) {
        return Err(MihomoError::InvalidSecret);
    }
    Ok(())
}

fn parse_snapshot(body: &[u8]) -> Result<MihomoSnapshot, MihomoError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| MihomoError::InvalidResponse)?;
    let proxies = root
        .get("proxies")
        .and_then(Value::as_object)
        .ok_or(MihomoError::InvalidResponse)?;
    let mut groups = Vec::new();
    for (group_name, group_value) in proxies {
        if bounded_controller_text(group_name, 128).is_none() || groups.len() >= 256 {
            continue;
        }
        let proxy_type = group_value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(
            proxy_type,
            "Selector" | "URLTest" | "Fallback" | "LoadBalance"
        ) {
            continue;
        }
        let Some(node_names) = group_value.get("all").and_then(Value::as_array) else {
            continue;
        };
        let mut nodes = node_names
            .iter()
            .filter_map(Value::as_str)
            .filter(|node_name| bounded_controller_text(node_name, 256).is_some())
            .take(256)
            .map(|node_name| {
                let node = proxies.get(node_name);
                MihomoNode {
                    name: node_name.to_owned(),
                    proxy_type: node
                        .and_then(|value| value.get("type"))
                        .and_then(Value::as_str)
                        .and_then(|value| bounded_controller_text(value, 64))
                        .map(str::to_owned),
                    delay_ms: node.and_then(latest_delay),
                    alive: node
                        .and_then(|value| value.get("alive"))
                        .and_then(Value::as_bool),
                }
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.name.cmp(&right.name));
        groups.push(MihomoSelectorGroup {
            name: group_name.clone(),
            selected: group_value
                .get("now")
                .and_then(Value::as_str)
                .and_then(|value| bounded_controller_text(value, 256))
                .map(str::to_owned),
            nodes,
        });
    }
    groups.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(MihomoSnapshot {
        checked_at: Utc::now().to_rfc3339(),
        groups,
        providers: Vec::new(),
        controller_url: None,
    })
}

#[allow(dead_code)]
fn parse_providers(body: &[u8]) -> Result<Vec<MihomoProvider>, MihomoError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| MihomoError::InvalidResponse)?;
    let providers = root
        .get("providers")
        .and_then(Value::as_object)
        .ok_or(MihomoError::InvalidResponse)?;
    let mut result = providers
        .iter()
        .filter_map(|(provider_name, provider)| {
            let name = provider
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(provider_name);
            Some(MihomoProvider {
                name: bounded_controller_text(name, 256)?.to_owned(),
                vehicle_type: provider
                    .get("vehicleType")
                    .and_then(Value::as_str)
                    .and_then(|value| bounded_controller_text(value, 64))
                    .map(str::to_owned),
                updated_at: provider
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .and_then(|value| bounded_controller_text(value, 128))
                    .map(str::to_owned),
                node_count: provider
                    .get("proxies")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            })
        })
        .take(100)
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(result)
}

fn bounded_controller_text(value: &str, maximum: usize) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control))
        .then_some(value)
}

fn latest_delay(value: &Value) -> Option<u64> {
    value
        .get("history")
        .and_then(Value::as_array)
        .and_then(|history| history.last())
        .and_then(|entry| entry.get("delay"))
        .and_then(Value::as_u64)
        .filter(|delay| *delay > 0)
}

fn controller_request(
    controller_url: &str,
    method: &str,
    path: &str,
    secret: &str,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, MihomoError> {
    let target = parse_controller_target(controller_url)?;
    let mut stream = connect_controller(&target, CONTROLLER_TIMEOUT)?;
    stream.set_io_timeout(Some(CONTROLLER_TIMEOUT))?;
    let authorization = Zeroizing::new(if secret.is_empty() {
        String::new()
    } else {
        format!("Authorization: Bearer {secret}\r\n")
    });
    let body = body.unwrap_or_default();
    let content_headers = if body.is_empty() {
        String::new()
    } else {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        )
    };
    let request = Zeroizing::new(format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\n{}{content_headers}Connection: close\r\n\r\n",
        target.host_header(),
        authorization.as_str(),
    ));
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;
    let _ = stream.flush();

    let response = Zeroizing::new(read_http_response(&mut stream)?);
    parse_http_response(&response)
}

fn controller_request_until(
    controller_url: &str,
    method: &str,
    path: &str,
    secret: &str,
    body: Option<&[u8]>,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Zeroizing<Vec<u8>>, MihomoError> {
    ensure_probe_active(deadline, cancelled)?;
    let target = parse_controller_target(controller_url)?;
    // Connect before constructing any authorization buffer. A slow or failed
    // connection therefore never retains an extra copy of the Vault secret.
    let connect_timeout =
        remaining_controller_timeout(deadline, cancelled)?.min(CONTROLLER_TIMEOUT);
    let mut stream = connect_controller(&target, connect_timeout)?;
    ensure_probe_active(deadline, cancelled)?;

    let authorization = Zeroizing::new(if secret.is_empty() {
        String::new()
    } else {
        format!("Authorization: Bearer {secret}\r\n")
    });
    let body = body.unwrap_or_default();
    let content_headers = if body.is_empty() {
        String::new()
    } else {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        )
    };
    let request = Zeroizing::new(format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\n{}{content_headers}Connection: close\r\n\r\n",
        target.host_header(),
        authorization.as_str(),
    ));
    drop(authorization);
    let request_result = write_all_until(&mut stream, request.as_bytes(), deadline, cancelled);
    // Do not retain the bearer header while a Controller trickles its response.
    drop(request);
    request_result?;
    write_all_until(&mut stream, body, deadline, cancelled)?;

    let response = read_http_response_until(&mut stream, deadline, cancelled)?;
    ensure_probe_active(deadline, cancelled)?;
    parse_http_response(&response).map(Zeroizing::new)
}

fn write_all_until(
    stream: &mut ControllerConn,
    mut bytes: &[u8],
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<(), MihomoError> {
    while !bytes.is_empty() {
        let timeout = remaining_controller_timeout(deadline, cancelled)?.min(CONTROLLER_IO_POLL);
        stream.set_io_timeout(Some(timeout))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(MihomoError::Io(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Mihomo Controller closed while a request was being written",
                )))
            }
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => {
                // Windows can surface a peer reset at the same instant that
                // the watchdog cancels this probe. Cancellation is the
                // authoritative result and must not become a flaky,
                // platform-specific socket failure.
                ensure_probe_active(deadline, cancelled)?;
                return Err(MihomoError::Io(error));
            }
        }
    }
    Ok(())
}

fn read_http_response_until(
    stream: &mut ControllerConn,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Zeroizing<Vec<u8>>, MihomoError> {
    let mut response = Zeroizing::new(Vec::new());
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        let timeout = remaining_controller_timeout(deadline, cancelled)?.min(CONTROLLER_IO_POLL);
        stream.set_io_timeout(Some(timeout))?;
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                if response.len() > MAX_RESPONSE_BYTES {
                    return Err(MihomoError::InvalidResponse);
                }
                if http_response_is_complete(&response) {
                    break;
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::ConnectionReset
                    && http_response_is_complete(&response) =>
            {
                break;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => {
                ensure_probe_active(deadline, cancelled)?;
                return Err(MihomoError::Io(error));
            }
        }
    }
    if response.is_empty() {
        return Err(MihomoError::InvalidResponse);
    }
    Ok(response)
}

fn ensure_probe_active(deadline: Instant, cancelled: &AtomicBool) -> Result<(), MihomoError> {
    remaining_controller_timeout(deadline, cancelled).map(|_| ())
}

fn remaining_controller_timeout(
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Duration, MihomoError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(MihomoError::Io(io::Error::new(
            io::ErrorKind::Interrupted,
            "Mihomo runtime guard check was cancelled",
        )));
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(MihomoError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "Mihomo runtime guard check exceeded its total deadline",
        )));
    }
    Ok(remaining)
}

fn read_http_response(stream: &mut impl Read) -> Result<Vec<u8>, MihomoError> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                if response.len() > MAX_RESPONSE_BYTES {
                    return Err(MihomoError::InvalidResponse);
                }
                if http_response_is_complete(&response) {
                    break;
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::ConnectionReset
                    && http_response_is_complete(&response) =>
            {
                break;
            }
            Err(error) => return Err(MihomoError::Io(error)),
        }
    }
    if response.is_empty() {
        return Err(MihomoError::InvalidResponse);
    }
    Ok(response)
}

fn http_response_is_complete(response: &[u8]) -> bool {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let Ok(header) = std::str::from_utf8(&response[..header_end]) else {
        return false;
    };
    let headers = header
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<HashMap<_, _>>();
    let body = &response[header_end + 4..];
    if let Some(length) = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
    {
        return body.len() >= length;
    }
    headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked") && decode_chunked(body).is_ok())
}

fn parse_controller_target(controller_url: &str) -> Result<ControllerTarget, MihomoError> {
    let url = Url::parse(controller_url).map_err(|_| MihomoError::UnsafeController)?;
    if url.scheme() == "pipe" {
        let name = url.host_str().unwrap_or_default();
        if !WINDOWS_CONTROLLER_PIPES.contains(&name)
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || !(url.path() == "/" || url.path().is_empty())
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(MihomoError::UnsafeController);
        }
        #[cfg(not(windows))]
        {
            return Err(MihomoError::UnsafeController);
        }
        #[cfg(windows)]
        {
            return Ok(ControllerTarget::Pipe(name.to_owned()));
        }
    }
    let host = url
        .host_str()
        .ok_or(MihomoError::UnsafeController)?
        .trim_matches(['[', ']']);
    let ip = host
        .parse::<IpAddr>()
        .map_err(|_| MihomoError::UnsafeController)?;
    if url.scheme() != "http"
        || !ip.is_loopback()
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(MihomoError::UnsafeController);
    }
    Ok(ControllerTarget::Tcp(SocketAddr::new(
        ip,
        url.port().ok_or(MihomoError::UnsafeController)?,
    )))
}

fn connect_controller(
    target: &ControllerTarget,
    timeout: Duration,
) -> Result<ControllerConn, MihomoError> {
    match target {
        ControllerTarget::Tcp(endpoint) => {
            let stream =
                TcpStream::connect_timeout(endpoint, timeout).map_err(map_connect_error)?;
            Ok(ControllerConn::Tcp(stream))
        }
        ControllerTarget::Pipe(name) => {
            #[cfg(not(windows))]
            {
                let _ = name;
                Err(MihomoError::UnsafeController)
            }
            #[cfg(windows)]
            {
                connect_windows_pipe(name, timeout)
                    .map(ControllerConn::Pipe)
                    .map_err(map_connect_error)
            }
        }
    }
}

fn map_connect_error(error: io::Error) -> MihomoError {
    match error.kind() {
        io::ErrorKind::ConnectionRefused
        | io::ErrorKind::TimedOut
        | io::ErrorKind::NotFound
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::BrokenPipe => MihomoError::ControllerUnreachable,
        _ => MihomoError::Io(error),
    }
}

#[cfg(windows)]
fn connect_windows_pipe(name: &str, timeout: Duration) -> io::Result<File> {
    if !WINDOWS_CONTROLLER_PIPES.contains(&name) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Clash controller pipe is not allowlisted",
        ));
    }
    let path = format!(r"\\.\pipe\{name}");
    let deadline = Instant::now() + timeout;
    loop {
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => return Ok(file),
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                if Instant::now() >= deadline {
                    return Err(error);
                }
                thread::sleep(Duration::from_millis(15));
            }
            Err(error) => return Err(error),
        }
    }
}

impl ControllerTarget {
    fn host_header(&self) -> String {
        match self {
            Self::Tcp(endpoint) if endpoint.is_ipv6() => {
                format!("[{}]:{}", endpoint.ip(), endpoint.port())
            }
            Self::Tcp(endpoint) => format!("{}:{}", endpoint.ip(), endpoint.port()),
            Self::Pipe(_) => "localhost".to_owned(),
        }
    }
}

impl ControllerConn {
    fn set_io_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => {
                stream.set_read_timeout(timeout)?;
                stream.set_write_timeout(timeout)
            }
            #[cfg(windows)]
            Self::Pipe(_) => Ok(()),
        }
    }
}

impl Read for ControllerConn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buf),
            #[cfg(windows)]
            Self::Pipe(file) => file.read(buf),
        }
    }
}

impl Write for ControllerConn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write(buf),
            #[cfg(windows)]
            Self::Pipe(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush(),
            #[cfg(windows)]
            Self::Pipe(file) => file.flush(),
        }
    }
}

fn parse_http_response(response: &[u8]) -> Result<Vec<u8>, MihomoError> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(MihomoError::InvalidResponse)?;
    let header =
        std::str::from_utf8(&response[..header_end]).map_err(|_| MihomoError::InvalidResponse)?;
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(MihomoError::InvalidResponse)?;
    if !(200..300).contains(&status) {
        return Err(MihomoError::HttpStatus(status));
    }
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<HashMap<_, _>>();
    let body = &response[header_end + 4..];
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        decode_chunked(body)
    } else if let Some(length) = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
    {
        body.get(..length)
            .map(<[u8]>::to_vec)
            .ok_or(MihomoError::InvalidResponse)
    } else {
        Ok(body.to_vec())
    }
}

fn decode_chunked(mut body: &[u8]) -> Result<Vec<u8>, MihomoError> {
    let mut decoded = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or(MihomoError::InvalidResponse)?;
        let size_text = std::str::from_utf8(&body[..line_end])
            .map_err(|_| MihomoError::InvalidResponse)?
            .split(';')
            .next()
            .ok_or(MihomoError::InvalidResponse)?;
        let size = usize::from_str_radix(size_text.trim(), 16)
            .map_err(|_| MihomoError::InvalidResponse)?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(decoded);
        }
        let chunk = body.get(..size).ok_or(MihomoError::InvalidResponse)?;
        if decoded.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(MihomoError::InvalidResponse);
        }
        decoded.extend_from_slice(chunk);
        body = body.get(size + 2..).ok_or(MihomoError::InvalidResponse)?;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        thread,
        time::{Duration, Instant},
    };

    use super::{
        capture_runtime_guard, inspect_controller, verify_runtime_guard,
        verify_runtime_guard_until, MihomoControllerInput, MihomoError, MihomoRuntimeGuard,
    };
    use crate::domain::ExternalMihomoBinding;

    const RESPONSE_BEFORE: &str = r#"{"proxies":{"GLOBAL":{"type":"Selector","now":"old","all":["old","Tokyo 01"]},"old":{"type":"Socks5","alive":true,"history":[{"delay":120}]},"Tokyo 01":{"type":"Socks5","alive":true,"history":[{"delay":42}]}}}"#;
    const RESPONSE_AFTER: &str = r#"{"proxies":{"GLOBAL":{"type":"Selector","now":"Tokyo 01","all":["old","Tokyo 01"]},"old":{"type":"Socks5"},"Tokyo 01":{"type":"Socks5","alive":true,"history":[{"delay":42}]}}}"#;
    const GROUP_RESPONSE: &str = r#"{"proxies":{"PROXY":{"type":"Selector","now":"US-01","all":["US-01","DIRECT"]},"US-01":{"type":"Ss","alive":true},"DIRECT":{"type":"Direct","alive":true}}}"#;

    fn write_json(stream: &mut std::net::TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write fake controller response");
    }

    fn write_json_keep_alive(stream: &mut std::net::TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write keep-alive controller response");
        stream.flush().expect("flush keep-alive response");
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        let expected_length = loop {
            let read = stream.read(&mut buffer).expect("read controller request");
            assert!(read > 0, "controller request closed before headers");
            request.extend_from_slice(&buffer[..read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let header = std::str::from_utf8(&request[..header_end])
                .expect("controller request headers are UTF-8");
            let content_length = header
                .split("\r\n")
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("valid request content length")
                })
                .unwrap_or(0);
            break header_end + 4 + content_length;
        };
        while request.len() < expected_length {
            let read = stream.read(&mut buffer).expect("read controller body");
            assert!(read > 0, "controller request closed before body");
            request.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8(request).expect("controller request is UTF-8")
    }

    fn test_runtime_guard(controller: std::net::SocketAddr) -> MihomoRuntimeGuard {
        MihomoRuntimeGuard {
            controller: super::ControllerTarget::Tcp(controller),
            proxy_endpoint: "127.0.0.1:7891".parse().expect("proxy endpoint"),
            configuration: serde_json::json!({
                "mode": "global",
                "socks-port": 7891,
                "mixed-port": 0,
                "allow-lan": false
            }),
            pinned: None,
        }
    }

    fn test_binding(controller: std::net::SocketAddr) -> ExternalMihomoBinding {
        ExternalMihomoBinding {
            controller_url: format!("http://127.0.0.1:{}/", controller.port()),
            selector_group: "GLOBAL".to_owned(),
            node_name: "Tokyo 01".to_owned(),
            controller_secret_reference: None,
        }
    }

    #[test]
    fn controller_snapshot_and_binding_are_checked_on_loopback() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind fake controller");
        let address = listener.local_addr().expect("fake controller address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept controller request");
            let request_text = read_request(&mut stream);
            assert!(request_text.contains("Authorization: Bearer controller-secret"));
            write_json(&mut stream, RESPONSE_BEFORE);
        });

        let controller_url = format!("http://127.0.0.1:{}/", address.port());
        let snapshot = inspect_controller(&MihomoControllerInput {
            controller_url: controller_url.clone(),
            secret: "controller-secret".to_owned(),
        })
        .expect("inspect controller");
        assert_eq!(snapshot.groups[0].name, "GLOBAL");
        assert_eq!(snapshot.groups[0].nodes[0].delay_ms, Some(42));
        assert!(snapshot.providers.is_empty());

        server.join().expect("fake controller exits");
    }

    #[test]
    fn remote_controllers_are_rejected() {
        assert!(inspect_controller(&MihomoControllerInput {
            controller_url: "http://192.0.2.10:9090/".to_owned(),
            secret: String::new(),
        })
        .is_err());
    }

    #[test]
    fn mixed_proxy_ports_are_not_accepted_as_controllers() {
        let error = inspect_controller(&MihomoControllerInput {
            controller_url: "http://127.0.0.1:7897/".to_owned(),
            secret: String::new(),
        })
        .expect_err("7897 is a mixed proxy port");
        assert!(error.to_string().contains("7897"), "{error}");
        assert!(error.to_string().contains("9097"), "{error}");
    }

    #[test]
    fn closed_controller_port_does_not_surface_os_refusal_text() {
        let error = inspect_controller(&MihomoControllerInput {
            controller_url: "http://127.0.0.1:65534/".to_owned(),
            secret: String::new(),
        })
        .expect_err("closed controller port");
        let text = error.to_string();
        assert!(text.contains("没有可用的 Clash 控制口"), "{text}");
        assert!(!text.contains("积极拒绝"), "{text}");
        assert!(!text.contains("connection refused"), "{text}");
    }

    #[test]
    fn unknown_pipe_controller_urls_are_rejected() {
        assert!(inspect_controller(&MihomoControllerInput {
            controller_url: "pipe://not-a-clash-pipe/".to_owned(),
            secret: String::new(),
        })
        .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn clash_verge_pipe_url_is_allowlisted() {
        assert!(matches!(
            super::parse_controller_target("pipe://verge-mihomo/"),
            Ok(super::ControllerTarget::Pipe(name)) if name == "verge-mihomo"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn probe_and_inspect_use_verge_pipe_when_http_controller_is_closed() {
        if !super::pipe_looks_like_clash("verge-mihomo") {
            return;
        }
        let probe = super::probe_local_clash("");
        assert_eq!(
            probe.controller_url.as_deref(),
            Some("pipe://verge-mihomo/")
        );
        let snapshot = inspect_controller(&MihomoControllerInput {
            controller_url: "pipe://verge-mihomo/".to_owned(),
            secret: String::new(),
        })
        .expect("read groups over Clash Verge kernel pipe");
        assert!(
            !snapshot.groups.is_empty(),
            "Clash Verge pipe returned no selector groups"
        );
    }

    #[test]
    fn runtime_guard_accepts_live_verge_rule_mode_selector() {
        if !super::pipe_looks_like_clash("verge-mihomo") {
            return;
        }
        let probe = super::probe_local_clash("");
        let mixed = probe
            .mixed_port
            .expect("Clash Verge mixed port should be discoverable");
        let snapshot = inspect_controller(&MihomoControllerInput {
            controller_url: "pipe://verge-mihomo/".to_owned(),
            secret: String::new(),
        })
        .expect("read groups over Clash Verge kernel pipe");
        let group = snapshot
            .groups
            .iter()
            .find(|group| {
                group.selected.as_deref().is_some_and(|selected| {
                    group.nodes.iter().any(|node| {
                        node.name == selected
                            && !matches!(
                                node.proxy_type
                                    .as_deref()
                                    .unwrap_or_default()
                                    .to_ascii_lowercase()
                                    .as_str(),
                                "direct" | "reject" | "reject-drop" | "pass" | "compatible"
                            )
                    })
                })
            })
            .expect("Clash Verge should have a selector with a proxy node");
        let binding = ExternalMihomoBinding {
            controller_url: "pipe://verge-mihomo/".to_owned(),
            selector_group: group.name.clone(),
            node_name: group.selected.clone().expect("selected node"),
            controller_secret_reference: None,
        };
        capture_runtime_guard(&binding, "127.0.0.1", mixed, None).unwrap_or_else(|error| {
            panic!("live Clash Verge launch guard failed: {error}");
        });
    }

    #[cfg(windows)]
    #[test]
    fn live_verge_runs_two_isolated_silos_without_changing_main_clash() {
        if !super::pipe_looks_like_clash("verge-mihomo") {
            return;
        }
        let snapshot = inspect_controller(&MihomoControllerInput {
            controller_url: "pipe://verge-mihomo/".to_owned(),
            secret: String::new(),
        })
        .expect("read groups over Clash Verge kernel pipe");
        let group = snapshot
            .groups
            .iter()
            .find(|group| {
                group.selected.as_deref().is_some_and(|selected| {
                    group.nodes.iter().any(|node| {
                        node.name == selected
                            && !matches!(
                                node.proxy_type
                                    .as_deref()
                                    .unwrap_or_default()
                                    .to_ascii_lowercase()
                                    .as_str(),
                                "direct" | "reject" | "reject-drop" | "pass" | "compatible"
                            )
                    })
                })
            })
            .expect("Clash Verge should have a selector with a proxy node");
        let binding = ExternalMihomoBinding {
            controller_url: "pipe://verge-mihomo/".to_owned(),
            selector_group: group.name.clone(),
            node_name: group.selected.clone().expect("selected node"),
            controller_secret_reference: None,
        };
        let main_before = super::read_runtime_configuration_without_port(&binding, "")
            .expect("read main Clash mode");
        let selected_before = group.selected.clone();
        let first = super::pin_selected_inbound(&binding, None, uuid::Uuid::new_v4())
            .expect("start first isolated Mihomo");
        let second = super::pin_selected_inbound(&binding, None, uuid::Uuid::new_v4())
            .expect("start second isolated Mihomo");
        assert_ne!(first.port, second.port);
        let roots = [&first, &second].map(|inbound| {
            inbound
                .runtime
                .lock()
                .expect("isolated runtime")
                .root
                .clone()
        });
        assert!(roots.iter().all(|root| root.is_dir()));

        super::unpin_inbound(&binding, None, &first).expect("stop first isolated Mihomo");
        super::socks5_noauth_hello(second.port).expect("second Silo remains live");
        assert!(!roots[0].exists());
        assert!(roots[1].is_dir());
        super::unpin_inbound(&binding, None, &second).expect("stop second isolated Mihomo");
        assert!(!roots[1].exists());

        let main_after = super::read_runtime_configuration_without_port(&binding, "")
            .expect("read main Clash mode after isolated runs");
        let snapshot_after = inspect_controller(&MihomoControllerInput {
            controller_url: binding.controller_url.clone(),
            secret: String::new(),
        })
        .expect("read main Clash after isolated runs");
        let selected_after = snapshot_after
            .groups
            .iter()
            .find(|candidate| candidate.name == binding.selector_group)
            .and_then(|candidate| candidate.selected.clone());
        assert_eq!(main_after.get("mode"), main_before.get("mode"));
        assert_eq!(selected_after, selected_before);
    }

    #[test]
    fn runtime_guard_total_deadline_caps_a_slow_controller_response() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind slow controller");
        let address = listener.local_addr().expect("slow controller address");
        let (request_seen_tx, request_seen) = mpsc::channel();
        let (release_tx, release) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept guard request");
            let request = read_request(&mut stream);
            assert!(request.contains("Authorization: Bearer controller-secret"));
            request_seen_tx.send(()).expect("report guard request");
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{{",
                RESPONSE_AFTER.len()
            );
            let _ = release.recv_timeout(Duration::from_secs(2));
        });
        let guard = test_runtime_guard(address);
        let binding = test_binding(address);
        let authentication =
            crate::vault::MihomoControllerAuthentication::new("controller-secret".to_owned());
        let cancelled = AtomicBool::new(false);
        let started_at = Instant::now();

        let result = verify_runtime_guard_until(
            &guard,
            &binding,
            "127.0.0.1",
            7891,
            Some(&authentication),
            started_at + Duration::from_millis(150),
            &cancelled,
        );

        request_seen
            .recv_timeout(Duration::from_secs(1))
            .expect("bounded guard sent its request");
        assert!(matches!(
            result,
            Err(MihomoError::Io(ref error))
                if error.kind() == std::io::ErrorKind::TimedOut
        ));
        assert!(started_at.elapsed() < Duration::from_secs(1));
        let _ = release_tx.send(());
        server.join().expect("slow controller exits");
    }

    #[test]
    fn runtime_guard_accepts_complete_keep_alive_responses_without_waiting_for_eof() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("bind keep-alive controller");
        let address = listener.local_addr().expect("controller address");
        let (release_tx, release) = mpsc::channel();
        let release = Arc::new(std::sync::Mutex::new(release));
        let server = thread::spawn(move || {
            let mut handlers = Vec::new();
            for body in [
                RESPONSE_AFTER.to_owned(),
                r#"{"mode":"global","socks-port":7891,"mixed-port":0,"allow-lan":false}"#
                    .to_owned(),
            ] {
                let (mut stream, _) = listener.accept().expect("accept keep-alive request");
                let release = Arc::clone(&release);
                handlers.push(thread::spawn(move || {
                    let _ = read_request(&mut stream);
                    write_json_keep_alive(&mut stream, &body);
                    let _ = release
                        .lock()
                        .expect("keep-alive release lock")
                        .recv_timeout(Duration::from_secs(2));
                }));
            }
            for handler in handlers {
                handler.join().expect("keep-alive handler exits");
            }
        });
        let guard = test_runtime_guard(address);
        let binding = test_binding(address);
        let cancelled = AtomicBool::new(false);
        let started_at = Instant::now();

        let result = verify_runtime_guard_until(
            &guard,
            &binding,
            "127.0.0.1",
            7891,
            None,
            started_at + Duration::from_secs(1),
            &cancelled,
        );

        assert!(result.is_ok());
        let _ = release_tx.send(());
        let _ = release_tx.send(());
        server.join().expect("keep-alive controller exits");
    }

    #[test]
    fn runtime_guard_cancellation_interrupts_a_secret_bearing_request() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("bind cancellable controller");
        let address = listener.local_addr().expect("controller address");
        let (request_seen_tx, request_seen) = mpsc::channel();
        let (release_tx, release) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept guard request");
            let request = read_request(&mut stream);
            assert!(request.contains("Authorization: Bearer controller-secret"));
            request_seen_tx.send(()).expect("report guard request");
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{{",
                RESPONSE_AFTER.len()
            );
            let _ = release.recv_timeout(Duration::from_secs(2));
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let guard = test_runtime_guard(address);
        let binding = test_binding(address);
        let (result_tx, result_rx) = mpsc::channel();
        let probe = thread::spawn(move || {
            let authentication =
                crate::vault::MihomoControllerAuthentication::new("controller-secret".to_owned());
            let result = verify_runtime_guard_until(
                &guard,
                &binding,
                "127.0.0.1",
                7891,
                Some(&authentication),
                Instant::now() + Duration::from_secs(5),
                &worker_cancelled,
            );
            result_tx.send(result).expect("return cancelled result");
        });
        request_seen
            .recv_timeout(Duration::from_secs(1))
            .expect("secret-bearing request was sent");

        cancelled.store(true, Ordering::Release);

        let result = result_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("cancelled guard exits within one I/O poll");
        assert!(matches!(
            result,
            Err(MihomoError::Io(ref error))
                if error.kind() == std::io::ErrorKind::Interrupted
        ));
        let _ = release_tx.send(());
        probe.join().expect("cancelled probe exits");
        server.join().expect("cancellable controller exits");
    }

    #[test]
    fn required_runtime_guard_rejects_config_drift_without_rebinding() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind fake controller");
        let address = listener.local_addr().expect("fake controller address");
        let server = thread::spawn(move || {
            for index in 0..4 {
                let (mut stream, _) = listener.accept().expect("accept guard request");
                let request = read_request(&mut stream);
                if request.starts_with("GET /proxies HTTP/1.1") {
                    write_json(&mut stream, RESPONSE_AFTER);
                } else {
                    assert!(request.starts_with("GET /configs HTTP/1.1"));
                    let config = if index < 2 {
                        r#"{"mode":"global","socks-port":7891,"mixed-port":0,"allow-lan":false}"#
                    } else {
                        r#"{"mode":"global","socks-port":7891,"mixed-port":0,"allow-lan":true}"#
                    };
                    write_json(&mut stream, config);
                }
            }
        });
        let binding = ExternalMihomoBinding {
            controller_url: format!("http://127.0.0.1:{}/", address.port()),
            selector_group: "GLOBAL".to_owned(),
            node_name: "Tokyo 01".to_owned(),
            controller_secret_reference: None,
        };

        let guard = capture_runtime_guard(&binding, "127.0.0.1", 7891, None)
            .expect("capture safe global runtime");
        assert!(matches!(
            verify_runtime_guard(&guard, &binding, "127.0.0.1", 7891, None),
            Err(MihomoError::ConfigurationDrift)
        ));
        server.join().expect("fake controller exits");
    }

    #[test]
    fn required_runtime_guard_accepts_rule_mode_and_non_global_group() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind fake controller");
        let address = listener.local_addr().expect("fake controller address");
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept guard request");
                let request = read_request(&mut stream);
                if request.starts_with("GET /proxies HTTP/1.1") {
                    write_json(&mut stream, GROUP_RESPONSE);
                } else {
                    assert!(request.starts_with("GET /configs HTTP/1.1"));
                    write_json(
                        &mut stream,
                        r#"{"mode":"rule","socks-port":0,"mixed-port":7897}"#,
                    );
                }
            }
        });
        let binding = ExternalMihomoBinding {
            controller_url: format!("http://127.0.0.1:{}/", address.port()),
            selector_group: "PROXY".to_owned(),
            node_name: "US-01".to_owned(),
            controller_secret_reference: None,
        };

        capture_runtime_guard(&binding, "127.0.0.1", 7897, None)
            .expect("rule mode and a named selector group are enough for launch");
        server.join().expect("fake controller exits");
    }

    #[test]
    fn required_runtime_guard_rejects_clash_direct_mode() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind fake controller");
        let address = listener.local_addr().expect("fake controller address");
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept guard request");
                let request = read_request(&mut stream);
                if request.starts_with("GET /proxies HTTP/1.1") {
                    write_json(&mut stream, RESPONSE_AFTER);
                } else {
                    assert!(request.starts_with("GET /configs HTTP/1.1"));
                    write_json(
                        &mut stream,
                        r#"{"mode":"direct","socks-port":7891,"mixed-port":0}"#,
                    );
                }
            }
        });
        let binding = ExternalMihomoBinding {
            controller_url: format!("http://127.0.0.1:{}/", address.port()),
            selector_group: "GLOBAL".to_owned(),
            node_name: "Tokyo 01".to_owned(),
            controller_secret_reference: None,
        };

        assert!(matches!(
            capture_runtime_guard(&binding, "127.0.0.1", 7891, None),
            Err(MihomoError::DirectFallbackPossible)
        ));
        server.join().expect("fake controller exits");
    }

    #[test]
    fn isolated_mihomo_requires_a_supported_local_provider() {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind fake controller");
        let address = listener.local_addr().expect("fake controller address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept controller request");
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /proxies"));
            write_json(&mut stream, GROUP_RESPONSE);
        });
        let binding = ExternalMihomoBinding {
            controller_url: format!("http://127.0.0.1:{}/", address.port()),
            selector_group: "PROXY".to_owned(),
            node_name: "US-01".to_owned(),
            controller_secret_reference: None,
        };
        assert!(matches!(
            super::pin_selected_inbound(&binding, None, uuid::Uuid::nil()),
            Err(MihomoError::PinnedInboundUnsupported)
        ));
        server.join().expect("fake controller exits");
    }

    #[test]
    fn isolated_config_contains_only_the_selected_node_and_its_dependency() {
        let source: serde_yaml::Value = serde_yaml::from_str(
            r#"
proxies:
  - name: relay
    type: socks5
    server: relay.example
    port: 443
    password: relay-secret
  - name: US-01
    type: ss
    server: exit.example
    port: 8443
    cipher: aes-128-gcm
    password: node-secret
    dialer-proxy: relay
  - name: unrelated
    type: ss
    server: other.example
    port: 9443
    password: unrelated-secret
"#,
        )
        .expect("source YAML");
        let config = super::isolated_mihomo_config(&source, "US-01", "verisilo-test", 32123)
            .expect("minimal isolated config");
        let root = config.as_mapping().expect("root mapping");
        let proxies = root
            .get("proxies")
            .and_then(serde_yaml::Value::as_sequence)
            .expect("proxy sequence");
        let names: Vec<_> = proxies
            .iter()
            .filter_map(|proxy| proxy.as_mapping()?.get("name")?.as_str())
            .collect();
        assert_eq!(names, ["relay", "US-01"]);
        let listener = root
            .get("listeners")
            .and_then(serde_yaml::Value::as_sequence)
            .and_then(|listeners| listeners.first())
            .and_then(serde_yaml::Value::as_mapping)
            .expect("listener");
        assert_eq!(
            listener.get("listen").and_then(serde_yaml::Value::as_str),
            Some("127.0.0.1")
        );
        assert_eq!(
            listener.get("proxy").and_then(serde_yaml::Value::as_str),
            Some("US-01")
        );
        assert_eq!(
            listener.get("port").and_then(serde_yaml::Value::as_u64),
            Some(32123)
        );
    }
}
