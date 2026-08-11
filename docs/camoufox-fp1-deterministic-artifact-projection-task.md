# Camoufox Managed Fingerprint Core FP1 — Deterministic Artifact Projection

- 状态：**Frozen task contract / Execution failed**
- 冻结日期：2026-08-10
- 当前 Gate：**FP1**
- 前置 checkpoint：M3-0 Accepted at
  `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`

本文冻结 FP1 的唯一目标、实现边界和验收方式。长期产品意图以
[身份平台北极星](identity-platform-north-star.md)为准，Camoufox-first 路线及
FP1–FP4 排序以
[Managed Engine 架构决策](camoufox-managed-engine-decision.md)为准，当前 Gate
和 checkpoint 以[状态页](camoufox-program-status.md)为准。

FP1 不是 M3-WI 的重跑或修复。第二 Host 生命周期调查已按 `inconclusive`
收口，M3-WI 继续为 `failed` / `experimental`；FP1 使用已经成立的 standalone
Host 研究和验证指纹投影，不得把 FP1 结果写成 M3-WI Accepted、桌面产品可用或
Managed Identity 已发布。

## 1. 固定基线与权威边界

本合同基于以下只读实现快照形成：

| 对象                   | 固定值                                          |
| ---------------------- | ----------------------------------------------- |
| source HEAD            | `186484feb935076766beab09595a9270f86f78ef`      |
| source tree            | `e33d6d68586a79796ffb9bcc668392e369dc97c6`      |
| Accepted ancestor      | `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`      |
| Artifact schema        | `verisilo-camoufox-resolved-identity/v3`        |
| Projection schema      | `verisilo-camoufox-stable-signal-projection/v3` |
| Observed digest schema | `verisilo-camoufox-observed-website/v2`         |
| Host protocol          | `verisilo-camoufox-host/v1` JSON Lines          |

执行 FP1 的起始提交可以是包含本合同和状态同步的后续纯文档提交，但其
receipt-producing 实现必须满足
`git merge-base --is-ancestor 186484feb935076766beab09595a9270f86f78ef <execution-HEAD>`；
`e33d6d68586a79796ffb9bcc668392e369dc97c6` 只用于核对该 baseline commit 的预期
tree，不把 tree 当作可判断 ancestry 的对象。执行前必须记录精确 HEAD/tree，同时确认
`e96ef3f` Accepted lineage 和 `186484f` 实现 baseline 都是祖先，且 tracked worktree
clean；任何实现改动完成后，真实运行必须绑定新的完整 commit/tree，不能仍声称运行在
`186484f`。

Windows 输入固定为：

| Artifact | 仓库路径                                      | raw SHA-256                                                        |
| -------- | --------------------------------------------- | ------------------------------------------------------------------ |
| A        | `tests/fixtures/camoufox/identity-win-a.json` | `a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f` |
| B        | `tests/fixtures/camoufox/identity-win-b.json` | `ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9` |

Artifact raw bytes、sidecar、固定 Camoufox/Playwright/BrowserForge、browser archive
和 browser tree 沿用当前 pin。FP1 不升级上游，不下载 `latest`，也不以未合并的
upstream PR 替代仓库内已物化的 Artifact。

## 2. 唯一目标与成功定义

FP1 只回答：

> 同一份 raw Resolved Identity Artifact，是否能在两个独立 Host 冷启动中重放
> 同一套最终 Camoufox 配置，并让本机 first-party probe 对目标浏览器可见字段得到
> 稳定结果；另一份 Artifact 是否只按预先声明的差异产生合理分离。

FP1 成功必须同时满足：

1. 每个可能影响浏览器可见身份的随机源都有明确归属，不存在“由 Camoufox 或
   BrowserForge 自行随机但 VeriSilo 不知道”的灰区；
2. A1 与 A2 从完全相同的 Artifact raw bytes/SHA 运行，规范化后的完整
   `CAMOU_CONFIG` 与磁盘 `resolvedConfig` 逐键、逐值相同；
3. A1 与 A2 的目标观察面逐字段一致，不能只比较
   `ObservedWebsiteDigest v2`；
4. B1 的差异与 A/B Artifact 的预先静态 diff 一致；未声明应分离的字段不得为了
   “看起来不同”而随机化，也不要求 A/B 每个字段都不同；
5. Profile A 的状态在 A1→A2 延续，Profile B 不继承 A 的 Cookie、LocalStorage
   或 boot count；
6. 三次运行都完成 fail-closed 生命周期检查，没有残留 Host、supervisor、
   Camoufox 进程、Windows Job active process、Profile lock 或错误 ownership；
7. 结果继续使用诚实的 evidence 语义，`verified:false`，不产生跨主机、不可检测、
   Chrome 模拟或完整字体隔离等声明。

这里的“同一 Artifact”是**重放同一份 raw 文件**，不是用相同 fixture RNG seed
重新调用 resolver 两次。`artifactId`、`generatedAtUtc` 等制品元数据本来就可能使
两次重新生成的 raw Artifact 不同，FP1 不把 resolver 输出字节相等作为目标。

## 3. 输出确定性不等于执行路径无 RNG

当前实现已经执行：

```text
disk resolvedConfig
→ deepcopy
→ launch_options(...)
→ normalize_camou_config_env(...)
→ 完整 diff 与 configured digest Gate
→ CAMOU_CONFIG
```

但 `launch_options()`、BrowserForge、Python `random` 或 NumPy 在计算候选默认值时
仍可能消费随机数。只要这些候选不会覆盖 Artifact 已物化的最终值，消费 RNG
本身不等于浏览器身份漂移。因此 FP1 必须分别证明：

- **output determinism**：不同环境 RNG 状态下，规范化后的完整
  `CAMOU_CONFIG` 都逐键等于同一磁盘 `resolvedConfig`；
- **observed determinism**：同一 Artifact 的 A1/A2 在浏览器实际观察面逐字段一致；
- **不作出的声明**：不得写成 `launch_options`、BrowserForge、Python、NumPy 或
  Camoufox 执行路径“没有使用随机数”。

静态回归应在不同 Python/NumPy RNG 状态下重放同一 Artifact，断言完整 sent config
及其 digest 不变。该回归只能证明最终投影不受这些 RNG 状态影响，不能替代真实
浏览器 A1/A2，也不能把未观察的内核随机面升级为稳定。

## 4. 完整随机源账本

真实浏览器运行前，必须对以下 producer 做静态审查：Artifact generator、
BrowserForge、Camoufox `launch_options`、固定 Camoufox 内部默认/seed 使用、Host、
probe 与浏览器运行时。每个身份相关来源必须归入且只能归入以下一种生命周期：

1. **Artifact explicit value**：最终值直接存在 v3 `resolvedConfig`；
2. **materialized replay seed**：seed 已存在 Artifact，并由固定引擎决定性派生网站
   可见结果，例如当前的 `canvas:seed`、`audio:seed`、`fonts:spacing_seed`；
3. **Silo/Vault secret**：若发现此类来源，只能记录其生命周期和不可泄漏边界；FP1
   不得把秘密写入 argv、日志、probe、result 或 Artifact；
4. **run/session non-identity entropy**：session ID、PID、临时目录、probe port、
   时间戳等运行期值，不得进入身份稳定性比较；
5. **currently uncontrollable / unavailable**：当前固定引擎无法明确控制或验证，
   必须公开记录原因，不能以默认值、推测或摘要遗漏掩盖。

账本在第一次真实启动前必须填完下表；任一候选来源没有分类时不得运行 A1：

| 表面/随机源                     | producer                                        | 生命周期分类                        | 身份相关 | `resolvedConfig` key          | 固定/派生机制                                             | first-party 观察字段                                     | 是否进入 digest v2   | A1/A2 预期               | A/B 预期                                    | evidence 状态 | 排除/待办原因                                           |
| ------------------------------- | ----------------------------------------------- | ----------------------------------- | -------- | ----------------------------- | --------------------------------------------------------- | -------------------------------------------------------- | -------------------- | ------------------------ | ------------------------------------------- | ------------- | ------------------------------------------------------- |
| UA / Navigator                  | Artifact generator；Camoufox MaskConfig         | Artifact explicit value             | 是       | `navigator.*`                 | 10 个显式 navigator 值覆盖 BrowserForge 候选              | UA、app/name/version/product、platform、oscpu、hardware  | 是                   | 显式字段必须相同         | 仅 `hardwareConcurrency` 应按静态 diff 分离 | configured    | `maxTouchPoints` 单列，不能伪装成 Artifact 控制         |
| Headers                         | Artifact generator；Camoufox 请求层             | Artifact explicit value             | 是       | `headers.Accept-Encoding`     | 完整 sent config 重放                                     | 当前 JS probe 无可靠 header 通道                         | 否                   | 配置必须相同             | A/B 配置相同                                | configured    | 实际请求头观察为 `unavailable`                          |
| Locale / Timezone               | Artifact generator；Camoufox MaskConfig         | Artifact explicit value             | 是       | `locale:*`、`timezone`        | 语言、地区、script 与 IANA timezone 显式重放              | `language(s)`、timezone、UTC offset                      | 是                   | 必须相同                 | A/B 配置相同                                | configured    | Direct-only；不宣称与网络出口协调                       |
| Screen / Window / DPR           | Artifact generator；Camoufox；Firefox UI        | explicit + host-bound               | 是       | `screen.*`、`window.*`        | screen/outer/screen offset 显式；inner/DPR 由固定引擎派生 | `screen`、DPR、完整 `windowGeometry`、session screen X/Y | 部分                 | Artifact-backed 必须相同 | 仅静态 screen/offset diff 对应字段应分离    | configured    | inner geometry 与 DPR 不能从 config 推断为 `applied`    |
| History                         | Artifact generator；Camoufox MaskConfig         | Artifact explicit value             | 是       | `window.history.length`       | 显式值阻止每次 launch 的随机 history 候选                 | `historyLength`                                          | 是                   | 必须相同                 | 应按静态 diff 分离                          | configured    | —                                                       |
| Canvas raw/export               | Artifact generator；Camoufox canvas patch       | materialized replay seed            | 是       | `canvas:seed`                 | 同一 materialized seed 交给固定引擎                       | `canvas.rawHash` / `canvas.exportHash`                   | 否                   | 两个 hash 均必须相同     | seed 不同，观察差异由真实结果决定           | configured    | export 历史上曾漂移；真实 A1/A2 是硬 Gate               |
| Audio                           | Artifact generator；Camoufox audio patch        | materialized replay seed            | 是       | `audio:seed`                  | 同一 materialized seed 交给固定引擎                       | `audioHash`                                              | 是                   | 必须相同                 | seed 不同，观察差异由真实结果决定           | configured    | —                                                       |
| WebGL / WebGL2                  | Artifact generator；固定 Camoufox 数据库        | Artifact explicit value             | 是       | `webGl:*`、`webGl2:*`         | vendor/renderer/参数/扩展/precision 已物化；候选只补缺失  | WebGL1/WebGL2 vendor、renderer、summary、availability    | WebGL1 部分          | 逐字段必须相同           | A/B 配置相同                                | configured    | probe 是摘要，未覆盖全部 shader precision               |
| Fonts / metrics                 | Artifact generator；Camoufox font patch         | explicit + materialized replay seed | 是       | `fonts`、`fonts:spacing_seed` | 字体列表与 spacing seed 均物化                            | injected availability、固定 universe widths、负控        | inherit 时部分排除   | 逐字段必须相同           | fonts/seed diff 对应字段由真实结果决定      | configured    | `fontMode=inherit`，完整宿主字体集合仍 host-bound       |
| Voices                          | Artifact generator；Camoufox voice patch        | Artifact explicit value             | 是       | `voices`                      | 完整 voice 列表已物化                                     | name/lang/URI/local/default                              | 是（保留 v2 旧形状） | 必须相同                 | A/B 配置相同                                | configured    | FP1 新增 default 只留在 `observedFull`                  |
| Media devices                   | Artifact generator；Camoufox/Firefox fake media | Artifact explicit value             | 是       | `mediaDevices:*`              | enabled 与三类设备计数显式；Windows deterministic prefs   | `mediaDevices`、readiness/counts                         | 是                   | 必须相同                 | A/B 配置相同                                | configured    | 只证明当前固定引擎/本机                                 |
| `navigator.maxTouchPoints`      | BrowserForge candidate；Firefox native          | currently unavailable / host-bound  | 是       | 无                            | candidate 经闭合 allowlist 审计后删除，最终采用 native 值 | `maxTouchPoints`                                         | 否                   | 同机观察并分类           | 不要求 A/B 不同                             | unavailable   | 未进入 v3；未知额外字段在 spawn 前 fail closed          |
| Browser/process/session entropy | Host / OS / Playwright                          | run/session non-identity entropy    | 否       | 无                            | 不进入最终 `CAMOU_CONFIG`                                 | 明确标注的 session/process/Job/lock receipt              | 否                   | 可不同                   | 可不同                                      | observed      | UUID、PID、port、临时路径、时间戳、RPC correlation 排除 |

实现审计还确认以下 RNG producer；这里区分“执行路径消费 RNG”与“最终身份被 RNG 改写”：

| producer                         | 何时消费                                        | 候选影响面                                        | FP1 处置                                                                                      |
| -------------------------------- | ----------------------------------------------- | ------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Python `random`                  | 每次 `launch_options()` 的 history/seeds 等候选 | history、Canvas/Audio/font seed、字体、voices     | 47-key Artifact 显式值/seed 先占位，`merge_into`/`set_into` 不覆盖；聚焦回归扰动并逐 key 比较 |
| NumPy / BrowserForge RNG         | 每次 BrowserForge fingerprint 与 WebGL 候选     | Navigator、screen、WebGL，以及偶发 maxTouchPoints | Artifact 覆盖已声明字段；唯一已分类额外字段为 maxTouchPoints；未知 extra fail closed          |
| Camoufox 字体/voice 子集随机选择 | 候选缺少 `fonts` / `voices` 时                  | 字体列表、voices                                  | v3 已显式物化，两条随机分支不改变最终投影                                                     |
| Camoufox noise seed 生成         | 候选缺少三个 seed 时                            | Canvas、Audio、font spacing                       | v3 已物化三个 seed，候选随机 seed 不覆盖                                                      |
| UUID / PID / port / path / time  | Host/session/probe/process 生命周期             | 无身份 config；仅运行 receipt                     | 归为 non-identity entropy；不得排除任何 Artifact-backed 浏览器观察字段                        |

聚焦静态回归以同一 raw A Artifact 执行 100 组 Python/NumPy/允许环境熵扰动；每一组都必须逐 key、逐类型等于磁盘 47-key config。该回归不宣称 BrowserForge 没有运行，也不替代 A1/A2 浏览器观察。

“待填写/待证明”是执行前工作，不是允许留到 Gate 之后的最终状态。最终表必须把
每行归为 `configured`、`applied`、`observed`、`unavailable` 之一；只有满足本合同
全部绑定条件的 FP1 总体验收才可在任务层写为 Accepted，但浏览器能力仍保持
`verified:false`。

## 5. Schema 与 seed 决策

FP1 默认保持 v3 Artifact、47-key `resolvedConfig`、Policy v3、Projection v3、
ObservedWebsiteDigest v2 和既有 fixture raw bytes不变。不增加面向用户的 master
seed，也不把 fixture-only `--rng-seed` 解释为 Silo seed 或浏览器进程输入。

若审计发现重要身份随机源不能由现有显式值或 materialized replay seed 确定表达：

1. 立即停止相关实现和真实运行；
2. 将该面标为 `unavailable`，并输出最小 source trace；
3. 单独提交 schema/ADR 提案，说明迁移、兼容、密钥生命周期和上游依赖；
4. 等主脑明确决策后另行冻结任务。

不得静默增加 Artifact 字段、重新生成 A/B fixtures、把 runtime 随机值写回 Profile，
或通过未固定上游版本“碰巧”获得稳定性。

## 6. 允许范围

未来执行 Agent 只可为 FP1 修改：

- standalone Host 内的 Artifact→`launch_options`→browser projection、脱敏阶段诊断
  和必要的 fail-closed cleanup；
- 现有 first-party probe，为当前表面增加缺失但不涉及 FP2 cross-realm 的观察字段；
- 现有 Host/Artifact/Windows 测试中的 focused deterministic projection、诊断、秘密
  扫描与生命周期回归；
- 本任务合同的 Gate result/覆盖表和状态页。

实现应复用现有 Host、probe、Windows supervisor、fixtures 和测试入口。不得为了
FP1 新建 runner、freezer、evidence manifest、manifest schema 或大型 evidence 系统，
也不得增加 production dependency。

## 7. 明确禁止范围

FP1 不做：

- RuntimeManager、Tauri、EngineAdapter production package、M3-WI Accepted 或桌面 UI；
- Standard Silo、Managed Silo 创建流程、installer、签名、升级或分发；
- proxy、GeoIP、地区、DNS、WebRTC、network fallback 或 Direct-only 政策变更；
- iframe、cross-origin iframe、Dedicated/Shared/Service Worker、多 context 等 FP2；
- 检测站点、普通网站矩阵、per-site fallback 等 FP4；
- Controlled Chromium、WSL、Hyper-V、VMware、Remote 或整机环境隔离；
- 十周期、5+5、R1/R2/R2H 或其他为追逐第二 Host 偶发挂起而扩张的矩阵；
- 重写 Host v1 协议、修改 stdout JSONL wire shape，或把进度帧写入 stdout；
- 修改、删除或重新生成任何 M0–M3-WI 历史 accepted/rejected evidence。

## 8. 最小生命周期可观测性硬化

FP1 只把现有粗阶段拆为下列持久、有限、协议级边界：

```text
launch_options
→ launch_persistent_context
→ supervisor_job_bind
→ new_page
→ goto
→ observed.fonts
→ observed.media
→ observed.identity
→ cookie
→ observed.write
→ response_write
```

边界定义固定如下：

| stage                       | start                                         | success boundary                                                 |
| --------------------------- | --------------------------------------------- | ---------------------------------------------------------------- |
| `launch_options`            | 调用固定 `launch_options()` 前                | config 已规范化并通过 disk/sent 全量 diff                        |
| `launch_persistent_context` | 调用 persistent `AsyncNewBrowser` 前          | context 对象已返回并写入 session                                 |
| `supervisor_job_bind`       | context 返回后                                | supervisor metadata 身份已校验；Windows Job 已成功打开并归属确认 |
| `new_page`                  | `ctx.new_page()` 前                           | 显式 page 对象已返回                                             |
| `goto`                      | 本地 probe navigation 前                      | `domcontentloaded` 已完成                                        |
| `observed.fonts`            | probe font inputs / `document.fonts.ready` 前 | 字体输入、ready 与负控所需准备完成                               |
| `observed.media`            | media readiness RPC 前                        | media readiness 已返回明确结果                                   |
| `observed.identity`         | `readIdentity()` RPC 前                       | 完整 first-party identity object 已返回                          |
| `cookie`                    | boot count / Cookie 操作前                    | boot count 和 API/page Cookie evidence 已收集                    |
| `observed.write`            | signals/digest/projection 组装前              | `observed.json` 已完整写入                                       |
| `response_write`            | 最终 protocol response 序列化前               | 单行 stdout 已写入并 flush                                       |

每个 command 内每个 stage 最多记录一个 `start` 和一个 terminal event；terminal 只能
是 `success`、`error`、`timeout` 或 `cancelled`。每条记录立即 flush 到 stderr 和
受控持久诊断文件。旧的粗阶段不得与新阶段重复产生含混的“success”。诊断预算必须
有显式事件/字节上限并为 close/failure 保留空间；达到上限时 fail closed 地省略后续
普通记录，但必须尽力保留最后 stage 与 cleanup 结论。

记录只允许包含固定 stage、event、单调 elapsed bucket、protocol request ID 的脱敏
关联值和非敏感错误类别；不得包含 URL、Cookie、LocalStorage、Artifact seed、Vault
值、token、代理凭据、argv、环境值或用户绝对路径。诊断文件在该 command/run 完成
审阅前不得由 runner 清理，stdout 继续只包含 Host v1 JSONL 最终响应。

Timeout 只冻结以下严格关系：

```text
具体 Playwright / Host 操作 deadline
< Host launch command deadline（须为 cleanup 与错误响应留出时间）
< 当前父端 stdout watchdog
```

本合同**不冻结新的秒数**。第二 Host 调查不足以支持凭猜测写入新的
`60/100/120` 组合；不得通过单纯扩大父端 watchdog 隐藏挂起。实现结果必须列出现有
父端 watchdog、实际采用的各级 deadline、其来源和不等式检查。若无法从当前实现、
已有有界 cleanup 和真实 clean-run 数据得到可审计预算，停止并返回
`blocked: timeout_budget_unfrozen`，不得由执行 Agent自行选择方便的秒数。

该硬化只改善错误归因和 fail-closed 返回，不得包装为第二 Host 底层挂起已经修复。

FP1 实现审计没有得到可冻结的新严格 timeout 层级：direct Python harness 与
test-only Rust M3-WI watchdog 均为 120 秒，但 production adapter 的初始 receipt
窗口是另一条 5 秒合同，不能把其中任一个泛化为 standalone Host 的唯一父端；固定
Playwright 1.60 的 persistent launch 默认是 180 秒，现有 `goto` 为 60 秒、supervisor
metadata 为 5 秒、media readiness 为 8 秒，而 `new_page`、fonts/identity evaluate 与
cookie RPC 没有独立 channel deadline。现有 cleanup 还包含没有总 deadline 的 server
shutdown 与文件 I/O，因此也无法证明 cleanup 总预算。FP1 本轮据此只实现阶段诊断，
不增加 operation/command deadline，不扩大任何父端 watchdog，也不把 timeout 层级写成
已经关闭的 Gate。

续执行对 `observed.media` 的语义判定是：launch 中的 media readiness 只是一项可选
Runtime Evidence，不是 Artifact 已应用的硬屏障；后续完整 probe 才是 FP1 的权威 media
观察。浏览器内 enumerate timeout 或 API unavailable 可以按固定 reason 继续到完整 probe，
但 channel timeout / Playwright exception 后页面不可安全复用，必须进入既有 fail-closed
cleanup。readiness 结果本身不把 Artifact 的 configured media count 升级为 `observed`；
后续完整 probe 返回的实际 count 即为 `observed`，无论是否匹配。只有匹配才能通过 FP1；
probe 无法可靠返回时标成 `unavailable`。

该聚焦收口复用 probe 已有的最多 3 秒 browser-side enumerate 边界、Host 已有的 8 秒
media readiness 边界和父端 120 秒 watchdog。既有 250 ms poll cadence 同时作为 channel
response/cancel-settle margin，因此首个 Python channel await 最多 7.75 秒，实际局部关系是
`3 < 7.75 < 8 < 120`；余量不足 500 ms 时不再发起新 RPC。这里的 8 秒是 media stage
deadline，不是新的全局 Host launch command deadline；失败路径虽已有 Playwright close
5 秒加进程树收口 6 秒的显式预算，但 server shutdown / 文件 I/O 仍无总 deadline，且失败
A1 的已记录 stage 合计 35.837 秒不包含全部 launch 工作，因此不能据此声称严格的全局
cleanup margin。本补丁不把尚未冻结的全局 command/cleanup 预算伪装为已建立。

## 9. 唯一真实验证矩阵

真实验证必须在同一台原生 Windows 交互式桌面、相同固定引擎/Host build、相同本地
first-party probe origin 下，严格只运行三个顺序 session：

| Run | Host / Profile            | Artifact                       | 初始状态                        | 必须证明                                                    |
| --- | ------------------------- | ------------------------------ | ------------------------------- | ----------------------------------------------------------- |
| A1  | 独立 Host H1 / `fp1-a`    | A raw SHA                      | run-owned 空 Profile            | 首次身份观察；boot `0→1`；写入 A 状态                       |
| A2  | 新 Host H2 / 同一 `fp1-a` | 与 A1 完全相同的 raw bytes/SHA | A1 clean close 后的同一 Profile | 目标身份逐字段等于 A1；boot `1→2`；Cookie/LocalStorage 延续 |
| B1  | 新 Host H3 / 独立 `fp1-b` | B raw SHA                      | run-owned 空 Profile            | 与静态 A/B diff 对应的身份分离；boot `0→1`；没有 A 状态     |

A1 close/shutdown、Job active count `0`、进程全退和两个 Profile lock byte 可重新取得
后才能启动 A2；A2 同样 clean 后才能启动 B1。三次使用同一固定 probe origin，使
Profile B 的 Cookie/LocalStorage 不继承 A 成为有效反例。B1 不得通过换 origin 隐藏
状态串扰。

运行前必须生成 A/B `resolvedConfig` 的规范逐字段 diff，并把每个差异映射到覆盖表。
验收比较规则为：

- A1/A2：只有同时满足“确属 run/session non-identity entropy”且“不由
  `resolvedConfig` 显式值或 materialized replay seed 支撑”的字段才能排除，目标字段
  必须深度相等；旧 policy 中的 `session-variable` 分类不能覆盖 FP1 的 Artifact
  所有权。`windowScreenX`、`windowScreenY`、Canvas `rawHash` / `exportHash`、Audio、
  完整 WebGL 摘要、字体 availability/width、voices 和 media device 结果都必须单独
  比较；无法可靠观察时必须写成 `unavailable`，不能静默排除；
- A/B：只对 Artifact 静态 diff 能影响的目标字段要求预期差异；共同字段仍应协调且
  稳定，不以“摘要不同”替代字段映射；
- `ObservedWebsiteDigest v2` 可作为辅助校验，但它排除 Canvas、内部 seed、Artifact
  字体输入，且 inherit font mode 下排除字体宽度，因此不能作为 FP1 的唯一证据；
- probe 无法观察的 configured 字段默认仍停留在 `configured`；只有存在已审计的
  per-key 引擎消费/应用证据时才能升级为 `applied`，当前引擎不能可靠控制时标成
  `unavailable`。任何一种情况都不能从 config 值推断成 `observed`。

## 10. Focused 自动回归

除 A1/A2/B1 外，只允许不扩张真实矩阵的 focused 回归：

1. 同一 raw Artifact 在扰动 Python/NumPy RNG 状态后，完整 normalized sent config
   仍逐键等于 disk config；
2. 任一 added/removed/changed key 继续在 browser spawn 前 `config_mutation`；
3. 每个新 stage 只产生 start + terminal，立即 flush，stdout 保持纯 JSONL；
4. secret/path sentinel 不出现在新增 stage stderr、持久诊断或 tracked/sanitized Gate
   摘要；既有 Host hello 中的受控绝对 root 只允许留在 gitignored 本地 raw bundle，
   不得复制进落库摘要；
5. fake delayed/hung call 能返回对应 stage 的 timeout/error 分类并进入既有
   fail-closed cleanup，不启动真实浏览器矩阵；
6. Profile lock、Windows Job 和精确 process ownership 的既有回归继续通过。

不为偶发第二 Host 问题增加新的真实循环。若问题在 A1/A2/B1 中自然出现，按下一节
处理。

## 11. 停止、失败与有限修复规则

- **随机源未分类**：A1 前停止；补完整账本，不运行浏览器碰运气。
- **v3 无法表达重要身份源**：停止并提交 schema/ADR；不得改 fixture 或偷偷持久化。
- **A1/A2 身份字段漂移**：FP1 failed；保存两份原始观察和最小字段 diff，只修已定位
  的 projection/seed 根因，不通过排除字段或修改 digest 掩盖。
- **A/B 出现未声明差异或应有差异缺失**：FP1 failed；先修静态 projection mapping，
  不通过扩大随机化满足 separation。
- **自然出现生命周期挂起且最后 stage 明确**：只允许修该具体调用和增加一个 focused
  regression；清理确认后只重跑受影响的 A1/A2/B1 序列，不恢复 R2/R2H 矩阵。
- **再次挂起但没有可靠最后 stage**：立即停止真实运行，判定诊断不足；先修可观测性，
  不增加次数、runner 或 manifest。
- **证明是固定 Camoufox/Playwright 上游问题**：保存脱敏最小复现和版本绑定，停止并
  交回主脑选择 pinned upgrade 或 workaround；执行 Agent不得自行升级。
- **close/Job/锁无法确认**：保持 failed/quarantined 和 ownership，不启动下一 run，
  不手工删除锁后冒充 clean。
- **需要 RuntimeManager、UI、proxy、FP2–FP4 或 public protocol/schema 变化**：停止并
  报告越界依赖，不扩张 FP1。

## 12. Evidence 语义

FP1 全程遵守：

| 状态          | FP1 中的含义                                                                                                                                |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `configured`  | 值存在于通过 raw SHA/schema 校验的 Artifact；disk/sent 全量相等仍只证明投影已准备好                                                         |
| `applied`     | 精确 config 已交给固定引擎、persistent context 成功返回、未发生 config mutation/rejection，并且该字段具有已审计的 per-key 引擎消费/应用证据 |
| `observed`    | 指定观察通道返回该值：身份字段使用 first-party probe，进程/生命周期使用明确标注的 runtime receipt                                           |
| `verified`    | FP1 不产生；所有 result 保持 `verified:false`                                                                                               |
| `unavailable` | 当前固定引擎或探针不能可靠控制/观察；必须保留原因                                                                                           |

A1/A2 一致只能写为 `observed stable on this Windows host`。它不证明跨物理主机、
跨浏览器版本、iframe/Worker、TLS/QUIC、网络身份、完整字体隔离或不可检测。B1
分离不等于所有 Silo 都具备唯一、不可关联的设备身份。

## 13. 结果包与 Gate 输出

原始输出写入唯一的 gitignored `artifacts/camoufox-fp1/<run-id>/`，不得包含秘密。
既有 Host hello 若包含受控绝对 root，只能保留在该本地 raw bundle；tracked Gate
结果必须使用仓库相对路径并完成脱敏。FP1 不创建 tracked evidence manifest、schema、
runner 或 freezer。最终结果必须在本合同末尾追加 Gate result，并至少包含：

1. source、receipt-producing、evidence-only（若有）commit/tree 与 clean/ancestry；
2. 固定 Host/browser/dependency/Artifact raw SHA 绑定；
3. 完整随机源账本和最终覆盖表；
4. A/B 静态 config diff；
5. A1/A2/B1 每个 run 的 raw result 相对路径、SHA-256、Host/session identity、阶段
   终点、clean close、Job、锁和残留进程结果；
6. normalized sent config 全量相等结果，以及逐字段观察 diff；
7. Cookie、LocalStorage、boot `0→1→2` 与 B `0→1` 的隔离结果；
8. focused tests 的精确命令、退出码和计数；
9. timeout 层级的实际值、来源和严格不等式，或明确 blocked reason；
10. 每一项 `configured/applied/observed/unavailable` 结论和仍不能宣称的能力；
11. integration extraction：可进入未来产品 patch 的最小 production diff、focused
    regressions，以及必须排除的研究基础设施；
12. 最终 Gate：`Accepted`、`Failed` 或 `Blocked`，不得只写总体测试“绿色”。

覆盖表使用下列固定格式，不得删除困难表面：

| 指纹面                    | Artifact 控制              | 固定机制                           | A1/A2      | A/B        | 观察字段                          | digest v2 覆盖 | evidence 状态 | 限制/排除                              |
| ------------------------- | -------------------------- | ---------------------------------- | ---------- | ---------- | --------------------------------- | -------------- | ------------- | -------------------------------------- |
| UA / Navigator / platform | 10 个显式值；touch 除外    | v3 逐键重放                        | 未取得结果 | 未执行     | navigator 全字段及 touch          | 是             | `configured`  | A1 未到 `observed.identity`            |
| Headers                   | `Accept-Encoding` 显式     | v3 逐键重放                        | 未取得结果 | 未执行     | 当前 probe 无请求头通道           | 否             | `configured`  | 实际 header observation `unavailable`  |
| Locale / timezone         | 显式                       | v3 逐键重放                        | 未取得结果 | 未执行     | language(s)、timezone、UTC offset | 是             | `configured`  | Direct-only；未观察                    |
| Screen / window / DPR     | screen/outer/offset 显式   | v3 显式值；inner/DPR 为 host-bound | 未取得结果 | 未执行     | screen、DPR、完整 geometry        | 部分           | `configured`  | A1 未写 observed result                |
| History                   | 显式                       | `window.history.length`            | 未取得结果 | 未执行     | `historyLength`                   | 是             | `configured`  | 未观察                                 |
| Canvas raw                | `canvas:seed`              | materialized replay seed           | 未取得结果 | 未执行     | `observedFull.canvas.rawHash`     | 否             | `configured`  | 无法验证 restart stability             |
| Canvas export             | `canvas:seed`              | materialized replay seed           | 未取得结果 | 未执行     | `observedFull.canvas.exportHash`  | 否             | `configured`  | 历史风险仍未关闭                       |
| Audio                     | `audio:seed`               | materialized replay seed           | 未取得结果 | 未执行     | `audioHash`                       | 是             | `configured`  | 未观察                                 |
| WebGL / WebGL2            | 参数、扩展、precision 显式 | v3 逐键重放                        | 未取得结果 | 未执行     | 两类 vendor/renderer/summary      | WebGL1 部分    | `configured`  | 新 WebGL2 probe 未得到真实结果         |
| Fonts / metrics           | fonts + spacing seed       | 显式列表 + materialized seed       | 未取得结果 | 未执行     | availability/widths               | 部分排除       | `configured`  | fonts 准备完成不等于 probe observation |
| Voices                    | 完整列表显式               | v3 逐键重放                        | 未取得结果 | 未执行     | 含 default 的完整 voice 列表      | 旧 shape       | `configured`  | 未观察                                 |
| Media devices             | 三类计数显式               | Windows fake-media prefs           | A1 挂起    | 未执行     | `mediaDevices`                    | 是             | `unavailable` | 卡在 readiness 内的 Playwright RPC     |
| `maxTouchPoints`          | 无                         | candidate 删除后采用 native 值     | 未取得结果 | 不要求差异 | `maxTouchPoints`                  | 否             | `unavailable` | closed allowlist；host-bound           |
| Cookie / LocalStorage     | Profile 所有               | Profile persistence                | 未取得结果 | 未执行     | API/page/SQLite/boot              | 非身份 digest  | `unavailable` | A1 未返回，不能声称状态写入或隔离      |

## 14. Gate result

### 2026-08-10 执行结论

**Failed。** 这是执行负责人按冻结合同给出的失败结果，不是主脑的 Accepted 判断，
也不改变 M3-WI 的 `failed` / `experimental` 状态。

实现绑定：

| 对象                   | commit / tree                                                                                            |
| ---------------------- | -------------------------------------------------------------------------------------------------------- |
| 文档 checkpoint        | `d4fd8993b6e51a47e9bdd9c84ceaa5cb1b328f35` / `2d65e0efde241b239109532465a5891c4720b8be`                  |
| receipt-producing 实现 | `6362e91a05413fce981f8e738d11ac21a169da48` / `06e3a129febb8e3bd9781cc46c80781aa2730fd2`                  |
| 固定浏览器归档         | Camoufox `v152.0.4-beta.28` / SHA-256 `386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d` |
| 固定 Python 执行依赖   | Camoufox `0.5.4`、BrowserForge `1.2.4`、Playwright `1.60.0`                                              |
| Artifact A raw SHA-256 | `a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f`                                       |
| Artifact B raw SHA-256 | `ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9`                                       |

实现前的聚焦回归成立：同一 raw A Artifact 在 100 组 Python/NumPy/允许环境熵扰动下，
normalized sent config 每次都逐 key、逐类型等于磁盘 47-key config；唯一出现的 candidate
extra 是闭合策略中的 `navigator.maxTouchPoints`，未知 extra 和无效类型均 fail closed。
这只证明 deterministic projection prepared，不替代浏览器观察。

A/B 的规范 47-key 静态 diff 精确为：`audio:seed`、`canvas:seed`、`fonts`、
`fonts:spacing_seed`、`navigator.hardwareConcurrency`、`screen.availHeight`、
`screen.availTop`、`screen.availWidth`、`screen.height`、`screen.width`、
`window.history.length`、`window.screenX`、`window.screenY`。B1 未执行，因此没有把
这些 configured 差异升级为 applied 或 observed 差异。

真实运行严格停在 A1：

| Run | 是否启动 | 结果                                                             |
| --- | -------- | ---------------------------------------------------------------- |
| A1  | 是       | 父端等待首个 launch response 120 秒后 timeout；stdout frame 为 0 |
| A2  | 否       | A1 失败后按合同停止，未启动第二 Host                             |
| B1  | 否       | A1 失败后按合同停止，未创建 B 浏览器身份                         |

A1 的持久阶段日志给出了可靠边界：

```text
launch_options:success                 285 ms
launch_persistent_context:success   30,660 ms
supervisor_job_bind:success              0 ms
new_page:success                      3,722 ms
goto:success                            105 ms
observed.fonts:success                1,065 ms
observed.media:start
<no terminal before the 120-second parent watchdog>
```

因此本次不是旧报告中无法区分的 persistent context / new page / later RPC 集合；当前
自然复现已收窄到 `wait_for_configured_media_devices()` 阶段。该 helper 的名义 8 秒
deadline 不能约束内部无界 Playwright RPC；现有证据还不能在其 `page.evaluate()` 与
后续 `page.wait_for_timeout()` 之间继续唯一归因。本轮没有据此修改 timeout、重跑 A1
或启动 A2/B1。

raw bundle（gitignored、非 manifest、非 Accepted evidence）：

- `artifacts/camoufox-fp1/run-20260810T145548Z-0f5dddff/fp1-run-summary.json`，
  SHA-256 `e42613bea6b53dc6bd770f5b3ccae352f723a113ea31fece615726e05572df8a`；
- `artifacts/camoufox-fp1/run-20260810T145548Z-0f5dddff/state/host-stderr.log`，
  SHA-256 `51865285975b1c85d45e3bf5ca1f84b56f59135730c7e7dff3db17c4a5bce910`；
- `artifacts/camoufox-fp1/run-20260810T145548Z-0f5dddff/a1/failed-attempt.json`，
  SHA-256 `82d68080d2c07707cc526fbb3ec660e67b2a62cc8da0b7a2d1ffd680c55406aa`。

失败后的 ownership 收口成立，但不能包装成 clean protocol close：Host 被 harness 在
watchdog 后结束；supervisor 与 Camoufox 精确 PID/creation-time identity 均已退出，
supervisor `exit.json` 为 `0`，命名 Job 已关闭且无活动进程，Profile byte 0/1 均可重新
取得，没有任务子进程残留。阶段日志不含 URL、Cookie 名/值、Artifact seed、租约 token、
代理 sentinel 或用户绝对路径；原生浏览器租约已由匹配 token 释放。

聚焦验证：

- `uv run --frozen --offline python test_identity_artifact.py`：32/32 passed；
- `uv run --frozen --offline python test_windows_host.py --close-context-regression`：
  normal/timeout/exception/job-not-exited 四种 case passed；
- 修改 Python 文件 `py_compile`、probe JavaScript 语法、Prettier、Markdown 相对链接、
  `git diff --check`：passed。

Integration extraction 保持有限：未来产品 patch 可抽取 candidate-extra closed policy、
deterministic normalization、per-launch 11-stage diagnostics、launch-before-context 的 Job
fail-closed ownership、最小 probe 字段及 focused regressions。不得抽取 R1/R2/R2H
runner、freezer/finalizer、manifest/schema、历史 receipt 或本次 Profile/cache/log。

进入 FP2 前仍缺：先对 `observed.media` 内的具体 Playwright RPC 建立有界失败归因和单一
focused regression，再由新授权执行完整 A1→A2→B1。当前没有 A1/A2 指纹逐字段结果，
所以不能宣称同一 Artifact 已稳定重放、Canvas raw/export 稳定、A/B 已分离或 FP1 passed。

### 2026-08-11 续执行结论

**Blocked（`blocked-upstream`）。** 这不是 FP1 Accepted，也不改变 M3-WI 的
`failed` / `experimental` 状态。实现从 `68861a37923f7fcd44f68f1435cc808c87fcc496`
/ tree `dfe3c5cf9d4d66b23aac1af7886e5426b6266b67` 起步，冻结为
`bc153f13c18af9f7404e3cb6674e3b29a18de800` / tree
`e3a788f6a06de9a8860b9c3bfe6ad0583f7f534d`；真实浏览器启动前工作树 clean，运行期间
没有修改源码。

media readiness 是 full probe 前的可选 Runtime Evidence，不是 Artifact 应用成功的 Host
launch hard barrier。完整 probe 返回的实际计数才是 `observed`；即使与 configured 不同
也仍是 observed mismatch，只有无法可靠返回才是 unavailable。旧 helper 的 8 秒本地
deadline 只在无界 Playwright `evaluate` / wait RPC 返回后检查，因此不能先于父端 watchdog
产生 typed terminal。本次修复加入 browser enumerate 最多 3 秒、Python channel 最多
7.75 秒、250 ms response/cancel margin 和 8 秒 media-stage 总预算；固定 terminal reason 为
`success`、`enumerate_timeout`、`readiness_timeout`、`count_mismatch`、
`playwright_exception` 或 `unavailable`。局部层级为
`browser JS ≤ 3s < channel ≤ 7.75s < media stage 8s < parent 120s`；没有伪称已存在全局
Host command deadline。

聚焦验证全部通过：

- `uv run --frozen --offline python test_identity_artifact.py`：39/39；
- `uv run --frozen --offline python test_windows_host.py --close-context-regression`：
  normal/timeout/exception/job-not-exited 四类通过，`secretFree=true`；
- 修改 Python 文件 `py_compile`、内嵌 media JavaScript 语法、Prettier、Markdown 链接、
  `git diff --check`：通过。

唯一真实序列只启动一次 A1 和一次 A2；A1/A2 比较失败后立即停止，B1 未启动：

| Run | Host PID / session                          | boot  | media                         | close / ownership                                      |
| --- | ------------------------------------------- | ----- | ----------------------------- | ------------------------------------------------------ |
| A1  | `7252` / `024a543c1ddb4d91904498555cc88f20` | `0→1` | `success`，234 ms，计数 1/1/0 | clean close；Job 0；两把 Profile lock 可重取           |
| A2  | `8552` / `df49c2779d9248b2b9cca0b13cb21a7e` | `1→2` | `success`，203 ms，计数 1/1/0 | clean close；Job 0；Cookie/LocalStorage 延续；锁可重取 |
| B1  | 未启动                                      | —     | —                             | A1/A2 Canvas export mismatch 后按合同停止              |

A1/A2 使用相同 raw Artifact SHA
`a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f`、相同
47-key normalized config、configured digest、Profile 和 probe origin，但两个独立 Host。
Navigator、locale/timezone、screen、完整 window geometry、DPR、Canvas raw、Audio、
WebGL/WebGL2、fonts/widths、voices/default、history、media 与辅助 ObservedWebsiteDigest
均深度相同；`maxTouchPoints=0` 保持 host-bound / unavailable。Cookie SQLite 中的值确认为
A1 写入并被 A2 延续，但值本身未写入日志或 tracked 结果。

唯一不相同的硬比较项是 Canvas export：

| 字段                | A1                                                                        | A2                                                                        | 结果 |
| ------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ---- |
| `canvas.rawHash`    | `sha256:acd1515152354d8bb340e2cf4d9516f48762504eb370ce5d0b3339801f9cfe10` | 同 A1                                                                     | 相同 |
| `canvas.exportHash` | `sha256:8a362ea76f7e46a7ce130b372eee1b59c618f71ecc4bf0ab158904a69cb090af` | `sha256:06d110cba2cd32c852359be9ef5227085254722bdf55a823bd2dbf59aad0c87e` | 漂移 |

probe 对确定场景先 hash `getImageData()` RGBA，再独立 hash 同一 canvas 的完整
`toDataURL("image/png")` 字符串，没有 probe RNG。相同 raw hash 排除了绘制输入和 raw
像素漂移；相同 raw Artifact、normalized config 和 configured digest 排除了 projection 漂移。
固定 Camoufox `v152.0.4-beta.28` 的 source tree 已删除 `canvas-spoofing.patch` 及
per-context `SetCanvasSeed` 路径，现有 `fingerprint-injection.patch` 也没有 Canvas hook；
因此 `canvas:seed` 虽仍被配置格式接受，却没有可审计的 per-key 应用路径，只能保持
`configured`，不能写为 `applied`。实际失败边界位于固定 Camoufox/Firefox Canvas export
路径，不在 Host、Playwright transport、静态 projection 或 probe hash 实现。

A/B 的静态 47-key diff 仍精确为冻结的 13 keys：`audio:seed`、`canvas:seed`、`fonts`、
`fonts:spacing_seed`、`navigator.hardwareConcurrency`、`screen.availHeight`、
`screen.availTop`、`screen.availWidth`、`screen.height`、`screen.width`、
`window.history.length`、`window.screenX`、`window.screenY`。Artifact B raw SHA 为
`ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9`；由于 B1 未启动，
这些 configured 差异没有升级为 applied/observed，Profile B 隔离也未取得浏览器证据。

raw bundle 仍是 gitignored、非 manifest、非 Accepted evidence：

- `artifacts/camoufox-fp1/run-20260811T023336Z-03647283/fp1-run-summary.json`，
  SHA-256 `a355b68cf27cdfae5991016fb7e0761a1f09c044a117ac268179f0c22669a325`；
- A1/A2 `attempt.json` SHA-256 分别为
  `6c74d645b133ddea4cae9ca05bbd1f2b65b91b8a1a7e60bdfcbf8ed5406fa3ae`、
  `56b39070a8329b5d587f42dfd304219de442c74e441079d68163db13dab87e6f`；
- `a1-a2-comparison.json` SHA-256
  `5e8d11e7cc52d44f5c5a02068cb04dc713de9bc6d6b786452e6f045d00c77ccf`。

所有本任务 Python Host、Playwright Node、supervisor、Camoufox、Job 和 probe server 均已
退出；两个 Job Object 已关闭且 active process 为 0，probe 端口可重新绑定，Profile byte
0/1 可重新取得；匹配 token 的原子 browser lease 已释放。没有重试、B1、FP2、依赖升级、
public schema/API 变化、push 或 PR。

继续关闭 FP1 的最小上游提案是：固定并重新绑定一个具有明确、可审计、跨独立 Host
deterministic Canvas export 应用路径的 Camoufox build（同时更新 archive/tree/binding 并
使现有 focused tests 重新通过）。另一选择是显式修改 Artifact/FP1 合同，把 export 重新
分类为 unavailable/session-variable；这会改变共享语义，且本次不得为了跑绿自行采用。
在主脑授权其中之一以前，不得重跑 A1/A2/B1，也不得进入 FP2。

### 2026-08-11 Canvas Engine Patch 合同冻结

主脑已经选择上述最小引擎决策：保持 Artifact/Silo-scoped Canvas identity，固定 FF152
并维护 VeriSilo downstream patch；不得把 export 降级为 optional，也不得把 site 或
browsing-session entropy 混入 Managed Canvas identity。

#### Browser contract

对存在 `canvas:seed` 的 Managed config（包括值 `0`）：

```text
same seed + same Canvas operation + independent browser process
→ exact same PNG/dataURL observable

different seed + same Canvas operation
→ deterministic different PNG/dataURL observable
```

key 固定为
`SHA-256(ASCII("verisilo-canvas-export-v1\0") || uint32_be(seed))`。不得加入
site/origin、Profile、PID、时间、session UUID 或其他运行熵。patch 只替换 Canvas export
resolver 的 randomization key；Firefox 原有 PNG encoder/`deBG` 结构继续使用，raw pixels
不加 noise。`canvas:seed` 缺失时必须原样走 Firefox CookieJar fallback。

FP1 v1 的硬观察表面是 `toDataURL("image/png")`；共享 resolver 的 PNG blob path 需要
源码/测试覆盖，但本轮不把 iframe、Worker、Service Worker 或跨 realm 一致性写成 FP1
通过条件。JPEG/WebP 等其他编码不得由 PNG 结果外推为已控制。

#### Binding and compatibility

- upstream Camoufox tag/commit 固定为 `v152.0.4-beta.28` /
  `0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d`；Firefox 固定为 `152.0.4`；
- 新 source lock、patch、archive lock、tree manifest 和 rebound Artifact fixtures 使用新名字；
  不覆盖 official lock、旧 Windows tree manifest、`identity-win-a/b/c` 或历史 evidence；
- 自建 archive 的 provenance 不能写成 GitHub official digest agreement，且保持
  `verified:false`；
- `canvas:seed` 字段和 Artifact 顶层 v3 schema 不变。旧 binding 继续要求 legacy
  session-variable Canvas policy；新 binding 才允许 deterministic Canvas Policy v3
  variant。validator 必须用 archive binding fail closed 选择唯一 variant。

#### Focused verification

browser source、patch set、archive、tree 和 focused harness 全部冻结且工作树 clean 后，
先取得原子 browser lease，只执行一次以下 sequence：

```text
canvas-A1: seed A / new Profile A / independent process
canvas-A2: same seed A / same Profile A / second independent process
canvas-B1: A config with only canvas:seed changed to seed B / Profile B /
           third independent process
```

三次必须使用同一 run-owned loopback HTTP origin 和同一 probe bytes；禁止
`about:blank`、`page.set_content()`、`data:` 或 `file:`。要求：

- A1/A2 raw RGBA、decoded PNG pixels、PNG bytes 和 dataURL hash 全部相同；
- B1 除 seed 外的最终 config 逐 key/类型等于 A，PNG 合法且 export hash 与 A 不同；
- 每次 clean close、Job active 0、Profile 两个 lock byte 可重取、无进程残留；
- `canvas:seed` 缺失时的 stock fallback 至少有源码级 focused regression；若不增加预声明
  browser control，不得把它写成 runtime-observed。

这三次不是完整 FP1，也不使用完整 Artifact B 的其他 12 个差异 key。只有 focused
sequence 一次通过后，才能从再次冻结的最终代码和新 binding 只执行一次完整
A1→A2→B1。任一失败立即停止，不修改后重跑、不选样、不进入 FP2。
