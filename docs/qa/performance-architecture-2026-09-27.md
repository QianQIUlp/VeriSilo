# 性能与架构原任务处置 — 2026-09-27

目标：以实际使用效果完成原会话中已发现的性能、架构与上层逻辑问题。范围由原始发现限定，不因验证工具故障扩成安装器或 Sandbox 工程。

本轮起点为 canonical baseline `9d7c238c4a0442333af07eab5454311495bfe3f7`。源码修复组合为 `a1c735f925b928cea7270b606758d9c43aa444b7`：core `b2d664e`、UI `e760f79`、Clash 契约 `8035910`。本文记录已有成果和本轮真实覆盖，测试通过不代表未执行的用户旅程。

## 原始发现的完整处置

| 原问题 | 最终实现或保留理由 | 证据与边界 |
| --- | --- | --- |
| 创建接近一分钟、启动严重卡顿 | 已有重命令转后台、创建身份时释放 Vault；本轮继续解除 status/diagnostic 等待 Runtime 时对 Vault 的占用 | 历史 warm provision 约 2.5 秒、spawn 约 5 秒无法证明原冷启动原因。Defender 归因没有直接测量，不改杀毒设置。当前开发运行结果见下文 |
| 启动后一会儿消失 | 已有修复 `83677e9` 将打包探针绑定到正确源码并拒绝旧探针 | 原 rc4 的 `document.fonts.check` 假阳性触发 fail-closed 关闭；原 native canvas 实验 32/32 负对照都被正确屏蔽。此结论解释该次故障，不泛化为全部闪退原因 |
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

**未完成：真实启动失败仍在诊断，不能据此宣称原体验问题已解决。** 使用本轮 debug CLI 与真实 Rust 后台、独立命名 Vault、Direct 网络。现有 `v0.1.0-rc2026092404/engine-package` 的 10 个 Host source digests 已与当前源码逐项匹配，复制进独立开发资源目录；runtime 保留正式签名和完整性校验。没有重做安装包或 Sandbox。

- 创建耗时依次为 21.549 秒、1.070 秒、1.259 秒，均成功。这不是严格冷/热受控实验，不据此声称已解释历史一分钟延迟。
- 第一次启动等待期间，一次 Silo 列表读取在 52 毫秒内完成，证明该次实际启动等待没有阻塞普通 Vault 读取。所谓创建期间的列表读取实际已晚于创建完成，不能计入并发证据。
- 三次启动都未返回可用 Managed 会话。CLI 在约 120 秒先报连接超时，后台随后将状态置为 `verification_failed` 并清理进程。后两次用于增加诊断信息，未改变引擎或安全策略；第二次 stderr 被丢弃，没有取得预期协议日志。
- 第三次使用包内 `DEBUG_FILE` 取得直接日志：04:18:22 UTC Juggler pipe 就绪并收发请求；04:18:52 GPU 子进程三次启动失败、两条 `gBrowser never populated`；04:18:53 `Browser.enable` 返回，但没有页面 target。Playwright 继续等待首个 Page，Host 停在 `launch_persistent_context`，尚未运行身份探针。
- 对实际包源码的检查表明，TargetRegistry 的 `gBrowser` 等待超时会使该窗口注册失败。GPU 失败与主窗口注册之间的具体因果尚未证实；不能改写为探针失败、Job 重构回归或历史闪退的统一解释。本轮 launch Job 是既有逻辑搬移，未增加 launch 的 Job 层数或限制。Windows 实际服务身份已核对为 `TELECASTER\\qiu`。

本地直接 evidence 位于本 worktree 的 `artifacts/performance-goal/`（忽略目录）；`handshake-driver.log`、`handshake-final-status.json` 保留失败结果。尚缺成功启动、存活、页面响应及停止的产品证据。验证也不覆盖用户特定网站、Clash 节点、长期运行或安装行为。
