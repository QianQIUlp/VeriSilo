# Camoufox Managed Engine 当前状态

- 状态：**当前路由页**
- 更新日期：2026-09-12
- 当前稳定产品分支：`codex/camoufox-m3-engine-adapter`
- 当前 canonical development source：`origin/baseline/dev`（本地工作引用为 `baseline/dev`，正常时两者精确相等）
- 当前公开版本：**v0.1.0-rc3**（`PUBLIC_GITHUB_PRERELEASE`；tag `v0.1.0-rc3`，rc3 release gate 已关闭）
- `RC3_SOURCE_SHA`：`407c501741c1be3444bfa607c7e693ecc7324409`
- 当前工程阶段：**post-rc3 product development / roadmap reentry**；新任务从 `origin/baseline/dev` 分叉
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
Sandbox 安装验收，以及 v0.1.0-rc3 公开 prerelease，均已达到各自文档边界。Profile、
Artifact、Engine、Network 与 Evidence 不合并，`configured`、`applied`、`observed`、
`verified` 与 `unavailable` 不混用。

rc2→rc3 的 source/package/installed acceptance 主线已经完成：

```text
source stabilization → rc2 build → package verification → packaged runtime
→ pristine Windows Sandbox installed lifecycle → v0.1.0-rc3 public prerelease
→ rc3 release gate closed
```

当前阶段是 **post-rc3 product development**：canonical development source 是
`origin/baseline/dev`，默认方向是 accepted differentiated roadmap，优先增强
identity/execution/network integrity、runtime evidence 与 attribution；只有真实回归、
新产品代码、新 candidate 或新的 release Gate 才重新触发相应 QA/build/acceptance。当前
不创建 FP5，不重跑输入未变化的 FP1–FP4，也不自动开启 rc4。

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
  Formal-v3、FP1–FP4 与 clean M3-WI Attempt 4 已改变当前状态，以“当前 Gate”为准。

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
- Camoufox 仍在独立 Managed Engine 工作树继续调查，不进入这条产品集成链。Profile
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
| Historical local `v0.1.0-rc1` artifact | **Historical candidate / superseded / never runtime-accepted**；source `6497828aa0643f94fed3ae708734eef6b85f8305`, dirty `true`, verifier passed for 1403 files, acceptance `Pending`, `verified:false`, `runtimeAcceptance:null` |
| v0.1.0-rc2 current-source candidate | **PUBLIC_GITHUB_PRERELEASE（已被 rc3 取代为公开版本）**；Current-source installed product accepted in pristine Windows Sandbox；source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`，installer SHA-256 `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3` |
| v0.1.0-rc3 public prerelease | **PUBLIC_GITHUB_PRERELEASE；release gate 已关闭**；tag `v0.1.0-rc3`，source `407c501741c1be3444bfa607c7e693ecc7324409` |
| Current source release readiness | **post-rc3 product development**；rc2 已在 exact Windows Sandbox 完成安装后生命周期验收，rc3 公开 prerelease 已发布且 gate 关闭；strict standard-user install/reinstall/uninstall semantics 仍 `NOT_PROVEN`，外层 Authenticode 仍 unsigned |

## 当前未证明的边界

- 当前 Artifact 的 `fontMode=inherit`；宿主字体可见，不声明字体隔离；
- 实际浏览器 DNS 路径、TLS ClientHello、QUIC、跨主机重放与“不可检测”未验证或 unavailable；
- FP3 不证明 Camoufox 原生 Geolocation provider 或 exhaustive native address inventory；
- FP4 只覆盖冻结的 V5 live-site matrix，不声明 universal compatibility；login、payment 与 CAPTCHA 未测试；
- clean M3-WI 的既有 Attempt 4 仍只证明当时的 test-only adapter 路径；rc2 已另外在 pristine
  Windows Sandbox 直接验收 current-source package/adapter、精确 runtime bindings 和安装后生命周期，
  但这不外推为所有 Windows hardware/edition 或 strict non-admin 用户路径；
- rc2 Sandbox 使用的 `WDAGUtilityAccount` 具有 `IsAdministrator = true`；strict unelevated
  standard-user install/reinstall/uninstall semantics 仍为 `NOT_PROVEN`，这是 future public-release /
  promotion boundary，而不是 installer lifecycle failure；
- rc2 与 rc3 的 Desktop/NSIS 外层均为 `authenticode=false` / unsigned；内部 engine detached CMS
  signature、approved signer/public pin 与外层 Windows Authenticode 是不同边界；rc2 与 rc3 的
  发布状态均为 `PUBLIC_GITHUB_PRERELEASE`；
- Formal-v3 runtime observation 只覆盖 rc2 在本次 Sandbox 中绑定的 candidate/Artifacts；
  Voices 只覆盖 A1、A2、B1 各自三秒 top-window trace，不是 exhaustive exclusion；FP4 及现有安装验收
  仍不声明 universal site compatibility、undetectability、login/payment/CAPTCHA、完整 TLS ClientHello、
  完整 QUIC、exhaustive browser DNS-path 或字体隔离（`fontMode=inherit`）。

## 当前下一任务

### post-rc3 product development（roadmap reentry）

当前 canonical development source 是 `origin/baseline/dev`（本地工作引用为 `baseline/dev`，
正常时两者精确相等），新任务从这里分叉。`c1688d5a392ffa69ae77c246bcb4bb78b083e26f` 是
rc2 的固定 source binding，`407c501741c1be3444bfa607c7e693ecc7324409` 是 v0.1.0-rc3 的
固定 source binding；两者都是历史锚点，不是会随开发继续推进的 canonical ref。

**Fresh Identity Recheck 已进入 source**：运行中的 Managed Identity Silo 现在可以在用户
主动触发「重新检查」时，通过 Host 的 `reobserve_identity` 命令在活动 session 上重新读取
一次 website-visible identity observation，用当前 Resolved Identity Artifact 重新完成
expected-vs-observed reconciliation，产生新的正式 Runtime Identity Evidence（新的
`observedAt`），并保持用户当前页面不被 probe 导航破坏。该能力由 Host 协议测试、
RuntimeManager 绑定/诚实失败测试与 UI 文案测试覆盖；不再作为未来任务书写。

**Create New Identity From This Silo 已进入 source**：Managed Identity Silo 可以把当前
安全可复用的 identity/network 配置作为新建模板，创建新的 Silo、Profile、seed 和
Artifact；不会复制浏览器 session/state 或 Vault 明文凭据。

**Current Session Integrity 已进入 source**：运行中的 Managed Identity Silo 卡片现在
提供「当前会话完整性」结构化摘要，从既有 Runtime Identity / Engine / Network evidence
与 attribution 派生身份、引擎、网络、运行归属四个状态，并如实利用 identity
`observedAt` 与 network `expiresAt` 表达新鲜度。它是派生摘要，不新增证据等级或
verification score；`Matched` 仍只是 expected-vs-observed reconciliation 结果。

下一个 product slice **尚未由用户冻结**。不要自动开始 rc4 或其他未批准方向。也不要
自动重跑输入未变化的 FP1–FP4、创建 FP5。

历史 `artifacts/release/managed-browser/v0.1.0-rc1` 仍保留为 superseded、从未 runtime-accepted
的候选记录；其原始 source、dirty 状态、installer hash、verifier 和 Pending acceptance 不变。

rc2 的 Sandbox 验收与 rc3 公开 prerelease 是各自边界的直接证据；它们不证明 strict
non-admin、所有 Windows hardware、universal site compatibility 或未列明的浏览器/网络能力。
rc4 / release gate 必须由用户通过新指令显式打开。

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
→ v0.1.0-rc3 public prerelease（release gate closed）
→ post-rc3 product development（canonical: origin/baseline/dev）
```

前半段是已经闭合的资格链，中段记录 rc2 的 source-bound candidate、package/runtime 与
pristine Windows Sandbox 安装验收，随后记录 rc3 公开 prerelease 与当前 post-rc3 开发阶段；
rc2 与 rc3 的发布状态均为 `PUBLIC_GITHUB_PRERELEASE`，且都没有 strict standard-user 证明。
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
| v0.1.0-rc2 source-bound candidate | `PUBLIC_GITHUB_PRERELEASE`（已被 rc3 取代为公开版本）；source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`；installer SHA-256 `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3`；current-source locally accepted installed candidate；外层 Authenticode unsigned，内部 engine CMS signature/public pin 分开 |
| v0.1.0-rc2 Windows Sandbox installed acceptance | QA branch `origin/agent/qa/v0-1-0-rc2-installed-69d6d6`；evidence tip `df1b82fe3ca2f071a4b6676adc92db046558cdd3`；Windows 11 Enterprise `10.0.26100` / AMD64 / pristine disposable Sandbox；`CURRENT_SOURCE_INSTALLED_PRODUCT_ACCEPTED_IN_WINDOWS_SANDBOX`；strict standard-user semantics `NOT_PROVEN`；验收期间的 `MANAGED_STOP_TRANSIENT_NETWORK_POLICY_MESSAGE` 观察项已由后续 source 修复关闭 |
| v0.1.0-rc3 public prerelease | tag `v0.1.0-rc3`（commit `407c501741c1be3444bfa607c7e693ecc7324409`）；`PUBLIC_GITHUB_PRERELEASE`；rc3 release gate 已关闭；strict standard-user 与外层 Authenticode 边界不变 |
| FP1-R1 carry-forward result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp1-r1-result.json`；SHA-256 `a4f0ef539ee09925d7715e6bfea1cbd74dde74ff62dac26f619ab56dbae5b197`；report `f05f2fd…`；claim `b1a37e60…`；this native Windows host only |
| FP2 attempt 1 result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-r1-result.json`；SHA-256 `bd91dff1a324cfdd3e6241aa5a61a59e0b64597e8ca173ff8d6a64374d309a24`；immutable Inconclusive |
| retired Formal-v1 FP2 aggregate | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-result.json`；SHA-256 `540472a6f33f2426fc66a6a1d0ea722356b259a8e315b19b10b445d813f045db`；attempt 2 immutable Failed |
| final Voices design checkpoint | `594d16700c7d8f5d169eaac6cf6fd62d5a12df49` |

原始 machine evidence 保留在既有本地 `artifacts/`、source locks、results 和 Git 历史中；
状态页不再复制每个文件的 SHA/size 表。

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
