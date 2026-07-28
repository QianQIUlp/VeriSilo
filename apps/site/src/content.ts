export type Locale = "en" | "zh";

export const links = {
  repository: "https://github.com/QianQIUlp/VeriSilo",
  architecture:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/architecture.md",
  capabilities:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/capabilities.md",
  threatModel:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/threat-model.md",
  development:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/development.md",
  license: "https://github.com/QianQIUlp/VeriSilo/blob/main/LICENSE",
} as const;

export const copy = {
  en: {
    meta: {
      title: "VeriSilo — Browser state, visibly separated",
      description:
        "VeriSilo is a Windows-first, open-source browser environment isolation and privacy-auditing project for Chrome and Edge.",
    },
    skipLink: "Skip to content",
    homeLabel: "VeriSilo home",
    factsLabel: "Key product facts",
    languageLabel: "中文",
    navigation: {
      aria: "Primary navigation",
      how: "How it works",
      evidence: "Evidence",
      boundaries: "Boundaries",
      source: "Source",
    },
    hero: {
      eyebrow: "WINDOWS-FIRST · OPEN SOURCE · IN DEVELOPMENT",
      titleLead: "Keep browser state",
      titleEmphasis: "in a Silo of its own.",
      body: "VeriSilo launches Chrome or Edge with a separate, managed data directory for every Silo. Cookies, storage, cache, service workers, permissions, and history stay in that environment—not in your default browser profile.",
      primaryAction: "Inspect the source",
      secondaryAction: "See how isolation works",
      releaseNote:
        "Public development build. No signed installer is available yet.",
    },
    model: {
      aria: "Diagram showing the default browser profile separated from two VeriSilo data directories",
      title: "Browser state map",
      live: "Isolation core",
      defaultLabel: "DEFAULT PROFILE",
      defaultName: "Your existing browser",
      defaultStatus: "UNTOUCHED",
      defaultPath: "Chrome / Edge · current profile",
      defaultTokens: ["cookies", "history", "saved logins"],
      boundaryTop: "NO IMPORT",
      boundaryBottom: "NO CLONE",
      siloLabel: "MANAGED DATA DIRECTORY",
      firstSilo: "Work.silo",
      secondSilo: "Research.silo",
      active: "ACTIVE",
      ready: "READY",
      path: "…/silos/{id}/browser-data",
      siloTokens: ["cookies", "storage", "cache", "workers"],
      commandLabel: "Launch evidence",
      command: "--user-data-dir=<managed path>",
      caption:
        "The desktop core creates the boundary. The companion extension is optional.",
    },
    facts: [
      {
        label: "STATE",
        value: "Separate by directory",
        detail: "Not a cookie switcher",
      },
      {
        label: "DEFAULT PROFILE",
        value: "Never selected",
        detail: "Not imported or mutated",
      },
      {
        label: "EVIDENCE",
        value: "Facts before scores",
        detail: "Applied is not yet verified",
      },
    ],
    how: {
      eyebrow: "THE ISOLATION PATH",
      title: "One boundary, three explicit steps.",
      intro:
        "A Silo is a browser-owned data directory plus the settings and evidence needed to explain how it was launched.",
      steps: [
        {
          label: "CREATE",
          title: "Define a Silo locally",
          body: "The desktop app records the Silo metadata, browser choice, and explicit network profile. Its encrypted vault protects VeriSilo metadata and seed material—not the browser’s entire profile.",
          note: "Local vault · no cloud account",
        },
        {
          label: "LAUNCH",
          title: "Start a separate browser directory",
          body: "Chrome or Edge receives a dedicated user-data directory through an argument-array launch. The existing default profile is never selected, copied, or modified.",
          note: "Browser-owned state · separate path",
        },
        {
          label: "VERIFY",
          title: "Inspect what can be proven",
          body: "The optional Companion observes browser-context signals, explains capability limits, and creates local redacted reports. A Silo works without the extension.",
          note: "Optional extension · evidence on demand",
        },
      ],
    },
    evidence: {
      eyebrow: "THE EVIDENCE MODEL",
      title: "“Protected” is a result, not a marketing adjective.",
      intro:
        "VeriSilo separates what it can control from what it can only observe—and what a normal browser launcher or extension cannot change at all.",
      pipelineLabel: "A control earns its state",
      pipeline: ["CONFIGURED", "APPLIED", "VERIFIED"],
      pipelineNote:
        "Each state requires its own evidence. Reliable never means automatically verified.",
      levels: [
        {
          tier: "RELIABLE",
          title: "Separate browser data directory",
          body: "Created and launched by the desktop core, with direct path and process evidence.",
          tone: "reliable",
        },
        {
          tier: "BEST EFFORT",
          title: "WebRTC preference and page signals",
          body: "Useful observations with browser, page, timing, permission, and context limits kept visible.",
          tone: "effort",
        },
        {
          tier: "UNSUPPORTED",
          title: "TLS, QUIC, hardware identity",
          body: "Outside the honest boundary of a normal Chrome or Edge extension and launcher.",
          tone: "unsupported",
        },
      ],
      docsAction: "Read the capability model",
    },
    architecture: {
      eyebrow: "LOCAL BY DESIGN",
      title: "The isolation core stays on your machine.",
      intro:
        "The desktop app owns the boundary. Chrome or Edge owns the browser files. The Companion adds explanations and user-triggered checks without becoming the isolation mechanism.",
      nodes: [
        {
          kind: "CORE",
          title: "Tauri desktop",
          body: "Creates Silos, discovers browsers, launches safely, and keeps one active Silo in V0.1.",
        },
        {
          kind: "STATE",
          title: "Browser data",
          body: "Cookies, LocalStorage, IndexedDB, cache, service workers, permissions, and history remain browser-owned.",
        },
        {
          kind: "OPTIONAL",
          title: "MV3 Companion",
          body: "Observes page context, verifies supported controls, and exports local redacted reports after confirmation.",
        },
      ],
      footnote:
        "Reports are not synced to a VeriSilo service. User-triggered network checks contact disclosed third-party endpoints and reveal the request IP to those providers.",
      docsAction: "Open the architecture notes",
    },
    boundaries: {
      eyebrow: "THE HONEST BOUNDARY",
      title: "What VeriSilo refuses to pretend.",
      intro:
        "Transparent limits are part of the product. Browser-state separation is useful without inventing claims the implementation cannot prove.",
      items: [
        {
          title: "No device impersonation",
          body: "It does not turn one physical machine into a different device.",
        },
        {
          title: "No fraud or policy bypass",
          body: "It is not built to evade account controls, restrictions, or site security.",
        },
        {
          title: "No network-stack rewrite",
          body: "It does not control TLS, HTTP/2, HTTP/3, QUIC, or every DNS path.",
        },
        {
          title: "No “undetectable” promise",
          body: "Page observations and best-effort settings always retain their limits.",
        },
      ],
      docsAction: "Read the threat model",
    },
    source: {
      eyebrow: "SOURCE AVAILABLE NOW",
      title: "Inspect the claim before you trust it.",
      body: "VeriSilo is being built in public under MPL-2.0. The source, product boundaries, tests, and release gates are open for review while the first signed Windows build is prepared.",
      primaryAction: "View VeriSilo on GitHub",
      secondaryAction: "Build from source",
      statusLabel: "PROJECT STATUS",
      statusValue: "Pre-release engineering",
      platformLabel: "TARGET",
      platformValue: "Windows · Chrome / Edge",
      licenseLabel: "SOURCE LICENSE",
      licenseValue: "MPL-2.0",
    },
    footer: {
      tagline: "Verifiable browser environment isolation and privacy auditing.",
      status: "Built in public. No telemetry, cloud sync, or advertising.",
      license: "MPL-2.0 source",
    },
  },
  zh: {
    meta: {
      title: "VeriSilo — 看得见边界的浏览器环境隔离",
      description:
        "VeriSilo 是面向 Windows、开源的 Chrome 与 Edge 浏览器环境隔离和隐私审计项目。",
    },
    skipLink: "跳到主要内容",
    homeLabel: "VeriSilo 首页",
    factsLabel: "产品要点",
    languageLabel: "EN",
    navigation: {
      aria: "主导航",
      how: "工作原理",
      evidence: "证据模型",
      boundaries: "能力边界",
      source: "源代码",
    },
    hero: {
      eyebrow: "WINDOWS 优先 · 开源 · 开发中",
      titleLead: "把浏览器状态",
      titleEmphasis: "放进各自的 Silo。",
      body: "VeriSilo 为每个 Silo 启动一套独立、受管理的 Chrome 或 Edge 数据目录。Cookie、存储、缓存、Service Worker、权限与历史记录留在各自环境中，不进入你的默认浏览器 Profile。",
      primaryAction: "查看源代码",
      secondaryAction: "了解隔离原理",
      releaseNote: "项目正在公开开发，暂未提供签名安装包。",
    },
    model: {
      aria: "默认浏览器 Profile 与两个 VeriSilo 独立数据目录的隔离示意图",
      title: "浏览器状态地图",
      live: "隔离核心",
      defaultLabel: "默认 PROFILE",
      defaultName: "你现有的浏览器",
      defaultStatus: "未触碰",
      defaultPath: "Chrome / Edge · 当前 Profile",
      defaultTokens: ["Cookie", "历史记录", "已保存登录"],
      boundaryTop: "不导入",
      boundaryBottom: "不克隆",
      siloLabel: "受管理的数据目录",
      firstSilo: "工作.silo",
      secondSilo: "研究.silo",
      active: "运行中",
      ready: "就绪",
      path: "…/silos/{id}/browser-data",
      siloTokens: ["Cookie", "存储", "缓存", "Worker"],
      commandLabel: "启动证据",
      command: "--user-data-dir=<managed path>",
      caption: "桌面端建立隔离边界，Companion 扩展是可选组件。",
    },
    facts: [
      {
        label: "状态隔离",
        value: "按数据目录分开",
        detail: "不只是切换 Cookie",
      },
      {
        label: "默认 PROFILE",
        value: "从不选用",
        detail: "不导入、不修改",
      },
      {
        label: "证据",
        value: "事实先于分数",
        detail: "已应用不等于已验证",
      },
    ],
    how: {
      eyebrow: "隔离路径",
      title: "一道边界，三个明确步骤。",
      intro:
        "Silo 是一个由浏览器管理的数据目录，也包含解释其启动方式所需的设置与证据。",
      steps: [
        {
          label: "创建",
          title: "在本机定义 Silo",
          body: "桌面端记录 Silo 元数据、浏览器选择与明确的网络配置。加密保险库保护的是 VeriSilo 元数据和种子，而不是整个浏览器 Profile。",
          note: "本地保险库 · 无需云端账户",
        },
        {
          label: "启动",
          title: "启用独立浏览器目录",
          body: "VeriSilo 通过参数数组为 Chrome 或 Edge 指定专属 user-data-dir；不会选用、复制或修改现有默认 Profile。",
          note: "浏览器管理状态 · 独立路径",
        },
        {
          label: "验证",
          title: "只陈述可以证明的结果",
          body: "可选 Companion 扩展负责观察浏览器上下文、解释能力边界并生成本地脱敏报告。没有扩展，Silo 也能工作。",
          note: "可选扩展 · 按需取证",
        },
      ],
    },
    evidence: {
      eyebrow: "证据模型",
      title: "“受到保护”是结果，不是宣传形容词。",
      intro:
        "VeriSilo 会区分能够可靠控制的能力、只能尽力观察的能力，以及普通浏览器启动器或扩展根本无法改变的部分。",
      pipelineLabel: "能力状态必须逐级获得",
      pipeline: ["已配置", "已应用", "已验证"],
      pipelineNote: "每个状态都需要自己的证据；可靠能力不会自动等于已经验证。",
      levels: [
        {
          tier: "可靠",
          title: "独立浏览器数据目录",
          body: "由桌面核心创建并启动，可直接核对路径与进程证据。",
          tone: "reliable",
        },
        {
          tier: "尽力",
          title: "WebRTC 偏好与页面信号",
          body: "提供有用观察，同时明确浏览器、页面、时序、权限与上下文限制。",
          tone: "effort",
        },
        {
          tier: "不支持",
          title: "TLS、QUIC 与硬件身份",
          body: "超出普通 Chrome / Edge 扩展与启动器能够诚实控制的边界。",
          tone: "unsupported",
        },
      ],
      docsAction: "阅读能力模型",
    },
    architecture: {
      eyebrow: "本地优先",
      title: "隔离核心留在你的设备上。",
      intro:
        "桌面端负责建立边界，Chrome 或 Edge 管理浏览器文件；Companion 提供解释与用户触发的检查，但它不是隔离机制本身。",
      nodes: [
        {
          kind: "核心",
          title: "Tauri 桌面端",
          body: "创建 Silo、发现浏览器、安全启动；V0.1 同时只允许一个活动 Silo。",
        },
        {
          kind: "状态",
          title: "浏览器数据",
          body: "Cookie、LocalStorage、IndexedDB、缓存、Service Worker、权限与历史记录仍由浏览器管理。",
        },
        {
          kind: "可选",
          title: "MV3 Companion",
          body: "观察页面上下文、验证受支持的控制，并在确认后导出本地脱敏报告。",
        },
      ],
      footnote:
        "报告不会同步到 VeriSilo 服务。用户主动发起的网络检查会访问已披露的第三方端点，这些提供方会看到请求 IP。",
      docsAction: "查看架构说明",
    },
    boundaries: {
      eyebrow: "诚实边界",
      title: "VeriSilo 明确拒绝伪装的能力。",
      intro:
        "透明的限制也是产品的一部分。浏览器状态隔离本身已经有价值，不需要杜撰实现无法证明的承诺。",
      items: [
        {
          title: "不伪装物理设备",
          body: "不会把同一台物理机器变成另一台设备。",
        },
        {
          title: "不绕过风控或策略",
          body: "不用于规避账号控制、访问限制或网站安全机制。",
        },
        {
          title: "不改写网络栈",
          body: "不控制 TLS、HTTP/2、HTTP/3、QUIC 或所有 DNS 路径。",
        },
        {
          title: "不承诺“不可检测”",
          body: "页面观察与尽力设置始终保留其真实限制。",
        },
      ],
      docsAction: "阅读威胁模型",
    },
    source: {
      eyebrow: "源码现已公开",
      title: "先审查主张，再决定是否信任。",
      body: "VeriSilo 以 MPL-2.0 在公开仓库中开发。首个签名 Windows 版本准备期间，源代码、产品边界、自动化测试与发布门槛都可供检查。",
      primaryAction: "在 GitHub 查看 VeriSilo",
      secondaryAction: "从源码构建",
      statusLabel: "项目状态",
      statusValue: "发布前工程阶段",
      platformLabel: "目标平台",
      platformValue: "Windows · Chrome / Edge",
      licenseLabel: "源码许可",
      licenseValue: "MPL-2.0",
    },
    footer: {
      tagline: "可验证的浏览器环境隔离与隐私审计。",
      status: "公开构建；无遥测、云同步或广告。",
      license: "MPL-2.0 源码",
    },
  },
} as const;
