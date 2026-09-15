# Commercial User Journey QA — post-rc3 bounded pre-RC product QA

- 状态：**已停止（确认产品 blocker）**
- 日期：2026-09-15
- QA branch：`agent/qa/commercial-user-journey-4b3cef`
- Canonical baseline：`cefe9dd062363f8f1682bb8c5fb868525634e18f`（`origin/baseline/dev`，开工时精确相等）
- RC4 source binding：同 baseline `cefe9dd`（RC4 candidate 已按用户显式授权构建；本轮不重建）
- QA 隔离：正式 `--vault` 隔离机制；未触碰用户真实 Vault
  - worktree：`.verisilo-worktrees/qa-commercial-user-journey-4b3cef`
  - 隔离 Vault：`qa-commercial-user-journe-4b3cef`
  - dev port：15716
- 测试层级：真实 Tauri dev（Mode B）+ 真实系统 Chrome + repository session-site fixture（port 4173）

## RC4 candidate build（用户显式授权的打包构建）

- ReleaseVersion：`v0.1.0-rc4`
- 构建源：主检出（clean，`cefe9dd`，与 `origin/baseline/dev` 精确相等）
- Engine package 输入：`artifacts/build/managed-browser/rc4-engine-package-input`
  （schema v3，`engineVersion 152.0.4-beta.28`，`engineRevision verisilo-camoufox-152.0.4-beta.28-r1-formal-v3`，
  memberCount 1388，`packageTreeSha256 062c1b58…`，`browserTreeSha256 d77002d0…`，CMS detached 签名 verified，
  signer pin `57f3b44c…` 与 Desktop 内嵌 pin 一致）
- 构建验证：`Managed-browser release verification passed for 1404 files`；provenance 1402 payload files verified；SBOM/licenses/SHA256SUMS 已生成
- Installer：`VeriSilo-Managed-Browser-v0.1.0-rc4-x64-setup.exe`；SHA-256 `1f3155090d1280637b5f310657dbdd08a384636e72530d13e7028e3822f26b16`
- outer Authenticode：unsigned（已知 backlog，非本轮 blocker）

### 构建期间观察（非产品 blocker）

`artifacts/build/managed-browser/rc3-engine-package` 副本曾被一次运行写入 3 个 runtime state 杂散文件（共 66 bytes），因此 `--check` 失败；本轮未使用该副本，也未修改它。RC4 使用干净的 `rc4-engine-package-input`，构建验证已通过。

## Evidence carry-forward（不重复 QA）

- `CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`（`docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md`）——Managed 成功 runtime、`fontMode=managed`、host font masking、cold restart 与 clean teardown
- FP1–FP4 / Surface Truth Matrix / Transport coherence —— fingerprint core
- `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`（Build 26200，0xc06d007e）——已知宿主环境问题，不重新诊断
- Local Report JSON/HTML 落盘 —— 既有 QA 证据

## Journey 结果

| Journey | 结果 | 备注 |
| --- | --- | --- |
| A Vault 日常使用 | PASS | 初始化、错误密码拒绝、正确解锁、Lock/Unlock、重启后再次解锁；A1–A5 截图已保存 |
| B Standard Silo 隔离与持久化 | PARTIAL | `standard-a`/`standard-b` 由 Desktop UI 创建；真实 Chrome 启动、独立 `user-data-dir`、重启路径和跨 Silo profile 路径已核对。cookie/localStorage marker 未核实：Chrome 控制扩展未安装，且 stock Chrome 不接受产品 `page` 命令；未使用 shell/Playwright 替代 |
| C Silo lifecycle | PARTIAL | Standard start/running/close/restart/close 与 Desktop 收起时 active browser 已执行；因同一 Chrome connector 限制，浏览器“用户手动关闭”未直接观察，使用精确 QA Chrome 进程收尾；未发现 QA 孤儿进程 |
| D Silo CRUD / Navigation | PASS_WITH_LIMITATION | 创建、选择、切换、Edit/rename、archive 已由 UI 完成；归档空间的 UI 永久删除两次返回通用错误，恢复后以产品 CLI 删除 disposable Silo 完成清理；记录为 backlog |
| E Managed Silo 入口 | BLOCKER | UI 创建成功且真实 `camoufox` Silo/profile/Artifact 落盘，但实际 Artifact 的 `policy.fontMode` 为 `inherit`，违反当前 production preset 的 `managed` 契约；一次性启动失败 UX 已如实恢复为“浏览器没有打开成功”/未运行，属于已知 native host 环境限制 |
| F Create New Identity | NOT_RUN_AFTER_BLOCKER | 按任务书的 blocker 停止规则，未继续创建第二个 Managed Silo |
| G Network policy UX | CARRIED_FORWARD_FROM_FP3 | QA worktree 无安全 deterministic proxy fixture；未搭建新代理设施 |
| H Report / Evidence UX | NOT_RUN_AFTER_BLOCKER | 既有 Local Report JSON/HTML 落盘证据继续 carry-forward；未为本轮失败 Managed runtime 构造新 report |
| UI/UX 最低商业可用 | PARTIAL_BEFORE_BLOCKER | Tauri primary UI、创建/切换/解锁/错误提示、Escape 关闭输入建议已观察；阻断后未继续做完整 1280×800 与 Tab/Enter 覆盖 |
| 进程清理 | PASS | QA dev/fixture 已停止；QA Vault 精确目录已删除；默认 Vault 仍存在；用户 Chrome 未触碰 |

## Blocker — exact reproduction and direct evidence

1. 在真实 Tauri dev + QA Vault 中，通过 Desktop UI：创建 Silo → Managed / 托管身份 → 默认 production 表单（Direct、中文简体 / `balanced-zh-cn`）→ 创建 `managed-qa-production`。
2. CLI 只读核对列出新的 `camoufox` Silo：`siloId=d96c1538-d430-49c6-9420-8492608d3825`，profile directory 为该 QA Vault 下独立的 `silos/<siloId>/browser-data`。
3. 一次性 Managed 启动后，真实 materialized Artifact 位于该 Silo 的 `identity/identity-5b28e289ee4d3a825bc46eee.json`；安全字段核对结果：schema `verisilo-camoufox-resolved-identity/v5`、Artifact 存在、`policy.fontMode=inherit`。未把 seed、credential、cookie 或原始敏感字段写入 evidence。
4. 当前源码 `apps/camoufox-host/provision_artifact.py:99-104` 对 `balanced-zh-cn` 明确声明 `fontMode=managed`；实际 Tauri dev 使用的 host SHA-256 为 `d670364a46a9a6e5fc8f4f636b13e08cfb100be80eee7b3c4be399a9ee254e5d`，与 RC4 `rc4-engine-package-input` 的 host 相同。

用户影响：Managed production Silo 的实际 Artifact 没有交付声明的 managed font policy，因此不能诚实宣称 Managed 入口满足当前生产契约或接受 RC4。

Owning layer：Managed Camoufox host/package Artifact provisioning seam（不是 Vault、Standard profile 或本 QA UI 操作）。QA 未修改产品源码。

## Known native-host failure UX

本轮唯一一次 Managed launch 已执行。host child/supervisor/browser 进程最终退出，UI 在 Desktop 重开后显示“浏览器没有打开成功，请再试一次。”，Silo 回到“未运行”，CLI diagnose 显示 Vault unlocked、`active=false`、`identityLocked=true`，没有 QA Managed runtime 孤儿。该现象与已接受的 Build 26200 / 0xc06d007e native-host 环境证据一致，不作为本次产品 blocker；Managed success path 使用 Sandbox carry-forward。

## Non-blocking backlog（最多 5 项）

1. 归档空间 UI 的永久删除动作返回通用错误；本次用恢复后产品 CLI 完成 disposable Silo 删除，需后续 owning task 处理 UI/archived delete seam。
2. Chrome 控制扩展未安装，导致 Standard cookie/localStorage marker persistence/isolation 无法由允许的 browser connector 直接核实。
3. Build 26200 native Camoufox launch environment failure（0xc06d007e）；沿用已有 Sandbox success evidence，不重复诊断。
4. Installer outer Authenticode 未签名。
5. strict standard-user lifecycle 尚未证明。

## Final verdict

`COMMERCIAL_USER_JOURNEY_PRODUCT_BLOCKER_CONFIRMED`
