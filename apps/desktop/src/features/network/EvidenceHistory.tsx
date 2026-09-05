import { type SiloNetworkEvidence } from "../../desktop-api.js";

import { type Silo } from "@verisilo/contracts";

import { dnsStateLabel, formatDate } from "../../shared/presentation.js";

import { useState } from "react";

export function SiloNetworkEvidenceHistory({
  busy,
  evidence,
  onClear,
  silos,
}: {
  busy: boolean;
  evidence: SiloNetworkEvidence[];
  onClear: (silo: Silo) => Promise<void>;
  silos: Silo[];
}) {
  if (evidence.length === 0) {
    return (
      <section className="panel evidence-history-panel empty-evidence-history">
        <div>
          <p className="eyebrow">网络检查记录</p>
          <h2>还没有 Silo 的检查结果</h2>
          <p>
            启动一个 Silo 后，从浏览器侧边栏运行网络检查。结果会加密保存在本机。
          </p>
        </div>
      </section>
    );
  }

  const siloById = new Map(silos.map((silo) => [silo.id, silo]));
  const visible = evidence.slice(0, 12);
  const clearableSilos = [
    ...new Map(
      visible
        .map((entry) => siloById.get(entry.siloId))
        .filter((silo): silo is Silo => silo !== undefined)
        .map((silo) => [silo.id, silo]),
    ).values(),
  ];

  return (
    <section className="panel evidence-history-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">网络检查记录</p>
          <h2>最近的 Silo 网络检查</h2>
          <p>
            这些结果来自你在 Silo 内主动运行的检查，并加密保存在这台电脑上。
          </p>
        </div>
        <span className="provider-badge">最近 {visible.length} 条</span>
      </div>
      <div className="evidence-history-list">
        {visible.map((entry) => {
          const silo = siloById.get(entry.siloId);
          const ip = entry.result.ip;
          return (
            <article className="evidence-history-row" key={entry.evidenceId}>
              <div className="evidence-history-heading">
                <span
                  className="silo-mark small-mark"
                  style={{ backgroundColor: silo?.color ?? "#667085" }}
                >
                  {(silo?.name ?? "?").slice(0, 1).toUpperCase()}
                </span>
                <div>
                  <strong>{silo?.name ?? "已删除的 Silo"}</strong>
                  <span>{formatDate(entry.result.checkedAt)}</span>
                </div>
              </div>
              <dl className="evidence-history-facts">
                <div>
                  <dt>当次请求出口</dt>
                  <dd>{ip?.address ?? "本次未取得"}</dd>
                </div>
                <div>
                  <dt>地区与网络</dt>
                  <dd>
                    {ip === null
                      ? "无可用观察"
                      : [
                          ip.countryCode ?? ip.country,
                          ip.region,
                          ip.city,
                          ip.asn,
                        ]
                          .filter((value): value is string => value !== null)
                          .join(" · ") || "未返回"}
                  </dd>
                </div>
                <div>
                  <dt>公共 DNS</dt>
                  <dd>{dnsStateLabel(entry.result)}</dd>
                </div>
              </dl>
            </article>
          );
        })}
      </div>
      <div className="evidence-history-actions">
        {clearableSilos.map((silo) => (
          <button
            className="button-secondary"
            disabled={busy}
            key={silo.id}
            onClick={() => void onClear(silo)}
            type="button"
          >
            清除「{silo.name}」记录
          </button>
        ))}
      </div>
    </section>
  );
}

export function LocalReportExportCard({
  busy,
  evidence,
  onDownload,
  silos,
}: {
  busy: boolean;
  evidence: SiloNetworkEvidence[];
  onDownload: (silo: Silo, format: "json" | "html") => void;
  silos: Silo[];
}) {
  const [selectedSiloId, setSelectedSiloId] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const selectedSilo = silos.find((silo) => silo.id === selectedSiloId);
  const selectedEvidenceCount = evidence.filter(
    (entry) => entry.siloId === selectedSiloId,
  ).length;
  const canDownload = selectedSilo !== undefined && confirmed && !busy;

  return (
    <section className="panel report-export-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">隐私检查报告</p>
          <h2>导出一个 Silo 的检查结果</h2>
          <p>报告只在这台电脑上生成，不会上传，也不会自动保存。</p>
        </div>
        <span className="provider-badge">默认脱敏</span>
      </div>

      <div className="report-export-controls">
        <label>
          要导出的 Silo
          <select
            aria-label="要导出的 Silo"
            disabled={busy}
            onChange={(event) => {
              setSelectedSiloId(event.target.value);
              setConfirmed(false);
            }}
            value={selectedSiloId}
          >
            <option value="">请选择一个 Silo</option>
            {silos.map((silo) => (
              <option key={silo.id} value={silo.id}>
                {silo.name}
                {silo.archivedAt === null ? "" : "（已归档）"}
              </option>
            ))}
          </select>
        </label>
        <div className="report-selection-summary" aria-live="polite">
          {selectedSilo === undefined
            ? "先选择 Silo，报告不会默认包含任何 Silo。"
            : `报告将包含 ${selectedEvidenceCount} 条该 Silo 的网络检查记录和当前设置。`}
        </div>
        <label className="report-confirmation">
          <input
            checked={confirmed}
            disabled={selectedSilo === undefined || busy}
            onChange={(event) => setConfirmed(event.target.checked)}
            type="checkbox"
          />
          <span>
            我确认只导出「{selectedSilo?.name ?? "所选 Silo"}」的脱敏本地报告。
          </span>
        </label>
        <div className="report-export-actions">
          <button
            disabled={!canDownload}
            onClick={() =>
              selectedSilo === undefined
                ? undefined
                : onDownload(selectedSilo, "html")
            }
            type="button"
          >
            下载报告
          </button>
          <button
            className="button-secondary"
            disabled={!canDownload}
            onClick={() =>
              selectedSilo === undefined
                ? undefined
                : onDownload(selectedSilo, "json")
            }
            type="button"
          >
            下载数据文件
          </button>
        </div>
      </div>

      <div className="report-boundary">
        <strong>报告说明</strong>
        <p>
          报告包含浏览器类型、版本和网络检查结果。DNS 信息只反映检查当时的结果。
        </p>
      </div>
      <details className="report-developer-details">
        <summary>报告中不包含的内容</summary>
        <p>
          报告不会包含浏览器数据位置、代理地址、完整 IP、城市、访问密钥、凭据
          或其他可以直接识别本机配置的信息。
        </p>
      </details>
    </section>
  );
}
