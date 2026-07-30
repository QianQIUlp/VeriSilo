import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  BrowserKind,
  NetworkCheckResult,
  NetworkProfile,
  RuntimeNetworkEvidence,
  Silo,
} from "@verisilo/contracts";
import { networkProfileSchema } from "@verisilo/contracts";

import {
  ENVIRONMENT_LAYERS,
  PRODUCT_CAPABILITIES,
  type CapabilityTone,
} from "./capabilities.js";
import {
  desktopApi,
  type BrowserCandidate,
  type CreateSiloInput,
  type DesktopStatus,
  type MihomoSnapshot,
  type WslStatus,
} from "./desktop-api.js";
import {
  describeActivation,
  describeNetwork,
  describeVault,
} from "./formatters.js";
import { runDesktopNetworkCheck } from "./network-check-client.js";
import { parseProxyInput } from "./proxy-input.js";
import { isLoopbackProxyProfile, localMihomoProfile } from "./proxy-presets.js";

const defaultColor = "#5b5ce2";

type Notice = { tone: "error" | "success" | "info"; message: string } | null;
type View = "overview" | "create" | "capabilities";

function emptyNetwork(): NetworkProfile {
  return { mode: "direct", proxyRequired: false };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function App() {
  const [view, setView] = useState<View>("overview");
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  const [silos, setSilos] = useState<Silo[]>([]);
  const [browsers, setBrowsers] = useState<BrowserCandidate[]>([]);
  const [passphrase, setPassphrase] = useState("");
  const [notice, setNotice] = useState<Notice>(null);
  const [name, setName] = useState("");
  const [color, setColor] = useState(defaultColor);
  const [browserPath, setBrowserPath] = useState("");
  const [browserKind, setBrowserKind] = useState<BrowserKind>("chrome");
  const [networkProfile, setNetworkProfile] =
    useState<NetworkProfile>(emptyNetwork);
  const [proxyImport, setProxyImport] = useState("");
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const [mihomoControllerUrl, setMihomoControllerUrl] = useState(
    "http://127.0.0.1:9090/",
  );
  const [mihomoControllerSecret, setMihomoControllerSecret] = useState("");
  const [mihomoSnapshot, setMihomoSnapshot] = useState<MihomoSnapshot | null>(
    null,
  );
  const [mihomoBusy, setMihomoBusy] = useState(false);
  const [networkResult, setNetworkResult] = useState<NetworkCheckResult | null>(
    null,
  );
  const [networkBusy, setNetworkBusy] = useState(false);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const [nextStatus, nextBrowsers] = await Promise.all([
      desktopApi.status(),
      desktopApi.discoverBrowsers(),
    ]);
    setStatus(nextStatus);
    setBrowsers(nextBrowsers);

    if (nextStatus.vault.state === "unlocked") {
      setSilos(await desktopApi.listSilos());
    } else {
      setSilos([]);
    }
  }, []);

  useEffect(() => {
    const refreshWithNotice = () =>
      void refresh().catch((error: unknown) =>
        setNotice({ tone: "error", message: errorMessage(error) }),
      );
    refreshWithNotice();
    const interval = window.setInterval(refreshWithNotice, 30_000);
    return () => window.clearInterval(interval);
  }, [refresh]);

  const candidateOptions = useMemo(
    () => browsers.filter((candidate) => candidate.kind === browserKind),
    [browserKind, browsers],
  );

  useEffect(() => {
    if (browserPath === "" && candidateOptions[0] !== undefined) {
      setBrowserPath(candidateOptions[0].executablePath);
    }
  }, [browserPath, candidateOptions]);

  const activeSilos = useMemo(
    () => silos.filter((silo) => silo.archivedAt === null),
    [silos],
  );

  const withBusy = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  const submitVault = () =>
    withBusy(async () => {
      if (passphrase.length < 12) {
        throw new Error("请使用至少 12 个字符的保险库口令。");
      }

      if (status?.vault.state === "uninitialized") {
        await desktopApi.initializeVault(passphrase);
        setNotice({
          tone: "success",
          message: "保险库已创建。请妥善保存口令；VeriSilo 不提供找回。",
        });
      } else {
        await desktopApi.unlockVault(passphrase);
        setNotice({ tone: "success", message: "保险库已解锁。" });
      }
      setPassphrase("");
      await refresh();
    });

  const lockVault = () =>
    withBusy(async () => {
      await desktopApi.lockVault();
      setView("overview");
      setNotice({ tone: "info", message: "保险库已锁定。" });
      await refresh();
    });

  const chooseBrowser = (candidate: BrowserCandidate) => {
    setBrowserKind(candidate.kind);
    setBrowserPath(candidate.executablePath);
  };

  const createSilo = () =>
    withBusy(async () => {
      if (!networkProfileSchema.safeParse(networkProfile).success) {
        throw new Error(
          "网络配置尚未填写完整。请检查协议、主机、端口、PAC URL 或 Mihomo 绑定。",
        );
      }
      const hasUsername = proxyUsername.trim() !== "";
      const hasPassword = proxyPassword !== "";
      if (hasUsername !== hasPassword) {
        throw new Error("代理用户名和密码需要同时填写；无认证代理请都留空。");
      }
      if (
        hasUsername &&
        networkProfile.mode === "fixed_proxy" &&
        !["http", "socks5"].includes(networkProfile.scheme)
      ) {
        throw new Error(
          "自动代理认证目前支持 HTTP 和 SOCKS5；HTTPS/SOCKS4 请交给外部 Mihomo 端口。",
        );
      }
      const input: CreateSiloInput = {
        name,
        color,
        browserKind,
        executablePath: browserPath,
        networkProfile,
        ...(networkProfile.mode === "fixed_proxy" && hasUsername
          ? {
              proxyCredentials: {
                username: proxyUsername.trim(),
                password: proxyPassword,
              },
            }
          : {}),
        ...(networkProfile.mode === "fixed_proxy" &&
        networkProfile.externalMihomo !== undefined &&
        mihomoControllerSecret !== ""
          ? {
              mihomoControllerSecret: { secret: mihomoControllerSecret },
            }
          : {}),
      };
      const silo = await desktopApi.createSilo(input);
      setName("");
      setNetworkProfile(emptyNetwork());
      setProxyImport("");
      setProxyUsername("");
      setProxyPassword("");
      setMihomoControllerSecret("");
      setMihomoSnapshot(null);
      setNotice({
        tone: "success",
        message: `已创建「${silo.name}」。它不会读取或改写你的默认浏览器 Profile。`,
      });
      setView("overview");
      await refresh();
    });

  const launchSilo = (silo: Silo) =>
    withBusy(async () => {
      const activation = await desktopApi.launchSilo(silo.id);
      setNotice({ tone: "success", message: describeActivation(activation) });
      await refresh();
    });

  const archiveSilo = (silo: Silo) =>
    withBusy(async () => {
      await desktopApi.archiveSilo(silo.id);
      setNotice({
        tone: "info",
        message: `已归档「${silo.name}」。浏览器数据目录仍保留，未被删除。`,
      });
      await refresh();
    });

  const checkNetwork = async () => {
    setNetworkBusy(true);
    try {
      const result = await runDesktopNetworkCheck();
      setNetworkResult(result);
      const useful = result.ip !== null || result.dns.providers.length > 0;
      setNotice({
        tone: useful ? "success" : "error",
        message: useful
          ? "网络检查完成。结果只代表本次桌面端请求，不会自动判断 IP 是否“纯净”。"
          : "网络检查没有获得有效结果，请检查网络后重试。",
      });
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    } finally {
      setNetworkBusy(false);
    }
  };

  const importProxy = () => {
    try {
      const parsed = parseProxyInput(proxyImport);
      setNetworkProfile(parsed.profile);
      setProxyUsername(parsed.credentials?.username ?? "");
      setProxyPassword(parsed.credentials?.password ?? "");
      setProxyImport("");
      setMihomoSnapshot(null);
      setNotice({
        tone: "success",
        message: parsed.credentials
          ? "代理地址已解析；认证信息只会进入加密保险库，不会写入浏览器启动参数。"
          : "代理地址已解析。默认开启“必须代理”，端口失效时不会回退真实出口。",
      });
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const inspectMihomoController = async () => {
    setMihomoBusy(true);
    try {
      const snapshot = await desktopApi.inspectMihomoController({
        controllerUrl: mihomoControllerUrl,
        secret: mihomoControllerSecret,
      });
      const group = snapshot.groups[0];
      if (group === undefined || group.nodes.length === 0) {
        throw new Error("控制器可连接，但没有返回可绑定的选择组和节点。");
      }
      const selectedNode =
        group.nodes.find((node) => node.name === group.selected) ??
        group.nodes[0];
      if (selectedNode === undefined) {
        throw new Error("控制器选择组没有返回可用节点。");
      }
      if (networkProfile.mode !== "fixed_proxy") {
        throw new Error("请先选择 Mihomo / Clash 网络方式。");
      }
      setMihomoSnapshot(snapshot);
      setNetworkProfile({
        ...networkProfile,
        proxyRequired: true,
        scheme: "socks5",
        bypassList: [],
        externalMihomo: {
          controllerUrl: mihomoControllerUrl,
          selectorGroup: group.name,
          nodeName: selectedNode.name,
        },
      });
      setNotice({
        tone: "success",
        message: `已读取本机 Mihomo 控制器，并把这个 Silo 预绑定到「${selectedNode.name}」。创建后每次启动都会重新选择并复查。`,
      });
    } catch (error) {
      setMihomoSnapshot(null);
      setNotice({ tone: "error", message: errorMessage(error) });
    } finally {
      setMihomoBusy(false);
    }
  };

  const selectMihomoGroup = (groupName: string) => {
    if (networkProfile.mode !== "fixed_proxy" || mihomoSnapshot === null) {
      return;
    }
    const group = mihomoSnapshot.groups.find((item) => item.name === groupName);
    const node =
      group?.nodes.find((item) => item.name === group.selected) ??
      group?.nodes[0];
    if (group === undefined || node === undefined) {
      return;
    }
    setNetworkProfile({
      ...networkProfile,
      proxyRequired: true,
      scheme: "socks5",
      bypassList: [],
      externalMihomo: {
        controllerUrl: mihomoControllerUrl,
        selectorGroup: group.name,
        nodeName: node.name,
      },
    });
  };

  const selectMihomoNode = (nodeName: string) => {
    if (
      networkProfile.mode !== "fixed_proxy" ||
      networkProfile.externalMihomo === undefined
    ) {
      return;
    }
    setNetworkProfile({
      ...networkProfile,
      externalMihomo: {
        ...networkProfile.externalMihomo,
        nodeName,
      },
    });
  };

  if (status === null) {
    return (
      <main className="shell loading-state">
        <Brand />
        <p>正在读取本机 VeriSilo 状态…</p>
      </main>
    );
  }

  const vaultLocked = status.vault.state !== "unlocked";

  return (
    <main className="shell">
      <header className="topbar">
        <Brand />
        <div className="topbar-status">
          <span className="local-pill">仅本机</span>
          <span className={`vault-pill${vaultLocked ? " locked" : ""}`}>
            {vaultLocked ? "保险库已锁定" : "保险库已解锁"}
          </span>
        </div>
      </header>

      <nav className="tabbar" aria-label="VeriSilo 桌面端功能">
        <TabButton
          active={view === "overview"}
          label="环境概览"
          onClick={() => setView("overview")}
        />
        <TabButton
          active={view === "create"}
          label="创建 Silo"
          onClick={() => setView("create")}
        />
        <TabButton
          active={view === "capabilities"}
          label="能力路线"
          onClick={() => setView("capabilities")}
        />
      </nav>

      {notice !== null ? (
        <div className={`notice ${notice.tone}`} role="status">
          {notice.message}
        </div>
      ) : null}

      {view === "overview" ? (
        vaultLocked ? (
          <VaultAccess
            busy={busy}
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
                    ? "创建你的第一个独立浏览器身份"
                    : `${activeSilos.length} 个长期浏览器身份已分开保存`}
                </h1>
                <p>
                  每个 Silo 都有自己的 Cookie、站点数据、历史和权限。VeriSilo
                  不会接触你的默认 Chrome 或 Edge Profile。
                </p>
              </div>
              <div className="hero-actions">
                <button onClick={() => setView("create")} type="button">
                  新建 Silo
                </button>
                <button
                  className="button-secondary"
                  disabled={busy}
                  onClick={() => void lockVault()}
                  type="button"
                >
                  立即锁定
                </button>
              </div>
            </section>

            <section className="status-grid" aria-label="当前状态">
              <StatusCard
                detail={describeVault(status.vault)}
                eyebrow="本地保险库"
                tone="good"
                value="已解锁"
              />
              <StatusCard
                detail={describeActivation(status.activation)}
                eyebrow="浏览器进程"
                tone={status.activation.state === "failed" ? "warn" : "good"}
                value={
                  status.activation.activeSiloId === null ? "空闲" : "运行中"
                }
              />
              <StatusCard
                detail={
                  browsers.length === 0
                    ? "可在创建页手动填写绝对路径"
                    : browsers.map((browser) => browser.displayName).join("、")
                }
                eyebrow="已发现浏览器"
                tone={browsers.length > 0 ? "good" : "warn"}
                value={`${browsers.length} 个安装`}
              />
              <StatusCard
                detail={
                  networkResult === null
                    ? "不会自动连接任何检测服务"
                    : `检查于 ${formatDate(networkResult.checkedAt)}`
                }
                eyebrow="桌面控制器出口"
                tone={
                  networkResult !== null && networkResult.ip !== null
                    ? "good"
                    : "neutral"
                }
                value={
                  networkResult?.ip?.address ??
                  (networkResult === null ? "尚未验证" : "获取失败")
                }
              />
            </section>

            <NetworkCheckCard
              busy={networkBusy}
              onCheck={() => void checkNetwork()}
              onClear={() => setNetworkResult(null)}
              result={networkResult}
            />

            {status.activation.networkEvidence !== null ? (
              <RuntimeNetworkEvidenceCard
                evidence={status.activation.networkEvidence}
              />
            ) : null}

            <SiloList
              activation={status.activation.activeSiloId}
              busy={busy}
              onArchive={archiveSilo}
              onCreate={() => setView("create")}
              onLaunch={launchSilo}
              silos={activeSilos}
            />
          </>
        )
      ) : null}

      {view === "create" ? (
        vaultLocked ? (
          <LockedRoute onUnlock={() => setView("overview")} />
        ) : (
          <CreateSiloPanel
            browserKind={browserKind}
            browserPath={browserPath}
            busy={busy}
            candidateOptions={candidateOptions}
            chooseBrowser={chooseBrowser}
            color={color}
            createSilo={createSilo}
            name={name}
            importProxy={importProxy}
            inspectMihomoController={inspectMihomoController}
            mihomoBusy={mihomoBusy}
            mihomoControllerSecret={mihomoControllerSecret}
            mihomoControllerUrl={mihomoControllerUrl}
            mihomoSnapshot={mihomoSnapshot}
            networkProfile={networkProfile}
            proxyImport={proxyImport}
            proxyPassword={proxyPassword}
            proxyUsername={proxyUsername}
            resetMihomoSnapshot={() => setMihomoSnapshot(null)}
            selectMihomoGroup={selectMihomoGroup}
            selectMihomoNode={selectMihomoNode}
            setBrowserKind={(kind) => {
              setBrowserKind(kind);
              setBrowserPath("");
            }}
            setBrowserPath={setBrowserPath}
            setColor={setColor}
            setMihomoControllerSecret={setMihomoControllerSecret}
            setMihomoControllerUrl={(value) => {
              setMihomoControllerUrl(value);
              setMihomoSnapshot(null);
              if (
                networkProfile.mode === "fixed_proxy" &&
                networkProfile.externalMihomo !== undefined
              ) {
                const { externalMihomo: _binding, ...withoutBinding } =
                  networkProfile;
                setNetworkProfile(withoutBinding);
              }
            }}
            setName={setName}
            setNetworkProfile={setNetworkProfile}
            setProxyImport={setProxyImport}
            setProxyPassword={setProxyPassword}
            setProxyUsername={setProxyUsername}
          />
        )
      ) : null}

      {view === "capabilities" ? <CapabilityRoadmap /> : null}

      <footer>
        VeriSilo
        的目标是把不同强度的环境隔离做成可选层级，而不是承诺“不可检测”。
        当前可用能力、未来版本和验证证据会始终分开显示。
      </footer>
    </main>
  );
}

function Brand() {
  return (
    <div className="brand">
      <div className="brand-mark" aria-hidden="true">
        VS
      </div>
      <div>
        <strong>VeriSilo</strong>
        <span>看懂并隔离你的浏览器身份</span>
      </div>
    </div>
  );
}

function TabButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-selected={active}
      className="tab"
      onClick={onClick}
      role="tab"
      type="button"
    >
      {label}
    </button>
  );
}

function VaultAccess({
  busy,
  passphrase,
  setPassphrase,
  status,
  submitVault,
}: {
  busy: boolean;
  passphrase: string;
  setPassphrase: (value: string) => void;
  status: DesktopStatus;
  submitVault: () => Promise<void>;
}) {
  const initialize = status.vault.state === "uninitialized";
  return (
    <section className="vault-layout">
      <article className="panel vault-intro">
        <p className="eyebrow">本地保险库</p>
        <h1>{initialize ? "先保护你的 Silo 配置" : "欢迎回来"}</h1>
        <p>
          保险库加密 Silo 元数据、稳定种子和可选网络配置。浏览器自身的 Profile
          文件仍由 Chrome/Edge 管理，不会被复制进保险库。
        </p>
        <ul className="plain-list">
          <li>默认 15 分钟自动锁定</li>
          <li>口令只在本机使用</li>
          <li>没有云端找回机制</li>
        </ul>
      </article>
      <article className="panel vault-form">
        <h2>{initialize ? "创建保险库" : "解锁保险库"}</h2>
        <p>请输入至少 12 个字符。遗忘口令后无法恢复。</p>
        <label>
          保险库口令
          <input
            aria-label="保险库口令"
            autoComplete={initialize ? "new-password" : "current-password"}
            disabled={busy}
            minLength={12}
            onChange={(event) => setPassphrase(event.target.value)}
            type="password"
            value={passphrase}
          />
        </label>
        <button
          disabled={busy || passphrase.length < 12}
          onClick={() => void submitVault()}
          type="button"
        >
          {initialize ? "创建本地保险库" : "解锁"}
        </button>
      </article>
    </section>
  );
}

function LockedRoute({ onUnlock }: { onUnlock: () => void }) {
  return (
    <section className="panel locked-route">
      <div className="locked-icon" aria-hidden="true">
        ◇
      </div>
      <h1>先解锁保险库</h1>
      <p>创建 Silo 需要读取加密的本地配置。能力路线不需要解锁即可查看。</p>
      <button onClick={onUnlock} type="button">
        返回解锁
      </button>
    </section>
  );
}

function StatusCard({
  detail,
  eyebrow,
  tone,
  value,
}: {
  detail: string;
  eyebrow: string;
  tone: "good" | "warn" | "neutral";
  value: string;
}) {
  return (
    <article className="status-card">
      <span className="status-label">{eyebrow}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
      <span className={`status-dot ${tone}`} aria-hidden="true" />
    </article>
  );
}

function NetworkCheckCard({
  busy,
  onCheck,
  onClear,
  result,
}: {
  busy: boolean;
  onCheck: () => void;
  onClear: () => void;
  result: NetworkCheckResult | null;
}) {
  return (
    <section className="panel network-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">用户主动验证</p>
          <h2>检查桌面端实际看到的网络出口</h2>
          <p>
            这项检查从 VeriSilo 桌面界面发出，用于确认公网
            IP、地区、ASN、出口时区和公共 DNS
            结果；它不代表某个已启动浏览器标签页一定走相同路径。
          </p>
        </div>
        <div className="panel-actions">
          <button disabled={busy} onClick={onCheck} type="button">
            {busy ? "正在检查…" : result === null ? "同意并检查" : "重新检查"}
          </button>
          {result !== null ? (
            <button
              className="button-secondary"
              onClick={onClear}
              type="button"
            >
              清除
            </button>
          ) : null}
        </div>
      </div>

      {result === null ? (
        <div className="empty-inline">
          <strong>尚未向三方检测端点发送请求</strong>
          <span>
            点击后会连接 ipwho.is、Cloudflare 1.1.1.1 和 Google Public
            DNS；三方会看到请求 IP，结果只保存在本次界面内存中。
          </span>
        </div>
      ) : (
        <div className="network-result">
          <div className="network-primary">
            <span>公网 IP</span>
            <strong>{result.ip?.address ?? "获取失败"}</strong>
            <small>{networkLocation(result)}</small>
          </div>
          <div className="network-details">
            <ResultItem label="网络归属" value={networkOwner(result)} />
            <ResultItem
              label="出口时区"
              value={result.ip?.timezone ?? "未知"}
            />
            <ResultItem label="公共 DNS" value={dnsStateLabel(result)} />
            <ResultItem
              label="DNSSEC"
              value={
                result.dns.dnssec === "validated"
                  ? "两家均返回已验证"
                  : "未完整验证"
              }
            />
          </div>
          <div className="network-chips">
            <span className={result.ip === null ? "warn" : "good"}>
              {result.ip === null ? "IP 未确认" : "IP 已确认"}
            </span>
            <span
              className={result.dns.state === "consistent" ? "good" : "warn"}
            >
              {dnsStateLabel(result)}
            </span>
            <span>IP 信誉/黑名单未评分</span>
          </div>
          <p className="scope-copy">
            两家公共 DoH
            结果一致，只说明这两次固定域名查询一致；不能证明系统、路由器或运营商
            DNS 一定没有污染或劫持。机房线路线索也不等于 IP 一定有风险。
          </p>
          {result.errors.length > 0 ? (
            <details className="error-details">
              <summary>查看部分检查错误</summary>
              <ul>
                {result.errors.map((error) => (
                  <li key={error}>{error}</li>
                ))}
              </ul>
            </details>
          ) : null}
        </div>
      )}
    </section>
  );
}

function ResultItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function RuntimeNetworkEvidenceCard({
  evidence,
}: {
  evidence: RuntimeNetworkEvidence;
}) {
  const stages = [
    ["网络配置", evidence.configuration],
    ["Mihomo 节点绑定", evidence.controllerBinding],
    ["代理端点", evidence.endpoint],
    ["代理认证", evidence.authentication],
    ["浏览器路由", evidence.browserRouting],
    ["Silo 实际出口", evidence.exit],
    ["DNS 路径证据", evidence.dns],
    ["WebRTC 路径", evidence.webRtc],
  ] as const;
  return (
    <section className="panel evidence-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">本次启动证据</p>
          <h2>配置成功，不等于出口已经验证</h2>
          <p>
            桌面端只确认它实际完成的步骤；公网出口、DNS 与 WebRTC 必须在已启动的
            Silo 内用 Companion 主动检查。
          </p>
        </div>
        <span className="provider-badge">
          {networkProviderLabel(evidence.provider)}
        </span>
      </div>
      {evidence.endpointLabel !== undefined ? (
        <p className="endpoint-chain">路径：{evidence.endpointLabel}</p>
      ) : null}
      <div className="evidence-grid">
        {stages
          .filter(([, state]) => state !== "not_applicable")
          .map(([label, state]) => (
            <div className="evidence-stage" key={label}>
              <span>{label}</span>
              <strong className={evidenceTone(state)}>
                {evidenceStateLabel(state)}
              </strong>
            </div>
          ))}
      </div>
      {evidence.safeguards.length > 0 ? (
        <div className="safeguard-list">
          {evidence.safeguards.map((safeguard) => (
            <span key={safeguard}>{safeguardLabel(safeguard)}</span>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function SiloList({
  activation,
  busy,
  onArchive,
  onCreate,
  onLaunch,
  silos,
}: {
  activation: string | null;
  busy: boolean;
  onArchive: (silo: Silo) => Promise<void>;
  onCreate: () => void;
  onLaunch: (silo: Silo) => Promise<void>;
  silos: Silo[];
}) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">长期身份</p>
          <h2>你的 Silos</h2>
          <p>切换 Silo 就是在切换一整套浏览器数据，而不是只替换 Cookie。</p>
        </div>
        <button className="button-secondary" onClick={onCreate} type="button">
          新建
        </button>
      </div>
      {silos.length === 0 ? (
        <div className="empty-silos">
          <span aria-hidden="true">◎</span>
          <strong>还没有 Silo</strong>
          <p>创建一个工作、个人或临时用途的独立浏览器环境。</p>
          <button onClick={onCreate} type="button">
            创建第一个 Silo
          </button>
        </div>
      ) : (
        <div className="silo-grid">
          {silos.map((silo) => (
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
                  <p>
                    {silo.browser.kind === "chrome"
                      ? "Google Chrome"
                      : "Microsoft Edge"}
                  </p>
                </div>
                {activation === silo.id ? (
                  <span className="running-badge">运行中</span>
                ) : null}
              </div>
              <dl className="silo-facts">
                <div>
                  <dt>网站数据</dt>
                  <dd>完整独立 Profile</dd>
                </div>
                <div>
                  <dt>网络</dt>
                  <dd>{describeNetwork(silo.networkProfile)}</dd>
                </div>
                {silo.networkProfile.mode === "fixed_proxy" &&
                silo.networkProfile.credentialRef !== undefined ? (
                  <div>
                    <dt>代理认证</dt>
                    <dd>凭据已加密，启动时走本机中继</dd>
                  </div>
                ) : null}
                <div>
                  <dt>实际出口</dt>
                  <dd>启动后在该 Silo 的 Companion 内验证</dd>
                </div>
                <div>
                  <dt>伴侣扩展</dt>
                  <dd>在此 Silo 内由你确认安装</dd>
                </div>
              </dl>
              <div className="card-actions">
                <button
                  disabled={busy || activation !== null}
                  onClick={() => void onLaunch(silo)}
                  type="button"
                >
                  启动 Silo
                </button>
                <button
                  className="button-secondary"
                  disabled={busy}
                  onClick={() => void onArchive(silo)}
                  type="button"
                >
                  归档
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function CreateSiloPanel({
  browserKind,
  browserPath,
  busy,
  candidateOptions,
  chooseBrowser,
  color,
  createSilo,
  importProxy,
  inspectMihomoController,
  mihomoBusy,
  mihomoControllerSecret,
  mihomoControllerUrl,
  mihomoSnapshot,
  name,
  networkProfile,
  proxyImport,
  proxyPassword,
  proxyUsername,
  resetMihomoSnapshot,
  selectMihomoGroup,
  selectMihomoNode,
  setBrowserKind,
  setBrowserPath,
  setColor,
  setMihomoControllerSecret,
  setMihomoControllerUrl,
  setName,
  setNetworkProfile,
  setProxyImport,
  setProxyPassword,
  setProxyUsername,
}: {
  browserKind: BrowserKind;
  browserPath: string;
  busy: boolean;
  candidateOptions: BrowserCandidate[];
  chooseBrowser: (candidate: BrowserCandidate) => void;
  color: string;
  createSilo: () => Promise<void>;
  importProxy: () => void;
  inspectMihomoController: () => Promise<void>;
  mihomoBusy: boolean;
  mihomoControllerSecret: string;
  mihomoControllerUrl: string;
  mihomoSnapshot: MihomoSnapshot | null;
  name: string;
  networkProfile: NetworkProfile;
  proxyImport: string;
  proxyPassword: string;
  proxyUsername: string;
  resetMihomoSnapshot: () => void;
  selectMihomoGroup: (groupName: string) => void;
  selectMihomoNode: (nodeName: string) => void;
  setBrowserKind: (kind: BrowserKind) => void;
  setBrowserPath: (path: string) => void;
  setColor: (color: string) => void;
  setMihomoControllerSecret: (secret: string) => void;
  setMihomoControllerUrl: (url: string) => void;
  setName: (name: string) => void;
  setNetworkProfile: (profile: NetworkProfile) => void;
  setProxyImport: (value: string) => void;
  setProxyPassword: (value: string) => void;
  setProxyUsername: (value: string) => void;
}) {
  const localProxySelected = isLoopbackProxyProfile(networkProfile);
  const mihomoBinding =
    networkProfile.mode === "fixed_proxy"
      ? networkProfile.externalMihomo
      : undefined;
  const selectedMihomoGroup = mihomoSnapshot?.groups.find(
    (group) => group.name === mihomoBinding?.selectorGroup,
  );
  return (
    <>
      <section className="create-hero panel">
        <p className="eyebrow">新环境</p>
        <h1>创建一个完整、长期的浏览器身份</h1>
        <p>
          VeriSilo
          会新建数据目录并用参数数组启动浏览器。不会复制现有账号，也不会修改默认
          Profile。
        </p>
        <div className="assurance-row">
          <span>✓ Cookie 与站点数据独立</span>
          <span>✓ 关闭后可继续使用</span>
          <span>✓ 单 Silo 安全启动</span>
        </div>
      </section>

      <section className="panel form-panel">
        <div className="step-heading">
          <span>1</span>
          <div>
            <h2>给这个身份一个名字</h2>
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

        <div className="step-heading">
          <span>2</span>
          <div>
            <h2>选择浏览器</h2>
            <p>首发支持 Windows 上的 Chrome 与 Edge Stable。</p>
          </div>
        </div>
        <div className="browser-switch" role="group" aria-label="浏览器类型">
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
              <span>{kind === "chrome" ? "Chrome Stable" : "Edge Stable"}</span>
            </button>
          ))}
        </div>
        {candidateOptions.length > 0 ? (
          <div className="candidate-list" aria-label="已检测到的浏览器">
            {candidateOptions.map((candidate) => (
              <button
                aria-pressed={browserPath === candidate.executablePath}
                className={
                  browserPath === candidate.executablePath ? "selected" : ""
                }
                key={candidate.executablePath}
                onClick={() => chooseBrowser(candidate)}
                type="button"
              >
                <span className="candidate-check" aria-hidden="true">
                  {browserPath === candidate.executablePath ? "✓" : ""}
                </span>
                <span>
                  <strong>{candidate.displayName}</strong>
                  <small>{candidate.version ?? "版本未知"}</small>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <p className="form-hint">
            尚未在常见 Windows 安装位置找到该浏览器，请填写可执行文件绝对路径。
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

        <div className="step-heading">
          <span>3</span>
          <div>
            <h2>选择网络方式</h2>
            <p>
              “已配置”与“实际出口已验证”是两件事，创建后可在概览页主动检查。
            </p>
          </div>
        </div>
        <fieldset disabled={busy || mihomoBusy}>
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
              description="连接本机 SOCKS/Mixed 端口"
              label="Mihomo / Clash"
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
              description="由 PAC URL 决定路由"
              label="PAC"
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
                    <span className="bridge-badge">外部内核桥接</span>
                    <div>
                      <strong>
                        订阅仍由你自己的 Mihomo / Clash 客户端管理
                      </strong>
                      <p>
                        只填监听端口时，VeriSilo 固定的是端口；连接本机
                        Controller 后，才会把选择组和节点也写进这个
                        Silo，并在每次启动前重新选择、读取确认。
                      </p>
                    </div>
                  </div>
                  <div className="controller-card">
                    <div className="controller-heading">
                      <div>
                        <strong>绑定本机 Controller（推荐）</strong>
                        <span>
                          只允许 127.0.0.1 / ::1 的 HTTP
                          Controller，拒绝远程管理地址。
                        </span>
                      </div>
                      <button
                        className="button-secondary"
                        disabled={
                          mihomoBusy || mihomoControllerUrl.trim() === ""
                        }
                        onClick={() => void inspectMihomoController()}
                        type="button"
                      >
                        {mihomoBusy ? "正在读取…" : "连接并读取节点"}
                      </button>
                    </div>
                    <div className="form-grid controller-grid">
                      <label>
                        Controller 地址
                        <input
                          autoComplete="off"
                          onChange={(event) =>
                            setMihomoControllerUrl(event.target.value)
                          }
                          placeholder="http://127.0.0.1:9090/"
                          spellCheck={false}
                          value={mihomoControllerUrl}
                        />
                      </label>
                      <label>
                        Controller Secret（可空）
                        <input
                          autoComplete="off"
                          onChange={(event) =>
                            setMihomoControllerSecret(event.target.value)
                          }
                          placeholder="只进入加密保险库"
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
                            {(selectedMihomoGroup?.nodes ?? []).map((node) => (
                              <option key={node.name} value={node.name}>
                                {node.name}
                                {node.delayMs === null
                                  ? ""
                                  : ` · ${node.delayMs} ms`}
                              </option>
                            ))}
                          </select>
                        </label>
                        <p className="controller-proof">
                          已于 {formatDate(mihomoSnapshot.checkedAt)}
                          读取控制器。延迟是 Mihomo
                          返回的最近记录，不等于实际出口已经验证。
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
                            : " 当前 Controller 未提供可读的订阅更新时间。"}
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
                    SOCKS5 端口。
                  </span>
                </div>
                <div className="form-grid auth-grid">
                  <label>
                    用户名
                    <input
                      autoComplete="off"
                      onChange={(event) => setProxyUsername(event.target.value)}
                      value={proxyUsername}
                    />
                  </label>
                  <label>
                    密码
                    <input
                      autoComplete="new-password"
                      onChange={(event) => setProxyPassword(event.target.value)}
                      type="password"
                      value={proxyPassword}
                    />
                  </label>
                </div>
                {proxyUsername !== "" &&
                !["http", "socks5"].includes(networkProfile.scheme) ? (
                  <p className="field-warning">
                    此协议不能由当前认证中继安全处理；请改用 HTTP/SOCKS5，或让
                    Mihomo 处理上游认证。
                  </p>
                ) : null}
              </div>
            </>
          ) : null}
          {networkProfile.mode === "pac" ? (
            <div className="form-grid pac-grid">
              <label>
                PAC URL
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
                必须代理（当前 PAC 尚无出口预检，会拒绝启动）
              </label>
            </div>
          ) : null}
        </fieldset>
        <div className="submit-row">
          <div>
            <strong>准备创建新的浏览器数据目录</strong>
            <span>不会导入、复制或改写已有 Profile。</span>
          </div>
          <button
            disabled={
              busy ||
              mihomoBusy ||
              name.trim().length === 0 ||
              browserPath.trim().length === 0
            }
            onClick={() => void createSilo()}
            type="button"
          >
            创建隔离 Silo
          </button>
        </div>
      </section>
    </>
  );
}

function NetworkOption({
  checked,
  description,
  label,
  onChange,
}: {
  checked: boolean;
  description: string;
  label: string;
  onChange: () => void;
}) {
  return (
    <label className={checked ? "network-option selected" : "network-option"}>
      <input
        checked={checked}
        name="network"
        onChange={onChange}
        type="radio"
      />
      <span>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
    </label>
  );
}

function CapabilityRoadmap() {
  const [wslStatus, setWslStatus] = useState<WslStatus | null>(null);
  const [wslBusy, setWslBusy] = useState(false);

  const checkWsl = async () => {
    setWslBusy(true);
    try {
      setWslStatus(await desktopApi.detectWsl());
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

  return (
    <>
      <section className="roadmap-hero panel">
        <div>
          <p className="eyebrow">产品路线</p>
          <h1>同一个 VeriSilo，按需要选择隔离强度</h1>
          <p>
            独立 Profile
            是已经可用的基础层。受控浏览器引擎、本地虚拟机和自托管远程环境已经列入
            V0.7–V0.9 正式路线；它们是更强的环境实现，不会被包装成扩展魔法。
          </p>
        </div>
        <span className="roadmap-principle">能力必须可验证</span>
      </section>

      <section className="layer-grid" aria-label="环境实现层级">
        {ENVIRONMENT_LAYERS.map((layer, index) => (
          <article
            className={`layer-card${layer.status === "available" ? " available" : ""}`}
            key={layer.id}
          >
            <div className="layer-topline">
              <span className="layer-index">0{index + 1}</span>
              <span className="layer-version">{layer.version}</span>
            </div>
            <h2>{layer.name}</h2>
            <p>{layer.summary}</p>
            <ul>
              {layer.delivers.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
            <span className="layer-status">
              {layer.status === "available" ? "已实现" : "已列入路线"}
            </span>
          </article>
        ))}
      </section>

      <section className="panel capability-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">逐项边界</p>
            <h2>你关心的参数，分别在哪一层实现</h2>
            <p>
              “计划实现”不是“现在可用”。每一项都同时说明当前事实、目标层级和以后怎样验收。
            </p>
          </div>
        </div>
        <div className="desktop-capability-table">
          {PRODUCT_CAPABILITIES.map((capability) => (
            <article className="capability-row" key={capability.id}>
              <div className="capability-name">
                <strong>{capability.name}</strong>
                <CapabilityBadge
                  label={capability.routeLabel}
                  tone={capability.tone}
                />
              </div>
              <p>{capability.currentReality}</p>
              <div className="evidence-rule">
                <span>验收证据</span>
                <p>{capability.evidenceRule}</p>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section className="panel provider-readiness">
        <div>
          <p className="eyebrow">第三阶段安全接口</p>
          <h2>WSL Chromium Provider 尚未启用，但可先做只读环境检查</h2>
          <p>
            此检查只调用固定的 wsl.exe 状态/列表参数，不安装发行版、不执行 Linux
            命令、不修改网络。完整 Provider、TUN 与随包 Mihomo
            将分别经过许可证和管理员权限审计。
          </p>
          {wslStatus !== null ? (
            <div className="provider-result">
              <strong>{wslStatus.available ? "环境可用" : "尚不可用"}</strong>
              <span>{wslStatus.message}</span>
              {wslStatus.distributions.length > 0 ? (
                <small>{wslStatus.distributions.join("、")}</small>
              ) : null}
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

      <section className="panel truth-panel">
        <div className="truth-icon" aria-hidden="true">
          i
        </div>
        <div>
          <h2>这条路线不等于“反检测”承诺</h2>
          <p>
            VeriSilo
            会参考成熟产品的环境分层、配置模板和远程会话思路，但不会照搬营销结论。
            即使进入专用引擎或
            VM，行为、账号关系、服务端历史和真实业务风控仍可能把会话关联起来。
          </p>
        </div>
      </section>
    </>
  );
}

function CapabilityBadge({
  label,
  tone,
}: {
  label: string;
  tone: CapabilityTone;
}) {
  return <span className={`capability-badge ${tone}`}>{label}</span>;
}

function networkLocation(result: NetworkCheckResult): string {
  if (result.ip === null) {
    return result.errors[0] ?? "第三方服务没有返回有效 IP 数据";
  }
  return (
    [
      result.ip.countryCode ?? result.ip.country,
      result.ip.region,
      result.ip.city,
    ]
      .filter((part): part is string => part !== null)
      .join(" · ") || "位置未知"
  );
}

function networkOwner(result: NetworkCheckResult): string {
  if (result.ip === null) {
    return "未知";
  }
  return (
    [result.ip.asn, result.ip.organization ?? result.ip.isp]
      .filter((part): part is string => part !== null)
      .join(" · ") || "未知"
  );
}

function dnsStateLabel(result: NetworkCheckResult): string {
  const labels: Record<NetworkCheckResult["dns"]["state"], string> = {
    consistent: "两家公共 DNS 结果一致",
    different: "两家公共 DNS 结果有差异",
    resolver_error: "公共 DNS 返回错误",
    partial: "仅一家公共 DNS 可用",
    failed: "公共 DNS 检查失败",
  };
  return labels[result.dns.state];
}

function networkProviderLabel(
  provider: RuntimeNetworkEvidence["provider"],
): string {
  return {
    direct: "直连",
    fixed_proxy: "固定代理",
    external_mihomo: "外部 Mihomo",
    pac: "PAC",
  }[provider];
}

function evidenceStateLabel(
  state: RuntimeNetworkEvidence["configuration"],
): string {
  return {
    not_applicable: "不适用",
    not_requested: "尚未验证",
    configured: "已配置",
    reachable: "端点可达",
    applied: "已应用",
    verified: "已验证",
    failed: "失败",
    unavailable: "当前不可验证",
  }[state];
}

function evidenceTone(state: RuntimeNetworkEvidence["configuration"]): string {
  if (["reachable", "applied", "verified"].includes(state)) {
    return "good";
  }
  if (["failed", "unavailable"].includes(state)) {
    return "warn";
  }
  return "neutral";
}

function safeguardLabel(safeguard: string): string {
  return (
    {
      no_direct_fallback: "无 DIRECT 回退",
      browser_dns_through_proxy: "浏览器域名交给代理",
      quic_disabled: "QUIC 已禁用",
      non_proxied_webrtc_udp_disabled: "非代理 WebRTC UDP 已禁用",
    }[safeguard] ?? safeguard
  );
}

function formatDate(isoDate: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(isoDate));
  } catch {
    return isoDate;
  }
}
