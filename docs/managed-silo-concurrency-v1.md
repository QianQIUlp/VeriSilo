# 本地 Managed Silo 有限并发 v1

起点：`0c93eddc1b8eb8636df58208616f849823d04b36`。目标是在同一桌面实例和 Vault 中同时使用两个本地 Camoufox 身份，保持 Profile、进程、网络、证据和恢复归属独立，增强 Execution/Network/Identity Integrity 与 Attribution。冷备份 v1 的未变能力继承 [73a6177b 验收](qa/managed-cold-backup-2026-09-28.md)。

## 产品与实现决策

- 并发仅允许本地 Managed Camoufox 的 Direct/固定代理组合，最多两个；唯一生产上限在 Rust `LOCAL_MANAGED_SESSION_LIMIT`，状态接口传给 UI。重复启动、第三个和不支持组合先拒绝，不关闭其他会话或降级直连。Standard、Mihomo/Clash、WSL、Remote 保留既有单会话边界。
- 复用 `RuntimeManager` 作为单会话所有者；新集合只负责按 Silo 找到对象、原子预占和短锁快照。每个对象独占 Host/浏览器、Profile lease、relay、health/runtimeId、恢复记录及 watchdog。只读引擎包可以共享。慢 I/O 只占目标对象，不占集合或 Vault 锁；状态读取可使用该对象最后发布的快照，不能借用另一对象的证据。
- 旧 `runtime/browser-session.json` 继续读取，新记录位于同目录按 Silo 命名；损坏、未知或尚未完成恢复的记录保留并阻止不安全启动。重启不承诺自动打开网页。
- `DesktopStatus.sessions` 为 `{ siloId, activation, websiteIdentity? }[]`，归属不依赖停止后会清空的 `activeSiloId`；`managedSessionLimit` 为集中上限。旧 `activation` 在唯一需管理会话时保持原对象，多会话时为 `null`，不得取第一条冒充唯一状态。无会话时保留 idle/旧单会话结束语义。
- 新增 Tauri `list_runtime_sessions` / `get_silo_runtime`，HTTP `GET /v1/sessions` / `GET /v1/silos/{id}/runtime`；启动、停止、重新观察和页面操作始终指定目标。CLI 支持集合及目标查询。Native Host/Companion 的无目标唯一查询在歧义时明确不可用，目标查询按 Silo/runtime 绑定，不接收错误归属的证据。
- UI 选择独立于运行集合，按 Silo 跟踪 busy 和异步响应；切换选择不改运行身份。Vault 锁定清所有敏感 UI 状态。
- 现有全局生命周期 reservation 变为全局写屏障/目标读屏障，目标对象自身串行操作。Vault 锁定、整库恢复、引擎维护和退出处理全部会话；单 Silo 冷备份只检查目标停机与占用，运行 B 时恢复 A 不能清除 B。备份格式和密码学不变。Vault 锁定沿用既有语义：全部会话撤销解密凭据和当前页面观察，依赖代理中继的会话失败关闭，Direct 浏览器不承诺强制关闭；锁定后页面 API 不可用。退出则关闭全部本地 Managed 会话。

## 验收与边界

一个 integration 任务完成设计、代码与验收。共享契约先小步提交，完整实现后执行一次必要 full integration 矩阵；后续修复只复验改变或失败部分。真实原生 Windows、一次性命名 Vault 与固定 Engine 验证双活持久 Cookie/localStorage/IndexedDB 隔离、独立重观察/停止/重启、本地可控固定代理路由与单侧故障关闭、运行 B 期间 A 冷恢复、上限/重复/锁定/退出与重启恢复归属。UI Preview 只验证交互、选择、迟到响应和错误状态。

本地代理测试不推导公网出口或完整无泄漏。沿用固定代理创建已有的必要出口观察；不新增生产依赖、改 Host 包、改全局 ACL、重建内核或打开 RC/installer/release。若这些边界确需改变，先报告具体必要性。

每项工作对应明确产品问题或必要不确定性，继承有效证据；修复跟随最低充分验证；无新证据/假设不昂贵重试；历史 Sandbox 与发布流程不是续接点；必须以必要产品行为得到直接证据判定完成。失败 attempt 原样保留，修复因果 blocker 后才产生新 attempt。最终 verify/check、发布 task 分支并快进发布 baseline，禁止强推或推进 main。
