# Pre-RC4 real product readiness QA（post-rc3 current source）

- 类型：QA / evidence-only。未修改任何产品源码；产品缺陷只记录并交回 owning lane。
- 执行日期：2026-09-13
- QA branch：`agent/qa/pre-rc4-qa-be15c6`
- 起始 canonical baseline：`b958e607dde6e2a31ad0d968c9ef018d5df07359`
  （`start` 前 fetch/prune 后 `baseline/dev` == `origin/baseline/dev` == 该 SHA，与任务下发一致）
- 本文档为唯一 QA 结论记录；本任务不创建 release/tag、不推进 main、不打开 rc4 release gate。

## 最终结论：`PRE_RC4_SOURCE_READY_WITH_NONBLOCKING_FINDINGS`

当前 source 未发现阻止进入 rc4 candidate freeze 的产品缺陷。发现一个 P2 evidence-semantics
不一致（见 F-1），建议在 rc4 source freeze 前由 `core` lane 修复；它不伪造能力、不破坏
生命周期，也不影响本轮任何 PASS 结论的真实性。

READY 仅指「当前 source 没发现阻止 candidate freeze 的产品问题」；它不是 rc4 release
acceptance。rc4 阶段仍必须完成（见文末「rc4 阶段必须完成」）。

## 测试环境

- Windows 11 专业版，`10.0.26200`，AMD64（开发机，非 pristine Sandbox）。
- 运行层级：全部证据来自 **Mode B — Tauri dev**（worktree 内 `node scripts/dev-desktop.mjs
  core --port 15421 --vault qa-pre-rc4-qa-be15c6`，真实 React → Tauri → application → 命名
  Vault/runtime）+ **CLI loopback**（`target/debug/verisilo-cli.exe --vault
  qa-pre-rc4-qa-be15c6`，驱动同一真实 backend 的本地 HTTP API）。未构建 installer，未创建
  release artifact，未使用 Mock。
- UI 交互证据：通过 UIA/AX 对真实 WebView 窗口做元素级驱动与观察（无截屏能力，见
  coverage boundary）。
- Preview（Mock API）**未使用**——所有 UI 状态都在真实桌面实例上核对。
- Managed runtime 输入：既有已验证 development engine package（只读复用，复制进本任务
  worktree 的 cargo staging 树；`engineRevision=verisilo-camoufox-152.0.4-beta.28-r1-formal-v3`，
  `artifactSha256=d670364a…`，与 rc3 构建产物一致）。未为本 QA 构建新 engine package。
- 测试 Vault：`qa-pre-rc4-qa-be15c6`（任务独立命名 Vault，仅含 QA 测试 Silo，可在验收后删除；
  口令为一次性测试口令，不记录于本文）。

## 结果总表

| 检查项 | 结论 | 层级 |
| --- | --- | --- |
| Vault initialize/lock/unlock、lock 后敏感操作拒绝、错误口令拒绝 | **PASS** | CLI real backend |
| Standard Silo create/launch/persistence/stop/close ownership/二次启动 | **PASS** | CLI + 真实 Edge |
| Managed Silo create/launch/status/stop + evidence attribution | **PASS** | CLI/UI + 真实 Camoufox |
| Fresh Identity Recheck（packaged runtime） | **EXPECTED PRE-RC4 PACKAGE BOUNDARY** | CLI/UI + rc3-frozen Host |
| Fresh Identity Recheck（source 证明） | **PASS** | Host 协议测试 + 桌面测试 |
| Desktop restart recovery（b958e60） | **PASS（运行时可达路径）** + source 测试 PASS | 真实重启 + 单元测试 |
| Create New Identity From This Silo | **PASS** | 真实 Desktop UI + real backend |
| Current Session Integrity 语义 | **PASS**（发现 F-1，见下） | 真实 Desktop UI |
| Local Report v2 导出（JSON+HTML 实际落盘） | **PASS** | 真实 Desktop UI（WebView2 下载） |
| 新 UI 真实交互 smoke | **PASS**（restore 受 a11y 边界限制，见 F-3/B-2） | 真实 Desktop UI |
| Identity Field 空间切换不启动浏览器 | **PASS** | 进程级证据 |

## 各项详情

### Vault（CLI real backend）

`initialize` → `unlocked`（autoLock 生效）→ `lock` → `state=locked, autoLockAt=null`，
此时 `silos`、`start` 全部被拒绝（`保险库已锁定…`，exit 1）→ `unlock` 后状态与数据完整恢复。
错误口令被拒绝（`The vault could not be decrypted.`，exit 1）。有 workspace 内容后再次
lock/unlock 复核一遍，结论相同。HTTP API 层以 discovery bearer token + vault 绑定校验请求。

### Standard Silo（CLI + 真实 Edge 153.0.4234.32）

- `create --standard`：创建成功，`engine.adapter=stock`，Edge 版本自动核验。
- `start`：真实 Edge 窗口启动；`browserVerification.state=verified`（路径/类型/版本/发布者
  基线）；Profile 目录真实落盘。
- 单活 ownership：`stop` 不强杀用户可见浏览器窗口，返回「请关闭 Silo 浏览器窗口…VeriSilo
  不会终止无关浏览器进程」；以 WM_CLOSE 关闭窗口（等同用户操作）后，桌面自动收敛
  （`activeSiloId=null`，`state=stopped`，「受管浏览器进程已退出」），QA Profile 的全部
  msedge 进程归零，用户自己的 Edge 进程不受影响（按 command line 归属甄别）。
- 二次启动复用同一 Profile，验证持久化；再次关闭收敛一致。

### Managed Silo（真实 Camoufox，rc3-frozen dev staging package）

- `create`：生成 Resolved Identity Artifact v5 绑定（`identity-9636afaf…`）。
- `start`：22s 完成启动；package 校验记录（`installed-package` verifier，含
  engineRevision/manifest/tree SHA）；identity evidence `state=matched`，`observedAt`
  与 `websiteIdentity`（`source=page-script`）一致；network evidence `provider=direct`、
  `runtimeId` 与 identity evidence 一致。
- `stop`：Direct stop 干净终止（camoufox/host/supervisor 9 → 0 进程）。
- Attribution：identity/engine/network evidence 均只绑定当前运行 Silo；其余 Silo 无证据借用。

### Fresh Identity Recheck

**运行时（packaged path）**：CLI 与 UI 各触发一次。桌面调用 Host `reobserve_identity`，
rc3-frozen Host 不支持该命令；产品行为诚实——activation message 明确
「网站可见身份重新观察失败；本次没有取得新的身份证据。」，identity evidence 保留
launch 时 `observedAt`，磁盘未写 `reobserved.json`，用户页面未被 probe 导航破坏。
分类：**EXPECTED PRE-RC4 PACKAGE BOUNDARY**（rc3 冻结包早于 `reobserve_identity`
commit `494943c`，2026-09-12 15:43）。**不得记为 PASS**：rc4 fresh engine package 进入
packaged product 后必须在 exact candidate QA 上重验。

**source 证明**：`apps/camoufox-host/test_page_command.py`（含 `check_reobserve_identity`：
fresh observation 写入 `reobserved.json`、evidence observedAt == reobservedAt、session/
artifact 绑定断言）PASS；`cargo test --lib reobserve` / `recheck_active` 9 项 PASS
（fresh evidence 绑定活动 session、拒绝不绑定响应、mismatch 诚实上报、失败不伪造证据）。

### Recovery consistency（b958e60）

真实实验：Managed 会话运行中强制终止 dev Desktop → 全部 camoufox/host/supervisor 子进程
被干净回收（0 孤儿）→ 重启 Desktop + unlock → 「上次记录的浏览器进程和 Profile 锁均已消失；
会话已解释为停止。」（诚实 reconcile-stopped），activation 的 identity evidence 保留
（matched，同 observedAt/runtimeId/sessionId）。重启后无活动 session，`websiteIdentity`
按设计为空。

b958e60 的核心行为（恢复路径对每个 session 取 observed/reobserved 中最新有效者，且只接受
uuid4().hex session 目录）在运行时的「recheck 后重启」分支依赖 rc4 fresh package 才能到达；
本轮以 source 测试为准：`website_identity::tests` 9 项 PASS（含
`latest_observation_prefers_a_newer_reobserved_observation`、
`latest_observation_selects_the_newest_per_session_before_comparing_sessions`、
recheck 不可用时的 fallback）。**运行时该分支记 NOT PROVEN，留给 rc4 candidate QA。**

### Create New Identity From This Silo

真实 UI：Managed Silo 更多操作 → 创建新身份。表单预填「QA-Managed-Recheck - 新身份」，
GPU/CPU/屏幕/时区/语言与源 Silo 一致（由引擎生成/12 核/1280×800/纽约/en-US），网络方式
Direct；**无任何 credential/secret 字段被填充**（模板只携带代理位置，不携带认证；
`proxyCredentialUsed`/`mihomoSecretUsed` 仅为布尔标志）。表单明确声明「原 Silo 不会被修改」。

real backend 验证：创建后 2 → 3 个 Silo；原 Silo 记录逐字节不变；新 Silo 拥有全新
`seedReference` 与全新 Artifact（`identity-0a3f81c2…` ≠ 源 `identity-9636afaf…`）。
新 Silo 身份节点诚实显示 `Unavailable · 尚无观测`；因另一 Silo 运行中，其「打开浏览器」
被正确禁用并提示单活语义。

### Current Session Integrity

运行中的 Managed Silo 四行摘要全部语义诚实：
身份 = `Matched`（「网站可见身份与声明一致 · 刚刚重新读取」，新鲜度来自 observedAt）；
引擎 = `已启动`（dev 下 packageVerification 不满足 verified 门槛时不显示「运行正常」）；
网络 = `出口观察进行中`（「已应用网络配置；尚未取得出口观察」——配置声明与观察分离）；
运行归属 = `当前 Silo · 本次运行`（证据与当前运行绑定一致）。
无 Matched→Verified、Observed→Verified、Unavailable→Pass 的语义抬升。
source：`CurrentSessionIntegrity.test.tsx` 16 项 PASS。

### Local Report v2（真实 Desktop 导出，实际文件验证）

导出面位于 本机工具 → 隐私检查报告：选择 Silo → 勾选确认（下载按钮在此之前禁用）→ 下载。
两份文件均真实落盘于用户 Downloads（非仅"下载请求成功"通知）：

- `exported-report.json`（schemaVersion=2，5.4KB）：`JSON.parse` 成功；`silo.name=
  QA-Managed-Recheck`、`browser.kind=managed`、`runtime.state=running`、
  `identity.state=matched`（仅信号名+对账状态+observedAt）；evidenceBoundary 完整。
- `exported-report.html`（4.9KB）：无 `<script>`、无外部 http(s) 引用、无内联事件处理器，
  自包含可离线打开；渲染正确的 Silo/身份/引擎/边界章节。
- 敏感信息扫描（两份）：`seed/seedReference/credentialRef/secret/artifactId/
  artifactFileSha256/runtimeId/sessionId/profileDirectory/executablePath/本机路径/完整
  Artifact hash` 均**不存在**。

### 新 UI 真实交互 smoke

- Identity Field 切换 Silo：`上一个/下一个身份` 按钮与 Arrow 键路径正常（计数器、聚焦
  Silo、节点内容联动正确）。
- 切换 focus 不启动浏览器：切换前后 QA 归属浏览器进程始终为 0（用户自己的 Edge 进程除外，
  按 command line 归属甄别）。
- 运行 Silo ≠ 当前查看 Silo：查看未运行 Silo 时显示单活提示且「打开浏览器」禁用；运行中
  Silo 的节点显示运行状态。
- 身份/网络/当前运行节点：各自打开正确内容；Managed 与 Standard 的身份语义文案正确区分
  （「持久身份 · 独立数据 · 运行证据」vs「设备身份跟随本机」——Standard 无 Managed 指纹
  identity 暗示）；Escape 关闭 lens 且焦点返回触发节点。
- running/recheck/create/archive action：running 后动作按钮切换为 重新检查/停止；UI
  recheck 诚实失败（与 CLI 一致）；创建新身份见上；归档后 Silo 从场中消失（CLI 确认
  `archivedAt` 落盘）。
- tools drawer（本机工具）：打开正常，含报告导出、出口检查（显式同意门控）、检查记录、
  当前状态（Vault/浏览器/系统浏览器安装数）。
- 长名称：>64 字符被后端明确拒绝（「Silo name must contain 1–64 characters.」）；53 字符
  混合 CJK/ASCII 名称正常创建、渲染、节点显示，无布局崩溃。
- 800px 宽窗口：所有关键按钮（打开/检查/更多操作/三个证据节点/底栏六项/新建/锁定/检查身份）
  均存在、有有效 bounds、无遮断；长名称自动换行。视觉质量本身受无截屏边界限制（见下）。
- prefers-reduced-motion：按任务要求未建 OS 自动化（代码证据已存在），本轮不重复。

## Findings

### F-1 · P2（建议 rc4 source freeze 前修复）· evidence-semantics：recheck 把 `packageVerification` 从 `not_requested` 抬升为 `verified`

- 复现（两次独立 session 复现）：
  1. 启动 Managed Silo（dev staging package，`installed-package` verifier）。
  2. status：`engineEvidence.packageVerification = "not_requested"`，details
     `digestVerified=false, signatureVerified=false`，`verifiedAt=启动时刻`。
  3. 触发重新检查（CLI `recheck` 或 UI「重新检查」，无论成败）。
  4. status：`packageVerification = "verified"`，**details 完全不变**
     （仍 `digestVerified=false, signatureVerified=false`，verifiedAt 不变）。
- Expected：同一运行会话内，未发生新的包 digest/CMS 校验时，`packageVerification` 不应
  从 `not_requested` 变为 `verified`（产品纪律：`verified`/`not_requested` 不混用）。
- Actual：`recheck_active` 把状态抬升为 `verified`。
- Owning layer：`core`（`apps/desktop/src-tauri/src/launcher.rs`，`recheck_active` 中
  `package_verification = RuntimeEvidenceState::Verified` 仅凭 `adapter.health()==Healthy`；
  对照 launch 路径要求 `digest_verified && signature_verified` 才置 Verified）。
- 最小 suspected seam：recheck 路径应保持 `not_requested`（或引入独立的「已安装已固定」
  状态），不应复用 `verified`；报告导出（`reports.ts` engineStages 逐字复制 activation）
  因此会在导出文件里呈现抬升后的 `verified`。
- Evidence：`evidence/managed-activation-first-launch.json`（not_requested）vs
  `evidence/managed-activation-after-recheck-cli.json` / `after-recheck-ui.json`（verified，
  details 不变）。
- 影响评估：不伪造新能力（包在安装时确实通过完整校验并被固定，tree 不可变），但同一字段
  在同一会话内语义翻转，且进入导出报告，属于 evidence honesty 缺陷而非功能缺陷。

### F-2 · LOW · a11y：部分按钮对程序化激活（UIA Invoke/AXPress、AXToggle、聚焦后 Enter/Space）无响应

- 复现：真实 WebView 上，「选择 Silo」dock 的 identity-token 按钮、「环境概览」底栏
  toggle、identity-focus 雕塑按钮（AXPress 形式）均"dispatched"但无效果；同树的
  `上一个/下一个身份`、`打开浏览器`、`重新检查`、`下载` 等 onClick 按钮程序化激活正常。
  `环境概览` 经 AXPress/AXToggle/Enter/Space 四种方式均无响应（「本机工具」曾以 AXToggle
  成功——同组行为不一致）。
- Owning layer：`ui`（`apps/desktop/src/features/silos/SiloList.tsx` identity-token
  roving-tabindex 按钮与底栏 toggle）。
- 影响：键盘/辅助技术路径部分受损（dock 有 ArrowLeft/Right/Home/End 处理、roving
  tabindex；底栏 toggle 无法以键盘确认激活）。鼠标路径代码上已接线（onClick），但本环境
  无截屏/坐标点击能力，**真实鼠标点击未验证**。
- Evidence：交互记录见本文「新 UI 真实交互 smoke」；因 restore 依赖「环境概览」视图，
  归档后的 **restore 未能在 UI 上执行**（数据完整：`archivedAt` 已确认，恢复逻辑有
  SiloList 测试覆盖）。

### F-3 · INFO · coverage boundary（非缺陷）

- 本环境无法截屏：所有 UI 证据为元素级（名称/状态/bounds/焦点），像素级布局质量、
  hover/动画、长名称的视觉截断效果未评；800px 检查以「关键控件存在+可达+有效 bounds」为准。
- 外部 CLI 变更（如 CLI 创建 Silo）约在数十秒轮询周期内出现在 UI；桌面内发起的变更即时
  反映。非缺陷，记录为预期行为边界。
- Managed stop 的 Direct stop 在 dev staging 包上干净（9→0 进程）；这与 rc2 Sandbox 验收
  一致，不外推到所有环境。

## 明确不属于本轮 PASS 范围（与任务下发一致）

1. 新 Windows system logo / 品牌资产（另一 UI task）；
2. rc4 installer（不存在，未构建）；
3. rc4 fresh Managed Engine Package（不存在，未构建）；
4. outer Authenticode 仍 unsigned；
5. strict unelevated standard-user lifecycle 仍 NOT_PROVEN；
6. universal site / login / CAPTCHA / payment compatibility 未证明。

本轮未发现当前 source 声称上述不存在能力的产品/文案缺陷。

## rc4 阶段必须完成（承接）

1. **fresh current-Host Managed Engine Package**：包含 `reobserve_identity` 的新包；
   Fresh Identity Recheck / Recovery 重启分支 / Create New Identity 的 seed+Artifact 链路
   在真实 packaged runtime 上重验（本轮为 source 证明 + EXPECTED PACKAGE BOUNDARY）。
2. **new Windows system brand assets**（另一 UI task 同步后核销）。
3. **exact rc4 installer build**（用户显式打开 RC gate 后 Mode C）。
4. **exact-candidate packaged QA**：在确定 candidate 上做安装后验收（含 rc2/rc3 既有边界：
   strict standard-user、Authenticode、site compatibility 声明保持不外推）。

## 证据索引（本目录）

- `evidence/managed-activation-first-launch.json` — 首次启动 activation（matched/attribution/not_requested）
- `evidence/managed-activation-after-recheck-cli.json` — CLI recheck 后（诚实失败 + F-1 翻转）
- `evidence/managed-activation-after-recheck-ui.json` — UI recheck 后（同上，F-1 第二次复现）
- `evidence/managed-activation-after-desktop-restart.json` — 桌面重启后（reconcile-stopped）
- `evidence/exported-report.json` / `exported-report.html` — 真实导出的 Local Report v2
- source 级测试输出见运行记录：Host `test_page_command.py` PASS；
  `cargo test --lib website_identity`（9）、`reobserve`（4）、`recheck_active`（3）PASS；
  desktop 前端 122 tests PASS（含 reports 17、CurrentSessionIntegrity 16、ManagedSiloForm 10、SiloList 5）。
