import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Silo } from "@verisilo/contracts";

import {
  desktopApi,
  type ManagedSiloBackupInspection,
} from "../../desktop-api.js";
import { formatBytes } from "../../shared/presentation.js";
import { type Notice } from "../../shared/notice.js";
import { WorkspaceSheet } from "../../shared/WorkspaceSheet.js";
import { UserFacingError } from "../../user-errors.js";

type Operation = "backup" | "restore";
type Inspection = ManagedSiloBackupInspection & { sourcePath: string };

export function ManagedSiloBackupPanel({
  silos,
  activeSiloId,
  busy,
  feedback,
  onNotice,
  onRefresh,
  runBusy,
}: {
  silos: Silo[];
  activeSiloId: string | null;
  busy: boolean;
  feedback?: ReactNode;
  onNotice: (notice: Notice | null) => void;
  onRefresh: () => Promise<unknown>;
  runBusy: (
    action: (isCurrent: () => boolean) => Promise<void>,
  ) => Promise<void>;
}) {
  const managedSilos = silos.filter(
    (silo) =>
      silo.engine.adapter === "camoufox" && silo.executionTarget.kind === "local",
  );
  const [operation, setOperation] = useState<Operation | null>(null);
  const [siloId, setSiloId] = useState("");
  const [backupPath, setBackupPath] = useState("");
  const [backupPassphrase, setBackupPassphrase] = useState("");
  const [confirmPassphrase, setConfirmPassphrase] = useState("");
  const [sourcePath, setSourcePath] = useState("");
  const [restorePassphrase, setRestorePassphrase] = useState("");
  const [inspection, setInspection] = useState<Inspection | null>(null);
  const [confirmOverwrite, setConfirmOverwrite] = useState(false);
  const [pending, setPending] = useState<"backup" | "inspect" | "restore" | null>(null);
  const inFlight = useRef(false);
  const generation = useRef(0);
  const mounted = useRef(true);
  const selectedSilo = managedSilos.find((silo) => silo.id === siloId) ?? managedSilos[0];
  const running = selectedSilo?.id === activeSiloId;
  const backupPassphraseCharacters = Array.from(backupPassphrase).length;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      generation.current += 1;
    };
  }, []);

  const clearInputs = () => {
    generation.current += 1;
    setBackupPath("");
    setBackupPassphrase("");
    setConfirmPassphrase("");
    setSourcePath("");
    setRestorePassphrase("");
    setInspection(null);
    setConfirmOverwrite(false);
  };

  const open = (next: Operation) => {
    clearInputs();
    setOperation(next);
  };

  const close = () => {
    if (inFlight.current) return;
    clearInputs();
    setOperation(null);
  };

  const execute = (
    kind: "backup" | "inspect" | "restore",
    action: (isCurrent: () => boolean, silo: Silo) => Promise<void>,
  ) => {
    if (busy || inFlight.current || !selectedSilo || running) return;
    inFlight.current = true;
    setPending(kind);
    onNotice(null);
    const currentGeneration = generation.current;
    void runBusy(async (sessionCurrent) => {
      const isCurrent = () =>
        sessionCurrent() &&
        mounted.current &&
        generation.current === currentGeneration;
      await action(isCurrent, selectedSilo);
    }).finally(() => {
      inFlight.current = false;
      if (mounted.current) setPending(null);
    });
  };

  const backup = () =>
    execute("backup", async (isCurrent, silo) => {
      if (backupPassphraseCharacters < 12 || backupPassphrase !== confirmPassphrase) {
        throw new UserFacingError("备份口令至少 12 个字符，且两次输入必须一致。");
      }
      const receipt = await desktopApi.backupManagedSilo(
        silo.id,
        backupPath.trim(),
        backupPassphrase,
      );
      if (!isCurrent()) return;
      setBackupPassphrase("");
      setConfirmPassphrase("");
      onNotice({
        tone: "success",
        message: `“${silo.name}”的完整冷备份已保存到 ${receipt.destinationPath}（${formatBytes(receipt.bytes)}，浏览器数据 ${formatBytes(receipt.profileBytes)}）。请妥善保管独立备份口令。`,
      });
    });

  const inspect = () =>
    execute("inspect", async (isCurrent, silo) => {
      const path = sourcePath.trim();
      const result = await desktopApi.inspectManagedSiloBackup(
        silo.id,
        path,
        restorePassphrase,
      );
      if (!isCurrent()) return;
      if (result.siloId !== silo.id) {
        throw new UserFacingError("此备份属于另一个 Silo。请选择原来的 Managed Silo。");
      }
      setInspection({ ...result, sourcePath: path });
      setConfirmOverwrite(false);
    });

  const restore = () =>
    execute("restore", async (isCurrent, silo) => {
      if (
        !inspection ||
        inspection.siloId !== silo.id ||
        inspection.sourcePath !== sourcePath.trim() ||
        !confirmOverwrite
      ) {
        throw new UserFacingError("备份信息已变化，请重新检查并确认覆盖。");
      }
      await desktopApi.restoreManagedSiloBackup(
        silo.id,
        inspection.sourcePath,
        restorePassphrase,
        inspection.archiveSha256,
        true,
      );
      if (!isCurrent()) return;
      await onRefresh();
      if (!isCurrent()) return;
      clearInputs();
      setOperation(null);
      onNotice({
        tone: "success",
        message: `“${silo.name}”已恢复原身份。请重新启动此 Silo，取得当前身份和网络证据；旧运行记录仍是历史记录。`,
      });
    });

  return (
    <>
      <section className="managed-backup-entry" aria-labelledby="managed-backup-title">
        <div>
          <p className="eyebrow">MANAGED / COLD BACKUP</p>
          <h2 id="managed-backup-title">完整备份一个 Managed Silo</h2>
          <p>
            加密保存 Profile、原始身份与网络绑定；在同机同用户同 Vault 恢复。
          </p>
        </div>
        <div className="managed-backup-entry-actions">
          <button
            disabled={managedSilos.length === 0 || busy}
            onClick={() => open("backup")}
            type="button"
          >
            完整冷备份
          </button>
          <button
            className="button-secondary"
            disabled={managedSilos.length === 0 || busy}
            onClick={() => open("restore")}
            type="button"
          >
            恢复原身份
          </button>
        </div>
        {managedSilos.length === 0 ? (
          <p className="managed-backup-empty">
            当前 Vault 没有可用的本地 Managed Silo。若原 Silo 元数据已删除，请先恢复 Vault 配置备份。
          </p>
        ) : null}
      </section>
      {operation !== null && (
        <WorkspaceSheet
          variant="console"
          title={operation === "backup" ? "Managed Silo 完整冷备份" : "恢复 Managed Silo 原身份"}
          closeDisabled={pending !== null}
          onClose={close}
        >
          {feedback}
          <section className={`panel settings-panel${operation === "restore" ? " danger-zone" : ""}`}>
            <div className="panel-heading">
              <div>
                <p className="eyebrow">{operation === "backup" ? "完整冷备份" : "原身份恢复"}</p>
                <h2>{operation === "backup" ? "保存整个 Managed 身份" : "先检查备份，再确认覆盖"}</h2>
                <p>
                  {operation === "backup"
                    ? "先停止目标 Silo。备份使用独立口令，包含 Profile 和身份相关配置，不包含引擎程序或当前运行证据。"
                    : "仅恢复原 UUID 的 Silo，不创建副本。原元数据须仍在 Vault；若已删除，请先恢复 Vault 配置备份。"}
                </p>
              </div>
            </div>
            <label className="managed-backup-target">
              目标 Managed Silo
              <select
                disabled={busy || pending !== null}
                onChange={(event) => {
                  setSiloId(event.target.value);
                  clearInputs();
                }}
                value={selectedSilo?.id ?? ""}
              >
                {managedSilos.map((silo) => (
                  <option key={silo.id} value={silo.id}>
                    {silo.name}{silo.archivedAt ? "（已归档）" : ""}
                  </option>
                ))}
              </select>
            </label>
            {running ? (
              <p className="field-error" role="alert">此 Silo 正在运行。请先停止它，等浏览器数据释放后再操作。</p>
            ) : null}
            {pending !== null ? (
              <p className="managed-backup-pending" role="status">
                {pending === "backup" ? "正在创建完整备份…" : pending === "inspect" ? "正在检查备份…" : "正在恢复原身份；完成前请保持窗口打开…"}
              </p>
            ) : null}
            {operation === "backup" ? (
              <>
                <div className="form-grid">
                  <label>
                    保存到
                    <input
                      autoComplete="off"
                      disabled={busy || pending !== null}
                      onChange={(event) => setBackupPath(event.target.value)}
                      placeholder="C:\\Users\\你\\Documents\\managed-silo.backup"
                      spellCheck={false}
                      value={backupPath}
                    />
                  </label>
                  <label>
                    独立备份口令（至少 12 个字符）
                    <input
                      autoComplete="new-password"
                      disabled={busy || pending !== null}
                      minLength={12}
                      onChange={(event) => setBackupPassphrase(event.target.value)}
                      type="password"
                      value={backupPassphrase}
                    />
                  </label>
                  <label>
                    再输一次备份口令
                    <input
                      autoComplete="new-password"
                      disabled={busy || pending !== null}
                      minLength={12}
                      onChange={(event) => setConfirmPassphrase(event.target.value)}
                      type="password"
                      value={confirmPassphrase}
                    />
                  </label>
                </div>
                <button
                  disabled={busy || pending !== null || running || backupPath.trim().length === 0 || backupPassphraseCharacters < 12 || backupPassphrase !== confirmPassphrase}
                  onClick={backup}
                  type="button"
                >
                  创建完整冷备份
                </button>
              </>
            ) : (
              <>
                <div className="form-grid">
                  <label>
                    完整备份文件
                    <input
                      autoComplete="off"
                      disabled={busy || pending !== null}
                      onChange={(event) => {
                        setSourcePath(event.target.value);
                        generation.current += 1;
                        setInspection(null);
                        setConfirmOverwrite(false);
                      }}
                      spellCheck={false}
                      value={sourcePath}
                    />
                  </label>
                  <label>
                    该备份的独立口令
                    <input
                      autoComplete="off"
                      disabled={busy || pending !== null}
                      onChange={(event) => {
                        setRestorePassphrase(event.target.value);
                        generation.current += 1;
                        setInspection(null);
                        setConfirmOverwrite(false);
                      }}
                      type="password"
                      value={restorePassphrase}
                    />
                  </label>
                </div>
                <button
                  className="button-secondary"
                  disabled={busy || pending !== null || running || sourcePath.trim().length === 0 || restorePassphrase.length === 0}
                  onClick={inspect}
                  type="button"
                >
                  检查备份内容
                </button>
                {inspection && inspection.siloId === selectedSilo?.id && inspection.sourcePath === sourcePath.trim() ? (
                  <div className="managed-backup-inspection">
                    <h3>已检查的备份</h3>
                    <dl className="managed-backup-facts">
                      <div><dt>备份中的 Silo</dt><dd>{inspection.siloName}</dd></div>
                      <div><dt>备份时间</dt><dd>{new Date(inspection.createdAt).toLocaleString("zh-CN")}</dd></div>
                      <div><dt>浏览器数据</dt><dd>{formatBytes(inspection.profileBytes)} · {inspection.fileCount} 个文件</dd></div>
                      <div><dt>引擎版本</dt><dd>Camoufox {inspection.engineVersion}</dd></div>
                      <div><dt>网络配置</dt><dd>{inspection.networkSummary}</dd></div>
                      <div><dt>身份制品</dt><dd>{inspection.artifactId}</dd></div>
                      <div className="managed-backup-digest"><dt>所检查文件的 SHA-256</dt><dd><code>{inspection.archiveSha256}</code></dd></div>
                    </dl>
                    <p className="managed-backup-consequence">
                      恢复将覆盖“{selectedSilo?.name}”当前的浏览器数据、身份制品与绑定、网络配置及凭据。
                      UUID 和归档状态保持；旧运行记录仍为历史，须重新启动取得当前证据。
                    </p>
                    <label className="check-field">
                      <input
                        checked={confirmOverwrite}
                        disabled={busy || pending !== null || running}
                        onChange={(event) => setConfirmOverwrite(event.target.checked)}
                        type="checkbox"
                      />
                      我确认用这份已检查的备份覆盖原 Silo 的当前数据
                    </label>
                    <button
                      className="button-danger"
                      disabled={busy || pending !== null || running || !confirmOverwrite}
                      onClick={restore}
                      type="button"
                    >
                      确认覆盖并恢复原身份
                    </button>
                  </div>
                ) : null}
              </>
            )}
          </section>
        </WorkspaceSheet>
      )}
    </>
  );
}
