# M3-0 Camoufox Host / EngineAdapter 集成任务合同

- 状态：**Main-brain Gate Accepted on 2026-08-09**
- 冻结日期：2026-08-09
- `main` 起始基线：`8de389db366d1d9ff510b1e885fab7f49a89aad0`（PR #10 merge commit）
- 执行分支：`codex/camoufox-m3-engine-adapter`
- 冻结实现起点：`b3d094cb0a7f3b7f9c113c53e4c4575d16babb67`
- Accepted checkpoint：`e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`
- 上一 Gate：M2-W **Accepted**；Linux 与 Windows standalone 证据保持 `verified: false`

本文既是 M3-0 的仓库任务卡，也是交给执行 Agent 的提示词。执行前必须依次阅读：

1. [身份平台北极星](identity-platform-north-star.md)；
2. [Camoufox-first 决策](camoufox-managed-engine-decision.md)；
3. [Agent 协作协议](agent-operating-model.md)；
4. [Camoufox 当前状态](camoufox-program-status.md)；
5. 本文；
6. [EngineAdapter 现状](engine-adapters.md)、[Host v1](camoufox-host-v1.md)和 [M2-W Gate](camoufox-m2w-windows.md)。

## Gate 结果

原始候选 `bc65e07fbc21ee8581b3ac60c91afd15d0effa20` 未通过主脑审阅，不能作为
accepted checkpoint。执行 Agent 以追加提交 `fc67418b946c2f82f69e13ec5751cff24f3f1e7f`、
`b50765986c7da0bf081ff0a9057dc38073813311` 和最终
`e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd` 关闭了 package/browser tree 分离、
secret 持久化扫描和真实 `RuntimeManager` fake Host 垂直切片等缺口。主脑逐项对照
冻结合同并做了小型只读接缝核对，接受最终 checkpoint；没有重跑完整测试矩阵。

Accepted evidence 是 contract-level fake Host 测试与提交绑定，不包含真实浏览器 run-id：

- `pnpm check`、`pnpm build`、`pnpm engine:verify` 通过；`pnpm test` 为 `150` tests；
- Desktop `cargo test --locked` 为 `184 passed / 0 failed`，fmt 与 clippy 通过；
- Artifact 锁定环境测试为 `25/25`；
- production signer pin 仍未配置并保持 fail-closed；
- Host evidence 仍为 `observed-on-this-host`、`verified: false`，`verifiedAdapter = null`。

本 Gate 只接受 package/transport/lifecycle/evidence 合同。它没有启动真实 Camoufox，
没有证明签名 Host package、发布 runtime、installer 或 shipped Managed Identity。
后继任务是 [M3-WI 原生 Windows 真实 Host 集成](camoufox-m3-wi-windows-task.md)。

## 角色与任务边界

你是本阶段的**执行 Agent**，不是重新决定项目方向的主脑。你的职责是在冻结范围内检查真实代码、实现、测试、提交并返回证据包；不得把 M3-0 扩张为 UI、安装器、网络身份、发布签名或另一个浏览器内核项目。

基线不匹配、任务要求彼此冲突，或必须修改禁止范围才能继续时，停止并向主脑报告。不要为了“看起来完成”而降低 Gate、伪造 receipt，或把 `configured`、`applied`、`observed` 和 `verified` 合并。

## 当前真实差距

仓库现有两套协议不能直接视为已经兼容：

- 桌面 external-engine 路径把新启动的子进程视为受控引擎，向 stdin 写一次 length-prefixed bootstrap，随后从 stdout 读取 ACK 和有序 phase receipt。
- standalone Camoufox Host v1 是长期存活的双向 JSON Lines 控制进程，支持 `hello/launch/status/close/shutdown`；`launch` 只接受 `artifactId/profileId/expectedArtifactFileSha256`。
- external package schema v2 把 `bin/camoufox.exe` 当成浏览器原始可执行文件，只绑定该文件；它没有定义 Host entrypoint、Host/runtime 树或浏览器树的包级绑定。
- 当前 Camoufox Silo 配置保存通用 `IdentityTemplate`，尚未保存 Artifact 文件的明确 ID + raw SHA 绑定。真实重放权威却是 v3 Resolved Identity Artifact。

M3-0 的目的不是用命名或适配器包装掩盖这些差距，而是形成一个窄、严格、可测试的桌面集成接缝。

## 核心目标

交付一个**不启动真实 Camoufox 的 contract-level 垂直切片**：现有 Tauri `RuntimeManager` 能通过专用、受界定的 transport 驱动一个 fake Camoufox Host v1，完成包重新验证、严格 `hello`、Artifact-bound `launch`、`status`、精确 `close/shutdown`、失败映射和诚实 capability/evidence 状态。

M3-0 通过只开放后续原生 Windows M3-WI 集成 Gate；它本身不证明真实桌面 Camoufox 已接入，更不构成可发布 Managed Identity Silo。

## 已冻结的架构选择

### 1. Host 是 Camoufox package entrypoint

- Camoufox package entrypoint 是 VeriSilo 管理的 Host，不是浏览器树中的 `camoufox.exe`。
- 一个活动 Camoufox Silo 对应一个由桌面精确启动和持有的 Host 子进程；Host 再拥有浏览器 Job/process tree。
- 不增加常驻全局 daemon，不共享不同 Silo 的 Host，不通过端口发现或进程名猜测接管旧进程。
- Controlled Chromium 的现有 native bootstrap/receipt v1 路径保持原样。

### 2. 为 Host 使用独立 transport，不伪造 generic receipt

在 Rust launch plan / runtime 中增加显式 transport discriminant，至少区分：

- stock / 无受控协议；
- 既有 `native-bootstrap-v1`；
- `camoufox-host-jsonl-v1`。

Camoufox transport 必须直接实现 Host v1 的 bounded JSON Lines 请求/响应、严格 request ID 关联、单一 stdout reader、超时和 EOF 处理。禁止把 Host `launch` 结果伪造成它没有发送的 `observe → apply → verify` receipt，也禁止让两种 framing 共用同一模糊解析器。

### 3. Artifact 是 Camoufox 身份权威

引入严格的 `CamoufoxArtifactBindingV1`（命名可按仓库惯例调整，但语义不可改变），至少包含：

- `artifactId`；
- 64-hex `artifactFileSha256`；
- 固定 Artifact schema `verisilo-camoufox-resolved-identity/v3`。

该绑定必须持久化在 Camoufox Silo 的 engine 配置中；旧的无绑定 Camoufox 配置可以继续反序列化，但启动必须以明确的 `identity_artifact_unavailable` 类错误 fail closed，不能在启动时临时生成、猜测或选择 Artifact。

过渡期保留的 `IdentityTemplate` 只能作为旧控制面声明，不能成为 Host 重放权威，不能发送给浏览器，也不能单独把 capability 提升为 `applied` 或 `verified`。不得修改 Artifact v3、Policy v3、Projection v3 或 ObservedWebsiteDigest v2 的 schema、canonicalization 和 digest 规则。

Desktop 从应用拥有的固定根目录解析 artifact/profile/state/tree 路径；Silo/API/页面不得提供任意路径。`profileId` 必须由 Silo ID 确定性派生。Host `launch` 仍只接收 ID 与 expected SHA。

### 4. 不把 Silo seed 或代理秘密送入 Host

Camoufox Host transport 不使用现有 Vault seed 派生 token 作为身份输入。Silo seed、Artifact 内部 seed、代理密码、Controller secret 和 bearer token 均不得出现在 Host/browser argv、日志或 protocol evidence 中。

现有短 token bootstrap 继续服务于 `native-bootstrap-v1`，不得为了复用代码而把它塞入 Camoufox Artifact 重放链路。

### 5. 版本化 Host package contract

为 Camoufox Host 定义新的严格 package manifest 版本；不能静默重解释 schema v2 的 `bin/camoufox.exe`。新 manifest 至少要绑定：

- `entrypoint.kind = camoufox-host-v1`；
- 固定 Host entrypoint 相对路径；
- Host protocol `verisilo-camoufox-host/v1`；
- entrypoint SHA-256；
- package tree manifest 的相对路径和 SHA-256；
- engine/version/platform/channel、capability 声明和现有 CMS signer policy。

CMS canonical payload 必须覆盖新增绑定。loader 必须逐层拒绝 traversal、未知/重复字段、symlink/reparse point、缺失/多余/变更的 tree member，以及 schema/engine/entrypoint 组合错误。

schema v2 的 Controlled Chromium 行为保持兼容；schema v2 Camoufox package 必须以明确的 unsupported entrypoint/schema 错误拒绝。M3-0 只使用 fake/test package，不加入发布 signer pin，不绕过 production verifier，也不提交真实浏览器 archive、Python runtime 或私钥。

### 6. 诚实映射运行证据

固定启动顺序：

```text
reverify signed package contract
→ spawn exact Host child with typed argv
→ hello（校验 protocol/host version/platform/asset/tree binding）
→ launch（artifactId/profileId/raw SHA）
→ 校验 response binding 与 state=running
→ RuntimeManager 进入 Running
```

Host 当前返回 `verified: false` / `observed-on-this-host`。因此：

- 可以记录 exact child、package verification、Artifact raw SHA 和 Host launch response；
- `launchedAdapter` 可以是 Camoufox；
- `verifiedAdapter` 必须保持空；
- Host launch evidence 最多映射为 `applied` / `observed`，不能映射为 `verified`；
- `runtimeReceipts` 不得标记为 generic phase receipt 已验证；
- TLS ClientHello、QUIC、Canvas 稳定身份、managed fonts、跨主机复现保持 `unavailable` 或未验证；
- `site_fallback` 在 Host v1 没有真实实现，Camoufox fallback rules 必须为空并显式 unavailable；
- M3-0 不接代理，Camoufox 只允许 Direct network policy；required proxy 在 spawn 前失败，`launch_network` 保持 unavailable。

若现有 capability model 无法表达 `observed`，优先把能力保守留在 `applied` 并把来源写入 bounded evidence；不要为了本任务扩大整个公共 capability enum。任何公共 contract version bump 都必须有兼容测试和明确迁移，不能顺手发生。

### 7. 生命周期必须使用 Host 所有权

- Desktop 持有 Host stdin/stdout 与 exact child handle；不得通过 PID 枚举或名称匹配关闭进程。
- watchdog/status 通过有界 `status` 请求更新状态；错误 ID、超时、malformed response、Host EOF/exit 都 fail closed，不能回退 stock。
- 对活动 Camoufox Silo，现有 `stop_silo` 可以发送 `close`，等待 `state=exited`、`processTreeExit.exited=true`，再发送 `shutdown` 并等待 exact Host child；Stock Chrome/Edge 的“用户自行关闭窗口”语义不变。
- `quarantined`、`profile_quarantined` 或未确认全树退出必须进入 `VerificationFailed` / `RecoveryRequired`，保留边界信息，不能标记 `Stopped` 或释放为可安全重用。
- Host 崩溃、桌面关闭和 stdin EOF 的清理语义必须由 fake-host 反例覆盖；M2-W 已验证的 Job Object/锁逻辑不得弱化。

## 允许修改

- `packages/contracts/src/engine.ts` 及其测试、由仓库脚本生成的对应 dist；
- `apps/desktop/src-tauri/src/engine.rs`、`launcher.rs`、`domain.rs`、`lib.rs` 及定向测试；
- `apps/desktop/src-tauri/resources/engine-package*.json` 与 source verifier；
- 新的测试专用 fake Host / package fixture；
- `apps/camoufox-host/host_v1.py` 的最小、向后兼容协议字段或测试修正，仅在 Desktop 无法从现有 response 建立必需绑定时；
- 与真实实现一致的 `docs/engine-adapters.md`、本任务卡和状态页。

TypeScript UI 只允许为既有模型编译所需的中性适配；不得新增界面、入口、营销文案或让用户选择尚不可运行的 Camoufox package。

## 禁止修改或提前实现

- Artifact/Policy/Projection/ObservedWebsiteDigest schema 与 accepted fixture/evidence manifest；
- M2-W 的 Job Object、文件锁、reparse point 和 tree-verification 保证；
- Managed Identity 生成 UI、安装器、自动下载、更新服务、发布签名基础设施；
- 代理注入、网络地区联动、site fallback、扩展生态、TLS/QUIC；
- Controlled Chromium、WSL、Hyper-V、VMware、Remote；
- 真实 Camoufox/Python archive、私钥、Token、seed 或代理秘密；
- 把 `verified: false` 改成 `true`，或把 fake-host 测试写成真实浏览器证据。

## 验收矩阵

### Contract / package

1. Camoufox Artifact binding 严格拒绝空 ID、错误 schema、bool/非字符串 SHA、非 64-hex、未知字段和缺失字段。
2. package 新版本严格拒绝错误 entrypoint/protocol、tree manifest digest、路径穿越、link/reparse、缺失/多余/篡改 member、重复/未知字段。
3. schema v2 Controlled Chromium 继续通过；schema v2 Camoufox 明确拒绝。
4. 生产 verifier 无 signer pin 时仍 fail closed；测试不得新增隐藏 bypass。

### Fake Host integration

5. 正常 `hello → launch → status → close → shutdown` 完成，request ID、Artifact ID/raw SHA、profile ID 和 Host child 全程一致。
6. `launch` 成功后 Runtime 为 Running，但 `verifiedAdapter` 为空，capability 不出现虚假的 `verified`。
7. Artifact/SHA/profile response mismatch、错误 Host protocol/version/asset binding 在 Running 前拒绝。
8. duplicate key、未知字段、无效 UTF-8、超长 frame、错序 response、重复/错误 ID、EOF、early exit 和 timeout 均有界失败。
9. `profile_in_use`、`profile_quarantined`、`quarantined`、`processTreeExit.exited=false` 映射为不能接管/不能安全停止。
10. 正常 close 只有在 Host 确认全树退出后才释放桌面 runtime/profile ownership；失败路径不误杀无关进程。
11. Direct Camoufox 可形成 plan；required proxy、非空 fallback rules 在 Host spawn 前拒绝。
12. argv/log/protocol snapshot 扫描不含 Vault seed、Artifact seed、token value 或代理秘密。

### 回归

13. Stock Chrome/Edge launch/stop 语义不变。
14. Controlled Chromium native bootstrap、ACK、ordered receipt、site fallback 和 package schema v2 测试继续通过。
15. Host v1 既有 standalone 单元/集成测试不因 M3-0 回退；无需重跑真实浏览器长测试。

## 必跑验证

至少执行并报告：

```bash
pnpm check
pnpm test
pnpm build
pnpm engine:verify
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

如当前 Linux 无法完成 Windows-only verifier 路径，必须由既有 GitHub Windows CI 覆盖并给出 check URL；不得用 Linux/WSL 冒充原生 Windows M3-WI。

## 停止条件

出现任一情况即停止并退回主脑，不得自行扩大：

- 必须改变 Artifact v3 或 M2-W accepted evidence 才能集成；
- 无法在不弱化 Controlled Chromium v1 或 production verifier 的前提下加入 Host transport；
- 需要真实 signer、发布 runtime、浏览器 archive、UI/安装器或代理才能让 contract tests 成立；
- 发现 generic receipt 与 Host evidence 无法诚实映射，且需要新的跨产品 evidence schema 决策；
- 需要删除、重写或重生成 M0–M2-W accepted 历史。

## 交付与结果包

执行 Agent 最终必须返回：

1. 起始 commit、平台、分支与结束 commit；
2. 逐项 1–15 验收结果；
3. transport、Artifact binding、package v3、生命周期和 evidence 映射的关键实现说明；
4. 修改文件清单和明确的禁止范围 diff 结论；
5. 实际测试命令、计数、GitHub checks 与失败后修复记录；
6. fake Host 证据位置，以及 malformed/timeout/quarantine/secret-scan 反例；
7. 工作树、残留进程、外部副作用与是否 push/开 PR；
8. 未验证边界和 M3-WI 所需真实 Windows 任务输入；
9. 执行结论只能是 `passed`、`failed` 或 `blocked`，不得自行宣布 M3 或产品 Gate 通过。

主脑收到结果后按[成本受控审阅协议](agent-operating-model.md)作 Gate 判断，不会默认重做整个执行任务。
