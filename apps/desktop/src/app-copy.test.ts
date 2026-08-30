// @ts-expect-error -- tests run in Node, while the production tsconfig omits Node globals.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Vite's test transform supplies the raw source; the desktop tsconfig does not
// include the broad `vite/client` ambient declarations on purpose.
// @ts-expect-error -- `?raw` is resolved by Vite/Vitest at test time.
import appSource from "./App.tsx?raw";
const stylesSource = readFileSync(
  new URL("./styles.css", import.meta.url),
  "utf8",
);
const createPanelSource = appSource.slice(
  appSource.indexOf("function CreateSiloPanel"),
  appSource.indexOf("function NetworkOption"),
);

describe("desktop product copy", () => {
  it("uses the shared website mark and extension primary color", () => {
    expect(appSource).toContain('const defaultColor = "#5b5ce2";');
    expect(appSource).toContain('src="/verisilo-mark.svg"');
    expect(appSource).toContain('className="brand-mark"');
    expect(appSource).toContain('alt=""');
    expect(appSource).toContain('aria-hidden="true"');
    expect(appSource).not.toMatch(
      /<div className="brand-mark"[^>]*>\s*VS\s*<\/div>/u,
    );
    expect(appSource).not.toContain('const defaultColor = "#0f766e";');
    expect(stylesSource).toContain("--primary: #5b5ce2;");
    expect(stylesSource).toContain("--primary-dark: #4344c5;");
    expect(stylesSource).toContain("--primary-soft: #eeeeff;");
    expect(stylesSource).toContain("--good: #067647;");
    expect(stylesSource).not.toContain("--primary: #0f766e;");
  });

  it("uses task-oriented navigation without exposing roadmap versions", () => {
    expect(appSource).toContain('label="运行位置设置"');
    expect(appSource).toContain("浏览器准备");
    expect(appSource).toContain("Linux 环境");
    expect(appSource).toContain('setEnvironmentSection("remote")');
    expect(appSource).toContain("旧远程环境");
    expect(appSource).not.toContain('label="能力路线"');
    expect(appSource).not.toContain('label="实验室"');
    expect(appSource).not.toMatch(/V0\.[0-9]/u);
    expect(appSource).not.toContain("产品路线");
    expect(appSource).not.toContain("fail closed");
    expect(appSource).not.toContain("环境 UUID");
    expect(appSource).not.toContain("随时可用");
  });

  it("places the user's Silos before diagnostic tools on the overview", () => {
    expect(appSource.indexOf("<SiloList")).toBeGreaterThan(-1);
    expect(appSource.indexOf("<SiloList")).toBeLessThan(
      appSource.indexOf("<NetworkCheckCard"),
    );
  });

  it("creates a Silo through identity, location, network, and visibility confirmation", () => {
    const headings = [
      "给这个 Silo 一个名字",
      "浏览器身份与运行位置",
      "选择网络方式",
      "确认网站将看到什么",
    ];
    const headingIndexes = headings.map((heading) =>
      createPanelSource.indexOf(heading),
    );
    expect(headingIndexes.every((index) => index >= 0)).toBe(true);
    expect(headingIndexes).toEqual(
      [...headingIndexes].sort((left, right) => left - right),
    );
    expect(createPanelSource).toContain("readyWslOptions.map");
    expect(createPanelSource).toContain("Linux 环境当前仅支持直连");
    expect(createPanelSource).not.toContain(
      'className="execution-card unavailable"',
    );
    expect(createPanelSource).toContain("websiteBoundaryConfirmed");
    expect(createPanelSource).not.toContain("Windows Sandbox");
    expect(createPanelSource).not.toContain("Hyper-V");
  });

  it("keeps the Standard Silo default path local, direct, automatic, and short", () => {
    const summaryIndex = createPanelSource.indexOf(
      'className="standard-default-summary"',
    );
    const advancedIndex = createPanelSource.indexOf(
      'className="create-advanced-toggle"',
    );
    expect(summaryIndex).toBeGreaterThan(-1);
    expect(advancedIndex).toBeGreaterThan(summaryIndex);
    expect(createPanelSource).toContain("推荐设置已就绪");
    expect(createPanelSource).toContain(
      "已自动选择本机浏览器并使用 Direct 直连。只需命名、核对边界并确认创建。",
    );
    expect(createPanelSource).toContain(
      "切换浏览器、手工路径、Linux 运行位置或网络方式",
    );
    expect(createPanelSource).toContain("aria-expanded={advancedOpen}");
    expect(createPanelSource).toContain("const nextOpen = !advancedOpen");
    expect(createPanelSource).toContain("hidden={!advancedOpen}");
    expect(appSource).not.toContain("void detectCreateWsl()");
    expect(appSource).toContain(
      "browsers.find((browser) => browser.kind === browserKind) ?? browsers[0]",
    );
    expect(appSource).toContain("!browserSelectionExplicitRef.current");
    expect(
      appSource.match(/browserSelectionExplicitRef\.current = true;/gu)
        ?.length ?? 0,
    ).toBeGreaterThanOrEqual(3);
  });

  it("polls only an active local stock runtime without reloading sensitive panels", () => {
    const pollStart = appSource.indexOf(
      "const pollLocalRuntimeStatus = useCallback",
    );
    const pollEnd = appSource.indexOf("const localRuntimeActive", pollStart);
    const pollSource = appSource.slice(pollStart, pollEnd);
    const intervalStart = appSource.indexOf(
      "if (!localRuntimeActive)",
      pollEnd,
    );
    const intervalEnd = appSource.indexOf(
      "const candidateOptions",
      intervalStart,
    );
    const intervalSource = appSource.slice(pollEnd, intervalEnd);

    expect(pollStart).toBeGreaterThan(-1);
    expect(pollEnd).toBeGreaterThan(pollStart);
    expect(pollSource).toContain("desktopApi.status()");
    expect(pollSource).not.toContain("discoverBrowsers");
    expect(pollSource).not.toContain("listNetworkEvidence");
    expect(pollSource).not.toContain("detectWsl");
    expect(intervalSource).toContain("2_000");
    expect(intervalSource).toContain("window.clearInterval(interval)");
    expect(intervalSource).toContain('silo.executionTarget.kind === "local"');
    expect(intervalSource).toContain('silo.engine.adapter === "stock"');
  });

  it("explains Standard capability evidence and browser-owned stopping honestly", () => {
    for (const state of ["native", "inherit", "unavailable"] as const) {
      expect(appSource).toContain(`<CapabilityState state="${state}" />`);
    }
    expect(appSource).toContain("本机原生");
    expect(appSource).toContain("跟随本机");
    expect(appSource).toContain("当前不可用");
    expect(appSource).toContain(
      "Standard Silo 不提供受控指纹，不会把 Profile 隔离标成指纹验证",
    );
    expect(appSource).toContain("关闭浏览器窗口以停止");
    expect(appSource).toMatch(/不会\s*强制关闭其他 Chrome 或 Edge\s*窗口/u);
  });

  it("keeps a launched identity read-only and stops WSL from the Silo card", () => {
    expect(appSource).toContain("silo.identityLockedAt !== null");
    expect(appSource).toContain("请创建新的 Silo");
    expect(appSource).toContain("desktopApi.stopSilo(silo.id)");
    expect(appSource).toContain('silo.executionTarget.kind === "wsl"');
  });

  it("keeps remote setup in user language and does not forward backend status copy", () => {
    expect(appSource).toContain("远程服务地址");
    expect(appSource).toContain("安全指纹");
    expect(appSource).toContain("一次性配对码");
    expect(appSource).not.toContain("{remoteStatus.message}");
    expect(appSource).not.toContain("严格引擎配置 JSON");
    expect(appSource).not.toContain("Agent 已接受");
    expect(appSource).not.toContain("message: verification.message");
    expect(appSource).not.toContain("remoteStatus.pairing.node.cost.notice");
    expect(appSource).not.toContain("安全连接可用");
    expect(appSource).not.toContain("此设备已配对");
  });

  it("exposes only cleanup actions for legacy remote bindings", () => {
    const recoveryStart = appSource.indexOf(
      'className="remote-recovery-warning"',
    );
    const recoveryEnd = appSource.indexOf(
      'environmentSection === "local"',
      recoveryStart,
    );
    const recoverySource = appSource.slice(recoveryStart, recoveryEnd);
    expect(recoveryStart).toBeGreaterThan(-1);
    expect(recoveryEnd).toBeGreaterThan(recoveryStart);
    for (const action of [
      'runRemoteCleanupOperation("stop")',
      'runRemoteCleanupOperation("health")',
      'runRemoteCleanupOperation("logs")',
      'runRemoteCleanupOperation("destroy")',
      "checkRemoteDeletionStatus()",
      "removeLocalRemoteConnection()",
    ]) {
      expect(recoverySource).toContain(action);
    }
    for (const forbidden of [
      "desktopApi.createRemoteEnvironment",
      "desktopApi.startRemoteEnvironment",
      "desktopApi.pauseRemoteEnvironment",
      "desktopApi.snapshotRemoteEnvironment",
      "desktopApi.configureRemoteEnvironmentNetwork",
      "pairRemoteEndpoint()",
      "runRemoteInteraction(",
    ]) {
      expect(recoverySource).not.toContain(forbidden);
    }
  });

  it("labels managed browsers accurately and only offers the safe system-browser switch", () => {
    expect(appSource).toContain("独立 Chromium");
    expect(appSource).toContain("Camoufox（Firefox 兼容）");
    expect(appSource).toContain('engine: { adapter: "stock" }');
    expect(appSource).toContain("managedBrowserNetworkMismatch");
    expect(appSource).not.toContain("identityTemplate.templateId");
  });

  it("keeps managed-browser creation bounded and accessible", () => {
    expect(appSource).toContain("系统浏览器");
    expect(appSource).toContain("托管身份浏览器");
    expect(appSource).toContain("请从托盘菜单退出");
    expect(appSource).toContain("managedEngineReady");
    expect(appSource).toContain(
      "disabled={!managedEngineReady || managedStatusBusy}",
    );
    expect(appSource).toContain("onSubmit={(event) => void submit(event)}");
    expect(appSource).toContain('role="alert"');
    expect(appSource).toContain("重试");
    expect(appSource).toContain("Direct 直连");
    expect(appSource).toContain("HTTP");
    expect(appSource).toContain("SOCKS5");
    expect(appSource).not.toContain("managed-package-path");
    expect(appSource).not.toContain("artifactFileSha256");
  });

  it("stops a running Camoufox Silo through the shared stop command", () => {
    expect(appSource).toContain(
      'const managedCamoufox = silo.engine.adapter === "camoufox";',
    );
    expect(appSource).toContain('"停止托管浏览器"');
    expect(appSource).toContain("onStop(silo)");
    expect(appSource).toContain("一次只运行一个 Silo");
  });

  it("keeps managed package, network, and binding evidence states distinct", () => {
    for (const state of [
      "configured",
      "reachable",
      "applied",
      "observed",
      "verified",
      "unavailable",
    ]) {
      expect(appSource).toContain(`${state}:`);
    }
    expect(appSource).toContain(
      'engineEvidence?.packageVerification === "verified"',
    );
    expect(appSource).toContain(
      'engineEvidence?.verifiedAdapter === "camoufox"',
    );
    expect(appSource).toContain('networkEvidence.exit === "observed"');
  });

  it("requires fresh approval after any remote pairing or fingerprint change", () => {
    expect(
      appSource.match(/setRemotePairingApproved\(false\)/gu)?.length ?? 0,
    ).toBeGreaterThanOrEqual(6);
    expect(
      appSource.match(/setRemoteRotationApproved\(false\)/gu)?.length ?? 0,
    ).toBeGreaterThanOrEqual(6);
    expect(appSource).toContain("!remotePairingFieldsValid");
    expect(appSource).toContain("!remoteRotationFieldsValid");
  });

  it("keeps remote confirmation checkboxes at intrinsic width", () => {
    expect(stylesSource).toMatch(
      /\.remote-confirmation input\s*\{[^}]*width:\s*auto;[^}]*min-height:\s*auto;/u,
    );
  });
});
