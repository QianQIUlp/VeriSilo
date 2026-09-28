import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { RecentRunRecord } from "@verisilo/contracts";

import { previewManagedSilo, previewSilo } from "../../preview/fixtures.js";
import { RecentRunDetails } from "./RecentRunDetails.js";

const RUN = "22222222-2222-4222-8222-222222222222";
const RUNTIME = "33333333-3333-4333-8333-333333333333";
const AT = "2026-09-28T00:00:00.000Z";

function markup(record: RecentRunRecord | null, managed = false) {
  return renderToStaticMarkup(
    createElement(RecentRunDetails, {
      record,
      silo: managed ? previewManagedSilo : previewSilo,
    }),
  );
}

describe("RecentRunDetails", () => {
  it("shows an honest empty state", () => {
    expect(markup(null)).toContain("尚无保存的本机运行记录");
  });

  it("labels Standard's missing identity and an early failure without evidence", () => {
    const record: RecentRunRecord = {
      siloId: previewSilo.id,
      runId: RUN,
      runtimeId: null,
      startedAt: AT,
      updatedAt: AT,
      endedAt: AT,
      profileSiloId: previewSilo.id,
      artifactBinding: null,
      engineAdapter: "stock-edge",
      networkPolicy: {
        mode: "direct",
        proxyRequired: false,
        endpointLabel: null,
        externalMihomo: false,
      },
      state: "failed",
      reason: "本次启动未完成。",
      identityEvidence: null,
      engineEvidence: null,
      networkEvidence: null,
    };
    const html = markup(record);
    expect(html).toContain("Standard 未配置身份 Artifact");
    expect(html).toContain("启动在取得运行时证据前失败");
    expect(html).toContain("历史快照");
  });

  it("shows the observed time and mismatch field without claiming verification", () => {
    const record: RecentRunRecord = {
      siloId: previewManagedSilo.id,
      runId: RUN,
      runtimeId: RUNTIME,
      startedAt: AT,
      updatedAt: AT,
      endedAt: null,
      profileSiloId: previewManagedSilo.id,
      artifactBinding: {
        artifactId: "identity-test",
        artifactFileSha256: "a".repeat(64),
        schema: "verisilo-camoufox-resolved-identity/v6",
      },
      engineAdapter: "camoufox",
      networkPolicy: {
        mode: "fixed_proxy",
        proxyRequired: true,
        endpointLabel: "127.0.0.1:7890",
        externalMihomo: true,
      },
      state: "running",
      reason: null,
      identityEvidence: {
        siloId: previewManagedSilo.id,
        runtimeId: RUNTIME,
        sessionId: "session-test",
        artifactId: "identity-test",
        artifactFileSha256: "a".repeat(64),
        engineAdapter: "camoufox",
        observedAt: AT,
        state: "mismatched",
        signals: [{ signal: "timezone", expected: "A", observed: "B", state: "mismatched" }],
      },
      engineEvidence: null,
      networkEvidence: null,
    };
    const html = markup(record, true);
    expect(html).toContain("1 项不一致");
    expect(html).toContain("时区");
    expect(html).toContain("期望");
    expect(html).toContain("观察");
    expect(html).toContain(`<time dateTime="${AT}"`);
    expect(html).not.toContain("全部已验证");
  });
});
