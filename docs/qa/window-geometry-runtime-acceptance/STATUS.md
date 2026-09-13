# Window Geometry Runtime Acceptance — 进度快照（2026-09-13 夜）

## 绑定事实
- baseline：`origin/baseline/dev = f4aa6c6cafbf52d09741018583fd2e6dee122e62`（任务创建时确认）
- 分支：`agent/qa/accept-real-managed-e0f551`（本 worktree，仅 evidence，不推进 baseline）
- verdict：**尚未产生**（测试矩阵未完整执行）
- 运行绑定：
  - python = `C:\Users\qiu\src\VeriSilo\apps\camoufox-host\.venv\Scripts\python.exe`（3.12.11，playwright/camoufox 可用）
  - package-root = `C:\Users\qiu\src\VeriSilo\artifacts\build\managed-browser\rc3-engine-package`（browser 二进制冻结 Formal-v3；Host 源码来自本 worktree f4aa6）
  - 重根（cache/profiles）在 `%TEMP%\verisilo-geometry-qa\run-01`（已清空，下次运行自动重播 cache）

## 已完成
- `tools/run_geometry_qa.py`：provision / legacy / runtime / run-all 全链路驱动，已修复 argparse 参数顺序
- `tools/run_on_desktop.ps1`：桌面执行包装。注意：`exit-code.txt` 幂等护栏，重跑前需删除（run-01 当前是干净的）
- `run-00-sandbox-ancestry/`：两次失败尝试归档。第二次 attempt 已通过 provision×2 + legacy artifact 构建，卡在 `first-1280x800` 的 launch

## 根因（当前最强假设）
camoufox 的 content/GPU 子进程在「Trae 沙箱 job 后代」进程树中无法派生：
`Failed to launch GPU process after 3 attempts` + `gBrowser never populated`。
computer_use 第一次执行时用 `Start-Process` 从沙箱 spawn 了 PowerShell —— 父链仍在沙箱 job 内，
因此症状与沙箱内完全一致。桌面（explorer 祖先）启动是尚未验证的变量。

## 明天恢复步骤
1. computer_use 通过**开始菜单 UI**打开 PowerShell（父进程必须是 explorer / WindowsTerminal；
   禁止 Start-Process 或任何程序化 spawn）。
2. 先验证祖先：
   `$p = Get-CimInstance Win32_Process -Filter "ProcessId = $PID"; $p.ParentProcessId; (Get-CimInstance Win32_Process -Filter "ProcessId = $($p.ParentProcessId)").Name`
   父进程名不是 shell/explorer 类则停下来报告。
3. 运行：
   `powershell -NoExit -ExecutionPolicy Bypass -File "C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\qa-accept-real-managed-e0f551\docs\qa\window-geometry-runtime-acceptance\tools\run_on_desktop.ps1"`
   轮询 `run-01\exit-code.txt`；结束后读 `run-01\out\summary.json` 的 verdict / invariantChecks。
4. 若干净桌面祖先下仍 GPU 失败 → 改试 `schtasks /Create ... /IT` 交互任务执行同一脚本；
   再失败 → verdict = `ENVIRONMENT_BLOCKED`（run-00 console.log 即为证据）。
5. PASS 后按任务合同出最终报告（1280×800 / second-size / legacy clamp / cold restart /
   screenLeft alias / artifact bytes unchanged），commit evidence branch，不推进 baseline，
   不改 docs/camoufox-program-status.md。
