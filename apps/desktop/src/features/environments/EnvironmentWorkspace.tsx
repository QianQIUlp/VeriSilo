import { type Silo } from "@verisilo/contracts";

import { useEffect, useState } from "react";

import {
  desktopApi,
  type EngineAdapterStatus,
  type RemoteEnvironmentStatus,
  type WslStatus,
} from "../../desktop-api.js";

import { errorMessage } from "../../shared/notice.js";

import {
  canConfigureWslDistribution,
  requiresExplicitWslSelection,
} from "../../wsl-selection.js";

import {
  engineAdapterLabel,
  engineHealthDescription,
  engineHealthLabel,
  environmentOperationLabel,
  remoteResultStateLabel,
  remoteStateLabel,
} from "../../shared/presentation.js";

import { InstrumentObject } from "../../shared/LivingWorkspace.js";

export function EnvironmentWorkspace({
  silos,
  vaultLocked,
}: {
  silos: Silo[];
  vaultLocked: boolean;
}) {
  const [locationOpen, setLocationOpen] = useState(false);
  const [inventoryLoading, setInventoryLoading] = useState(!vaultLocked);
  const [inventoryRetry, setInventoryRetry] = useState(0);
  const [environmentSection, setEnvironmentSection] =
    useState<EnvironmentSection>("browser");
  const [wslStatus, setWslStatus] = useState<WslStatus | null>(null);
  const [wslBusy, setWslBusy] = useState(false);
  const [selectedWslDistribution, setSelectedWslDistribution] = useState("");
  const [engineStatuses, setEngineStatuses] = useState<EngineAdapterStatus[]>(
    [],
  );
  const [technologyError, setTechnologyError] = useState<string | null>(null);
  const [remoteStatus, setRemoteStatus] =
    useState<RemoteEnvironmentStatus | null>(null);
  const [selectedRemoteSilo, setSelectedRemoteSilo] = useState("");
  const [remoteBusy, setRemoteBusy] = useState(false);
  const [remoteActionMessage, setRemoteActionMessage] = useState<{
    tone: "success" | "error";
    text: string;
    detail?: string;
  } | null>(null);

  useEffect(() => {
    if (vaultLocked) {
      setEngineStatuses([]);
      setRemoteStatus(null);
      setTechnologyError(null);
      return;
    }
    let current = true;
    setInventoryLoading(true);
    void Promise.allSettled([
      desktopApi.listEngineAdapters(),
      desktopApi.remoteEnvironmentStatus(),
    ]).then(([engines, remote]) => {
      if (!current) return;
      setEngineStatuses(engines.status === "fulfilled" ? engines.value : []);
      if (remote.status === "fulfilled") {
        setRemoteStatus(remote.value);
      }
      setTechnologyError(
        [engines, remote].some(
          (result) => result.status === "rejected",
        )
          ? "部分运行环境的状态暂时无法读取，请稍后重试。"
          : null,
      );
      setInventoryLoading(false);
    });
    return () => {
      current = false;
    };
  }, [vaultLocked, inventoryRetry]);

  useEffect(() => {
    if (vaultLocked) {
      return;
    }
    const firstBinding = remoteStatus?.bindings[0];
    const selectionHasBinding = remoteStatus?.bindings.some(
      (binding) => binding.siloId === selectedRemoteSilo,
    );
    if (firstBinding !== undefined && !selectionHasBinding) {
      setSelectedRemoteSilo(firstBinding.siloId);
    } else if (selectedRemoteSilo === "" && silos[0] !== undefined) {
      setSelectedRemoteSilo(firstBinding?.siloId ?? silos[0].id);
    }
  }, [remoteStatus, selectedRemoteSilo, silos, vaultLocked]);

  useEffect(() => {
    if (
      environmentSection === "remote" &&
      (remoteStatus?.bindings.length ?? 0) === 0
    ) {
      setEnvironmentSection("browser");
    }
  }, [environmentSection, remoteStatus?.bindings.length]);

  const checkWsl = async () => {
    setWslBusy(true);
    try {
      const detected = await desktopApi.detectWsl();
      setWslStatus(detected);
      setSelectedWslDistribution("");
    } catch (error) {
      setWslStatus({
        supportedPlatform: false,
        available: false,
        distributions: [],
        message: errorMessage(error),
      });
    } finally {
      setWslBusy(false);
    }
  };

  const configureWslDistribution = async () => {
    if (
      wslStatus === null ||
      !canConfigureWslDistribution(
        wslStatus.distributions,
        selectedWslDistribution,
      )
    ) {
      return;
    }
    setWslBusy(true);
    try {
      await desktopApi.selectWslEnvironmentDistribution(
        selectedWslDistribution,
      );
    } catch (error) {
      setTechnologyError(errorMessage(error));
    } finally {
      setWslBusy(false);
    }
  };

  const refreshRemoteStatus = async () => {
    const next = await desktopApi.remoteEnvironmentStatus();
    setRemoteStatus(next);
    return next;
  };

  const checkRemoteDeletionStatus = async () => {
    if (selectedRemoteBinding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个仍保留连接记录的 Silo。",
      });
      return;
    }
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      await desktopApi.recoverRemoteDeletionProof(selectedRemoteBinding.siloId);
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: "已确认远程环境完成删除，并移除了这台电脑上的连接记录。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: errorMessage(
          error,
          "暂时无法确认远程环境已删除。请先恢复连接，或向远程服务运营者核实。",
        ),
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const removeLocalRemoteConnection = async () => {
    if (selectedRemoteBinding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个仍保留连接记录的 Silo。",
      });
      return;
    }
    const selectedSilo = silos.find(
      (silo) => silo.id === selectedRemoteBinding.siloId,
    );
    if (
      !window.confirm(
        `Force Detach「${selectedSilo?.name ?? "所选 Silo"}」？这只会移除本机连接记录，不会删除远程环境；它可能仍在运行并继续产生费用。请确认你已阅读此风险。`,
      )
    ) {
      return;
    }
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      setRemoteStatus(
        await desktopApi.forceDetachRemoteEnvironment(
          selectedRemoteBinding.siloId,
        ),
      );
      setRemoteActionMessage({
        tone: "success",
        text: "已移除这台电脑上的连接记录。远程环境没有被删除，请按需联系运营者完成清理。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: errorMessage(error, "暂时无法移除本地连接记录，请稍后重试。"),
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const runRemoteCleanupOperation = async (
    operation: "stop" | "health" | "logs" | "destroy",
  ) => {
    const binding = remoteStatus?.bindings.find(
      (candidate) => candidate.siloId === selectedRemoteSilo,
    );
    if (binding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个仍保留连接记录的远程环境。",
      });
      return;
    }
    const silo = silos.find((candidate) => candidate.id === binding.siloId);
    if (silo === undefined) {
      setRemoteActionMessage({
        tone: "error",
        text: "找不到这个远程环境对应的本地 Silo，不能安全执行清理。",
      });
      return;
    }
    if (
      operation === "destroy" &&
      !window.confirm(
        `确认删除「${silo.name}」的远程环境？这会联系远程服务；成功后本机连接记录也会移除。`,
      )
    ) {
      return;
    }

    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      const result =
        operation === "stop"
          ? await desktopApi.stopRemoteEnvironment(silo.id)
          : operation === "health"
            ? await desktopApi.healthRemoteEnvironment(silo.id)
            : operation === "logs"
              ? await desktopApi.logsRemoteEnvironment(silo.id, null, 50)
              : await desktopApi.destroyRemoteEnvironment(silo.id);
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: `${environmentOperationLabel(operation)}完成：${remoteResultStateLabel(result.state)}。`,
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: errorMessage(error, "远程清理操作没有完成，请检查连接后重试。"),
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const selectedRemoteBinding = remoteStatus?.bindings.find(
    (binding) => binding.siloId === selectedRemoteSilo,
  );
  const selectedRemoteResult = remoteStatus?.lastResults.find(
    (result) => result.siloId === selectedRemoteSilo,
  );
  return (
    <section
      className={`location-observatory living-page${locationOpen ? " location-open" : ""}`}
    >
      <header className="living-heading">
        <p className="eyebrow">04 / EXECUTION MAP</p>
        <h1>每个身份，都有落点。</h1>
        <p>在地图里查看浏览器与可选运行位置，进入后准备或修复。</p>
      </header>
      {technologyError !== null && (
        <div className="location-read-error" role="alert">
          {technologyError}
          <button
            type="button"
            className="button-secondary"
            disabled={inventoryLoading}
            onClick={() => setInventoryRetry((value) => value + 1)}
          >
            重新读取状态
          </button>
        </div>
      )}
      <nav className="location-map" aria-label="运行位置设置类别">
        <div className="map-connection" aria-hidden="true" />
        <button
          className="map-place place-browser"
          aria-pressed={environmentSection === "browser" && locationOpen}
          aria-expanded={environmentSection === "browser" && locationOpen}
          aria-controls="location-console"
          onClick={() => {
            setEnvironmentSection("browser");
            setLocationOpen(true);
          }}
          type="button"
        >
          <InstrumentObject
            kind="browser"
            active={environmentSection === "browser" && locationOpen}
          />
          <span className="place-index">01 / WINDOWS</span>
          <strong>浏览器准备</strong>
          <small>
            {vaultLocked
              ? "解锁后读取浏览器状态"
              : inventoryLoading
                ? "正在读取…"
                : `${engineStatuses.length} 个引擎状态`}
          </small>
          <i aria-hidden="true">进入 ↗</i>
        </button>
        <button
          className="map-place place-linux"
          aria-pressed={environmentSection === "local" && locationOpen}
          aria-expanded={environmentSection === "local" && locationOpen}
          aria-controls="location-console"
          onClick={() => {
            setEnvironmentSection("local");
            setLocationOpen(true);
          }}
          type="button"
        >
          <InstrumentObject
            kind="linux"
            active={environmentSection === "local" && locationOpen}
          />
          <span className="place-index">02 / LOCAL LINUX</span>
          <strong>Linux 环境</strong>
          <small>
            {wslStatus === null
              ? "进入后检查本机 WSL"
              : wslStatus.available
                ? `${wslStatus.distributions.length} 个发行版`
                : "尚不可用"}
          </small>
          <i aria-hidden="true">进入 ↗</i>
        </button>
        {(remoteStatus?.bindings.length ?? 0) > 0 && (
          <button
            className="map-place place-remote"
            aria-pressed={environmentSection === "remote" && locationOpen}
            aria-expanded={environmentSection === "remote" && locationOpen}
            aria-controls="location-console"
            onClick={() => {
              setEnvironmentSection("remote");
              setLocationOpen(true);
            }}
            type="button"
          >
            <InstrumentObject
              kind="remote"
              active={environmentSection === "remote" && locationOpen}
            />
            <span className="place-index">03 / EXISTING REMOTE</span>
            <strong>旧远程环境</strong>
            <small>{remoteStatus?.bindings.length} 个已有连接</small>
            <i aria-hidden="true">进入 ↗</i>
          </button>
        )}
      </nav>
      {!locationOpen && (
        <p className="map-boundary">
          选择一个落点进入查看。Silo 的运行位置仍在创建时选择。
        </p>
      )}
      <div
        id="location-console"
        className="location-console"
        hidden={!locationOpen}
      >
        <div className="location-console-bar">
          <span>
            {environmentSection === "browser"
              ? "WINDOWS / 浏览器准备"
              : environmentSection === "local"
                ? "LOCAL / Linux 环境"
                : "REMOTE / 已有远程环境"}
          </span>
          <button
            type="button"
            className="button-secondary"
            onClick={() => setLocationOpen(false)}
          >
            返回地图 ↗
          </button>
        </div>
        <div className="location-console-body" key={environmentSection}>
          {environmentSection === "browser" ? (
            <section className="panel provider-catalog">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">浏览器</p>
                  <h2>管理 Silo 可以使用的浏览器</h2>
                  <p>
                    已安装的 Chrome 和 Edge 可以直接使用。已有 Silo
                    使用独立浏览器时，也可以在这里查看并维护对应组件。
                  </p>
                </div>
              </div>
              <div className="provider-status-grid">
                {engineStatuses.map((engine) => (
                  <article
                    className="provider-status-card"
                    key={engine.descriptor.id}
                  >
                    <div>
                      <strong>
                        {engineAdapterLabel(engine.descriptor.id)}
                      </strong>
                      <span
                        className={`provider-health ${engine.health.state}`}
                      >
                        {engineHealthLabel(engine.health.state)}
                      </span>
                    </div>
                    <p>{engineHealthDescription(engine.health.state)}</p>
                    <small>
                      {engine.descriptor.externallyPackaged
                        ? "通过完整性检查后才会用于新的浏览会话。"
                        : "由浏览器供应商更新；VeriSilo 使用这台电脑上已安装的版本。"}
                    </small>
                  </article>
                ))}
                {engineStatuses.length === 0 ? (
                  <p className="empty-provider-copy">尚未发现可用浏览器。</p>
                ) : null}
              </div>
            </section>
          ) : null}

          {environmentSection === "remote" &&
          (remoteStatus?.bindings.length ?? 0) > 0 ? (
            <section className="panel provider-catalog remote-provider-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">旧远程环境</p>
                  <h2>只处理已经存在的远程环境</h2>
                  <p>
                    这台电脑仍保留远程环境连接记录。此处只提供停止、检查、日志和删除，
                    不会创建新环境，也不会打开远程浏览器控制。
                  </p>
                </div>
                <span
                  className={`provider-health ${remoteStatus?.state === "paired" ? "healthy" : "unavailable"}`}
                >
                  {remoteStatus === null
                    ? "状态未知"
                    : remoteStateLabel(remoteStatus.state)}
                </span>
              </div>
              <p className="remote-recovery-warning">
                连接状态不会改变这个界面的权限。配对有效、过期或已取消时，都只能清理旧环境；
                不能在这里重新配对、启动或交互。
              </p>
              <label>
                Silo
                <select
                  disabled={remoteBusy || vaultLocked}
                  onChange={(event) => {
                    setSelectedRemoteSilo(event.target.value);
                    setRemoteActionMessage(null);
                  }}
                  value={selectedRemoteSilo}
                >
                  {remoteStatus?.bindings.map((binding) => {
                    const silo = silos.find(
                      (candidate) => candidate.id === binding.siloId,
                    );
                    return (
                      <option key={binding.siloId} value={binding.siloId}>
                        {silo?.name ?? "已移除的本地 Silo"}
                      </option>
                    );
                  })}
                </select>
              </label>
              {selectedRemoteBinding !== undefined ? (
                <div className="remote-selected-state">
                  <dl className="remote-binding-facts">
                    <div>
                      <dt>服务地址</dt>
                      <dd>{selectedRemoteBinding.endpoint.origin}</dd>
                    </div>
                    <div>
                      <dt>网络</dt>
                      <dd>
                        {selectedRemoteBinding.network.mode === "direct"
                          ? "直连"
                          : "使用远程代理"}
                      </dd>
                    </div>
                    <div>
                      <dt>存储</dt>
                      <dd>已加密</dd>
                    </div>
                    <div>
                      <dt>最近活动</dt>
                      <dd>
                        {new Date(
                          selectedRemoteBinding.lastActivityAtUnixMs,
                        ).toLocaleString("zh-CN")}
                      </dd>
                    </div>
                  </dl>
                  <div className="environment-operation-grid remote-operation-grid">
                    <button
                      className="button-secondary"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void runRemoteCleanupOperation("stop")}
                      type="button"
                    >
                      停止远程环境
                    </button>
                    <button
                      className="button-secondary"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void runRemoteCleanupOperation("health")}
                      type="button"
                    >
                      检查状态
                    </button>
                    <button
                      className="button-secondary"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void runRemoteCleanupOperation("logs")}
                      type="button"
                    >
                      查看日志
                    </button>
                    <button
                      className="button-danger"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void runRemoteCleanupOperation("destroy")}
                      type="button"
                    >
                      删除远程环境
                    </button>
                  </div>
                  {selectedRemoteResult !== undefined ? (
                    <div className="remote-result-card">
                      <div>
                        <span>最近一次清理结果</span>
                        <strong>
                          {environmentOperationLabel(
                            selectedRemoteResult.operation,
                          )}
                          ：{remoteResultStateLabel(selectedRemoteResult.state)}
                        </strong>
                      </div>
                      {selectedRemoteResult.logs !== undefined ? (
                        <ul className="remote-log-list">
                          {selectedRemoteResult.logs.map((log) => (
                            <li key={log.sequence}>
                              <span>{log.level}</span>
                              <code>{log.message}</code>
                            </li>
                          ))}
                        </ul>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="remote-proof-recovery">
                    <strong>远程服务已确认删除时</strong>
                    <p>
                      只验证远程服务提供的删除证明，并移除本机连接记录；不会重新创建或启动环境。
                    </p>
                    <button
                      className="button-secondary"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void checkRemoteDeletionStatus()}
                      type="button"
                    >
                      验证远程删除证明
                    </button>
                  </div>
                  <div className="remote-force-detach">
                    <strong>无法连接时的最后手段</strong>
                    <p>
                      Force Detach
                      只移除这台电脑上的连接记录，不会删除远程环境。
                      远程环境可能继续运行并产生费用，请先联系远程服务运营者。
                    </p>
                    <button
                      className="button-danger"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void removeLocalRemoteConnection()}
                      type="button"
                    >
                      Force Detach：仅移除本机记录
                    </button>
                  </div>
                </div>
              ) : null}
              {remoteActionMessage !== null ? (
                <p
                  className={`environment-action-message ${remoteActionMessage.tone}`}
                  role={
                    remoteActionMessage.tone === "error" ? "alert" : "status"
                  }
                >
                  {remoteActionMessage.text}
                  {remoteActionMessage.detail !== undefined ? (
                    <span className="error-detail">
                      {remoteActionMessage.detail}
                    </span>
                  ) : null}
                </p>
              ) : null}
            </section>
          ) : null}

          {environmentSection === "local" ? (
            <section className="panel provider-readiness">
              <div>
                <p className="eyebrow">WSL 设置</p>
                <h2>选择要用于 Silo 的 Linux 环境</h2>
                <p>检查这台电脑已安装的 WSL 发行版，然后选择一个供本次使用。</p>
                {wslStatus !== null ? (
                  <div className="provider-result">
                    <strong>
                      {wslStatus.available ? "发现 WSL" : "尚不可用"}
                    </strong>
                    <span>
                      {wslStatus.available
                        ? `发现 ${wslStatus.distributions.length} 个可选发行版。`
                        : "请先在 Windows 中安装并启用 WSL。"}
                    </span>
                    {wslStatus.distributions.length > 0 ? (
                      <fieldset className="distribution-picker">
                        <legend>Linux 发行版</legend>
                        {wslStatus.distributions.map((distribution) => (
                          <label
                            className="distribution-option"
                            key={distribution}
                          >
                            <input
                              type="radio"
                              name="workspace-wsl-distribution"
                              disabled={wslBusy}
                              checked={selectedWslDistribution === distribution}
                              onChange={() =>
                                setSelectedWslDistribution(distribution)
                              }
                              value={distribution}
                            />
                            <span aria-hidden="true">◇</span>
                            <strong>{distribution}</strong>
                            <small>
                              {selectedWslDistribution === distribution
                                ? "已选择 · 等待应用"
                                : "选择此落点"}
                            </small>
                          </label>
                        ))}
                      </fieldset>
                    ) : null}
                    <button
                      className="button-secondary"
                      disabled={
                        wslBusy ||
                        !canConfigureWslDistribution(
                          wslStatus.distributions,
                          selectedWslDistribution,
                        )
                      }
                      onClick={() => void configureWslDistribution()}
                      type="button"
                    >
                      使用此发行版
                    </button>
                  </div>
                ) : null}
              </div>
              <button
                className="button-secondary"
                disabled={wslBusy}
                onClick={() => void checkWsl()}
                type="button"
              >
                {wslBusy ? "正在检查…" : "检查本机 WSL"}
              </button>
            </section>
          ) : null}
        </div>
      </div>
    </section>
  );
}

type EnvironmentSection = "browser" | "local" | "remote";
