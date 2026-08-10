# VeriSilo 环境实现路线：浏览器引擎、虚拟机与远程环境

## 目标

VeriSilo 不把浏览器扩展当作全部产品，也不把独立 `user-data-dir` 当作环境隔离的终点。产品正式采用四层实现：

| 层级           | 版本      | 状态                                         | 主要边界                                                 |
| -------------- | --------- | -------------------------------------------- | -------------------------------------------------------- |
| 独立 Silo      | V0.1–V0.6 | 基线部分实现                                 | 独立浏览器 Profile、网站数据、权限、历史和启动级网络配置 |
| 受控浏览器引擎 | V0.7      | standalone/M3-0 已验收；M3-WI 失败且仍属实验 | 协调浏览器可见信号、引擎网络能力和跨上下文一致性         |
| 本地虚拟环境   | V0.8      | 已列入路线                                   | 独立操作系统、字体、设备视图和每环境网络出口             |
| 自托管远程环境 | V0.9      | 已列入路线                                   | 远程浏览器会话、独立网络栈、持久环境和生命周期审计       |

这些层级是可选能力。轻量用户可以只用扩展或独立 Silo；需要更强边界的用户可以逐级升级。界面必须始终区分“当前可用”“已列入路线”“已配置”“已应用”和“已验证”。

“基线部分实现”只描述当前已有代码，不表示 V0.1–V0.6 已整体验收。逐项证据和缺口以[桌面端完成度审计](desktop-completion-audit.md)为准。

## V0.1–V0.6 冻结定义

为避免把六个版本笼统写成“独立 Silo 已实现”，首发阶段按以下边界验收：

| 版本 | 冻结交付范围                                                                        | 升级为完成所需的直接证据                                                                           |
| ---- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| V0.1 | Vault 基础、Chrome/Edge 发现、独立 `user-data-dir`、Silo 生命周期、目录锁与单活启动 | Vault/状态机单测，以及 Win10/11 × Chrome/Edge 的 A/B 站点数据隔离、默认 Profile 不变和异常恢复 E2E |
| V0.2 | 可选 Companion、生产 Native Host、固定代理/PAC/外部 Mihomo 与 fail-closed 网络路径  | 正负协议测试、HKCU 安装测试、扩展缺失降级，以及从实际 Silo 内完成的断线和出口证据                  |
| V0.3 | 模块化观察、人话解释、桌面证据历史、脱敏 JSON/HTML 报告                             | 采集器失败隔离、覆盖声明、桌面回传/存储/迁移和导出确认测试                                         |
| V0.4 | 每 Silo 稳定实验控制、`observe → apply → verify → restore`、按站点回退              | 短期派生配置、跨上下文一致性、失败回滚、重启恢复和兼容性矩阵                                       |
| V0.5 | `VeriSilo Labs` 中的 Cookie/Worker 等高风险实验及泄漏停止条件                       | 独立 feature gate、默认关闭、逐项泄漏/兼容测试；未满足停止条件不得进入默认模式                     |
| V0.6 | 封顶能力报告、Windows 安装升级卸载、SBOM/许可证/校验和/签名和商店发布门槛           | 可追溯 Windows 产物、E2E 日志、供应链报告、哈希和签名验证；路线文档或示例报告不算完成              |

### V0.5 当前可直接核对的窄实现

V0.5 不再只有路线文字：contracts、Companion 与桌面 UI 共享版本化 `LabsExperiment`/receipt 模型，实验默认关闭，要求当前 Silo＋站点明确授权，并实现 `observe → apply → verify → restore`、到期和泄漏即停状态机。无可用桌面 Silo 时只能建立有界 `local_temporary` 运行，不能伪装成 Silo 证据。

当前唯一可选实现是 MAIN-world 的新建 Dedicated Worker constructor 包装。它只处理开启后新建的同源/blob classic Worker，用短期随机 canary 做 Worker handshake 与同源 iframe 一致性自检；任一可观察的跨标签页/iframe/Service Worker URL/页面可见 Cookie canary 泄漏、页面或 Worker 异常、超时、权限变化、导航、scope 违规或验证失败，都会恢复原 constructor、停用该站点并写入不含 canary/Cookie/token 的本机脱敏收据。

该入口由用户点击后注入，不能证明早于页面脚本，因此成功状态只能是 `best_effort`，不能是 `verified`。既有 Worker、module/cross-origin Worker、SharedWorker、Service Worker 内存均明确不覆盖。Cookie 仓库虚拟化与全面 Set-Cookie 截获在 stock MV3 中保持不可选 `unsupported`；当前可靠替代仍是桌面 Silo 的独立 `user-data-dir`。这解决 feature gate、停止条件和窄真实实现缺口，但不代表 V0.5 全部 Windows/浏览器兼容性验收已经完成。

后续层级不能替代这些基础验收。例如 V0.8 VM 后端存在，也不能自动证明 stock Chrome/Edge 的 V0.1 默认 Profile 安全。

## 从现有产品和项目吸收什么

以下资料用于学习产品分层、更新机制、配置一致性和环境生命周期；它们的营销描述不自动成为 VeriSilo 的能力证据。

| 来源                                                                                                              | 可借鉴的思路                                                         | VeriSilo 的约束                                                                            |
| ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| [GoLogin Orbita](https://gologin.com/docs/orbita-browser)                                                         | 将修改过的 Chromium 作为可持续更新的浏览器产品，而不是一次性脚本注入 | 每个字段仍需跨上下文验证；不复述“不可检测”等营销结论                                       |
| [GoLogin Cloud Browser](https://gologin.com/docs/api-reference/cloud-browser/what-is-gologin-cloud-browser)       | 持久远程 Profile、远程会话和自动化连接的生命周期模型                 | V0.9 默认面向用户自托管节点，不建立默认 VeriSilo 公有云                                    |
| [Multilogin Mimic / Stealthfox](https://multilogin.com/help/cn_CN/how-to-use-mimic-and-stealthfox)                | 多引擎适配、主引擎与遗留引擎的维护分级                               | 每个引擎独立发布兼容矩阵和维护状态                                                         |
| [Multilogin X 功能说明](https://multilogin.com/help/en_US/introduction-to-multilogin-x/multilogin-x-top-features) | Profile 模板、代理绑定、本地或云端存储的产品结构                     | 默认仍为本地优先；任何远端存储都需要单独威胁模型和明确选择                                 |
| [Multilogin Cloud Phones](https://multilogin.com/help/en_US/how-to-create-mobile-profiles)                        | 当浏览器字段控制不够时，直接提供完整操作系统环境                     | 移动或真实设备环境不进入 V0.8 首版，待 V0.9 后单独评估                                     |
| [Donut Browser](https://github.com/zhom/donutbrowser)                                                             | Profile、代理、浏览器引擎与本地自动化入口的产品编排                  | AGPL-3.0 代码不得复制进 MPL-2.0 核心，只能独立实现通用思想                                 |
| [Camoufox](https://github.com/daijro/camoufox)                                                                    | Firefox 引擎级控制、配置注入和 BrowserForge 分布                     | 当前第一条 Managed Engine 路线；固定资产、逐阶段取证，维护缺口和一致性风险必须在 UI 中可见 |
| [BrowserForge](https://github.com/daijro/browserforge)                                                            | 按现实分布生成互相约束的请求头与浏览器特征模板                       | 固定版本、记录来源；模板先经过规则约束和验证，不能盲目随机化                               |
| [FingerprintJS](https://github.com/fingerprintjs/fingerprintjs)                                                   | 模块化采集器、错误容忍和稳定信号提取                                 | 哈希不是绝对身份；采集只用于解释与回归验证                                                 |
| [CreepJS](https://github.com/abrahamjuliot/creepjs)                                                               | 跨 Window、iframe、Worker 和渲染信号寻找自相矛盾                     | 只借鉴矛盾规则与测试结构，不把单站结果当成“不可检测”证明                                   |

任何第三方兼容代码都要先记录版本、许可证、归属、修改和 SBOM 条目。

## V0.7：受控浏览器引擎

### 当前优先级

Standard Silo 仍是长期产品基础层，但当前工程优先级是关闭缺失的指纹执行风险。V0.7 采用 Camoufox-first：Linux 的 `Resolved Identity Artifact → standalone Host → Persistent Profile → probe evidence` 与原生 Windows M2-W 已 Accepted；[M3-0](camoufox-m3-engine-adapter-task.md) 的 EngineAdapter/Host 合同集成也已在 `e96ef3f` Accepted。真实 Windows 的 M3-WI 只存在于 test-only integration seam，第二 Host 调查没有定位唯一根因，Gate 仍为 Failed，能力保持 `experimental`，更不表示已有 production package。

当前执行顺序是 FP1 确定性 Artifact 投影、FP2 跨 realm 一致性、FP3 网络/地区协调、FP4 实站兼容，再用最终 Managed Engine 重新冻结一次 clean M3-WI。旧 R2/R2H 矩阵不再作为当前主线主动重复。当前证据、Gate 和未验证边界见[Camoufox Managed Engine 状态](camoufox-program-status.md)，路线原因见[Camoufox-first 决策](camoufox-managed-engine-decision.md)。

Controlled Chromium 不再与 Camoufox 并行开发。只有 Chromium 专属 API/扩展生态成为明确需求、需要直接控制 Chromium TLS/QUIC 或 V8 行为、Camoufox 无法满足兼容性/维护/分发要求，或项目资源足以承担长期 patch 与多平台构建时，才重新评估。

### 交付范围

1. 冻结 `EngineAdapter` 接口：安装、更新、签名验证、启动参数、身份模板、能力探测、健康检查和回滚。
2. 继续把 Chrome/Edge Stable 作为 `stock-chromium` 基线适配器；它只承诺独立 Profile 和受支持的启动配置。
3. 完成一条 Camoufox Managed Engine 垂直路径：固定 Firefox 系引擎资产、生成和重放约束型 Artifact、持久化 Profile、严格 Host 生命周期、已接受的 EngineAdapter 合同映射，以及 FP1–FP4 后重新执行的 clean 原生 Windows Gate。它不是 Chrome 模拟，也不因 standalone 或 contract Gate 通过而默认捆绑。
4. 冻结约束型身份模板：操作系统、浏览器版本、UA/UA-CH、语言、时区、屏幕、渲染、字体、媒体设备和网络设置必须满足显式规则。
5. 所有控制采用 `observe → apply → verify → restore`。长期种子不进入页面主世界；页面只接收短期派生令牌或已经约束的配置。
6. 引擎包必须支持签名校验、锁定版本、可复现元数据、SBOM、许可证清单、增量更新失败回滚和紧急禁用。

### 发布门槛

- Window、iframe、Dedicated Worker、请求头和引擎网络层的同一字段不存在未解释矛盾。
- Canvas/WebGL/字体模板在同一 Silo 重启后稳定，在不同 Silo 间按配置可区分。
- QUIC 只有在启动配置和协议观测都确认后才显示“已验证关闭”；TLS 以真实 ClientHello/协商结果为证据。
- 自建测试页、FingerprintJS 类采集器和 CreepJS 类矛盾规则均进入兼容矩阵；单一测试站通过不能作为总体验收。
- 网站兼容性失败可按站点回退，并清楚显示当前已回退的控制。

## V0.8：本地虚拟环境

### 后端选择

- [WSL 2](https://learn.microsoft.com/en-us/windows/wsl/about) 作为较轻的 Linux Chromium Provider：它可以提供独立 Linux 用户态、文件系统、字体集合和浏览器 Profile，但默认仍共享或经过 Windows 的网络/GPU/WSLg 集成，不能宣传成完整独立设备。当前仓库已有固定 guest-agent、精确 Profile/PID 生命周期与用户自托管的 loopback SOCKS5H 出口/代理 DNS 证据；来宾 OS resolver 与 OS 级强制仍不可用，且真实 Windows/WSLg 验收仍是发布门槛。网络模式依据[微软 WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking)验证。
- [Windows Sandbox](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/) 用作一次性兼容实验室。它适合可重复的干净测试，但环境默认是临时的，当前也不能同时运行多个实例，因此不能作为多 Silo 持久后端；固定控制器只证明精确宿主进程生命周期，因无可靠来宾回传通道，来宾健康、网络、DNS 和浏览器就绪保持不可用。`.wsb` 的可控项以[微软配置文档](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file)为准。
- [Hyper-V](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/overview) 用作持久 VM 后端，提供独立来宾操作系统、磁盘和网络边界。仓库内控制器会固定 VM GUID/名称、代际、镜像哈希与失败回执；合法 VHDX、固定来宾 Agent 及真实签名生命周期验收仍是外部阻塞条件，不能由控制面回执替代。

界面必须在启用前检测 Windows 版本、虚拟化能力、管理员要求和重启要求。Windows Sandbox 与 Hyper-V 的系统要求、并发和可用版本差异不得隐藏；根据[微软 Hyper-V 安装要求](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/get-started/Install-Hyper-V)，Hyper-V 不应在 Windows Home 上显示成可直接启用。

### 交付范围

1. 冻结 `EnvironmentBackend` 接口：创建、启动、暂停、快照、销毁、网络配置、健康检查和日志导出。
2. 提供签名的基础镜像清单和可复现配置，不直接分发来源不明的系统镜像。
3. 默认关闭宿主可写目录映射、剪贴板、摄像头、麦克风、USB 和不必要的 GPU 透传；用户启用时显示它对隔离边界的影响。
4. 每个 VM 独立保存浏览器 Profile、字体集合和网络配置；出口验证必须从来宾环境内部发起。
5. “真实硬件已改变”永远不是默认结论。透传 GPU、虚拟 CPU 和虚拟设备都要按事实展示。

WSL、Sandbox 和 Hyper-V 都通过同一 `EnvironmentBackend` 抽象接入；WSL 不成为 VeriSilo 核心依赖。每个后端的出口和 DNS 必须从其内部验证，不能复用桌面控制器请求。代理与外部 Mihomo 的当前实现边界见[网络身份与出口 Provider](network-identity-providers.md)。

### 发布门槛

- 来宾与宿主的 Cookie、磁盘、剪贴板、映射目录、端口和设备访问符合默认拒绝策略。
- 快照恢复不会把两个 Silo 的密钥、种子或浏览器数据合并。
- 关闭网络或代理验证失败时，“必须代理”环境无法进入正常浏览会话。
- 卸载 VeriSilo 不会静默删除用户仍在使用的 VM；删除环境需要单独确认并给出数据影响。

## V0.9：自托管远程环境

V0.9 的首个远程模式是用户自有服务器或组织自托管节点，不是默认 VeriSilo 公有云。远程会话生命周期可以参考 [Browserless 的 session 管理](https://docs.browserless.io/baas/session-management)，但协议、加密卷、浏览器控制和审计由 VeriSilo 自己定义与验证。

### 交付范围

1. 一个最小远程 Agent：相互认证、版本协商、环境创建、会话启动、健康检查、TTL 和销毁。
2. 每个远程环境使用独立进程或 VM 边界、加密持久卷和单独网络配置；出口 IP、DNS 和协议检查从远端内部运行。
3. 提供浏览器画面/输入通道与可选 Playwright 类自动化入口；人工会话和自动化权限分开授权。
4. 桌面端明确显示数据所在地区、节点所有者、密钥归属、在线成本、最后活动时间和删除状态。
5. 默认不将 Cookie、凭证或报告复制到 VeriSilo 服务；导入导出都需要用户明确操作和审计记录。

### 发布门槛

- 控制面失联后远端会话按用户策略锁定或到期，不无限期暴露。
- Agent 拒绝降级协议、未知字段、过大消息和未授权环境 ID。
- 删除操作能证明计算实例、持久卷、快照和临时密钥分别进入预期状态。
- 桌面端断言与远端证据可对应：配置、应用和实际出口验证不能混为一个状态。

## V0.9 之后

真实移动设备、设备农场、Linux/macOS 宿主和组织级调度可以在上述接口稳定后评估。它们不会阻塞 Windows 首发，也不会被扩展 UI 伪装成已经存在。

## 许可证和供应链规则

- VeriSilo 核心保持 MPL-2.0；文档保持 CC BY 4.0。
- FingerprintJS/CreepJS 类 MIT 代码、BrowserForge 的 Apache-2.0 代码和 Camoufox 的 MPL-2.0 代码只有在完成版本锁定、NOTICE、归属、修改记录和 SBOM 后才可进入发行物。
- CreepJS 名称受商标政策约束；VeriSilo 只能描述其测试思想或记录兼容结果，不能把自己的组件、服务或公开测试站命名为 CreepJS。
- Donut Browser 的 AGPL-3.0 代码不得复制或改写进 MPL 核心；只允许根据公开行为独立实现通用产品思想。
- 第三方浏览器引擎必须作为可替换适配器；安装、升级、回滚、停止维护和安全公告都要在 UI 中可见。

## 统一声明

专用引擎、VM 和远程环境提高的是隔离层级与配置一致性，不保证绕过风控，也不消除行为、账号关系、支付信息、服务端历史或人为操作带来的关联。VeriSilo 的发布标准是“边界清楚且有证据”，不是“绝对不可检测”。
