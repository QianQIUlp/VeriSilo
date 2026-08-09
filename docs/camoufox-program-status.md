# Camoufox Managed Engine 当前状态

- 状态：**可变项目状态页**
- 更新日期：2026-08-09

本文只记录当前执行阶段、证据 checkpoint 和下一项任务。长期产品意图见[身份平台北极星](identity-platform-north-star.md)，路线原因见[Camoufox-first Managed Engine 决策](camoufox-managed-engine-decision.md)。每次 Gate 变化后更新本文，不用本文反向改写长期决策。

## Git 状态

| 对象                      | 当前值                                               | 含义                                                    |
| ------------------------- | ---------------------------------------------------- | ------------------------------------------------------- |
| `origin/main` 基线        | `527516ab65f49061ccac67287fa96fa93f006421`           | 当前主线尚未包含 Camoufox M0–M2.0.3                     |
| Camoufox 分支             | `codex/camoufox-m0-m2-minimal`                       | 自包含的 Linux Managed Engine 垂直切片                  |
| M2.0.3 代码 checkpoint    | `3b53830`                                            | 严格进程树、quarantine、JSON 和 RFC3339 收口            |
| Linux accepted checkpoint | `d596afd76e59ba64915b036fbc732a2c28f1ec54`           | evidence manifest 冻结提交                              |
| Camoufox Draft PR         | [#10](https://github.com/QianQIUlp/VeriSilo/pull/10) | 目标为 `main`；未合并前不能称为主线或 shipped 能力      |
| 上下文文档 Draft PR       | [#11](https://github.com/QianQIUlp/VeriSilo/pull/11) | 本页所在的项目事实源 PR；必须先合并，再建立 M2-W 新起点 |

`d596afd` 是已经接受的 Linux 证据 checkpoint，不应因后续文档合并或 Windows 分支建立而改写。新的 M2-W 执行基线必须在本上下文落库 PR 合并、更新后的 `main` 合入 Camoufox 分支后单独记录。

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

## Accepted evidence

| 项目              | 证据                                                                 |
| ----------------- | -------------------------------------------------------------------- |
| Artifact 单元测试 | `21/21`                                                              |
| Host 集成测试     | `19/19`                                                              |
| Stability         | `run-1786158540-228a3340`，identity-a 5/5 相同 ObservedWebsiteDigest |
| Separation        | `run-1786158560-43ea2bd1`，A/B/C 两两不同                            |
| Tamper            | `run-1786158573-fef25c08`，四类篡改拒绝                              |
| M0 recheck        | `run-1786158578-047efb35`，三周期持久化、退出码 0                    |

原始 Profile 和完整运行报告位于执行环境的 gitignored `artifacts/`；分支中 tracked evidence manifest 是脱敏索引。上述结果是该 Linux 主机上的 accepted execution evidence，保持 `verified: false`。

## 当前 Gate

| Gate                                            | 状态                                    |
| ----------------------------------------------- | --------------------------------------- |
| Linux 资产固定、Artifact 重放与 standalone Host | **Accepted，M0–M2.0.3 关闭**            |
| 原生 Windows M2-W                               | **下一阶段；上下文文档 PR 合并前等待**  |
| EngineAdapter / Tauri 集成                      | **不允许；必须等待 M2-W 三项核心 Gate** |
| Managed Identity UI、代理联动、生产打包         | **后续阶段**                            |

## 文档 PR 合并后的衔接顺序

1. 先把“项目事实源与防漂移文档” [Draft PR #11](https://github.com/QianQIUlp/VeriSilo/pull/11) 合入 `main`；合并前不启动 M2-W，也不通过复制旧聊天绕过仓库事实源。
2. 更新本地 `origin/main`，再以 **merge** 方式合入 `codex/camoufox-m0-m2-minimal`；不 rebase、不改写 M0–M2.0.3 或 `d596afd` 的证据历史。
3. 在 Camoufox 分支追加一个小型状态同步提交，记录合并后的 M2-W 起始 commit，并推送更新 Draft PR #10。
4. 重新生成主脑接手提示词和 Windows 执行提示词。提示词只携带当前任务和摘要，完整上下文由本仓库四份事实源承担。
5. 只有上述新起点冻结后，才在原生 Windows 开始 M2-W；旧 `d596afd` 仍是 Linux accepted checkpoint，但不再作为 Windows 任务的直接起点。

下面的“M2-W 冻结目标”只定义阶段目标，不是完整执行任务合同。正式提示词还必须在新起始 commit 产生后冻结验收矩阵、关键失败反例、证据格式和停止条件。

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
