import {
  desktopApi,
  type CreateManagedSiloInput,
  type ManagedIdentityPreset,
  type MihomoSnapshot,
} from "../../desktop-api.js";

import { useRef, useState, type FormEvent } from "react";

import {
  managedCoreChoices,
  managedScreenChoices,
  screenChoiceForThisDisplay,
} from "../../shared/defaults.js";

import {
  defaultTimezoneForPreset,
  TIMEZONE_PRESETS,
} from "../../timezone-presets.js";

import {
  clashControllerLabel,
  clashControllerPort,
  clashControllerUrl,
  isClashPipeController,
  MIHOMO_DEFAULT_MIXED_PORT,
} from "../../proxy-presets.js";

import { UserFacingError } from "../../user-errors.js";

import { managedErrorMessage } from "../../shared/notice.js";

import { readMihomoGroups } from "../network/controller.js";

import { type NetworkProfile } from "@verisilo/contracts";

import { parseProxyInput } from "../../proxy-input.js";

import { GPU_PRESETS } from "../../gpu-presets.js";

export function ManagedSiloForm({
  busy,
  initialColor,
  onSubmit,
}: {
  busy: boolean;
  initialColor: string;
  onSubmit: (input: CreateManagedSiloInput) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [color, setColor] = useState(initialColor);
  const [identityPreset, setIdentityPreset] =
    useState<ManagedIdentityPreset>("balanced-zh-cn");
  const [followNetworkExit, setFollowNetworkExit] = useState(true);
  const [screenWidth, setScreenWidth] = useState<number>(
    () => screenChoiceForThisDisplay()[0],
  );
  const [screenHeight, setScreenHeight] = useState<number>(
    () => screenChoiceForThisDisplay()[1],
  );
  const [hardwareConcurrency, setHardwareConcurrency] = useState<number | "">(
    "",
  );
  const [gpuPreset, setGpuPreset] = useState("auto");
  const [timezone, setTimezone] = useState<string>(
    defaultTimezoneForPreset("balanced-zh-cn"),
  );
  const [networkMode, setNetworkMode] = useState<"direct" | "clash" | "remote">(
    "direct",
  );
  const [proxyScheme, setProxyScheme] = useState<"http" | "socks5">("socks5");
  const [proxyHost, setProxyHost] = useState("");
  const [proxyPort, setProxyPort] = useState("8080");
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const [proxyImport, setProxyImport] = useState("");
  const [mixedPort, setMixedPort] = useState(String(MIHOMO_DEFAULT_MIXED_PORT));
  const [controllerUrl, setControllerUrl] = useState("");
  const [mihomoControllerSecret, setMihomoControllerSecret] = useState("");
  const [mihomoSnapshot, setMihomoSnapshot] = useState<MihomoSnapshot | null>(
    null,
  );
  const [findingClash, setFindingClash] = useState(false);
  const [readingGroups, setReadingGroups] = useState(false);
  const [clashStatus, setClashStatus] = useState<string | null>(null);
  const [selectorGroup, setSelectorGroup] = useState("");
  const [nodeName, setNodeName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const formRef = useRef<HTMLFormElement>(null);
  const errorRef = useRef<HTMLDivElement>(null);
  const hasProxy = networkMode !== "direct";
  const mihomoControllerUrl = controllerUrl;

  const applyClashSnapshot = (snapshot: MihomoSnapshot) => {
    const group = snapshot.groups[0];
    if (group === undefined || group.nodes.length === 0) {
      throw new UserFacingError("Clash 已经连上，但没有可用的代理组。");
    }
    const selectedNode =
      group.nodes.find((node) => node.name === group.selected) ??
      group.nodes[0];
    if (selectedNode === undefined) {
      throw new UserFacingError("所选代理组里没有线路。");
    }
    setMihomoSnapshot(snapshot);
    setSelectorGroup(group.name);
    setNodeName(selectedNode.name);
  };

  const probeClash = async () => {
    setError(null);
    setFindingClash(true);
    try {
      const probe = await desktopApi.probeLocalClash(mihomoControllerSecret);
      setClashStatus(probe.detail);
      if (probe.mixedPort !== null) {
        setMixedPort(String(probe.mixedPort));
      }
      if (probe.controllerUrl !== null) {
        setControllerUrl(probe.controllerUrl);
      }
    } catch (probeError) {
      setError(managedErrorMessage(probeError));
    } finally {
      setFindingClash(false);
    }
  };

  const inspectClash = async () => {
    setError(null);
    setReadingGroups(true);
    try {
      const inspected = await readMihomoGroups(
        controllerUrl,
        mihomoControllerSecret,
      );
      setControllerUrl(inspected.controllerUrl);
      applyClashSnapshot(inspected.snapshot);
      setClashStatus(
        `已读取代理组（${clashControllerLabel(inspected.controllerUrl)}）。`,
      );
    } catch (inspectError) {
      setMihomoSnapshot(null);
      setError(managedErrorMessage(inspectError));
    } finally {
      setReadingGroups(false);
    }
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setSuccess(null);
    if (name.trim() === "") {
      setError("请填写 Silo 名称。");
      return;
    }
    let networkProfile: NetworkProfile;
    let proxyCredentials: CreateManagedSiloInput["proxyCredentials"];
    let mihomoControllerSecretInput: CreateManagedSiloInput["mihomoControllerSecret"];
    if (networkMode === "direct") {
      networkProfile = { mode: "direct", proxyRequired: false };
    } else if (networkMode === "clash") {
      const parsedMixed = Number(mixedPort);
      if (
        !Number.isInteger(parsedMixed) ||
        parsedMixed < 1 ||
        parsedMixed > 65_535
      ) {
        setError("请填写本机 Clash 代理端口，常见是 7897 或 7890。");
        return;
      }
      const clashProfile: Extract<NetworkProfile, { mode: "fixed_proxy" }> = {
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: "socks5",
        host: "127.0.0.1",
        port: parsedMixed,
        bypassList: [],
      };
      if (selectorGroup !== "" && nodeName !== "") {
        if (controllerUrl.trim() === "") {
          setError("请先读取代理组，或只使用本机代理端口、不绑定线路。");
          return;
        }
        networkProfile = {
          ...clashProfile,
          externalMihomo: {
            controllerUrl,
            selectorGroup,
            nodeName,
          },
        };
      } else {
        networkProfile = clashProfile;
      }
      if (selectorGroup !== "" && mihomoControllerSecret.trim() !== "") {
        mihomoControllerSecretInput = {
          secret: mihomoControllerSecret,
        };
      }
    } else {
      const port = Number(proxyPort);
      if (
        proxyHost.trim() === "" ||
        !Number.isInteger(port) ||
        port < 1 ||
        port > 65_535
      ) {
        setError("请填写有效的代理主机和 1–65535 之间的端口。");
        return;
      }
      const hasUsername = proxyUsername.trim() !== "";
      const hasPassword = proxyPassword !== "";
      if (hasUsername !== hasPassword) {
        setError("代理用户名和密码需要同时填写；无认证代理请都留空。");
        return;
      }
      networkProfile = {
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: proxyScheme,
        host: proxyHost.trim(),
        port,
        bypassList: [],
      };
      if (hasUsername) {
        proxyCredentials = {
          username: proxyUsername.trim(),
          password: proxyPassword,
        };
      }
    }
    try {
      await onSubmit({
        name: name.trim(),
        color,
        identityPreset,
        followNetworkExit: hasProxy && followNetworkExit,
        screenWidth,
        screenHeight,
        hardwareConcurrency:
          hardwareConcurrency === "" ? null : hardwareConcurrency,
        gpuPreset: gpuPreset === "auto" ? null : gpuPreset,
        timezone: hasProxy && followNetworkExit ? null : timezone,
        networkProfile,
        ...(proxyCredentials === undefined ? {} : { proxyCredentials }),
        ...(mihomoControllerSecretInput === undefined
          ? {}
          : { mihomoControllerSecret: mihomoControllerSecretInput }),
      });
      setSuccess("托管身份浏览器已创建。");
    } catch (submitError) {
      setError(managedErrorMessage(submitError));
      window.setTimeout(() => errorRef.current?.focus(), 0);
    }
  };

  return (
    <section className="panel managed-create-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">托管身份浏览器</p>
          <h1>创建托管身份浏览器</h1>
          <p>
            选择网络出口和网站可见身份。创建后立刻能看到 UA、语言、时区、屏幕和
            WebGL；第一次启动前还可以微调或换一套指纹。
          </p>
        </div>
        <span className="provider-health healthy">独立浏览器已就绪</span>
      </div>
      <form
        aria-busy={busy}
        className="managed-create-form"
        noValidate
        onSubmit={(event) => void submit(event)}
        ref={formRef}
      >
        {error !== null ? (
          <div
            aria-labelledby="managed-create-error-title"
            className="managed-form-message error"
            ref={errorRef}
            role="alert"
            tabIndex={-1}
          >
            <strong id="managed-create-error-title">创建没有完成</strong>
            <span>{error}</span>
            <button
              className="button-secondary"
              disabled={busy}
              onClick={() => formRef.current?.requestSubmit()}
              type="button"
            >
              重试
            </button>
          </div>
        ) : null}
        {success !== null ? (
          <p className="managed-form-message success" role="status">
            {success}
          </p>
        ) : null}
        <div className="form-grid identity-grid">
          <label htmlFor="managed-silo-name">
            Silo 名称
            <input
              disabled={busy}
              id="managed-silo-name"
              maxLength={64}
              onChange={(event) => setName(event.target.value)}
              placeholder="例如：隔离工作账号"
              value={name}
            />
          </label>
          <label htmlFor="managed-silo-color">
            标识颜色
            <span className="color-control">
              <input
                disabled={busy}
                id="managed-silo-color"
                onChange={(event) => setColor(event.target.value)}
                type="color"
                value={color}
              />
              <span>{color.toUpperCase()}</span>
            </span>
          </label>
        </div>
        <fieldset className="managed-network-fieldset">
          <legend>网络方式</legend>
          <div
            className="network-options"
            role="radiogroup"
            aria-label="托管浏览器网络方式"
          >
            <label
              className={
                networkMode === "direct"
                  ? "network-option selected"
                  : "network-option"
              }
            >
              <input
                checked={networkMode === "direct"}
                disabled={busy}
                name="managed-network"
                onChange={() => setNetworkMode("direct")}
                type="radio"
              />
              <span>
                <strong>Direct 直连</strong>
                <small>不使用代理。</small>
              </span>
            </label>
            <label
              className={
                networkMode === "clash"
                  ? "network-option selected"
                  : "network-option"
              }
            >
              <input
                checked={networkMode === "clash"}
                disabled={busy}
                name="managed-network"
                onChange={() => {
                  setNetworkMode("clash");
                  setFollowNetworkExit(true);
                }}
                type="radio"
              />
              <span>
                <strong>本机 Clash / Mihomo</strong>
                <small>走本机端口，并可读取代理组。</small>
              </span>
            </label>
            <label
              className={
                networkMode === "remote"
                  ? "network-option selected"
                  : "network-option"
              }
            >
              <input
                checked={networkMode === "remote"}
                disabled={busy}
                name="managed-network"
                onChange={() => {
                  setNetworkMode("remote");
                  setFollowNetworkExit(true);
                }}
                type="radio"
              />
              <span>
                <strong>远程代理</strong>
                <small>HTTP 或 SOCKS5；不可用就拒绝启动。</small>
              </span>
            </label>
          </div>
          {networkMode === "clash" ? (
            <div className="managed-proxy-fields">
              <p className="form-hint">
                浏览器走 Clash 已经开着的本机代理端口，常见是 7897 或 7890。
                Clash Verge 默认关闭 9097，读取代理组会自动走内核管道。
                启动时会重新选中该节点；不要求把 Clash 切成全局模式。
              </p>
              <div className="form-grid proxy-grid">
                <label htmlFor="managed-clash-mixed">
                  本机代理端口
                  <input
                    disabled={busy || findingClash}
                    id="managed-clash-mixed"
                    inputMode="numeric"
                    onChange={(event) => setMixedPort(event.target.value)}
                    placeholder="7897"
                    value={mixedPort}
                  />
                </label>
                <button
                  className="button-secondary"
                  disabled={busy || findingClash}
                  onClick={() => void probeClash()}
                  type="button"
                >
                  {findingClash ? "正在查找…" : "查找本机 Clash"}
                </button>
              </div>
              {clashStatus !== null ? (
                <p className="form-hint">{clashStatus}</p>
              ) : null}
              <div className="form-grid controller-grid">
                <label htmlFor="managed-clash-controller">
                  读取代理组用的控制口
                  <input
                    disabled={busy || readingGroups}
                    id="managed-clash-controller"
                    readOnly={isClashPipeController(controllerUrl)}
                    onChange={(event) => {
                      const value = event.target.value.trim();
                      if (/^\d{2,5}$/.test(value)) {
                        setControllerUrl(clashControllerUrl(Number(value)));
                        return;
                      }
                      setControllerUrl(value);
                    }}
                    placeholder="可空；Clash Verge 不用填 9097"
                    value={
                      isClashPipeController(controllerUrl)
                        ? clashControllerLabel(controllerUrl)
                        : clashControllerPort(controllerUrl) || controllerUrl
                    }
                  />
                </label>
                <label htmlFor="managed-clash-secret">
                  Clash 密钥（没设过就空着）
                  <input
                    disabled={busy || readingGroups}
                    id="managed-clash-secret"
                    onChange={(event) =>
                      setMihomoControllerSecret(event.target.value)
                    }
                    placeholder="Clash 设置里的 secret，大多数人不用填"
                    type="password"
                    value={mihomoControllerSecret}
                  />
                </label>
              </div>
              <p className="form-hint">
                找到 Clash Verge 后这里会显示「内核管道」。其他客户端才需要 9097
                或 9090。不要把 7897 填到这里。
              </p>
              <button
                className="button-secondary"
                disabled={busy || readingGroups}
                onClick={() => void inspectClash()}
                type="button"
              >
                {readingGroups ? "正在读取…" : "读取代理组"}
              </button>
              {mihomoSnapshot !== null ? (
                <div className="form-grid controller-selection">
                  <label htmlFor="managed-clash-group">
                    选择组
                    <select
                      disabled={busy}
                      id="managed-clash-group"
                      onChange={(event) => {
                        const group = mihomoSnapshot.groups.find(
                          (item) => item.name === event.target.value,
                        );
                        setSelectorGroup(event.target.value);
                        setNodeName(group?.nodes[0]?.name ?? "");
                      }}
                      value={selectorGroup}
                    >
                      {mihomoSnapshot.groups.map((group) => (
                        <option key={group.name} value={group.name}>
                          {group.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label htmlFor="managed-clash-node">
                    节点
                    <select
                      disabled={busy}
                      id="managed-clash-node"
                      onChange={(event) => setNodeName(event.target.value)}
                      value={nodeName}
                    >
                      {(
                        mihomoSnapshot.groups.find(
                          (group) => group.name === selectorGroup,
                        )?.nodes ?? []
                      ).map((node) => (
                        <option key={node.name} value={node.name}>
                          {node.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              ) : null}
            </div>
          ) : null}
          {networkMode === "remote" ? (
            <div className="managed-proxy-fields">
              <div className="proxy-import-row">
                <input
                  disabled={busy}
                  onChange={(event) => setProxyImport(event.target.value)}
                  placeholder="socks5://user:password@host:port"
                  spellCheck={false}
                  type="password"
                  value={proxyImport}
                />
                <button
                  className="button-secondary"
                  disabled={busy || proxyImport.trim() === ""}
                  onClick={() => {
                    try {
                      const parsed = parseProxyInput(proxyImport);
                      if (parsed.profile.mode !== "fixed_proxy") {
                        setError("请粘贴 HTTP 或 SOCKS5 代理。");
                        return;
                      }
                      if (
                        parsed.profile.scheme !== "http" &&
                        parsed.profile.scheme !== "socks5"
                      ) {
                        setError("托管身份目前支持 HTTP 或 SOCKS5。");
                        return;
                      }
                      setProxyScheme(parsed.profile.scheme);
                      setProxyHost(parsed.profile.host);
                      setProxyPort(String(parsed.profile.port));
                      setProxyUsername(parsed.credentials?.username ?? "");
                      setProxyPassword(parsed.credentials?.password ?? "");
                      setProxyImport("");
                      setError(null);
                    } catch (parseError) {
                      setError(managedErrorMessage(parseError));
                    }
                  }}
                  type="button"
                >
                  解析
                </button>
              </div>
              <div className="form-grid proxy-grid">
                <label htmlFor="managed-proxy-scheme">
                  代理协议
                  <select
                    disabled={busy}
                    id="managed-proxy-scheme"
                    onChange={(event) =>
                      setProxyScheme(event.target.value as "http" | "socks5")
                    }
                    value={proxyScheme}
                  >
                    <option value="http">HTTP</option>
                    <option value="socks5">SOCKS5</option>
                  </select>
                </label>
                <label htmlFor="managed-proxy-host">
                  代理主机
                  <input
                    disabled={busy}
                    id="managed-proxy-host"
                    onChange={(event) => setProxyHost(event.target.value)}
                    placeholder="127.0.0.1 或 proxy.example.test"
                    value={proxyHost}
                  />
                </label>
                <label htmlFor="managed-proxy-port">
                  代理端口
                  <input
                    disabled={busy}
                    id="managed-proxy-port"
                    inputMode="numeric"
                    max="65535"
                    min="1"
                    onChange={(event) => setProxyPort(event.target.value)}
                    type="number"
                    value={proxyPort}
                  />
                </label>
              </div>
              <div className="form-grid auth-grid">
                <label htmlFor="managed-proxy-username">
                  代理用户名（可空）
                  <input
                    autoComplete="username"
                    disabled={busy}
                    id="managed-proxy-username"
                    onChange={(event) => setProxyUsername(event.target.value)}
                    value={proxyUsername}
                  />
                </label>
                <label htmlFor="managed-proxy-password">
                  代理密码（可空）
                  <input
                    autoComplete="current-password"
                    disabled={busy}
                    id="managed-proxy-password"
                    onChange={(event) => setProxyPassword(event.target.value)}
                    type="password"
                    value={proxyPassword}
                  />
                </label>
              </div>
              <p className="form-hint">
                代理凭据只会进入加密保险库；托管浏览器不会看到原始凭据。
              </p>
            </div>
          ) : null}
        </fieldset>
        <fieldset className="managed-network-fieldset">
          <legend>网站可见身份</legend>
          <div className="form-grid identity-grid">
            <label htmlFor="managed-identity-preset">
              语言
              <select
                disabled={busy}
                id="managed-identity-preset"
                onChange={(event) => {
                  const next = event.target.value as ManagedIdentityPreset;
                  setIdentityPreset(next);
                  setTimezone(defaultTimezoneForPreset(next));
                }}
                value={identityPreset}
              >
                <option value="balanced-zh-cn">中文（简体）</option>
                <option value="balanced-en-us">English (US)</option>
                <option value="balanced-de-de">Deutsch</option>
              </select>
            </label>
            <label htmlFor="managed-timezone">
              时区
              <select
                disabled={busy || (hasProxy && followNetworkExit)}
                id="managed-timezone"
                onChange={(event) => setTimezone(event.target.value)}
                value={timezone}
              >
                {TIMEZONE_PRESETS.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.label}
                  </option>
                ))}
              </select>
            </label>
            <label htmlFor="managed-screen">
              屏幕
              <select
                disabled={busy}
                id="managed-screen"
                onChange={(event) => {
                  const [widthText, heightText] = event.target.value.split("x");
                  const width = Number(widthText);
                  const height = Number(heightText);
                  if (Number.isInteger(width) && Number.isInteger(height)) {
                    setScreenWidth(width);
                    setScreenHeight(height);
                  }
                }}
                value={`${screenWidth}x${screenHeight}`}
              >
                {managedScreenChoices.map(([width, height]) => (
                  <option
                    key={`${width}x${height}`}
                    value={`${width}x${height}`}
                  >
                    {width}×{height}
                  </option>
                ))}
              </select>
            </label>
            <label htmlFor="managed-cores">
              CPU 核数
              <select
                disabled={busy}
                id="managed-cores"
                onChange={(event) =>
                  setHardwareConcurrency(
                    event.target.value === "" ? "" : Number(event.target.value),
                  )
                }
                value={hardwareConcurrency}
              >
                <option value="">由引擎选择</option>
                {managedCoreChoices.map((cores) => (
                  <option key={cores} value={cores}>
                    {cores} 核
                  </option>
                ))}
              </select>
            </label>
            <label htmlFor="managed-gpu">
              GPU / WebGL
              <select
                disabled={busy}
                id="managed-gpu"
                onChange={(event) => setGpuPreset(event.target.value)}
                value={gpuPreset}
              >
                {GPU_PRESETS.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {hasProxy ? (
            <label className="check-field">
              <input
                checked={followNetworkExit}
                disabled={busy}
                onChange={(event) => setFollowNetworkExit(event.target.checked)}
                type="checkbox"
              />
              时区、语言和地理位置跟随代理出口
            </label>
          ) : null}
          <p className="form-hint">
            User-Agent 跟随内置 Firefox 内核，不能改成 Chrome。Canvas / Audio
            噪声在创建时生成，点「换一套指纹」会变。字体目前跟随这台电脑。
            WebRTC 在走代理时用出口
            IP，直连时由引擎生成，不会露出这台电脑的网卡地址。 创建后能看到完整
            UA、时区和 WebGL。
          </p>
        </fieldset>
        <div className="submit-row">
          <div>
            <strong>只创建独立的托管浏览器 Profile</strong>
            <span>不会导入或改写系统浏览器数据。</span>
          </div>
          <button disabled={busy || name.trim() === ""} type="submit">
            {busy ? "正在创建…" : "创建托管身份浏览器"}
          </button>
        </div>
      </form>
    </section>
  );
}
