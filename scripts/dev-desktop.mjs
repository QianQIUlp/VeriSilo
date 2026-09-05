import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    port: { type: "string", default: "1420" },
    preview: { type: "boolean", default: false },
    "dry-run": { type: "boolean", default: false },
  },
});
const [name] = positionals;
if (
  positionals.length !== 1 ||
  !/^[a-z0-9][a-z0-9_-]{0,23}$/.test(name ?? "")
) {
  throw new Error(
    "Usage: pnpm desktop:worktree <name> --port 1421 [--preview]",
  );
}
const port = Number(values.port);
if (
  !/^\d+$/.test(values.port) ||
  !Number.isInteger(port) ||
  port < 1024 ||
  port > 65535
) {
  throw new Error(
    "Development port must be an integer between 1024 and 65535.",
  );
}
const cwd = resolve(dirname(fileURLToPath(import.meta.url)), "../apps/desktop");
const require = createRequire(resolve(cwd, "package.json"));
const url = `http://127.0.0.1:${port}`;
const vault = `dev-${name}`;
const config = { build: { devUrl: url } };
const cli = values.preview
  ? resolve(dirname(require.resolve("vite/package.json")), "bin/vite.js")
  : resolve(
      dirname(require.resolve("@tauri-apps/cli/package.json")),
      "tauri.js",
    );
const args = values.preview
  ? [cli, "--host", "127.0.0.1", "--port", String(port), "--strictPort"]
  : [
      cli,
      "dev",
      "--config",
      JSON.stringify(config),
      "--",
      "--",
      "--vault",
      vault,
    ];

console.log(
  JSON.stringify(
    {
      vault: values.preview ? null : vault,
      url: url + (values.preview ? "/preview.html" : ""),
      cwd,
      args,
    },
    null,
    2,
  ),
);
if (!values["dry-run"]) {
  const child = spawn(process.execPath, args, {
    cwd,
    stdio: "inherit",
    windowsHide: true,
    env: { ...process.env, VERISILO_DEV_PORT: String(port) },
  });
  child.on("error", (error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
  child.on("exit", (code) => {
    process.exitCode = code ?? 1;
  });
}
