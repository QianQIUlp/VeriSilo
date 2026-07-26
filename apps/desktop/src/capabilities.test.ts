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
});
