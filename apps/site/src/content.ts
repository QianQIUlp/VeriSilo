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
  milestone:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/milestones/0.1-identity-isolation-core.md",
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
      milestone: "Milestone",
      how: "How it works",
      evidence: "Evidence",
      boundaries: "Boundaries",
      source: "Source",
    },
    hero: {
      eyebrow: "WINDOWS-FIRST · OPEN SOURCE · 0.1 SOURCE MILESTONE",
      titleLead: "Keep browser state",
      titleEmphasis: "in a Silo of its own.",
      body: "VeriSilo launches Chrome or Edge in a separate, managed environment for every Silo. Browser-owned state stays in its own data directory, while the desktop app manages the Silo lifecycle, runtime binding, and optional network profile without importing or modifying the default browser profile.",
      primaryAction: "Inspect the source",
      secondaryAction: "View the current milestone",
      releaseNote:
        "The 0.1 source milestone is complete. No signed Windows build is available yet.",
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
    milestone: {
      eyebrow: "MILESTONE 01 · 2026-08",
      title: "The identity-isolation core now works end to end.",
      intro:
        "VeriSilo has moved from separate experiments to a connected Windows product skeleton: create a Silo, keep its browser state separate, bind its runtime and network profile, inspect bounded evidence, stop it, and recover it without touching the default browser profile.",
      groups: [
        {
          tone: "closed",
          label: "IMPLEMENTED",
          title: "Identity isolation core",
          items: [
            "Encrypted local Vault and explicit Silo lifecycle.",
            "Separate Chrome or Edge data directories with runtime identity binding.",
            "Per-Silo direct, fixed-proxy, and external Mihomo network profiles.",
            "Fail-closed behavior for network paths that require a proxy.",
            "Optional Companion and Native Host evidence with local redacted reports.",
          ],
        },
        {
          tone: "gated",
          label: "NOT YET AVAILABLE",
          title: "Public distribution and stronger environments",
          items: [
            "No signed Windows installer or browser-store version yet.",
            "Fingerprint fields still come from the stock browser.",
            "Real-machine verification does not yet cover every Windows and virtualization setup.",
            "Evidence covers documented cases, not every real machine.",
          ],
        },
        {
          tone: "next",
          label: "NEXT",
          title: "Fingerprint consistency",
          items: [
            "Keep one Silo stable across restarts.",
            "Keep Window, iframe, Worker, headers, and network observations consistent.",
            "Coordinate browser-visible fields through a controlled engine.",
            "Apply, verify, restore, and fall back per site without claiming undetectability.",
          ],
        },
      ],
      docsAction: "Read the milestone notes",
    },
    how: {
      eyebrow: "THE ISOLATION PATH",
      title: "One boundary, three explicit steps.",
      intro:
        "A Silo is a browser-owned data directory plus the settings and evidence needed to explain how it was launched.",
      steps: [
        {
          label: "CREATE",
          title: "Define a Silo locally",
          body: "Record the browser, execution target, network profile, and Silo metadata in the desktop app. The encrypted Vault protects VeriSilo metadata and optional network credentials, not the browser-owned profile files.",
          note: "Local vault · no cloud account",
        },
        {
          label: "LAUNCH",
          title: "Start a bound browser environment",
          body: "Launch Chrome or Edge with a dedicated data directory and an explicit runtime identity. Required proxy paths must fail closed instead of silently returning to the host network.",
          note: "Dedicated data directory · runtime identity",
        },
        {
          label: "INSPECT",
          title: "Verify, stop, and recover",
          body: "Inspect bounded runtime and network evidence, optionally add Companion observations, export local redacted reports, and stop or recover the Silo without force-killing unrelated browser processes.",
          note: "Bounded evidence · local redacted reports",
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
          title: "Independent state and controlled launch",
          body: "Desktop-created data directories, Silo lifecycle controls, runtime binding, and supported fail-closed launch paths have direct local evidence. Configured, applied, and verified remain separate runtime states.",
          tone: "reliable",
        },
        {
          tier: "BEST EFFORT",
          title: "Page and network observations",
          body: "WebRTC preferences, public-IP or DNS observations, and page signals retain browser, timing, permission, coverage, and provenance limits.",
          tone: "effort",
        },
        {
          tier: "UNSUPPORTED",
          title: "Stock-browser fingerprint and hardware identity",
          body: "The current Chrome and Edge path does not rewrite TLS, QUIC, or real hardware. Stronger browser engines and environments are future work, not current features.",
          tone: "unsupported",
        },
      ],
      docsAction: "Read the capability model",
    },
    architecture: {
      eyebrow: "LOCAL BY DESIGN",
      title: "The isolation core stays on your machine.",
      intro:
        "The desktop app owns Silo lifecycle and runtime binding. Chrome or Edge owns the browser files. Network providers and the optional Companion add controlled routing and evidence without becoming device impersonation.",
      nodes: [
        {
          kind: "CONTROL",
          title: "Desktop core",
          body: "Creates, edits, archives, restores, and deletes Silos; protects metadata and network secrets in the encrypted Vault; and binds the selected Silo to one active runtime.",
        },
        {
          kind: "IDENTITY",
          title: "Browser and network",
          body: "Keeps browser-owned state in a dedicated data directory and applies the Silo’s explicit direct, fixed-proxy, or external Mihomo network profile with fail-closed behavior where required.",
        },
        {
          kind: "EVIDENCE",
          title: "Companion and Native Host",
          body: "Adds optional page observations, user-triggered exit checks, and local redacted evidence while keeping the desktop isolation boundary usable without the extension.",
        },
      ],
      footnote:
        "Stock Chrome and Edge still expose their real browser engine and hardware environment. Stronger engines, virtual environments, and remote paths are separate future options, not current fingerprint protection.",
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
      eyebrow: "0.1 SOURCE MILESTONE",
      title: "Inspect the claim before you trust it.",
      body: "VeriSilo is being built in public under MPL-2.0. The 0.1 source milestone completes the first identity-isolation baseline, while the signed Windows build, controlled browser engine, and stronger environments remain unfinished.",
      primaryAction: "View VeriSilo on GitHub",
      secondaryAction: "Build from source",
      statusLabel: "STATUS",
      statusValue: "0.1 source milestone · pre-release",
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
      milestone: "当前阶段",
      how: "工作原理",
      evidence: "证据模型",
      boundaries: "能力边界",
      source: "源代码",
    },
    hero: {
      eyebrow: "WINDOWS 优先 · 开源 · 0.1 源码里程碑",
      titleLead: "把浏览器状态",
      titleEmphasis: "放进各自的 Silo。",
      body: "VeriSilo 为每个 Silo 启动一套独立、受管理的 Chrome 或 Edge 环境。浏览器状态留在各自的数据目录中，桌面端负责 Silo 的生命周期、运行绑定与可选网络配置，不导入或修改默认浏览器 Profile。",
      primaryAction: "查看源代码",
      secondaryAction: "查看当前阶段",
      releaseNote: "0.1 源码里程碑已经完成；签名 Windows 构建暂未提供。",
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
    milestone: {
      eyebrow: "MILESTONE 01 · 2026-08",
      title: "身份隔离核心现已端到端工作。",
      intro:
        "VeriSilo 已经从分散的实验能力，进入一套相互连接的 Windows 产品骨架：创建 Silo、隔离浏览器状态、绑定运行环境与网络配置、查看有界证据、停止并恢复环境，同时不触碰默认浏览器 Profile。",
      groups: [
        {
          tone: "closed",
          label: "已实现",
          title: "身份隔离核心",
          items: [
            "本地加密 Vault 与明确的 Silo 生命周期。",
            "独立 Chrome / Edge 数据目录与运行身份绑定。",
            "每个 Silo 独立的直连、固定代理或外部 Mihomo 网络配置。",
            "对必须使用代理的网络路径采用 fail-closed。",
            "可选 Companion、Native Host 与本地脱敏证据报告。",
          ],
        },
        {
          tone: "gated",
          label: "尚未提供",
          title: "公开分发与更强环境",
          items: [
            "暂无签名 Windows 安装包或商店版本。",
            "指纹字段仍来自标准浏览器本身。",
            "真机验证尚未覆盖所有 Windows 与虚拟化组合。",
            "证据覆盖已记录的场景，而非每一台真实机器。",
          ],
        },
        {
          tone: "next",
          label: "下一阶段",
          title: "浏览器指纹一致性",
          items: [
            "同一个 Silo 在重启后保持稳定。",
            "Window、iframe、Worker、请求头和网络观察之间不互相矛盾。",
            "通过受控浏览器引擎协调网站可见字段。",
            "每项控制均支持应用、验证、恢复与按站点回退，不承诺“不可检测”。",
          ],
        },
      ],
      docsAction: "阅读里程碑说明",
    },
    how: {
      eyebrow: "隔离路径",
      title: "一道边界，三个明确步骤。",
      intro:
        "Silo 是一个由浏览器管理的数据目录，也包含解释其启动方式所需的设置与证据。",
      steps: [
        {
          label: "创建",
          title: "在本机定义 Silo",
          body: "在桌面端记录浏览器、运行位置、网络配置与 Silo 元数据。加密 Vault 保护 VeriSilo 元数据和可选网络凭据，而不是浏览器自身的 Profile 文件。",
          note: "本地保险库 · 无需云端账户",
        },
        {
          label: "启动",
          title: "运行绑定后的浏览器环境",
          body: "使用独立数据目录和明确的运行身份启动 Chrome 或 Edge。必须使用代理的路径在失败时阻断连接，而不是静默回退到宿主网络。",
          note: "独立数据目录 · 运行身份",
        },
        {
          label: "检查",
          title: "验证、停止与恢复",
          body: "查看有界的运行和网络证据，按需加入 Companion 观察，导出本地脱敏报告，并在不强杀无关浏览器进程的情况下停止或恢复 Silo。",
          note: "有界证据 · 本地脱敏报告",
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
          title: "独立状态与受控启动",
          body: "桌面端创建的数据目录、Silo 生命周期、运行绑定和受支持的 fail-closed 启动路径都有直接本地证据；已配置、已应用与已验证仍是不同的运行状态。",
          tone: "reliable",
        },
        {
          tier: "尽力",
          title: "页面与网络观察",
          body: "WebRTC 偏好、公共 IP 或 DNS 观察及页面信号仍受浏览器、时序、权限、覆盖范围和证据来源限制。",
          tone: "effort",
        },
        {
          tier: "不支持",
          title: "标准浏览器路径下的指纹与硬件身份",
          body: "当前 Chrome 和 Edge 路径不会改写 TLS、QUIC 或真实硬件。更强的浏览器引擎与环境属于未来工作，而非当前功能。",
          tone: "unsupported",
        },
      ],
      docsAction: "阅读能力模型",
    },
    architecture: {
      eyebrow: "本地优先",
      title: "隔离核心留在你的设备上。",
      intro:
        "桌面端负责 Silo 生命周期与运行绑定，Chrome 或 Edge 管理浏览器文件；网络 Provider 和可选 Companion 增加受控路由与证据，但不会因此变成设备伪装。",
      nodes: [
        {
          kind: "控制",
          title: "桌面核心",
          body: "负责创建、编辑、归档、恢复和删除 Silo；使用加密 Vault 保护元数据与网络凭据；并把所选 Silo 绑定到一个明确的活动运行环境。",
        },
        {
          kind: "身份",
          title: "浏览器与网络",
          body: "将浏览器状态保存在独立数据目录中，并应用该 Silo 明确选择的直连、固定代理或外部 Mihomo 网络配置；需要代理时采用 fail-closed。",
        },
        {
          kind: "证据",
          title: "Companion 与 Native Host",
          body: "提供可选页面观察、用户触发的出口检查和本地脱敏证据；即使不安装扩展，桌面端建立的隔离边界仍然可以工作。",
        },
      ],
      footnote:
        "标准 Chrome 和 Edge 仍会暴露真实浏览器引擎与硬件环境。更强的引擎、虚拟环境和远程路径属于独立的未来选项，不是当前的指纹保护。",
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
      eyebrow: "0.1 源码里程碑",
      title: "先审查主张，再决定是否信任。",
      body: "VeriSilo 以 MPL-2.0 在公开仓库中开发。0.1 源码里程碑完成了第一阶段身份隔离基线，而签名 Windows 构建、受控浏览器引擎与更强的环境仍未完成。",
      primaryAction: "在 GitHub 查看 VeriSilo",
      secondaryAction: "从源码构建",
      statusLabel: "状态",
      statusValue: "0.1 源码里程碑 · 发布前",
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
