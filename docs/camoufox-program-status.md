# Camoufox Managed Engine 当前状态

- 状态：**可变项目状态页**
- 更新日期：2026-08-10

本文只记录当前执行阶段、证据 checkpoint 和下一项任务。长期产品意图见[身份平台北极星](identity-platform-north-star.md)，路线原因见[Camoufox-first Managed Engine 决策](camoufox-managed-engine-decision.md)。每次 Gate 变化后更新本文，不用本文反向改写长期决策。

## Git 状态

| 对象                        | 当前值                                                                                                      | 含义                                                                       |
| --------------------------- | ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `origin/main` 基线          | `8de389db366d1d9ff510b1e885fab7f49a89aad0`                                                                  | PR #10 已合并；包含 M0–M2-W standalone Host、Artifact 与 accepted evidence |
| Linux accepted checkpoint   | `d596afd76e59ba64915b036fbc732a2c28f1ec54`                                                                  | 保持 accepted，不因 M3 失败改写                                            |
| Windows accepted checkpoint | `1bf0854e4fac7142baef9792967851593b804912`                                                                  | M2-W standalone Windows Gate accepted                                      |
| M3 研究分支                 | `codex/camoufox-m3-engine-adapter` / `186484feb935076766beab09595a9270f86f78ef`                             | 本地未 push；保留完整 M3-0 与失败的 M3-WI 研究历史                         |
| M3-0 accepted checkpoint    | `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`                                                                  | fake Host / EngineAdapter contract Gate 已关闭                             |
| M3-WI 终局 checkpoint       | `186484feb935076766beab09595a9270f86f78ef`                                                                  | R2H 第三项 persistence 失败；没有 Accepted manifest                        |
| Standard 产品分支           | `codex/standard-silo-windows-preview` / `aa72eadaf8300d1cd33a2c32173c06e3e677ca89`                         | Profile Isolation Windows local Preview passed；unsigned，未 push          |
| 已合并 PR                   | [#12](https://github.com/QianQIUlp/VeriSilo/pull/12) / [#10](https://github.com/QianQIUlp/VeriSilo/pull/10) | M2-W 证据与 Camoufox standalone 已进入 `main`                              |

`d596afd` 与 `1bf0854` 是已接受的 standalone checkpoint。M3 研究分支没有 push，
不属于 `origin/main` 或 shipped 产品；其中的真实 Windows run 只能按各自 Gate 结论解释。

## 已关闭阶段

### M0 / M0.1：Linux 兼容性与资产固定

- 固定 Camoufox、Playwright、BrowserForge、Python 和 Linux 浏览器 archive。
- 本地 SHA-256、GitHub asset digest 和大小一致。
- Persistent Context 三周期启动成功，Cookie/LocalStorage 保留。
- 禁止 Camoufox webdl 自动获取缺失资产。
- 结论只限本机兼容性；不宣称供应链签名或完整出站网络观测。

### M1 / M1.1：Resolved Identity Artifact

- Artifact、Policy、Projection 使用 v3，ObservedWebsiteDigest 使用 v2。
- 每次冷启动从磁盘重新读取完整 resolved config。
- Artifact 与 archive、BuildID、SourceStamp、properties.json 和生成器版本绑定。
- 同一 Artifact 稳定重放，A/B/C 按预定信号分离，篡改在启动前拒绝。
- ConfiguredIdentityDigest 与网站可见 ObservedWebsiteDigest 分离。

### M2-0–M2.0.3：standalone Linux Host

- stdio JSON Lines Host，支持 `hello/launch/status/close/shutdown`。
- 严格 Artifact JSON、raw SHA、sidecar、browser tree 和不可信输入边界。
- Persistent Profile 跨 Host 进程保留 Cookie/LocalStorage。
- profile 独占、PID+start-time 所有权、全树退出确认和 fail-closed quarantine。
- Linux Host 仍是 standalone prototype，没有接入 Tauri/EngineAdapter。

## Linux accepted evidence

| 项目              | 证据                                                                 |
| ----------------- | -------------------------------------------------------------------- |
| Artifact 单元测试 | `21/21`                                                              |
| Host 集成测试     | `19/19`                                                              |
| Stability         | `run-1786158540-228a3340`，identity-a 5/5 相同 ObservedWebsiteDigest |
| Separation        | `run-1786158560-43ea2bd1`，A/B/C 两两不同                            |
| Tamper            | `run-1786158573-fef25c08`，四类篡改拒绝                              |
| M0 recheck        | `run-1786158578-047efb35`，三周期持久化、退出码 0                    |

原始 Profile 和完整运行报告位于执行环境的 gitignored `artifacts/`；分支中 tracked evidence manifest 是脱敏索引。上述结果是该 Linux 主机上的 accepted execution evidence，保持 `verified: false`。

## M2-W 已接受证据

以下结果来自原生 Windows Server 2025 RDP 桌面，会话内保持 standalone，且全部为 `verified: false`。执行 Agent 冻结结果后，主脑对同步祖先、禁止范围、code tree、manifest 引用、tracked Artifact 字节闭环和 Linux protected hashes 做了成本受控核对，并接受 M2-W Gate。

| 项目                   | 执行结果                                                                               |
| ---------------------- | -------------------------------------------------------------------------------------- |
| Artifact 严格单元测试  | `25/25`；包含 UTF-8/LF/no-BOM 精确字节 writer 回归                                     |
| Windows Host 驱动      | `10/10`；summary `summary-1786258836`                                                  |
| 跨 Host profile 持久化 | `run-1786258659-d77032e9`；两个不同 Host，bootCount `1 → 2`，Cookie/LocalStorage 保留  |
| Job/lifecycle          | `run-1786258752-26800060`；EOF 与 forced parent exit 后 active process count 均为 `0`  |
| 全新 cache 5 次冷启动  | `run-1786258892-4dd7e256`；5/5 digest 均为 `sha256:60f7f3…`，媒体设备计数匹配 Artifact |
| A/B/C separation       | `run-1786258999-b077d87e`；三个 ObservedWebsiteDigest 两两不同                         |
| Artifact tamper        | `run-1786259074-2f2ec9c1`；四种篡改全部启动前拒绝                                      |

tracked receipt 位于 `tests/fixtures/camoufox/evidence-manifest-windows.json`。它保留旧 run 集为 `preSyncEvidence`，只把同步后、最终代码 revision 生成并通过 sidecar 校验的 run 作为 accepted evidence。

## 主脑 Gate 决策

- 日期：2026-08-09
- 结论：**M2-W Accepted；三项核心 Gate 关闭**
- Persistent Profile：通过，证据为 `run-1786258659-d77032e9`
- Job Object / process ownership：通过，证据为 `run-1786258752-26800060`
- Windows Artifact replay：通过，证据为 `run-1786258892-4dd7e256`
- Evidence 完整性：三份 Artifact 的 Git blob、工作树、clean checkout、sidecar 与 manifest SHA 一致；14 个 current receipts 和 code tree 绑定一致
- 范围：没有接入 Tauri、EngineAdapter、UI 或安装器，没有改变 Artifact v3 / ObservedWebsiteDigest v2 语义

主脑没有重跑完整 Windows 测试；判定基于执行 Agent 的原生 Windows evidence，以及对远程 Git ancestry、diff scope、tracked bytes、manifest cross-reference 和 protected hashes 的最小核对。

## M3 研究结论

- M3-0 在 `e96ef3f` 关闭了 fake Host package、transport、failure matrix、
  RuntimeManager lifecycle 和 evidence 语义；它没有真实浏览器 run-id。
- M3-WI 的 R2 十周期真实 soak 证明同一 Profile、Cookie 和 observed digest 可以在
  一次受控序列中稳定保持，但同一 Host/test 源码的后续 Host matrix 六次只有一次通过。
- 最后的 R2H test-only 候选为 `186484f` / tree `e33d6d6`。预声明序列的 persistence
  与 lock-crash 各通过一次，第三项 persistence 在第二 Host `launch` 等待 stdout
  response 120 秒后失败；没有重试、没有 evidence manifest、没有 Accepted commit。
- 主脑终局：**M3-WI failed**。Camoufox Windows Managed 集成为 experimental，
  productionization 暂停；不再创建 R3/R4 或新的 test-only 子 Gate。

## Standard Silo Windows preview 首次执行

- 主脑合同提交为 `944dff9`；产品候选为 `b93259a` / tree `76eb5f3`，只修改创建页、
  UI contract test、样式和人工 Windows runbook。
- 候选让 Edge-only/Chrome-only 机器自动选择首个有效浏览器，默认路径收敛到
  Local + Direct；WSL、手工路径与网络配置进入高级设置；本机 stock Silo 运行时短周期
  核对状态，并诚实展示 `native`、`inherit`、`unavailable`。
- `pnpm` check/test/build、desktop Rust fmt/test 和 unsigned desktop-only build 通过；
  起点已有的两项 WSL Clippy warning 未在本任务越界修复。既有 Windows acceptance
  driver 缺少 `execution_target`，由 `5b09f04` 仅补 `SiloExecutionTarget::Local` 后编译通过。
- 正式 Edge/desktop-core acceptance 没有开始。执行 Agent 错误地用裸
  `msedge.exe --version` 探测版本，Edge 实际以默认 Profile 启动；虽从启动前零 Edge
  进程出发并按精确根 PID 回收整个新进程树，因没有启动前 Profile 指纹，默认 Profile
  是否被修改为 **unknown**。
- 主脑 Gate：**failed**。没有 desktop-core receipt、browser E2E summary 或真实 preview
  smoke，unsigned 构建不得作为 Accepted、正式 installer 或 shipped 产品使用；不自动
  重试，不读取用户 Edge Profile，也不删除或改写现场。

## Standard/Profile Isolation Windows local Preview 收口

- 最终产品 checkpoint 为 `aa72eadaf8300d1cd33a2c32173c06e3e677ca89` / tree
  `cd126770be02a33c6bb698853813512748b894c8`。原生 Windows Server 上的 source-bound
  desktop-core acceptance、Edge A/B Profile 与冷启动持久化、默认 Profile metadata
  前后核对及真实 Preview smoke 已通过；上节记录的历史默认 Profile 影响仍保持
  **unknown**，本次 metadata 一致只证明本次没有新增影响。
- 交付物是 **unsigned local Preview**，不是签名 installer、shipped release 或正式发布。
  已验证平台是 Windows Server；正常 Windows 10/11 client release matrix 仍待补充。
- Standard/Profile Isolation 只包含独立 Profile、网站状态持久化、单活 ownership 和本机
  Chrome/Edge 生命周期。它不包含 Managed Identity、设备或浏览器指纹虚拟化、代理隔离、
  WSL/Remote/Hyper-V 或整机虚拟化。
- Camoufox 仍在独立 Managed Engine 工作树继续调查，不进入这条产品集成链。Profile
  隔离层从本 checkpoint 起冻结；除真实回归缺陷外不再扩张。

## 当前 Gate

| Gate                                            | 状态                                             |
| ----------------------------------------------- | ------------------------------------------------ |
| Linux 资产固定、Artifact 重放与 standalone Host | **Accepted，M0–M2.0.3 关闭**                     |
| 原生 Windows M2-W                               | **Accepted；三项核心 Gate 关闭**                 |
| M3-0 EngineAdapter contract 集成                | **Accepted at `e96ef3f`；仅 fake Host contract** |
| 原生 Windows M3-WI 真实桌面集成                 | **Failed；Gate 关闭，不再重试**                  |
| Camoufox Windows Managed 产品化                 | **Experimental；暂停**                           |
| Standard Silo Windows 用户垂直切片              | **Local Preview passed；unsigned，待 client matrix** |

## Git 集成历史

1. [PR #11](https://github.com/QianQIUlp/VeriSilo/pull/11) 已合入 `main`，merge commit 为 `dab74e9`。
2. `origin/main` 已以 **merge** 方式合入 Camoufox 分支，生成同步基线 `9e88c0a`；没有 rebase，也没有改写 M0–M2.0.3 或 `d596afd` 的证据历史。
3. Windows 分支通过 merge commit `13cebd8` 合入更新后的 `origin/codex/camoufox-m0-m2-minimal`；没有 rebase、reset、cherry-pick 或历史改写。
4. 合并后按根 `AGENTS.md` 重新阅读四份事实源；旧基线 run-id 作为 `preSyncEvidence` 保留，没有冒充新基线结果。
5. 执行 Agent 只针对实际证据缺口修复了 Windows platformdirs cache 绑定、媒体枚举就绪、report/Artifact 精确字节 sidecar 和 evidence-side bounded Job cleanup；tracked Artifact 现为 UTF-8/LF/no-BOM 且禁用 Git 文本转换；没有接入 Tauri、EngineAdapter、UI 或安装器，也没有修改 Artifact v3 / ObservedWebsiteDigest v2 语义。
6. 最终 tracked manifest 由 summary/report/sidecar 与严格验证后的 tracked Artifact bytes 自动派生，绑定 receipt-producing code revision `3511d12`；主脑已接受 M2-W。
7. Windows stacked [PR #12](https://github.com/QianQIUlp/VeriSilo/pull/12) 在 7/7 checks 通过后合并。
8. 汇总 [PR #10](https://github.com/QianQIUlp/VeriSilo/pull/10) 随后合入 `main`，merge commit 为 `8de389d`。
9. M3 分支从 `8de389d` 创建；M3-0 accepted，M3-WI 最终 failed，分支未 push。

下面的“M2-W 冻结目标”定义阶段目标。Windows 任务现有验收合同继续有效，但 PR #11 中的产品语义、禁止范围和证据措辞优先；若二者冲突，停止扩大实现并退回主脑裁决。

## M2-W 冻结目标

M2-W 必须在原生 Windows（不是 Linux、WSL、Wine 或模拟器）验证：

1. Windows 专属 v3 Artifact 与固定 Windows Camoufox 资产能够稳定重放；
2. 相同 Profile 在两个 Host 进程间保持 Cookie、LocalStorage 和同源状态；
3. Windows 文件锁、process handle/creation-time 和 Job Object 形成内核所有权，Host/父管道退出不留下孤儿浏览器。

同时验证 reparse point、CRLF/binary stdio 和 Windows tree manifest。三项核心 Gate 任一失败都不能开放 M3。

## 下一阶段

Standard/Profile Isolation Windows local Preview 已在 `aa72ead` 通过并冻结。下一步仅做
主线集成准备与正常 Windows 10/11 client release matrix；不重复扩张 Profile 隔离层，
不把 unsigned Preview 改写成 shipped release。Camoufox Managed Engine 继续留在独立
工作树调查，不进入 Standard 产品集成链。

## 已知边界

- 当前 artifacts 使用 `fontMode=inherit`；宿主字体仍可见，不宣称字体隔离。
- Canvas 不进入稳定身份 Gate；不宣称其 seed 已形成可靠跨平台身份。
- TLS ClientHello、QUIC、跨主机复现和不可检测保持未验证或 unavailable。
- Linux 用户态树确认覆盖父进程存活期间捕获的后代；最后枚举后的瞬时 fork 需要 Windows Job Object 等内核所有权关闭。
- self-digest 和 SHA sidecar 是完整性门禁，不是发布者签名。
- M3-0 与失败的 M3-WI 只存在于本地研究分支；`main` 不具备 shipped Camoufox
  Managed Silo，且当前没有受信 signer、签名 Host package 或发布 runtime。
- Standard Silo 的独立 Profile 不等于指纹控制；产品文案必须保持这一边界。

## 更新规则

每次阶段 Gate 后，负责主脑必须更新：

- 日期、执行平台和代码 checkpoint；
- 分支/PR 状态；
- 测试计数和 accepted run-id；
- Gate 表和下一任务；
- 新增边界及其所属 backlog；
- 新任务使用的明确起始 commit。

状态更新不得删除历史 accepted checkpoint，也不得把计划、控制面或执行 Agent 自报结果写成已验证产品能力。
