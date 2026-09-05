import { useDesktopWorkspace } from "./workspace/useDesktopWorkspace.js";

import {
  Brand,
  LockedRoute,
  StatusCard,
  TabButton,
} from "./shared/components.js";

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
    status,
    uiVaultLocked,
    vaultTransition,
    vaultBusy,
    inspectIdentity,
    setInspectIdentity,
    view,
    setView,
    notice,
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
  if (status === null) {
    return (
      <main className="shell loading-state">
        <Brand />
        <p>正在读取本机 VeriSilo 状态…</p>
      </main>
    );
  }

  const vaultLocked = uiVaultLocked || status.vault.state !== "unlocked";

  return (
    <main className="shell">
      <header className="topbar">
        <Brand />
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
          {vaultLocked ? null : (
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
            onClick={() => setView("overview")}
          />
          <TabButton
            active={view === "create"}
            label="创建 Silo"
            onClick={() => setView("create")}
          />
          <TabButton
            active={view === "settings"}
            label="保险库与数据"
            onClick={() => setView("settings")}
          />
          <TabButton
            active={view === "environments"}
            label="运行位置"
            onClick={() => setView("environments")}
          />
          <TabButton
            active={view === "cli"}
            label="命令行"
            onClick={() => setView("cli")}
          />
        </nav>
      ) : null}

      {notice !== null ? (
        <div
          aria-live={notice.tone === "error" ? "assertive" : "polite"}
          className={`notice ${notice.tone}`}
          role={notice.tone === "error" ? "alert" : "status"}
        >
          {notice.message}
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
            <section className="overview-hero panel">
              <div>
                <p className="eyebrow">环境概览</p>
                <h1>
                  {activeSilos.length === 0
                    ? "创建你的第一个 Silo"
                    : `${activeSilos.length} 个 Silo`}
                </h1>
                <p>
                  每个空间有自己的
                  Cookie、登录和网站数据。打开其中一个，就是在用那一套身份上网。
                </p>
              </div>
              <div className="hero-actions">
                <button onClick={() => setView("create")} type="button">
                  新建 Silo
                </button>
                <button
                  className="button-secondary"
                  disabled={busy || vaultBusy}
                  onClick={() => void lockVault()}
                  type="button"
                >
                  锁定
                </button>
              </div>
            </section>

            {inspectIdentity ? (
              <IdentityInspectPanel
                activeSiloId={status.activation.activeSiloId}
                identityPreviews={identityPreviews}
                observation={status.websiteIdentity ?? null}
                silos={activeSilos}
              />
            ) : null}

            <SiloList
              activation={status.activation.activeSiloId}
              busy={busy}
              onArchive={archiveSilo}
              onCreate={() => setView("create")}
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
          </>
        )
      ) : null}

      {view === "create" ? (
        vaultLocked ? (
          <LockedRoute onUnlock={() => setView("overview")} />
        ) : (
          <CreateSiloPanel {...creation} />
        )
      ) : null}

      {view === "edit" ? (
        vaultLocked || editingSilo === null ? (
          <LockedRoute onUnlock={() => setView("overview")} />
        ) : (
          <>
            {inspectIdentity ? (
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

      <footer>
        VeriSilo
        把不同用途的浏览分开存放。关窗口后还在托盘里，要从托盘菜单才能退出。
      </footer>
    </main>
  );
}
