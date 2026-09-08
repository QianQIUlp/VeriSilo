# Managed Artifact P1 接手文档

更新：2026-09-08。状态：**BLOCKED / 未完成产品验收**。用户要求停止继续验收，先交接给其他 agent。

## 工作区与授权边界

- 主检出：`C:/Users/qiu/src/VeriSilo`
- 本任务工作区：`C:/Users/qiu/src/VeriSilo/.verisilo-worktrees/host-managed-artifact-p1-1ba11b`
- 分支：`agent/host/managed-artifact-p1-1ba11b`；lane：`host`
- 开始时及交接时 baseline：`4b3163de471b064a266fac31e79b949b10edd36d`，来自 `refs/heads/baseline/dev`。
- 继续本任务应直接进入上述 worktree，保留已有修复和证据，不重新 start 相同任务。
- 用户授权：精确定位 Managed create → ArtifactIntegrityError，最小安全修复，真实隔离 Vault 验收，提交；不 push、不 advance baseline、不开始下一轮 QA、不构建 installer/RC。
- 不跳过完整性检查、不放宽 schema/hash、不 fake Artifact/Host、不改 FP contract、不重建架构、不重新解决 package discovery。
- `AGENTS.md`、`docs/agent-task-routing.md`、`docs/development-worktrees.md` 已读取。Ponytail full 生效。

## 已定位的精确根因与修复

`apps/camoufox-host/provision_artifact.py::apply_identity_overrides` 把生成配置的 screen width/height、availWidth/availHeight 改成 preset 尺寸，却保留 BrowserForge 原先生成的非零 availTop/availLeft。

实际失败样本：

| 字段/约束 | 期望 | 修复前实际 |
| --- | --- | --- |
| screen.height | 800 | 800 |
| screen.availHeight | 800 | 800 |
| screen.availTop | 全屏可用区域原点为 0 | 4 |
| availTop + availHeight <= height | <= 800 | 804 > 800 |

首次不一致发生于 **complete_resolved_config 完成后，apply_identity_overrides 重写尺寸时**。前面的 complete_resolved_config 已按生成时的尺寸夹取 offset；之后改尺寸使该约束失效。

随后 `validate_artifact_strict` 拒绝并抛出：

`ArtifactIntegrityError: strict validation failed: screen avail geometry inconsistent`

此时尚未发布 JSON/sidecar，更未返回给 Rust 或写入 Vault。故本样本不是 raw SHA、canonical digest、Artifact ID、schema 或 Formal-v3 binding 的不匹配。

两行修复：在设置 availWidth/availHeight 后同时设 `screen.availLeft = 0`、`screen.availTop = 0`。两个现有 caller（初始 provisioning 和 v6 rebind 后的 overrides）一起覆盖。没有修改 verifier、格式、hash 算法、ID 算法、签名 pin 或 FP contract；fail-closed 保持。

改动文件：

- `apps/camoufox-host/provision_artifact.py`：两行修复。
- `apps/camoufox-host/test_package_contract.py`：已有 dependency-free 检查中加入非零 top/left + 全屏尺寸的几何回归断言。
- 本接手文档。

历史证据：`git show c21db17 -- apps/camoufox-host/provision_artifact.py` 显示后置尺寸 override 在该 baseline WIP checkpoint 中加入。之前生成链只对原生成尺寸夹取 offset。当前 canonical CLI 真正通过 discovery 后执行 fresh provisioning，随机生成的非零 offset 才暴露此回归。FP 的固定 Artifact / 旧生成路径通过不能覆盖这个后置 override。不要宣称所有旧 Gate 都已再次验证。

## 实际复现与局部验证

Python runtime（只作为已安装依赖读取，未改其代码/依赖）：

`C:/Users/qiu/src/VeriSilo/apps/camoufox-host/.venv/Scripts/python.exe`

旧的 development package（原有资源，接手时不要覆盖/删除）：

`C:/Users/qiu/src/VeriSilo/.verisilo-worktrees/core-managed-cli-development-d1ebfa/apps/desktop/src-tauri/target/verisilo-managed-browser-resources/engine-package`

- 当前源码未修复时，调用真实 provision_artifact，preset balanced-zh-cn、window [1280,800]、direct，使用测试 seed `[5]*32` 的一次实际样本记录到 availTop=4，并打印上述确切错误。
- 旧打包 Host 通过真实 length-prefixed provisioning 协议也失败：`artifacts/p1-packaged-failing-seed/response.txt`、`state/host-stderr.log`。输入 seed `[5]*32`、preset balanced-zh-cn、window [1280,800]、followNetwork=false。exit 1，响应包含上述具体错误。
- 不能把 seed 当成跨进程必然复现同一 BrowserForge 样本的保证：另外一次新进程的同 seed 观察生成了不同 screen。回归测试直接固定非零 offset，稳定覆盖已证实 owning seam。
- 修复后真实源码 provisioning 成功，并以同一 artifact root 重复调用确认已存 Artifact 完全相同；`verify_artifact_raw`、`verify_browser_binding` 通过。
- 实际成功 Artifact 在 `artifacts/p1-source-fixed/identity/`，结果在 `artifacts/p1-source-fixed/result.json`：
  - ID：`identity-c1ba5bbd1f443edbd41ee4fa`
  - schema：`verisilo-camoufox-resolved-identity/v5`
  - raw SHA：`20e8498291d7eaa2ddc0e49000d622b022cd362125abab40d81fc0100b7f7eb7`
  - configured digest：`sha256:211389bfa41cce6483b1cb96228ebdc79a0d4224af6b8792d36680dfb685d801`
- 改坏 geometry 的对象仍被 strict verifier 拒绝；在真实 Artifact 原始文件末尾追加空格，保留旧 expected SHA/sidecar，也被 raw SHA verifier 拒绝。损坏样本：`artifacts/p1-source-fixed/damaged.json`。
- 上述是实际 Host/Artifact 局部证据，**不能代替 Managed CLI create/Vault persistence/start 验收**。

## Tests / build / 检查状态

已通过：

- `node C:/Users/qiu/src/VeriSilo/scripts/agent-task.mjs verify`：host lane 的 package contract + page command tests 均通过。
- `cargo build --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --bin verisilo --bin verisilo-cli`：本 worktree 独立 debug 构建成功，日志 `artifacts/p1-desktop-build.log`。
- 新 Host PyInstaller 6.22.2 构建成功；新 package 内容和 Formal-v3 binding 的 `recheck_formal_package` 通过，见下节。
- 两个产品改动的 `git diff --check`、final diff inspection、host scope 与主检出 contamination check 通过。增加本文后会再做文档 diff/scope 检查。

额外执行 `test_identity_artifact.py`：57/59，通过/失败完整输出在 `artifacts/p1-identity-tests.log`。

未解决的两项：

1. `test_fp1_fake_stage_timeout_crosses_protocol_and_fail_closed_cleanup`：空 AssertionError，未进一步定位，也未重跑旧 FP Gate。
2. `test_self_built_launch_rechecks_the_pinned_tree_before_profile_use`：test 期待 `verify_tree_contents=True`，当前 implementation 调用 `False`。

对应 test / owning runtime 文件本任务均未改，不能声称此套件全绿，也未通过改测试或改安全边界消除失败。尚未在未修改 baseline 上另跑这两项，不应把“未修改”写成“已独立证明 baseline 同样失败”。

## 新 development package 与签名阻塞

新包：

`apps/desktop/src-tauri/target/verisilo-managed-browser-resources/engine-package`

- 仅换入本任务源码构建的 Host；Formal-v3 browser、supervisor、probe 和 asset/tree bindings来自上述现有 development package。
- local 构建脚本：`artifacts/p1-build-package.py`，日志：`artifacts/p1-package-build.log`，PyInstaller 输出：`artifacts/p1-pyinstaller/`。
- 初次尝试正式 builder 因所需原始 runtime-asset 输入文件本机没有而停止，未生成包。随后使用已有 package 的冻结 binding，重建 Host/package tree，并用 `recheck_formal_package` 检查最终新包。
- 现有旧 package 多出 `browser/version.json`，其余 package-tree 成员无缺失/修改；browser_tree.py 明确把 version.json 列为 runtime extra。新包复制时不带这个 generated file，没有改 verifier 或旧包。
- 注意 Host cache 使用 junction，运行时 seed_camoufox_cache 可能在 browser root 生成 version.json；不要把检查/清理扩大到其他工作树，也不要误删原有证据。未记录旧包 version.json 的调用前状态，不能断言其创建者。
- **交接检查时新包仍未签名**：keyId 全零。不能通过 production adapter，这是尚未完成 Managed 产品验收的前置阻塞。
- production pin：`57f3b44cf572571e8b133c6b605b061e0d1c4d9dd75a490b14f658c292bebd93`。
- PFX：`C:/Users/qiu/.verisilo-signing/engine-package-rc1.pfx` 存在；CurrentUser/My 枚举为空；Process/User/Machine 的 `VERISILO_CAMOUFOX_PFX_PASSWORD` 均未提供。没有索取/读取/输出口令，没有改 pin、没有新建签名身份。

已经给用户本机终端命令。用户尚未回复“已签名”。**不要用聊天工具索取口令**；只让用户在自己的 PowerShell 终端执行：

```powershell
& 'C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\host-managed-artifact-p1-1ba11b\artifacts\p1-sign-package.ps1'
```

该 wrapper 调用仓库已有 sign-camoufox-host-manifest.ps1，以 Read-Host -AsSecureString 本机隐藏输入口令。不要在 agent 后台执行交互提示并期待用户能输入。签完只需用户回复“已签名”，然后检查实际 manifest/signature，不根据回复推定成功。

`artifacts/` 是本机忽略目录，不随提交复制；在当前机器沿用工作区即可复用。不要删除这些待验收产物。

## 当前真实 QA Vault / 进程现场

- Vault：`host-managed-artifact-p1-1ba11b`，本任务从空 Vault 初始化。
- 数据根：`C:/Users/qiu/AppData/Local/io.verisilo.app/vaults/host-managed-artifact-p1-1ba11b`
- CLI：本 worktree `apps/desktop/src-tauri/target/debug/verisilo-cli.exe`
- desktop：同目录 `verisilo.exe`；后台模式已运行，交接时 PID 8888（使用前重新确认，PID 不是永久身份）。
- 这是独立 debug/Tauri backend 的真实 CLI/background 产品路径，没有 installer、没有 mock Host。
- QA-only 可丢弃测试口令见 `artifacts/p1-standard-cli.py` 的 `password`，不是用户口令；只通过 CLI stdin 提交，没有打印。不要将其用于其他 Vault。
- Standard A 已真实 create/start/readback 成功：
  - ID `9ebfa89a-5ad8-4c44-a9a2-aa3b02117d20`
  - Edge `152.0.4191.66`，adapter stock-edge，Network Direct，activation running。
  - 独立 profile 位于本 Vault silo/browser-data。
  - 交接时仍活动。根 Edge PID 18540，父 PID 8888，命令行绑定上述 silo profile；使用前重新确认。
- `artifacts/p1-standard-cli.json` 保存已执行命令和原始输出。
- 其最后一步 `stop Standard A` 返回明确错误：本机 stock browser 需关闭其窗口，产品不会终止无关 browser processes。脚本在此断言停止，**后续 service restart/unlock/readback 未执行**。
- 保留 Standard A 活动，是为了下一步在其 running 时验证 Managed create，不能先关闭 Standard 来掩盖 create 触发条件。
- 最初 Python subprocess capture_output 遇到 background 子进程管道未关闭而挂住；Vault 实际初始化成功。该任务自建 Python driver 已按精确进程停掉，改为 stdout/stderr 文件后正常执行。不要再次完整重跑 standard driver（会重复 create）；复用其 call helper 或直接 CLI。
- 当前没有未完成的代码构建或验证脚本需要继续等待。保留的产品进程是上述隔离 desktop/Standard。

## Managed create 与 single-active 的结论

静态调用链已检查：

`CLI/local_api → application/identity.rs::create_managed_silo_with → provision_managed_artifact → production adapter → Host provision_artifact → Vault::create_managed_silo`

create 两端都没有禁止其他 Silo running 的条件；创建应当允许。临时 provisioning root 用新 UUID，成功/失败会清理；失败时没有部分 Managed record。

Managed **start** 的 single-active 错误为 `managed_another_silo_running`（LauncherError::AnotherSiloRunning 的映射）；这不能当作 create 的合法完整性错误。静态结论待下一步活动 Standard 下真实 Managed create 证明。

## 接手后最短剩余路径

1. 进入本任务 worktree，读本文/当前 diff/任务 metadata；保留两行修复。
2. 如用户尚未本机签名，只提供上面的终端命令，绝不要求在聊天输入口令。签名后确认现有 pin、签名与最终 tree/Host bytes；不修改 production trust。
3. 保留/确认 Standard A running，在当前全新隔离 QA Vault 执行：

   ```powershell
   & './apps/desktop/src-tauri/target/debug/verisilo-cli.exe' --vault host-managed-artifact-p1-1ba11b create --name 'Managed B' --network direct --preset balanced-zh-cn --json
   ```

   若 Vault auto-lock，先用 stdin unlock；不要把口令放命令参数或输出。
4. 验证 create 成功、无 ArtifactIntegrityError、记录持久化、adapter Camoufox、Direct、Artifact ID/raw SHA/schema 自洽、Formal-v3 engine binding 正确。再以不同名称执行相同 preset/network，验证新生成路径不再失败。
5. 可在 Standard 仍 running 时尝试 Managed start，确认 single-active 正确错误。然后只关闭本任务 Standard 的窗口（按确切 profile/PID 识别，不终止无关 Edge），等待产品 reconcile，再做 Managed start → stop → reopen smoke。
6. service stop → restart（CLI 会自动启动 sibling desktop）→ unlock → readback。确认持久化 bindings 与首次一致；最小 reopen 后清理本任务运行进程。
7. 若出现新错误，沿直接因果 seam 定位；不要把两行修复成功等同于整个产品目标已完成。新 schema/共享契约/跨层修改需按 lane 路由处理，不硬塞 host scope。
8. 已验证真实损坏 Artifact 被 verifier 拒绝，可复用该证据；仅输入/代码变化才重测。不要机械重跑 FP1–FP4。
9. 根据新变化执行最低充分验证、verify/check/scope/contamination/final diff，补齐最终报告并提交；不 push，不 advance baseline，不开始下一轮 QA。

最终只能在完整验收达成时报告 `MANAGED_ARTIFACT_P1_FIXED`；当前应报告 `BLOCKED`，不能声称 Managed CLI create/start/stop/restart 已通过。
