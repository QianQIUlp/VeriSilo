import type {
  RuntimeActivation,
  RuntimeIdentityEvidence,
  RuntimeIdentitySignal,
  Silo,
} from "@verisilo/contracts";

import "./evidence-diagnostics.css";

export const identityEvidenceLabels = {
  matched: ["Matched", "已匹配"],
  mismatched: ["Mismatched", "不匹配"],
  unavailable: ["Unavailable", "不可用"],
  stale: ["Stale", "归属已失效"],
} as const;

const signalLabels: Record<string, string> = {
  userAgent: "浏览器标识",
  language: "语言",
  platform: "系统平台",
  oscpu: "系统信息",
  screen: "屏幕",
  devicePixelRatio: "像素比例",
  hardwareConcurrency: "硬件线程",
  historyLength: "历史记录",
  timezone: "时区",
  utcOffsetMinutes: "UTC 偏移",
  globalPrivacyControl: "GPC",
  doNotTrack: "DNT",
  mediaDevices: "媒体设备",
  webglVendor: "WebGL 厂商",
  webglRenderer: "WebGL 渲染器",
  webgl2Vendor: "WebGL 2 厂商",
  webgl2Renderer: "WebGL 2 渲染器",
  acceptEncoding: "接受的压缩编码",
  voices: "语音",
  fonts: "字体",
};

const signalPriority = { mismatched: 0, unavailable: 1, stale: 2, matched: 3 };

/** This guards presentation only. Host reconciliation remains the sole verdict source. */
export function identityEvidenceContext(activation: RuntimeActivation, silo: Silo): {
  evidence: RuntimeIdentityEvidence | null;
  current: boolean;
  scopeLabel: string;
  note: string | null;
} {
  const evidence = activation.identityEvidence?.siloId === silo.id
    ? activation.identityEvidence
    : null;
  if (evidence === null) return { evidence: null, current: false, scopeLabel: "无观察", note: null };
  if (activation.activeSiloId !== silo.id || activation.state !== "running") {
    return { evidence, current: false, scopeLabel: "最后已知", note: "上次运行的最后已知观察，不能代表当前会话。" };
  }
  const binding = silo.engine.adapter === "camoufox"
    ? silo.engine.artifactBinding
    : undefined;
  if (
    evidence.engineAdapter !== silo.engine.adapter ||
    binding === undefined ||
    evidence.artifactId !== binding.artifactId ||
    evidence.artifactFileSha256 !== binding.artifactFileSha256
  ) {
    return { evidence, current: false, scopeLabel: "归属未确认", note: "这份观察不属于当前 Artifact，不能代表当前身份。" };
  }
  if (evidence.state === "stale") {
    return { evidence, current: false, scopeLabel: "最后已知", note: "Host 标记这份身份观察的归属已失效。" };
  }
  if (activation.networkEvidence === null) {
    return { evidence, current: false, scopeLabel: "运行归属未确认", note: "缺少可关联的网络 runtime 证据，不能确认这份身份观察属于当前运行。" };
  }
  if (activation.networkEvidence.runtimeId !== evidence.runtimeId) {
    return { evidence, current: false, scopeLabel: "最后已知", note: "这份观察来自另一次运行，不能代表当前会话。" };
  }
  return { evidence, current: true, scopeLabel: "当前运行", note: null };
}

export function safeEvidenceReason(reason: string | undefined): string | null {
  if (!reason) return null;
  // Reasons are prose from Host. Never dump a serialized runtime payload into UI.
  if (/^\s*[\[{]/u.test(reason) || /"(?:runtimeId|sessionId|artifactFileSha256|signals)"\s*:/u.test(reason)) {
    return "Host 未提供可直接展示的说明。";
  }
  return reason;
}

function evidenceValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "object") {
    try {
      return JSON.stringify(value, null, 2) ?? "—";
    } catch {
      return "无法显示此值";
    }
  }
  if (typeof value === "boolean") return value ? "是" : "否";
  return String(value);
}

function valuePreview(signal: string, value: unknown, full: string): string {
  if (signal === "voices" && Array.isArray(value)) {
    const names = value.slice(0, 5).map((entry) => {
      if (typeof entry === "string") return entry;
      if (entry !== null && typeof entry === "object") {
        const voice = entry as { name?: unknown; lang?: unknown };
        if (typeof voice.name === "string") {
          return `${voice.name}${typeof voice.lang === "string" ? ` (${voice.lang})` : ""}`;
        }
      }
      return evidenceValue(entry);
    });
    return `${names.join("、") || "无语音"}${value.length > 5 ? ` 等 ${value.length} 项` : ""} · 展开完整值`;
  }
  if (signal === "screen" && value !== null && typeof value === "object" && !Array.isArray(value)) {
    const screen = value as { width?: unknown; height?: unknown };
    if (screen.width !== undefined && screen.height !== undefined) {
      return `${screen.width}×${screen.height} · 展开完整值`;
    }
  }
  if (signal === "mediaDevices" && value !== null && typeof value === "object" && !Array.isArray(value)) {
    const counts = value as Record<string, unknown>;
    return `麦克风 ${String(counts.audioinput ?? "—")} · 摄像头 ${String(counts.videoinput ?? "—")} · 输出 ${String(counts.audiooutput ?? "—")} · 展开完整值`;
  }
  if (Array.isArray(value)) return `${value.slice(0, 5).map(evidenceValue).join("、")}${value.length > 5 ? ` 等 ${value.length} 项` : ""} · 展开完整值`;
  return typeof value === "object" ? "对象 · 展开完整值" : `${full.slice(0, 80)}… · 展开完整值`;
}

function EvidenceValue({ signal, value }: { signal: string; value: unknown }) {
  const full = evidenceValue(value);
  if (typeof value === "object" && value !== null || full.length > 120) {
    const preview = valuePreview(signal, value, full);
    return <details className="identity-evidence-value"><summary>{preview}</summary><pre>{full}</pre></details>;
  }
  return <pre>{full}</pre>;
}

function SignalRows({ signals }: { signals: RuntimeIdentitySignal[] }) {
  return (
    <dl className="identity-evidence-table">
      {signals.map((signal) => (
        <div className={`identity-evidence-row ${signal.state}`} key={signal.signal}>
          <dt>
            {signalLabels[signal.signal] ?? `其他字段：${signal.signal}`}
            <small>{identityEvidenceLabels[signal.state][1]}</small>
          </dt>
          <dd><small>期望</small><EvidenceValue signal={signal.signal} value={signal.expected} /></dd>
          <dd><small>观察</small><EvidenceValue signal={signal.signal} value={signal.observed} /></dd>
          {safeEvidenceReason(signal.reason) ? <p>{safeEvidenceReason(signal.reason)}</p> : null}
        </div>
      ))}
    </dl>
  );
}

export function IdentityEvidenceSignals({ signals }: { signals: RuntimeIdentitySignal[] }) {
  const ordered = [...signals].sort((a, b) => signalPriority[a.state] - signalPriority[b.state]);
  const priority = ordered.filter((signal) => signal.state !== "matched");
  const matched = ordered.filter((signal) => signal.state === "matched");
  return (
    <div className="identity-evidence-signals">
      {priority.length > 0 ? <SignalRows signals={priority} /> : null}
      {matched.length > 0 ? (
        <details className="identity-evidence-details">
          <summary>查看已匹配字段（{matched.length}）</summary>
          <SignalRows signals={matched} />
        </details>
      ) : null}
      {ordered.length === 0 ? <p>这份证据没有逐字段结果。</p> : null}
    </div>
  );
}
