import {
  buildNetworkCheckResult,
  NETWORK_CHECK_ENDPOINTS,
  type NetworkCheckResult,
} from "@verisilo/contracts";

const RESPONSE_LIMIT_BYTES = 64 * 1_024;
const REQUEST_TIMEOUT_MS = 10_000;

interface ProbeResult {
  value: unknown | null;
  error: string | null;
}

export async function runDesktopNetworkCheck(): Promise<NetworkCheckResult> {
  const [ipProbe, cloudflareProbe, googleProbe] = await Promise.all([
    probeJson("IP 出口", NETWORK_CHECK_ENDPOINTS.ip),
    probeJson("Cloudflare DNS", NETWORK_CHECK_ENDPOINTS.cloudflareDns, {
      Accept: "application/dns-json",
    }),
    probeJson("Google DNS", NETWORK_CHECK_ENDPOINTS.googleDns),
  ]);

  return buildNetworkCheckResult({
    ipPayload: ipProbe.value,
    cloudflareDnsPayload: cloudflareProbe.value,
    googleDnsPayload: googleProbe.value,
    errors: [ipProbe.error, cloudflareProbe.error, googleProbe.error].filter(
      (error): error is string => error !== null,
    ),
  });
}

async function probeJson(
  label: string,
  url: string,
  headers?: Record<string, string>,
): Promise<ProbeResult> {
  try {
    return { value: await fetchBoundedJson(url, headers), error: null };
  } catch (error) {
    return {
      value: null,
      error: `${label}：${errorMessage(error)}`.slice(0, 300),
    };
  }
}

async function fetchBoundedJson(
  url: string,
  headers?: Record<string, string>,
): Promise<unknown> {
  const controller = new AbortController();
  const timeout = window.setTimeout(
    () => controller.abort(),
    REQUEST_TIMEOUT_MS,
  );
  try {
    const request: RequestInit = {
      method: "GET",
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
      referrerPolicy: "no-referrer",
      signal: controller.signal,
    };
    if (headers !== undefined) {
      request.headers = headers;
    }
    const response = await fetch(url, request);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const text = await response.text();
    if (new TextEncoder().encode(text).byteLength > RESPONSE_LIMIT_BYTES) {
      throw new Error("响应超过 64 KiB");
    }
    return JSON.parse(text) as unknown;
  } finally {
    window.clearTimeout(timeout);
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof DOMException && error.name === "AbortError") {
    return "请求超时";
  }
  return error instanceof Error ? error.message : String(error);
}
