import { type DesktopStatus } from "../../desktop-api.js";

export function VaultAccess({
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
          保险库会加密保存 Silo 配置、身份和可选网络设置。每个 Silo
          的登录和网站数据放在这台电脑的独立文件夹里，不会写进保险库。
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
