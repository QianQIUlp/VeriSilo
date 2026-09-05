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
  ManagedStatusGroups,
} from "../identity/IdentityDetails.js";

import { CapabilityState } from "../../shared/components.js";

import { describeNetwork } from "../../formatters.js";

export function SiloList({
  activation,
  busy,
  managedEngineReady,
  networkEvidence,
  onArchive,
  onCreate,
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
  return (
    <section className="panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">我的 Silo</p>
          <h2>选择一个浏览器空间</h2>
          <p>
            换一个空间就是换一套浏览器数据，不只是换 Cookie。一次只打开一个。
          </p>
        </div>
        <button className="button-secondary" onClick={onCreate} type="button">
          新建
        </button>
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
        <div className="silo-grid">
          {silos.map((silo) => {
            const managedCamoufox = silo.engine.adapter === "camoufox";
            const identityPreview = identityPreviews[silo.id];
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
              <article className="silo-card" key={silo.id}>
                <div className="silo-heading">
                  <span
                    className="silo-mark"
                    style={{ backgroundColor: silo.color }}
                  >
                    {silo.name.slice(0, 1).toUpperCase()}
                  </span>
                  <div>
                    <h3>{silo.name}</h3>
                    <p>{siloBrowserLabel(silo)}</p>
                  </div>
                  {activation === silo.id ? (
                    <span className="running-badge">
                      {runtimeState === "verification_failed"
                        ? "已结束"
                        : runtimeState === "recovery_required"
                          ? "待确认"
                          : "运行中"}
                    </span>
                  ) : null}
                </div>
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
                <div className="card-actions">
                  <button
                    disabled={
                      busy ||
                      (activation !== null && activation !== silo.id) ||
                      (activation === silo.id && !canStop && !canClear)
                    }
                    onClick={() =>
                      void (canStop || canClear ? onStop(silo) : onLaunch(silo))
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
                          : "检查状态"}
                    </button>
                  ) : (
                    <button
                      className="button-secondary"
                      disabled={busy || runtimeState === "verification_failed"}
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
              </article>
            );
          })}
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
            <span className="silo-mark" style={{ backgroundColor: silo.color }}>
              {silo.name.slice(0, 1).toUpperCase()}
            </span>
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
