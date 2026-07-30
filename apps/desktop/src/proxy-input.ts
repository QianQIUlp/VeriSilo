import type {
  NetworkProfile,
  ProxyCredentialsInput,
} from "@verisilo/contracts";

const SUPPORTED_SCHEMES = new Set(["http", "https", "socks4", "socks5"]);
const HOST_PATTERN = /^[A-Za-z0-9.:-]+$/u;

export interface ParsedProxyInput {
  profile: Extract<NetworkProfile, { mode: "fixed_proxy" }>;
  credentials: ProxyCredentialsInput | null;
}

export function parseProxyInput(rawInput: string): ParsedProxyInput {
  const input = rawInput.trim();
  if (input === "" || input.length > 4_096 || /[\r\n\0]/u.test(input)) {
    throw new Error("请输入一行有效的代理地址。");
  }

  if (input.includes("://")) {
    return parseProxyUrl(input);
  }
  return parseColonProxy(input);
}

function parseProxyUrl(input: string): ParsedProxyInput {
  let url: URL;
  try {
    url = new URL(input);
  } catch {
    throw new Error("代理 URL 无法解析。");
  }
  const scheme = normalizeScheme(url.protocol.slice(0, -1));
  if (
    url.hostname === "" ||
    url.port === "" ||
    (url.pathname !== "" && url.pathname !== "/") ||
    url.search !== "" ||
    url.hash !== ""
  ) {
    throw new Error("代理 URL 只能包含协议、认证、主机和端口。");
  }
  const host = normalizeHost(url.hostname);
  const port = parsePort(url.port);
  const username = decodeCredential(url.username);
  const password = decodeCredential(url.password);
  return buildParsedProxy(scheme, host, port, username, password);
}

function parseColonProxy(input: string): ParsedProxyInput {
  const parts = input.split(":");
  if (parts.length !== 2 && parts.length < 4) {
    throw new Error(
      "请使用 host:port、host:port:username:password 或标准代理 URL。",
    );
  }
  const [rawHost, rawPort, rawUsername, ...passwordParts] = parts;
  if (rawHost === undefined || rawPort === undefined) {
    throw new Error("代理地址缺少主机或端口。");
  }
  return buildParsedProxy(
    "http",
    normalizeHost(rawHost),
    parsePort(rawPort),
    rawUsername ?? "",
    passwordParts.join(":"),
  );
}

function buildParsedProxy(
  scheme: Extract<NetworkProfile, { mode: "fixed_proxy" }>["scheme"],
  host: string,
  port: number,
  username: string,
  password: string,
): ParsedProxyInput {
  if (/\p{C}/u.test(username) || /\p{C}/u.test(password)) {
    throw new Error("代理认证信息不能包含控制字符。");
  }
  if (username.length > 512 || password.length > 1_024) {
    throw new Error("代理认证信息过长。");
  }
  if ((username === "") !== (password === "")) {
    throw new Error("用户名和密码需要同时填写；无认证代理请都留空。");
  }
  return {
    profile: {
      mode: "fixed_proxy",
      proxyRequired: true,
      scheme,
      host,
      port,
      bypassList: [],
    },
    credentials: username === "" ? null : { username, password },
  };
}

function normalizeScheme(
  scheme: string,
): Extract<NetworkProfile, { mode: "fixed_proxy" }>["scheme"] {
  const normalized = scheme.toLowerCase();
  if (!SUPPORTED_SCHEMES.has(normalized)) {
    throw new Error("仅支持 HTTP、HTTPS、SOCKS4 和 SOCKS5 代理。");
  }
  return normalized as Extract<
    NetworkProfile,
    { mode: "fixed_proxy" }
  >["scheme"];
}

function normalizeHost(host: string): string {
  const normalized = host.trim().replace(/^\[|\]$/gu, "");
  if (
    normalized === "" ||
    normalized.length > 253 ||
    !HOST_PATTERN.test(normalized)
  ) {
    throw new Error("代理主机格式无效。");
  }
  return normalized;
}

function parsePort(value: string): number {
  const port = Number(value);
  if (
    !/^\d{1,5}$/u.test(value) ||
    !Number.isInteger(port) ||
    port < 1 ||
    port > 65_535
  ) {
    throw new Error("代理端口必须在 1 到 65535 之间。");
  }
  return port;
}

function decodeCredential(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    throw new Error("代理认证信息包含无效的 URL 编码。");
  }
}
