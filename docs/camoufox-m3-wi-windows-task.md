# M3-WI 原生 Windows Desktop / Real Host 集成任务合同

- 状态：**Frozen，等待执行 Agent**
- 冻结日期：2026-08-09
- 执行分支：`codex/camoufox-m3-engine-adapter`
- M3-0 Accepted checkpoint：`e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`
- M3-WI 实现代码基线：`e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`
- 上一 Gate：M3-0 **Accepted**；只证明 fake Host contract，不包含真实浏览器 run-id

本文既是 M3-WI 的冻结任务卡，也是交给执行 Agent 的提示词。首次完整包含本文与
对应状态更新的纯文档 Git commit 是执行起始 checkpoint；主脑在派发提示词中固定其
完整 SHA。执行 Agent 必须确认 HEAD 精确等于该 SHA 且工作树 clean；不匹配即停止，
不得自行 reset、rebase、cherry-pick 或改写历史。

执行前必须依次完整阅读：

1. 根 `AGENTS.md`；
2. [身份平台北极星](identity-platform-north-star.md)；
3. [Camoufox-first 决策](camoufox-managed-engine-decision.md)；
4. [Agent 协作协议](agent-operating-model.md)；
5. [Camoufox 当前状态](camoufox-program-status.md)；
6. [M3-0 任务与 Gate 结果](camoufox-m3-engine-adapter-task.md)；
7. 本文；
8. [EngineAdapter 现状](engine-adapters.md)、[Host v1](camoufox-host-v1.md)和
   [M2-W Accepted Gate](camoufox-m2w-windows.md)；
9. 只为理解接缝而阅读相关实现和测试，不扩张到 UI、installer 或其他 engine。

## 角色与 Gate 含义

你是本阶段的**执行 Agent**，不是重新决定产品方向的主脑。你的任务是在原生
Windows 交互式桌面中，用真实 standalone Python Host 和固定 Camoufox 浏览器关闭
M3-0 之后仍存在的运行时风险，并返回可审计证据；你不能自行宣布 M3-WI Accepted、
产品已 shipped，或把这一主机上的观察提升为跨主机验证。

M3-WI 要证明的是：桌面后端现有 `RuntimeManager`、EngineAdapter launch plan、
`camoufox-host-jsonl-v1` transport、Artifact/Profile binding 和 stop/refresh 生命周期，
能够真正驱动 `apps/camoufox-host/host_v1.py`，再由 Host 驱动固定的 Windows
Camoufox 树。它不是 UI Gate、签名发布 Gate、installer Gate 或生产 package Gate。

## 当前真实差距

M3-0 已关闭 fake Host 的 package、transport、failure matrix、生命周期和 evidence
合同，但仍没有执行真实 Python Host 或真实浏览器。与此同时：

- production external-engine verifier 没有受信 signer pin，必须继续 fail-closed；
- 仓库没有签名 Host package、发布 runtime 或 installer；
- 当前 v3 package 合同把 package entrypoint 定义为 Host executable，而开发树中的
  真实 Host 仍是锁定 Python 环境中的 `host_v1.py`；
- 因此不能通过放宽 production verifier、伪造签名或把系统 Python 当成签名 package
  成员来完成本 Gate。

M3-WI 必须诚实拆分两类证据：

1. M3-0 已接受的 production-shaped package/transport **合同证据**；
2. 本 Gate 新产生的 native-Windows integration-only **真实 Host/浏览器执行证据**。

两者同时成立仍不等于已有可发布的受信 Host package。

## 冻结目标

在一个原生 Windows 交互式桌面会话中，从精确 clean checkpoint 启动，完成以下
最小真实垂直切片：

1. `RuntimeManager.launch_with_identity_deriver` 通过一个编译期隔离的
   integration-only adapter/entrypoint 形成 Camoufox Host plan；
2. 真实 `spawn_engine_child` 启动锁定的 `uv/Python + host_v1.py`，不是 fake Host；
3. 同一条生产 Host transport 完成 `hello → launch → status → close → shutdown`；
4. Host 启动 release `v152.0.4-beta.28` 的真实 Windows Camoufox，并使用 Windows
   Artifact v3、browser tree 和 Persistent Profile；
5. 两个不同 Host child / 两个新 `RuntimeManager` 周期重用同一 Profile，证明
   Cookie、LocalStorage、cookies.sqlite 与 boot count 延续；
6. 正常停止、Host EOF/crash 和桌面父级关闭都保持精确 process/profile ownership，
   不遗留 Camoufox/supervisor 子树，也不误杀无关进程；
7. `RuntimeActivation`、持久化 `RuntimeRecord` 与新 tracked receipt 只记录有依据的
   `configured/applied/observed/unavailable`，保持 `verified: false` 和
   `verifiedAdapter = null`。

所有可写 app/artifact/profile/state/cache/report root 必须位于本次唯一 run-id 拥有的
`artifacts/camoufox-m3-wi-windows-gate/` 子树；不得复用或清理普通用户 Profile，也不得
写入 accepted fixture 目录。

## 原生 Windows 与固定输入

所有 Gate 证据必须来自原生 Windows，不接受 Linux、WSL、Wine、容器、虚拟浏览器
或 fake Host 替代。执行会话必须有真实交互式桌面（可为 RDP），并记录 Windows
edition/version/build、session、architecture、Python、uv、Rust、Camoufox、Playwright
和 BrowserForge 版本。

固定输入如下：

- Camoufox Python package：`0.5.4`；
- Playwright：`1.60.0`；
- BrowserForge：`1.2.4`；
- browser release：`v152.0.4-beta.28`；
- Windows asset lock：
  `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-windows-x86_64.json`；
- archive SHA-256：
  `386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d`；
- browser tree：`tests/fixtures/camoufox/browser-tree-manifest-windows.json`，
  canonical manifest SHA-256
  `1c749534d139b7efcb425faf03de9cfe1d59004034a1fe1c5ba423b86239c37b`；
- Artifact：`tests/fixtures/camoufox/identity-win-a.json`，raw SHA-256
  `a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f`；
- dependency lock：`apps/camoufox-host/uv.lock`；运行必须使用
  `uv run --frozen --offline`，不得下载或更新依赖、addon 或浏览器资产。

执行前必须重新计算实际 tracked bytes、sidecar、树和本地 archive/cache 绑定；任一
不匹配即在浏览器启动前 fail-closed。不得为让测试通过而重生成 accepted Artifact、
M2-W manifest 或 browser tree。

## integration-only 启动接缝

因为 production signer/package 尚不存在，本 Gate 允许一个最小、显式且不可发布的
真实 Host 启动接缝。它必须满足全部约束：

- 只能存在于 `#[cfg(test)]`、Windows-only ignored integration test，或等价的
  non-shipping test target 中；production build 不得引用或选择它；
- 可以把锁定 `uv`/Python 解释器和 `host_v1.py` 组合为测试命令，但必须记录解释器
  路径与版本、`host_v1.py` raw SHA、`uv.lock` raw SHA 和全部固定输入 SHA；
- 必须复用真实 `RuntimeManager` launch/refresh/stop、`spawn_engine_child`、
  `CamoufoxHostTransport`、Artifact/Profile roots、runtime record 和 evidence 映射；
- 不得用另一个直接 Python driver 绕过桌面接缝来冒充 M3-WI；既有 standalone
  driver 只能作为回归或反例证据；
- 不得把 test verifier 的结果写成 production signature verification；tracked receipt
  必须明确记录 `integrationPath=test-only-real-host`、
  `productionPackageVerified=false`、`shipped=false`；
- `ExternalPackageEngineAdapter::production_prototype` 在缺少 signer pin 时必须继续
  fail-closed，并保留独立回归测试；
- 不得增加 production dependency、私钥、证书、signer pin、隐藏 feature flag、环境
  后门或可从 release build 触发的 verifier bypass。

若无法在这些约束内启动真实 Host，停止并报告 package/runtime 交付缺口；不要把
M3-WI 静默扩大为签名和发布项目。

## 允许修改

- `apps/desktop/src-tauri/src/engine.rs`、`launcher.rs` 及相邻的 Windows-only/test-only
  集成 harness；
- `apps/camoufox-host/` 中为真实桌面接缝所必需的最小、向后兼容测试或 evidence
  支持；
- 新的 M3-WI Windows runner/freezer、JSON schema 和
  `tests/fixtures/camoufox/evidence-manifest-m3-wi-windows.json`；
- 与实际行为同步的 `docs/engine-adapters.md`、本文和状态页；
- 必需的 test target/Cargo 配置，但不得引入新的 production dependency。

允许范围不是必须修改清单。优先复用 M3-0 与 M2-W 已有实现，不重写 Host、Artifact
生成器、Windows supervisor 或 generic desktop launcher。

## 禁止修改与禁止宣称

- 不改 Artifact/Policy/Projection v3、ObservedWebsiteDigest v2 或 Host v1 的既有
  语义；若真实 response 缺少不可替代的绑定字段，先停止交回主脑裁决；
- 不改写、删除或重新生成 M0–M2-W accepted evidence、run-id、fixture 或 protected
  hashes；
- 不增加 UI、Managed Identity 菜单、installer、auto-download/update、代理注入、
  network geo coupling、site fallback、Controlled Chromium、JS stealth、WSL、VMware、
  Hyper-V 或 Remote backend；
- 不加入真实 signer、签名 package、发布 runtime 或 production verifier bypass；
- 不把 Camoufox 宣传为 Chrome 模拟；它是 Firefox 身份引擎；
- 不宣称字体隔离、Canvas 稳定身份、TLS/QUIC 身份、跨主机复现、不可检测或
  `verified: true`；
- 不把 integration-only run 写成 shipped Tauri product、production package evidence
  或真实用户可用能力。

## 验收矩阵

以下 1–16 项必须逐项返回 `passed`、`failed` 或 `blocked`，不得用总体测试通过替代。

### Baseline 与信任边界

1. **原生基线**：确认原始 Windows checkout、精确起始 commit、clean 工作树、原生
   Windows/交互式 session；执行前后没有本任务遗留的 Host/Camoufox/supervisor。
2. **固定输入闭环**：dependency lock、asset lock/archive、browser tree、Artifact raw
   bytes/sidecar 与上文固定值一致；全程 offline，DownloadGuard 未观测到 webdl。
3. **test-only 隔离**：真实 Host 启动接缝不能进入 production build；缺 signer pin 的
   production adapter 仍在 spawn 前 fail-closed，receipt 不声称 package verified。

### 真实 Host / browser 垂直切片

4. **真实路径**：证据证明调用链实际经过
   `RuntimeManager → adapter.launch_plan → spawn_engine_child → host_v1.py → Camoufox`；
   Host PID、browser/supervisor ownership 与 run-id 可交叉绑定，fake Host 未参与。
5. **hello/launch binding**：protocol、Host version、platform、release、asset SHA、browser
   tree 路径/SHA、Artifact ID/raw SHA、Profile ID 全部精确一致；任一 mismatch 在
   Running 前拒绝。
6. **真实观测**：真实 browser launch 返回 Artifact/config binding、
   ObservedWebsiteDigest v2、media-device match 和 `evidenceClass`；只记本机 observed，
   不据此升级 fingerprint、Canvas、font、TLS/QUIC 或跨主机结论。

### Persistent Profile 与生命周期

7. **跨桌面周期持久化**：从本 Gate 新建、run-owned 的空 Profile root 开始，两个不同
   Host PID、两个新 `RuntimeManager` 实例顺序使用同一 Silo/Profile/Artifact；boot
   count `1 → 2`，Cookie API、document cookie、cookies.sqlite 与 LocalStorage 延续，
   两个 run 都绑定相同 Artifact raw SHA。
8. **正常停止**：只有 `close.processTreeExit.exited=true`、Windows Job active process
   count `0`、`shutdown` 与精确 Host child exit 全部确认后，才发布 `Stopped` 并释放
   Profile lease。
9. **EOF/crash/desktop close**：活动 session 中分别覆盖 Host EOF/异常退出和桌面父级
   关闭；只回收本 RuntimeManager 精确拥有的 child/tree。无法确认全树退出时状态必须
   为 `VerificationFailed` 或 `RecoveryRequired`，Profile lease 不得被错误释放。
10. **并发与无关进程**：第二个相同 Profile launch 被拒绝；预先创建的无关进程和
    无关浏览器不被等待、终止、接管或写入 evidence。

### Fail-closed 与秘密边界

11. **跨接缝反例**：至少覆盖 Artifact raw SHA、browser tree SHA、Host hello binding、
    profile-in-use、active-tree exit unconfirmed、malformed/oversize frame、timeout 和
    early-exit；失败有界且不能错误进入 Running。
12. **网络策略**：真实成功 run 只能使用 `Direct { proxy_required: false }`；required
    proxy、FixedProxy、PAC 与非空 fallback 在 Host spawn 前拒绝，不能回退或偷偷把
    Chromium proxy argv 传给 Host。
13. **秘密扫描**：Vault deriver sentinel、Artifact seed、token value、proxy username /
    password sentinel 不得出现在 Host/browser argv、环境快照、plan、JSONL wire、错误、
    `RuntimeActivation`、持久化 `RuntimeRecord` 或 tracked/raw evidence。

### Evidence 与回归

14. **语义诚实**：真实 Host 成功仅把 `hostLaunch` 记为 `Observed`；generic bootstrap /
    receipt 字段保持 `NotApplicable`，`verifiedAdapter=null`、`verified:false`。失败后旧
    observed evidence 不得继续有效。
15. **回归**：M3-0 package/transport tests、Stock Chrome/Edge、Controlled Chromium
    bootstrap/receipt/fallback、Artifact 25/25、Desktop Rust 全套 check/test/clippy 和
    JS contracts/desktop/extension 测试继续通过。
16. **可审计 receipt**：新增独立 M3-WI tracked manifest，绑定 receipt-producing
    commit/tree、原生 Windows host、固定输入 SHA、每个 run-id/report/sidecar SHA、
    lifecycle/secret scan 结果和明确的 test-only/未发布边界；raw profiles/reports 保持
    gitignored，manifest 不含秘密或用户路径之外的敏感数据。

## 真实运行与 evidence 冻结顺序

1. 在 clean 文档 checkpoint 上实现并运行不启动浏览器的快速测试；
2. 提交完整 receipt-producing code，记录完整 commit 与 tree hash；
3. 再次确认 clean，在该精确 commit 的原生 Windows 交互式桌面运行 M3-WI real suite；
4. 原始 report 与 sidecar 写入新的 gitignored
   `artifacts/camoufox-m3-wi-windows-gate/runs/`，每次运行使用唯一 run-id；
5. freezer 只从已完成、sidecar 验证通过且 `codeGitRevision` 精确一致的报告派生新的
   tracked manifest；
6. 用后续 evidence-only 提交冻结 manifest。若此后修改任何 receipt-producing runtime、
   harness 或 schema，必须重新运行受影响 Gate；不得 amend/rebase 旧 evidence 来伪装
   同一 revision。

不得复用 M2-W run-id 冒充本 Gate，也不得把 M3-0 fake Host 测试命名为真实浏览器
run。新 manifest 至少记录：

- schema `verisilo-camoufox-m3-wi-windows-evidence-manifest/v1`；
- `codeGitRevision`、`codeTreeHash`、branch、host/session/tool versions；
- `integrationPath=test-only-real-host`、`productionPackageVerified=false`、
  `shipped=false`、`verified=false`、`evidenceClass=observed-on-this-windows-host`；
- Host source/lock、asset/archive、browser tree、Artifact 的精确 SHA；
- real launch、persistence、clean stop、EOF/crash/desktop-close 和 negative matrix 的
  run-id、report 路径与 SHA-256；
- `productionVerifierFailClosed=true`、DownloadGuard、secret scan、residual process 与
  protected accepted-file hash 结果。

## 必跑验证

至少执行并报告实际命令、退出码和计数：

```powershell
pnpm check
pnpm test
pnpm build
pnpm engine:verify
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
Set-Location apps/camoufox-host
uv run --frozen --offline python test_identity_artifact.py
```

此外必须执行一个命名明确、单线程、有界超时的 Windows-only M3-WI real integration
命令，输出新的 run-id 和 summary sidecar。若实现为 ignored Rust test，命令必须形如：

```powershell
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml <精确测试名> -- --ignored --nocapture --test-threads=1
```

若修改 `apps/camoufox-host/host_v1.py`、`host_platform.py`、`browser_tree.py`、Windows
supervisor 或 accepted Windows fixtures，则除 M3-WI suite 外，还必须重跑完整
`test_windows_host.py`，并把影响交回主脑重新判断；没有修改这些文件时，不要求为了
形式重跑全部 M2-W 长矩阵，但第 7–11 项的跨桌面接缝证据仍不可省略。

## 停止条件

出现任一情况立即停止并返回 `blocked` 或 `failed`，不得自行扩大：

- 实际权限、cwd、分支、起始 commit、工作树或原生 Windows/交互式 desktop 不符合；
- 固定 archive/cache/uv 环境不存在或不匹配，继续需要网络下载或更新 lock；
- 真实 Host 只能通过 production verifier bypass、伪签名、真实 signer 或发布 package
  才能启动；
- 必须改变 Artifact v3、ObservedWebsiteDigest v2、Host v1 语义或 accepted M0–M2-W
  evidence；
- 必须增加 production dependency、UI、installer、代理、Controlled Chromium 或其他
  延后后端；
- 浏览器/Job tree 退出无法确认、Profile ownership 不确定、出现无关进程误杀或秘密
  泄漏；
- 真实 run 不是由最终 receipt-producing commit 生成，且无法安全重跑。

## 执行 Agent 结果包

最终返回必须按以下顺序：

1. 执行结论：`passed`、`failed` 或 `blocked`；不得自行写 Accepted；
2. 原生 Windows host/session、精确起始 commit、receipt-producing commit、evidence
   commit、branch/upstream、工作树与 push/PR 状态；
3. 逐项 1–16 Gate 矩阵及每项对应的 run-id、测试名或代码证据；
4. 真实调用链与 integration-only 隔离说明，明确 production verifier 仍 fail-closed；
5. 固定 Artifact/asset/browser tree/Host/lock SHA 与真实 Host/browser PID/Job ownership；
6. real launch、两周期 persistence、clean stop、EOF/crash/desktop-close、negative matrix
   的 summary/report/sidecar 路径和 SHA；
7. 完整测试命令、计数、失败后修正记录和未运行项目；
8. secret scan、residual process、临时目录、网络下载和外部副作用结论；
9. 修改文件、禁止范围 diff、未验证边界和下一 Gate 输入。

执行结束后等待主脑按成本受控协议审阅。fake Host 全绿、真实 browser 单次打开、
standalone M2-W 旧 receipt 或执行 Agent 自报 `passed` 都不能单独关闭 M3-WI。
