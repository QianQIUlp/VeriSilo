import { z } from "zod";

export const ENGINE_CONTRACT_VERSION = 1 as const;
export const ENGINE_RUNTIME_RECEIPT_VERSION = 1 as const;
export const MAX_ENGINE_RUNTIME_RECEIPT_BYTES = 32 * 1024;
export const CAMOUFOX_ARTIFACT_SCHEMA =
  "verisilo-camoufox-resolved-identity/v3" as const;
export const CAMOUFOX_HOST_PROTOCOL = "verisilo-camoufox-host/v1" as const;
export const CAMOUFOX_HOST_ENTRYPOINT_KIND = "camoufox-host-v1" as const;

export const engineAdapterIdSchema = z.enum([
  "stock-chrome",
  "stock-edge",
  "controlled-chromium",
  "camoufox",
]);
export type EngineAdapterId = z.infer<typeof engineAdapterIdSchema>;

export const engineChannelSchema = z.enum([
  "stable",
  "experimental",
  "development",
]);
export type EngineChannel = z.infer<typeof engineChannelSchema>;

export const engineCapabilityIdSchema = z.enum([
  "profile_isolation",
  "launch_network",
  "identity_template",
  "ua_ua_ch",
  "language_timezone",
  "screen",
  "canvas",
  "webgl",
  "fonts",
  "media_devices",
  "request_headers",
  "window",
  "iframe",
  "dedicated_worker",
  "tls_client_hello",
  "quic",
  "site_fallback",
]);
export type EngineCapabilityId = z.infer<typeof engineCapabilityIdSchema>;

export const engineCapabilityAvailabilitySchema = z.enum([
  "supported",
  "experimental",
  "unavailable",
]);
export type EngineCapabilityAvailability = z.infer<
  typeof engineCapabilityAvailabilitySchema
>;

export const engineCapabilityOperationSchema = z.enum([
  "not_configured",
  "configured",
  "applied",
  "verified",
  "failed",
]);
export type EngineCapabilityOperation = z.infer<
  typeof engineCapabilityOperationSchema
>;

export const engineCapabilityStateSchema = z
  .object({
    id: engineCapabilityIdSchema,
    availability: engineCapabilityAvailabilitySchema,
    operation: engineCapabilityOperationSchema,
    reason: z.string().trim().min(1).max(300),
    verifiedAt: z.string().datetime().nullable(),
    evidence: z.array(z.string().trim().min(1).max(512)).max(16),
  })
  .strict()
  .superRefine((capability, context) => {
    if (
      capability.availability === "unavailable" &&
      !["not_configured", "failed"].includes(capability.operation)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["operation"],
        message: "An unavailable capability cannot be applied or verified.",
      });
    }
    if (
      capability.operation === "verified" &&
      (capability.verifiedAt === null || capability.evidence.length === 0)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["operation"],
        message: "Verified engine capabilities require direct evidence.",
      });
    }
    if (capability.operation !== "verified" && capability.verifiedAt !== null) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["verifiedAt"],
        message: "Only verified capabilities may carry verifiedAt.",
      });
    }
  });
export type EngineCapabilityState = z.infer<typeof engineCapabilityStateSchema>;

export const engineCapabilityEvidenceSchema = z
  .object({
    id: engineCapabilityIdSchema,
    evidence: z.array(z.string().trim().min(1).max(512)).min(1).max(16),
  })
  .strict();
export type EngineCapabilityEvidence = z.infer<
  typeof engineCapabilityEvidenceSchema
>;

export const engineDescriptorSchema = z
  .object({
    contractVersion: z.literal(ENGINE_CONTRACT_VERSION),
    id: engineAdapterIdSchema,
    adapterVersion: z.string().trim().min(1).max(64),
    engineVersion: z.string().trim().min(1).max(64),
    channel: engineChannelSchema,
    browserFamily: z.enum(["chromium", "firefox"]),
    platform: z.literal("windows-x64"),
    externallyPackaged: z.boolean(),
    emergencyDisabled: z.boolean(),
  })
  .strict();
export type EngineDescriptor = z.infer<typeof engineDescriptorSchema>;

const uaChBrandSchema = z
  .object({
    brand: z.string().trim().min(1).max(64),
    version: z.string().regex(/^\d{1,3}$/u),
  })
  .strict();

const uaChSchema = z
  .object({
    brands: z.array(uaChBrandSchema).min(1).max(8),
    platform: z.literal("Windows"),
    platformVersion: z.string().regex(/^\d+(?:\.\d+){0,3}$/u),
    architecture: z.literal("x86"),
    bitness: z.enum(["32", "64"]),
    mobile: z.boolean(),
  })
  .strict();

export const identityTemplateSchema = z
  .object({
    schemaVersion: z.literal(1),
    templateId: z.string().uuid(),
    os: z
      .object({
        family: z.literal("windows"),
        version: z.enum(["10", "11"]),
        architecture: z.literal("x64"),
      })
      .strict(),
    browser: z
      .object({
        family: z.enum(["chromium", "firefox"]),
        majorVersion: z.number().int().min(100).max(999),
        userAgent: z.string().trim().min(20).max(512),
        uaCh: uaChSchema.nullable(),
      })
      .strict(),
    languages: z
      .object({
        primary: z.string().regex(/^[A-Za-z]{2,3}(?:-[A-Za-z]{2})?$/u),
        accepted: z
          .array(z.string().regex(/^[A-Za-z]{2,3}(?:-[A-Za-z]{2})?$/u))
          .min(1)
          .max(8),
      })
      .strict(),
    timezone: z
      .string()
      .trim()
      .min(1)
      .max(80)
      .refine(
        (value) =>
          value === "UTC" ||
          /^[A-Za-z0-9_+.-]+(?:\/[A-Za-z0-9_+.-]+)+$/u.test(value),
        "Timezone must be UTC or an IANA-style identifier.",
      ),
    screen: z
      .object({
        width: z.number().int().min(800).max(16_384),
        height: z.number().int().min(600).max(16_384),
        availableWidth: z.number().int().min(640).max(16_384),
        availableHeight: z.number().int().min(480).max(16_384),
        devicePixelRatio: z.number().min(0.5).max(8),
        colorDepth: z.union([z.literal(24), z.literal(30), z.literal(32)]),
      })
      .strict(),
    render: z
      .object({
        canvas: z.enum(["native", "normalized", "controlled"]),
        webGlVendor: z.string().trim().min(1).max(160).nullable(),
        webGlRenderer: z.string().trim().min(1).max(300).nullable(),
      })
      .strict(),
    fonts: z
      .object({
        families: z.array(z.string().trim().min(1).max(100)).min(1).max(64),
      })
      .strict(),
    media: z
      .object({
        microphones: z.number().int().min(0).max(16),
        cameras: z.number().int().min(0).max(16),
        speakers: z.number().int().min(0).max(16),
        labelsExposed: z.boolean(),
      })
      .strict(),
    network: z
      .object({
        proxyRequired: z.boolean(),
        countryCode: z
          .string()
          .regex(/^[A-Z]{2}$/u)
          .nullable(),
        timezone: z.string().trim().min(1).max(80).nullable(),
        locale: z
          .string()
          .regex(/^[A-Za-z]{2,3}(?:-[A-Za-z]{2})?$/u)
          .nullable(),
        desiredQuic: z.literal("browser_default"),
      })
      .strict(),
  })
  .strict()
  .superRefine((template, context) => {
    if (!template.browser.userAgent.includes("Windows NT 10.0")) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["browser", "userAgent"],
        message: "Windows 10/11 templates must use the Windows NT 10.0 token.",
      });
    }
    const versionPattern =
      template.browser.family === "chromium"
        ? /(?:Chrome|Edg)\/(\d{1,3})/u
        : /Firefox\/(\d{1,3})/u;
    const uaMajor = Number(
      versionPattern.exec(template.browser.userAgent)?.[1],
    );
    if (uaMajor !== template.browser.majorVersion) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["browser", "majorVersion"],
        message:
          "Browser majorVersion must match the user-agent major version.",
      });
    }
    if (template.browser.family === "chromium") {
      const chromiumBrand = template.browser.uaCh?.brands.find(
        (brand) => brand.brand === "Chromium",
      );
      if (
        template.browser.uaCh === null ||
        chromiumBrand?.version !== String(template.browser.majorVersion) ||
        template.browser.uaCh.bitness !== "64" ||
        template.browser.uaCh.mobile
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["browser", "uaCh"],
          message:
            "Desktop Chromium templates require matching 64-bit, non-mobile UA-CH.",
        });
      }
    } else if (template.browser.uaCh !== null) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["browser", "uaCh"],
        message: "The Camoufox prototype does not accept Chromium UA-CH data.",
      });
    }
    if (
      template.languages.accepted[0] !== template.languages.primary ||
      new Set(template.languages.accepted.map((value) => value.toLowerCase()))
        .size !== template.languages.accepted.length
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["languages", "accepted"],
        message: "Primary language must be first and languages must be unique.",
      });
    }
    if (
      template.screen.availableWidth > template.screen.width ||
      template.screen.availableHeight > template.screen.height
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["screen"],
        message:
          "Available screen dimensions cannot exceed physical dimensions.",
      });
    }
    if (
      (template.render.webGlVendor === null) !==
      (template.render.webGlRenderer === null)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["render"],
        message: "WebGL vendor and renderer must be configured together.",
      });
    }
    if (
      new Set(template.fonts.families.map((font) => font.toLowerCase()))
        .size !== template.fonts.families.length ||
      !template.fonts.families.includes("Segoe UI")
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["fonts", "families"],
        message: "Windows templates require unique fonts including Segoe UI.",
      });
    }
    if (
      template.media.labelsExposed &&
      template.media.microphones +
        template.media.cameras +
        template.media.speakers ===
        0
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["media", "labelsExposed"],
        message: "Device labels cannot be exposed when no devices exist.",
      });
    }
    if (
      template.network.timezone !== null &&
      template.network.timezone !== template.timezone
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["network", "timezone"],
        message: "Network and browser timezone declarations must agree.",
      });
    }
    if (
      template.network.locale !== null &&
      template.network.locale.toLowerCase() !==
        template.languages.primary.toLowerCase()
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["network", "locale"],
        message: "Network locale and primary browser language must agree.",
      });
    }
  });
export type IdentityTemplate = z.infer<typeof identityTemplateSchema>;

export const identityDerivationContextSchema = z
  .object({
    siloId: z.string().uuid(),
    seedReference: z.string().uuid(),
    templateId: z.string().uuid(),
    sessionId: z.string().uuid(),
    issuedAt: z.string().datetime(),
    expiresAt: z.string().datetime(),
  })
  .strict()
  .superRefine((context, refinement) => {
    const lifetime =
      Date.parse(context.expiresAt) - Date.parse(context.issuedAt);
    if (lifetime <= 0 || lifetime > 60 * 60 * 1_000) {
      refinement.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAt"],
        message: "Derived identity tokens must expire within one hour.",
      });
    }
  });
export type IdentityDerivationContext = z.infer<
  typeof identityDerivationContextSchema
>;

export const derivedIdentityTokenSchema = z
  .object({
    tokenId: z.string().uuid(),
    delivery: z.literal("secure_stdin_before_navigation"),
    expiresAt: z.string().datetime(),
  })
  .strict();
export type DerivedIdentityToken = z.infer<typeof derivedIdentityTokenSchema>;

export const camoufoxArtifactBindingV1Schema = z
  .object({
    artifactId: z
      .string()
      .regex(/^identity-[a-z0-9][a-z0-9-]{0,63}$/u),
    artifactFileSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    schema: z.literal(CAMOUFOX_ARTIFACT_SCHEMA),
  })
  .strict();
export type CamoufoxArtifactBindingV1 = z.infer<
  typeof camoufoxArtifactBindingV1Schema
>;

export const siteFallbackRuleSchema = z
  .object({
    sitePattern: z
      .string()
      .trim()
      .min(1)
      .max(253)
      .regex(
        /^(?:\*\.)?[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$/u,
      ),
    disableCapabilities: z.array(engineCapabilityIdSchema).min(1).max(16),
    action: z.literal("restore_then_reload"),
  })
  .strict();

export const siloEngineConfigSchema = z
  .discriminatedUnion("adapter", [
    z.object({ adapter: z.literal("stock") }).strict(),
    z
      .object({
        adapter: z.literal("controlled-chromium"),
        identityTemplate: identityTemplateSchema,
        fallbackRules: z.array(siteFallbackRuleSchema).max(100),
      })
      .strict(),
    z
      .object({
        adapter: z.literal("camoufox"),
        identityTemplate: identityTemplateSchema,
        fallbackRules: z.array(siteFallbackRuleSchema).max(100),
        /** Older persisted Camoufox configs may omit this and fail closed at launch. */
        artifactBinding: camoufoxArtifactBindingV1Schema.optional(),
      })
      .strict(),
  ])
  .superRefine((config, context) => {
    if (
      config.adapter === "controlled-chromium" &&
      config.identityTemplate.browser.family !== "chromium"
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["identityTemplate", "browser", "family"],
        message: "Controlled Chromium requires a Chromium identity template.",
      });
    }
    if (
      config.adapter === "camoufox" &&
      config.identityTemplate.browser.family !== "firefox"
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["identityTemplate", "browser", "family"],
        message: "Camoufox requires a Firefox identity template.",
      });
    }
  });
export type SiloEngineConfig = z.infer<typeof siloEngineConfigSchema>;

export const engineControlPlanSchema = z
  .object({
    sessionId: z.string().uuid(),
    templateId: z.string().uuid(),
    phases: z.tuple([
      z.literal("observe"),
      z.literal("apply"),
      z.literal("verify"),
      z.literal("restore"),
    ]),
    capabilities: z.array(engineCapabilityStateSchema).max(32),
    siteFallback: z
      .object({
        defaultAction: z.literal("restore_experimental_controls"),
        rules: z.array(siteFallbackRuleSchema).max(100),
      })
      .strict(),
  })
  .strict();
export type EngineControlPlan = z.infer<typeof engineControlPlanSchema>;

export const engineTransportSchema = z.enum([
  "stock",
  "native-bootstrap-v1",
  "camoufox-host-jsonl-v1",
]);
export type EngineTransport = z.infer<typeof engineTransportSchema>;

export const camoufoxHostLaunchSchema = z
  .object({
    protocol: z.literal(CAMOUFOX_HOST_PROTOCOL),
    hostVersion: z.string().trim().min(1).max(64),
    platform: z.string().trim().min(1).max(64),
    artifactId: z.string().regex(/^identity-[a-z0-9][a-z0-9-]{0,63}$/u),
    artifactFileSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    profileId: z.string().regex(/^[a-z0-9][a-z0-9-]{0,63}$/u),
    browserRelease: z
      .string()
      .regex(/^v(?:[1-9][0-9]{2})\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/u),
    browserAssetSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    browserTreeManifestPath: z.string().trim().min(1).max(4096),
    browserTreeManifestSha256: z.string().regex(/^[a-f0-9]{64}$/u),
  })
  .strict();
export type CamoufoxHostLaunch = z.infer<typeof camoufoxHostLaunchSchema>;

export const engineBootstrapPackageBindingSchema = z
  .object({
    engineVersion: z.string().trim().min(1).max(64),
    artifactSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    verifierId: z.string().trim().min(1).max(100),
    verifiedAt: z.string().datetime(),
  })
  .strict();

const engineRuntimeReceiptPayloadSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("phase"),
      phase: z.enum(["observe", "apply", "verify", "restore"]),
      capabilities: z.array(engineCapabilityEvidenceSchema).max(17),
    })
    .strict(),
  z
    .object({
      kind: z.literal("site_fallback"),
      site: z
        .string()
        .min(1)
        .max(253)
        .regex(
          /^(?=.{1,253}$)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)(?:\.(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?))*$/u,
        ),
      matchedPattern: z.string().trim().min(1).max(255),
      action: z.enum(["restore_experimental_controls", "restore_then_reload"]),
      capabilities: z.array(engineCapabilityEvidenceSchema).min(1).max(17),
    })
    .strict(),
]);

export const engineRuntimeReceiptFrameSchema = z
  .object({
    receiptVersion: z.literal(ENGINE_RUNTIME_RECEIPT_VERSION),
    contractVersion: z.literal(ENGINE_CONTRACT_VERSION),
    adapterId: z.enum(["controlled-chromium", "camoufox"]),
    siloId: z.string().uuid(),
    sessionId: z.string().uuid(),
    tokenId: z.string().uuid(),
    package: engineBootstrapPackageBindingSchema,
    sequence: z.number().int().min(1).max(Number.MAX_SAFE_INTEGER),
    issuedAt: z.string().datetime(),
    expiresAt: z.string().datetime(),
    receipt: engineRuntimeReceiptPayloadSchema,
  })
  .strict()
  .superRefine((frame, context) => {
    const issuedAt = Date.parse(frame.issuedAt);
    const expiresAt = Date.parse(frame.expiresAt);
    if (expiresAt <= issuedAt || expiresAt - issuedAt > 30_000) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAt"],
        message:
          "Runtime receipt validity must be positive and at most 30 seconds.",
      });
    }
    if (
      new TextEncoder().encode(JSON.stringify(frame)).byteLength >
      MAX_ENGINE_RUNTIME_RECEIPT_BYTES
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Runtime receipt frame exceeds 32 KiB.",
      });
    }
  });
export type EngineRuntimeReceiptFrame = z.infer<
  typeof engineRuntimeReceiptFrameSchema
>;

export const engineLaunchPlanSchema = z
  .object({
    adapter: engineDescriptorSchema,
    transport: engineTransportSchema,
    executablePath: z.string().trim().min(1).max(4_096),
    arguments: z.array(z.string().min(1).max(4_096)).max(64),
    profileDirectory: z.string().trim().min(1).max(4_096),
    shell: z.literal(false),
    capabilities: z.array(engineCapabilityStateSchema).max(32),
    identityDelivery: z
      .object({
        tokenId: z.string().uuid(),
        delivery: z.literal("secure_stdin_before_navigation"),
        expiresAt: z.string().datetime(),
      })
      .strict()
      .nullable(),
    control: engineControlPlanSchema.nullable(),
    camoufoxHost: camoufoxHostLaunchSchema.nullable(),
    packageVerification: z
      .object({
        verifierId: z.string().trim().min(1).max(100),
        artifactSha256: z.string().regex(/^[a-f0-9]{64}$/u),
        digestVerified: z.boolean(),
        signatureVerified: z.boolean(),
        verifiedAt: z.string().datetime(),
      })
      .strict()
      .nullable(),
  })
  .strict()
  .superRefine((plan, context) => {
    const hasControlledBootstrap =
      plan.transport === "native-bootstrap-v1" &&
      plan.identityDelivery !== null &&
      plan.control !== null;
    const packageVerified =
      plan.packageVerification !== null &&
      plan.packageVerification.digestVerified &&
      plan.packageVerification.signatureVerified;
    if (plan.adapter.externallyPackaged !== packageVerified) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["adapter", "externallyPackaged"],
        message:
          "Only a per-launch verified external adapter may use an external launch transport.",
      });
    }
    if (plan.transport === "stock") {
      if (
        plan.adapter.externallyPackaged ||
        plan.identityDelivery !== null ||
        plan.control !== null ||
        plan.camoufoxHost !== null ||
        plan.packageVerification !== null
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["transport"],
          message: "Stock transport cannot carry controlled-engine bindings.",
        });
      }
    }
    if (
      plan.transport === "native-bootstrap-v1" &&
      (!hasControlledBootstrap || plan.adapter.id !== "controlled-chromium")
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["transport"],
        message:
          "Native bootstrap transport requires the Controlled Chromium adapter and both native control bindings.",
      });
    }
    if (plan.transport === "native-bootstrap-v1" && plan.camoufoxHost !== null) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["camoufoxHost"],
        message: "Native bootstrap transport cannot carry a Camoufox Host binding.",
      });
    }
    if (plan.transport === "camoufox-host-jsonl-v1") {
      if (
        plan.adapter.id !== "camoufox" ||
        plan.identityDelivery !== null ||
        plan.control !== null ||
        plan.camoufoxHost === null
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["transport"],
          message:
            "Camoufox Host transport requires a Camoufox adapter and Host binding without generic receipts.",
        });
      }
    }
  });
export type EngineLaunchPlan = z.infer<typeof engineLaunchPlanSchema>;

const signatureSchema = z
  .object({
    algorithm: z.literal("cms-detached-sha256"),
    /** Exact SHA-256 of the pinned DER signing certificate. */
    keyId: z.string().regex(/^[a-f0-9]{64}$/u),
    /** Canonical, padded standard base64 detached CMS SignedData. */
    value: z
      .string()
      .min(256)
      .max(60_000)
      .regex(
        /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u,
      ),
  })
  .strict();

const externalEngineCapabilitySchema = z.enum([
  "identity_template",
  "ua_ua_ch",
  "language_timezone",
  "screen",
  "canvas",
  "webgl",
  "fonts",
  "media_devices",
  "request_headers",
  "window",
  "iframe",
  "dedicated_worker",
  "site_fallback",
]);

const enginePackageManifestV2Schema = z
  .object({
    schemaVersion: z.literal(2),
    engineId: z.literal("controlled-chromium"),
    engineVersion: z
      .string()
      .regex(
        /^(?:[1-9][0-9]{2})\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/u,
      ),
    channel: z.literal("experimental"),
    platform: z.literal("windows-x64"),
    executableRelativePath: z.enum(["bin/chromium.exe", "bin/camoufox.exe"]),
    artifactSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    signature: signatureSchema,
    capabilities: z
      .array(externalEngineCapabilitySchema)
      .max(externalEngineCapabilitySchema.options.length)
      .refine(
        (capabilities) => new Set(capabilities).size === capabilities.length,
        "Package capabilities must be unique.",
      ),
  })
  .strict()
  .superRefine((manifest, context) => {
    const expectedExecutable = "bin/chromium.exe";
    if (manifest.executableRelativePath !== expectedExecutable) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["executableRelativePath"],
        message: "Executable path must match the selected engine adapter.",
      });
    }
    for (const required of ["identity_template", "site_fallback"] as const) {
      if (!manifest.capabilities.includes(required)) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["capabilities"],
          message: `External engine packages must declare ${required}.`,
        });
      }
    }
  });

const enginePackageVersionSchema = z.string().regex(
  /^(?:[1-9][0-9]{2})\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/u,
);

const enginePackageSignatureSchema = signatureSchema;

export const camoufoxHostPackageTreeManifestSchema = z
  .object({
    schema: z.literal("verisilo-camoufox-host-package-tree/v1"),
    entries: z
      .array(
        z
          .object({
            path: z
              .string()
              .regex(/^(?![\\/])(?:[A-Za-z0-9._-]+\/)*[A-Za-z0-9._-]+$/u),
            sha256: z.string().regex(/^[a-f0-9]{64}$/u),
          })
          .strict(),
      )
      .min(1)
      .max(65_536),
  })
  .strict()
  .superRefine((manifest, context) => {
    const paths = manifest.entries.map((entry) => entry.path);
    if (new Set(paths).size !== paths.length) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["entries"],
        message: "Package tree entries must be unique.",
      });
    }
  });
export type CamoufoxHostPackageTreeManifest = z.infer<
  typeof camoufoxHostPackageTreeManifestSchema
>;

export const camoufoxHostPackageManifestSchema = z
  .object({
    schemaVersion: z.literal(3),
    engineId: z.literal("camoufox"),
    engineVersion: enginePackageVersionSchema,
    channel: z.literal("experimental"),
    platform: z.literal("windows-x64"),
    artifactSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    signature: enginePackageSignatureSchema,
    capabilities: z
      .array(externalEngineCapabilitySchema)
      .max(externalEngineCapabilitySchema.options.length)
      .refine(
        (capabilities) => new Set(capabilities).size === capabilities.length,
        "Package capabilities must be unique.",
      ),
    entrypoint: z
      .object({
        kind: z.literal(CAMOUFOX_HOST_ENTRYPOINT_KIND),
        relativePath: z
          .string()
          .regex(/^(?![\\/])(?:[A-Za-z0-9._-]+\/)*[A-Za-z0-9._-]+$/u),
        protocol: z.literal(CAMOUFOX_HOST_PROTOCOL),
        sha256: z.string().regex(/^[a-f0-9]{64}$/u),
      })
      .strict(),
    treeManifest: z
      .object({
        relativePath: z
          .string()
          .regex(/^(?![\\/])(?:[A-Za-z0-9._-]+\/)*[A-Za-z0-9._-]+$/u),
        sha256: z.string().regex(/^[a-f0-9]{64}$/u),
      })
      .strict(),
    browserTreeManifest: z
      .object({
        relativePath: z
          .string()
          .regex(/^(?![\\/])(?:[A-Za-z0-9._-]+\/)*[A-Za-z0-9._-]+$/u),
        sha256: z.string().regex(/^[a-f0-9]{64}$/u),
      })
      .strict(),
    hostVersion: z.string().trim().min(1).max(64),
    browserRelease: z
      .string()
      .regex(/^v(?:[1-9][0-9]{2})\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/u),
    browserAssetSha256: z.string().regex(/^[a-f0-9]{64}$/u),
  })
  .strict()
  .superRefine((manifest, context) => {
    if (manifest.artifactSha256 !== manifest.entrypoint.sha256) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["artifactSha256"],
        message: "Camoufox package artifactSha256 must bind the Host entrypoint.",
      });
    }
    if (manifest.browserRelease !== `v${manifest.engineVersion}`) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["browserRelease"],
        message: "Camoufox browserRelease must bind the accepted v-prefixed engine release.",
      });
    }
    if (!manifest.capabilities.includes("identity_template")) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["capabilities"],
        message: "Camoufox Host packages must declare identity_template.",
      });
    }
    if (manifest.capabilities.includes("site_fallback")) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["capabilities"],
        message: "Camoufox Host v1 does not implement site_fallback.",
      });
    }
  });
export type CamoufoxHostPackageManifest = z.infer<
  typeof camoufoxHostPackageManifestSchema
>;

export const enginePackageManifestSchema = z.union([
  enginePackageManifestV2Schema,
  camoufoxHostPackageManifestSchema,
]);
export type EnginePackageManifest = z.infer<typeof enginePackageManifestSchema>;

export const enginePackageRequestSchema = z
  .object({
    packageRoot: z.string().trim().min(1).max(4_096),
    expectedVersion: z
      .string()
      .regex(/^\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$/u),
  })
  .strict();
export type EnginePackageRequest = z.infer<typeof enginePackageRequestSchema>;

export const engineMaintenanceReceiptSchema = z
  .object({
    action: z.enum(["install", "update", "rollback"]),
    adapterId: engineAdapterIdSchema,
    engineVersion: z.string().trim().min(1).max(64),
    verifierId: z.string().trim().min(1).max(100),
    verifiedAt: z.string().datetime(),
  })
  .strict();
export type EngineMaintenanceReceipt = z.infer<
  typeof engineMaintenanceReceiptSchema
>;

export const engineHealthSchema = z
  .object({
    state: z.enum(["healthy", "degraded", "unavailable", "emergency_disabled"]),
    checkedAt: z.string().datetime(),
    message: z.string().trim().min(1).max(500),
  })
  .strict();
export type EngineHealth = z.infer<typeof engineHealthSchema>;

export const engineNegotiationSchema = z
  .object({
    adapter: engineDescriptorSchema,
    capabilities: z.array(engineCapabilityStateSchema).max(32),
    accepted: z.array(engineCapabilityIdSchema).max(32),
    rejected: z.array(engineCapabilityIdSchema).max(32),
  })
  .strict();
export type EngineNegotiation = z.infer<typeof engineNegotiationSchema>;

export interface EngineAdapterContract {
  descriptor(): EngineDescriptor;
  negotiate(requested: EngineCapabilityId[]): EngineNegotiation;
  install(request: EnginePackageRequest): EngineMaintenanceReceipt;
  update(request: EnginePackageRequest): EngineMaintenanceReceipt;
  launchPlan(request: {
    profileDirectory: string;
    identity: IdentityTemplate | null;
    token: DerivedIdentityToken | null;
    fallbackRules: z.infer<typeof siteFallbackRuleSchema>[];
  }): EngineLaunchPlan;
  health(): EngineHealth;
  rollback(): EngineMaintenanceReceipt;
  setEmergencyDisabled(disabled: boolean, reason: string | null): void;
  validateIdentityTemplate(template: IdentityTemplate): void;
  controlPlan(
    sessionId: string,
    template: IdentityTemplate,
    fallbackRules: z.infer<typeof siteFallbackRuleSchema>[],
  ): EngineControlPlan;
}

export function parseIdentityTemplate(value: unknown): IdentityTemplate {
  return identityTemplateSchema.parse(value);
}
