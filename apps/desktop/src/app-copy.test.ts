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

describe("desktop product copy", () => {
  it("uses task-oriented navigation without exposing roadmap versions", () => {
    expect(appSource).toContain('label="运行环境"');
    expect(appSource).toContain("浏览器");
    expect(appSource).toContain("本机隔离");
    expect(appSource).toContain("远程环境");
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
