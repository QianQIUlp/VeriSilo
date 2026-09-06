import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { previewSilo, previewStatus } from "../../preview/fixtures.js";
import { SiloList } from "./SiloList.js";

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
    expect(populated).toContain("先关闭它的浏览器窗口");
  });
});
