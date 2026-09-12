import { type DesktopStatus } from "../../desktop-api.js";

import { type Silo } from "@verisilo/contracts";

import { formatDate } from "../../shared/presentation.js";

type RuntimeActivation = DesktopStatus["activation"];
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

const FRESH_OBSERVATION_WINDOW_MS = 120_000;

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
  if (age >= 0 && age < FRESH_OBSERVATION_WINDOW_MS) {
    return "刚刚重新读取";
  }
  if (age >= 0 && age < 3_600_000) {
    return `${Math.max(1, Math.floor(age / 60_000))} 分钟前读取`;
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
  const identity =
    activation.identityEvidence !== null &&
    activation.identityEvidence.siloId === silo.id
      ? activation.identityEvidence
      : null;
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
  const runtimeMismatch =
    activation.networkEvidence !== null &&
    activation.networkEvidence.runtimeId !== identity.runtimeId;
  if (runtimeMismatch) {
    return row(
      "identity",
      "身份",
      "Stale",
      "这份证据来自另一次运行，不能代表当前会话；可点「重新检查」。",
      "warn",
      "unconfirmed",
    );
  }
  switch (identity.state) {
    case "matched":
      return row(
        "identity",
        "身份",
        "Matched",
        `网站可见身份与声明一致 · ${formatObservedFreshness(identity.observedAt, now)}`,
        "good",
        "ok",
      );
    case "mismatched":
      return row(
        "identity",
        "身份",
        "Mismatched",
        identity.reason ?? "当前观测与声明身份不一致；可点「重新检查」。",
        "warn",
        "attention",
      );
    case "unavailable":
      return row(
        "identity",
        "身份",
        "Unavailable",
        identity.reason ?? "这次无法取得网站可见身份；可点「重新检查」。",
        "danger",
        "unconfirmed",
      );
    case "stale":
      return row(
        "identity",
        "身份",
        "Stale",
        identity.reason ?? "这份证据不再属于当前 Artifact 或活动 runtime。",
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
      activation.message ?? "这次运行已结束；请点「结束会话」后再打开浏览器。",
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

function deriveNetworkRow(
  activation: RuntimeActivation,
  silo: Silo,
  now: number,
): CurrentSessionRow {
  const network = activation.networkEvidence;
  const proxyRequired = silo.networkProfile.proxyRequired;
  const starting = ["preflight", "launching"].includes(activation.state);
  const running = activation.state === "running";
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
    return row(
      "network",
      "网络",
      "直连",
      "当前策略不要求代理出口。",
      "neutral",
      "ok",
    );
  }
  const expiry =
    network.expiresAt === null ? null : formatExpiry(network.expiresAt, now);
  if (expiry === "最近证据已过期") {
    return row(
      "network",
      "网络",
      "无法确认当前出口",
      proxyRequired
        ? "当前策略要求代理出口；最近证据已过期，可点「重新检查」。"
        : "最近证据已过期；可点「重新检查」。",
      "warn",
      proxyRequired ? "attention" : "unconfirmed",
    );
  }
  if (network.exit === "failed" || network.exit === "unavailable") {
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
    return row(
      "network",
      "网络",
      proxyRequired ? "代理出口已观测" : "直连已观测",
      `${endpoint} · ${expiry ?? formatObservedFreshness(network.observedAt, now)} · 出口观察由${networkProvenanceLabels[network.provenance]}提供`,
      "good",
      "ok",
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

function deriveAttributionRow(
  activation: RuntimeActivation,
  silo: Silo,
): CurrentSessionRow {
  const identity =
    activation.identityEvidence !== null &&
    activation.identityEvidence.siloId === silo.id
      ? activation.identityEvidence
      : null;
  const network = activation.networkEvidence;
  if (identity === null && network === null) {
    return row(
      "attribution",
      "运行归属",
      "启动后确认",
      "打开后这里会确认证据属于当前运行。",
      "neutral",
      "unconfirmed",
    );
  }
  const runtimeMismatch =
    identity !== null &&
    network !== null &&
    network.runtimeId !== identity.runtimeId;
  if (runtimeMismatch) {
    return row(
      "attribution",
      "运行归属",
      "证据不属于当前运行",
      "这份证据来自另一次运行；不能代表当前会话。",
      "warn",
      "attention",
    );
  }
  return row(
    "attribution",
    "运行归属",
    "当前 Silo · 本次运行",
    "证据与当前运行的 Silo 绑定一致。",
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
  silo,
}: {
  activation: RuntimeActivation;
  managedEngineReady: boolean;
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
          </div>
        ))}
      </dl>
    </section>
  );
}
