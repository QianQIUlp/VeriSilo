# Pre-RC Stabilization QA Round 2

结论：**FAIL**

本轮在集成 Round 1 修复后的 fresh canonical baseline 上完成了独立稳定化 QA。Vault/Core、Standard、Managed 生命周期、Network/diagnostics 以及 4 项 Round 1 回归均通过；在两个 fresh Managed runtime 上，当前源码仍无法完成 page action，记录为 1 个高影响 P2。未发现 P0 或 P1。FAIL 表示存在真实产品缺陷，不表示全部矩阵失败。

## 测试对象与约束

| 项目 | 实际值 |
| --- | --- |
| Canonical remote baseline | `origin/baseline/dev` |
| Baseline exact SHA | `1dc78b3dedf892bf8f04d74e73ee0137cf18ced5` |
| QA lane | `qa` |
| Task branch | `agent/qa/pre-rc-stabilization-qa-2edf5b` |
| Remote task branch | `origin/agent/qa/pre-rc-stabilization-qa-2edf5b` |
| Fresh worktree | `.verisilo-worktrees/qa-pre-rc-stabilization-qa-2edf5b` |
| Fresh Vault | `qa-pre-rc-stabilization-q-2edf5b` |
| Preview port | `15623` |
| Host | Windows 10.0.26200, x86_64 |
| Browsers | Chrome 152.0.7977.76；Edge 152.0.4191.66 |

开工时执行 `git fetch --prune origin`，确认 `baseline/dev == origin/baseline/dev == 1dc78b3d...`，远端没有改变用户指定的固定测试对象。随后用 repository task workflow 创建全新的 QA branch/worktree/Vault/port；没有复用 Round 1 的分支、Vault、Silo 或 runtime state。

遵守 NO-BUILD：没有构建 Camoufox/browser package、Managed release package、desktop production build、RC 或 installer，也没有 install/repair/uninstall。为执行 canonical source，只编译并运行了 debug development Core/CLI；Managed 消费已有 development engine package。其 `packageVerification=not_requested`、`digestVerified=false`、`signatureVerified=false`、`verifiedAdapter=null`，因此这里没有 release/package qualification claim。

## Vault / Core lifecycle

结论：**PASS（REAL_RUNTIME）**

- fresh Vault 初始化成功；lock 后错误密码 unlock 返回非零并保持 locked，正确密码恢复 unlocked。
- Standard Chrome running 时停止 service，浏览器进程仍在；current-source service 重启后准确进入 `recovery_required` 并指向原 Silo，unlock 后根据 live PID/Profile lock 恢复同一运行状态。
- 全矩阵结束后再次 restart：Vault 为 locked、activation 为 stopped；unlock 后 7 个预期 Silo 和各自独立 Profile path 均可恢复。
- 一个预检失败的 Managed fixed-proxy create 没有留下残余 Silo（前后数量均为 7）。未观察到数据丢失、错误 Silo 写入或错误进程操作。

## Standard Silo

结论：**PASS，存在 coverage boundaries**

| 检查 | 结果 | Evidence |
| --- | --- | --- |
| Chrome / Edge fresh create/readback | exact browser kind/version/path、direct network 与独立 Profile path 均一致 | REAL_RUNTIME |
| Chrome / Edge start 与 ownership | browser verification 为 verified；每个进程组只使用对应 fresh Silo Profile | REAL_RUNTIME |
| single-active | Chrome running 时启动 Edge 被拒绝，原 Chrome 保持 active | REAL_RUNTIME |
| service restart recovery | Chrome live PID/Profile lock 被正确归属于原 Silo并恢复 | REAL_RUNTIME |
| 正常关闭后的收敛 | 只向精确核对的 root window 发送正常 close；PID/Profile lock 消失，状态收敛为 stopped | REAL_RUNTIME |
| own-runtime attribution | 真正运行并停止的 Chrome diagnose 显示属于自己的 stopped evidence | REAL_RUNTIME |
| CLI edit | 当前 CLI 没有 edit command | COVERAGE_BOUNDARY |
| 真实网站 state persistence / 完整人工关窗 UX | CLI 无法诚实自动证明；未增加 kill/page automation 命令 | COVERAGE_BOUNDARY |

本机同时存在 Chrome 和 Edge，因此没有浏览器缺失环境限制。

## Managed Identity Silo

结论：**核心生命周期 PASS；page action FAIL**

| 检查 | 结果 | Evidence |
| --- | --- | --- |
| fresh create | Managed A/B 均成功；`Managed create -> ArtifactIntegrityError` 未回归 | REAL_RUNTIME |
| Artifact / Engine / Network binding | A/B 各自的 V5 Artifact id/SHA、Camoufox engine、direct network 均可独立 readback | REAL_RUNTIME |
| start / stop / reopen | A/B 均成功启动、停止和重新打开；Host launch observed，adapter 为 Camoufox | REAL_RUNTIME |
| runtime identity | active Silo 的 page-script observation 与自身 `siloId` 和身份参数一致 | REAL_RUNTIME |
| service restart / persistence | restart + unlock 后 A/B 与各自 Artifact/Profile binding 保留 | REAL_RUNTIME |
| multi-Silo / single-active | B running 时启动 A 返回 `managed_another_silo_running`；inactive A 不继承 B 的 runtime/identity | REAL_RUNTIME |
| stale identity | B 停止且另一个 Silo 启动失败后，global identity 不再展示 B 的历史 observation 为当前身份 | REAL_RUNTIME |
| page action | 两个 fresh current-source Managed runtime 上，`snapshot` 首请求均 ID mismatch，随后 `windows` 均 fail-closed 拒绝 | REAL_RUNTIME / FAIL |
| package qualification | 仅消费已有、未执行 package verification 的 development package | COVERAGE_BOUNDARY |

configured/readback、observed runtime 与 verified release package 在本报告中保持分离。

## QA-R1-01 current-source development-runtime verdict

结论：**FAIL（REAL_RUNTIME · current-source development runtime）**

本轮实际运行的 Rust Core/CLI 来自 baseline `1dc78b3d...` 的 debug development binaries，并消费已有 development Managed engine package。没有使用 Round 1 的 installed Desktop service 来回答该问题。

在 Managed A 上：start 成功并取得 page-script identity；紧接着 `page snapshot` 返回 `Camoufox Host response ID did not match the outstanding request`；随后 `page windows` 返回 `Camoufox Host transport is desynced and refuses further requests: a response arrived while m3-6 was outstanding and carried id <missing>`。stop 成功。

service restart 后用独立 Managed B 重复：start 成功；`page snapshot` 再次返回同一 ID mismatch；下一条 `page windows` 明确以 desynced/fail-closed 拒绝，原因对应另一个请求 `m3-5` 和 `id <missing>`。

因此，timeout/mismatch 后 quarantine 与后续 fail-closed 行为可观察到，但 `page snapshot` 和 `page windows` 的产品旅程仍未成功；Round 1 的 packaged-runtime coverage boundary 已被当前源码 REAL_RUNTIME 证据替代，但未来 RC installer binary 仍未被本轮证明。

## Network / diagnostics

结论：**PASS，存在 external-forwarding coverage boundaries**

| 检查 | 结果 | Evidence |
| --- | --- | --- |
| Direct data minimization | Standard/Managed direct diagnose 都没有 provider inventory、account metadata、controller data 或本机 Clash discovery | REAL_RUNTIME |
| fixed proxy offline | isolated unused localhost endpoint 在 browser spawn 前 fail-closed；无 active Silo、无目标浏览器进程、无 direct fallback | REAL_RUNTIME |
| failed Managed create | fixed offline preflight 失败且不产生残余 Silo | REAL_RUNTIME |
| PAC | synthetic localhost PAC 返回 `DIRECT`；Chrome start 成功，进程带精确 PAC URL，fixture 收到 3 次请求 | REAL_RUNTIME |
| provider-bound normal | synthetic controller 只收到 bound configs/group/node 请求；diagnose 只输出 bound group/node 与 delay | REAL_RUNTIME |
| provider 401 | diagnose 明确提示检查当前 binding 的 controller address/Secret，不回显 secret | REAL_RUNTIME |
| selector drift | diagnose 报告 bound selector 未保持，且不输出 fixture 中无关的 selected-node/inventory | REAL_RUNTIME |
| provider offline | 恢复文案指出当前 Silo 的 exact controller binding，并明确不会探测本机其他 Clash | REAL_RUNTIME |
| cross-Silo isolation | never-started/inactive Silo 不继承其他 Silo 的 runtime state、message 或 identity | REAL_RUNTIME |

全程只使用 localhost synthetic controller/listener 和合成名称；没有调用全局 Clash inventory，没有读取、复制、保存或粘贴用户真实 provider 数据或 secret。

## HMR / Preview UX

结论：**PASS（UI_ONLY）**

| 场景 | 观察 | Evidence |
| --- | --- | --- |
| first-use / locked | 本机加密、无云恢复、密码要求与 disabled action 清楚；locked recovery messaging 可理解 | UI_ONLY |
| loading / empty / error / running | 均有明确 presentation；running 时 edit/archive disabled 并说明先关闭浏览器 | UI_ONLY |
| Standard collapsed | 默认主路径简洁；不再暴露 raw `native/inherit/unavailable` token，高级设置仍可展开 | UI_ONLY |
| Managed collapsed | 默认聚焦名称、颜色、network、identity preset 与结果型选项，不铺开 CPU/GPU/UA/Canvas 等工程字段 | UI_ONLY |
| Managed expanded | timezone、screen、CPU/GPU、UA、Canvas/Audio、font、WebRTC 等真实可配置项仍可达且有说明 | UI_ONLY |
| Managed unavailable | overview 中 disabled，并给出诚实原因 | UI_ONLY |
| error/retry | 操作失败有 alert，用户可再次发起操作 | UI_ONLY |
| narrow viewport | 390x844 下无水平溢出；advanced controls 与 submit 均在 viewport 内 | UI_ONLY |

Preview console 未观察到 error/warning。Preview/Mock 没有被用来证明 Vault、Engine、Host、Identity 或 Profile persistence。

## Round 1 regression verdicts

| Round 1 item | Round 2 verdict | Evidence |
| --- | --- | --- |
| QA-R1-01 Managed page action | **FAIL**：当前源码 `snapshot` 首请求仍 ID mismatch；下一请求正确 desync refusal | REAL_RUNTIME |
| QA-R1-02 stale identity | **PASS**：无 active Silo 时不再把 B 的历史 observation 展示为当前身份；active B 时仍显示 B 自身 observation | REAL_RUNTIME |
| QA-R1-03 diagnose runtime attribution | **PASS**：never-started Edge/fixed/PAC/provider 均为 null/no attributable evidence；A 不继承 active B；真正运行停止的 Chrome 显示自己的 stopped evidence | REAL_RUNTIME |
| QA-R1-04 bound provider offline copy | **PASS**：文案指向 exact binding，明确不探测本机其他 Clash，无 inventory/secret 泄漏 | REAL_RUNTIME |
| QA-R1-05 Create Silo UI | **PASS**：collapsed/expanded/unavailable/error-retry/narrow viewport 均符合预期 | UI_ONLY |

## Defects

### QA-R2-01 — P2（高影响）— current-source Managed page actions 仍不可用

- Evidence class：REAL_RUNTIME。
- 最短复现：在 fresh Vault 创建并启动 fresh Managed Silo；执行 `page <silo> snapshot`；再执行 `page <silo> windows`。
- Expected：`snapshot` 返回当前页面数据，`windows` 返回当前窗口数据；若首请求真实 timeout，也应返回准确 timeout 并 quarantine transport。
- Actual：两个独立 Silo/runtime 的首条 `snapshot` 都立即返回 response ID mismatch；下一条 `windows` 都以 transport desynced 拒绝，原因显示 outstanding request 收到 `id <missing>` response。
- 直接 evidence：Managed start、Host launch 和 page-script identity 均已 observed；A/B 的 mismatch 与后续 desync refusal 分别发生在 service restart 两侧。
- 最可能 owning lane：`core`（Rust `CamoufoxHostTransport` request/response correlation 与 quarantine seam；后续 targeted fix 可同时核对 Host 的 missing-id frame contract）。
- 污染：否。仅阻断 Managed page-action/网站状态覆盖；create/start/stop/persistence/identity/network 等独立证据仍可信。

另有一次 Managed B 首次 start 返回泛化 `managed_browser_open_failed`，在 clean state 下随后多次成功且未再复现；记录为未稳定复现的观察，不计 defect，也不改变矩阵 verdict。

## Coverage boundaries / environment limitations

- Standard CLI edit 不存在；这是测试控制面边界，不是产品 defect。
- Standard stock browser 的真实网站状态持久化和完整人工关窗 UX 未由 CLI 自动证明。
- Managed page action 网站状态测试被 QA-R2-01 阻断。
- successful external fixed-proxy forwarding 和 provider-bound browser pinning 缺少安全、隔离的外部目标；未作成功转发 claim。PAC 仅证明 synthetic PAC delivery 与 browser application。
- 已有 Managed development package 未执行 digest/signature/adapter package verification；不能替代 RC/release evidence。
- current-source debug development runtime 不证明未来 RC installer 中的 production binary。
- 未执行 clean Windows acceptance、FP Gate/FP5、release/installer 或 production build。

ENVIRONMENT_LIMITATION：无浏览器缺失；Chrome 与 Edge 均可用。安全隔离的 external forwarding target 不存在，因此对应成功路径保留为 COVERAGE_BOUNDARY，而不是本轮产品失败或整轮 BLOCKED。

## 收尾与建议

- P0：0；P1：0；高影响 P2：1（QA-R2-01）。
- 主控需要为 QA-R2-01 创建 targeted `core` fix task，并在修复后用 fresh current-source Managed runtime 重测 `snapshot` 和 `windows`；这不是对本 QA 分支的修复授权。
- 本分支只新增去敏 QA evidence；没有修改产品代码、没有 integration、没有推进 baseline，也没有开始 Round 3、RC 或 installer。

## Workflow verification

- `node scripts/agent-task.mjs verify`：exit 0；qa lane 没有自动命令，按 workflow 以确切候选版本、复现步骤和实际观察作为 evidence。
- `node scripts/agent-task.mjs check`：exit 0；1 个 evidence file 在 qa scope 内，无 boundary violation；主检出无 workspace contamination。
- Commit/publish SHA 与 local/remote exact equality：在提交发布后由 Git ref 核对。
