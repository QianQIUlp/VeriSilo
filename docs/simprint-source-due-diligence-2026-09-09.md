# Simprint source-level due diligence

状态：**Accepted product-direction input**。本文记录一次冻结的源码级 review，不是
Simprint runtime qualification，也不是 VeriSilo 当前 baseline 或 release evidence。

## Frozen review objects

| 对象 | Review revision |
| --- | --- |
| Simprint main | `d2dc939bfae8efda8e4e4d791587d7a364ca4dd8` |
| `simprint-browser-kernel` | `simprint/m144`，`9f7b8e0` |
| VeriSilo review baseline | `f6ae647764b2577f2286c1f208c9f02acd4b6fde` |

这些 revision 只定义当时源码审查的输入对象；它们不是当前 VeriSilo canonical baseline。

## What Simprint did better in the reviewed source

Review 中清晰可见的优势包括：

- desktop Profile CRUD breadth，以及 groups、tags、workspaces、recycle-bin 式工作站能力；
- REST API、52 个 MCP tools、per-environment CDP endpoint 和 multi-window input sync；
- account import、extension management、proxy asset usability/health checks；
- updater/release cadence；
- 更广的 Chromium source-patch surface。

其 Chromium patch surface 包含 Camoufox/Firefox 当前不直接匹配的更深 Chromium/net-layer
工作，包括 TLS 相关 patch 与 MAC/network-layer 修改。VeriSilo 不应为了维护自身叙事而贬低
这些真实优势。

`VeriSilo has real differentiators != VeriSilo is globally more advanced`：下文所有
user-value 论证都不削弱本节记录的 Simprint 领先事实。

## VeriSilo's structural difference

VeriSilo 当前实现的差异不在于“本地而对方不本地”，而在于：

- **Identity Integrity**：Resolved Identity Artifact 版本化、SHA-256 完整性、稳定持久化、
  首次成功启动锁定，并绑定到 engine/runtime；
- **Execution Integrity**：声明的 Artifact 必须由声明且经过验证的 engine/package 执行；
- **Runtime Evidence**：明确区分 `configured`、`applied`、`observed`、`verified`、
  `unavailable`，并将证据绑定到正确 Silo；
- **Network Integrity**：诊断和 evidence 归属于 Silo、provider、selector、node，
  must-proxy 路径在 launch 前 fail closed，不静默降级；
- **Engine/package trust**：exact asset binding、package tree、detached CMS signature、
  Desktop public signer pin 和 fail-closed verification。

`local-first itself is NOT a differentiator`。VeriSilo 的定位不是开源版 AdsPower、免费
商业指纹浏览器克隆或完整浏览器工作站替代品。

## Accepted product verdict

**B — NARROW / DIFFERENTIATE**

这意味着：不停止 VeriSilo，不追求 Simprint/AdsPower 的广度或 feature parity；强化已经
存在、结构不同且用户可感知的 integrity/evidence 能力。该结论不是立即重写架构，也不
移除 Standard Silo；Camoufox-first 仍可作为当前 Managed Engine strategy。

未来大型功能提案默认需要增强至少一项：

1. Identity Integrity；
2. Execution Integrity；
3. Network Integrity；
4. Runtime Evidence；
5. Attribution。

如果功能完全不增强这五项，不能仅因为商业指纹浏览器普遍具备，就自动进入 roadmap。

### Why not A — STOP / ADOPT

不选择“停止 VeriSilo、直接采用 Simprint”，因为 Simprint reviewed source 没有覆盖
VeriSilo 主张的核心结构能力：

- Resolved Identity Artifact / identity integrity（版本化、完整性、首次成功启动锁定、
  可重放）；
- Runtime Evidence / website-visible observation（`configured`/`applied`/`observed`/
  `verified`/`unavailable` 语义与证据归属）；
- precise network attribution；
- equivalent unified fail-closed semantics（如 must-proxy launch preflight）。

reviewed source 中未发现这些等价机制，因此 VeriSilo 的结构性主张不能被简单替代或并入。
注意保持 source-evidence boundary：这是 source-level 结论，不是 runtime qualification。

### Why not C — CONTINUE BROADLY

不选择“继续全面扩张、与 Simprint 正面竞争”，因为 Simprint 在以下方向已有大量真实覆盖
和明显领先：

- desktop workstation breadth（Profile CRUD、groups、tags、workspaces、recycle-bin）；
- automation（REST API、52 个 MCP tools、per-environment CDP、multi-window input sync）；
- Chromium source-patch breadth（含 TLS/MAC/net-layer 深层修改）；
- proxy usability 与 asset health checks；
- extensions；
- release/update/product maturity。

在 feature-parity 赛道上追赶更成熟的产品没有结构性回报。

综合两者：**VeriSilo 应继续存在，但只在自身结构性差异有真实用户价值的条件下继续投资。**
该用户价值的展开见下一节。

## User-value thesis

Verdict B 成立的前提不是“架构更漂亮”，而是以下能力对特定用户有真实价值。本节是基于
source review 的产品假设（product thesis），不是市场调研证明。

### Identity Artifact / Identity Integrity

用户价值不是“架构更漂亮”，而是：

- 长期身份防漂移；
- 可以回答一个账号过去到底以什么身份运行；
- 配置变化可审计；
- 身份可以稳定重放。

对于高价值、长寿命、不希望环境被随手改坏的账号/身份，这是真实价值；对于随建随弃的一次性
环境，增量价值较低。

### Runtime Evidence

核心语义是：**配置了什么 != 实际运行成什么**。Evidence 的实际价值包括：

- 启动后知道引擎/身份/网络是否真的应用；
- 减少人工打开检测站逐一确认；
- 出现账号事故时可以回看运行事实，而不是猜；
- stale evidence 不冒充 current。

Evidence 必须最终成为低摩擦的产品体验，而不能永远只是内部工程 evidence。

### Fail-closed network

这是当前差异化中现实用户价值最高的方向之一。核心事故模型：

```text
required proxy fails
→ silently falls back to direct
→ real network identity exposed
→ multiple persistent identities may be correlated
```

VeriSilo 的价值不是“代理功能更多”，而是：**当用户明确声明 must-proxy 时，系统宁可拒绝
启动，也不静默用错误网络继续。**

按 Evidence precision 边界，本节不声称 Simprint runtime 已被证明主浏览器流量直连泄漏；
reviewed source 的直接观察是至少一条辅助 IP 检测路径含 direct fallback，且未发现等价的
统一 must-proxy launch contract。

### Engine/package trust

诚实记录：普通用户对 CMS signing、certificate pin、exact executable provenance 的直接
感知价值可能低于前三项。它的主要增量价值是：

- 防静默 engine drift；
- 供应链完整性；
- 审计 / controlled runtime。

Simprint 已有 content-addressed kernel binding，因此不能写成“对方完全没有 engine
integrity”；差异主要是保证强度（exact executable provenance、detached CMS signature、
public signer pin 与 fail-closed verification）。

### Encrypted Vault

用户价值：

- 多账号/代理凭据集中存储时避免明文落盘；
- sensitive material 的 lifecycle 更可控；
- zeroize 等属于 defense-in-depth。

Vault 是支撑能力，不单独构成产品全部定位。

## Counterfactual: would we start VeriSilo today?

Accepted answer: **ONLY_IF**。

只有以下条件同时成立，今天从零开始做 VeriSilo 才合理：

1. 目标用户确实需要 identity artifact + runtime evidence + fail-closed
   execution/network semantics，而不仅是多个浏览器环境；
2. 接受不与 Simprint 竞争 Chromium patch breadth：当前使用 Camoufox/Managed Engine，
   SSL/MAC 等 Chromium-specific patch breadth 不作为必须追平项；
3. 接受 VeriSilo 在 workstation/product breadth 上可能长期落后，依靠 integrity、
   evidence、attribution、trust 成立，而不是 feature count。

如果目标只是普通多环境桌面工作台 + 更多 CRUD + 自动化 + 多窗口运营便利，counterfactual
answer 是 **NO**：这类需求从零开始更合理的选择是采用或 fork 已存在的 Simprint 或同类
产品，而不是重做 VeriSilo。

这不是要求现在停掉 VeriSilo；它是 future strategic guardrail，用于判断“继续独立投资”
何时失去理由。

## 3–6 month investment judgment

Accepted conclusion: **YES — CONDITIONALLY**。

继续投入的理由不是“已经写了很多代码”——沉没成本不构成理由——而是：identity integrity、
runtime evidence、fail-closed network/execution 与 attribution 是当时 Simprint reviewed
source 中真实缺失或明显更弱、同时可能产生用户价值的结构性能力。

未来 3–6 个月继续投入的前提是执行 B — NARROW / DIFFERENTIATE，重点投资：

- identity integrity；
- evidence UX；
- expected-vs-observed reconciliation；
- fail-closed execution/network；
- attribution；
- engine/package trust；
- 支撑这些能力所需的最小产品 UI。

明确停止条件：如果未来实际 roadmap 变回“做一个功能更全的商业指纹浏览器工作台”——以
团队/工作区、大量自动化、通用 RPA 或大规模 Chromium patch breadth 为竞争中心——那么
继续独立投资 VeriSilo 的理由需要重新评估，默认不应仅凭沉没成本继续。

> If the project returns to broad feature-parity competition, the investment thesis must
> be re-opened.

这是重新评估条件，不是不可逆的永久禁令；真正的新用户需求仍可以重新打开决策。

## What VeriSilo should keep

1. Resolved Identity Artifact / immutable identity chain；
2. Runtime Evidence + website-visible observation；
3. fail-closed Network Policy + attribution；
4. encrypted Vault / secret handling；
5. engine package trust + EngineAdapter contract；
6. CLI / diagnostics / direct runtime verification；
7. 支撑以上能力的最小 Desktop UX。

## What VeriSilo should stop competing on by default

除非新的具体用户需求重新打开决策，默认 roadmap 不再主动扩张以下方向：

- broad team/workspace/recycle-bin workstation breadth；
- generic REST API for every resource；
- huge MCP tool catalog；
- general RPA；
- multi-window syncer；
- account-import automation；
- generic proxy asset-management platform；
- 仅为 feature count 的 Chromium patch arms race；
- competitor feature checklist parity；
- 把 VM/Hyper-V/Remote 变成普通 Silo 执行层（Windows Sandbox acceptance 是测试实验室
  证据，不是 VeriSilo 新增 Hyper-V 产品能力的理由）。

这不是删除已有实现的命令：已落地的能力按各自生命周期保留。含义是默认 roadmap 不再主动
扩张这些方向。

## Differentiated roadmap

第一优先方向是把“期望身份”与“网站实际观测身份”连接成可解释的产品闭环：

```text
Resolved Identity Artifact
        ↓ expected
website observed.json
        ↓ observed
automatic comparison
        ↓
matched / mismatched / unavailable / stale
```

即 expected identity + observed website-visible identity → reconciliation → mismatch
explanation → evidence presentation。用户最终应能低摩擦看到：**我声明的身份，网站实际上
看到了什么，两者哪里一致、哪里不一致、哪些字段当前无法验证。**

Evidence 语义纪律保持不变：

> expected-vs-observed reconciliation 是缩小 `observed → verified` 差距的主要产品路线；
> 只有满足未来 capability-specific verification contract（能力专属验证合同）时，才能升级
> 为 `verified`。

观测值相等不会自动把结果提升为 `verified`；`unavailable` 与 `stale` 仍是合法结果。

本节记录的是 product direction、investment condition 与 roadmap priority。具体实现
（evidence 比较引擎、presentation、Artifact schema 或 RuntimeEvidence 状态机变化）属于
后续 product task，不在本 documentation task 内执行。

## Evidence precision

### Fingerprint seed

源码审查发现某 noise seed path 在没有实际 environment/profile id 时 fallback 到
`"default"`。正确结论是：

> Source review found a potential cross-profile fixed-noise correlation defect.

本次 review 没有运行该缺陷的 runtime experiment，因此不能写成“已证明不同 Profile 可以被
关联”。

### Proxy/direct fallback

源码审查发现至少一个 IP/timezone/language detection 辅助路径存在 direct fallback，且没有
发现 VeriSilo 风格的统一 must-proxy launch preflight。正确结论是：

> Simprint did not show an equivalent unified fail-closed launch contract, and at least one auxiliary IP-detection path contains direct fallback.

这不是 Simprint 主浏览器流量已经在 runtime 被证明代理失败后直连泄漏。

### Runtime evidence mechanism

可以说的结论是：reviewed source 未发现与 VeriSilo 等价的系统性 runtime evidence 机制。
本次 original review 是 source-level only，没有运行过 Simprint，本文不暗示任何 Simprint
runtime 实验。
