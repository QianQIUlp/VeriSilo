import { memo, useCallback, useRef, useState } from "react";
import { IdentityMark } from "../../shared/IdentityMark.js";
import {
  type ManagedIdentityPreview,
  type RuntimeSessionStatus,
  type SiloNetworkEvidence,
} from "../../desktop-api.js";
import { idleRuntimeActivation, sessionNeedsManagement } from "../../runtime-status.js";

import { type RecentRunRecord, type RuntimeActivation, type Silo } from "@verisilo/contracts";

import {
  formatDate,
  formatStorageSuffix,
  siloBrowserLabel,
  siloExecutionTargetLabel,
  siloWebsiteIdentityBoundary,
} from "../../shared/presentation.js";

import {
  identityEvidenceLabels,
  ManagedIdentityFacts,
  ManagedIdentityEvidence,
  ManagedStatusGroups,
} from "../identity/IdentityDetails.js";

import { identityEvidenceContext } from "../identity/evidence-diagnostics.js";

import { RecentRunDetails } from "../identity/RecentRunDetails.js";

import { CurrentSessionIntegrity } from "../identity/CurrentSessionIntegrity.js";

import { CapabilityState } from "../../shared/components.js";

import { activationStatusLabel, describeNetwork } from "../../formatters.js";

type LensKind = "identity" | "network" | "session" | "configuration";

const lensTitles: Record<LensKind, string> = {
  identity: "身份观测",
  network: "网络与技术证据",
  session: "运行与记录",
  configuration: "身份与配置",
};

function canRunBesideManaged(silo: Silo): boolean {
  return silo.executionTarget.kind === "local" &&
    silo.engine.adapter === "camoufox" &&
    (silo.networkProfile.mode === "direct" ||
      silo.networkProfile.mode === "fixed_proxy" && silo.networkProfile.externalMihomo === undefined);
}

/**
 * One per-silo scene. Memoized so unrelated workspace renders (notices,
 * polls, unrelated state) do not re-render every silo; props are per-silo
 * values plus stable callbacks.
 */
export const SiloScene = memo(function SiloScene({
  silo,
  index,
  selected,
  isActive,
  launchBlockReason,
  runtimeActivation,
  runtimeState,
  busy,
  isLaunching,
  managedEngineReady,
  networkEvidence,
  identityPreview,
  recentRun = null,
  recentRunsError = null,
  storageBytes,
  lens,
  openLens,
  closeLens,
  choose,
  onArchive,
  onCreateIdentity,
  onEdit,
  onLaunch,
  onRebindMihomo,
  onRecheckBrowser,
  onRecheckRuntime,
  onStop,
}: {
  silo: Silo;
  index: number;
  selected: boolean;
  isActive: boolean;
  launchBlockReason: string | null;
  runtimeActivation: RuntimeActivation;
  runtimeState: RuntimeActivation["state"];
  busy: boolean;
  isLaunching: boolean;
  managedEngineReady: boolean;
  networkEvidence: SiloNetworkEvidence[];
  identityPreview: ManagedIdentityPreview | undefined;
  recentRun?: RecentRunRecord | null;
  recentRunsError?: string | null;
  storageBytes: number | null | undefined;
  lens: LensKind | null;
  openLens: (next: LensKind, trigger: HTMLButtonElement) => void;
  closeLens: () => void;
  choose: (index: number, focus?: boolean) => void;
  onArchive: (silo: Silo) => Promise<void>;
  onCreateIdentity?: ((silo: Silo) => void) | undefined;
  onEdit: (silo: Silo) => void;
  onLaunch: (silo: Silo) => Promise<void>;
  onRebindMihomo: (silo: Silo) => Promise<void>;
  onRecheckBrowser: (silo: Silo) => Promise<void>;
  onRecheckRuntime: (silo: Silo) => Promise<void>;
  onStop: (silo: Silo) => Promise<void>;
}) {
  // Drag state is per-scene: only the visible (selected) scene receives pointers.
  const drag = useRef<{ start: number; moved: boolean } | null>(null);
  const dragged = useRef(false);
  const managedCamoufox = silo.engine.adapter === "camoufox";
  const showRuntimeState = isActive || ["verification_failed", "failed"].includes(runtimeState);
  const identityPreviewForSilo = identityPreview;
  const blockedByRunning = launchBlockReason !== null;
  const canStop =
    isActive &&
    runtimeState === "running" &&
    (silo.executionTarget.kind === "wsl" || managedCamoufox);
  const canClear =
    isActive &&
    managedCamoufox &&
    ["verification_failed", "failed", "recovery_required"].includes(
      runtimeState,
    );
  const evidenceContext = identityEvidenceContext(runtimeActivation, silo);
  const evidence = evidenceContext.evidence;
  const evidenceState = evidence === null ? "unavailable"
    : evidenceContext.current ? evidence.state : "stale";
  return (
    <article
      className={`silo-scene ${isActive ? "is-active " : ""}state-${showRuntimeState ? runtimeState : "idle"}`}
      id={`silo-${silo.id}`}
      aria-label={silo.name}
      hidden={!selected}
    >
      <div className="scene-world">
        <div className="scene-copy">
          <span className={`running-badge ${showRuntimeState ? runtimeState : "idle"}`}>
            {showRuntimeState ? activationStatusLabel(runtimeState) : "未运行"}
          </span>
          <h2
            className={silo.name.length > 20 ? "long-name" : undefined}
            title={silo.name}
          >
            {silo.name}
          </h2>
          <p>
            {siloBrowserLabel(silo)}
            <br />
            {siloExecutionTargetLabel(silo)}
          </p>
          <button
            type="button"
            className="scene-config-link"
            onClick={(event) => openLens("configuration", event.currentTarget)}
            aria-expanded={lens === "configuration"}
          >
            身份与配置 <span aria-hidden="true">↗</span>
          </button>
          <p className="scene-boundary">
            {managedCamoufox
              ? "持久身份 · 独立数据 · 运行证据"
              : "网站数据独立保存 · 设备身份跟随本机"}
          </p>
        </div>
        <div className="scene-field">
          <div className="field-orbit orbit-outer" aria-hidden="true" />
          <div className="field-orbit orbit-inner" aria-hidden="true" />
          <button
            type="button"
            className="identity-focus"
            aria-label={`查看${silo.name}的身份观测`}
            aria-expanded={lens === "identity"}
            onPointerDown={(event) => {
              if (!event.isPrimary || event.button !== 0) return;
              drag.current = { start: event.clientX, moved: false };
              dragged.current = false;
              event.currentTarget.classList.add("is-dragging");
              event.currentTarget.setPointerCapture(event.pointerId);
            }}
            onPointerMove={(event) => {
              if (!drag.current) return;
              const delta = event.clientX - drag.current.start;
              drag.current.moved ||= Math.abs(delta) > 8;
              event.currentTarget.style.setProperty(
                "--drag-x",
                `${Math.max(-110, Math.min(110, delta * 0.55))}px`,
              );
            }}
            onPointerUp={(event) => {
              if (!drag.current) return;
              const delta = event.clientX - drag.current.start;
              dragged.current = drag.current.moved;
              if (Math.abs(delta) > 65) choose(index + (delta < 0 ? 1 : -1));
              drag.current = null;
              event.currentTarget.classList.remove("is-dragging");
              event.currentTarget.style.setProperty("--drag-x", "0px");
            }}
            onPointerCancel={(event) => {
              drag.current = null;
              dragged.current = true;
              event.currentTarget.classList.remove("is-dragging");
              event.currentTarget.style.setProperty("--drag-x", "0px");
            }}
            onClick={(event) => {
              if (event.detail === 0 || !dragged.current)
                openLens("identity", event.currentTarget);
            }}
          >
            <span className="identity-sculpture">
              <IdentityMark id={silo.id} color={silo.color} />
            </span>
            <span className="focus-coordinate" aria-hidden="true">
              {String(index + 1).padStart(2, "0")} / SILO
            </span>
          </button>
          <button
            type="button"
            className={`field-node node-identity ${managedCamoufox ? evidenceState : "standard"}`}
            onClick={(event) => openLens("identity", event.currentTarget)}
            aria-expanded={lens === "identity"}
          >
            <span className="node-icon" aria-hidden="true">
              {managedCamoufox
                ? {
                    matched: "≍",
                    mismatched: "≠",
                    unavailable: "∅",
                    stale: "◷",
                  }[evidenceState]
                : "◌"}
            </span>
            <span>
              <small>身份观测</small>
              <strong>
                {managedCamoufox
                  ? evidence !== null && !evidenceContext.current
                    ? evidenceContext.scopeLabel
                    : identityEvidenceLabels[evidenceState][0]
                  : "跟随本机"}
              </strong>
              <em key={evidence?.observedAt ?? "none"}>
                {evidence ? (
                  <time dateTime={evidence.observedAt}>
                    {formatDate(evidence.observedAt)}
                  </time>
                ) : managedCamoufox ? (
                  "尚无观测"
                ) : (
                  "Standard Silo"
                )}
              </em>
            </span>
            <b aria-hidden="true">↗</b>
          </button>
          <button
            type="button"
            className="field-node node-network"
            onClick={(event) => openLens("network", event.currentTarget)}
            aria-expanded={lens === "network"}
          >
            <span className="node-icon" aria-hidden="true">
              ↗
            </span>
            <span>
              <small>网络策略</small>
              <strong>
                {silo.networkProfile.mode === "direct"
                  ? "Direct 直连"
                  : silo.networkProfile.mode === "pac"
                    ? "PAC 规则"
                    : "代理出口"}
              </strong>
              <em>查看配置与证据</em>
            </span>
            <b aria-hidden="true">↗</b>
          </button>
          <button
            type="button"
            className="field-node node-session"
            onClick={(event) => openLens("session", event.currentTarget)}
            aria-expanded={lens === "session"}
          >
            <span className="node-icon" aria-hidden="true">
              ◎
            </span>
            <span>
              <small>{isActive ? "当前运行" : "最近运行"}</small>
              <strong>
                {isActive ? activationStatusLabel(runtimeState) : recentRunsError !== null ? "记录不可用" : recentRun ? "已有记录" : "暂无记录"}
              </strong>
              <em>{isActive ? "查看本次运行" : recentRun ? "查看最后保存的状态" : "查看记录状态"}</em>
            </span>
            <b aria-hidden="true">↗</b>
          </button>
          <p className="field-hint">左右拖动轮廓切换身份 · 点节点展开证据</p>
        </div>
        <div className="scene-action-area">
          <div className="scene-actions">
            <button
              className="scene-primary-action"
              disabled={
                busy ||
                isLaunching ||
                blockedByRunning ||
                (isActive && !canStop && !canClear)
              }
              onClick={() => {
                choose(index);
                void (canStop || canClear ? onStop(silo) : onLaunch(silo));
              }}
              title={launchBlockReason ?? undefined}
              type="button"
            >
              {isLaunching
                ? "正在打开…"
                : canStop
                  ? "停止"
                  : canClear
                    ? "结束会话"
                    : isActive &&
                        silo.executionTarget.kind === "local" &&
                        !managedCamoufox
                      ? "关掉窗口即可停止"
                      : "打开浏览器"}
            </button>
            {isActive ? (
              <button
                className="button-secondary"
                disabled={busy}
                onClick={() => void onRecheckRuntime(silo)}
                type="button"
              >
                {runtimeState === "recovery_required"
                  ? "再检查一次"
                  : runtimeState === "verification_failed"
                    ? "查看原因"
                    : "重新检查"}
              </button>
            ) : (
              <button
                className="button-secondary"
                disabled={busy}
                onClick={() => void onRecheckBrowser(silo)}
                type="button"
              >
                检查浏览器
              </button>
            )}

            <details className="scene-more">
              <summary>
                更多操作 <span aria-hidden="true">＋</span>
              </summary>
              <div className="scene-action-menu">
                <button
                  className="button-secondary"
                  disabled={busy || isActive}
                  onClick={() => onEdit(silo)}
                  type="button"
                >
                  编辑
                </button>
                {managedCamoufox &&
                identityPreviewForSilo !== undefined &&
                onCreateIdentity !== undefined ? (
                  <button
                    className="button-secondary"
                    disabled={busy}
                    onClick={() => onCreateIdentity(silo)}
                    title="以此 Silo 的设备和网络设置为模板创建新的托管身份；原 Silo 保持不变"
                    type="button"
                  >
                    创建新身份
                  </button>
                ) : null}
                <button
                  className="button-secondary"
                  disabled={busy || isActive}
                  onClick={() => void onArchive(silo)}
                  type="button"
                >
                  归档
                </button>
                {isActive &&
                silo.networkProfile.mode === "fixed_proxy" &&
                silo.networkProfile.externalMihomo !== undefined ? (
                  <button
                    className="button-secondary"
                    disabled={busy}
                    onClick={() => void onRebindMihomo(silo)}
                    type="button"
                  >
                    {runtimeState === "verification_failed"
                      ? "换节点后再打开"
                      : "更换代理节点"}
                  </button>
                ) : null}
              </div>
            </details>
          </div>
          {isActive &&
          silo.executionTarget.kind === "local" &&
          silo.engine.adapter === "stock" ? (
            <p className="local-runtime-guidance">
              用完后直接关掉这个浏览器窗口即可。不会动你其他的 Chrome 或 Edge
              窗口。
            </p>
          ) : null}
          {blockedByRunning ? (
            <p className="mutex-note" role="note">
              {launchBlockReason}
            </p>
          ) : null}
        </div>
      </div>
      <aside
        className="scene-lens"
        aria-label={lens ? lensTitles[lens] : "身份详情"}
        hidden={lens === null}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            closeLens();
          }
        }}
      >
        <header className="lens-heading">
          <div>
            <span className="eyebrow">{silo.name}</span>
            <h2 id={`lens-heading-${silo.id}`} tabIndex={-1}>
              {lens ? lensTitles[lens] : "身份详情"}
            </h2>
          </div>
          <button
            type="button"
            className="button-secondary"
            aria-label="收起证据"
            onClick={closeLens}
          >
            ↗
          </button>
        </header>
        <div className="lens-scroll" key={`${silo.id}:${lens}`}>
          {selected && lens === "identity" ? (
            <div>
              {managedCamoufox ? (
                <ManagedIdentityEvidence
                  activation={runtimeActivation}
                  silo={silo}
                />
              ) : (
                <div className="standard-identity-note">
                  <h3>网站数据独立，设备身份跟随本机。</h3>
                  <p>
                    Standard Silo 单独保存登录、Cookie
                    与网站数据。系统浏览器不会换成另一套设备身份。
                  </p>
                  <CapabilityState state="inherit" />
                </div>
              )}
            </div>
          ) : null}
          {selected && lens === "session" ? (
            <div>
              {managedCamoufox && isActive ? (
                <CurrentSessionIntegrity
                  activation={runtimeActivation}
                  managedEngineReady={managedEngineReady}
                  silo={silo}
                  onInspectIdentity={(trigger) => openLens("identity", trigger)}
                  onInspectNetwork={(trigger) => openLens("network", trigger)}
                />
              ) : (
                <div className="session-resting">
                  <span aria-hidden="true">◎</span>
                  <h3>
                    {isActive
                      ? activationStatusLabel(runtimeState)
                      : "这个身份当前未运行"}
                  </h3>
                  <p>
                    {isActive
                      ? "用完后关闭浏览器窗口。"
                      : "打开浏览器后，在这里查看当前运行；已保存的记录在下方。"}
                  </p>
                </div>
              )}
              {recentRunsError !== null ? (
                <p role="status">最近运行记录暂不可用：{recentRunsError}</p>
              ) : (
                <RecentRunDetails silo={silo} record={recentRun} />
              )}
            </div>
          ) : null}
          {selected && lens === "configuration" ? (
            <div>
              <dl className="silo-facts">
                <div>
                  <dt>网站数据</dt>
                  <dd>
                    登录和 Cookie 单独保存
                    {formatStorageSuffix(storageBytes)}
                  </dd>
                </div>
                <div>
                  <dt>运行位置</dt>
                  <dd>{siloExecutionTargetLabel(silo)}</dd>
                </div>
                <div>
                  <dt>对外身份</dt>
                  <dd>
                    {siloWebsiteIdentityBoundary(silo, identityPreviewForSilo)}
                  </dd>
                </div>
                {identityPreviewForSilo !== undefined ? (
                  <ManagedIdentityFacts preview={identityPreviewForSilo} />
                ) : null}
                {silo.engine.adapter === "stock" ? (
                  <>
                    <div>
                      <dt>这台电脑</dt>
                      <dd>
                        <CapabilityState state="inherit" />
                        指纹跟这台电脑上的 Chrome 或 Edge 一样
                      </dd>
                    </div>
                    <div>
                      <dt>独立指纹</dt>
                      <dd>
                        <CapabilityState state="unavailable" />
                        系统浏览器不会换成另一套设备身份
                      </dd>
                    </div>
                  </>
                ) : null}
                <div>
                  <dt>网络</dt>
                  <dd>{describeNetwork(silo.networkProfile)}</dd>
                </div>
                <div>
                  <dt>身份状态</dt>
                  <dd>
                    <span
                      className={`identity-lock-state${
                        silo.identityLockedAt === null ? " pending" : " locked"
                      }`}
                    >
                      {silo.identityLockedAt === null
                        ? "打开成功后锁定，改指纹请新建"
                        : "已锁定"}
                    </span>
                  </dd>
                </div>
                {silo.networkProfile.mode === "fixed_proxy" &&
                silo.networkProfile.credentialRef !== undefined ? (
                  <div>
                    <dt>代理密码</dt>
                    <dd>已加密保存在本机</dd>
                  </div>
                ) : null}
              </dl>
            </div>
          ) : null}
          {selected && lens === "network" ? (
            <div>
              <div className="network-policy-summary">
                <span className="eyebrow">已配置的网络策略</span>
                <p>{describeNetwork(silo.networkProfile)}</p>
                <small>配置声明与实际出口观察分别呈现。</small>
              </div>
              {silo.engine.adapter !== "stock" ? (
                <ManagedStatusGroups
                  activation={runtimeActivation}
                  evidence={networkEvidence}
                  engineHealthy={managedEngineReady}
                  runtimeState={isActive ? runtimeState : "idle"}
                  silo={silo}
                />
              ) : (
                <p>在「本机工具」中查看浏览器侧取得的网络检查记录。</p>
              )}
            </div>
          ) : null}
        </div>
      </aside>
    </article>
  );
});

export function SiloList({
  focusedSiloId,
  onFocusSilo,
  busy,
  siloBusyIds,
  launchingSiloIds,
  managedSessionLimit,
  managedEngineReady,
  networkEvidence,
  onArchive,
  onCreate,
  onCreateIdentity,
  onEdit,
  onLaunch,
  onRebindMihomo,
  onRecheckBrowser,
  onRecheckRuntime,
  onStop,
  sessions,
  silos,
  identityPreviews,
  recentRuns = {},
  recentRunsError = null,
  storageUsage,
}: {
  focusedSiloId?: string | undefined;
  onFocusSilo?: (id: string) => void;
  busy: boolean;
  siloBusyIds: Set<string>;
  launchingSiloIds: Set<string>;
  managedSessionLimit: number;
  managedEngineReady: boolean;
  networkEvidence: SiloNetworkEvidence[];
  onArchive: (silo: Silo) => Promise<void>;
  onCreate: () => void;
  onCreateIdentity?: (silo: Silo) => void;
  onEdit: (silo: Silo) => void;
  onLaunch: (silo: Silo) => Promise<void>;
  onRebindMihomo: (silo: Silo) => Promise<void>;
  onRecheckBrowser: (silo: Silo) => Promise<void>;
  onRecheckRuntime: (silo: Silo) => Promise<void>;
  onStop: (silo: Silo) => Promise<void>;
  sessions: RuntimeSessionStatus[];
  silos: Silo[];
  identityPreviews: Record<string, ManagedIdentityPreview>;
  recentRuns?: Record<string, RecentRunRecord>;
  recentRunsError?: string | null;
  storageUsage: Record<string, number | null>;
}) {
  const [selectedId, setSelectedId] = useState<string>();
  const [lens, setLens] = useState<LensKind | null>(null);
  const lensTrigger = useRef<HTMLButtonElement | null>(null);
  const selected =
    silos.find((silo) => silo.id === (focusedSiloId ?? selectedId))?.id ??
    silos[0]?.id;
  const selectedIndex = silos.findIndex((silo) => silo.id === selected);
  const currentSessions = sessions.filter(sessionNeedsManagement);
  const openLens = useCallback(
    (next: LensKind, trigger: HTMLButtonElement) => {
      lensTrigger.current = trigger;
      setLens(next);
      requestAnimationFrame(() =>
        document
          .getElementById(`lens-heading-${selected}`)
          ?.focus({ preventScroll: true }),
      );
    },
    [selected],
  );
  const closeLens = useCallback(() => {
    setLens(null);
    requestAnimationFrame(() => {
      const trigger = lensTrigger.current;
      if (trigger?.getClientRects().length)
        trigger.focus({ preventScroll: true });
      else
        document
          .getElementById(`identity-choice-${selected}`)
          ?.focus({ preventScroll: true });
    });
  }, [selected]);
  const choose = useCallback(
    (index: number, focus = false) => {
      const next = silos[Math.max(0, Math.min(index, silos.length - 1))];
      if (!next) return;
      setSelectedId(next.id);
      onFocusSilo?.(next.id);
      requestAnimationFrame(() => {
        const token = document.getElementById(`identity-choice-${next.id}`);
        token?.scrollIntoView({ block: "nearest", inline: "nearest" });
        if (focus) token?.focus({ preventScroll: true });
      });
    },
    [onFocusSilo, silos],
  );
  return (
    <section
      className={`identity-stage${lens ? " lens-open" : ""}`}
      aria-label="身份场"
    >
      <header className="stage-heading">
        <div>
          <p className="eyebrow">VERISILO / IDENTITY FIELD</p>
          <h1>你的身份，各有引力。</h1>
        </div>
        <div className="stage-travel">
          <span>
            {silos.length ? String(selectedIndex + 1).padStart(2, "0") : "00"}
            <i> / {String(silos.length).padStart(2, "0")}</i>
          </span>
          <button
            className="button-secondary"
            type="button"
            disabled={selectedIndex <= 0}
            aria-label="上一个身份"
            onClick={() => choose(selectedIndex - 1)}
          >
            ←
          </button>
          <button
            className="button-secondary"
            type="button"
            disabled={selectedIndex >= silos.length - 1}
            aria-label="下一个身份"
            onClick={() => choose(selectedIndex + 1)}
          >
            →
          </button>
          <button
            type="button"
            className="button-secondary stage-create"
            onClick={onCreate}
          >
            新建 Silo ＋
          </button>
        </div>
      </header>
      {silos.length === 0 ? (
        <div className="empty-field">
          <IdentityMark id="empty-field" color="#1553ff" />
          <div>
            <span className="eyebrow">A SPACE OF YOUR OWN</span>
            <h2>还没有 Silo</h2>
            <p>为工作、生活，或下一种可能，留一个独立空间。</p>
            <button onClick={onCreate} type="button">
              创建第一个 Silo
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="scene-stack">
            {silos.map((silo, index) => {
              const session = sessions.find((entry) => entry.siloId === silo.id);
              const runtimeActivation = session?.activation ?? idleRuntimeActivation;
              const isActive = session !== undefined && sessionNeedsManagement(session);
              const others = currentSessions.filter((entry) => entry.siloId !== silo.id);
              const peersSupported = canRunBesideManaged(silo) && others.every((entry) => {
                const peer = silos.find((candidate) => candidate.id === entry.siloId);
                return peer !== undefined && canRunBesideManaged(peer);
              });
              const launchBlockReason = isActive || others.length === 0 ? null
                : !peersSupported
                  ? "此 Silo 与当前会话不能并行。并发仅支持本地 Managed 的 Direct 或固定代理（不含 Clash/Mihomo）。"
                  : currentSessions.length >= managedSessionLimit
                    ? `本地 Managed 同时运行已达 ${managedSessionLimit} 个上限。请先停止其中一个。`
                    : null;
              return (
                <SiloScene
                  busy={busy || siloBusyIds.has(silo.id)}
                  choose={choose}
                  closeLens={closeLens}
                  identityPreview={identityPreviews[silo.id]}
                  recentRun={recentRuns[silo.id] ?? null}
                  recentRunsError={recentRunsError}
                  index={index}
                  isActive={isActive}
                  isLaunching={launchingSiloIds.has(silo.id)}
                  key={silo.id}
                  lens={lens}
                  managedEngineReady={managedEngineReady}
                  networkEvidence={networkEvidence}
                  onArchive={onArchive}
                  onCreateIdentity={onCreateIdentity}
                  onEdit={onEdit}
                  onLaunch={onLaunch}
                  onRebindMihomo={onRebindMihomo}
                  onRecheckBrowser={onRecheckBrowser}
                  onRecheckRuntime={onRecheckRuntime}
                  onStop={onStop}
                  openLens={openLens}
                  launchBlockReason={launchBlockReason}
                  runtimeActivation={runtimeActivation}
                  runtimeState={runtimeActivation.state}
                  selected={selected === silo.id}
                  silo={silo}
                  storageBytes={storageUsage[silo.id]}
                />
              );
            })}
          </div>
          <div className="identity-dock-wrap">
            <div
              className="identity-dock"
              role="group"
              aria-label="选择 Silo"
              onKeyDown={(event) => {
                if (
                  !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                    event.key,
                  )
                )
                  return;
                event.preventDefault();
                choose(
                  event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? silos.length - 1
                      : selectedIndex + (event.key === "ArrowRight" ? 1 : -1),
                  true,
                );
              }}
            >
              {silos.map((silo) => {
                const session = sessions.find((entry) => entry.siloId === silo.id);
                const isActive = session !== undefined && sessionNeedsManagement(session);
                const showRuntimeState = isActive || ["verification_failed", "failed"].includes(session?.activation.state ?? "idle");
                return (
                <button
                  type="button"
                  id={`identity-choice-${silo.id}`}
                  key={silo.id}
                  className="identity-token"
                  title={silo.name}
                  aria-pressed={selected === silo.id}
                  aria-controls={`silo-${silo.id}`}
                  tabIndex={selected === silo.id ? 0 : -1}
                  onClick={() => choose(silos.indexOf(silo))}
                >
                  <IdentityMark id={silo.id} color={silo.color} />
                  <span>
                    <strong>{silo.name}</strong>
                    <small>
                      {silo.engine.adapter === "stock"
                        ? "STANDARD"
                        : silo.engine.adapter === "camoufox"
                          ? "MANAGED"
                          : "CONTROLLED"}{" "}
                      ·{" "}
                      {showRuntimeState
                        ? activationStatusLabel(session!.activation.state)
                        : "未运行"}
                    </small>
                  </span>
                  {showRuntimeState && (
                    <i
                      aria-hidden="true"
                      className={`token-state ${session!.activation.state}`}
                    />
                  )}
                </button>
                );
              })}
            </div>
            <span className="recognition-note">
              轮廓用于辨认，状态来自证据。切换选择不改变正在运行的身份。
            </span>
          </div>
        </>
      )}
    </section>
  );
}

export function ArchivedSiloList({
  busy,
  onDelete,
  onRestore,
  silos,
  storageUsage,
}: {
  busy: boolean;
  onDelete: (silo: Silo) => Promise<void>;
  onRestore: (silo: Silo) => Promise<void>;
  silos: Silo[];
  storageUsage: Record<string, number | null>;
}) {
  if (silos.length === 0) {
    return null;
  }

  return (
    <section className="panel archived-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">已归档</p>
          <h2>数据仍在，只是不再出现在启动列表</h2>
          <p>恢复不会复制数据；永久删除才会移除这个 Silo 的浏览器数据。</p>
        </div>
      </div>
      <div className="archived-list">
        {silos.map((silo) => (
          <article className="archived-row" key={silo.id}>
            <IdentityMark id={silo.id} color={silo.color} />
            <div className="archived-copy">
              <strong>{silo.name}</strong>
              <span>
                {silo.archivedAt === null
                  ? "归档时间未知"
                  : `归档于 ${formatDate(silo.archivedAt)}`}
                {formatStorageSuffix(storageUsage[silo.id])}
              </span>
            </div>
            <div className="card-actions compact-actions">
              <button
                className="button-secondary"
                disabled={busy}
                onClick={() => void onRestore(silo)}
                type="button"
              >
                恢复
              </button>
              <button
                className="button-danger"
                disabled={busy}
                onClick={() => void onDelete(silo)}
                type="button"
              >
                永久删除
              </button>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
