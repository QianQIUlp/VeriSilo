import { InstrumentObject } from "../../shared/LivingWorkspace.js";
import { type DesktopStatus } from "../../desktop-api.js";

import { useEffect, useState } from "react";

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
  const [passphraseConfirm, setPassphraseConfirm] = useState("");
  useEffect(() => {
    if (passphrase === "") {
      setPassphraseConfirm("");
    }
  }, [passphrase]);
  const confirmPending =
    initialize && passphrase.length >= 12 && passphraseConfirm !== passphrase;
  const confirmHint = !confirmPending
    ? null
    : passphraseConfirm === ""
      ? "请再输入一次相同的口令进行确认。"
      : "两次输入的口令不一致，请检查后重试。";
  return (
    <section className={`vault-entry${busy ? " entry-opening" : ""}`}>
      <div className="entry-orbit" aria-hidden="true">
        <InstrumentObject kind="vault" active={busy} />
      </div>
      <header className="entry-heading">
        <p className="eyebrow">本地保险库</p>
        <h1>{initialize ? "先保护你的 Silo 配置" : "你的世界，留在这里。"}</h1>
      </header>
      <form
        className="entry-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (!busy && passphrase.length >= 12 && !confirmPending)
            void submitVault();
        }}
      >
        <h2>{initialize ? "创建保险库" : "解锁保险库"}</h2>
        <p>请输入至少 12 个字符。遗忘口令后无法恢复。</p>
        <label>
          保险库口令
          <input
            aria-label="保险库口令"
            autoFocus
            autoComplete={initialize ? "new-password" : "current-password"}
            disabled={busy}
            id="vault-passphrase"
            minLength={12}
            onChange={(event) => setPassphrase(event.target.value)}
            type="password"
            value={passphrase}
          />
        </label>
        {initialize ? (
          <label>
            再次输入口令
            <input
              aria-label="再次输入保险库口令"
              autoComplete="new-password"
              disabled={busy}
              onChange={(event) => setPassphraseConfirm(event.target.value)}
              type="password"
              value={passphraseConfirm}
            />
          </label>
        ) : null}
        {confirmHint !== null ? (
          <p className="field-error" role="alert">
            {confirmHint}
          </p>
        ) : null}
        <button
          disabled={busy || passphrase.length < 12 || confirmPending}
          type="submit"
        >
          {busy ? "正在打开…" : initialize ? "创建本地保险库" : "解锁"}
          <span aria-hidden="true">↗</span>
        </button>
      </form>
      <div className="entry-boundary">
        <p>
          保险库会加密保存 Silo 配置、身份和可选网络设置。每个 Silo
          的登录和网站数据放在这台电脑的独立文件夹里，不会写进保险库。
        </p>
        <ul className="plain-list">
          <li>默认 15 分钟自动锁定</li>
          <li>口令只在本机使用</li>
          <li>没有云端找回机制</li>
        </ul>
      </div>
    </section>
  );
}
