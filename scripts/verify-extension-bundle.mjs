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

if ((manifest.host_permissions ?? []).length > 0) {
  throw new Error("VeriSilo must not ship permanent host permissions.");
}

const background = await readFile(resolve(dist, "background.js"), "utf8");
if (!background.includes("storage.local.setAccessLevel")) {
  throw new Error(
    "VeriSilo extension must restrict storage.local to trusted contexts.",
  );
}
if (
  /https?:\/\//u.test(background) &&
  /(?:import\(|fetch\()/u.test(background)
) {
  throw new Error("Extension bundle appears to fetch or execute remote code.");
}

console.log(
  "VeriSilo extension bundle passed manifest and remote-code checks.",
);
