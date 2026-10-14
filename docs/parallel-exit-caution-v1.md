# 并行同出口提醒 v1（Parallel same-exit caution）

起点：`e3a0ec8e4a92da13e470f3dae7ff7d48fba1db57`（baseline/dev 工作分支
`arena/2b566e80-verisilo`）。[本地 Managed Silo 有限并发 v1](managed-silo-concurrency-v1.md)
允许最多两个本地 Camoufox 会话同时运行；本切片补上它直接创造的归因风险面：
**两个长期身份在同一网络出口上并行时，网站有机会通过出口 IP 把它们关联起来**。
此前 UI 只有数量上限提示，没有任何共用出口提示。增强 Network Integrity 与
Attribution，不改变任何生命周期语义。

## 产品与实现决策

- 提醒**不阻止运行**：不修改启动条件、并发上限或停止语义；只对运行中的本地
  Managed Camoufox 会话展示一条 `role="note"` 提醒，并在未运行的本地 Managed
  Silo 将与现有会话形成配置面共用出口时，在启动按钮附近展示一条打开前提示。
- 两种依据严格分开，不混用生命周期词：
  - `configured`（按当前配置）：两个会话都是直连（同一本机网络出口），或
    fixed_proxy 的 scheme/host/port 完全一致（同一代理入口）。代理入口相同
    不等于实际出口相同（代理可能按凭据/规则分配不同出口），文案如实写明
    「实际出口以打开后的出口检查为准」。PAC 无法静态比较，不产生配置面提醒。
  - `observed`（按最近出口检查记录）：两个会话的 Silo 各有一条**未过期**的
    `SiloNetworkEvidence`，且最新记录的出口地址一致（大小写/空白归一化）。
    它来自历史 Companion 检查，文案明确「不是当前运行的实时验证」。
  - 同一对 Silo 只产出一条：能观测时优先观测，否则回退配置。
- 提醒对象是本地 Managed Camoufox 会话（并发受众、偶然关联成本最高的身份）；
  peer 可以是任何本地 Silo（含 stock），因为直连的 stock 会话与 Managed 共享
  同一本机出口。`stopped`/`failed`/`recovery_required` 会话不视为在承载流量；
  WSL/Remote 会话不参与比较。
- 观测新鲜度用分钟粒度时钟复核，记录过期后提醒自动回退到配置面（若配置面也
  成立）或消失；旧记录绝不冒充当前观测。
- 不新增依赖、不改 contracts schema、不改 Rust core/Host、不动引擎包与代理
  中继；出口地址只在 UI 内比较，不新增遥测或网络请求。

## 验证

- 纯函数 `apps/desktop/src/features/network/parallel-exit.ts`：23 个单测覆盖
  配置键归一化（大小写/scheme/port/PAC）、直连对、同入口代理对、直连-代理不
  提醒、观测优先与归一化匹配、过期记录回退与不冒充、非承载流量状态忽略、
  Managed 目标/stock peer 边界、非本地目标排除，以及打开前提示的各边界。
- 组件级：`SiloList.test.tsx` 新增 3 测——两个直连 Managed 并行时提醒出现且
  不阻塞按钮、直连+代理组合不提醒、未运行直连 Managed 在加入已运行直连会话
  前出现打开前提示。
- `tsc --noEmit` 通过；desktop 全量 vitest 190/190 通过。这些是 UI 层验证，
  不构成 runtime 证据，也不重新 qualify 并发或网络行为。

## 边界

本切片不声称任何网站关联行为已发生或必然发生；共用出口只是关联的必要条件
之一，UI 文案与文档都保持这一定位。Pre-RC verify 矩阵、安装验收、指纹
Gate、RC/release Gate 均不在本切片内。
