# Chrome、Edge 与 Windows 桌面端手工验收操作手册

这是一份“照着做”的操作手册。它用于真实 Windows 10/11，而不是用自动化测试代替人工验收。每个浏览器、每个 Windows 版本都要单独留证。

结果只使用四种状态：

- `PASS`：实际执行，结果和通过条件一致，并保存了证据。
- `FAIL`：实际执行，结果和通过条件不一致；建 Bug。
- `BLOCKED`：已开始执行，但缺少外部服务、硬件、策略权限或真实制品。
- `SKIP`：没有执行。发布门禁中 `BLOCKED` 和 `SKIP` 都不算通过。

## 0. 先保证测试的是正确代码

当前验收修复可能还在工作树里。仅在 Windows 上重新拉取旧 `main`，不会自动得到未提交文件。开始前必须把整个当前工作树同步到 Windows，或先形成可追踪的 commit/patch。

在 Windows PowerShell 7 中执行：

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$Repo = 'C:\src\VeriSilo'
$Evidence = Join-Path $env:USERPROFILE ("Desktop\VeriSilo-acceptance-" + (Get-Date -Format 'yyyyMMdd-HHmmss'))
$Scratch = Join-Path $env:TEMP ("VeriSilo-manual-" + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $Evidence | Out-Null
New-Item -ItemType Directory -Path $Scratch | Out-Null
Start-Transcript -Path (Join-Path $Evidence 'powershell-transcript.txt')
Set-Location $Repo

git status --short | Tee-Object (Join-Path $Evidence 'git-status.txt')
git rev-parse HEAD | Tee-Object (Join-Path $Evidence 'git-revision.txt')
git diff --stat | Tee-Object (Join-Path $Evidence 'git-diff-stat.txt')
if (-not (Test-Path .\apps\extension\src\native-messaging.ts -PathType Leaf)) {
  throw '当前插件验收修复没有完整到达 Windows 测试机。'
}
if (-not (Test-Path .\docs\acceptance\manual-windows-acceptance-runbook.md -PathType Leaf)) {
  throw 'Windows 手工验收手册不属于当前受测工作树。'
}
```

任一检查抛错就先停止，当前修复没有完整到达测试机。`$ErrorActionPreference` 两行也要放进之后新开的每个 PowerShell 7 窗口；否则原生命令失败后 PowerShell 仍可能继续执行，制造一串无效结果。

正式候选验收必须使用 clean commit，且 `git status --porcelain` 没有输出。当前 dirty worktree 可以做开发期功能验收，但必须在源机器和 Windows 测试机分别计算同一个临时 Git tree ID，避免只按旧 HEAD 记结果：

```powershell
$TemporaryIndex = Join-Path $env:TEMP ("verisilo-index-" + [guid]::NewGuid().ToString('N'))
try {
  $env:GIT_INDEX_FILE = $TemporaryIndex
  git read-tree HEAD
  git add -A
  git write-tree | Tee-Object (Join-Path $Evidence 'worktree-tree-id.txt')
} finally {
  Remove-Item Env:GIT_INDEX_FILE -ErrorAction SilentlyContinue
  Remove-Item $TemporaryIndex -Force -ErrorAction SilentlyContinue
}
```

两台机器的 `worktree-tree-id.txt` 必须完全一致。这个 tree ID 只绑定开发工作树，不会把 dirty tree 变成可发布候选；正式发布仍必须绑定 clean commit 和候选制品 provenance。

当前 Linux 源机器可用等价命令先生成要比较的 ID：

```bash
temporary_index_root="$(mktemp -d)"
export GIT_INDEX_FILE="$temporary_index_root/index"
git read-tree HEAD
git add -A
git write-tree
unset GIT_INDEX_FILE
```

所有测试都使用一次性标准用户或 VM 快照。不要删除或覆盖日常使用的 Vault、默认 Chrome/Edge Profile 或真实代理配置。

`$Scratch` 用于浏览器 Profile、临时 Vault、代理凭据和其他敏感运行数据，绝不能上传、提交或当作普通证据目录。`$Evidence` 中的截图、Console、transcript 和网络结果也可能含公网 IP、用户名、绝对路径或页面内容；上传前逐个脱敏，禁止保存 Cookie/token/口令，且只把最小必要证据交给受控的缺陷系统。

## 1. Windows 测试机准备

每个测试单元记录：Windows 产品名、版本、build、Chrome/Edge 完整版本、Git revision、测试时间和证据目录。

最低准备项：

- Windows 10 x64 或 Windows 11 x64；使用标准用户，安装生命周期也不得用提升权限的终端。
- Chrome Stable、Edge Stable、WebView2 Runtime。
- Node.js 22+、PowerShell 7.4+、pnpm 11.17.0。
- Rust 1.88.0、Cargo、rustfmt、Clippy。
- Visual Studio 2022 Build Tools 的“使用 C++ 的桌面开发”工作负载、C++ CMake tools 和当前 Windows SDK。
- x64 NASM，并确保 `nasm.exe` 在 PATH；这是当前 `aws-lc-sys` Windows x64 默认源码构建要求。参考 [AWS-LC Rust Windows requirements](https://aws.github.io/aws-lc-rs/requirements/windows.html)。

Rust 使用[官方 rustup 安装器](https://rust-lang.org/tools/install/)安装。安装后打开新的 PowerShell 7，并为本次会话固定 MSVC host：

```powershell
rustup toolchain install 1.88.0-x86_64-pc-windows-msvc --profile minimal -c rustfmt -c clippy -t x86_64-pc-windows-msvc
$env:RUSTUP_TOOLCHAIN = '1.88.0-x86_64-pc-windows-msvc'

rustup --version
rustc --version
rustc -vV
cargo --version
rustup show active-toolchain
rustup target list --installed
node --version
corepack enable
corepack pnpm --version
Get-Command pnpm
pnpm --version
pwsh --version
```

进入仓库后 `rustc` 和 `cargo` 都应显示 `1.88.0`，`rustc -vV` 的 host 必须是 `x86_64-pc-windows-msvc`；`pnpm` 应显示 `11.17.0`。裸 `pnpm` 必须可执行，因为 Tauri 配置的 build/dev hook 会直接调用它。然后记录系统和浏览器版本：

```powershell
Get-ComputerInfo |
  Select-Object WindowsProductName, WindowsVersion, OsBuildNumber, OsArchitecture |
  Format-List | Out-File (Join-Path $Evidence 'windows-version.txt')

$Chrome = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
$Edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
(Get-Item $Chrome).VersionInfo.ProductVersion | Out-File (Join-Path $Evidence 'chrome-version.txt')
(Get-Item $Edge).VersionInfo.ProductVersion | Out-File (Join-Path $Evidence 'edge-version.txt')
```

如果浏览器不在上述路径，先找出实际安装路径并修改 `$Chrome` 或 `$Edge`；不要把“找不到浏览器”记成通过。

## 2. 构建插件并启动本地测试站点

在仓库根目录执行：

```powershell
Set-Location $Repo
corepack pnpm install --frozen-lockfile
corepack pnpm --filter @verisilo/extension check
corepack pnpm --filter @verisilo/extension test
corepack pnpm extension:build
corepack pnpm extension:verify
corepack pnpm session-fixture:self-test
$Dist = (Resolve-Path (Join-Path $Repo 'apps\extension\dist')).Path
$Dist | Tee-Object (Join-Path $Evidence 'extension-dist-path.txt')
Get-ChildItem $Dist -File -Recurse |
  Sort-Object FullName |
  ForEach-Object {
    [pscustomobject]@{
      Path = $_.FullName.Substring($Dist.Length).TrimStart('\')
      Length = $_.Length
      Sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  } |
  Export-Csv (Join-Path $Evidence 'extension-dist-sha256.csv') -NoTypeInformation
```

全部命令必须退出码为 0。加载目录以 `$Dist` 输出为准，不要选择 `apps\extension` 源码目录。

再开两个 PowerShell 窗口，并保持它们运行。

窗口 A：

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$Repo = 'C:\src\VeriSilo' # 改成第 0 节的实际值
Set-Location $Repo
$env:PORT = '4173'
node .\tests\fixtures\session-site\server.mjs
```

窗口 B：

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$Repo = 'C:\src\VeriSilo' # 改成第 0 节的实际值
Set-Location $Repo
$env:PORT = '4174'
node .\tests\fixtures\session-site\server.mjs
```

在第三个窗口验证：

```powershell
Invoke-RestMethod http://127.0.0.1:4173/health.json
Invoke-RestMethod http://127.0.0.1:4174/health.json
```

两次都应返回 `fixture = verisilo-session-site`、`schemaVersion = 1`。测试 URL 必须写 `127.0.0.1`，不要写 `localhost`。

## 3. 为 Chrome 和 Edge 建立隔离的手测 Profile

先完全退出日常 Chrome/Edge。正式手测要求新的 disposable Windows 用户，默认 Profile 根在开始时必须不存在：

```powershell
$ChromeDefaultRoot = Join-Path $env:LOCALAPPDATA 'Google\Chrome\User Data'
$EdgeDefaultRoot = Join-Path $env:LOCALAPPDATA 'Microsoft\Edge\User Data'
if ((Test-Path $ChromeDefaultRoot) -or (Test-Path $EdgeDefaultRoot)) {
  throw '默认 Chrome/Edge Profile 已存在；请换新的 disposable 标准用户或 VM 快照。'
}

$ChromeProfile = Join-Path $Scratch 'chrome-manual-profile'
$EdgeProfile = Join-Path $Scratch 'edge-manual-profile'

Start-Process -FilePath $Chrome -ArgumentList @(
  "--user-data-dir=`"$ChromeProfile`"",
  '--no-first-run',
  '--disable-sync'
)

Start-Process -FilePath $Edge -ArgumentList @(
  "--user-data-dir=`"$EdgeProfile`"",
  '--no-first-run',
  '--disable-sync'
)
```

之后先完整做 Chrome，再完整做 Edge。不要用 Chrome 的结果代替 Edge。

### 3.1 加载 unpacked 插件

Chrome：

1. 打开 `chrome://extensions`。
2. 打开右上角“开发者模式”。
3. 点击“加载已解压的扩展程序”。
4. 选择第 2 节 `$Dist` 输出并保存在 `extension-dist-path.txt` 的目录。
5. 确认版本是 `0.2.4`，卡片没有 manifest 或 Service Worker 错误。
6. 进入“详细信息”，打开“允许在无痕模式下使用”。
7. 记录扩展 ID 到 `$Evidence\chrome-extension-id.txt`，并固定工具栏图标。

Edge：

1. 打开 `edge://extensions`。
2. 打开“开发人员模式”。
3. 点击“加载解压缩的扩展”。
4. 选择同一个 `$Dist` 目录。
5. 确认版本 `0.2.4` 且无错误。
6. 进入“详细信息”，打开“允许在 InPrivate 中”。
7. 记录扩展 ID 到 `$Evidence\edge-extension-id.txt`，并固定工具栏图标。

每次重新执行 `pnpm extension:build` 后，都要回扩展管理页点击“重新加载”。重新加载本身也要执行第 4.8 节的 Labs 恢复测试。

## 4. 插件逐项操作：Chrome 和 Edge 各做一遍

以下每一项都保存截图。建议名称为 `W11-Chrome-04.1-entry.png`、`W11-Edge-04.1-entry.png`。浏览器 DevTools、扩展 Service Worker DevTools 和下载文件属于原始证据，不要只写“看起来正常”。

### 4.1 入口、四个页面和控制台

1. 打开 `http://127.0.0.1:4173/`。
2. 点击 VeriSilo 工具栏图标。
3. 确认 Side Panel 打开，而不是普通网页或空白页。
4. 依次点击“身份概览”“轻量隔离”“实验室”“技术数据”。
5. 确认一次只显示一个页面；没有两个页面叠在一起。
6. 需要查权限或错误时，在扩展管理页点击该扩展的 Service Worker“检查/Inspect”并留证。普通操作完成后可以关闭；执行超时、后台回收和扩展 reload 场景前必须关闭 inspector，避免 DevTools 人为阻止 MV3 Service Worker 休眠。

通过条件：四个入口都可达、切换后内容正确、Console 没有未处理异常或持续报错。

### 4.2 当前站点权限：拒绝、允许、撤销

1. 在“身份概览”展开“扫描提示‘无法访问’？”。
2. 点击“请求当前站点权限”，在浏览器权限框点拒绝/取消。
3. 确认 UI 明确提示未授权，没有显示假成功。
4. 在 Service Worker Console 执行：

```javascript
chrome.permissions
  .contains({ origins: ["http://127.0.0.1:4173/*"] })
  .then(console.log);
```

结果应为 `false`。

5. 再点“请求当前站点权限”，这次允许，重复上面的 Console 检查；应为 `true`。
6. 点击“撤销当前站点长期权限”，再查一次；应回到 `false`。
7. 再次允许，为后续扫描和 Labs 测试保留权限。

### 4.3 扫描、异常结束和标签页归属

1. 点击“扫描当前页面”。按钮应保持 busy，直到新报告出现或显示有界错误；不能立刻展示旧报告。
2. 在“技术数据”核对信号终态：正常 fixture 路径必须是 15 个 isolated-world 信号加 1 个 MAIN-world 信号，共 16 个；若 MAIN 注入被浏览器明确阻断，15 个 isolated-world 信号可保留，但报告必须清楚标为覆盖降级，现有信号仍要全部有终态。
3. 新开标签页 `http://127.0.0.1:4174/`，不要扫描它。
4. 切到 4174 标签页；若浏览器关闭 Side Panel，就在 4174 再点一次插件 action。打开后的 Side Panel 应显示空态，不能继续显示 4173 的报告。
5. 切回 4173，原报告应恢复。
6. Chrome 打开 `chrome://settings/`，Edge 打开 `edge://settings/`。在这个浏览器受限页面打开/切回 Side Panel 并点击扫描；应在有界时间内明确提示无法访问，不能永久转圈，也不能显示 4173/4174 的旧报告。然后回 fixture 继续。

### 4.4 报告历史、清除确认和导出脱敏

1. 在 4173 完成一次扫描，切到“技术数据”。
2. 确认 JSON、HTML 导出按钮已启用，本机历史数增加。
3. 点击“清除本地报告历史”，先在确认框点取消；历史必须仍存在。
4. 再点一次并确认；历史应清空，但当前会话的报告仍可导出。
5. 点击前在 PowerShell 执行 `$DownloadStart = Get-Date`，再分别点击“导出脱敏 JSON”和“导出可读 HTML”。
6. 打开 `chrome://downloads/` 或 `edge://downloads/`，以本次 report ID 和时间确认两项下载成功，点击“在文件夹中显示”取得实际路径；不要假设企业策略仍使用默认 Downloads。把两个实际路径填入 `$JsonPath`、`$HtmlPath`，再执行：

```powershell
$JsonPath = '<本次 JSON 的实际绝对路径>'
$HtmlPath = '<本次 HTML 的实际绝对路径>'
$Downloaded = Get-Item $JsonPath, $HtmlPath
if ($Downloaded.Count -ne 2 -or ($Downloaded | Where-Object LastWriteTime -lt $DownloadStart)) {
  throw '本次报告下载没有全部真实落盘。'
}
$Report = Get-Content $JsonPath -Raw | ConvertFrom-Json
$UnexpectedHighValues = @(
  $Report.signals | Where-Object {
    $_.sensitivity -eq 'high' -and
    $_.PSObject.Properties.Name -contains 'value' -and
    $_.value -ne '[redacted by default]'
  }
)
if ($UnexpectedHighValues.Count -ne 0) {
  throw 'JSON 导出包含未脱敏的 high-sensitivity 值。'
}
if (-not (Select-String -Path $HtmlPath -SimpleMatch '[redacted by default]' -Quiet)) {
  throw 'HTML 导出没有显示预期的 high-sensitivity 脱敏占位。'
}
```

7. 打开 JSON，确认是合法 JSON；打开 HTML，确认能离线阅读。
8. 不要把真实 renderer、Cookie 或其他 high-sensitivity 原值复制到 transcript、截图或公开 Issue。上面的结构化断言必须通过。

任何下载缺失、JSON 不合法、HTML 空白或高敏感原值泄漏都记 `FAIL`。

### 4.5 网络检查：拒绝、允许、清除权限

1. 回“身份概览”，找到网络出口项，点击“同意并检查当前环境出口”。
2. 第一次在浏览器权限框点取消；UI 应说明没有发起检查。
3. 第二次允许三个端点：`ipwho.is`、`cloudflare-dns.com`、`dns.google`。
4. 等待出口 IP、地区/ASN、公共 DNS 和 DNSSEC 状态出现。没有桌面端时，跨端状态必须明确为“仅本地”，不能伪装成已提交。
5. 点击“清除结果并撤销权限”。结果应消失。
6. 在 Service Worker Console 执行：

```javascript
chrome.permissions
  .contains({
    origins: [
      "https://ipwho.is/*",
      "https://cloudflare-dns.com/*",
      "https://dns.google/*",
    ],
  })
  .then(console.log);
```

结果应为 `false`。若测试机无法访问公共端点，记录 `BLOCKED` 和具体网络错误；UI 假报成功仍是 `FAIL`。

### 4.6 轻量隔离、privacy 权限和无痕/InPrivate

1. 在普通窗口的 fixture 输入 `regular-Chrome` 或 `regular-Edge`，点击“Save all browser state”。
2. 在干净测试 Profile 的 Service Worker Console 先确认下面两个 restore-point key 均不存在。点击“启用推荐保护”，第一次拒绝 privacy 权限；再次确认 key 仍不存在，UI 不得声称设置已改：

   ```javascript
   chrome.storage.local
     .get(["webrtc-restore-point", "network-prediction-restore-point"])
     .then(console.log);
   ```

3. 再点一次并允许。UI 应显示 WebRTC 非代理 UDP 已限制、网络预测已关闭，或明确指出策略/其他扩展接管；不得假报“已验证”。
4. 若应用成功，Console 中两个 restore-point 应记录应用前基线。点击“恢复原设置”；插件必须释放两个设置、清除 restore-point 并撤销 privacy 权限。当前 UI 没有独立的“只授权不应用”入口，因此人工证据是保存/清除 restore-point、应用后的 API 回读和控制权释放；不要把它夸大成外部浏览器策略审计。
5. 在 Service Worker Console 检查：

```javascript
chrome.permissions.contains({ permissions: ["privacy"] }).then(console.log);
```

结果应为 `false`。

再执行一次上面的 `chrome.storage.local.get(...)`，结果应为空对象。

6. 点击“用当前网站打开隐私窗口”。在新无痕/InPrivate 窗口点“Read all browser state”：五个值应为 `null`，`indexedDBExists`/`cacheExists` 应为 `false`，Service Worker registration/controller 应为 `null`。
7. 在隐私窗口写入 `private-Chrome` 或 `private-Edge`；回普通窗口点 Read，仍应只看到 `regular-*`。
8. 关闭全部隐私窗口，再重新打开隐私窗口；值应再次为空。
9. 在普通窗口记录报告历史数和 Labs 收据数。在隐私窗口扫描一次，并开启、停止一次 Labs；回普通窗口确认两个计数都没有增加。

### 4.7 桌面端不可用时的降级

Native Host 注册位于 HKCU，不会因为使用隔离浏览器 Profile 而自动消失。先在 PowerShell 检查：

```powershell
$ChromeHostRegistered = Test-Path 'HKCU:\Software\Google\Chrome\NativeMessagingHosts\io.verisilo.host'
$EdgeHostRegistered = Test-Path 'HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\io.verisilo.host'
$ChromeHostRegistered
$EdgeHostRegistered
```

对应浏览器的结果必须是 `False` 才执行本节；如果是 `True`，回干净 VM 快照，不要为测试临时删除日常注册表项。

1. “轻量隔离”页点击“打开 VeriSilo 桌面端”。
2. 最迟 10 秒内应显示 Native Host/桌面不可用的明确错误，不能无限等待或显示已打开。
3. 点击“查看项目页”，确认它是独立入口。
4. Service Worker Console 不应出现未处理 Promise 或消息端口泄漏。

### 4.8 Labs：每种停止原因都单独执行

先在网页 DevTools Console 保存原生 Worker 引用：

```javascript
globalThis.__verisiloNativeWorker = Worker;
```

每个场景开始前都导航/刷新到干净的 4173 页面，刷新后重新执行上面的 `globalThis.__verisiloNativeWorker = Worker`。确认停止按钮禁用、没有 enabled 的实验即可；前一份收据存在时 UI 可以写“已恢复”，不强求回到首次安装的“默认关闭”文案。点击“为当前站点开启 2 分钟”时先检查确认框的取消分支，再确认并允许站点权限。运行后应为 `Best-effort/窄实验`，并明确写“本机临时、非桌面 Silo”。

运行后在网页 Console 检查：

```javascript
Worker === globalThis.__verisiloNativeWorker;
```

应为 `false`。然后按以下场景逐一执行；每个场景都打开最新收据，检查 Observe、Apply、Verify、Restore、停止原因、覆盖范围和恢复结果。

1. **显式停止**：点击“立即恢复并停用”。上式应回到 `true`，收据应显示恢复成功。
2. **页面异常**：重新启用后，在网页 Console 执行：

   ```javascript
   setTimeout(() => {
     throw new Error("verisilo-labs-manual-error");
   }, 0);
   ```

   页面抛错是本步骤的触发器；Labs 应自动停止并恢复。

3. **同文档导航**：重新启用后执行：

   ```javascript
   history.pushState({ verisilo: "manual" }, "", "?labs-navigation=1");
   ```

   Labs 应自动停止；URL 带查询参数时刷新仍应返回测试页，不能 404。

4. **跨 origin 导航**：重新启用后执行：

   ```javascript
   location.href = "http://127.0.0.1:4174/";
   ```

   到达 4174 后重新打开 Side Panel；实验应关闭，最新收据应说明导航停止与恢复/旧 realm 销毁结果。

5. **权限撤销**：回 4173 重新启用，在“身份概览”撤销当前站点长期权限。Labs 应停止并恢复。
6. **超时/到期**：关闭 Service Worker inspector，重新启用后不操作页面，等待至少 2 分 10 秒。实验应自动停止；根据 MV3 回收时机，停止码可以是运行超时或授权到期，但都必须 `enabled=false` 且恢复成功。
7. **扩展重新加载**：关闭旧 inspector，重新启用后，在 `chrome://extensions` 或 `edge://extensions` 点击插件“重新加载”，再回页面打开 Side Panel；需要 Console 时打开新的 inspector。旧 Worker 包装不能继续失控存在；状态必须 fail-closed，并有可解释的恢复结果。残留包装、丢失状态或假报恢复均记 `FAIL`。
8. **清除收据**：点击“清除收据”，先取消，数量不变；再确认，数量归零。

Cookie 虚拟化和 Set-Cookie 全面截获必须保持“不可选/unsupported”。如果 UI 允许开启，记 `FAIL`。

## 5. 构建并启动真实 Windows 桌面端

`pnpm desktop:dev` 只启动 Vite 前端，不是完整桌面应用。先用 `tauri dev` 做开发联调，再生成不依赖 dev server 的 standalone debug exe；桌面崩溃/重启、Native Host 和长时间手测统一使用后者。

在 Visual Studio 2022 的开发者 PowerShell或已正确载入 MSVC 环境的 PowerShell 7 中执行：

```powershell
Set-Location $Repo
where.exe cl
nasm -v

cargo fetch --locked --manifest-path .\crates\verisilo-desktop-core-harness\Cargo.toml
corepack pnpm desktop-core:verify

corepack pnpm --filter @verisilo/desktop check
corepack pnpm --filter @verisilo/desktop test
corepack pnpm --filter @verisilo/desktop build

cargo fmt --manifest-path .\apps\desktop\src-tauri\Cargo.toml -- --check
cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml --target x86_64-pc-windows-msvc
cargo test --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml --lib --target x86_64-pc-windows-msvc
cargo clippy --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml --all-targets --target x86_64-pc-windows-msvc -- -D warnings

corepack pnpm --filter @verisilo/desktop tauri dev
```

应出现真实 Tauri 窗口；只出现 `127.0.0.1` 网页不算桌面启动通过。完成快速联调后关闭 dev 进程，再构建并运行 standalone debug exe：

```powershell
corepack pnpm --filter @verisilo/desktop tauri build --debug --no-bundle
$DebugDir = (Resolve-Path .\apps\desktop\src-tauri\target\debug).Path
$Desktop = Join-Path $DebugDir 'verisilo.exe'
if (-not (Test-Path $Desktop -PathType Leaf)) {
  throw 'standalone debug desktop exe 没有生成。'
}
& $Desktop
```

后续写“重新打开桌面”时都执行 `& $Desktop`，不要直接运行依赖 Vite 的旧 dev 进程。

## 6. Windows 桌面端逐项操作

以下操作在新的标准用户或 disposable VM 中执行。测试口令至少 12 个字符，不要使用真实密码。

### 6.1 Vault 初始化、解锁、锁定和自动锁

1. 首次启动输入测试口令，点击“创建本地保险库”。
2. 点击“立即锁定”；确认 Silo、代理凭据、远程状态、报告选择等敏感 UI 立即清空或不可达。
3. 用错误口令解锁；应失败且保持锁定。
4. 用正确口令解锁；Silo 列表应恢复。
5. 解锁后保持 15 分钟完全无操作；到期应自动锁定。
6. 再解锁，让 Windows 睡眠后恢复；回到应用时不得把已过期会话继续当作解锁。
7. 在“保险库与数据”更换口令：先输错当前口令，确认无变化；再正确更换。锁定后旧口令必须失败，新口令必须成功。

### 6.2 加密备份和恢复

1. 先创建至少一个测试 Silo。
2. 在 PowerShell 创建仅供测试的用户目录，并复制命令输出的绝对路径到桌面 UI：

   ```powershell
   $VaultTest = New-Item -ItemType Directory -Path (Join-Path $Scratch 'vault-test') -Force
   $BackupPath = Join-Path $VaultTest.FullName 'verisilo-vault.backup'
   $BackupPath
   ```

   在“保险库与数据”把备份写到这个尚不存在的 `$BackupPath`。

3. 再次备份到同一路径；应拒绝覆盖，而不是静默改写。
4. 用文本搜索确认备份不含 Silo 名、代理用户名/密码等明文测试哨兵。
5. 复制备份为 `verisilo-vault-corrupt.backup`，只破坏副本；尝试恢复，应拒绝且当前 Vault 不变。
6. 用错误备份口令恢复，应拒绝且当前 Vault 不变。
7. 勾选明确覆盖确认，用正确备份和口令恢复；Silo 元数据应恢复。
8. 确认 UI 明确说明浏览器 Profile 不在备份内；不得声称网站数据已备份。

### 6.3 Silo 创建、编辑、归档、恢复和永久删除

1. 创建 `Chrome-A`：浏览器选 Chrome，网络选 Direct。
2. 创建 `Chrome-B`：同样选 Chrome/Direct。
3. 创建 `Edge-A` 和 `Edge-B`：浏览器选 Edge/Direct。
4. 编辑其中一个 Silo 的名称和颜色；保存后 Profile 归属、已有网站数据不能改变。
5. 归档一个未运行 Silo；应从启动列表消失，但显示在“已归档”，数据目录仍保留。
6. 点击“恢复”；原 Silo 和数据应回来。
7. 再归档一个只用于删除的 Silo，点击“永久删除”：第一次取消，数据应保留；第二次确认，受管 Profile 和 Vault 记录应删除，默认浏览器 Profile 不得受影响。

### 6.4 Chrome/Edge Profile 隔离

对 Chrome-A/B 和 Edge-A/B 分别执行：

1. 启动 A，打开 `http://127.0.0.1:4173/`。
2. 输入唯一值，例如 `Chrome-A-20260730-01`，点击“Save all browser state”。
3. 页面 JSON 应显示 LocalStorage、SessionStorage、Cookie、IndexedDB、Cache 都是该值，`indexedDBExists`/`cacheExists` 都是 `true`，`serviceWorkerRegistration` 是 `activated`。刷新一次后，`serviceWorkerController` 也应是 `activated`。
4. 正常退出浏览器，桌面点击“重新核验浏览器”或等待状态变为停止。
5. 启动 B，打开同一 URL，点击“Read all browser state”。首次结果的五个值应全是 `null`，`indexedDBExists`/`cacheExists` 是 `false`，Service Worker registration/controller 都是 `null`；读取动作本身不得创建空容器。
6. 在 B 写入另一个唯一值并正常退出。
7. 重新启动 A。LocalStorage、Cookie、IndexedDB、Cache 和 Service Worker registration/controller 应恢复 A 的值；新进程中的 SessionStorage 为空是浏览器正确行为。
8. 重新启动 B，确认只看到 B 的值。
9. 在只用于销毁的测试 Silo 点击“Clear all browser state”。当前页面的 controller 可能活到本次导航结束；刷新/新开同一 URL 后，五个值必须是 `null`、两个容器存在位必须是 `false`，Service Worker registration/controller 都必须是 `null`。

任何 A/B 串值或默认 Profile 被写入都记 `FAIL`。

### 6.5 正常退出、重复启动、浏览器崩溃和桌面崩溃

1. Silo 运行时，其他 Silo 的“启动 Silo”按钮应禁用。再执行一次 `& $Desktop`，single-instance 机制应只激活现有桌面窗口，不能出现第二个桌面控制器。真正的第二 runtime/Profile lock 竞争由第 8 节 acceptance driver 验证，不把按钮禁用冒充底层锁证明。
2. 正常关闭浏览器，点击“复查运行与网络”；状态应收口为停止，之后可再次启动。
3. 启动测试 Silo，用任务管理器结束该测试浏览器进程树；回桌面复查，必须识别崩溃/停止并允许安全恢复。
4. 再启动 Silo，然后用任务管理器结束 `verisilo.exe`，不要结束浏览器。重新打开桌面并解锁；应显示待恢复核对，不能把残留浏览器误当成新 runtime。
5. 精确 PID tree、不相关进程存活和真实 Chromium `SingletonLock` 拒绝由第 8 节 source-exact acceptance driver/harness 验证；桌面 UI 不暴露足够的 PID/Profile path，禁止仅凭进程名手工强杀后声称这些底层断言通过。

### 6.6 Direct、固定代理、PAC、Mihomo 与 fail-closed

Direct：创建 Direct Silo，启动并访问 fixture；应可达，启动证据明确是 Direct。

不可达 required proxy 的可复现负向测试：

1. 先运行 `Test-NetConnection 127.0.0.1 -Port 9 -InformationLevel Quiet`。只有结果为 `False` 时才使用端口 `9`；若为 `True`，选择另一个经同样检查确认未监听的 loopback 端口。创建固定 HTTP 代理 Silo，填写该端口并勾选“代理不可连接就拒绝启动”。
2. 点击“启动 Silo”。应在浏览器访问任何站点前拒绝启动或进入“网络已阻断”；不得回退 Direct。
3. 改为一个启动后中断的真实测试代理，再点“复查阻断状态（不重开端口）”；不得偷偷重开或直连。

真实 HTTP 与 SOCKS5：分别使用测试实验室提供的无认证、用户名/密码端点。启动后在 Silo 内通过 Companion 做出口检查；桌面控制器自身的出口检查不能代替 Silo 出口。没有真实代理就记 `BLOCKED`。

PAC：

1. 建一个 optional PAC Silo，验证 PAC URL 可用、不可用和缓存切换。
2. 勾选“必须代理”；当前实现应明确拒绝启动，因为 PAC 没有 required-proxy 预检。若静默 Direct，记 `FAIL`。

Mihomo：

1. 先启动用户自己的 Mihomo/Clash 测试实例和本机 Controller。
2. 在桌面选择 Mihomo，填写 `http://127.0.0.1:9090/` 和测试 Secret。
3. 点击“连接并读取节点”，选择组和固定节点，创建 Silo。
4. 启动后核对 Controller 回读、节点绑定、loopback 中继和 Companion 实际出口。
5. 在运行中切换节点、停 Controller、停监听端口；点击“复查运行与网络”或等待 watchdog 执行后，runtime 必须进入 `verification_failed/网络已阻断`、managed relay 关闭，页面不能 Direct 回退。

Controller 不存在、没有合法测试节点或没有流量观测条件时，Mihomo 项记 `BLOCKED`，不能用静态 UI 记 `PASS`。

## 7. 注册调试 Native Host，做真实插件—桌面联动

第 5 节已生成 standalone `$Desktop`。再把 Native Host 构建到同一 debug 目录：

```powershell
Set-Location $Repo
cargo build --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml --bin verisilo-native-host

$DebugDir = (Resolve-Path .\apps\desktop\src-tauri\target\debug).Path
$Host = Join-Path $DebugDir 'verisilo-native-host.exe'
if (-not (Test-Path $Host -PathType Leaf)) { throw 'Native Host 没有生成。' }
if (-not (Test-Path $Desktop -PathType Leaf)) { throw 'Desktop exe 没有生成。' }
```

用第 3.1 节记录的开发扩展 ID 注册：

```powershell
pwsh -NoProfile -File .\scripts\register-native-host.ps1 `
  -Browser chrome `
  -ExtensionId '<Chrome 开发扩展 ID>' `
  -HostPath $Host

pwsh -NoProfile -File .\scripts\register-native-host.ps1 `
  -Browser edge `
  -ExtensionId '<Edge 开发扩展 ID>' `
  -HostPath $Host
```

回 Chrome/Edge 扩展管理页重新加载插件。然后在桌面启动的真实 Silo 浏览器 Profile 内再次“加载已解压”，选择同一个 `dist`，记录该 Profile 实际显示的 ID；若 ID 不同，按实际 ID重新注册。

证据边界：Native Host 接受浏览器声明的 extension ID，并把 snapshot 中的 Silo/runtime ID 绑定到 `extension_asserted` 证据；它不认证发消息的 Chrome/Edge 进程或 Profile 本身。即使 UI 显示同一 Silo/runtime，也不能把它写成进程级归属证明。

逐项验证：

1. 桌面解锁 Vault，启动一个 Silo；等待桌面写出新鲜 Native Messaging 状态。
2. 在该 Silo 中打开插件，“打开 VeriSilo 桌面端”应激活现有桌面窗口，而不是报 Host 缺失或重复启动。
3. 在 Silo 内运行网络检查。插件应显示已提交/绑定状态；桌面应出现 snapshot 所声明的同一 Silo、同一 runtime 的 `extension_asserted` Companion 网络证据。
4. 关闭 Silo 浏览器，让桌面把 runtime 收口为 stopped。随后从第 3 节的独立手测 Profile 重新运行一次网络检查；它只能明确降级为本地或因 `runtime_not_ready` 被拒绝，不能附着到已停止或下一次启动。旧 frame/request 的精确 replay 由自动化测试验证，UI 没有 replay 按钮，不把新请求冒充 replay 测试。
5. 在桌面 Silo 中启用 Labs。范围应显示桌面 Silo ID/站点，不应再写“本机临时、非 Silo”。收据应绑定正确归属。
6. Labs 运行时，在网页 Console 保存当前原生 Worker 引用并锁定 Vault，等待一次 snapshot/watchdog 窗口后核对 Worker、停止按钮、停止码和 receipt。当前源码没有持续轮询 Vault/Silo runtime 的 Labs 重新授权，这是预计会暴露的已知发布阻断风险：若包装仍在运行或旧绑定继续可用，明确记 `FAIL` 并建 Bug，不能因桌面 UI 已清空就记通过。
7. 解锁并重新启动 Silo；旧 receipt/网络结果不得自动升级为当前 runtime 证据。
8. 分开做两种退出：
   - 正常退出 standalone desktop：运行状态文件应立即清除；插件下一次操作直接显示桌面不可用/本地降级，不需要等待 45 秒。
   - 重新启动 desktop 和 Silo 后，用任务管理器强制结束 `verisilo.exe`，保留崩溃时的旧 snapshot；等待超过 45 秒后再从仍开的 Silo 浏览器操作。旧 snapshot 必须因过期被拒绝或明确降级为本地，不能假装仍属于运行中 Silo。

## 8. 运行仓库自带 Windows E2E harness

先做一次诊断运行；它不加载插件，不能替代第 4、7 节：

```powershell
if (Get-Process chrome, msedge -ErrorAction SilentlyContinue) {
  throw '先完全关闭第 3/7 节的所有 Chrome 和 Edge 测试进程，再运行 harness。'
}
$ChromeDefaultRoot = Join-Path $env:LOCALAPPDATA 'Google\Chrome\User Data'
$EdgeDefaultRoot = Join-Path $env:LOCALAPPDATA 'Microsoft\Edge\User Data'
if ((Test-Path $ChromeDefaultRoot) -or (Test-Path $EdgeDefaultRoot)) {
  throw '手工验收期间创建或改写了默认 Chrome/Edge Profile 根。'
}
$HarnessEvidence = Join-Path $Evidence 'windows-harness'
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloWindowsE2E.ps1 `
  -Browser Both `
  -ExpectedWindowsVersion 'Windows 11' `
  -ArtifactDirectory $HarnessEvidence `
  -KeepArtifacts
```

在 Windows 10 上把期望值改为 `'Windows 10'`。查看：

```powershell
Get-Content (Join-Path $HarnessEvidence 'summary.json') -Raw
```

诊断运行中的 `SKIP/BLOCKED` 是真实缺口，不是通过。只有 Native Host、release config、desktop driver 和精确候选制品都到位时才运行 `-RequireAll` 作为候选门禁。

环境后端的非破坏 self-test：

```powershell
$EnvironmentEvidence = Join-Path $Evidence 'environment-harness'
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloEnvironmentAcceptance.ps1 `
  -ArtifactDirectory $EnvironmentEvidence `
  -SelfTest
```

真实 WSL、Sandbox、Hyper-V 只能在相应 Windows SKU、已启用功能和合法固定镜像到位后执行：

```powershell
# WSL：名称必须来自当前 wsl.exe --list --quiet
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloEnvironmentAcceptance.ps1 `
  -ArtifactDirectory (Join-Path $Evidence 'environment-wsl') `
  -RunWslIdentity `
  -WslDistribution '<精确发行版名称>'

# Windows Sandbox：要求可用 SKU，以及同一有效签名者和时间戳的 provider scripts
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloEnvironmentAcceptance.ps1 `
  -ArtifactDirectory (Join-Path $Evidence 'environment-sandbox') `
  -RunSandboxLaunch

# Hyper-V：只在提升权限的 disposable VM/主机上，以已审核镜像执行
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloEnvironmentAcceptance.ps1 `
  -ArtifactDirectory (Join-Path $Evidence 'environment-hyperv') `
  -RunHyperVLifecycle `
  -HyperVApprovedImageRoot '<已审核镜像根目录>' `
  -HyperVImageFile '<VHDX 叶文件名>' `
  -HyperVImageSha256 '<精确 SHA-256>' `
  -ConfirmHyperVDestroy
```

Hyper-V destroy 是破坏性操作；不要为了“完成清单”在日常主机上尝试。Sandbox/Hyper-V 缺少合法、同签名且带时间戳的 provider scripts 时应 `BLOCKED`。

Remote Agent 的默认部署样例把 Provider 能力声明为 unavailable。没有固定真实 Provider、合法镜像、真实 WAN 和媒体/输入 transport 时：

- 只能测试本地 schema、UI 和 unavailable 门禁；真实 pinned HTTPS 配对与 lifecycle 协议、远程浏览器均记 `BLOCKED`。
- Screen 只能验证授权 channel metadata；不能把它记成远程画面通过。
- Input 只能验证类型化命令与授权边界；不能把它记成真实输入 transport 通过。

## 9. 安装、升级、卸载放在最后

只有插件、桌面和集成问题收口后才执行安装生命周期。当前 unsigned desktop-only 配置最多允许生成 desktop-only artifact；在真实 Windows 安装、启动和卸载前，连“安装成功”也不能声称，更不能作为 Native Host 集成或正式发布证据。

正式候选必须使用两个真实版本、标准用户、真实签名和保留数据路径。先手工安装 V1，用真实 UI 创建 Vault/Silo，确认 `InstallDirectory` 和每个 `RetainedDataPath` 已存在。NSIS-only 诊断可省略 `-RequireAll`，随后单独核对 `nsis_silent_install_upgrade_uninstall_data_retention == PASS`；若使用 `-RequireAll`，还必须同时提供 Native Host、release config、Desktop、acceptance driver、candidate descriptor 和 manifest root 等全部正式参数，详见 `tests/windows/README.md`。执行前建立 VM 快照；安装/卸载会真实修改当前用户应用目录。

## 10. 每项测试的记录模板

建议建立 `results.csv` 或表格，至少包含：

| 字段     | 内容                                            |
| -------- | ----------------------------------------------- |
| Case     | 例如 `EXT-LABS-PUSHSTATE`                       |
| Windows  | 产品名、版本、build                             |
| Browser  | Chrome/Edge 完整版本                            |
| Revision | commit、工作树状态/tree ID、extension dist 哈希 |
| Steps    | 实际执行的步骤，不写“按文档”                    |
| Expected | 本文列出的通过条件                              |
| Actual   | 实际 UI 文案、状态、时间和错误                  |
| Result   | PASS / FAIL / BLOCKED / SKIP                    |
| Evidence | 截图、Console、下载文件、`summary.json` 的路径  |
| Bug      | FAIL 对应的 Issue/Bug ID                        |

测试结束后执行：

```powershell
Stop-Transcript
```

四个最终插件单元是 Windows 10 × Chrome、Windows 10 × Edge、Windows 11 × Chrome、Windows 11 × Edge。任何一个单元没有实际执行或缺证据，都不能把插件阶段标为通过。
