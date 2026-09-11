# VeriSilo Agent routing

接到一个开发/QA/集成任务时，不要向用户索要 worktree、Vault、端口或 baseline：
按 [Agent 任务路由工作流](docs/agent-task-routing.md) 自主完成 ——
判定 lane（`ui` / `core` / `host` / `qa` / `integration`）→
`node scripts/agent-task.mjs start --lane <lane> --task "<任务>"` 创建隔离任务工作区
（脚本先 fetch/prune `origin`，并要求本地 `baseline/dev` 与 canonical
`origin/baseline/dev` 精确相等）→ 在边界内修改 → `verify` + `check` 通过 → 提交并发布自己的 task branch。
`check` 报两类不同问题：scope violation（worktree 内越界修改，exit 2）与
WORKSPACE CONTAMINATION（主检出被污染，exit 3）。
`origin/baseline/dev` 是 canonical development source；本地 `baseline/dev` 是其工作引用，
正常状态必须精确相等。task/integration branch 完成 verify + check + commit 后按 exact refspec
推送到 `origin`，fetch 后核对远端 SHA；禁止 force push、`git push --all` 和 `git push --mirror`。
单 lane task 在「仍从当前 canonical baseline 分叉、baseline 未被推进、verify 已在当前 HEAD 通过、
无 RESTRICTED/共享契约/越界改动、worktree clean 且 branch 已发布」时，可用
`node scripts/agent-task.mjs promote` 直接 fast-forward 推进 canonical baseline；baseline 已被
其他任务推进时返回 `PROMOTION_REQUIRES_INTEGRATION`，把 branch 交给 integration，不要自动
merge/rebase。integration 合并多个已验证 task branch 时默认轻量验证（remote SHA/ancestry/merge/
冲突/scope/`git diff --check` + 与本次 composition 直接相关的 focused tests），共享契约、
lockfile、workflow/config、同 seam 多任务、冲突解决或 QA/RC 候选才用 `verify --full` 跑完整矩阵。
baseline 只能由 integration `baseline advance`/`baseline publish`，或合格单 task 的 `promote`
显式推进。
`codex/camoufox-m3-engine-adapter` 保留为稳定主线，但不再是新 task 的 canonical development source。
Lane 范围、修改边界与验证命令的唯一事实源是 [scripts/agent-task.mjs](scripts/agent-task.mjs) 顶部配置。

任务拆分默认原则：

- **Implementation-authorized task**：用户已授权的修复按 `diagnose → minimal fix → focused
  validation` 在 owning lane 单任务完成；不默认拆 `QA reproduction → fix task`。只有用户明确
  要求独立 QA、ownership 仍未知且调查本身有价值、或需要独立 acceptance 时才用独立 QA task。
- **Cross-layer feature**：天然横跨 Host/Core/UI/contracts 的 coherent product slice 用一个
  integration task 实现并在完成后推进 baseline；不拆 `integration implementation → second
  integration merge task`。
- **Parallel batch**：并行 tasks 默认积累到一个自然 integration batch，不要求每个 task 完成
  立刻推进 baseline。

并行开发、工作树隔离或桌面结构调整的手工细节先读
[模块边界与开发入口](docs/development-worktrees.md)，再读取实际 owning code/test。

运行验证实例时用「足以验证当前修改的最低成本运行层级」（三档定义见
development-worktrees.md「三档开发循环」）：纯 UI/UX 修改用 UI Preview
（真实组件 + Mock API + Vite HMR）；触到 Tauri command、application、Vault/runtime
或前后端集成用 Tauri dev（前端 HMR、Rust 增量编译自动重启）；只有 installer/安装行为、
production 打包行为或发布验收才进入 RC installer 流程。普通 Pre-RC verify 不执行显式
desktop production build；不要为 Preview 或 Tauri dev 能覆盖的修改构建或安装 release artifact。
能由 CLI + real backend 诚实验证的 Vault、Silo、生命周期、Managed、Network、持久化和恢复优先
用 CLI；CLI 无法表达的视觉/交互再用 HMR/Preview，并记录 coverage boundary。Mock/Preview evidence
不能冒充 runtime evidence。Agent 不得自行打开 RC/release gate；必须有用户明确的新指令。

## 默认读取路径

Camoufox / Managed Identity 普通任务先读：

1. [Camoufox 当前状态](docs/camoufox-program-status.md)中的“当前下一任务”；
2. 实际 owning code/test；只有状态页明确要求独立 active contract 时才再读取它。

Standard Silo、EngineAdapter 或环境后端任务先读各自 owning 文档/代码；只有它实际依赖
当前 Camoufox Gate 时才读 Camoufox 状态页。

只有任务可能改变产品语义、架构路线或长期能力边界时，才额外读取：

1. [身份平台北极星](docs/identity-platform-north-star.md)；
2. [Camoufox-first 决策](docs/camoufox-managed-engine-decision.md)。

只有在委派复杂工作、创建新 Gate 或审阅外部 evidence package 时，才读取
[Agent 协作协议](docs/agent-operating-model.md)。历史任务合同、旧 run 和 superseded
checkpoint 不属于默认上下文；仅在调查对应事实时按状态页链接读取。

若多个事实源真正冲突，权威顺序是：产品北极星 → 已接受架构决策 → 当前任务合同
→ 当前状态 → 实现与直接证据。历史合同中的旧“当前状态”不覆盖状态页标明的新状态。

## 成本与验证

使用能解决当前不确定性的最低充分流程：

- 文档或局部机械修改：只检查相关引用、格式和 diff；
- 普通代码修复：检查 owning seam/callers，做最小修复和 focused tests；
- 浏览器、引擎构建、证据 claim、发布或难回滚操作：使用冻结输入、直接 evidence 和明确停止条件；
- 产品/架构、安全或数据边界变化：先做显式决策。

不要因为历史阶段曾使用 one-shot、manifest、全量 hash 或完整回归，就把它们复制到不需要
这些保证的新任务。稳定且相关状态未变化时复用既有证据；只有矛盾、新失败或关键输入变化
才扩大检查。详细分级见 Agent 协作协议。

**Evidence remains valid until a relevant input changes**：Host runtime evidence 不因无关
site change 失效；site build evidence 不因无关 Rust change 失效；core lifecycle test 不因
无关 Host source change 自动失效。integration 只验证 composition 新增的不确定性，不重新
证明没有变化的事实。

Immutable/one-shot 只约束已冻结的 attempt 及其 evidence。`failed` 或 `inconclusive`
必须保留且不得重试未变输入或选样本，但它不自动结束所属工程 Gate，也不自动创建
recovery Gate。在原授权和风险范围内，修复已确认或证据支持的因果 blocker，完成最低
充分验证后再生成清晰版本化的新 attempt。

## 不可弱化的产品边界

- Standard Silo 长期保留；当前产品阶段、候选状态和下一任务以 [Camoufox 当前状态](docs/camoufox-program-status.md) 为准，不在本文件硬编码易变化阶段；
- 新产品/功能提案默认必须说明其是否增强 Identity Integrity、Execution Integrity、Network Integrity、Runtime Evidence 或 Attribution；不因竞品 feature parity 自动进入 roadmap；
- Profile、Identity Artifact、Engine、Network Policy 与 Evidence 保持不同生命周期；
- 原生 Windows 专属结论不能由 Linux、WSL 或 Wine 结果替代；
- 不同时扩张 Controlled Chromium、WSL、VMware、Hyper-V 与 Remote；
- 不混用 `configured`、`applied`、`observed`、`verified` 与 `unavailable`；
- 配置声明、测试通过或编译成功都不能冒充尚未取得的 runtime/product Gate。
