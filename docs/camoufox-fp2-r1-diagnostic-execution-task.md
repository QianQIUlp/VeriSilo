# FP2-R1 Voices Diagnostic Execution Readiness Contract

- 状态：**Accepted：execution-package-ready-no-browser**
- 日期：2026-08-24
- 起始代码 checkpoint：`0efe8aa57bf2e83fc5f3552c1ecb0d1f8e645b72`
- Browser Gate：**v1 CONSUMED；executor-recovery v2 conditional on clean/pushed checkpoint**

## 1. 目标与边界

本任务只把已经完成 provenance closure 的 R1-diag Windows binary 变成一次可执行、
有界、fail-closed 的 Voices 因果诊断包。它不执行浏览器，不接受 FP2-R1，不证明
Voices 已修复，不验证 GPC runtime，也不创建 Formal R1 candidate 或 `0005`。

下一次浏览器执行必须由主脑单独授权。授权前默认命令只能验证 readiness 或物化并重读
浏览器树；`--execute-browser-diagnostic` 是唯一浏览器入口。

## 2. 精确输入绑定

| 对象 | 绑定 |
| --- | --- |
| diagnostic build run | `r1diag-engine-20260823t1542z` |
| closure source lock | SHA-256 `6b93a2425cbf8c54c542a8d134a051d51be39f32239150d2f7ae515b2f00186b`；`50158` bytes |
| build-input source lock | SHA-256 `02b7de1a0e6d87cd4a08be1c7bffe3b5979be3f4f9ffcf85c951ca80720441a7` |
| Windows ZIP | SHA-256 `241b656945260963ff66b4fcff8ded313bd1b45f066b000b726f950b08a8ae3d`；`493471385` bytes |
| `camoufox.exe` | SHA-256 `9fef022fea062f22e4916e4c125c913931eefe8afe522d3930089ed3393dbfd5` |
| bundle metadata | BuildID `20260811045234`；SourceStamp `e39c605adc0fc049a165d7fe4a3f6517b761edf7` |
| extraction tree | SHA-256 `d65b168849b4df8f1fde52e8627e834e3d0b85b4c4e7befb5b179a8440211e06`；514 entries / 503 files / `982403785` bytes |
| historical Artifact bridge | `identity-win-canvas-v1-a.json` SHA-256 `e273ca6376c9f4984a3bd7d78885771d3d5c712881da49691f67c2a44a8684bb` |
| historical native reference | Gen5 `raw-realms.json` SHA-256 `ebf10af98b0074b1a48ba0da1bc45788e7cb410bbf9e94714529c84cfc13d9c8` |
| native supervisor | observed host-local support asset SHA-256 `d12204d76ecebed681f601a95e47f29a75bc67a879b6106fb3c1f38579054a98`；`185856` bytes；不宣称独立 build provenance |
| capture runtime | CPython `3.12.13`；Playwright `1.60.0`；`pw:browser` bridge receipts 在 readiness/claim 中逐文件绑定 |

9000 SHA-256 保持
`1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8`，首行仍为
`# VERISILO-DIAGNOSTIC-MARKER: v1`，`diagnosticOnly = true`、
`formalCarryForward = never`。本任务不修改任何 patch bytes 或 source lock。

## 3. 主脑对旧判别表的 fail-closed 裁决

Implementation Contract §2.2/§2.4 描述的是理想 E1–E8 设计；冻结 9000 的实际 bytes
只实现 E1–E7。固定源码审计进一步证明旧表有不可达或过强的正向签名。因此，对
`1542z` 的执行解释由 `actual-9000-amendment-v1` supersede；历史文本不被追改。

冻结 9000 的实际事件只有：

| 事件 | 实际载荷 |
| --- | --- |
| E1 | `proc=P, seq, n|null` |
| E2a/E2b | `proc=P, seq` |
| E3a/E3b | `proc=P, seq, n` / `proc=P, seq` |
| E4 | `proc=P, seq, n` |
| E5 | `proc=P, seq, h` |
| E6 initial/add | `proc=C, seq, n` / `proc=C, seq, h` |
| E7 | `proc=C, seq, ctx, n, cache, first` |

实际事件使用每进程连续 `seq`，没有 timestamp/tid、E4 actorTag/inventory hash，也没有
E8。`P` 与 `C` 的 `seq` 不可互比；`proc=C` 没有 PID。因此执行必须保持单一 top
content producer，并以 C 内部连续序列判定，不能伪造跨进程总时序。

固定源码与精确 pre-seam bytes 给出以下裁决：

- V1 原签名 `E4 < E3b` 为 `source-refuted-as-written`：E4 在记录前调用
  `GetInstance()->GetVoiceCount()`，受管批次在 `GetInstance()` 返回前已完成。
- V2 原签名为 `source-refuted-as-written`：固定 SAPI 路径同步完成枚举和注册后记录
  E2b，随后才进入 E1/E3；正常的 `E2b < E3b` 不是异步交错证据。
- V3 没有 E8，且固定 `GetVoices` 在重建 `mVoiceCache` 前已形成返回结果；无 E6 的
  计数变化只能判为 `inconclusive / unexplained content-local transition`，不得称
  cache causal。
- V4 在 config delivery 独立成立时，E1 为 null/缺失或 E3 缺失只能形成
  `source-seam suspicion`，随后转 source diff audit；不得称 hunk drift 已证实。

## 4. 实际可执行判别：T1

T1 是对实际 9000 的补偿签名，不重命名成 V1：

```text
同一 top window、同一 speechSynthesis object
E7 first: 5 个精确已知 native URI hash
  → 同一 C seq 中出现 E6 initial 或精确 managed add delivery
E7 second: 同一 5 native + 精确 53 managed URI hash
同时 E4 parent snapshot = 58
```

只有上述集合、计数、E4 与投递闭包全部成立，才允许输出
`T1_contentMirrorIncrementalDelivery = supported`。仅有 `5 → 58` 计数、未知 voice、
parent snapshot 矛盾或跨 producer 推断均不得形成 T1 正向结论。

一次执行可以保持 `inconclusive`；`not-observed` 不等于 refuted，且不允许 exhaustive
exclusion claim。

## 5. 有界执行形状

唯一计划是 top-only：加载本机 `127.0.0.1` 静态页，在同一个原生
`speechSynthesis` object 上立即查询一次，固定等待 3 秒，再查询一次并关闭。不得添加
iframe、Worker、Playwright smoke、FP1-R1、FP2-R1 或 FP3 流程。

Playwright 1.60 默认吞入 browser stderr。执行包复用其既有 `pw:browser` debug 通道，
强制 `DEBUG=pw:browser`、`DEBUG_COLORS=0`、`DEBUG_HIDE_DATE=1` 并移除
`DEBUG_FILE`，只接受：

```text
pw:browser [pid=<positive integer>][err] VSIDIAG <strict JSON>
```

固定 Python transport、Node driver 和 Windows supervisor 源码证明该 wrapper 连接到
Camoufox 继承的 stderr；固定 FF152 的 Windows process/sandbox source 也把父进程
stdout/stderr handle 传给 content process。所有实际 VSIDIAG 必须来自同一 wrapper PID；raw/fake wrapper、
unknown event、字段漂移、P/C 序列 gap/duplicate 或 OVERFLOW 均 fail-closed。Python
与 Node/browser 日志没有可用的跨 producer 总序，因此分类器只使用各自 P/C 连续 seq，
不按日志行位置圈定或排序 E7。

## 6. 资产、claim 与生命周期隔离

- ZIP 只物化到 Git ignored 的 diagnostic runtime root；临时树完整重读通过后才原子改名。
- 每次 launch 前重算 closure lock、tree manifest 和 503-file live tree；reparse、额外、
  缺失或变化文件全部拒绝。
- browser child 只能消费 parent 创建的单次 token authorization；authorization 同时绑定
  run-id、claim bytes、runner、runtime interpreter、Playwright bridge、native supervisor
  bytes 和全部 child paths，消费记录使用 exclusive create。
- claim 在 launch 前写入独立 `artifacts/camoufox-fp2-r1-diag/` namespace；不得复用
  `artifacts/camoufox-fp2/` 或 Formal claim。
- parent watchdog 为 150 秒，launch/close 各自 60 秒；clean close、Job active=0、exit 0
  和目标进程清空是 evidence capture 的必要条件。
- 报告固定 `diagnosticOnly=true`、`formalEligible=false`、`verified=false`，并显式把
  `fp2R1Accepted`、`formalR1`、`voicesFixed`、`gpcRuntimeVerified`、
  `remediationSuccess` 全部写为 false。

## 7. 本 Gate 的接受条件与下一步

readiness closure 只接受：精确输入/树/运行时/log bridge 可重读；actual-9000 parser、
分类器、单次 authorization、claim 隔离和 bounded lifecycle 均由无浏览器回归覆盖；默认
和 materialize 模式的 `browserLaunches=0`；tracked worktree 最终 clean 且已 push。

本 Gate 通过时只允许表述：

> FP2-R1 V1–V4 diagnostic execution readiness closure passed.

它不表示 FP2-R1、V1–V4、Voices remediation、GPC runtime 或 Formal R1 通过。该授权
运行及其结果现记录于下一节；本 readiness Gate 本身不被运行结果追改。

## 8. 2026-08-24 唯一授权运行与离线重裁决

唯一授权 run 为 `fp2-r1-diag-20260824T055549Z-56f7c5fced`，浏览器只启动一次。
child completed、browser/child exit 0、clean close、process tree exited、parent
`processClean=true`；单 transport PID 捕获 125 个事件，P seq `0..63`、C seq `0..60`
连续且无 OVERFLOW。原 claim SHA-256 为
`2b9151e9032ecfda4e1ea29e3dd1d38b368ab734b61239753c4a1c6ff582b34c`。

原 runner report 必须保持 `Failed / config_delivery_unproven`，SHA-256
`40ed7905f3eb313452b55238b9bd515b514f7a7dd4107ac404bc765a4ac94728`。失败发生在
post-processing：runner 错把规范的 `sha256:<64hex>` configured digest 当成裸 64 hex；
browser、9000、config delivery 与 lifecycle 均没有失败。最小修复 commit 为
`1e889bb8aed23852fbe8582462edf38e59e862b6`，tree
`9cb1265f8f4fd69e89a01ca6b63955ccb906a20a`；回归改为接受规范前缀并拒绝裸值。

没有启动第二次浏览器。修复后的 clean/pushed classifier 重读 8 个固定原始 evidence
receipt 与 sidecar，并另写不覆盖原文件的
`verisilo-fp2-r1-diag-offline-readjudication/v1`。receipt SHA-256 为
`2ffe6b55952c1d27704044f384ce6d09dc7663e96a86f6e6e53e8a0178a390ef`，size `6435`；
sidecar raw SHA-256 为
`f734d03e21082b3d741d3cbc2be0edf964efddb1fac7e44864612fbd5c0e0f73`，size `98`。

实际观测为同一 C context 的 `E7(0) → 58 个精确 E6 add + E6 initial(58) → E7(58)`；
第二次 inventory 精确为 5 known native + 53 managed，E4=58。它不满足冻结 T1 的
first=5 条件，因此离线结论是 `inconclusive`，`supported=[]`，T1/V3/V4 均
`not-observed`；V1/V2 仍为 `source-refuted-as-written`。该 0→58 模式可以作为新的
观测事实，但不得事后扩写 T1、冒充 FP2-R1/Voices remediation 成功或据此作者化 `0005`。

## 9. Voices phase-anchor readiness amendment

Gen5 immutable raw evidence 是 `top=5 / same-origin iframe=58 / cross-origin iframe=58`；
top 的 5 个 URI hash 与 actual-9000 C 序列最前面的 5 个 native E6 完全相同，两个
iframe 的 58 个 hash 则与完整 `5 native + 53 managed` 集合完全相同。Gen5 冻结 probe
又按 top → same-origin iframe → cross-origin iframe 串行执行；其 `voiceSnapshot()` 在
初始为空时等待第一次 `voiceschanged`，随后只重查一次。

固定 FF152 source 与 actual-9000 连续序列共同支持以下三相解释：

```text
A0  content registry 已创建、inventory 尚未到达：0
A1  SAPI native 5 已增量到达，首个 voiceschanged 已发布：5
A2  managed 53 与 initial snapshot 已到达：58
```

该解释排除了稳定 realm inventory 与持久 stale cache 作为首选根因，但 Gen5 没有同次
run 的 E6/E7、单调时间或 object anchor；跨 run hash 前缀对齐只能形成高置信 source-
supported inference，不能追认旧 T1，也不能直接作者化 `0005`。因此下一 Gate 收缩为
`voices-phase-anchor-v1`，不修改 9000、不重编 binary：

1. top-only JS 持有一个 `speechSynthesis` object；在首次 `getVoices()` 前注册
   `voiceschanged` listener；
2. 记录 S0 initial；event handler 只递增计数，并仅在首个 event callback 内同步记录 S1；
   固定 3 秒后记录 S2 final；不得 resolve 后再查询、不得轮询、不得创建 iframe/Worker；
3. observer inventory 与每个 E7 一一对应；只使用同一 C 连续 seq，不把 P/C seq 或
   stderr 行序拼成总时序；`ctx` 只作单 producer 一致性约束，不冒充 object ID；
4. exact positive signature 为
   `E7(0) → native E6×5 → event 内 E7(exact native 5) → managed E6×53 →
   initial(58) → final E7(58)`，且 P 侧保持
   `SAPI native 5 → managed 53 → E4(58)`；后续 notification 只计数，不再制造 E7；
5. 首个 event 已在 settled 后触发或没有 event 时都只裁决
   `not-observed / Inconclusive`，不得反证历史 race；event 非 trusted/target 不匹配、未知
   voice、E7 无法映射、序列/transport/config/lifecycle 不闭合则 `Failed`。

新 claim 使用独立
`artifacts/camoufox-fp2-r1-diag/fp2-r1-voices-phase-anchor-v1-one-shot-claim.json`
namespace；旧 claim 与原 run bytes 只读保留供离线重裁决。classifier 即使输出
`supported` 也保持 `0005-remains-closed`，只把证据交回主脑另行裁决，更不表示 FP2-R1、
Voices remediation、GPC runtime 或 Formal R1 通过。本 amendment 与 runner 回归本身均为
no-browser readiness；浏览器执行仍需单独 Gate。

## 10. Voices phase-anchor v1 单次执行失败闭包

主脑授权的唯一 v1 run 为
`fp2-r1-phase-anchor-20260824T102701Z-ee4a02f604`，绑定 clean/pushed HEAD
`2db42c00f296006210daaca69ff21a17ec9546e5` / tree
`ff7684d07d85898d592e661a64311e9314db8607`。one-shot claim SHA-256 为
`c384c41d57c5018297c097dcfb7da62a57505067f51a3905e9107c272aa341f4`，size
`3482` bytes；global claim 与 run copy 逐字节相同。

该 run 启动浏览器一次，但在首个 realm observation 前失败。supervisor 与 Camoufox
parent/Juggler 已启动，随后日志连续记录 8 次
`Failed to launch tab subprocess @SB::LA::SpawnTarget (Error:0)` 与 2 次
`gBrowser never populated`；`host.launch()` 没有返回 launch receipt，60 秒 watchdog
最终输出 `diagnostic_session_watchdog_timeout`。失败命令实际运行身份为
`telecaster\codexsandboxonline`；主脑在同一 checkpoint 的只读对照确认原生非 sandbox
执行身份为 `telecaster\qiu`。因此 **executor-context mismatch at Gecko content-process
creation 是当前最强 bounded diagnosis 与下一判别项**。raw evidence 本身
没有形成 engine/9000、Voices observation 或 classifier failure evidence；是否由 executor
identity 因果触发，仍须未来在冻结的新 lineage 中用非 sandbox 原生执行作直接判别。

本 run 没有 `voice-observation.json`、`vsidiag-timeline.json` 或
`phase-anchor-decision.json`，stderr 中 VSIDIAG 数量为 0；所以它既不是 supported，也不是
valid-but-Inconclusive。child exit 已确认，parent 最终复核 `processClean=true`，当前无目标
进程或 18193 listener，但缺少 clean launch/close receipt，execution verdict 保持
**Failed / no observation**。关键 raw SHA-256：

- `child-stderr.log`：`7147755d6d774b24eb7c4379acc001a800e2d20f22bee7e3577d1e24685a5b51`
  / 8959 bytes；
- `child-result.json`：`e69ab1936f1700baeda65f9fb2e98dcf83a94d38685df499b2dc88da2c863c43`
  / 304 bytes；
- `run-report.json`：`3d6d5b3733e2730d6c41f7d5dfdb1bbff1e14bc7b1920f665bd6e01e9fe605b3`
  / 5814 bytes。

全部现存 JSON/log sidecar 已独立重算匹配，原 claim/run bytes 保持不变。v1 不重试；
`0005`、FP2-R1、Formal R1、FP3 与其他浏览器实验继续关闭。若继续，只能先以新的 clean
checkpoint 冻结独立 executor-recovery claim lineage，并从非 sandbox 原生身份执行；不得
删除或复用本次 v1 claim。

## 11. Executor-recovery v2 no-browser freeze

v1 失败闭包确认当时命令运行于 `telecaster\codexsandboxonline`，但 executor identity 与
Gecko `SpawnTarget` 失败之间仍只是当前最强 bounded diagnosis。为直接判别该项，v2 只改
execution lineage，不改任何 measurement、classifier、binary、9000 或 timeout：

- v1 global claim 与 run copy 必须逐字节相同；run 的 7 个 primary JSON/log 及现有
  sidecar 全部按 §10 的固定 SHA/size 重读，且语义仍为 browser launch 1、watchdog failure、
  no observation / timeline / decision / VSIDIAG；
- 新 claim 为
  `fp2-r1-voices-phase-anchor-v2-executor-recovery-one-shot-claim.json`，schema
  `verisilo-fp2-r1-voices-phase-anchor-executor-recovery-one-shot-claim/v2`，并内嵌完整
  `priorAttempt` receipt；v1 claim 不删除、不改写、不复用；
- parent 在 readiness、run directory 与 claim 创建前，以 Windows
  `GetUserNameExW(NameSamCompatible)` 读取真实 token identity；只接受
  `telecaster\qiu`。child 在消费 authorization、启动浏览器前重算同一 receipt；环境变量、
  `getpass.getuser()` 等可继承值不参与 Gate；
- `PHASE_CONTRACT`、observation/decision schema、top-only JS、listener-before-S0、首 event
  callback 内同步 S1、固定 3 秒 S2 与 exact 5/53/58 classifier 均保持 v1。JS 与 classifier
  source fingerprint 分别固定为 SHA-256
  `93af1e2e68c5fbe1c568abf7435ff8494b19eef054440c9b00b114ce7685a708` 与
  `11286655ca82fdcd3c5f4c81bb5badbefa5796b624b3f6644b3faae702112388`。

v2 只有在本实现与直接回归形成 clean/pushed checkpoint 后才可消费一次。native executor
Gate 未通过时必须在 claim/run 创建前停止，`browserLaunches=0`；通过后无论 supported、
Inconclusive 或 Failed 都保留原始 bytes、禁止自动重试，并返回主脑裁决。classifier 继续
输出 `0005-remains-closed`；FP2-R1、Formal R1 与 FP3 不开放。
