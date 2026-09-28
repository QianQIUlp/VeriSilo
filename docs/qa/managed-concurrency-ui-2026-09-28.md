# 本地 Managed 双会话：UI Preview 验收

状态：**模拟交互通过**。本页只记录真实 React 界面与 Mock API 的显示、目标选择和迟到响应；不据此声称浏览器进程、Profile、代理出口或冷备份文件通过原生验收。Preview 入口是 `http://127.0.0.1:15485/preview.html`，相关场景为 `concurrent`、`concurrent-slow`、`concurrent-fault`。页面明确标示模拟数据，刷新会重置场景。

| 场景 | 直接观察 |
| --- | --- |
| 双会话与上限 | `concurrent` 初始 A 运行；启动 B 后，A/B 在选择栏都显示运行中。选择 A、B 或 C 不改变 A/B 的运行标记；C 的打开按钮被禁用并解释从后端状态读取的 2 个上限。停止 A 后 B 仍显示运行；重新启动 A 后两者再次同时显示运行。 |
| 按目标冷恢复 | B 运行、A 停止时，选择 B 的完整冷备份显示当前会话阻止；选择 A 可完成模拟备份、检查、勾选确认和原身份恢复。恢复后 A 未运行，旧观察显示为“最后已知”；B 仍显示运行。此处没有产生真实 archive。 |
| 迟到响应与独立操作 | `concurrent-slow` 中，A 的重新检查延迟 4 秒。在它等待期间切换到 B 并停止 B；A 的结果随后以 A 的名称显示，选择仍停在 B，B 仍未运行，A 仍运行。B 的停止按钮没有被 A 的 busy 状态禁用。 |
| 单侧失败 | `concurrent-fault` 中，B 的重新检查模拟代理故障并释放 B 的当前会话。B 显示“检查未通过”，当前身份观察为 Unavailable，重新打开和浏览器检查可操作；A 继续显示运行。提示明确说 B 浏览器已关闭，不再要求点击不存在的“结束会话”。这不验证真实网络路由或无泄漏。 |
| 全局锁定 | A/B 显示同时运行时点击锁定，界面进入 Vault 锁定页，当前会话与敏感界面状态从 UI 清除。进程清理由原生验收验证。 |

实现上，界面用 `DesktopStatus.sessions` 的 `siloId` wrapper 给每张 Silo 卡、身份详情、停止、重新观察和冷恢复定目标；`focusedSiloId` 只控制选中状态。单 Silo 操作各自标记 busy；Vault 锁定使所有未完成 UI 回调失效。无残留占用的失败记录显示失败但不占并发名额，仍由后端作最终启动守卫。`activation` 为多会话时的 `null`，界面不从集合取第一条冒充唯一会话。Preview A/B/C 使用不同的模拟 Profile 目录、Seed 和 Artifact 摘要。

最后一轮受影响验证：`pnpm --filter @verisilo/desktop check` 通过；focused Vitest 包含 `app-copy`、`formatters`、`runtime-status`、`SiloList`、`CurrentSessionIntegrity`，5 个文件、63 项通过。Preview 没有检查真实 Cookie、localStorage、IndexedDB、代理端点、Host 健康监视、恢复文件或应用退出；这些结论只从同任务的 CLI/API 与原生 Windows 浏览器证据取得。
