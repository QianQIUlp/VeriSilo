import {
  analyzeConsistency,
  type ObservationReport,
  type ObservedSignal,
} from "@verisilo/contracts";

export type SummaryTone = "normal" | "info" | "attention";

export interface HumanFact {
  id: string;
  label: string;
  value: string;
  detail: string;
}

export interface HumanFinding {
  id: string;
  tone: SummaryTone;
  title: string;
  detail: string;
  action?: string;
}

export interface HumanReportSummary {
  tone: SummaryTone;
  headline: string;
  description: string;
  facts: HumanFact[];
  findings: HumanFinding[];
}

export function summarizeReport(report: ObservationReport): HumanReportSummary {
  const navigatorValue = signalRecord(report, "navigator");
  const uaChValue = signalRecord(report, "ua_ch");
  const screenValue = signalRecord(report, "screen");
  const webGlValue = signalRecord(report, "webgl");
  const mediaValue = signalRecord(report, "media_devices");
  const permissionsValue = signalRecord(report, "permissions");
  const storageValue = signalRecord(report, "storage");
  const timezone = signalPrimitive(report, "timezone");
  const userAgent = stringValue(navigatorValue?.userAgent);
  const language = stringValue(navigatorValue?.language) ?? "未知";
  const platform = stringValue(navigatorValue?.platform) ?? "未知平台";
  const bitness = stringValue(recordValue(uaChValue?.highEntropy)?.bitness);

  const facts: HumanFact[] = [
    {
      id: "browser",
      label: "浏览器与系统",
      value: `${browserLabel(userAgent)} · ${systemLabel(userAgent, platform, bitness)}`,
      detail: "当前页面可读取这些值；扩展只做 observed 观测，不控制或验证它们。",
    },
    {
      id: "region",
      label: "语言与时区",
      value: `${language} · ${typeof timezone === "string" ? timezone : "未知时区"}`,
      detail:
        "语言、时区和网络出口地区若不符合预期，网站可能看到不自然的组合。",
    },
    {
      id: "network",
      label: "网络出口",
      value: "本次未验证",
      detail:
        "IP、ASN、出口地区和 DNS 是多账号环境的关键部分；扩展不会自动连接检测服务。",
    },
    {
      id: "site-state",
      label: "登录与站点数据",
      value: siteStateLabel(storageValue),
      detail:
        "同一普通浏览器环境中的 Cookie 和站点存储会继续关联访问；隐私窗口只提供一个临时边界，长期分离需独立 Silo Profile。",
    },
    {
      id: "display",
      label: "设备与屏幕",
      value: screenLabel(screenValue, navigatorValue),
      detail: "分辨率、缩放比例、内存和 CPU 线程数会增加设备辨识度。",
    },
    {
      id: "graphics",
      label: "显卡与图形",
      value: graphicsLabel(webGlValue),
      detail: "WebGL/WebGPU 可暴露显卡型号和驱动渲染路径。",
    },
  ];

  const findings: HumanFinding[] = analyzeConsistency(report).map(
    (finding) => ({
      id: finding.id,
      tone: finding.severity === "warning" ? "attention" : finding.severity,
      title: finding.title,
      detail: finding.beginnerSummary,
    }),
  );

  if (mediaValue?.labelsExposed === true) {
    findings.push({
      id: "media-labels-exposed",
      tone: "attention",
      title: "摄像头或麦克风名称可见",
      detail: "页面已能看到媒体设备标签；具体设备名称可能增强身份关联。",
      action: "不使用时撤销该站点的摄像头和麦克风权限。",
    });
  }

  const exposedSignals = [
    report.signals.some(
      (signal) => signal.id === "canvas_hash" && signal.status === "ok",
    )
      ? "Canvas"
      : null,
    report.signals.some(
      (signal) => signal.id === "audio" && signal.status === "ok",
    )
      ? "音频特征"
      : null,
    report.signals.some(
      (signal) => signal.id === "webgl" && signal.status === "ok",
    )
      ? "显卡信息"
      : null,
    report.signals.some(
      (signal) => signal.id === "fonts" && signal.status === "ok",
    )
      ? "字体集合"
      : null,
  ].filter((value): value is string => value !== null);
  if (exposedSignals.length > 0) {
    findings.push({
      id: "high-entropy-signals-visible",
      tone: "info",
      title: "页面可读取较强的设备特征",
      detail: `本次可见：${exposedSignals.join("、")}。它们不等于泄漏了真实姓名，但可用于关联多次访问。`,
      action:
        "独立 Standard Silo 可分开网站状态，但设备与指纹仍跟随本机。",
    });
  }

  if (storageValue?.cookiesEnabled === true) {
    findings.push({
      id: "site-state-enabled",
      tone: "info",
      title: "网站登录状态会正常保存",
      detail:
        "Cookie、LocalStorage 和 IndexedDB 可用；它们便于保持登录，也能关联同一浏览器环境中的访问。",
      action: "不同账户需要真正分离时，使用独立 Silo 或临时隔离窗口。",
    });
  }

  const locationPermission = stringValue(permissionsValue?.geolocation);
  if (locationPermission === "denied") {
    findings.push({
      id: "location-denied",
      tone: "normal",
      title: "精确位置权限已拒绝",
      detail:
        "该页面不能直接读取浏览器提供的精确地理位置。网络出口仍可能暴露大致地区。",
    });
  }

  if (stringValue(permissionsValue?.notifications) === "granted") {
    findings.push({
      id: "notifications-granted",
      tone: "info",
      title: "该站点可以发送通知",
      detail:
        "通知权限不会直接泄漏密码，但会保留一项站点授权，并可能暴露账号使用痕迹。",
      action: "不需要时，可在浏览器的站点权限中撤销通知。",
    });
  }

  const hasAttention = findings.some((finding) => finding.tone === "attention");
  return {
    tone: hasAttention ? "attention" : "normal",
    headline: hasAttention
      ? "当前页面观测有几项值得关注"
      : "当前页面观测未发现明显矛盾",
    description: `已从当前页面采集 ${report.signals.length} 组浏览器信号，并提炼成 ${findings.length} 条解读；这是有限 observed 观测，不是 verified 身份证明。`,
    facts,
    findings,
  };
}

function signalRecord(
  report: ObservationReport,
  signalId: string,
): Record<string, unknown> | null {
  return recordValue(signal(report, signalId)?.value);
}

function signalPrimitive(report: ObservationReport, signalId: string): unknown {
  return signal(report, signalId)?.value;
}

function signal(
  report: ObservationReport,
  signalId: string,
): ObservedSignal | undefined {
  return report.signals.find((candidate) => candidate.id === signalId);
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function browserLabel(userAgent: string | null): string {
  const edge = userAgent?.match(/Edg\/([\d.]+)/u)?.[1];
  if (edge !== undefined) {
    return `Microsoft Edge ${edge.split(".")[0]}`;
  }
  const chrome = userAgent?.match(/Chrome\/([\d.]+)/u)?.[1];
  if (chrome !== undefined) {
    return `Chrome ${chrome.split(".")[0]}`;
  }
  return "未知浏览器";
}

function systemLabel(
  userAgent: string | null,
  platform: string,
  bitness: string | null,
): string {
  if (userAgent?.includes("Windows NT 10.0") === true || platform === "Win32") {
    return `Windows 10/11${bitness === null ? "" : ` · ${bitness} 位`}`;
  }
  if (userAgent?.includes("Mac OS X") === true) {
    return "macOS";
  }
  if (userAgent?.includes("Linux") === true) {
    return "Linux";
  }
  return platform;
}

function screenLabel(
  screenValue: Record<string, unknown> | null,
  navigatorValue: Record<string, unknown> | null,
): string {
  const width = numberValue(screenValue?.width);
  const height = numberValue(screenValue?.height);
  const ratio = numberValue(screenValue?.pixelRatio);
  const memory = numberValue(navigatorValue?.deviceMemory);
  const threads = numberValue(navigatorValue?.hardwareConcurrency);
  const parts = [
    width === null || height === null ? null : `${width} × ${height}`,
    ratio === null ? null : `${ratio}× 缩放`,
    memory === null ? null : `${memory} GB 内存`,
    threads === null ? null : `${threads} 线程`,
  ].filter((value): value is string => value !== null);
  return parts.length > 0 ? parts.join(" · ") : "设备信息不可用";
}

function graphicsLabel(webGlValue: Record<string, unknown> | null): string {
  const renderer = stringValue(webGlValue?.renderer);
  if (renderer === null) {
    return "显卡型号未暴露";
  }
  const rendererParts = renderer
    .split(/[,()]/u)
    .map((part) => part.trim())
    .filter((part) => part !== "");
  const model = rendererParts.find((part) =>
    /(?:GeForce|Radeon|Intel.*(?:Graphics|GPU)|Apple M\d)/iu.test(part),
  );
  return model ?? renderer.slice(0, 72);
}

function siteStateLabel(storageValue: Record<string, unknown> | null): string {
  if (storageValue === null) {
    return "未能确认";
  }
  const available = [
    storageValue.cookiesEnabled,
    storageValue.localStorage,
    storageValue.indexedDb,
  ].filter((value) => value === true).length;
  if (available === 3) {
    return "会保留登录状态";
  }
  if (available === 0) {
    return "主要存储不可用";
  }
  return "部分存储可用";
}
