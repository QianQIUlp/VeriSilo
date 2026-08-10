# Standard Silo Windows 可运行垂直切片

状态：**冻结执行合同**。精确起始 commit 由主脑派发时指定。

## 目标

让普通用户在原生 Windows 上不理解 EngineAdapter、WSL、代理或 Profile 目录，也能
完成一条最短 Standard Silo 旅程：

```text
初始化 Vault
→ 使用自动发现的 Chrome 或 Edge
→ 创建 Local + Direct Standard Silo
→ 启动独立 Profile
→ 用户关闭浏览器
→ 桌面端回到空闲
→ 再次启动同一 Silo，并保留该 Profile 的状态
```

本任务优化现有产品路径，不创建新的浏览器后端。Standard 只声明独立 Profile、登录
状态和本地生命周期；硬件与浏览器指纹继续写成 `native`、`inherit` 或
`unavailable`。

## 产品结果

1. “创建 Silo”的默认主路径是 Windows 本机、Direct 和一个已自动发现的系统浏览器；
   用户只需命名并确认即可创建。
2. WSL、代理、手工浏览器路径和其他高级配置不占据默认主路径；现有能力可以放在
   明确的高级入口中，但后端语义不得改变。
3. 主路径和 Silo 卡片明确说明：网站数据与登录状态独立，设备/浏览器指纹仍跟随
   本机；不得使用“受控身份”“指纹隔离”或 `verified` 误导 Standard 用户。
4. 启动后清楚提示用户关闭该 Silo 的浏览器窗口即可停止；桌面端核对真实进程与
   Profile lock 后回到空闲，不提供会误杀日常浏览器的“强制停止”按钮。
5. 产出一个可本机运行的 unsigned desktop-only Windows preview。它不是签名发布包，
   不得称为正式 installer 或 shipped 产品。

## 允许范围

- `apps/desktop/src/**` 的 Standard Silo 首次创建、概览、状态和诚实文案；
- `apps/desktop/src-tauri/src/**` 中仅与 stock Chrome/Edge、Vault、独立 Profile 和本地
  生命周期直接相关的最小修复；
- 既有 desktop tests、`windows_acceptance_driver.rs`、
  `tests/windows/Invoke-VeriSiloWindowsE2E.ps1` 和 session-site fixture 的必要复用；
- 本任务文档、现有人工验收指南的同步。

先运行和理解现有路径，只修复实际阻断或完成上述明确产品结果；不得为了“有改动”而
重写已成立的代码。

## 禁止范围

- 不修改 `apps/camoufox-host/**`、Camoufox/Controlled Chromium adapter、Managed
  Artifact 或任何 M0–M3 evidence；
- 不实现或扩张代理、Mihomo、PAC、WSL、Remote、Sandbox、Hyper-V 或安装签名；
- 不增加 production dependency，不修改公开协议或数据语义；
- 不新建 runner、freezer、finalizer、evidence schema 或多层 manifest；
- 不接触、删除或迁移用户现有 VeriSilo、Chrome、Edge 数据；不关闭用户日常浏览器。

## 环境与安全

- 原生交互式 Windows；使用当前原始 checkout，不创建 worktree；
- 起点必须是主脑派发的 clean checkpoint；发现任何非本任务改动立即停止；
- 所有自动化 app data、Profile、报告和 session fixture 都位于唯一的临时/任务目录；
- 启动 preview 时用 run-owned `LOCALAPPDATA` 隔离真实 `%LOCALAPPDATA%\VeriSilo`；
- 只使用已安装且自动发现的 Chrome 或 Edge；至少一个可用即可，不安装新浏览器；
- 不使用真实账号、口令、Cookie、代理秘密或真实业务网站。

## 验收

执行 Agent 只运行一次最终候选验收，不为单项失败挑选历史绿色结果：

1. UI/contract tests 证明默认主路径为 Local + Direct + 自动发现浏览器，高级配置需
   显式展开，Standard 边界文案存在。
2. 复用现有 Windows desktop-core acceptance driver，在 run-owned root 上通过真实
   Vault、Silo 创建、独立 `user-data-dir`、stock launch、Profile lock、单活拒绝和
   精确进程恢复。
3. 复用现有 Windows browser E2E 的一个已安装浏览器单元，确认 A/B Profile 隔离，
   Cookie/localStorage/IndexedDB 跨冷启动保留，默认 Profile 未改动。缺少第二种浏览器
   不阻断本切片。
4. 构建 unsigned desktop-only Tauri preview；用 run-owned `LOCALAPPDATA` 启动真实
   executable，确认窗口进程可启动且无立即崩溃。完整点击旅程若没有可靠桌面自动化，
   必须明确交回人工 smoke，不得伪造成自动通过。
5. 最终只运行一次常规 `pnpm check`、`pnpm test`、`pnpm build`，以及 desktop Rust
   fmt/test/clippy；复用仓库锁定依赖，不更新 lockfile。
6. 结束时工作树 clean，无测试 Chrome/Edge、fixture、Tauri 或编译进程残留；用户日常
   浏览器和默认 Profile 未被操作。

机器可读结果复用现有 acceptance receipt/summary；本任务只返回一个简短执行摘要和
preview 路径，不创建新的证据供应链。

## 停止条件

- 需要修改 Managed Engine、网络后端、发布签名、公共协议或新增 production dependency；
- 只能通过关闭用户日常浏览器、读取真实 Vault/Profile 或放宽安全边界才能继续；
- 真实 stock browser 或桌面 core 暴露 Profile 串用、默认 Profile 修改、误杀无关进程；
- 同一最终候选的核心旅程失败。失败时保留现场并返回主脑，不拆分新的子 Gate。

## 交付

- `passed`、`failed` 或 `blocked`，不得自行宣布产品 Accepted；
- 起点、最终 commit/tree、修改文件和工作树；
- 用户路径的实际变化与边界文案；
- desktop-core receipt、Windows browser summary、preview executable/NSIS 路径；
- 常规验证计数、未完成的人工 smoke、残留进程与禁止范围核对。
