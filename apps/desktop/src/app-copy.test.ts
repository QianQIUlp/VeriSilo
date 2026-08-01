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
    expect(appSource).not.toContain('setEnvironmentSection("remote")');
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

  it("keeps implemented remote operations available without exposing internal receipts", () => {
    expect(appSource).toContain("desktopApi.openRemoteHumanSession");
    expect(appSource).toContain("desktopApi.closeRemoteHumanSession");
    expect(appSource).toContain("desktopApi.grantRemoteAutomation");
    expect(appSource).toContain("desktopApi.revokeRemoteAutomation");
    expect(appSource).toContain("desktopApi.openRemoteScreen");
    expect(appSource).toContain("desktopApi.sendRemoteInput");
    expect(appSource).toContain("desktopApi.recoverRemoteDeletionProof");
    expect(appSource).toContain("desktopApi.forceDetachRemoteEnvironment");
  });

  it("labels managed browsers accurately and only offers the safe system-browser switch", () => {
    expect(appSource).toContain("独立 Chromium");
    expect(appSource).toContain("Camoufox（Firefox 兼容）");
    expect(appSource).toContain('engine: { adapter: "stock" }');
    expect(appSource).toContain("managedBrowserNetworkMismatch");
    expect(appSource).not.toContain("identityTemplate.templateId");
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
