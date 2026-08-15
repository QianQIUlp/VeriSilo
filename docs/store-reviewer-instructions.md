# Store reviewer test instructions

- 状态：**粘贴到 Chrome Web Store / Edge Partner Center 的 Test instructions 草稿**
- 对应扩展包：`VeriSilo-Companion-0.2.10-chrome-edge.zip`（MV3）
- 注意：本扩展与桌面端通过 Native Messaging 协作，但**扩展本身可独立工作**。请勿把"桌面端未安装时桥接不可用"判定为缺陷——这是设计内的降级行为。

## Before you start

The extension scans only ordinary `http(s)` pages. It does nothing automatically: every action is triggered from its side panel.

For the full feature set, additionally install the VeriSilo Desktop (Windows) so the Native Messaging host `io.verisilo.host` is registered. This is optional for reviewing the core scan flow.

## Test 1 — Install and open the side panel

1. Install the extension.
2. Pin it to the toolbar, open a normal website (e.g. `https://example.com`), and click the VeriSilo toolbar icon to open the side panel.
3. Expected: the panel opens with the **Local only** status pill visible, a Scan button, and four tabs (Overview / Isolation / Labs / Raw). The pill can be dismissed for the session and reappears when the panel is reopened.

## Test 2 — Language

1. Use the language selector in the panel to switch between English and 简体中文.
2. Expected: interface copy changes language; the red/green status notices are dismissible.

## Test 3 — Scan a page

1. On a normal `http(s)` page, open the side panel via the toolbar icon and click **Scan**.
2. Expected: a report appears listing observable browser signals (user agent, platform, language, timezone, screen, and summary digests of Canvas/WebGL/audio/fonts/WebRTC). Results describe only the scanned page.
3. The browser-internal pages, store pages, and PDFs are intentionally not scannable; the panel shows an explanatory notice instead.

## Test 4 — Optional permissions

1. `activeTab`/`scripting`: the first scan of a page may prompt for the one-time access granted by clicking the toolbar icon. Follow the notice.
2. Site access (`http://*/*` / `https://*/*`): requested only when you invoke a scan or the network check on a site. Decline to confirm the extension still scans the current tab via `activeTab` when opened from the toolbar.
3. `privacy` (optional): requested only in the Labs tab when you explicitly enable temporary WebRTC / network-prediction controls. The UI shows observe → apply → verify → restore state and can restore at any time.

## Test 5 — Network check

1. In the panel, open the network check section and follow the prompt to grant the optional host permission, then confirm the check.
2. Expected: the extension contacts `ipwho.is` and compares public DoH answers from Cloudflare 1.1.1.1 and Google Public DNS for a fixed `example.com` query. The result is labeled as an exit observation (not DNS leak detection) and can be cleared from the panel.
3. Without the permission grant the check is refused with an explanatory notice and no request is sent.

## Test 6 — Native Messaging with VeriSilo Desktop (optional prerequisite)

1. Install VeriSilo Desktop and its Native Messaging host (`io.verisilo.host`, registered per-user under `HKCU` for both Chrome and Edge).
2. Start the desktop app, unlock the Vault, and create/launch a Local + Direct Silo.
3. Open the companion inside that Silo's browser and scan a page.
4. Expected: the panel's status changes from `Local only` to a Silo-bound state; a user-triggered network check can be handed off to the desktop's encrypted local Vault history.
5. Expected without the desktop: the panel remains fully functional and shows `Local only` with a clear explanation. The "open desktop" button starts the desktop if installed; otherwise it reports the desktop is unavailable.

## Expected failure modes (not defects)

- Scanning a `chrome://` / `edge://` page, a store page, or a PDF shows an explanatory notice.
- Network check without the optional host permission is refused without sending any request.
- Native Messaging handoff fails locally when the desktop is not running, the Vault is locked, or no matching Silo is active — the result stays in the extension session storage.

## Privacy

No data leaves the device except the user-triggered network check requests described above and the optional handoff to the locally installed desktop. See the linked privacy policy and `docs/extension-data-flow-inventory.md` in the repository for the complete data-flow inventory.

---

## 中文摘要（供中文审阅者）

- 安装扩展后固定在工具栏，在普通网页点击 VeriSilo 图标打开侧栏；面板显示 **Local only** 状态、扫描按钮和四个标签页。
- 扫描只读当前页面可见的浏览器信号，结果只代表该页面；浏览器内部页面/商店页/PDF 会显示说明性提示而非失败。
- 可选权限（站点访问、privacy 控制）只在用户调用相关功能时请求；拒绝后功能优雅降级。
- 网络检查需用户确认后才请求 ipwho.is / Cloudflare / Google Public DNS，结果标注为出口观测并可清除。
- 桌面端 + Native Host 未安装时，扩展完全可用，仅桥接状态保持 `Local only`；这不是功能故障。
- 除用户触发的网络检查与可选的本地桌面交接外，数据不出本机。
