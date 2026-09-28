# Managed Silo 冷备份恢复：原生 Windows 开发验收

状态：**待运行**。本页只记录当前 integration candidate 的一次有界原生验收；执行结果以本页后续填入的 `result.json` 和确切 source SHA 为准。不会借用历史 Sandbox、RC 或安装验收结论。

## 要回答的问题

同一台 Windows、同一正常用户、同一命名 Vault 中，停机的 Managed A 能否把 Profile、Identity Artifact、Engine 绑定和 Silo 配置备份成加密 archive；在 A 的浏览器数据随后被改写后，恢复原 A 是否重新读出备份时的持久 Cookie、localStorage、IndexedDB，同时保留 B；错误的 archive hash 和口令是否拒绝且不改变当前 A/B。

执行器为 [`tests/windows/managed-cold-backup-acceptance.mjs`](../../tests/windows/managed-cold-backup-acceptance.mjs)。它通过真实 Local API 和现有 [`loopback-server.mjs`](../../tests/windows/fixtures/loopback-server.mjs) 操作浏览器页面。测试用一次性 `coldbk-*` Vault，Direct 网络，本地 `127.0.0.1` 站点，随机合成标记；先备份停机 A，再改写 A 的三种持久标记，验明拒绝不破坏，然后恢复并读回原标记，最后启动 B 读回 B 标记。本轮不对 sessionStorage 或 session Cookie 的冷启动行为下结论。Profile 目录由产品管理，执行器不直接复制、删除或修改 Profile 文件。

验收使用 normal-user LocalAppData 下的独立开发程序目录与已有 Engine package 的文件硬链接；不改包文件、全局 ACL、默认 Vault、已安装产品或历史 performance 开发目录。只构建 debug `verisilo.exe` 与 `verisilo-cli.exe`；CLI 后台服务不需要 Vite/UI server。执行器核验 `telecaster\qiu`、干净的候选 worktree 与显式 source SHA，并记录两个 debug exe 与 Engine manifest 的 SHA-256。archive、Vault 与结果留在本次专用目录供诊断；报告不保存口令、Local API token 或 fixture token。

成功边界：三个 API 返回的 archive/Silo/Artifact 摘要一致；A 的原标记经改写和冷恢复后重新读出；B 的元数据和标记不变；两种错误恢复均拒绝且当前 A 改写标记仍可读；所有 Managed 会话正常停止。失败保留原始 `result.json`，修复有证据支持的原因后才用新 run ID 重试。浏览器无法启动时先核对实际账户与 Engine 路径，不把工作树或 Codex sandbox 的既有 DACL 限制算作产品回归。

## 本次结果

待填：候选 source SHA、正常用户目录、`result.json` 路径、各关键步骤的结果与直接证据、任何未验证边界。该验收不是 production build、installer 或发布 Gate。
