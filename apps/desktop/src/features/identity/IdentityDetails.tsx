import {
  type DesktopStatus,
  type ManagedIdentityPreview,
  type SiloNetworkEvidence,
  type WebsiteIdentityObservation,
} from "../../desktop-api.js";

import { type Silo } from "@verisilo/contracts";

import { useState } from "react";

import { formatDate } from "../../shared/presentation.js";
import {
  IdentityEvidenceSignals,
  identityEvidenceContext,
  identityEvidenceLabels,
  safeEvidenceReason,
} from "./evidence-diagnostics.js";
import { deriveNetworkRow, NetworkEvidenceDetails } from "./CurrentSessionIntegrity.js";

export { identityEvidenceLabels } from "./evidence-diagnostics.js";

type ManagedUiState =
  | "configured"
  | "reachable"
  | "applied"
  | "observed"
  | "verified"
  | "unavailable"
  | "not_requested";

function managedUiStateLabel(state: ManagedUiState): string {
  const labels: Record<ManagedUiState, string> = {
    configured: "已配置",
    reachable: "可达",
    applied: "已应用",
    observed: "已观察",
    verified: "已验证",
    unavailable: "不可用",
    not_requested: "未请求",
  };
  return labels[state];
}

export function ManagedStatusGroups({
  activation,
  evidence,
  engineHealthy,
  runtimeState,
  silo,
  now = Date.now(),
}: {
  activation: DesktopStatus["activation"];
  evidence: SiloNetworkEvidence[];
  engineHealthy: boolean;
  runtimeState: DesktopStatus["activation"]["state"];
  silo: Silo;
  now?: number;
}) {
  const runtimeApplied = ["preflight", "launching", "running"].includes(
    runtimeState,
  );
  const latestEvidence = evidence.some((entry) => entry.siloId === silo.id);
  const activeEvidence = activation.activeSiloId === silo.id;
  const engineEvidence = activeEvidence ? activation.engineEvidence : null;
  const artifactConfigured =
    silo.engine.adapter === "camoufox" &&
    silo.engine.artifactBinding !== undefined;
  const hostBindingVerified =
    engineEvidence?.verifiedAdapter === "camoufox" &&
    engineEvidence.hostLaunch === "verified";
  const packageVerified =
    engineEvidence?.packageVerification === "verified" &&
    engineEvidence.packageVerificationDetails !== null;
  const currentIdentity = identityEvidenceContext(activation, silo);
  const networkRow = deriveNetworkRow(activation, silo, now);
  const packageDetails = engineEvidence?.packageVerificationDetails;
  const states: Array<[string, ManagedUiState | "network", string]> = [
    [
      "数据文件夹",
      hostBindingVerified ? "applied" : "configured",
      "登录数据已经单独放好，打开时才会用上。",
    ],
    [
      "身份",
      !artifactConfigured ? "unavailable"
        : currentIdentity.current && currentIdentity.evidence?.state === "matched" ? "observed"
        : hostBindingVerified ? "applied" : "configured",
      !artifactConfigured ? "还没有可用的对外身份。"
        : currentIdentity.current && currentIdentity.evidence?.state === "matched"
          ? "Host 已观察到当前 Artifact 的网站可见身份匹配。"
          : "当前 Artifact 已配置；逐字段结果见身份详情。",
    ],
    [
      "内置浏览器",
      packageVerified
        ? "verified"
        : engineHealthy
          ? runtimeApplied
            ? "applied"
            : "configured"
          : "unavailable",
      packageVerified
        ? `内置浏览器已经检查过${packageDetails?.engineRevision === null || packageDetails?.engineRevision === undefined ? "。" : ` · ${packageDetails.engineRevision}`}`
        : engineHealthy
          ? "内置浏览器可用。"
          : "内置浏览器现在不可用。",
    ],
    [
      "网络",
      "network",
      networkRow.detail,
    ],
    [
      "检查记录",
      currentIdentity.current || latestEvidence
          ? "observed"
          : "not_requested",
      currentIdentity.current
        ? "当前身份有逐字段观察；网络和引擎各有独立证据。"
        : latestEvidence ? "你做过出口检查。" : "还没有检查记录。",
    ],
  ];
  return (
    <details className="managed-status-groups" open>
      <summary>技术细节</summary>
      <div className="managed-status-heading">
        <strong>当前绑定</strong>
        <span>给排查用，日常打开浏览器不用看这里。</span>
      </div>
      <div className="managed-status-grid">
        {states.map(([name, state, detail]) => (
          <div key={name}>
            <span>{name}</span>
            <strong className={`managed-state ${state === "network" ? networkRow.tone : state}`}>
              {state === "network" ? networkRow.stateLabel : managedUiStateLabel(state)}
            </strong>
            <small>{detail}</small>
          </div>
        ))}
      </div>
      <div className="managed-network-details">
        <strong>逐阶段网络证据</strong>
        <NetworkEvidenceDetails activation={activation} silo={silo} now={now} />
      </div>
    </details>
  );
}

export function IdentityInspectPanel({
  activeSiloId,
  activation,
  silos,
}: {
  activeSiloId: string | null;
  activation?: DesktopStatus["activation"];
  identityPreviews?: Record<string, ManagedIdentityPreview>;
  observation?: WebsiteIdentityObservation | null;
  silos: Silo[];
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const silo =
    silos.find((candidate) => candidate.id === selectedId) ??
    silos.find((candidate) => candidate.id === activeSiloId) ??
    silos.find((candidate) => candidate.engine.adapter === "camoufox") ??
    silos[0];

  if (silo === undefined) {
    return (
      <section className="panel identity-inspect-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">检查身份</p>
            <h2>网站会读到什么</h2>
            <p>
              先创建一个独立浏览器，运行后这里会展示 Host 的网站可见身份观察。
            </p>
          </div>
        </div>
      </section>
    );
  }

  const context = activation === undefined ? null : identityEvidenceContext(activation, silo);
  const evidence = context?.evidence ?? null;

  return (
    <section className="panel identity-inspect-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">检查身份</p>
          <h2>网站会读到什么</h2>
          <p>
            期望值与观察值来自正式运行证据；差异判断由 Host 给出，不是网站风控评分。
          </p>
        </div>
        {silos.length > 1 ? (
          <label className="inspect-silo-select">
            查看哪个空间
            <select
              onChange={(event) => setSelectedId(event.target.value)}
              value={silo.id}
            >
              {silos.map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  {candidate.name}
                </option>
              ))}
            </select>
          </label>
        ) : null}
      </div>
      {silo.engine.adapter === "stock" ? (
        <p className="identity-inspect-note">
          Standard Silo 独立保存网站数据；设备身份跟随本机，没有独立身份 Artifact。
        </p>
      ) : evidence === null ? (
        <p className="identity-inspect-note">
          这处还没有正式运行身份观察；运行后可查看 Host 的逐字段结果。
        </p>
      ) : (
        <div>
          <p className="identity-inspect-note">
            身份 {identityEvidenceLabels[evidence.state][0]} · {identityEvidenceLabels[evidence.state][1]}
            {" · "}{context?.scopeLabel}
            {" · 观察于 "}<time dateTime={evidence.observedAt}>{formatDate(evidence.observedAt)}</time>
          </p>
          {context?.note ? <p className="identity-evidence-context">{context.note}</p> : null}
          {safeEvidenceReason(evidence.reason) ? (
            <p className="identity-inspect-note">{safeEvidenceReason(evidence.reason)}</p>
          ) : null}
          <IdentityEvidenceSignals signals={evidence.signals} />
        </div>
      )}
    </section>
  );
}

export function ManagedIdentityFacts({
  preview,
}: {
  preview: ManagedIdentityPreview;
}) {
  return (
    <>
      <div>
        <dt>浏览器标识</dt>
        <dd className="identity-ua">{preview.userAgent}</dd>
      </div>
      <div>
        <dt>屏幕 / CPU</dt>
        <dd>
          {preview.screenWidth}×{preview.screenHeight} ·{" "}
          {preview.hardwareConcurrency} 核
        </dd>
      </div>
      <div>
        <dt>显卡</dt>
        <dd>
          {preview.webglVendor} · {preview.webglRenderer}
        </dd>
      </div>
      {preview.countryCode !== null ? (
        <div>
          <dt>出口地区</dt>
          <dd>
            {preview.countryCode}
            {preview.publicAddress !== null
              ? ` · ${preview.publicAddress}`
              : ""}
          </dd>
        </div>
      ) : null}
    </>
  );
}

export function ManagedIdentityEvidence({
  activation,
  silo,
}: {
  activation: DesktopStatus["activation"];
  silo: Silo;
}) {
  const context = identityEvidenceContext(activation, silo);
  const evidence = context.evidence;
  const state = evidence?.state ?? "unavailable";
  const [stateLabel, stateDescription] = identityEvidenceLabels[state];
  const reason =
    safeEvidenceReason(evidence?.reason) ??
    (evidence === null
      ? "启动这个 Managed Identity Silo 后，Host 才会取得网站观察。"
      : null);

  return (
    <section className={`identity-evidence ${state}`}>
      <div className="identity-evidence-heading">
        <div>
          <p className="eyebrow">Identity evidence</p>
          <strong>
            <span className="identity-evidence-symbol" aria-hidden="true">
              {
                { matched: "≍", mismatched: "≠", unavailable: "∅", stale: "◷" }[
                  state
                ]
              }
            </span>
            Identity {stateLabel}
          </strong>
          <span>{stateDescription}{evidence !== null ? ` · ${context.scopeLabel}` : ""}</span>
        </div>
        {evidence !== null ? (
          <small>
            观察于{" "}
            <time dateTime={evidence.observedAt} title={evidence.observedAt}>
              {formatDate(evidence.observedAt)}
            </time>{" "}
            · Camoufox Host
          </small>
        ) : null}
      </div>
      {context.note ? <p className="identity-evidence-context">{context.note}</p> : null}
      {reason !== null ? (
        <p className="identity-evidence-reason">{reason}</p>
      ) : null}
      {evidence !== null ? <IdentityEvidenceSignals signals={evidence.signals} /> : null}
    </section>
  );
}
