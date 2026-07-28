import { z } from "zod";

export const NETWORK_CHECK_ORIGINS = [
  "https://ipwho.is/*",
  "https://cloudflare-dns.com/*",
  "https://dns.google/*",
] as const;

export const NETWORK_CHECK_ENDPOINTS = {
  ip: "https://ipwho.is/",
  cloudflareDns:
    "https://cloudflare-dns.com/dns-query?name=example.com&type=A&do=true",
  googleDns:
    "https://dns.google/resolve?name=example.com&type=A&do=true&edns_client_subnet=0.0.0.0%2F0",
} as const;

export const NETWORK_REPUTATION_EXPLANATION =
  "未查询商业信誉库或黑名单；运营商与机房线索不能代表 IP 一定干净或一定有风险。" as const;

const nullableBoundedString = (maximum: number) =>
  z.string().trim().min(1).max(maximum).nullable();

export const ipExitObservationSchema = z
  .object({
    address: z.string().trim().min(1).max(64),
    version: z.enum(["IPv4", "IPv6", "unknown"]),
    country: nullableBoundedString(100),
    countryCode: nullableBoundedString(8),
    region: nullableBoundedString(120),
    city: nullableBoundedString(120),
    asn: z
      .string()
      .regex(/^AS\d{1,10}$/u)
      .nullable(),
    organization: nullableBoundedString(160),
    isp: nullableBoundedString(160),
    timezone: nullableBoundedString(80),
    networkHint: z.enum(["cloud_or_hosting", "unknown"]),
  })
  .strict();
export type IpExitObservation = z.infer<typeof ipExitObservationSchema>;

export const dnsProviderObservationSchema = z
  .object({
    provider: z.enum(["Cloudflare", "Google"]),
    status: z.number().int().min(0).max(65_535),
    dnssecAuthenticated: z.boolean(),
    addresses: z.array(z.string().regex(/^(?:\d{1,3}\.){3}\d{1,3}$/u)).max(16),
  })
  .strict();
export type DnsProviderObservation = z.infer<
  typeof dnsProviderObservationSchema
>;

export const networkCheckResultSchema = z
  .object({
    schemaVersion: z.literal(1),
    checkedAt: z.string().datetime(),
    ip: ipExitObservationSchema.nullable(),
    dns: z
      .object({
        state: z.enum([
          "consistent",
          "different",
          "resolver_error",
          "partial",
          "failed",
        ]),
        dnssec: z.enum([
          "validated",
          "not_validated",
          "partial",
          "unavailable",
        ]),
        queryName: z.literal("example.com"),
        providers: z
          .array(dnsProviderObservationSchema)
          .max(2)
          .refine(
            (providers) =>
              new Set(providers.map((provider) => provider.provider)).size ===
              providers.length,
            "Public DoH providers must be unique.",
          ),
      })
      .strict(),
    reputation: z
      .object({
        state: z.literal("not_scored"),
        explanation: z.literal(NETWORK_REPUTATION_EXPLANATION),
      })
      .strict(),
    errors: z.array(z.string().max(300)).max(10),
  })
  .strict();
export type NetworkCheckResult = z.infer<typeof networkCheckResultSchema>;

export interface NetworkCheckInput {
  ipPayload: unknown | null;
  cloudflareDnsPayload: unknown | null;
  googleDnsPayload: unknown | null;
  errors?: string[];
  checkedAt?: string;
}

export function buildNetworkCheckResult(
  input: NetworkCheckInput,
): NetworkCheckResult {
  const ip = parseIpExit(input.ipPayload);
  const providers = [
    parseDnsProvider("Cloudflare", input.cloudflareDnsPayload),
    parseDnsProvider("Google", input.googleDnsPayload),
  ].filter((provider): provider is DnsProviderObservation => provider !== null);
  return {
    schemaVersion: 1,
    checkedAt: input.checkedAt ?? new Date().toISOString(),
    ip,
    dns: {
      state: dnsState(providers),
      dnssec: dnssecState(providers),
      queryName: "example.com",
      providers,
    },
    reputation: {
      state: "not_scored",
      explanation: NETWORK_REPUTATION_EXPLANATION,
    },
    errors: [...(input.errors ?? [])].slice(0, 10),
  };
}

export function parseIpExit(value: unknown): IpExitObservation | null {
  const payload = recordValue(value);
  const address = boundedString(payload?.ip, 64);
  if (payload?.success !== true || address === null) {
    return null;
  }
  const connection = recordValue(payload.connection);
  const timezone = recordValue(payload.timezone);
  const organization =
    boundedString(connection?.org, 160) ?? boundedString(connection?.isp, 160);
  return {
    address,
    version:
      payload.type === "IPv4" || payload.type === "IPv6"
        ? payload.type
        : "unknown",
    country: boundedString(payload.country, 100),
    countryCode: boundedString(payload.country_code, 8),
    region: boundedString(payload.region, 120),
    city: boundedString(payload.city, 120),
    asn: asnLabel(connection?.asn),
    organization,
    isp: boundedString(connection?.isp, 160),
    timezone: boundedString(timezone?.id, 80),
    networkHint: hasCloudOrHostingHint(organization)
      ? "cloud_or_hosting"
      : "unknown",
  };
}

export function parseDnsProvider(
  provider: DnsProviderObservation["provider"],
  value: unknown,
): DnsProviderObservation | null {
  const payload = recordValue(value);
  const status = finiteInteger(payload?.Status, 0, 65_535);
  if (payload === null || status === null) {
    return null;
  }
  const answers = Array.isArray(payload.Answer) ? payload.Answer : [];
  const addresses = answers
    .map(recordValue)
    .filter((answer) => answer?.type === 1)
    .map((answer) => boundedString(answer?.data, 64))
    .filter((address): address is string => address !== null)
    .filter(isIpv4Address);
  return {
    provider,
    status,
    dnssecAuthenticated: payload.AD === true,
    addresses: [...new Set(addresses)].sort(),
  };
}

export function isNetworkCheckResult(
  value: unknown,
): value is NetworkCheckResult {
  return networkCheckResultSchema.safeParse(value).success;
}

function dnsState(
  providers: DnsProviderObservation[],
): NetworkCheckResult["dns"]["state"] {
  if (providers.length === 0) {
    return "failed";
  }
  if (providers.length === 1) {
    return "partial";
  }
  if (providers.some((provider) => provider.status !== 0)) {
    return "resolver_error";
  }
  const [first, second] = providers;
  if (first === undefined || second === undefined) {
    return "partial";
  }
  return first.addresses.length > 0 &&
    JSON.stringify(first.addresses) === JSON.stringify(second.addresses)
    ? "consistent"
    : "different";
}

function dnssecState(
  providers: DnsProviderObservation[],
): NetworkCheckResult["dns"]["dnssec"] {
  if (providers.length === 0) {
    return "unavailable";
  }
  if (providers.length === 1) {
    return "partial";
  }
  return providers.every((provider) => provider.dnssecAuthenticated)
    ? "validated"
    : "not_validated";
}

function hasCloudOrHostingHint(organization: string | null): boolean {
  return (
    organization !== null &&
    /amazon|aws|azure|cloudflare|digitalocean|google cloud|hetzner|leaseweb|linode|microsoft|oracle cloud|ovh|tencent cloud|vultr/iu.test(
      organization,
    )
  );
}

function asnLabel(value: unknown): string | null {
  const number = finiteInteger(value, 1, 4_294_967_295);
  if (number !== null) {
    return `AS${number}`;
  }
  const text = boundedString(value, 32);
  if (text === null) {
    return null;
  }
  return /^AS\d+$/iu.test(text) ? text.toUpperCase() : null;
}

function isIpv4Address(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every((part) => {
      const number = Number(part);
      return /^\d{1,3}$/u.test(part) && number >= 0 && number <= 255;
    })
  );
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function boundedString(value: unknown, maximumLength: number): string | null {
  return typeof value === "string" &&
    value.trim() !== "" &&
    value.length <= maximumLength
    ? value
    : null;
}

function finiteInteger(
  value: unknown,
  minimum: number,
  maximum: number,
): number | null {
  return typeof value === "number" &&
    Number.isInteger(value) &&
    value >= minimum &&
    value <= maximum
    ? value
    : null;
}
