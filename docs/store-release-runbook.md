# Store release runbook — Native Host dual store IDs

- 状态：**P5/P6 操作手册**
- 前置：Chrome Web Store 与 Microsoft Edge Add-ons 各有一个 Draft item，且已取得两个商店扩展 ID
- 相关能力全部已实现：`scripts/prepare-native-host-release.mjs`、`scripts/install-native-host.ps1`、`scripts/verify-native-host-install.ps1`、`apps/desktop/src-tauri/build.rs` 与 `native_host.rs` 编译期/运行期校验。本手册只讲顺序。

## 0. 为什么不能跳过

两个商店给同一份 ZIP 分配**不同**的扩展 ID；Native Host 的 `allowed_origins` 不能写通配符。Chrome 侧只认 `chrome-extension://<Chrome ID>/`，Edge 侧只认 `chrome-extension://<Edge ID>/`。因此发布前必须把两个 ID 同时：写入 release config、编译进 Host、写入两个 HKCU 注册的 manifest。

## 1. 拿 ID（P5，账号持有人）

- Chrome：Developer Dashboard → Add new item → 上传 `VeriSilo-Companion-0.2.11-chrome-edge.zip`（SHA-256 见 [`store-release-evidence.md`](store-release-evidence.md)）→ 保存 Draft → 记下 Item ID。
- Edge：Partner Center → Microsoft Edge → 创建 Extension → 上传同一 ZIP → 保存 Draft → 记下扩展 ID。
- 注意：本计划采用 **CWS 自动生成 key**。因此本地旁加载的 unpacked ID 与商店 ID 不同，无法在提交前用商店 ID 做本地正握手；正握手安排在商店侧可安装后（unlisted / trusted testers / 审核通过未发布）。

## 2. 生成 release config（P6）

```powershell
$env:VERISILO_CHROME_EXTENSION_ID = '<Chrome Store Item ID>'
$env:VERISILO_EDGE_EXTENSION_ID = '<Edge Add-ons ID>'
node scripts/prepare-native-host-release.mjs --out artifacts/native-host
```

脚本拒绝空值、非 `[a-p]{32}`、占位符 ID 和字符多样性不足的 ID。生成的 `artifacts/native-host/native-host-release-config.json` 是公开标识符，不是秘密；但**在拿到真实商店 ID 前不要提交生成物**（见 [`development.md`](development.md)）。

## 3. 在同一环境构建 Host

```powershell
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --features native-host --bin verisilo-native-host
```

`build.rs` 会因 env 变化重编；`native_host.rs` 在运行期再次校验编译进去的 ID，缺失或畸形时拒绝授权任何生产扩展。

## 4. 安装并验证

把 `verisilo-native-host.exe` 与 `verisilo.exe` 放到同一目录（Host 只允许唤醒同目录 `verisilo.exe`），然后以**标准用户**运行：

```powershell
pwsh -File scripts/install-native-host.ps1 `
  -HostPath '<install-dir>\verisilo-native-host.exe' `
  -ReleaseConfigPath 'artifacts\native-host\native-host-release-config.json'
pwsh -File scripts/verify-native-host-install.ps1 `
  -HostPath '<install-dir>\verisilo-native-host.exe' `
  -ReleaseConfigPath 'artifacts\native-host\native-host-release-config.json'
```

`install-native-host.ps1` 写 `HKCU:\Software\Google\Chrome\NativeMessagingHosts\io.verisilo.host` 与 `HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\io.verisilo.host`，各自指向只含本商店 ID 的 manifest；失败时事务回滚。验证脚本核对 manifest 字段、origin 精确匹配、注册表指向、install-record 一致性，并拒绝 reparse point。

## 5. 负向测试（本机可做，无需商店 ID）

按 [`release.md`](release.md) 第 331–335 条核对：

1. 生产 Host 拒绝错误/缺失 ID、非授权 origin、未知字段、秘密形态字段、协议不匹配和 >16 KiB 消息；
2. `open_desktop` 只启动同目录 `verisilo.exe` 且无参数；过期/畸形 snapshot 绝不作为当前状态返回；
3. `uninstall-native-host.ps1` 连续执行两次：两个浏览器注册与 manifest 全部消失，而 Vault、报告和 Silo Profile 目录保留。

## 6. 正握手测试（需要商店侧可安装）

顺序建议：Chrome 先走 trusted testers 或 Deferred Publishing（审核通过不自动公开），Edge 用 draft 的可见性选项；两浏览器各安装一次商店版本后验证：

1. 扩展内状态从 `local_only` 变为绑定状态（桌面端运行、Vault 解锁、Silo 匹配）；
2. Network Check 结果经 `submit_network_evidence` 进入桌面端 Vault 历史；
3. 无桌面端/未解锁时优雅降级为 `local_only`，不报"功能坏了"。

## 7. 收口

- 把两个商店 ID 与验证结果补记进 [`store-release-evidence.md`](store-release-evidence.md)（ID 是公开标识符，可入仓库）。
- 对照商店 listing 确认两个 ID 确实属于各自 listing（语法合法 ≠ 拥有权，见 [`release.md`](release.md)）。
- 之后才允许 Submit for Review（Chrome 建议 Deferred Publishing）。
