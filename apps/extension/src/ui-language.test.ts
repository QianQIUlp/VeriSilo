import { describe, expect, it } from "vitest";

import {
  labsBrowserBackgroundLabel,
  labsEmbeddedPageLabel,
  labsEvidenceLabel,
  labsInjectionOrderLabel,
  labsNewBackgroundTaskLabel,
  labsVisibleSiteDataLabel,
  observedSignalName,
  observedSignalSourceLabel,
  observedSignalStatusLabel,
  observationWorkerCoverageLabel,
  signalFailureLabel,
  userFacingError,
} from "./ui-language.js";

describe("user-facing extension language", () => {
  it("translates experiment evidence and coverage enums", () => {
    expect(labsEvidenceLabel("new_worker_handshake")).toBe(
      "新建后台任务响应正常",
    );
    expect(labsInjectionOrderLabel("late_or_unknown")).toBe(
      "无法确认早于网站脚本",
    );
    expect(labsNewBackgroundTaskLabel("same_origin_blob_classic_only")).toBe(
      "仅检查开启后新建的同站后台任务",
    );
    expect(labsEmbeddedPageLabel("same_origin_probe_passed")).toBe(
      "同站内嵌页面检查通过",
    );
    expect(labsVisibleSiteDataLabel("visible_canary_observation_only")).toBe(
      "仅检查页面可见的随机测试标记",
    );
    expect(labsBrowserBackgroundLabel("registration_urls_only")).toBe(
      "仅检查后台任务的注册地址",
    );
  });

  it("translates report coverage and known collection failures", () => {
    expect(observationWorkerCoverageLabel("self_test_only")).toBe(
      "仅完成扩展自检",
    );
    expect(signalFailureLabel("Dedicated Worker is unavailable.")).toBe(
      "当前页面无法检查网页后台任务。",
    );
    expect(signalFailureLabel("internal_backend_code")).toBe(
      "采集失败，未获得可显示的结果。",
    );
    expect(observedSignalName("canvas_hash")).toBe("图形绘制特征摘要");
    expect(observedSignalStatusLabel("unsupported")).toBe("浏览器不支持");
    expect(observedSignalSourceLabel("worker")).toBe("网页后台任务");
  });

  it("does not expose unknown English runtime failures", () => {
    expect(
      userFacingError(
        new Error("Native backend exploded at C:\\secret"),
        "操作失败。",
      ),
    ).toBe("操作失败。");
    expect(
      userFacingError(
        new Error("No active browser tab is available."),
        "操作失败。",
      ),
    ).toBe("没有找到当前标签页，请切回普通网页后重试。");
  });
});
