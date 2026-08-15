export const privacyCopy = {
  en: {
    meta: {
      title: "Privacy Policy — VeriSilo Companion",
      description:
        "How the VeriSilo Companion browser extension handles browser signals, storage, network checks, and native messaging.",
    },
    updated: "Last updated: 2026-08-15",
    homeLabel: "VeriSilo home",
    languageLabel: "中文",
    title: "VeriSilo Companion Privacy Policy",
    intro:
      "This policy covers the VeriSilo Companion browser extension for Chrome and Edge and, where stated, its optional connection to the separately installed VeriSilo desktop application. VeriSilo Companion is an inspection tool: it observes browser signals on pages you explicitly scan and explains them. It does not sell data, shows no advertising, and operates without a VeriSilo server.",
    sections: [
      {
        heading: "What the extension reads",
        body: [
          "Nothing is read automatically. Only after you open the side panel on a page and click Scan does the extension read browser-visible signals from that one tab: user agent and platform, language, screen and timezone, hardware concurrency, and summary digests of Canvas, WebGL, WebGPU, audio, fonts, media devices, WebRTC, iframes, and Workers.",
          "The extension never reads cookie values, page text, form input, passwords, localStorage/IndexedDB contents, or browsing history.",
        ],
      },
      {
        heading: "Where data is stored",
        body: [
          "The current scan report is kept in chrome.storage.session and disappears with the browser session. A redacted history of at most 20 reports is kept locally in chrome.storage.local for up to 30 days; nothing is saved from incognito/InPrivate tabs. Both stores are restricted to trusted extension contexts and are never synced to any cloud account. You can inspect and clear the local history from the panel at any time.",
          "Exported JSON/HTML reports redact high-sensitivity signal values by default and are generated only when you explicitly export.",
        ],
      },
      {
        heading: "Network checks",
        body: [
          "A network check never runs automatically. After you grant the optional site access and confirm the action, the extension contacts ipwho.is (exit IP, geolocation, ASN), Cloudflare 1.1.1.1, and Google Public DNS with a fixed example.com DoH query for comparison. Those providers receive your request IP address. The result is stored in session storage and can be cleared from the panel. The comparison is not presented as DNS leak detection.",
        ],
      },
      {
        heading: "Connection to the VeriSilo desktop app",
        body: [
          "If you have separately installed the VeriSilo desktop application, the extension may use Native Messaging to read a short-lived, redacted runtime status and to submit the network result you just triggered. The desktop stores a bounded local history in its encrypted Vault until you clear it or delete the Silo. The extension never sends cookies, page storage, credentials, browsing history, the full observation report, or Vault secrets through this bridge. The companion remains fully functional without the desktop app.",
        ],
      },
      {
        heading: "What is never done",
        body: [
          "The extension does not transmit browsing activity, authentication information, cookies, or reports to VeriSilo servers because none exist. It does not sell, share, or use data for advertising. It contains no remote code. The only external navigation is the VeriSilo project page, opened only when you click the corresponding button.",
        ],
      },
      {
        heading: "Your controls",
        body: [
          "Everything the extension does is user-triggered. Optional website access and optional privacy controls are requested only when you invoke the related feature, and can be revoked in the browser's extension settings. Session results and local history can be cleared from the panel. Uninstalling the extension removes its storage.",
        ],
      },
      {
        heading: "Changes and contact",
        body: [
          "This policy is versioned with the extension. Material changes will be described in the extension changelog. For questions, open an issue in the VeriSilo repository.",
        ],
      },
    ],
    repositoryLabel: "VeriSilo repository",
  },
  zh: {
    meta: {
      title: "隐私政策 — VeriSilo Companion",
      description:
        "VeriSilo Companion 浏览器扩展如何处理浏览器信号、存储、网络检查与原生消息通信。",
    },
    updated: "最后更新：2026-08-15",
    homeLabel: "VeriSilo 首页",
    languageLabel: "English",
    title: "VeriSilo Companion 隐私政策",
    intro:
      "本政策适用于面向 Chrome 与 Edge 的 VeriSilo Companion 浏览器扩展，以及在相关章节说明的、与单独安装的 VeriSilo 桌面端的可选连接。VeriSilo Companion 是一个检查工具：它观察并解释你显式扫描的页面上的浏览器信号。它不出售数据、不展示广告，也不依赖任何 VeriSilo 服务器运行。",
    sections: [
      {
        heading: "扩展读取什么",
        body: [
          "没有任何自动读取。只有你在某个页面打开侧栏并点击扫描后，扩展才读取该标签页中网站可见的浏览器信号：User-Agent 与平台、语言、屏幕与时区、硬件并发数，以及 Canvas、WebGL、WebGPU、音频、字体、媒体设备、WebRTC、iframe 与 Worker 的摘要指纹。",
          "扩展从不读取 Cookie 值、网页正文、表单输入、密码、LocalStorage/IndexedDB 内容或浏览历史。",
        ],
      },
      {
        heading: "数据存放位置",
        body: [
          "当前扫描报告保存在 chrome.storage.session，随浏览器会话结束而消失。最多 20 份脱敏历史报告本地保存在 chrome.storage.local，最长保留 30 天；无痕/InPrivate 标签页不保存任何内容。两处存储都限制为受信任的扩展上下文，且从不与任何云端账户同步。你可以随时在侧栏中查看并清除本地历史。",
          "导出的 JSON/HTML 报告默认对高敏感信号值做脱敏，且只有在你显式导出时才会生成。",
        ],
      },
      {
        heading: "网络检查",
        body: [
          "网络检查从不自动运行。只有在你授权可选网站访问权限并确认操作后，扩展才会请求 ipwho.is（出口 IP、地理位置、ASN）、Cloudflare 1.1.1.1 与 Google Public DNS（对固定 example.com 的 DoH 查询，用于对比）。这些服务方会看到你的请求来源 IP。结果保存在会话存储中，可从侧栏清除。该对比不会被描述为 DNS 泄漏检测。",
        ],
      },
      {
        heading: "与 VeriSilo 桌面端的连接",
        body: [
          "如果你单独安装了 VeriSilo 桌面端，扩展可能通过原生消息通信读取一份短期有效、已脱敏的运行状态，并提交你刚触发的网络检查结果。桌面端在其加密 Vault 中保存有界本地历史，直到你清除历史或删除 Silo。扩展从不通过该桥接发送 Cookie、页面存储、凭据、浏览历史、完整观察报告或 Vault 密钥。没有桌面端时扩展功能完全不受影响。",
        ],
      },
      {
        heading: "从不会做的事",
        body: [
          "扩展不会把浏览活动、认证信息、Cookie 或报告发送给任何 VeriSilo 服务器——因为不存在这样的服务器。它不出售、共享数据，也不用于广告。扩展内不含远程代码。唯一的外部跳转是 VeriSilo 项目页，且只有在你点击相应按钮时才打开。",
        ],
      },
      {
        heading: "你的控制权",
        body: [
          "扩展的所有行为都由用户触发。可选网站访问权限与可选隐私控制只在你调用相关功能时请求，可在浏览器扩展设置中随时撤销。会话结果与本地历史可在侧栏清除。卸载扩展即删除其存储。",
        ],
      },
      {
        heading: "变更与联系",
        body: [
          "本政策随扩展版本一起维护。实质变更会在扩展更新日志中说明。如有疑问，请在 VeriSilo 仓库提交 issue。",
        ],
      },
    ],
    repositoryLabel: "VeriSilo 仓库",
  },
} as const;
