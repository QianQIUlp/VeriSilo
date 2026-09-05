//! Loopback HTTP API for the running desktop. The CLI is a thin client of this
//! listener; Vault secrets never go in argv.

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::application::{
    create_managed_silo_with, delete_silo_with, desktop_status_with, diagnose_silo_with,
    initialize_vault_with, launch_silo_with, list_silos_with, lock_vault_with, page_action_with,
    stop_silo_with, unlock_vault_with, DesktopCore,
};
use crate::domain::{active_vault_name, app_data_root, CreateManagedSiloInput};
use crate::mihomo::diagnose_local_clash;

pub const DISCOVERY_FILE: &str = "local-api.json";
pub const DISCOVERY_SCHEMA: &str = "verisilo-local-api/v1";
const PREFERRED_PORT: u16 = 17300;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalApiDiscovery {
    pub schema: String,
    pub url: String,
    pub pid: u32,
    pub token: String,
    #[serde(default = "default_vault_name")]
    pub vault_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalApiInfo {
    pub url: String,
    pub pid: u32,
    pub discovery_path: PathBuf,
    pub cli_path: PathBuf,
    pub vault_name: String,
}

fn default_vault_name() -> String {
    crate::domain::DEFAULT_VAULT_NAME.to_owned()
}

pub struct VaultInstanceGuard {
    #[cfg(target_os = "windows")]
    handle: isize,
}

impl VaultInstanceGuard {
    pub fn acquire(vault_name: &str) -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            use std::ptr::null;
            use windows_sys::Win32::{
                Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS},
                System::Threading::CreateMutexW,
            };

            let name = format!("Local\\VeriSilo.Vault.{vault_name}")
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
            if handle.is_null() {
                return Err("无法创建 Vault 进程锁。".to_owned());
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(handle) };
                return Err(format!(
                    "Vault `{vault_name}` 已由另一个 VeriSilo 进程打开。"
                ));
            }
            Ok(Self {
                handle: handle as isize,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = vault_name;
            Ok(Self {})
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for VaultInstanceGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe { CloseHandle(self.handle as _) };
    }
}

pub struct LocalApiServer {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    discovery_path: PathBuf,
}

impl LocalApiServer {
    pub fn info(&self, url: String) -> LocalApiInfo {
        LocalApiInfo {
            url,
            pid: std::process::id(),
            discovery_path: self.discovery_path.clone(),
            cli_path: sibling_cli_path(),
            vault_name: active_vault_name().unwrap_or_else(|_| default_vault_name()),
        }
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.discovery_path);
    }
}

impl Drop for LocalApiServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn discovery_path() -> Result<PathBuf, String> {
    Ok(app_data_root()
        .map_err(|error| error.to_string())?
        .join(DISCOVERY_FILE))
}

pub fn load_discovery() -> Result<LocalApiDiscovery, String> {
    let path = discovery_path()?;
    let raw = fs::read(&path).map_err(|_| "没有找到 VeriSilo 本机服务。".to_owned())?;
    let discovery: LocalApiDiscovery = serde_json::from_slice(&raw)
        .map_err(|_| "本机 API 发现文件无法读取。请重启 VeriSilo 桌面应用。".to_owned())?;
    if discovery.schema != DISCOVERY_SCHEMA {
        return Err("本机 API 版本不匹配。请更新 CLI 或重启桌面应用。".to_owned());
    }
    if discovery.vault_name != active_vault_name().map_err(|error| error.to_string())? {
        return Err("本机 API 发现文件属于另一个 Vault。".to_owned());
    }
    Ok(discovery)
}

pub fn request_existing_app_open() -> bool {
    let Ok(discovery) = load_discovery() else {
        return false;
    };
    let Ok(address) = discovery
        .url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .parse::<SocketAddr>()
    else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(250)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = format!(
        "POST /v1/app/open HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        discovery.token
    );
    if stream.write_all(request.as_bytes()).is_err() || stream.flush().is_err() {
        return false;
    }
    let mut response = Vec::new();
    stream.read_to_end(&mut response).is_ok() && response.starts_with(b"HTTP/1.1 200")
}

pub fn sibling_cli_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("verisilo-cli.exe")))
        .unwrap_or_else(|| PathBuf::from("verisilo-cli.exe"))
}

pub fn spawn<R: Runtime>(app: AppHandle<R>) -> Result<(LocalApiServer, String), String> {
    let listener = bind_loopback()?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("无法读取本机 API 端口：{error}"))?;
    let url = format!("http://127.0.0.1:{}/", address.port());
    let token = random_token();
    let discovery_path = discovery_path()?;
    let discovery = LocalApiDiscovery {
        schema: DISCOVERY_SCHEMA.to_owned(),
        url: url.clone(),
        pid: std::process::id(),
        token: token.clone(),
        vault_name: active_vault_name().map_err(|error| error.to_string())?,
    };
    write_discovery(&discovery_path, &discovery)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("无法设置本机 API 监听：{error}"))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let thread = thread::Builder::new()
        .name("verisilo-local-api".to_owned())
        .spawn(move || serve(listener, app, token, thread_shutdown))
        .map_err(|error| format!("无法启动本机 API：{error}"))?;
    Ok((
        LocalApiServer {
            shutdown,
            thread: Some(thread),
            discovery_path,
        },
        url,
    ))
}

fn bind_loopback() -> Result<TcpListener, String> {
    TcpListener::bind(("127.0.0.1", PREFERRED_PORT))
        .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
        .map_err(|error| format!("无法绑定本机 API：{error}"))
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            out.push_str(&format!("{byte:02x}"));
            out
        })
}

fn write_discovery(path: &PathBuf, discovery: &LocalApiDiscovery) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let encoded = serde_json::to_vec_pretty(discovery).map_err(|error| error.to_string())?;
    fs::write(path, encoded).map_err(|error| error.to_string())
}

fn serve<R: Runtime>(
    listener: TcpListener,
    app: AppHandle<R>,
    token: String,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, address)) => {
                if !address.ip().is_loopback() {
                    continue;
                }
                let app = app.clone();
                let token = token.clone();
                thread::spawn(move || {
                    let _ = handle_connection(stream, &app, &token);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn handle_connection<R: Runtime>(
    mut stream: TcpStream,
    app: &AppHandle<R>,
    token: &str,
) -> Result<(), ()> {
    let _ = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
    let mut request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            write_json(&mut stream, 400, &error_body(&error));
            return Ok(());
        }
    };
    if request.path == "/v1/health" && request.method == "GET" {
        request.body.zeroize();
        write_json(
            &mut stream,
            200,
            &serde_json::json!({"ok": true, "name": "verisilo-local-api"}),
        );
        return Ok(());
    }
    if request.token.as_deref() != Some(token) {
        request.body.zeroize();
        write_json(
            &mut stream,
            401,
            &error_body("本机 API 需要桌面发现文件中的令牌。"),
        );
        return Ok(());
    }
    if request.path == "/v1/service/stop" && request.method == "POST" {
        request.body.zeroize();
        write_json(
            &mut stream,
            200,
            &ok_body(serde_json::json!({"stopping": true})),
        );
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            crate::exit_from_tray(&app);
        });
        return Ok(());
    }
    if request.path == "/v1/app" && request.method == "GET" {
        request.body.zeroize();
        let window = app.get_webview_window("main");
        let visible = window
            .as_ref()
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);
        let size = window.as_ref().and_then(|window| window.outer_size().ok());
        write_json(
            &mut stream,
            200,
            &ok_body(serde_json::json!({
                "created": window.is_some(),
                "visible": visible,
                "width": size.as_ref().map(|size| size.width),
                "height": size.as_ref().map(|size| size.height),
            })),
        );
        return Ok(());
    }
    if request.path == "/v1/app/open" && request.method == "POST" {
        request.body.zeroize();
        let handle = app.clone();
        let result = app.run_on_main_thread(move || crate::show_main_window(&handle));
        write_json(
            &mut stream,
            if result.is_ok() { 200 } else { 500 },
            &match result {
                Ok(()) => ok_body(serde_json::json!({"opening": true})),
                Err(error) => error_body(&format!("无法打开 VeriSilo 窗口：{error}")),
            },
        );
        return Ok(());
    }
    if request.path == "/v1/app/hide" && request.method == "POST" {
        request.body.zeroize();
        let handle = app.clone();
        let result = app.run_on_main_thread(move || {
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.hide();
            }
        });
        write_json(
            &mut stream,
            if result.is_ok() { 200 } else { 500 },
            &match result {
                Ok(()) => ok_body(serde_json::json!({"hiding": true})),
                Err(error) => error_body(&format!("无法隐藏 VeriSilo 窗口：{error}")),
            },
        );
        return Ok(());
    }
    let state = app.state::<crate::AppState>();
    let (status, body) = dispatch(&state.core, &request);
    request.body.zeroize();
    write_json(&mut stream, status, &body);
    Ok(())
}

struct ApiRequest {
    method: String,
    path: String,
    token: Option<String>,
    body: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultPassphraseInput {
    passphrase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PermanentDeleteInput {
    confirm_permanent: bool,
}

fn read_request(stream: &mut TcpStream) -> Result<ApiRequest, String> {
    let mut raw = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| format!("无法读取本机 API 请求：{error}"))?;
        if read == 0 {
            return Err("本机 API 请求不完整。".to_owned());
        }
        raw.extend_from_slice(&buffer[..read]);
        if raw.len() > MAX_HEADER_BYTES + MAX_BODY_BYTES {
            return Err("本机 API 请求过大。".to_owned());
        }
        if let Some(index) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            break index;
        }
        if raw.len() > MAX_HEADER_BYTES {
            return Err("本机 API 请求头过大。".to_owned());
        }
    };
    let header = std::str::from_utf8(&raw[..header_end])
        .map_err(|_| "本机 API 请求头不是 UTF-8。".to_owned())?;
    let mut lines = header.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "本机 API 请求行为空。".to_owned())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "本机 API 方法缺失。".to_owned())?
        .to_ascii_uppercase();
    let path = parts
        .next()
        .ok_or_else(|| "本机 API 路径缺失。".to_owned())?
        .to_owned();
    let mut content_length = 0_usize;
    let mut token = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value
                .parse()
                .map_err(|_| "本机 API Content-Length 无效。".to_owned())?;
        } else if name.eq_ignore_ascii_case("authorization") {
            token = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .map(str::to_owned);
        } else if name.eq_ignore_ascii_case("x-verisilo-token") {
            token = Some(value.to_owned());
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err("本机 API 请求体过大。".to_owned());
    }
    let mut body = raw[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| format!("无法读取本机 API 请求体：{error}"))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
        if body.len() > MAX_BODY_BYTES {
            return Err("本机 API 请求体过大。".to_owned());
        }
    }
    body.truncate(content_length);
    Ok(ApiRequest {
        method,
        path,
        token,
        body,
    })
}

fn dispatch(state: &DesktopCore, request: &ApiRequest) -> (u16, serde_json::Value) {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/status") => map_result(desktop_status_with(state)),
        ("POST", "/v1/vault/initialize") => dispatch_vault_passphrase(state, request, true),
        ("POST", "/v1/vault/unlock") => dispatch_vault_passphrase(state, request, false),
        ("POST", "/v1/vault/lock") => map_result(lock_vault_with(state)),
        ("GET", "/v1/silos") => map_result(list_silos_with(state).map_err(map_locked)),
        ("POST", "/v1/silos") => {
            match serde_json::from_slice::<CreateManagedSiloInput>(&request.body) {
                Ok(input) => map_result(create_managed_silo_with(state, input)),
                Err(error) => (400, error_body(&format!("创建参数无效：{error}"))),
            }
        }
        ("GET", "/v1/clash") => (200, ok_body(diagnose_local_clash(""))),
        ("GET", "/v1/cli") => (
            200,
            ok_body(LocalApiInfo {
                url: String::new(),
                pid: std::process::id(),
                discovery_path: discovery_path().unwrap_or_default(),
                cli_path: sibling_cli_path(),
                vault_name: active_vault_name().unwrap_or_else(|_| default_vault_name()),
            }),
        ),
        (method, path) => {
            if let Some((silo, action)) = parse_silo_path(path) {
                return dispatch_silo(state, method, &silo, action, &request.body);
            }
            (404, error_body("没有这个本机 API 路径。"))
        }
    }
}

fn dispatch_vault_passphrase(
    state: &DesktopCore,
    request: &ApiRequest,
    initialize: bool,
) -> (u16, serde_json::Value) {
    let input = match serde_json::from_slice::<VaultPassphraseInput>(&request.body) {
        Ok(input) if !input.passphrase.is_empty() => input,
        Ok(_) => return (400, error_body("保险库口令不能为空。")),
        Err(error) => return (400, error_body(&format!("保险库参数无效：{error}"))),
    };
    let passphrase = Zeroizing::new(input.passphrase);
    map_result(if initialize {
        initialize_vault_with(state, &passphrase)
    } else {
        unlock_vault_with(state, &passphrase)
    })
}

fn dispatch_silo(
    state: &DesktopCore,
    method: &str,
    spec: &str,
    action: Option<&str>,
    body: &[u8],
) -> (u16, serde_json::Value) {
    match (method, action) {
        ("GET", None) => match resolve_silo(state, spec) {
            Ok(silo) => (200, ok_body(silo)),
            Err(error) => error_status(error),
        },
        ("GET", Some("diagnose")) => match resolve_silo_id(state, spec) {
            Ok(id) => map_result(diagnose_silo_with(state, id)),
            Err(error) => error_status(error),
        },
        ("POST", Some("start")) => match resolve_silo_id(state, spec) {
            Ok(id) => map_result(launch_silo_with(state, id)),
            Err(error) => error_status(error),
        },
        ("POST", Some("stop")) => match resolve_silo_id(state, spec) {
            Ok(id) => map_result(stop_silo_with(state, id)),
            Err(error) => error_status(error),
        },
        ("DELETE", None) => match serde_json::from_slice::<PermanentDeleteInput>(body) {
            Ok(input) if input.confirm_permanent => match resolve_silo_id(state, spec) {
                Ok(id) => map_result(delete_silo_with(state, id, true)),
                Err(error) => error_status(error),
            },
            _ => (400, error_body("永久删除需要明确确认。")),
        },
        ("POST", Some("page")) => match resolve_silo_id(state, spec) {
            Ok(id) => match serde_json::from_slice::<serde_json::Value>(body) {
                Ok(action) if action.is_object() => map_result(page_action_with(state, id, action)),
                Ok(_) => (400, error_body("页面动作必须是 JSON 对象。")),
                Err(error) => (400, error_body(&format!("页面动作参数无效：{error}"))),
            },
            Err(error) => error_status(error),
        },
        _ => (404, error_body("没有这个本机 API 路径。")),
    }
}

fn parse_silo_path(path: &str) -> Option<(String, Option<&str>)> {
    let rest = path.strip_prefix("/v1/silos/")?;
    if rest.is_empty() {
        return None;
    }
    let mut parts = rest.splitn(2, '/');
    let spec = percent_decode(parts.next()?);
    let action = parts.next();
    Some((spec, action))
}

fn percent_decode(value: &str) -> String {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn resolve_silo_id(state: &DesktopCore, spec: &str) -> Result<Uuid, String> {
    Ok(resolve_silo(state, spec)?.id)
}

fn resolve_silo(state: &DesktopCore, spec: &str) -> Result<crate::domain::Silo, String> {
    let silos = list_silos_with(state).map_err(map_locked)?;
    if let Ok(id) = Uuid::parse_str(spec) {
        return silos
            .into_iter()
            .find(|silo| silo.id == id)
            .ok_or_else(|| format!("没有找到 Silo {spec}。"));
    }
    let matches: Vec<_> = silos.into_iter().filter(|silo| silo.name == spec).collect();
    match matches.len() {
        0 => Err(format!("没有找到名为「{spec}」的 Silo。")),
        1 => Ok(matches.into_iter().next().expect("one match")),
        _ => Err(format!("有多个 Silo 同名「{spec}」，请改用 id。")),
    }
}

fn map_locked(error: String) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("locked") || error.contains("保险库") {
        "保险库已锁定。请先运行 verisilo-cli vault unlock。".to_owned()
    } else {
        error
    }
}

fn map_result<T: Serialize>(result: Result<T, String>) -> (u16, serde_json::Value) {
    match result {
        Ok(value) => (200, ok_body(value)),
        Err(error) => error_status(error),
    }
}

fn error_status(error: String) -> (u16, serde_json::Value) {
    let status = if error.contains("锁定") {
        409
    } else if error.contains("没有找到") {
        404
    } else {
        400
    };
    (status, error_body(&error))
}

fn ok_body<T: Serialize>(value: T) -> serde_json::Value {
    serde_json::json!({ "ok": true, "data": value })
}

fn error_body(error: &str) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": error })
}

fn write_json(stream: &mut TcpStream, status: u16, body: &serde_json::Value) {
    let encoded = serde_json::to_vec(body).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        encoded.len()
    );
    let _ = stream.write_all(&encoded);
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::{parse_silo_path, percent_decode};

    #[test]
    fn silo_paths_split_id_and_action() {
        assert_eq!(
            parse_silo_path("/v1/silos/abc/start"),
            Some(("abc".to_owned(), Some("start")))
        );
        assert_eq!(
            parse_silo_path("/v1/silos/%E7%BE%8E%E5%9B%BD"),
            Some(("美国".to_owned(), None))
        );
        assert_eq!(parse_silo_path("/v1/silos/"), None);
    }

    #[test]
    fn percent_decode_keeps_ascii() {
        assert_eq!(percent_decode("shop-1"), "shop-1");
    }
}
