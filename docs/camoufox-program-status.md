# Camoufox Managed Engine 当前状态

- 状态：**当前路由页**
- 更新日期：2026-08-30
- 当前分支：`codex/camoufox-m3-engine-adapter`
- 当前 source candidate：Formal-v3（`0000 → … → 0007`）

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

Standard Silo 长期保留；近期只关闭一个 Camoufox Managed Engine 垂直切片。Profile、
Artifact、Engine、Network 与 Evidence 不合并，`configured`、`applied`、`observed`、
`verified` 与 `unavailable` 不混用。

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

## 当前 Gate

| 能力 | 当前结论 |
| --- | --- |
| Linux M0–M2 standalone Host / Artifact | **Accepted**；仅对应已记录平台，`verified:false` |
| 原生 Windows M2-W | **Accepted**；Profile、Artifact replay 与 Job/process ownership Gate 已关闭 |
| M3-0 EngineAdapter contract slice | **Accepted** at `e96ef3f`；fake Host contract，不是 shipped desktop |
| 历史 M3-WI real Windows desktop/Host | **Failed / Inconclusive**；旧合同不复活，Camoufox Windows Managed 保持 experimental |
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
| production package/signing/UI | **Implementation/build closed** at `804aca6803fcbbbf34c47d84d2f73e03729f3d33`；内部 CMS 签名 package、public pin、production adapter、Managed Silo UI 与 outer-unsigned current-user NSIS 已通过 release checks；production adapter 的 installed runtime launch/close 尚未通过 clean Windows 11 产品验收，结论为 **Pending / Not Run**，`verified:false` |

## 当前未证明的边界

- 当前 Artifact 的 `fontMode=inherit`；宿主字体可见，不声明字体隔离；
- 实际浏览器 DNS 路径、TLS ClientHello、QUIC、跨主机重放与“不可检测”未验证或 unavailable；
- FP3 不证明 Camoufox 原生 Geolocation provider 或 exhaustive native address inventory；
- FP4 只覆盖冻结的 V5 live-site matrix，不声明 universal compatibility；login、payment 与 CAPTCHA 未测试；
- clean M3-WI 的既有 Attempt 4 仍只证明当时的 test-only adapter 路径；它不自动证明新 RC1
  production package/adapter。RC1 已包含受 pin 的内部 CMS signer、签名 Host/runtime/browser package
  与 production adapter，但最终发布制品中的 `packageVerification`、`verifiedAdapter` 和精确 runtime
  bindings 尚未在 clean Windows 11 用户路径中直接验收；
- RC1 installer 与 Desktop 外层明确为 `authenticode=false`；它是本地 unsigned RC，不是公开可信签名发布；
- Formal-v3 runtime observation 只覆盖本机绑定 candidate/Artifacts；Voices 只覆盖 A1、A2、B1
  各自三秒 top-window trace，不是 exhaustive exclusion；desktop Managed Identity 已构建为本地 RC，尚未
  通过最终 Windows 用户验收或作为正式产品 shipped。

## 当前下一任务

### VeriSilo Managed Browser v0.1 RC1 clean Windows 11 acceptance

M3-P1 的实现接缝和产品封装已进入 source revision
`804aca6803fcbbbf34c47d84d2f73e03729f3d33`：schema-v3 package 由单 signer detached CMS SHA-256
签名，Desktop 内嵌 public certificate pin；production `ExternalPackageEngineAdapter`、per-Silo
Artifact/Profile/Engine roots、Managed 创建/重绑 UI、required FixedProxy relay 和 current-user NSIS
均已实现。Rust 全测为 `202 passed / 0 failed / 5 ignored`，前端 check 与全测通过，最终 release verifier
对 `1447` 个文件、SBOM、license evidence、provenance 与 SHA256SUMS 全部通过。发布报告仍保持
`Pending / verified:false / runtimeAcceptance:null`。

当前唯一 Gate 是从精确 unsigned installer
`VeriSilo-Managed-Browser-v0.1.0-rc1-x64-setup.exe` 在 clean Windows 11 x64 标准用户环境完成：Vault、
production package verification/adapter launch、required-proxy Silo A、Direct Silo B、Profile 与会话持久化、
A/B isolation、single-active、proxy fail-closed、四类关闭路径、应用重启、覆盖重装、卸载保留数据和再次安装
重开。当前构建机只从最终 `verisilo.exe` 观察到首次 Vault 页面正常渲染；这不替代上述验收。没有完整
用户路径直接 evidence 前不得把 RC1、`verifiedAdapter` 或整体体验裁决为 Passed。
该 frozen installer 的 SHA-256 为
`1d4decf51b6b86eb8d6355353b4e208f2e877bee6f96d6204d85b558330381a0`；验收只使用这些精确 bytes，
不因重新构建或替换样本改变当前候选。

下一次执行只读取 [RC1 acceptance runbook](acceptance/managed-browser-rc1.md) 和 owning code；仅在发布制品
暴露新的可复现因果失败时修改实现，不重复未变化的 FP1–FP4 历史矩阵，也不创建新的研究 Gate。

## 后续 Gate 顺序

```text
Formal-v3 static source candidate（已闭合）
→ fresh Windows build/provenance（已闭合）
→ native launch discriminator + FP2 A1→A2→B1 qualification（已闭合）
→ FP3-0 configured network identity input（已闭合）
→ FP3-1a local required FixedProxy Host routing seam（已闭合）
→ FP3-1b native Windows required FixedProxy discriminator（已闭合）
→ FP4 ordinary-site compatibility（已闭合）
→ clean M3-WI definition/refreeze（已闭合）
→ clean M3-WI evidence-semantics correction（已闭合）
→ clean M3-WI Attempt 4 native two-cycle qualification（已闭合）
→ M3-P1 production package/signing + production adapter implementation（build/release checks 已闭合；runtime proof 并入 RC1 验收）
→ Managed Silo UI + outer-unsigned self-contained NSIS（build/release checks 已闭合）
→ clean Windows 11 A/B / proxy / persistence / lifecycle / reinstall / uninstall acceptance（当前唯一 Gate；Pending）
```

每一步只验证新增不确定性；复用既有 builder/supervisor，不创建新的 build、retry 或 recovery
框架。

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
| RC1 signed engine package | `artifacts/release/managed-browser/v0.1.0-rc1/engine-package`；manifest `0967dd88729521785a376c72738bfa8c5e7d81a480ab1cf418cea9244318617a`；package tree `d3a6855f987e9c47cbfbe68a4396d42ceba44d065c64b9240b6e52906548d4f5`；browser tree `8434ab9925bf0f7d95cc4ff06fe94b7dcf9963a0691f37638469d68cda58ace2`；Host `2428d79813a0c2e715f5cd81aa3d57825d18e1c26e79c13e8678dbc720970a59`；signer pin `57f3b44cf572571e8b133c6b605b061e0d1c4d9dd75a490b14f658c292bebd93` |
| RC1 local frozen release artifact | 本机构建且 Git-ignored，revision `804aca6803fcbbbf34c47d84d2f73e03729f3d33`；installer `434488813` bytes，SHA-256 `1d4decf51b6b86eb8d6355353b4e208f2e877bee6f96d6204d85b558330381a0`；provenance `sourceDirty=false`、`runnerOs=win32`、`authenticode=false`；release checks Passed，Windows acceptance **Pending / Not Run** |
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
