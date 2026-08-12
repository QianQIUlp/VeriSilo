import { readFile, readdir } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const dist = resolve(root, "apps/extension/dist");
const manifest = JSON.parse(
  await readFile(resolve(dist, "manifest.json"), "utf8"),
);
const extensionPackage = JSON.parse(
  await readFile(resolve(root, "apps/extension/package.json"), "utf8"),
);
if (
  manifest.version !== "0.2.7" ||
  extensionPackage.version !== manifest.version
) {
  throw new Error(
    "Extension package and bundled manifest must use the current aligned version.",
  );
}
if (!manifest.description?.includes("查看当前网页可读取的浏览器信息")) {
  throw new Error(
    "Extension manifest must describe local observation without an isolation claim.",
  );
}

const expectedIcons = Object.fromEntries(
  [16, 32, 48, 128].map((size) => [String(size), `icons/verisilo-${size}.png`]),
);
if (
  JSON.stringify(manifest.icons) !== JSON.stringify(expectedIcons) ||
  JSON.stringify(manifest.action?.default_icon) !==
    JSON.stringify(expectedIcons)
) {
  throw new Error(
    "Extension manifest must use the website brand icon at 16/32/48/128px.",
  );
}
for (const [sizeText, relativePath] of Object.entries(expectedIcons)) {
  const size = Number(sizeText);
  const png = await readFile(resolve(dist, relativePath));
  if (
    !png
      .subarray(0, 8)
      .equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) ||
    png.readUInt32BE(16) !== size ||
    png.readUInt32BE(20) !== size
  ) {
    throw new Error(
      `Extension icon is not a ${size}x${size} PNG: ${relativePath}`,
    );
  }
}

const sidepanelHtml = await readFile(resolve(dist, "sidepanel.html"), "utf8");
const bundledFiles = await readdir(dist, { recursive: true });
if (bundledFiles.some((file) => file.endsWith(".map"))) {
  throw new Error("Extension release/test bundle must not ship source maps.");
}
const isolationPanel = sidepanelHtml.match(
  /<section[^>]+id="panel-isolation"[\s\S]*?<\/section>/u,
)?.[0];
if (isolationPanel === undefined) {
  throw new Error("Extension bundle is missing the isolation panel.");
}
if (/V0\.\d/u.test(isolationPanel) || /正式路线/u.test(isolationPanel)) {
  throw new Error(
    "Isolation guidance must describe current desktop capabilities without roadmap versions.",
  );
}
for (const requiredGuidance of [
  "高级诊断工具 · 默认关闭",
  "不含账号或个人信息",
  "普通浏览和多账号隔离不需要开启",
  "只有点击开启才会改变当前网页",
  "当前扩展不支持",
  "结果只代表当前页面",
  "信号概览",
  "每个桌面身份使用独立浏览器资料目录",
  "设备与浏览器特征继续跟随本机",
  "不提供指纹控制",
  "最近实验记录",
]) {
  if (!sidepanelHtml.includes(requiredGuidance)) {
    throw new Error(`Extension Labs guidance is missing: ${requiredGuidance}`);
  }
}
for (const staleLabel of [
  "不可选 · unsupported",
  "V0.7 路线",
  "看懂并隔离你的浏览器身份",
  "桌面端 · 专用引擎",
  "桌面端 · 虚拟/远程环境",
  "same_origin_blob_classic_only",
  "late_or_unknown",
  "extension observation",
  "user-data-dir",
  "Native Host",
]) {
  if (sidepanelHtml.includes(staleLabel)) {
    throw new Error(
      `Extension bundle still exposes stale UI copy: ${staleLabel}`,
    );
  }
}
if (
  !/\.labs-warning\s*\{[^}]*overflow:\s*hidden;[^}]*padding:\s*16px;/u.test(
    sidepanelHtml,
  )
) {
  throw new Error(
    "Labs introduction card is missing its bounded inner spacing.",
  );
}

const requiredPermissions = [
  "storage",
  "sidePanel",
  "activeTab",
  "nativeMessaging",
  "scripting",
];
for (const permission of requiredPermissions) {
  if (!manifest.permissions?.includes(permission)) {
    throw new Error(
      `Extension manifest is missing required permission: ${permission}`,
    );
  }
}

for (const permission of ["proxy", "declarativeNetRequest"]) {
  if (manifest.optional_permissions?.includes(permission)) {
    throw new Error(
      `Edge does not allow ${permission} to be declared as an optional permission.`,
    );
  }
}

if ((manifest.host_permissions ?? []).length > 0) {
  throw new Error("VeriSilo must not ship permanent host permissions.");
}

for (const pattern of [
  "https://ipwho.is/*",
  "https://cloudflare-dns.com/*",
  "https://dns.google/*",
]) {
  if (
    !manifest.optional_host_permissions?.includes(pattern) &&
    !manifest.optional_host_permissions?.includes("https://*/*")
  ) {
    throw new Error(
      `Extension manifest cannot request the audited network host: ${pattern}`,
    );
  }
}

const background = await readFile(resolve(dist, "background.js"), "utf8");
const storeDisclosure = await readFile(
  resolve(root, "docs/store-disclosure.md"),
  "utf8",
);
if (!background.includes("storage.local.setAccessLevel")) {
  throw new Error(
    "VeriSilo extension must restrict storage.local to trusted contexts.",
  );
}
if (
  background.includes("import(") ||
  /\beval\(|new Function\(/u.test(background)
) {
  throw new Error("Extension bundle appears to execute dynamic remote code.");
}

const allowedNetworkUrls = new Set([
  "https://ipwho.is/",
  "https://ipwho.is/*",
  "https://cloudflare-dns.com/dns-query?name=example.com&type=A&do=true",
  "https://cloudflare-dns.com/*",
  "https://dns.google/resolve?name=example.com&type=A&do=true&edns_client_subnet=0.0.0.0%2F0",
  "https://dns.google/*",
]);
const bundledNetworkUrls = new Set(
  background.match(/https?:\/\/[^\s"'`]+/gu) ?? [],
);
for (const url of bundledNetworkUrls) {
  if (!allowedNetworkUrls.has(url)) {
    throw new Error(
      `Extension bundle contains an unapproved network URL: ${url}`,
    );
  }
}
for (const url of allowedNetworkUrls) {
  if (!bundledNetworkUrls.has(url)) {
    throw new Error(
      `Extension bundle is missing an audited network URL: ${url}`,
    );
  }
}

for (const requiredDisclosure of [
  "chrome.storage.session",
  "chrome.storage.local",
  "Native Messaging",
  "encrypted Vault",
  "extension observation",
  "does not transmit browsing activity",
]) {
  if (!storeDisclosure.includes(requiredDisclosure)) {
    throw new Error(
      `Store disclosure is missing the current local data-flow boundary: ${requiredDisclosure}`,
    );
  }
}

console.log(
  "VeriSilo extension bundle passed manifest and remote-code checks.",
);
