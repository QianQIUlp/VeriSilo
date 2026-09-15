export type Locale = "en" | "zh";
export const links = {
  repository: "https://github.com/QianQIUlp/VeriSilo",
  release: "https://github.com/QianQIUlp/VeriSilo/releases/tag/v0.1.0-rc3",
  releaseInstaller:
    "https://github.com/QianQIUlp/VeriSilo/releases/download/v0.1.0-rc3/VeriSilo-Managed-Browser-v0.1.0-rc3-x64-setup.exe",
  architecture:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/architecture.md",
  capabilities:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/capabilities.md",
  threatModel:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/threat-model.md",
  development:
    "https://github.com/QianQIUlp/VeriSilo/blob/main/docs/development.md",
  license: "https://github.com/QianQIUlp/VeriSilo/blob/main/LICENSE",
};
export const copy = {
  zh: {
    meta: {
      title: "VeriSilo — 每一种你，各有引力。",
      description:
        "你的本地身份工作室。独立的浏览器空间、持久身份与看得见的运行证据。无需安装，先在官网亲手体验 VeriSilo。",
    },
    homeLabel: "VeriSilo 首页",
    skipLink: "跳到主要内容",
    languageLabel: "EN",
    nav: ["亲手体验", "为什么不同", "下载"],
    motion: "动态效果",
    motionOptions: ["随系统", "完整动态", "减少动态"],
    hero: {
      eyebrow: "A SPACE FOR EVERY YOU",
      lead: "每一种你，",
      emphasis: "各有引力。",
      body: "给身份一个持续存在的空间。\n让声明、运行与证据，在这里相遇。",
      enter: "进入你的下一种可能",
      secondary: "认识 VeriSilo",
      note: "本地身份工作室 / WINDOWS · OPEN SOURCE",
      interact: "点一下，换一种你",
      names: ["专注于工作", "保持好奇", "自由创作"],
      tags: ["WORK / 01", "EXPLORE / 02", "CREATE / 03"],
      scroll: "向下，进入空间",
      marker: "轮廓用来辨认，信心来自证据。",
    },
    demo: {
      eyebrow: "01 / STEP INSIDE",
      title: "这一刻，轮到你。",
      body: "直接打开，亲手试试。拖动身份轮廓，展开证据，或创造一个新的空间。",
      expand: "展开体验",
      close: "收起体验",
      reset: "重置",
      loading: "正在展开你的工作空间…",
      fallback: "独立打开",
      note: "示例数据 · 不连接你的桌面应用",
      version: "当前开发版界面 · 与 rc3 下载版可能不同",
      mobile:
        "这是完整桌面的缩放视图。展开后可左右滑动操作；建议在电脑上体验。",
      exit: "继续探索 ↓",
    },
    principles: {
      eyebrow: "02 / MORE THAN A PROFILE",
      title: "身份有形。\n边界清楚。",
      intro:
        "把长期使用的身份环境，真正组织起来。每个空间都有自己的连续性，也有清楚的边界。",
      items: [
        {
          title: "每一次回来，还是你。",
          label: "PERSISTENT IDENTITY",
          text: "独立的 Profile 保存各自的网站数据。Managed Silo 将受控身份、引擎与网络配置连接起来，让一个身份能被持续使用、重新观测。",
          aside: "轮廓帮助辨认，不是验证印章。",
        },
        {
          title: "发生了什么，看得见。",
          label: "EVIDENCE, IN THE OPEN",
          text: "把配置声明放在实际观测旁边。知道哪里匹配、哪里不同、什么时候读取，也知道哪些信息尚未取得。",
          aside: "Configured · Applied · Observed · Verified，各有含义。",
        },
        {
          title: "你的空间，你来掌握。",
          label: "LOCAL FIRST",
          text: "身份与配置由本机管理。保险库保护配置和网络凭据，浏览器登录与网站数据保存在各自独立的 Profile 目录。",
          aside: "明确的网络策略；代理必需时，失败就停止。",
        },
      ],
    },
    evidence: {
      eyebrow: "03 / CONFIDENCE THROUGH EVIDENCE",
      title: "安心，\n有迹可循。",
      intro:
        "选择一种观测结果，看看区别。好的证据，也容得下一个诚实的「不知道」。",
      sample: "交互示例 / 时区字段 · 模拟数据",
      expected: "配置声明",
      observed: "页面观测",
      choices: ["匹配", "不一致", "未取得"],
      states: ["Matched", "Mismatched", "Unavailable"],
      observations: ["America/New_York", "Asia/Singapore", "—"],
      explanations: [
        "这一字段与声明一致。它不是对整个身份的无限保证。",
        "观察与声明不同。差异留在视野里，等待检查原因。",
        "没有取得这一次的观测。没有数据，也不会显示为通过。",
      ],
      link: "了解证据与能力边界",
    },
    boundaries: {
      eyebrow: "04 / CLEAR BY DESIGN",
      title: "有主张，也有边界。",
      items: [
        {
          title: "Standard 与 Managed，有什么不同？",
          text: "Standard Silo 在系统 Chrome 或 Edge 中隔离网站数据，设备身份跟随本机。Managed Silo 使用受控 Camoufox 引擎、独立 Profile 与身份 Artifact，并提供运行时观测。两种方式都保留，各有明确用途。",
        },
        {
          title: "哪些事，VeriSilo 不会承诺？",
          text: "不承诺无法检测、绝对匿名或对所有网站兼容。不重写真实硬件，也不把 TLS、QUIC 或所有 DNS 路径都宣称为受控。观测的范围、时间和来源始终重要。",
        },
        {
          title: "官网演示会操作我的电脑吗？",
          text: "不会。它使用真实界面组件和模拟 API，状态只存在当前演示页面中。重置会清除演示操作。它不会打开真实浏览器、探测你的网络出口或读取本机保险库。请不要输入真实口令或代理凭据。",
        },
        {
          title: "下载之前，我需要知道什么？",
          text: "公开版本为 v0.1.0-rc3，属于 Windows x64 预发布版。安装程序尚未进行 Authenticode 签名，Windows 可能显示未知发布者或 SmartScreen 提示。严格的普通用户安装语义仍未得到证明。请在 Release 页面核对 SHA256、来源信息与已知限制。",
        },
      ],
      link: "阅读威胁模型",
    },
    download: {
      eyebrow: "TAKE YOUR SPACE WITH YOU",
      title: "下一种你。\n从这里开始。",
      body: "先试，再把这个空间留在你的电脑上。",
      action: "下载 Windows 预发布版",
      version: "v0.1.0-rc3 · Windows x64",
      source: "打开源代码",
      verify: "校验和、来源与已知限制 ↗",
      note: "公开下载版与上方开发版演示可能不同。安装包暂未进行 Windows 发布者签名。",
      privacy: "隐私政策",
      license: "MPL-2.0 开源",
      footer: "身份有形，证据可见。",
    },
  },
  en: {
    meta: {
      title: "VeriSilo — Every you. Its own gravity.",
      description:
        "Your local identity studio. Persistent browser spaces, controlled identities, and visible runtime evidence. Step inside an interactive VeriSilo demo. No install required.",
    },
    homeLabel: "VeriSilo home",
    skipLink: "Skip to content",
    languageLabel: "中文",
    nav: ["Step inside", "Why VeriSilo", "Download"],
    motion: "Motion",
    motionOptions: ["System", "Full motion", "Reduced"],
    hero: {
      eyebrow: "A SPACE FOR EVERY YOU",
      lead: "Every you.",
      emphasis: "Its own gravity.",
      body: "A lasting space for each identity.\nWhere intention, runtime, and evidence meet.",
      enter: "Step into your next possibility",
      secondary: "Meet VeriSilo",
      note: "YOUR LOCAL IDENTITY STUDIO / WINDOWS · OPEN SOURCE",
      interact: "Tap to meet another you",
      names: [
        "Deep in the work",
        "Following curiosity",
        "Making something new",
      ],
      tags: ["WORK / 01", "EXPLORE / 02", "CREATE / 03"],
      scroll: "Scroll into your space",
      marker: "A shape to recognize. Evidence to understand.",
    },
    demo: {
      eyebrow: "01 / STEP INSIDE",
      title: "Your turn, now.",
      body: "Open it. Move an identity. Follow its evidence. Make room for another you.",
      expand: "Expand the experience",
      close: "Close expanded view",
      reset: "Reset",
      loading: "Opening your workspace…",
      fallback: "Open separately",
      note: "Sample data · No connection to your desktop app",
      version: "Development UI · May differ from the rc3 download",
      mobile:
        "A scaled view of the whole desktop. Expand to pan and interact at full size; best experienced on a computer.",
      exit: "Keep exploring ↓",
    },
    principles: {
      eyebrow: "02 / MORE THAN A PROFILE",
      title: "An identity,\nwith a place.",
      intro:
        "Give a lasting identity environment its own shape, continuity, and clear boundaries.",
      items: [
        {
          title: "Come back as yourself.",
          label: "PERSISTENT IDENTITY",
          text: "Each Profile keeps its own website data. A Managed Silo connects a controlled identity, engine, and network policy into an environment you can keep using and observe again.",
          aside: "Its shape is a recognition aid, never a verification seal.",
        },
        {
          title: "See what actually happened.",
          label: "EVIDENCE, IN THE OPEN",
          text: "Read the declared configuration beside the observation. See what matches, what differs, when it was observed, and what could not be read.",
          aside:
            "Configured, Applied, Observed, and Verified mean different things.",
        },
        {
          title: "Keep the controls close.",
          label: "LOCAL FIRST",
          text: "Identity and configuration stay under local control. The Vault protects configuration and network credentials. Logins and website data live in separate Profile directories.",
          aside: "Explicit network policy. When a required proxy fails, stop.",
        },
      ],
    },
    evidence: {
      eyebrow: "03 / CONFIDENCE THROUGH EVIDENCE",
      title: "Confidence,\nwith a trace.",
      intro:
        "Choose an observation to explore the difference. Good evidence leaves room for an honest “unknown.”",
      sample: "INTERACTIVE EXAMPLE / TIMEZONE · SIMULATED DATA",
      expected: "DECLARED",
      observed: "OBSERVED",
      choices: ["Match", "Difference", "Not observed"],
      states: ["Matched", "Mismatched", "Unavailable"],
      observations: ["America/New_York", "Asia/Singapore", "—"],
      explanations: [
        "This field matches the declaration. It is not an unlimited guarantee about the entire identity.",
        "The observation differs from the declaration. The difference stays visible so you can investigate.",
        "No observation was obtained this time. Missing data is never presented as a pass.",
      ],
      link: "Explore the evidence and capability boundaries",
    },
    boundaries: {
      eyebrow: "04 / CLEAR BY DESIGN",
      title: "Character. And clarity.",
      items: [
        {
          title: "How do Standard and Managed differ?",
          text: "A Standard Silo separates website data in system Chrome or Edge while retaining the local device identity. A Managed Silo uses a controlled Camoufox engine, independent Profile and Identity Artifact, with runtime observations. Both remain available for distinct purposes.",
        },
        {
          title: "What does VeriSilo never promise?",
          text: "No undetectability, absolute anonymity, or universal site compatibility. It does not rewrite physical hardware or claim control of TLS, QUIC, or every DNS path. Observation scope, timing, and provenance always matter.",
        },
        {
          title: "Does the demo control my computer?",
          text: "No. It runs real interface components with simulated APIs. Its state lives only in the current demo page and is cleared by Reset. It does not launch a browser, probe your network exit, or read a local Vault. Do not enter real passphrases or proxy credentials.",
        },
        {
          title: "What should I know before downloading?",
          text: "The public version is v0.1.0-rc3, a Windows x64 pre-release. The installer is not Authenticode-signed and may trigger an Unknown publisher or SmartScreen prompt. Strict standard-user installation semantics remain unproven. Check the Release page for SHA256, provenance, and known limits.",
        },
      ],
      link: "Read the threat model",
    },
    download: {
      eyebrow: "TAKE YOUR SPACE WITH YOU",
      title: "Your next you.\nStarts here.",
      body: "Try the space. Then make it yours, on your computer.",
      action: "Get the Windows pre-release",
      version: "v0.1.0-rc3 · Windows x64",
      source: "Explore the source",
      verify: "Checksums, provenance & known limits ↗",
      note: "The public download may differ from the development demo above. The installer is not Windows publisher-signed yet.",
      privacy: "Privacy",
      license: "MPL-2.0 open source",
      footer: "Identity, with form. Evidence, in view.",
    },
  },
} as const;
