import { describe, expect, it } from "vitest";

import {
  activationStatusLabel,
  describeActivation,
  describeNetwork,
} from "./formatters.js";

describe("desktop formatters", () => {
  it("labels a terminal runtime failure as blocked instead of running", () => {
    expect(
      describeActivation({
        activeSiloId: "11111111-1111-4111-8111-111111111111",
        state: "verification_failed",
        updatedAt: "2026-07-28T12:00:00.000Z",
        message: null,
        engineEvidence: null,
        networkEvidence: null,
      }),
    ).toContain("已阻止本次启动");
  });

  it("never forwards a native activation message into the product UI", () => {
    expect(
      describeActivation({
        activeSiloId: "11111111-1111-4111-8111-111111111111",
        state: "failed",
        updatedAt: "2026-07-28T12:00:00.000Z",
        message: "provider receipt UUID mismatch",
        engineEvidence: null,
        networkEvidence: null,
      }),
    ).toBe("Silo 启动失败");
  });

  it("presents a stopped runtime as user-visible idle", () => {
    expect(activationStatusLabel("stopped")).toBe("空闲");
  });

  it("never describes a direct Silo as proxy protected", () => {
    expect(describeNetwork({ mode: "direct", proxyRequired: false })).toContain(
      "直连",
    );
  });
});
