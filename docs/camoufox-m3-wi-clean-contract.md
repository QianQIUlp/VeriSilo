# Clean M3-WI 原生 Windows Desktop / Real Host 合同

- 状态：**Attempts 1–2 immutable Failed；Attempt 3 corrected input frozen**
- 冻结日期：2026-08-28
- 执行分支：`codex/camoufox-m3-engine-adapter`
- definition 基线：`02e5bad7962aabcf44839e3a69c7c5d21d7b7927`
- 历史边界：[旧 M3-WI 合同与调查](camoufox-m3-wi-windows-task.md)保持 Failed / Inconclusive，
  不复活其 1–16 矩阵、R1/R2/R2H runner、freezer、schema 或 evidence manifest。

本文只冻结 clean M3-WI 的一个新增问题、输入、直接判据和停止条件。产品方向与既有事实分别以
[身份平台北极星](identity-platform-north-star.md)、
[Camoufox-first 决策](camoufox-managed-engine-decision.md)和
[当前状态页](camoufox-program-status.md)为准。

## 唯一产品问题

在原生 Windows 上，桌面后端现有 `RuntimeManager` 能否通过 test-only exact
`EngineAdapter` plan、现有 `camoufox-host-jsonl-v1` transport 和真实 `host_v1.py` 实现，驱动已经
通过 FP2/FP3/FP4 的 Formal-v3 Engine 与 Artifact v6，在同一 Silo / Persistent Profile 上完成
两个顺序的 `launch → running → clean stop` 周期，并保持 Artifact、Engine、Network Policy 与
Runtime Evidence 的绑定和层级诚实，且不把独立 `IdentityTemplate` 声明冒充 Artifact 已应用事实？

这是 desktop backend integration Gate，不是 UI、production package、签名、installer 或 release
Gate。即使 Passed，也必须保持：

- `productionPackageVerified=false`、`shipped=false`、`verified=false`；
- `packageVerification=not_requested`、`hostLaunch=observed`、`verifiedAdapter=null`；
- Host 自报最多支持对应 `applied/observed`，不能提升为 `verified`。

FP4 是 fingerprint qualification 的最后一个 Gate，只回答冻结候选能否完成代表性 ordinary-site
核心任务。它不证明“不可检测”、universal compatibility、commercial product readiness 或 desktop
integration。FP4 Passed 因此支持继续 Camoufox-first，但不替代本 Gate；这里不创建 FP5，也不重跑
live-site matrix。

## Identity authority 与执行前置条件

Camoufox 的唯一运行时身份权威是 **Resolved Identity Artifact**。BrowserForge/桌面模板最多产生候选
或策略输入，Host 实际接收并应用的是 exact Artifact ID、raw SHA 与 schema；`IdentityTemplate` 不是
第二份可独立生效的身份。

当前 owning seam 尚未守住这条边界：`SiloEngineConfig::Camoufox` 分别校验 Template 与 Artifact
binding，未建立二者的语义绑定；Host launch plan 又从 Template 配置 identity capabilities，却只把
Artifact binding 交给 Host。Host 进入 running 后，`apply_camoufox_host_capability_evidence` 目前会用
一条通用 running 证据把全部 Template-derived capability 从 `configured` 提升为 `applied`。冻结的
FP3 Template 甚至声明 `1920×1080` screen，而 exact Artifact 声明 `4300×1800`，因此这不是理论风险。

native attempt 开始前必须先做一个最小本地修正并通过 focused test：

- generic Host running evidence 只能把直接绑定的 `ProfileIsolation` 从 `configured` 提升为
  `applied`；
- `IdentityTemplate`、UA/UA-CH、language/timezone、screen、Canvas、WebGL、fonts、media 与 realm
  capabilities 保持 `configured`，除非以后有逐项 Artifact-aware direct evidence；
- required proxy 的 `configured/reachable/applied` 仍由独立 Network Evidence seam 给出，不能混入
  identity capability；
- `hostLaunch=observed`、`verifiedAdapter=null` 与 `verified=false` 保持不变；Stock 与 Controlled
  Chromium 路径不变。

focused test 只需证明 Profile binding 可升格、Template-derived identity claims 不随 Host running
升格。这里不新增 Artifact parser、Template→Artifact projection/schema 或通用 evidence framework；
若未来产品仍需要 Template 成为可见身份输入，应另行删除双重权威或建立显式可验证 projection。

## 已接受并直接复用的证据

- M3-0 Accepted checkpoint `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd` 已关闭 fake Host 的
  package/transport/fail-closed/evidence 合同，不证明真实浏览器。
- FP3 authoritative result
  `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp3-result.json`
  SHA-256 `8a821eca7b9e11716668d6742ac356743b7438ab2b9a7ca8b0d604264be86e62`
  已直接证明一次 native Windows `RuntimeManager → required FixedProxy → real Host → Formal-v3`
  周期、外部出口/Geo/ICE 观测与 clean stop，`verified:false`。
- FP4 authoritative result
  `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp4-result.json`
  SHA-256 `14c7de3a8a14b8037cf0e16ec7b5dc213294b68050665a57513dea79efd8f2de`
  已直接证明当前 Host/Engine 的两个顺序生命周期、Profile replay 与冻结 V5 ordinary-site matrix，
  `verified:false`。
- FP3 terminal code revision 到 definition 基线之间，`engine.rs`、`launcher.rs`、
  `launcher_m3_wi_windows_tests.rs`、`domain.rs`、`lib.rs` 与 `host_v1.py` 无代码差异；FP4 后的
  `run_spike.py` 行为由上述 FP4 result 覆盖。

因此 clean M3-WI 不重跑 FP2 cross-realm、FP3 外部出口/Geo/ICE 或 FP4 live-site matrix。新增且仍
未被同一条证据直接覆盖的只有：**桌面 evidence 不再错误升格 Template claims，以及当前最终输入
经过桌面组合后，第二个全新 RuntimeManager / Host 周期能否继续成功并干净释放。**

## 冻结输入

- 平台：原生 Windows interactive desktop；Linux、WSL、Wine、容器结果不能替代。
- Silo：ID `74444444-4444-4444-8444-444444444444`，seed reference
  `75555555-5555-4555-8555-555555555555`，同一 `SiloEngineConfig::Camoufox` 与同一 run-owned
  Profile；Phase B 必须新建 `RuntimeManager` 与 Host child，不能复用 Phase A 进程。
- Compatibility Template：精确复用 FP3 的下列 `IdentityTemplate`。它只满足现有 adapter validation
  并产生 `configured` policy/capability input，不是运行时身份权威；整个值由本合同 SHA 冻结：

```json
{
  "schemaVersion": 1,
  "templateId": "73333333-3333-4333-8333-333333333333",
  "os": { "family": "windows", "version": "11", "architecture": "x64" },
  "browser": {
    "family": "firefox",
    "majorVersion": 152,
    "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:152.0) Gecko/20100101 Firefox/152.0",
    "uaCh": null
  },
  "languages": { "primary": "en-US", "accepted": ["en-US"] },
  "timezone": "Asia/Hong_Kong",
  "screen": {
    "width": 1920,
    "height": 1080,
    "availableWidth": 1920,
    "availableHeight": 1040,
    "devicePixelRatio": 1.0,
    "colorDepth": 24
  },
  "render": { "canvas": "native", "webGlVendor": null, "webGlRenderer": null },
  "fonts": { "families": ["Segoe UI"] },
  "media": { "microphones": 1, "cameras": 1, "speakers": 1, "labelsExposed": true },
  "network": {
    "proxyRequired": true,
    "countryCode": "HK",
    "timezone": "Asia/Hong_Kong",
    "locale": "en-US",
    "desiredQuic": "browser_default"
  }
}
```

- Resolved Artifact：
  `artifacts/camoufox-fp3-1b-attempt-7/identity-fp3-1b-formal-v3-a.json`，SHA-256
  `8a4cd0d10a0a456678d1f3b4beb1515195d5d171742c4695c2d909132a26e722`；sidecar file SHA-256
  `e027eb101fa2783adbc697fa8b47a339e7d66bf00170eacdca7b71a8983f8b86`，内容也必须精确匹配。
- Engine：`verisilo-camoufox-152.0.4-beta.28-r1-formal-v3`；runtime asset lock
  `artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-runtime-asset-lock.json`，SHA-256
  `81e73a69347272d0b770bfa3c9b3eb07449bb165efb0c16948eece2e5a0678ce`；browser-tree manifest
  raw / canonical SHA-256 分别为
  `8434ab9925bf0f7d95cc4ff06fe94b7dcf9963a0691f37638469d68cda58ace2` /
  `68d78d0f414d90545691560858b46ed179ee163b7258306c44f0d850bcde6204`；`camoufox.exe`
  SHA-256 `b147602826db5bf852e5777f56cd56036dc04e8ea8868a8e55f8b08744f142a6`。
- Host source：`apps/camoufox-host/host_v1.py`，SHA-256
  `b3b313d4cf6d2eaadceaff4320e5a6bb8afb5d39212652b2c51474eb6809aad0`。Attempt 3 通过
  `apps/camoufox-host/run_m3_wi_clean_host.py`（SHA-256
  `2015e91bf0902cc6b7276aadb6e8589ca728eb0dc11791a457d0b9744bae5ee8`）进入；该薄入口只复用已存在的
  `apps/camoufox-host/run_fp3_1b_windows.py` Formal-v3 exact lock/tree 校验 hooks（SHA-256
  `73a4fe9b20a95588d8bd03335aeffddf2a93b53cd0ddf9c24301ea99d6437785`），然后恢复基础
  `host_v1.CamoufoxHost`，不使用 FP3 的 `FP3ManagedHost`，因此不执行出口、Geo、Geolocation 或
  ICE/STUN 观察。Python dependency lock
  `apps/camoufox-host/uv.lock`，SHA-256
  `41f63b2c12c3102573266b4d9ac002fbd29f7f95cc3d291b8a41d09e411f8f6f`。
- Network Policy：required unauthenticated SOCKS5 `127.0.0.1:7897`。执行前只做一次
  `Test-NetConnection 127.0.0.1 -Port 7897`；失败则不创建 attempt、不启动浏览器。
- 尝试：`clean-m3-wi-attempt-1` 与 `clean-m3-wi-attempt-2` 保持 immutable Failed；修正后的新
  code/input 使用 `clean-m3-wi-attempt-3`。这不是 retry/recovery Gate，不引入 delay、sample
  rotation、cache workaround 或 site fallback。

Attempt 3 沿用本 Gate 已取得的 native Windows browser authorization；本 Gate 不需要也不访问出口 IP、Geo、
Geolocation、ICE/STUN 或 ordinary-site 外部检查服务。

## Attempts 1–2 直接结论与修正边界

Attempt 1 的 `run-report.json` 与 `native-evidence.json` 保持不可变 Failed。它在 Phase A 的 Host hello
前直接失败，浏览器没有启动。独立 hello-only 诊断证明基础 Host 的 `run_spike` asset allowlist 只接受
official/旧 canvas-v1 lock，因而拒绝本合同冻结的 Formal-v3 runtime lock；该 `SystemExit` 写入已被
launcher 丢弃的 OS stderr，最终只表现为 stdout EOF。工程 Gate 仍 open。

Attempt 2 通过了上述 Formal-v3 Host 准备并返回了可解析 hello，但在发送 `launch` 前被严格 root binding
拒绝，浏览器仍未启动。直接探针证明 clean test 的 Rust `join("runtime/app")` 保留 `/`，而 Python
`Path.absolute()` 将同一路径正规化为 `\`；validator 对三个 typed roots 做精确字符串比较，因此必然
不相等。

Attempt 3 只把该 test-owned root 改为逐组件 `join("runtime").join("app")`。clean-only entrypoint 仍只
复用 FP3 已有的 Formal-v3 严格 lock/tree 验证并恢复基础 `CamoufoxHost`；它不扩大共享 `run_spike`
trust allowlist，不改变 Profile、Artifact、Engine 或 Network 生命周期，也不复用 FP3 外部网络采集。

## Owning seam

```text
RuntimeManager::launch_with_identity_deriver
→ cfg(test, windows) exact Camoufox adapter plan
→ desktop ProxyRelay + bind_camoufox_host_proxy
→ spawn_engine_child / spawn_camoufox_host
→ CamoufoxHostTransport hello / launch / status
→ clean-only Formal-v3 input binder → real host_v1.py / base CamoufoxHost
→ exact Formal-v3 browser tree

RuntimeManager::stop_managed_camoufox
→ Host close / shutdown
→ exact Host child exit
→ relay + Profile lease release
```

Test-only adapter 只替代尚不存在的受信 Host package / signer 输入。它必须生成 shell-free、严格 typed
plan，`package_verification=None`，且不得进入 non-test build。plan 形成后必须走现有 production
`RuntimeManager`、relay、transport、binding validation 与 stop path；direct Python driver 不能冒充
本 Gate。

## 唯一执行序列

1. 从 clean synced Attempt 3 implementation commit 创建一个 run-owned root，复制精确 Artifact 与 sidecar；
   记录 code revision/tree/origin、合同 SHA、Python path/version 和全部冻结输入 hash。
2. Phase A：新 `RuntimeManager` 启动同一 Silo；要求 `Running`、exact Artifact/Profile/Engine binding、
   Host status 中唯一 relay URI、`ProfileIsolation=applied`，Template-derived identity capabilities
   仍为 `configured`；desktop network evidence 为 `configured/reachable/applied`，而 exit/DNS/WebRTC
   保持 `not_requested`；Vault identity deriver 未调用。
3. Phase A clean stop：Host `close/shutdown` 成功、exit code 0、`processTreeExit.exited=true`、Job
   active count 0、无 quarantine/forced cleanup、exact child 退出、relay 关闭、Profile lease 释放。
4. Phase B：从同一 app root 新建另一个 `RuntimeManager` 与不同 Host PID，使用同一 Silo、Artifact、
   Profile 和 Network Policy；要求 boot count `1 → 2`、managed cookie 与
   `observedWebsiteDigest` continuity 成立。
5. Phase B 同样 clean stop；两个周期的全部 owned PID 归零，没有操作无关进程，没有 secret/token
   进入 argv、wire、runtime activation、Host state 或 evidence。

只新增一个 Windows-only ignored focused test（可放在现有私有测试模块或相邻 clean 模块）和保存上述
直接 evidence 所需的最少代码。不得执行旧 soak、failure matrix、desktop-drop、crash、EOF、concurrent
或十周期场景；这些不是当前新增不确定性。

## 裁决

### Passed

执行前 focused evidence-semantics test 与上述五步全部直接成立，输入/hash/evidence receipts 完整，
两个生命周期 clean，且所有诚实边界保持 `verified:false`。结论只能是 **clean M3-WI integration
slice Passed on this native Windows host**。

### Failed

focused test 未通过表示实现前置条件尚未满足：不创建 attempt、不生成 Failed result，工程 Gate 继续
保持 open。冻结输入可用且 attempt 已开始后，任一 RuntimeManager/adapter/relay/JSONL Host binding、
第二周期、Profile continuity 或 clean-stop 必需判据直接失败，才裁决该 attempt 为 Failed。Failed
attempt 保持不可变；找到新且有证据的根因后，任何修复都构成新 code/input 与新 attempt，不重写
任何 Failed attempt，也不创建 recovery Gate。

### Inconclusive

attempt 已开始但 code/input binding、原始 evidence 或环境归因不足以裁决；不能把缺失证据当作 Passed。
执行前 SOCKS5 端口不可达属于未满足输入，此时不创建 attempt。

## 最小 evidence 与停止条件

一个 attempt 只保存：

- `native-evidence.json`：两阶段直接 observations、bindings、lifecycle、PID/Job/relay/lease 与诚实 evidence
  state；
- `run-report.json`：输入/hash、执行命令/exit、native evidence SHA、adjudication 与 limitations；
- 两个 JSON 的 SHA-256 sidecar，以及解释直接失败所必需的 bounded stderr；不创建新 schema/freezer/
  manifest framework。

Definition/refreeze 在本合同与状态页通过 focused 文档/hash/diff 检查、形成 clean commit 并推送后即
闭合。随后停止；下一任务是实现这一个 focused test 并在新的显式授权下执行 native attempt，不提前
声称 M3-WI Passed、production package verified、Managed Identity shipped 或 `verified:true`。
