import { useState } from "react";
import { IdentityMark } from "../../shared/IdentityMark.js";
import {
  type DesktopStatus,
  type ManagedIdentityPreview,
  type SiloNetworkEvidence,
} from "../../desktop-api.js";

import { type Silo } from "@verisilo/contracts";

import {
  formatDate,
  formatStorageSuffix,
  siloBrowserLabel,
  siloExecutionTargetLabel,
  siloWebsiteIdentityBoundary,
} from "../../shared/presentation.js";

import {
  ManagedIdentityFacts,
  ManagedIdentityEvidence,
  ManagedStatusGroups,
} from "../identity/IdentityDetails.js";

import { CurrentSessionIntegrity } from "../identity/CurrentSessionIntegrity.js";

import { CapabilityState } from "../../shared/components.js";

import { activationStatusLabel, describeNetwork } from "../../formatters.js";

export function SiloList({
  activation,
  busy,
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
  runtimeActivation,
  runtimeState,
  silos,
  identityPreviews,
  storageUsage,
}: {
  activation: string | null;
  busy: boolean;
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
  runtimeActivation: DesktopStatus["activation"];
  runtimeState: DesktopStatus["activation"]["state"];
  silos: Silo[];
  identityPreviews: Record<string, ManagedIdentityPreview>;
  storageUsage: Record<string, number | null>;
}) {
  const [selectedId, setSelectedId] = useState<string>();
  const selected =
    silos.find((silo) => silo.id === selectedId)?.id ??
    silos.find((silo) => silo.id === activation)?.id ??
    silos[0]?.id;
  return (
    <section className="atlas-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">我的 Silo</p>
          <h2>
            我的身份空间{" "}
            <span className="collection-count">{silos.length}</span>
          </h2>
          <p>每个轮廓，都是一个熟悉的空间。一次只打开一个 Silo。</p>
        </div>
      </div>
      {silos.length === 0 ? (
        <div className="empty-silos">
          <strong>还没有 Silo</strong>
          <p>创建一个工作、个人或临时用途的独立浏览器环境。</p>
          <button onClick={onCreate} type="button">
            创建第一个 Silo
          </button>
        </div>
      ) : (
        <div className="identity-workspace">
          <div className="identity-index" role="group" aria-label="选择 Silo">
            {silos.map((silo) => (
              <button
                type="button"
                key={silo.id}
                className="identity-index-item"
                aria-pressed={selected === silo.id}
                aria-controls={`silo-${silo.id}`}
                onClick={() => setSelectedId(silo.id)}
              >
                <IdentityMark id={silo.id} color={silo.color} />
                <span className="index-copy">
                  <strong>{silo.name}</strong>
                  <small>
                    {silo.engine.adapter === "stock"
                      ? "STANDARD"
                      : silo.engine.adapter === "camoufox"
                        ? "MANAGED"
                        : "CONTROLLED"}{" "}
                    ·{" "}
                    {silo.networkProfile.mode === "direct"
                      ? "直连"
                      : silo.networkProfile.mode === "pac"
                        ? "PAC"
                        : "代理"}
                  </small>
                  <span
                    className={`index-state ${activation === silo.id ? runtimeState : "idle"}`}
                  >
                    {activation === silo.id
                      ? activationStatusLabel(runtimeState)
                      : "未运行"}
                  </span>
                  {runtimeActivation.identityEvidence?.siloId === silo.id ? (
                    <small className="index-evidence">
                      Identity · {runtimeActivation.identityEvidence.state}
                    </small>
                  ) : null}
                </span>
                <span className="index-arrow" aria-hidden="true">
                  ↗
                </span>
              </button>
            ))}
            <p className="identity-index-note">
              轮廓仅用于辨认 Silo。
              <br />
              身份状态由实际证据呈现。
            </p>
          </div>
          <div className="silo-grid">
            {silos.map((silo) => {
              const managedCamoufox = silo.engine.adapter === "camoufox";
              const identityPreview = identityPreviews[silo.id];
              const blockedByRunning =
                activation !== null && activation !== silo.id;
              const runningSilo = blockedByRunning
                ? silos.find((candidate) => candidate.id === activation)
                : undefined;
              const canStop =
                activation === silo.id &&
                runtimeState === "running" &&
                (silo.executionTarget.kind === "wsl" || managedCamoufox);
              const canClear =
                activation === silo.id &&
                managedCamoufox &&
                ["verification_failed", "failed", "recovery_required"].includes(
                  runtimeState,
                );
              return (
                <article
                  className={`silo-card ${activation === silo.id ? `is-active state-${runtimeState}` : ""}`}
                  key={silo.id}
                  id={`silo-${silo.id}`}
                  hidden={selected !== silo.id}
                  aria-label={silo.name}
                >
                  <div className="silo-portrait">
                    <IdentityMark id={silo.id} color={silo.color} />
                    <div className="portrait-caption">
                      <span>IDENTITY LANDSCAPE</span>
                      <span>
                        {silo.engine.adapter === "stock"
                          ? "STANDARD / 独立数据"
                          : silo.engine.adapter === "camoufox"
                            ? "MANAGED / 托管身份"
                            : "CONTROLLED / 受控浏览器"}
                      </span>
                    </div>
                  </div>
                  <div className="silo-heading">
                    <div>
                      <h3>{silo.name}</h3>
                      <p>
                        {siloBrowserLabel(silo)} <span> / </span>{" "}
                        {siloExecutionTargetLabel(silo)}
                      </p>
                    </div>
                    <span
                      className={`running-badge ${activation === silo.id ? runtimeState : "idle"}`}
                    >
                      {activation === silo.id
                        ? activationStatusLabel(runtimeState)
                        : "未运行"}
                    </span>
                  </div>
                  {silo.engine.adapter === "stock" ? (
                    <p className="standard-boundary">
                      网站数据独立保存 · 设备身份跟随本机
                    </p>
                  ) : null}
                  <div className="silo-at-a-glance">
                    <span>网络策略</span>
                    <strong>{describeNetwork(silo.networkProfile)}</strong>
                  </div>
                  <div className="card-actions">
                    <button
                      disabled={
                        busy ||
                        blockedByRunning ||
                        (activation === silo.id && !canStop && !canClear)
                      }
                      onClick={() =>
                        void (canStop || canClear
                          ? onStop(silo)
                          : onLaunch(silo))
                      }
                      title={
                        blockedByRunning ? "一次只能打开一个 Silo" : undefined
                      }
                      type="button"
                    >
                      {canStop
                        ? "停止"
                        : canClear
                          ? "结束会话"
                          : activation === silo.id &&
                              silo.executionTarget.kind === "local" &&
                              !managedCamoufox
                            ? "关掉窗口即可停止"
                            : "打开浏览器"}
                    </button>
                    <button
                      className="button-secondary"
                      disabled={busy || activation === silo.id}
                      onClick={() => onEdit(silo)}
                      type="button"
                    >
                      编辑
                    </button>
                    {managedCamoufox &&
                    identityPreview !== undefined &&
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
                      disabled={busy || activation === silo.id}
                      onClick={() => void onArchive(silo)}
                      type="button"
                    >
                      归档
                    </button>
                    {activation === silo.id ? (
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
                        disabled={
                          busy || runtimeState === "verification_failed"
                        }
                        onClick={() => void onRecheckBrowser(silo)}
                        type="button"
                      >
                        检查浏览器
                      </button>
                    )}
                    {activation === silo.id &&
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
                  {managedCamoufox ? (
                    <ManagedIdentityEvidence
                      activation={runtimeActivation}
                      silo={silo}
                    />
                  ) : null}
                  {managedCamoufox && activation === silo.id ? (
                    <CurrentSessionIntegrity
                      activation={runtimeActivation}
                      managedEngineReady={managedEngineReady}
                      silo={silo}
                    />
                  ) : null}
                  <details className="silo-configuration">
                    <summary>
                      身份与配置 <span>Profile · 设备 · 网络</span>
                    </summary>
                    <dl className="silo-facts">
                      <div>
                        <dt>网站数据</dt>
                        <dd>
                          登录和 Cookie 单独保存
                          {formatStorageSuffix(storageUsage[silo.id])}
                        </dd>
                      </div>
                      <div>
                        <dt>运行位置</dt>
                        <dd>{siloExecutionTargetLabel(silo)}</dd>
                      </div>
                      <div>
                        <dt>对外身份</dt>
                        <dd>
                          {siloWebsiteIdentityBoundary(silo, identityPreview)}
                        </dd>
                      </div>
                      {identityPreview !== undefined ? (
                        <ManagedIdentityFacts preview={identityPreview} />
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
                              silo.identityLockedAt === null
                                ? " pending"
                                : " locked"
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
                  </details>
                  {silo.engine.adapter !== "stock" ? (
                    <ManagedStatusGroups
                      activation={runtimeActivation}
                      evidence={networkEvidence}
                      engineHealthy={managedEngineReady}
                      runtimeState={
                        activation === silo.id ? runtimeState : "idle"
                      }
                      silo={silo}
                    />
                  ) : null}
                  {activation === silo.id &&
                  silo.executionTarget.kind === "local" &&
                  silo.engine.adapter === "stock" ? (
                    <p className="local-runtime-guidance">
                      用完后直接关掉这个浏览器窗口即可。不会动你其他的 Chrome 或
                      Edge 窗口。
                    </p>
                  ) : null}
                  {blockedByRunning ? (
                    <p className="mutex-note" role="note">
                      「{runningSilo?.name ?? "另一个 Silo"}
                      」正在运行。一次只能打开一个
                      Silo——先关闭它的浏览器窗口，再回来打开这个。
                    </p>
                  ) : null}
                </article>
              );
            })}
          </div>
        </div>
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
