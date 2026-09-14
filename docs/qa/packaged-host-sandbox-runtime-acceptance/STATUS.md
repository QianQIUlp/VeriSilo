# Packaged-Host Windows Sandbox Runtime Acceptance — STATUS（2026-09-14）

## 判定与分类（Verdicts & Classifications）

- **Host Runtime Environment**: `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`
  - 宿主机（Windows 11 Build 26200）Gecko 子进程遇 MSVC `0xc06d007e` 延迟加载模块缺失异常崩溃，无法原生派生子进程。
- **Alternate Environment (Windows Sandbox)**: `ALTERNATE_ENVIRONMENT_ACCEPTED`
  - 原生 Windows Sandbox（Windows 11 Enterprise Build 26100, UBR 9445）环境健康，支持完整多进程树派生与硬件加速。
- **Packaged Host Runtime Acceptance**: `CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`
  - 基于当前源码构建的自包含 Dev Engine Package 在健康 Windows Sandbox 内通过全部 4 个真实浏览器会话验证与进程树清理。
- **Overall Verdict**: **`PASS`**

---

## 一、任务背景与验收目标

上一阶段（`managed-runtime-proof-repair`）确立了宿主机环境阻断判定（`CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`），并在纯净 Windows Sandbox 镜像中确认了冻结浏览器二进制完好具备多进程派生能力，但由于沙箱缺少受控测试工具链而处于 `ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`。

本任务在 `integration` 工作线完成运行时证明链的最小闭环：
1. 基于当前 canonical baseline（`521ae2496a90fd45d001e6b2864f20e67ea4586e`）现编 Supervisor，生成 source-bound 唯一的独立 Dev Engine Package（不覆盖历史 `rc2`/`rc3`，禁止代码签名）；
2. 构建自包含 Windows Sandbox 自动化验收驱动（PowerShell 原生命名管道 IPC + 二进制/JSONL 独立通道）；
3. 在健康 Windows Sandbox 隔离容器中运行 4 阶段真实浏览器验收矩阵，完整采集安全证据并落盘。

---

## 二、基线、工件与代码 Provenance

- **Canonical Baseline**: `521ae2496a90fd45d001e6b2864f20e67ea4586e`（`origin/baseline/dev`）
- **Integration Task Branch**: `agent/integration/packaged-host-windows-6ea6c8`
- **Worktree**: `C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\integration-packaged-host-windows-6ea6c8`
- **Dev Engine Package**: `artifacts/qa/dev-engine-package-521ae24`
  - Package Tree SHA: `5dc9e86333ea6fb5a8eeb4a5e2f7596ff1842eb412613d9f37c35eb85aa882c1`
  - Browser Tree Manifest SHA: `d77002d0f872a1ca57675d9b3bc2f9d88769e406d2d082cd94b07d78c9f075e6`
  - Host Executable (`camoufox-host.exe`) SHA: `3636f3223067ddad6d0df2c71f5cebb6dfc924bc98ec6d56d814be953a992fb2`
  - Windows Supervisor (`verisilo-camoufox-supervisor.exe`) SHA: `d9c2e0c7c1a5efab1bfb4a89210bd37ad59342bc80e54bf2c72394a8602fad87`（现编 Release，附带构建源自当前工作区）
  - Classification: `UNSIGNED_DEV_ENGINE_PACKAGE`（严格禁止代码签名，未查询或 mock 签名凭据）

---

## 三、Windows Sandbox 验收矩阵执行结果

沙箱验收脚本 `scripts/run-sandbox-acceptance.ps1` 驱动原生 Windows Sandbox 自动映射输入并执行，全过程耗时 157.3 秒，输出结果汇总于 `results.json`：

### 1. Session 1: 主功能验收与 Evidence Coverage Closure II（1280x800）
- **Artifact Provision**: `identity-86e45f432062e595dc257f24`（preset: `balanced-en-us`，window: 1280×800，fontMode: `managed`）
- **Spawn Time**: 31.8s（含 supervisor、Gecko 主进程与 7 个子进程）
- **Signals**:
  - `webgl2Vendor`: `Google Inc. (Intel)`（`state: matched`，与 Artifact 声明一致，**PASS**）
  - `webgl2Renderer`: `state: unavailable`（`unavailableCorrect: true`，严格符合 contract，**PASS**）
  - `acceptEncoding`: `gzip, deflate, br, zstd`（`state: matched`，与 Host 契约一致，**PASS**）
- **Window Geometry Invariants**:
  - `screenX == screenLeft`: True（0 == 0）
  - `screenY == screenTop`: True（0 == 0）
  - `screen.availLeft <= screenX`: True（0 <= 0）
  - `screenX + outerWidth <= availWidth`: True（0 + 1166 <= 1280）
  - `screen.availTop <= screenY`: True（0 <= 0）
  - `screenY + outerHeight <= availHeight`: True（0 + 632 <= 800）
  - 几何不变量判定：**PASS**
- **Fresh Recheck & Page Sentinel 对账**:
  - 启动页首屏稳定后注入 Page Sentinel Token：`window.__verisilo_sentinel`
  - 发送 `reobserve_identity` 命令：重新读取磁盘 Artifact、启动临时 loopback 探针页完成独立对账并关闭临时页
  - 对账后检验用户当前页面状态：
    - `timestampAdvanced`: True（`2026-09-14T12:04:51.782551Z` > `2026-09-14T12:04:38.588065Z`）
    - `sentinelIntact`: **True**（Sentinel Token 保持未损）
    - `urlIntact`: **True**（用户活跃页面 URL 保持未变）
    - `webgl2VendorReobserved`: **True**
    - `webgl2RendererReobserved`: **True**
    - `acceptEncodingReobserved`: **True**
  - Session 1 综合判定：**PASS**

### 2. Session 2: 冷重启与 Profile 复用（1280x800）
- **Profile Reuse**: 复用 Session 1 的 Profile 目录 `profile-1280-a` 与 Artifact 进行冷重启；
- **Window Geometry**: 观察到 `outerWidth=1166, outerHeight=632, screenX=0, screenY=0`；全部 6 项几何不变量判定 **PASS**；
- 优雅关闭并退出，Session 2 综合判定：**PASS**。

### 3. Session 3: 第二尺寸几何相干性（1024x768）
- **Artifact Provision**: `identity-01d09cbe572a2405074aaba2`（window: 1024×768）；
- **Window Geometry**: 观察到 `outerWidth=1024, outerHeight=632, screenX=0, screenY=0`；全部 6 项几何不变量在不同尺寸屏幕下完全满足，无虚假 clamp 失败；
- Session 3 综合判定：**PASS**。

### 4. Session 4: Legacy Artifact 坐标修正与磁盘文件不可变性
- **Staged Legacy Artifact**: 预先由宿主 Python `compute_artifact_digest` + `write_artifact_with_sidecar` 生成合规 Legacy Artifact `identity-f49f1202d9ac4d79f991f794`，注入严重越界坐标 `screenX: 2232, screenY: 140`；
- **Launch & Clamping**:
  - 启动前磁盘文件 SHA-256：`2edabe511a057dc165a5c469df697b4ecaaf2cdbc134700f2d6cf1a0c653e103`；
  - 浏览器启动并在交互模式（`VERISILO_INTERACTIVE=1`）下自动修正窗口坐标：
    - 实际观察到：`screenX = 114, screenY = 140`
    - 计算虚拟屏幕可用最大边界：`maxX = 1280 - 1166 = 114`
    - `screenX` 成功从越界的 `2232` 动态裁剪收敛至虚拟屏幕边界内 `114`；
    - 6 项几何不变量判定 **PASS**；
  - 进程优雅退出后重新读取磁盘文件 SHA-256：`2edabe511a057dc165a5c469df697b4ecaaf2cdbc134700f2d6cf1a0c653e103`；
  - `artifactFileUnchanged: True`（修正仅在运行时生效，磁盘 Artifact 与 sidecar 字节分毫不变）；
- Session 4 综合判定：**PASS**。

### 5. Cleanup: 进程与锁清理校验
- 4 轮会话结束后等待 2 秒进行沙箱内残余进程排查：
  - `camoufox.exe` 残留数：**0**
  - `camoufox-host.exe` 残留数：**0**
  - `verisilo-camoufox-supervisor.exe` 残留数：**0**
- Profile 锁与 Job Object 正常释放，Cleanup 判定：**PASS**。

---

## 四、安全与数据隔离保证

1. **零机密外泄**：
   - 临时 Profile、Cookie、Sqlite 缓存、临时 Artifact 均存放于沙箱本地临时目录（`C:\Users\WDAGUtilityAccount\AppData\Local\Temp\verisilo-acceptance-*`），并在测试结束前完全清除；
   - 映射回宿主目录（`docs/qa/packaged-host-sandbox-runtime-acceptance/`）的文件仅包含安全摘要与结构化观测结果（`results.json`、`geometry-observation.json`、`identity-evidence-summary.json`、`environment.json`、`package-provenance.json`、`driver.log`、`host-stderr.log` 以及各阶段 sentinel），无 seed、无 raw cookie、无个人数据。
2. **零宿主环境破坏**：
   - 未在宿主系统安装全局包、未修改宿主系统字体或注册表。
3. **未开 RC / Release Gate**：
   - 本次产物仅用于 QA / Integration Acceptance，不修改 UI、不触碰生产打包、不开 release gate。

---

## 五、最终结论与证明链状态

| 门禁 / 证明项 | 状态 | 证明事实 |
|---|---|---|
| Window Geometry Coherence | **`ACCEPTED_IN_WINDOWS_SANDBOX`** | 1280x800、1024x768、冷重启与越界 Legacy Artifact 四重场景在真实浏览器中通过全部不变量与动态裁剪检验 |
| Evidence Coverage Closure II | **`ACCEPTED_IN_WINDOWS_SANDBOX`** | WebGL2 Vendor/Renderer 与 Accept-Encoding 信号、Managed 字体屏蔽、Page Sentinel 与 URL 保持性在真实浏览器运行与重观测中完全验证 |
| Current Host Environment | `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN` | Windows 11 Build 26200 宿主环境已知故障保留记录 |
| Windows Sandbox Environment | **`ALTERNATE_ENVIRONMENT_ACCEPTED`** | 原生 Windows Sandbox Build 26100 成功通过全套运行时验收 |
| Packaged-Host Runtime Proof | **`CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`** | 当前源码打包之 Host 在健康 Windows 环境中完整闭环 |
