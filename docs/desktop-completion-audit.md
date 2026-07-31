# VeriSilo 桌面端完成度滚动审计

> **历史快照提示（2026-07-30）**：本文主体是 2026-07-28 的审计快照，其中对 V0.7–V0.9“无实现/只有路线”的若干表述已经落后于最新 `main`。当前可达代码、自动化边界和真实 Windows 待验收项以 [`acceptance/windows-desktop-integration-matrix.md`](acceptance/windows-desktop-integration-matrix.md) 为准。本文保留用于追踪原始缺口，不应单独用作当前完成度结论。

> 审计日期：2026-07-28
>
> 审计对象：`codex/desktop-complete-v0-9` 在功能基线 `9abe37ec273d0f123329face439e551e7ca8defa` 上的仓库证据
> 性质：持续更新的实现审计，不是最终发布审计

## 判定规则

本报告只使用以下五种状态：

| 状态                   | 含义                                                                                                                                       |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| **已实现且自动验证**   | 当前仓库有实现，并且存在直接覆盖该断言的自动测试或构建检查；窄范围测试不能证明更宽的运行时断言。                                           |
| **已实现仅实验室验证** | 实现已存在，但目前只有受控手工环境证据；最终审计必须附环境、步骤、日志和产物哈希。                                                         |
| **部分实现**           | 有接口、UI、局部实现、测试夹具或路线文档，但缺少需求的一部分或缺少足够强的验证。                                                           |
| **外部条件阻塞**       | 仓库内工作已具备验收条件，剩余证明依赖真实 Windows、浏览器、虚拟化、合法镜像、远端节点、正式 ID 或证书。该状态不能用来掩盖尚未编写的代码。 |
| **明确不支持**         | 产品边界有意拒绝此能力，或当前层明确不能提供；UI 和文档必须说清替代层或原因。                                                              |

“有按钮”“有类型”“能编译”“有 mock”“写入路线图”都不单独构成已实现。每个网络能力还必须区分 `configured`、`applied` 和 `verified`。

## Git 与成果保全

| 对象           | 已核对锚点                                                           | 结论与保全要求                                                             |
| -------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 当前实施分支   | `codex/desktop-complete-v0-9`，实施前 HEAD `9abe37e`                 | 分支从网络身份成果继续，实施过程中不得重写该基线。                         |
| `origin/main`  | `5910cd96d64aae816dc680e9c0d456fa36feb138`                           | 是 PR #2 合并后的主线基准；最终审计需记录其后合并变化。                    |
| 网络身份分支   | 本地及 `origin/codex/network-identity-providers` 均指向 `9abe37e`    | V0.1–V0.6 网络基线可由 Git 对象恢复。                                      |
| PR #3          | [QianQIUlp/VeriSilo#3](https://github.com/QianQIUlp/VeriSilo/pull/3) | PR 的在线状态属于可变外部证据；发布前必须重新查询，不能仅凭本地 ref 推断。 |
| 原始 WIP stash | `stash@{0}`，对象 `6cf8e50e6b80f3249878026b86661a683802e6b8`         | 不得 drop；在最终审计确认所有独有改动已吸收或明确废弃前保留。              |
| stash 归档分支 | `codex/archive-network-isolation-wip` → `6cf8e50`                    | 已把 WIP 固定为普通分支引用，避免只依赖 stash reflog。                     |

复核命令：

```bash
git rev-parse HEAD
git rev-parse origin/main
git rev-parse origin/codex/network-identity-providers
git show-ref --verify refs/heads/codex/archive-network-isolation-wip
git stash list
git diff --stat 5910cd9..9abe37e
```

残余风险：这些命令只能证明本地 Git 对象和 ref；PR 审核状态、远端分支保护和远端备份仍需通过 GitHub 重新验证。

## 当前结论摘要

- 当前产品已具备独立 Profile 启动、加密 Vault、完整 Silo 元数据/归档/删除生命周期、固定代理/外部 Mihomo、Companion→Native Host→加密 Vault 网络证据历史，以及分离的 unsigned candidate 和证书 secrets gated signed Windows workflow 定义；后者尚未用真实证书或 installer 实跑，它仍不是 V0.9 完整桌面平台。
- 桌面端现可在用户明确选择一个 Silo、勾选确认并点击后，把该 Silo 的非秘密元数据、启动状态和 Vault 网络证据导出为本地脱敏 JSON/HTML Blob；真实 Windows 的“另存为”对话框行为尚未验证。
- 自动证据现覆盖模型校验、Vault 改口令/备份/恢复与 schema 迁移、Silo CRUD/受管目录边界、代理中继、Mihomo Controller、Native Host/证据 inbox、前端命令契约、SBOM/发布策略生成器和扩展构建审计。
- 当前仍没有真实 Windows 浏览器 E2E、EngineAdapter、受控浏览器制品、EnvironmentBackend、远程 Agent、真实 NSIS 安装升级卸载结果或 Authenticode 签名产物。
- `apps/desktop/src/capabilities.ts` 和 `docs/environment-roadmap.md` 中的 V0.7–V0.9 内容是产品承诺和 UI 路线，不能作为实现证据。

## 2026-07-28：核心闭环第一批复核

| 能力                                                                      | 当前五态             | 直接证据                                                                                                                                                                                                                                               | 尚未达到                                                                                                                            |
| ------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| Vault 改口令、加密 envelope 备份/恢复、schema 1–6 → 7 迁移                | **已实现且自动验证** | [`vault.rs`](../apps/desktop/src-tauri/src/vault.rs)；Rust 1.88 非 Tauri core harness 的 Vault 定向测试 22/22                                                                                                                                          | Profile 原始目录不在备份或 Vault 加密范围；真实 Windows 磁盘故障、权限、跨卷与升级恢复仍需 E2E。                                    |
| Silo 编辑、重命名、归档/恢复、明确永久删除、磁盘占用                      | **已实现且自动验证** | [`vault.rs`](../apps/desktop/src-tauri/src/vault.rs)、[`lib.rs`](../apps/desktop/src-tauri/src/lib.rs)、[`App.tsx`](../apps/desktop/src/App.tsx)；路径/符号链接/锁/运行中拒绝测试                                                                      | Windows junction 分支、真实 Chrome 锁和删除失败后的 `.deleting-*` 清理需 Windows 验证；资料与网络更新仍是两个命令。                 |
| Companion 主动出口观察交给桌面                                            | **已实现且自动验证** | [`protocol.ts`](../packages/contracts/src/protocol.ts)、[`native_host.rs`](../apps/desktop/src-tauri/src/native_host.rs)、[`background.ts`](../apps/extension/src/background.ts)；严格 16 KiB/TTL/Silo/Vault 鉴权测试                                  | 尚无真实 Chrome/Edge Native Messaging E2E；Host 不可用时结果只留扩展本地，这是预期降级。                                            |
| 加密网络证据历史与桌面人话展示                                            | **已实现且自动验证** | Vault schema v4 的导入、去重、100 条/Silo 与 1000 条总量、清除/随 Silo 删除、schema 4–6 迁移测试；旧无 runtime 绑定记录保持 unbound observation；桌面 TypeScript 构建                                                                                  | 真实浏览器回传尚未实验室验证；IP 只证明当次第三方 HTTPS 请求，公共 DoH 仍不证明实际 DNS 路径。                                      |
| 单 Silo 本地脱敏 JSON/HTML 证据报告                                       | **已实现且自动验证** | [`reports.ts`](../apps/desktop/src/reports.ts)、[`reports.test.ts`](../apps/desktop/src/reports.test.ts)、[`App.tsx`](../apps/desktop/src/App.tsx)：显式选择/勾选/点击后用本地 Blob 下载；测试覆盖 IPv4 /24、IPv6 /48、秘密字段排除、HTML 转义与确定性 | 在真实 Windows Tauri/WebView 上确认下载文件名、下载目录和系统“另存为”对话框语义；真实 Companion 回传仍需浏览器 E2E。                |
| Native Host 当前用户生产注册材料                                          | **部分实现**         | 生产 ID 编译 gate、Chrome/Edge 分 manifest、幂等安装/验证/卸载脚本、NSIS hooks 和 source verifier                                                                                                                                                      | 正式商店 ID、真实 HKCU/Chrome/Edge、安装升级卸载和 Authenticode 均未实跑。                                                          |
| SBOM、SHA-256/provenance、release policy 与 unsigned/signed NSIS workflow | **已实现且静态验证** | `pnpm release:self-test`；锁文件生成 663 个组件的 CycloneDX 1.6/SPDX 2.3；签名顺序、EXE/PS1 覆盖和完整 Action SHA pin policy                                                                                                                           | SPDX 许可证仍为 `NOASSERTION`，需法律/许可证复核；Windows workflow 尚未产生真实 artifact，hosted runner 与工具下载也并非 hermetic。 |

本批复核命令：

```bash
pnpm check
pnpm test
pnpm build
pnpm format:check
pnpm extension:verify
pnpm native-host:verify
pnpm release:self-test
node scripts/generate-sbom.mjs --out /tmp/verisilo-sbom
node scripts/generate-sbom.mjs --out /tmp/verisilo-sbom --check
docker run --rm -v "$PWD:/workspace:ro" -v /tmp/verisilo-corecheck:/check \
  -v /tmp/verisilo-cargo-registry:/usr/local/cargo/registry -w /check \
  rust:1.88-slim cargo test --offline --locked
```

最后一条命令直接引用当前 `domain.rs`、`launcher.rs`、`mihomo.rs`、`native_host.rs`、`proxy_relay.rs` 和 `vault.rs`，本轮结果为 **42 passed / 0 failed**；它仍不覆盖 Tauri 宏、Windows API、WebView、NSIS 或真实浏览器。

### Vault schema 兼容矩阵（无 Docker 复核）

当前写格式是 schema 7；schema 1–6 均是受支持的加密导入格式，不走“不明确失败”的隐式弃用路径：

| 源 schema | 迁移必须保留                                                          | 缺失新字段的安全默认                                                         |
| --------- | --------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| 1         | Silo 身份、网络配置、seed reference 与 32-byte seed                   | proxy/Mihomo 凭据、网络历史、remote state 为空。                             |
| 2         | schema 1 + proxy credential reference/value                           | Mihomo secret、网络历史、remote state 为空。                                 |
| 3         | schema 2 + Mihomo Controller secret reference/value                   | 网络历史、remote state 为空。                                                |
| 4         | schema 3 + 脱敏 network inbox observation                             | 缺 runtime 绑定的旧记录保持 nil/unbound observation，不迁成当前 `verified`。 |
| 5         | schema 4 + endpoint、pairing/replay ledger、binding、operation result | deletion proof 缺省 `None`，orphan receipts 为空。                           |
| 6         | schema 5 + authenticated deletion proof                               | orphan receipts 为空。                                                       |
| 7         | 当前全部字段，包括明确“不证明远端已删除”的 orphan receipt             | 当前格式缺必需字段属于损坏，不静默补齐。                                     |

本轮直接执行：

```bash
/tmp/rust-1.88.0/bin/rustfmt --edition 2021 --check \
  apps/desktop/src-tauri/src/vault.rs
PATH=/tmp/rust-1.88.0/bin:$PATH cargo metadata --locked --no-deps \
  --format-version 1 --manifest-path apps/desktop/src-tauri/Cargo.toml
PATH=/tmp/rust-1.88.0/bin:$PATH \
  CARGO_TARGET_DIR=/tmp/verisilo-vault-harness-target \
  cargo test --offline --manifest-path /tmp/verisilo-vault-harness/Cargo.toml \
  vault::tests:: -- --test-threads=1
```

临时 core harness 只用 `#[path]` 直接引用当前工作树的 `engine.rs`、`domain.rs`、`native_host.rs` 和 `vault.rs`，没有复制被测 Vault 实现，也没有修改生产依赖或 `Cargo.lock`；结果为 **22 passed / 0 failed**。正常 desktop `cargo test ... vault --lib` 在编译项目源码前被本机缺少 `glib-2.0` / `gobject-2.0` 的 `pkg-config` 元数据阻塞，因此不能把本轮结果写成 Tauri、真实 Windows 或完整 desktop build 通过。矩阵另有逐字段 property/table test，并拒绝未知字段、未知版本、跨 schema 降级形状、缺必需字段、envelope metadata/ciphertext 篡改和错误口令；错误文本检查不包含 fixture 口令或凭据。

## V0.1：隔离启动核心

| 要求                                                                    | 当前状态                       | 当前证据                                                                                                                                                                                                                                                                                                               | 完整验收                                                                                                                     | 残余风险                                                                                                                        |
| ----------------------------------------------------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Vault 初始化、解锁、错误口令拒绝、Argon2id KEK + 随机 DEK + AES-256-GCM | **已实现且自动验证**           | [`vault.rs`](../apps/desktop/src-tauri/src/vault.rs) 的 v2 envelope、legacy rewrap 和 round-trip 测试；[`Cargo.toml`](../apps/desktop/src-tauri/Cargo.toml) 固定密码学依赖                                                                                                                                             | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml vault`；再审查明文 envelope 不含 Silo、种子或凭据              | 尚无独立密码学审计；Profile 原始目录不受 Vault 加密。                                                                           |
| 15 分钟自动锁与敏感操作拒绝                                             | **部分实现**                   | [`vault.rs`](../apps/desktop/src-tauri/src/vault.rs) 的 `auto_lock_at`、`expire_if_needed`、`unlocked_mut`；UI 有立即锁定                                                                                                                                                                                              | 使用可控时钟测试到期、活动续期、后台静置、系统睡眠恢复和到期后的全部敏感命令                                                 | 当前到期在下一次状态/业务调用时惰性执行，没有定时事件测试。                                                                     |
| 改口令、加密备份/恢复、版本迁移                                         | **已实现且自动验证**           | 新 Argon2id salt/KEK 并轮换随机 DEK；schema 1–6 真实加密 fixture 逐版解锁/恢复迁移到 7；原子重写/稳定重开；schema 7 orphan receipt 改口令、备份、恢复；未知/降级/篡改/错误口令拒绝                                                                                                                                     | Windows 磁盘故障、权限和跨卷路径 E2E；备份 UI 人工流程                                                                       | 备份有意不包含浏览器 Profile；历史备份仍由其原口令独立解密；恢复不自动删除旧孤儿目录。                                          |
| 创建 Silo 和独立 `browser-data` 目录                                    | **部分实现**                   | [`domain.rs`](../apps/desktop/src-tauri/src/domain.rs)、[`vault.rs`](../apps/desktop/src-tauri/src/vault.rs)、[`lib.rs`](../apps/desktop/src-tauri/src/lib.rs)；锁定 Vault 不创建目录有测试                                                                                                                            | Win10/11 上创建 A/B，运行本地 session fixture，验证 Cookie/LocalStorage/IndexedDB/Cache/Service Worker 互不可见且 A 重启持久 | 尚无真实浏览器 A/B E2E；正向创建与浏览器状态隔离未被自动证明。                                                                  |
| 编辑、重命名、归档、恢复、明确删除、磁盘占用                            | **已实现且自动验证**           | Rust 生命周期/路径边界测试与桌面 UI/命令契约测试；运行中、Singleton lock、symlink/reparse 越界拒绝；删除清 seed/secret/证据引用                                                                                                                                                                                        | Windows junction 与真实浏览器锁 E2E；`.deleting-*` 安全孤儿审计                                                              | 元数据与网络替换为两个命令，跨命令不是事务；失败删除可能安全遗留 quarantine。                                                   |
| Chrome/Edge 发现、版本核验与安全参数数组                                | **已实现，待真实 Windows E2E** | [`domain.rs`](../apps/desktop/src-tauri/src/domain.rs)、[`vault.rs`](../apps/desktop/src-tauri/src/vault.rs)：创建/显式更新时保存 canonical path 与实际版本；每次启动前核验文件名、`--version` 产品前缀、版本基线，Windows 额外核验 Authenticode 发布者；漂移进入 `version_drift` 且需显式 `recheck_silo_browser` 接受 | Win10/11 × Chrome/Edge Stable/Beta 更新前后运行；签名损坏、junction/symlink 目标替换、路径大小写和厂商签名链矩阵             | 自动测试使用仅在 `cfg(test)` 生效的版本输出 harness；不能替代真实 GUI 浏览器 `--version`、PowerShell 签名链和企业安装路径 E2E。 |
| 独立 `--user-data-dir` 且默认 Profile 不变                              | **部分实现**                   | `Silo::launch_arguments*` 固定受管目录；不提供默认 Profile 导入 API                                                                                                                                                                                                                                                    | 启动前后对默认 Chrome/Edge Profile 做目录快照/哈希与进程参数检查                                                             | 代码意图正确，但没有真实 Windows 自动证据。                                                                                     |
| 目录锁拒绝、单 Silo 运行、不自动强杀                                    | **已实现，待真实 Windows E2E** | [`launcher.rs`](../apps/desktop/src-tauri/src/launcher.rs) 同时解释最小运行记录中的 PID 与三个 Chromium Singleton 文件；任何歧义进入 `recovery_required`，实现从不删除锁、kill PID 或声称可强制关闭已启动浏览器                                                                                                        | 真实浏览器活锁/陈旧锁/PID 复用/浏览器进程接管/桌面重启/多个控制器 E2E                                                        | `tasklist` 与 Profile 锁组合仍只是恢复证据，不是进程所有权证明；歧义有意留给用户处理。                                          |
| 运行状态、正常退出与异常恢复                                            | **已实现，待真实 Windows E2E** | `RuntimeManager` 持久化仅含 `siloId/pid/startedAt/lastSeenAt/state` 的明文非敏感记录；恢复 harness 覆盖 PID+锁、停止和 required relay 丢失后的 `verification_failed`                                                                                                                                                   | 浏览器/relay/desktop/系统分别崩溃，验证 PID 接管、锁建立时序、磁盘写入中断和记录损坏                                         | 不持久化启动参数、凭据或 relay 监听器；桌面崩溃后 relay 无法接管，required proxy 因此按 fail-closed 失败而不是恢复为“健康”。    |

## V0.2：Companion、Native Host 与网络配置

| 要求                                            | 当前状态                       | 当前证据                                                                                                                                                                                                                                              | 完整验收                                                                                       | 残余风险                                                                                    |
| ----------------------------------------------- | ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| 固定 HTTP/SOCKS5、凭据保护和随机 loopback relay | **已实现且自动验证**           | [`proxy_relay.rs`](../apps/desktop/src-tauri/src/proxy_relay.rs) 覆盖 HTTP CONNECT、SOCKS5 auth、随机 loopback；[`vault.rs`](../apps/desktop/src-tauri/src/vault.rs) 验证凭据只以 reference 出现在解密数据模型                                        | Rust 测试 + 真实带/不带认证端点 E2E；检查进程参数、日志和 envelope                             | 单元测试不证明所有浏览器版本的 DNS/WebRTC/IPv6 路径。HTTPS/SOCKS4 带认证不由 relay 支持。   |
| “必须代理”不含 DIRECT fallback                  | **已实现，待真实 Windows E2E** | [`domain.rs`](../apps/desktop/src-tauri/src/domain.rs) 禁止 bypass、加入 resolver/QUIC/WebRTC 参数；浏览器只连本地 relay；[`launcher.rs`](../apps/desktop/src-tauri/src/launcher.rs) 运行中检查 relay，失效转 `verification_failed` 且明确不改 DIRECT | 从已启动 Silo 内停止 upstream/Mihomo/relay，证明页面请求、DNS、WebRTC、QUIC 均无宿主直连       | Rust 能证明状态和参数不改 DIRECT，不能证明所有真实 Chrome/Edge 版本及所有协议路径没有旁路。 |
| 外部 Mihomo Controller、稳定节点绑定和回读      | **已实现，待真实 Windows E2E** | [`mihomo.rs`](../apps/desktop/src-tauri/src/mihomo.rs) 限 loopback，启动时显式 PUT 并回读；运行中健康检查只读当前选择，`rebind_silo_mihomo` 才再次写入                                                                                                | Mihomo 版本矩阵覆盖认证、401、未知组/节点、超时、崩溃和热重载                                  | VeriSilo 不控制 Mihomo 内部规则是否又含 DIRECT；Controller 回读也不是实际出口证据。         |
| 随包 Mihomo、订阅托管、TUN、多 Silo 并行        | **部分实现**                   | 当前只有外部 Controller 适配和 WSL 只读探测；文档列出 gate                                                                                                                                                                                            | 许可证/SBOM、签名更新、端口归属、提权、路由回滚、崩溃恢复和防泄漏 E2E 全部通过后才启用         | 核心生命周期、订阅解析、TUN 和并行调度均未实现。                                            |
| 明确轮换操作、健康监控与失败告警                | **已实现，待真实 Windows E2E** | Rust 命令 `recheck_silo_runtime` 执行用户触发回读，`rebind_silo_mihomo` 执行用户触发重绑；常规健康刷新只检查既有 relay/绑定，不后台随机轮换长期身份                                                                                                   | 真实运行中节点漂移、Controller 失联、relay 线程退出、恢复后人工重绑和出口复验                  | 当前是轮询式状态刷新而非独立后台服务；重绑成功仍需 Companion 用户主动验证实际出口。         |
| Silo 内出口 IP、DNS、WebRTC、QUIC 证据回传桌面  | **部分实现**                   | 严格协议将用户主动 `NetworkCheckResult` 与固定覆盖声明交给活动 Silo；Host inbox 二次鉴权后写入加密 Vault 历史；实际公网 IP 可更新为 verified                                                                                                          | Chrome/Edge 真实 Native Messaging E2E；QUIC/TLS/WebRTC 与实际 DNS 路径需单独观测               | 公共 DoH 仅答案比较；DNS/WebRTC/QUIC 明确保持 unavailable/not observed。                    |
| Native Messaging 严格协议和敏感字段拒绝         | **已实现且自动验证**           | [`native_host.rs`](../apps/desktop/src-tauri/src/native_host.rs)、[`protocol.ts`](../packages/contracts/src/protocol.ts) 限版本、未知字段、256 KiB、origin 和敏感 key；正负测试                                                                       | 跨语言 contract fixtures、frame fuzz、部分读取/多帧、Windows stdio 集成、每个命令授权测试      | Rust/TS schema 目前分别维护；`get_runtime_status` 与 `open_desktop` 固定返回 unavailable。  |
| 生产 ID 白名单、HKCU 注册、安装/升级/卸载       | **部分实现**                   | [`register-native-host.ps1`](../scripts/register-native-host.ps1) 可按显式 ID 写 HKCU；[`native-host-manifest.template.json`](../apps/desktop/src-tauri/resources/native-host-manifest.template.json) 仍是占位符                                      | 正式 Chrome/Edge ID、安装器生成 manifest/allowlist、升级路径更新、卸载仅移除注册且保留用户数据 | 脚本允许任意合法格式 ID，适合开发但不是生产固定白名单；NSIS 尚未集成注册生命周期。          |
| 扩展缺失时桌面核心降级                          | **部分实现**                   | 启动器不依赖扩展，UI 说明 Companion 可选                                                                                                                                                                                                              | 在全新 Silo 未安装扩展时完成创建/启动/代理；桌面显示“未连接”且不误报 Silo 内证据               | 桌面目前没有可靠的每 Silo 扩展连接状态。                                                    |

## V0.3：观察、解释、证据历史和报告

| 要求                                                                                                                                      | 当前状态     | 当前证据                                                                                                                                                   | 完整验收                                                                   | 残余风险                                                                      |
| ----------------------------------------------------------------------------------------------------------------------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| 分模块采集 Navigator、UA-CH、时区、屏幕、Canvas、WebGL/WebGPU、Audio、字体、媒体设备、权限、存储、WebRTC、Window/iframe、Dedicated Worker | **部分实现** | Companion content/MAIN-world 采集器及 [`models.ts`](../packages/contracts/src/models.ts)；单个采集错误由报告结构承载                                       | 每个采集器独立 fixture、失败隔离、覆盖声明、Chrome/Edge 版本矩阵           | MAIN world 不保证先于页面；通用 Worker/SharedWorker/Service Worker 仍不覆盖。 |
| 新手摘要、原因说明、开发者原始值                                                                                                          | **部分实现** | Companion [`report-summary.ts`](../apps/extension/src/report-summary.ts) 有人话事实和规则测试；桌面 [`App.tsx`](../apps/desktop/src/App.tsx) 有能力边界 UI | 桌面接收真实 Silo 报告，并以相同信息层级展示；可访问性和中英文流程人工验收 | 当前桌面展示的是路线/控制器网络信息，不是 Silo 扫描报告。                     |
| 本地证据历史                                                                                                                              | **部分实现** | Companion 将当前页存入 `storage.session`，脱敏记录存入 trusted-context `storage.local`                                                                     | 桌面端持久证据模型、按 Silo 查询、保留/删除策略、迁移和损坏恢复            | 扩展本地记录不是桌面证据历史；没有跨 Silo 归属和生命周期审计。                |
| 脱敏 JSON/HTML 导出与明确确认                                                                                                             | **部分实现** | [`report-export.ts`](../apps/extension/src/report-export.ts) 默认脱敏高敏值、HTML 转义且有测试                                                             | 桌面导出、用户确认、文件写入失败/覆盖、schema 版本和导入兼容测试           | 当前只覆盖 Companion 报告；没有桌面合并网络/运行/环境证据的审计包。           |
| 可观察 API 调用边界                                                                                                                       | **部分实现** | UI/文档明确 MAIN-world 与 Worker 覆盖限制                                                                                                                  | 自动覆盖声明测试证明未采集路径不会被标为成功                               | 现有测试主要校验合约，尚无恶意页面/时序回归套件。                             |

## V0.4：每 Silo 稳定实验控制

| 要求                                                                          | 当前状态       | 当前证据                                                                                                     | 完整验收                                                        | 残余风险                                                                   |
| ----------------------------------------------------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `observe → apply → verify → restore` 状态机                                   | **部分实现**   | Companion 可选 privacy 设置保存 restore point，能力模型区分 configured/applied/verified                      | 桌面/扩展共享的显式状态机、失败回滚、浏览器重启恢复和跨版本测试 | 没有通用修改器协议，也没有把验证证据持久绑定到 Silo。                      |
| Silo 派生短期令牌，长期种子不进页面                                           | **部分实现**   | Vault 为每个 Silo 保存随机 seed reference，Native Host 禁止敏感浏览器状态                                    | HKDF/域分离设计、短期令牌生命周期、页面/日志泄漏测试            | 当前 seed 未用于控制层，也没有派生协议。                                   |
| UA/UA-CH、语言、时区、Window/iframe、Dedicated Worker、请求头协调和按站点回退 | **部分实现**   | 仅有观察、少量扩展 privacy 设置和路线 UI                                                                     | 跨上下文一致性、请求头观测、站点开关、兼容性失败自动恢复        | stock Chrome/Edge 扩展无法可靠覆盖所有上下文；受控引擎尚未存在。           |
| SharedWorker、Service Worker、通用跨域 Worker 修改                            | **明确不支持** | [`extension-ceiling.md`](extension-ceiling.md)、[`known-leaks.md`](known-leaks.md) 明确当前 MV3/stock 层边界 | UI 始终不可选择并指向受控引擎/VM；测试不得误报覆盖              | 即使未来引擎支持其中一部分，也必须按引擎和版本重新取证，不能沿用当前声明。 |

## V0.5：VeriSilo Labs

| 要求                                                                    | 当前状态     | 当前证据                                                       | 完整验收                                                                    | 残余风险                                                                        |
| ----------------------------------------------------------------------- | ------------ | -------------------------------------------------------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| 标签页 Cookie 虚拟化、Set-Cookie 截获、Dedicated Worker 包装只进入 Labs | **部分实现** | 文档把高风险实验排除在默认隔离之外；当前仓库没有对应 Labs 后端 | 独立 feature gate、显著实验标识、隔离/泄漏/兼容停止条件、逐项关闭和恢复测试 | 目前不是实验实现，只是未实现；不得把独立 Profile 隔离等同标签页 Cookie 虚拟化。 |
| 泄漏停止条件与默认模式保护                                              | **部分实现** | [`known-leaks.md`](known-leaks.md) 列出部分已知边界            | 机器可执行 gate 阻止未达标能力进入默认 build；回归报告记录泄漏是否停止      | 当前文档没有与构建/发布流水线绑定。                                             |

## V0.6：封顶审计与 Windows 发布

| 要求                                               | 当前状态         | 当前证据                                                                                                                                                                                                                                         | 完整验收                                                                                              | 残余风险                                                                                                 |
| -------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| capability report、known leaks、兼容矩阵、权限说明 | **部分实现**     | [`capabilities.md`](capabilities.md)、[`capability-report.example.json`](capability-report.example.json)、[`known-leaks.md`](known-leaks.md)、[`compatibility-matrix.md`](compatibility-matrix.md)、[`store-disclosure.md`](store-disclosure.md) | 从自动测试/运行记录生成版本化报告，报告链接构建来源和哈希                                             | 目前主要是手写文档/示例，容易与实现漂移。                                                                |
| NSIS 当前用户安装包                                | **部分实现**     | [`tauri.conf.json`](../apps/desktop/src-tauri/tauri.conf.json) 选择 `nsis` 与 `currentUser`                                                                                                                                                      | Win10/11 安装、升级、回滚、卸载、保留/删除数据矩阵；验证 Native Host 注册                             | 只有打包配置，没有安装生命周期 E2E或可审计产物。                                                         |
| 可复现构建、SBOM、第三方许可证、校验和             | **部分实现**     | 锁文件生成 663 组件依赖清单、CycloneDX 1.6/SPDX 2.3；SHA-256/provenance generate/check；NOTICE 明确未捆绑组件；分离的 unsigned/signed Windows workflows                                                                                          | 在干净 Windows runner 生成真实 artifact、比较构建；补依赖许可证法律结论和签名日志                     | SPDX 许可证当前诚实标为 `NOASSERTION`；尚无真实 CI artifact，不能声称 bit-reproducible。                 |
| Authenticode 与商店披露                            | **外部条件阻塞** | 商店披露草案、ID 注入/占位符拒绝、EXE/PS1 签名 gate、内层先签和最终 coverage policy 已在仓库；正式 ID、证书和审核结果不在仓库                                                                                                                    | 用受控证书运行 signed workflow、验证 timestamp/signature report，并以正式 Chrome/Edge ID 完成安装测试 | workflow 定义和 self-test 不是实际 Authenticode 产物；证书托管、timestamp 可用性和商店审核仍是外部门槛。 |

## V0.7：受控浏览器引擎

| 要求                                                                               | 当前状态     | 当前证据                                                               | 完整验收                                                              | 残余风险                                                               |
| ---------------------------------------------------------------------------------- | ------------ | ---------------------------------------------------------------------- | --------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| 具备 capability negotiation 的 `EngineAdapter`                                     | **部分实现** | 只有路线定义和 stock launcher；没有接口、注册表、状态机或 adapter 测试 | stock/controlled/Camoufox 共用 contract fixtures，未知能力安全降级    | UI 中“V0.7”是路线，不是 adapter。                                      |
| 可发布 stock Chrome/Edge adapter                                                   | **部分实现** | 当前 launcher 可启动 stock Chrome/Edge                                 | 迁入 adapter 后保持 V0.1 行为，冻结能力清单、版本探测、健康和回滚语义 | 现有直接调用结构不满足 EngineAdapter 生命周期。                        |
| 受控 Chromium 实验实现                                                             | **部分实现** | 无源码、补丁集、二进制清单、更新器或制品哈希                           | 固定上游版本/patch、可复现构建、签名校验、安装/更新/回滚、紧急禁用    | 合法托管与签名是外部条件，但仓库内 patch/build/update 代码也尚未实现。 |
| 可选 Camoufox 原型                                                                 | **部分实现** | 只有参考与许可证规则                                                   | adapter、固定版本、包校验、MPL NOTICE、兼容矩阵和非默认安装流程       | 不能把上游项目存在当作 VeriSilo 原型。                                 |
| 约束型身份模板和每 Silo 稳定配置                                                   | **部分实现** | 只有 per-Silo 随机 seed；没有身份模板 schema/规则引擎                  | 约束求解、确定性、跨 Silo 区分、版本迁移和错误组合拒绝测试            | 随机 seed 本身不产生一致、真实分布的设备配置。                         |
| apply/verify/restore、站点回退、包更新/回滚/禁用                                   | **部分实现** | 无引擎级实现                                                           | 故障注入、原子更新、签名失败、回滚、站点兼容和 kill-switch 测试       | 当前无可验收制品。                                                     |
| Canvas/WebGL/字体/UA/UA-CH/语言/时区/Window/iframe/Worker/请求头/TLS/QUIC 直接证据 | **部分实现** | Companion 能观察部分 JS 信号；required proxy 只“应用”禁 QUIC 参数      | 跨上下文、请求头和真实 ClientHello/协议协商记录；每字段单独状态       | JS 扫描不能证明 TLS/QUIC；参数不能证明协议实际未使用。                 |

## V0.8：本地环境后端

| 要求                                              | 当前状态         | 当前证据                                                                                                                | 完整验收                                                                             | 残余风险                                                                     |
| ------------------------------------------------- | ---------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| 统一、按后端声明能力的 `EnvironmentBackend`       | **仓库内实现**   | Rust trait/manager 与严格 contracts 覆盖九项操作；每后端逐项返回 available/unavailable，typed argv 无 shell 文本        | 在真实 Windows SKU 上执行 WSL/Sandbox/Hyper-V contract suite                         | 仓库单测证明控制器与 schema，不证明虚拟化运行时。                            |
| WSL Chromium 生命周期与独立 Profile/网络/来宾证据 | **仓库内实现**   | 固定 root guest-agent、UUID/Profile/PID 绑定、loopback SOCKS5H 出口与自托管 proxy-DNS 回执、过期/错绑定拒绝             | WSLg/Chromium、无配置/错答案/直连诱饵/代理崩溃及真实 DNS 泄漏 Windows E2E            | guest OS resolver 与 OS 级网络强制保持 unavailable；证据失败不自动杀浏览器。 |
| Windows Sandbox 实验室                            | **控制面实现**   | 默认拒绝 `.wsb`、只读 bootstrap、PID/start-time/exe/descriptor/hash receipt、优雅 stop 与 exit-before-destroy           | Home/Pro 差异、真实 feature、窗口关闭、单实例与默认拒绝策略 E2E                      | 无可靠 guest return channel；网络/DNS/browser-ready 保持 unavailable。       |
| Hyper-V 持久 VM                                   | **控制面实现**   | 受限 PowerShell、镜像清单、VM GUID/name/gen2/image hash receipt、磁盘/交换机与精确 lifecycle、失败 receipt              | 合法 VHDX、固定 guest-agent version/hash、真实签名生命周期与 guest 网络/Profile 回执 | 合法镜像与来宾 Agent 是外部条件；控制面不冒充来宾。                          |
| 默认拒绝宿主可写映射、剪贴板、设备和透传          | **仓库内实现**   | Sandbox descriptor 与 Hyper-V drift checks 默认拒绝 writable mapping、clipboard、设备、vGPU/GPU partition 等            | 配置生成测试 + 来宾/宿主访问负向 Windows E2E                                         | 静态配置不能代替实际 SKU/虚拟化行为。                                        |
| 来宾内部出口、DNS 与 Profile/字体隔离             | **部分实现**     | WSL guest-agent 可返回绑定出口与 proxy DNS；guest resolver 单列 unavailable；Sandbox/Hyper-V guest evidence unavailable | 从真实来宾验证出口、resolver/DNS 泄漏、Profile/字体与代理故障                        | 桌面/Sandbox/Hyper-V 控制器回执不能替代来宾证据。                            |
| 真实 WSL/Sandbox/Hyper-V 环境矩阵                 | **外部条件阻塞** | 验收必须运行在启用对应功能的 Windows SKU/硬件，Hyper-V 还需合法镜像与固定 Agent                                         | Win10/11、Home/Pro/Enterprise、虚拟化开关、重启、真实浏览器/guest-agent 矩阵         | 当前剩余是外部运行与制品证据，未执行前 V0.8 不能宣称完整验收。               |

## V0.9：自托管远程环境

| 要求                                            | 当前状态         | 当前证据                                             | 完整验收                                                          | 残余风险                                                |
| ----------------------------------------------- | ---------------- | ---------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------- |
| 最小远程 Agent、相互认证和协议版本协商          | **部分实现**     | 无 Agent crate/package、远程 schema 或 mTLS/密钥协议 | 正负协议测试、密钥轮换、重放/降级/未知字段/过大消息/越权 fuzz     | Native Messaging 协议是本机扩展桥，不能替代远程控制面。 |
| 环境创建、持久会话、健康、TTL、销毁             | **部分实现**     | 只有路线文档                                         | 断线重连、TTL 到期、幂等销毁、超限和孤儿资源回收集成测试          | 没有远端状态机或资源实现。                              |
| 独立网络/加密卷和远端内部证据                   | **部分实现**     | 无实现                                               | 每环境进程/VM 边界、卷密钥归属、IP/DNS/协议证据、删除证明         | 不能用 VPS/代理供应商的声明替代现场证据。               |
| 画面/输入通道与人工/自动化分权                  | **部分实现**     | 无实现                                               | 通道加密、剪贴板/文件/输入授权、会话并发和撤销测试                | Playwright/CDP 入口不得自动获得人工会话全部权限。       |
| 节点所有者、地区、密钥、成本、活动、删除状态 UI | **部分实现**     | 当前桌面无远程节点模型/UI                            | schema、来源标注、价格仅提示、时钟/离线语义和删除状态测试         | 地区与成本可能过期，必须带来源和时间。                  |
| 真实自托管节点与网络验收                        | **外部条件阻塞** | 需要用户所有的远端主机、DNS、TLS 和真实出口          | 至少两个地区节点的故障/延迟/失联/删除 E2E，并保存可脱敏日志和哈希 | 当前 Agent/控制面未实现，外部节点本身不能解除代码缺口。 |

## 测试与证据闭环

| 要求                                                       | 当前状态             | 当前证据                                                                                                 | 完整验收                                                                                                               | 残余风险                                                         |
| ---------------------------------------------------------- | -------------------- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| TypeScript 类型、单元、生产构建、扩展包静态审计            | **已实现且自动验证** | 2026-07-28 在 Linux 工作区执行 `pnpm check`、`pnpm test`、`pnpm build`、`pnpm extension:verify` 均退出 0 | CI 每次 PR 固定 Node/pnpm/lockfile，保存日志与构建 artifact                                                            | 这些检查不启动 Tauri、Chrome、Edge 或 Windows installer。        |
| Rust 模型/Vault/launcher/Mihomo/relay/Native Host 单元测试 | **部分实现**         | 源码含 21 个 Rust `#[test]`；[CI](../.github/workflows/ci.yml) 配置 Windows Rust 1.88 check/test         | 在当前提交的 Windows runner 重新运行并保存 job URL/log；增加缺失状态机/迁移测试                                        | 本次本地环境没有 `cargo`，因此本报告没有产生新的 Rust 执行日志。 |
| 模型/协议模糊测试                                          | **部分实现**         | 有严格 schema 和少量恶意输入单测                                                                         | cargo-fuzz/proptest 与 JS property tests，建立 corpus、超限、未知字段、partial frame 和路径输入覆盖                    | 严格反序列化不等于 fuzz。                                        |
| Windows 10/11 × Chrome/Edge E2E                            | **部分实现**         | 只有 [`tests/fixtures/session-site`](../tests/fixtures/session-site) 本地站点和 release checklist        | A/B 登录与存储、默认 Profile、异常恢复、Vault、代理/Mihomo 断线、Silo 内证据、Native Host 正负消息、扩展缺失全部自动化 | 当前 fixture 未被测试 runner 调用。                              |
| WSL/Sandbox/Hyper-V/远端实验矩阵                           | **外部条件阻塞**     | 需要多种 Windows SKU、虚拟化硬件、镜像和远端节点                                                         | 先完成 provider，再在列明环境中保存 machine-readable 结果                                                              | 当前主要阻塞仍是实现缺失，不能只等待实验室。                     |

## 安装、发布和供应链

| 要求                                 | 当前状态         | 当前证据                                                                                                         | 完整验收                                                                                                      | 残余风险                                                                        |
| ------------------------------------ | ---------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| 安装/升级/卸载、数据保留与回滚       | **部分实现**     | Tauri release sidecar、NSIS postinstall/preuninstall hooks、幂等 HKCU 注册/验证/注销脚本和失败回滚策略           | 全新安装、同版本修复、跨版本升级、失败回滚、卸载保留 Vault/Profile 的真实 Win10/11 E2E                        | PowerShell/NSIS hooks 尚未在真实安装器运行；VM 数据策略留待 V0.8。              |
| Native Host 两个浏览器注册           | **部分实现**     | 手工 PowerShell 脚本与模板存在                                                                                   | NSIS 按正式 ID 原子写两个 manifest、allowlist 和 HKCU；卸载清理注册，不删用户 Profile                         | 模板仍有占位符，安装包未强制拒绝占位符。                                        |
| SBOM、许可证和 NOTICE                | **部分实现**     | npm/Cargo lockfile、MPL license、基础 NOTICE                                                                     | CycloneDX/SPDX + Rust/JS license report；每个引擎/核心/镜像单列版本、来源、许可证和修改                       | 第三方依赖清单尚未生成；未来 Mihomo/Camoufox/Chromium 义务更复杂。              |
| Checksums、provenance、可复现性      | **部分实现**     | Git commit 是源码锚点；没有发布 artifact 清单                                                                    | 对 installer、desktop、native host、extension、engine manifest 生成 SHA-256 和 provenance；双 runner 重建比较 | 本基线没有可引用的 Windows installer/host hash。                                |
| Authenticode 签名 dry-run 与正式签名 | **部分实现**     | 已有 DryRunSigning/SignAndVerify/VerifySigned gate、PS1 覆盖和 secrets-gated workflow 定义，但未实际执行证书签名 | 在隔离 Windows 测试中用测试证书做签名/验证/篡改负测；正式发布再使用受控证书或 HSM                             | 正式证书是外部条件；仓库内静态闭环不能替代真实时间戳、签名链和 installer 验证。 |
| 正式商店 ID、审核和产品域 DNS/HTTPS  | **外部条件阻塞** | 只有 manifest/文档占位和项目 URL 计划                                                                            | 商店审核后把不可变 ID 注入 release；域名配置 DNS/HTTPS 后运行链接检查                                         | 不得在此之前把占位 ID、开发扩展或未配置域名称为生产可用。                       |

## 当前可重复命令与环境

以下命令在 2026-07-28 的 Linux 开发工作区、Node 22/pnpm 11 lockfile 环境中执行成功：

```bash
pnpm check
pnpm test
pnpm build
pnpm extension:verify
```

当前环境没有 `cargo`，因此 Rust 结果必须在配置了 Rust 1.88 的 Windows runner 复核：

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

最终审计还必须记录：Windows build number/SKU、Chrome/Edge 完整版本、WSL kernel/发行版、Sandbox/Hyper-V 配置、Mihomo 版本、远端 Agent/OS、验证端点、测试开始结束时间、Git commit、构建 provenance、每个交付物 SHA-256 和原始日志位置。

## 不能由本基线声称的结论

- 不能声称 V0.1–V0.6 “整体完成”；当前只有若干窄能力已自动验证。
- 不能声称代理设置等于 DNS、WebRTC、TLS 或 QUIC 全路径无泄漏。
- 不能声称扩展扫描结果已回传桌面，或桌面已有按 Silo 证据历史。
- 不能声称 V0.7–V0.9 已实现；它们目前主要是路线和 UI 能力模型。
- 不能声称 VM/远端环境改变了真实硬件，也不能承诺绕过风控或不可检测。
- 不能声称已有可发布 Windows installer、生产 Native Host 注册、SBOM、可复现构建或 Authenticode 签名。

本文件应在每个实现批次后更新。状态升级必须同时补齐直接证据；最终审计不能删除未达项，只能说明它被实现、由外部条件阻塞或被产品明确排除。
