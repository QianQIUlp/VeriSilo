import { describe, expect, it } from "vitest";

import {
  activationStatusLabel,
  describeActivation,
  describeIdentityRecheck,
  describeNetwork,
} from "./formatters.js";
import type { RuntimeIdentityEvidence } from "@verisilo/contracts";

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
        identityEvidence: null,
      }),
    ).toContain("结束会话");
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
        identityEvidence: null,
      }),
    ).toBe("浏览器没有打开成功");
  });

  it("presents a stopped runtime as user-visible idle", () => {
    expect(activationStatusLabel("stopped")).toBe("空闲");
  });

  it("never describes a direct Silo as proxy protected", () => {
    expect(describeNetwork({ mode: "direct", proxyRequired: false })).toContain(
      "直连",
    );
  });

  it("explains that a Clash binding uses a Silo-only proxy", () => {
    expect(
      describeNetwork({
        mode: "fixed_proxy",
        scheme: "socks5",
        host: "127.0.0.1",
        port: 7897,
        proxyRequired: true,
        bypassList: [],
        externalMihomo: {
          controllerUrl: "http://127.0.0.1:9097",
          selectorGroup: "GLOBAL",
          nodeName: "直连-美国04",
        },
      }),
    ).toBe("Silo 专属代理 · 本机 Clash「直连-美国04」");
  });

  it("describes an explicit recheck with the fresh identity result and time", () => {
    const observedAt = new Date(Date.now() - 30_000).toISOString();
    const evidence: RuntimeIdentityEvidence = {
      siloId: "11111111-1111-4111-8111-111111111111",
      runtimeId: "22222222-2222-4222-8222-222222222222",
      sessionId: "s",
      artifactId: "identity-a",
      artifactFileSha256: "a".repeat(64),
      engineAdapter: "camoufox",
      observedAt,
      state: "matched",
      signals: [],
    };
    const message = describeIdentityRecheck({
      activeSiloId: "11111111-1111-4111-8111-111111111111",
      state: "running",
      updatedAt: new Date().toISOString(),
      message: null,
      engineEvidence: null,
      networkEvidence: null,
      identityEvidence: evidence,
    }, null);
    expect(message).toContain("网站可见身份：Matched");
    expect(message).toContain("本次已取得新观察");
    expect(message).toContain("浏览器正在运行");
  });

  it("reports a mismatched fresh observation as a fact, not a failure", () => {
    const evidence: RuntimeIdentityEvidence = {
      siloId: "11111111-1111-4111-8111-111111111111",
      runtimeId: "22222222-2222-4222-8222-222222222222",
      sessionId: "s",
      artifactId: "identity-a",
      artifactFileSha256: "a".repeat(64),
      engineAdapter: "camoufox",
      observedAt: new Date().toISOString(),
      state: "mismatched",
      signals: [],
    };
    const message = describeIdentityRecheck({
      activeSiloId: "11111111-1111-4111-8111-111111111111",
      state: "running",
      updatedAt: new Date().toISOString(),
      message: null,
      engineEvidence: null,
      networkEvidence: null,
      identityEvidence: evidence,
    }, null);
    expect(message).toContain("Mismatched");
    expect(message).not.toContain("检测");
    expect(message).not.toContain("风控");
  });

  it("keeps the plain activation description when no identity evidence belongs to the active Silo", () => {
    const evidence: RuntimeIdentityEvidence = {
      siloId: "33333333-3333-4333-8333-333333333333",
      runtimeId: "22222222-2222-4222-8222-222222222222",
      sessionId: "s",
      artifactId: "identity-a",
      artifactFileSha256: "a".repeat(64),
      engineAdapter: "camoufox",
      observedAt: new Date().toISOString(),
      state: "matched",
      signals: [],
    };
    expect(
      describeIdentityRecheck({
        activeSiloId: "11111111-1111-4111-8111-111111111111",
        state: "running",
        updatedAt: new Date().toISOString(),
        message: null,
        engineEvidence: null,
        networkEvidence: null,
        identityEvidence: evidence,
      }),
    ).toBe("浏览器正在运行");
  });

  it("does not claim a new observation when Core retains the old time after failure", () => {
    const before: RuntimeIdentityEvidence = {
      siloId: "11111111-1111-4111-8111-111111111111",
      runtimeId: "22222222-2222-4222-8222-222222222222",
      sessionId: "s", artifactId: "identity-a", artifactFileSha256: "a".repeat(64),
      engineAdapter: "camoufox", observedAt: "2026-09-12T08:00:00.000Z",
      state: "matched", signals: [],
    };
    const after = { ...before, state: "unavailable" as const };
    const activation = {
      activeSiloId: before.siloId, state: "running" as const,
      updatedAt: "2026-09-12T08:05:00.000Z", message: null,
      engineEvidence: null, networkEvidence: null, identityEvidence: after,
    };
    const retained = describeIdentityRecheck(activation, before);
    expect(retained).toContain("Unavailable");
    expect(retained).toContain("未取得可确认的新观察");
    expect(retained).not.toContain("本次已取得新观察");

    const uncertain = describeIdentityRecheck(activation);
    expect(uncertain).toContain("未确认本次重新取得观察");

    const newlyUnavailable = describeIdentityRecheck({
      ...activation,
      identityEvidence: { ...after, observedAt: "2026-09-12T08:05:00.000Z" },
    }, before);
    expect(newlyUnavailable).toContain("本次取得新记录，但网站可见字段不可用");
  });
});
