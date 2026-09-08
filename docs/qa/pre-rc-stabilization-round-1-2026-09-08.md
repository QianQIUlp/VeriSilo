# Pre-RC Stabilization QA Round 1

结论：**FAIL**

本轮在固定的 canonical baseline 上完成了 fresh QA。Vault、Standard、Managed 核心生命周期以及网络 fail-closed / data-minimization 主路径可用；发现 5 个可复现的 P2（4 个 REAL_RUNTIME、1 个 UI_ONLY）。未发现 P0 或 P1。FAIL 表示存在真实产品缺陷，不表示整轮或所有矩阵项失败。

## 测试对象与约束

| 项目 | 实际值 |
| --- | --- |
| Canonical remote baseline | `origin/baseline/dev` |
| Baseline exact SHA | `66e2619e4201964989f1f2510b4c72b3bc79023c` |
| QA lane | `qa` |
| Task branch | `agent/qa/pre-rc-stabilization-r1-787569` |
| Fresh worktree | `.verisilo-worktrees/qa-pre-rc-stabilization-r1-787569` |
| Fresh Vault | `qa-pre-rc-stabilization-r-787569` |
| Preview port | `15517` |
| Host | Windows 11 Pro 10.0.26220, x86_64 |
| Browsers | Chrome 152.0.7977.76；Edge 152.0.4191.66 |

开工时先 `fetch --prune origin`，随后 repository workflow 确认 `baseline/dev == origin/baseline/dev == 66e2619e...`。测试开始后未修改产品源码，也未改变固定测试对象。

遵守 NO-BUILD 边界：没有构建 Camoufox/browser package、Managed Browser release package、RC、installer 或 desktop production build，也没有 install/repair/uninstall。为运行 canonical baseline 的真实 CLI/backend，仅构建了 debug development binaries；Managed 使用工作区中已经存在的 development engine package，没有重新构建该 package。该 package 的运行证据明确为 `packageVerification=not_requested`、`digestVerified=false`、`signatureVerified=false`、`verifiedAdapter=null`，因此本轮不作 release 或 package qualification claim。

## Vault / Core lifecycle

结论：**PASS（REAL_RUNTIME）**

- fresh Vault 初始化成功，初始化后为 unlocked。
- lock 后错误密码 unlock 返回非零并保持 locked；正确密码恢复 unlocked。
- 分别在 Standard running、Managed running 和全部测试数据落盘后执行 service restart。Standard 的 live PID/Profile lock 被识别为 `recovery_required`，unlock 后恢复到正确 Silo；Managed 的 service stop 正常关闭 runtime，restart 后 Vault 为 locked，unlock 后数据仍可读。
- 最终 readback 有 7 个预期 Silo；全部独立 Profile path 存在，failed Managed create 没有产生残留 Silo。未观察到错误 Silo 操作或数据丢失。

## Standard Silo

结论：**PASS，存在 coverage boundaries**

| 检查 | 结果 | Evidence |
| --- | --- | --- |
| Chrome fresh create/readback | stock Chrome、独立 Profile path、direct network 与读回一致 | REAL_RUNTIME |
| Chrome start / ownership | 验证到 Chrome exact path/version/publisher；root PID 及子进程均带该 Silo 的 `--user-data-dir` | REAL_RUNTIME |
| Chrome service restart recovery | restart 后正确识别 live process/Profile ownership，unlock 后恢复同一 active Silo | REAL_RUNTIME |
| Edge fresh create/readback/start | exact Edge path/version；root PID 及子进程均指向 Edge Silo 独立 Profile | REAL_RUNTIME |
| single-active | Chrome running 时启动 Edge 被拒绝，原 Chrome 保持 active | REAL_RUNTIME |
| 正常关闭后的状态收敛 | 对精确核对过的 root window 发送正常 close；随后状态为 stopped，PID/Profile lock 消失 | REAL_RUNTIME |
| CLI edit | 当前 CLI 没有 edit command | COVERAGE_BOUNDARY |
| 真实网站 state persistence / 人工关窗 UX | CLI 无法诚实自动控制；本轮只用正常窗口关闭释放测试状态，不把它升级成网站持久化证明 | COVERAGE_BOUNDARY |

Chrome 和 Edge 在本机都存在，因此没有把浏览器缺失记为产品缺陷或环境限制。

## Managed Identity Silo

结论：**核心生命周期 PASS；page action FAIL**

| 检查 | 结果 | Evidence |
| --- | --- | --- |
| fresh create | 两个 Managed Silo 均成功；此前的 `Managed create -> ArtifactIntegrityError` 未回归 | REAL_RUNTIME |
| Artifact / Engine / Network binding | 每个 Silo 的 Resolved Identity Artifact id/SHA、Camoufox engine、direct network 均可独立 readback | REAL_RUNTIME |
| start / stop / reopen | A start、stop、reopen 均成功；Host launch observed，launched adapter 为 Camoufox | REAL_RUNTIME |
| runtime identity | page-script observation 的 `siloId`、language/timezone/UA 与当前 Managed Silo 一致 | REAL_RUNTIME |
| service restart / Vault persistence | service stop 正常收敛为 stopped；restart + unlock 后两个 Managed Silo 和各自 Artifact binding 均保留 | REAL_RUNTIME |
| multi-Silo / single-active | B running 时启动 A 返回 `managed_another_silo_running`；A diagnose 不带 B 的 runtime/website identity，B diagnose 只带自己的 observation | REAL_RUNTIME |
| Managed page action | `page snapshot` 与 `page windows` 失败；重启 service 后新 runtime 仍可复现 response-id mismatch | REAL_RUNTIME / FAIL |
| package qualification | 仅消费现成 development package；未执行、也不能从本轮推断 signed/digest verified release package | COVERAGE_BOUNDARY |

这里严格区分 configured 与 observed：Artifact/Engine/Network readback 是配置/持久化证据；Host launch 和 page-script identity 是运行观察；development package 未被验证为 release artifact。

## Network / diagnostics

结论：**data-minimization 与 fail-closed 主路径 PASS；诊断语义存在 P2**

| 检查 | 结果 | Evidence |
| --- | --- | --- |
| Direct diagnose data minimization | Standard 与 Managed direct diagnose 都没有 `clash`、provider inventory、account metadata 或 controller data | REAL_RUNTIME |
| fixed proxy offline | 指向隔离的 unused localhost port；start 在浏览器创建前失败，activeSiloId 为 null，零目标浏览器进程；evidence 标记 endpoint failed 且保留 no-direct-fallback 等 safeguards | REAL_RUNTIME |
| Managed fixed proxy offline create | preflight 失败且没有写入残留 Managed Silo | REAL_RUNTIME |
| PAC | create/readback 与 diagnose 保留 PAC URL；未建立成功 PAC browser-routing runtime | REAL_RUNTIME + COVERAGE_BOUNDARY |
| provider-bound synthetic fixture | 只请求 bound controller/group/node；输出只含 bound group/node 和 delay，不含 fixture 中故意放置的无关 inventory，也不回显 synthetic secret | REAL_RUNTIME |
| provider authentication failure | synthetic controller 返回 401；diagnose 明确提示检查 bound controller address/secret | REAL_RUNTIME |
| provider configuration drift | synthetic selector 返回非 bound node；diagnose 报告 selector 未保持 bound node，同时不输出无关 selected-node 名称 | REAL_RUNTIME |
| provider offline | 没有枚举或读取用户本机真实 Clash inventory，但 recovery copy 错误引导到本机 Clash discovery | REAL_RUNTIME / FAIL |
| cross-Silo isolation | inactive Managed A diagnose 不带 active B 的 runtimeState/websiteIdentity；两个 Silo Artifact/Profile 均独立 | REAL_RUNTIME |

全程未调用全局 Clash inventory 命令，未读取、复制、保存或粘贴用户真实 Clash/provider 数据。Controller 行为只使用 localhost synthetic listener 和 synthetic names/credentials。

## HMR / Preview UX

结论：**主要状态可理解；高级工程字段暴露为 P2 UI finding**

| 场景 | 观察 | Evidence |
| --- | --- | --- |
| first-use | 明确说明本机加密、不会上传、无云端恢复；密码长度和 disabled create 可见 | UI_ONLY |
| Vault locked | unlock 目的、无密码恢复边界和输入状态清楚 | UI_ONLY |
| empty / loading / error | 有明确 loading、empty CTA 和 mock error presentation | UI_ONLY |
| Standard vs Managed | 首页与 create flow 能区分 Standard/Managed；Managed unavailable 有 disabled 状态和原因 | UI_ONLY |
| running / stopped | 状态标签清楚；running 时编辑/归档禁用并说明先关闭浏览器 | UI_ONLY |
| Create Silo 信息层级 | Standard 与 Managed 主表单直接暴露较多普通用户不需要的 raw/engineering fields | UI_ONLY / FAIL |

Preview 使用真实 UI components + Mock API，只用于展示/交互判断；没有用它证明 Vault 写入、浏览器启动、Host、Identity 应用或 Profile persistence。Preview console 未观察到 error/warning。

## Defects

### QA-R1-01 — P2 — Managed page action response correlation failure

- Evidence：REAL_RUNTIME
- 最短复现：创建并启动 fresh Managed Silo；执行 `page snapshot` 或 `page windows`。
- Expected：CLI 返回当前 Managed runtime 的页面树/窗口列表，或返回与当前请求准确关联的受控失败。
- Actual：CLI 返回非零：`Camoufox Host response ID did not match the outstanding request`。service restart 后新启动 runtime 再次复现。
- 直接证据：Managed start 成功、Host launch 为 observed、websiteIdentity 为 page-script observed；紧随其后的两个 page command 均产生同一 response-id mismatch。
- 最可能 owning lane：`host`（Camoufox Host page-command response protocol；修复任务应同时核对 core transport seam）。
- 污染：否。start/stop/lifecycle 证据仍可信；Managed 网站 page-action 覆盖被阻断。

### QA-R1-02 — P2 — CLI identity 输出保留且误归因旧 Managed observation

- Evidence：REAL_RUNTIME
- 最短复现：启动 Managed B 并取得 page-script observation；停止 B；对另一个 fixed-proxy Silo 执行失败的 start；运行全局 `identity`。
- Expected：没有当前 active Silo 时，CLI 不应把旧 B observation 描述成“这次打开时”的当前身份；至少应明确 Silo id/name 与 historical/stale 状态。
- Actual：activation 为 failed、activeSiloId 为 null，但 `identity` 仍显示 B 的 observation，并用“这次打开时”表述且没有 Silo 归属。
- 直接证据：status 的 websiteIdentity.siloId 仍为 Managed B，而 activation 已切换到另一个 Silo 的 failed 状态。
- 最可能 owning lane：`core`（global status/CLI attribution）。UI Identity panel 按 siloId 过滤，未据此认定 UI 或跨 Silo 数据泄漏。
- 污染：否。旧 observation 有明确 siloId；问题是 CLI presentation/attribution，不是写入错误 Silo。

### QA-R1-03 — P2 — diagnose 把全局 stopped 状态显示给从未启动的 Silo

- Evidence：REAL_RUNTIME
- 最短复现：停止唯一 active Silo；创建一个新 Silo但从未 start；对新 Silo执行 `diagnose`。
- Expected：runtime state 为未运行/未尝试，或明确没有该 Silo 的 runtime evidence。
- Actual：新建的 fixed proxy、PAC、provider-bound Silo 都显示 `runtimeState=stopped`、`runtimeMessage=浏览器已停止。`。
- 直接证据：这些 Silo 创建后没有 start 记录，却共享上一 runtime 收敛后的 stopped presentation。
- 最可能 owning lane：`core`（per-Silo diagnostic state association）。
- 污染：否；只影响诊断语义，配置 readback 与后续独立测试仍可信。

### QA-R1-04 — P2 — provider-bound offline recovery copy 指向无关的本机 Clash discovery

- Evidence：REAL_RUNTIME
- 最短复现：创建显式绑定 synthetic Mihomo controller 的 Silo；关闭该 controller；执行 `diagnose`。
- Expected：提示恢复该 Silo 明确绑定的 controller/address，且不引导发现无关 provider。
- Actual：诊断正确没有 fallback probe，但文案称本机没有可用 Clash controller，并引导“查找本机 Clash”或走内核管道。
- 直接证据：binding readback 仍为精确 synthetic controller；offline diagnose 未输出 inventory，却给出与 binding 不一致的恢复步骤。
- 最可能 owning lane：`core`（network diagnostics/recovery messaging）。
- 污染：否；安全的数据最小化边界保持，缺陷是恢复语义。

### QA-R1-05 — P2 — Create Silo 主流程暴露过多高级工程字段

- Evidence：UI_ONLY
- 最短复现：Preview 打开 Create Silo，分别查看 Standard 与 Managed 表单。
- Expected：普通用户先完成名称、浏览器/身份与网络等必要选择；高级 fingerprint/runtime 细节应按需展开或由安全默认值承载。
- Actual：Standard 主表单直接显示 `native`、`inherit`、`unavailable` 等 raw tokens 及 WebRTC/DNS/SSO 字段；Managed 主表单直接显示 GPU/WebGL、CPU、UA、Canvas/Audio noise、WebRTC、font mode 等工程字段。
- 直接证据：上述字段在默认 create flow 中可见，不需要进入 advanced disclosure。
- 最可能 owning lane：`ui`。
- 污染：否。只基于 Preview，不升级为 runtime/backend 缺陷事实。

## Coverage boundaries / environment limits

- Standard CLI edit 不存在；这只是测试控制面边界，不自动视为产品 bug。
- Standard stock browser 的真实网站状态持久化和完整人工关窗 UX 未自动化证明。
- Managed page action 的真实网站状态测试被 QA-R1-01 阻断。
- PAC successful browser routing、successful fixed-proxy forwarding、provider-bound browser pinning 未取得完整 runtime evidence；本轮已覆盖各自配置读回、离线/认证/drift 与 data-minimization 的最低充分路径。
- Managed 使用现成 development engine package；其 package verification 未请求且未通过，不能替代 RC/release evidence。
- 没有执行 development Desktop；CLI 能回答的 runtime 问题已由真实 backend 覆盖，剩余视觉问题由 Preview 覆盖。
- 按本轮边界没有执行 clean Windows acceptance、installer、production build、release package 或 Camoufox FP qualification。

环境限制（ENVIRONMENT_LIMITATION）：无浏览器缺失；Chrome 与 Edge 均可用。现成 Managed development package 足以运行本轮生命周期，但不足以证明 release/package qualification；后者已列为 COVERAGE_BOUNDARY，而非本轮 BLOCKED。

## 收尾与建议

- P0/P1：无。
- 高影响 P2：QA-R1-01（Managed page actions 全部不可用）。
- 其他 P2：QA-R1-02、QA-R1-03、QA-R1-04、QA-R1-05。
- 建议主控后续分别创建 targeted fix：`host` 处理 QA-R1-01；`core` 处理 QA-R1-02/03/04；`ui` 处理 QA-R1-05。需要修复任务验证跨层 transport 时，再由主控决定是否提升到 integration ownership。
- 本 QA 分支只记录去敏 evidence；没有修复产品代码、没有 integration、没有推进 baseline，也没有开始 QA Round 2 或 RC/installer gate。

## Workflow verification

- `node scripts/agent-task.mjs verify`：exit 0；qa lane 按 repository workflow 没有自动命令，以本报告的固定版本、复现步骤和实际观察作为 evidence。
- `node scripts/agent-task.mjs check`：exit 0；1 个 evidence file 在 qa scope 内，无 boundary violation；主检出无 workspace contamination。
- Commit / publish exact SHA：由本文件所在 task branch 的最终 commit 和 remote equality check 证明。
