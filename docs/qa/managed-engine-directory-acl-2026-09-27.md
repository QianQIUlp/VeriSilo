# Windows Managed Engine 目录启动差异：ACL 因果对照

日期：2026-09-27。候选源码：`origin/baseline/dev` 的 `b6f6b67d3348921fc179aa2225896ef3472e7aca`。环境：原生 Windows `10.0.26200.0`。本调查只处理开发工作树中的 Camoufox/Gecko 启动差异，没有改产品代码、Engine 内容、安全策略或安装流程。

## 结论与边界

**已确认的因果变量是 browser tree 的继承 DACL。** 在普通用户 `telecaster\qiu`、同一 browser executable 绝对路径、同一 Playwright driver 和相同启动选项下，隔离副本继承工作树的 DACL 时 60 秒内无法建立页面；仅将该副本全树 DACL 重置为普通父目录的继承规则后，5.344 秒建立一个页面。owner 仍为 `qiu`，文件内容不变。重置前根目录/`camoufox.exe` 分别有 21/14 条 ACL 规则，之后为 5/5 条。504 个文件与原失败树逐项 SHA-256 一致。

失败首先出现在 **Gecko sandbox broker 派生 GPU、tab、utility 子进程**：主 `camoufox.exe` 已创建、Juggler pipe 已通信，随后记录 `Failed to launch tab subprocess @SB::LA::SpawnTarget (Error:0)`、GPU 连续三次启动失败及 `gBrowser never populated`。这将 owning layer 收敛到文件系统安全描述符与 Gecko 子进程启动的交界处。日志没有给出具体 Windows API 的拒绝码或某个 DLL 的加载失败；没有把单条 ACE、Defender、loader 或 Chromium sandbox 指认为更深层根因。Camoufox 此处自报 `Firefox/152.0.4-beta.28`，并非 Chromium。

失败树由 `CodexSandboxOnline` 拥有，其继承 DACL 含七组 `S-1-15-2-*` package SID 规则及额外 sandbox 用户规则；普通成功副本由 `qiu` 拥有，继承五条普通用户/SYSTEM/Administrators 规则。owner 差异本身不足以解释结果：保留失败 DACL、但 owner 为 `qiu` 的副本仍失败。尚未细分是哪一条 ACE、根目录还是子文件的规则触发 Gecko 子进程失败。

## 受控结果

以下最小 probe 均复用失败 package 的 Playwright driver，去除 `CAMOU_CONFIG_*`/`VERISILO_*`，使用新的临时 Profile、`about:blank`、headful、60 秒超时；不经过 Host、supervisor、Vault 或产品 Job。实际 executable 由 Playwright `<launching>` 日志确认。`W` 是原失败树 `C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\integration-performance-completion-37abe4\apps\desktop\src-tauri\target\debug\managed-browser\engine-package\browser`；`U` 是普通用户目录 `C:\Users\qiu\src\VeriSiloEngineProbe`。完整原始日志位于本 QA 工作树的忽略目录 `artifacts/managed-directory/`。

| 父进程账户 | browser executable | 文件/目录条件 | 结果 |
| --- | --- | --- | --- |
| 历史探针未记录账户 | `W/camoufox.exe` | 原工作树 ACL | 失败；Juggler 正常，GPU 三次失败 |
| `CodexSandboxOnline` | 工作树内指向 `W` 的短 junction | 同一原文件对象 | 失败；路径别名本身无效 |
| `CodexSandboxOnline` | `U/browser/camoufox.exe` | `W` 的 504 个硬链接，原文件 DACL/owner | 失败 |
| `CodexSandboxOnline` | `U/copy-browser/camoufox.exe` | 独立副本，普通继承 DACL/owner `qiu` | 失败 |
| `qiu` | `W/camoufox.exe` | 原工作树 ACL | 失败；GPU 三次失败 |
| `qiu` | `U/browser/camoufox.exe` | `W` 的硬链接，原文件 DACL/owner | 失败 |
| `qiu` | `U/copy-browser/camoufox.exe` | 独立副本，普通继承 DACL/owner `qiu` | **通过：4.894 秒、1 page** |
| `qiu` | `U/dacl-browser/camoufox.exe` | 通过 `robocopy /COPY:DATS` 保留失败树 DACL，owner `qiu` | **失败：60 秒超时** |
| `qiu` | **同一** `U/dacl-browser/camoufox.exe` | 仅对这份隔离副本执行 `icacls /reset /T`，owner、路径和文件内容未变 | **通过：5.344 秒、1 page** |

普通用户目录的独立副本与 DACL 对照副本各有 504 个文件；逐项 SHA-256 与 `W` 比较，失配数均为 0。两次 `dacl-browser` 启动之间唯一有意修改的是该隔离树的 DACL。每次 probe 使用新的空 Profile 路径，不能据此宣称 Profile 字符串逐字相同；既有成功对照的 Profile 也位于工作树内，因此工作树 Profile 本身不是充分失败条件。探针结束后的相关 `camoufox.exe` 残留为 0。临时 `U` 副本和 junction 已清理；原 Engine 树仍在。

`CodexSandboxOnline` 是另一条开发环境边界：它对普通目录的干净副本及已安装 LocalAppData 浏览器的本次最小 probe 也失败；这些失败不用于否定 `qiu` 的 DACL 对照，也不能推断正常产品路径失败。账户来自 `whoami /user`，不能由继承的 `USERNAME=qiu` 环境变量推断。尚未把该 sandbox 账户的子进程限制进一步归因于某个 Windows 机制。

## 产品处置

既有完整产品证据见 [性能与架构任务处置](performance-architecture-2026-09-27.md)：独立 LocalAppData 开发目录、新 Vault `perf-native-37abe4`、日志确认真实 Engine binding 后，Managed 创建、页面 `readyState=complete`、网络、`observed`/`matched`、reobserve、stop/restart 和进程清理均通过。此次未改该 package 或产品源码，该证据仍可继承。正式安装/运行路径没有被本次 ACL 对照证明存在缺陷；目前将其界定为 **当前 Codex sandbox/worktree 的继承 ACL 开发限制**，不修改产品沙箱、Job、Defender、全局 ACL 或安装器。

开发时若要复现真实 Managed 路径，应在正常用户上下文使用普通继承 ACL 的独立包目录，并用新 Vault 注册/绑定；以 driver 日志中的 Engine package root 和 browser executable 为准。不要只按桌面 exe 所在目录判断 binding，也不要直接改原工作树 ACL。上述对照证明 DACL 的因果作用，但没有证明清理原工作树 ACL 后 `CodexSandboxOnline` 能运行 Engine。

## 最小复现与未取得的证据

专用脚本：[managed-engine-directory-probe.cjs](../../tests/windows/managed-engine-directory-probe.cjs)。命令：

```powershell
whoami /user
$env:DEBUG = 'pw:browser'
node tests/windows/managed-engine-directory-probe.cjs <engine-package> <browser-exe> <output-dir>
```

脚本输出实际 package root、executable、real path、cwd、Profile、账户及 TEMP/TMP/LocalAppData，随后输出页面结果或 Playwright 错误。它不使用 Vault；产品 binding 的独立证据来自上述完整产品日志。脚本在 DACL 重置后的隔离目录实测通过：6.679 秒、1 page。未取得 GPU 子进程退出码、具体 `CreateProcess`/loader 失败码，也未逐项比对 inherited handles、Job membership 或 MSIX identity；因此不对这些更深层机制作结论。若未来正常产品目录出现同类故障，需在那个确切目录与账户重新采集原生证据，不能套用本次开发限制结论。
