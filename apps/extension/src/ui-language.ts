import type {
  LabsCoverage,
  LabsPhaseReceipt,
  ObservationReport,
  ObservedSignal,
} from "@verisilo/contracts";

export function labsEvidenceLabel(
  code: LabsPhaseReceipt["evidenceCodes"][number],
): string {
  const labels: Record<LabsPhaseReceipt["evidenceCodes"][number], string> = {
    site_permission_present: "已确认当前站点权限",
    main_world_observed: "已读取当前页面环境",
    constructor_restorable: "已确认可恢复原网页行为",
    constructor_wrapped: "已启用临时检查",
    new_worker_handshake: "新建后台任务响应正常",
    same_origin_iframe_consistent: "同站内嵌页面结果一致",
    visible_cookie_probe_clear: "页面可见网站数据未发现测试标记",
    service_worker_url_probe_clear: "浏览器后台任务地址未发现测试标记",
    cross_tab_probe_clear: "其他同站标签页未发现测试标记",
    injection_order_unproven: "无法确认检查早于网站脚本",
    restore_confirmed: "已确认恢复原网页行为",
    restore_unconfirmed: "未能确认恢复原网页行为",
  };
  return labels[code];
}

export function labsInjectionOrderLabel(
  value: LabsCoverage["injectionOrder"],
): string {
  return {
    not_attempted: "未检查",
    late_or_unknown: "无法确认早于网站脚本",
    document_start_guaranteed: "已确认从页面载入时生效",
  }[value];
}

export function labsNewBackgroundTaskLabel(
  value: LabsCoverage["newDedicatedWorkers"],
): string {
  return {
    not_attempted: "未检查",
    same_origin_blob_classic_only: "仅检查开启后新建的同站后台任务",
    failed: "检查失败",
  }[value];
}

export function labsEmbeddedPageLabel(
  value: LabsCoverage["windowIframe"],
): string {
  return {
    not_attempted: "未检查",
    same_origin_probe_passed: "同站内嵌页面检查通过",
    failed: "检查失败",
  }[value];
}

export function labsVisibleSiteDataLabel(
  value: LabsCoverage["cookies"],
): string {
  return {
    not_observed: "未检查",
    visible_canary_observation_only: "仅检查页面可见的随机测试标记",
  }[value];
}

export function labsBrowserBackgroundLabel(
  value: LabsCoverage["serviceWorkers"],
): string {
  return {
    not_observed: "未检查",
    registration_urls_only: "仅检查后台任务的注册地址",
  }[value];
}

export function observationWorkerCoverageLabel(
  value: ObservationReport["coverage"]["worker"],
): string {
  return {
    self_test_only: "仅完成扩展自检",
    not_attempted: "未检查",
  }[value];
}

export function observedSignalName(signalId: string): string {
  const names: Record<string, string> = {
    navigator: "浏览器与设备",
    ua_ch: "浏览器版本与系统信息",
    timezone: "时区",
    screen: "屏幕",
    canvas_hash: "图形绘制特征摘要",
    webgl: "显卡信息",
    webgpu: "新一代图形功能可用性",
    audio: "音频特征摘要",
    fonts: "字体可见性",
    media_devices: "摄像头与麦克风",
    permissions: "网站权限",
    storage: "登录信息与网站存储",
    webrtc: "直接网络连接",
    window_iframe: "内嵌页面环境",
    dedicated_worker: "网页后台任务环境",
    main_world_navigator: "页面读取到的浏览器信息",
  };
  return names[signalId] ?? "其他浏览器信号";
}

export function observedSignalStatusLabel(
  status: ObservedSignal["status"],
): string {
  return {
    ok: "已读取",
    blocked: "被页面阻止",
    unsupported: "浏览器不支持",
    error: "读取失败",
  }[status];
}

export function observedSignalSourceLabel(
  source: ObservedSignal["source"],
): string {
  return {
    window: "当前页面",
    iframe: "内嵌页面",
    worker: "网页后台任务",
    header: "网页请求信息",
    extension: "扩展自检",
  }[source];
}

export function signalFailureLabel(error: string | null | undefined): string {
  const labels: Record<string, string> = {
    "User-Agent Client Hints are unavailable.": "浏览器未提供客户端提示信息。",
    "Canvas 2D context is unavailable.": "当前页面无法使用二维图形能力。",
    "Offline audio rendering is unavailable.": "当前页面无法使用离线音频能力。",
    "Font Loading API is unavailable.": "当前页面无法检查字体可见性。",
    "Media Devices API is unavailable.": "当前页面无法检查摄像头与麦克风信息。",
    "Same-origin iframe window is unavailable.":
      "当前页面无法检查同站内嵌页面。",
    "Dedicated Worker is unavailable.": "当前页面无法检查网页后台任务。",
  };
  return error === null || error === undefined
    ? "未获得可显示的结果。"
    : (labels[error] ?? "采集失败，未获得可显示的结果。");
}

export function userFacingError(error: unknown, fallback: string): string {
  const raw = error instanceof Error ? error.message : String(error);
  if (/[\u3400-\u9fff]/u.test(raw)) {
    return raw;
  }
  if (/No active browser tab is available/iu.test(raw)) {
    return "没有找到当前标签页，请切回普通网页后重试。";
  }
  if (/Extension context invalidated/iu.test(raw)) {
    return "扩展刚刚更新，请重新打开侧栏后重试。";
  }
  if (
    /Receiving end does not exist|Could not establish connection/iu.test(raw)
  ) {
    return "扩展后台正在重新载入，请稍后重试。";
  }
  if (
    /Cannot access contents of|The extensions gallery cannot be scripted/iu.test(
      raw,
    )
  ) {
    return "无法访问当前页面。浏览器内部页面、扩展商店和 PDF 不支持此操作。";
  }
  if (/denied|not granted|permission/iu.test(raw)) {
    return "没有获得所需权限，操作未执行。";
  }
  return fallback;
}
