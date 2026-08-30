import type {
  BrowserKind,
  NativeNetworkEvidenceCoverage,
  NetworkCheckResult,
  RuntimeActivation,
  RuntimeEvidenceState,
  RuntimeNetworkEvidence,
  Silo,
} from "@verisilo/contracts";

export const LOCAL_REPORT_SCHEMA_VERSION = 1 as const;

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
] as const;

/**
 * Builds a deliberately narrow, serializable report. Do not spread source
 * objects into this model: source records contain local paths, identifiers,
 * endpoints, and error text that must never be exported by default.
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

  return {
    schemaVersion: LOCAL_REPORT_SCHEMA_VERSION,
    generatedAt: input.generatedAt,
    product: "VeriSilo",
    summary: buildSummary(input.silo, selectedIsActive, companionEvidence),
    evidenceBoundary: {
      reportScope: "selected_silo_only",
      trigger: "user_confirmed_local_export",
      publicDoh: "answer_comparison_only",
      actualDnsPath: "not_observed",
      webRtc: "not_observed",
      quic: "not_observed",
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

  return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
  <title>VeriSilo 本地 Silo 脱敏报告</title>
  <style>body{max-width:920px;margin:32px auto;padding:0 20px;color:#172036;background:#f8f9fc;font:15px/1.55 system-ui,sans-serif}section{margin:18px 0;border:1px solid #e3e8f2;border-radius:12px;padding:18px;background:#fff}h1,h2{margin-top:0}table{width:100%;border-collapse:collapse}th,td{padding:8px;border-bottom:1px solid #e3e8f2;text-align:left;overflow-wrap:anywhere}small{color:#667085}code{overflow-wrap:anywhere}</style>
</head>
<body>
  <header><h1>VeriSilo 本地 Silo 脱敏报告</h1><p>生成时间：<code>${escapeHtml(report.generatedAt)}</code></p></header>
  <section><h2>所选 Silo</h2><dl><dt>名称</dt><dd>${escapeHtml(report.silo.name)}</dd><dt>浏览器</dt><dd>${escapeHtml(report.silo.browser.kind)}${report.silo.browser.version === null ? "" : ` ${escapeHtml(report.silo.browser.version)}`}</dd><dt>生命周期</dt><dd>${escapeHtml(report.silo.lifecycle)}</dd><dt>网络配置</dt><dd>${escapeHtml(report.silo.networkConfiguration.mode)}；必须代理：${report.silo.networkConfiguration.proxyRequired ? "是" : "否"}；已配置外部控制器：${report.silo.networkConfiguration.externalControllerBindingConfigured ? "是" : "否"}</dd></dl></section>
  <section><h2>摘要</h2><ul>${report.summary.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>
  <section><h2>运行时证据</h2><p>状态：<code>${escapeHtml(report.runtime.state)}</code>；观测时间：<code>${report.runtime.observedAt === null ? "无独立证据" : escapeHtml(report.runtime.observedAt)}</code>；来源：<code>${escapeHtml(report.runtime.observationSource)}</code></p>${runtime === null ? "" : `<p>总体来源：<code>${escapeHtml(runtime.provenance)}</code>；代理认证来源：<code>${escapeHtml(runtime.authenticationProvenance)}</code>。<code>extension_asserted</code> 与 <code>relay_observed</code> 是联合的本机观测，不代表独立可信的浏览器进程证明。</p>`}<ul>${runtimeStages}</ul></section>
  <section><h2>Companion 证据（${report.companionEvidence.length}）</h2><p>公共 DoH 仅用于答案对比；实际 DNS 路径、WebRTC 和 QUIC 均未在本报告中观测。</p><table><thead><tr><th>检查时间</th><th>出口</th><th>地址前缀</th><th>公共 DoH</th><th>DNSSEC</th></tr></thead><tbody>${evidenceRows || '<tr><td colspan="5">这个 Silo 尚无 Companion 证据。</td></tr>'}</tbody></table></section>
  <section><h2>导出边界</h2><p>范围仅限所选 Silo。默认排除：${report.evidenceBoundary.excluded.map(escapeHtml).join("、")}。</p></section>
</body>
</html>`;
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
): string[] {
  const browser =
    silo.browser === null
      ? "托管身份浏览器"
      : silo.browser.kind === "chrome"
        ? "Chrome"
        : "Edge";
  const lines = [
    `${browser} Silo 配置已脱敏，不包含本机路径或代理端点。`,
    selectedIsActive
      ? "运行时证据属于当前正在运行的所选 Silo。"
      : "所选 Silo 当前未运行，因此不会把其他环境的运行时证据归给它。",
    `报告包含 ${evidence.length} 条由用户主动触发、保存在本地 Vault 的 Companion 扩展观测声明；Native inbox 未做本机进程级认证。`,
    "已配置、已应用和已验证是不同状态；缺少观测时不会按验证成功处理。",
  ];
  return lines;
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
