import { type Notice } from "../../shared/notice.js";

import { useState } from "react";

import { UserFacingError } from "../../user-errors.js";

import { desktopApi } from "../../desktop-api.js";

import { formatBytes } from "../../shared/presentation.js";

export function VaultAndDataPanel({
  busy,
  onNotice,
  onRefresh,
  onVaultRestored,
  runBusy,
}: {
  busy: boolean;
  onNotice: (notice: Notice) => void;
  onRefresh: () => Promise<unknown>;
  onVaultRestored: () => Promise<void>;
  runBusy: (
    action: (isCurrent: () => boolean) => Promise<void>,
  ) => Promise<void>;
}) {
  const [currentPassphrase, setCurrentPassphrase] = useState("");
  const [newPassphrase, setNewPassphrase] = useState("");
  const [confirmPassphrase, setConfirmPassphrase] = useState("");
  const [backupPath, setBackupPath] = useState("");
  const [restorePath, setRestorePath] = useState("");
  const [restorePassphrase, setRestorePassphrase] = useState("");
  const [confirmOverwrite, setConfirmOverwrite] = useState(false);

  const changePassphrase = () =>
    runBusy(async (isCurrent) => {
      if (newPassphrase.length < 12) {
        throw new UserFacingError("新口令至少需要 12 个字符。");
      }
      if (newPassphrase !== confirmPassphrase) {
        throw new UserFacingError("两次输入的新口令不一致。");
      }
      await desktopApi.changeVaultPassphrase(currentPassphrase, newPassphrase);
      if (!isCurrent()) {
        return;
      }
      setCurrentPassphrase("");
      setNewPassphrase("");
      setConfirmPassphrase("");
      onNotice({
        tone: "success",
        message: "口令已更换。浏览器里的登录数据不会变。",
      });
      await onRefresh();
    });

  const backupVault = () =>
    runBusy(async (isCurrent) => {
      const receipt = await desktopApi.backupVault(backupPath.trim());
      if (!isCurrent()) {
        return;
      }
      onNotice({
        tone: "success",
        message: `已备份加密保险库：${receipt.destinationPath}（${formatBytes(receipt.bytes)}）。浏览器数据未包含在内。`,
      });
    });

  const restoreVault = () =>
    runBusy(async (isCurrent) => {
      if (!confirmOverwrite) {
        throw new UserFacingError("请先确认覆盖当前保险库记录。");
      }
      await desktopApi.restoreVault(
        restorePath.trim(),
        restorePassphrase,
        true,
      );
      if (!isCurrent()) {
        return;
      }
      await onVaultRestored();
    });

  return (
    <div className="settings-stack">
      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">保险库口令</p>
            <h1>更换保护本地配置的口令</h1>
            <p>
              口令忘了就找不回。换口令后，旧备份还是旧口令才能打开，请当敏感文件保管。
            </p>
          </div>
        </div>
        <div className="form-grid three-columns">
          <label>
            当前口令
            <input
              autoComplete="current-password"
              disabled={busy}
              onChange={(event) => setCurrentPassphrase(event.target.value)}
              type="password"
              value={currentPassphrase}
            />
          </label>
          <label>
            新口令
            <input
              autoComplete="new-password"
              disabled={busy}
              minLength={12}
              onChange={(event) => setNewPassphrase(event.target.value)}
              type="password"
              value={newPassphrase}
            />
          </label>
          <label>
            再输一次
            <input
              autoComplete="new-password"
              disabled={busy}
              minLength={12}
              onChange={(event) => setConfirmPassphrase(event.target.value)}
              type="password"
              value={confirmPassphrase}
            />
          </label>
        </div>
        <button
          disabled={
            busy ||
            currentPassphrase.length === 0 ||
            newPassphrase.length < 12 ||
            newPassphrase !== confirmPassphrase
          }
          onClick={() => void changePassphrase()}
          type="button"
        >
          更新保险库口令
        </button>
      </section>

      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">加密备份</p>
            <h2>备份 Silo 配置，不含登录数据</h2>
            <p>
              备份里是加密后的配置和网络设置，没有 Cookie、浏览记录或网站文件。
            </p>
          </div>
        </div>
        <label>
          保存到
          <input
            autoComplete="off"
            disabled={busy}
            onChange={(event) => setBackupPath(event.target.value)}
            placeholder="C:\\Users\\你\\Documents\\verisilo-vault.backup"
            spellCheck={false}
            value={backupPath}
          />
        </label>
        <button
          disabled={busy || backupPath.trim().length === 0}
          onClick={() => void backupVault()}
          type="button"
        >
          创建加密备份
        </button>
      </section>

      <section className="panel settings-panel danger-zone">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">恢复保险库</p>
            <h2>覆盖前先验证备份口令和格式</h2>
            <p>
              这会替换当前保险库记录，但不会自动删除、复制或覆盖任何浏览器
              浏览器数据。
            </p>
          </div>
        </div>
        <div className="form-grid">
          <label>
            备份文件
            <input
              autoComplete="off"
              disabled={busy}
              onChange={(event) => {
                setRestorePath(event.target.value);
                setConfirmOverwrite(false);
              }}
              spellCheck={false}
              value={restorePath}
            />
          </label>
          <label>
            该备份的口令
            <input
              autoComplete="off"
              disabled={busy}
              onChange={(event) => {
                setRestorePassphrase(event.target.value);
                setConfirmOverwrite(false);
              }}
              type="password"
              value={restorePassphrase}
            />
          </label>
        </div>
        <label className="check-field">
          <input
            checked={confirmOverwrite}
            disabled={busy}
            onChange={(event) => setConfirmOverwrite(event.target.checked)}
            type="checkbox"
          />
          我确认覆盖当前保险库记录，并理解浏览器数据不在此备份中
        </label>
        <button
          className="button-danger"
          disabled={
            busy ||
            !confirmOverwrite ||
            restorePath.trim().length === 0 ||
            restorePassphrase.length === 0
          }
          onClick={() => void restoreVault()}
          type="button"
        >
          验证并恢复保险库
        </button>
      </section>
    </div>
  );
}
