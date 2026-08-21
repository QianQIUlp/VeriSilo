# Camoufox Managed Engine 当前状态

- 状态：**可变项目状态页**
- 更新日期：2026-08-20

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
| 当前 FP1 任务                   | [冻结合同、执行历史与离线证据闭包](camoufox-fp1-deterministic-artifact-projection-task.md) / closure baseline `b7a615ac39606deb741b3b3ea13d3584a987a39c` / tree `9461adcc6924539dc4c2bb80963fab71a2efef49` | 原始 full runner verdict 保持 Failed；主脑基于 immutable A1/A2/B1 evidence 与 corrected contract adjudication 接受 FP1，`verified:false`；FP2 未进入，只允许成为下一项单独冻结任务 |
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
| FP1 Deterministic Artifact Projection           | **Accepted based on immutable original A1/A2/B1 evidence plus corrected contract adjudication；原始 runner verdict 保持 Failed；`verified:false`** |
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
17. `blocked-build-environment` 已在受控 Linux builder 上解除。source commit
    `4f1f01f00844e1888139b4236424550c94a6e10f` / tree
    `35da4d372bf8f468062c6eb5ea64187be1c6d595` 生成 Windows archive
    `8221486f42f547603339da7442e4c412671afc66d6742d01f99918f12f85be1d`
    （493,054,882 bytes）。独立 self-built asset lock raw SHA-256 为
    `0ce34b8a44c90e6c313aad66030a800359bbca78b97ca565cbdefa4e4eb95cfe`；tree
    manifest raw/canonical SHA-256 分别为
    `68ae52e3d11bba5b2868b68ea90af962840c6890a4418fc24199ba9a96138bf3` /
    `ebc35ddbdc59c32b9856b56a9dcfc6e375d5a090abc6b498238e6cd874c09dfa`，
    绑定 503 files / 981,198,096 bytes。runtime seam、deterministic policy、rebound
    fixtures 和 probe 冻结于 `970b357d8e2cf0ffb619d2e478519a12242abc5a` / tree
    `6119c15f18aebe49cae488cd0f9870ba37ad42e4`。该 checkpoint 只关闭
    build/finalization 与静态绑定子步骤，保持 `verified:false`；没有取得 browser lease，
    也没有运行 Canvas focused 或完整 FP1。
18. Canvas focused 在 clean HEAD `96546259574f73ec50f4c3715c3d978641155d8d` / tree
    `4a3248f4564ddb5f8983b93db74faca9a77f77d8` 上只执行一次 A1→A2→seed-B1 并通过。
    run `canvas-focused-20260815T081637990579Z` 的 gitignored report SHA-256 为
    `1329506655594e5a65b73ffbad6edb0f7f45f6ddeecd92ecc6d7f1f3e8263460`。
    A1/A2 的 raw RGBA、decoded PNG pixels、PNG bytes 和 dataURL hash 全同；seed-B 的
    raw/decoded pixels 保持相同，PNG bytes 与 dataURL hash 确定性分离。三次独立 Host/browser
    都 clean close、Job active 0、无进程残留且 Profile 两个 lock byte 可重取。该结果仍为
    `verified:false`，missing-seed 只有源码回归，完整 FP1 未执行，FP2 未开放。
19. 唯一 full FP1 从 clean HEAD `7a5465e8f971a81ef067be4735c764ce3cb0ea29` / tree
    `460a386f12fc46b004a2d8121d31cb0208d07b05` 启动，run
    `fp1-full-20260815T090107649930Z` 的 report SHA-256 为
    `9c90b95d3f4932535d6f3fb46be5232429ae5587dc66d7e3ee565386566a49a7`。
    A1 的 47-key sent config、Canvas、media `1/1/0`、Cookie/LocalStorage/SQLite observation
    均成功，但 `page.close()` 成功后 `ctx.close()` 在 9,992 ms timeout；Host 以 named Job
    强制清理至 active 0 / remaining 空，因此生命周期 Gate 为 `unclean_close`。A2/B1 未
    启动，候选 full one-shot claim 已消费；FP1 Failed / 未 Accepted，M3-WI 不变，FP2 关闭。
20. 2026-08-16 close 根因收口：实验 C 校正 marker 时间线后当日 11/11 干净关闭，确认
    竞态为间歇性、与 media 后端激活非充分因果；结合实验 A 的 pipe 健康
    证据与上游取证，根因定位为 camoufox Juggler `Browser.close` 的无界 pre-quit
    await（idleTasks/startComplete/XPI startup promises）。下游补丁
    `0002-verisilo-juggler-bounded-close.patch` 以 3 s 共享 deadline 加必然 quit，
    source engine revision 升为 `canvas-export-v1-close-bound-v1`，源闭合冻结于
    `9ee93e4`（tracked-only 33 项通过；二进制 binding 未动）。重建、rebind、focused
    与新 full FP1 未执行。
21. builder 镜像因 strict_build 驱动变更需重建：`b6a9f0b` 将 lock 的
    `builderImageBinding` 置 null（prepare-image 的前置条件），tracked 33 项仍通过。
    构建 bundle（`verisilo-b6a9f0b.bundle`，SHA-256
    `15bded3d…0bf5d41`）与 lespaul 阶段一脚本
    `lespaul-run-closebound-image.sh` 已备好；lespaul 当前离线，镜像重建、binding
    提交、引擎构建与后续验证待其开机后执行。分支已推送至 PR #16。

下面的“M2-W 冻结目标”定义阶段目标。Windows 任务现有验收合同继续有效，但 PR #11 中的产品语义、禁止范围和证据措辞优先；若二者冲突，停止扩大实现并退回主脑裁决。

## M2-W 冻结目标

M2-W 必须在原生 Windows（不是 Linux、WSL、Wine 或模拟器）验证：

1. Windows 专属 v3 Artifact 与固定 Windows Camoufox 资产能够稳定重放；
2. 相同 Profile 在两个 Host 进程间保持 Cookie、LocalStorage 和同源状态；
3. Windows 文件锁、process handle/creation-time 和 Job Object 形成内核所有权，Host/父管道退出不留下孤儿浏览器。

同时验证 reparse point、CRLF/binary stdio 和 Windows tree manifest。三项核心 Gate 任一失败都不能开放 M3。

## 下一阶段

FP1 已在 2026-08-20 完成 offline evidence closure。原始 runner report、failure code、
one-shot claim、Artifact、probe 和所有 A1/A2/B1 referenced evidence 均保持原字节；
corrected comparator 恢复冻结合同并由无浏览器回归覆盖。主脑 FP1 Gate 为
**Accepted based on immutable original A1/A2/B1 evidence plus corrected contract
adjudication**，但 `verified:false`，且不追改原始 runner 的 Failed verdict。

下一阶段只允许把 FP2 定义为一项新的、单独冻结的任务：先写明合同、基线、禁止范围和
独立 Gate，再决定是否执行。当前没有进入 FP2，不得把本次 offline adjudication 写成新的
runner report、浏览器重跑或产品 verified 能力；M3-WI、UI、代理联动、生产签名/打包状态
均不因 FP1 Gate 改变。

后续顺序固定为 FP2 跨 realm 一致性 → FP3 网络/地区/WebRTC 协调 → FP4 实站
兼容性 → 使用届时最终 Managed Engine 冻结新的 clean M3-WI 合同。旧 M3-WI
合同不复活。UI、安装器、代理、production package/signing、Controlled Chromium 和虚拟化
后端均不进入 FP1。

## 已知边界

- 当前 artifacts 使用 `fontMode=inherit`；宿主字体仍可见，不宣称字体隔离。
- M2 evidence 没有证明 Canvas 稳定；旧 official build 的 FP1 A1/A2 只观察到 raw hash
  稳定而 export hash 漂移。精确绑定的 self-built candidate 已在本 Windows host 的一次
  focused A1/A2/seed-B1 中观察到 Canvas export contract 成立；这支持该候选的
  `canvas:seed` Canvas-focused applied/observed 结论，但不证明完整 47-key Artifact 已稳定
  重放、完整 A/B 分离、跨主机一致或 FP1 Accepted。
- 唯一 full FP1 的 A1 已再次观察到稳定的 A Canvas 与 media `1/1/0`，但随后
  `ctx.close()` timeout 并触发 forced Job cleanup；生命周期失败使 A1 不能进入完整
  A1/A2 比较，A2/B1 也未启动。该部分 observation 不改变 FP1 Failed / 未 Accepted。
- TLS ClientHello、QUIC、跨主机复现和不可检测保持未验证或 unavailable。
- close 竞态的修复是行为级 bound（保证退出与响应路径可达），不是对某个上游
  promise 为何 stall 的机理证明；`0002` 的阶段日志将在未来再现时给出具体卡点。
- `test_windows_host.py` protocol 检查的 `"stage":"response write"` 断言在当前
  Host 代码下不可满足（记录器仅 `launch` 激活），clean `0455dec` 同样失败；属存量
  过期断言，列入 backlog 单独修正，不影响本阶段 Gate。
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

### 2026-08-18 close-bound Windows candidate finalization checkpoint

lespaul 的 one-shot Windows cross-build 已以 `container-passed` / exit 0 结束，并绑定
source commit `e571f6c0b2cea90955b929a4ff04ad54007778fa`、tree
`7d6e41dd892c68aadccb3d177f5e33a6bd974486` 和 source-lock SHA-256
`42b14bfe7331e6c023a3a6fa49da614852b6fd0a28c3225cc84bfc515d4036d5`。
新 archive SHA-256 为
`148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94`，大小
493,100,709 bytes；Windows 安全提取后的完整树为 503 files / 981,205,753 bytes，
tree manifest raw SHA-256 为
`3a7b9ba83d93e1d40fc30cb4831750d9a125c76db0551459197c74f6b14c86f9`，
`camoufox.exe` SHA-256 为
`172f51387bc61e331446883e5499c67611aea5fd81091f68df26b166c9687bf1`。

现有 v3 self-built lock、deterministic Canvas exact binding 与三份 47-key rebound
Artifact 已切换到该候选；production tree verification 与无浏览器 binding/policy tests
通过。所有 lock/result 继续保持 `verified:false`，当前只证明 compiled candidate 与静态
binding 闭合，不证明 bounded-close、Canvas runtime 或 FP1 已通过。下一项任务仅为从新的
clean checkpoint 执行一次 Canvas focused A1→A2→seed-B1；通过并再次冻结后，才允许一次
新的 full FP1。FP1 仍为 Failed / 未 Accepted，FP2 继续关闭。

### 2026-08-18 close-bound Canvas focused runtime checkpoint

close-bound candidate 已在 clean checkpoint
`91c7f4252126d5286509c79e2ce98c82e44aa1ef` / tree
`ec14bcc6e4c2f8a2a669807c29a25e95533e460e` 上完成唯一一次 Windows
Canvas focused `A1 -> A2 -> seed-B1`：

- run：`canvas-focused-20260818T095204967986Z`；
- report：SHA-256
  `06a9b68876dbeef5a2c34a0fe43f03efb949bfddea1a3dc2eb71d160069e170d`，
  18,862 bytes，sidecar 匹配；
- one-shot claim：SHA-256
  `799856606b2903697cc97775e0673482e0e4f2d912a0774a4ebac648ab27072e`，
  517 bytes；
- harness：SHA-256
  `fa63703abb858c3f9843728b97b18b653044545e20bfcde536f00e97ea51cee8`；
- A1/A2 raw RGBA、decoded PNG pixels、PNG bytes 与 dataURL 全部相同；
  seed-B1 保持 raw/decoded pixels 相同，同时 PNG bytes 与 dataURL 均与 A
  分离；三次 PNG signature/decode/240x120/image/png 均有效；
- A Profile boot 为 `0 -> 1 -> 2`，seed-B Profile 为 `0 -> 1`；三次
  Host/session/supervisor/browser identity 独立；
- A1/A2/seed-B1 的 `ctx.close()` 均成功，分别约 0.726 s、0.372 s、
  1.021 s；三次均 exit 0、exit file observed、Job active 0、无 remaining
  process、forced cleanup `not_needed`，两类 Profile lock byte 均可重取；
- 终态无 Camoufox/supervisor 残留，18191 无 listener，tracked worktree clean。

因此 close-bound candidate 的 **Canvas focused 子 Gate Accepted**，并直接证明
旧 full FP1 的 `ctx.close()` 失败在此 focused 路径未重现。报告仍正确保持
`verified:false`、`fullFp1Executed:false`、`fp1Accepted:false`、
`fp2Entered:false`。下一步是先冻结本次 focused evidence 与新的 full harness
绑定，再只执行一次 full FP1 A1/A2/full-B1；不得重跑 focused、不得进入 FP2。

### 2026-08-20 FP1 offline evidence closure

本次工作从 native Windows 工作树的精确只读基线开始：branch
`codex/camoufox-m3-engine-adapter`，HEAD
`b7a615ac39606deb741b3b3ea13d3584a987a39c`，tree
`9461adcc6924539dc4c2bb80963fab71a2efef49`，tracked 与 cached diff 均为空。没有
reset、restore、stash 或 clean，也没有启动浏览器。随后逐字节关闭当前合同、状态页、
ignored harness/test、三份 Artifact、probe、`0002`、source/asset lock、tree manifest、
archive、`camoufox.exe`、503-file extracted tree、focused/full report/sidecar/claim 与 full
A1/A2/B1 的 observed/session/protocol/stderr。闭包 receipt SHA-256 为
`2e5f46872f1108b037e55bfe8320e4b950d9c1fc3df2561e01a154d73af3986f`；503
文件共 981,205,753 bytes，canonical tree SHA-256
`42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1`，全部匹配。

裁决对象严格限定为 immutable run `fp1-full-20260818T103842543519Z`：

- report SHA-256
  `45a40123a1877e1c97d6989f5ba763c32b62a3b1c0e6f74968da13e957abd588`；
- report sidecar raw SHA-256
  `6d98e8df5382b2b2344636f9bcccacb03d765ce61dd2eb6a649d41dfae5e1f43`；
- one-shot claim SHA-256
  `f609ff5d72353bc820e69b509d5764a509a3f438a1967eddeea5d41bc3ff8e12`；
- offline adjudication receipt SHA-256
  `2494a0da712bf6598c774dc7b33657784d78226b05a92536e4984c211f51106d`。

ignored full comparator/harness 只做合同内最小修正，最终 raw SHA-256 为
`c7c6b90b01a02e46a83b934e50f6569261ab7ac25c037f26b805e8eef074c9d6`；无浏览器
test raw SHA-256 为
`c09ede5a2077099e7623aa56b504bf1eaa3ff2d4b279b03ceba7c204bc8e7a57`。
规则现为：A1/A2 六个 Canvas hash 全等；focused seed-only B 的 raw/decoded pixels
等于 A 且 PNG/dataURL/export 分离；full B 不要求 raw 无条件等于 A，只有 Artifact 的
`fonts` / `fonts:spacing_seed` 绘图输入及对应 observed font/width evidence 同时变化、
common families 无漂移时才允许 raw 改变；每次 PNG shape 与既有内部一致性继续硬验。
report、sidecar、claim 或任一 referenced-file hash mismatch 均 fail closed。

无浏览器验证结果为 full harness 21/21、focused harness 10/10、candidate finalizer 6/6；
其中显式覆盖 A1/A2 raw 漂移失败、focused seed-B raw 漂移失败、font-driven full-B raw
变化通过、无法解释的 common-family 漂移失败、invalid PNG 失败，以及
report/sidecar/claim/referenced hash mismatch 失败。

corrected adjudication 重新解析 immutable observed/session/protocol/stderr 后确认：A1/A2
全部 14 个 hard family（含六个 Canvas hash）相同；full B 的 13-key 静态 diff 精确映射，
Canvas raw/rawRgba/decoded pixels 同步变化，且只由已变化的 `fonts` 与
`fonts:spacing_seed` 及对应 injected-font/width evidence 解释，common families 稳定。
A boot `0 -> 1 -> 2`、B boot `0 -> 1`，A Cookie 持久、B Cookie/Profile 隔离；三次
media configured counts 均匹配；每个 protocol 均有四个成功 response 且 secret scan
干净；三次均 clean close、exit 0、Job active 0、forced cleanup `not_needed`。因此
lifecycle、storage、media 与 integrity 全部闭合。

最终 Gate 冻结如下：

- **Original runner verdict:** `failed`；
- **Reason:** false negative caused by non-contractual A/B `canvas.rawHash` equality assertion；
- **Main-brain FP1 Gate:** Accepted based on immutable original A1/A2/B1 evidence plus corrected contract adjudication；
- **verified:** `false`；
- **FP2:** 未进入；只允许作为下一项单独冻结的任务。

原始 report、failure code 与 claim 未修改，也没有生成伪装成原始 runner verdict 的新报告。
本 Gate 不改变 M3-WI、production package/signing、UI 或 shipped 状态。

### 2026-08-20 FP2 execution checkpoint

FP2 task contract frozen / execution in progress

### 2026-08-20 FP2 generation 1 Gate and Runtime Preflight Closure

FP2 generation 1 is **Blocked before first browser observation**. The preserved
generation-1 claim is `e77204a09d9dfdbdf7d6c3b00a96114f477fd5b93d01c7fa6a7fd3dd71b28402`
for run `fp2-20260820T121344Z-470b08fdb9`; the child used the wrong bare Python runtime,
which lacked `camoufox`, and no Host/browser/realm observation was produced. This is an
execution-environment precondition block, not a Camoufox realm-consistency failure.

The next authorized work is the no-browser **FP2 Runtime Preflight Closure**. It preserves
the generation-1 claim, fixes the claim-before-runtime-preflight contract boundary, fixes
the FP2 report finalization call to `ensure_sanitized(report, label)`, and freezes the
generation-2 child runtime as the repository FP1 venv (`CPython 3.12.13`, Camoufox 0.5.4,
Playwright 1.60.0, BrowserForge 1.2.4). No generation-2 claim or browser matrix is authorized
by this status update; FP3 remains closed and all FP2 results remain `verified:false`.

Runtime Preflight Closure implementation is now complete in the allowed FP2 runner/test and
contract scope. The exact child preflight and success/failed/blocked synthetic finalization
passed without browser launch; 18192 was free before and after, target processes were absent,
and no preflight lock was created. The resulting receipt remains gitignored and binds the
generation-1 blocked claim, runtime interpreter/dependency closure and child invocation. This
does not authorize generation-2 claim creation; that authorization remains a separate
main-brain decision. FP2 remains **Blocked**, FP1 remains **Accepted / verified:false**, and
FP3 remains **Closed**.

### 2026-08-21 FP2 generation-2 pre-claim Gate correction

The generation-2 pre-claim Gate correction is limited to the FP2 runner and its
no-browser regressions. Deterministic runtime preflight may be repeated before
claim creation; it must still stop before browser launch and cannot consume the
one-shot. Target-process cleanliness now uses the existing `tasklist.exe`
backend with an independent PowerShell `Get-Process` fallback. Access denied is
not treated as an empty process list; if all allowed backends are unavailable,
the result is `blocked: process_cleanliness_unverifiable`.

The generation-1 claim and evidence remain byte-preserved. Generation-2 claim
creation and browser execution have not started; all FP2 results remain
`verified:false`, and FP3 remains **Closed**.
