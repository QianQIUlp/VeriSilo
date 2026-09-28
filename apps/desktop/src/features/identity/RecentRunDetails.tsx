import type { RecentRunRecord, Silo } from "@verisilo/contracts";

import { IdentityEvidenceSignals } from "./evidence-diagnostics.js";
import "./RecentRunDetails.css";

export interface RecentRunDetailsProps {
  record: RecentRunRecord | null;
  silo: Silo;
}

const stateLabels: Record<RecentRunRecord["state"], string> = {
  idle: "未知",
  preflight: "启动检查中",
  launching: "启动中",
  running: "最后已知运行中",
  verification_failed: "验证未通过",
  recovery_required: "需要恢复",
  stopped: "已停止",
  failed: "启动失败",
};

const engineLabels: Record<RecentRunRecord["engineAdapter"], string> = {
  "stock-chrome": "Standard · Chrome",
  "stock-edge": "Standard · Edge",
  "controlled-chromium": "Controlled Chromium",
  camoufox: "Managed · Camoufox",
};

const networkLabels: Record<RecentRunRecord["networkPolicy"]["mode"], string> = {
  direct: "直连",
  fixed_proxy: "固定代理",
  pac: "PAC",
};

function time(value: string | null) {
  return value === null ? "未知" : <time dateTime={value}>{new Date(value).toLocaleString()}</time>;
}

/** A historical display only; it never derives live integrity or guards. */
export function RecentRunDetails({ record, silo }: RecentRunDetailsProps) {
  const run = record?.siloId === silo.id ? record : null;
  const signals = run?.identityEvidence?.signals ?? [];
  const mismatches = signals.filter((signal) => signal.state === "mismatched");
  const unavailable = signals.filter((signal) => signal.state === "unavailable");
  return (
    <section className="recent-run" aria-label="最近运行记录">
      <p className="eyebrow">最近运行 · 历史快照</p>
      {run === null ? (
        <p className="recent-run-empty">这个 Silo 尚无保存的本机运行记录。</p>
      ) : (
        <>
          <p className="recent-run-note">
            以下是这次运行最后保存的状态与证据，不代表当前浏览器状态或新的验证。
          </p>
          <dl className="recent-run-grid">
            <div><dt>终态 / 最后已知状态</dt><dd>{stateLabels[run.state]}</dd></div>
            <div><dt>开始</dt><dd>{time(run.startedAt)}</dd></div>
            <div><dt>最后更新</dt><dd>{time(run.updatedAt)}</dd></div>
            <div><dt>结束</dt><dd>{time(run.endedAt)}</dd></div>
            <div><dt>Profile 归属</dt><dd>这个 Silo 的独立 Profile · 当次配置，未独立观测</dd></div>
            <div><dt>身份 Artifact</dt><dd>{run.artifactBinding ? "当次已绑定" : "无 · Standard 不使用身份 Artifact"}</dd></div>
            <div><dt>引擎声明</dt><dd>{engineLabels[run.engineAdapter]}</dd></div>
            <div>
              <dt>网络政策声明</dt>
              <dd>
                {networkLabels[run.networkPolicy.mode]}
                {run.networkPolicy.proxyRequired ? " · 代理必需" : " · 非必需代理"}
                {run.networkPolicy.externalMihomo ? " · 外部 Mihomo" : ""}
                {run.networkPolicy.endpointLabel ? ` · ${run.networkPolicy.endpointLabel}` : ""}
              </dd>
            </div>
            <div>
              <dt>身份观测</dt>
              <dd>
                {run.identityEvidence
                  ? <>{run.identityEvidence.state} · {time(run.identityEvidence.observedAt)}</>
                  : run.artifactBinding
                    ? "unavailable · 尚无网站可见身份观测"
                    : "不适用 · Standard 未配置身份 Artifact"}
              </dd>
            </div>
            <div>
              <dt>引擎证据</dt>
              <dd>{run.engineEvidence?.verifiedAdapter
                ? `verified · ${engineLabels[run.engineEvidence.verifiedAdapter]}`
                : run.engineEvidence?.launchedAdapter
                  ? `已启动 · ${engineLabels[run.engineEvidence.launchedAdapter]}`
                  : "unavailable"}</dd>
            </div>
            <div>
              <dt>网络出口证据</dt>
              <dd>{run.networkEvidence
                ? <>{run.networkEvidence.exit} · {time(run.networkEvidence.observedAt)}</>
                : "unavailable · 尚无运行时网络证据"}</dd>
            </div>
            {run.identityEvidence && (
              <div>
                <dt>身份信号</dt>
                <dd>{mismatches.length} 项不一致 · {unavailable.length} 项不可用</dd>
              </div>
            )}
          </dl>
          {run.reason && <p className="recent-run-reason">记录原因：{run.reason}</p>}
          {run.state === "failed" && !run.networkEvidence && !run.engineEvidence && (
            <p className="recent-run-note">启动在取得运行时证据前失败；这些项目保持 unavailable。</p>
          )}
          <details className="recent-run-details">
            <summary>记录归属与证据详情</summary>
            <dl className="recent-run-grid">
              <div><dt>Silo ID</dt><dd>{run.siloId}</dd></div>
              <div><dt>Run ID</dt><dd>{run.runId}</dd></div>
              <div><dt>Runtime ID</dt><dd>{run.runtimeId ?? "未知"}</dd></div>
              <div><dt>Artifact ID</dt><dd>{run.artifactBinding?.artifactId ?? "不适用"}</dd></div>
              {run.artifactBinding && (
                <div><dt>Artifact SHA-256</dt><dd>{run.artifactBinding.artifactFileSha256}</dd></div>
              )}
              <div><dt>Profile 配置归属</dt><dd>{run.profileSiloId}</dd></div>
              {run.networkPolicy.endpointLabel && (
                <div><dt>当次网络端点</dt><dd>{run.networkPolicy.endpointLabel}</dd></div>
              )}
            </dl>
            <IdentityEvidenceSignals signals={signals} />
          </details>
        </>
      )}
    </section>
  );
}
