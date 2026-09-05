import { type NetworkCheckResult } from "@verisilo/contracts";

import {
  dnsStateLabel,
  networkLocation,
  networkOwner,
} from "../../shared/presentation.js";

import { ResultItem } from "../../shared/components.js";

export function NetworkCheckCard({
  busy,
  onCheck,
  onClear,
  result,
}: {
  busy: boolean;
  onCheck: () => void;
  onClear: () => void;
  result: NetworkCheckResult | null;
}) {
  return (
    <section className="panel network-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">出口检查</p>
          <h2>看看这台电脑现在从哪出去</h2>
          <p>查的是 VeriSilo 这个窗口自己的出口，不是已经打开的那个浏览器。</p>
        </div>
        <div className="panel-actions">
          <button disabled={busy} onClick={onCheck} type="button">
            {busy ? "正在检查…" : result === null ? "同意并检查" : "重新检查"}
          </button>
          {result !== null ? (
            <button
              className="button-secondary"
              onClick={onClear}
              type="button"
            >
              清除
            </button>
          ) : null}
        </div>
      </div>

      {result === null ? (
        <div className="empty-inline">
          <strong>还没检查</strong>
          <span>
            点一下会向公开查询服务询问当前
            IP。对方能看到这次请求的地址，结果只留在这个窗口里。
          </span>
        </div>
      ) : (
        <div className="network-result">
          <div className="network-primary">
            <span>公网 IP</span>
            <strong>{result.ip?.address ?? "获取失败"}</strong>
            <small>{networkLocation(result)}</small>
          </div>
          <div className="network-details">
            <ResultItem label="网络归属" value={networkOwner(result)} />
            <ResultItem
              label="出口时区"
              value={result.ip?.timezone ?? "未知"}
            />
            <ResultItem label="公共 DNS" value={dnsStateLabel(result)} />
            <ResultItem
              label="DNSSEC"
              value={
                result.dns.dnssec === "validated"
                  ? "两家均返回已验证"
                  : "未完整验证"
              }
            />
          </div>
          <div className="network-chips">
            <span className={result.ip === null ? "warn" : "good"}>
              {result.ip === null ? "IP 未确认" : "IP 已确认"}
            </span>
            <span
              className={result.dns.state === "consistent" ? "good" : "warn"}
            >
              {dnsStateLabel(result)}
            </span>
          </div>
          <p className="scope-copy">
            DNS 结果只反映本次检查。网络设置发生变化后，请重新检查。
          </p>
          {result.errors.length > 0 ? (
            <details className="error-details">
              <summary>查看部分检查错误</summary>
              <ul>
                {result.errors.map((error) => (
                  <li key={error}>{error}</li>
                ))}
              </ul>
            </details>
          ) : null}
        </div>
      )}
    </section>
  );
}
