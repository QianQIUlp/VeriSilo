import { z } from "zod";

export const LABS_SCHEMA_VERSION = 1 as const;
export const LABS_DEFINITION_REVISION = 1 as const;
export const LABS_RUNTIME_TTL_MS = 2 * 60 * 1_000;
export const LABS_LOCAL_AUTHORIZATION_TTL_MS = 10 * 60 * 1_000;
export const LABS_SILO_AUTHORIZATION_TTL_MS = 24 * 60 * 60 * 1_000;
export const LABS_RECEIPT_TTL_MS = 30 * 24 * 60 * 60 * 1_000;

export const labsExperimentIdSchema = z.enum([
  "dedicated_worker_constructor",
  "cookie_virtualization",
  "set_cookie_interception",
]);
export type LabsExperimentId = z.infer<typeof labsExperimentIdSchema>;

export const labsExperimentStateSchema = z.enum([
  "disabled",
  "permission_missing",
  "applying",
  "best_effort",
  "verified",
  "failed",
  "leak_detected",
  "restored",
  "unsupported",
]);
export type LabsExperimentState = z.infer<typeof labsExperimentStateSchema>;

export const labsExperimentPhaseSchema = z.enum([
  "observe",
  "apply",
  "verify",
  "restore",
]);
export type LabsExperimentPhase = z.infer<typeof labsExperimentPhaseSchema>;

export const labsStopConditionCodeSchema = z.enum([
  "cross_tab_canary_leak",
  "iframe_canary_leak",
  "worker_canary_leak",
  "service_worker_canary_leak",
  "cookie_canary_leak",
  "window_canary_leak",
  "page_error",
  "worker_error",
  "timeout",
  "permission_taken_over",
  "site_navigation",
  "scope_violation",
  "verification_failed",
  "extension_context_lost",
  "user_requested",
  "expired",
]);
export type LabsStopConditionCode = z.infer<typeof labsStopConditionCodeSchema>;

export const labsStopConditionSchema = z
  .object({
    code: labsStopConditionCodeSchema,
    action: z.literal("restore_and_disable_site"),
  })
  .strict();
export type LabsStopCondition = z.infer<typeof labsStopConditionSchema>;

export const LABS_STOP_CONDITIONS = labsStopConditionCodeSchema.options.map(
  (code) =>
    ({ code, action: "restore_and_disable_site" }) satisfies LabsStopCondition,
);

const httpOriginSchema = z
  .string()
  .url()
  .max(2_048)
  .refine((value) => {
    const url = new URL(value);
    return (
      ["http:", "https:"].includes(url.protocol) &&
      value === url.origin &&
      url.username === "" &&
      url.password === ""
    );
  }, "Labs site scope must be an HTTP(S) origin without credentials or a path.");

const labsDesktopSiloScopeSchema = z
  .object({
    mode: z.literal("desktop_silo"),
    siloId: z.string().uuid(),
    tabId: z.number().int().positive(),
    siteOrigin: httpOriginSchema,
    siteHost: z.string().trim().min(1).max(253),
    authorizedAt: z.string().datetime(),
    expiresAt: z.string().datetime(),
  })
  .strict();

const labsLocalTemporaryScopeSchema = z
  .object({
    mode: z.literal("local_temporary"),
    siloId: z.null(),
    tabId: z.number().int().positive(),
    siteOrigin: httpOriginSchema,
    siteHost: z.string().trim().min(1).max(253),
    authorizedAt: z.string().datetime(),
    expiresAt: z.string().datetime(),
  })
  .strict();

export const labsExperimentScopeSchema = z
  .discriminatedUnion("mode", [
    labsDesktopSiloScopeSchema,
    labsLocalTemporaryScopeSchema,
  ])
  .superRefine((scope, context) => {
    const origin = new URL(scope.siteOrigin);
    if (origin.host !== scope.siteHost) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["siteHost"],
        message: "Labs site host must match the authorized origin.",
      });
    }
    if (Date.parse(scope.expiresAt) <= Date.parse(scope.authorizedAt)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAt"],
        message: "Labs authorization must expire after it was granted.",
      });
    }
  });
export type LabsExperimentScope = z.infer<typeof labsExperimentScopeSchema>;

export const labsCoverageSchema = z
  .object({
    injectionOrder: z.enum([
      "not_attempted",
      "late_or_unknown",
      "document_start_guaranteed",
    ]),
    newDedicatedWorkers: z.enum([
      "not_attempted",
      "same_origin_blob_classic_only",
      "failed",
    ]),
    existingDedicatedWorkers: z.literal("not_covered"),
    moduleWorkers: z.literal("not_covered"),
    crossOriginWorkers: z.literal("not_covered"),
    sharedWorkers: z.literal("not_covered"),
    serviceWorkers: z.enum(["not_observed", "registration_urls_only"]),
    windowIframe: z.enum([
      "not_attempted",
      "same_origin_probe_passed",
      "failed",
    ]),
    cookies: z.enum(["not_observed", "visible_canary_observation_only"]),
    setCookie: z.literal("not_intercepted"),
  })
  .strict();
export type LabsCoverage = z.infer<typeof labsCoverageSchema>;

export const DEFAULT_LABS_COVERAGE: LabsCoverage = {
  injectionOrder: "not_attempted",
  newDedicatedWorkers: "not_attempted",
  existingDedicatedWorkers: "not_covered",
  moduleWorkers: "not_covered",
  crossOriginWorkers: "not_covered",
  sharedWorkers: "not_covered",
  serviceWorkers: "not_observed",
  windowIframe: "not_attempted",
  cookies: "not_observed",
  setCookie: "not_intercepted",
};

export const labsExperimentDefinitionSchema = z
  .object({
    schemaVersion: z.literal(LABS_SCHEMA_VERSION),
    revision: z.literal(LABS_DEFINITION_REVISION),
    id: labsExperimentIdSchema,
    title: z.string().trim().min(1).max(100),
    summary: z.string().trim().min(1).max(500),
    tier: z.enum(["best_effort", "unsupported"]),
    selectable: z.boolean(),
    defaultEnabled: z.literal(false),
    limitations: z.array(z.string().trim().min(1).max(300)).min(1).max(12),
    alternative: z.string().trim().min(1).max(500).nullable(),
    stopConditions: z.array(labsStopConditionSchema).min(1).max(32),
  })
  .strict()
  .superRefine((definition, context) => {
    if (
      definition.tier === "unsupported" &&
      (definition.selectable || definition.alternative === null)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["selectable"],
        message:
          "Unsupported Labs experiments must be unselectable and name an alternative.",
      });
    }
    if (
      new Set(definition.stopConditions.map((condition) => condition.code))
        .size !== definition.stopConditions.length
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["stopConditions"],
        message: "Labs stop-condition codes must be unique.",
      });
    }
  });
export type LabsExperimentDefinition = z.infer<
  typeof labsExperimentDefinitionSchema
>;

export const LABS_EXPERIMENT_DEFINITIONS: readonly LabsExperimentDefinition[] =
  [
    labsExperimentDefinitionSchema.parse({
      schemaVersion: LABS_SCHEMA_VERSION,
      revision: LABS_DEFINITION_REVISION,
      id: "dedicated_worker_constructor",
      title: "新建 Dedicated Worker 一致性",
      summary:
        "临时包装当前页面 MAIN world 的 Worker constructor，只观察并验证之后新建的同源/blob classic Dedicated Worker。",
      tier: "best_effort",
      selectable: true,
      defaultEnabled: false,
      limitations: [
        "用户开启时页面脚本通常已经运行，因此当前实现不能证明注入顺序。",
        "既有 Worker、module Worker、跨域 Worker、SharedWorker 与 ServiceWorker 不覆盖。",
        "页面可以观察或干扰 MAIN world 包装；异常会立即恢复原 constructor。",
      ],
      alternative: null,
      stopConditions: LABS_STOP_CONDITIONS,
    }),
    labsExperimentDefinitionSchema.parse({
      schemaVersion: LABS_SCHEMA_VERSION,
      revision: LABS_DEFINITION_REVISION,
      id: "cookie_virtualization",
      title: "Cookie 仓库虚拟化",
      summary:
        "普通 MV3 扩展无法为任意网站可靠提供完整、透明且跨上下文一致的独立 Cookie 仓库。",
      tier: "unsupported",
      selectable: false,
      defaultEnabled: false,
      limitations: [
        "HttpOnly、Cookie Store API、Service Worker 与浏览器内部写入无法由页面脚本完整代理。",
        "窄 canary 只能观察页面可见 Cookie，不会改写 Cookie。",
      ],
      alternative:
        "使用桌面 Silo 的独立 user-data-dir，让浏览器自己隔离 Cookie 与站点数据。",
      stopConditions: LABS_STOP_CONDITIONS,
    }),
    labsExperimentDefinitionSchema.parse({
      schemaVersion: LABS_SCHEMA_VERSION,
      revision: LABS_DEFINITION_REVISION,
      id: "set_cookie_interception",
      title: "Set-Cookie 全面截获",
      summary:
        "普通 MV3 扩展不能可靠截获并重写所有导航、子资源、Worker 与 Service Worker 响应的 Set-Cookie。",
      tier: "unsupported",
      selectable: false,
      defaultEnabled: false,
      limitations: [
        "没有声明 webRequestBlocking、declarativeNetRequest 或永久 host 权限。",
        "观察页面可见 Cookie 不能证明 HttpOnly 或网络响应路径已覆盖。",
      ],
      alternative:
        "使用桌面 Silo 的独立 user-data-dir；需要网络层控制时改用受控引擎或独立环境。",
      stopConditions: LABS_STOP_CONDITIONS,
    }),
  ];

export const labsPhaseReceiptSchema = z
  .object({
    phase: labsExperimentPhaseSchema,
    outcome: z.enum(["passed", "failed", "skipped"]),
    recordedAt: z.string().datetime(),
    evidenceCodes: z
      .array(
        z.enum([
          "site_permission_present",
          "main_world_observed",
          "constructor_restorable",
          "constructor_wrapped",
          "new_worker_handshake",
          "same_origin_iframe_consistent",
          "visible_cookie_probe_clear",
          "service_worker_url_probe_clear",
          "cross_tab_probe_clear",
          "injection_order_unproven",
          "restore_confirmed",
          "restore_unconfirmed",
        ]),
      )
      .max(16),
  })
  .strict();
export type LabsPhaseReceipt = z.infer<typeof labsPhaseReceiptSchema>;

export const labsExperimentReceiptSchema = z
  .object({
    schemaVersion: z.literal(LABS_SCHEMA_VERSION),
    receiptId: z.string().uuid(),
    runId: z.string().uuid(),
    experimentId: labsExperimentIdSchema,
    state: labsExperimentStateSchema,
    scope: z
      .object({
        mode: z.enum(["desktop_silo", "local_temporary"]),
        siloId: z.string().uuid().nullable(),
        siteHost: z.string().trim().min(1).max(253),
      })
      .strict(),
    startedAt: z.string().datetime(),
    finalizedAt: z.string().datetime(),
    expiresAt: z.string().datetime(),
    phases: z.array(labsPhaseReceiptSchema).min(1).max(4),
    stopCode: labsStopConditionCodeSchema.nullable(),
    restore: z
      .object({ attempted: z.boolean(), succeeded: z.boolean() })
      .strict(),
    coverage: labsCoverageSchema,
    sanitized: z.literal(true),
  })
  .strict()
  .superRefine((receipt, context) => {
    if (Date.parse(receipt.finalizedAt) < Date.parse(receipt.startedAt)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["finalizedAt"],
        message: "A Labs receipt cannot finish before it starts.",
      });
    }
    if (Date.parse(receipt.expiresAt) <= Date.parse(receipt.finalizedAt)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAt"],
        message: "A Labs receipt must have a bounded retention period.",
      });
    }
    if (
      receipt.state === "verified" &&
      receipt.coverage.injectionOrder !== "document_start_guaranteed"
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["state"],
        message:
          "A Dedicated Worker experiment cannot be verified without guaranteed document-start ordering.",
      });
    }
    if (
      receipt.state === "leak_detected" &&
      (receipt.stopCode === null || !receipt.restore.succeeded)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["restore"],
        message: "A leak receipt must record a successful automatic restore.",
      });
    }
    if (
      receipt.experimentId !== "dedicated_worker_constructor" &&
      receipt.state === "verified"
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["state"],
        message: "Unsupported Labs experiments can never be verified.",
      });
    }
  });
export type LabsExperimentReceipt = z.infer<typeof labsExperimentReceiptSchema>;

export const labsExperimentSchema = z
  .object({
    schemaVersion: z.literal(LABS_SCHEMA_VERSION),
    definitionRevision: z.literal(LABS_DEFINITION_REVISION),
    runId: z.string().uuid().nullable(),
    id: labsExperimentIdSchema,
    state: labsExperimentStateSchema,
    phase: labsExperimentPhaseSchema.nullable(),
    enabled: z.boolean(),
    assurance: z.enum(["unverified", "best_effort", "verified", "unsupported"]),
    scope: labsExperimentScopeSchema.nullable(),
    updatedAt: z.string().datetime(),
    expiresAt: z.string().datetime().nullable(),
    coverage: labsCoverageSchema,
    stopConditions: z.array(labsStopConditionSchema).min(1).max(32),
    lastReceipt: labsExperimentReceiptSchema.nullable(),
  })
  .strict()
  .superRefine((experiment, context) => {
    const unsupported = experiment.id !== "dedicated_worker_constructor";
    if (
      unsupported &&
      (experiment.state !== "unsupported" ||
        experiment.enabled ||
        experiment.assurance !== "unsupported" ||
        experiment.scope !== null)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["state"],
        message:
          "Unsupported Labs experiments must stay unavailable, disabled, and unscoped.",
      });
    }
    if (
      ["applying", "best_effort", "verified"].includes(experiment.state) &&
      (!experiment.enabled ||
        experiment.scope === null ||
        experiment.runId === null ||
        experiment.expiresAt === null)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["enabled"],
        message:
          "An active Labs experiment requires an explicit scoped authorization and bounded run.",
      });
    }
    if (
      [
        "disabled",
        "permission_missing",
        "failed",
        "leak_detected",
        "restored",
      ].includes(experiment.state) &&
      experiment.enabled
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["enabled"],
        message: "A stopped Labs state cannot remain enabled.",
      });
    }
    if (
      experiment.state === "verified" &&
      (experiment.assurance !== "verified" ||
        experiment.coverage.injectionOrder !== "document_start_guaranteed")
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["state"],
        message:
          "Verified Labs state requires verified assurance and guaranteed document-start ordering.",
      });
    }
    if (
      experiment.state === "best_effort" &&
      experiment.assurance !== "best_effort"
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["assurance"],
        message: "Best-effort state must remain explicitly best-effort.",
      });
    }
  });
export type LabsExperiment = z.infer<typeof labsExperimentSchema>;

export const labsSiteAuthorizationSchema = z
  .object({
    schemaVersion: z.literal(LABS_SCHEMA_VERSION),
    experimentId: z.literal("dedicated_worker_constructor"),
    scope: labsExperimentScopeSchema,
    enabled: z.boolean(),
    updatedAt: z.string().datetime(),
  })
  .strict();
export type LabsSiteAuthorization = z.infer<typeof labsSiteAuthorizationSchema>;

const allowedTransitions: Record<
  LabsExperimentState,
  readonly LabsExperimentState[]
> = {
  disabled: ["permission_missing", "applying", "unsupported"],
  permission_missing: ["disabled", "applying"],
  applying: [
    "best_effort",
    "verified",
    "failed",
    "leak_detected",
    "restored",
    "permission_missing",
  ],
  best_effort: ["failed", "leak_detected", "restored"],
  verified: ["failed", "leak_detected", "restored"],
  failed: ["disabled", "applying", "restored"],
  leak_detected: ["disabled", "applying", "restored"],
  restored: ["disabled", "applying"],
  unsupported: ["unsupported"],
};

export function canTransitionLabsExperimentState(
  from: LabsExperimentState,
  to: LabsExperimentState,
): boolean {
  return from === to || allowedTransitions[from].includes(to);
}

export function createDefaultLabsExperiments(
  now = new Date(),
): LabsExperiment[] {
  const updatedAt = now.toISOString();
  return LABS_EXPERIMENT_DEFINITIONS.map((definition) =>
    labsExperimentSchema.parse({
      schemaVersion: LABS_SCHEMA_VERSION,
      definitionRevision: LABS_DEFINITION_REVISION,
      runId: null,
      id: definition.id,
      state: definition.tier === "unsupported" ? "unsupported" : "disabled",
      phase: null,
      enabled: false,
      assurance:
        definition.tier === "unsupported" ? "unsupported" : "unverified",
      scope: null,
      updatedAt,
      expiresAt: null,
      coverage: DEFAULT_LABS_COVERAGE,
      stopConditions: definition.stopConditions,
      lastReceipt: null,
    }),
  );
}

export function isLabsExperimentExpired(
  experiment: LabsExperiment,
  nowUnixMs = Date.now(),
): boolean {
  return (
    experiment.expiresAt !== null &&
    Date.parse(experiment.expiresAt) <= nowUnixMs
  );
}
