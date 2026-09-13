import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ManagedIdentityPreview } from "../../desktop-api.js";
import { previewSilo, previewStatus } from "../../preview/fixtures.js";
import { SiloList } from "./SiloList.js";

const identityPreview: ManagedIdentityPreview = {
  userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
  language: "zh-CN",
  timezone: "Asia/Shanghai",
  screenWidth: 1920,
  screenHeight: 1080,
  hardwareConcurrency: 8,
  webglVendor: "NVIDIA Corporation",
  webglRenderer: "NVIDIA GeForce RTX 3060, or similar",
  platform: "Win32",
  countryCode: null,
  publicAddress: null,
  latitude: null,
  longitude: null,
  networkBound: false,
};

describe("Silo list presentation", () => {
  it("renders empty and occupied workspaces without a desktop runtime", () => {
    const noAction = async () => {};
    const props = {
      activation: null,
      busy: false,
      managedEngineReady: false,
      networkEvidence: [],
      identityPreviews: {},
      storageUsage: {},
      onArchive: noAction,
      onCreate: noAction,
      onEdit: noAction,
      onLaunch: noAction,
      onRebindMihomo: noAction,
      onRecheckBrowser: noAction,
      onRecheckRuntime: noAction,
      onStop: noAction,
      runtimeActivation: previewStatus().activation,
      runtimeState: "idle" as const,
    };
    const empty = renderToStaticMarkup(
      createElement(SiloList, { ...props, silos: [] }),
    );
    expect(empty).toContain("还没有 Silo");
    const populated = renderToStaticMarkup(
      createElement(SiloList, { ...props, silos: [previewSilo] }),
    );
    expect(populated).toContain(previewSilo.name);
    expect(populated).toContain("打开浏览器");
    expect(populated).not.toContain("还没有 Silo");
    const missingActive = renderToStaticMarkup(
      createElement(SiloList, {
        ...props,
        activation: "removed-silo",
        silos: [previewSilo],
      }),
    );
    expect(missingActive).toContain(`id="silo-${previewSilo.id}" aria-label=`);
    expect(missingActive.match(/<article[^>]*>/u)?.[0]).not.toContain(
      'hidden=""',
    );
  });

  it("explains why other Silos cannot launch while one is running", () => {
    const noAction = async () => {};
    const runningSilo = {
      ...previewSilo,
      id: "running-silo-id",
      name: "正在用的空间",
    };
    const waitingSilo = {
      ...previewSilo,
      id: "waiting-silo-id",
      name: "个人空间",
    };
    const populated = renderToStaticMarkup(
      createElement(SiloList, {
        activation: runningSilo.id,
        focusedSiloId: waitingSilo.id,
        busy: false,
        managedEngineReady: false,
        networkEvidence: [],
        identityPreviews: {},
        storageUsage: {},
        onArchive: noAction,
        onCreate: noAction,
        onEdit: noAction,
        onLaunch: noAction,
        onRebindMihomo: noAction,
        onRecheckBrowser: noAction,
        onRecheckRuntime: noAction,
        onStop: noAction,
        runtimeActivation: previewStatus().activation,
        runtimeState: "running" as const,
        silos: [runningSilo, waitingSilo],
      }),
    );
    expect(populated).toContain("」正在运行。一次只能打开一个");
    expect(populated).toContain("「正在用的空间」");
    const sceneTags = populated.match(/<article[^>]*>/gu) ?? [];
    expect(
      sceneTags.find((tag) => tag.includes(`id="silo-${waitingSilo.id}"`)),
    ).not.toContain('hidden=""');
    expect(
      sceneTags.find((tag) => tag.includes(`id="silo-${runningSilo.id}"`)),
    ).toContain('hidden=""');
    const launch = populated.match(
      /<button[^>]*disabled=""[^>]*>打开浏览器<\/button>/u,
    );
    expect(launch).not.toBeNull();
    expect(populated).toContain("先关闭它的浏览器窗口");
  });

  it("shows reconciled identity evidence without calling it verified", () => {
    const noAction = async () => {};
    const managedSilo = {
      ...previewSilo,
      engine: {
        adapter: "camoufox" as const,
        artifactBinding: {
          artifactId: "identity-preview",
          artifactFileSha256: "0".repeat(64),
          schema: "verisilo-camoufox-resolved-identity/v3" as const,
        },
      },
    };
    const status = {
      ...previewStatus().activation,
      activeSiloId: managedSilo.id,
      state: "running" as const,
      identityEvidence: {
        siloId: managedSilo.id,
        runtimeId: "11111111-1111-4111-8111-111111111111",
        sessionId: "session-preview",
        artifactId: "identity-preview",
        artifactFileSha256: "0".repeat(64),
        engineAdapter: "camoufox" as const,
        observedAt: "2026-09-10T00:00:00.000Z",
        state: "matched" as const,
        signals: [
          {
            signal: "timezone",
            expected: "UTC",
            observed: "UTC",
            state: "matched" as const,
          },
          {
            signal: "fonts",
            expected: null,
            observed: null,
            state: "unavailable" as const,
            reason: "fontMode=inherit",
          },
        ],
      },
    };
    const rendered = renderToStaticMarkup(
      createElement(SiloList, {
        activation: managedSilo.id,
        busy: false,
        managedEngineReady: true,
        networkEvidence: [],
        identityPreviews: {},
        storageUsage: {},
        onArchive: noAction,
        onCreate: noAction,
        onEdit: noAction,
        onLaunch: noAction,
        onRebindMihomo: noAction,
        onRecheckBrowser: noAction,
        onRecheckRuntime: noAction,
        onStop: noAction,
        runtimeActivation: status,
        runtimeState: "running" as const,
        silos: [managedSilo],
      }),
    );
    expect(rendered).toContain("Identity evidence");
    expect(rendered).toContain("Identity Matched");
    expect(rendered).toContain("时区");
    expect(rendered).not.toContain("Identity verified");
  });

  it("offers create-new-identity on managed silos only", () => {
    const noAction = async () => {};
    const managedSilo = {
      ...previewSilo,
      name: "托管空间",
      engine: {
        adapter: "camoufox" as const,
        artifactBinding: {
          artifactId: "identity-preview",
          artifactFileSha256: "0".repeat(64),
          schema: "verisilo-camoufox-resolved-identity/v6" as const,
        },
      },
    };
    const onCreateIdentity = () => {};
    const managed = renderToStaticMarkup(
      createElement(SiloList, {
        activation: null,
        busy: false,
        managedEngineReady: true,
        networkEvidence: [],
        identityPreviews: { [managedSilo.id]: identityPreview },
        storageUsage: {},
        onArchive: noAction,
        onCreate: noAction,
        onCreateIdentity,
        onEdit: noAction,
        onLaunch: noAction,
        onRebindMihomo: noAction,
        onRecheckBrowser: noAction,
        onRecheckRuntime: noAction,
        onStop: noAction,
        runtimeActivation: previewStatus().activation,
        runtimeState: "idle" as const,
        silos: [managedSilo],
      }),
    );
    expect(managed).toContain("创建新身份");

    const standard = renderToStaticMarkup(
      createElement(SiloList, {
        activation: null,
        busy: false,
        managedEngineReady: true,
        networkEvidence: [],
        identityPreviews: {},
        storageUsage: {},
        onArchive: noAction,
        onCreate: noAction,
        onCreateIdentity,
        onEdit: noAction,
        onLaunch: noAction,
        onRebindMihomo: noAction,
        onRecheckBrowser: noAction,
        onRecheckRuntime: noAction,
        onStop: noAction,
        runtimeActivation: previewStatus().activation,
        runtimeState: "idle" as const,
        silos: [previewSilo],
      }),
    );
    expect(standard).not.toContain("创建新身份");
  });

  it("keeps create-new-identity available while the managed silo is running", () => {
    const noAction = async () => {};
    const managedSilo = {
      ...previewSilo,
      name: "托管空间",
      engine: {
        adapter: "camoufox" as const,
        artifactBinding: {
          artifactId: "identity-preview",
          artifactFileSha256: "0".repeat(64),
          schema: "verisilo-camoufox-resolved-identity/v6" as const,
        },
      },
    };
    const rendered = renderToStaticMarkup(
      createElement(SiloList, {
        activation: managedSilo.id,
        busy: false,
        managedEngineReady: true,
        networkEvidence: [],
        identityPreviews: { [managedSilo.id]: identityPreview },
        storageUsage: {},
        onArchive: noAction,
        onCreate: noAction,
        onCreateIdentity: () => {},
        onEdit: noAction,
        onLaunch: noAction,
        onRebindMihomo: noAction,
        onRecheckBrowser: noAction,
        onRecheckRuntime: noAction,
        onStop: noAction,
        runtimeActivation: previewStatus().activation,
        runtimeState: "running" as const,
        silos: [managedSilo],
      }),
    );
    expect(rendered).toContain("创建新身份");
  });
});
