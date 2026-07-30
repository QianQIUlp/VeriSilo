import { describe, expect, it } from "vitest";

// Vite supplies the raw source during the test transform.
// @ts-expect-error -- `?raw` is resolved by Vite/Vitest at test time.
import appSource from "./App.tsx?raw";

describe("authoritative Vault lock UI cleanup", () => {
  it("scrubs every top-level draft that can retain sensitive data", () => {
    const scrubStart = appSource.indexOf("const scrubSensitiveUi");
    const scrubEnd = appSource.indexOf("const refresh", scrubStart);
    const scrubBlock = appSource.slice(scrubStart, scrubEnd);

    expect(scrubStart).toBeGreaterThan(-1);
    expect(scrubEnd).toBeGreaterThan(scrubStart);
    for (const setter of [
      "setSilos([])",
      "setArchivedSilos([])",
      "setEditingSilo(null)",
      "setNetworkEvidenceHistory([])",
      'setPassphrase("")',
      'setName("")',
      'setBrowserPath("")',
      "setNetworkProfile(emptyNetwork())",
      'setProxyImport("")',
      'setProxyUsername("")',
      'setProxyPassword("")',
      'setMihomoControllerSecret("")',
      "setMihomoSnapshot(null)",
      "setNetworkResult(null)",
      "setBusy(false)",
      "setNotice(null)",
      "scrubDesktopStatusForLockedUi(currentStatus)",
    ]) {
      expect(scrubBlock).toContain(setter);
    }
  });

  it("locks and remounts before the status refresh can complete", () => {
    expect(appSource).toContain("applyVaultUiLock(true)");
    expect(appSource).toContain(
      'key={`${vaultUiGeneration}:${vaultLocked ? "locked" : "unlocked"}`}',
    );
    expect(appSource).toContain("uiVaultLocked ||");
  });

  it("applies Vault status independently from browser discovery", () => {
    expect(appSource).toContain(
      "const browsersPromise = desktopApi.discoverBrowsers().then(",
    );
    expect(appSource).toContain("nextStatus = await desktopApi.status()");
    expect(appSource).not.toContain(
      "const [nextStatus, nextBrowsers] = await Promise.all",
    );
    expect(appSource).toContain('return "stale"');
    expect(appSource).toContain('nextStatus.vault.state === "unlocked"');
    expect(appSource).toContain("scrubDesktopStatusForLockedUi(nextStatus)");
    expect(appSource).toMatch(
      /catch \(error\) \{\s+if \(\s+requestId !== refreshRequestRef\.current \|\|\s+!vaultUiSessionRef\.current\.accepts\(sessionEpoch\)/u,
    );
  });

  it("uses a non-interactive loading boundary while applying a restore", () => {
    expect(appSource).toContain('setVaultTransition("restoring")');
    expect(appSource).toContain("正在载入恢复后的保险库");
    expect(appSource).toContain("正在载入恢复后的权威状态");
    expect(appSource).toContain('result === "stale"');
    expect(appSource).toContain("retryRestoredVaultState");
    expect(appSource).toContain("重新载入权威状态");
    expect(appSource).toContain("加密保险库已恢复，但在载入期间已锁定");

    const completionStart = appSource.indexOf("const completeVaultRestore");
    const completionEnd = appSource.indexOf(
      "const finishVaultRestore",
      completionStart,
    );
    const completionBlock = appSource.slice(completionStart, completionEnd);
    const catchStart = completionBlock.indexOf("} catch (error) {");
    expect(catchStart).toBeGreaterThan(-1);
    expect(completionBlock.slice(0, catchStart)).toContain(
      'setVaultTransition("idle")',
    );
    expect(completionBlock.slice(catchStart)).not.toContain(
      'setVaultTransition("idle")',
    );
  });

  it("rejects late unlocked work and stale busy completions", () => {
    expect(
      appSource.match(/vaultUiSessionRef\.current\.accepts\(sessionEpoch\)/gu)
        ?.length ?? 0,
    ).toBeGreaterThanOrEqual(5);
    expect(appSource).toContain("unlockedOperationRef.current += 1");
    expect(appSource).toContain("networkRequestRef.current += 1");
    expect(appSource).toContain("mihomoRequestRef.current += 1");
    expect(appSource).toContain("operationId === unlockedOperationRef.current");
    expect(appSource).toContain(
      "remoteBusy || vaultLocked || remoteStatus?.pairing !== null",
    );
  });

  it("checks the authoritative deadline synchronously before exporting cached data", () => {
    const exportStart = appSource.indexOf("const downloadLocalReport");
    const exportEnd = appSource.indexOf("const checkNetwork", exportStart);
    const exportBlock = appSource.slice(exportStart, exportEnd);

    expect(exportBlock).toContain(
      "vaultAutoLockDeadlinePassed(status.vault.autoLockAt)",
    );
    expect(exportBlock.indexOf("vaultAutoLockDeadlinePassed")).toBeLessThan(
      exportBlock.indexOf("buildLocalSiloReport"),
    );
    expect(exportBlock).toContain("applyVaultUiLock(true)");
  });
});
