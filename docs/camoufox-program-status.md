# Camoufox Managed Engine 当前状态

- 状态：**当前路由页**
- 更新日期：2026-08-26
- 当前分支：`codex/camoufox-m3-engine-adapter`
- Formal build 输入 checkpoint：`6acae1eca3c8b5ff2126da2d0f63ef003173487f`

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
| Formal source + Windows-target build/provenance | **Passed build closure**；`compiled-not-runtime-verified` |
| Formal R1 runtime / FP1-R1 | **FP1-R1 Passed on this native Windows host**；`verified:false`，不是 Formal R1 pass |
| FP2 / FP3 | **FP2 Active**；attempt 1 永久 Inconclusive；attempt 2 在完整 A1 observation 上 Failed，直接暴露 DNT/GPC/DPR 合同不一致；FP3 未开放 |
| production package/signing/UI | **未开放** |

## 当前未证明的边界

- 当前 Artifact 的 `fontMode=inherit`；宿主字体可见，不声明字体隔离；
- TLS ClientHello、QUIC、跨主机重放与“不可检测”未验证或 unavailable；
- 没有受信 signer、签名 Host package、installer 或 production runtime；
- GPC 已被 A1 runtime 直接判为不匹配；Voices 仅有 A1 bounded phase accepted，A2/B1 replay
  尚未执行；desktop Managed Identity 尚未 shipped。

## 已关闭的当前诊断结论

唯一有效 phase-anchor run：

```text
fp2-r1-phase-anchor-recovery-v2-20260824T112146Z-fee3df2667

same speechSynthesis object:
S0 = 0
→ native VoiceAdded ×5
→ first trusted voiceschanged = exact native 5
→ managed VoiceAdded ×53
→ settled = native 5 + managed 53
```

claim SHA-256：
`47ce7d90230d90654d26e21c9aba5b73634760b154db10d1c3a902e915486e61`。
run clean close、process tree exited；它只支持 native-only first-notification publication
phase，不是 exhaustive exclusion，也不是 Voices fixed、FP2-R1 或 Formal R1 通过。

固定 FF152 source 解释该序列：首个 content actor 的 `SendInit()` 懒触发 parent registry；
SAPI 先同步注册/通知 native voices，随后才注入 managed voices。根因冻结为：
**actor 已可接收增量时，parent canonical managed state 尚未形成。**

## 最近关闭的静态 Gate

`0005-verisilo-voices-final.patch` 是单文件 constructor guard；Formal series 精确为
`0000 → 0001 → 0002 → 0003 → 0003a → 0004 → 0005`，Formal 路径拒绝 9000。

- patch SHA-256：`998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec`；
- `SpeechSynthesisParent.cpp` preimage：
  `c6171e3689fab1789c459b924c7420786d2efed0caf2741747b910e0a3dcd61f`；
- postimage：`c43447ff66ad5b03b21a9c76d0202c23a699904868a282f2d53e63e01227093e`；
- fresh exact tree 已完成 `--fuzz=0` apply/reverse/apply；
- Artifact v4 managed/native 闭合 schema、确定性派生与 pre-launch strict rejection 已实现；
  历史 v3 Artifact 仍按 v3 schema 读取，不作为 v4 Formal 输入。

该 authoring closure 只证明 source candidate/policy；后续 Windows-target build 只增加
compiled/provenance evidence，仍不表示 Voices fixed、FP2-R1、Formal R1 或 runtime verified。

## 当前下一任务

### FP2 product remediation

要回答的唯一问题：

> FF152 可控边界、Artifact policy 与 runtime 投影能否重新对齐，使 fresh candidate/Artifacts
> 可以进入一次完整的 A1→A2→B1 FP2 资格判定？

attempt 1 永久保留为 **Inconclusive**。其最小 remediation 已证明有效：attempt 2 的
MediaDevices readiness 首次即得到 `1/1/0`，完整六 realm/六 header A1 observation 已形成，
且 A1 top-window Voices 三秒 phase 为 `empty* → exact managed53*`、即时 replay 仍 exact53。

attempt 2 因完整 observation 与绑定 Artifact 冲突而永久判为 **Failed**，不是新的 recovery
Gate：DNT 配置 `"1"` 而 Window 为 `"unspecified"`、六请求无 DNT；GPC 配置 `true` 而六
realm 均为 `false`、六请求无 Sec-GPC；Artifact DPR 声明 `1` 而三个 Window 均为 `1.5`。
后续离线审查同时关闭了会隐藏 Worker GPC、raw privacy value 与 DPR mismatch 的三个
comparator 缺口，69 个 focused no-browser tests 通过。

当前最短完整修复只有三个 owning seam：

1. 将唯一 GPC projection 从 profile prefs 初始化前移到 `FinishInitializingUserPrefs()` 后、
   首个 window/network channel 前；不增加新 pref 或 fallback；
2. 按已接受 FF>=135 合同生成 fresh versioned DNT-native Artifacts，并补齐
   `gpcPolicy ∈ {native, managed-opt-out}` strict validation；历史 A/B 不重写，也不加 Gecko DNT patch；
3. DPR 按既有 host-bound 边界从 managed stable 声明移出，除非先直接证明真实 engine control。

完成 focused source/policy validation 后只 rebuild 一次，并把必要 carry-forward 与 fresh FP2
attempt 作为本 Gate 内部工作连续完成；attempt 编号只属于 evidence lineage。

## 后续 Gate 顺序

```text
Formal source lock + Windows build/provenance（已闭合）
→ FP1-R1 rebuilt-engine carry-forward（已闭合）
→ FP2 product remediation + fresh qualification（当前；attempt 1 Inconclusive，attempt 2 Failed）
→ FP3
```

每一步只验证新增不确定性。历史 diagnostic builder、9000 和 ultratone scratch 流程不复制到
Formal 路线，除非实际 build evidence 证明某个 owning recipe seam 必须复用。

## 关键证据索引

| 对象 | 当前锚点 |
| --- | --- |
| upstream Camoufox | tag `v152.0.4-beta.28`；commit `0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d`；tree `1435d544d9b61dee7fcf74cf92462952ca43d38e` |
| Firefox source | `799102676` bytes；SHA-512 `0c5662aba8fb897902af95dbb2fd988b196d9cf9ae8b987ae89e0a6492ac753b8d4b8bb7b3274909c2eb200ab098df356e23cd6084556467f55e69127317f39a` |
| R1-diag closure lock | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json`；SHA-256 `6b93a2425cbf8c54c542a8d134a051d51be39f32239150d2f7ae515b2f00186b` |
| diagnostic ZIP | SHA-256 `241b656945260963ff66b4fcff8ded313bd1b45f066b000b726f950b08a8ae3d`; diagnostic only |
| frozen 9000 | SHA-256 `1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8`; `formalCarryForward=never` |
| Formal `0005` static candidate | patch SHA-256 `998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec`；parent pre/post `c6171e…` / `c43447…` |
| Formal R1 source/recipe lock | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-source.json`；SHA-256 `a614f58d32adf7e8c5e787478aa4fbbfd8d28caa97dd151571df8e3b2819455c`；frozen build input |
| Formal Windows-target build result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-build-result.json`；run `r1formal-engine-20260825t060544z`；ZIP SHA-256 `a81649c538a101dce106e42f13f11dbdb08cbc0e8a1c9af6b497719a392a6cdc`；compiled only |
| FP1-R1 carry-forward result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp1-r1-result.json`；SHA-256 `a4f0ef539ee09925d7715e6bfea1cbd74dde74ff62dac26f619ab56dbae5b197`；report `f05f2fd…`；claim `b1a37e60…`；this native Windows host only |
| FP2 attempt 1 result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-r1-result.json`；SHA-256 `bd91dff1a324cfdd3e6241aa5a61a59e0b64597e8ca173ff8d6a64374d309a24`；immutable Inconclusive |
| FP2 aggregate result | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-result.json`；SHA-256 `540472a6f33f2426fc66a6a1d0ea722356b259a8e315b19b10b445d813f045db`；attempt 2 Failed；report `274cdf14…`；claim `4f3e376f…`；this native Windows host only |
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
