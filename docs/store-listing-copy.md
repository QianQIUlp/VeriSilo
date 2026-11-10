# Store listing copy

- Status: **draft for pasting**, English + Simplified Chinese (简体中文), shared by both stores
- Principle: the single purpose is uniformly stated as "inspect and understand the observable exposure of the current browsing environment". Network Check and the temporary privacy controls are capabilities that serve this purpose. They are not separate parallel features.
- The description embedded in the manifest is unchanged (`apps/extension/_locales/*/messages.json`). The store page copy and the manifest are two different things.

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
> **Check your network exit.** Run an optional network check that queries ipwho.is (with fallback exit-IP services if unavailable) and compares public DoH answers from Cloudflare 1.1.1.1 and Google Public DNS. You grant site access first, and the result stays in session storage until you clear it. This is an exit observation, not DNS leak detection.
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
> **检查网络出口。** 可选运行网络检查：请求 ipwho.is（不可用时使用备用出口 IP 服务），并对比 Cloudflare 1.1.1.1 与 Google Public DNS 的公开 DoH 应答。先授权站点访问，结果保存在会话存储中，可随时清除。这是出口观测，不是 DNS 泄漏检测。
>
> **临时、可恢复的控制。** 可选地对当前浏览器上下文启用 WebRTC 与网络预测限制。控制作用于整个浏览器上下文、生效后如实报告，并可随时恢复。
>
> **默认只在本机。** 扫描结果保存在本机扩展存储：会话报告随会话消失，本地历史最多 20 条脱敏记录、保留 30 天。VeriSilo 不运营服务器、不展示广告、不出售任何数据。若单独安装的 VeriSilo 桌面端正在运行，你可以选择把网络结果通过原生消息通信交给其加密的本地 Vault。
>
> 没有桌面端时扩展功能完全可用。

## Asset checklist (separate Chrome / Edge forms)

| Asset                          | Chrome                                                    | Edge                                    |
| ------------------------------ | --------------------------------------------------------- | --------------------------------------- |
| 128×128 icon                   | Already present (`icons/verisilo-128.png` in the package) | Minimum 128×128 is met                  |
| 300×300 logo                   | —                                                         | Recommended, see the asset directory    |
| 440×280 small promo tile       | Required                                                  | Optional                                |
| Screenshots 1280×800 / 640×400 | Required (at least one)                                   | Optional (1280×800 or 640×480)          |
| Category                       | Privacy & Security                                        | —                                       |
| Language                       | English + Simplified Chinese (简体中文)                   | English + Simplified Chinese (简体中文) |

The main image assets are in `assets/store/`: `store-screenshot-1280x800-en-scan.png`,
`store-screenshot-1280x800-zh-report.png`,
`store-screenshot-1280x800-en-private-space.png`, and the corresponding `640×400` files.
These are store display compositions that are explicitly labeled `example.test`. The side panel on the right comes from the actual build of the frozen RC. Do not describe the synthetic values in the example report as real network or device evidence.

## Manual review items before submission

- [ ] Check that every data disclosure point required by the Chrome policy is consistent with `docs/store-privacy-forms.md`.
- [ ] Confirm that the copy contains no statements about unimplemented capabilities such as "anti-detect", "fingerprint spoofing" or "isolation guarantee" (repository evidence terms: observed/configured/applied/verified must not be mixed).
