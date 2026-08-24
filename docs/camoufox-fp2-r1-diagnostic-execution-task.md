# FP2-R1 Voices Diagnostic Execution Readiness Contract

- 状态：**Accepted：execution-package-ready-no-browser**
- 日期：2026-08-24
- 起始代码 checkpoint：`0efe8aa57bf2e83fc5f3552c1ecb0d1f8e645b72`
- Browser Gate：**CLOSED**

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

它不表示 FP2-R1、V1–V4、Voices remediation、GPC runtime 或 Formal R1 通过。下一步只
能是另一次明确授权的一次 bounded top-only browser diagnostic run；运行结果仍可为
inconclusive。
