import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { previewManagedSilo, previewRuntimeActivation } from "../../preview/fixtures.js";
import { IdentityInspectPanel, ManagedIdentityEvidence, ManagedStatusGroups } from "./IdentityDetails.js";
import { deriveCurrentSessionSummary } from "./CurrentSessionIntegrity.js";
import { identityEvidenceContext } from "./evidence-diagnostics.js";

const silo = previewManagedSilo;

describe("identity evidence diagnostics", () => {
  it("shows Host mismatch first and preserves expandable full objects, voices and long strings", () => {
    const activation = previewRuntimeActivation(silo);
    const longValue = "x".repeat(130) + "TAIL";
    activation.identityEvidence = {
      ...activation.identityEvidence!,
      state: "mismatched",
      reason: "{\"runtimeId\":\"secret\",\"signals\":[]}",
      signals: [
        { signal: "language", expected: "en-US", observed: "en-US", state: "matched" },
        { signal: "voices", expected: [{ name: "Voice A", lang: "en-US" }],
          observed: [{ name: "Voice B", lang: "en-GB" }], state: "mismatched",
          reason: "语音名称和语言不同。" },
        { signal: "webgl2Renderer", expected: longValue, observed: longValue,
          state: "mismatched" },
        { signal: "screen", expected: { width: 1920, height: 1080 },
          observed: { width: 1366, height: 768 }, state: "mismatched" },
        { signal: "mediaDevices", expected: { audioinput: 1, videoinput: 2, audiooutput: 1 },
          observed: { audioinput: 0, videoinput: 1, audiooutput: 0 }, state: "mismatched" },
        { signal: "acceptEncoding", expected: ["gzip", "br"], observed: null,
          state: "unavailable" },
        { signal: "futureSignal", expected: { nested: [1, 2] }, observed: { nested: [1, 3] },
          state: "mismatched" },
      ],
    };
    const markup = renderToStaticMarkup(createElement(IdentityInspectPanel, {
      activeSiloId: silo.id, activation, silos: [silo],
    }));
    expect(markup).toContain("身份 Mismatched");
    expect(markup).toContain("语音名称和语言不同");
    expect(markup).toContain("<summary>Voice A (en-US) · 展开完整值</summary>");
    expect(markup).toContain("<summary>Voice B (en-GB) · 展开完整值</summary>");
    expect(markup).toContain("<summary>1920×1080 · 展开完整值</summary>");
    expect(markup).toContain("<summary>1366×768 · 展开完整值</summary>");
    expect(markup).toContain("麦克风 1 · 摄像头 2 · 输出 1");
    expect(markup).toContain("麦克风 0 · 摄像头 1 · 输出 0");
    expect(markup).toContain("TAIL");
    expect(markup).toContain("WebGL 2 渲染器");
    expect(markup).toContain("接受的压缩编码");
    expect(markup).toContain("其他字段：futureSignal");
    expect(markup).toContain("查看已匹配字段（1）");
    expect(markup.indexOf("语音")).toBeLessThan(markup.indexOf("查看已匹配字段"));
    expect(markup).not.toContain("secret");
    expect(markup).not.toContain("已取得");
  });

  it("separates a Host verdict from current, stale and last known scope", () => {
    const activation = previewRuntimeActivation(silo);
    expect(identityEvidenceContext(activation, silo).current).toBe(true);
    expect(identityEvidenceContext({ ...activation, networkEvidence: null }, silo).scopeLabel)
      .toBe("运行归属未确认");
    expect(identityEvidenceContext({
      ...activation,
      identityEvidence: { ...activation.identityEvidence!, state: "stale" },
    }, silo).current).toBe(false);
    const stopped = { ...activation, state: "stopped" as const };
    expect(identityEvidenceContext(stopped, silo).scopeLabel).toBe("最后已知");
    const markup = renderToStaticMarkup(createElement(ManagedIdentityEvidence, {
      activation: stopped, silo,
    }));
    expect(markup).toContain("Identity Matched");
    expect(markup).toContain("最后已知");
    expect(markup).not.toContain("当前运行</span>");
  });

  it("keeps the network lens claim at the session summary level while retaining old stages", () => {
    const now = Date.parse("2026-09-28T12:00:00.000Z");
    const base = previewRuntimeActivation(silo);
    const cases = [
      {
        activation: { ...base, networkEvidence: {
          ...base.networkEvidence!, expiresAt: "2026-09-28T11:59:00.000Z",
        } },
        label: "无法确认当前出口", note: "证据已过期；非当前有效",
      },
      {
        activation: { ...base, networkEvidence: {
          ...base.networkEvidence!, runtimeId: "33333333-3333-4333-8333-333333333333",
        } },
        label: "运行归属未确认", note: "来自不同 runtime；非当前有效",
      },
      {
        activation: { ...base, state: "stopped" as const },
        label: "最后已知出口", note: "最后已知记录；非当前有效",
      },
    ];
    for (const { activation, label, note } of cases) {
      const summary = deriveCurrentSessionSummary({
        activation, silo, managedEngineReady: true, now,
      });
      const network = summary.rows.find((row) => row.key === "network");
      expect(network?.stateLabel).toBe(label);
      const lens = renderToStaticMarkup(createElement(ManagedStatusGroups, {
        activation, evidence: [], engineHealthy: true,
        runtimeState: activation.state, silo, now,
      }));
      expect(lens).toContain(`<strong class="managed-state warn">${label}</strong>`);
      expect(lens).toContain(network!.detail);
      expect(lens).toContain(note);
      expect(lens).toContain("逐阶段网络证据");
      expect(lens).toContain("浏览器路由");
      expect(lens).toContain("已观察");
    }
  });
});
