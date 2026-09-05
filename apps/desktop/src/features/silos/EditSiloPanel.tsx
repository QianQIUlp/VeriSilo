import {
  type BrowserCandidate,
  type ManagedIdentityPreview,
  type UpdateManagedIdentityInput,
  type UpdateSiloEngineInput,
  type UpdateSiloInput,
  type UpdateSiloNetworkInput,
} from "../../desktop-api.js";

import {
  type BrowserKind,
  type NetworkProfile,
  networkProfileSchema,
  type Silo,
} from "@verisilo/contracts";

import { useState } from "react";

import { emptyNetwork } from "../../shared/defaults.js";

import { parseProxyInput } from "../../proxy-input.js";

import { errorMessage } from "../../shared/notice.js";

import {
  localePresetFromPreview,
  siloBrowserLabel,
  siloExecutionTargetLabel,
  siloWebsiteIdentityBoundary,
} from "../../shared/presentation.js";

import { ManagedIdentityFacts } from "../identity/IdentityDetails.js";

import { gpuPresetFromWebgl } from "../../gpu-presets.js";

import { isSupportedTimezone } from "../../timezone-presets.js";

import { describeNetwork } from "../../formatters.js";

export function EditSiloPanel({
  browsers,
  busy,
  identityPreview,
  onCancel,
  onSave,
  onUpdateIdentity,
  silo,
}: {
  browsers: BrowserCandidate[];
  busy: boolean;
  identityPreview?: ManagedIdentityPreview | undefined;
  onCancel: () => void;
  onSave: (
    input: UpdateSiloInput,
    networkInput: UpdateSiloNetworkInput | null,
    engineInput: UpdateSiloEngineInput | null,
  ) => Promise<void>;
  onUpdateIdentity: (input: UpdateManagedIdentityInput) => Promise<void>;
  silo: Silo;
}) {
  const [name, setName] = useState(silo.name);
  const [color, setColor] = useState(silo.color);
  const [browserKind, setBrowserKind] = useState<BrowserKind>(
    silo.browser?.kind ?? "chrome",
  );
  const [executablePath, setExecutablePath] = useState(
    silo.browser?.executablePath ?? "",
  );
  const [replaceNetwork, setReplaceNetwork] = useState(false);
  const [useSystemBrowser, setUseSystemBrowser] = useState(false);
  const [replacementNetwork, setReplacementNetwork] =
    useState<NetworkProfile>(emptyNetwork);
  const [proxyImport, setProxyImport] = useState("");
  const [proxyImportError, setProxyImportError] = useState<string | null>(null);
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const localExecution = silo.executionTarget.kind === "local";
  const identityLocked = silo.identityLockedAt !== null;
  const managedIdentity =
    silo.engine.adapter !== "stock" || silo.browser === null;
  const candidates = browsers.filter(
    (candidate) => candidate.kind === browserKind,
  );
  const networkValid =
    networkProfileSchema.safeParse(replacementNetwork).success;
  const credentialsValid =
    (proxyUsername.trim() === "" && proxyPassword === "") ||
    (proxyUsername.trim() !== "" && proxyPassword !== "");
  const credentialsSupported =
    proxyUsername.trim() === "" ||
    (replacementNetwork.mode === "fixed_proxy" &&
      ["http", "socks5"].includes(replacementNetwork.scheme));
  const managedProxyRequired =
    silo.engine.adapter === "controlled-chromium"
      ? silo.engine.identityTemplate.network.proxyRequired
      : null;
  const managedBrowserNetworkMismatch =
    replaceNetwork &&
    silo.engine.adapter !== "stock" &&
    !useSystemBrowser &&
    managedProxyRequired !== null &&
    replacementNetwork.proxyRequired !== managedProxyRequired;
  const applyProxyImport = () => {
    try {
      const parsed = parseProxyInput(proxyImport);
      setReplacementNetwork(parsed.profile);
      setProxyUsername(parsed.credentials?.username ?? "");
      setProxyPassword(parsed.credentials?.password ?? "");
      setProxyImport("");
      setProxyImportError(null);
    } catch (error) {
      setProxyImportError(errorMessage(error));
    }
  };

  return (
    <section className="panel settings-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">编辑 Silo</p>
          <h1>调整资料，必要时明确替换网络</h1>
          <p>保存后会继续使用原有浏览器数据。运行中的 Silo 不能编辑。</p>
        </div>
      </div>
      <div className="form-grid identity-grid">
        <label>
          名称
          <input
            disabled={busy}
            maxLength={64}
            onChange={(event) => setName(event.target.value)}
            value={name}
          />
        </label>
        <label>
          颜色
          <input
            disabled={busy}
            onChange={(event) => setColor(event.target.value)}
            type="color"
            value={color}
          />
        </label>
      </div>

      {silo.engine.adapter === "camoufox" && identityPreview !== undefined ? (
        <div className="identity-readonly-card">
          <div>
            <span className="readonly-kicker">
              {identityLocked ? "已锁定的托管身份" : "首次启动前可调整"}
            </span>
            <strong>{siloBrowserLabel(silo)}</strong>
            <p>{siloWebsiteIdentityBoundary(silo, identityPreview)}</p>
          </div>
          <dl className="visibility-facts">
            <ManagedIdentityFacts preview={identityPreview} />
          </dl>
          {identityLocked ? (
            <p>
              这个空间已经启动过，身份保持稳定。如需另一套指纹，请创建新的托管身份浏览器。
            </p>
          ) : (
            <div className="card-actions">
              <button
                className="button-secondary"
                disabled={busy}
                onClick={() =>
                  void onUpdateIdentity({
                    identityPreset: localePresetFromPreview(identityPreview),
                    followNetworkExit: silo.networkProfile.proxyRequired,
                    screenWidth: identityPreview.screenWidth,
                    screenHeight: identityPreview.screenHeight,
                    hardwareConcurrency: identityPreview.hardwareConcurrency,
                    gpuPreset: gpuPresetFromWebgl(
                      identityPreview.webglVendor,
                      identityPreview.webglRenderer,
                    ),
                    timezone: isSupportedTimezone(identityPreview.timezone)
                      ? identityPreview.timezone
                      : null,
                    rotateSeed: true,
                  })
                }
                type="button"
              >
                换一套指纹
              </button>
            </div>
          )}
        </div>
      ) : !localExecution ? (
        <div className="identity-readonly-card">
          <div>
            <span className="readonly-kicker">浏览器与运行位置</span>
            <strong>
              {silo.executionTarget.kind === "wsl"
                ? "Linux Chromium"
                : "远程浏览器"}
            </strong>
            <p>{siloExecutionTargetLabel(silo)}</p>
          </div>
          <p>
            {silo.executionTarget.kind === "wsl"
              ? "这个 Silo 使用所选 Linux 环境内的 Chromium，不读取 Windows 浏览器路径。运行位置和浏览器身份不能在这里更换。"
              : "这个 Silo 使用已连接位置中的浏览器，不读取本机浏览器路径。运行位置和浏览器身份不能在这里更换。"}
          </p>
        </div>
      ) : identityLocked || managedIdentity ? (
        <div className="identity-readonly-card">
          <div>
            <span className="readonly-kicker">已锁定的身份配置</span>
            <strong>{siloBrowserLabel(silo)}</strong>
            <p>{siloExecutionTargetLabel(silo)}</p>
          </div>
          <p>{siloWebsiteIdentityBoundary(silo, identityPreview)}</p>
        </div>
      ) : (
        <div className="form-grid">
          <label>
            浏览器
            <select
              disabled={
                busy || (silo.engine.adapter !== "stock" && !useSystemBrowser)
              }
              onChange={(event) => {
                const nextKind = event.target.value as BrowserKind;
                setBrowserKind(nextKind);
                const next = browsers.find(
                  (candidate) => candidate.kind === nextKind,
                );
                if (next !== undefined) {
                  setExecutablePath(next.executablePath);
                }
              }}
              value={browserKind}
            >
              <option value="chrome">Google Chrome</option>
              <option value="edge">Microsoft Edge</option>
            </select>
          </label>
          <label>
            浏览器可执行文件
            <input
              disabled={
                busy || (silo.engine.adapter !== "stock" && !useSystemBrowser)
              }
              onChange={(event) => setExecutablePath(event.target.value)}
              value={executablePath}
            />
          </label>
        </div>
      )}
      {localExecution &&
      !identityLocked &&
      !managedIdentity &&
      candidates.length > 0 ? (
        <div className="candidate-row">
          {candidates.map((candidate) => (
            <button
              className="button-secondary"
              disabled={
                busy || (silo.engine.adapter !== "stock" && !useSystemBrowser)
              }
              key={candidate.executablePath}
              onClick={() => setExecutablePath(candidate.executablePath)}
              type="button"
            >
              使用已发现的 {candidate.displayName}
              {candidate.version === null ? "" : ` ${candidate.version}`}
            </button>
          ))}
        </div>
      ) : null}
      <div className="boundary-note">
        <strong>当前浏览器方式</strong>
        <span>
          {siloBrowserLabel(silo)} · {siloExecutionTargetLabel(silo)}
        </span>
      </div>
      {identityLocked || managedIdentity ? (
        <div className="identity-clone-note">
          <strong>浏览器身份配置与运行位置已锁定</strong>
          <p>
            要更换浏览器家族、受控身份或运行位置，请创建新的 Silo。名称、颜色和
            停止运行后的网络设置仍可在这里调整。
          </p>
        </div>
      ) : null}
      {localExecution &&
      !identityLocked &&
      silo.engine.adapter !== "stock" &&
      silo.browser !== null ? (
        <label className="check-field network-replace-toggle">
          <input
            checked={useSystemBrowser}
            disabled={busy}
            onChange={(event) => {
              setUseSystemBrowser(event.target.checked);
              if (!event.target.checked) {
                setBrowserKind(silo.browser?.kind ?? "chrome");
                setExecutablePath(silo.browser?.executablePath ?? "");
              }
            }}
            type="checkbox"
          />
          改用这台电脑上的{" "}
          {silo.browser?.kind === "chrome" ? "Google Chrome" : "Microsoft Edge"}
        </label>
      ) : null}
      <div className="boundary-note">
        <strong>当前网络设置</strong>
        <span>{describeNetwork(silo.networkProfile)}</span>
      </div>
      <label className="check-field network-replace-toggle">
        <input
          checked={replaceNetwork}
          disabled={busy}
          onChange={(event) => {
            setReplaceNetwork(event.target.checked);
            if (event.target.checked) {
              setReplacementNetwork(emptyNetwork());
              setProxyImport("");
              setProxyUsername("");
              setProxyPassword("");
              setProxyImportError(null);
            }
          }}
          type="checkbox"
        />
        替换网络设置，并清除原有代理登录信息和本机代理访问密钥
      </label>
      {replaceNetwork ? (
        <div className="network-replace-card">
          <p className="field-warning">
            替换会清除原有代理登录信息和本机代理访问密钥。需要认证时请在下方重新输入；普通改名请不要勾选此项。
          </p>
          <label>
            新网络方式
            <select
              disabled={busy}
              onChange={(event) => {
                const mode = event.target.value;
                setProxyUsername("");
                setProxyPassword("");
                if (mode === "direct") {
                  setReplacementNetwork(emptyNetwork());
                } else if (mode === "fixed_proxy") {
                  setReplacementNetwork({
                    mode: "fixed_proxy",
                    proxyRequired: true,
                    scheme: "socks5",
                    host: "127.0.0.1",
                    port: 7890,
                    bypassList: [],
                  });
                } else {
                  setReplacementNetwork({
                    mode: "pac",
                    proxyRequired: false,
                    pacUrl: "",
                  });
                }
              }}
              value={replacementNetwork.mode}
            >
              <option value="direct">直连</option>
              {localExecution ? (
                <>
                  <option value="fixed_proxy">固定 HTTP / SOCKS5 代理</option>
                  <option value="pac">自动代理规则（PAC）</option>
                </>
              ) : null}
            </select>
          </label>
          {!localExecution ? (
            <p className="field-warning">
              Linux 运行位置当前仅支持直连，暂无其他网络方式可选。
            </p>
          ) : null}
          {replacementNetwork.mode === "fixed_proxy" ? (
            <>
              <div className="proxy-import-row edit-proxy-import">
                <label>
                  一行代理
                  <input
                    aria-describedby={
                      proxyImportError === null
                        ? undefined
                        : "edit-proxy-import-error"
                    }
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => setProxyImport(event.target.value)}
                    placeholder="socks5://host:port:user:password"
                    spellCheck={false}
                    type="password"
                    value={proxyImport}
                  />
                </label>
                <button
                  className="button-secondary"
                  disabled={busy || proxyImport.trim() === ""}
                  onClick={applyProxyImport}
                  type="button"
                >
                  解析
                </button>
              </div>
              {proxyImportError !== null ? (
                <p
                  className="field-error"
                  id="edit-proxy-import-error"
                  role="alert"
                >
                  {proxyImportError}
                </p>
              ) : null}
              <div className="form-grid auth-grid">
                <label>
                  用户名（可空）
                  <input
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => setProxyUsername(event.target.value)}
                    value={proxyUsername}
                  />
                </label>
                <label>
                  密码（可空）
                  <input
                    autoComplete="new-password"
                    disabled={busy}
                    onChange={(event) => setProxyPassword(event.target.value)}
                    type="password"
                    value={proxyPassword}
                  />
                </label>
              </div>
              {!credentialsSupported ? (
                <p className="field-error" role="alert">
                  需要登录信息时，请使用 HTTP、SOCKS5，或交给本机 Clash
                  应用处理。
                </p>
              ) : null}
              <div className="boundary-note compact-boundary">
                <strong>准备应用</strong>
                <span>{describeNetwork(replacementNetwork)}</span>
              </div>
            </>
          ) : null}
          {replacementNetwork.mode === "pac" ? (
            <div className="form-grid pac-grid">
              <label>
                自动代理配置地址（PAC）
                <input
                  disabled={busy}
                  onChange={(event) =>
                    setReplacementNetwork({
                      ...replacementNetwork,
                      pacUrl: event.target.value,
                    })
                  }
                  value={replacementNetwork.pacUrl}
                />
              </label>
              <label className="check-field">
                <input
                  checked={replacementNetwork.proxyRequired}
                  disabled={busy}
                  onChange={(event) =>
                    setReplacementNetwork({
                      ...replacementNetwork,
                      proxyRequired: event.target.checked,
                    })
                  }
                  type="checkbox"
                />
                必须代理（如果规则可能直连，启动前阻止）
              </label>
            </div>
          ) : null}
        </div>
      ) : null}
      {managedBrowserNetworkMismatch ? (
        <p className="field-error" role="alert">
          当前独立浏览器的网络保护方式与新设置不一致。请同时改用这台电脑上的系统浏览器，或保持原网络设置。
        </p>
      ) : null}
      <div className="submit-row">
        <div>
          <strong>浏览器数据保持不变</strong>
          <span>现有 Cookie、登录状态和网站设置会继续保留。</span>
        </div>
        <div className="card-actions">
          <button
            className="button-secondary"
            disabled={busy}
            onClick={onCancel}
            type="button"
          >
            取消
          </button>
          <button
            disabled={
              busy ||
              name.trim().length === 0 ||
              (!managedIdentity && executablePath.trim().length === 0) ||
              (replaceNetwork &&
                (!networkValid ||
                  !credentialsValid ||
                  !credentialsSupported ||
                  managedBrowserNetworkMismatch))
            }
            onClick={() => {
              if (
                replaceNetwork &&
                managedIdentity &&
                !window.confirm(
                  "确认重新绑定托管身份浏览器的网络？当前网络证据会失效，必须代理不会回退直连。",
                )
              ) {
                return;
              }
              void onSave(
                {
                  name: name.trim(),
                  color,
                  browserKind,
                  executablePath: executablePath.trim(),
                },
                replaceNetwork
                  ? {
                      networkProfile: replacementNetwork,
                      ...(proxyUsername.trim() !== "" && proxyPassword !== ""
                        ? {
                            proxyCredentials: {
                              username: proxyUsername.trim(),
                              password: proxyPassword,
                            },
                          }
                        : {}),
                    }
                  : null,
                useSystemBrowser ? { engine: { adapter: "stock" } } : null,
              );
            }}
            type="button"
          >
            保存更改
          </button>
        </div>
      </div>
    </section>
  );
}
