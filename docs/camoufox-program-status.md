# Camoufox Managed Engine 当前状态

- 状态：**当前路由页**
- 更新日期：2026-09-28
- 当前稳定产品分支：`codex/camoufox-m3-engine-adapter`
- 当前 canonical development source：`origin/baseline/dev`（本地工作引用为 `baseline/dev`，正常时两者精确相等）
- 当前公开版本：**v0.1.0-rc5**（`PUBLIC_GITHUB_PRERELEASE`；tag `v0.1.0-rc5`，release gate 已关闭）
- `RC5_SOURCE_SHA`：`7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`；这是固定 release source，不是开发基线
- 当前工程阶段：**post-rc5 normal product development**；新任务从最新 `origin/baseline/dev` 分叉
- 历史 rc2 candidate：source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`，installer SHA-256
  `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3`；文档变化不改变其 source binding

本文只保留当前事实、下一任务和关键证据索引。旧 checkpoint、失败 run、完整 hash 表与历史
措辞由 Git、lock/result、evidence 和对应历史合同保存，不再永久追加到默认必读页。

普通 Camoufox 任务读完本文后，按“当前下一任务”本节和 owning code/test 执行；只有该节
明确指定独立 active contract 时才再读取。只有改变产品或架构时才读取
[北极星](identity-platform-north-star.md)和
[Camoufox-first 决策](camoufox-managed-engine-decision.md)。

## 当前产品方向

VeriSilo 要交付可持久、可重放、可验证的浏览器身份环境：

```text
Silo = Persistent Profile
     + Resolved Identity Artifact
     + Engine Binding
     + Network Policy
     + Runtime Evidence
```

Standard Silo 长期保留。Camoufox Managed Engine 的 standalone、资格链、生产 package/
adapter、Managed Silo 产品路径、current-user NSIS、rc2 的 packaged runtime 和 Windows
Sandbox 安装验收，以及 v0.1.0-rc5 公开 prerelease，均已达到各自文档边界。Profile、
Artifact、Engine、Network 与 Evidence 不合并，`configured`、`applied`、`observed`、
`verified` 与 `unavailable` 不混用。

rc2→rc4 的历史资格与公开发布序列、rc5 的 source-bound 发布已经完成；
rc5 安装证据只按其单独的 bounded report 分类：

```text
source stabilization → rc2 build → package verification → packaged runtime
→ pristine Windows Sandbox installed lifecycle → v0.1.0-rc3 → v0.1.0-rc4 public prerelease
→ rc4 release gate closed → post-rc4 product development → v0.1.0-rc5 public prerelease
→ rc5 release gate closed
```

当前阶段是 **post-rc5 normal product development**：canonical development source 是
`origin/baseline/dev`，`main` 不因落后而自动推进。近期性能与响应性工作已经收口，并在原生
Windows Managed 开发实例中取得有限生命周期证据；工作树 Camoufox 启动差异已定位为当前
Codex sandbox/worktree 的继承 DACL 开发限制，正常用户上下文的独立 LocalAppData 产品路径通过。
详见[性能任务处置](qa/performance-architecture-2026-09-27.md)和
[目录 ACL 因果对照](qa/managed-engine-directory-acl-2026-09-27.md)。这些结果不构成普遍性能保证或安装验收。
当前没有待集成的产品任务分支，也没有活动中的 fingerprint、QA 或 release Gate。

## v0.1.0-rc2 current-source installed acceptance（历史记录）

rc2 的固定身份仍为：

```text
releaseVersion = v0.1.0-rc2
RC_SOURCE_SHA = c1688d5a392ffa69ae77c246bcb4bb78b083e26f
installerSha256 = 3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3
```

候选 package 已具备 current dual-boot Formal-v3 exact bindings、current Host、detached
CMS signature、approved signer/public pin match、package-tree verification、release
provenance 和 SBOM/license evidence。现有 packaged Managed runtime 已完成：

```text
provision → hello → launch → page snapshot → page windows → status → close → shutdown
```

response-ID correlation 通过。既有 stale development package 不支持 `page` 的 coverage
boundary 已关闭。

安装验收 evidence：

- QA branch：`origin/agent/qa/v0-1-0-rc2-installed-69d6d6`
- evidence tip：`df1b82fe3ca2f071a4b6676adc92db046558cdd3`
- 环境：pristine/disposable Windows Sandbox，Microsoft Windows 11 Enterprise `10.0.26100`，AMD64
- candidate transfer integrity、install、Desktop GUI、Vault initialize/lock/unlock、Standard
  create/first start/second start、Managed provision/package verification/first start/second
  start、Desktop restart persistence、same-version reinstall、Vault/Standard/Managed preservation、
  Managed post-reinstall start、uninstall、application binary removal 和 test-data preservation：**PASS**；
  `reinstallAfterUninstall=NOT_RUN_OPTIONAL`
- 最终分类：`CURRENT_SOURCE_INSTALLED_PRODUCT_ACCEPTED_IN_WINDOWS_SANDBOX` / `CLEAN_DISPOSABLE_WINDOWS_SANDBOX_ACCEPTANCE_PASS`

Sandbox 使用的 `WDAGUtilityAccount` 具有 `IsAdministrator = true`。因此 strict standard-user
install/reinstall/uninstall semantics 仍为 **NOT_PROVEN**；这不是 installer lifecycle failure，
也不是 rc2 acceptance pending，而是未来 public-release/promotion boundary。外层 Desktop/NSIS
Authenticode 仍为 unsigned；内部 engine CMS signature 与外层 Authenticode 是不同边界。

rc2 验收期间的已知观察项 `MANAGED_STOP_TRANSIENT_NETWORK_POLICY_MESSAGE`
（首次 Managed close/Direct stop 时短暂显示 proxy/network mismatch，分类
`LOW / NON_BLOCKING_UX_INCONSISTENCY`）已在此后由 owning source 修复关闭，
不再是当前未决观察。

## M3 研究结论

- M3-0 在 `e96ef3f` 关闭了 fake Host package、transport、failure matrix、
  RuntimeManager lifecycle 和 evidence 语义；它没有真实浏览器 run-id。
- M3-WI 的 R2 十周期真实 soak 证明同一 Profile、Cookie 和 observed digest 可以在
  一次受控序列中稳定保持，但同一 Host/test 源码的后续 Host matrix 六次只有一次通过。
- 最后的 R2H test-only 候选为 `186484f` / tree `e33d6d6`。预声明序列的 persistence
  与 lock-crash 各通过一次，第三项 persistence 在第二 Host `launch` 等待 stdout
  response 120 秒后失败；没有重试、没有 evidence manifest、没有 Accepted commit。
- 该历史 investigation 当时的主脑终局为 **M3-WI failed**，Camoufox Windows Managed
  productionization 在该 checkpoint 暂停，且不再创建 R3/R4 或新的 test-only 子 Gate；后续
  Formal-v3、FP1–FP4 与 clean M3-WI Attempt 4 已改变当前状态，以本文开头的当前状态为准。

## Standard Silo Windows preview 首次执行

- 主脑合同提交为 `944dff9`；产品候选为 `b93259a` / tree `76eb5f3`，只修改创建页、
  UI contract test、样式和人工 Windows runbook。
- 候选让 Edge-only/Chrome-only 机器自动选择首个有效浏览器，默认路径收敛到
  Local + Direct；WSL、手工路径与网络配置进入高级设置；本机 stock Silo 运行时短周期
  核对状态，并诚实展示 `native`、`inherit`、`unavailable`。
- `pnpm` check/test/build、desktop Rust fmt/test 和 unsigned desktop-only build 通过；
  起点已有的两项 WSL Clippy warning 未在本任务越界修复。既有 Windows acceptance
  driver 缺少 `execution_target`，由 `5b09f04` 仅补 `SiloExecutionTarget::Local` 后编译通过。
- 正式 Edge/desktop-core acceptance 没有开始。执行 Agent 错误地用裸
  `msedge.exe --version` 探测版本，Edge 实际以默认 Profile 启动；虽从启动前零 Edge
  进程出发并按精确根 PID 回收整个新进程树，因没有启动前 Profile 指纹，默认 Profile
  是否被修改为 **unknown**。
- 主脑 Gate：**failed**。没有 desktop-core receipt、browser E2E summary 或真实 preview
  smoke，unsigned 构建不得作为 Accepted、正式 installer 或 shipped 产品使用；不自动
  重试，不读取用户 Edge Profile，也不删除或改写现场。

## Standard/Profile Isolation Windows local Preview 收口

- 最终产品 checkpoint 为 `aa72eadaf8300d1cd33a2c32173c06e3e677ca89` / tree
  `cd126770be02a33c6bb698853813512748b894c8`。原生 Windows Server 上的 source-bound
  desktop-core acceptance、Edge A/B Profile 与冷启动持久化、默认 Profile metadata
  前后核对及真实 Preview smoke 已通过；上节记录的历史默认 Profile 影响仍保持
  **unknown**，本次 metadata 一致只证明本次没有新增影响。
- 交付物是 **unsigned local Preview**，不是签名 installer、shipped release 或正式发布。
  已验证平台是 Windows Server；正常 Windows 10/11 client release matrix 仍待补充。
- Standard/Profile Isolation 只包含独立 Profile、网站状态持久化、单活 ownership 和本机
  Chrome/Edge 生命周期。它不包含 Managed Identity、设备或浏览器指纹虚拟化、代理隔离、
  WSL/Remote/Hyper-V 或整机虚拟化。
- 在该历史 checkpoint，Camoufox 仍在独立 Managed Engine 工作树调查，不进入这条产品集成链。Profile
  隔离层从本 checkpoint 起冻结；除真实回归缺陷外不再扩张。

## 已关闭 Gate 与当前 release-readiness 状态

| 能力 | 当前结论 |
| --- | --- |
| Standard Silo Windows Profile Isolation | **Closed**；Windows Profile isolation、生命周期与回归边界已闭合，Standard Silo 长期保留 |
| Linux M0–M2 standalone Host / Artifact | **Accepted**；仅对应已记录平台，`verified:false` |
| 原生 Windows M2-W | **Accepted**；Profile、Artifact replay 与 Job/process ownership Gate 已关闭 |
| M3-0 EngineAdapter contract slice | **Accepted** at `e96ef3f`；fake Host contract，不是 shipped desktop |
| 历史 M3-WI real Windows desktop/Host | **Failed / Inconclusive**；旧合同不复活，旧 test-only 路径保持 historical/experimental，不代表当前 production path |
| FP1 deterministic Artifact projection | **Accepted by corrected adjudication of immutable A1/A2/B1 evidence**；原 runner verdict 仍 Failed，`verified:false` |
| 历史 FP2 candidate | **Failed / retired**；Generation 6 永久关闭 |
| R1-diag Windows build/provenance | **Passed diagnostic-only closure**；不是 Formal 或 runtime pass |
| actual-9000 diagnostic run | 原 runner Failed，离线裁决 Inconclusive |
| Voices phase-anchor | v1 Failed/no observation；唯一 v2 run 直接支持 A0→A1→A2 |
| Voices `0005` + Artifact v4 policy | **Static authoring closed**；仅为 Formal source candidate 输入，不是 runtime pass |
| Formal source + Windows-target build/provenance | Formal-v3 **Passed build/provenance closure**；精确 runtime tree 已绑定并用于原生 Windows qualification |
| Formal R1 runtime / FP1-R1 | **Formal R1 Passed on this native Windows host**；FP1 carry-forward 与 Formal-v3 FP2 均已闭合，`verified:false` |
| FP2 / FP3 | **FP2 与 FP3 均 Passed on this native Windows host**；FP3 覆盖 exact required route、出口、timezone/locale、Geo、ICE 与 clean lifecycle，`verified:false` |
| FP4 ordinary-site compatibility | **Passed on this native Windows host**；精确 V5 六项 task、Profile replay 与 clean lifecycle 全部通过，`verified:false` |
| clean M3-WI | **Passed on this native Windows host** at Attempt 4；真实 Desktop RuntimeManager / test-only adapter / Host / Browser 两周期闭合，`verified:false` |
| production package/signing/UI/S1 stabilization | **Implemented/build closed** in the current source；内部 CMS 签名 package、Desktop public pin、production adapter、Managed Silo UI/network/run path、outer-unsigned current-user NSIS，以及 S1 的 Create Silo UX、reconcile-stopped lifecycle、BUG-01 和 acceptance-driver 修复已进入当前源码 |
| Managed Font Isolation | **Closed on this native Windows host**；生产预设统一采用 `managed` 模式，Windows 未声明宿主字体 masking 经 rendering oracle 与 fail-closed 静态/运行时双重验证（commit `1b41b81`）；跨主机字形渲染的一致性（cross-host hermeticity）未声明 |
| Window Geometry Coherence | **ACCEPTED_IN_WINDOWS_SANDBOX**；源码 clamp 与真实浏览器四场景验收已完成，详见 [Packaged-Host 验收](qa/packaged-host-sandbox-runtime-acceptance/STATUS.md)；旧环境阻断记录仅是当时的历史状态 |
| Evidence Coverage Closure II (WebGL2 + Accept-Encoding) | **ACCEPTED_IN_WINDOWS_SANDBOX**；真实浏览器 WebGL2、请求头与 Fresh Recheck 对账已通过，详见 [Packaged-Host 验收](qa/packaged-host-sandbox-runtime-acceptance/STATUS.md)；旧 harness pending 记录已被取代 |
| Transport Coherence | **Observed Coherent**；QA 诊断（commit `94a3641`）在真实 Managed session 上直接观察到 TLS ClientHello 与 HTTP/2 行为与 Firefox 152 NSS 自洽，观察到 QUIC v1 Initial 握手尝试；formal verifier 仍 unavailable，保持诚实未声称 |
| WebGPU Boundary | **Policy Boundary Open / No Leak Observed**（分类 `WEBGPU_NO_LEAK_OBSERVED_BUT_POLICY_BOUNDARY_STILL_OPEN`）；pinned Camoufox 152 无 WebGPU 伪装原语，安全上下文中 `requestAdapter()` 在本虚拟/无独显环境返回 `null`，未观察到真实硬件泄露；禁用将造成 `navigator.gpu` 缺失反常，需底层 Gecko 补丁 |
| Historical local `v0.1.0-rc1` artifact | **Historical candidate / superseded / never runtime-accepted**；source `6497828aa0643f94fed3ae708734eef6b85f8305`, dirty `true`, verifier passed for 1403 files, acceptance `Pending`, `verified:false`, `runtimeAcceptance:null` |
| v0.1.0-rc2 historical candidate | **PUBLIC_GITHUB_PRERELEASE（已被 rc5 取代为当前公开版本）**；该固定候选在 pristine Windows Sandbox 完成安装后生命周期验收；source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`，installer SHA-256 `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3` |
| v0.1.0-rc3 public prerelease | **PUBLIC_GITHUB_PRERELEASE；release gate 已关闭**；tag `v0.1.0-rc3`，source `407c501741c1be3444bfa607c7e693ecc7324409` |
| v0.1.0-rc4 historical public prerelease | **PUBLIC_GITHUB_PRERELEASE；release gate 已关闭**；tag `v0.1.0-rc4`，source `68e21c3d601e1df3699f1b21431cc23da873e546` |
| v0.1.0-rc5 current public prerelease | **PUBLIC_GITHUB_PRERELEASE；release gate 已关闭**；tag `v0.1.0-rc5`，source `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`；单独 bounded installed report：`INCONCLUSIVE — installed files matched; Managed runtime/repair/uninstall flow not completed`；完整旧合同 `windows-acceptance-report.json` 仍为 `Pending` / `verified:false` / `runtimeAcceptance:null` |
| Current source release readiness | **post-rc5 normal product development**；当前没有活动中的 QA/release Gate；strict standard-user install/reinstall/uninstall semantics 仍 `NOT_PROVEN`，不要将 bounded RC5 安装检查外推至完整旧合同矩阵或所有环境 |

## 当前未证明的边界

- 生产预设已采用 `fontMode=managed`，Windows 宿主未声明字体遮蔽已通过渲染神谕与 fail-closed 验证生效（commit `1b41b81`），但跨主机字形渲染的完全一致性（cross-host hermeticity）仍未声明（仍依赖宿主可用字体库与字体回退规则）；
- 传输层与反检测：TLS ClientHello、HTTP/2 特征与 QUIC v1 已在受控测试中直接观察到自洽（QA commit `94a3641`），但 formal transport verifier 仍 unavailable，不声明绝对“不可检测”；
- WebGPU 边界：分类为 `WEBGPU_NO_LEAK_OBSERVED_BUT_POLICY_BOUNDARY_STILL_OPEN`。安全上下文暴露 `navigator.gpu`，`requestAdapter()` 在当前环境下返回 `null`，无硬件直接泄漏；禁用 WebGPU 会导致 API 缺失异常，尚无针对 WebGPU adapter info 的伪装原语；
- FP3 不证明 Camoufox 原生 Geolocation provider 或 exhaustive native address inventory；
- FP4 只覆盖冻结的 V5 live-site matrix，不声明 universal compatibility；login、payment 与 CAPTCHA 未测试；
- clean M3-WI 的既有 Attempt 4 仍只证明当时的 test-only adapter 路径；rc2 已另外在 pristine
  Windows Sandbox 直接验收 current-source package/adapter、精确 runtime bindings 和安装后生命周期，
  但这不外推为所有 Windows hardware/edition 或 strict non-admin 用户路径；
- rc2 Sandbox 使用的 `WDAGUtilityAccount` 具有 `IsAdministrator = true`；strict unelevated
  standard-user install/reinstall/uninstall semantics 仍为 `NOT_PROVEN`，这是 future public-release /
  promotion boundary，而不是 installer lifecycle failure；
- rc2 与 rc3 的 Desktop/NSIS 外层均为 `authenticode=false` / unsigned；内部 engine detached CMS
  signature、approved signer/public pin 与外层 Windows Authenticode 是不同边界；这些历史候选的签名状态
  不自动证明 rc4、rc5 或未来候选的签名状态；
- Formal-v3 runtime observation 只覆盖 rc2 在本次 Sandbox 中绑定的 candidate/Artifacts；
  Voices 只覆盖 A1、A2、B1 各自三秒 top-window trace，不是 exhaustive exclusion；FP4 及现有安装验收
  仍不声明 universal site compatibility、undetectability、login/payment/CAPTCHA、完整 TLS ClientHello、
  完整 QUIC 或 exhaustive browser DNS-path。

## 当前下一任务

本轮正常产品开发已完成三项能力：统一当前身份/网络证据的解释与差异定位、Managed 创建后的解析结果确认、每个本机 Standard/Managed Silo 的一条加密最近运行记录。最近记录始终表示历史/最后已知状态，不提供当前运行验证；匹配、无法验证和过期边界保持独立。实现范围和组合验证结果见[产品完整性切片记录](product-integrity-slices-2026-09-28.md)。

随后已完成 [Managed Silo 冷备份与原身份恢复 v1](managed-silo-cold-backup-v1.md)：停止后的 Profile、原始 Artifact、身份/引擎绑定和必要网络配置一起加密保存，通过检查摘要与明确覆盖确认恢复原身份。范围限定同一 Windows、同一系统用户、同一 Vault 路径及兼容引擎；原 Silo 元数据须存在，缺失时先恢复 Vault 配置备份。[冷备份原生开发验收](qa/managed-cold-backup-2026-09-28.md)直接证明 A 的持久 Cookie、LocalStorage、IndexedDB 经改写和恢复后读回原值，Artifact 绑定保持且 B 不受影响；错误口令/摘要被拒绝。恢复后重新取得当前证据，不能把旧记录当成当前验证。

用户随后授权的[本地 Managed Silo 有限并发 v1](managed-silo-concurrency-v1.md)现已实现：同一实例和 Vault 最多两个本地 Camoufox Managed，会话各自管理进程、Profile、代理中继、健康、runtimeId、证据和恢复记录。支持 Direct 与现有固定代理组合，其他引擎、运行位置及 Clash/Mihomo 仍保持单会话边界。[原生证据](qa/managed-concurrency-2026-09-28.md)覆盖 A/B 同站存储隔离、独立重观察/停止/重启、B 运行时 A 冷恢复、本地固定代理路由与单侧失败关闭、锁定及退出/重启归属。UI Preview 覆盖目标、独立 busy、迟到响应和错误；真实窗口已核对 P/Q 选择与双运行显示，90 秒自动 runtimeId 后置对照超时仍记未确认，详见[UI 记录](qa/managed-concurrency-ui-2026-09-28.md)。本地代理实验不推导公网出口或完整无泄漏；这些开发变化未进入 rc4 安装包；rc5 固定候选包含这些能力。

2026-09-28 用户授权的 [RC5 发布](acceptance/managed-browser-rc5.md)已完成：从 `86ee698e5d7d7838b29478625be71a0d905885c6` 开始，固定候选 source `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027` 并公开 prerelease。单独的 bounded installed report 记录 `INCONCLUSIVE — installed files matched; Managed runtime/repair/uninstall flow not completed`，范围见 [RC5 smoke procedure](qa/rc5-installed-smoke.md)；完整旧合同 `windows-acceptance-report.json` 保持 `Pending`。当前没有活动中的 RC Gate、fingerprint Gate 或已知待集成的产品代码分支；下一产品任务尚未指定。

本次 RC5 继承未受影响的产品证据，只对实际候选增加有边界的安装检查。不要默认重跑 FP1–FP4、开启 FP5、继续当前 Codex worktree 的 ACL 深挖或重跑已完成的性能/Managed 生命周期验收。未来若正常产品目录和正常用户上下文出现同类启动故障，再对那个确切环境采集原生证据；旧工作树失败不是当前产品 blocker。

近期性能与响应性工作已完成：[处置和原生 Windows 开发生命周期证据](qa/performance-architecture-2026-09-27.md)记录 Managed 创建 2.952/1.136 秒、创建期间 Vault 读取 25.9 毫秒、两次启动 25.171/13.920 秒、reobserve 1.873 秒、stop 约 1 秒，及页面 ready、122 秒存活、最终 owned processes 0。这些是有边界的开发运行证据，不是普遍性能保证或 rc5 安装验收。[目录 ACL 对照](qa/managed-engine-directory-acl-2026-09-27.md)证明继承 DACL 对当前工作树启动失败的因果作用；尚未识别具体 ACE、Windows API 错误或更深层机制。

rc2/rc3/rc4/rc5 各有固定 source binding；它们不是随开发移动的 ref。rc5 公开 prerelease 已发布且 release gate 关闭。新的 release Gate 只由用户明确指令打开。

## 历史资格链与后续发布路径

```text
Formal-v3 build/provenance
→ FP1 deterministic Artifact projection
→ FP2 cross-realm consistency/replay
→ FP3 network/geo/timezone/locale coherence
→ FP4 bounded ordinary-site compatibility
→ clean M3-WI Attempt 4 native qualification
→ production package/signing/adapter + Managed Silo UI + current-user NSIS
→ v0.1.0-rc2 source-bound freeze
→ package verification + packaged runtime
→ pristine Windows Sandbox installed acceptance
→ v0.1.0-rc3 → v0.1.0-rc4 public prerelease（release gate closed）
→ post-rc4 product development → v0.1.0-rc5 public prerelease（release gate closed）
→ post-rc5 normal product development（canonical: origin/baseline/dev）
```

前半段是已经闭合的资格链，中段记录 rc2 的 source-bound candidate、package/runtime 与
pristine Windows Sandbox 安装验收，随后记录 rc3、rc4、rc5 公开 prerelease 与当前 post-rc5 开发阶段；
rc2、rc3、rc4、rc5 均为 `PUBLIC_GITHUB_PRERELEASE`，既有验收没有证明 strict standard-user 语义。
这条记录不构成新的研究 Gate，也不把历史 RC1 变成当前候选。

## 关键证据索引

| 对象 | 当前锚点 |
| --- | --- |
| upstream Camoufox | tag `v152.0.4-beta.28`；commit `0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d`；tree `1435d544d9b61dee7fcf74cf92462952ca43d38e` |
| Firefox source | `799102676` bytes；SHA-512 `0c5662aba8fb897902af95dbb2fd988b196d9cf9ae8b987ae89e0a6492ac753b8d4b8bb7b3274909c2eb200ab098df356e23cd6084556467f55e69127317f39a` |
| R1-diag closure lock | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json`；SHA-256 `6b93a2425cbf8c54c542a8d134a051d51be39f32239150d2f7ae515b2f00186b` |
| diagnostic ZIP | SHA-256 `241b656945260963ff66b4fcff8ded313bd1b45f066b000b726f950b08a8ae3d`; diagnostic only |
| frozen 9000 | SHA-256 `1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8`; `formalCarryForward=never` |
| Formal `0005` static candidate | patch SHA-256 `998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec`；parent pre/post `c6171e…` / `c43447…` |
| Formal-v3 source/recipe lock | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json`；SHA-256 `a32cf21852909be6ed4a3a4b10dec9310533908996dd73e465535e262f61bc53`；static candidate |
| 最近 Windows-target build result | Formal-v3 result lock SHA-256 `4eeffbf1dc505c743871a90510f81854243f48fc9abffc4fd1459079cab3b631`；ZIP SHA-256 `032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5`；已绑定到 FP2 runtime evidence |
| FP2 Attempt 8 | run `fp2-20260827T082048Z-9a7821e264`；report SHA-256 `86f0ae525925809757456c11fec33b5c7a20a4d6fa00d686bda903f75ca1cc53`；immutable Failed at native-DNT harness mapping；launch/Search/MediaDevices/Voices discriminator passed |
| FP2 Attempt 9 | run `fp2-20260827T084257Z-c9d6dcc498`；report SHA-256 `590e90cb20a7c9a1341fb36a03c9a04bf7a0c36b034717fadd830d952d4339a3`；A1/A2/B1 phases passed，immutable Failed at post-sequence storage harness semantics |
| FP2 Attempt 10 | run `fp2-20260827T090954Z-7a85050695`；report SHA-256 `d14bf5f2881ce1c48ec49cf0ba1184b940d61013a462f56218fb1569d873455b`；A1/A2/B1 execution passed，runner 保持 awaiting-main-brain 边界 |
| FP2 Formal-v3 aggregate result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp2-result.json`；SHA-256 `caa5ed4005c3e9c392c76a5d264d3d7d4d30cb741ac675fd27803c7f5fa06fa6`；**Passed on this native Windows host**；`verified:false` |
| FP3 Attempt 7 | run `fp3-20260828T024057905465Z`；report SHA-256 `697a190ff485814a3f310cf3977792698e9ac2aaa2bcbae625bbbf7797acc25d`；required route、出口、Geo、ICE 与 lifecycle 全部通过 |
| FP3 Formal-v3 aggregate result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp3-result.json`；SHA-256 `8a821eca7b9e11716668d6742ac356743b7438ab2b9a7ca8b0d604264be86e62`；**Passed on this native Windows host**；`verified:false` |
| FP4 Attempt 13 | run `fp4-853f5fe2c6ad4238ac76776f3668f163`；report SHA-256 `de69f0083f7babfdfae5d3d1887fbf18e22e711981e2ca26e9f79e89bcc9e6a7`；V5 六项 task、Profile replay、bindings 与 lifecycle 全部通过 |
| FP4 Formal-v3 aggregate result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp4-result.json`；SHA-256 `14c7de3a8a14b8037cf0e16ec7b5dc213294b68050665a57513dea79efd8f2de`；**Passed on this native Windows host**；`verified:false` |
| clean M3-WI input contract | `docs/camoufox-m3-wi-clean-contract.md`；SHA-256 `acdc725dbbb1ccb0c39571cea43f6eb7ef3137429f4f8b256ec764f3be20af74`；Attempts 1–3 immutable Failed，Attempt 4 Passed |
| clean M3-WI Attempt 4 | `artifacts/camoufox-m3-wi-clean-attempt-4/run-report.json`；SHA-256 `edd08b83497e09a73a0a0e29203475f1e9163b20366b2dd7c899aea8634262fe`；native evidence SHA-256 `2f292585a010dbdc3cad35bfcf26b14800bad402ed4a160c5123f41005c972ad`；revision `26ded609bf5bf52882c9ba37496f783ab2b01681`；**Passed on this native Windows host**，`verified:false` |
| Historical RC1 release artifact | `artifacts/release/managed-browser/v0.1.0-rc1`；source revision `6497828aa0643f94fed3ae708734eef6b85f8305`；source dirty `true`；installer SHA-256 `ea1108e7623118df6b45b7ceb570a4481fc3a030c32b64747395551efd7ce7df`；verifier `Managed-browser release verification passed for 1403 files.`；acceptance `Pending`、`verified:false`、`runtimeAcceptance:null`；historical candidate, superseded, never runtime-accepted |
| v0.1.0-rc2 source-bound candidate | `PUBLIC_GITHUB_PRERELEASE`（已被 rc5 取代为当前公开版本）；source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`；installer SHA-256 `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3`；在其固定边界内完成 installed acceptance；外层 Authenticode unsigned，内部 engine CMS signature/public pin 分开 |
| v0.1.0-rc2 Windows Sandbox installed acceptance | QA branch `origin/agent/qa/v0-1-0-rc2-installed-69d6d6`；evidence tip `df1b82fe3ca2f071a4b6676adc92db046558cdd3`；Windows 11 Enterprise `10.0.26100` / AMD64 / pristine disposable Sandbox；`CURRENT_SOURCE_INSTALLED_PRODUCT_ACCEPTED_IN_WINDOWS_SANDBOX`；strict standard-user semantics `NOT_PROVEN`；验收期间的 `MANAGED_STOP_TRANSIENT_NETWORK_POLICY_MESSAGE` 观察项已由后续 source 修复关闭 |
| v0.1.0-rc3 public prerelease | tag `v0.1.0-rc3`（commit `407c501741c1be3444bfa607c7e693ecc7324409`）；`PUBLIC_GITHUB_PRERELEASE`；rc3 release gate 已关闭；strict standard-user 与外层 Authenticode 边界不变 |
| v0.1.0-rc4 historical public prerelease | tag `v0.1.0-rc4`（commit `68e21c3d601e1df3699f1b21431cc23da873e546`）；`PUBLIC_GITHUB_PRERELEASE`；release gate 已关闭 |
| v0.1.0-rc5 current public prerelease | tag `v0.1.0-rc5`（commit `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`）；`PUBLIC_GITHUB_PRERELEASE`；bounded installed report `INCONCLUSIVE — installed files matched; Managed runtime/repair/uninstall flow not completed`，完整旧合同报告仍 `Pending`；release gate 已关闭 |
| Recent performance and Managed development lifecycle | [性能任务处置](qa/performance-architecture-2026-09-27.md)；原生 Windows 独立 LocalAppData 开发路径有限生命周期通过，非 universal performance / installed-candidate claim |
| Codex worktree inherited-DACL limitation | [目录 ACL 因果对照](qa/managed-engine-directory-acl-2026-09-27.md)；504 文件 hash 相同、同一 executable/owner、仅改 DACL 后 60 秒失败转为 5.344 秒建页；非已证实产品 blocker |
| Surface Truth Matrix 审计 | QA branch `origin/agent/qa/fingerprint-surface-52ebde`；commit `96543d74e34089c8d3f0b6199aaa974a2cff9922`；四层能力审计，基线 `0d74290`，零 CONTROL_GAP |
| 传输层自洽性诊断 | QA branch `origin/agent/qa/transport-fingerprint-d96e42`；commit `94a364143f68f270597f86baf9ce735573632aff`；TLS ClientHello / HTTP2 / QUIC v1 观察自洽，`TRANSPORT_COHERENCE_NO_CONTROL_BLOCKER_FOUND` |
| Managed Font Isolation | commit `1b41b812f3d10bdb2dc6543ea4a740a665f8d7c9`；`fontMode=managed` 成为生产预设，Windows 宿主未声明字体遮蔽经 oracle 验证 |
| Window Geometry Coherence | `ACCEPTED_IN_WINDOWS_SANDBOX`；`provision_artifact.py`、`generate_identity.py`、`host_runtime.py` 强制 virtual screen bounds clamp；真实浏览器四场景验收通过，详见 `docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md` |
| WebGPU Boundary 结论 | 本任务裁定；`WEBGPU_NO_LEAK_OBSERVED_BUT_POLICY_BOUNDARY_STILL_OPEN`；无硬件泄露观察，保持开启策略边界 |
| Evidence Coverage Closure II | `ACCEPTED_IN_WINDOWS_SANDBOX`；WebGL2、Accept-Encoding、Managed 字体遮蔽与 Fresh Recheck Sentinel 对账真实运行全量通过，详见 `docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md` |
| Packaged-Host Windows Sandbox Acceptance | `CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`；基于当前 baseline 自包含 Dev Engine Package 在健康 Windows Sandbox 中完成 4 阶段真实浏览器验收矩阵与进程清理，详见 `docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md` |
| FP1-R1 carry-forward result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp1-r1-result.json`；SHA-256 `a4f0ef539ee09925d7715e6bfea1cbd74dde74ff62dac26f619ab56dbae5b197`；report `f05f2fd…`；claim `b1a37e60…`；this native Windows host only |
| FP2 attempt 1 result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-r1-result.json`；SHA-256 `bd91dff1a324cfdd3e6241aa5a61a59e0b64597e8ca173ff8d6a64374d309a24`；immutable Inconclusive |
| retired Formal-v1 FP2 aggregate | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-result.json`；SHA-256 `540472a6f33f2426fc66a6a1d0ea722356b259a8e315b19b10b445d813f045db`；attempt 2 immutable Failed |
| final Voices design checkpoint | `594d16700c7d8f5d169eaac6cf6fd62d5a12df49` |

原始 machine evidence 曾保留在本地 `artifacts/` 目录；该目录不在 Git 内
（gitignored），本地已清理，上表指向 `artifacts/…` 的锚点无法从仓库解析，
durable 副本只有 `apps/camoufox-host/lock/` 下的 locks/results 与 Git 历史中的
revision；状态页不再复制每个文件的 SHA/size 表。

## 历史索引

只在调查对应事实时读取：

- [FP1 historical contract/evidence](camoufox-fp1-deterministic-artifact-projection-task.md)
- [FP2 generation history](camoufox-fp2-cross-realm-consistency-task.md)
- [R1-diag durable builder history](camoufox-r1-diag-durable-builder-evidence-contract.md)
- [actual-9000 / phase-anchor execution history](camoufox-fp2-r1-diagnostic-execution-task.md)
- [M3-WI failed investigation](camoufox-m3-wi-windows-task.md)

## 更新规则

状态页在 Gate 变化时**替换**当前 Gate、下一任务和必要证据索引，不再追加完整历史章节。
历史准确性由 Git commit、immutable claim/result、lock/manifest 和上述历史文档承担。

一次普通状态更新不要求全量回归或重算稳定 artifacts；只检查改动引用和直接相关事实。
浏览器/build/product claim 仍按 [Agent 工作模型](agent-operating-model.md) 的 L2/L3 规则执行。
