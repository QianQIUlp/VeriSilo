import {
  type CreateManagedSiloInput,
  type ManagedIdentityPreset,
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

import { MIHOMO_DEFAULT_MIXED_PORT } from "../../proxy-presets.js";

import { managedErrorMessage } from "../../shared/notice.js";

import {
  proxyCredentialsPaired,
  PROXY_CREDENTIAL_PAIRING_MESSAGE,
} from "../../proxy-input.js";

import { ClashBindingCard } from "../network/ClashBindingCard.js";

import { useClashBinding } from "../network/useClashBinding.js";

import { type NetworkProfile } from "@verisilo/contracts";

import { parseProxyInput } from "../../proxy-input.js";

import { GPU_PRESETS } from "../../gpu-presets.js";

import type { ManagedSiloTemplate } from "./managedTemplate.js";

export function ManagedSiloForm({
  busy,
  initialColor,
  managedEngineReady,
  managedStatusBusy = false,
  onSubmit,
  name: controlledName,
  onNameChange,
  color: controlledColor,
  onColorChange,
  template,
}: {
  busy: boolean;
  initialColor: string;
  /** Whether the managed engine currently reports healthy readiness. */
  managedEngineReady: boolean;
  managedStatusBusy?: boolean;
  onSubmit: (input: CreateManagedSiloInput) => Promise<void>;
  name?: string;
  onNameChange?: (value: string) => void;
  color?: string;
  onColorChange?: (value: string) => void;
  template?: ManagedSiloTemplate | null;
}) {
  const seed = template ?? null;
  const [fallbackName, setFallbackName] = useState("");
  const [fallbackColor, setFallbackColor] = useState(initialColor);
  const name = controlledName ?? fallbackName;
  const color = controlledColor ?? fallbackColor;
  const changeName = (value: string) => {
    if (controlledName === undefined) {
      setFallbackName(value);
    } else {
      onNameChange?.(value);
    }
  };
  const changeColor = (value: string) => {
    if (controlledColor === undefined) {
      setFallbackColor(value);
    } else {
      onColorChange?.(value);
    }
  };
  const [identityPreset, setIdentityPreset] =
    useState<ManagedIdentityPreset>(seed?.identityPreset ?? "balanced-zh-cn");
  const [followNetworkExit, setFollowNetworkExit] = useState(
    seed?.followNetworkExit ?? true,
  );
  const [screenWidth, setScreenWidth] = useState<number>(
    () => seed?.screenWidth ?? screenChoiceForThisDisplay()[0],
  );
  const [screenHeight, setScreenHeight] = useState<number>(
    () => seed?.screenHeight ?? screenChoiceForThisDisplay()[1],
  );
  const [hardwareConcurrency, setHardwareConcurrency] = useState<number | "">(
    () =>
      seed !== null &&
      managedCoreChoices.some((choice) => choice === seed.hardwareConcurrency)
        ? seed.hardwareConcurrency
        : "",
  );
  const [gpuPreset, setGpuPreset] = useState<string>(seed?.gpuPreset ?? "auto");
  const [timezone, setTimezone] = useState<string>(
    () => seed?.timezone ?? defaultTimezoneForPreset(seed?.identityPreset ?? "balanced-zh-cn"),
  );
  const [networkMode, setNetworkMode] = useState<"direct" | "clash" | "remote">(
    seed?.networkMode ?? "direct",
  );
  const [proxyScheme, setProxyScheme] = useState<"http" | "socks5">(
    seed?.proxyScheme ?? "socks5",
  );
  const [proxyHost, setProxyHost] = useState(seed?.proxyHost ?? "");
  const [proxyPort, setProxyPort] = useState(seed?.proxyPort ?? "8080");
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const [proxyImport, setProxyImport] = useState("");
  const [mixedPort, setMixedPort] = useState(
    seed?.mixedPort ?? String(MIHOMO_DEFAULT_MIXED_PORT),
  );
  const clash = useClashBinding({
    initialControllerUrl: seed?.controllerUrl ?? "",
    initialSelection:
      seed?.selectorGroup !== undefined &&
      seed?.selectorGroup !== "" &&
      seed?.nodeName !== undefined &&
      seed?.nodeName !== ""
        ? { selectorGroup: seed.selectorGroup, nodeName: seed.nodeName }
        : null,
  });
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(seed !== null);
  const formRef = useRef<HTMLFormElement>(null);
  const errorRef = useRef<HTMLDivElement>(null);
  const hasProxy = networkMode !== "direct";
  const screenChoiceOptions: readonly (readonly [number, number])[] =
    managedScreenChoices.some(
      ([width, height]) => width === screenWidth && height === screenHeight,
    )
      ? managedScreenChoices
      : [...managedScreenChoices, [screenWidth, screenHeight]];

  const findLocalClash = async () => {
    const probe = await clash.probe();
    if (probe !== null && probe.mixedPort !== null) {
      setMixedPort(String(probe.mixedPort));
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
      if (clash.selectedGroup !== "" && clash.selectedNode !== "") {
        if (clash.controllerUrl.trim() === "") {
          setError("请先读取代理组，或只使用本机代理端口、不绑定线路。");
          return;
        }
        networkProfile = {
          ...clashProfile,
          externalMihomo: {
            controllerUrl: clash.controllerUrl,
            selectorGroup: clash.selectedGroup,
            nodeName: clash.selectedNode,
          },
        };
      } else {
        networkProfile = clashProfile;
      }
      if (clash.selectedGroup !== "" && clash.secret.trim() !== "") {
        mihomoControllerSecretInput = {
          secret: clash.secret,
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
      if (!proxyCredentialsPaired(proxyUsername, proxyPassword)) {
        setError(PROXY_CREDENTIAL_PAIRING_MESSAGE);
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
      if (proxyUsername.trim() !== "") {
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
          <p className="eyebrow">
            {seed !== null ? "创建新身份" : "托管身份浏览器"}
          </p>
          <h2>
            {seed !== null ? "以此配置创建新的托管身份浏览器" : "创建托管身份浏览器"}
          </h2>
          <p>
            {seed !== null
              ? `复用「${seed.sourceSiloName}」的设备和网络设置，创建新的独立 Profile 与身份。原 Silo 不会被修改。`
              : "选择网络出口和网站可见身份。第一次启动前还可以在 Silo 的编辑页微调或换一套指纹。"}
          </p>
        </div>
        <span
          className={`provider-health${managedEngineReady ? " healthy" : ""}`}
        >
          {managedStatusBusy
            ? "正在检查独立浏览器…"
            : managedEngineReady
              ? "独立浏览器已就绪"
              : "独立浏览器暂时不可用"}
        </span>
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
              autoFocus
              disabled={busy}
              id="managed-silo-name"
              maxLength={64}
              onChange={(event) => changeName(event.target.value)}
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
                onChange={(event) => changeColor(event.target.value)}
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
              {seed?.mihomoSecretUsed ? (
                <p className="field-warning" role="note">
                  本机 Clash 绑定已带入；原 Clash 密钥不会自动复制，如果原来的
                  Clash 设置了密钥，请重新填写。
                </p>
              ) : null}
              <p className="form-hint">
                浏览器走 Clash 已经开着的本机代理端口，常见是 7897 或 7890。
                Clash Verge 默认关闭 9097，读取代理组会自动走内核管道。
                启动时会重新选中该节点；不要求把 Clash 切成全局模式。
              </p>
              <div className="form-grid proxy-grid">
                <label htmlFor="managed-clash-mixed">
                  本机代理端口
                  <input
                    disabled={busy}
                    id="managed-clash-mixed"
                    inputMode="numeric"
                    onChange={(event) => setMixedPort(event.target.value)}
                    placeholder="7897"
                    value={mixedPort}
                  />
                </label>
                <button
                  className="button-secondary"
                  disabled={busy || clash.busy}
                  onClick={() => void findLocalClash()}
                  type="button"
                >
                  {clash.busy ? "正在查找…" : "查找本机 Clash"}
                </button>
              </div>
              <ClashBindingCard
                clash={clash}
                controllerDisplay="port"
                controllerHint={
                  <p className="form-hint">
                    找到 Clash Verge 后这里会显示「内核管道」。其他客户端才需要
                    9097 或 9090。不要把 7897 填到这里。
                  </p>
                }
                controllerLabel="读取代理组用的控制口"
                description="Clash Verge 默认关闭 9097，读取代理组会自动走内核管道。启动时会重新选中该节点；不要求把 Clash 切成全局模式。"
                disabled={busy}
                nodeLabel="节点"
                readLabel="读取代理组"
                secretPlaceholder="Clash 设置里的 secret，大多数人不用填"
                title="连接本机 Clash"
              />
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
              {seed?.proxyCredentialUsed ? (
                <p className="field-warning" role="note">
                  代理地址已带入；原代理的登录信息保存在加密保险库中，不会自动复制。需要认证时请在上方重新输入用户名和密码。
                </p>
              ) : null}
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
                <option value="balanced-ja-jp">日本語（日本）</option>
                <option value="balanced-ko-kr">한국어（韩国）</option>
                <option value="balanced-en-gb">English (UK)</option>
                <option value="balanced-en-ca">English (Canada)</option>
                <option value="balanced-en-au">English (Australia)</option>
                <option value="balanced-en-sg">English (Singapore)</option>
                <option value="balanced-en-in">English (India)</option>
                <option value="balanced-en-ph">English (Philippines)</option>
                <option value="balanced-fr-fr">Français（法国）</option>
                <option value="balanced-es-es">Español（西班牙）</option>
                <option value="balanced-it-it">Italiano（意大利）</option>
                <option value="balanced-pt-br">Português（巴西）</option>
                <option value="balanced-ru-ru">Русский（俄罗斯）</option>
                <option value="balanced-tr-tr">Türkçe（土耳其）</option>
                <option value="balanced-ar-eg">العربية（埃及）</option>
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
            网站会看到一套由 VeriSilo 管理的独立身份：语言按这里的选择，其余特征使用托管引擎的安全默认值；创建后可在身份详情里查看实际结果。
          </p>
        </fieldset>
        <button
          aria-expanded={advancedOpen}
          className="create-advanced-toggle"
          disabled={busy}
          onClick={() => setAdvancedOpen(!advancedOpen)}
          type="button"
        >
          <span>
            <strong>高级身份设置</strong>
            <small>精确屏幕尺寸、CPU 核数、GPU 与时区手工覆盖</small>
          </span>
          <span className="create-advanced-state">
            {advancedOpen ? "收起" : "使用安全默认值"}
          </span>
        </button>
        {advancedOpen ? (
          <fieldset className="managed-network-fieldset managed-advanced-fieldset">
            <legend>高级身份参数</legend>
            <div className="form-grid identity-grid">
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
                    const [widthText, heightText] =
                      event.target.value.split("x");
                    const width = Number(widthText);
                    const height = Number(heightText);
                    if (Number.isInteger(width) && Number.isInteger(height)) {
                      setScreenWidth(width);
                      setScreenHeight(height);
                    }
                  }}
                  value={`${screenWidth}x${screenHeight}`}
                >
                  {screenChoiceOptions.map(([width, height]) => (
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
            {hasProxy && followNetworkExit ? (
              <p className="form-hint">
                时区、语言和地理位置正在跟随代理出口；关闭上面的跟随选择后才能手工固定时区。
              </p>
            ) : null}
            <p className="form-hint">
              User-Agent 跟随内置 Firefox 内核，不能改成 Chrome。Canvas / Audio
              噪声在创建时生成，创建后在 Silo
              的编辑页点「换一套指纹」会重新生成。字体目前跟随这台电脑。WebRTC
              在走代理时用出口 IP，直连时由引擎生成，不会露出这台电脑的网卡地址。
              创建后可在身份详情里看到完整 UA、时区和 WebGL。
            </p>
          </fieldset>
        ) : null}
        <div className="submit-row">
          <div>
            <strong>只创建独立的托管浏览器 Profile</strong>
            <span>不会导入或改写系统浏览器数据。</span>
            {name.trim() === "" ? (
              <span className="submit-missing">
                创建前还需要：填写 Silo 名称。
              </span>
            ) : null}
          </div>
          <button disabled={busy} type="submit">
            {busy ? "正在创建…" : "创建托管身份浏览器"}
          </button>
        </div>
      </form>
    </section>
  );
}
