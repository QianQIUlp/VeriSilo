use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WslStatus {
    pub supported_platform: bool,
    pub available: bool,
    pub distributions: Vec<String>,
    pub message: String,
}

#[cfg(target_os = "windows")]
pub fn detect_wsl() -> WslStatus {
    let status = std::process::Command::new("wsl.exe")
        .args(["--status"])
        .output();
    let Ok(status) = status else {
        return WslStatus {
            supported_platform: true,
            available: false,
            distributions: Vec::new(),
            message: "Windows 未找到 wsl.exe；VeriSilo 没有修改系统功能。".to_owned(),
        };
    };
    if !status.status.success() {
        return WslStatus {
            supported_platform: true,
            available: false,
            distributions: Vec::new(),
            message: "WSL 当前不可用；VeriSilo 只做了只读检查。".to_owned(),
        };
    }

    let distributions = std::process::Command::new("wsl.exe")
        .args(["--list", "--quiet"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| decode_windows_output(&output.stdout))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    WslStatus {
        supported_platform: true,
        available: true,
        message: if distributions.is_empty() {
            "WSL 可用，但未发现已安装的 Linux 发行版。".to_owned()
        } else {
            format!("WSL 可用，发现 {} 个 Linux 发行版。", distributions.len())
        },
        distributions,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn detect_wsl() -> WslStatus {
    WslStatus {
        supported_platform: false,
        available: false,
        distributions: Vec::new(),
        message: "WSL Provider 只适用于 Windows；本次没有执行系统命令。".to_owned(),
    }
}

#[cfg(target_os = "windows")]
fn decode_windows_output(bytes: &[u8]) -> String {
    if bytes.chunks(2).skip(1).any(|pair| pair.get(1) == Some(&0)) {
        let words = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&words)
            .trim_matches('\u{feff}')
            .to_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}
