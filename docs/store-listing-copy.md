# Store listing copy

- 状态：**粘贴用草稿**，English + 简体中文，两个商店共用
- 原则：单一目的统一为“检查并理解当前浏览环境的可观察暴露面”；Network Check 与临时 privacy controls 是服务该目的的能力，不是并列的独立功能
- manifest 内嵌 description 不变（`apps/extension/_locales/*/messages.json`），商店页文案与 manifest 是两个东西

## English

### Listing title

> VeriSilo Companion — Inspect your browser's observable exposure

### Short description

> See what browser signals the current page can read, check your network exit, and apply explicit temporary controls — all locally, all user-triggered.

### Long description

> VeriSilo Companion helps you inspect and understand the observable exposure of your current browsing environment.
>
> **Scan on demand.** Open the side panel on a page and scan the signals that page can read: user agent and platform, language, timezone, screen, and summary digests of Canvas, WebGL, WebGPU, audio, fonts, media devices, and WebRTC. Nothing runs automatically, and results describe only the page you scanned.
>
> **Check your network exit.** Run an optional network check that queries ipwho.is and compares public DoH answers from Cloudflare 1.1.1.1 and Google Public DNS. You grant site access first, and the result stays in session storage until you clear it. This is an exit observation, not DNS leak detection.
>
> **Temporary, reversible controls.** Optionally enable WebRTC and network-prediction restrictions for the current browser context. Controls apply to the whole browser context, report when they are verified, and can be restored at any time.
>
> **Local by default.** Scan results live in extension storage on your device: session reports vanish with the session, and at most 20 redacted history entries are kept for 30 days. VeriSilo runs no servers, shows no ads, and sells nothing. If the separately installed VeriSilo Desktop is running, you can optionally hand the network result to its encrypted local Vault through Native Messaging.
>
> The extension works fully without the desktop app.

## 简体中文

### 商店名称

> VeriSilo Companion — 检查当前浏览环境的可观察暴露面

### 简介

> 查看当前网页可读取的浏览器信号、检查网络出口、按需启用可恢复的临时控制——全部本地运行、全部由你触发。

### 详细介绍

> VeriSilo Companion 帮助你检查并理解当前浏览环境的可观察暴露面。
>
> **按需扫描。** 在页面打开侧栏，扫描该页面可读取的信号：User-Agent 与平台、语言、时区、屏幕，以及 Canvas、WebGL、WebGPU、音频、字体、媒体设备、WebRTC 的摘要指纹。没有任何自动运行，结果只代表你扫描的页面。
>
> **检查网络出口。** 可选运行网络检查：请求 ipwho.is，并对比 Cloudflare 1.1.1.1 与 Google Public DNS 的公开 DoH 应答。先授权站点访问，结果保存在会话存储中，可随时清除。这是出口观测，不是 DNS 泄漏检测。
>
> **临时、可恢复的控制。** 可选地对当前浏览器上下文启用 WebRTC 与网络预测限制。控制作用于整个浏览器上下文、生效后如实报告，并可随时恢复。
>
> **默认只在本机。** 扫描结果保存在本机扩展存储：会话报告随会话消失，本地历史最多 20 条脱敏记录、保留 30 天。VeriSilo 不运营服务器、不展示广告、不出售任何数据。若单独安装的 VeriSilo 桌面端正在运行，你可以选择把网络结果通过原生消息通信交给其加密的本地 Vault。
>
> 没有桌面端时扩展功能完全可用。

## 素材清单（Chrome / Edge 各自表单）

| 素材                     | Chrome                                | Edge                        |
| ------------------------ | ------------------------------------- | --------------------------- |
| 128×128 icon             | 已有（包内 `icons/verisilo-128.png`） | 最低 128×128 满足           |
| 300×300 logo             | —                                     | 推荐，见素材目录            |
| 440×280 small promo tile | 必需                                  | 可选                        |
| 截图 1280×800 / 640×400  | 必需（至少一张）                      | 可选（1280×800 或 640×480） |
| 类别                     | Privacy & Security                    | —                           |
| 语言                     | English + 简体中文                    | English + 简体中文          |

主图素材位于 `assets/store/`：`store-screenshot-1280x800-en-scan.png`、
`store-screenshot-1280x800-zh-report.png`、
`store-screenshot-1280x800-en-private-space.png`，以及对应的 `640×400` 文件。
这些是明确标注 `example.test` 的商店展示构图，右侧面板来自冻结 RC 的实际构建；不要把
示例报告中的合成值描述为真实网络或设备证据。

## 提交前人工复核项

- [ ] 检查 Chrome 政策要求的所有数据披露点是否与 `docs/store-privacy-forms.md` 一致。
- [ ] 确认文案不含 "anti-detect"、"fingerprint spoofing"、"isolation guarantee" 等未实现能力表述（仓库 evidence 术语：observed/configured/applied/verified 不得混用）。
