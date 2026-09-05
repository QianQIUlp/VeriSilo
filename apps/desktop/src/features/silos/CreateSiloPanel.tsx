import {
  type BrowserKind,
  type NetworkProfile,
  type SiloExecutionTarget,
} from "@verisilo/contracts";

import {
  type BrowserCandidate,
  type CreateManagedSiloInput,
  type MihomoSnapshot,
  type WslStatus,
} from "../../desktop-api.js";

import {
  type CreateMode,
  emptyNetwork,
  type WslCreationOption,
} from "../../shared/defaults.js";

import { useEffect, useState } from "react";

import {
  isLoopbackProxyProfile,
  localMihomoProfile,
} from "../../proxy-presets.js";

import { ManagedSiloForm } from "../identity/ManagedSiloForm.js";

import { describeNetwork } from "../../formatters.js";

import { CapabilityState, NetworkOption } from "../../shared/components.js";

import { formatDate } from "../../shared/presentation.js";

export function CreateSiloPanel({
  browserKind,
  browserPath,
  busy,
  candidateOptions,
  chooseBrowser,
  color,
  createSilo,
  createManagedSilo,
  executionTarget,
  importProxy,
  inspectMihomoController,
  mihomoBusy,
  mihomoControllerSecret,
  mihomoControllerUrl,
  mihomoSnapshot,
  managedEngineReady,
  managedStatusBusy,
  managedStatusError,
  name,
  networkProfile,
  proxyImport,
  proxyPassword,
  proxyUsername,
  refreshManagedStatus,
  refreshWsl,
  resetMihomoSnapshot,
  selectMihomoGroup,
  selectMihomoNode,
  setBrowserKind,
  setBrowserPath,
  setColor,
  setExecutionTarget,
  setMihomoControllerSecret,
  setMihomoControllerUrl,
  setName,
  setNetworkProfile,
  setProxyImport,
  setProxyPassword,
  setProxyUsername,
  wslBusy,
  wslOptions,
  wslStatus,
}: {
  browserKind: BrowserKind;
  browserPath: string;
  busy: boolean;
  candidateOptions: BrowserCandidate[];
  chooseBrowser: (candidate: BrowserCandidate) => void;
  color: string;
  createSilo: () => Promise<void>;
  createManagedSilo: (input: CreateManagedSiloInput) => Promise<void>;
  executionTarget: SiloExecutionTarget;
  importProxy: () => void;
  inspectMihomoController: () => Promise<void>;
  mihomoBusy: boolean;
  mihomoControllerSecret: string;
  mihomoControllerUrl: string;
  mihomoSnapshot: MihomoSnapshot | null;
  managedEngineReady: boolean;
  managedStatusBusy: boolean;
  managedStatusError: string | null;
  name: string;
  networkProfile: NetworkProfile;
  proxyImport: string;
  proxyPassword: string;
  proxyUsername: string;
  refreshManagedStatus: () => Promise<void>;
  refreshWsl: () => Promise<void>;
  resetMihomoSnapshot: () => void;
  selectMihomoGroup: (groupName: string) => void;
  selectMihomoNode: (nodeName: string) => void;
  setBrowserKind: (kind: BrowserKind) => void;
  setBrowserPath: (path: string) => void;
  setColor: (color: string) => void;
  setExecutionTarget: (target: SiloExecutionTarget) => void;
  setMihomoControllerSecret: (secret: string) => void;
  setMihomoControllerUrl: (url: string) => void;
  setName: (name: string) => void;
  setNetworkProfile: (profile: NetworkProfile) => void;
  setProxyImport: (value: string) => void;
  setProxyPassword: (value: string) => void;
  setProxyUsername: (value: string) => void;
  wslBusy: boolean;
  wslOptions: WslCreationOption[];
  wslStatus: WslStatus | null;
}) {
  const [websiteBoundaryConfirmed, setWebsiteBoundaryConfirmed] =
    useState(false);
  const [creationMode, setCreationMode] = useState<CreateMode>("standard");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const localProxySelected = isLoopbackProxyProfile(networkProfile);
  const mihomoBinding =
    networkProfile.mode === "fixed_proxy"
      ? networkProfile.externalMihomo
      : undefined;
  const selectedMihomoGroup = mihomoSnapshot?.groups.find(
    (group) => group.name === mihomoBinding?.selectorGroup,
  );
  useEffect(() => {
    setWebsiteBoundaryConfirmed(false);
  }, [browserKind, browserPath, executionTarget, networkProfile]);

  const localExecution = executionTarget.kind === "local";
  const readyWslOptions = wslOptions.filter((option) => option.ready);
  const unavailableWslCount = wslOptions.length - readyWslOptions.length;
  const selectedBrowserCandidate = candidateOptions.find(
    (candidate) => candidate.executablePath === browserPath,
  );
  const standardDefaultsSelected =
    localExecution &&
    networkProfile.mode === "direct" &&
    selectedBrowserCandidate?.kind === browserKind;
  return (
    <>
      <section className="create-hero panel">
        <p className="eyebrow">新环境</p>
        <h1>创建一个独立的浏览器空间</h1>
        <p>
          VeriSilo 会为每个 Silo 保存独立的网站数据，并关闭浏览器同步；
          不会读取或修改默认浏览器的数据。
        </p>
        <div className="assurance-row">
          <span>Cookie 与站点数据独立</span>
          <span>浏览器同步已关闭</span>
          <span>看起来像这台电脑</span>
        </div>
      </section>

      <section
        aria-busy={managedStatusBusy}
        aria-label="选择浏览器方式"
        className="panel browser-mode-panel"
      >
        <div className="panel-heading">
          <div>
            <p className="eyebrow">浏览器方式</p>
            <h2>选择要创建的浏览器</h2>
            <p>系统浏览器跟这台电脑长得一样；独立浏览器可以换一套对外身份。</p>
          </div>
        </div>
        <div
          className="browser-mode-grid"
          role="radiogroup"
          aria-label="浏览器方式"
        >
          <button
            aria-checked={creationMode === "standard"}
            className={`browser-mode-card${creationMode === "standard" ? " selected" : ""}`}
            onClick={() => setCreationMode("standard")}
            role="radio"
            type="button"
          >
            <strong>系统浏览器</strong>
            <span>使用这台电脑上已安装的 Chrome 或 Edge。</span>
            <small>登录数据单独保存，看起来仍像这台电脑。</small>
          </button>
          <button
            aria-checked={creationMode === "managed"}
            aria-describedby="managed-browser-availability"
            className={`browser-mode-card${creationMode === "managed" ? " selected" : ""}`}
            disabled={!managedEngineReady || managedStatusBusy}
            onClick={() => setCreationMode("managed")}
            role="radio"
            type="button"
          >
            <strong>托管身份浏览器</strong>
            <span>用 VeriSilo 自带的独立浏览器，指纹可以设置。</span>
            <small>
              {managedStatusBusy
                ? "正在检查…"
                : managedEngineReady
                  ? "可以创建。"
                  : "现在还不能创建，请稍后再试。"}
            </small>
          </button>
        </div>
        <div id="managed-browser-availability">
          {managedStatusError !== null ? (
            <div className="managed-status-error" role="alert">
              <span>{managedStatusError}</span>
              <button
                className="button-secondary"
                disabled={managedStatusBusy}
                onClick={() => void refreshManagedStatus()}
                type="button"
              >
                {managedStatusBusy ? "检查中…" : "重新检查"}
              </button>
            </div>
          ) : !managedEngineReady && !managedStatusBusy ? (
            <p className="field-warning" role="status">
              独立浏览器准备好之后才能创建；系统浏览器随时可以建。
            </p>
          ) : null}
        </div>
      </section>

      {creationMode === "managed" ? (
        <ManagedSiloForm
          busy={busy}
          initialColor={color}
          onSubmit={createManagedSilo}
        />
      ) : (
        <section className="panel form-panel">
          <div className="step-heading">
            <span>1</span>
            <div>
              <h2>给这个 Silo 一个名字</h2>
              <p>名称和颜色只用于本机识别，不会发送给网站。</p>
            </div>
          </div>
          <div className="form-grid identity-grid">
            <label>
              Silo 名称
              <input
                disabled={busy}
                maxLength={64}
                onChange={(event) => setName(event.target.value)}
                placeholder="例如：工作账号"
                value={name}
              />
            </label>
            <label className="color-field">
              标识颜色
              <span className="color-control">
                <input
                  disabled={busy}
                  onChange={(event) => setColor(event.target.value)}
                  type="color"
                  value={color}
                />
                <span>{color.toUpperCase()}</span>
              </span>
            </label>
          </div>

          <section
            aria-label="Standard Silo 当前设置"
            className="standard-default-summary"
          >
            <div>
              <p className="eyebrow">
                {standardDefaultsSelected ? "推荐设置已就绪" : "当前设置"}
              </p>
              <h2>
                {localExecution
                  ? browserPath === ""
                    ? "尚未发现可用的 Chrome 或 Edge"
                    : `${browserKind === "chrome" ? "Google Chrome" : "Microsoft Edge"} · Windows 本机`
                  : executionTarget.kind === "wsl"
                    ? `Chromium · ${executionTarget.distribution}`
                    : "远程运行"}
              </h2>
              <p>
                {standardDefaultsSelected
                  ? "已自动选择本机浏览器并使用 Direct 直连。只需命名、核对边界并确认创建。"
                  : "你调整了高级设置；创建前请核对下面显示的运行位置、网络与网站可见边界。"}
              </p>
            </div>
            <div className="standard-default-facts">
              <span>
                <strong>运行位置</strong>
                {localExecution ? "Windows 本机" : "Linux 环境"}
              </span>
              <span>
                <strong>网络</strong>
                {localExecution
                  ? describeNetwork(networkProfile)
                  : "Direct 直连"}
              </span>
              <span>
                <strong>网站数据</strong>独立 Profile
              </span>
            </div>
          </section>

          <button
            aria-expanded={advancedOpen}
            className="create-advanced-toggle"
            disabled={busy}
            onClick={() => {
              const nextOpen = !advancedOpen;
              setAdvancedOpen(nextOpen);
              if (nextOpen && wslStatus === null && !wslBusy) {
                void refreshWsl();
              }
            }}
            type="button"
          >
            <span>
              <strong>高级设置</strong>
              <small>切换浏览器、手工路径、Linux 运行位置或网络方式</small>
            </span>
            <span className="create-advanced-state">
              {advancedOpen
                ? "收起"
                : standardDefaultsSelected
                  ? "使用推荐默认值"
                  : "已调整"}
            </span>
          </button>
          <div
            className="step-heading advanced-step-heading"
            hidden={!advancedOpen}
          >
            <span>A</span>
            <div>
              <h2>浏览器身份与运行位置</h2>
              <p>
                先决定浏览器在哪里运行。网站可见的系统和硬件边界会随运行位置变化。
              </p>
            </div>
          </div>
          <div
            className="execution-target-grid"
            aria-label="运行位置"
            hidden={!advancedOpen}
          >
            <button
              aria-pressed={executionTarget.kind === "local"}
              className={`execution-card${
                executionTarget.kind === "local" ? " selected" : ""
              }`}
              disabled={busy}
              onClick={() => setExecutionTarget({ kind: "local" })}
              type="button"
            >
              <span className="execution-card-kicker">这台电脑</span>
              <strong>Windows 本机</strong>
              <small>使用已安装的 Chrome 或 Edge，网站数据单独保存。</small>
              <span className="execution-card-state">可用</span>
            </button>
            {readyWslOptions.map(({ distribution }) => {
              const selected =
                executionTarget.kind === "wsl" &&
                executionTarget.distribution === distribution;
              return (
                <button
                  aria-pressed={selected}
                  className={`execution-card${selected ? " selected" : ""}`}
                  disabled={busy || wslBusy}
                  key={distribution}
                  onClick={() =>
                    setExecutionTarget({ kind: "wsl", distribution })
                  }
                  type="button"
                >
                  <span className="execution-card-kicker">Linux 环境</span>
                  <strong>{distribution}</strong>
                  <small>使用环境内的 Chromium；当前只允许直连网络。</small>
                  <span className="execution-card-state">已就绪</span>
                </button>
              );
            })}
          </div>
          <div
            className="wsl-discovery-row"
            hidden={!advancedOpen}
            role="status"
          >
            <span>
              {wslBusy
                ? "正在检查这台电脑上的 Linux 环境…"
                : wslStatus === null
                  ? "尚未检查 Linux 环境。"
                  : readyWslOptions.length > 0
                    ? `已确认 ${readyWslOptions.length} 个可创建的 Linux 环境。`
                    : wslStatus.distributions.length > 0
                      ? "已发现 Linux 环境，但尚未通过完整运行检查。"
                      : wslStatus.message || "没有发现已就绪的 Linux 环境。"}
            </span>
            <button
              className="button-secondary button-quiet"
              disabled={busy || wslBusy}
              onClick={() => void refreshWsl()}
              type="button"
            >
              {wslBusy ? "检查中…" : "重新检查"}
            </button>
          </div>
          {unavailableWslCount > 0 ? (
            <p className="wsl-unavailable-note" hidden={!advancedOpen}>
              另有 {unavailableWslCount} 个 Linux
              环境未通过启动、停止、网络和状态检查。请在“运行位置设置”中修复后重新检查。
            </p>
          ) : null}

          {localExecution ? (
            <div className="local-browser-choice" hidden={!advancedOpen}>
              <div className="subsection-heading">
                <strong>选择本机浏览器</strong>
                <span>支持 Windows 版 Google Chrome 和 Microsoft Edge。</span>
              </div>
              <div
                className="browser-switch"
                role="group"
                aria-label="浏览器类型"
              >
                {(["chrome", "edge"] as const).map((kind) => (
                  <button
                    aria-pressed={browserKind === kind}
                    className={browserKind === kind ? "selected" : ""}
                    disabled={busy}
                    key={kind}
                    onClick={() => setBrowserKind(kind)}
                    type="button"
                  >
                    <strong>
                      {kind === "chrome" ? "Google Chrome" : "Microsoft Edge"}
                    </strong>
                    <span>
                      {kind === "chrome" ? "Chrome Stable" : "Edge Stable"}
                    </span>
                  </button>
                ))}
              </div>
              {candidateOptions.length > 0 ? (
                <div className="candidate-list" aria-label="已检测到的浏览器">
                  {candidateOptions.map((candidate) => (
                    <button
                      aria-pressed={browserPath === candidate.executablePath}
                      className={
                        browserPath === candidate.executablePath
                          ? "selected"
                          : ""
                      }
                      key={candidate.executablePath}
                      onClick={() => chooseBrowser(candidate)}
                      type="button"
                    >
                      <span>
                        <strong>{candidate.displayName}</strong>
                        <small>{candidate.version ?? "版本未知"}</small>
                      </span>
                    </button>
                  ))}
                </div>
              ) : (
                <p className="form-hint">
                  尚未在常见 Windows
                  安装位置找到该浏览器，请填写程序文件的位置。
                </p>
              )}
              <label>
                浏览器可执行文件路径
                <input
                  disabled={busy}
                  onChange={(event) => setBrowserPath(event.target.value)}
                  placeholder="C:\\Program Files\\...\\browser.exe"
                  value={browserPath}
                />
              </label>
              {browserKind === "edge" ? (
                <p className="form-hint">
                  Edge 仍可能显示 Windows
                  已登录的微软账户，但不会复用默认浏览器的
                  Cookie。微软或企业网站仍可能通过 Windows 单点登录识别该账户。
                </p>
              ) : null}
            </div>
          ) : executionTarget.kind === "wsl" ? (
            <div className="wsl-browser-summary" hidden={!advancedOpen}>
              <div>
                <strong>
                  使用 {executionTarget.distribution} 中的 Chromium
                </strong>
                <p>
                  不需要选择 Windows 浏览器路径。网站会看到 Linux 浏览器环境；
                  CPU、内存和图形特征仍跟随该环境与本机硬件。
                </p>
              </div>
            </div>
          ) : null}

          <div
            className="step-heading advanced-step-heading"
            hidden={!advancedOpen}
          >
            <span>B</span>
            <div>
              <h2>选择网络方式</h2>
              <p>
                “已配置”与“实际出口已验证”是两件事，创建后可在概览页主动检查。
              </p>
            </div>
          </div>
          {localExecution ? (
            <fieldset disabled={busy || mihomoBusy} hidden={!advancedOpen}>
              <legend className="sr-only">网络配置</legend>
              <div className="proxy-import-card">
                <div>
                  <strong>有现成代理？粘贴一行即可</strong>
                  <span>
                    支持 host:port、host:port:user:password 或带协议的代理 URL。
                  </span>
                </div>
                <div className="proxy-import-row">
                  <input
                    aria-label="一行代理配置"
                    autoComplete="off"
                    onChange={(event) => setProxyImport(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        importProxy();
                      }
                    }}
                    placeholder="socks5://user:password@host:port"
                    spellCheck={false}
                    type="password"
                    value={proxyImport}
                  />
                  <button
                    className="button-secondary"
                    disabled={proxyImport.trim() === ""}
                    onClick={importProxy}
                    type="button"
                  >
                    解析并填入
                  </button>
                </div>
              </div>
              <div className="network-options">
                <NetworkOption
                  checked={networkProfile.mode === "direct"}
                  description="不使用系统代理"
                  label="直连"
                  onChange={() => {
                    setNetworkProfile(emptyNetwork());
                    setProxyUsername("");
                    setProxyPassword("");
                    resetMihomoSnapshot();
                  }}
                />
                <NetworkOption
                  checked={localProxySelected}
                  description="走本机 7897 或 7890 代理端口"
                  label="本机 Clash"
                  onChange={() => {
                    setNetworkProfile(localMihomoProfile());
                    resetMihomoSnapshot();
                  }}
                />
                <NetworkOption
                  checked={
                    networkProfile.mode === "fixed_proxy" && !localProxySelected
                  }
                  description="HTTP、HTTPS 或 SOCKS"
                  label="远程固定代理"
                  onChange={() => {
                    setNetworkProfile({
                      mode: "fixed_proxy",
                      proxyRequired: true,
                      scheme: "socks5",
                      host: "",
                      port: 1080,
                      bypassList: [],
                    });
                    resetMihomoSnapshot();
                  }}
                />
                <NetworkOption
                  checked={networkProfile.mode === "pac"}
                  description="按照自动代理配置决定路由"
                  label="自动代理规则"
                  onChange={() => {
                    setNetworkProfile({
                      mode: "pac",
                      proxyRequired: false,
                      pacUrl: "",
                    });
                    setProxyUsername("");
                    setProxyPassword("");
                    resetMihomoSnapshot();
                  }}
                />
              </div>
              {networkProfile.mode === "fixed_proxy" ? (
                <>
                  {localProxySelected ? (
                    <div className="mihomo-stack">
                      <div className="proxy-bridge-note">
                        <span className="bridge-badge">使用本机代理应用</span>
                        <div>
                          <strong>订阅仍由你自己的 Clash 管理</strong>
                          <p>
                            只填监听端口时，VeriSilo
                            固定的是端口；连接本机代理应用后， 还会为这个 Silo
                            记住选择组和节点，并在每次启动前重新确认。
                          </p>
                        </div>
                      </div>
                      <div className="controller-card">
                        <div className="controller-heading">
                          <div>
                            <strong>连接本机 Clash（推荐）</strong>
                            <span>
                              浏览器走代理端口（常见 7897 或 7890）。Clash Verge
                              默认关闭
                              9097，读取代理组会走内核管道，不必手填控制口。
                            </span>
                          </div>
                          <button
                            className="button-secondary"
                            disabled={mihomoBusy}
                            onClick={() => void inspectMihomoController()}
                            type="button"
                          >
                            {mihomoBusy ? "正在读取…" : "连接并读取节点"}
                          </button>
                        </div>
                        <div className="form-grid controller-grid">
                          <label>
                            Clash 控制地址
                            <input
                              autoComplete="off"
                              onChange={(event) =>
                                setMihomoControllerUrl(event.target.value)
                              }
                              placeholder="可空；Clash Verge 不用填 9097"
                              spellCheck={false}
                              value={mihomoControllerUrl}
                            />
                          </label>
                          <label>
                            Clash 密钥（没设过就空着）
                            <input
                              autoComplete="off"
                              onChange={(event) =>
                                setMihomoControllerSecret(event.target.value)
                              }
                              placeholder="Clash 设置里的 secret"
                              type="password"
                              value={mihomoControllerSecret}
                            />
                          </label>
                        </div>
                        {mihomoSnapshot !== null ? (
                          <div className="form-grid controller-selection">
                            <label>
                              选择组
                              <select
                                onChange={(event) =>
                                  selectMihomoGroup(event.target.value)
                                }
                                value={
                                  mihomoBinding?.selectorGroup ??
                                  mihomoSnapshot.groups[0]?.name ??
                                  ""
                                }
                              >
                                {mihomoSnapshot.groups.map((group) => (
                                  <option key={group.name} value={group.name}>
                                    {group.name}
                                  </option>
                                ))}
                              </select>
                            </label>
                            <label>
                              固定节点
                              <select
                                onChange={(event) =>
                                  selectMihomoNode(event.target.value)
                                }
                                value={mihomoBinding?.nodeName ?? ""}
                              >
                                {(selectedMihomoGroup?.nodes ?? []).map(
                                  (node) => (
                                    <option key={node.name} value={node.name}>
                                      {node.name}
                                      {node.delayMs === null
                                        ? ""
                                        : ` · ${node.delayMs} ms`}
                                    </option>
                                  ),
                                )}
                              </select>
                            </label>
                            <p className="controller-proof">
                              已于 {formatDate(mihomoSnapshot.checkedAt)}
                              读取代理组。延迟是 Clash
                              最近测到的，不等于浏览器出口已经验证。
                              {mihomoSnapshot.providers.length > 0
                                ? ` 订阅：${mihomoSnapshot.providers
                                    .map(
                                      (provider) =>
                                        `${provider.name}（${provider.nodeCount} 节点${
                                          provider.updatedAt === null
                                            ? ""
                                            : `，更新于 ${formatDate(provider.updatedAt)}`
                                        }）`,
                                    )
                                    .join("、")}。`
                                : " 当前代理应用未提供可读的订阅更新时间。"}
                            </p>
                          </div>
                        ) : (
                          <p className="controller-proof">
                            尚未绑定节点。仍可按固定端口使用，但外部客户端切换节点时，Silo
                            的出口也会随之变化。
                          </p>
                        )}
                      </div>
                    </div>
                  ) : null}
                  <div className="form-grid proxy-grid">
                    <label>
                      协议
                      <select
                        disabled={mihomoBinding !== undefined}
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            scheme: event.target.value as
                              "http" | "https" | "socks4" | "socks5",
                          })
                        }
                        value={networkProfile.scheme}
                      >
                        <option value="http">HTTP</option>
                        <option value="https">HTTPS</option>
                        <option value="socks4">SOCKS4</option>
                        <option value="socks5">SOCKS5</option>
                      </select>
                    </label>
                    <label>
                      {localProxySelected ? "本机监听地址" : "代理主机"}
                      <input
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            host: event.target.value,
                          })
                        }
                        placeholder="127.0.0.1"
                        value={networkProfile.host}
                      />
                    </label>
                    <label>
                      {localProxySelected ? "本机端口" : "代理端口"}
                      <input
                        max="65535"
                        min="1"
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            port: Number(event.target.value),
                          })
                        }
                        type="number"
                        value={networkProfile.port}
                      />
                    </label>
                    <label className="check-field">
                      <input
                        disabled={mihomoBinding !== undefined}
                        checked={networkProfile.proxyRequired}
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            proxyRequired: event.target.checked,
                          })
                        }
                        type="checkbox"
                      />
                      {localProxySelected
                        ? "本机代理端口不可连接就拒绝启动，不回退真实出口"
                        : "代理不可连接就拒绝启动，不回退真实出口"}
                    </label>
                  </div>
                  <div className="proxy-auth-card">
                    <div>
                      <strong>代理认证（可空）</strong>
                      <span>
                        支持 HTTP Basic 与 SOCKS5
                        用户名/密码。凭据只进入加密保险库，浏览器只看到随机本机
                        SOCKS5
                        端口；这不是按浏览器进程授权，同一用户下其他本机进程若发现端口仍可能借用。
                      </span>
                    </div>
                    <div className="form-grid auth-grid">
                      <label>
                        用户名
                        <input
                          autoComplete="off"
                          onChange={(event) =>
                            setProxyUsername(event.target.value)
                          }
                          value={proxyUsername}
                        />
                      </label>
                      <label>
                        密码
                        <input
                          autoComplete="new-password"
                          onChange={(event) =>
                            setProxyPassword(event.target.value)
                          }
                          type="password"
                          value={proxyPassword}
                        />
                      </label>
                    </div>
                    {proxyUsername !== "" &&
                    !["http", "socks5"].includes(networkProfile.scheme) ? (
                      <p className="field-warning">
                        当前方式无法安全保存这类代理的登录信息；请改用
                        HTTP/SOCKS5，或让本机 Clash 自行处理。
                      </p>
                    ) : null}
                  </div>
                </>
              ) : null}
              {networkProfile.mode === "pac" ? (
                <div className="form-grid pac-grid">
                  <label>
                    自动代理配置地址（PAC）
                    <input
                      onChange={(event) =>
                        setNetworkProfile({
                          ...networkProfile,
                          pacUrl: event.target.value,
                        })
                      }
                      placeholder="https://example.test/proxy.pac"
                      value={networkProfile.pacUrl}
                    />
                  </label>
                  <label className="check-field">
                    <input
                      checked={networkProfile.proxyRequired}
                      onChange={(event) =>
                        setNetworkProfile({
                          ...networkProfile,
                          proxyRequired: event.target.checked,
                        })
                      }
                      type="checkbox"
                    />
                    必须代理（如果规则可能直连，启动前阻止）
                  </label>
                </div>
              ) : null}
            </fieldset>
          ) : executionTarget.kind === "wsl" ? (
            <div className="wsl-network-boundary" hidden={!advancedOpen}>
              <div>
                <strong>Linux 环境当前仅支持直连</strong>
                <p>
                  浏览器通过该 Linux
                  环境的默认网络访问网站。代理、自动代理规则和 本机 Mihomo
                  绑定暂不能用于这个运行位置。
                </p>
              </div>
              <span className="boundary-status">直连</span>
            </div>
          ) : null}

          <div className="step-heading">
            <span>2</span>
            <div>
              <h2>确认网站将看到什么</h2>
              <p>
                这些是当前能够诚实说明的身份边界，不会把未控制的硬件特征标为已隐藏。
              </p>
            </div>
          </div>
          <section
            className="website-visibility-card"
            aria-label="网站可见信息确认"
          >
            <div className="visibility-heading">
              <div>
                <strong>网站可见身份摘要</strong>
                <p>
                  首次成功启动时，浏览器身份配置与运行位置会固定到这个 Silo。
                </p>
              </div>
            </div>
            <dl className="visibility-facts">
              <div>
                <dt>运行位置</dt>
                <dd>
                  {localExecution
                    ? "这台 Windows 电脑"
                    : executionTarget.kind === "wsl"
                      ? `WSL · ${executionTarget.distribution}`
                      : "远程运行"}
                </dd>
              </div>
              <div>
                <dt>浏览器身份</dt>
                <dd>
                  {localExecution
                    ? `${browserKind === "chrome" ? "Google Chrome" : "Microsoft Edge"} · Windows`
                    : "Chromium · Linux"}
                </dd>
              </div>
              <div>
                <dt>网站数据</dt>
                <dd>
                  <CapabilityState state="native" />
                  每个 Silo 独立保存 Cookie、登录状态和网站数据
                </dd>
              </div>
              <div>
                <dt>这台电脑</dt>
                <dd>
                  <CapabilityState state="inherit" />
                  {localExecution
                    ? "看起来就像这台电脑上的 Chrome 或 Edge"
                    : "看起来就像这台电脑上的 Linux 浏览器"}
                </dd>
              </div>
              <div>
                <dt>独立指纹</dt>
                <dd>
                  <CapabilityState state="unavailable" />
                  系统浏览器不改这台电脑的指纹。登录数据分开保存，但这不是换了一套设备身份。
                </dd>
              </div>
              <div>
                <dt>网络出口</dt>
                <dd>
                  {localExecution
                    ? describeNetwork(networkProfile)
                    : "通过 Linux 环境直连；实际出口可在创建后主动检查"}
                </dd>
              </div>
              <div>
                <dt>语言与时区</dt>
                <dd>
                  {localExecution
                    ? "跟随所选浏览器与 Windows，当前未固定"
                    : "跟随 Linux 环境，当前未固定"}
                </dd>
              </div>
              <div>
                <dt>屏幕</dt>
                <dd>跟随当前显示设备，创建时不伪装为固定值</dd>
              </div>
              <div>
                <dt>WebRTC 与 DNS</dt>
                <dd>创建时尚未测量；启动后从浏览器侧边栏主动检查</dd>
              </div>
              {localExecution && browserKind === "edge" ? (
                <div>
                  <dt>系统账户</dt>
                  <dd>微软或企业网站仍可能通过 Windows 单点登录识别设备账户</dd>
                </div>
              ) : null}
            </dl>
            <label className="visibility-confirmation">
              <input
                checked={websiteBoundaryConfirmed}
                disabled={busy || mihomoBusy}
                onChange={(event) =>
                  setWebsiteBoundaryConfirmed(event.target.checked)
                }
                type="checkbox"
              />
              <span>
                <strong>我已核对以上边界</strong>
                <small>
                  按这些设置创建，并在首次启动时锁定浏览器身份配置与运行位置。
                </small>
              </span>
            </label>
          </section>
          <div className="submit-row">
            <div>
              <strong>准备创建新的 Silo</strong>
              <span>不会导入、复制或改写已有浏览器数据。</span>
            </div>
            <button
              disabled={
                busy ||
                mihomoBusy ||
                !websiteBoundaryConfirmed ||
                name.trim().length === 0 ||
                executionTarget.kind === "remote" ||
                (localExecution && browserPath.trim().length === 0)
              }
              onClick={() => void createSilo()}
              type="button"
            >
              创建 Silo
            </button>
          </div>
        </section>
      )}
    </>
  );
}
