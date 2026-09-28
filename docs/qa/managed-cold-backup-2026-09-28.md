# Managed Silo 冷备份恢复：原生 Windows 开发验收

状态：**原生 Windows 开发验收通过**。最终结果是 `73a6177b`；此前三次独立 attempt 也保留如下，不把其中的失败或后来发现的摘要缺陷改写为成功。

## 要回答的问题

同一台 Windows、同一正常用户、同一命名 Vault 中，停机的 Managed A 能否把 Profile、Identity Artifact、Engine 绑定和 Silo 配置备份成加密 archive；在 A 的浏览器数据随后被改写后，恢复原 A 是否重新读出备份时的持久 Cookie、localStorage、IndexedDB，同时保留 B；错误的 archive hash 和口令是否拒绝且不改变当前 A/B。

执行器为 [`tests/windows/managed-cold-backup-acceptance.mjs`](../../tests/windows/managed-cold-backup-acceptance.mjs)。它通过真实 Local API 和现有 [`loopback-server.mjs`](../../tests/windows/fixtures/loopback-server.mjs) 操作浏览器页面。测试用一次性 `coldbk-*` Vault，Direct 网络，本地 `127.0.0.1` 站点，随机合成标记；先备份停机 A，再改写 A 的三种持久标记，验明拒绝不破坏，然后恢复并读回原标记，最后启动 B 读回 B 标记。本轮不对 sessionStorage 或 session Cookie 的冷启动行为下结论。Profile 目录由产品管理，执行器不直接复制、删除或修改 Profile 文件。

验收使用 normal-user LocalAppData 下的独立开发程序目录与已有 Engine package 的文件硬链接；不改包文件、全局 ACL、默认 Vault、已安装产品或历史 performance 开发目录。只构建 debug `verisilo.exe` 与 `verisilo-cli.exe`；CLI 后台服务不需要 Vite/UI server。执行器核验 `telecaster\qiu`、干净的候选 worktree 与显式 source SHA，并记录两个 debug exe 与 Engine manifest 的 SHA-256。archive、Vault 与结果留在本次专用目录供诊断；报告不保存口令、Local API token 或 fixture token。

成功边界：三个 API 返回的 archive/Silo/Artifact 摘要一致；A 的原标记经改写和冷恢复后重新读出；B 的元数据和标记不变；两种错误恢复均拒绝且当前 A 改写标记仍可读；所有 Managed 会话正常停止。失败保留原始 `result.json`，修复有证据支持的原因后才用新 run ID 重试。浏览器无法启动时先核对实际账户与 Engine 路径，不把工作树或 Codex sandbox 的既有 DACL 限制算作产品回归。

## 本次结果

最终执行器 source SHA 为 `2353571da2646e2032c80c14e791a0ac8f40526d`；`verisilo.exe` 与 `verisilo-cli.exe` 从产品修复 SHA `4936292c94e9d5a58661ff984818006f2dad2826` 构建，SHA-256 分别为 `2abe748b5d0c31a79de7674644242df668f220f065c11ffb96ed2230de070b7f` 和 `4851079316fa194168b38afc69e1bb22f11a82a0af2e1907d4fec940f76a3b79`。Engine manifest SHA-256 为 `7bfb701935221b523027738d0c7abaa386851471e99f08eec7eb2bf09917278e`。程序目录为 `C:\Users\qiu\AppData\Local\VeriSiloDev\managed-cold-backup-880904`，账户为 `telecaster\qiu`。

| Attempt | 执行器 source / 程序 build SHA | 结果与原因 | 原始 `result.json` SHA-256 |
| --- | --- | --- | --- |
| [`84097239`](./managed-cold-backup-2026-09-28/84097239.json) | `d242c4cea22d65125efa670384924e5d9aa6a53f` / 同左 | 失败。真实 A/B 标记已持久化、备份 API 已返回；执行器直接比较表面路径与 Windows/Codex 重定向后的 canonical 路径，提前退出。 | `da46c1e183fd6d4d0ee7d3ffdd48a7e2a5104e0d3106b8047fde4e76f6e43241` |
| [`7ccdc3f8`](./managed-cold-backup-2026-09-28/7ccdc3f8.json) | `1e6e6af541ddd7d6e6a199adf19ab65e64d8ae8b` / `d242c4cea22d65125efa670384924e5d9aa6a53f` | 执行器当时标为 passed，A/B 恢复闭环通过；事后独立文件哈希发现 `backup-inspect.archiveSha256` 为 `a41e7df54985bb207a6d4404152f7e098af6266d1e973e68b09c09fa8b34dde0`，实际 archive SHA-256 为 `ed98e1ae3905510882d54fac1983f8459ff275a1b4e9a079138142c659c6bb29`。因此这次不能作为最终摘要验收。 | `70575adac5a5b7cedac4f4fa955fde551e447372cd2a36d75d0ef49c32a88a75` |
| [`255679a1`](./managed-cold-backup-2026-09-28/255679a1.json) | `4936292c94e9d5a58661ff984818006f2dad2826` / 同左 | 失败。服务已启动，但首次 `POST /v1/vault/initialize` 的 fetch 失败；未创建 A/B，cleanup 正常停止服务。该版执行器未记录底层 transport cause，故原因未证实。 | `6425eb123fe9b466dcba1c8f20e2a79db0d5d09ef381417c3c10e9015b00c540` |
| [`73a6177b`](./managed-cold-backup-2026-09-28/73a6177b.json) | `2353571da2646e2032c80c14e791a0ac8f40526d` / `4936292c94e9d5a58661ff984818006f2dad2826` | **通过**。初始化前只读 `GET /v1/status` 已成功；独立 archive SHA、拒绝路径、A/B 恢复闭环均通过。 | `53ec96ae35e4f5a0facf6781e9eea60da5fcf175d1dc153c3d1e76df9bb3a190` |

最终原始报告仍在 `C:\Users\qiu\AppData\Local\VeriSiloAcceptance\cold-backup-73a6177b\result.json`；表中四份仓库副本与原始文件逐字节 SHA-256 相同。archive 留在同目录，大小 32,256,758 字节，包含 66 个 Profile 文件；独立 `Get-FileHash`、Node 文件哈希与检查 API 均得到 `ffaaaf282c285519018ba9b06bdbabbab31533800a919ef7fe2bb87b9135adf3`。修复的是 `Sha256State::update` 在 partial buffer 未填满时错误继续处理后续输入的问题；新增分块/空输入已知向量和 archive 一次性哈希对照回归。

最终运行使用真实 Camoufox、Direct 网络和 `127.0.0.1` fixture：A 与 B 各写入持久 Cookie、localStorage、IndexedDB。停机 A 备份并检查后，A 被改写；错误摘要与错误口令均收到 HTTP 400，改写后的 A 数据仍可读，A/B 元数据不变。按正确摘要恢复后，A 的原三种持久标记、Identity Artifact 绑定与 Silo 配置恢复，B 的原标记和元数据保持不变。服务 PID `23764` 已退出，事后未发现本轮 `verisilo.exe`、`verisilo-cli.exe`、Camoufox 或 fixture 进程。

验证边界：原始产品候选 `d242c4c` 的完整 verify 已通过 desktop 281 项与 core harness 276 项；其 CLI 单测 7/7。摘要修复后 focused engine 24/24 与 backup 7/7（含摘要、秘密与恢复边界）通过；未因只改摘要实现与执行器重跑无关矩阵。UI Preview 中实际交互验证了备份、错误口令、检查摘要、路径变更后检查失效、明确恢复确认、成功刷新、取消清空秘密、运行中禁用，以及 800/640 宽无横向溢出；前端类型检查和 app-copy/vault-ui-lock 26 项通过。Preview 使用 Mock API，真实 runtime 结论只来自上面的原生执行。本轮不验证 sessionStorage、session Cookie、跨账户/机器、代理网络、installer、production 打包或反检测能力；不构成 RC/发布 Gate。
