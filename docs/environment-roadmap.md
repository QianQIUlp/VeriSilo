# VeriSilo 环境实现路线：浏览器引擎、虚拟机与远程环境

## 目标

VeriSilo 不把浏览器扩展当作全部产品，也不把独立 `user-data-dir` 当作环境隔离的终点。产品正式采用四层实现：

| 层级           | 版本      | 状态       | 主要边界                                                 |
| -------------- | --------- | ---------- | -------------------------------------------------------- |
| 独立 Silo      | V0.1–V0.6 | 当前实现   | 独立浏览器 Profile、网站数据、权限、历史和启动级网络配置 |
| 受控浏览器引擎 | V0.7      | 已列入路线 | 协调浏览器可见信号、引擎网络能力和跨上下文一致性         |
| 本地虚拟环境   | V0.8      | 已列入路线 | 独立操作系统、字体、设备视图和每环境网络出口             |
| 自托管远程环境 | V0.9      | 已列入路线 | 远程浏览器会话、独立网络栈、持久环境和生命周期审计       |

这些层级是可选能力。轻量用户可以只用扩展或独立 Silo；需要更强边界的用户可以逐级升级。界面必须始终区分“当前可用”“已列入路线”“已配置”“已应用”和“已验证”。

## 从现有产品和项目吸收什么

以下资料用于学习产品分层、更新机制、配置一致性和环境生命周期；它们的营销描述不自动成为 VeriSilo 的能力证据。

| 来源                                                                                                              | 可借鉴的思路                                                         | VeriSilo 的约束                                                 |
| ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------- |
| [GoLogin Orbita](https://gologin.com/docs/orbita-browser)                                                         | 将修改过的 Chromium 作为可持续更新的浏览器产品，而不是一次性脚本注入 | 每个字段仍需跨上下文验证；不复述“不可检测”等营销结论            |
| [GoLogin Cloud Browser](https://gologin.com/docs/api-reference/cloud-browser/what-is-gologin-cloud-browser)       | 持久远程 Profile、远程会话和自动化连接的生命周期模型                 | V0.9 默认面向用户自托管节点，不建立默认 VeriSilo 公有云         |
| [Multilogin Mimic / Stealthfox](https://multilogin.com/help/cn_CN/how-to-use-mimic-and-stealthfox)                | 多引擎适配、主引擎与遗留引擎的维护分级                               | 每个引擎独立发布兼容矩阵和维护状态                              |
| [Multilogin X 功能说明](https://multilogin.com/help/en_US/introduction-to-multilogin-x/multilogin-x-top-features) | Profile 模板、代理绑定、本地或云端存储的产品结构                     | 默认仍为本地优先；任何远端存储都需要单独威胁模型和明确选择      |
| [Multilogin Cloud Phones](https://multilogin.com/help/en_US/how-to-create-mobile-profiles)                        | 当浏览器字段控制不够时，直接提供完整操作系统环境                     | 移动或真实设备环境不进入 V0.8 首版，待 V0.9 后单独评估          |
| [Donut Browser](https://github.com/zhom/donutbrowser)                                                             | Profile、代理、浏览器引擎与本地自动化入口的产品编排                  | AGPL-3.0 代码不得复制进 MPL-2.0 核心，只能独立实现通用思想      |
| [Camoufox](https://github.com/daijro/camoufox)                                                                    | Firefox 引擎级控制、配置注入和 BrowserForge 分布                     | 仅作为可选 MPL 适配器原型；维护缺口和一致性风险必须在 UI 中可见 |
| [BrowserForge](https://github.com/daijro/browserforge)                                                            | 按现实分布生成互相约束的请求头与浏览器特征模板                       | 固定版本、记录来源；模板先经过规则约束和验证，不能盲目随机化    |
| [FingerprintJS](https://github.com/fingerprintjs/fingerprintjs)                                                   | 模块化采集器、错误容忍和稳定信号提取                                 | 哈希不是绝对身份；采集只用于解释与回归验证                      |
| [CreepJS](https://github.com/abrahamjuliot/creepjs)                                                               | 跨 Window、iframe、Worker 和渲染信号寻找自相矛盾                     | 只借鉴矛盾规则与测试结构，不把单站结果当成“不可检测”证明        |

任何第三方兼容代码都要先记录版本、许可证、归属、修改和 SBOM 条目。

## V0.7：受控浏览器引擎

### 交付范围

1. 冻结 `EngineAdapter` 接口：安装、更新、签名验证、启动参数、身份模板、能力探测、健康检查和回滚。
2. 继续把 Chrome/Edge Stable 作为 `stock-chromium` 基线适配器；它只承诺独立 Profile 和受支持的启动配置。
3. 建立两个并行可行性轨道：
   - 受控 Chromium 构建，用于评估 Canvas、WebGL、字体、请求头、TLS 与 QUIC 的引擎级控制成本。
   - Camoufox 可选适配器原型，用于验证多引擎接口和 MPL 分发流程，不默认捆绑。
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

- [WSL 2](https://learn.microsoft.com/en-us/windows/wsl/about) 作为较轻的 Linux Chromium Provider：它可以提供独立 Linux 用户态、文件系统、字体集合和浏览器 Profile，但默认仍共享或经过 Windows 的网络/GPU/WSLg 集成，不能宣传成完整独立设备。当前桌面端只实现固定参数的只读可用性/发行版检查；启动 Chromium、代理注入和生命周期管理仍需独立发布门槛。网络模式依据[微软 WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking)验证。
- [Windows Sandbox](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/) 用作一次性兼容实验室。它适合可重复的干净测试，但环境默认是临时的，当前也不能同时运行多个实例，因此不能作为多 Silo 持久后端；`.wsb` 的可控项以[微软配置文档](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file)为准。
- [Hyper-V](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/overview) 用作持久 VM 后端，提供独立来宾操作系统、磁盘和网络边界。

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
