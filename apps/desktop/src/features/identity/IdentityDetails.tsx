import {
  type DesktopStatus,
  type ManagedIdentityPreview,
  type SiloNetworkEvidence,
  type WebsiteIdentityObservation,
} from "../../desktop-api.js";

import { type Silo } from "@verisilo/contracts";

import { useState } from "react";

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
}: {
  activation: DesktopStatus["activation"];
  evidence: SiloNetworkEvidence[];
  engineHealthy: boolean;
  runtimeState: DesktopStatus["activation"]["state"];
  silo: Silo;
}) {
  const runtimeApplied = ["preflight", "launching", "running"].includes(
    runtimeState,
  );
  const latestEvidence = evidence.some((entry) => entry.siloId === silo.id);
  const activeEvidence = activation.activeSiloId === silo.id;
  const engineEvidence = activeEvidence ? activation.engineEvidence : null;
  const networkEvidence = activeEvidence ? activation.networkEvidence : null;
  const artifactConfigured =
    silo.engine.adapter === "camoufox" &&
    silo.engine.artifactBinding !== undefined;
  const hostBindingVerified =
    engineEvidence?.verifiedAdapter === "camoufox" &&
    engineEvidence.hostLaunch === "verified";
  const packageVerified =
    engineEvidence?.packageVerification === "verified" &&
    engineEvidence.packageVerificationDetails !== null;
  const networkState: ManagedUiState =
    networkEvidence === null
      ? "configured"
      : networkEvidence.exit === "observed"
        ? "observed"
        : networkEvidence.browserRouting === "applied"
          ? "applied"
          : networkEvidence.endpoint === "reachable"
            ? "reachable"
            : [
                  networkEvidence.configuration,
                  networkEvidence.endpoint,
                  networkEvidence.browserRouting,
                ].some((state) => state === "failed" || state === "unavailable")
              ? "unavailable"
              : "configured";
  const packageDetails = engineEvidence?.packageVerificationDetails;
  const states: Array<[string, ManagedUiState, string]> = [
    [
      "数据文件夹",
      hostBindingVerified ? "applied" : "configured",
      "登录数据已经单独放好，打开时才会用上。",
    ],
    [
      "身份",
      artifactConfigured
        ? hostBindingVerified
          ? "applied"
          : "configured"
        : "unavailable",
      artifactConfigured
        ? "对外身份已经绑在这个 Silo 上。"
        : "还没有可用的对外身份。",
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
      networkState,
      silo.networkProfile.proxyRequired
        ? "必须走代理；连不上不会改成直连。"
        : "现在是直连。",
    ],
    [
      "检查记录",
      hostBindingVerified && packageVerified
        ? "verified"
        : latestEvidence
          ? "observed"
          : "not_requested",
      hostBindingVerified && packageVerified
        ? "这次打开用的身份、浏览器和网络是对得上的。"
        : latestEvidence
          ? "你做过出口检查。"
          : "还没有检查记录。",
    ],
  ];
  return (
    <details className="managed-status-groups">
      <summary>技术细节</summary>
      <div className="managed-status-heading">
        <strong>当前绑定</strong>
        <span>给排查用，日常打开浏览器不用看这里。</span>
      </div>
      <div className="managed-status-grid">
        {states.map(([name, state, detail]) => (
          <div key={name}>
            <span>{name}</span>
            <strong className={`managed-state ${state}`}>
              {managedUiStateLabel(state)}
            </strong>
            <small>{detail}</small>
          </div>
        ))}
      </div>
    </details>
  );
}

export function IdentityInspectPanel({
  activeSiloId,
  identityPreviews,
  observation,
  silos,
}: {
  activeSiloId: string | null;
  identityPreviews: Record<string, ManagedIdentityPreview>;
  observation: WebsiteIdentityObservation | null | undefined;
  silos: Silo[];
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const silo =
    silos.find((candidate) => candidate.id === selectedId) ??
    silos.find((candidate) => candidate.id === activeSiloId) ??
    silos.find((candidate) => candidate.id === observation?.siloId) ??
    silos.find((candidate) => candidate.engine.adapter === "camoufox") ??
    silos[0];
  const preview = silo === undefined ? undefined : identityPreviews[silo.id];
  const pageRead =
    silo !== undefined && observation?.siloId === silo.id ? observation : null;

  if (silo === undefined) {
    return (
      <section className="panel identity-inspect-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">检查身份</p>
            <h2>网站会读到什么</h2>
            <p>
              先创建一个独立浏览器，打开一次后，这里会对照写入的值和页面读到的值。
            </p>
          </div>
        </div>
      </section>
    );
  }

  const writtenGpu =
    preview === undefined
      ? "—"
      : `${preview.webglVendor} · ${preview.webglRenderer}`;
  const readGpu =
    pageRead === null
      ? "打开后才会出现"
      : pageRead.webglVendor === "未读到" && pageRead.webglRenderer === "未读到"
        ? "这次没读到"
        : `${pageRead.webglVendor} · ${pageRead.webglRenderer}`;
  const rows: Array<{
    label: string;
    written: string;
    read: string;
    pending?: boolean;
    compare?: boolean;
  }> = [
    {
      label: "浏览器标识",
      written: preview?.userAgent ?? "—",
      read: pageRead?.userAgent ?? "打开后才会出现",
      pending: pageRead === null,
    },
    {
      label: "语言",
      written: preview?.language ?? "—",
      read: pageRead?.language ?? "打开后才会出现",
      pending: pageRead === null,
    },
    {
      label: "系统平台",
      written: preview?.platform ?? "—",
      read: pageRead?.platform ?? "打开后才会出现",
      pending: pageRead === null,
    },
    {
      label: "时区",
      written: preview?.timezone ?? "—",
      read: pageRead?.timezone ?? "打开后才会出现",
      pending: pageRead === null,
    },
    {
      label: "屏幕",
      written:
        preview === undefined
          ? "—"
          : `${preview.screenWidth}×${preview.screenHeight}`,
      read:
        pageRead === null
          ? "打开后才会出现"
          : `${pageRead.screenWidth}×${pageRead.screenHeight}`,
      pending: pageRead === null,
    },
    {
      label: "CPU",
      written:
        preview === undefined ? "—" : `${preview.hardwareConcurrency} 核`,
      read:
        pageRead === null
          ? "打开后才会出现"
          : `${pageRead.hardwareConcurrency} 核`,
      pending: pageRead === null,
    },
    {
      label: "显卡",
      written: writtenGpu,
      read: readGpu,
      pending: pageRead === null,
    },
  ];
  if (pageRead?.webdriver !== null && pageRead?.webdriver !== undefined) {
    rows.push({
      label: "自动化标记",
      written: "—",
      read: pageRead.webdriver ? "有" : "没有",
      compare: false,
    });
  }

  return (
    <section className="panel identity-inspect-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">检查身份</p>
          <h2>网站会读到什么</h2>
          <p>
            左边是写入这套浏览器的值。右边是这次打开时，页面脚本实际读到的值。这不是网上那些指纹检测页的打分。
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
          系统浏览器跟这台电脑长得一样，没有另一套可以对照的身份。
        </p>
      ) : (
        <div className="identity-inspect-table">
          <div className="identity-inspect-head">
            <span>项目</span>
            <span>写入的</span>
            <span>页面读到的</span>
          </div>
          <dl>
            {rows.map((row) => {
              const tone =
                row.pending || row.compare === false
                  ? "pending"
                  : inspectValuesMatch(row.written, row.read)
                    ? "match"
                    : "differ";
              return (
                <div className={`identity-inspect-row ${tone}`} key={row.label}>
                  <dt>{row.label}</dt>
                  <dd
                    className={
                      row.label === "浏览器标识" ? "identity-ua" : undefined
                    }
                  >
                    {row.written}
                  </dd>
                  <dd
                    className={
                      row.label === "浏览器标识" ? "identity-ua" : undefined
                    }
                  >
                    {row.read}
                  </dd>
                </div>
              );
            })}
          </dl>
        </div>
      )}
      {pageRead === null && silo.engine.adapter === "camoufox" ? (
        <p className="identity-inspect-note">
          还没有页面读到的结果。打开这个独立浏览器一次后，这里会出现对照。
        </p>
      ) : null}
      {pageRead !== null &&
      rows.some(
        (row) =>
          row.compare !== false &&
          !row.pending &&
          !inspectValuesMatch(row.written, row.read),
      ) ? (
        <p className="identity-inspect-note">
          有几项对不上。这只说明写入的值和这次页面读到的值不同，不等于被网站识破。
        </p>
      ) : null}
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

function inspectValuesMatch(written: string, read: string): boolean {
  return written.trim().toLowerCase() === read.trim().toLowerCase();
}
