# VeriSilo Managed Fingerprint FP2 — Cross-Realm Consistency

- 状态：**FP2 generation 1 blocked / generation 2 pre-claim Gate correction in progress / claim not created**
- task version：generation 1 `fp2-v1`；generation 2 execution package `fp2-v2`
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

## Runtime Preflight Closure：generation 1 blocked 后的 generation 2 pre-claim 路径

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

### Claim 前硬 Gate

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

### Generation 2 claim boundary

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

### Runtime Preflight Closure result

上一版 Runtime Preflight Closure 已在 clean implementation checkpoint 上通过，但 process
enumeration fallback 修正后必须重新形成绑定新 runner SHA 的 closure。generation 1 blocked
claim 保持原字节，generation 2 claim 尚未创建，A1/A2/B1 尚未启动。FP2 主脑 Gate 仍为
`Blocked`，所有 FP2 结果保持 `verified:false`。
