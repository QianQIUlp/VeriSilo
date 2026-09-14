# Managed Runtime Proof Repair & Environment Qualification — STATUS（2026-09-14）

## 判定与分类（Verdicts & Classifications）

- **Host Runtime Environment**: `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`
- **Alternate Environment (Windows Sandbox)**: `ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`
- **Overall Runtime Verdict**: `MANAGED_RUNTIME_ENVIRONMENT_BLOCKED`
- **Harness Code Status**: `REPAIRED_AND_VERIFIED`（代码与单元测试通过，符合 canonical baseline 入库标准）

---

## 一、背景与任务目标

2026-09-14 发现当前宿主机（Windows 11 25H2, Build 26200, UBR 9445）上冻结版 Camoufox 浏览器无法派生任何子进程（GPU、content、socket、RDD），导致真实运行时验收链（Window Geometry 验收及 Evidence Coverage Closure II 真实浏览器证明）受阻。
本任务在 `integration` 工作线按授权开展：
1. 诊断宿主机子进程崩溃原因并完成判别实验；
2. 比较验证 engine package 的字节完整性；
3. 探索并验证替代运行环境（Windows Sandbox）；
4. 修复 Evidence Coverage Closure II 的真实 runtime 验收 harness（`apps/camoufox-host/test_real_runtime_evidence_closure_ii.py`）；
5. 完整归档 QA 判定与证据。

---

## 二、基线与分支上下文

- **Canonical Baseline**: `origin/baseline/dev = 0d177a3e68657f048bf44a8690bb967c588f76c1`
- **Task Branch**: `agent/integration/runtime-proof-repair-3698b4`
- **Worktree**: `C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\integration-runtime-proof-repair-3698b4`
- **Prior QA Work Preservation**: 既有 Window Geometry QA 分支已发布至 `origin/agent/qa/accept-real-managed-e0f551`（commit `f0b00e16df093e4b48474bef82b66796d1629f83`），不推进 baseline，完整保留其复现证据。

---

## 三、Engine Package 字节一致性证明

对以下两个引擎包的 browser 树进行了全量逐文件 SHA-256 哈希比对：
- `artifacts/build/managed-browser/rc3-engine-package/browser`
- `artifacts/release/managed-browser/v0.1.0-rc2/engine-package/browser`

比对结果：
- 共有 504 个文件（含 `browser-tree-manifest.json`）；
- 全部 504 个文件的 SHA-256 哈希值精确一致；
- 对照 `browser-tree-manifest.json` 校验通过（503 files verified True）；
- 核心可执行文件 `camoufox.exe` SHA-256：`c5535c7c25c04b8bba90b9b3e64fca13fce01a52fcb5a1532130e99e4804b49e`，与 `runtime-asset-lock.json` 精确匹配；
- 结论：两个 package 之间的浏览器二进制树 **`BYTE_IDENTICAL`**。排除因 package 构建损坏或文件缺失引起的运行问题。

---

## 四、宿主机判别实验与诊断（Host Diagnostics）

### 1. Plain Launch 对照实验
在宿主机上使用独立 PowerShell（直接从 explorer 启动，无 Host、supervisor、Playwright 或 Juggler 介入），使用全新临时 profile 启动 `camoufox.exe`：
- **观察现象**：
  - 主进程启动存活；
  - 约 36 秒后出现窗口（1280×1040，由于 GPU 缺失回退至软件渲染）；
  - 全程子进程数量为 **0**（无 GPU 进程、无 socket 进程、无 tab/content 进程）。

### 2. 崩溃日志与根因定位
Windows 事件查看器（Application Error 日志）显示：
- 子进程在被创建瞬间立即崩溃；
- 崩溃异常代码：MSVC 异常 `0xc06d007e`（位于 `KERNELBASE.dll` 偏移 `0xc41ca`）；
- 语义确认为：**delay-load exception carrying `ERROR_MOD_NOT_FOUND` semantics**；
- 结论：宿主机当前环境（Windows 11 Build 26200）缺失某些动态链接依赖或延迟加载模块，导致 Gecko 子进程无法正常启动。
- 宿主机判定：**`CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`**。

---

## 五、Windows Sandbox 替代环境预检（Alternate Environment Qualification）

在不进行破坏性 OS 操作的前提下，利用原生 Windows Sandbox 进行无污染隔离预检：
- **Guest OS**: Microsoft Windows 11 Enterprise, Build `26100`, UBR `9445`, AMD64；
- **用户环境**: `WDAGUtilityAccount`（`IsAdministrator = true`）；
- **Plain Launch 测试**：
  - 将 `rc3-engine-package` 映射入沙箱，在全新临时 profile 下执行 `camoufox.exe`；
  - **`childProcessesSpawned: true`**；
  - 浏览器主进程成功派生出 **7 个子进程**（GPU process、tab content process、socket process、utility/RDD 进程）；
  - 0 个崩溃，窗口在 2-3 秒内正常展现。
  - **该结果决定性地证明：冻结版 Camoufox 浏览器二进制树完好无损，在健康的 Windows 11 (Build 26100) 环境下完全具备派生子进程与硬件加速的能力。**

### 工具链受阻分析
- Windows Sandbox 属于全新干净（pristine）系统镜像，系统内未预装 Python 或 Playwright；
- 尝试通过沙箱映射主机 40GB 主工程目录时，由于 volume projection 超时而失败；
- 根据任务约束，禁止在沙箱内随意安装未受管的系统软件或构建临时发行包；
- 替代环境判定：**`ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`**。
- 整体运行环境判定：**`MANAGED_RUNTIME_ENVIRONMENT_BLOCKED`**。

---

## 六、Real-Runtime Acceptance Harness 修复（Harness Repair）

文件：`apps/camoufox-host/test_real_runtime_evidence_closure_ii.py`

### 修复内容：
1. **信号检索契约对齐**：
   - 修复前直接使用字典索引 `evidence.get("headers.Accept-Encoding")`，与后端返回的 `identityEvidence.signals` 数组格式冲突；
   - 修复后实现 `signal_by_name(evidence, name)` 遍历 `signals` 数组；
   - 规范信号名称为 `acceptEncoding`（与 `packages/contracts` 及 Host 对齐）。
2. **严格显式参数化**：
   - 引入 `--package-root` 命令行参数与 `VERISILO_PACKAGE_ROOT` 环境变量支持；
   - 未指定时直接 fail-closed 并以 exit code 2 退出，杜绝隐式回退或静默猜测。
3. **完整 Provenance 记录**：
   - 输出完整的 `packageRoot`、`browserExecutableSha256`、`browserTreeManifestSha256`、`runtimeAssetLockSha256`、`hostSourceTreeSha` 以及 `repoSha`。
4. **规范 Artifact Provisioning**：
   - 废弃过时的 `artifact-cold1.json`（原配置使用 `fontMode=inherit`）；
   - 使用与当前产品规范完全一致的 `provision_artifact`（`preset="balanced-en-us"`, `fontMode="managed"`, `window=[1280, 800]`）。
5. **单次积极验收循环与烟雾门禁**：
   - 优化为单一积极验收循环（launch -> 验证 WebGL2 与 Accept-Encoding -> fresh reobserve 对账 -> 优雅关闭 session）；
   - 增加 Managed Font Masking 运行门禁（`state == "running"`）。

### 验证记录：
- `python apps/camoufox-host/test_evidence_coverage_closure_ii.py`: **10/10 PASS**
- `python apps/camoufox-host/test_package_contract.py`: **PASS**
- `python apps/camoufox-host/test_page_command.py`: **PASS**
- 缺少参数时 fail-closed 退出码验证：**ExitCode = 2**

---

## 七、受影响门禁状态汇总

| 门禁 / 验收项 | 状态 | 说明 |
|---|---|---|
| Window Geometry Coherence | `source fix closed / real-browser post-fix acceptance pending` | 源码修复在基线中闭合，受宿主机子进程崩溃阻断，QA 证据已归档至 `origin/agent/qa/accept-real-managed-e0f551` |
| Evidence Coverage Closure II | `source fix closed / harness repaired / real-runtime acceptance pending` | 源码闭合，Harness 已修复并通过规范单元测试；真实运行时验收待环境就绪后执行 |
| Current Host Environment | `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN` | Windows 11 Build 26200 宿主子进程报 `0xc06d007e` 延迟加载崩溃 |
| Windows Sandbox Toolchain | `ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED` | Build 26100 沙箱派生子进程正常，但纯净镜像缺失 Python/Playwright 测试工具链 |
| Overall Managed Runtime | `MANAGED_RUNTIME_ENVIRONMENT_BLOCKED` | 待宿主机环境恢复或具备自包含工具链的干净镜像就绪后解锁 |

---

## 八、恢复运行指南（Resume Guide）

一旦具备可用且具备测试工具链的环境（或宿主机的 delay-load 问题排查解决）：
1. 运行 real runtime acceptance：
   ```bash
   python apps/camoufox-host/test_real_runtime_evidence_closure_ii.py --package-root artifacts/build/managed-browser/rc3-engine-package
   ```
2. 运行 Window Geometry runtime acceptance：
   ```powershell
   & "apps/camoufox-host/.venv/Scripts/python.exe" docs/qa/window-geometry-runtime-acceptance/tools/plain_launch_desktop.py
   ```
