# Agent 任务路由与自动化工作流

用户只用自然语言描述任务（例如"重新设计创建 Silo 的 UX"、"修复 Camoufox 启动异常"、"检查安装流程有没有 bug"）。
Agent 自己完成：判定 lane → 创建任务工作区 → 按边界修改 → lane 级验证 → scope 检查 → 交给 integration。
不要求用户指定 worktree、Vault、端口或允许修改的目录。

事实源是 [scripts/agent-task.mjs](../scripts/agent-task.mjs) 顶部的 `LANES` / `RESTRICTED` / `SHARED` 配置；本文解释工作流，配置变更必须改脚本并同步本文。

## 标准流程

```bash
# 1) 判定 lane（见下表），在主检出（primary checkout）创建任务工作区；
#    start 会 fetch/prune origin，并要求本地 baseline/dev 与 origin/baseline/dev 精确相等，
#    与当前 shell 在哪个分支无关
node scripts/agent-task.mjs start --lane ui --task "重新设计创建 Silo 的 UX"
#    输出 JSON：branch / worktree 目录 / vault / port / baseline，以及下一步命令

# 2) 进入任务工作区并安装依赖
cd .verisilo-worktrees/<dir>
pnpm install

# 3) 按 lane 需要，用 start 输出的确切命令启动 preview 或真实实例（已含独立 Vault 与端口）

# 4) 工作、小步提交

# 5) 结束前，两项都必须通过
node scripts/agent-task.mjs verify   # lane 最小充分验证；exit 1=失败
node scripts/agent-task.mjs check    # exit 0=通过 · exit 2=scope violation · exit 3=WORKSPACE CONTAMINATION

# 6) 提交并发布自己的 task branch；publish 只允许非强制 fast-forward
git add -A && git commit -m "..."
node scripts/agent-task.mjs publish

# 7) integration 用 list 发现 task；远端 branch 是可审计输入
node scripts/agent-task.mjs list
```

`check`/`verify` 必须在任务 worktree 内运行：当前 git toplevel 与任务元数据所在目录不一致时直接报错，
不会在错误目录静默继续。`start` 是幂等的：同一 lane + 相同任务描述会复用已有分支与工作区
（端口被占时会自动重分配）；但若 canonical baseline 已被推进，旧任务的 metadata 与新 baseline 不一致，
start 会拒绝复用并说明差异——继续旧任务直接进 worktree，开新任务换描述或 `--name`。
任务文本哈希进入分支名（`agent/<lane>/<slug>-<hash6>`）；中文任务请用 `--name <英文短slug>` 得到可读分支名。

放弃或清理一次性任务：`git worktree remove --force .verisilo-worktrees/<dir> && git branch -D agent/<lane>/<branch>`；
Windows 上 node_modules 的 junction 可能导致目录残留，再 `rm -rf .verisilo-worktrees/<dir>` 即可
（pnpm 全局 store 使用硬链接与独立链接计数，删除任务工作区不影响它）。

## Lane 判定

Lane = 责任与修改边界；Task（worktree）= 一次实际工作。**同一 lane 可以同时有任意多个任务 worktree**，互不冲突；不要假设一个 lane 只有一个分支。

| Lane          | 主要产物落在哪                                           | 任务示例                                               | 不选它的信号                                      |
| ------------- | -------------------------------------------------------- | ------------------------------------------------------ | ------------------------------------------------- |
| `ui`          | 界面、表单、交互、文案、样式、预览                       | 重新设计创建 Silo 的 UX；新增预览场景                  | 需要改后端 DTO 或业务行为                         |
| `core`        | 桌面业务层、领域模型、Tauri/CLI 入口、EngineAdapter 接入 | 修复创建 Silo 的 Vault 写入顺序；CLI 输出问题          | 只动前端展示，或缺陷在 Python Host                |
| `host`        | Python Host、Camoufox 补丁、engine package 构建脚本      | 修复 Camoufox 启动异常（定位在 host_v1）；更新补丁系列 | 缺陷最终落在 Rust launcher/adapter —— 那是 `core` |
| `qa`          | 复现步骤、验收测试、evidence（不改产品代码）             | 检查安装流程有没有 bug；对某候选回归                   | 已定位修复方案 → 修复任务回 owning lane           |
| `integration` | 跨层、共享契约、汇总开发状态                             | NetworkProfile 加字段贯通前后端；合并本批任务供 QA     | 单一层内可完成 → 用对应 lane                      |

判定规则：

1. 按**主要修改/产物**归属选 lane，不按"谁报告了问题"。QA 发现的 bug，其修复任务属于 owning lane。
2. 预期改动会落在哪个 lane 的 allow 范围，就选哪个；明确跨层才选 `integration`。
3. 不确定时：先做只读调查（不 `start`），定位主修改层后再 `start`；或按当前最佳判断 `start`，结束时 `check` 会告诉你是否越界——回退或升级，不硬塞。
4. 不要为了"先跑起来"把任务塞进错误的 lane；换 lane = 回主检出重新 `start`，把已完成部分迁移过去。

## Canonical baseline（B0 → B1）

所有任务从同步后的 remote canonical baseline 分叉，baseline 绝不隐式等于"某次执行 start 时
shell 所在分支的 HEAD"：

```text
origin/baseline/dev = B0
baseline/dev = B0（本地工作引用，必须与上行精确相等）
├─ agent/ui/...      （worktree，从 B0 分叉）
├─ agent/core/...    （worktree，从 B0 分叉）
├─ agent/qa/...      （worktree，从 B0 分叉）
└─ integration 汇总并验证通过
   ↓ 显式推进（唯一的推进方式）
baseline/dev = origin/baseline/dev = B1   之后的新任务统一从 B1 开始
```

- canonical identity 是 remote ref `origin/baseline/dev`；本地 `baseline/dev` 是工作引用，正常状态必须精确相等。
- 两者都可由 `git rev-parse` 解析并由 Git reflog/remote history 审计；没有数据库、daemon 或 registry。
- 新任务只能在 fetch/prune 后从两者 exact-equal 的 baseline 创建；remote 缺失或 local/remote 不一致时 `start` fail closed，不从任意 shell HEAD 启动。
- **baseline 只能由 integration 显式推进**：先 `baseline advance <sha|ref>`，再 `baseline publish`。发布只允许 fast-forward 或首次 bootstrap，禁止 force。
- 普通 task 完成 verify + check + commit 后运行 `node scripts/agent-task.mjs publish`；脚本 fetch 后核对 `origin/agent/...` 与本地 SHA。远端不能 fast-forward 时报告 `REMOTE_DIVERGENCE`，不覆盖。
- `codex/camoufox-m3-engine-adapter` 保留为稳定主线，但不再是新 task 的 canonical development source；`stable/checkpoint-*` 与历史 refs 不参与日常路由。不要执行 `git push --all` / `--mirror`。
- 查看当前指向与同步状态：`node scripts/agent-task.mjs baseline`。

## 修改边界（scope guard 与 contamination guard）

`check` 报两类不同的问题，不要混淆：

1. **Scope violation（exit 2）**——修改发生在任务 worktree 内，但越过了 lane 边界：
   - `RESTRICTED`：任何 lane（除 `integration`）都不可改，即使落在自己的 allow 里。包括：`packages/contracts/**`、`apps/desktop/src/desktop-api.ts`、根 `package.json`/`pnpm-lock.yaml`、`AGENTS.md`、顶层 `docs/*.md`、`.github/**`、路由脚本本身。
   - `SHARED`：默认拒绝，但可被 lane allow 覆盖（host 的构建/验证脚本、qa 的 `docs/qa/**` 与 `docs/acceptance/**`）。
   - `check` 对 `baseline..HEAD` + 未提交 + 未跟踪文件全量分类；exit 2 时二选一，不静默接受：
     1. **顺手修改** → `git restore --source=<baseline> --staged --worktree -- <file>`（未跟踪直接删除）；
     2. **任务天然跨层** → 在主检出 `start --lane integration` 拆显式跨层任务，不在本 lane 分支混入。
2. **WORKSPACE CONTAMINATION（exit 3）**——任务把修改写到了**主检出**（filesystem 级越界，例如从 worktree 用 `../../` 相对路径写进主检出）。`start` 会对主检出的 dirty 状态做快照，`check` 用 `git status --porcelain -uall` 与快照对比：
   - 只报**新增**条目；任务启动前已有的 dirty 状态不会被归责给当前任务（已知限制：已存在条目的进一步内容变化无法区分，不误报）。
   - 发现污染时**不自动删除或覆盖**：属于本任务的越界写入→在主检出回退对应文件；属于用户或其他任务的合法修改→如实上报，不要动它。

**共享契约显式处理**：`packages/contracts`、`desktop-api.ts`、Rust DTO、Host 协议、数据格式的变化，必须先由一个 `integration` 契约任务做小步提交，调用方任务随后跟进适配；任何 lane 不得私自定义同一个字段。

## 最小充分验证（lane 级）

命令以脚本 `LANES[...].verify` 为准（`verify` 子命令自动执行）：

| Lane          | 自动化验证                                                                          | Agent 补充义务                                                                                    |
| ------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `ui`          | desktop check + desktop test                                                        | 用 preview 场景人工核对受影响交互；preview 是 UI 证据，不是 runtime Gate                          |
| `core`        | desktop cargo check + harness `application::` tests                                 | 触到窗口/托盘/进程路径时补 owning module focused tests                                            |
| `host`        | package contract + page command 测试                                                | `test_identity_artifact.py` 需 numpy（有条件则跑）；内核/包/指纹结论必须来自真实 runtime evidence |
| `qa`          | （无自动化命令）                                                                    | 验证=证据：复现步骤 + 实际观察 + 针对的确切候选版本；修复回 owning lane                           |
| `integration` | 递归 check/test、两个 crate 的 cargo check/test、Host 测试、脚本自测（Pre-RC 不含 desktop production build；排除依赖用户本机 provider inventory 的 live test） | 完整自动化之后，真实安装与用户旅程验收仍按 acceptance 流程在专用环境对确定候选执行 |

不要把"配置声明/测试通过/编译成功"冒充尚未取得的 runtime/product Gate；lane 验证只覆盖其名称所指的范围。

## 运行层级（Mode A / B / C）

lane 级 verify 之外，运行验证实例时用「足以验证当前修改的最低成本运行层级」
（三档完整定义见 [development-worktrees.md](development-worktrees.md) 的「三档开发循环」）：

- **Mode A — UI Preview**：`node scripts/dev-desktop.mjs <name> --port <port> --preview`。
  纯 UI/UX 修改（样式、布局、组件、文案、表单 UX、loading/error/empty/running 状态）。
  真实组件 + Mock API + Vite HMR；不启动 Rust backend，不读真实 Vault。
- **Mode B — Desktop Dev**：`node scripts/dev-desktop.mjs core --port <port> --vault <vault>`。
  触到 Tauri command、application、Vault/runtime 或前后端集成时使用；前端 HMR、
  Rust 增量编译后自动重启，不构建 installer。
- **Mode C — RC / Release**：只有 installer/安装行为变化、production 打包行为或发布验收
  才进入，且必须有用户明确打开 RC/release gate；走专用环境 acceptance 流程，不与开发实例混用。

`ui` / `qa` 等纯前端 lane 默认 Mode A；`core` / `host` 及跨层任务默认 Mode B；
不要为 Preview 或 Tauri dev 能覆盖的修改构建或安装 release artifact。

## CLI-first 与 RC 用户 gate

- 能由 CLI + real backend 诚实验证的 Vault、Silo、生命周期、Managed、Network、持久化与恢复，
  优先用 CLI；这不是第二套 desktop/browser automation。
- CLI 无法表达某个行为时，记录 coverage boundary，不自动把整轮 QA 判为 BLOCKED，也不为每个
  UI/browser 场景扩 CLI。需要视觉、交互、文案或状态核对时使用 Vite HMR/Preview；Mock/Preview
  evidence 不能冒充 runtime evidence。
- 只有 CLI/HMR 都不能诚实验证且确实需要真实 Tauri/runtime 集成时才进入 Desktop Dev；仍不
  install、package 或 release build。
- Agent 不得因“看起来 bug 很少”、代码能 build 或静态检查通过而自行开始 package、RC、installer
  或 clean Windows acceptance。必须由用户通过新指令明确打开 RC/release gate；Pre-RC 的 QA
  收敛信号只用于 release-readiness 判断，不是自动触发器。

## Integration 工作流

1. 各任务 worktree 完成 verify + check 后提交，并用 `node scripts/agent-task.mjs publish` 发布自己的
   `agent/<lane>/...` branch；输入必须可从 origin fetch 到，不能以本地孤立 commit 代替。
2. `start --lane integration --task "..."` 创建集成工作区；integration fetch 后只合并已核对的 remote task SHA。
3. 逐个 `git merge --no-ff origin/agent/<lane>/<branch>`；冲突按任务归属 lane 的 owning code 原则解决，
   契约冲突退回显式契约任务。
4. 运行迁移后的 integration verify（Pre-RC 不执行显式 desktop production build），再做组合 smoke、scope、
   contamination 和最终 diff 检查。
5. 验证通过后由 integration 执行 `baseline advance <集成结果 SHA>`，再执行 `baseline publish`；fetch 后必须
   证明 `baseline/dev == origin/baseline/dev`。发布只允许 fast-forward/首次 bootstrap，禁止 force。
6. 当前产品阶段是 Pre-RC product stabilization。只有用户明确打开 RC/release gate 且 release-readiness
   条件满足后，才冻结 source-bound RC；随后安装/覆盖安装/用户旅程验收在专用环境执行，不与开发实例混用。

## 运行隔离与共享资源

| 资源                                 | 规则                                                                                            |
| ------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Vault                                | 每任务独立（start 自动生成并校验 ≤32 位小写），绝不使用 `default`；通过 `--vault` 传入 dev 实例 |
| 端口                                 | 每任务独立 Vite 端口（15400 起，start 探测空闲 + 跳过其他任务已声明端口）                       |
| node_modules / Rust target / staging | 各工作树自有，不跨树链接                                                                        |
| Mihomo                               | 同一 controller/selector group 会互相切节点：并行网络测试用独立实例或串行                       |
| engine package                       | 固定包只读复用；新候选写新目录，不覆盖他人正在验收的包                                          |
| 安装/卸载                            | 由验收任务在专用环境执行，工作树名称不隔离安装目录与系统注册                                    |

## 环境注意

- Node ≥22（本机 fnm：`$APPDATA/fnm/node-versions/v22.23.2/installation`）；pnpm 11 可经 `corepack pnpm` 调用。`verify` 会自动在 `pnpm` 与 `corepack pnpm` 间探测；Git Bash 里 pnpm shim 可能因路径改写不可用，属已知情况。
- Host `test_identity_artifact.py` 依赖本机 python 的 numpy，当前环境缺失（非阻塞，按条件运行）。
- 已知偶发 flake：desktop Rust 全量 lib 测试中的 fake Host 一秒握手超时；对失败项单独串行重跑确认后再下结论，并如实记录。

底层约定（代码归属表、共同基线原则、合并规则）见 [development-worktrees.md](development-worktrees.md)。
