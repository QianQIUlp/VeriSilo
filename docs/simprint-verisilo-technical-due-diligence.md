# Simprint × VeriSilo 源码级技术尽调 / 生存性审查报告

日期：2026-09-09
性质：read-only architecture / implementation audit（未修改任何仓库；分析期临时 worktree 已清理）
审查人：AI agent（GLM-5.3，Trae）

---

# Executive Conclusion

**最终结论：B — NARROW / DIFFERENTIATE（收缩并差异化）。**

Simprint 是一个**产品成熟度远高于 VeriSilo 的本地优先开源指纹浏览器工作台**：完整的 Profile CRUD / 分组 / 标签 / 团队 / 回收站、130+ 个真实 Chromium 源码级指纹补丁、内容寻址的内核包管理（三层校验）、本地 REST API + 52 个 MCP 工具 + 每环境 CDP 端点、同步器（多窗口输入镜像）、账号自动导入、扩展管理、代理健康检查。在「桌面多环境管理产品」这一层，VeriSilo 与 Simprint 是大面积重复，且 Simprint 已解决大量 VeriSilo 未解决的产品问题。

但 VeriSilo 的核心五元模型中，有**两层 Simprint 源码明确不存在**：

1. **Resolved Identity Artifact**：Simprint 的指纹是前端 `Math.random()` 生成、以自由 JSON 存入 `environment_configs` 表的可变配置块——无版本、无 schema 校验、无 hash/integrity、无锁定语义、无历史。它更接近任务书里的「模型 A：一组可编辑 browser launch settings」，而不是身份工件。
2. **Runtime Evidence**：Simprint 没有任何「实际运行观察」机制——无会话历史表、无网站可见身份观察、无出口 IP 运行记录、无证据归属；运行状态是纯内存态，DB 状态列永远 `ready`（状态更新函数是死代码）。日志只能证明启动意图。

另有**两层 Simprint 只是部分覆盖**：Engine Binding（有内容寻址 kernel 绑定与 hash 校验，但无代码签名、二进制构建来源不可证明）与 Network Policy（有环境级代理绑定与 mihomo 编排，但无 fail-closed 语义——代理失败时 IP 检测路径明确回退直连、凭据明文存储、无启动前 preflight gate）。

同时必须诚实记录：**Simprint 在引擎级伪装的面上有 VeriSilo（经 Camoufox）做不到的事**——SSL/TLS 指纹（BoringSSL 扩展排列）、MAC 地址替换（net 层）、端口扫描保护、Client Hints 与 JS UA 的分版本伪装，这些是 Chromium 源码补丁独有的能力面。

因此这不是 A（STOP/ADOPT）——Simprint 没有覆盖 VeriSilo 的证据链与身份完整性层；也不是 C（CONTINUE）——「产品表面类似」的说法会严重低估 Simprint 在产品层与 Chromium 补丁层的实质覆盖度。VeriSilo 应收缩为「可验证身份运行时（verifiable identity runtime）」，停止与 Simprint 竞争桌面工作台的完整度。

---

# Repository Baselines

## Simprint（主仓库）

- path：`C:\Users\qiu\src\simprint`
- branch：`main`（= `origin/main` = `origin/HEAD`）
- HEAD：`d2dc939bfae8efda8e4e4d791587d7a364ca4dd8`（"refactor: keep only production Cargo feature"）
- remote：`https://github.com/Simprint/simprint.git`
- working tree：干净 [VERIFIED]
- tags：`v0.2.31` … `v0.2.22`（近 10 个，v0.2.x 系列）
- submodules：无（`.gitmodules` 不存在）[VERIFIED]
- 技术栈：Tauri 2 + React 19 + Rust 2024 + sqlx(SQLite)；许可证 AGPLv3
- 仓库包含完整源码（前端 `src/` + 插件页 `plugins/pages/` + Rust `src-tauri/`（crates: business/runtime）+ `deploy/` 发布脚本）

## simprint-browser-kernel（内核仓库）

- path：`C:\Users\qiu\src\simprint-browser-kernel`
- branch：`main` = **纯文档 landing branch**（仅 README.md，无实现）
- 实际源码分支：`origin/simprint/m144` @ `9f7b8e0`（"ci: use current action runnings"），Chromium 基线 `144.0.7559.118`
- remote：`https://github.com/Simprint/simprint-browser-kernel.git`
- working tree：干净 [VERIFIED]；tags：无
- m144 分支结构：`core/`（kernel_api v1 头文件）、`driver/`（Python patch 编排器，uv 管理）、`overlay/`（branding/ntp/syner/auth/fingerprint/review/account/proxy/cookie 九模块）、`port/`（branch-local port 契约）、`tests/`（契约测试）
- 状态判定：**legacy overlay/patch 管线（迁移中），非完整 Chromium fork**。证据：`port/manifest.toml` 全部 9 个 capability `state = "legacy-patch"`；`core/README.md:12-14` 自述仍在迁移；kernel_api v1 仅 2 个头文件（版本常量 + 能力枚举 + 纯虚接口），且 app 仓库 grep `kernel_api|CapabilityProvider` 零命中 [VERIFIED]
- 分析方式：为只读审查在 `%TEMP%\sbk-m144` 创建了 detached worktree（分析完毕已移除），未触碰任何分支与工作树

## VeriSilo

- path：`C:\Users\qiu\src\VeriSilo`
- branch：`baseline/dev`（= `origin/baseline/dev`；本 HEAD 同时被 `agent/integration/prepare-current-source-c6f17a` 引用）
- HEAD：`f6ae647764b2577f2286c1f208c9f02acd4b6fde`（"build(release): parameterize managed-browser candidate version instead of hardcoded rc1"）
- remote：`https://github.com/QianQIUlp/VeriSilo.git`
- working tree：干净 [VERIFIED]
- 当前阶段（git log + AGENTS.md 一致）：pre-RC 产品稳定化（qa-r1/qa-r2 分支合入、发布构建参数化）
- 规模：`apps/desktop/src-tauri/src` 29 个 .rs、46,962 行（含内联测试；launcher.rs 338KB / vault.rs 302KB / engine.rs 285KB）；前端 12,560 行 TS/TSX

三个仓库均在本地、均为完整源码、无 `NOT_LOCALLY_AVAILABLE` 项。

---

# Scope and Evidence Standard

- 只读审查：未 commit、未 push、未开 PR、未修改任何仓库文件；唯一临时产物为 `%TEMP%\sbk-m144` detached worktree（已清理）与仓库外的本报告。
- 证据分级：[VERIFIED] = 源码直接证明（含审查方抽样复核的关键断言）；[INFERENCE] = 多个源码事实推导；[PROJECT_CLAIM] = 仅 README/docs 声明；[UNKNOWN] = 现有源码无法确认。
- 审查深度：Simprint 主仓库全量结构探查 + 5 条调用链逐级追踪；kernel m144 分补丁文件级审读；VeriSilo 主检出源码级核验（不含 `.verisilo-worktrees/` 工作树副本）。未执行任何一方产品（无 runtime 行为验证），故「运行时语义」结论均限于源码可证范围。
- 关键断言抽样复核记录：前端 `Math.random` 指纹生成（fingerprint-generator.ts:389-412 复核）、`fingerprint_settings: serde_json::Value` 自由 JSON（entitys/dto 复核）、kernel 噪声 seed 回退 `"default"`（fingerprint_browser_fingerprint.cc:579-602 复核）、VeriSilo 身份锁定（vault.rs:1043-1045 复核）、`observed.json` 网站观察（website_identity.rs:3,56 复核）、引擎版本 pin（engine.rs:42-50 复核）。

---

# Simprint Architecture Overview

分层（均为 [VERIFIED]）：

```text
React 19 前端 (src/, plugins/pages/*)
   │ Tauri invoke / HTTP
Rust app 层 (src-tauri/src: commands / services / local_api / mcp)
   │ dispatcher
business crate (SQLite via sqlx: 43 migrations, environments/proxies/kernels/...)
   │
runtime crate (环境生命周期: launcher / eventbus / kernel 管理 / status)
   │ Windows Named Pipe  \\.\pipe\simprint_{env_id}
   │ (Topic: LaunchConfig 0x0900, FingerprintApply 0x0200, ...)
定制 Chromium 144 内核 (simprint-browser-kernel m144: 130+ patches)
   │ CDP: 127.0.0.1:29200-29499 (端口池)
外部: mihomo(用户自备) / R2(内核下载) / GitHub(应用更新) / realip.cc 等 4 个 IP API
```

要点：
- 指纹与代理**不走命令行**（无 `--proxy-server`/`--user-agent`），启动后经命名管道 EventBus 全量下发，由 Chromium 内核补丁消费（`crates\runtime\src\infrastructure\eventbus\manager.rs:257-298`；kernel `auth\patches\003-eventbus-topics-h.patch` 的 `kLaunchConfig=0x0900` 与之对应）[VERIFIED]
- `services\auth` 是纯本地 SQLite 账户（`commands\auth.rs:13 create_local_user`）；无云端 sync、无 telemetry（sentry/posthog/umami/analytics 全仓零命中）[VERIFIED]
- IP/API 契约：local API 为 axum（默认 127.0.0.1，`remote_access=true` 时绑 `0.0.0.0`，`server\bootstrap.rs:34-38`）+ `sp-api-key` header；MCP streamable-HTTP `http://127.0.0.1:37110/mcp`，52 个工具 [VERIFIED]

---

# Simprint Profile Model

**数据结构** [VERIFIED]：
- `environments` 表（`migrations\20250120000010_create_environments.sql:4-34`）：`id, uuid, user_uuid, team_uuid, name, description, icon, icon_color, status(默认'ready'), group_uuid, proxy_uuid, system_info, kernel_info, fingerprint_summary, last_opened_at, created_at, updated_at, deleted_at`；FK：group `SET NULL`、proxy `SET NULL`。
- `environment_configs` 表（1:1，`20250120000011`）：`environment_uuid UNIQUE, window_info, basic_settings, fingerprint_settings, device_settings, preference_settings, project_metadata`——**全部是 TEXT 自由 JSON**。
- 内核绑定独立表 `environment_kernel_bindings`（`environment_uuid PK → CASCADE, kernel_id → browser_kernel_artifacts RESTRICT`）。
- **无独立 fingerprint 表、无 fingerprint_id**；`environments.fingerprint_summary` 列全仓 11 处引用均为读取、从未被写入（遗留空列）[VERIFIED]。

**一个 Simprint profile 包含**：磁盘 user-data-dir（`<cache>/browser/cache/<env_uuid>`，按 env_uuid 物理隔离，`runtime_bridge.rs:376-377`）+ DB 行 + config JSON（含指纹）+ proxy FK + kernel 绑定 + DB 侧 cookies/urls/accounts/tags + 扩展（CRX 缓存于 `<cache>/browser/extensions/`，解压进 user-data-dir 经 `--load-extension` 加载）[VERIFIED]。

**隔离性**：user data / cache / cookies / storage / extensions 物理按目录隔离 [VERIFIED]；cookies 同时有 DB 表用于启动注入（eventbus）。

**运行状态是纯内存**：app 层与 runtime 层各一个 `EnvironmentStatusManager` HashMap；DB `status` 列永远 `ready`——`update_environment_status`（`models\environments.rs:622`）全仓无调用者（死代码）[VERIFIED]。应用重启后状态 map 清空，无恢复逻辑、无 stale 清理代码；孤儿进程由 Windows Job Object `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`（`kernel\job.rs:27-64`）在宿主死亡时由 OS 兜底击杀 [VERIFIED]。

**删除/克隆**：软删除（`deleted_at`），**磁盘 user-data-dir 不清理**；**无环境克隆 route** [VERIFIED]。

---

# Simprint Fingerprint Model

结论：**模型 A（一组可编辑 browser launch settings）**，带两处局部成组约束。

- 数据模型：入口 `fingerprint_settings: serde_json::Value`（`entitys\environments.rs:44`），无 schema 校验、无版本、无 hash/integrity [VERIFIED 复核]。Rust 侧投影 `FingerprintConfig`（约 49 个 Option 字段，`infrastructure\runtime\api.rs:8-58`）；双端（TS/Rust/C++ 三处）各自手动维护强类型投影。
- 生成：**完全在前端、创建时一次性、`Math.random()` 逐字段随机**（`plugins\pages\create-window\src\utils\fingerprint-generator.ts:389-412`）[VERIFIED 复核]。无 BrowserForge、无设备数据库、无 dataset（`crates\business\resources` 只有内核清单 JSON）。
- 一致性约束（仅两处成组）：① `HARDWARE_PROFILES` 把 GPU vendor+renderer+CPU 核数+内存绑成合理组合（fingerprint-generator.ts:155-287）；② UA 由官方 Chrome 版本表 + OS 生成、切 OS 时联动重生成（user-agent-generator.ts:33-168）。
- **缺失的跨字段约束**：webglVendor/renderer 与 OS 不联动（Windows 环境可随机出 Apple GPU）；fontList 从 CREEPJS 列表随机取 10-25 个、与 OS 无关；resolution/DPR/maxTouchPoints 独立随机 [VERIFIED]。
- 持久化：`upsert_environment_config` 整块 JSON 覆盖；改单字段不联动其他字段；后端无 regenerate/校验逻辑；无克隆语义；`random_fingerprint_on_launch` 配置项全后端无消费者（无效配置）[VERIFIED]。
- 可复现性：同一 profile 重启后配置逐字节稳定（除 language/timezone 为空或 'ip' 时每次启动按代理 IP 重检测）[VERIFIED]。
- **kernel 噪声层缺陷（重要）**：Canvas/WebGL/Audio 噪声 seed 派生自 `SetCurrentProfileId(profile_id)`，但全 overlay 无任何调用点传入真实 env id——`GetCanvasSeed()` 等在未设置时回退 `SetCurrentProfileId("default")`（`fingerprint_browser_fingerprint.cc:579-602`）[VERIFIED 复核]。即**所有环境共享同一噪声 seed：相同页面内容在不同 profile 下产生相同噪声输出——跨 profile 可关联**。另 WebGL GLSL 版本 "random" 模式用 `static counter` 轮换，同一会话内每次 `getParameter` 返回不同值（059 补丁:43-60）——会话内自不一致。
- 无 versioning/migration：47 个 migration 无任何指纹 schema 变更；kernel 侧解析对缺失字段静默用默认值 [VERIFIED]。

---

# Browser Kernel Implementation

**实现层定位：真实、深度、源码级 Chromium patch 管线**（非 JS 注入、非扩展、非 CDP override）[VERIFIED]：

- 130+ 补丁（68 个属 fingerprint 模块）+ 新增 C++ 源文件，`driver`（Python）按 `order.txt` 顺序对**用户本地外部 Chromium 144 树**执行 `patch --batch --forward`，`sources/` 复制进树；构建由开发者自行 gn/autoninja（`unit.py:107` build 阶段 no-op）。
- 配置传递链：`RenderProcessHostImpl::Init()` 内 `ExportAllConfigAsJson()` → 新增 mojom 参数 `InitializeRenderer(..., fingerprint_config_json)` → renderer `ImportAllConfigFromJson`（补丁 016/017/019）——browser→renderer 进程级下发。
- 启动强制认证（auth 模块）：无 `--simprint-env-id` → 静默退出；EventBus 认证 5 秒超时未通过 → 静默退出（fail-safe）[VERIFIED]。
- **kernel_api v1 是迁移目标而非现状**：仅 `version.h`（kApiVersion=1）+ `capability.h`（4 能力枚举 + 纯虚接口），无实现、app 无消费者 [VERIFIED]。
- CI：kernel 仓库仅 `port-contract.yml`（契约校验 + 单测），**无构建 workflow、无签名步骤、无 tag** [VERIFIED]。

**发现的能力缺口**：时区配置有存储（`config_fingerprint_config_storage.cc:256 SetTimezone`）且 app 侧会检测下发，但全部补丁中未找到消费 `GetTimezone` 改 JS Date/ICU 时区的补丁——**时区伪装可能实际未生效** [UNKNOWN]。

---

# Fingerprint Surface Implementation Matrix

| 指纹面 | 机制 | 执行层 | kernel 侧位置 |
|---|---|---|---|
| UA (JS) | patch `GetConfiguredUserAgent()` | Blink | 037-navigator-base |
| UA / Client Hints / 设备名（网络侧+高熵） | patch `GetUserAgent()`/`GetUserAgentMetadata()`（注释原文："JavaScript UA 显示 144.0.0.0，Client Hints 显示 144.0.6477.96"） | browser process | 014-embedder_support |
| hardwareConcurrency / deviceMemory / platform / language | Blink patch | Blink | 037-041 |
| Screen（分辨率/色深） | Blink patch | Blink | 043 |
| Canvas | `ApplyCanvasNoise`（LCG ±1，~1/8000 像素，seed 混入内容哈希） | Blink C++ | 045/049/050 |
| WebGL | `GetParameter` 拦截 RENDERER/VENDOR/UNMASKED + readPixels 噪声 | Blink C++ | 059 |
| WebGPU | Blink patch | Blink | 061/062 |
| Audio | `ApplyAudioNoise`（±2e-10，1% 样本） | Blink C++ | 058 |
| 字体 | API 层 + FontCache 双层白名单（白名单外返回 null）+ TextMetrics 噪声 | Blink C++ | 032/033/034/046/065 |
| WebRTC | ICE candidate 过滤（kReplace/kReal/kDisable） | Blink | 055 |
| mediaDevices / speech / geolocation | 模式 patch | Blink(+browser) | 053/057/044/005 |
| MAC 地址 | `GetNetworkListImpl` 内替换 | **net 层** | 022 |
| SSL/TLS 指纹 | `SSL_set_permute_extensions`（BoringSSL 扩展排列开关） | **net 层** | 024 |
| Accept-Language / 端口扫描保护 | net patch | net | 025/023 |
| 时区 | 仅存储未见消费 | [UNKNOWN] | — |
| 代理认证（含 SOCKS5 用户名密码） | 新增 `socks5_auth_cache` + net patch | net+browser | proxy 模块 010-013 |
| 账号自动导入 | eventbus → `PasswordImporter` | browser | account 模块 |

---

# Profile / Identity / Proxy Binding

**Profile ↔ Fingerprint**：指纹即 `environment_configs` JSON，创建时由前端生成一次、之后可整块覆盖修改；无 regenerate、无历史、无锁定。修改后下次启动生效；不存在「浏览器启动与 DB 显示不同的 fingerprint」的机制性风险（单一 JSON 源），但存在「身份被随意编辑而无任何记录」的模型性风险。

**Profile ↔ Proxy（Chain E）** [VERIFIED]：
- 数据模型：`proxies` 表（`dto\proxies.rs:8`：host/port/type/username/password/…）；`environments.proxy_uuid` FK `SET NULL`。**密码明文存储**（`infrastructure\proxy\types.rs:7` 注释原文「兼容旧结构；当前仅使用明文密码」；CSV 导出同样明文）。
- 启动链：`resolve_environment_proxy_config`（`launch_runtime\mod.rs:136`，本地 mihomo 绑定优先）→ `BrowserProxyConfigPayload`（fixed_servers + auth 明文 map，`kernel\types.rs:89`）→ EventBus LaunchConfig → kernel `ProxyConfigParser::ParseProxyConfig` 应用 + 运行时刷新（kProxySet）。
- **不走 `--proxy-server` 命令行**；`to_proxy_arg` 已定义但全仓无调用。
- mihomo 路径：attach 用户自备实例，写 http listeners（127.0.0.1:17601+）；无 TUN/system proxy。
- validation：显式 `test_proxy` / `test_direct_ip` / mihomo 测速；**无 launch 前强制代理 gate**。
- **fail 行为**：浏览器侧由闭源内核决定 [UNKNOWN]；桌面侧有明确 leak 面——时区/语言检测「代理失败则直连」（`kernel\timezone.rs:41-45` 原文：`if proxy_result.success {...} else { detect_with_direct_connection().await }`）[VERIFIED]。桌面侧 reqwest 用 `socks5h`（远端 DNS），浏览器 DNS/WebRTC 由内核处理、桌面不可证。

**Chain D（停止/崩溃/恢复）** [VERIFIED]：停止 = EventBus Disconnect + JobHandle drop（kill 整树）；崩溃由 CDP 轮询 + `browser.try_wait()` 与 eventbus 断连事件观察（`set_stopped_unless_error`）；应用重启 = 状态清零重来（无 DB 残留、无恢复需求，因 Job Object 已在宿主死亡时杀掉浏览器）。desired/persisted/observed 三态在代码里的形态：**desired 与 observed 都是内存态，persisted 永远 ready**——UI 显示 Running 是内存 map 说它 Running（其信号源含真实进程退出事件，但无主动存活探测）。

---

# Runtime Evidence Model

**Simprint：不存在。** [VERIFIED]

- `crates\runtime\src\infrastructure\diagnostics\mod.rs` 全部内容 = `LogLevel` 枚举 + 4 个日志包装函数 + 时间戳工具（审查方独立 grep 复核 browserleaks/whoer/creepjs/observed 零命中）。
- 无会话表、无启动历史表（43 张表清单核验）；唯一持久化运行相关数据是 `proxy_health_checks`（显式测试记录，非运行观察）。
- 日志仅有启动意图（"Browser process spawned for environment {} with pid {}"）；`wait_for_cdp_ready` 只验证调试端口，不评估指纹。
- `chrome://review`（review 模块）是内核内部调试 WebUI + 窗口数字徽章，README 明言"不应暴露给最终用户"。
- 因此 Simprint 无法回答：「这个 Profile 当前实际运行的是哪个 binary？实际用了哪个 proxy？网站最终看到了什么 fingerprint？」——至少无系统化机制。

---

# Local / Cloud Boundary

**判断：数据 fully local；分发 cloud-assisted（可选）。** [VERIFIED]

```text
Desktop(Tauri) ↔ 本地 SQLite(全部业务数据) ↔ 定制 Chromium(named pipe)
                ↔ GitHub Releases(应用更新 latest.json)
                ↔ Cloudflare R2(内核 zip, 固定 URL + catalog)
                ↔ realip.cc / ipapi.co / iprust.io / api.ip.sb(IP 检测)
                ↔ 用户自备 mihomo controller
```

- 无自有 hosted 后端；`deploy\` 纯构建脚本；auth 纯本地；无 telemetry [VERIFIED]。
- 断开官方基础设施后：产品能力基本完整（应用更新与内核新版本下载不可用，已有安装不受影响）。内核下载 URL 内嵌于 catalog JSON（`default-browser-kernels.json`），用户可扩展本地 catalog。
- 残留商业痕迹：billing 相关 UI/入口待清理（README 自述）[PROJECT_CLAIM]。

---

# Browser Package and Integrity Chain

Simprint 内核包链 [VERIFIED]：

```text
catalog(default-browser-kernels.json / 用户扩展)     ← 唯一内核: Chrome 144, 144.0.7559.118.5
→ R2 固定 URL 下载 (184,547,016 字节, 审查期实测可下载)
→ zip 全文件 SHA-256 校验 (downloader.rs:301)
→ 解压 → chrome.dll 前 10MiB SHA-256 == signature (types.rs:125)
→ 原子安装 (.staging-<uuid> → .backup-<uuid> → rename)
→ 安装记录 (browser_kernel_installations)
```

- kernel_id = 身份字段 canonical JSON 的 SHA-256（URL/显示名变化不改 identity；hash 变化→新 kernel_id）——**内容寻址设计，工程上扎实**。
- 缺口：① 无 OS 代码签名/证书校验；② **R2 二进制与开源 patch 管线之间的构建可重现性无任何证明**（无构建 CI、无 tag、无 provenance）[UNKNOWN]——即用户无法验证下载的二进制确实由开源补丁管线产出；③ 复用已装目录时仅校验 chrome.dll 头 10MB。
- 对照 VeriSilo（见下）：自建包 + CMS-SHA256 分离签名 + 证书 pin + 版本常量 pin，并把「digest+signature 双验证」作为 launch 硬前置。

---

# Automation and Product Layer

**这是 Simprint 明显领先的层** [VERIFIED]：

- local REST API：environments start/stop/batch/**cdp-endpoint**/list/detail/delete/urls/cookies/回收站、proxies batch、workspaces/groups/tags/browser_kernels 等；`cdp-endpoint` 返回 `browser_ws_url` 等（`services\environments.rs:276`）。
- MCP：52 个工具（start/stop/batch_start/cdp_endpoint/create/delete/proxies CRUD/cookies/urls/workspace…），loopback + local API key。
- CDP：每环境 `--remote-debugging-port`（29200-29499 端口池）+ `--remote-allow-origins=*`。
- Syncer：主控/从控多窗口键盘鼠标 IME 粘贴镜像（syner 模块，EventBus 重放）。
- 账号管理：环境绑定账号，启动时自动导入浏览器密码管理器（AES-256-GCM 解密由 Tauri 侧）。
- RPA 任务表（rpa_tasks/steps/runs schema 存在）；模板、分组、标签、团队、工作区、回收站、配额、审计日志表。
- 应用更新走 Tauri updater（GitHub latest.json）。

这些强化的是「工作台产品力」，对身份隔离模型本身无直接贡献——但对目标用户（自动化运营团队）是决定性产品价值。

---

# VeriSilo Architecture Verification

（以下全部 [VERIFIED]，源码级核验；文档概念均有代码落点。）

## 1. Silo 聚合根真实存在

`domain.rs:544-564 Silo`：`id, schema_version, name, color, browser(Option), execution_target, profile_directory, network_profile, engine, seed_reference, created_at, identity_locked_at, archived_at`。持久化为**单文件加密 Vault**（`vault.json`，envelope v2，Argon2id 派生 KEK 包裹 DEK，AES-256-GCM 双层，原子写 + 中断恢复，schema 1→9 全代原子迁移测试 `vault.rs:5166`）。跨生命周期实体按 ID 映射存于同一加密 payload：`seed_material`、`identity_artifacts`、`proxy_credentials/mihomo_controller_secrets`。Evidence 按 `silo_id` 归属（`network_evidence` inbox，上限 1000/100）。

## 2. Identity Artifact 完整存在

- `vault.rs:365-370 StoredIdentityArtifact {artifact_id, schema, raw_json, raw_sha256}`（Drop 全字段 zeroize）；绑定元组 `engine.rs:120-124 CamoufoxArtifactBindingV1 {artifact_id, artifact_file_sha256, schema}` 强校验。
- 生成链：`application\identity.rs:178 provision_managed_artifact` → `engine.rs:2726 ExternalPackageEngineAdapter::provision_camoufox_artifact`（spawn 引擎包 Host `--provision-artifact`，stdin 4KiB JSON（seed/preset/proxyServer/window/cores），响应 ≤8KiB，**回读后重算 sha256 不符即拒绝**）；preset 仅 4 个固定值、窗口/核心数受约束集限制；代理场景临时起 `ProxyRelay` 并 10s verify。
- 版本化：`verisilo-camoufox-resolved-identity/v3|v5|v6` 三代共存，v6=代理 Silo、v5=直连。
- **不可变 + 锁定**：无 mutate API；唯一替换入口 `replace_managed_identity`（`vault.rs:1035`）在 `identity_locked_at` 已设时拒绝——复核原文「托管身份已在首次启动后锁定，请创建新的 Silo。」；`materialize_identity_artifact` 注释明言从不重序列化、Vault 记录为权威、落盘 `.json + .json.sha256` sidecar 仅作可重建缓存。
- Engine 投影：launch 参数 verbatim 携带 `{artifactId, profileId, expectedArtifactFileSha256}`（`launcher.rs:3420-3424`）——**identity 期望值随 launch plan 下发并可对账**。

## 3. EngineAdapter / Managed Engine 调用链

`launcher.rs:1400 launch_with_identity_deriver` → `engine.rs:2334 production_engine_adapter_for_silo` → `engine.rs:3178 launch_plan`（注释原文："Camoufox identity comes from the Vault Artifact binding. It is deliberately not accepted as a template or sent over stdin"）→ `launcher.rs:3350 spawn_camoufox_host`（JSONL 三段 hello/launch/status 验证）。
引擎包：**代码内无下载 URL**，随安装分发（self-built）；版本 pin `CAMOUFOX_FORMAL_V3_ENGINE_VERSION="152.0.4-beta.28"` + revision + browser asset SHA256 + executable SHA256（`engine.rs:42-50` 复核）；签名校验 Windows detached CMS-SHA256 + **证书 pin**（`VERISILO_ENGINE_SIGNER_SHA256`）；launch 时 `digest_verified && signature_verified` 才置 `Verified`；fail-closed 原文："A controlled failure is terminal; it never falls back to stock."

## 4. Runtime Evidence 完整存在

- 网站可见身份观察：`website_identity.rs` 模块头原文（复核）："The Host writes `observed.json` during launch. That file is **observed evidence from the probe page, not a verified product Gate**."——解析 Host 落盘的 `observedFull`（UA/语言/时区/WebGL vendor+renderer/webdriver…），含 `silo_id` 归属。
- 九态状态机 `domain.rs:1388-1398 RuntimeEvidenceState`：`NotApplicable/NotRequested/Configured/Reachable/Applied/Observed/Verified/Failed/Unavailable`，各态均有代码落点（preflight→Reachable/Verified；路由应用→Applied；出口观察→Observed/Failed；包验签→Verified）。
- 归属与 stale 保护：`application\silos.rs:34 diagnose_silo_with` 注释原文 "`active_silo_id == None` alone never proves that the global state belongs to the requested Silo"；专属测试：`hydrating_identity_without_active_silo_drops_the_historical_observation`、`diagnosis_attributes_global_stopped_evidence_only_to_the_silo_that_ran`、`global_status_stops_presenting_a_historical_observation`；失效路径 `invalidate_network_evidence` 全态降级 `Unavailable`。
- 独立 30s 原生 watchdog（`runtime_watchdog.rs`）；扩展网络证据 inbox（`native_host.rs`，transport 限定 `companion_extension_fetch`，Vault 注释明言"证据而非 DNS/WebRTC/QUIC 全控证明"）。
- 次级证据通道：本地 SOCKS 回执 `RelayReceiptStore` + stale 修剪。

## 5. Network 绑定

`NetworkProfile`：`Direct / FixedProxy / Pac` + `external_mihomo` provider 绑定；凭据加密入 Vault（Drop zeroize），UI 只见 `credential_reference`。Fail-closed 证据：PAC+must-proxy 直接拒绝（原文："A PAC rules may return DIRECT"）；required proxy 禁 bypass；启动前 `preflight_proxy` 失败即 `LauncherError::ProxyPreflight` 终止；mihomo pin 失败置 Failed 并终止；DNS 经代理（`--host-resolver-rules=MAP * ~NOTFOUND`）+ 禁 QUIC + `--webrtc-ip-handling-policy=disable_non_proxied_udp`，并作为 safeguards 枚举记录。诊断归因 `mihomo.rs:284 diagnose_binding` 只查本 Silo 绑定的 controller/selector/node（原文"诊断不会探测本机其他 Clash"），选择漂移报 `SelectionNotApplied`。

## 6. CLI / 测试 / 阶段

CLI（loopback 薄客户端，口令不上命令行）：vault unlock/lock、status/identity/silos/clash/diagnose/start/stop/create/page 等。测试为真实后端 roundtrip + Windows-only 真实 Host 集成面 + 验收驱动。当前处于 pre-RC 稳定化（QA-R1/R2 合入、release 参数化）。

---

# Direct Architecture Comparison

| 维度 | Simprint | VeriSilo |
|---|---|---|
| 身份的本质 | 可编辑配置块（前端随机生成、自由 JSON、可整块覆盖、无历史） | 不可变锁定工件（seed→引擎派生、sha256、版本化、锁定后禁改） |
| 身份→引擎 | JSON 经 EventBus 下发，内核 C++ 消费 | Artifact binding（含期望 sha256）随 launch plan 下发并验签 |
| 引擎 | 自有 Chromium 144 patch 管线（130+ 补丁，闭源构建产物） | Camoufox(Firefox 152) 自建包 + CMS 签名 + 证书 pin |
| 代理 | FK + launch 附着；明文凭据；检测失败回退直连 | NetworkProfile 聚合成员；加密凭据；preflight fail-closed；DNS/WebRTC 收敛 |
| 运行事实 | 内存态状态 map；DB 恒 ready；无观察 | 九态证据机 + observed.json 网站观察 + 归属 + stale 防护 |
| 存储 | 明文 SQLite | Argon2id+AES-GCM 加密 Vault + zeroize |
| 产品面 | 完整工作台（CRUD/团队/API/MCP/CDP/syncer/RPA 表） | 引擎先行薄壳 + CLI（约 3.7:1 的 Rust:前端比） |
| 平台 | Windows（EventBus 命名管道；Tauri 跨平台壳） | Windows（专属测试与验收） |

**同一句话的两种实现**：「fingerprint」在 Simprint 是「一组会随编辑漂移的设置」；在 VeriSilo 是「首次启动即锁定、带期望哈希、可对账的身份工件」。「proxy」在 Simprint 是「启动时附着的网络配置」；在 VeriSilo 是「Silo 网络档案的持久成员，带 preflight、fail-closed 与诊断归因」。

---

# Simprint Clear Wins

（明确列出，不为 VeriSilo 辩护。）

- **SIMPRINT_CLEAR_WIN：产品成熟度**——完整 Profile CRUD/分组/标签/团队/工作区/回收站/模板/配额/审计表；VeriSilo 是薄壳。
- **SIMPRINT_CLEAR_WIN：自动化面**——local REST API（含 cdp-endpoint、batch start/stop）、52 个 MCP 工具、每环境 CDP、syncer 多窗输入镜像、账号自动导入、RPA 任务 schema；VeriSilo 仅 CLI + page action。
- **SIMPRINT_CLEAR_WIN：Chromium 引擎补丁面**——SSL 指纹（BoringSSL permute）、MAC 替换、端口扫描保护、Client Hints/JS UA 分版本伪装、SOCKS5 认证缓存等 **Camoufox（Firefox）无法或难以提供的面**；且 144 为当前主流大版本，站点兼容性好。
- **SIMPRINT_CLEAR_WIN：内核包管理工程**——内容寻址 kernel_id、catalog 可扩展、三层校验、原子安装/备份回滚、并行多版本；VeriSilo 是单包 pin（更严但更死）。
- **SIMPRINT_CLEAR_WIN：代理可用性**——健康检查/延迟/批量导入导出/CSV；VeriSilo 代理 CRUD 面窄。
- **SIMPRINT_CLEAR_WIN：扩展支持**——CRX 下载/安装/`--load-extension`；VeriSilo 未见同等机制。
- **SIMPRINT_CLEAR_WIN：发布与更新**——v0.2.31 逐版发布 + Tauri updater；VeriSilo 未 RC。
- **SIMPRINT_CLEAR_WIN：onboarding/文档/社区**——双语 README、贡献指南、TG/QQ 社区、演示 GIF；VeriSilo 面向 agent 协作的内部文档体系。

---

# VeriSilo Candidate Differentiators

# Differentiator Verification Matrix

| 候选差异 | 结论 | 依据 |
|---|---|---|
| Resolved Identity Artifact | **VERIFIED_DIFFERENTIATOR** | Simprint：自由 JSON、无版本/hash/锁定/历史（entitys\environments.rs:44 等）；VeriSilo：完整工件链（vault.rs:365, engine.rs:120, 锁定 vault.rs:1043 复核） |
| Identity integrity | **VERIFIED_DIFFERENTIATOR** | Simprint 无任何完整性机制；VeriSilo raw_sha256 + 落盘 sidecar + launch 期望哈希对账 |
| Engine binding | **PARTIAL_DIFFERENTIATOR** | Simprint 有内容寻址绑定表+RESTRICT+安装校验（真实机制）；VeriSilo 增加版本/资产/executable 三重 pin + CMS 签名 + 证书 pin + launch 硬前置。模型同类、保证强度不同 |
| Engine package trust | **PARTIAL_DIFFERENTIATOR** | Simprint 双层 SHA-256 但无签名、构建 provenance [UNKNOWN]；VeriSilo 自建+签名+证书 pin，但依赖发布纪律 |
| Runtime Evidence | **VERIFIED_DIFFERENTIATOR** | Simprint 零机制（diagnostics=日志包装，复核 grep）；VeriSilo 九态机+归属+stale 防护+watchdog |
| Website-visible identity observation | **VERIFIED_DIFFERENTIATOR** | Simprint 无（chrome://review 为内部调试页）；VeriSilo observed.json 探针页观察（代码自标 observed≠verified，诚实分级） |
| Network attribution | **VERIFIED_DIFFERENTIATOR** | Simprint 无归因（无会话/证据记录）；VeriSilo diagnose_binding 精确到 Silo 绑定的 node + SelectionNotApplied 漂移检测 |
| Local-first ownership | **NOT_A_DIFFERENTIATOR** | 双方均 fully local 数据 + 无 telemetry；Simprint 甚至无自有后端 |
| Fail-closed semantics | **VERIFIED_DIFFERENTIATOR** | Simprint：无 launch 前代理 gate、IP 检测失败回退直连（timezone.rs:41-45）、明文凭据；VeriSilo：preflight 终止、PAC 拒绝、bypass 禁止、DNS/QUIC/WebRTC 收敛 + safeguards 枚举 |

---

# Duplication Matrix

| 能力 | 重合度 0–4 | Simprint | VeriSilo | 关键差异 |
|---|---:|---|---|---|
| Profile persistence | 3 | SQLite+JSON+目录 | 加密 Vault+目录 | 同为持久目录+元数据存储；VeriSilo 加密、schema 迁移 |
| Profile isolation | 4 | per-env user-data-dir+Job Object | per-silo profile dir+watchdog | 实质等价的物理隔离 |
| Fingerprint generation | 2 | 前端逐字段随机 | seed→引擎派生 | 模型 A vs 模型 B（生成位置与确定性不同） |
| Fingerprint coherence | 2 | 两处成组约束（硬件档/UA↔OS），GPU↔OS、字体↔OS 失联 | preset+seed 派生 + 约束集 | 双方均不完整；Simprint 可随机出不自洽组合 |
| Fingerprint persistence | 1 | 自由可变 JSON | 不可变锁定工件 | 可编辑设置 vs 身份工件 |
| Engine-level spoofing | 3 | 自有 Chromium 补丁（含 SSL/MAC 独有面） | Camoufox(Firefox) 引擎级 | 两侧均为深层引擎伪装，面与深度各有胜负 |
| Identity artifact | 0 | 无 | 完整（版本+哈希+锁定） | Simprint 不存在该机制 |
| Engine binding | 2 | 内容寻址 kernel_id+RESTRICT | 三重 pin+签名+launch 验证 | 同类机制、保证强度不同 |
| Proxy binding | 2 | FK+launch 附着+mihomo | NetworkProfile 聚合成员+fail-closed | 附加配置 vs 身份成员 |
| Runtime lifecycle | 3 | Job Object+内存态+CDP 轮询 | watchdog+生命周期+证据态 | 覆盖同级；VeriSilo 多证据语义 |
| Runtime evidence | 0 | 无 | 九态+观察+归属+stale | Simprint 不存在该机制 |
| Website observation | 0 | 无 | observed.json 探针观察 | Simprint 不存在该机制 |
| Package integrity | 2 | 双层 SHA-256（无签名/provenance） | SHA+CMS+证书 pin+自建 | 同类校验、信任链强度不同 |
| Local-first | 4 | fully local+无遥测 | fully local+加密 Vault | 实质等价（数据主权） |
| Automation | 2 | REST API+MCP52+CDP+syncer | CLI+page action | 概念相似、深度悬殊（Simprint 胜） |
| Product UX | 3 | 成熟工作台 | 薄壳+CLI | 概念高度重合、成熟度悬殊（Simprint 胜） |

---

# User-Value Analysis

逐项问「删掉它，用户实际失去什么」：

- **Identity Artifact（不可变+锁定+哈希）**：删掉后失去——长期身份的防漂移（账号环境不被随手编辑毁掉）、封号归因时可回答「该账号整个生命周期运行在什么身份下」、工件可迁移/可审计。这不是内部洁癖：多账号运营的真实痛点正是「环境参数被改过但没人知道改了什么」。**有实际用户价值**（面向高价值长寿命账号的运营者），但对「随建随弃」的短命环境用户价值有限。
- **Runtime Evidence + 网站观察**：删掉后失去——启动后自动确认「代理真的生效了/指纹真的呈现了」，替代人工开 ip 检测站；封号排查从猜测变成读记录。Simprint 用户今天必须手动做这些。**有实际用户价值**，前提是把它做成低摩擦的产品体验（CLI 诊断已是好起点）。
- **Engine Binding 强 pin**：删掉后失去——引擎升级静默改变身份的风险防护。注意 Simprint 的内容寻址 catalog 实际上已防止「自动升级漂移」（不自动升级、hash 变化即新 kernel_id）——普通版本锁在此场景基本够用。**VeriSilo 的增量价值在签名+证书 pin+launch 验签的供应链防御**，对受控/审计场景有价值，对普通用户价值有限。
- **Fail-closed 网络**：删掉后失去——「代理挂了→直连→家宽 IP 暴露→全部账号关联」这一灾难性故障模式的系统性防护。Simprint 源码可证存在直连回退路径（检测层）且无 preflight gate。**这是本域最高价值的差异化**——一次泄漏的代价可能超过一切产品功能。
- **加密 Vault/zeroize**：本地多账号凭据集中存储场景下有真实价值（Simprint 明文 SQLite 存代理与账号密码是其实质短板）。

**反向警示（错误推理防御）**：不要因为上述差异化就得出「VeriSilo 全面更先进」——Simprint 的 Chromium 补丁深度、自动化产品面、内核管理工程都**强于** VeriSilo 对应面；也不要因为 Simprint 功能多就判 VeriSilo 无价值——功能数量不替代证据链与 fail-closed 语义。

---

# What VeriSilo Should Stop Building

1. **停止追逐桌面工作台完整度**：分组/标签/团队/工作区/回收站/模板/批量管理 UI 的产品面竞争——Simprint 已完成且开源（AGPL），重做无价值。
2. **停止扩张自动化广度**：不建 REST API 全资源面/MCP 大工具集/通用 RPA——Simprint 已覆盖；VeriSilo 自动化应只服务于「验证与诊断」（CLI/status/diagnose/page-action 已是正确形态）。
3. **停止通用代理 CRUD/健康检查产品化**：保留 NetworkProfile+诊断，不做代理资产管理工作台。
4. **不做 syncer/RPA/账号导入等多环境运营便利功能**。
5. **停止在文档层与 Simprint 做功能表对比**（本报告即替代）。

# What VeriSilo Should Keep

1. **身份工件链**（provision→sha256→锁定→launch 期望哈希对账）——核心差异化，继续收紧（含 v3 旧代清退）。
2. **Runtime Evidence 全链**（九态、observed.json 网站观察、silo 归属、stale 防护、watchdog）——并把「observed→verified」的差距作为路线图（如探针页结果与 artifact 期望值的自动对账告警：这正是 Simprint 完全没有的护城河）。
3. **Fail-closed 网络与诊断归因**（preflight、PAC 拒绝、DNS/QUIC/WebRTC 收敛、SelectionNotApplied 漂移检测）。
4. **加密 Vault + 凭据 zeroize**。
5. **引擎包信任链**（自建+签名+证书 pin）与 EngineAdapter 契约。
6. **CLI 验证面与真实后端 roundtrip 测试**。
7. **pre-RC QA 收敛**（当前阶段正确，别被本审查打断）。

**VeriSilo 真正应该成为的产品**：「可验证身份运行时」——面向把账号当资产的运营者/团队的本地-first、fail-closed、带运行证据与审计能力的身份执行层；产品叙事从「另一个指纹浏览器」改为「唯一能证明身份真的按声明运行的指纹浏览器」。

---

# Counterfactual: Would We Start VeriSilo Today?

> 如果 VeriSilo 今天一行代码都没有，已知 Simprint 的存在与实现，是否还从零开始？

**ONLY_IF** ——只有当以下条件全部成立时才值得：

1. 目标用户明确需要「身份工件 + 运行证据 + fail-closed」这三件 Simprint 结构上没有的事（如高价值长寿命账号、审计/合规场景、防关联是硬需求而非偏好）；
2. 接受引擎层不自研 Chromium 补丁（直接采用 Camoufox 或未来其他 managed engine），即承认 SSL 指纹/MAC 等补丁面是 Simprint 的地盘；
3. 接受在产品成熟度上长期落后于 Simprint，靠「验证与信任」而非「功能」立足。

否则 **NO**——普通多环境工作台需求直接用 Simprint（或 fork 它）更理性。

---

# 3–6 Month Investment Judgment

> 忽略沉没成本，未来 3–6 个月继续投入是否有技术/产品合理性？

**YES（有条件的 yes）**——条件即执行 B 收缩：

- 已写代码不是理由；理由是：**证据链、身份完整性、fail-closed 是 Simprint 源码级验证不存在的能力**，且这些能力对该域目标用户有真实价值（防关联灾难、封号归因、审计）。46,962 行 Rust 中与差异化相关的部分（vault/engine/launcher/mihomo/website_identity/native_host）是有效资产，不是沉没成本意义上的错误方向。
- 3–6 个月应投向：① 完成 pre-RC 稳定化与发布；② 把 evidence 做成用户可见的低摩擦体验（诊断 CLI→UI 摘要、observed vs expected 自动对账）；③ 收缩产品面到差异化解所需的最小集；④ 明确不做的清单（见上节）。
- 若团队实际想做的仍是「功能更全的指纹浏览器工作台」——则 **NO**，应立即停并用 Simprint。

---

# Final Verdict: A / B / C

## **B — NARROW / DIFFERENTIATE**

- 保留：身份工件链、运行证据与网站观察、fail-closed 网络与诊断归因、加密 Vault、引擎包信任链、CLI 验证面。
- 删除/停止：桌面工作台产品面竞赛、自动化广度、通用代理管理产品化、一切与 Simprint 重复的"多环境管理"叙事。
- 不再与 Simprint 竞争：产品完整度、Chromium 补丁面广度、自动化生态、onboarding。
- 成为什么：**可验证身份运行时（verifiable, fail-closed, evidence-backed identity runtime）**。

依据链：Simprint 源码证明其在产品层与引擎补丁层的高覆盖（重复度高），同时证明其在身份工件/运行证据/fail-closed 三层的结构性缺失（差异化真实）；VeriSilo 源码证明这三层不是文档概念而是 47K 行里的可运行实现。A 不成立（覆盖不完整），C 不成立（重复度远超"表面类似"）。

---

# Unknowns and Evidence Gaps

1. **Simprint R2 内核二进制的构建来源**：开源 patch 管线与 184MB 下载产物之间无 CI/tag/provenance 证明 [UNKNOWN]——若该链断裂，「内核可信」只剩 hash 传输完整性。
2. **Simprint 闭源内核运行行为**：浏览器流量在代理失效时 fail-closed 还是 direct fallback、内核内 DNS 处理、时区伪装是否实际生效（有存储无消费补丁）[UNKNOWN]。
3. **Simprint 噪声 seed 未绑定 env id 的实际影响面**：源码可证（复核 verbatim），但运行期跨 profile 关联性未实测。
4. **VeriSilo observed.json 是 observed 而非 verified**（代码自我声明）——「verified」级证据尚无 Gate；这是 VeriSilo 自己的诚实缺口与路线图空间。
5. **双方均未做运行时验证**：本审查未启动任何浏览器/产品，所有 runtime 语义结论限于源码可证范围。
6. VeriSilo 引擎为 Camoufox/Firefox 152 beta：与 Chromium 站点生态的兼容性/风控特征差异未评估（超出本次范围）。

---

# Appendix: Important Source Paths and Call Chains

## Simprint（repo `simprint` @ d2dc939b，除注明 kernel 外均在此仓库）

**Chain A — Create Profile**
```text
plugins\pages\create-window\src\components\create-window-content.tsx:222 (一键随机)
→ utils\fingerprint-generator.ts:389 generateRandomFingerprintSettings()  [Math.random 逐字段]
→ utils\user-agent-generator.ts:570 generateUserAgentByKernel()
→ src\lib\request.ts:45 post('environments/create')
→ src-tauri\src\commands\network.rs:19 http_post
→ crates\business\src\services\environments.rs:41 create_environment_service
   ├→ browser_kernels.rs:443 resolve_requested_kernel
   ├→ models\environments.rs:255 insert_environment
   ├→ upsert_environment_config (指纹 JSON 持久化)
   └→ browser_kernels.rs:401 bind_environment_kernel (environment_kernel_bindings)
```

**Chain B — Start Profile**
```text
src-tauri\src\commands\environment.rs:123 start_environment_by_uuid
→ launch_runtime\mod.rs:100 build_launch_request
   ├→ detail.rs:8 get_environment_launch_detail
   ├→ launch_runtime\kernel.rs:17 resolve_kernel_launch → browser_kernels.rs:429
   ├→ launch_runtime\fingerprint.rs:7 build_fingerprint_config (JSON→FingerprintConfig)
   └→ mod.rs:144 resolve_environment_proxy_config (mihomo 绑定优先)
→ runtime_bridge.rs:358 prepare_start_request (exe 存在性检查/user-data-dir/ip 检测)
→ app\runtime.rs:50 send_environment_command(StartEnvironment)
→ crates\runtime\...\launcher.rs:32 launch_browser → :356 spawn_browser_process
   switches: --simprint-env-id / --user-data-dir / --remote-debugging-port /
            --remote-allow-origins=* / --disable-skia-graphite / --load-extension ...
   [无 --proxy-server / --user-agent：指纹与代理经 EventBus]
```

**Chain C — Fingerprint Apply**
```text
environment_configs.fingerprint_settings (自由 JSON)
→ crates\runtime\src\infrastructure\eventbus\manager.rs:257-298
   Topic::LaunchConfig(0x0900) + Topic::FingerprintApply(0x0200) → 命名管道 \\.\pipe\simprint_{env_id}
→ [kernel] 030-simprint-eventbus.patch:87 → FingerprintConfigManager::ApplyConfigFromJson (fail-soft)
→ [kernel] 016-content-render-process-host-impl.patch:19 (ExportAllConfigAsJson → mojom 新参数)
→ [kernel] 019-content-render-thread-impl-cc.patch (ImportAllConfigFromJson, renderer)
→ Canvas/WebGL/Audio: fingerprint_browser_fingerprint.cc:221/442/294 (ApplyCanvasNoise/ApplyWebGLImageNoise/ApplyAudioNoise)
   seed: fingerprint_fingerprint_seed.cc:11-30 派生自 profile_id，
   但唯一回退调用 SetCurrentProfileId("default") (browser_fingerprint.cc:579-602) [未绑定真实 env id]
```

**Chain D — Stop / Crash / Recover**
```text
running browser → CDP try_wait/断连事件 (app\runtime.rs:197-243 set_stopped_unless_error)
→ launcher.rs:634 stop_environment (EventBus Disconnect + job_manager.remove)
→ JobHandle (kernel\job.rs:27-64, KILL_ON_JOB_CLOSE) → 宿主死亡→OS 杀整树
→ 应用重启: 内存态清零，DB 恒 'ready'，无恢复/stale 清理（update_environment_status 为死代码）
```

**Chain E — Proxy**
```text
proxies 表 (dto\proxies.rs:8; 密码明文 types.rs:7)
→ launch_runtime\mod.rs:136 resolve_environment_proxy_config
→ runtime_bridge.rs:457 → BrowserProxyConfigPayload (fixed_servers + auth)
→ EventBus LaunchConfig → [kernel] proxy_config_parser.h:57 ParseProxyConfig → net/browser 层应用
   失败路径: kernel\timezone.rs:41-45 代理失败→直连检测 [leak 面]
   mihomo: services\mihomo\mod.rs:26 listeners 127.0.0.1:17601+ (attach 用户实例)
```

## Simprint kernel（repo `simprint-browser-kernel` branch `simprint/m144` @ 9f7b8e0）

- `overlay\fingerprint\patches\`（68 个，apply_order.txt）+ `sources\simprint\fingerprint_*.cc`（16 文件）
- `overlay\{auth,account,proxy,syner,ntp,branding,review,cookie}\`：启动认证/账号导入/代理应用/输入同步/新标签页+IP/品牌/内部调试页/占位
- `core\include\simprint\kernel_api\v1\{version.h, capability.h}`：迁移目标 API（无消费者）
- `port\manifest.toml`：9 capability 全部 legacy-patch；`driver\cli.py`：apply/deploy/validate/plan
- `.github\workflows\port-contract.yml`：仅契约校验，无构建/签名/tag

## VeriSilo（repo `VeriSilo` branch `baseline/dev` @ f6ae647，均在 `apps\desktop\src-tauri\src\`）

```text
Silo 聚合: domain.rs:544 | Vault: vault.rs:77-103 (envelope v2, schema 9, Argon2id+AES-GCM)
Artifact: vault.rs:365 StoredIdentityArtifact | engine.rs:120 BindingV1 | 锁定: vault.rs:1035-1047
生成: application\identity.rs:178 → engine.rs:2726 provision (spawn Host, sha256 回读对账)
启动: launcher.rs:1400 → engine.rs:3178 launch_plan → launcher.rs:3350 spawn_camoufox_host
      launch 参数含 expectedArtifactFileSha256 (launcher.rs:3420-3424)
引擎信任: engine.rs:42-50 (152.0.4-beta.28 pin) / 1887,1954 (CMS+证书 pin)
证据: website_identity.rs (observed.json, silo_id 归属) | domain.rs:1388 九态
      归属/stale: application\silos.rs:34 | 失效: launcher.rs:3150 | watchdog: runtime_watchdog.rs
网络: domain.rs:442-458 (PAC 拒绝/禁 bypass) | launcher.rs:4240 preflight | mihomo.rs:284 diagnose_binding
      domain.rs:525-534 (DNS via proxy/禁 QUIC/WebRTC policy) | proxy_relay.rs (回执+stale 修剪)
CLI: bin\verisilo-cli.rs (vault/status/identity/diagnose/start/stop/create/page)
```

---

*本报告所有重要结论均可由上述 repo/commit/file/symbol 复现。审查全程只读；两个产品均未被执行，运行时行为以源码可证范围为限。*
