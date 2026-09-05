# 并行开发与模块边界

VeriSilo 保持单仓库、单桌面产品。工作树隔离源码；命名 Vault、端口和独立构建输出隔离开发实例。

Agent 的自动化任务流（lane 判定、worktree 自动准备、scope guard、lane 级验证、integration 汇总）
见 [Agent 任务路由工作流](agent-task-routing.md)，由 `scripts/agent-task.mjs` 驱动；
本文描述它依赖的底层约定与手工流程。

## 代码归属

| 工作                                 | Owning code                                                                 | 最小验证                                                       |
| ------------------------------------ | --------------------------------------------------------------------------- | -------------------------------------------------------------- |
| 页面、表单、交互                     | `apps/desktop/src/features/{silos,identity,vault,network,environments,cli}` | desktop check/test；受影响页面预览                             |
| 页面组装                             | `apps/desktop/src/App.tsx`                                                  | desktop check/build                                            |
| 跨页面会话、刷新、异步失效与操作协调 | `apps/desktop/src/workspace/useDesktopWorkspace.ts`                         | Vault UI tests；锁定后的界面检查                               |
| Standard 创建草稿与清理              | `apps/desktop/src/features/silos/useSiloDraft.ts`                           | Vault UI cleanup tests；创建页                                 |
| 共用业务操作                         | `apps/desktop/src-tauri/src/application/`                                   | core harness 的 `application::` tests                          |
| 窗口、托盘、Tauri 入口               | `apps/desktop/src-tauri/src/lib.rs`、`commands.rs`                          | desktop Rust check；相应原生窗口路径                           |
| CLI / HTTP 入口                      | `local_api.rs`、`bin/verisilo-cli.rs`                                       | desktop Rust check/test；本地 API 契约                         |
| 生命周期、包验证、网络、存储         | `launcher.rs`、`engine.rs`、`mihomo.rs`、`proxy_relay.rs`、`vault.rs`       | owning module focused tests                                    |
| Host 与内核                          | `apps/camoufox-host/` 中对应 Python、patch、lock                            | owning Host/patch tests；按受影响能力取得原生 runtime evidence |
| 用户验收与发布                       | `tests/windows/`、`docs/acceptance/`、既有 release scripts                  | 精确候选上的对应验收                                           |

`application` 不依赖 Tauri，也不启动本地 HTTP 服务。它持有 `DesktopCore`，接收明确的数据根和资源根。
Tauri 命令与 Local API 调用同一份业务操作；`lib.rs` 持有窗口层状态、Vault 实例锁和 HTTP server。
core harness 通过 path modules 编译生产业务源码，无需维护副本。

前端通过 `desktop-api.ts` 调用后端；功能组件不反向导入 `App.tsx` 或 workspace coordinator。
创建、编辑、托管表单、Vault 设置与环境面板的局部状态随各自组件维护。
Vault 会话失效、跨页面数据刷新和操作顺序仍由 coordinator 维护，锁定时统一清理创建草稿。

## 共同基线

Agent 任务的共同分叉点是 canonical baseline ref `refs/heads/baseline/dev`
（查看：`node scripts/agent-task.mjs baseline`；只能由 integration 在一轮汇总验证通过后
用 `baseline advance` 显式推进，见 [agent-task-routing.md](agent-task-routing.md)）。
手工流程也应从同一 ref 创建工作分支，不要从任意本地 HEAD 分叉。

新工作树只包含提交中的文件。分支前先把当前需要的源码（包括新增文件）提交，
不要把本机生成的 Host 构建目录、浏览器包或旧 evidence 混入源码提交。
已有未提交工作不能通过从旧 HEAD 创建工作树自动带过去。

从共同 checkpoint 创建 `qiu/ui`、`qiu/core` 等工作分支；每个工作树分别安装依赖，保留自己的
`node_modules/.vite`、Rust target、staging 和 release 输出。不要把这些可写目录链接到另一工作树。

## 启动

在各自工作树根目录执行：

```powershell
# UI 开发：浏览器内模拟数据，不启动 Tauri 或真实浏览器内核
pnpm desktop:worktree ui --port 1421 --preview
# 打开 http://127.0.0.1:1421/preview.html

# 真实桌面开发：使用 dev-core Vault；不使用 default Vault
pnpm desktop:worktree core --port 1422
# agent 任务传入自己的独立 Vault（agent-task.mjs start 会给出确切命令）
pnpm desktop:worktree core --port 15437 --vault ui-create-silo-ux-3f9a2c

# 查看实际参数而不启动
pnpm desktop:worktree core --port 1422 --dry-run
```

名称与端口都由调用者明确分配；同时运行时两者都不能重复。
脚本同步设置 Vite 端口和 Tauri devUrl，并通过应用参数传入 `--vault dev-<name>`。
它不修改系统环境变量、不安装产品、不复制用户 Profile，也不创建 engine package。
真实 Managed launch 仍需既有构建/封装流程准备对应资源，不能用 UI 预览替代。

预览页面提供概览、空列表、锁定、首次使用、运行中、启动失败与独立托管创建表单。
它使用真实组件和内存中的示例 API；未模拟操作明确报错。刷新即丢弃模拟数据。
托管表单的模拟提交只用于交互检查；桌面出口检查按钮仍是明确同意后执行的真实公开网络查询。
`preview.html` 是开发入口，默认 production build 只打包 `index.html`，不加载模拟 API。

## 合并与共享资源

- `desktop-api.ts`、`packages/contracts`、Rust DTO、Host 协议、数据格式变更先形成一个小的共同提交，
  调用方更新后再并行实现，不在多个工作树分别定义同一个字段。
- `shared/` 和公共 `styles.css` 由一个明确任务统筹；功能工作优先改各自组件。
  本次保留 CSS cascade，不用大规模样式重排改变现有界面。
- 同一个 Mihomo controller/selector group 会共享节点状态。需要修改节点的并行测试使用独立实例，
  或串行执行；命名 Vault 不会隔离外部代理程序。
- 固定 engine package 可作为只读输入复用；新包使用新的输出目录，不能覆盖别人正在验证的包。
- 安装、覆盖安装与卸载由一个验收任务在专用环境执行；不能靠工作树名称隔离安装目录和系统注册。
- 测试工作提交复现与对应测试；产品修复回到 owning module，避免一个无边界的“修所有 bug”分支。

## 验证命令

```powershell
pnpm --filter @verisilo/desktop check
pnpm --filter @verisilo/desktop test
pnpm --filter @verisilo/desktop build
node --test scripts/dev-desktop.test.mjs
cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib application::
cargo check --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
```

其余逻辑变更运行对应模块测试。原生 Windows 浏览器、包、Profile、网络或安装结论继续遵循
[当前状态](camoufox-program-status.md)与 owning acceptance；这次结构调整不更新任何产品 Gate。

本次暂不进一步拆 `launcher.rs` / `engine.rs` / `vault.rs` 为 crates，也不改 Host 协议与内核补丁。
它们已可以与 UI、桌面业务入口分别工作；后续出现具体文件争用时再沿 owning seam 提取内部模块。
