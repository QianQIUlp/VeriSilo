# VeriSilo 可验证身份运行时产品决策

- 状态：**Accepted**
- 日期：2026-09-09
- 决策类型：产品定位 / 长期能力边界
- 证据基础：[Simprint × VeriSilo 源码级技术尽调](simprint-verisilo-technical-due-diligence.md)
- 审查基线：Simprint `d2dc939b`；simprint-browser-kernel `simprint/m144` @ `9f7b8e0`；VeriSilo `baseline/dev` @ `f6ae647`

## 决策

VeriSilo 不停止开发，但停止把“做一个功能更完整的开源商业指纹浏览器工作台”作为产品扩张方向。

长期定位收缩为：

> **VeriSilo 是一个本地优先、fail-closed、以可验证身份为核心的隔离浏览器运行时。**

核心价值不是“支持多少可编辑指纹字段”或“拥有多少多环境运营功能”，而是把一个长期身份作为受约束、可追踪的执行对象，并尽可能回答：

> **声明要运行的身份、引擎和网络策略，最终是否真的按声明运行。**

现有五元领域模型继续成立：

```text
Silo
= Persistent Profile
+ Resolved Identity Artifact
+ Engine Binding
+ Network Policy
+ Runtime Evidence
```

这五个部分保持不同生命周期和明确绑定，不合并成普通 Profile 配置块。

## 为什么不是停止项目

2026-09-09 的源码级对比确认，Simprint 已经在“开源本地多 Profile 指纹浏览器工作台”这一产品层形成强覆盖，并在 Chromium 源码补丁、自动化和桌面运营能力上明显领先。

但 Simprint 当前源码没有覆盖 VeriSilo 的以下核心机制：

1. **Resolved Identity Artifact / Identity Integrity**：VeriSilo 的身份工件版本化、带 SHA-256 完整性、首启锁定并随 launch plan 携带期望绑定；Simprint 当前是可编辑自由 JSON 配置。
2. **Runtime Evidence / Website Observation**：VeriSilo 明确区分 configured / applied / observed / verified / unavailable，并把网站观察、运行证据、Silo 归属和 stale 防护作为一等模型；Simprint 当前没有同等持久运行证据模型。
3. **Network Attribution**：VeriSilo 把网络事实归属到具体 Silo / provider / selector / node，并检测选择漂移；Simprint 当前没有同等证据归属模型。
4. **Fail-closed Network Semantics**：VeriSilo 对 must-proxy 场景在启动前 preflight，拒绝 PAC 直连可能性并收敛 DNS/QUIC/WebRTC；Simprint 当前没有统一的启动前 fail-closed 契约。

因此，Simprint 没有把 VeriSilo 的核心价值层做完；停止项目会同时放弃这些已经由源码实现、且对高价值长期身份具有实际意义的机制。

## 为什么也不是继续“大而全”

Simprint 已经证明下面这些能力不是 VeriSilo 应继续投入大量时间重复建设的差异化：

- Profile CRUD 的广度；
- 分组、标签、团队、工作区、回收站、模板等运营工作台能力；
- 通用 REST API / MCP 大工具集；
- 通用 RPA；
- 多窗口输入同步；
- 账号自动导入；
- 通用代理资产管理、批量导入导出和健康检查工作台；
- 以“支持更多 Chromium 指纹 patch 面”为目的的并行内核军备竞赛。

这些能力可以在出现明确用户需求时单独重评，但不能再因为“商业指纹浏览器通常有”而自动进入 VeriSilo 路线图。

## 保留并强化的核心能力

### 1. Identity Integrity

必须保留并强化：

- identity intent → resolved artifact；
- schema / version；
- artifact hash；
- 首次成功启动后的身份锁定；
- profile / artifact / engine binding 的明确关系；
- identity drift（身份漂移）检测和可解释失败。

用户价值：长期账号身份不会因为一次无记录编辑或静默配置漂移而失去可追踪性。

### 2. Execution Integrity

必须能够明确回答实际使用的：

- engine family；
- engine version；
- package identity；
- artifact binding；
- 当前运行实例归属。

受控执行失败不得静默回退到另一个没有同等保证的浏览器路径。

### 3. Network Integrity

对于声明 must-proxy / bound-provider 的 Silo：

- 代理前置条件失败应阻止运行，而不是静默直连；
- DNS / QUIC / WebRTC 等绕行面只在已有能力边界内声明；
- 配置、应用和实际出口证据不能混为一谈；
- 网络诊断必须归属到当前 Silo 的绑定，而不是扫描或借用无关本机代理状态。

### 4. Runtime Evidence

Runtime Evidence 是今后最重要的产品差异化之一。

目标不是产生更多日志，而是建立：

```text
Expected
→ Applied
→ Observed
→ Compared
→ Attributed
```

的低摩擦用户体验。

当前 `observed.json` 仍是 observed evidence，不是自动 verified Gate。未来如果实现 expected ↔ observed 自动对账，应继续保留这种证据等级差异，而不是把观测结果直接升级成 verified。

### 5. Evidence Attribution

任何运行状态、网络事实、网站身份观察和错误证据都必须能回答“属于哪个 Silo / 哪次运行 / 哪个绑定”。

无可靠归属时应使用 unavailable / unknown / stale 等诚实状态，不得把历史全局状态展示成当前身份事实。

## 功能进入路线图的 Gate

新的较大产品功能默认必须强化至少一个核心维度：

1. **Identity Integrity** — 身份不漂移；
2. **Execution Integrity** — 正确引擎执行正确身份；
3. **Network Integrity** — 网络策略不泄漏、不静默降级；
4. **Runtime Evidence** — 能直接证明或观察实际发生了什么；
5. **Attribution** — 证据准确归属于正确 Silo / run。

如果一个功能一个维度都不增强，默认不进入主路线图。

这不是绝对禁止。只有新的明确用户需求、兼容性事实或产品证据足以改变收益判断时，才通过新的显式产品决策重评。

## 当前明确不扩张的方向

在没有新决策前，不把以下内容作为“补齐商业指纹浏览器功能”的默认任务：

- 团队 / 工作区 / 回收站 / 模板等大规模运营后台；
- 通用 RPA 平台；
- 为功能数量而扩张 REST / MCP 接口面；
- syncer 类多窗口运营自动化；
- 通用账号资产管理；
- 通用代理资产市场或代理运营控制台；
- 与 Simprint 竞赛 Chromium patch 数量；
- 仅为了宣传“商业级功能齐全”而增加的非核心功能。

Standard Silo 不因本决策删除。用户仍需要足够完成创建、启动、停止、继续使用、诊断和理解证据的桌面产品体验；“停止工作台竞赛”不等于“停止做可用 UI”。

## Simprint 证据的使用边界

本决策依赖审查时冻结的三个源码基线。以后引用竞争对手事实必须保持以下边界：

- Simprint kernel 中 `"default"` fingerprint seed 回退是**源码级潜在关联缺陷**；没有运行实测前，不得扩大成“已证明所有 Profile 都能被网站关联”。
- Simprint 的 timezone/language IP 检测路径存在代理失败后的 direct fallback；但浏览器主流量在代理失效时是否回退直连仍是 `UNKNOWN`。不得宣传成“已证明 Simprint 浏览器一定泄漏真实 IP”。
- Simprint 的实现可以变化。任何以后用于改变 VeriSilo 产品方向的竞争结论都必须重新核验当时源码，而不是永久复用 2026-09-09 的快照。

## 当前阶段不变

本决策改变的是**长期产品边界**，不是当前工程阶段。

当前仍处于 **Pre-RC product stabilization**，继续遵循：

```text
QA → targeted fix → integration → baseline advance → fresh QA
```

不要因为本次竞争审查而推倒当前架构、重启 FP1–FP4、创建 FP5、切换 Chromium、或者提前打开 RC / installer gate。

## 重评条件

以下任一情况出现时，应重新审查本决策，而不是静默漂移：

- Simprint 或其他开源项目实现了同等级的 immutable identity artifact + runtime evidence + fail-closed attribution 链；
- 用户研究表明目标用户对 evidence / fail-closed / identity integrity 没有足够价值感；
- Camoufox 的兼容性、维护或分发成本使核心执行路径不可持续；
- Controlled Chromium 的 TLS/QUIC/V8/扩展能力变成核心用户需求，而不是功能竞赛；
- VeriSilo 的 evidence 链长期无法从内部工程机制变成普通用户可理解、可使用的产品体验；
- 新的竞争格局使独立实现的预期价值低于采用 / fork / upstream 其他项目。

## 结论

2026-09-09 的项目级结论是：

**B — NARROW / DIFFERENTIATE。**

VeriSilo 继续，但不再以“另一个更完整的开源指纹浏览器工作台”为目标。

后续投资优先围绕：

```text
Identity Integrity
+ Execution Integrity
+ Network Integrity
+ Runtime Evidence
+ Attribution
```

形成一个真正可验证、失败不静默降级、证据有明确归属的长期浏览器身份运行时。
