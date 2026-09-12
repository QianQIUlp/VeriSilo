# VeriSilo 身份平台北极星

状态：**规范性产品意图**。本文描述 VeriSilo 长期要解决的问题和不应随单个里程碑漂移的原则。阶段进度以[Camoufox Managed Engine 状态](camoufox-program-status.md)为准，工程边界以对应实现和验收文档为准。

> **VeriSilo is a local-first, fail-closed, evidence-backed identity runtime for persistent browser identities.**
>
> VeriSilo 是一个面向持久浏览器身份的本地优先、失败关闭、以运行证据为基础的身份运行环境。

产品中心问题是：**声明的身份、引擎与网络策略，是否真的按照声明运行？**

因此，产品差异化优先落在 Identity Integrity、Execution Integrity、Network Integrity、
Runtime Evidence 和 Attribution，而不是更大的工作站功能数量。`local-first` 本身不是
独占优势；产品方向不追求成为广泛的 Simprint/AdsPower workstation clone。

## 面向谁、解决什么问题

VeriSilo 面向希望在本机长期管理多个浏览器身份的普通用户和个人开发者。目标体验是：

```text
安装 VeriSilo
→ 创建多个 Silo
→ 为每个 Silo 保存独立登录状态
→ 选择浏览器引擎、身份策略和网络出口
→ 启动、关闭并在以后继续使用
→ 查看当前实际应用和观测到的证据
```

长期产品结果不止于 `create → launch`。完整体验还应包括：

```text
declare identity/network intent
→ run
→ see what actually happened
→ detect mismatch/staleness/unavailability
```

用户不应被要求理解浏览器源码构建、WSL、虚拟机、Profile 目录或 Artifact JSON，才能完成最基本的创建、启动和继续使用。托管身份 Silo 向用户展示网站可见身份摘要，并允许在首次成功启动前微调语言、屏幕、硬件并发，以及时区/语言/地理是否跟随网络出口；首次启动后身份保持稳定。底层种子和原始 Artifact 仍不进入页面。

VeriSilo 可以服务普通的本地多 Profile 用户；但 Managed Identity 的核心差异化受众，是把浏览器身份或账号视为长期资产、对身份漂移、网络降级和事后归因敏感的用户。这些用户更可能具有：

```text
persistent identities
high cost of accidental correlation
long-lived sessions/accounts
need for diagnosis/audit
need to know what actually ran
```

这是基于 Simprint source review 的产品假设（product thesis），不是市场调研证明；也不把产品重新定位成 enterprise compliance platform。

## Silo 的领域定义

一个可重放的 Silo 身份由彼此独立但明确绑定的部分组成：

```text
Silo
= Persistent Profile
+ Resolved Identity Artifact
+ Engine Binding
+ Network Policy
+ Runtime Evidence
```

- **Persistent Profile** 保存 Cookie、LocalStorage、IndexedDB、CacheStorage、Service Worker、权限、历史和浏览器设置等浏览器自有状态。
- **Resolved Identity Artifact** 保存已经解析、可版本化重放的网站可见身份配置；它不是 Profile，也不保存代理秘密。
- **Engine Binding** 将身份绑定到实际可执行的浏览器家族、版本和受验证资产，避免“声明版本”与真实能力错位。
- **Network Policy** 独立描述直连、固定代理、用户自管网络 Provider 及地区/时区联动要求。
- **Runtime Evidence** 区分配置、应用、观测、验证失败和不可用；它不能由配置声明替代。

这个公式定义的是领域槽位，不要求 Standard Silo 伪造一份 Managed Artifact。Standard 当前只应把相关身份能力记录为 `native`、`inherit` 或 `unavailable`；是否把这份原生投影物化为统一 Artifact，要到桌面产品契约阶段显式决定。在此之前不得宣称 Standard 拥有可重放的受控指纹配置。

## 三类隔离不能混为一谈

| 层面             | 要解决的问题                                    | 典型机制                |
| ---------------- | ----------------------------------------------- | ----------------------- |
| Profile 隔离     | Cookie 和浏览器状态不互通                       | 独立 `user-data-dir`    |
| Fingerprint 控制 | 网站看到稳定、协调且与引擎一致的浏览器/设备信号 | Camoufox 或未来受控引擎 |
| Environment 隔离 | 浏览器与宿主文件、进程、操作系统或虚拟硬件隔离  | Hyper-V、VM、远端环境   |

因此：

```text
独立 Profile ≠ 指纹浏览器
指纹浏览器 ≠ 虚拟机
虚拟机 ≠ 自动拥有协调可信的网站身份
```

## 三层产品形态

### Standard Silo

- 使用系统 Chrome 或 Edge。
- 提供独立 Profile、登录状态、Vault、网络策略、生命周期和本地证据。
- 大多数硬件和浏览器指纹跟随真实机器。
- 不宣称控制 Canvas、WebGL、字体、UA/UA-CH、Worker、TLS 或 QUIC。

Standard Silo 是长期保留、具有独立产品价值的基础层，不会因为 Managed Engine 到来而被移除。

### Managed Identity Silo

- 使用固定、受追踪的受控浏览器引擎；当前优先 Camoufox。
- 为每个 Silo 生成并保存稳定 Identity Artifact。
- 将 Profile、身份、引擎版本和网络位置策略绑定，但保持各自生命周期清晰。
- 通过受控探针验证同一身份跨启动稳定、不同身份按策略分离。
- 默认追求内部一致，而不是让每个值都随机或都不同。

当前 Camoufox M0–M2 工作属于这一层的执行面验证，而不是整个 VeriSilo 产品的替代实现。

### Isolated Machine Silo

- 在固定 Guest Image、差分磁盘、Guest Agent 和独立虚拟网络中运行浏览器。
- 主要解决宿主安全与完整环境隔离，不是普通账号管理的默认方式。
- Windows 上优先考虑 Hyper-V；WSL 只称为 Linux 浏览器环境，VMware 暂不进入主线。

## 身份策略：一致性优先

指纹不是修改得越多越好。默认策略必须优先保证 OS、浏览器版本、UA、语言、时区、屏幕、渲染、字体和网络位置彼此协调，并在同一 Silo 中长期稳定。

面向用户的策略最终应表达意图，而不是直接暴露随机种子：

| 策略                         | 含义                               |
| ---------------------------- | ---------------------------------- |
| `inherit` / `native`         | 跟随当前机器或浏览器真实值         |
| `coherent` / `common_bucket` | 根据身份约束选择常见且互相一致的值 |
| `fixed`                      | 用户或已解析 Artifact 明确指定     |
| `disabled`                   | 明确关闭某项可控能力               |
| `unavailable`                | 当前引擎不能可靠控制或验证         |

Resolved Identity Artifact 是底层重放制品，不等同于最终用户配置模型。

## 证据与产品表述

- `configured` 只表示配置已形成。
- `applied` 只表示受控执行层报告已经应用。
- `observed` 表示指定观测渠道看到了结果，但未必具备强进程来源证明。
- `verified` 必须满足当前能力声明规定的所有直接证据和绑定条件。
- `unavailable` 必须作为正常结果保留，不能被默认值或推测替代。

VeriSilo 不提供“匿名分数”，不宣传“不可检测”，不把单一反检测网站结果当成总体验收，也不把 Profile 隔离、代理或虚拟机控制面回执夸大为完整设备身份验证。

Durable evidence 现状与路线：**expected-vs-observed reconciliation 已经是产品事实**——Managed Identity Silo 在启动时会做一次 website-visible identity observation，用户也可以在运行中主动触发一次新的 fresh observation（re-observe），两者都用当前 Resolved Identity Artifact 自动调和，并以 matched / mismatched / unavailable / stale 语义呈现正式 Runtime Identity Evidence。它仍是缩小 `observed → verified` 差距的主要产品路线：只有满足未来 capability-specific verification contract 时证据才升级为 `verified`，观测值相等不自动提升 verified，能力级 `observed → verified` 的更强验证（如进程来源证明、跨主机重放）依然是 roadmap，不因 reconciliation 存在而声称已完成。

## 长期非目标

- 不承诺绕过风控、反欺诈或网站关联分析，也不把产品定位为“万能反检测浏览器”。
- 不通过伪造所有字段来冒充任意真实设备；无法协调或验证的字段应保持 `native` 或 `unavailable`。
- 不建立默认的 VeriSilo 公有云、代理出口市场或“IP 纯度”评分服务；远端节点和网络 Provider 优先由用户拥有或选择。
- 不要求所有用户进入虚拟机或受控引擎；Standard Silo 始终可以独立成立。
- 不把多个受控浏览器内核的并行维护当作默认路线；每新增一个引擎都需要明确需求、资源与独立证据。
- 不把 Profile、身份配置、代理秘密和证据合并成一个不可迁移、不可审计的黑盒目录。

## 长期顺序与当前执行阶段

长期产品顺序仍然是 Standard、Managed、Isolated 三层并存。2026-08 的风险优先阶段完成了 Camoufox standalone、Artifact、原生 Windows Host 和 M3-0 contract 接缝；FP1–FP4 以及 clean M3-WI Attempt 4 随后完成了各自冻结边界内的资格链。这里的历史结果不改变三层产品模型，也不把任一层的证据扩大为整体产品或发布验收。

当前 source/package/installed acceptance 主线已经完成到公开的 `v0.1.0-rc3` prerelease
（tag `v0.1.0-rc3`，source `407c501741c1be3444bfa607c7e693ecc7324409`），rc3 release gate 已关闭；
其前的 `v0.1.0-rc2`（source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`）已在 pristine Windows
Sandbox 完成安装后生命周期验收。两者都是 `PUBLIC_GITHUB_PRERELEASE`，不是 Windows
Authenticode signed release；strict standard-user 语义仍未
证明。当前阶段是 post-rc3 product development，canonical development source 是
`origin/baseline/dev`，默认方向是 accepted differentiated roadmap，优先推进五项 integrity/evidence
能力；真实回归、新产品代码、新 candidate 或新的 release Gate 才重新触发对应 QA/build/
acceptance。旧 RC1、FP1–FP4 和 M3-WI 合同只作为历史证据。

当前阶段、Gate 和未验证边界以[Camoufox Managed Engine 状态页](camoufox-program-status.md)为准；稳定架构理由与重评条件见[Camoufox-first Managed Engine 决策](camoufox-managed-engine-decision.md)，本次 Simprint 源码尽调见[Simprint source-level due diligence](simprint-source-due-diligence-2026-09-09.md)。

## 变更规则

以下变化必须作为新的显式产品/架构决策记录，不能由单个执行任务静默改变：

- 将当前主引擎从 Camoufox 切换为 Controlled Chromium 或其他内核；
- 将 WSL、Hyper-V、VMware 或 Remote 设为普通 Silo 默认执行层；
- 合并 Profile、Identity Artifact、网络秘密或证据生命周期；
- 弱化 `configured`、`applied`、`observed`、`verified`、`unavailable` 的区别；
- 将“协调稳定身份”改为每次启动重新随机。
