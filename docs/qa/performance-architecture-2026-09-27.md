# 性能与架构原任务处置 — 2026-09-27

目标：以实际使用效果完成原会话中已发现的性能、架构与上层逻辑问题。范围由原始发现限定，不因验证工具故障扩成安装器或 Sandbox 工程。

本轮起点为 canonical baseline `9d7c238c4a0442333af07eab5454311495bfe3f7`。最终产品源码组合为 `e89c0cc437293bad646bbe51e874c3936650d193`：core `b2d664e`、UI `e760f79`、Clash 契约 `8035910`，以及 CLI 超时提示、Host 阶段错误和测试修正。原发现均已修复或给出有依据的保留理由；真实运行覆盖及剩余边界如下。

## 原始发现的完整处置

| 原问题 | 最终实现或保留理由 | 证据与边界 |
| --- | --- | --- |
| 创建接近一分钟、启动严重卡顿 | 已有重命令转后台、创建身份时释放 Vault；本轮继续解除 status/diagnostic 等待 Runtime 时对 Vault 的占用 | 真实创建 2.952/1.136 秒，创建期间列表读取 25.9 毫秒；启动 25.171/13.920 秒。不是历史冷启动的受控前后对照，未宣称已解释全部一分钟延迟或达到即时启动；Defender 归因没有直接测量 |
| 启动后一会儿消失 | 已有修复 `83677e9` 将打包探针绑定到正确源码并拒绝旧探针 | 原 rc4 的 `document.fonts.check` 假阳性触发 fail-closed 关闭；原 native canvas 实验 32/32 负对照被正确屏蔽。本轮两个完整 Managed 启动均通过，首次 122 秒后仍运行且身份匹配；不泛化为长期或任意环境不闪退 |
| 重型 Tauri 命令阻塞 UI | 已有 `db7fdc4`、`be06755`、`f491bc4` 将阻塞工作放入 worker | 本轮检查 commands；纯参数校验保持同步 |
| 身份创建持 Vault 锁运行外部进程 | 已有 snapshot → provision → revalidate/commit | 本轮核对 `application/identity.rs`；未再增加新后台任务框架 |
| status/诊断把健康探测等待传播到 Vault | 本轮分阶段取得短快照，健康探测期间不持 Vault；生命周期 reservation 保留 | 行为测试阻塞 Runtime/health，同时完成 Vault 列表读取；过期后 revoke、磁盘快照 Locked、身份清除、旧 Silo 证据 stale 均覆盖 |
| Profile 大小 N+1、统计持 Vault 锁 | 已有批量命令和路径快照 | 文件遍历位于 Vault guard 之外 |
| Profile 统计无界递归 | 本轮迭代遍历；一次请求共享 100,000 项、2 秒预算；超限整体报不可用 | 不返回部分总量；跳过已消失项，不跟随 symlink/junction。2 秒是文件调用之间的协作式预算，不能中断已阻塞的 OS 调用 |
| Vault 整库克隆/加密/写盘 | 重复网络证据的无效 clone/write 已由 `9d7c238` 消除；保留原子加密快照 | 原 debug 约 1.26 MB 测量中的 clone 约 1.4 ms、JSON 约 36 ms、磁盘约 5 ms，不支持直接迁移存储格式。整库写入成本仍随数据量增长，未宣称消失 |
| 每 2 秒全量轮询、隐藏窗口轮询 | 已有 stock-active status 窄轮询、隐藏跳过、相等对象复用 | 保留现有实现，没有新增轮询层 |
| 全部 Silo 详情即使折叠仍挂载 | 本轮只挂载选中 Silo 的当前详情 | UI Preview：折叠 0、展开 1、切换仍 1；焦点恢复保持 |
| 异步旧结果覆盖新状态 | 主 workspace/Clash 已有 guard；本轮补引擎状态请求编号和 Vault 会话 guard | 延迟 Mock 响应验证旧成功、旧失败、busy 和锁定跨会话行为。此证据属于前端，不能代替真实 runtime |
| 失效环境 UI、大文件职责 | 已有 `50fddb6` 删除不可达 Remote UI；未按文件长度机械拆分 | 文件大不独立构成产品缺陷；本轮处理了具体锁竞争和生命周期问题，未引入通用状态管理框架 |
| 跨层 preset/timezone/GPU/CPU 漂移 | 复用既有 `check_managed_contracts.py` | 19 presets、22 timezones、10 GPU profiles、cores 2/4/6/8/12/16；没有新增代码生成系统 |
| 用错误文案驱动控制逻辑 | Managed/local API 已有明确 code；本轮补 Clash `{code,message}` | 仅不可达/传输/mixed-port 三种 code 允许重新发现；认证、协议、未知错误不换目标；前后端针对性测试覆盖 |
| Host 重复身份观察逻辑 | 已有共享 `_observe_website_identity` | launch/reobserve 复用同一路径 |
| Host 重复字体枚举、日志增长和依赖声明 | 已有 `5bd392d`：字体集合按 Host 进程缓存，启动时对超过 8 MiB 的日志轮转，更新依赖声明并清理失效逻辑 | 核对当前 owning code，复用本轮 Host 检查；日志阈值按启动检查，不宣称运行中绝不超过阈值 |
| Windows mklink 解码错误 | 已有字节输出处理与测试替换解码 | 继承现有修复，未重复实现 |
| 身份 provision Host 生命周期遗漏 | 本轮复用 launch 的 kill-on-close Job guard，覆盖 provision 整个作用域 | Windows 子进程测试和 launch/stop 组合测试通过；单测不冒充桌面硬退出后的真实后代树验收 |
| 包树重复 hash/复制 | 已有同次 digest、post-smoke digest 复用、最终 hardlink | 可变输入和签名前后的完整性边界仍需验证，不能简单删掉所有重复看似相同的检查 |
| 串行验证、重复 dev-desktop | 已有按工具链并行、移除重复运行 | 本批按项目规定验证；没有创建新的永久验收驱动 |

## 本轮验证

- Core 在 `b2d664e`：lane verify/check 通过，20 项 application 测试通过；Profile 三项测试、Job ownership 测试、launch/stop 组合测试通过。
- UI 在 `e760f79`：lane verify/check 通过，123 项测试；Preview 与真实 workspace hook + 延迟 Mock API 检查通过。
- Clash focused 检查：22 项前端测试及 TypeScript 通过；其中 13 项直接覆盖重发现分支选择。
- Integration 完整矩阵：合约、前端 check/test、Host 检查及任务脚本检查通过。两个 Rust suite 首轮均暴露同一个旧测试问题：把 Python Host 的时间戳与 Rust `Utc::now()` 作零容差比较；改为验证新响应专属信号及会话绑定后，desktop 268 项、harness 263 项全部通过（各 5 ignored、3 filtered）。其他未变输入的已通过结果复用。升级完整矩阵的原因是新增 Rust/Tauri/前端共享错误契约。
- 真实诊断另外发现 CLI 的 120 秒等待可先于后台结束。补充超时提示，明确后台可能继续、需查询同一 Vault；Host 超时错误带命令名。2 项 CLI 分类测试与既有迟到响应隔离测试通过，未调长超时或自动重试。

## 原生 Windows Managed 开发验证

**本轮有限生命周期验收通过。** 使用当前源码的 debug CLI 与真实 Rust 后台、独立 Vault `perf-native-37abe4`、Direct 网络。现有 `v0.1.0-rc2026092404/engine-package` 的 10 个 Host source digests 与源码逐项匹配。包复制至独立 LocalAppData 开发目录，应用仍按正常引擎注册与 Artifact 绑定路径运行，没有修改包内容、默认 Vault 或安全设置，没有构建安装包或运行 Sandbox。

| 行为 | 实际结果 | 本地直接证据（`artifacts/performance-goal/`） |
| --- | --- | --- |
| 创建 Managed Silo | 首个 2.952 秒；活动会话期间另建 1.136 秒，均成功 | `localappdata-create-timing.json`、`localappdata-concurrent-timing.json` |
| 创建期间普通读取 | 列表 25.9 毫秒返回；读取前后创建进程都尚未退出 | `localappdata-during-create.json`、`localappdata-concurrent-timing.json` |
| 第一次完整启动 | 25.171 秒；`running`、identity `matched`、Host `observed` | `localappdata-start.json`、`localappdata-start-timing.json` |
| 页面及有限存活 | 页面 `readyState=complete`，Google 正常响应；启动返回后 122 秒仍运行且身份匹配；普通列表约 70 毫秒 | `localappdata-page-first.json`、`localappdata-survival.json`、`localappdata-silos-timing.json` |
| 活动会话重新观察 | 1.873 秒成功；页面 sentinel、URL 均保持 | `localappdata-recheck.json`、`localappdata-page-after-recheck.json` |
| 停止后再次启动 | 第一次停止 1.003 秒；第二次启动 13.920 秒，仍运行、身份匹配、页面可响应 | `localappdata-stop-first-timing.json`、`localappdata-start-second.json`、`localappdata-page-second.json` |
| 最终清理 | 第二次停止后 `stopped`、activeSilo=null；Vault locked，服务退出，所属进程剩余 0 | `localappdata-stopped-status.json`、`localappdata-cleanup.json` |

上述是源码开发实例的真实运行证据。返回值中 `packageVerification=not_requested`、`verifiedAdapter=null`，不把 `observed`/`matched` 改称 formal engine verification，也不据此打开新的产品或 release Gate。这些测量没有严格历史前后对照，不覆盖用户特定网站、Clash 节点、长期运行、所有路径或安装行为。

## 开发目录失败及验证边界

工作树内的同包副本此前启动失败：Juggler pipe 正常收发；GPU 子进程三次启动失败，TargetRegistry 报 `gBrowser never populated`；Playwright 等不到首个 Page，Host 停在 `launch_persistent_context`，尚未运行身份探针。CLI 先于后台超时的问题因此被发现并修正。该失败期间的一次 Vault 列表读取仍在 52 毫秒完成。

最小原生对照在绕过产品 Host/Job、移除身份配置与产品偏好后仍失败；独立于命令执行器外层 Job 的对照也失败。但相同 driver 使用已安装目录的浏览器约 8.795 秒即成功创建页面。两处 browser 的 504 个文件逐 SHA-256 完全相同。复制当前 debug 应用和同包至独立 LocalAppData 目录、用新 Vault 注册后，完整产品路径取得上表结果。

因此，本次保留的是**与运行目录相关的未解释限制**。尚无足够证据指定 ACL、Defender、路径长度、某个 DLL 或 OS 版本为根因；不据此削弱浏览器沙箱或 Job，不把整台宿主机断言为不可运行。静态 PE 检查未发现常规/延迟导入缺失或非 API-set 导出缺失；它不能排除动态装载问题。本轮 launch Job 只是既有逻辑搬移，没有增加层数或限制。

**后续状态（2026-09-27）：**[目录 ACL 因果对照](managed-engine-directory-acl-2026-09-27.md)已把上述“未解释限制”收敛为当前 Codex sandbox/worktree 的继承 DACL 开发限制；同一路径、同一 owner、相同 504 个文件仅改 DACL 后，建页由 60 秒失败变为 5.344 秒成功。仍未识别具体 ACE、Windows API 错误或更深层机制。正常用户上下文的独立 LocalAppData 产品路径通过，本问题不作为当前产品 blocker 或后续默认调查任务。

一次迁移后的启动仍引用旧 Vault 注册的工作树 package 路径，不能计为新目录失败。后续新 Vault 的 driver 日志明确记录实际新路径；在本机 Codex 的 MSIX 环境中，它解析到 `Packages/OpenAI.Codex_2p2nqsd0c76g0/LocalCache/Local/VeriSiloDev/performance-37abe4`。验证迁移应先核对真实引擎绑定，不能仅凭桌面 exe 所在位置判断包路径。

失败原始日志及结果保留在本 worktree 的忽略目录 `artifacts/performance-goal/`；成功证据见上表。没有把这些限制伪装成通用环境修复，也没有为本次局部目录问题新增永久诊断框架。
