# Window Geometry Runtime Acceptance — VERDICT: ENVIRONMENT_BLOCKED（2026-09-14）

## 判定

**ENVIRONMENT_BLOCKED** —— 非修复失败、非方法失败：本机（Windows 11 25H2, build 26200）
自 2026-09-13 起无法让冻结版 Camoufox 浏览器派生任何子进程（GPU/content/socket/RDD 全部），
任何真实 runtime 几何观测都无法进行。测试矩阵（1280×800 / second-size / legacy clamp /
cold restart）未执行，无 PASS/FAIL 结论。

关键判别实验：**plain 启动**（explorer 祖先 PowerShell、无 Host/supervisor/Playwright/Juggler、
全新 temp profile）复现同一症状 —— 主进程存活、36 秒后窗口出现（1280×1040，软件渲染回退）、
**零子进程**。产品源码与几何修复完全不在启动路径上，故与本任务验收对象无关。

## 证据链

1. **baseline**：`origin/baseline/dev = f4aa6c6cafbf52d09741018583fd2e6dee122e62`（任务创建时确认，
   本分支 `agent/qa/accept-real-managed-e0f551` 直接分叉于它）
2. **real runtime path**：source `apps/camoufox-host/host_v1.py`（f4aa6c6）+ repo venv
   （playwright 1.60.0 / camoufox 0.5.4 / browserforge 1.2.4，与 lock 一致）+ 冻结 rc3 engine
   package（browser exe sha256 `c5535c7c…804b49e` = runtime-asset-lock 值）
3. **run-01**（干净 explorer 祖先）：provision×2 OK、legacy Artifact 构造 OK（注入 screenX=2232,
   screenY=140，超界）；`first-1280x800` launch 失败 ——
   `Failed to launch GPU process after 3 attempts` → `gBrowser never populated` → Playwright 180s 超时
4. **对照实验**（`diagnostics/`）：
   - `plain-launch-desktop.json`：桌面 plain 启动 —— 窗口出现（pid 11104, title "Camoufox",
     1280×1040 @ x=3,y=0），全程零子进程
   - `plain-launch-control.json` / `plain-launch-seeded.json`：沙箱内 plain 启动 —— 同样零子进程
     （camoufox.exe 是 launcher stub，真实浏览器进程被重新父化后存活）
   - `run-00-sandbox-ancestry/`：早期失败尝试归档
5. **字节未变证明**：rc2 包、rc3 包、09-09 smoke seeded 副本三处 camoufox.exe sha256 均等于
   lock 值 `c5535c7c…`；omni.ja mtime 2026-09-02 未动
6. **最后已知成功**：2026-09-09T08:48Z RC2 packaged-host wire smoke PASSED（同一字节：
   google.com 真实加载、content target 正常、真实窗口 1280×800 @ x=3,y=3）—— 证据
   `artifacts/rc2-candidate-acceptance/packaged-host-wire-smoke.json`
7. **排除项**：Host 源码 / Artifact prefs / 沙箱 job 祖先 / Playwright 版本 / 系统级 Exploit
   Protection（全 NOTSET）/ IFEO（无 camoufox.exe 键）/ WER 崩溃事件（无记录）
8. **窗口期机器变化**（候选而非定论，超出 QA 范围）：KB5126052、KB5124008、KB5124007
   安装于 09-09（09-11 重启生效）；另有包 09-13 staged、09-14 08:59 开机生效。最后成功
   09-09 16:48 local → 首次观测失败 09-13（本任务 run-00）

## 复现步骤（交 owning lane / 环境所有者）

```
# 桌面 PowerShell（explorer 祖先），~70 秒，自动清理：
& "C:\Users\qiu\src\VeriSilo\apps\camoufox-host\.venv\Scripts\python.exe" `
  "C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\qa-accept-real-managed-e0f551\docs\qa\window-geometry-runtime-acceptance\tools\plain_launch_desktop.py" `
  --exe "C:\Users\qiu\src\VeriSilo\artifacts\release\managed-browser\v0.1.0-rc2\engine-package\browser\camoufox.exe" `
  --profile "C:\Users\qiu\AppData\Local\Temp\camoufox-plain-desktop\profile" `
  --out "...\diagnostics\plain-launch-desktop-rerun.json"
# 预期（当前坏状态）：窗口 ~36s 出现，camoufoxPids 仅主进程，trees 全空
# 健康（09-09 状态）：窗口 2-5s 出现，trees 含 GPU/content 等多个子进程
```

附带症状：Playwright 路径下 juggler 报 `gBrowser never populated`（content 进程缺失使初始
tab 永不就绪）；沙箱内运行还观察到 `SimpleChannel.js redeclaration` 噪声与 SkeletonUI lock
写入被沙箱拦截（后者仅沙箱环境）。

## 测试矩阵状态

| 项 | 状态 |
|---|---|
| Test 1: 1280×800 observation | 未执行（launch 阻断） |
| Test 2: second size (1024×768) | 未执行 |
| Test 3: legacy Artifact clamp | Artifact 已构造（run-01/artifacts + legacy 注入记录），launch 阻断 |
| Cold restart | 未执行 |
| screenLeft/screenTop alias | 未执行 |
| Artifact bytes unchanged | 未到可验证点 |

## 恢复条件

机器子进程派生能力恢复后（如卸载/回滚嫌疑更新、修复环境策略），本任务可直接续跑：
`tools/run_on_desktop.ps1`（桌面 explorer 祖先执行）→ `run-01` 输出完整矩阵 + summary.json。
工具链已全部就绪并经过参数顺序修复。
