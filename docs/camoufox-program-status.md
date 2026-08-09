# Camoufox Managed Engine 当前状态

- 状态：**可变项目状态页**
- 更新日期：2026-08-09

本文只记录当前执行阶段、证据 checkpoint 和下一项任务。长期产品意图见[身份平台北极星](identity-platform-north-star.md)，路线原因见[Camoufox-first Managed Engine 决策](camoufox-managed-engine-decision.md)。每次 Gate 变化后更新本文，不用本文反向改写长期决策。

## Git 状态

| 对象                      | 当前值                                               | 含义                                                         |
| ------------------------- | ---------------------------------------------------- | ------------------------------------------------------------ |
| `origin/main` 基线        | `dab74e9be00287e0d357874db883748370bcb2aa`           | PR #11 已合并；主线仍未包含 Camoufox M0–M2.0.3               |
| Camoufox 分支             | `codex/camoufox-m0-m2-minimal`                       | 自包含的 Linux Managed Engine 垂直切片                       |
| M2.0.3 代码 checkpoint    | `3b53830`                                            | 严格进程树、quarantine、JSON 和 RFC3339 收口                 |
| Linux accepted checkpoint | `d596afd76e59ba64915b036fbc732a2c28f1ec54`           | evidence manifest 冻结提交；保持不变                         |
| M2-W 同步基线             | `9e88c0aa2486dd18a3ef241b1d4dcca3a7890efc`           | 以 merge 方式把 `origin/main` 合入 Camoufox 分支的提交       |
| Windows 工作分支          | `codex/camoufox-m2-windows-gate`                     | 已从旧 checkpoint 提前开始；现有工作保留，等待合入同步后基线 |
| M2-W execution code      | `3511d120862283c3b90f91589f5f33d1de8325f9`           | Windows runtime/test 与 tracked Artifact 字节闭环代码；manifest 绑定 tree `b42d7d9` |
| Camoufox Draft PR         | [#10](https://github.com/QianQIUlp/VeriSilo/pull/10) | 目标为 `main`；未合并前不能称为主线或 shipped 能力           |
| 上下文文档 PR             | [#11](https://github.com/QianQIUlp/VeriSilo/pull/11) | 已合并；四份事实源现在是 `main` 的规范上下文                 |

`d596afd` 是已经接受的 Linux 证据 checkpoint，不因文档合并或 Windows 工作而改写。`9e88c0a` 是新的 M2-W 内容同步基线，不替代既有 Linux evidence manifest。Windows 分支在它产生前已经开始，因此旧起点上的实现和调试结果可以保留，但只有把更新后的 Camoufox 分支 merge 进去后生成的最终证据，才能用于 M2-W Gate。

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

## M2-W 执行 Agent 候选证据

以下结果来自原生 Windows Server 2025 RDP 桌面，会话内保持 standalone，且全部为 `verified: false`。它们已经由执行 Agent 冻结，但尚未由主脑完成 M2-W Gate 审阅，因此本节不开放 M3。

| 项目 | 执行结果 |
| --- | --- |
| Artifact 严格单元测试 | `25/25`；包含 UTF-8/LF/no-BOM 精确字节 writer 回归 |
| Windows Host 驱动 | `10/10`；summary `summary-1786258836` |
| 跨 Host profile 持久化 | `run-1786258659-d77032e9`；两个不同 Host，bootCount `1 → 2`，Cookie/LocalStorage 保留 |
| Job/lifecycle | `run-1786258752-26800060`；EOF 与 forced parent exit 后 active process count 均为 `0` |
| 全新 cache 5 次冷启动 | `run-1786258892-4dd7e256`；5/5 digest 均为 `sha256:60f7f3…`，媒体设备计数匹配 Artifact |
| A/B/C separation | `run-1786258999-b077d87e`；三个 ObservedWebsiteDigest 两两不同 |
| Artifact tamper | `run-1786259074-2f2ec9c1`；四种篡改全部启动前拒绝 |

tracked receipt 位于 `tests/fixtures/camoufox/evidence-manifest-windows.json`。它保留旧 run 集为 `preSyncEvidence`，只把同步后、最终代码 revision 生成并通过 sidecar 校验的 run 作为当前候选 evidence。

## 当前 Gate

| Gate                                            | 状态                                    |
| ----------------------------------------------- | --------------------------------------- |
| Linux 资产固定、Artifact 重放与 standalone Host | **Accepted，M0–M2.0.3 关闭**            |
| 原生 Windows M2-W                               | **执行证据已冻结；等待主脑成本受控 Gate 审阅** |
| EngineAdapter / Tauri 集成                      | **不允许；必须等待 M2-W 三项核心 Gate** |
| Managed Identity UI、代理联动、生产打包         | **后续阶段**                            |

## PR #11 合并后的实际衔接

1. [PR #11](https://github.com/QianQIUlp/VeriSilo/pull/11) 已合入 `main`，merge commit 为 `dab74e9`。
2. `origin/main` 已以 **merge** 方式合入 Camoufox 分支，生成同步基线 `9e88c0a`；没有 rebase，也没有改写 M0–M2.0.3 或 `d596afd` 的证据历史。
3. Windows 分支通过 merge commit `13cebd8` 合入更新后的 `origin/codex/camoufox-m0-m2-minimal`；没有 rebase、reset、cherry-pick 或历史改写。
4. 合并后按根 `AGENTS.md` 重新阅读四份事实源；旧基线 run-id 作为 `preSyncEvidence` 保留，没有冒充新基线结果。
5. 执行 Agent 只针对实际证据缺口修复了 Windows platformdirs cache 绑定、媒体枚举就绪、report/Artifact 精确字节 sidecar 和 evidence-side bounded Job cleanup；tracked Artifact 现为 UTF-8/LF/no-BOM 且禁用 Git 文本转换；没有接入 Tauri、EngineAdapter、UI 或安装器，也没有修改 Artifact v3 / ObservedWebsiteDigest v2 语义。
6. 最终 tracked manifest 由 summary/report/sidecar 与严格验证后的 tracked Artifact bytes 自动派生，绑定 receipt-producing code revision `3511d12`；M3 是否开放仍由主脑审阅决定。

下面的“M2-W 冻结目标”定义阶段目标。Windows 任务现有验收合同继续有效，但 PR #11 中的产品语义、禁止范围和证据措辞优先；若二者冲突，停止扩大实现并退回主脑裁决。

## M2-W 冻结目标

M2-W 必须在原生 Windows（不是 Linux、WSL、Wine 或模拟器）验证：

1. Windows 专属 v3 Artifact 与固定 Windows Camoufox 资产能够稳定重放；
2. 相同 Profile 在两个 Host 进程间保持 Cookie、LocalStorage 和同源状态；
3. Windows 文件锁、process handle/creation-time 和 Job Object 形成内核所有权，Host/父管道退出不留下孤儿浏览器。

同时验证 reparse point、CRLF/binary stdio 和 Windows tree manifest。三项核心 Gate 任一失败都不能开放 M3。

## 已知边界

- 当前 artifacts 使用 `fontMode=inherit`；宿主字体仍可见，不宣称字体隔离。
- Canvas 不进入稳定身份 Gate；不宣称其 seed 已形成可靠跨平台身份。
- TLS ClientHello、QUIC、跨主机复现和不可检测保持未验证或 unavailable。
- Linux 用户态树确认覆盖父进程存活期间捕获的后代；最后枚举后的瞬时 fork 需要 Windows Job Object 等内核所有权关闭。
- self-digest 和 SHA sidecar 是完整性门禁，不是发布者签名。
- PR #10 未合并前，主线 EngineAdapter 和桌面产品不具备该 Host 能力。

## 更新规则

每次阶段 Gate 后，负责主脑必须更新：

- 日期、执行平台和代码 checkpoint；
- 分支/PR 状态；
- 测试计数和 accepted run-id；
- Gate 表和下一任务；
- 新增边界及其所属 backlog；
- 新任务使用的明确起始 commit。

状态更新不得删除历史 accepted checkpoint，也不得把计划、控制面或执行 Agent 自报结果写成已验证产品能力。
