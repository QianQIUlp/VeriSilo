import type {
  BrowserKind,
  EngineAdapterId,
  IdentityEvidenceState,
  NativeNetworkEvidenceCoverage,
  NetworkCheckResult,
  RuntimeActivation,
  RuntimeEvidenceState,
  RuntimeIdentityEvidence,
  RuntimeNetworkEvidence,
  Silo,
} from "@verisilo/contracts";

/**
 * v2 adds first-class identity and execution sections to the report. The v1
 * keys keep their names and semantics; v2 is an additive superset, but the
 * report now answers materially more questions, so the version moves. A v1
 * reader must skip unknown keys; there is no stricter consumer contract.
 */
export const LOCAL_REPORT_SCHEMA_VERSION = 2 as const;

/**
 * The subset of a Vault evidence record that is available to the desktop UI.
 * Identifiers deliberately remain outside the report model.
 */
export interface VaultNetworkEvidenceForReport {
  siloId: string;
  receivedAt: string;
  coverage: NativeNetworkEvidenceCoverage;
  result: NetworkCheckResult;
}

export interface LocalSiloReportInput {
  generatedAt: string;
  silo: Silo;
  activation: RuntimeActivation;
  vaultEvidence: readonly VaultNetworkEvidenceForReport[];
}

/**
 * Whether a piece of runtime evidence can be attributed to the selected
 * Silo's report. Evidence that belongs to a different Silo is never
 * re-attributed or partially exported; only the attribution marker is.
 */
export type ReportEvidenceAttribution =
  | "current_for_selected_silo"
  | "last_known_for_selected_silo_not_active"
  | "belongs_to_different_silo"
  | "no_evidence_available";

export interface SanitizedIdentityEvidence {
  attribution: ReportEvidenceAttribution;
  state: IdentityEvidenceState | null;
  observedAt: string | null;
  engineAdapter: EngineAdapterId | null;
  binding: {
    /** An identity artifact binding is carried by the attributable evidence. */
    present: boolean;
    /**
     * The binding belongs to the selected Silo's active runtime and the
     * observation is not stale. Null when no evidence is attributable.
     */
    current: boolean | null;
    /** The evidence belongs to the selected Silo (not another Silo's). */
    selectedSiloAttributionValid: boolean;
  };
  /** Signal names and reconciliation states only; never expected/observed values. */
  signals: Array<{ signal: string; state: IdentityEvidenceState }>;
  signalTotals: {
    matched: number;
    mismatched: number;
    unavailable: number;
    total: number;
  };
}

export interface SanitizedExecutionEvidence {
  /** Engine/browser evidence only exists for the currently active runtime. */
  attribution: ReportEvidenceAttribution;
  engineAdapter: {
    configured: EngineAdapterId;
    launched: EngineAdapterId | null;
    verified: EngineAdapterId | null;
  } | null;
  engineStages: {
    packageVerification: RuntimeEvidenceState;
    bootstrapDelivery: RuntimeEvidenceState;
    hostLaunch: RuntimeEvidenceState;
    runtimeReceipts: RuntimeEvidenceState;
    restoreReceipt: RuntimeEvidenceState;
  } | null;
  browserVerification: {
    state:
      | "verified"
      | "baseline_missing"
      | "version_drift"
      | "missing"
      | "path_changed"
      | "kind_mismatch"
      | "publisher_mismatch"
      | "probe_failed";
    expectedKind: BrowserKind;
    expectedVersion: string | null;
    actualVersion: string | null;
    checkedAt: string;
  } | null;
}

export interface LocalSiloReport {
  schemaVersion: typeof LOCAL_REPORT_SCHEMA_VERSION;
  generatedAt: string;
  product: "VeriSilo";
  summary: string[];
  evidenceBoundary: {
    reportScope: "selected_silo_only";
    trigger: "user_confirmed_local_export";
    publicDoh: "answer_comparison_only";
    actualDnsPath: "not_observed";
    webRtc: "not_observed";
    quic: "not_observed";
    contains: string[];
    doesNotProve: string[];
    excluded: string[];
  };
  silo: {
    name: string;
    browser: {
      kind: BrowserKind | "managed";
      version: string | null;
    };
    lifecycle: "active" | "archived";
    networkConfiguration: {
      mode: Silo["networkProfile"]["mode"];
      proxyRequired: boolean;
      externalControllerBindingConfigured: boolean;
    };
  };
  runtime: {
    state: RuntimeActivation["state"] | "not_active_for_selected_silo";
    observedAt: string | null;
    observationSource:
      "vault_companion_checked_at" | "runtime_evidence_observed_at" | "none";
    networkEvidence: SanitizedRuntimeNetworkEvidence | null;
  };
  identity: SanitizedIdentityEvidence;
  execution: SanitizedExecutionEvidence;
  companionEvidence: SanitizedCompanionEvidence[];
}

export interface SanitizedRuntimeNetworkEvidence {
  provider: RuntimeNetworkEvidence["provider"];
  observedAt: string;
  expiresAt: string | null;
  provenance: RuntimeNetworkEvidence["provenance"];
  authenticationProvenance: RuntimeNetworkEvidence["authenticationProvenance"];
  stages: {
    configuration: Record<
      "configuration" | "controllerBinding" | "endpoint" | "authentication",
      RuntimeEvidenceState
    >;
    application: Record<"browserRouting", RuntimeEvidenceState>;
    verification: Record<"exit" | "dns" | "webRtc", RuntimeEvidenceState>;
  };
}

export interface SanitizedCompanionEvidence {
  checkedAt: string;
  receivedAt: string;
  coverage: {
    trigger: "user_initiated";
    transport: "companion_extension_fetch";
    ip: "third_party_https_observation";
    publicDoh: "public_doh_answer_comparison";
    actualDnsPath: "not_observed";
    webRtc: "not_observed";
    quic: "not_observed";
  };
  exit: {
    state: "observed" | "not_observed";
    addressPrefix: string | null;
    version: "IPv4" | "IPv6" | "unknown" | null;
    countryCode: string | null;
    asn: string | null;
    networkHint: "cloud_or_hosting" | "unknown" | null;
  };
  publicDoh: {
    state: NetworkCheckResult["dns"]["state"];
    dnssec: NetworkCheckResult["dns"]["dnssec"];
    providers: Array<{
      provider: "Cloudflare" | "Google";
      status: number;
      dnssecAuthenticated: boolean;
    }>;
  };
  reputation: "not_scored";
}

const EXCLUDED_FIELDS = [
  "本机 Profile 路径",
  "浏览器可执行文件路径",
  "请求标识符",
  "代理主机和端口",
  "完整 IP 地址",
  "城市和区域位置",
  "原始错误",
  "秘密、凭据、种子和引用标识符",
  "身份信号期望值与观察值",
  "身份 Artifact 标识、Artifact 摘要、运行时与会话标识符",
  "引擎能力诊断文本与站点回退记录",
] as const;

const REPORT_CONTAINS = [
  "所选 Silo 的脱敏配置与生命周期状态",
  "可归属到所选 Silo 的运行时身份证据结论（仅信号名称与对账状态）",
  "当前运行时的引擎适配器、引擎阶段与浏览器验证状态",
  "运行时网络证据与所选 Silo 的 Companion 观测声明",
] as const;

const REPORT_DOES_NOT_PROVE = [
  "身份不可检测或能绕过反欺诈系统",
  "与所有网站兼容",
  "TLS ClientHello 已验证",
  "QUIC 已完整验证",
  "公共 DoH 答案对比等于实际 DNS 路径",
  "出口 IP 观察等于本机浏览器进程级验证",
  "WebRTC 与 QUIC 不可用等于不存在泄露（仅代表未观测）",
  "fontMode=inherit 时字体隔离已对账",
] as const;

/**
 * Builds a deliberately narrow, serializable report. Do not spread source
 * objects into this model: source records contain local paths, identifiers,
 * endpoints, error text, raw identity signal values, and engine diagnostic
 * text that must never be exported by default. Runtime evidence is only
 * attributed to the selected Silo's own active runtime; other Silos'
 * evidence is never borrowed.
 */
export function buildLocalSiloReport(
  input: LocalSiloReportInput,
): LocalSiloReport {
  const selectedIsActive = input.activation.activeSiloId === input.silo.id;
  const runtimeEvidence = selectedIsActive
    ? sanitizeRuntimeEvidence(input.activation.networkEvidence)
    : null;
  const companionEvidence = input.vaultEvidence
    .filter((entry) => entry.siloId === input.silo.id)
    .map(sanitizeCompanionEvidence)
    .sort(compareCompanionEvidence);
  const latestCompanionEvidence = companionEvidence.at(-1);
  const runtimeObservedAt = runtimeEvidence?.observedAt ?? null;
  const identity = buildIdentitySection(
    input.activation.identityEvidence,
    input.silo.id,
    selectedIsActive,
  );
  const execution = buildExecutionSection(input.activation, input.silo.id);

  return {
    schemaVersion: LOCAL_REPORT_SCHEMA_VERSION,
    generatedAt: input.generatedAt,
    product: "VeriSilo",
    summary: buildSummary(
      input.silo,
      selectedIsActive,
      companionEvidence,
      identity,
      execution,
      runtimeEvidence,
    ),
    evidenceBoundary: {
      reportScope: "selected_silo_only",
      trigger: "user_confirmed_local_export",
      publicDoh: "answer_comparison_only",
      actualDnsPath: "not_observed",
      webRtc: "not_observed",
      quic: "not_observed",
      contains: [...REPORT_CONTAINS],
      doesNotProve: [...REPORT_DOES_NOT_PROVE],
      excluded: [...EXCLUDED_FIELDS],
    },
    silo: {
      name: input.silo.name,
      browser: {
        kind: input.silo.browser?.kind ?? "managed",
        version: input.silo.browser?.version ?? null,
      },
      lifecycle: input.silo.archivedAt === null ? "active" : "archived",
      networkConfiguration: {
        mode: input.silo.networkProfile.mode,
        proxyRequired: input.silo.networkProfile.proxyRequired,
        externalControllerBindingConfigured:
          input.silo.networkProfile.mode === "fixed_proxy" &&
          input.silo.networkProfile.externalMihomo !== undefined,
      },
    },
    runtime: {
      state: selectedIsActive
        ? input.activation.state
        : "not_active_for_selected_silo",
      observedAt:
        latestCompanionEvidence?.checkedAt ?? runtimeObservedAt ?? null,
      observationSource:
        latestCompanionEvidence !== undefined
          ? "vault_companion_checked_at"
          : runtimeObservedAt !== null
            ? "runtime_evidence_observed_at"
            : "none",
      networkEvidence: runtimeEvidence,
    },
    identity,
    execution,
    companionEvidence,
  };
}

export function serializeLocalSiloReport(report: LocalSiloReport): string {
  return `${JSON.stringify(report, null, 2)}\n`;
}

/** A self-contained document: no script tags, network URLs, or unescaped data. */
export function renderLocalSiloReportHtml(report: LocalSiloReport): string {
  const runtime = report.runtime.networkEvidence;
  const evidenceRows = report.companionEvidence
    .map(
      (entry) => `<tr>
        <td>${escapeHtml(entry.checkedAt)}</td>
        <td>${escapeHtml(entry.exit.state)}</td>
        <td>${escapeHtml(entry.exit.addressPrefix ?? "not observed")}</td>
        <td>${escapeHtml(entry.publicDoh.state)}</td>
        <td>${escapeHtml(entry.publicDoh.dnssec)}</td>
      </tr>`,
    )
    .join("");
  const runtimeStages =
    runtime === null
      ? "所选 Silo 当前没有可归属的运行时网络证据。"
      : [
          renderRuntimeStage("配置", runtime.stages.configuration),
          renderRuntimeStage("应用", runtime.stages.application),
          renderRuntimeStage("验证", runtime.stages.verification),
        ].join("");
  const identity = report.identity;
  const identitySignalRows = identity.signals
    .map(
      (signal) =>
        `<tr><td><code>${escapeHtml(signal.signal)}</code></td><td><code>${escapeHtml(signal.state)}</code></td></tr>`,
    )
    .join("");
  const identityAttribution = {
    current_for_selected_silo: "证据属于当前正在运行的所选 Silo",
    last_known_for_selected_silo_not_active:
      "证据属于所选 Silo，但当前没有活跃的运行时证据，不作为当前运行结论",
    belongs_to_different_silo:
      "所选 Silo 当前无可归属的运行时身份证据；现有运行时证据属于其他 Silo，未纳入本报告",
    no_evidence_available: "所选 Silo 当前没有可归属的运行时身份证据",
  }[identity.attribution];
  const identitySection = `<section><h2>身份证据</h2><p>归属：<code>${escapeHtml(identity.attribution)}</code>（${escapeHtml(identityAttribution)}）；状态：<code>${identity.state === null ? "无" : escapeHtml(identity.state)}</code>；观察时间：<code>${identity.observedAt === null ? "无" : escapeHtml(identity.observedAt)}</code>；引擎适配器：<code>${identity.engineAdapter === null ? "无" : escapeHtml(identity.engineAdapter)}</code></p><p>Artifact 绑定：存在 ${identity.binding.present ? "是" : "否"}；当前有效 ${identity.binding.current === null ? "未知" : identity.binding.current ? "是" : "否"}；所选 Silo 归属有效：${identity.binding.selectedSiloAttributionValid ? "是" : "否"}。</p>${
    identity.signals.length === 0
      ? "<p>没有可导出的身份信号对账结果。</p>"
      : `<p>对账计数：matched ${identity.signalTotals.matched} · mismatched ${identity.signalTotals.mismatched} · unavailable ${identity.signalTotals.unavailable}（共 ${identity.signalTotals.total} 项）。身份信号仅导出字段名称与对账状态，不导出期望值与观察值。</p><table><thead><tr><th>网站可见字段</th><th>对账状态</th></tr></thead><tbody>${identitySignalRows}</tbody></table>`
  }</section>`;
  const execution = report.execution;
  const executionParts: string[] = [];
  if (execution.engineAdapter !== null) {
    executionParts.push(
      `<p>引擎适配器：配置 <code>${escapeHtml(execution.engineAdapter.configured)}</code>；启动 <code>${execution.engineAdapter.launched === null ? "无" : escapeHtml(execution.engineAdapter.launched)}</code>；验证 <code>${execution.engineAdapter.verified === null ? "未取得" : escapeHtml(execution.engineAdapter.verified)}</code>。配置、启动与验证是不同状态。</p>`,
    );
  }
  if (execution.engineStages !== null) {
    executionParts.push(
      `<p>引擎阶段：包验证 <code>${escapeHtml(execution.engineStages.packageVerification)}</code>；引导交付 <code>${escapeHtml(execution.engineStages.bootstrapDelivery)}</code>；Host 启动 <code>${escapeHtml(execution.engineStages.hostLaunch)}</code>；运行时回执 <code>${escapeHtml(execution.engineStages.runtimeReceipts)}</code>；恢复回执 <code>${escapeHtml(execution.engineStages.restoreReceipt)}</code>。</p>`,
    );
  }
  if (execution.browserVerification !== null) {
    executionParts.push(
      `<p>浏览器验证：<code>${escapeHtml(execution.browserVerification.state)}</code>（检查于 <code>${escapeHtml(execution.browserVerification.checkedAt)}</code>）；期望类型 <code>${escapeHtml(execution.browserVerification.expectedKind)}</code>${execution.browserVerification.expectedVersion === null ? "" : `，期望版本 <code>${escapeHtml(execution.browserVerification.expectedVersion)}</code>`}${execution.browserVerification.actualVersion === null ? "" : `，实际版本 <code>${escapeHtml(execution.browserVerification.actualVersion)}</code>`}。可执行文件路径不在本报告中。</p>`,
    );
  }
  const executionSection = `<section><h2>执行与引擎</h2><p>归属：<code>${escapeHtml(execution.attribution)}</code>。</p>${
    executionParts.length === 0
      ? "<p>当前没有可归属的活跃运行时执行证据。</p>"
      : executionParts.join("")
  }</section>`;

  return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
  <title>VeriSilo 本地 Silo 脱敏报告</title>
  <style>body{max-width:920px;margin:32px auto;padding:0 20px;color:#172036;background:#f8f9fc;font:15px/1.55 system-ui,sans-serif}section{margin:18px 0;border:1px solid #e3e8f2;border-radius:12px;padding:18px;background:#fff}h1,h2,h3{margin-top:0}table{width:100%;border-collapse:collapse}th,td{padding:8px;border-bottom:1px solid #e3e8f2;text-align:left;overflow-wrap:anywhere}small{color:#667085}code{overflow-wrap:anywhere}</style>
</head>
<body>
  <header><h1>VeriSilo 本地 Silo 脱敏报告</h1><p>生成时间：<code>${escapeHtml(report.generatedAt)}</code> <small>（数据结构版本 ${report.schemaVersion}）</small></p></header>
  <section><h2>所选 Silo</h2><dl><dt>名称</dt><dd>${escapeHtml(report.silo.name)}</dd><dt>浏览器</dt><dd>${escapeHtml(report.silo.browser.kind)}${report.silo.browser.version === null ? "" : ` ${escapeHtml(report.silo.browser.version)}`}</dd><dt>生命周期</dt><dd>${escapeHtml(report.silo.lifecycle)}</dd><dt>网络配置</dt><dd>${escapeHtml(report.silo.networkConfiguration.mode)}；必须代理：${report.silo.networkConfiguration.proxyRequired ? "是" : "否"}；已配置外部控制器：${report.silo.networkConfiguration.externalControllerBindingConfigured ? "是" : "否"}</dd></dl></section>
  <section><h2>摘要</h2><ul>${report.summary.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>
  ${identitySection}
  ${executionSection}
  <section><h2>运行时证据</h2><p>状态：<code>${escapeHtml(report.runtime.state)}</code>；观测时间：<code>${report.runtime.observedAt === null ? "无独立证据" : escapeHtml(report.runtime.observedAt)}</code>；来源：<code>${escapeHtml(report.runtime.observationSource)}</code></p>${runtime === null ? "" : `<p>总体来源：<code>${escapeHtml(runtime.provenance)}</code>；代理认证来源：<code>${escapeHtml(runtime.authenticationProvenance)}</code>。<code>extension_asserted</code> 与 <code>relay_observed</code> 是联合的本机观测，不代表独立可信的浏览器进程证明。</p>`}<ul>${runtimeStages}</ul></section>
  <section><h2>Companion 证据（${report.companionEvidence.length}）</h2><p>公共 DoH 仅用于答案对比；实际 DNS 路径、WebRTC 和 QUIC 均未在本报告中观测。</p><table><thead><tr><th>检查时间</th><th>出口</th><th>地址前缀</th><th>公共 DoH</th><th>DNSSEC</th></tr></thead><tbody>${evidenceRows || '<tr><td colspan="5">这个 Silo 尚无 Companion 证据。</td></tr>'}</tbody></table></section>
  <section><h2>导出边界</h2><p>范围仅限所选 Silo；触发方式是用户确认的本地导出。</p><h3>本报告包含</h3><ul>${report.evidenceBoundary.contains.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul><h3>本报告不证明</h3><ul>${report.evidenceBoundary.doesNotProve.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul><h3>默认排除</h3><ul>${report.evidenceBoundary.excluded.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>
</body>
</html>`;
}

function buildIdentitySection(
  evidence: RuntimeIdentityEvidence | null,
  selectedSiloId: string,
  selectedIsActive: boolean,
): SanitizedIdentityEvidence {
  if (evidence === null || evidence.siloId !== selectedSiloId) {
    return {
      attribution:
        evidence === null
          ? "no_evidence_available"
          : "belongs_to_different_silo",
      state: null,
      observedAt: null,
      engineAdapter: null,
      binding: {
        present: false,
        current: null,
        selectedSiloAttributionValid: false,
      },
      signals: [],
      signalTotals: { matched: 0, mismatched: 0, unavailable: 0, total: 0 },
    };
  }
  return {
    attribution: selectedIsActive
      ? "current_for_selected_silo"
      : "last_known_for_selected_silo_not_active",
    state: evidence.state,
    observedAt: evidence.observedAt,
    engineAdapter: evidence.engineAdapter,
    binding: {
      present: true,
      current: selectedIsActive && evidence.state !== "stale",
      selectedSiloAttributionValid: true,
    },
    signals: evidence.signals.map((signal) => ({
      signal: signal.signal,
      state: signal.state,
    })),
    signalTotals: {
      matched: evidence.signals.filter((s) => s.state === "matched").length,
      mismatched: evidence.signals.filter((s) => s.state === "mismatched")
        .length,
      unavailable: evidence.signals.filter((s) => s.state === "unavailable")
        .length,
      total: evidence.signals.length,
    },
  };
}

function buildExecutionSection(
  activation: RuntimeActivation,
  selectedSiloId: string,
): SanitizedExecutionEvidence {
  if (activation.activeSiloId !== selectedSiloId) {
    return {
      attribution:
        activation.activeSiloId === null
          ? "no_evidence_available"
          : "belongs_to_different_silo",
      engineAdapter: null,
      engineStages: null,
      browserVerification: null,
    };
  }
  const engineEvidence = activation.engineEvidence;
  const browserVerification = activation.browserVerification ?? null;
  return {
    attribution: "current_for_selected_silo",
    engineAdapter:
      engineEvidence === null
        ? null
        : {
            configured: engineEvidence.configuredAdapter,
            launched: engineEvidence.launchedAdapter,
            verified: engineEvidence.verifiedAdapter,
          },
    engineStages:
      engineEvidence === null
        ? null
        : {
            packageVerification: engineEvidence.packageVerification,
            bootstrapDelivery: engineEvidence.bootstrapDelivery,
            hostLaunch: engineEvidence.hostLaunch,
            runtimeReceipts: engineEvidence.runtimeReceipts,
            restoreReceipt: engineEvidence.restoreReceipt,
          },
    browserVerification:
      browserVerification === null
        ? null
        : {
            state: browserVerification.state,
            expectedKind: browserVerification.expectedKind,
            expectedVersion: browserVerification.expectedVersion,
            actualVersion: browserVerification.actualVersion,
            checkedAt: browserVerification.checkedAt,
          },
  };
}

function sanitizeRuntimeEvidence(
  evidence: RuntimeNetworkEvidence | null,
): SanitizedRuntimeNetworkEvidence | null {
  if (evidence === null) {
    return null;
  }
  return {
    provider: evidence.provider,
    observedAt: evidence.observedAt,
    expiresAt: evidence.expiresAt,
    provenance: evidence.provenance,
    authenticationProvenance: evidence.authenticationProvenance,
    stages: {
      configuration: {
        configuration: evidence.configuration,
        controllerBinding: evidence.controllerBinding,
        endpoint: evidence.endpoint,
        authentication: evidence.authentication,
      },
      application: { browserRouting: evidence.browserRouting },
      verification: {
        exit: evidence.exit,
        dns: evidence.dns,
        webRtc: evidence.webRtc,
      },
    },
  };
}

function sanitizeCompanionEvidence(
  entry: VaultNetworkEvidenceForReport,
): SanitizedCompanionEvidence {
  const ip = entry.result.ip;
  return {
    checkedAt: entry.result.checkedAt,
    receivedAt: entry.receivedAt,
    coverage: {
      trigger: entry.coverage.trigger,
      transport: entry.coverage.transport,
      ip: entry.coverage.ip,
      publicDoh: entry.coverage.publicDns,
      actualDnsPath: entry.coverage.actualDnsPath,
      webRtc: entry.coverage.webRtc,
      quic: entry.coverage.quic,
    },
    exit:
      ip === null
        ? {
            state: "not_observed",
            addressPrefix: null,
            version: null,
            countryCode: null,
            asn: null,
            networkHint: null,
          }
        : {
            state: "observed",
            addressPrefix: redactIpAddress(ip.address, ip.version),
            version: ip.version,
            countryCode: ip.countryCode,
            asn: ip.asn,
            networkHint: ip.networkHint,
          },
    publicDoh: {
      state: entry.result.dns.state,
      dnssec: entry.result.dns.dnssec,
      providers: entry.result.dns.providers.map((provider) => ({
        provider: provider.provider,
        status: provider.status,
        dnssecAuthenticated: provider.dnssecAuthenticated,
      })),
    },
    reputation: "not_scored",
  };
}

function redactIpAddress(
  address: string,
  version: "IPv4" | "IPv6" | "unknown",
): string {
  if (version === "IPv4") {
    const parts = address.split(".");
    if (
      parts.length === 4 &&
      parts.every((part) => /^\d{1,3}$/u.test(part) && Number(part) <= 255)
    ) {
      return `${parts.slice(0, 3).join(".")}.0/24`;
    }
  }
  if (version === "IPv6") {
    const expanded = expandIpv6(address);
    if (expanded !== null) {
      return `${expanded.slice(0, 3).join(":")}::/48`;
    }
  }
  return "redacted";
}

function expandIpv6(address: string): string[] | null {
  const normalized = address.toLowerCase();
  if (!/^[0-9a-f:]+$/u.test(normalized) || normalized.includes(":::")) {
    return null;
  }
  const [left = "", right] = normalized.split("::");
  if (normalized.split("::").length > 2) {
    return null;
  }
  const head = left === "" ? [] : left.split(":");
  const tail = right === undefined || right === "" ? [] : right.split(":");
  if (
    head.length + tail.length > 8 ||
    [...head, ...tail].some((part) => !/^[0-9a-f]{1,4}$/u.test(part))
  ) {
    return null;
  }
  const zeroes = normalized.includes("::") ? 8 - head.length - tail.length : 0;
  const groups = [...head, ...Array<string>(zeroes).fill("0"), ...tail];
  return groups.length === 8
    ? groups.map((part) => part.padStart(4, "0"))
    : null;
}

function compareCompanionEvidence(
  left: SanitizedCompanionEvidence,
  right: SanitizedCompanionEvidence,
): number {
  return (
    left.checkedAt.localeCompare(right.checkedAt) ||
    left.receivedAt.localeCompare(right.receivedAt) ||
    (left.exit.addressPrefix ?? "").localeCompare(
      right.exit.addressPrefix ?? "",
    )
  );
}

function buildSummary(
  silo: Silo,
  selectedIsActive: boolean,
  evidence: SanitizedCompanionEvidence[],
  identity: SanitizedIdentityEvidence,
  execution: SanitizedExecutionEvidence,
  runtimeEvidence: SanitizedRuntimeNetworkEvidence | null,
): string[] {
  const browser =
    silo.browser === null
      ? "托管身份浏览器"
      : silo.browser.kind === "chrome"
        ? "Chrome"
        : "Edge";
  const lines = [
    `${browser} Silo 配置已脱敏，不包含本机路径或代理端点。`,
    identitySummaryLine(identity),
    selectedIsActive
      ? "运行时证据属于当前正在运行的所选 Silo。"
      : "所选 Silo 当前未运行，因此不会把其他环境的运行时证据归给它。",
    executionSummaryLine(execution),
    `报告包含 ${evidence.length} 条由用户主动触发、保存在本地 Vault 的 Companion 扩展观测声明；Native inbox 未做本机进程级认证。`,
    "已配置、已应用和已验证是不同状态；缺少观测时不会按验证成功处理。",
  ];
  const unconfirmed = unconfirmedSummaryLine(identity, runtimeEvidence);
  if (unconfirmed !== null) {
    lines.push(unconfirmed);
  }
  return lines;
}

function identitySummaryLine(identity: SanitizedIdentityEvidence): string {
  const observedAt = identity.observedAt ?? "时间未知";
  switch (identity.attribution) {
    case "current_for_selected_silo":
      switch (identity.state) {
        case "matched":
          return `网站可见身份与当前声明一致（matched），观察于 ${observedAt}；证据属于当前正在运行的所选 Silo。`;
        case "mismatched":
          return `身份对账发现不一致（mismatched），观察于 ${observedAt}；这只说明写入的值和这次页面读到的值不同，不等于被网站识破。`;
        case "stale":
          return `身份观察已过期（stale），观察于 ${observedAt}：它不再属于当前 Silo 的 Artifact 或活动 runtime。`;
        default:
          return `没有取得可比较的网站身份观察（unavailable），观察尝试于 ${observedAt}。`;
      }
    case "last_known_for_selected_silo_not_active":
      return `身份证据属于所选 Silo（${identity.state ?? "unknown"}，观察于 ${observedAt}），但当前没有活跃的运行时证据，不作为当前运行结论。`;
    case "belongs_to_different_silo":
      return "所选 Silo 当前无可归属的运行时身份证据；现有运行时证据属于其他 Silo，未纳入本报告。";
    default:
      return "所选 Silo 当前没有可归属的运行时身份证据。";
  }
}

function executionSummaryLine(execution: SanitizedExecutionEvidence): string {
  if (execution.attribution !== "current_for_selected_silo") {
    return "所选 Silo 当前没有可归属的活跃运行时执行证据。";
  }
  const parts: string[] = [];
  if (execution.engineAdapter !== null) {
    const { configured, launched, verified } = execution.engineAdapter;
    parts.push(
      `引擎适配器配置为 ${configured}${launched === null ? "" : `、启动为 ${launched}`}${verified === null ? "，尚未取得适配器验证" : `、验证为 ${verified}`}`,
    );
  }
  if (execution.engineStages !== null) {
    parts.push(
      `包验证 ${execution.engineStages.packageVerification}、Host 启动 ${execution.engineStages.hostLaunch}`,
    );
  }
  if (execution.browserVerification !== null) {
    parts.push(
      `浏览器验证状态 ${execution.browserVerification.state}（检查于 ${execution.browserVerification.checkedAt}）`,
    );
  }
  if (parts.length === 0) {
    return "当前运行时没有可导出的引擎或浏览器验证证据。";
  }
  return `执行证据：${parts.join("；")}。`;
}

function unconfirmedSummaryLine(
  identity: SanitizedIdentityEvidence,
  runtimeEvidence: SanitizedRuntimeNetworkEvidence | null,
): string | null {
  const items: string[] = [];
  const mismatched = identity.signals
    .filter((signal) => signal.state === "mismatched")
    .map((signal) => signal.signal);
  const unavailable = identity.signals
    .filter((signal) => signal.state === "unavailable")
    .map((signal) => signal.signal);
  if (mismatched.length > 0) {
    items.push(`身份不一致项：${mismatched.join("、")}`);
  }
  if (unavailable.length > 0) {
    items.push(`身份未确认项：${unavailable.join("、")}`);
  }
  if (runtimeEvidence?.stages.verification.dns === "unavailable") {
    items.push("运行时 DNS 验证未取得");
  }
  if (items.length === 0) {
    return null;
  }
  return `本次未确认：${items.join("；")}。`;
}

function renderRuntimeStage(
  label: string,
  stages: Record<string, RuntimeEvidenceState>,
): string {
  return `<li><strong>${escapeHtml(label)}</strong>: ${escapeHtml(
    Object.entries(stages)
      .map(([stage, state]) => `${stage}=${state}`)
      .join(", "),
  )}</li>`;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/gu, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "'": "&#39;",
      '"': "&quot;",
    };
    return entities[character] ?? character;
  });
}
