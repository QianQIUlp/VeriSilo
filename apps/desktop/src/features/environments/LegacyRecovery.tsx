import { type LegacyEnvironmentArtifact } from "../../desktop-api.js";

import { type Silo } from "@verisilo/contracts";

import { legacyEnvironmentLabel } from "../../shared/presentation.js";

export function LegacyEnvironmentRecoveryPanel({
  artifacts,
  busy,
  onCleanup,
  silos,
}: {
  artifacts: LegacyEnvironmentArtifact[];
  busy: boolean;
  onCleanup: (artifact: LegacyEnvironmentArtifact) => Promise<void>;
  silos: Silo[];
}) {
  if (artifacts.length === 0) {
    return null;
  }

  return (
    <section className="panel legacy-environment-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">需要处理一次</p>
          <h2>清理不再使用的旧运行环境</h2>
          <p>
            这些环境来自较早的设置，不属于 Silo
            当前选择的运行位置。清理后才能正常归档、删除或恢复保险库。
          </p>
        </div>
      </div>
      <div className="legacy-environment-list">
        {artifacts.map((artifact) => {
          const silo = silos.find(
            (candidate) => candidate.id === artifact.siloId,
          );
          return (
            <div
              className={`legacy-environment-row${
                artifact.cleanupAvailable ? "" : " blocked"
              }`}
              key={`${artifact.siloId}-${artifact.backend}`}
            >
              <div>
                <strong>{silo?.name ?? "未知 Silo"}</strong>
                <span>{legacyEnvironmentLabel(artifact.backend)}</span>
                <p>
                  {artifact.cleanupAvailable
                    ? "归属信息已核对，可以安全清理，不会改变当前运行位置。"
                    : "归属信息不完整或与当前运行位置不一致，VeriSilo 已阻止自动删除。"}
                </p>
              </div>
              {artifact.cleanupAvailable ? (
                <button
                  className="button-danger"
                  disabled={busy}
                  onClick={() => void onCleanup(artifact)}
                  type="button"
                >
                  核对并清理
                </button>
              ) : (
                <span className="provider-badge warning">需要人工核对</span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
