# 本地 Managed Silo 有限并发 v1：验收记录

状态：**待原生 Windows 执行**。本文件先固定本轮要回答的问题与验收口径；只有冻结提交、正常用户上下文中的原始结果产生后，才能填写运行结论。冷备份 v1 的原生结果 `73a6177b` 对未修改的归档格式、密码学与单 Silo 恢复路径继续有效。

## 执行对象

原生执行器为 [`tests/windows/managed-concurrency-acceptance.mjs`](../../tests/windows/managed-concurrency-acceptance.mjs)。它要求提交后的准确 source SHA、干净工作树、`telecaster\qiu`、一次性 `managed2-*` Vault，以及正常用户 LocalAppData 下独立开发程序目录中的 debug `verisilo.exe`、`verisilo-cli.exe` 和既有固定 Engine package（manifest SHA-256 `7bfb701935221b523027738d0c7abaa386851471e99f08eec7eb2bf09917278e`）。它不覆盖旧程序目录或改系统 ACL。程序、CLI 和 Engine manifest 哈希写入原始 `result.json`；结果留在一次性输出目录，失败保留原始状态，不用未变输入直接重跑。

需要原生 UI 人工切换检查时传 `--ui-checkpoint`（默认无暂停）。双 Direct 会话运行后，执行器用 CLI 打开同一实例桌面，写入 `<outputDir>/ui-checkpoint.json`：只有 A/B 的 Silo ID、runtimeId、服务 PID、候选 SHA、随机 checkpointId 与继续文件路径，不含 API token、Vault/代理口令。检查者在该桌面切换选择并记录 UI 证据后，原子写入 `<outputDir>/ui-continue.json`，内容为 `{"schema":"verisilo-managed-concurrency-ui-continue/v1","checkpointId":"<checkpoint中的值>","result":"passed"}`；失败写 `"failed"`。执行器最长等待十分钟，且继续后再向同一 API 核对两者 runtimeId 与运行状态；超时/失败会留结果并清理，不把等待时间当作通过。UI Preview 的迟到响应和错误显示仍单独验证。

本地站沿用 [`loopback-server.mjs`](../../tests/windows/fixtures/loopback-server.mjs)，用随机合成值读写 Cookie、LocalStorage、IndexedDB。固定代理验收使用两个独立 [`loopback-connect-proxy.mjs`](../../tests/windows/fixtures/loopback-connect-proxy.mjs)：仅监听 `127.0.0.1`，把保留测试地址 `198.51.100.9` 映射到测试站，并分别记录浏览器操作 token 实际经过的 CONNECT；不会向该保留地址发包。代理还透传 Managed 现有身份创建所需的 `ipwho.is:443` 请求；该请求不用于推断实际公网出口。其他目标拒绝。代理 fixture 的路由与拒绝自检由 Node 内置测试完成。

## 直接验收口径

| 产品问题 | 原生判据 |
| --- | --- |
| 双活与身份隔离 | A/B 同一 Vault、Direct、同时 `running`；各自 `runtimeId` 不同；A 在 B 慢启动期间仍能响应按 Silo 状态和页面请求；两边各自读回不同的持久 Cookie、LocalStorage、IndexedDB。 |
| 独立管理与上限 | 第三 Managed Silo 和重复启动 A 被拒绝，既有 A/B 保持运行；重新观察 A 不改变 B 的最近运行绑定；停止 A 后 B 页面仍可读，重启 A 保留原 Artifact 与持久数据。 |
| 代理路由与故障 | P/Q 两个固定 HTTP 代理会话同时运行，两个独立 CONNECT fixture 各只记录自己会话的页面操作 token；杀掉 P 上游后，本地站没有收到 P 的直接降级请求，Q 仍能通过自己的代理读回数据；Direct A 与代理 Q 可同时运行。 |
| 冷恢复组合 | B 活着时，运行中的 B 备份拒绝；停机 A 完成备份、摘要检查，随后运行中的 A 覆盖拒绝；停止 A 后明确恢复，改写过的 A 恢复原持久值，B 页面和最近运行绑定保持。 |
| 全局操作与重启 | Vault 锁定使全部活动页面操作被拒绝，解锁后两个会话仍可按原规则管理；应用退出清理两个会话；服务重启后按 Silo 查询的状态、最近运行仍归属于 A/Q，不能显示仍运行或借另一 Silo 的证据。锁定不被解释为 Direct 浏览器必然退出。旧单会话 API/CLI 与旧恢复格式的兼容性由对应 focused 自动测试覆盖。 |

`matched` 仅表示当前身份匹配，不能上升为 `verified`。最近运行记录是历史；比较 B 时检查 runId、runtimeId、Profile/Artifact/网络策略绑定，不冻结健康线程可能合法更新的时间戳。对于代理失败，页面未直连与另一会话继续可用是直接观察；本地实验不证明公网出口、DNS/WebRTC 完整无泄漏或真实代理服务的可用性。

Native Host protocol 2 的运行查询可以明确传 `siloId`；无目标且双会话时返回 `unavailable`，Companion 因而只保留本地观察，不会把任一会话当成唯一活动身份。提交及读取暂存网络证据按 `siloId` 和 `runtimeId` 同时校验。旧快照没有 `sessions` 字段时仍按原单会话解释，协议版本和备份格式不变。

## 共享契约验证矩阵

本轮首次 `verify --full` 已执行九组命令，但**初轮不是全绿**：Managed contracts、`pnpm check`、desktop `cargo check`、Host package/page、agent-task 工作流 40 项通过；`pnpm test` 的 app-copy 旧文案断言失败；desktop lib 为 281 pass、3 fail、5 ignored、3 Verge filtered，harness 为 276 pass、相同的 3 fail、5 ignored、3 filtered。UI 文案修复后的 focused 29 项已通过；`pnpm test` 尾部未执行的 session fixture 自检和 dev desktop 3 项已单独补过。Rust 三处初轮失败分别涉及两个诊断 idle/null-root 断言与最近运行归属；修复后的 focused 结果及最终提交仍待集成负责人确认并记录。初轮失败保留为事实，不把后续局部通过写成初轮全绿，也不无变化重跑全部矩阵。

## 结果

待填写：冻结 source SHA、debug 程序哈希、原始结果路径与哈希、A/B/P/Q 状态、路由计数、失败归属、恢复摘要、清理状态。UI Preview 的交互、显示、迟到响应与错误状态由本轮 UI 记录补充；Preview 不代替本文件的后端和真实浏览器结论。本轮不涉及内核重建、RC、installer 或产品 release。
