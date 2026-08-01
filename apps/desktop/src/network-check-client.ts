import {
  buildNetworkCheckResult,
  NETWORK_CHECK_ENDPOINTS,
  readBoundedUtf8Response,
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
      error: `${label}：${networkCheckErrorMessage(error)}`.slice(0, 300),
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
    const text = await readBoundedUtf8Response(response, RESPONSE_LIMIT_BYTES);
    return JSON.parse(text) as unknown;
  } finally {
    window.clearTimeout(timeout);
  }
}

export function networkCheckErrorMessage(error: unknown): string {
  if (error instanceof DOMException && error.name === "AbortError") {
    return "请求超时";
  }
  if (error instanceof SyntaxError) {
    return "返回内容无法识别";
  }
  return "请求失败";
}
