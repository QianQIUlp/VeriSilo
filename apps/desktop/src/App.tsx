import { useEffect, useRef, useState } from "react";

import { useDesktopWorkspace } from "./workspace/useDesktopWorkspace.js";

import {
  Brand,
  LockedRoute,
  StatusCard,
  TabButton,
} from "./shared/components.js";

import { WorkspaceSheet } from "./shared/WorkspaceSheet.js";
import { IdentityMark } from "./shared/IdentityMark.js";

import { VaultAccess } from "./features/vault/VaultAccess.js";

import { IdentityInspectPanel } from "./features/identity/IdentityDetails.js";

import { ArchivedSiloList, SiloList } from "./features/silos/SiloList.js";

import { LegacyEnvironmentRecoveryPanel } from "./features/environments/LegacyRecovery.js";

import {
  activationStatusLabel,
  describeActivation,
  describeVault,
} from "./formatters.js";

import { activationStatusTone, formatDate } from "./shared/presentation.js";

import { NetworkCheckCard } from "./features/network/NetworkCheckCard.js";

import {
  LocalReportExportCard,
  SiloNetworkEvidenceHistory,
} from "./features/network/EvidenceHistory.js";

import { CreateSiloPanel } from "./features/silos/CreateSiloPanel.js";

import { EditSiloPanel } from "./features/silos/EditSiloPanel.js";

import { VaultAndDataPanel } from "./features/vault/VaultAndDataPanel.js";

import { CliPanel } from "./features/cli/CliPanel.js";

import { EnvironmentWorkspace } from "./features/environments/EnvironmentWorkspace.js";

export function App() {
  const {
    creation,
    startIdentityFromSilo,
    clearManagedTemplate,
    status,
    uiVaultLocked,
    vaultTransition,
    vaultBusy,
    inspectIdentity,
    setInspectIdentity,
    view,
    setView,
    notice,
    closeHintVisible,
    acknowledgeTrayCloseHint,
    keepWindowOpen,
    retryRestoredVaultState,
    passphrase,
    setPassphrase,
    submitVault,
    activeSilos,
    busy,
    lockVault,
    identityPreviews,
    archiveSilo,
    setEditingSilo,
    launchSilo,
    rebindSiloMihomo,
    recheckSiloBrowser,
    recheckSiloRuntime,
    stopSilo,
    networkEvidenceHistory,
    engineStatuses,
    storageUsage,
    legacyEnvironmentArtifacts,
    cleanupLegacyEnvironment,
    archivedSilos,
    deleteSilo,
    restoreArchivedSilo,
    browsers,
    networkResult,
    networkBusy,
    checkNetwork,
    setNetworkResult,
    clearNetworkEvidence,
    downloadLocalReport,
    editingSilo,
    updateSilo,
    updateManagedIdentity,
    setNotice,
    refresh,
    finishVaultRestore,
    withBusy,
    vaultUiGeneration,
  } = useDesktopWorkspace();
  const [toolsOpen, setToolsOpen] = useState(false);
  const [focusedSiloId, setFocusedSiloId] = useState<string>();
  const previousSilos = useRef<Set<string> | null>(null);
  useEffect(() => {
    if (!status || uiVaultLocked) {
      previousSilos.current = null;
      return;
    }
    const added =
      previousSilos.current &&
      activeSilos.find((silo) => !previousSilos.current?.has(silo.id));
    if (added) setFocusedSiloId(added.id);
    previousSilos.current = new Set(activeSilos.map((silo) => silo.id));
  }, [activeSilos, status, uiVaultLocked]);
  useEffect(() => {
    if (notice === null || notice.tone === "error") {
      return;
    }
    const timer = window.setTimeout(() => setNotice(null), 8_000);
    return () => window.clearTimeout(timer);
  }, [notice, setNotice]);
  if (status === null) {
    return (
      <main className="shell loading-state">
        <Brand />
        <p>正在读取本机 VeriSilo 状态…</p>
      </main>
    );
  }

  const vaultLocked = uiVaultLocked || status.vault.state !== "unlocked";
  const hasInspectableIdentity = activeSilos.some(
    (silo) => silo.engine.adapter !== "stock",
  );

  const toolsVisible =
    toolsOpen &&
    !vaultLocked &&
    vaultTransition === "idle" &&
    !closeHintVisible;
  const inspectionVisible =
    inspectIdentity &&
    hasInspectableIdentity &&
    !vaultLocked &&
    view === "overview" &&
    !toolsVisible &&
    !closeHintVisible &&
    vaultTransition === "idle";
  const workspaceNotice =
    notice !== null ? (
      <div
        aria-live={notice.tone === "error" ? "assertive" : "polite"}
        className={`notice ${notice.tone}`}
        role={notice.tone === "error" ? "alert" : "status"}
      >
        <span>{notice.message}</span>
        <button
          aria-label="关闭通知"
          className="notice-dismiss"
          onClick={() => setNotice(null)}
          type="button"
        >
          ×
        </button>
      </div>
    ) : null;

  return (
    <main
      className={`shell spatial-shell view-${view}${vaultLocked ? " is-locked" : ""}`}
    >
      <a className="skip-link" href="#workspace-content">
        跳到工作区
      </a>
      <header className="topbar">
        <Brand />
        <span className="shell-caption">A SPACE FOR EVERY YOU.</span>
        <div className="topbar-status">
          <span className="local-pill">本地控制</span>
          <span className={`vault-pill${vaultLocked ? " locked" : ""}`}>
            {vaultTransition === "restoring"
              ? vaultBusy
                ? "正在载入恢复后的保险库"
                : "恢复状态等待重新载入"
              : vaultLocked
                ? "保险库已锁定"
                : "保险库已解锁"}
          </span>
          {!vaultLocked && (
            <button
              type="button"
              className="button-secondary vault-lock"
              disabled={busy || vaultBusy}
              onClick={() => void lockVault()}
            >
              锁定
            </button>
          )}
          {vaultLocked || !hasInspectableIdentity ? null : (
            <button
              aria-pressed={inspectIdentity}
              className="inspect-toggle"
              onClick={() => {
                setInspectIdentity((current) => {
                  const next = !current;
                  try {
                    window.localStorage.setItem(
                      "verisilo.inspectIdentity",
                      next ? "1" : "0",
                    );
                  } catch {
                    // Preference is local-only; ignore quota or private-mode failures.
                  }
                  return next;
                });
              }}
              type="button"
            >
              检查身份
            </button>
          )}
        </div>
      </header>

      {vaultTransition === "idle" ? (
        <nav className="tabbar" aria-label="VeriSilo 桌面端功能">
          <TabButton
            active={view === "overview" || view === "edit"}
            label="环境概览"
            icon="atlas"
            onClick={() => setView("overview")}
          />
          <TabButton
            active={view === "create"}
            label="创建 Silo"
            icon="create"
            onClick={() => {
              clearManagedTemplate();
              setView("create");
            }}
          />
          <TabButton
            active={view === "settings"}
            label="保险库与数据"
            icon="vault"
            onClick={() => setView("settings")}
          />
          <TabButton
            active={view === "environments"}
            label="运行位置"
            icon="location"
            onClick={() => setView("environments")}
          />
          <TabButton
            active={view === "cli"}
            label="命令行"
            icon="terminal"
            onClick={() => setView("cli")}
          />
          {!vaultLocked && (
            <TabButton
              active={toolsOpen}
              label="本机工具"
              icon="tools"
              onClick={() => setToolsOpen(true)}
            />
          )}
        </nav>
      ) : null}

      <div
        className={`workspace-content${view === "overview" && !vaultLocked ? " workspace-field" : ""}`}
        id="workspace-content"
        tabIndex={-1}
      >
        {toolsVisible || inspectionVisible ? null : workspaceNotice}

        {closeHintVisible ? (
          <div className="close-hint-backdrop">
            <section
              aria-labelledby="close-hint-title"
              aria-modal="true"
              className="panel close-hint-panel"
              role="dialog"
            >
              <h2 id="close-hint-title">VeriSilo 没有退出，只是收进系统托盘</h2>
              <p>
                关闭窗口后 VeriSilo
                会继续在托盘运行，已打开的浏览器和自动锁定计时都保持不变。要真正退出，请使用托盘图标菜单里的「退出」。
              </p>
              <div className="card-actions">
                <button
                  className="button-secondary"
                  onClick={keepWindowOpen}
                  type="button"
                >
                  先不最小化
                </button>
                <button onClick={acknowledgeTrayCloseHint} type="button">
                  知道了，收进托盘
                </button>
              </div>
            </section>
          </div>
        ) : null}

        {view === "overview" ? (
          vaultTransition === "restoring" ? (
            <section
              aria-busy="true"
              aria-live="polite"
              className="panel loading-state"
            >
              <p className="eyebrow">保险库恢复</p>
              <h1>{vaultBusy ? "正在恢复安全状态…" : "恢复尚未完成"}</h1>
              <p>
                VeriSilo
                已清除旧会话中的表单和授权信息。完成保险库与运行状态核对前，
                相关操作会保持锁定。
              </p>
              <button
                disabled={vaultBusy}
                onClick={() => void retryRestoredVaultState()}
                type="button"
              >
                {vaultBusy ? "正在重新载入…" : "重新载入"}
              </button>
            </section>
          ) : vaultLocked ? (
            <VaultAccess
              busy={vaultBusy}
              passphrase={passphrase}
              setPassphrase={setPassphrase}
              status={status}
              submitVault={submitVault}
            />
          ) : (
            <>
              <SiloList
                focusedSiloId={focusedSiloId}
                onFocusSilo={setFocusedSiloId}
                activation={status.activation.activeSiloId}
                busy={busy}
                onArchive={archiveSilo}
                onCreate={() => {
                  clearManagedTemplate();
                  setView("create");
                }}
                onCreateIdentity={(silo) => {
                  if (startIdentityFromSilo(silo, identityPreviews[silo.id])) {
                    setView("create");
                  }
                }}
                onEdit={(silo) => {
                  setEditingSilo(silo);
                  setView("edit");
                }}
                onLaunch={launchSilo}
                onRebindMihomo={rebindSiloMihomo}
                onRecheckBrowser={recheckSiloBrowser}
                onRecheckRuntime={recheckSiloRuntime}
                onStop={stopSilo}
                runtimeActivation={status.activation}
                runtimeState={status.activation.state}
                silos={activeSilos}
                identityPreviews={identityPreviews}
                networkEvidence={networkEvidenceHistory}
                managedEngineReady={engineStatuses.some(
                  (engine) =>
                    engine.descriptor.id === "camoufox" &&
                    engine.health.state === "healthy",
                )}
                storageUsage={storageUsage}
              />
            </>
          )
        ) : null}

        {view === "create" ? (
          vaultLocked ? (
            <LockedRoute onUnlock={() => setView("overview")} />
          ) : (
            <div className="creation-space">
              <aside className="creation-preview">
                <button
                  type="button"
                  className="scene-config-link"
                  onClick={() => setView("overview")}
                >
                  ← 回到身份场
                </button>
                <span className="eyebrow">TAKING SHAPE / 一个空间正在成形</span>
                <IdentityMark id="new-identity" color={creation.color} />
                <h2>{creation.name || "下一种可能。"}</h2>
                <p>
                  先定义它的样子，
                  <br />
                  再给它一个独立的空间。
                </p>
                <small>外观预览 · 完成创建后才会生成 Silo</small>
              </aside>
              <div className="creation-form">
                <CreateSiloPanel {...creation} />
              </div>
            </div>
          )
        ) : null}

        {view === "edit" ? (
          vaultLocked || editingSilo === null ? (
            <LockedRoute onUnlock={() => setView("overview")} />
          ) : (
            <>
              {inspectIdentity && editingSilo.engine.adapter !== "stock" ? (
                <IdentityInspectPanel
                  activeSiloId={status.activation.activeSiloId}
                  identityPreviews={identityPreviews}
                  observation={status.websiteIdentity ?? null}
                  silos={[editingSilo]}
                />
              ) : null}
              <EditSiloPanel
                browsers={browsers}
                busy={busy}
                identityPreview={identityPreviews[editingSilo.id]}
                onCancel={() => {
                  setEditingSilo(null);
                  setView("overview");
                }}
                onSave={(input, networkInput, engineInput) =>
                  updateSilo(editingSilo, input, networkInput, engineInput)
                }
                onUpdateIdentity={(input) =>
                  updateManagedIdentity(editingSilo, input)
                }
                silo={editingSilo}
              />
            </>
          )
        ) : null}

        {view === "settings" ? (
          vaultLocked ? (
            <LockedRoute onUnlock={() => setView("overview")} />
          ) : (
            <VaultAndDataPanel
              busy={busy}
              onNotice={setNotice}
              onRefresh={refresh}
              onVaultRestored={finishVaultRestore}
              runBusy={withBusy}
            />
          )
        ) : null}

        {view === "cli" ? (
          vaultLocked ? (
            <LockedRoute onUnlock={() => setView("overview")} />
          ) : (
            <CliPanel busy={busy} onNotice={setNotice} />
          )
        ) : null}

        {view === "environments" ? (
          <EnvironmentWorkspace
            key={`${vaultUiGeneration}:${vaultLocked ? "locked" : "unlocked"}`}
            silos={activeSilos}
            vaultLocked={vaultLocked}
          />
        ) : null}

        {toolsVisible ? (
          <WorkspaceSheet title="本机工具" onClose={() => setToolsOpen(false)}>
            {workspaceNotice}
            <LegacyEnvironmentRecoveryPanel
              artifacts={legacyEnvironmentArtifacts}
              busy={busy}
              onCleanup={cleanupLegacyEnvironment}
              silos={[...activeSilos, ...archivedSilos]}
            />

            <ArchivedSiloList
              busy={busy}
              onDelete={deleteSilo}
              onRestore={restoreArchivedSilo}
              silos={archivedSilos}
              storageUsage={storageUsage}
            />

            <section className="status-grid" aria-label="当前状态">
              <StatusCard
                detail={describeVault(status.vault)}
                eyebrow="本地保险库"
                tone="good"
                value="已解锁"
              />
              <StatusCard
                detail={describeActivation(status.activation)}
                eyebrow="浏览器"
                tone={activationStatusTone(status.activation)}
                value={activationStatusLabel(status.activation.state)}
              />
              <StatusCard
                detail={
                  browsers.length === 0
                    ? "创建系统浏览器时可以手动填写文件位置"
                    : browsers.map((browser) => browser.displayName).join("、")
                }
                eyebrow="系统里的 Chrome / Edge"
                tone={browsers.length > 0 ? "good" : "warn"}
                value={`${browsers.length} 个安装`}
              />
              <StatusCard
                detail={
                  networkResult === null
                    ? "不会自动去查"
                    : `检查于 ${formatDate(networkResult.checkedAt)}`
                }
                eyebrow="这台电脑的出口"
                tone={
                  networkResult !== null && networkResult.ip !== null
                    ? "good"
                    : "neutral"
                }
                value={
                  networkResult?.ip?.address ??
                  (networkResult === null ? "还没检查" : "没有查到")
                }
              />
            </section>

            <NetworkCheckCard
              busy={networkBusy}
              onCheck={() => void checkNetwork()}
              onClear={() => setNetworkResult(null)}
              result={networkResult}
            />

            <SiloNetworkEvidenceHistory
              busy={busy}
              evidence={networkEvidenceHistory}
              onClear={clearNetworkEvidence}
              silos={[...activeSilos, ...archivedSilos]}
            />

            <LocalReportExportCard
              busy={busy}
              evidence={networkEvidenceHistory}
              onDownload={downloadLocalReport}
              silos={[...activeSilos, ...archivedSilos]}
            />
          </WorkspaceSheet>
        ) : null}
        {inspectionVisible ? (
          <WorkspaceSheet
            title="网站可见身份"
            onClose={() => setInspectIdentity(false)}
          >
            {workspaceNotice}
            <IdentityInspectPanel
              activeSiloId={status.activation.activeSiloId}
              identityPreviews={identityPreviews}
              observation={status.websiteIdentity ?? null}
              silos={activeSilos}
            />
          </WorkspaceSheet>
        ) : null}

        <footer>
          <span>VeriSilo · 身份留在本机，证据如实呈现。</span>
          <span>关闭窗口后继续运行，从托盘菜单退出。</span>
        </footer>
      </div>
    </main>
  );
}
