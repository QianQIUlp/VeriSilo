import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const dist = resolve(root, "apps/extension/dist");
const manifest = JSON.parse(
  await readFile(resolve(dist, "manifest.json"), "utf8"),
);

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
