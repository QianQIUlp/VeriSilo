# Camoufox Managed Engine 当前状态

- 状态：**可变项目状态页**
- 更新日期：2026-08-11

本文只记录当前执行阶段、证据 checkpoint 和下一项任务。长期产品意图见[身份平台北极星](identity-platform-north-star.md)，路线原因见[Camoufox-first Managed Engine 决策](camoufox-managed-engine-decision.md)。每次 Gate 变化后更新本文，不用本文反向改写长期决策。

## Git 状态

| 对象                            | 当前值                                                                                                           | 含义                                                                                    |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `origin/main` 集成基线          | `8de389db366d1d9ff510b1e885fab7f49a89aad0`                                                                       | PR #10 已合并；M0–M2-W Host、Artifact 与 evidence 已进入主线                            |
| 历史 Camoufox 证据分支          | `codex/camoufox-m0-m2-minimal` / `da8c00c`                                                                       | PR #12 合入后的完整证据历史；不再是当前开发起点                                         |
| M3 执行/研究分支                | `codex/camoufox-m3-engine-adapter`                                                                               | M3-0 Accepted 后继续承载 M3-WI 与 R1/R2/R2H 研究历史；不得整支抽取为产品 patch          |
| M3-0 任务与 Accepted checkpoint | [任务合同](camoufox-m3-engine-adapter-task.md) / `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`                      | contract-level fake Host Gate 已关闭；不包含真实 Camoufox run-id                        |
| M3-WI 历史任务                  | [合同与调查收口](camoufox-m3-wi-windows-task.md) / original baseline `e96ef3f`                                   | **Failed**；第二 Host 根因调查 **Inconclusive**；不存在 Accepted fix                    |
| M3-WI 调查结束快照              | `186484feb935076766beab09595a9270f86f78ef` / tree `e33d6d68586a79796ffb9bcc668392e369dc97c6`                     | `e96ef3f` 为祖先；调查结束时 tracked tree clean；没有 production fix                    |
| R2 tracked 候选 evidence        | `ecafca9` / `evidence-manifest-m3-wi-r2-windows.json`                                                            | 执行 Agent 候选；主脑未接受，不是 M3-WI Accepted evidence                               |
| R2H 研究基础                    | `186484f`                                                                                                        | 只有 runner/freezer/schema 与 Host test 变更；无 tracked result/manifest                |
| 当前 FP1 任务                   | [冻结合同与续执行结果](camoufox-fp1-deterministic-artifact-projection-task.md) / source implementation `eea8606` | Canvas source patch 已精确绑定；当前无可用受支持构建环境，尚无 candidate Windows binary |
| M2.0.3 代码 checkpoint          | `3b53830`                                                                                                        | 严格进程树、quarantine、JSON 和 RFC3339 收口                                            |
| Linux accepted checkpoint       | `d596afd76e59ba64915b036fbc732a2c28f1ec54`                                                                       | evidence manifest 冻结提交；保持不变                                                    |
| Windows accepted checkpoint     | `1bf0854e4fac7142baef9792967851593b804912`                                                                       | M2-W evidence 冻结提交；主脑 Gate 已接受                                                |
| M2-W execution code             | `3511d120862283c3b90f91589f5f33d1de8325f9`                                                                       | Windows runtime/test 与 tracked Artifact 字节闭环代码；manifest 绑定 tree `b42d7d9`     |
| Windows stacked PR              | [#12](https://github.com/QianQIUlp/VeriSilo/pull/12)（已合并）                                                   | merge commit `da8c00ca76504941099e27cdc1d5ecdd93d91d13`                                 |
| Camoufox 集成 PR                | [#10](https://github.com/QianQIUlp/VeriSilo/pull/10)（已合并）                                                   | merge commit `8de389db366d1d9ff510b1e885fab7f49a89aad0`，保留完整 evidence 历史         |
| 上下文文档 PR                   | [#11](https://github.com/QianQIUlp/VeriSilo/pull/11)（已合并）                                                   | 四份事实源是 `main` 的规范上下文                                                        |

`d596afd` 是已经接受的 Linux 证据 checkpoint，不因后续合并而改写。`9e88c0a` 是 M2-W 内容同步基线，不替代既有 Linux evidence manifest。Windows 分支通过 `13cebd8` 合入同步基线，最终由 `1bf0854` 冻结主脑已接受的 Windows evidence；旧起点结果仅作为 `preSyncEvidence` 保留。PR #12 和 PR #10 都使用 merge commit，没有 rebase、squash 或改写上述历史。

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

## M3-0 已接受证据

- 执行平台：原生 Windows checkout；contract-level fake Host，没有启动真实 Camoufox。
- 起始 checkpoint：`b3d094cb0a7f3b7f9c113c53e4c4575d16babb67`。
- 原始候选：`bc65e07fbc21ee8581b3ac60c91afd15d0effa20`，主脑 Gate 未接受。
- 最终 Accepted checkpoint：`e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`；其间只有追加修正提交，没有 amend、reset、rebase 或历史改写。
- 分支状态（Gate 时）：`codex/camoufox-m3-engine-adapter` 相对 upstream ahead 4；工作树 clean，未 push、未创建 PR。

| 证据类别                  | Accepted 结果                                                                                      |
| ------------------------- | -------------------------------------------------------------------------------------------------- |
| package / tree            | v3 Host package、package tree 与 browser tree 分离绑定；旧 v2 Camoufox、篡改和额外成员 fail-closed |
| transport / lifecycle     | 专用 `camoufox-host-jsonl-v1`；fake Host 覆盖 launch/close/shutdown、EOF、crash、timeout 与畸形帧  |
| RuntimeManager 垂直切片   | 实际经过 adapter plan、spawn、Host transport、stop；精确 child/profile ownership                   |
| network / secret boundary | Camoufox 仅 Direct；Vault/Artifact seed、token、proxy secret 不进入 argv、wire、evidence 或记录    |
| evidence 语义             | `hostLaunch=observed`、`verifiedAdapter=null`、`verified:false`；不伪造 generic receipt            |
| JS / Rust / Artifact 回归 | `pnpm test` 150；Desktop `cargo test` 184；Artifact `25/25`；check/build/fmt/clippy 均通过         |

M3-0 没有真实浏览器 run-id；其 accepted evidence 是上述测试计数、提交链与合同级 fixture，不能冒充原生 Windows 真实浏览器证据。`pnpm engine:verify` 通过的含义包括 production signer pin 缺失时继续 fail-closed，不表示已有受信 signer 或签名发布包。

## M3-WI Gate 与调查收口

- 主脑结论：**M3-WI Failed；第二 Host 根因调查 Inconclusive**。
- 没有 production fix、focused regression 或 post-fix final real verification；最后 Accepted
  checkpoint 仍是 `e96ef3f`。
- tracked R2 manifest 自报 `execution-passed-awaiting-main-brain-gate`，但其明确记录
  `m3WiFullGateRerun=false`，Windows Host `10/10` 复用较早 clean receipt，且六次 R2
  Host matrix 只有一次通过，同时保留两次 `ctx.close` timeout。主脑未接受
  该 manifest；其中的十周期同 Profile 成功只是有限正向观察。
- R2H 快照 `186484f` 增加了独立 5+5 runner、freezer、schema 和 Host test 设施，
  但仓库没有 tracked R2H result/manifest；不再运行该矩阵。

主脑在 gitignored `artifacts/cfx-life/run-0cdd1ed6/` 上收口了三个有判别力的
真实浏览器序列。该 bundle 不是 manifest-bound evidence，没有进入 Git：

1. direct two-Host/same-Profile 两次成功，第二次 launch `12.047s`，
   boot `1 → 2`、Cookie/digest、clean close、Job `0` 和锁释放成立；
2. 绕过 Host/supervisor 的 fixed Camoufox/Playwright 两次 Profile reopen 成功；
3. 深度阶段实验在第一 Host 完成 launch/response 后遇到独立 `ctx.close`
   timeout，未进入第二 Host。

实验 1/2 是修复前对照，不能冒充修复后验证；实验 3 被独立 close 失败截断。
历史 raw stage log 已被清理，所以无法将旧 120 秒唯一归因到
`launch_persistent_context`、`ctx.new_page()` 或后续的 fonts/media/identity/cookie
Playwright RPC。被调查的 120 秒来自 direct Python harness 的
`stdout.readline()` watchdog，不是该次 RuntimeManager receipt 或 Camoufox 自带超时；
Rust 中另有 test-only `M3_WI_REAL_HOST_TIMEOUT=120s`，不得泛化成“Rust 没有
120 秒”。

三份本地摘要的 SHA-256、实验 3 的 `closeOk=true` / session `failed` 区分、只能
作为执行 Agent 报告的 Security 4688 观察，以及“无 production diff / 无 focused
regression”的 integration extraction 均冻结在
[M3-WI 历史任务收口](camoufox-m3-wi-windows-task.md)。

## 当前 Gate

| Gate                                            | 状态                                                                     |
| ----------------------------------------------- | ------------------------------------------------------------------------ |
| Linux 资产固定、Artifact 重放与 standalone Host | **Accepted，M0–M2.0.3 关闭**                                             |
| 原生 Windows M2-W                               | **Accepted；三项核心 Gate 关闭**                                         |
| M3-0 EngineAdapter contract 集成                | **Accepted at `e96ef3f`；fake Host Gate 关闭**                           |
| 原生 Windows M3-WI 桌面/真实 Host 集成          | **Failed；investigation inconclusive；experimental；未修复、未 shipped** |
| FP1 Deterministic Artifact Projection           | **Blocked build environment；source patch 未编译/运行；未 Accepted**     |
| Managed Identity UI、代理联动、生产打包         | **后续阶段；本阶段不开放**                                               |

## Git 集成历史

1. [PR #11](https://github.com/QianQIUlp/VeriSilo/pull/11) 已合入 `main`，merge commit 为 `dab74e9`。
2. `origin/main` 已以 **merge** 方式合入 Camoufox 分支，生成同步基线 `9e88c0a`；没有 rebase，也没有改写 M0–M2.0.3 或 `d596afd` 的证据历史。
3. Windows 分支通过 merge commit `13cebd8` 合入更新后的 `origin/codex/camoufox-m0-m2-minimal`；没有 rebase、reset、cherry-pick 或历史改写。
4. 合并后按根 `AGENTS.md` 重新阅读四份事实源；旧基线 run-id 作为 `preSyncEvidence` 保留，没有冒充新基线结果。
5. 执行 Agent 只针对实际证据缺口修复了 Windows platformdirs cache 绑定、媒体枚举就绪、report/Artifact 精确字节 sidecar 和 evidence-side bounded Job cleanup；tracked Artifact 现为 UTF-8/LF/no-BOM 且禁用 Git 文本转换；没有接入 Tauri、EngineAdapter、UI 或安装器，也没有修改 Artifact v3 / ObservedWebsiteDigest v2 语义。
6. 最终 tracked manifest 由 summary/report/sidecar 与严格验证后的 tracked Artifact bytes 自动派生，绑定 receipt-producing code revision `3511d12`；主脑已接受 M2-W。
7. Windows stacked [PR #12](https://github.com/QianQIUlp/VeriSilo/pull/12) 在 7/7 checks 通过后以 merge commit `da8c00c` 合入 Camoufox 分支。
8. 汇总 [PR #10](https://github.com/QianQIUlp/VeriSilo/pull/10) 的最新 HEAD 在 7/7 checks 通过后以 merge commit `8de389d` 合入 `main`。
9. M3 分支从精确的 `8de389d` 创建，不从旧 `d596afd` 或 Windows evidence commit 直接继续。
10. M3-0 从冻结 checkpoint `b3d094c` 执行；原始候选 `bc65e07` 未被接受，最终追加修正至 `e96ef3f` 后由主脑接受。该分支尚未 push 或创建 PR。
11. M3-WI 在 `e96ef3f` 之后进入 test-only 真实 Host 集成和 R1/R2 研究；
    tracked R2 manifest 最终冻结于 `ecafca9`，但没有通过主脑 Gate，不能追认为
    Accepted evidence。
12. `186484f` 只在 R2 后增加 R2H runner/freezer/schema 和 Host test 基础；没有
    tracked R2H run-id、result 或 manifest。
13. 第二 Host 收口调查在 `186484f` 的 clean tree 上只生成 gitignored 诊断；
    没有新提交，没有生产修复，M3-WI 保持 Failed。
14. FP1 media 边界修复冻结于 `bc153f13c18af9f7404e3cb6674e3b29a18de800` / tree
    `e3a788f6a06de9a8860b9c3bfe6ad0583f7f534d`；39/39 Artifact/focused tests 与
    4-case close regression 通过，未改变共享 schema/API 或固定依赖。
15. 唯一真实序列在该 clean tree 上运行 A1、A2 后停止：两次 media 均 `success`，
    Canvas raw hash 相同但 export hash 不同；按合同未启动 B1，未重试或选择历史样本。
16. Artifact/Silo-scoped Canvas source patch 冻结于
    `eea8606b6512922cfcf071e977a9e0cf2958deaf` / tree
    `5693e8879348970078aeab79c51eab2ecfc333c0`；build-route provenance 与 exact-test
    no-skip closure 追加冻结于 `dbad62d24c6e8f6d57bb29ba5864a308584e9ca4` / tree
    `df761be583b0171335a0955a7af46cb683f6e0bf`。精确 source closure 9/9、Artifact/Host 回归
    40/40 与 Python 静态检查通过；这些结果只证明 source inputs、patch application、seam
    hashes 和当前 caller-file closure，不是 compile、binary 或 runtime evidence。

下面的“M2-W 冻结目标”定义阶段目标。Windows 任务现有验收合同继续有效，但 PR #11 中的产品语义、禁止范围和证据措辞优先；若二者冲突，停止扩大实现并退回主脑裁决。

## M2-W 冻结目标

M2-W 必须在原生 Windows（不是 Linux、WSL、Wine 或模拟器）验证：

1. Windows 专属 v3 Artifact 与固定 Windows Camoufox 资产能够稳定重放；
2. 相同 Profile 在两个 Host 进程间保持 Cookie、LocalStorage 和同源状态；
3. Windows 文件锁、process handle/creation-time 和 Job Object 形成内核所有权，Host/父管道退出不留下孤儿浏览器。

同时验证 reparse point、CRLF/binary stdio 和 Windows tree manifest。三项核心 Gate 任一失败都不能开放 M3。

## 下一阶段

固定 FF152/Camoufox downstream Canvas source patch 与 source binding 已冻结。Camoufox
支持在 Linux execution environment 中交叉构建 Windows target，也支持在 Windows physical
host 上使用其 Linux Docker container；direct native-Windows 和 WSL build 不受支持。当前
机器没有 Docker/container engine，也没有已授权的外部 Linux build host，因此不存在无需
新增 machine-level runtime 或外部写入即可执行的受支持路线。下一项工作是在受控 Linux
build environment 中锁定 toolchain/provenance 并生成独立 VeriSilo Windows
archive/tree/binding。取得并冻结该 binary 后，才只执行一次 Canvas focused
A1→A2→B1；focused 通过后才允许唯一一次完整 FP1 A1→A2→B1。不得用 WSL 或未经支持的
原生 Windows recipe 替代，不得覆盖历史 official binding/fixtures/evidence，不得进入 FP2、
恢复 R2/R2H 或重跑选样。

后续顺序固定为 FP2 跨 realm 一致性 → FP3 网络/地区/WebRTC 协调 → FP4 实站
兼容性 → 使用届时最终 Managed Engine 冻结新的 clean M3-WI 合同。旧 M3-WI
合同不复活。UI、安装器、代理、production package/signing、Controlled Chromium 和虚拟化
后端均不进入 FP1。

## 已知边界

- 当前 artifacts 使用 `fontMode=inherit`；宿主字体仍可见，不宣称字体隔离。
- M2 evidence 没有证明 Canvas 稳定；FP1 A1/A2 已观察到 raw hash 稳定但 export hash
  漂移。`eea8606` 已提供精确绑定的 `canvas:seed` source consumption path，但尚未编译成
  Windows binary，也没有 runtime observation，因此该 seed 仍只能写为 configured，不能
  升级为 applied 或宣称 Canvas 身份稳定。
- TLS ClientHello、QUIC、跨主机复现和不可检测保持未验证或 unavailable。
- Linux 用户态树确认覆盖父进程存活期间捕获的后代；最后枚举后的瞬时 fork 需要 Windows Job Object 等内核所有权关闭。
- self-digest 和 SHA sidecar 是完整性门禁，不是发布者签名。
- M3 分支已经在 fake Host 合同测试中经过 `RuntimeManager`/EngineAdapter 接缝，但 production Tauri 仍未通过受信 package 调用真实 standalone Host；“contract Gate 通过”不等于桌面 Managed Identity 能力已经 shipped。
- 当前没有受信 signer pin、签名 Host package 或发布 runtime；production external-engine launch 仍 fail closed。
- 当前 M3-WI 已 Failed，第二 Host 调查 Inconclusive；正向对照不是底层挂起已修复的证据。
- 未来 clean M3-WI 即使通过，也只会证明对应原生 Windows 主机上的 integration-only 真实 Host/浏览器路径；生产 package/signing、installer、跨主机复现和产品 UI 仍属于后续 Gate。

## 更新规则

每次阶段 Gate 后，负责主脑必须更新：

- 日期、执行平台和代码 checkpoint；
- 分支/PR 状态；
- 测试计数和 accepted run-id；
- Gate 表和下一任务；
- 新增边界及其所属 backlog；
- 新任务使用的明确起始 commit。

状态更新不得删除历史 accepted checkpoint，也不得把计划、控制面或执行 Agent 自报结果写成已验证产品能力。
