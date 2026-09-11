# Store privacy form answers

- 状态：**粘贴用草稿**
- 事实基线：[`extension-data-flow-inventory.md`](extension-data-flow-inventory.md) 与 [`store-disclosure.md`](store-disclosure.md)
- 适用：Chrome Web Store Developer Dashboard 与 Microsoft Partner Center 的隐私/权限表单

## Chrome Web Store

### Privacy practices — Single purpose

> VeriSilo Companion helps you inspect and understand the observable exposure of your current browsing environment: what browser signals the page you are on can read, what your network exit looks like, and which temporary controls are active. All scans, network checks, and controls are user-initiated.

### Privacy practices — Permission justification

| 权限                            | 用途说明（表单文案）                                                                                                                       |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `storage`                       | Keeps the current scan report and a bounded, clearable local history (max 20 redacted reports, 30 days) inside trusted extension contexts. |
| `sidePanel`                     | Provides the Companion side-panel UI.                                                                                                      |
| `activeTab`                     | Lets the user scan only the page they opened the panel on; access expires when they leave.                                                 |
| `scripting`                     | Injects the read-only signal collectors on the page the user is scanning.                                                                  |
| `nativeMessaging`               | Talks to the separately installed VeriSilo Desktop's local Native Host for a redacted runtime status and user-submitted network results.   |
| `privacy` (optional)            | Requested only when the user explicitly enables temporary WebRTC / network-prediction controls.                                            |
| `http(s)://*/*` (optional host) | Requested only when the user invokes a scan or the network check that needs to contact the listed services.                                |

### Privacy practices — Data usage

- 收集的数据：用户触发扫描产生的浏览器可见信号摘要；用户触发的网络检查结果。
- 用途：本地显示与解释；无服务器上传。
- 是否出售/传输：否。网络检查会向 ipwho.is、Cloudflare 1.1.1.1、Google Public DNS 发出固定查询（对方看到请求 IP）。
- 保留：session 报告随会话消失；本地历史最多 20 条 / 30 天；用户可清除。
- Remote code：无（bundle gate 校验）。

## Microsoft Edge Add-ons（Partner Center）

### Single purpose（新版隐私页）

> VeriSilo Companion is a single-purpose inspection tool: it helps users understand the observable exposure of their current browsing environment through user-initiated scans, an optional network check, and explicit temporary controls. Network checks and privacy controls serve this same purpose and never run automatically.

### Permission justification（逐项，与 Chrome 相同文案可用）

同上表；Edge 额外注意：`proxy` / `declarativeNetRequest` 未声明（bundle gate 明确拒绝其作为 optional）。

### Remote code

> None. The extension ships all code in the package; it does not load or execute remote scripts. The store package is verified by a deterministic bundle gate.

### Data usage

- 同 Chrome 的 Data usage 文案。
- 补充：Extension does not collect payment data, personal communications, or user activity for advertising.

## 提交前人工复核项

- [ ] 表单文字与 `docs/store-disclosure.md` 逐条对照（该文档受 bundle gate 校验，是当前最完整披露）。
- [ ] 确认 Network Check 端点仍是 ipwho.is / cloudflare-dns.com / dns.google 且无新增 URL（`scripts/verify-extension-bundle.mjs` 已校验，人工复核一次）。
- [ ] 隐私政策 URL 填写 `https://verisilo.qiu.works/privacy`（上线后验证 HTTPS 可访问）。
