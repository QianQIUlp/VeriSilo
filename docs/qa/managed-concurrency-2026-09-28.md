# 本地 Managed Silo 有限并发 v1：验收记录

状态：**原生 Windows A/B/C/D/E 后端验收已覆盖，真实 UI 选择核对未确认**。两份失败 attempt 的已完成步骤和失败原因分别保留；第三份只针对代理 fixture 因果修复后的 C/E 续验通过，不将 UI 超时升级为通过。冷备份 v1 的原生结果 `73a6177b` 对未修改的归档格式、密码学与单 Silo 恢复路径继续有效。

## 执行对象

原生执行器为 [`tests/windows/managed-concurrency-acceptance.mjs`](../../tests/windows/managed-concurrency-acceptance.mjs)。它要求提交后的准确 source SHA、干净工作树、`telecaster\qiu`、一次性 `managed2-*` Vault，以及正常用户 LocalAppData 下独立开发程序目录中的 debug `verisilo.exe`、`verisilo-cli.exe` 和既有固定 Engine package（manifest SHA-256 `7bfb701935221b523027738d0c7abaa386851471e99f08eec7eb2bf09917278e`）。它不覆盖旧程序目录或改系统 ACL。程序、CLI 和 Engine manifest 哈希写入原始 `result.json`；结果留在一次性输出目录，失败保留原始状态，不用未变输入直接重跑。

需要原生 UI 人工切换检查时才显式传 `--ui-checkpoint`；后台续验默认不打开桌面窗口或等待 UI。该可选模式在双活会话运行后用 CLI 打开同一实例桌面，写入 `<outputDir>/ui-checkpoint.json`：只有两个 Silo ID、runtimeId、服务 PID、候选 SHA、随机 checkpointId 与继续文件路径，不含 API token、Vault/代理口令。检查者在该桌面切换选择并记录 UI 证据后，原子写入 `<outputDir>/ui-continue.json`，内容为 `{"schema":"verisilo-managed-concurrency-ui-continue/v1","checkpointId":"<checkpoint中的值>","result":"passed"}`；失败写 `"failed"`。首次直接验收最多等待十分钟，`--continue-after-backup` 的 P/Q 双活检查最多等待 90 秒，继续后再向同一 API 核对两者 runtimeId 与运行状态；后者超时或失败只记 UI 未确认，后端 C/E 继续。等待时间不算通过。UI Preview 的迟到响应和错误显示仍单独验证。

本地站沿用 [`loopback-server.mjs`](../../tests/windows/fixtures/loopback-server.mjs)，用随机合成值读写 Cookie、LocalStorage、IndexedDB。固定代理验收使用两个独立 [`loopback-connect-proxy.mjs`](../../tests/windows/fixtures/loopback-connect-proxy.mjs)：仅监听 `127.0.0.1`，把保留测试地址 `198.51.100.9` 映射到测试站，并分别记录浏览器操作 token 实际经过的 CONNECT；不会向该保留地址发包。代理只透传既有 Host 启动所需 `ipwho.is:443`、`api.ipify.org:443`、`api.ip.sb:443` 的出口观察请求；其他目标拒绝。该观察仅满足原产品的启动规则，不由同机本地代理实验推断真实代理公网出口。代理 fixture 的允许/拒绝与路由自检由 Node 内置测试完成。

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

本轮首次 `verify --full` 已执行九组命令，但**初轮不是全绿**：Managed contracts、`pnpm check`、desktop `cargo check`、Host package/page、agent-task 工作流 40 项通过；`pnpm test` 的 app-copy 旧文案断言失败；desktop lib 为 281 pass、3 fail、5 ignored、3 Verge filtered，harness 为 276 pass、相同的 3 fail、5 ignored、3 filtered。UI 文案修复后的 focused 29 项通过，最终 UI check 与五文件 focused 63/63 通过；`pnpm test` 尾部未执行的 session fixture 自检和 dev desktop 3 项已单独补过。Rust 初轮三个失败涉及两个诊断 idle/null-root 断言与最近运行归属。修复后 desktop crate 定向验证 `local_runtimes::` 5/5、`application::tests::` 22/22、launcher record 与 restart recovery 各 1/1、`recent_run_` 2/2；随后同 ID 历史归属新增回归单独 1/1。harness crate 的 `application::tests::` 随后定向 23/23。初轮失败保留为事实，不因局部闭环改写，也不无变化重跑全部矩阵。

## 结果

生产 debug 程序来自 `043821b69699ecff6c739175f25607b37709b49f`，桌面与 CLI SHA-256 分别是 `8bd9d34eb7abe34205b057d3be285d2aaf79d4da68f4c37061d54a4c605c91d3`、`0e36e90766263e1ed6adcf790a2844f8db528aea3183c97f931779d2f80c4de7`；固定 Engine manifest SHA-256 为 `7bfb701935221b523027738d0c7abaa386851471e99f08eec7eb2bf09917278e`。未修改或重建这三个生产输入。

- 首次结果：`C:\Users\qiu\AppData\Local\VeriSiloAcceptance\managed-concurrency-043821b\result.json`，SHA-256 `03ad43eb934413d004b95f22f45a8c0203fad3ccabcf53ab28a89fef860bdbb0`，source `043821b`。A/B 真双活、不同 Profile/运行编号，同一站点 Cookie、LocalStorage、IndexedDB 标记各自读回；A 在 B 启动期间 177 ms 响应按 Silo 状态与页面。随后因可选原生 UI 检查点十分钟没有收到结果而 `failed`；无 UI 通过 claim。服务和测试会话清理完毕。
- 第一次续验：`C:\Users\qiu\AppData\Local\VeriSiloAcceptance\managed-concurrency-8580fa-continuation\result.json`，SHA-256 `cdedc80c9e271a613a29752220dfd994881cd59d25266a6694e4243f169012b1`，harness source `8580fa3`，`continuedFrom` 精确引用首次结果及相同生产哈希。重复 A 与第三 Silo 拒绝；reobserve A、stop/restart A 不改 B 最近运行绑定，B 页面继续响应；B 活着时 A 的备份、inspect、明确恢复成功，运行中的 B 备份与 A 覆盖拒绝。A 归档 45,000,716 bytes、71 files，archive SHA-256 `257036fa0a35653a52e81e46425cdf7c9287e2e745014fffd918068d23e015d8`。首个固定代理 P 启动因 `network_exit_unavailable` 而 `failed`：测试代理遗漏现有 Host 启动必需的 `api.ipify.org:443` 与 `api.ip.sb:443`，故 P/Q 路由尚未验证；退出后服务 PID 5408 已不存在。原生 UI 曾实际点选 B，详情切到 B，但 A/B 稳定双活的完整选择/前后 runtimeId 核对仍未取得，不据此宣布 UI 验收完成。
- 代理 fixture 定向修复后续验：`C:\Users\qiu\AppData\Local\VeriSiloAcceptance\managed-concurrency-e1197213\result.json`，SHA-256 `bcc005fc4d6833ab0ce61fc5c3b2623964e0377f326ceff8de1370a50de4d464`，harness source `94e76498a6188b432233fc5f60253b6d9ccb1abb`，`continuedFrom` 精确引用上份结果及生产 `043821b`；桌面、CLI、Engine manifest 哈希与前两次相同。后台 C/E 为 `passed`、清理无错误：P/Q 双活时，各有一条带独立页面操作 token 的本地测试站 CONNECT，互不借用；P 代理终止后该侧为 `verification_failed`、请求没有直连测试站，Q 原路由继续可读。Direct A 与固定代理 Q 同时运行；Vault lock 后两侧 page API 都拒绝，解锁后仍能分别管理；服务退出并重启后，A/Q 各自的最近运行归属保留，状态均为 `stopped`（原服务 PID 7364，新 PID 9108）。这些本地代理结果不证明真实公网出口或完整无泄漏。该次脚本于 08:25:04–08:28:47 UTC 执行；只读磁盘恢复文件在清理后仍按 Silo 存储。

本次真实 UI 检查点在 P/Q 双活时创建，初始 runtimeId 为 P `f884fa3b-165f-49ef-bb9c-fac267fb4188`、Q `65f01aaa-a0a2-4229-835f-5554cdfc31a7`。人工曾点选 P、Q，列表两者显示 Running、详情随选择切换；但 `ui-continue.json` 在 90 秒期限后、P 故障步骤期间才写入，脚本已记录 `uiSelection: unconfirmed`，没有取得同一双活时刻的后置 runtimeId 快照。随后 Q 在 E 阶段重新启动，最终磁盘 runtimeId 已变化，不能拿来补作 UI 后置证据。UI Preview 的交互、显示、迟到响应与错误状态只证明前端行为，不代替后端和真实浏览器结论。本轮不涉及内核重建、RC、installer 或产品 release。
