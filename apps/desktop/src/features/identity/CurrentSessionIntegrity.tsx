import { type RuntimeActivation, type Silo } from "@verisilo/contracts";

import { formatDate } from "../../shared/presentation.js";
import { identityEvidenceContext, safeEvidenceReason } from "./evidence-diagnostics.js";

type RuntimeNetworkEvidence = NonNullable<
  RuntimeActivation["networkEvidence"]
>;

export type CurrentSessionTone = "good" | "warn" | "danger" | "neutral";

/** How a row affects the overall session state. */
export type CurrentSessionImpact = "ok" | "unconfirmed" | "attention";

export interface CurrentSessionRow {
  key: "identity" | "engine" | "network" | "attribution";
  label: string;
  stateLabel: string;
  detail: string;
  tone: CurrentSessionTone;
  impact: CurrentSessionImpact;
}

export type CurrentSessionOverall =
  | "starting"
  | "no_anomaly"
  | "attention"
  | "partial"
  | "stopped";

export interface CurrentSessionSummary {
  overall: CurrentSessionOverall;
  overallLabel: string;
  overallDetail: string;
  rows: CurrentSessionRow[];
}

const networkProviderLabels: Record<
  RuntimeNetworkEvidence["provider"],
  string
> = {
  direct: "直连",
  fixed_proxy: "固定代理",
  external_mihomo: "本机 Clash 代理",
  pac: "PAC 代理",
};

const networkProvenanceLabels: Record<
  RuntimeNetworkEvidence["provenance"],
  string
> = {
  desktop_control_plane: "桌面控制面",
  extension_asserted: "浏览器内检查断言",
  relay_observed: "受管中继观测",
};

const networkPhaseLabels: Record<RuntimeNetworkEvidence["exit"], string> = {
  not_applicable: "不适用",
  not_requested: "未请求",
  configured: "已配置",
  reachable: "可达",
  applied: "已应用",
  observed: "已观察",
  verified: "已验证",
  failed: "失败",
  unavailable: "不可用",
};

const networkPhases: Array<[keyof Pick<RuntimeNetworkEvidence,
  "configuration" | "controllerBinding" | "endpoint" | "authentication" |
  "browserRouting" | "exit" | "dns" | "webRtc">, string]> = [
  ["configuration", "配置"],
  ["controllerBinding", "控制器绑定"],
  ["endpoint", "端点"],
  ["authentication", "认证"],
  ["browserRouting", "浏览器路由"],
  ["exit", "出口"],
  ["dns", "DNS"],
  ["webRtc", "WebRTC"],
];

type NetworkScope = "current" | "missing" | "other_silo" | "last_known" | "expired" | "runtime_mismatch";

function networkEvidenceScope(activation: RuntimeActivation, silo: Silo, now: number): NetworkScope {
  if (activation.activeSiloId !== silo.id) return "other_silo";
  const network = activation.networkEvidence;
  if (network === null) return "missing";
  if (activation.state !== "running") return "last_known";
  if (activation.identityEvidence?.siloId === silo.id &&
    activation.identityEvidence.runtimeId !== network.runtimeId) return "runtime_mismatch";
  if (network.expiresAt !== null && formatExpiry(network.expiresAt, now) === "最近证据已过期") return "expired";
  return "current";
}

/** Shared stage display for the session summary and the network lens. */
export function NetworkEvidenceDetails({
  activation,
  silo,
  now = Date.now(),
}: {
  activation: RuntimeActivation;
  silo: Silo;
  now?: number;
}) {
  const scope = networkEvidenceScope(activation, silo, now);
  const evidence = scope === "other_silo" ? null : activation.networkEvidence;
  if (evidence === null) {
    return <p className="network-evidence-empty">{silo.networkProfile.mode === "direct"
      ? "直连策略已配置，尚无出口观察。"
      : "尚无这次运行的网络阶段证据。"}</p>;
  }
  const expired = evidence.expiresAt !== null && formatExpiry(evidence.expiresAt, now) === "最近证据已过期";
  const scopeNote = {
    current: "当前运行记录",
    missing: "尚无证据",
    other_silo: "非所选 Silo",
    last_known: "最后已知记录；非当前有效",
    expired: "证据已过期；非当前有效",
    runtime_mismatch: "身份与网络证据来自不同 runtime；非当前有效",
  }[scope];
  return (
    <div className="network-evidence-details">
      <p>
        {networkProviderLabels[evidence.provider]} · {evidence.endpointLabel ?? "无端点名称"}
        {` · ${scopeNote}`}
      </p>
      <p>来源：{networkProvenanceLabels[evidence.provenance]}；认证来源：{networkProvenanceLabels[evidence.authenticationProvenance]}</p>
      <p>观察于 <time dateTime={evidence.observedAt}>{formatDate(evidence.observedAt)}</time>；
        {evidence.expiresAt === null ? "未提供有效期" : expired
          ? `已过期（${formatDate(evidence.expiresAt)}）`
          : `有效至 ${formatDate(evidence.expiresAt)}`}</p>
      <dl>
        {networkPhases.map(([key, label]) => (
          <div key={key}><dt>{label}</dt><dd>{networkPhaseLabels[evidence[key]]}</dd></div>
        ))}
      </dl>
    </div>
  );
}

function row(
  key: CurrentSessionRow["key"],
  label: string,
  stateLabel: string,
  detail: string,
  tone: CurrentSessionTone,
  impact: CurrentSessionImpact,
): CurrentSessionRow {
  return { key, label, stateLabel, detail, tone, impact };
}

/** Display-only freshness for the identity observation. This is not a TTL:
 * staleness of identity evidence stays an attribution/binding decision made
 * by reconciliation, never an age threshold. */
function formatObservedFreshness(observedAt: string, now: number): string {
  const observed = new Date(observedAt).getTime();
  if (!Number.isFinite(observed) || observed <= 0) {
    return `上次观察于 ${formatDate(observedAt)}`;
  }
  const age = now - observed;
  if (age >= 0 && age < 60_000) {
    return "不到 1 分钟前观察";
  }
  if (age >= 0 && age < 3_600_000) {
    return `${Math.floor(age / 60_000)} 分钟前观察`;
  }
  return `上次观察于 ${formatDate(observedAt)}`;
}

function formatExpiry(expiresAt: string, now: number): string | null {
  const expiry = new Date(expiresAt).getTime();
  if (!Number.isFinite(expiry) || expiry <= 0) {
    return null;
  }
  return expiry < now
    ? "最近证据已过期"
    : `证据有效至 ${formatDate(expiresAt)}`;
}

function deriveIdentityRow(
  activation: RuntimeActivation,
  silo: Silo,
  now: number,
): CurrentSessionRow {
  const context = identityEvidenceContext(activation, silo);
  const identity = context.evidence;
  const starting = ["preflight", "launching"].includes(activation.state);
  const running = activation.state === "running";
  if (identity === null) {
    if (starting) {
      return row(
        "identity",
        "身份",
        "等待观察",
        "打开后 Host 才会取得网站可见身份。",
        "neutral",
        "unconfirmed",
      );
    }
    return running
      ? row(
          "identity",
          "身份",
          "Unavailable",
          "这次运行还没有取得网站可见身份；可点「重新检查」。",
          "danger",
          "unconfirmed",
        )
      : row(
          "identity",
          "身份",
          "无法确认",
          "这次运行没有取得网站可见身份。",
          "neutral",
          "unconfirmed",
        );
  }
  if (!context.current) {
    return row(
      "identity",
      "身份",
      identity.state === "stale" || activation.networkEvidence?.runtimeId !== identity.runtimeId && activation.networkEvidence !== null
        ? "Stale" : "归属未确认",
      context.note ?? "这份观察不能代表当前会话。",
      "warn",
      "unconfirmed",
    );
  }
  switch (identity.state) {
    case "matched":
      const unavailableCount = identity.signals.filter((signal) => signal.state === "unavailable").length;
      return row(
        "identity",
        "身份",
        "Matched",
        `Host 判定网站可见身份匹配 · ${formatObservedFreshness(identity.observedAt, now)}${unavailableCount > 0 ? ` · ${unavailableCount} 项字段不可用` : ""}`,
        "good",
        unavailableCount > 0 ? "unconfirmed" : "ok",
      );
    case "mismatched":
      return row(
        "identity",
        "身份",
        "Mismatched",
        safeEvidenceReason(identity.reason) ?? "当前观测与声明身份不一致；可点「重新检查」。",
        "warn",
        "attention",
      );
    case "unavailable":
      return row(
        "identity",
        "身份",
        "Unavailable",
        safeEvidenceReason(identity.reason) ?? "这次无法取得网站可见身份；可点「重新检查」。",
        "danger",
        "unconfirmed",
      );
    case "stale":
      return row(
        "identity",
        "身份",
        "Stale",
        safeEvidenceReason(identity.reason) ?? "这份证据不再属于当前 Artifact 或活动 runtime。",
        "warn",
        "unconfirmed",
      );
  }
}

function deriveEngineRow(
  activation: RuntimeActivation,
  managedEngineReady: boolean,
): CurrentSessionRow {
  const state = activation.state;
  const engine = activation.engineEvidence;
  if (state === "verification_failed") {
    return row(
      "engine",
      "引擎",
      "已结束",
      activation.message ?? (activation.activeSiloId === null
        ? "这次运行的检查未通过，浏览器已关闭；请检查原因后重新打开。"
        : "这次运行已结束；请点「结束会话」后再打开浏览器。"),
      "warn",
      "attention",
    );
  }
  if (state === "recovery_required") {
    return row(
      "engine",
      "引擎",
      "需要确认",
      activation.message ?? "上次浏览还没完全结束；请关掉残留窗口后再确认。",
      "warn",
      "attention",
    );
  }
  if (state === "failed") {
    return row(
      "engine",
      "引擎",
      "启动未成功",
      activation.message ?? "浏览器没有打开成功。",
      "danger",
      "attention",
    );
  }
  if (["preflight", "launching"].includes(state)) {
    return row(
      "engine",
      "引擎",
      "正在启动",
      managedEngineReady
        ? "内置浏览器包可用；正在启动 Camoufox Host。"
        : "正在启动 Camoufox Host。",
      "neutral",
      "unconfirmed",
    );
  }
  const packageVerified =
    engine?.packageVerification === "verified" &&
    engine.packageVerificationDetails !== null;
  const hostVerified =
    engine?.hostLaunch === "verified" &&
    engine?.verifiedAdapter === "camoufox";
  const engineBlocked =
    engine !== null &&
    [engine.hostLaunch, engine.packageVerification].some(
      (phase) => phase === "failed" || phase === "unavailable",
    );
  if (engineBlocked) {
    return row(
      "engine",
      "引擎",
      "需要重新检查",
      "本次运行的引擎检查未通过；请结束会话后重新打开。",
      "danger",
      "attention",
    );
  }
  if (hostVerified) {
    return row(
      "engine",
      "引擎",
      "运行正常",
      packageVerified
        ? "Camoufox Host 已启动；内置浏览器包校验通过。"
        : "Camoufox Host 已启动。",
      "good",
      "ok",
    );
  }
  if (engine !== null && ["observed", "applied"].includes(engine.hostLaunch)) {
    return row(
      "engine",
      "引擎",
      "已启动",
      "Camoufox Host 已启动；本次运行的检查仍在进行。",
      "neutral",
      "unconfirmed",
    );
  }
  if (managedEngineReady) {
    return row(
      "engine",
      "引擎",
      "引擎可用",
      "内置浏览器包健康；本次运行的引擎检查尚未完成。",
      "neutral",
      "unconfirmed",
    );
  }
  return row(
    "engine",
    "引擎",
    "无法确认",
    "当前无法确认引擎状态；可点「重新检查」。",
    "danger",
    "unconfirmed",
  );
}

export function deriveNetworkRow(
  activation: RuntimeActivation,
  silo: Silo,
  now: number,
): CurrentSessionRow {
  const network = activation.networkEvidence;
  const proxyRequired = silo.networkProfile.proxyRequired;
  const starting = ["preflight", "launching"].includes(activation.state);
  const running = activation.state === "running";
  const scope = networkEvidenceScope(activation, silo, now);
  if (scope === "other_silo") {
    return row("network", "网络", "非当前运行", "这份网络状态不属于所选 Silo。", "neutral", "unconfirmed");
  }
  if (network === null) {
    if (starting) {
      return row(
        "network",
        "网络",
        "等待检查",
        proxyRequired
          ? "正在准备代理网络；打开后取得出口观察。"
          : "正在准备网络；打开后取得出口观察。",
        "neutral",
        "unconfirmed",
      );
    }
    if (proxyRequired && running) {
      return row(
        "network",
        "网络",
        "尚未确认出口",
        "当前策略要求代理出口；这次运行还没有出口观察。",
        "warn",
        "unconfirmed",
      );
    }
    return silo.networkProfile.mode === "direct"
      ? row("network", "网络", "直连已配置", "当前策略为直连；尚无出口观察。", "neutral", "unconfirmed")
      : row("network", "网络", "代理策略已配置", "当前配置了代理；尚无出口观察。", "neutral", "unconfirmed");
  }
  if (scope === "last_known") {
    return row("network", "网络", "最后已知出口", "这份网络记录来自已结束或尚未完成的运行；非当前有效。", "warn", "unconfirmed");
  }
  if (scope === "runtime_mismatch") {
    return row("network", "网络", "运行归属未确认", "网络与身份观察来自不同运行；非当前有效。", "warn", "unconfirmed");
  }
  const expiry =
    network.expiresAt === null ? null : formatExpiry(network.expiresAt, now);
  if (scope === "expired") {
    return row(
      "network",
      "网络",
      "无法确认当前出口",
      proxyRequired
        ? "当前策略要求代理出口；最近证据已过期，非当前有效，可点「重新检查」。"
        : "最近证据已过期，非当前有效；可点「重新检查」。",
      "warn",
      proxyRequired ? "attention" : "unconfirmed",
    );
  }
  if ([network.configuration, network.endpoint, network.browserRouting,
    network.exit].some((phase) => phase === "failed" || phase === "unavailable")) {
    return row(
      "network",
      "网络",
      "网络检查未通过",
      "本次运行的网络检查未通过；不会改成直连。",
      "danger",
      "attention",
    );
  }
  if (network.exit === "observed" || network.exit === "verified") {
    const endpoint = network.endpointLabel ?? networkProviderLabels[network.provider];
    const unavailable = networkPhases
      .filter(([key]) => evidencePhaseUnconfirmed(network[key]))
      .map(([, label]) => label);
    return row(
      "network",
      "网络",
      network.provider === "direct" ? "直连已观测" : "代理出口已观测",
      `${endpoint} · ${expiry ?? formatObservedFreshness(network.observedAt, now)} · 出口观察由${networkProvenanceLabels[network.provenance]}提供${unavailable.length ? ` · ${unavailable.join("、")}尚未确认` : ""}`,
      "good",
      unavailable.length ? "unconfirmed" : "ok",
    );
  }
  return row(
    "network",
    "网络",
    "出口观察进行中",
    "已应用网络配置；尚未取得出口观察。",
    "neutral",
    "unconfirmed",
  );
}

function evidencePhaseUnconfirmed(state: RuntimeNetworkEvidence["exit"]): boolean {
  return state === "unavailable" || state === "not_requested";
}

function deriveAttributionRow(
  activation: RuntimeActivation,
  silo: Silo,
): CurrentSessionRow {
  const identity = identityEvidenceContext(activation, silo);
  const network = activation.activeSiloId === silo.id ? activation.networkEvidence : null;
  if (identity.evidence === null || network === null) {
    return row(
      "attribution",
      "运行归属",
      "尚未确认",
      "Profile 配置属于此 Silo；当前缺少身份与网络的共同 runtime 关联。",
      "neutral",
      "unconfirmed",
    );
  }
  if (!identity.current) {
    return row(
      "attribution",
      "运行归属",
      network.runtimeId === identity.evidence.runtimeId ? "归属未确认" : "证据不属于当前运行",
      identity.note ?? "这份证据不能代表当前会话。",
      "warn",
      network.runtimeId === identity.evidence.runtimeId ? "unconfirmed" : "attention",
    );
  }
  return row(
    "attribution",
    "运行归属",
    "当前 Silo · 本次运行",
    "身份与网络证据的 runtime 关联一致；Profile 仅确认配置归属。",
    "good",
    "ok",
  );
}

/** Derives the compact "current session" summary for an active Managed
 * Identity Silo from existing runtime evidence. This is a structured
 * summary, not a verification score: every row restates what the current
 * evidence contract already records, and "当前未发现异常" is not "全部已验证". */
export function deriveCurrentSessionSummary(input: {
  silo: Silo;
  activation: RuntimeActivation;
  managedEngineReady: boolean;
  now?: number;
}): CurrentSessionSummary {
  const now = input.now ?? Date.now();
  const { silo, activation, managedEngineReady } = input;
  const rows = [
    deriveIdentityRow(activation, silo, now),
    deriveEngineRow(activation, managedEngineReady),
    deriveNetworkRow(activation, silo, now),
    deriveAttributionRow(activation, silo),
  ];
  const state = activation.state;
  if (["preflight", "launching"].includes(state)) {
    return {
      overall: "starting",
      overallLabel: "正在启动",
      overallDetail: "正在准备这次运行；各项检查完成后会更新。",
      rows,
    };
  }
  if (
    state === "verification_failed" ||
    state === "failed" ||
    state === "stopped"
  ) {
    return {
      overall: "stopped",
      overallLabel: state === "failed" ? "启动未成功" : "本次运行已结束",
      overallDetail:
        activation.message ??
        (state === "failed"
          ? "浏览器没有打开成功。"
          : state === "stopped"
            ? "浏览器已停止；旧运行证据仅供回看。"
          : state === "verification_failed" && activation.activeSiloId === null
            ? "这次运行的检查未通过，浏览器已关闭；可重新打开。"
            : "这次运行已结束；请点「结束会话」。"),
      rows,
    };
  }
  if (rows.some((entry) => entry.impact === "attention")) {
    return {
      overall: "attention",
      overallLabel: "需要注意",
      overallDetail: "有项目需要处理；详情见下方各行与对应证据区域。",
      rows,
    };
  }
  if (rows.some((entry) => entry.impact !== "ok")) {
    return {
      overall: "partial",
      overallLabel: "部分无法确认",
      overallDetail: "部分能力暂时无法确认；这不代表存在异常。",
      rows,
    };
  }
  return {
    overall: "no_anomaly",
    overallLabel: "当前未发现异常",
    overallDetail: "以上各项均未发现异常；这不代表全部已验证。",
    rows,
  };
}

export function CurrentSessionIntegrity({
  activation,
  managedEngineReady,
  onInspectIdentity,
  onInspectNetwork,
  silo,
}: {
  activation: RuntimeActivation;
  managedEngineReady: boolean;
  onInspectIdentity?: (trigger: HTMLButtonElement) => void;
  onInspectNetwork?: (trigger: HTMLButtonElement) => void;
  silo: Silo;
}) {
  const summary = deriveCurrentSessionSummary({
    activation,
    managedEngineReady,
    silo,
  });
  return (
    <section className={`current-session ${summary.overall}`}>
      <div className="current-session-heading">
        <p className="eyebrow">当前会话完整性</p>
        <strong>{summary.overallLabel}</strong>
        <span>{summary.overallDetail}</span>
      </div>
      <dl className="current-session-grid">
        {summary.rows.map((entry) => (
          <div key={entry.key}>
            <span>{entry.label}</span>
            <strong className={`current-session-state ${entry.tone}`}>
              {entry.stateLabel}
            </strong>
            <small>{entry.detail}</small>
            {entry.key === "identity" && onInspectIdentity ? (
              <button type="button" className="current-session-inspect" onClick={(event) => onInspectIdentity(event.currentTarget)}>
                查看身份字段
              </button>
            ) : null}
            {entry.key === "network" && onInspectNetwork ? (
              <button type="button" className="current-session-inspect" onClick={(event) => onInspectNetwork(event.currentTarget)}>
                查看网络证据
              </button>
            ) : null}
          </div>
        ))}
      </dl>
      <details className="current-session-network-details">
        <summary>网络阶段与来源</summary>
        <NetworkEvidenceDetails activation={activation} silo={silo} />
      </details>
    </section>
  );
}
