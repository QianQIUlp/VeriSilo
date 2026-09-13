# QA evidence wrapper: run the window-geometry acceptance matrix on the
# interactive desktop (outside any agent sandbox job).  Evidence-only tooling;
# it modifies no product source and touches no user Silo data.
#
# Heavy roots (browser cache copy, profiles) live under the user temp dir;
# only small artifact JSONs and observation outputs land inside the worktree.

$ErrorActionPreference = "Continue"

$worktree = "C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\qa-accept-real-managed-e0f551"
$python   = "C:\Users\qiu\src\VeriSilo\apps\camoufox-host\.venv\Scripts\python.exe"
$pkgRoot  = "C:\Users\qiu\src\VeriSilo\artifacts\build\managed-browser\rc3-engine-package"
$qaRoot   = Join-Path $worktree "docs\qa\window-geometry-runtime-acceptance"
$driver   = Join-Path $qaRoot "tools\run_geometry_qa.py"
$runDir   = Join-Path $qaRoot "run-01"
$heavy    = "C:\Users\qiu\AppData\Local\Temp\verisilo-geometry-qa\run-01"
$console  = Join-Path $runDir "console.log"
$exitFile = Join-Path $runDir "exit-code.txt"

if (Test-Path $exitFile) {
    Write-Output "run-01 already executed (exit-code.txt exists); refusing to overwrite."
    exit 3
}

foreach ($dir in @(
    (Join-Path $runDir "artifacts"),
    (Join-Path $runDir "out"),
    (Join-Path $heavy "profiles"),
    (Join-Path $heavy "state")
)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

& $python $driver `
    --python $python `
    --package-root $pkgRoot `
    run-all `
    --seed "3f9a1c67d2e84b05a7c4198e6d0f2b539c7e5a12b4d6f8031e2a5c7b9d0f4468" `
    --artifact-root (Join-Path $runDir "artifacts") `
    --profile-root (Join-Path $heavy "profiles") `
    --state-root (Join-Path $heavy "state") `
    --second-window 1024x768 `
    --legacy-screen-x 2232 `
    --legacy-screen-y 140 `
    --out-dir (Join-Path $runDir "out") `
    *>> $console

$code = $LASTEXITCODE
if ($null -eq $code) { $code = 1 }
Set-Content -Path $exitFile -Value $code
Write-Output "GEOMETRY_QA_EXIT_CODE=$code"
exit $code
