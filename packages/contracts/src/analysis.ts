import type { ObservationReport, ObservedSignal } from "./models.js";

export type FindingSeverity = "normal" | "info" | "warning";

export interface ConsistencyFinding {
  id: string;
  severity: FindingSeverity;
  title: string;
  beginnerSummary: string;
  developerEvidence: Record<string, unknown>;
}

export function analyzeConsistency(
  report: ObservationReport,
): ConsistencyFinding[] {
  const findings: ConsistencyFinding[] = [];
  const navigatorSignal = signalValue<Record<string, unknown>>(
    report,
    "navigator",
  );
  const mainWorldSignal = signalValue<Record<string, unknown>>(
    report,
    "main_world_navigator",
  );

  if (navigatorSignal !== undefined && mainWorldSignal !== undefined) {
    const keys = ["userAgent", "platform", "languages"] as const;
    const differences = keys.filter(
      (key) =>
        JSON.stringify(navigatorSignal[key]) !==
        JSON.stringify(mainWorldSignal[key]),
    );
    if (differences.length > 0) {
      findings.push({
        id: "window-main-world-difference",
        severity: "warning",
        title: "页面与扩展看到的浏览器信息不同",
        beginnerSummary:
          "网页主环境和扩展隔离环境返回了不同的浏览器信息。这不一定表示风险，但说明当前环境存在需要进一步检查的差异。",
        developerEvidence: { differences, navigatorSignal, mainWorldSignal },
      });
    }
  }

  if (navigatorSignal !== undefined) {
    const userAgent = stringValue(navigatorSignal.userAgent);
    const touchPoints = numberValue(navigatorSignal.maxTouchPoints);
    if (/\bMobile\b/iu.test(userAgent) && touchPoints === 0) {
      findings.push({
        id: "mobile-without-touch",
        severity: "warning",
        title: "移动端声明与触控能力不协调",
        beginnerSummary:
          "浏览器声明自己像移动设备，但没有报告触控点。真实设备也可能出现这种情况；这里只将它标为需要理解的组合，而不是异常判定。",
        developerEvidence: { userAgent, maxTouchPoints: touchPoints },
      });
    }
  }

  for (const signal of report.signals.filter(
    (candidate) => candidate.status !== "ok",
  )) {
    findings.push(unavailableSignalFinding(signal));
  }

  if (findings.length === 0) {
    findings.push({
      id: "no-obvious-conflict",
      severity: "normal",
      title: "当前扫描未发现明显矛盾",
      beginnerSummary:
        "这只表示本次有限扫描没有发现已实现规则能够解释的问题，不表示网站无法识别、关联或采集其他信号。",
      developerEvidence: {
        signalCount: report.signals.length,
        coverage: report.coverage,
      },
    });
  }

  return findings;
}

function unavailableSignalFinding(signal: ObservedSignal): ConsistencyFinding {
  return {
    id: `signal-${signal.id}-${signal.status}`,
    severity: "info",
    title: `${signal.id} 未完整可用`,
    beginnerSummary:
      "某项浏览器信号无法被当前页面读取或采集。限制可能来自浏览器、权限、网站设置或扩展覆盖范围；这不是自动的隐私保护证明。",
    developerEvidence: {
      signalId: signal.id,
      status: signal.status,
      error: signal.error,
      source: signal.source,
    },
  };
}

function signalValue<T>(report: ObservationReport, id: string): T | undefined {
  const signal = report.signals.find(
    (candidate) => candidate.id === id && candidate.status === "ok",
  );
  return signal?.value as T | undefined;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}
