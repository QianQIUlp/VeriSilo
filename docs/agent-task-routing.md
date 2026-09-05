# Agent 任务路由与自动化工作流

用户只用自然语言描述任务（例如"重新设计创建 Silo 的 UX"、"修复 Camoufox 启动异常"、"检查安装流程有没有 bug"）。
Agent 自己完成：判定 lane → 创建任务工作区 → 按边界修改 → lane 级验证 → scope 检查 → 交给 integration。
不要求用户指定 worktree、Vault、端口或允许修改的目录。

事实源是 [scripts/agent-task.mjs](../scripts/agent-task.mjs) 顶部的 `LANES` / `RESTRICTED` / `SHARED` 配置；本文解释工作流，配置变更必须改脚本并同步本文。

## 标准流程

```bash
# 1) 判定 lane（见下表），在主检出（primary checkout）创建任务工作区
node scripts/agent-task.mjs start --lane ui --task "重新设计创建 Silo 的 UX"
#    输出 JSON：branch / worktree 目录 / vault / port / baseline，以及下一步命令

# 2) 进入任务工作区并安装依赖
cd .verisilo-worktrees/<dir>
pnpm install

# 3) 按 lane 需要，用 start 输出的确切命令启动 preview 或真实实例（已含独立 Vault 与端口）

# 4) 工作、小步提交

# 5) 结束前，两项都必须通过
node scripts/agent-task.mjs verify   # lane 最小充分验证；失败=未完成
node scripts/agent-task.mjs check    # scope guard；exit 2=越界，不允许静默接受

# 6) 提交后任务即"可集成"；integration agent 用 list 发现并合并 agent/* 分支
node scripts/agent-task.mjs list
```

`start` 是幂等的：同一 lane + 相同任务描述会复用已有分支与工作区（端口被占时会自动重分配）。
任务文本哈希进入分支名（`agent/<lane>/<slug>-<hash6>`），同一描述永远得到同名分支；不同任务不会互撞。
中文任务请用 `--name <英文短slug>` 得到可读分支名。

## Lane 判定

Lane = 责任与修改边界；Task（worktree）= 一次实际工作。**同一 lane 可以同时有任意多个任务 worktree**，互不冲突；不要假设一个 lane 只有一个分支。

| Lane | 主要产物落在哪 | 任务示例 | 不选它的信号 |
|---|---|---|---|
| `ui` | 界面、表单、交互、文案、样式、预览 | 重新设计创建 Silo 的 UX；新增预览场景 | 需要改后端 DTO 或业务行为 |
| `core` | 桌面业务层、领域模型、Tauri/CLI 入口、EngineAdapter 接入 | 修复创建 Silo 的 Vault 写入顺序；CLI 输出问题 | 只动前端展示，或缺陷在 Python Host |
| `host` | Python Host、Camoufox 补丁、engine package 构建脚本 | 修复 Camoufox 启动异常（定位在 host_v1）；更新补丁系列 | 缺陷最终落在 Rust launcher/adapter —— 那是 `core` |
| `qa` | 复现步骤、验收测试、evidence（不改产品代码） | 检查安装流程有没有 bug；对某候选回归 | 已定位修复方案 → 修复任务回 owning lane |
| `integration` | 跨层、共享契约、汇总出候选 | NetworkProfile 加字段贯通前后端；合并本批任务出 RC | 单一层内可完成 → 用对应 lane |

判定规则：

1. 按**主要修改/产物**归属选 lane，不按"谁报告了问题"。QA 发现的 bug，其修复任务属于 owning lane。
2. 预期改动会落在哪个 lane 的 allow 范围，就选哪个；明确跨层才选 `integration`。
3. 不确定时：先做只读调查（不 `start`），定位主修改层后再 `start`；或按当前最佳判断 `start`，结束时 `check` 会告诉你是否越界——回退或升级，不硬塞。
4. 不要为了"先跑起来"把任务塞进错误的 lane；换 lane = 回主检出重新 `start`，把已完成部分迁移过去。

## 修改边界（scope guard）

- `RESTRICTED`：任何 lane（除 `integration`）都不可改，即使落在自己的 allow 里。包括：`packages/contracts/**`、`apps/desktop/src/desktop-api.ts`、根 `package.json`/`pnpm-lock.yaml`、`AGENTS.md`、顶层 `docs/*.md`、`.github/**`、路由脚本本身。
- `SHARED`：默认拒绝，但可被 lane allow 覆盖（host 的构建/验证脚本、qa 的 `docs/qa/**` 与 `docs/acceptance/**`）。
- `check` 对 `baseline..HEAD` + 未提交 + 未跟踪文件全量分类；exit 2 时二选一，不静默接受：
  1. **顺手修改** → `git restore --source=<baseline> --staged --worktree -- <file>`（未跟踪直接删除）；
  2. **任务天然跨层** → 在主检出 `start --lane integration` 拆显式跨层任务，不在本 lane 分支混入。

**共享契约显式处理**：`packages/contracts`、`desktop-api.ts`、Rust DTO、Host 协议、数据格式的变化，必须先由一个 `integration` 契约任务做小步提交，调用方任务随后跟进适配；任何 lane 不得私自定义同一个字段。

## 最小充分验证（lane 级）

命令以脚本 `LANES[...].verify` 为准（`verify` 子命令自动执行）：

| Lane | 自动化验证 | Agent 补充义务 |
|---|---|---|
| `ui` | desktop check + desktop test | 用 preview 场景人工核对受影响交互；preview 是 UI 证据，不是 runtime Gate |
| `core` | desktop cargo check + harness `application::` tests | 触到窗口/托盘/进程路径时补 owning module focused tests |
| `host` | package contract + page command 测试 | `test_identity_artifact.py` 需 numpy（有条件则跑）；内核/包/指纹结论必须来自真实 runtime evidence |
| `qa` | （无自动化命令） | 验证=证据：复现步骤 + 实际观察 + 针对的确切候选版本；修复回 owning lane |
| `integration` | 递归 check/test、desktop build、两个 crate 的 cargo check/test、Host 测试、脚本自测 | 完整自动化之后，真实安装与用户旅程验收仍按 acceptance 流程在专用环境对确定候选执行 |

不要把"配置声明/测试通过/编译成功"冒充尚未取得的 runtime/product Gate；lane 验证只覆盖其名称所指的范围。

## Integration 工作流

1. 各任务 worktree 完成 verify + check 后提交，形成可集成 commit。
2. `start --lane integration --task "..."` 创建集成工作区；`node scripts/agent-task.mjs list` 发现全部 `agent/*` 分支。
3. 逐个 `git merge --no-ff agent/<lane>/<branch>`；冲突按"任务归属 lane 的 owning code"原则解决，契约冲突退回显式契约任务。
4. 运行 integration verify（完整自动化套件）。
5. 形成确定 RC 候选后，安装/覆盖安装/用户旅程验收在**专用环境**由专门验收任务执行——不与任何开发实例混用。已有 RC1 证据保留，新候选单独标识。

## 运行隔离与共享资源

| 资源 | 规则 |
|---|---|
| Vault | 每任务独立（start 自动生成并校验 ≤32 位小写），绝不使用 `default`；通过 `--vault` 传入 dev 实例 |
| 端口 | 每任务独立 Vite 端口（15400 起，start 探测空闲 + 跳过其他任务已声明端口） |
| node_modules / Rust target / staging | 各工作树自有，不跨树链接 |
| Mihomo | 同一 controller/selector group 会互相切节点：并行网络测试用独立实例或串行 |
| engine package | 固定包只读复用；新候选写新目录，不覆盖他人正在验收的包 |
| 安装/卸载 | 由验收任务在专用环境执行，工作树名称不隔离安装目录与系统注册 |

## 环境注意

- Node ≥22（本机 fnm：`$APPDATA/fnm/node-versions/v22.23.2/installation`）；pnpm 11 可经 `corepack pnpm` 调用。`verify` 会自动在 `pnpm` 与 `corepack pnpm` 间探测；Git Bash 里 pnpm shim 可能因路径改写不可用，属已知情况。
- Host `test_identity_artifact.py` 依赖本机 python 的 numpy，当前环境缺失（非阻塞，按条件运行）。
- 已知偶发 flake：desktop Rust 全量 lib 测试中的 fake Host 一秒握手超时；对失败项单独串行重跑确认后再下结论，并如实记录。

底层约定（代码归属表、共同基线原则、合并规则）见 [development-worktrees.md](development-worktrees.md)。
