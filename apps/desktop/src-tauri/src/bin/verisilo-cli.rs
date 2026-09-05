//! Thin loopback client for the running VeriSilo desktop.
//! Vault passphrase is never accepted on the command line.

use std::{
    env,
    io::{self, Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};
use verisilo_desktop_lib::local_api::load_discovery;
use verisilo_desktop_lib::domain::{
    active_vault_name, available_vault_names, select_vault_name, DEFAULT_VAULT_NAME,
};
use zeroize::Zeroize;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let (vault_name, args) = extract_vault_flag(args)?;
    select_vault_name(&vault_name).map_err(|error| error.to_string())?;
    let (json_out, args) = split_json_flag(args);
    let command = args.first().map(String::as_str).unwrap_or("help");
    match command {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        "vault" => vault_command(&args, json_out),
        "app" => app_command(&args, json_out),
        "service" => service_command(&args, json_out),
        "status" => print_value(api("GET", "/v1/status", None)?, json_out, print_status),
        "identity" => print_value(api("GET", "/v1/status", None)?, json_out, print_identity),
        "silos" => print_value(api("GET", "/v1/silos", None)?, json_out, print_silos),
        "clash" => print_value(api("GET", "/v1/clash", None)?, json_out, print_clash),
        "diagnose" => {
            let spec = args.get(1).map(String::as_str).unwrap_or("");
            if spec.is_empty() {
                print_value(api("GET", "/v1/clash", None)?, json_out, print_clash)?;
                return match api("GET", "/v1/silos", None) {
                    Ok(silos) => print_value(silos, json_out, print_silos),
                    Err(error) => {
                        eprintln!("{error}");
                        Ok(())
                    }
                };
            }
            let path = format!("/v1/silos/{}/diagnose", url_encode(spec));
            print_value(api("GET", &path, None)?, json_out, print_diagnose)
        }
        "start" => {
            let spec = require_spec(&args, "start")?;
            let path = format!("/v1/silos/{}/start", url_encode(&spec));
            print_value(api("POST", &path, None)?, json_out, print_activation)
        }
        "stop" => {
            let spec = require_spec(&args, "stop")?;
            let path = format!("/v1/silos/{}/stop", url_encode(&spec));
            print_value(api("POST", &path, None)?, json_out, print_activation)
        }
        "delete" => {
            let spec = require_spec(&args, "delete")?;
            if !args.iter().any(|arg| arg == "--yes") {
                return Err("永久删除需要加 --yes。".to_owned());
            }
            let path = format!("/v1/silos/{}", url_encode(&spec));
            api("DELETE", &path, Some(&json!({"confirmPermanent": true})))?;
            if json_out {
                println!("{}", json!({"deleted": true, "silo": spec}));
            } else {
                println!("已永久删除「{spec}」。");
            }
            Ok(())
        }
        "create" => {
            let body = create_payload(&args)?;
            print_value(api("POST", "/v1/silos", Some(&body))?, json_out, print_created)
        }
        "create-batch" => create_batch(&args, json_out),
        "page" => page_command(&args, json_out),
        other => Err(format!("未知命令 `{other}`。运行 verisilo-cli help。")),
    }
}

fn app_command(args: &[String], json_out: bool) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("status") => print_value(api("GET", "/v1/app", None)?, json_out, print_app),
        Some("open") => {
            api("GET", "/v1/app", None)?;
            activate_desktop()?;
            print_value(wait_for_app_visibility(true)?, json_out, print_app)
        }
        Some("hide") => {
            api("POST", "/v1/app/hide", None)?;
            print_value(wait_for_app_visibility(false)?, json_out, print_app)
        }
        _ => Err("用法：verisilo-cli app status|open|hide".to_owned()),
    }
}

fn activate_desktop() -> Result<(), String> {
    let desktop = sibling_desktop_path()?;
    let mut command = Command::new(&desktop);
    let vault_name = active_vault_name().map_err(|error| error.to_string())?;
    if vault_name != DEFAULT_VAULT_NAME {
        command.arg("--vault").arg(vault_name);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 {}：{error}", desktop.display()))
}

fn wait_for_app_visibility(visible: bool) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let status = api("GET", "/v1/app", None)?;
        if status.get("visible").and_then(Value::as_bool) == Some(visible) {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(if visible {
        "VeriSilo 桌面窗口没有在 15 秒内打开。"
    } else {
        "VeriSilo 桌面窗口没有在 15 秒内隐藏。"
    }
    .to_owned())
}

fn extract_vault_flag(args: Vec<String>) -> Result<(String, Vec<String>), String> {
    let mut vault_name = DEFAULT_VAULT_NAME.to_owned();
    let mut remaining = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--vault" {
            vault_name = iter
                .next()
                .ok_or_else(|| "--vault 后面需要 Vault 名称。".to_owned())?;
        } else {
            remaining.push(arg);
        }
    }
    Ok((vault_name, remaining))
}

fn split_json_flag(args: Vec<String>) -> (bool, Vec<String>) {
    let json_out = args.iter().any(|arg| arg == "--json");
    let args = args.into_iter().filter(|arg| arg != "--json").collect();
    (json_out, args)
}

fn require_spec(args: &[String], command: &str) -> Result<String, String> {
    args.get(1)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| format!("用法：verisilo-cli {command} <silo名称或id>"))
}

fn vault_command(args: &[String], json_out: bool) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("list") => {
            let active = active_vault_name().map_err(|error| error.to_string())?;
            let names = available_vault_names().map_err(|error| error.to_string())?;
            if json_out {
                println!("{}", json!({"active": active, "vaults": names}));
            } else {
                for name in names {
                    println!("{}{}", if name == active { "* " } else { "  " }, name);
                }
            }
            Ok(())
        }
        Some("init") => {
            let mut passphrase = read_secret("新保险库口令：")?;
            let mut confirmation = read_secret("再次输入：")?;
            if passphrase != confirmation {
                passphrase.zeroize();
                confirmation.zeroize();
                return Err("两次输入的保险库口令不一致。".to_owned());
            }
            confirmation.zeroize();
            print_value(
                vault_passphrase_request("/v1/vault/initialize", passphrase)?,
                json_out,
                print_vault,
            )
        }
        Some("unlock") => {
            let passphrase = read_secret("保险库口令：")?;
            print_value(
                vault_passphrase_request("/v1/vault/unlock", passphrase)?,
                json_out,
                print_vault,
            )
        }
        Some("lock") => print_value(
            api("POST", "/v1/vault/lock", None)?,
            json_out,
            print_vault,
        ),
        _ => Err("用法：verisilo-cli vault list|init|unlock|lock".to_owned()),
    }
}

fn service_command(args: &[String], json_out: bool) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("stop") => {
            let previous = load_discovery().ok();
            let value = api("POST", "/v1/service/stop", None)?;
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline && same_service(previous.as_ref()) {
                thread::sleep(Duration::from_millis(50));
            }
            if same_service(previous.as_ref()) {
                return Err("VeriSilo 本机服务没有在 15 秒内退出。".to_owned());
            }
            print_value(value, json_out, |_| {
                println!("本机服务已退出。");
                Ok(())
            })
        }
        _ => Err("用法：verisilo-cli service stop".to_owned()),
    }
}

fn same_service(previous: Option<&verisilo_desktop_lib::local_api::LocalApiDiscovery>) -> bool {
    load_discovery().ok().is_some_and(|current| {
        previous.is_some_and(|previous| {
            current.pid == previous.pid && current.token == previous.token
        })
    })
}

fn vault_passphrase_request(path: &str, mut passphrase: String) -> Result<Value, String> {
    let mut body = json!({"passphrase": std::mem::take(&mut passphrase)});
    let result = api("POST", path, Some(&body));
    if let Some(Value::String(secret)) = body.get_mut("passphrase") {
        secret.zeroize();
    }
    passphrase.zeroize();
    result
}

fn read_secret(prompt: &str) -> Result<String, String> {
    eprint!("{prompt}");
    io::stderr().flush().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let console = unsafe {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, STD_INPUT_HANDLE,
        };
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT);
            Some((handle, mode))
        } else {
            None
        }
    };
    let mut value = String::new();
    let read = io::stdin().read_line(&mut value);
    #[cfg(windows)]
    if let Some((handle, mode)) = console {
        unsafe {
            use windows_sys::Win32::System::Console::SetConsoleMode;
            let _ = SetConsoleMode(handle, mode);
        }
        eprintln!();
    }
    read.map_err(|error| error.to_string())?;
    while value.ends_with('\r') || value.ends_with('\n') {
        value.pop();
    }
    if value.is_empty() {
        return Err("保险库口令不能为空。".to_owned());
    }
    Ok(value)
}

fn print_help() {
    println!(
        "\
VeriSilo 本机命令行（需要时自动在后台启动本机服务）

  verisilo-cli [--vault 名称] vault list|init|unlock|lock
  verisilo-cli [--vault 名称] app status|open|hide
  verisilo-cli [--vault 名称] service stop
  verisilo-cli status
  verisilo-cli identity
  verisilo-cli silos
  verisilo-cli clash
  verisilo-cli diagnose [名称或id]
  verisilo-cli start <名称或id>
  verisilo-cli stop <名称或id>
  verisilo-cli delete <名称或id> --yes
  verisilo-cli create --name <名称> [--network direct|clash] [--mixed-port 7897] [--group 组] [--node 节点] [--preset balanced-zh-cn]
  verisilo-cli create --request                         # 从 stdin 读取完整 JSON
  verisilo-cli create-batch --prefix <名称> --count <数量> [create 的网络与身份参数]
  verisilo-cli create-batch --request                   # 从 stdin 读取 JSON 数组
  verisilo-cli page <名称或id> snapshot
  verisilo-cli page <名称或id> goto <https://...>
  verisilo-cli page <名称或id> click <selector>
  verisilo-cli page <名称或id> fill <selector>        # 从 stdin 读取值
  verisilo-cli page <名称或id> press <key> [selector]
  verisilo-cli page <名称或id> evaluate               # 从 stdin 读取 JavaScript
  verisilo-cli page <名称或id> screenshot
  verisilo-cli page <名称或id> windows                # Windows 窗口数量、位置和尺寸
  verisilo-cli page <名称或id> request                # 从 stdin 读取完整 JSON 动作

在任意命令加 --vault agent 可选择独立 Vault；default 保持原来的数据目录。
加上 --json 输出原始 JSON。保险库口令只从隐藏输入读取，不进入命令行参数。"
    );
}

fn api(method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let (discovery, mut stream) = api_connection()?;
    let host_port = discovery
        .url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();
    let mut payload = body.map(ToString::to_string).unwrap_or_default();
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let write_result = write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        discovery.token,
        payload.len()
    );
    payload.zeroize();
    write_result.map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| "本机 API 返回了空响应。".to_owned())?;
    let value: Value = serde_json::from_str(body.trim())
        .map_err(|_| format!("本机 API 返回了无法解析的内容：{}", body.trim()))?;
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(value.get("data").cloned().unwrap_or(Value::Null))
    } else {
        Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("本机 API 请求失败。")
            .to_owned())
    }
}

fn api_connection() -> Result<(verisilo_desktop_lib::local_api::LocalApiDiscovery, TcpStream), String> {
    let stale_discovery = load_discovery().ok();
    if let Ok(connection) = connect_discovered_api() {
        return Ok(connection);
    }
    start_background_service()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(discovery) = load_discovery() {
            if !discovery_changed(stale_discovery.as_ref(), &discovery) {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            let host_port = discovery
                .url
                .trim_start_matches("http://")
                .trim_end_matches('/');
            if let Ok(stream) = TcpStream::connect(host_port) {
                return Ok((discovery, stream));
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("VeriSilo 本机服务没有在 15 秒内启动。".to_owned())
}

fn discovery_changed(
    old: Option<&verisilo_desktop_lib::local_api::LocalApiDiscovery>,
    new: &verisilo_desktop_lib::local_api::LocalApiDiscovery,
) -> bool {
    old.is_none_or(|old| old.pid != new.pid || old.token != new.token)
}

fn connect_discovered_api(
) -> Result<(verisilo_desktop_lib::local_api::LocalApiDiscovery, TcpStream), String> {
    let discovery = load_discovery()?;
    let host_port = discovery
        .url
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let stream = TcpStream::connect(host_port).map_err(|error| error.to_string())?;
    Ok((discovery, stream))
}

fn start_background_service() -> Result<(), String> {
    let desktop = sibling_desktop_path()?;
    let mut command = Command::new(&desktop);
    command.arg("--cli-background");
    let vault_name = active_vault_name().map_err(|error| error.to_string())?;
    if vault_name != DEFAULT_VAULT_NAME {
        command.arg("--vault").arg(vault_name);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法启动 {}：{error}", desktop.display()))
}

fn sibling_desktop_path() -> Result<PathBuf, String> {
    let current = env::current_exe().map_err(|error| error.to_string())?;
    let name = if cfg!(windows) { "verisilo.exe" } else { "verisilo" };
    let path = current
        .parent()
        .map(|parent| parent.join(name))
        .ok_or_else(|| "无法定位 VeriSilo 桌面程序。".to_owned())?;
    if !path.is_file() {
        return Err(format!("没有找到 {}。", path.display()));
    }
    Ok(path)
}

fn create_payload(args: &[String]) -> Result<Value, String> {
    if args.iter().any(|arg| arg == "--request") {
        let value: Value = serde_json::from_str(&read_stdin_text()?)
            .map_err(|error| format!("create request JSON 无效：{error}"))?;
        if !value.is_object() {
            return Err("create request 必须是 JSON 对象。".to_owned());
        }
        return Ok(value);
    }
    let name = flag(args, "--name").ok_or_else(|| "用法：verisilo-cli create --name <名称>".to_owned())?;
    let network = flag(args, "--network").unwrap_or_else(|| "clash".to_owned());
    let preset = flag(args, "--preset").unwrap_or_else(|| {
        if network == "clash" {
            "balanced-zh-cn".to_owned()
        } else {
            "balanced-en-us".to_owned()
        }
    });
    let follow = network != "direct";
    let network_profile = match network.as_str() {
        "direct" => json!({ "mode": "direct", "proxyRequired": false }),
        "clash" => clash_profile(args)?,
        _ => {
            return Err("create --network 只支持 direct 或 clash。".to_owned());
        }
    };
    Ok(json!({
        "name": name,
        "color": "#5b5ce2",
        "identityPreset": preset,
        "followNetworkExit": follow,
        "networkProfile": network_profile,
    }))
}

fn create_batch(args: &[String], json_out: bool) -> Result<(), String> {
    let bodies = if args.iter().any(|arg| arg == "--request") {
        let value: Value = serde_json::from_str(&read_stdin_text()?)
            .map_err(|error| format!("create-batch request JSON 无效：{error}"))?;
        let values = value
            .as_array()
            .filter(|values| (1..=100).contains(&values.len()))
            .ok_or_else(|| "create-batch request 必须是包含 1 到 100 个对象的 JSON 数组。".to_owned())?;
        if values.iter().any(|value| !value.is_object()) {
            return Err("create-batch request 中的每一项都必须是 JSON 对象。".to_owned());
        }
        values.clone()
    } else {
        let prefix = flag(args, "--prefix").ok_or_else(|| {
            "用法：verisilo-cli create-batch --prefix <名称> --count <数量>".to_owned()
        })?;
        let count = flag(args, "--count")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|count| (1..=100).contains(count))
            .ok_or_else(|| "create-batch --count 必须在 1 到 100 之间。".to_owned())?;
        (1..=count)
            .map(|index| {
                let mut item_args = args.to_vec();
                item_args.push("--name".to_owned());
                item_args.push(format!("{prefix}-{index}"));
                create_payload(&item_args)
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut created = Vec::with_capacity(bodies.len());
    for (offset, body) in bodies.iter().enumerate() {
        let index = offset + 1;
        match api("POST", "/v1/silos", Some(body)) {
            Ok(silo) => {
                if !json_out {
                    print_created(&silo)?;
                }
                created.push(silo);
            }
            Err(error) => {
                return Err(format!("批量创建在第 {index} 个失败（此前已创建 {} 个）：{error}", created.len()));
            }
        }
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&created).map_err(|error| error.to_string())?);
    }
    Ok(())
}

fn page_command(args: &[String], json_out: bool) -> Result<(), String> {
    let spec = require_spec(args, "page")?;
    let action = args.get(2).map(String::as_str).unwrap_or("snapshot");
    let body = match action {
        "snapshot" | "screenshot" | "windows" => json!({"action": action}),
        "goto" => json!({
            "action": "goto",
            "url": args.get(3).ok_or_else(|| "page goto 需要 URL。".to_owned())?,
        }),
        "click" => json!({
            "action": "click",
            "selector": args.get(3).ok_or_else(|| "page click 需要 selector。".to_owned())?,
        }),
        "fill" => json!({
            "action": "fill",
            "selector": args.get(3).ok_or_else(|| "page fill 需要 selector。".to_owned())?,
            "value": read_stdin_text()?,
        }),
        "press" => {
            let mut body = json!({
                "action": "press",
                "key": args.get(3).ok_or_else(|| "page press 需要按键名。".to_owned())?,
            });
            if let Some(selector) = args.get(4) {
                body["selector"] = Value::String(selector.clone());
            }
            body
        }
        "evaluate" => json!({"action": "evaluate", "script": read_stdin_text()?}),
        "request" => {
            let text = read_stdin_text()?;
            serde_json::from_str::<Value>(&text)
                .map_err(|error| format!("page request JSON 无效：{error}"))?
        }
        other => return Err(format!("不支持页面动作 `{other}`。运行 verisilo-cli help。")),
    };
    let path = format!("/v1/silos/{}/page", url_encode(&spec));
    print_value(api("POST", &path, Some(&body))?, json_out, print_page)
}

fn read_stdin_text() -> Result<String, String> {
    let mut value = String::new();
    io::stdin()
        .read_to_string(&mut value)
        .map_err(|error| error.to_string())?;
    while value.ends_with('\r') || value.ends_with('\n') {
        value.pop();
    }
    if value.is_empty() {
        return Err("stdin 不能为空。".to_owned());
    }
    Ok(value)
}

fn clash_profile(args: &[String]) -> Result<Value, String> {
    let probed = api("GET", "/v1/clash", None).ok();
    let mixed = flag(args, "--mixed-port")
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| {
            probed
                .as_ref()
                .and_then(|value| value.get("mixedPort"))
                .and_then(Value::as_u64)
                .map(|port| port as u16)
        })
        .ok_or_else(|| "没有找到 Clash 代理端口。请加 --mixed-port 7897，或先打开 Clash Verge。".to_owned())?;
    let controller = flag(args, "--controller").or_else(|| {
        probed
            .as_ref()
            .and_then(|value| value.get("controllerUrl"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let mut group = flag(args, "--group");
    let mut node = flag(args, "--node");
    if (group.is_none() || node.is_none()) && probed.is_some() {
        if let Some((auto_group, auto_node)) = first_live_selection(probed.as_ref().unwrap()) {
            group = group.or(Some(auto_group));
            node = node.or(Some(auto_node));
        }
    }
    let mut profile = json!({
        "mode": "fixed_proxy",
        "proxyRequired": true,
        "scheme": "socks5",
        "host": "127.0.0.1",
        "port": mixed,
        "bypassList": []
    });
    if let (Some(controller), Some(group), Some(node)) = (controller, group, node) {
        profile["externalMihomo"] = json!({
            "controllerUrl": controller,
            "selectorGroup": group,
            "nodeName": node,
        });
    }
    Ok(profile)
}

fn first_live_selection(clash: &Value) -> Option<(String, String)> {
    let groups = clash.get("groups")?.as_array()?;
    for group in groups {
        let name = group.get("name")?.as_str()?.to_owned();
        let selected = group.get("selected").and_then(Value::as_str);
        let nodes = group.get("nodes")?.as_array()?;
        let chosen = selected
            .and_then(|selected| {
                nodes.iter().find(|node| node.get("name").and_then(Value::as_str) == Some(selected))
            })
            .or_else(|| nodes.first())?;
        let node_name = chosen.get("name")?.as_str()?.to_owned();
        let proxy_type = chosen
            .get("proxyType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            proxy_type.as_str(),
            "direct" | "reject" | "reject-drop" | "pass" | "compatible"
        ) {
            continue;
        }
        return Some((name, node_name));
    }
    None
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn print_value(
    value: Value,
    json_out: bool,
    printer: fn(&Value) -> Result<(), String>,
) -> Result<(), String> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()));
        Ok(())
    } else {
        printer(&value)
    }
}

fn print_status(value: &Value) -> Result<(), String> {
    let vault = value
        .pointer("/vault/state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let runtime = value
        .pointer("/activation/state")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let message = value
        .pointer("/activation/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    println!("保险库：{vault}");
    println!("运行：{runtime}");
    if !message.is_empty() {
        println!("{message}");
    }
    Ok(())
}

fn print_app(value: &Value) -> Result<(), String> {
    if value.get("created").and_then(Value::as_bool) != Some(true) {
        println!("桌面窗口：未创建（本机服务正在后台运行）");
        return Ok(());
    }
    let state = if value.get("visible").and_then(Value::as_bool) == Some(true) {
        "已显示"
    } else {
        "已隐藏"
    };
    let width = value.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = value.get("height").and_then(Value::as_u64).unwrap_or(0);
    println!("桌面窗口：{state} · {width}×{height}");
    Ok(())
}

fn print_vault(value: &Value) -> Result<(), String> {
    println!(
        "保险库：{}",
        value.get("state").and_then(Value::as_str).unwrap_or("unknown")
    );
    Ok(())
}

fn print_page(value: &Value) -> Result<(), String> {
    if let Some(count) = value.get("count").and_then(Value::as_u64) {
        println!("可见浏览器窗口：{count}");
        if let Some(windows) = value.get("windows").and_then(Value::as_array) {
            for window in windows {
                println!(
                    "  {}×{} @ {},{}  {}",
                    window.get("width").and_then(Value::as_i64).unwrap_or(0),
                    window.get("height").and_then(Value::as_i64).unwrap_or(0),
                    window.get("x").and_then(Value::as_i64).unwrap_or(0),
                    window.get("y").and_then(Value::as_i64).unwrap_or(0),
                    window.get("title").and_then(Value::as_str).unwrap_or("")
                );
            }
        }
        if let Some(page) = value.get("page") {
            println!("页面窗口：{}", serde_json::to_string(page).unwrap_or_default());
        }
        return Ok(());
    }
    if let Some(url) = value.get("url").and_then(Value::as_str) {
        println!("URL：{url}");
    }
    if let Some(title) = value.get("title").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        println!("标题：{title}");
    }
    if let Some(path) = value.get("path").and_then(Value::as_str) {
        println!("截图：{path}");
    }
    if let Some(result) = value.get("value") {
        println!("{}", serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string()));
    } else if let Some(aria) = value.get("aria").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        println!("{aria}");
    } else if let Some(text) = value.get("text").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        println!("{text}");
    }
    Ok(())
}

fn print_identity(value: &Value) -> Result<(), String> {
    let Some(identity) = value.get("websiteIdentity") else {
        println!("还没有页面读到的结果。先打开一次独立浏览器。");
        return Ok(());
    };
    println!("这次打开时，页面脚本读到的身份：");
    print_identity_line("浏览器标识", identity.get("userAgent"));
    print_identity_line("语言", identity.get("language"));
    print_identity_line("系统平台", identity.get("platform"));
    print_identity_line("时区", identity.get("timezone"));
    let width = identity.get("screenWidth").and_then(Value::as_u64);
    let height = identity.get("screenHeight").and_then(Value::as_u64);
    if let (Some(width), Some(height)) = (width, height) {
        println!("屏幕：{width}×{height}");
    }
    if let Some(cores) = identity.get("hardwareConcurrency").and_then(Value::as_u64) {
        println!("CPU：{cores} 核");
    }
    let vendor = identity
        .get("webglVendor")
        .and_then(Value::as_str)
        .unwrap_or("");
    let renderer = identity
        .get("webglRenderer")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !vendor.is_empty() || !renderer.is_empty() {
        println!("显卡：{vendor} · {renderer}");
    }
    match identity.get("webdriver").and_then(Value::as_bool) {
        Some(true) => println!("自动化标记：有"),
        Some(false) => println!("自动化标记：没有"),
        None => {}
    }
    Ok(())
}

fn print_identity_line(label: &str, value: Option<&Value>) {
    if let Some(text) = value.and_then(Value::as_str).filter(|text| !text.is_empty()) {
        println!("{label}：{text}");
    }
}

fn print_silos(value: &Value) -> Result<(), String> {
    let Some(list) = value.as_array() else {
        println!("没有 Silo。");
        return Ok(());
    };
    if list.is_empty() {
        println!("没有 Silo。");
        return Ok(());
    }
    for silo in list {
        let id = silo.get("id").and_then(Value::as_str).unwrap_or("?");
        let name = silo.get("name").and_then(Value::as_str).unwrap_or("(未命名)");
        let adapter = silo
            .pointer("/engine/adapter")
            .and_then(Value::as_str)
            .unwrap_or("stock");
        println!("{name}\t{id}\t{adapter}");
    }
    Ok(())
}

fn print_clash(value: &Value) -> Result<(), String> {
    if let Some(detail) = value.get("detail").and_then(Value::as_str) {
        println!("{detail}");
    }
    if let Some(port) = value.get("mixedPort") {
        println!("代理端口：{port}");
    }
    if let Some(controller) = value.get("controllerUrl").and_then(Value::as_str) {
        println!("控制口：{controller}");
    }
    if let Some(mode) = value.get("mode").and_then(Value::as_str) {
        println!("Clash 模式：{mode}");
    }
    if let Some(groups) = value.get("groups").and_then(Value::as_array) {
        for group in groups.iter().take(12) {
            let name = group.get("name").and_then(Value::as_str).unwrap_or("?");
            let selected = group.get("selected").and_then(Value::as_str).unwrap_or("-");
            println!("  {name} → {selected}");
        }
    }
    Ok(())
}

fn print_diagnose(value: &Value) -> Result<(), String> {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("?");
    let id = value.get("siloId").and_then(Value::as_str).unwrap_or("?");
    println!("{name} ({id})");
    println!(
        "引擎：{}",
        value.get("adapter").and_then(Value::as_str).unwrap_or("?")
    );
    println!(
        "运行：{} {}",
        value.get("runtimeState").and_then(Value::as_str).unwrap_or("?"),
        value.get("runtimeMessage").and_then(Value::as_str).unwrap_or("")
    );
    if let Some(clash) = value.get("clash") {
        print_clash(clash)?;
    }
    Ok(())
}

fn print_activation(value: &Value) -> Result<(), String> {
    let state = value.get("state").and_then(Value::as_str).unwrap_or("?");
    let message = value.get("message").and_then(Value::as_str).unwrap_or("");
    println!("{state}");
    if !message.is_empty() {
        println!("{message}");
    }
    Ok(())
}

fn print_created(value: &Value) -> Result<(), String> {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("?");
    let id = value.get("id").and_then(Value::as_str).unwrap_or("?");
    println!("已创建 {name} ({id})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use verisilo_desktop_lib::local_api::{LocalApiDiscovery, DISCOVERY_SCHEMA};

    #[test]
    fn url_encode_preserves_id_and_encodes_names() {
        assert_eq!(super::url_encode("shop-1"), "shop-1");
        assert!(super::url_encode("美国").contains('%'));
    }

    #[test]
    fn background_start_waits_for_fresh_discovery() {
        let old = LocalApiDiscovery {
            schema: DISCOVERY_SCHEMA.to_owned(),
            url: "http://127.0.0.1:17300".to_owned(),
            pid: 1,
            token: "old".to_owned(),
            vault_name: "default".to_owned(),
        };
        assert!(!super::discovery_changed(Some(&old), &old));
        let mut new = old.clone();
        new.pid = 2;
        assert!(super::discovery_changed(Some(&old), &new));
    }

    #[test]
    fn vault_flag_is_global_and_removed_before_command_parsing() {
        let (vault, args) = super::extract_vault_flag(vec![
            "status".to_owned(),
            "--vault".to_owned(),
            "agent".to_owned(),
            "--json".to_owned(),
        ])
        .expect("vault flag");
        assert_eq!(vault, "agent");
        assert_eq!(args, ["status", "--json"]);
    }
}
