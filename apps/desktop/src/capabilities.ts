export type CapabilityRoute = "current" | "engine" | "local_vm" | "remote";

export type CapabilityTone =
  "available" | "best_effort" | "planned" | "boundary";

export interface ProductCapability {
  id: string;
  name: string;
  currentReality: string;
  route: CapabilityRoute;
  routeLabel: string;
  tone: CapabilityTone;
  evidenceRule: string;
}

export interface EnvironmentLayer {
  id: CapabilityRoute;
  version: string;
  name: string;
  status: "available" | "scheduled";
  summary: string;
  delivers: string[];
}

export const ENVIRONMENT_LAYERS: readonly EnvironmentLayer[] = [
  {
    id: "current",
    version: "现在",
    name: "独立 Silo",
    status: "available",
    summary: "为每个身份启动独立 Chrome/Edge 数据目录，隔离完整网站状态。",
    delivers: [
      "Cookie 与站点数据",
      "浏览历史与权限",
      "固定代理 / 外部 Mihomo fail-closed 路由",
    ],
  },
  {
    id: "engine",
    version: "V0.7",
    name: "受控浏览器引擎",
    status: "scheduled",
    summary: "增加可更新、可回滚、可验证的引擎适配层，协调浏览器可见信号。",
    delivers: ["一致性身份模板", "Canvas/WebGL/字体控制", "TLS/QUIC 能力探测"],
  },
  {
    id: "local_vm",
    version: "V0.8",
    name: "本地虚拟环境",
    status: "scheduled",
    summary:
      "以 WSL Chromium 做轻量 Linux Provider，以 Windows Sandbox / Hyper-V 提供更强边界。",
    delivers: ["Linux / VM 后端", "独立字体与设备视图", "从环境内部验证出口"],
  },
  {
    id: "remote",
    version: "V0.9",
    name: "自托管远程环境",
    status: "scheduled",
    summary: "连接用户自己的远程节点，持久保存环境并从远端验证实际出口。",
    delivers: ["远端浏览器会话", "独立网络栈", "生命周期与审计记录"],
  },
] as const;

export const PRODUCT_CAPABILITIES: readonly ProductCapability[] = [
  {
    id: "site_state",
    name: "账号与完整站点数据",
    currentReality:
      "当前已由独立 user-data-dir 隔离 Cookie、LocalStorage、IndexedDB、缓存和 Service Worker。",
    route: "current",
    routeLabel: "当前可用",
    tone: "available",
    evidenceRule: "以不同 Silo 登录状态互不可见、重启后状态仍保留为验收证据。",
  },
  {
    id: "proxy",
    name: "固定代理与外部 Mihomo",
    currentReality:
      "当前可把每个 Silo 固定到 HTTP/SOCKS 端点或本机 Mihomo 节点；可选凭据进入 Vault，必须代理不含 DIRECT 回退。",
    route: "current",
    routeLabel: "启动保护已实现",
    tone: "available",
    evidenceRule:
      "端点、认证、节点回读和浏览器路由分别记录；只有 Silo 内 Companion 主动检查后才能把公网出口标为已验证。",
  },
  {
    id: "network_observation",
    name: "出口 IP 与公共 DNS",
    currentReality:
      "桌面控制器与 Silo 内 Companion 均可由用户主动检查各自请求看到的公网 IP、地区、ASN、出口时区和公共 DoH 答案一致性。",
    route: "current",
    routeLabel: "主动验证",
    tone: "available",
    evidenceRule:
      "结果只描述本次请求；不把公共 DoH 一致性宣传成本机 DNS 无劫持。",
  },
  {
    id: "identity_signals",
    name: "UA、语言与时区",
    currentReality:
      "独立 Profile 不会自动生成另一台设备；扩展页面修改覆盖不完整且可被网站观察。",
    route: "engine",
    routeLabel: "V0.7 受控引擎",
    tone: "planned",
    evidenceRule: "跨 Window、iframe、Worker 与请求头一致后，才可标为已验证。",
  },
  {
    id: "rendering_signals",
    name: "Canvas、WebGL 与字体",
    currentReality:
      "当前 Chrome/Edge 启动器不会改变真实渲染栈；专用引擎可协调可见值，VM 提供更强系统边界。",
    route: "engine",
    routeLabel: "V0.7 引擎 / V0.8 VM",
    tone: "planned",
    evidenceRule:
      "必须验证稳定性、跨上下文一致性和站点兼容性，不承诺不可检测。",
  },
  {
    id: "quic",
    name: "QUIC / HTTP/3",
    currentReality:
      "必须代理的当前启动链会加入禁用 QUIC 参数，减少绕过 TCP 代理的路径；尚未进行协议层观测，不能标为已验证关闭。",
    route: "current",
    routeLabel: "已应用 / 待协议验证",
    tone: "best_effort",
    evidenceRule: "只有启动参数生效并由网络测试确认协议未使用，才显示已应用。",
  },
  {
    id: "tls",
    name: "TLS 指纹",
    currentReality:
      "TLS 由浏览器网络栈决定，独立 Profile 无法改变；需要受控引擎或不同的远程网络栈。",
    route: "engine",
    routeLabel: "引擎 / 远程环境",
    tone: "boundary",
    evidenceRule:
      "按真实 ClientHello 与协议协商结果验证，不根据页面 JavaScript 推测。",
  },
  {
    id: "hardware",
    name: "真实硬件与操作系统",
    currentReality:
      "user-data-dir 不会改变 CPU、GPU 或 Windows；要获得不同系统边界必须运行 VM、远程环境或真实设备。",
    route: "local_vm",
    routeLabel: "V0.8 VM / V0.9 远程",
    tone: "boundary",
    evidenceRule:
      "明确显示环境来源与透传设备，绝不把字段改写描述成真实硬件变化。",
  },
] as const;
