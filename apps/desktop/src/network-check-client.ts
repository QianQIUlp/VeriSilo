import {
  buildNetworkCheckResult,
  NETWORK_CHECK_ENDPOINTS,
  parseIpExit,
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
    probePublicIp(),
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

async function probePublicIp(): Promise<ProbeResult> {
  const urls = [
    NETWORK_CHECK_ENDPOINTS.ip,
    ...NETWORK_CHECK_ENDPOINTS.ipFallback,
  ];
  let lastError = "请求失败";
  for (const url of urls) {
    try {
      const value = await fetchBoundedJson(url);
      if (parseIpExit(value) !== null) {
        return { value, error: null };
      }
      lastError = "没有有效 IP";
    } catch (error) {
      lastError = networkCheckErrorMessage(error);
    }
  }
  return {
    value: null,
    error: `IP 出口：${lastError}`.slice(0, 300),
  };
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
