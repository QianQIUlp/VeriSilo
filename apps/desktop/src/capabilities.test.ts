import { describe, expect, it } from "vitest";

import { ENVIRONMENT_LAYERS, PRODUCT_CAPABILITIES } from "./capabilities.js";

describe("desktop capability catalogue", () => {
  it("keeps capability identifiers unique", () => {
    const ids = PRODUCT_CAPABILITIES.map((capability) => capability.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("makes every scheduled route visible in the product layers", () => {
    const routes = new Set(ENVIRONMENT_LAYERS.map((layer) => layer.id));
    for (const capability of PRODUCT_CAPABILITIES) {
      expect(routes.has(capability.route)).toBe(true);
      expect(capability.evidenceRule.length).toBeGreaterThan(10);
    }
  });

  it("does not claim that TLS or hardware are controlled by a profile", () => {
    expect(
      PRODUCT_CAPABILITIES.find((capability) => capability.id === "tls")
        ?.currentReality,
    ).toContain("独立 Profile 无法改变");
    expect(
      PRODUCT_CAPABILITIES.find((capability) => capability.id === "hardware")
        ?.route,
    ).toBe("local_vm");
  });

  it("keeps site-state isolation as an implemented mechanism pending host validation", () => {
    const siteState = PRODUCT_CAPABILITIES.find(
      (capability) => capability.id === "site_state",
    );
    expect(siteState?.tone).toBe("available");
    expect(siteState?.currentReality).toContain("独立 user-data-dir");
    expect(siteState?.currentReality).toContain("仍待");
    expect(siteState?.evidenceRule).toContain("未执行前不得标为本机已验证");
  });

  it("describes the implemented remote control plane without claiming a provider", () => {
    const remote = ENVIRONMENT_LAYERS.find((layer) => layer.id === "remote");
    expect(remote?.status).toBe("implemented");
    expect(remote?.summary).toContain("自托管 Agent 已实现");
    expect(remote?.summary).toContain("真实 VM、浏览器和媒体流仍需外部");
    expect(remote?.delivers.join(" ")).not.toContain(
      "网络客户端、Agent、持久存储均未实现",
    );
  });

  it("keeps local environment control receipts separate from guest evidence", () => {
    const local = ENVIRONMENT_LAYERS.find((layer) => layer.id === "local_vm");
    expect(local?.summary).toContain("已配置、来宾观测、已验证与不可用");
    expect(local?.delivers.join(" ")).toContain("来宾 OS resolver 不可用");
    expect(local?.delivers.join(" ")).toContain("来宾网络与浏览器就绪不可用");
    expect(local?.delivers.join(" ")).toContain("合法 VHDX");
  });
});
