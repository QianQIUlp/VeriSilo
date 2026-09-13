import { type Notice } from "../../shared/notice.js";

import { useRef, useState, type ReactNode } from "react";

import { InstrumentObject } from "../../shared/LivingWorkspace.js";
import { WorkspaceSheet } from "../../shared/WorkspaceSheet.js";

import { UserFacingError } from "../../user-errors.js";

import { desktopApi } from "../../desktop-api.js";

import { formatBytes } from "../../shared/presentation.js";

export function VaultAndDataPanel({
  feedback,
  busy,
  onNotice,
  onRefresh,
  onVaultRestored,
  runBusy,
}: {
  feedback?: ReactNode;
  busy: boolean;
  onNotice: (notice: Notice) => void;
  onRefresh: () => Promise<unknown>;
  onVaultRestored: () => Promise<void>;
  runBusy: (
    action: (isCurrent: () => boolean) => Promise<void>,
  ) => Promise<void>;
}) {
  const [tool, setTool] = useState<"passphrase" | "backup" | "restore" | null>(
    null,
  );
  const [backupComplete, setBackupComplete] = useState(false);
  const [aim, setAim] = useState<typeof tool>(null);
  const pull = useRef<{
    x: number;
    y: number;
    moved: boolean;
    aim: typeof tool;
  } | null>(null);
  const pulled = useRef(false);
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
      setBackupComplete(true);
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
    <section
      className={`vault-chamber living-page${tool ? " chamber-open" : ""}${backupComplete ? " backup-complete" : ""}`}
    >
      <header className="living-heading">
        <p className="eyebrow">03 / THE VAULT</p>
        <h1>配置，在你手中。</h1>
        <p>一份留在本机的加密核心。选择一个方向，展开它。</p>
      </header>
      <div className="vault-object-stage" data-aim={aim ?? "none"}>
        <div className="vault-orbit-line" aria-hidden="true" />
        <button
          className="vault-core"
          type="button"
          aria-label="拖动保险库核心选择操作，或点击打开备份"
          aria-haspopup="dialog"
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            event.currentTarget.focus({ preventScroll: true });
            pull.current = {
              x: event.clientX,
              y: event.clientY,
              moved: false,
              aim: null,
            };
            pulled.current = false;
            event.currentTarget.setPointerCapture(event.pointerId);
            event.currentTarget.classList.add("core-pulling");
          }}
          onPointerMove={(event) => {
            const start = pull.current;
            if (!start) return;
            const dx = event.clientX - start.x,
              dy = event.clientY - start.y;
            start.moved ||= Math.hypot(dx, dy) > 12;
            event.currentTarget.style.setProperty("--pull-x", `${dx * 0.55}px`);
            event.currentTarget.style.setProperty("--pull-y", `${dy * 0.55}px`);
            let target: typeof tool = null;
            if (Math.hypot(dx, dy) > 50) {
              const targets = Array.from(
                event.currentTarget.parentElement!.querySelectorAll<HTMLButtonElement>(
                  "[data-vault-tool]",
                ),
              );
              targets.sort((a, b) => {
                const ar = a.getBoundingClientRect(),
                  br = b.getBoundingClientRect();
                return (
                  Math.hypot(
                    event.clientX - ar.x - ar.width / 2,
                    event.clientY - ar.y - ar.height / 2,
                  ) -
                  Math.hypot(
                    event.clientX - br.x - br.width / 2,
                    event.clientY - br.y - br.height / 2,
                  )
                );
              });
              target = (targets[0]?.dataset.vaultTool as typeof tool) ?? null;
            }
            if (target !== start.aim) {
              start.aim = target;
              setAim(target);
            }
          }}
          onPointerUp={(event) => {
            pulled.current = pull.current?.moved ?? false;
            const target = pull.current?.aim;
            pull.current = null;
            setAim(null);
            event.currentTarget.classList.remove("core-pulling");
            event.currentTarget.style.removeProperty("--pull-x");
            event.currentTarget.style.removeProperty("--pull-y");
            if (target) setTool(target);
          }}
          onPointerCancel={(event) => {
            pull.current = null;
            pulled.current = true;
            setAim(null);
            event.currentTarget.classList.remove("core-pulling");
            event.currentTarget.style.removeProperty("--pull-x");
            event.currentTarget.style.removeProperty("--pull-y");
          }}
          onClick={(event) => {
            if (event.detail === 0 || !pulled.current) setTool("backup");
          }}
        >
          <InstrumentObject kind="vault" active={tool !== null} />
          <span className="vault-core-label">
            ENCRYPTED CONFIGURATION<small>Silo 配置 · 身份 · 网络设置</small>
          </span>
          <span className="vault-pull-hint">
            {aim ? "松开，展开操作" : "拖动核心选择操作 · 点击查看备份"}
          </span>
        </button>
        <button
          className="vault-action vault-action-key"
          data-vault-tool="passphrase"
          type="button"
          onClick={() => setTool("passphrase")}
          aria-haspopup="dialog"
        >
          <span aria-hidden="true">01 / ⌁</span>
          <strong>更换口令</strong>
          <small>重新设定开启方式</small>
          <i aria-hidden="true">↗</i>
        </button>
        <button
          className="vault-action vault-action-backup"
          data-vault-tool="backup"
          type="button"
          onClick={() => setTool("backup")}
          aria-haspopup="dialog"
        >
          <span aria-hidden="true">02 / ↗</span>
          <strong>抽取加密备份</strong>
          <small>
            {backupComplete ? "本次备份已生成" : "将配置留一份副本"}
          </small>
          <i aria-hidden="true">↗</i>
        </button>
        <button
          className="vault-action vault-action-restore"
          data-vault-tool="restore"
          type="button"
          onClick={() => setTool("restore")}
          aria-haspopup="dialog"
        >
          <span aria-hidden="true">03 / ↙</span>
          <strong>从备份恢复</strong>
          <small>验证后替换当前记录</small>
          <i aria-hidden="true">↗</i>
        </button>
      </div>
      <p className="chamber-boundary">
        <span aria-hidden="true">◌</span> 浏览器登录、Cookie
        与网站文件独立保存，不在保险库备份中。
      </p>
      {tool !== null && (
        <WorkspaceSheet
          variant="console"
          title={
            tool === "passphrase"
              ? "更换保险库口令"
              : tool === "backup"
                ? "抽取加密备份"
                : "从备份恢复"
          }
          onClose={() => setTool(null)}
        >
          {feedback}
          {tool === "passphrase" ? (
            <section className="panel settings-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">保险库口令</p>
                  <h2>更换保护本地配置的口令</h2>
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
                    onChange={(event) =>
                      setCurrentPassphrase(event.target.value)
                    }
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
                    onChange={(event) =>
                      setConfirmPassphrase(event.target.value)
                    }
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
          ) : null}
          {tool === "backup" ? (
            <section className="panel settings-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">加密备份</p>
                  <h2>备份 Silo 配置，不含登录数据</h2>
                  <p>
                    备份里是加密后的配置和网络设置，没有
                    Cookie、浏览记录或网站文件。
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
          ) : null}
          {tool === "restore" ? (
            <section className="panel settings-panel danger-zone">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">恢复保险库</p>
                  <h2>覆盖前先验证备份口令和格式</h2>
                  <p>
                    这会替换当前保险库记录，但不会自动删除、复制或覆盖任何浏览器数据。
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
                  onChange={(event) =>
                    setConfirmOverwrite(event.target.checked)
                  }
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
          ) : null}
        </WorkspaceSheet>
      )}
    </section>
  );
}
