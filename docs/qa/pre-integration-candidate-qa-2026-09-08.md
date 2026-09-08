# Pre-integration candidate QA — 2026-09-08

结论：**PASS**

本轮对 canonical baseline 加单一 QA-R2-01 Host 修复提交形成的 candidate source tree 做 fresh candidate QA。候选源码 Host 的健康 page journey、错误关联协议与第二个 fresh runtime 均通过；Vault/Core、Standard、Managed source Host、Network/diagnostics 和 Preview 代表性用户旅程没有发现新的 P0、P1 或高影响 P2。

**Candidate source is ready for final integration.**

## 固定测试对象

| 项目 | 实际值 |
| --- | --- |
| Tested candidate | `5dfe36c42440f5cea7e26c79daeb3ea91818a757` |
| Candidate remote ref | `origin/agent/host/qa-r2-01-host-protocol-dcc823` |
| Candidate parent | `1dc78b3dedf892bf8f04d74e73ee0137cf18ced5` |
| Canonical baseline remained | `origin/baseline/dev` = `1dc78b3dedf892bf8f04d74e73ee0137cf18ced5` |
| QA lane | `qa`（一次性 manual pre-integration candidate workflow） |
| QA branch | `agent/qa/pre-integration-candidate-qa-5dfe36c` |
| Remote QA branch | `origin/agent/qa/pre-integration-candidate-qa-5dfe36c` |
| Fresh worktree | `.verisilo-worktrees/qa-pre-integration-candidate-qa-5dfe36c` |
| Fresh Vault | `qa-preint-5dfe36c-0908a` |
| Development port | `15641` |
| Host | Windows 10.0.26200, x86_64 |
| Browsers | Chrome 152.0.7977.76；Edge 152.0.4191.66 |

开工及收尾前均执行 `git fetch --prune origin`；两次均确认 baseline、candidate remote ref 和 candidate parent 与上表完全一致。没有静默更换测试对象。没有运行正常 `agent-task start`，也没有伪造 `.agent-task.json`。

遵守 NO-BUILD：没有构建或重建 Managed/browser package、Managed release package、desktop production build、RC 或 installer，也没有 install/repair/uninstall。只使用 candidate source 的 debug development Core/CLI、candidate source Host、已有 browser resources 和 HMR Preview。

## QA-R2-01 candidate source Host

结论：**PASS（REAL_RUNTIME · candidate source Host）**

candidate diff 仅包含 `apps/camoufox-host/host_v1.py` 与 `apps/camoufox-host/test_page_command.py`。先运行 owning Host regression test，结果为 `page command check passed`。随后使用 candidate `host_v1.py`、已有 development browser resources 和两个独立 fresh Profile/runtime，直接执行实际 Host JSONL 协议。

| 检查 | Runtime A | Runtime B |
| --- | --- | --- |
| `hello` | PASS | PASS |
| `launch` | PASS | PASS |
| `page snapshot` | PASS，返回当前页面结构 | PASS，返回当前页面结构 |
| `page windows` | PASS，`window_count=1` | PASS，`window_count=1` |
| `status` | PASS | PASS |
| `close` | PASS，浏览器干净关闭 | PASS，浏览器干净关闭 |
| `shutdown` | PASS，Host exit 0 | PASS，Host exit 0 |
| request/response ID correlation | PASS | PASS |

Runtime B 在全新 Host 进程与 Profile 上重复完整序列，因此覆盖了 source Host restart 两侧。没有出现 false mismatch、stale response 消费或后续级联 desync。

### Protocol error correlation

结论：**PASS（REAL_RUNTIME · candidate source Host）**

| 输入 | 实际响应 |
| --- | --- |
| valid ID + invalid page action | 同一 ID；`ok=false`；`bad_type` |
| valid ID + unknown command | 同一 ID；`ok=false`；`unknown_command` |
| valid ID + unknown parameter | 同一 ID；`ok=false`；`unknown_field` |
| invalid JSON | `id=null`；`invalid_json` |
| duplicate key | `id=null`；`duplicate_key` |
| non-object frame | `id=null`；`frame_not_object` |
| invalid ID type | `id=null`；`bad_type` |

这证明 candidate source 在可靠解析出合法 request ID 后保留关联；无法可信取得 ID 的 frame 继续 fail-closed 为 null。未观察到 JSON、duplicate-key、UTF-8 或 frame validation 弱化。本节不声称 packaged product runtime 已验证。

## Vault / Core

结论：**PASS（REAL_RUNTIME）**

- fresh Vault 初始化成功；lock 后错误密码 unlock 返回非零且状态保持 locked，正确密码恢复 unlocked。
- Standard Chrome running 时停止 current-source service；浏览器进程保留。服务重启后准确进入 `recovery_required` 并指向原 Silo，unlock 后以 live PID/Profile lock 恢复同一 active Silo。
- 多次 service restart 后 4 个预期 Silo、各自独立 Profile path 和 stopped 状态均可恢复。
- stale packaged Host 的 Managed create 失败没有留下 Managed Silo；最终只有 4 个预期 Standard/synthetic-network Silo。未观察到错误 Silo 写入、数据丢失或错误进程操作。

## Standard

结论：**PASS（REAL_RUNTIME）**

| 检查 | 结果 |
| --- | --- |
| Chrome / Edge fresh create/readback | browser kind/version/path、direct network 与各自独立 Profile path 一致 |
| Chrome start / ownership | verification `verified`；12 个 owned processes 只使用 Chrome Silo Profile |
| single-active | Chrome running 时 Edge start 被拒绝，Chrome 保持 active |
| service restart recovery | live Chrome PID/Profile lock 正确归属于原 Silo并恢复 |
| normal close / stop convergence | 仅向精确核对的 root window 发送正常 close；owned PID/Profile lock 消失，状态为 stopped |
| Edge start / ownership | verification `verified`；13 个 owned processes 只使用 Edge Silo Profile；正常 close 后为 stopped |

没有使用 force-kill 或新增浏览器自动化命令。本机 Chrome 与 Edge 均可用。

## Managed

结论：**candidate source Host PASS；packaged Core journey 为已知 artifact boundary**

- **REAL_RUNTIME · candidate source Host：** fresh Artifact provision、Engine/browser-resource launch、identity observation、`page snapshot`、`page windows`、status、close 和第二个 runtime reopen 均通过。
- **REAL_RUNTIME：** candidate Core/Vault 的 inactive Standard/provider Silo diagnose 均为 `runtimeState=null`、`runtimeMessage=null`，website identity 为 null；global identity 没有把历史结果归为当前打开身份。
- **COVERAGE_BOUNDARY / PACKAGE_STALENESS：** 现有 09-02 development package 内 Host binary 早于 `page` command，也不包含 candidate Host。用该旧 package 尝试 packaged Core Managed create 时返回 `ArtifactIntegrityError`，且没有写入错误 Silo。candidate source Host 对相同既有 browser resources 的 provision、launch 和 page journey 已通过，因此这不是 candidate source defect，也没有据此判 FAIL。
- `configured`/readback、source Host observed runtime 与 packaged/release verification 在本报告中保持分离；未声称未来 RC package 已验证。

Round 2 已通过且本次两个 Host 文件不涉及的 Core/Vault Managed binding、single-active、stale identity 和 runtime attribution 低价值重复项没有机械穷举；本轮对改变层的 candidate source Host 和关键隔离信号重新取得直接 evidence。

## Network / diagnostics

结论：**PASS（REAL_RUNTIME）**

| 检查 | 结果 |
| --- | --- |
| Direct data minimization | Direct Silo diagnose 没有 `clash` object、provider inventory、account metadata、controller data 或本机 Clash discovery |
| fixed proxy offline | synthetic `127.0.0.1:65529` 在浏览器 spawn 前 fail-closed；无 direct fallback、无目标 Chrome 进程 |
| provider-bound offline/copy | 只报告 exact synthetic binding `127.0.0.1:15642`、bound group/node，并明确不会探测本机其他 Clash |
| secret/inventory minimization | 没有读取或输出用户真实 Clash data；没有 secret；没有枚举不相关 inventory |
| cross-Silo isolation | Edge diagnose 没有继承 unrelated fixed/provider failure 的 runtime state、message 或 identity |

只使用 synthetic localhost endpoint 和合成名称；未调用全局 `clash` command。正常 provider、401 与 selector drift 已在 Round 2 通过且不受本次 Host 两文件影响，本轮不作低价值重复。

## UI / Preview

结论：**PASS（UI_ONLY）**

| 场景 | 观察 |
| --- | --- |
| stopped overview | 当前状态、Silo 主操作和用户下一步清楚 |
| Standard create | 默认路径简洁；Standard/Managed 区别诚实；Managed unavailable 有原因；没有 raw `native/inherit/unavailable` token；高级设置可发现 |
| Managed collapsed | 默认只突出名称、颜色、network、language/result；CPU、screen、GPU、timezone、UA、Canvas/Audio、font、WebRTC 不在默认主路径铺开 |
| Managed expanded | timezone、screen、CPU、GPU 等可配置项可达；UA、Canvas/Audio、font、WebRTC 边界有说明 |
| error/retry | 失败以 alert 呈现并给出检查设置后重试的恢复动作；没有暴露 raw internal error |
| running | 状态明确；edit/archive disabled；文案说明正常关浏览器窗口即可停止且不会影响其他窗口 |
| narrow viewport | 390×844 collapsed/expanded 均无水平溢出；controls 与 submit 保持在 viewport 内 |

Preview console 无 warning/error。截图 evidence 位于同名目录：

1. `01-overview-stopped.png`
2. `02-standard-create.png`
3. `03-managed-collapsed.png`
4. `04-managed-expanded.png`
5. `05-error-retry.png`
6. `06-running.png`
7. `07-managed-narrow.png`
8. `08-managed-narrow-expanded.png`

Preview/Mock 没有被用于证明 Vault、Engine、Host、Identity 或 Profile persistence。

## New defects and severity count

新 defect：**无**。

| Severity | 数量 |
| --- | --- |
| P0 | 0 |
| P1 | 0 |
| High-impact P2 | 0 |

没有 finding 污染其他矩阵。本轮曾由 direct source Host 启动产生一个 package tree 外的 browser-generated `version.json`；已停止服务并把该精确 QA 生成物移出预存 package，恢复原始 package tree。它没有进入 Git diff，也没有作为产品 defect 计数。

## Coverage boundaries / environment limitations

- **Known remaining boundary:** stale pre-existing development package does not contain the candidate Host; packaged-runtime verification requires a future authorized rebuild.
- stale package 的 Managed Core create/start/page、未来 RC binary、installer 和 clean Windows acceptance 均未由本轮证明。
- Standard CLI 没有 edit；真实网站状态持久化和完整人工关窗 UX 未由 CLI 自动证明。
- successful external fixed-proxy/provider forwarding 缺少安全隔离的真实外部目标；未作成功转发 claim。
- 没有浏览器缺失的 environment limitation；Chrome 与 Edge 均可用。

## Workflow verification and publication

- `node scripts/agent-task.mjs verify --lane qa`：exit 0；qa lane 按 workflow 以确切候选、可复现步骤和实际观察作为 evidence。
- `node scripts/agent-task.mjs check --lane qa`：exit 0；9 个 QA evidence files 均在 scope 内、无 boundary violation；本次 manual exception 没有 metadata，因此脚本明确跳过主检出快照比较，另以开工/收尾 Git status 手工核对。
- `git diff --check`：exit 0。
- Final evidence commit 与 local/remote equality 由提交后的 Git ref 核对记录；不在提交内容中自引用尚未生成的 SHA。
- QA evidence 是 candidate 后唯一提交；没有产品代码污染、integration、baseline advance、package build、RC 或 installer。
