# VeriSilo Managed Fingerprint FP2 — Cross-Realm Consistency

- 状态：**FP2 generation 1 blocked / generation 2 failed on HTTP harness / generation 3 failed on insufficient historical failure evidence / generation 4 failed formal execution with confirmed probe lifecycle semantic defect / ServiceWorker semantic closure accepted / generation 5 execution package frozen / claim not created**
- task version：generation 1 `fp2-v1`；generation 2 execution package `fp2-v2`；generation 3 execution package `fp2-v3`；generation 4 execution package `fp2-v4`；generation 5 execution package `fp2-v5`
- execution boundary：本文件、独立 `tests/fingerprint-probe/fp2/` bundle、纯比较/runner 与 gitignored FP2 evidence
- `verified`：始终为 `false`

本文只冻结 FP2 的执行合同和本次实际输入。长期产品意图、Camoufox-first 路线、主脑/执行 Agent 分工和当前 Gate 分别以四份权威事实源为准。FP1 的原始 runner `failed`、immutable evidence、offline adjudication、Accepted checkpoint 与 `verified:false` 均保持不变；本文不重新解释 FP1，也不打开 FP3。

## 固定输入

真实运行只允许在当前 close-bound Windows candidate 上执行一次 `A1 → A2 → B1`：

| 输入                              | 固定值                                                                                |
| --------------------------------- | ------------------------------------------------------------------------------------- |
| engine revision                   | `verisilo-camoufox-152.0.4-beta.28-canvas-export-v1-close-bound-v1`                   |
| source commit                     | `e571f6c0b2cea90955b929a4ff04ad54007778fa`                                            |
| archive SHA-256 / size            | `148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94` / `493100709`      |
| `camoufox.exe` SHA-256            | `172f51387bc61e331446883e5499c67611aea5fd81091f68df26b166c9687bf1`                    |
| asset lock SHA-256                | `ce05302d317ec562b096eba52e806ed20302d99d472229640c5eea840d7f98ac`                    |
| browser tree manifest raw SHA-256 | `3a7b9ba83d93e1d40fc30cb4831750d9a125c76db0551459197c74f6b14c86f9`                    |
| browser tree canonical digest     | `42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1`                    |
| browser root                      | `artifacts/camoufox-fp1/windows-candidate-20260818T061456Z-e571f6c/extracted-browser` |

FP2 从当前文件和 sidecar 重新计算 Artifact SHA，不使用旧提示词值：

| Artifact | 路径                                                    | 当前 raw SHA-256                                                   |
| -------- | ------------------------------------------------------- | ------------------------------------------------------------------ |
| A        | `tests/fixtures/camoufox/identity-win-canvas-v1-a.json` | `e273ca6376c9f4984a3bd7d78885771d3d5c712881da49691f67c2a44a8684bb` |
| B        | `tests/fixtures/camoufox/identity-win-canvas-v1-b.json` | `a35b40d7023f93313167e4e9cbd07fe090cad8d20421cf80c78c3ac48b7c7821` |

两份 `resolvedConfig` 都是 47 keys，静态差异精确为：

```text
audio:seed
canvas:seed
fonts
fonts:spacing_seed
navigator.hardwareConcurrency
screen.availHeight
screen.availTop
screen.availWidth
screen.height
screen.width
window.history.length
window.screenX
window.screenY
```

实际 diff 若不等于上述 13 keys，runner 在 claim 前返回 `blocked: artifact_baseline_drift`，不修改 Artifact、不消费 claim。

## Realm 与观察关系

固定 bundle 覆盖：Top Window、same-origin iframe、cross-origin iframe、DedicatedWorker、SharedWorker、ServiceWorker。两个明确 origin 使用同一受控 server：

```text
primary:   http://127.0.0.1:<selected-run-port>
secondary: http://localhost:<selected-run-port>
```

cross-origin iframe 只接受精确的 parent origin、source window 与 session nonce；所有超时、错误消息和 nonce/source/origin 不匹配均 fail closed。每个 realm 发起同类 `GET`/`no-store`/`credentials=omit` fetch，server 返回和记录身份 header；Origin、Referer、Sec-Fetch-*、Accept、Cookie 只作为请求上下文记录，不参与跨 realm 全等判断。

身份 header 的冻结映射是：

```text
HTTP User-Agent        == navigator.userAgent
Accept-Language token  == navigator.language / navigator.languages 的冻结顺序映射
Accept-Encoding        == resolvedConfig["headers.Accept-Encoding"]
DNT / Sec-GPC          == 已配置且引擎实际支持的对应 navigator value
```

Window-family 的 Navigator、locale/timezone/UTC offset、screen/DPR、Canvas、Audio、WebGL/WebGL2、fonts、voices、media 与身份 header 跨同一 Artifact 必须协调；geometry/history 按 relation matrix 只做同一 realm type 的稳定性检查，不把合法 iframe 上下文差异误判为身份漂移。Worker 至少比较 WorkerNavigator、Intl、UTC offset 与 request headers；OffscreenCanvas、PNG blob、Worker WebGL/WebGL2、Worker fonts、DNT/GPC、maxTouchPoints 按运行前 applicability ledger 分类。

Canvas window-family 使用同一绘图场景并分别记录 raw、raw RGBA、decoded PNG pixels、PNG bytes、dataURL、export hash 及 PNG signature/decode/size/mime。Worker 若适用则记录 raw pixels、blob PNG bytes、decode 与 PNG validity；window `toBlob` 与 Worker `convertToBlob` 的关系按 relation matrix 处理，不未经证据假定 bytes 全等。完整 B 的 raw 变化只有在 `fonts` / `fonts:spacing_seed` 与对应 font/width observation 同步变化时才允许；seed-only export 关系不能被改写。

ServiceWorker 必须在 A1 首次注册并 activated，记录 script path/SHA/scope/controller 或受控页面与 MessageChannel evidence；A2 新 Host/同一 `fp2-a` 必须复用 registration 且 evidence 稳定；B1 fresh `fp2-b` 不得继承 A 的 registration、Cookie 或 LocalStorage。

### ServiceWorker activation lifecycle semantic closure

`navigator.serviceWorker.ready` 只建立 registration 存在 non-null active worker 的
barrier；registration 的 `active` worker 可以先处于 `activating`，再通过
`statechange` 到达 `activated`。因此 probe 不得把 `ready` 的 resolve 直接当作
`active.state === "activated"` 的证明。

FP2 ServiceWorker evidence 现在保持同一个 registration、script、scope、controller/
controlled-page 和 MessageChannel 合同，并在既有 `REALM_STAGE_DEADLINE_SECONDS = 15`
的 activation/stage budget 内按以下状态机运行：

```text
ready returned
→ active missing: service_worker_active_missing
→ active.state == activated: continue
→ active.state == activating: install statechange listener, re-read state, await activated
→ activating → redundant: service_worker_redundant
→ deadline exhausted: service_worker_activation_timeout
→ other state: service_worker_unexpected_state:<state>
```

等待只监听已经开始的 `statechange` 转换，并在 listener 安装后再次读取 state 以
关闭 event race；不重新 register、不 sleep、不 retry、不 reload，也不扩大既有
timeout。`activated` 仍不等于当前页面受控，controller 或受控页面检查保持为后续
独立步骤。

本闭包的语义审计为：

```json
{
  "probeSemanticChange": true,
  "measurementContractCorrection": "ready is not an activated barrier",
  "observabilityChange": true,
  "unchanged": [
    "realm surfaces",
    "collector order outside ServiceWorker activation barrier",
    "Canvas scene",
    "HTTP request shape",
    "Artifact mapping",
    "applicability",
    "relation matrix",
    "frozen timeout value",
    "controller/controlled-page contract"
  ]
}
```

## 冻结账本与 timeout

运行前固定并哈希：

- `tests/fingerprint-probe/fp2/probe-bundle-manifest.json`；每个 bundle file 单独 SHA-256；
- `tests/fingerprint-probe/fp2/applicability-ledger.json`；
- `tests/fingerprint-probe/fp2/relation-matrix.json`；
- `apps/camoufox-host/fp2_cross_realm.py`；纯 comparator 与 runner 使用同一精确字节；
- `apps/camoufox-host/test_fp2_cross_realm.py`；
- 当前 A/B raw Artifact、sidecar、candidate lock/tree/archive/executable。

本次只能使用以下已冻结的事件驱动余量，不修改既有 Host close timeout 或 parent watchdog：

```text
browser-side operation deadline: 3 s
< realm stage deadline:           15 s
< session watchdog:               60 s
< existing parent watchdog:       120 s
```

Host 既有 close/context/process-tree budget（10 s / 8 s）保持原值。无法建立该余量时，claim 前返回 `blocked: timeout_budget_unfrozen`。

## 输出边界

one-shot claim 在第一次 A1 前以 `O_EXCL` 原子创建，绑定当前 HEAD/tree、candidate、A/B raw bytes、bundle/ledger/relation/comparator/runner/no-browser hashes、selected port 与 run ID。claim 一旦存在即消费；失败或中断不重试、不换 port、不改 Artifact、不改 probe、不删除 claim。

原始 evidence 只写入 `artifacts/camoufox-fp2/`，报告与 sidecar、applicability/relation/static diff、三次 realm observations、request-header captures、ServiceWorker registration、lifecycle/protocol/stderr、referenced hashes、final offline adjudication 与 byte closure 均绑定 SHA。sanitized evidence 不写 Cookie/LocalStorage 值、Artifact seeds、Vault/token/proxy secret、用户绝对路径或完整 argv/environment；只保留 continuity/isolation 布尔结论与 hash binding。

执行 Agent 的成功字符串只能是 `execution-passed-awaiting-main-brain-gate`；失败或阻塞只能是 `failed` 或 `blocked`。本文件不得由执行 Agent写成 FP2 Accepted、Managed Identity verified 或 FP3 Open。

## Historical Runtime Preflight Closure：generation 1 blocked 后的 generation 2 pre-claim 路径

### Generation 1 frozen outcome

generation 1 的旧 claim 永久保留且不得覆盖：

```text
claim: artifacts/camoufox-fp2/fp2-v1-one-shot-claim.json
claim SHA-256: e77204a09d9dfdbdf7d6c3b00a96114f477fd5b93d01c7fa6a7fd3dd71b28402
run: fp2-20260820T121344Z-470b08fdb9
browser observations: 0
classification: pre-browser-runtime-dependency-block
```

该 claim 的 blocked 状态来自裸 `python` child 缺少 `camoufox`；Host 和 browser 均未
launch。它不是浏览器语义 evidence，也不是可删除或复用的 claim。generation 2 是修复
claim-before-runtime-preflight 合同缺口后的新 execution package，不是换 run-id/port 的 retry。

### Claim 前硬 Gate（generation 2 historical package）

任何 generation 2 claim 创建以前，runner 必须按以下顺序完成：

```text
candidate / Artifact / probe byte closure
→ no-browser static tests
→ exact interpreter closure
→ exact child-path runtime preflight
→ synthetic success/failed/blocked report finalization
→ process / port / preflight-lock clean
→ only then O_EXCL create fp2-v2 claim
→ explicit browser execution A1 → A2 → B1
```

固定 child runtime 为：

```text
relative interpreter: apps/camoufox-host/.venv/Scripts/python.exe
implementation: CPython
version: 3.12.13
camoufox: 0.5.4
playwright: 1.60.0
browserforge: 1.2.4
```

Preflight 使用与真实 session child 相同的 interpreter、repository cwd、`PYTHONPATH`、
`PYTHONUNBUFFERED` environment construction 和 runner script。它只导入 Host/FP2/runtime
依赖，解析 `AsyncNewBrowser` 及其实际 launch helpers，并在
`AsyncNewBrowser(playwright, from_options=opts, persistent_context=True)` 前停止；不打开
candidate、不创建 Profile、不创建 lock、不监听 loopback port、不消费 claim。

Preflight receipt 必须绑定 interpreter SHA、dependency closure SHA、child invocation SHA、
runner SHA、selected port、previous blocked claim，并明确 `browserLaunchCalled:false`、
`browserProcessCreated:false`、`profileCreated:false`、`lockFilesCreated:false`。

Runtime preflight 是 deterministic 的 claim 前环境完整性检查，可以在 claim 创建前重复执行，
包括一次独立 closure 和正式 runner 内部的一次 pre-claim closure。它不创建 browser process、
不产生 realm observation、不消费 one-shot，因此不属于 browser evidence selection，也不构成
generation 3 或 retry。preflight 失败必须在 claim 创建前返回 `blocked`。

目标进程 clean check 优先使用现有 Windows `tasklist.exe` backend；如果该 backend 返回
Access Denied 或其他不可用结果，runner 使用独立的 PowerShell `Get-Process` backend 查询
`camoufox.exe` 与 `verisilo-camoufox-supervisor.exe`。backend 不可用不能解释为空；只有
所有允许 backend 都无法证明目标进程状态时，才返回 `blocked: process_cleanliness_unverifiable`。

### Generation 2 claim boundary（historical）

generation 2 claim 使用独立路径：

```text
artifacts/camoufox-fp2/fp2-v2-one-shot-claim.json
```

它必须显式引用 generation 1 blocked claim，并绑定新的 implementation commit/tree、runner
SHA、no-browser test SHA、runtime preflight receipt SHA、runtime dependency closure SHA 和
child invocation SHA；candidate archive、executable、asset lock、browser tree、Artifact A/B、
probe bundle、applicability ledger 与 relation matrix 继续使用完全相同的冻结字节。

默认 runner invocation 只执行 Runtime Preflight Closure。只有显式提供
`--execute-browser-matrix`，且所有 claim 前 Gate 已通过时，才允许创建 generation 2 claim
并进入一次 `A1 → A2 → B1`。任何 child runtime dependency、interpreter、spawn-boundary、
finalization、process、port 或 lock failure 都必须在 claim 前返回 `blocked`；claim creation
后 runtime binding 改变则 fail closed，不能偷偷切换 interpreter。

`ensure_sanitized(report, label)` 是 finalization 的固定调用形式。success、failed、blocked
三种 synthetic report 都必须能写 report sidecar、offline adjudication sidecar 和 byte
closure；该 closure 不产生浏览器 observation，也不改变 FP1/FP3/M3-WI/Standard 状态。

### Runtime Preflight Closure result（generation 2 historical checkpoint）

上一版 Runtime Preflight Closure 已在 clean implementation checkpoint 上通过；该段记录的是
generation 2 pre-claim checkpoint，不是当前 Gate。随后 generation 2 正式 claim 被消费并在
浏览器启动后因 HTTP evidence handler 失败而停止，详见下方冻结记录。

## Generation-2 frozen formal failure

generation 2 是真实 formal execution，claim 和 evidence 永久保留，不得覆盖或重跑：

```text
run: fp2-20260821T053550Z-9f98de991d
claim: artifacts/camoufox-fp2/fp2-v2-one-shot-claim.json
claim SHA-256: bcf9170cb26e46a35664ebad3cd8b39a2ec93928e597b21b84037e8cc6f22b67
implementation: 359aa14be6c2d3bf5ef912ce1655f089fb7d7b85
browser launched: true
valid realm observations: 0
header capture count: 0
classification: harness-http-capture-failure
```

失败发生在实际 loopback HTTP request handler 调用 evidence capture API 时：handler 的
`self.command` 被错误地从 `FP2HTTPServer` 读取，触发 `AttributeError`，随后 A1 realm
probe 以 `ProtocolError` 结束。该 evidence 证明的是 generation-2 measurement harness
无法完成采集，不是 Managed Engine 的跨-realm capability verdict；FP2 capability 仍未裁决，
所有结果继续为 `verified:false`。

## Historical Generation-3 Harness Closure (pre-run checkpoint)

generation 3 只修复上述已定位的 harness seam，并新增真实无浏览器 loopback 回归：

```text
request handler owns: method / path / request headers
FP2HTTPServer owns: evidence capture storage
```

回归必须经过真实 `FP2HTTPServer`、标准库 loopback HTTP request 和实际 request-handler
class，验证 method/path/realm/header capture、malformed metadata fail-closed、sanitization
以及 shutdown 后端口释放。不得通过直接调用 capture method 替代该 seam。

generation-3 claim 必须显式引用 generation 1 的 blocked claim 和 generation 2 的 failed
harness claim；两份旧 claim、report 和 referenced evidence 保持逐字节不变。generation-3
只允许改变新的 implementation commit/tree、runner/test hash、fresh runtime-preflight
closure 和新 claim；candidate、Artifact A/B、probe bundle、applicability ledger、relation
matrix 与 static 13-key mapping 必须保持冻结。新 claim 创建前不得启动浏览器；若 Gen3 再次
出现未授权的 harness failure，不自行创建 generation 4。

该段记录的是 generation-3 正式运行前的历史 checkpoint：harness closure 实现与无浏览器回归已完成，新的 immutable
implementation checkpoint 与 fresh runtime closure 是下一道 Gate；generation-3 claim
尚未创建，A1/A2/B1 尚未启动。FP1 仍为 `Accepted / verified:false`，FP3 仍为 `Closed`。

## Generation-3 Failure Observability Closure

The immutable generation-3 formal run is preserved as a failed execution:

```text
run: fp2-20260821T055448Z-8f6b69c851
claim SHA-256: 0cf358045f86257af3126eb27b8f21f8df254984ee8c1fe91f6f79bbe44e09c7
implementation: daf42584ad1bae670aa9c25fcb67650bafd20590
browser launched: true
valid realm observations: 0
header captures: 0
execution classification: failed
root-cause adjudication: blocked / gen3_failure_evidence_insufficient
```

The preserved evidence establishes Python bootstrap, Host hello, browser context,
page creation, navigation and FP2 top-script start. It does not identify the first
operation inside the old `collectWindowRealm()` because the probe saved only
`error.name`; the old claim, report, sidecar, adjudication and byte closure remain
byte-preserved and are not reinterpreted.

This closure changes only measurement observability. The new probe failure object is
versioned and carries `realm`, `stage`, `operation`, `errorName`, bounded sanitized
`errorMessage`, `lastSuccessfulStage` and `probeCompleted:false`. Window and Worker
collectors preserve their existing operation order; frame/Worker messages and the
Python child/parent/report/offline-adjudication path retain the same object. No
Canvas scene, HTTP request shape, Artifact mapping, applicability, relation,
timeout, Worker behavior or ServiceWorker behavior is changed.

The machine-readable semantic audit for this closure is:

```json
{
  "probeSemanticChange": false,
  "observabilityChange": true,
  "changed": ["stage markers", "error name/message preservation", "sanitized failure metadata"],
  "unchanged": ["realm surfaces", "operation order", "Canvas scene", "HTTP request shape", "Artifact mapping", "applicability", "comparison rules", "timeouts"]
}
```

The old probe bundle manifest SHA-256 was
`95e27ceb55e687841dd13398b869bc8709d2edef845ca4584cfadd5b3c5370cc`.
The observability-closure manifest SHA-256 is
`b4be8f80d56621b817b351ccb12d51d8b04eeafe6d9bc26d6e03c144799e621c`.
Generation-1/2/3 evidence remains bound to its own historical probe hashes; none is
rewritten. The new manifest is only a future execution-package binding.

No browser was started by this closure and no generation-4 claim was created.
At that historical closure checkpoint, Generation 4 remained closed pending a separate
main-brain decision; FP2 remained `Failed / Not Accepted`, its capability verdict
remained unresolved, and FP3 remained closed.

## Generation-4 execution package freeze

The main brain has authorized generation 4 after accepting the generation-3 failure
observability closure. This no-browser closure aligns the executable runner with that
authorization without changing FP2 measurement semantics:

```text
executionGeneration: 4
taskVersion: fp2-v4
claimPath: artifacts/camoufox-fp2/fp2-v4-one-shot-claim.json
claimSchema: verisilo-camoufox-fp2-one-shot-claim/v4
```

The generation-1, generation-2 and generation-3 claims remain separate immutable
history. Generation-4 claim creation verifies each historical claim's exact SHA-256,
identity and preserved failed/blocked classification before any new claim can be
created. The generation-3 claim and report are also checked as the formal failed run
with zero valid realm observations and zero header captures.

The probe bundle, applicability ledger, relation matrix, candidate, Artifact A/B,
13-key mapping, timeouts and browser engine are unchanged. This package freeze creates
no claim, browser process, profile or realm observation; a fresh runtime preflight
receipt bound to the new runner checkpoint is required before the generation-4 claim
Gate can proceed.

## Generation-4 ServiceWorker lifecycle semantic closure

The immutable generation-4 failure is preserved as a failed formal execution. Its first
structured probe failure was `service_worker_not_activated` in the top-window
`serviceWorkerEvidence` stage. Offline review established that the old probe treated
`navigator.serviceWorker.ready` as an `active.state === "activated"` barrier. That is
too strong: `ready` can resolve once a non-null active worker is assigned while the
worker is still `activating`.

The probe now waits for the existing worker's `statechange` within the unchanged
15-second realm-stage deadline. The wait is event-driven, installs the listener before
a second state read to close the listener-installation race, accepts `activated`, and
fails explicitly for `redundant`, unexpected states, or deadline exhaustion. It does
not re-register, sleep, retry, reload, or change the controller/controlled-page check;
an activated worker and a controlled current page remain separate predicates.

The Host mapping also preserves a structured probe failure without reading an
undeclared `ProtocolError.detail` attribute. This closure changes the ServiceWorker
lifecycle measurement semantics to match the required state machine and preserves the
failure metadata path; it does not change the other realm surfaces, HTTP request shape,
Artifact mapping, applicability ledger, relation matrix or timeout value. The semantic
audit is:

```json
{
  "probeSemanticChange": true,
  "observabilityChange": true,
  "measurementContractCorrection": "ready is not an activated barrier"
}
```

The current probe bundle manifest SHA-256 is
`d69e61c4da482c8cebaed912a6c24b57b73ac0c465a9fafd6a0be8dc974cfb37`. Historical
generation-1/2/3 claims and evidence remain bound to their original probe hashes and
are not rewritten. This no-browser closure created no generation-5 claim, browser
process, profile or realm observation; generation 4 remains failed/not accepted and
the Managed Engine capability remains unresolved pending main-brain adjudication.

## Generation-5 execution package freeze

The main brain has accepted the generation-4 ServiceWorker lifecycle semantic closure
at checkpoint `700f0a62fd67f59b2ffc5f6047c749988fb09ac3` / tree
`8a775660a08dbdb95e2814d77931927e2668aae6` and authorized only the no-browser
construction of a new executable package:

```text
executionGeneration: 5
taskVersion: fp2-v5
reportSchema: verisilo-camoufox-fp2-cross-realm-run/v5
claimPath: artifacts/camoufox-fp2/fp2-v5-one-shot-claim.json
claimSchema: verisilo-camoufox-fp2-one-shot-claim/v5
```

Generation-4 execution stays permanently `Failed`; the original `service_worker_not_activated`
observation and all generation-1/2/3/4 claims, reports and evidence stay byte-preserved.
What changed is only the main-brain root-cause adjudication recorded in lineage: the probe
incorrectly treated `navigator.serviceWorker.ready` as proof that `active.state` was already
`activated`. Generation 4 therefore did not adjudicate Managed Engine ServiceWorker capability.

### Generation-5 fail-closed lineage bindings

Before any generation-5 claim can be created, the runner must verify each item and embed it
in the claim, report and preflight receipt lineage; any drift fails closed before the claim:

```text
generation-1 claim e77204a09d9dfdbdf7d6c3b00a96114f477fd5b93d01c7fa6a7fd3dd71b28402
generation-2 claim bcf9170cb26e46a35664ebad3cd8b39a2ec93928e597b21b84037e8cc6f22b67
generation-3 claim 0cf358045f86257af3126eb27b8f21f8df254984ee8c1fe91f6f79bbe44e09c7
generation-4 claim 44e4ee032c027f80a2470ecfd3a502ab5176789fc9ae1de052f9262e7c979059
generation-4 run    fp2-20260821T072352Z-c9af4cfd7d (status failed, verified:false)
generation-4 report SHA-256 77ad1771c26449bfbddbe63acd4562c8c1d931bc6819278d811c9aedefefed2d
generation-4 corrected root cause: confirmed ServiceWorker probe lifecycle semantic defect
semantic closure commit/tree 700f0a62fd67f59b2ffc5f6047c749988fb09ac3 / 8a775660a08dbdb95e2814d77931927e2668aae6
semantic audit artifacts/camoufox-fp2/fp2-v4-service-worker-lifecycle-semantic-audit.json
audit SHA-256 8626c37a6952009e6cc07044d2d68866e4f34ad8fe540cc26b9e6812bbb46923
probe bundle manifest pinned exactly to d69e61c4da482c8cebaed912a6c24b57b73ac0c465a9fafd6a0be8dc974cfb37
prior runtime evidence (generation-4 closure): preflight receipt 77e244d7…29580a and byte closure d1ae9872…fc2a37
```

The semantic-closure runtime evidence is bound as prior-generation history only. Because the
runner bytes change with this package, that evidence cannot be reused as the generation-5
runtime closure; a fresh runtime preflight bound to the generation-5 runner checkpoint is
required after this package freeze commit.

### Generation-5 claim Gate and auto-authorization

The generation-5 runner keeps the frozen order: git/baseline/semantic-closure ancestry,
candidate and Artifact byte closure, probe manifest exact pin, ledger/relation hashes,
no-browser tests, generation-1..4 claim verification, semantic-closure bindings,
process/port/lock cleanliness, then one deterministic runtime preflight — and only then the
`O_EXCL` creation of `fp2-v5-one-shot-claim.json`. A generation-4 claim on disk does not
block generation 5; an existing generation-5 claim fails closed with
`one_shot_claim_already_exists`; any historical claim hash drift rejects the package.

Per the main-brain authorization rule, once the generation-5 runner is committed, the
worktree is clean, all frozen hashes match, a fresh generation-5 runtime closure has passed,
port 18192 is free, target processes and locks are absent, and no generation-5 claim exists,
browser execution `A1 → A2 → B1` is auto-authorized without returning to this Gate. After
the claim is created there is no code, probe, timeout or package change, no retry and no
generation 6; the run freezes immutable evidence and returns to the main brain regardless
of outcome. This section changes no measurement semantics: realm surfaces, ServiceWorker
activation state machine, HTTP request shape, Artifact mapping, ledger, relation matrix,
candidate and timeout values remain exactly as frozen by the semantic closure.

## Generation-5 frozen formal failure and capability adjudication

The generation-5 one-shot was consumed and the formal run failed:

```text
run: fp2-20260822T065118Z-18fdbee7a8
claim SHA-256: 3ce470074c09d1f949c19fb63da3064ea7bd4d60f58467d47fbd7e27ea04eb70
first runner failure: dnt_mapping_mismatch / A1.top-window.navigator
valid realm observations: first FP2 evidence obtained (raw-realms 111767 B,
realm-observations 135598 B, 6 header captures); ServiceWorker activation path
observed working (script/scope/sha/activated + topController true)
```

The offline semantic adjudication (`fp2-v5-offline-semantic-adjudication.json`,
SHA-256 `586d37b10a736632147207076c9aa0ae5eb0f9e64c7ebd07d9795f7c9ab74b13`)
established, and the main brain accepted:

1. **Real capability gaps** in this candidate: GPC projection plus cross-realm
   coherence (window `false` / worker `true` / header absent against configured
   `true`), and voices projection (configured 53 -> observed 5 host SAPI voices
   with zero intersection). Delivery of the exact resolvedConfig through
   camoufox `launch_options` into the engine boundary is verified, and the
   candidate's own `properties.json` declares all three keys as supported engine
   properties — so the defects are inside this build's consumption/application.
2. **Comparator category error**: the httpHeaders surface carried contextHeaders
   into cross-realm identity equality although the frozen contract excludes
   Origin/Referer/Sec-Fetch-* from it. All 15 Gen5 pairwise flags reduced to
   referer hashes. The comparator now projects only `identityHeaders` +
   `requestPolicy`; `contextHeaders` stays recorded but excluded — a structural
   three-segment split, not a referer special case.
3. **Lifecycle instrumentation gap**: the Gen5 ctx.close exception (~1 ms,
   bounded-close timeout signature excluded) could not be root-caused because
   close evidence was not persisted on the failure path.

The remediation phase closes (2) and (3) in code: close outcomes persist a
bounded path-redacted message next to `exceptionType`, and failure-path teardown
(contextClose/closeOutcome/exit state/closeSeconds) is written into child-result
lifecycle even when validation fails before `host.close()` returns.

DNT product policy decision (main brain): Firefox >= 135 removed user-facing
DNT, so future identity policy stops declaring managed `DNT=1` for such targets
(native/unavailable under a version-aware policy variant, specified separately).
Historical A/B Artifacts remain byte-unchanged; Gen5 remains Failed.

Generation 6 is closed permanently for this candidate; the current candidate is
not eligible for another FP2 browser attempt. Any engine change creates a new
source revision/archive/executable/tree manifest/engine binding and opens a fresh
candidate-scoped lineage (`FP2-R1`). FP2 remains **Failed / Not Accepted** with
all results `verified:false`, and FP3 remains **Closed**.
