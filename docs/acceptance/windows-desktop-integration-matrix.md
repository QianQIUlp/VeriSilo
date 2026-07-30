# Windows 桌面端与插件集成验收矩阵

## 使用方法

这份矩阵接在[插件端验收](extension-functional-acceptance-2026-07-30.md)之后。它区分三类状态：

- **仓库证据**：代码、schema、单元测试或静态门禁存在。
- **现有自动化边界**：自动化实际覆盖到哪里，以及刻意不覆盖什么。
- **真实验收**：必须在 Windows 10/11、Chrome/Edge 或指定环境上执行并保存原始证据。

“仓库证据存在”不等于功能验收通过；本文件中的真实验收项在产生机器可读结果前一律视为待执行。

实际执行时使用[逐步手工验收操作手册](manual-windows-acceptance-runbook.md)；本文件保留范围、证据等级和发布门槛，不重复每个点击步骤。

## 2026-07-30 本地 Rust 工具链检查

当前 Linux 开发环境已通过官方 rustup 安装仓库锁定的 Rust/Cargo `1.88.0`，并安装 `x86_64-unknown-linux-gnu`、`x86_64-pc-windows-msvc`、`x86_64-pc-windows-gnu` 标准库 target。结果只证明下列范围，不是 Windows 桌面验收：

| 检查                                                   | 结果                                                               | 证据边界                                                                                                           |
| ------------------------------------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| desktop core harness：fmt/check/test/Clippy            | PASS；150 tests、0 failed，Clippy `-D warnings`                    | 编译真实 production Rust 模块，但不含 Tauri/WebView2/Windows 运行时                                                |
| Remote Agent：fmt/check/test/Clippy                    | PASS；7 个 test target 共 72 tests、0 failed，Clippy `-D warnings` | Linux target 控制面验证；没有真实 Provider/WAN/媒体输入                                                            |
| Tauri desktop Cargo metadata/fmt                       | PASS                                                               | manifest、锁文件和 Rust 格式可解析                                                                                 |
| Linux → `x86_64-pc-windows-msvc` desktop `cargo check` | BLOCKED 于 `aws-lc-sys` 的 MSVC C/NASM 工具链                      | Linux 的 GNU `cc` 不能替代 MSVC；必须在第 5 节所述真实 Windows Build Tools + NASM 环境重跑，不能记成源码通过或失败 |

因此，Rust/Cargo 缺失已收口；Windows native 编译、链接、Tauri 启动、WebView2 和 NSIS 仍是待执行的 Windows 证据。

## V0.1–V0.6 桌面核心

| 范围                          | 仓库证据与当前可达 UI                                                  | 现有自动化边界                                                                          | 真实 Windows 验收                                                                                               |
| ----------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Vault 初始化/解锁/锁定        | Tauri command、Argon2id + AES-GCM envelope、锁定时 UI 敏感状态清理     | Rust/TS 单元与 acceptance-only driver 覆盖状态机；本地 Linux 未启动 Tauri               | 初始化、错密码、锁定后所有敏感入口拒绝、重启后解锁；Win10/11 各执行                                             |
| 自动锁                        | UI 有自动锁计时与 session generation 防旧请求回填                      | TS 单元覆盖 deadline/刷新/锁定 UI                                                       | 前后台、系统睡眠/唤醒、长操作、时钟变化和 30 秒轮询边界                                                         |
| Vault 备份/恢复/口令轮换      | command/UI 和加密 envelope 路径存在                                    | Rust 模型/迁移测试；不证明磁盘故障或 Windows ACL                                        | 正常备份恢复、错文件/截断/旧 schema、目标已存在、磁盘满、权限拒绝、原子性                                       |
| Silo 创建/编辑/归档/恢复/删除 | overview/create/edit/settings UI 与 active/archived 列表存在           | TS API/format/parser 测试；Rust model tests                                             | 每个操作逐项点击；运行中拒绝；永久删除失败回滚；Profile 路径不越界                                              |
| Profile 隔离                  | 每个 Silo 独立 `--user-data-dir`，拒绝默认 Profile                     | Windows harness 可测 A/B 浏览器存储和默认 Profile 指纹；它不加载插件                    | Chrome/Edge A/B Cookie、local/session storage、IndexedDB、缓存、Service Worker、权限、历史；默认 Profile 零改动 |
| 浏览器发现/启动/退出          | Chrome/Edge discovery、argument-array launch、单 active runtime        | acceptance-only driver 可启动真实浏览器并检查 Profile lock；普通 TS 测试不启动浏览器    | 常规退出、崩溃、孤儿进程、重复启动、浏览器自身 SingletonLock、未知/移动后的 exe                                 |
| 恢复与锁文件                  | RuntimeManager、精确 PID tree、运行状态恢复与拒绝强杀语义存在          | driver 覆盖部分真实 lock/refusal；不覆盖所有 Windows 崩溃模式                           | 桌面崩溃、浏览器崩溃、Windows 重启、残留 Singleton 文件、无关进程存活                                           |
| Direct                        | direct profile 与 UI 可选                                              | schema/model 通过                                                                       | Chrome/Edge 实际直连，记录基线出口/DNS/WebRTC/QUIC，仅作对照不称安全模式                                        |
| Fixed HTTP/SOCKS5             | required proxy、加密凭据、随机 loopback relay                          | Rust relay/proxy 测试；Windows harness 仅测不可达 proxy fail-closed，不证明真实桌面参数 | 有/无认证、407/SOCKS method、代理中断、DNS/WebRTC/QUIC 抓取、无宿主直连回退                                     |
| PAC                           | UI 与 NetworkProfile 支持 optional PAC；required PAC 被明确拒绝        | schema/launcher 参数测试；acceptance driver 不走 PAC                                    | Chrome/Edge PAC URL/脚本失败、缓存、代理切换、认证和错误文案；不得把 optional PAC 计入 required-proxy 通过      |
| External Mihomo               | loopback Controller、Secret、selector/node/readback、端口/config drift | adapter/relay 单元与源码门禁；不运行用户 Mihomo core                                    | 正常绑定、Secret 错误、非 GLOBAL、DIRECT/REJECT/未知 node、进程退出、配置漂移、旧 relay 不重开                  |
| required-proxy fail-closed    | launcher/runtime/relay 有拒绝直连与终态 blocked 语义                   | loopback 测试和不可达代理浏览器 case；不证明 Windows 全路径                             | 每种故障都确认页面失败且宿主 fixture 零请求；用真实 Chrome/Edge 抓 DNS/WebRTC/QUIC                              |
| 本地报告                      | 桌面可生成/导出 Silo 本地报告与网络证据历史                            | TS report 测试；插件完整扫描报告不进入桌面                                              | UI 到达、导出脱敏、锁定后清理；明确区分 desktop controller 与 managed browser 证据                              |

## 插件、Native Host 与桌面联动

| 场景              | 当前仓库状态                                                                     | 真实验收与通过条件                                                                                  |
| ----------------- | -------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Native Host 注册  | Chrome/Edge manifest 模板、HKCU 注册/验证/注销脚本与 NSIS hooks 存在             | 用正式双商店 ID 安装；两个浏览器分别 handshake；未知 origin/message 拒绝；标准用户无需提权          |
| Host 协议         | schema、origin allowlist、length-prefixed framing、消息大小与路径边界存在        | 浏览器真实 Native Messaging 调用；不能只把 frame 直接喂给 exe                                       |
| 插件网络证据提交  | 只提交用户触发、脱敏、有界的网络证据；Native Host 校验新鲜度、Vault、active Silo | 在真实 Silo 中提交；错 Silo、过期、锁定、停止/重启、重放全部拒绝；成功后 Vault 历史可见             |
| 完整扫描报告      | **当前未联动**；插件报告留在插件本地，桌面只接网络证据                           | 产品若需要完整跨端报告，必须先明确 schema/隐私/保留语义并实现；发布文案不得暗示已存在               |
| Vault 锁定同步    | 桌面 UI 会清敏感状态；Host 在请求时重新校验                                      | 插件已打开时锁 Vault，后续提交必须拒绝并清除过期 badge；当前缺少主动推送，需记录 UI 延迟            |
| Silo 归属与恢复   | runtime snapshot、activeSiloId、运行 UUID 与 freshness 校验存在                  | 浏览器/桌面分别崩溃与恢复；旧 receipt、旧 runtime、另一 Silo、另一个 Profile 均不得串用             |
| Labs desktop_silo | schema 支持 Silo 绑定授权；无桌面时退化为 local_temporary                        | 解锁并运行真实 Silo 后启用；锁定/停止/权限撤销/页面变化必须恢复；receipt 的 Silo ID 与 runtime 一致 |
| 插件缺失          | 桌面把 Companion 证据标为空/未请求，不阻止独立 Profile 基线                      | acceptance driver 已覆盖窄路径；还需真实 UI 文案与重装插件后的恢复                                  |

## V0.7 引擎层

| 范围                 | 仓库证据                                                                                   | 真实验收门槛                                                                                                  |
| -------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| EngineAdapter 控制面 | 版本化 adapter、per-Silo 配置、包签名/更新/回滚/禁用状态机、UI capability/receipt 路径存在 | 必须提供固定、合法、签名并可复现的真实 engine artifact；当前没有可发布 engine 包                              |
| stock Chrome/Edge    | 仍是支持基线，不因 V0.7 存在而改变声明                                                     | 与 V0.1–V0.6 四单元矩阵一起验收                                                                               |
| 受控字段证据         | schema 可表达 apply/verify/restore 与能力状态                                              | Canvas/WebGL/字体/UA/UA-CH/语言/时区/iframe/Worker/请求头/TLS/QUIC 必须逐字段记录直接证据，不能由插件扫描推断 |

## V0.8 本地环境后端

| 后端            | 仓库证据                                                                     | 明确限制与真实验收                                                                                     |
| --------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| WSL Chromium    | controller、固定 guest-agent、Profile/PID 绑定、SOCKS5H 和 proxy-DNS receipt | 需真实 WSLg/Chromium；guest OS resolver 仍 unavailable；代理崩溃、错误答案、直连诱饵、Profile 隔离实测 |
| Windows Sandbox | 默认拒绝 `.wsb`、只读 bootstrap、进程/descriptor/hash lifecycle receipt      | 无可靠 guest return channel；required proxy、DNS、browser-ready 保持 unavailable；按 Windows SKU 实测  |
| Hyper-V         | 受限 PowerShell、镜像清单、VM/image hash、lifecycle/drift receipt            | 需合法固定 VHDX 与 reviewed guest agent；vSwitch、磁盘、失败恢复、签名生命周期实测                     |
| 共通环境边界    | contracts 对九项 operation 逐项 available/unavailable                        | Home/Pro/Enterprise、虚拟化开关、重启、宿主映射/剪贴板/设备/vGPU 负测；静态脚本不能代替                |

## V0.9 自托管 Remote Agent、Screen/Input

| 范围                 | 仓库证据                                                                         | 明确限制与真实验收                                                                                       |
| -------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Remote control plane | pinned PKI/SPKI、短期凭据、typed operations、桌面状态/UI、Linux/Unix Agent crate | 默认示例九项能力均 `unavailable`；必须安装固定真实 Provider，做 WAN、失联、重放、TTL、幂等销毁与密钥轮换 |
| Screen               | `openScreen` 只返回授权 channel metadata，桌面可展示/刷新/关闭                   | 当前不解码或渲染视频/音频；不能称远程画面可用                                                            |
| Input                | typed input schema/命令与人工/自动化 principal 边界存在                          | 当前无随附真实加密媒体/输入 transport；需撤销、并发、越权、剪贴板/文件默认拒绝实测                       |
| Provider/guest       | bridge 与能力协商存在                                                            | 仓库未随附真实 VM/容器/浏览器 Provider；没有 Provider 时 V0.9 只能称“控制面”                             |

## 安装、升级、卸载与发布

| 范围              | 仓库证据                                                                                                                                                                         | 真实验收                                                                                                                                           |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| NSIS current-user | signed 配置包含 sidecar、postinstall/preuninstall、HKCU Host scripts；unsigned 配置已明确清空 sidecar/resources/hooks，只产 desktop-only NSIS，避免用 `AllSigned` 执行未签名 PS1 | disposable 标准用户执行 signed V1 安装、同版本修复、V2 升级、失败回滚、卸载；Vault/Profile 指纹保持；全新机器还要确认 Trusted Publisher/非交互行为 |
| 候选件            | unsigned/signed workflow、SBOM、license report、SHA-256/provenance、promotion attestation 代码存在                                                                               | 必须绑定同一 candidate ID/digest/revision；不能拿本地重建替代上传候选件                                                                            |
| Authenticode      | DryRun/SignAndVerify/VerifySigned gate 与 secrets-gated workflow 定义存在                                                                                                        | 测试证书负测后，再用正式证书与可信时间戳；验证 desktop、Host、installer、卸载器和 sidecar                                                          |
| 四单元发布矩阵    | `tests/windows/Invoke-VeriSiloWindowsE2E.ps1` 与 promotion gate 已定义                                                                                                           | Win10/11 × Chrome/Edge 全部 `RequireAll`；`SKIP`、`BLOCKED`、缺 runner、错 OS/浏览器均为失败                                                       |

## 推荐执行顺序

1. 在 Windows 11 标准用户机跑桌面 smoke：Vault → Silo → Chrome/Edge → Direct/required proxy → 正常/异常退出。
2. 接入正式测试 Native Host ID，在真实 Silo 中重复插件验收并验证网络证据、锁定/停止和恢复。
3. 在 Windows 10 复跑相同矩阵，再做 Mihomo、浏览器崩溃和安装生命周期。
4. V0.7 只有在真实签名 engine artifact 到位后执行；V0.8 按后端和 SKU 分开；V0.9 只有真实 Provider 到位后执行。
5. 所有功能问题收口后，才进入正式 Authenticode、商店 ID 和 promotion gate。

详细 runner 参数与证据格式见 [`tests/windows/README.md`](../../tests/windows/README.md)；发布边界见 [`docs/release.md`](../release.md)。
