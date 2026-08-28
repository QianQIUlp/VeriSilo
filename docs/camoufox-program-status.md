# Camoufox Managed Engine 当前状态

- 状态：**当前路由页**
- 更新日期：2026-08-28
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

## 当前 Gate

| 能力 | 当前结论 |
| --- | --- |
| Linux M0–M2 standalone Host / Artifact | **Accepted**；仅对应已记录平台，`verified:false` |
| 原生 Windows M2-W | **Accepted**；Profile、Artifact replay 与 Job/process ownership Gate 已关闭 |
| M3-0 EngineAdapter contract slice | **Accepted** at `e96ef3f`；fake Host contract，不是 shipped desktop |
| M3-WI real Windows desktop/Host | **Failed / Inconclusive**；无 production fix，保持 experimental |
| FP1 deterministic Artifact projection | **Accepted by corrected adjudication of immutable A1/A2/B1 evidence**；原 runner verdict 仍 Failed，`verified:false` |
| 历史 FP2 candidate | **Failed / retired**；Generation 6 永久关闭 |
| R1-diag Windows build/provenance | **Passed diagnostic-only closure**；不是 Formal 或 runtime pass |
| actual-9000 diagnostic run | 原 runner Failed，离线裁决 Inconclusive |
| Voices phase-anchor | v1 Failed/no observation；唯一 v2 run 直接支持 A0→A1→A2 |
| Voices `0005` + Artifact v4 policy | **Static authoring closed**；仅为 Formal source candidate 输入，不是 runtime pass |
| Formal source + Windows-target build/provenance | Formal-v3 **Passed build/provenance closure**；精确 runtime tree 已绑定并用于原生 Windows qualification |
| Formal R1 runtime / FP1-R1 | **Formal R1 Passed on this native Windows host**；FP1 carry-forward 与 Formal-v3 FP2 均已闭合，`verified:false` |
| FP2 / FP3 | **FP2 与 FP3 均 Passed on this native Windows host**；FP3 覆盖 exact required route、出口、timezone/locale、Geo、ICE 与 clean lifecycle，`verified:false` |
| FP4 ordinary-site compatibility | **Gate open**；Attempt 3 为 Inconclusive：状态及重启重放 Passed，exact bindings 与 lifecycle 全部 clean；仍无 direct product failure |
| production package/signing/UI | **未开放** |

## 当前未证明的边界

- 当前 Artifact 的 `fontMode=inherit`；宿主字体可见，不声明字体隔离；
- 实际浏览器 DNS 路径、TLS ClientHello、QUIC、跨主机重放与“不可检测”未验证或 unavailable；
- FP3 不证明 Camoufox 原生 Geolocation provider 或 exhaustive native address inventory；
- 没有受信 signer、签名 Host package、installer 或 production runtime；
- Formal-v3 runtime observation 只覆盖本机绑定 candidate/Artifacts；Voices 只覆盖 A1、A2、B1
  各自三秒 top-window trace，不是 exhaustive exclusion；desktop Managed Identity 尚未 shipped。

## 当前下一任务

### FP4-1 native Windows ordinary-site discriminator

FP3-1b 已在冻结 Formal-v3 candidate、Artifact v6、required SOCKS5 route 与 direct negative control
上闭合。原生 Windows Attempt 7 直接观察到 exact route binding、香港代理出口、Artifact
timezone/locale/Geolocation、匹配出口的 ICE `srflx` address 与 clean lifecycle。Geo 的应用边界是
managed Host 以 Artifact 坐标设置 Playwright persistent context；不声明 Camoufox 原生 Geo provider。
合同见 [FP3 network identity contract](camoufox-fp3-network-identity-contract.md)。

FP4 已按产品兼容性 go/no-go 重冻结 `fp4-ordinary-sites-v1`：文档/导航、复杂 JavaScript、
交互图形、音视频、表单/状态五类普通站点任务；每类一个 selected primary 和一个预声明
fallback。2026-08-28 的一次 SOCKS5 availability preflight 选择了全部 primary。native attempt
不得重试或在运行时换站，并须用同一临时 Profile 完成 `0 -> 1`、clean close、`1 -> 2` 重启
状态重放及最终 Host/process tree/Job clean close。合同见
[FP4 ordinary-site compatibility contract](camoufox-fp4-ordinary-site-compatibility-contract.md)。

Attempt 1 的 pre-launch sidecar Failed 与 Attempt 2 的三项 harness Inconclusive 保持 immutable，
均不自动关闭工程 Gate。Attempt 3 ([report](../artifacts/camoufox-fp4-attempt-3/run-report.json)，
SHA-256 `ae8d22960c2a09bad3e5c8a183aeb8f46ce0d244c44e078ac0ee9011cc7b52a8`) 再次直接证明 exact
Formal/Artifact/route/boot 绑定、六张截图、两次 clean close、Job/process tree 清零、临时运行根
移除与跨重启 Profile state；无 crash/page error/direct product failure。文档任务暴露 Playwright
键名应为 `Alt+ArrowLeft/Alt+ArrowRight`；OSM 已完成搜索、pan、zoom 和 layer panel，但 hidden
CyclOSM radio 不能由 `.check()` 模拟用户选择，改为点击关联 label。GitHub 弹层本轮未出现，媒体
本轮 pause/seek wait 未闭合；两者在 Attempt 2 分别到达 label selection 与 Passed，仍按外部/时序
歧义处理，不增加 retry/delay。当前下一步是在两个确定修复的 checkpoint 上生成新的不可变
Attempt 4；不声明 compatibility pass。

## 后续 Gate 顺序

```text
Formal-v3 static source candidate（已闭合）
→ fresh Windows build/provenance（已闭合）
→ native launch discriminator + FP2 A1→A2→B1 qualification（已闭合）
→ FP3-0 configured network identity input（已闭合）
→ FP3-1a local required FixedProxy Host routing seam（已闭合）
→ FP3-1b native Windows required FixedProxy discriminator（已闭合）
→ FP4 ordinary-site compatibility（合同已冻结；native discriminator 下一步）
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
| FP4 latest attempt | Attempt 3 `artifacts/camoufox-fp4-attempt-3/run-report.json`；SHA-256 `ae8d22960c2a09bad3e5c8a183aeb8f46ce0d244c44e078ac0ee9011cc7b52a8`；Inconclusive，无 direct product failure；bindings/lifecycle clean，`verified:false` |
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
