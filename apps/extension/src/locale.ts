export type UiLocale = "zh-CN" | "en";

const UI_LOCALE_KEY = "ui:locale";
const SKIP_LOCALIZATION_SELECTOR = "script, style, pre, code, [data-no-i18n]";

let currentLocale: UiLocale = browserDefaultLocale();
let localizationObserver: MutationObserver | null = null;
const originalText = new WeakMap<Text, string>();
const originalAttributes = new WeakMap<Element, Map<string, string>>();

const ENGLISH_TEXT: Record<string, string> = {
  "观察当前浏览器，长期状态交给独立 Silo":
    "Inspect this browser; keep long-term identities in separate Silos",
  仅本机: "Local only",
  信号概览: "Overview",
  临时空间: "Private space",
  实验室: "Labs",
  技术数据: "Technical data",
  当前页面观测: "Current page scan",
  网站当前能读取什么: "What this site can read",
  "提取可能参与账号关联、地区一致性和设备识别的信息。结果只代表当前页面 能读取到的有限内容，不是身份认证；页面报告默认留在本机，网络检查只有点击后才会联网。":
    "Inspect information that may contribute to account linking, region consistency, and device recognition. Results cover only what this page can read and are not identity verification. Page reports stay local by default; network checks run only when you click them.",
  扫描当前页面: "Scan current page",
  "扫描提示“无法访问”？": "Scan says it cannot access the page?",
  "先在目标网页点击工具栏中的 VeriSilo 图标。仍失败时，再请求该站点访问权限。":
    "Click the VeriSilo toolbar icon on the target page first. If scanning still fails, request access to this site.",
  请求当前站点权限: "Request access to this site",
  撤销当前站点长期权限: "Revoke persistent site access",
  还没有扫描结果: "No scan results yet",
  "打开你想检查的网站，再点击上面的扫描按钮。结果会自动整理成可读结论。":
    "Open the site you want to inspect, then click the scan button above. Results will be organized into readable findings.",
  关键浏览器信号: "Key browser signals",
  当前页面能够读取的六组信息: "Six groups of information this page can read",
  重点: "Highlights",
  扫描说明: "How scanning works",
  结果说明: "About this result",
  扫描边界: "Scan boundaries",
  使用前须知: "Before you begin",
  临时空间说明: "About private space",
  桌面端说明: "About the desktop app",
  技术说明: "Technical notes",
  本机报告历史: "Local report history",
  观察限制: "Observation limits",
  值得关注: "Worth reviewing",
  "从全部信号中提炼的解读，不是扫描进度":
    "Interpretation of all signals, not scan progress",
  "0 条解读": "0 findings",
  "本次扫描没有发现矛盾，不等于网站无法识别或关联你。扩展只观察有限页面信号， 不控制指纹；网站还可能结合网络出口、底层网络协议、行为和账号关系。":
    "No conflict found in this scan does not mean the site cannot recognize or link you. The extension observes a limited set of page signals and does not control fingerprints; sites may also use network egress, lower-level protocols, behavior, and account relationships.",
  "高级诊断工具 · 默认关闭": "Advanced diagnostic tool · Off by default",
  临时检查网页是否泄漏身份标记:
    "Temporarily check whether a page leaks an identity marker",
  "实验室面向需要排查网站兼容性或身份泄漏的高级用户。只有你手动开启后， 它才会在当前网站放入一个不含账号或个人信息的一次性随机测试标记， 检查页面、内嵌页面和开启后新建的后台任务之间是否异常传播。 普通浏览和多账号隔离不需要开启，请使用“信号概览”或桌面 Silo。":
    "Labs is for advanced users investigating site compatibility or identity leakage. Only after you enable it does VeriSilo place a one-time random test marker containing no account or personal data on the current site, then check whether it spreads unexpectedly between the page, embedded pages, and newly created background tasks. Normal browsing and multi-account separation do not require Labs; use Overview or a desktop Silo.",
  默认不运行: "Off by default",
  只有点击开启才会改变当前网页: "Changes the page only after you enable it",
  不含个人信息: "Contains no personal data",
  只使用一次性随机测试标记: "Uses only a one-time random test marker",
  异常自动恢复: "Automatically restores on problems",
  "仅当前网站，最长 2 分钟": "Current site only, for up to 2 minutes",
  "可能让当前网站短暂异常。开启前请保存正在编辑的内容；发现异常、 权限变化或测试标记泄漏时会立即停止并恢复。没有桌面连接时不会保存实验授权。":
    "This may briefly disrupt the current site. Save any edits before enabling it. VeriSilo stops and restores the page immediately if it detects an error, a permission change, or test-marker leakage. Experiment authorization is not saved without a desktop connection.",
  网页后台任务一致性: "Web background-task consistency",
  "检查开启实验后新建的网页后台任务和同站内嵌页面，是否意外 读到同一个随机测试标记。受浏览器限制，这项检查只能覆盖部分场景。":
    "Check whether background tasks created after enabling the experiment, or embedded pages on the same site, unexpectedly read the same random test marker. Browser limitations restrict this check to some scenarios.",
  有限覆盖: "Limited coverage",
  默认关闭: "Off by default",
  尚未为当前网站开启实验: "The experiment is not enabled for this site.",
  "尚未为当前网站开启实验。": "The experiment is not enabled for this site.",
  "尚无实验记录。": "No experiment records yet.",
  "为当前站点开启 2 分钟": "Enable for this site for 2 minutes",
  立即恢复并停用: "Restore and stop now",
  覆盖范围与停止条件: "Coverage and stop conditions",
  "只覆盖开启检查后新建的部分同站网页后台任务。":
    "Covers only some same-site background tasks created after the check starts.",
  "不覆盖更早运行的脚本、已经存在的任务、跨站任务或浏览器长期后台任务。":
    "Does not cover earlier scripts, existing tasks, cross-site tasks, or long-lived browser background tasks.",
  "用户点击后才开始检查，无法证明早于网站自身脚本运行， 因此结果只能标为“有限覆盖”，不能当成完整验证。":
    "The check begins only after you click, so it cannot prove that it ran before the site's own scripts. Results are therefore limited coverage, not complete verification.",
  "异常、超时、权限变化或随机测试标记泄漏时，都会恢复原网页行为并停止检查。":
    "Errors, timeouts, permission changes, or test-marker leakage restore the original page behavior and stop the check.",
  为网站数据创建独立空间: "Create a separate space for site data",
  "浏览器扩展无法可靠分开网站的全部登录信息、存储和后台任务。 实验室只观察页面可见的随机测试标记，不读取或改写登录内容。":
    "A browser extension cannot reliably separate all site sign-in data, storage, and background tasks. Labs observes only a page-visible random test marker and does not read or modify sign-in content.",
  当前扩展不支持: "Not supported by this extension",
  "可用替代：桌面端为每个身份创建独立浏览器资料目录， 由浏览器分开登录信息、网站数据、权限和后台任务。":
    "Alternative: the desktop app creates a separate browser profile directory for each identity, allowing the browser to separate sign-in data, site data, permissions, and background tasks.",
  拦截所有网站数据写入: "Intercept all site-data writes",
  "当前扩展不会拦截或改写网站的全部网络响应，也不会申请永久访问所有网站。":
    "This extension does not intercept or rewrite every site response and does not request permanent access to every site.",
  "需要网络层控制时，应使用专门的受控环境；桌面端的独立浏览器资料目录 是当前可靠的网站数据边界。":
    "Use a dedicated controlled environment when network-layer control is required. The desktop app's separate browser profile directories are the current reliable site-data boundary.",
  最近实验记录: "Recent experiment records",
  "正在读取本机脱敏实验记录…": "Loading local redacted experiment records…",
  清除记录: "Clear records",
  浏览器原生临时边界: "Browser-provided temporary boundary",
  打开一个临时隐私窗口: "Open a temporary private window",
  "Chrome 无痕 / Edge InPrivate 与普通窗口的登录信息、网站存储和缓存分开， 适合临时登录。它不会创建多个长期身份；全部隐私窗口仍共享浏览器的同一个临时空间。":
    "Chrome Incognito and Edge InPrivate separate sign-in data, site storage, and cache from regular windows, making them useful for temporary sign-ins. They do not create multiple long-term identities; all private windows still share one temporary browser space.",
  由浏览器自身提供临时网站数据隔离:
    "Temporary site-data separation provided by the browser",
  用当前网站打开隐私窗口: "Open current site in a private window",
  "正在检查隐私窗口权限…": "Checking private-window permission…",
  减少网络旁路信号: "Reduce network-bypass signals",
  "限制部分可能绕过代理的直接网络连接，并关闭网络预测。 设置作用于当前浏览器环境，不是单个账号；状态显示成功也不代表所有网络流量都已验证。":
    "Limit some direct connections that may bypass a proxy and disable network prediction. These settings affect the current browser environment, not one account; a successful status does not verify all network traffic.",
  尽力: "Best effort",
  直接连接保护未检查: "Direct-connection protection: not checked",
  "直接连接保护：未检查": "Direct-connection protection: not checked",
  "网络预测：未检查": "Network prediction: not checked",
  启用推荐保护: "Enable recommended protection",
  恢复原设置: "Restore original settings",
  "扩展与 Standard Silo 的能力边界":
    "Extension and Standard Silo capability boundaries",
  "扩展只做信息查看和临时工具；长期浏览器资料隔离由 VeriSilo 桌面端完成。":
    "The extension provides information and temporary tools. Long-term browser-profile separation is provided by the VeriSilo desktop app.",
  临时登录空间: "Temporary sign-in space",
  "隐私窗口与普通窗口分开；全部隐私窗口共享一个临时上下文":
    "Private windows are separate from regular windows; all private windows share one temporary context",
  浏览器提供: "Browser-provided",
  长期网站状态: "Persistent site state",
  每个桌面身份使用独立浏览器资料目录保存登录信息和网站数据:
    "Each desktop identity uses a separate browser profile directory for sign-in and site data",
  桌面端提供: "Desktop-provided",
  页面信号扫描: "Page-signal scan",
  "只表示当前页面通道观测到的值，不是独立进程或身份验证":
    "Shows only values observed through the current page; it is not process isolation or identity verification",
  有限观察: "Limited observation",
  设备与浏览器指纹: "Device and browser fingerprint",
  "Standard Silo 跟随这台电脑和系统 Chrome / Edge":
    "Standard Silo inherits this computer and its system Chrome or Edge",
  跟随本机: "Inherits this device",
  受控指纹: "Controlled fingerprint",
  "扩展和 Standard Silo 均不提供受控指纹能力":
    "Neither the extension nor Standard Silo provides controlled fingerprints",
  不提供: "Unavailable",
  网络检查: "Network check",
  "仅在用户点击后查看当前浏览器的网络出口；结果可以交给正在运行的桌面身份":
    "Checks the current browser's network egress only after you click; results can be sent to the active desktop identity",
  按需检查: "On-demand",
  底层网络路径: "Lower-level network path",
  "当前扩展不控制，也不把公共域名解析服务的答案对比当成实际路径证明":
    "The extension does not control this and does not treat public DNS answer comparisons as proof of the actual path",
  "需要多个长期账号身份？": "Need multiple persistent account identities?",
  "VeriSilo 桌面端用独立浏览器资料目录分开并持久保存网站数据； 设备与浏览器特征继续跟随本机，不提供指纹控制，也不把观察结果称为身份认证。":
    "The VeriSilo desktop app separates and persists site data in independent browser profile directories. Device and browser characteristics still inherit the local machine; fingerprint control is not provided, and observations are not presented as identity verification.",
  "打开 VeriSilo 桌面端": "Open VeriSilo desktop",
  查看项目页: "View project page",
  高级信息: "Advanced information",
  原始信号与报告导出: "Raw signals and report export",
  "这里保留给开发者核对证据。日常使用只需查看“信号概览”；导出时会默认隐藏高敏感值。":
    "This section is for developers checking evidence. For everyday use, Overview is sufficient. High-sensitivity values are hidden by default in exports.",
  "导出脱敏 JSON": "Export redacted JSON",
  "导出可读 HTML": "Export readable HTML",
  清除本地报告历史: "Clear local report history",
  "本机最多保留 20 份脱敏报告，30 天后自动清理。":
    "Up to 20 redacted reports are kept locally and removed after 30 days.",
  "扫描后将在这里显示技术数据。":
    "Technical data will appear here after a scan.",
  "“未观察到调用”不等于网站没有采集。页面深入观察由用户点击后开始， 不能保证早于网站自身脚本，也不覆盖所有网页和浏览器后台任务。":
    "Not observing a call does not mean the site collected nothing. Deeper page observation begins after you click, cannot be guaranteed to run before the site's own scripts, and does not cover every page or browser background task.",
  "开启当前站点的网页后台任务检查？它可能让网站短暂异常；发现测试标记泄漏、页面异常、超时或权限变化时会立即恢复并停用。":
    "Enable the web background-task check for this site? It may briefly disrupt the site. VeriSilo will restore and stop it if a test marker leaks, the page errors, the check times out, or permissions change.",
  "正在开始检查…": "Starting check…",
  "未授予当前站点权限；实验保持关闭，也没有注入页面。":
    "Site permission was not granted. The experiment remains off and nothing was injected into the page.",
  "实验室返回了无法识别的状态。": "Labs returned an unrecognized status.",
  "网页后台任务检查已启动；覆盖范围有限，只在当前浏览器中临时运行。":
    "The web background-task check is running with limited coverage in this browser only.",
  "网页后台任务检查已启动，并关联当前桌面身份与网站；由于无法覆盖所有网页运行区域，状态标为“有限覆盖”。":
    "The web background-task check is running and linked to the current desktop identity and site. Because it cannot cover every page execution area, its status is limited coverage.",
  "检测到随机测试标记泄漏；原网页行为已恢复，当前检查已停用。":
    "Random test-marker leakage was detected. Original page behavior was restored and the check was stopped.",
  "检查未能完成确认，已恢复原网页行为并停用当前站点。":
    "The check could not be confirmed. Original page behavior was restored and the check was stopped for this site.",
  "无法开启网页后台任务检查，请稍后重试。":
    "Could not start the web background-task check. Try again shortly.",
  "正在恢复…": "Restoring…",
  "已恢复原网页行为，并停用当前站点检查。":
    "Original page behavior was restored and the check was stopped for this site.",
  "无法停止当前检查，请稍后重试。":
    "Could not stop the current check. Try again shortly.",
  状态不可用: "Status unavailable",
  "暂时无法读取实验状态，请稍后重试。":
    "Experiment status is temporarily unavailable. Try again shortly.",
  "暂时无法读取本地实验收据。":
    "Local experiment records are temporarily unavailable.",
  "清除全部本地脱敏实验记录？": "Clear all local redacted experiment records?",
  "正在清除…": "Clearing…",
  "无法清除实验记录，请稍后重试。":
    "Could not clear experiment records. Try again shortly.",
  "尚未为当前网站开启检查。": "The check is not enabled for this site.",
  "尚未为当前网站开启检查；默认关闭。":
    "The check is not enabled for this site and is off by default.",
  "不覆盖开启检查前已运行、跨站或浏览器长期运行的后台任务":
    "Does not cover background tasks already running before the check, cross-site tasks, or long-lived browser tasks",
  尚未执行恢复: "Restoration has not run",
  没有补充记录: "No additional record",
  开始前检查: "Pre-check",
  启用检查: "Enable check",
  结果确认: "Result confirmation",
  恢复网页: "Restore page",
  运行中无停止条件: "Running; no stop condition",
  随机测试标记传播到其他标签页: "Random test marker spread to another tab",
  随机测试标记传播到内嵌页面: "Random test marker spread to an embedded page",
  随机测试标记传播到网页后台任务:
    "Random test marker spread to a web background task",
  随机测试标记出现在浏览器后台任务地址中:
    "Random test marker appeared in a browser background-task URL",
  随机测试标记出现在页面可见网站数据中:
    "Random test marker appeared in page-visible site data",
  随机测试标记传播到页面环境:
    "Random test marker spread to the page environment",
  页面异常: "Page error",
  网页后台任务异常: "Web background-task error",
  运行超时: "Timed out",
  站点权限已撤销或被接管: "Site permission was revoked or taken over",
  页面已切换: "Page changed",
  超出实验范围: "Outside experiment scope",
  验证失败: "Confirmation failed",
  扩展上下文丢失: "Extension context was lost",
  用户手动停止: "Stopped by the user",
  授权已过期: "Authorization expired",
  缺少站点权限: "Site permission required",
  正在开启: "Enabling",
  检查通过: "Check passed",
  失败并停用: "Failed and stopped",
  泄漏即停: "Leak detected; stopped",
  已恢复: "Restored",
  不支持: "Not supported",
  "正在扫描…": "Scanning…",
  "扫描未在 7 秒内完成。页面可能阻止了某项浏览器信号；请重试或查看扩展错误日志。":
    "The scan did not finish within 7 seconds. The page may have blocked a browser signal; try again or check the extension error log.",
  "基础扫描已完成。页面主环境观察不可用，结论已明确标注覆盖边界。":
    "The basic scan completed. Deeper page observation was unavailable, and the result clearly marks the coverage boundary.",
  "扫描已完成，结果已整理为身份结论。":
    "Scan complete. Results have been organized into browser-identity findings.",
  "扫描失败，请稍后重试。": "Scan failed. Try again shortly.",
  "正在检查…": "Checking…",
  "当前站点权限已授予，可以扫描或运行明确开启的实验室检查。":
    "Access to this site was granted. You can scan or run an explicitly enabled Labs check.",
  "未授予当前站点权限；VeriSilo 没有注入或扫描页面。":
    "Access to this site was not granted. VeriSilo did not inject into or scan the page.",
  "工具栏已为当前标签页授予一次性访问权限，可以直接扫描；跨站导航或关闭标签页后会自动失效。":
    "The toolbar granted one-time access to this tab. You can scan now; access expires after cross-site navigation or when the tab closes.",
  "当前站点已有长期访问权限，无需重复请求。可以直接扫描。":
    "Persistent access to this site already exists. You can scan without requesting it again.",
  "已向浏览器发起站点访问请求。请点击地址栏中的“允许”提示，授权后再扫描。":
    "A site-access request was sent to the browser. Select Allow in the address-bar prompt, then scan again.",
  "未能发起当前站点访问请求。":
    "Could not start a site-access request for the current page.",
  "无法更改当前站点权限，请稍后重试。":
    "Could not change permission for this site. Try again shortly.",
  "正在撤销…": "Revoking…",
  "已撤销当前站点的长期访问权限；正在运行的该站点实验室检查也会停止并恢复。":
    "Persistent access to this site was revoked. Any active Labs check for the site will also stop and restore.",
  "当前站点没有长期访问权限。工具栏授予的一次性权限会在跨站导航或关闭标签页后失效。":
    "This site has no persistent access grant. One-time toolbar access expires after cross-site navigation or when the tab closes.",
  "无法撤销当前站点权限，请稍后重试。":
    "Could not revoke permission for this site. Try again shortly.",
  "正在打开…": "Opening…",
  "浏览器没有创建隐私窗口。": "The browser did not create a private window.",
  "已在 Chrome 无痕 / Edge InPrivate 中打开当前网站。它与普通窗口的网站数据分开；关闭全部隐私窗口后临时网站数据会被清除。":
    "Opened the current site in Chrome Incognito or Edge InPrivate. Its site data is separate from regular windows and is cleared after all private windows close.",
  "无法打开隐私窗口，请检查扩展设置。":
    "Could not open a private window. Check the extension settings.",
  "正在验证…": "Confirming…",
  "未授予隐私控制权限，VeriSilo 没有更改浏览器设置。":
    "Privacy-control permission was not granted. VeriSilo did not change browser settings.",
  "推荐保护已应用并复查：部分直接网络连接已限制，网络预测已关闭。":
    "Recommended protection was applied and checked: some direct connections are restricted and network prediction is off.",
  "无法启用推荐保护，请稍后重试。":
    "Could not enable recommended protection. Try again shortly.",
  "部分设置未能恢复；它可能已被浏览器策略或其他扩展接管。":
    "Some settings could not be restored; they may be controlled by browser policy or another extension.",
  "无法恢复原设置，请稍后重试。":
    "Could not restore the original settings. Try again shortly.",
  "未检测到可连接的 VeriSilo 桌面端。扩展仍可独立扫描和使用临时工具；安装并启动兼容的桌面端后才能使用桌面联动。":
    "No reachable VeriSilo desktop app was detected. The extension can still scan and use temporary tools independently; desktop integration requires a compatible installed and running desktop app.",
  "已打开 VeriSilo 桌面端。": "Opened the VeriSilo desktop app.",
  "无法连接 VeriSilo 桌面端。":
    "Could not connect to the VeriSilo desktop app.",
  "已打开 VeriSilo 项目页。": "Opened the VeriSilo project page.",
  "无法打开 VeriSilo 项目页。": "Could not open the VeriSilo project page.",
  "暂时无法读取扫描结果。": "Scan results are temporarily unavailable.",
  "已打开本机保存的脱敏报告。原始高敏值不会恢复。":
    "Opened a locally saved redacted report. Original high-sensitivity values are not restored.",
  "暂时无法读取本地报告历史状态。":
    "Local report history is temporarily unavailable.",
  "清除本机保存的全部脱敏报告历史？此操作不可撤销。":
    "Clear all locally saved redacted report history? This cannot be undone.",
  "无法清除本地报告，请稍后重试。":
    "Could not clear local reports. Try again shortly.",
  "已允许隐私窗口。注意：所有 Chrome 无痕 / Edge InPrivate 窗口共享同一个临时空间，不等于多个独立账号容器。":
    "Private windows are allowed. Note that all Chrome Incognito or Edge InPrivate windows share one temporary space; they are not separate account containers.",
  "首次使用需要在“扩展管理 → VeriSilo Companion”中允许扩展在 Chrome 无痕 / Edge InPrivate 中运行。":
    "Before first use, open Extension management → VeriSilo Companion and allow the extension to run in Chrome Incognito or Edge InPrivate.",
  直接连接保护: "Direct-connection protection",
  网络预测: "Network prediction",
  "暂时无法检查隐私窗口权限。":
    "Private-window permission is temporarily unavailable.",
  "未授予三方检测端点权限，没有发送网络检查请求。":
    "Access to the external check services was not granted. No network-check request was sent.",
  "网络检查返回了无法识别的结果。":
    "The network check returned an unrecognized result.",
  "当前浏览器环境的出口检查完成，结果已交给正在运行的桌面身份。两家公共域名解析服务只做答案对比，不能证明浏览器实际使用的解析路径。":
    "The current browser egress check completed and was sent to the active desktop identity. The two public DNS services only compare answers and do not prove the browser's actual resolver path.",
  "当前浏览器环境的出口检查完成，结果仅在扩展本地显示。两家公共域名解析服务只做答案对比，不能证明浏览器实际使用的解析路径。":
    "The current browser egress check completed and is shown only in the extension. The two public DNS services only compare answers and do not prove the browser's actual resolver path.",
  "网络检查没有获得有效结果，请查看网络或扩展权限后重试。":
    "The network check produced no usable result. Check the network and extension permissions, then try again.",
  "网络检查失败，请稍后重试。": "Network check failed. Try again shortly.",
  "无法清除网络检查结果。": "Could not clear the network-check result.",
  同意并检查当前环境出口: "Allow and check current egress",
  清除结果并撤销权限: "Clear result and revoke permission",
  "结果属于当前浏览器环境；成功交给正在运行的桌面身份后，桌面端会显示这次结果。点击后会连接 ipwho.is、Cloudflare 1.1.1.1 和 Google Public DNS，这些服务会看到请求的公网地址。两家域名解析服务只做答案对比，不能证明浏览器实际使用的解析路径。不会自动运行。":
    "This result belongs to the current browser environment. After successful handoff to an active desktop identity, the desktop app will show it. Clicking connects to ipwho.is, Cloudflare 1.1.1.1, and Google Public DNS; those services will see the requesting public IP. The DNS services compare answers only and do not prove the browser's actual resolver path. The check never runs automatically.",
  同意并重新检查: "Allow and check again",
  "点击后可查看当前浏览器环境的公网地址、出口地区、网络运营商、时区或语言建议，以及两家公共域名解析服务的答案对比。":
    "Click to inspect the current browser's public IP, egress region, network provider, time-zone and language suggestions, and answer comparison from two public DNS services.",
  "点击后可查看当前浏览器环境的公网地址、出口地区、网络运营商、时区/语言建议，以及两家公共域名解析服务的答案对比。":
    "Click to inspect the current browser's public IP, egress region, network provider, time-zone or language suggestions, and answer comparison from two public DNS services.",
  公网地址获取失败: "Could not obtain public IP",
  "一个或多个检测服务没有返回有效结果。":
    "One or more check services returned no usable result.",
  公网地址未确认: "Public IP not confirmed",
  公网地址已确认: "Public IP confirmed",
  运营商未知: "Provider unknown",
  已确认: "confirmed",
  未确认: "not confirmed",
  未能确认恢复: "could not confirm restoration",
  通过: "passed",
  失败: "failed",
  未执行: "skipped",
  并撤销了隐私控制权限: "and revoked the privacy-controls permission",
  "，并撤销了隐私控制权限": ", and revoked the privacy-controls permission",
  并撤销检测服务访问权限: "and revoked access to the network-check services",
  "，并撤销检测服务访问权限":
    ", and revoked access to the network-check services",
  出口时区: "Egress time zone",
  云或机房线路线索: "Cloud or hosting-network indicator",
  "云/机房线路线索": "Cloud or hosting-network indicator",
  线路类型未判定: "Network type not determined",
  浏览器与出口时区一致: "Browser and egress time zones match",
  浏览器与出口时区不一致: "Browser and egress time zones differ",
  语言地区与出口国家一致: "Language region and egress country match",
  语言地区与出口国家不同仅建议: "Language region and egress country differ",
  "语言地区与出口国家不同（仅建议）":
    "Language region and egress country differ (informational only)",
  两家域名解析结果一致: "Both DNS services returned matching answers",
  两家域名解析结果有差异: "The DNS services returned different answers",
  域名解析服务返回错误: "A DNS service returned an error",
  仅一家域名解析服务可用: "Only one DNS service was available",
  域名解析检查失败: "DNS comparison failed",
  两家解析服务均通过安全校验: "Both DNS services passed security validation",
  域名解析安全校验不完整: "DNS security validation was incomplete",
  公网地址信誉未评分: "Public-IP reputation not scored",
  桌面端已接收结果: "Desktop app received the result",
  桌面端结果已过期: "Desktop result expired",
  "桌面不可用 · 仅本地": "Desktop unavailable · Local only",
  "桌面身份未运行 · 仅本地": "No active desktop identity · Local only",
  "桌面拒绝接收 · 仅本地": "Desktop rejected result · Local only",
  仅本地显示: "Shown locally only",
  "请先扫描当前页面。": "Scan the current page first.",
  "导出前会默认脱敏高敏感信号。是否继续？":
    "High-sensitivity values will be redacted by default before export. Continue?",
  "已导出默认脱敏的本地报告。": "Exported a locally redacted report.",
  无值: "No value",
  无法安全显示该值: "This value cannot be displayed safely",
  页面环境未检查: "Page environment not checked",
  页面环境已观察: "Page environment observed",
  页面环境部分覆盖: "Page environment partially covered",
  页面环境不可用: "Page environment unavailable",
  已确认当前站点权限: "Current-site permission confirmed",
  已读取当前页面环境: "Current page environment read",
  已确认可恢复原网页行为: "Original page behavior can be restored",
  已启用临时检查: "Temporary check enabled",
  新建后台任务响应正常: "New background task responded normally",
  同站内嵌页面结果一致: "Same-site embedded-page result is consistent",
  页面可见网站数据未发现测试标记:
    "No test marker found in page-visible site data",
  浏览器后台任务地址未发现测试标记:
    "No test marker found in browser background-task URLs",
  其他同站标签页未发现测试标记: "No test marker found in other same-site tabs",
  无法确认检查早于网站脚本: "Cannot confirm the check ran before site scripts",
  已确认恢复原网页行为: "Original page behavior restored",
  未能确认恢复原网页行为: "Could not confirm restoration of page behavior",
  未检查: "Not checked",
  无法确认早于网站脚本: "Cannot confirm it ran before site scripts",
  已确认从页面载入时生效: "Confirmed active from page load",
  仅检查开启后新建的同站后台任务:
    "Checks only same-site background tasks created after enabling",
  检查失败: "Check failed",
  同站内嵌页面检查通过: "Same-site embedded-page check passed",
  仅检查页面可见的随机测试标记: "Checks only page-visible random test markers",
  仅检查后台任务的注册地址: "Checks only background-task registration URLs",
  仅完成扩展自检: "Extension self-test only",
  浏览器与设备: "Browser and device",
  浏览器版本与系统信息: "Browser version and system information",
  时区: "Time zone",
  屏幕: "Display",
  图形绘制特征摘要: "Graphics-rendering fingerprint summary",
  显卡信息: "Graphics information",
  新一代图形功能可用性: "Modern graphics feature availability",
  音频特征摘要: "Audio fingerprint summary",
  字体可见性: "Font visibility",
  摄像头与麦克风: "Camera and microphone",
  网站权限: "Site permissions",
  登录信息与网站存储: "Sign-in data and site storage",
  直接网络连接: "Direct network connections",
  内嵌页面环境: "Embedded-page environment",
  网页后台任务环境: "Web background-task environment",
  页面读取到的浏览器信息: "Browser information read by the page",
  其他浏览器信号: "Other browser signal",
  已读取: "Read",
  被页面阻止: "Blocked by the page",
  浏览器不支持: "Not supported by the browser",
  读取失败: "Read failed",
  当前页面: "Current page",
  内嵌页面: "Embedded page",
  网页后台任务: "Web background task",
  网页请求信息: "Page request information",
  扩展自检: "Extension self-test",
  "浏览器未提供客户端提示信息。": "The browser did not provide client hints.",
  "当前页面无法使用二维图形能力。":
    "Two-dimensional graphics are unavailable on this page.",
  "当前页面无法使用离线音频能力。":
    "Offline audio processing is unavailable on this page.",
  "当前页面无法检查字体可见性。":
    "Font visibility cannot be checked on this page.",
  "当前页面无法检查摄像头与麦克风信息。":
    "Camera and microphone information cannot be checked on this page.",
  "当前页面无法检查同站内嵌页面。":
    "Same-site embedded pages cannot be checked on this page.",
  "当前页面无法检查网页后台任务。":
    "Web background tasks cannot be checked on this page.",
  "未获得可显示的结果。": "No displayable result was obtained.",
  "采集失败，未获得可显示的结果。":
    "Collection failed and produced no displayable result.",
  "没有找到当前标签页，请切回普通网页后重试。":
    "No active tab was found. Switch to a regular web page and try again.",
  "扩展刚刚更新，请重新打开侧栏后重试。":
    "The extension was just updated. Reopen the side panel and try again.",
  "扩展后台正在重新载入，请稍后重试。":
    "The extension background is reloading. Try again shortly.",
  "无法访问当前页面。浏览器内部页面、扩展商店和 PDF 不支持此操作。":
    "This page cannot be accessed. Browser-internal pages, extension stores, and PDFs are not supported.",
  "没有获得所需权限，操作未执行。":
    "The required permission was not granted. Nothing was changed.",
  未知: "Unknown",
  未知平台: "Unknown platform",
  浏览器与系统: "Browser and system",
  "当前页面可以读取这些值；扩展只负责展示，不会改变或认证它们。":
    "The current page can read these values. The extension only displays them; it does not modify or verify them.",
  语言与时区: "Language and time zone",
  "语言、时区和网络出口地区若不符合预期，网站可能看到不自然的组合。":
    "If language, time zone, and network egress region do not match expectations, sites may see an unusual combination.",
  网络出口: "Network egress",
  尚未检查: "Not checked yet",
  "公网地址、网络运营商、出口地区和域名解析可能影响网站看到的地区；扩展不会自动连接检测服务。":
    "Public IP, network provider, egress region, and DNS may affect the region a site sees. The extension never contacts check services automatically.",
  登录与站点数据: "Sign-in and site data",
  "同一普通浏览器环境中的登录信息和网站数据会继续关联访问；隐私窗口只提供临时隔离，长期分离需要桌面端的独立身份。":
    "Sign-in and site data in the same regular browser environment continue to link visits. Private windows provide temporary separation; persistent separation requires an independent desktop identity.",
  设备与屏幕: "Device and display",
  "分辨率、缩放比例、内存和 CPU 线程数会增加设备辨识度。":
    "Resolution, scale factor, memory, and CPU thread count can make a device more recognizable.",
  显卡与图形: "Graphics",
  "网站可通过图形功能读取显卡型号和驱动渲染方式。":
    "Sites can use graphics features to read the GPU model and driver rendering path.",
  摄像头或麦克风名称可见: "Camera or microphone names are visible",
  "页面已能看到媒体设备标签；具体设备名称可能增强身份关联。":
    "The page can see media-device labels. Specific device names may strengthen identity linking.",
  "不使用时撤销该站点的摄像头和麦克风权限。":
    "Revoke camera and microphone permission for this site when not needed.",
  图形绘制特征: "Graphics-rendering feature",
  音频特征: "Audio feature",
  字体集合: "Font set",
  页面可读取较强的设备特征: "The page can read distinctive device features",
  "桌面端的独立身份可以分开网站状态，但设备与浏览器特征仍跟随本机。":
    "Independent desktop identities can separate site state, but device and browser characteristics still inherit the local machine.",
  网站登录状态会正常保存: "Site sign-in state will be saved normally",
  "登录信息和网站存储可用；它们便于保持登录，也能关联同一浏览器环境中的访问。":
    "Sign-in data and site storage are available. They keep you signed in and can also link visits in the same browser environment.",
  "不同账户需要真正分离时，使用桌面端独立身份或临时隐私窗口。":
    "Use an independent desktop identity or a temporary private window when accounts must be separated.",
  精确位置权限已拒绝: "Precise location permission is denied",
  "该页面不能直接读取浏览器提供的精确地理位置。网络出口仍可能暴露大致地区。":
    "This page cannot directly read precise browser-provided location. Network egress may still reveal an approximate region.",
  该站点可以发送通知: "This site can send notifications",
  "通知权限不会直接泄漏密码，但会保留一项站点授权，并可能暴露账号使用痕迹。":
    "Notification permission does not reveal passwords, but it persists a site grant and may expose signs of account usage.",
  "不需要时，可在浏览器的站点权限中撤销通知。":
    "Revoke notification permission in browser site settings when it is not needed.",
  当前页面观测有几项值得关注: "A few current-page findings are worth reviewing",
  当前页面观测未发现明显矛盾:
    "No obvious conflicts found in the current-page scan",
  页面与扩展看到的浏览器信息不同:
    "The page and extension see different browser information",
  "网页主环境和扩展隔离环境返回了不同的浏览器信息。这不一定表示风险，但说明当前环境存在需要进一步检查的差异。":
    "The page environment and extension environment returned different browser information. This is not necessarily a risk, but the difference deserves further review.",
  移动端声明与触控能力不协调:
    "Mobile-device claim and touch capability do not align",
  "浏览器声明自己像移动设备，但没有报告触控点。真实设备也可能出现这种情况；这里只将它标为需要理解的组合，而不是异常判定。":
    "The browser presents itself as a mobile device but reports no touch points. Real devices can also behave this way, so this is a combination to understand rather than an automatic anomaly.",
  当前扫描未发现明显矛盾: "No obvious conflicts found in this scan",
  "这只表示本次有限扫描没有发现已实现规则能够解释的问题，不表示网站无法识别、关联或采集其他信号。":
    "This limited scan found no issue covered by its current rules. It does not mean a site cannot recognize, link, or collect other signals.",
  "某项浏览器信号无法被当前页面读取或采集。限制可能来自浏览器、权限、网站设置或扩展覆盖范围；这不是自动的隐私保护证明。":
    "A browser signal could not be read or collected from the current page. The limitation may come from the browser, permissions, site settings, or extension coverage; it is not automatic proof of privacy protection.",
  未知浏览器: "Unknown browser",
  设备信息不可用: "Device information unavailable",
  显卡型号未暴露: "GPU model not exposed",
  未能确认: "Could not confirm",
  会保留登录状态: "Sign-in state will persist",
  主要存储不可用: "Primary site storage unavailable",
  部分存储可用: "Some site storage available",
};

export function getUiLocale(): UiLocale {
  return currentLocale;
}

export async function initializeUiLocale(): Promise<UiLocale> {
  const stored = await chrome.storage.local.get(UI_LOCALE_KEY);
  const locale = parseLocale(stored[UI_LOCALE_KEY]) ?? browserDefaultLocale();
  applyUiLocale(locale, false);
  return locale;
}

export async function setUiLocale(locale: UiLocale): Promise<void> {
  applyUiLocale(locale, true);
  await chrome.storage.local.set({ [UI_LOCALE_KEY]: locale });
}

export function installUiLocalization(root: Document | HTMLElement): void {
  localizationObserver?.disconnect();
  localizeTree(root);
  localizationObserver = new MutationObserver((mutations) => {
    localizationObserver?.disconnect();
    try {
      for (const mutation of mutations) {
        if (
          mutation.type === "characterData" &&
          mutation.target instanceof Text
        ) {
          localizeTextNode(mutation.target);
        }
        for (const node of mutation.addedNodes) {
          if (node instanceof Text) {
            localizeTextNode(node);
          } else if (node instanceof Element) {
            localizeTree(node);
          }
        }
      }
    } finally {
      observeLocalizationRoot(root);
    }
  });
  observeLocalizationRoot(root);
}

export function translateUiText(text: string, locale = currentLocale): string {
  if (locale === "zh-CN") {
    return text;
  }
  const normalized = normalizeText(text);
  return ENGLISH_TEXT[normalized] ?? translateDynamicText(normalized);
}

function applyUiLocale(locale: UiLocale, announce: boolean): void {
  currentLocale = locale;
  document.documentElement.lang = locale;
  localizationObserver?.disconnect();
  localizeTree(document);
  if (localizationObserver !== null) {
    observeLocalizationRoot(document);
  }
  if (announce) {
    document.dispatchEvent(
      new CustomEvent("verisilo:locale-changed", { detail: { locale } }),
    );
  }
}

function localizeTree(root: Document | Element): void {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let node = walker.nextNode();
  while (node !== null) {
    if (node instanceof Text) {
      localizeTextNode(node);
    }
    node = walker.nextNode();
  }
  const elements =
    root instanceof Document
      ? Array.from(
          root.querySelectorAll("[aria-label], [title], [placeholder]"),
        )
      : [
          ...(root.matches("[aria-label], [title], [placeholder]")
            ? [root]
            : []),
          ...root.querySelectorAll("[aria-label], [title], [placeholder]"),
        ];
  for (const element of elements) {
    localizeAttributes(element);
  }
}

function localizeTextNode(node: Text): void {
  const parent = node.parentElement;
  if (parent === null || parent.closest(SKIP_LOCALIZATION_SELECTOR) !== null) {
    return;
  }
  const current = node.data;
  if (currentLocale === "zh-CN") {
    const source = originalText.get(node);
    if (source !== undefined) {
      node.data = source;
    }
    return;
  }
  if (containsChinese(current)) {
    originalText.set(node, current);
  }
  const source = originalText.get(node) ?? current;
  node.data = preserveOuterWhitespace(source, translateUiText(source, "en"));
}

function localizeAttributes(element: Element): void {
  if (element.closest(SKIP_LOCALIZATION_SELECTOR) !== null) {
    return;
  }
  let originals = originalAttributes.get(element);
  if (originals === undefined) {
    originals = new Map<string, string>();
    originalAttributes.set(element, originals);
  }
  for (const attribute of ["aria-label", "title", "placeholder"]) {
    const current = element.getAttribute(attribute);
    if (current === null) {
      continue;
    }
    if (currentLocale === "zh-CN") {
      const source = originals.get(attribute);
      if (source !== undefined) {
        element.setAttribute(attribute, source);
      }
      continue;
    }
    if (containsChinese(current)) {
      originals.set(attribute, current);
    }
    element.setAttribute(
      attribute,
      translateUiText(originals.get(attribute) ?? current, "en"),
    );
  }
}

function observeLocalizationRoot(root: Document | HTMLElement): void {
  localizationObserver?.observe(root, {
    childList: true,
    characterData: true,
    subtree: true,
  });
}

function translateDynamicText(text: string): string {
  const replacements: Array<[RegExp, (...values: string[]) => string]> = [
    [
      /^实验室检查已因“(.+)”停止；原网页行为(.+)。$/u,
      (reason, restoration) =>
        `The Labs check stopped because “${translateUiText(reason, "en")}”; original page behavior ${restoration === "已恢复" ? "was restored" : "could not be confirmed as restored"}.`,
    ],
    [
      /^实验室检查已因“(.+)”自动停止并恢复。$/u,
      (reason) =>
        `The Labs check stopped automatically because “${translateUiText(reason, "en")}” and restored the page.`,
    ],
    [/^(\d+) 条解读$/u, (count) => `${count} findings`],
    [/^网站：(.+)$/u, (site) => `Site: ${site}`],
    [/^扫描：(.+)$/u, (time) => `Scanned: ${time}`],
    [/^采集时间：(.+)$/u, (time) => `Collected: ${time}`],
    [/^信号组数：(\d+)$/u, (count) => `Signal groups: ${count}`],
    [
      /^建议：(.+)$/u,
      (advice) => `Suggestion: ${translateUiText(advice, "en")}`,
    ],
    [
      /^本机保存 (\d+)\/(\d+) 份脱敏收据；(\d+) 天后自动清理；下方显示最近 (\d+) 份。$/u,
      (count, maximum, days, shown) =>
        `Saved ${count}/${maximum} redacted records locally; removed after ${days} days; showing the latest ${shown}.`,
    ],
    [
      /^本机已保存 (\d+)\/(\d+) 份脱敏报告；超过 (\d+) 天自动清理。$/u,
      (count, maximum, days) =>
        `Saved ${count}/${maximum} redacted reports locally; removed after ${days} days.`,
    ],
    [
      /^已清除 (\d+) 份本地脱敏实验记录。$/u,
      (count) => `Cleared ${count} local redacted experiment records.`,
    ],
    [
      /^已清除 (\d+) 份本地脱敏报告。$/u,
      (count) => `Cleared ${count} local redacted reports.`,
    ],
    [
      /^范围：当前桌面身份 \/ (.+)。检查权限到期：(.+)。$/u,
      (site, expiry) =>
        `Scope: current desktop identity / ${site}. Permission expires: ${expiry}.`,
    ],
    [
      /^范围：(.+) 的本机临时实验，不关联桌面身份，关闭浏览器或到期即失效。$/u,
      (site) =>
        `Scope: local temporary experiment for ${site}; not linked to a desktop identity and expires when the browser closes or the authorization expires.`,
    ],
    [
      /^启用时机：(.+)$/u,
      (value) => `Start timing: ${translateUiText(value, "en")}`,
    ],
    [
      /^网页后台任务：(.+)$/u,
      (value) => `Web background tasks: ${translateUiText(value, "en")}`,
    ],
    [
      /^内嵌页面：(.+)$/u,
      (value) => `Embedded pages: ${translateUiText(value, "en")}`,
    ],
    [
      /^覆盖：(.+)；(.+)；启用时机：(.+)；网站数据：(.+)；浏览器后台任务：(.+)。$/u,
      (workers, frames, timing, siteData, browserTasks) =>
        `Coverage: ${translateUiText(workers, "en")}; ${translateUiText(frames, "en")}; start timing: ${translateUiText(timing, "en")}; site data: ${translateUiText(siteData, "en")}; browser background tasks: ${translateUiText(browserTasks, "en")}.`,
    ],
    [/^恢复(.+)$/u, (value) => `Restoration ${translateUiText(value, "en")}`],
    [
      /^(.+)：通过 · (.+)$/u,
      (phase, evidence) =>
        `${translateUiText(phase, "en")}: passed · ${translateList(evidence)}`,
    ],
    [
      /^(.+)：失败 · (.+)$/u,
      (phase, evidence) =>
        `${translateUiText(phase, "en")}: failed · ${translateList(evidence)}`,
    ],
    [
      /^(.+)：未执行 · (.+)$/u,
      (phase, evidence) =>
        `${translateUiText(phase, "en")}: skipped · ${translateList(evidence)}`,
    ],
    [
      /^当前页面读取 (\d+) 组浏览器信息，并整理成 (\d+) 条解读；结果覆盖范围有限，不是身份认证。$/u,
      (signals, findings) =>
        `Read ${signals} browser-information groups from the current page and organized them into ${findings} findings. Coverage is limited and this is not identity verification.`,
    ],
    [
      /^已从当前页面读取 (\d+) 组浏览器信息，并整理成 (\d+) 条解读；结果覆盖范围有限，不是身份认证。$/u,
      (signals, findings) =>
        `Read ${signals} browser-information groups from the current page and organized them into ${findings} findings. Coverage is limited and this is not identity verification.`,
    ],
    [
      /^本次可见：(.+)。它们不等于泄漏了真实姓名，但可用于关联多次访问。$/u,
      (items) =>
        `Visible in this scan: ${translateList(items)}. These do not reveal your real name, but they can help link visits.`,
    ],
    [
      /^(.+) 未完整可用$/u,
      (signal) => `${translateUiText(signal, "en")} was not fully available`,
    ],
    [/^([0-9.]+) GB 内存$/u, (memory) => `${memory} GB memory`],
    [/^(\d+) 线程$/u, (threads) => `${threads} threads`],
    [/^([0-9.]+)× 缩放$/u, (ratio) => `${ratio}× scale`],
    [/^(\d+) 位$/u, (bits) => `${bits}-bit`],
    [/^网络编号 (.+)$/u, (asn) => `Network number ${asn}`],
    [/^出口时区 (.+)$/u, (timezone) => `Egress time zone ${timezone}`],
    [
      /^已确认 (\d+)\/(\d+) 项；其余设置可能被策略或其他扩展接管。$/u,
      (verified, total) =>
        `Confirmed ${verified}/${total} settings; the remaining settings may be controlled by policy or another extension.`,
    ],
    [
      /^VeriSilo 已恢复原设置，或确认没有需要恢复的设置(.*)。$/u,
      (suffix) =>
        `VeriSilo restored the original settings or confirmed that no restoration was needed${suffix === "" ? "" : `, ${translateUiText(suffix.replace(/^，/u, ""), "en")}`}.`,
    ],
    [
      /^已从本次浏览器会话中清除网络检查结果(.*)。$/u,
      (suffix) =>
        `Cleared the network-check result from this browser session${suffix === "" ? "" : `, ${translateUiText(suffix.replace(/^，/u, ""), "en")}`}.`,
    ],
    [
      /^页面环境：(.+) · 网页后台任务：(.+)$/u,
      (page, worker) =>
        `Page environment: ${translateUiText(page, "en")} · Web background tasks: ${translateUiText(worker, "en")}`,
    ],
    [
      /^(.+) · (.+)$/u,
      (left, right) =>
        `${translateUiText(left, "en")} · ${translateUiText(right, "en")}`,
    ],
    [
      /^(.+)：待授权$/u,
      (label) => `${translateUiText(label, "en")}: permission required`,
    ],
    [/^(.+)：已生效$/u, (label) => `${translateUiText(label, "en")}: active`],
    [
      /^(.+)：被其他扩展控制$/u,
      (label) =>
        `${translateUiText(label, "en")}: controlled by another extension`,
    ],
    [
      /^(.+)：被策略锁定$/u,
      (label) => `${translateUiText(label, "en")}: locked by policy`,
    ],
    [
      /^(.+)：未启用$/u,
      (label) => `${translateUiText(label, "en")}: not enabled`,
    ],
  ];
  for (const [pattern, replacement] of replacements) {
    const match = pattern.exec(text);
    if (match !== null) {
      return replacement(...match.slice(1));
    }
  }
  return text;
}

function translateList(text: string): string {
  return text
    .split("、")
    .map((value) => translateUiText(value, "en"))
    .join(", ");
}

function preserveOuterWhitespace(source: string, translated: string): string {
  const leading = source.match(/^\s*/u)?.[0] ?? "";
  const trailing = source.match(/\s*$/u)?.[0] ?? "";
  return `${leading}${translated}${trailing}`;
}

function normalizeText(text: string): string {
  return text.replace(/\s+/gu, " ").trim();
}

function containsChinese(text: string): boolean {
  return /[\u3400-\u9fff]/u.test(text);
}

function parseLocale(value: unknown): UiLocale | null {
  return value === "zh-CN" || value === "en" ? value : null;
}

function browserDefaultLocale(): UiLocale {
  if (
    typeof chrome === "undefined" ||
    chrome.i18n?.getUILanguage === undefined
  ) {
    return "zh-CN";
  }
  return chrome.i18n.getUILanguage().toLowerCase().startsWith("zh")
    ? "zh-CN"
    : "en";
}
