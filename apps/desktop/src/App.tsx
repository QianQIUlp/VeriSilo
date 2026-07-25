import { useCallback, useEffect, useMemo, useState } from "react";

import type { BrowserKind, NetworkProfile, Silo } from "@verisilo/contracts";

import {
  desktopApi,
  type BrowserCandidate,
  type CreateSiloInput,
  type DesktopStatus,
} from "./desktop-api.js";
import {
  describeActivation,
  describeNetwork,
  describeVault,
} from "./formatters.js";

const defaultColor = "#4f46e5";

type Notice = { tone: "error" | "success" | "info"; message: string } | null;

function emptyNetwork(): NetworkProfile {
  return { mode: "direct", proxyRequired: false };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function App() {
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

  const chooseBrowser = (candidate: BrowserCandidate) => {
    setBrowserKind(candidate.kind);
    setBrowserPath(candidate.executablePath);
  };

  const createSilo = () =>
    withBusy(async () => {
      const input: CreateSiloInput = {
        name,
        color,
        browserKind,
        executablePath: browserPath,
        networkProfile,
      };
      const silo = await desktopApi.createSilo(input);
      setName("");
      setNetworkProfile(emptyNetwork());
      setNotice({
        tone: "success",
        message: `已创建「${silo.name}」。其浏览器数据目录与默认 Profile 完全分离。`,
      });
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

  if (status === null) {
    return <main className="shell">正在加载 VeriSilo…</main>;
  }

  const vaultLocked = status.vault.state !== "unlocked";

  return (
    <main className="shell">
      <header className="hero">
        <div>
          <p className="eyebrow">VERIFIED BROWSER SILOS</p>
          <h1>VeriSilo</h1>
          <p>可验证的浏览器环境隔离与隐私审计。</p>
        </div>
        <div className="status-card">
          <strong>{describeVault(status.vault)}</strong>
          <span>{describeActivation(status.activation)}</span>
        </div>
      </header>

      {notice !== null ? (
        <p className={`notice ${notice.tone}`}>{notice.message}</p>
      ) : null}

      {vaultLocked ? (
        <section className="panel narrow">
          <h2>
            {status.vault.state === "uninitialized"
              ? "创建保险库"
              : "解锁保险库"}
          </h2>
          <p>
            保险库仅加密 VeriSilo 的 Silo 元数据和种子。Chrome/Edge 自身管理的
            Profile 数据不会被复制进保险库。
          </p>
          <label>
            口令
            <input
              aria-label="保险库口令"
              autoComplete="current-password"
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
            {status.vault.state === "uninitialized" ? "创建本地保险库" : "解锁"}
          </button>
          <p className="hint">默认 15 分钟自动锁定；遗忘口令无法恢复。</p>
        </section>
      ) : (
        <>
          <section className="panel">
            <div className="panel-heading">
              <div>
                <h2>新建 Silo</h2>
                <p>
                  新 Silo 只使用新的浏览器数据目录，不会接触你现有的 Chrome 或
                  Edge Profile。
                </p>
              </div>
              <button
                disabled={busy}
                onClick={() => void desktopApi.lockVault().then(refresh)}
                type="button"
              >
                立即锁定
              </button>
            </div>
            <div className="form-grid">
              <label>
                名称
                <input
                  disabled={busy}
                  onChange={(event) => setName(event.target.value)}
                  placeholder="例如：工作"
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
              <label>
                浏览器
                <select
                  disabled={busy}
                  onChange={(event) => {
                    const kind = event.target.value as BrowserKind;
                    setBrowserKind(kind);
                    setBrowserPath("");
                  }}
                  value={browserKind}
                >
                  <option value="chrome">Google Chrome</option>
                  <option value="edge">Microsoft Edge</option>
                </select>
              </label>
              <label>
                可执行文件路径
                <input
                  disabled={busy}
                  onChange={(event) => setBrowserPath(event.target.value)}
                  placeholder="选择已检测到的浏览器或粘贴绝对路径"
                  value={browserPath}
                />
              </label>
            </div>
            {candidateOptions.length > 0 ? (
              <div className="candidate-list" aria-label="已检测到的浏览器">
                {candidateOptions.map((candidate) => (
                  <button
                    key={candidate.executablePath}
                    onClick={() => chooseBrowser(candidate)}
                    type="button"
                  >
                    使用 {candidate.displayName}
                    {candidate.version === null
                      ? ""
                      : ` (${candidate.version})`}
                  </button>
                ))}
              </div>
            ) : (
              <p className="hint">
                尚未在常见 Windows 安装位置找到该浏览器；可填写绝对路径。
              </p>
            )}

            <fieldset disabled={busy}>
              <legend>网络配置</legend>
              <div className="radio-row">
                <label>
                  <input
                    checked={networkProfile.mode === "direct"}
                    name="network"
                    onChange={() => setNetworkProfile(emptyNetwork())}
                    type="radio"
                  />
                  直连
                </label>
                <label>
                  <input
                    checked={networkProfile.mode === "fixed_proxy"}
                    name="network"
                    onChange={() =>
                      setNetworkProfile({
                        mode: "fixed_proxy",
                        proxyRequired: false,
                        scheme: "socks5",
                        host: "",
                        port: 1080,
                        bypassList: [],
                      })
                    }
                    type="radio"
                  />
                  固定代理
                </label>
                <label>
                  <input
                    checked={networkProfile.mode === "pac"}
                    name="network"
                    onChange={() =>
                      setNetworkProfile({
                        mode: "pac",
                        proxyRequired: false,
                        pacUrl: "",
                      })
                    }
                    type="radio"
                  />
                  PAC
                </label>
              </div>
              {networkProfile.mode === "fixed_proxy" ? (
                <div className="form-grid compact">
                  <label>
                    协议
                    <select
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
                    主机
                    <input
                      onChange={(event) =>
                        setNetworkProfile({
                          ...networkProfile,
                          host: event.target.value,
                        })
                      }
                      value={networkProfile.host}
                    />
                  </label>
                  <label>
                    端口
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
                  <label className="check">
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
                    要求代理预检通过
                  </label>
                </div>
              ) : null}
              {networkProfile.mode === "pac" ? (
                <div className="form-grid compact">
                  <label>
                    PAC URL
                    <input
                      onChange={(event) =>
                        setNetworkProfile({
                          ...networkProfile,
                          pacUrl: event.target.value,
                        })
                      }
                      value={networkProfile.pacUrl}
                    />
                  </label>
                  <label className="check">
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
                    要求代理预检通过
                  </label>
                </div>
              ) : null}
            </fieldset>
            <button
              disabled={
                busy ||
                name.trim().length === 0 ||
                browserPath.trim().length === 0
              }
              onClick={() => void createSilo()}
              type="button"
            >
              创建隔离 Silo
            </button>
          </section>

          <section className="panel">
            <h2>你的 Silos</h2>
            {silos.length === 0 ? (
              <p>还没有 Silo。创建第一个隔离浏览器环境即可开始。</p>
            ) : null}
            <div className="silo-grid">
              {silos
                .filter((silo) => silo.archivedAt === null)
                .map((silo) => (
                  <article className="silo-card" key={silo.id}>
                    <span
                      className="silo-dot"
                      style={{ backgroundColor: silo.color }}
                    />
                    <h3>{silo.name}</h3>
                    <p>
                      {silo.browser.kind === "chrome"
                        ? "Google Chrome"
                        : "Microsoft Edge"}
                    </p>
                    <p>{describeNetwork(silo.networkProfile)}</p>
                    <p className="hint">
                      伴侣扩展：首次启动后由你在该 Silo 内从商店确认安装。
                    </p>
                    <div className="actions">
                      <button
                        disabled={
                          busy || status.activation.activeSiloId !== null
                        }
                        onClick={() => void launchSilo(silo)}
                        type="button"
                      >
                        启动
                      </button>
                      <button
                        className="secondary"
                        disabled={busy}
                        onClick={() => void archiveSilo(silo)}
                        type="button"
                      >
                        归档
                      </button>
                    </div>
                  </article>
                ))}
            </div>
          </section>
        </>
      )}

      <footer>
        VeriSilo 不会承诺设备伪装或不可检测性。TLS、QUIC、硬件、系统字体和通用
        Worker 修改均不在本地扩展与启动器的可靠控制范围内。
      </footer>
    </main>
  );
}
