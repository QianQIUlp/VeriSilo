# Camoufox Managed Engine 当前状态

- 状态：**当前路由页**
- 更新日期：2026-08-26
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
| Formal source + Windows-target build/provenance | Formal-v2 **Passed build closure**，但其 native launch qualification 未形成 context；Formal-v3 static source candidate 已闭合、待 build |
| Formal R1 runtime / FP1-R1 | **FP1-R1 Passed on this native Windows host**；`verified:false`，不是 Formal R1 pass |
| FP2 / FP3 | **FP2 Active**；既有 attempts 保持不可变；Artifact v5/GPC/DNT/DPR remediation 已静态闭合，当前只关闭 Formal-v3 build→native qualification；FP3 未开放 |
| production package/signing/UI | **未开放** |

## 当前未证明的边界

- 当前 Artifact 的 `fontMode=inherit`；宿主字体可见，不声明字体隔离；
- TLS ClientHello、QUIC、跨主机重放与“不可检测”未验证或 unavailable；
- 没有受信 signer、签名 Host package、installer 或 production runtime；
- Artifact v5 与 GPC/DNT/DPR seam 仅有静态证据；Formal-v3 尚未 build 或 runtime observed；
  Voices 只有既有 A1 bounded phase accepted，完整 A1→A2→B1 尚未通过；desktop Managed
  Identity 尚未 shipped。

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

Formal-v3 只在冻结 Formal-v2 后追加单文件、纯删除的
`0007-verisilo-ff152-search-schema-repair.patch`，移除 FF152 Rust selector 不接受的旧 Search
schema injection，恢复 Firefox 既有 Remote Settings→packaged local dump 路径；不接管 Search
schema/default-engine 语义。Formal series 精确为
`0000 → 0001 → 0002 → 0003 → 0003a → 0004 → 0005 → 0006 → 0007`，拒绝 9000。

- `0007` SHA-256：`3902cc7187362a306954eb7b18cedb06f74c454d26cc543c28c1fef069a054bb`；
- Search seam pre/post：`ca843d9379f8cf4b5ed04e3da35fa7ace2cbbe6f2ec5a652afea09f8642ffff3`
  / `e3d5351945fc5f4f0866c55021d969f358dc9c59ee405751a308b6ffd10430d9`；
- fresh exact preimage 已完成 GNU patch `--fuzz=0` apply/reverse/apply；focused v3、remediation
  与 frozen v2 tests 通过。

这只关闭 source candidate authoring。Search source defect 已直接证明；它是否造成 launch hang 仍须
由新 build 的 native Windows discriminator 判定。

## 当前下一任务

### FP2 Formal-v3 native qualification

要回答的唯一问题：

> 删除已证明 malformed 的旧 Search schema 后，fresh Formal-v3 是否能稳定返回默认 context，
> 并继续完成一次 A1→A2→B1 FP2 资格判定？

Formal-v2 的 direct/supervised native Windows evidence 已证明 Juggler 启动，同时出现确定性的
`missing field recordType` / no-engine Search 错误；context 未返回。但更早同一 Formal-v2 曾成功
launch，因此 Search defect 是高价值候选 blocker，不是已证明的唯一 launch 根因。

当前最短完整路径：

1. Formal-v3 fresh Windows-target build 已完成并绑定为 compiled-only candidate：ZIP
   `032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5`；
2. 在本机用既有 supervisor/deadline 做 launch discriminator：pipe handles valid、默认 context 在
   60 秒内返回，stderr 不再出现旧 schema/no-engine 错误；
3. discriminator 通过后，在同一 FP2 Gate 内直接完成 A1→A2→B1；若 Search 错误消失但仍 hang，
   只根据新 evidence 继续定位，不扩大 timeout、retry 或另造 recovery Gate。

attempt 编号只属于 immutable evidence lineage，不是工程 Gate。

## 后续 Gate 顺序

```text
Formal-v3 static source candidate（已闭合）
→ fresh Windows build/provenance（已闭合，compiled only）
→ native launch discriminator + FP2 A1→A2→B1 qualification（当前）
→ FP3
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
| 最近 Windows-target build result | Formal-v3 result lock SHA-256 `4eeffbf1dc505c743871a90510f81854243f48fc9abffc4fd1459079cab3b631`；ZIP SHA-256 `032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5`；compiled only，等待 native qualification |
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
