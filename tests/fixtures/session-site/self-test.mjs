import assert from "node:assert/strict";
import { once } from "node:events";
import { spawn } from "node:child_process";
import { resolve } from "node:path";

const serverPath = resolve(import.meta.dirname, "server.mjs");
const child = spawn(process.execPath, [serverPath], {
  env: { ...process.env, PORT: "0" },
  stdio: ["ignore", "pipe", "pipe"],
});

let stderr = "";
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

try {
  const origin = await fixtureOrigin(child);

  const healthResponse = await fetch(`${origin}/health.json`);
  assert.equal(healthResponse.status, 200);
  assert.deepEqual(await healthResponse.json(), {
    fixture: "verisilo-session-site",
    schemaVersion: 1,
  });

  const pageResponse = await fetch(`${origin}/index.html?labs-navigation=1`);
  assert.equal(pageResponse.status, 200);
  assert.match(pageResponse.headers.get("content-type") ?? "", /^text\/html;/u);
  assert.match(await pageResponse.text(), /Save all browser state/u);

  const workerResponse = await fetch(`${origin}/service-worker.js`);
  assert.equal(workerResponse.status, 200);
  assert.match(
    workerResponse.headers.get("content-type") ?? "",
    /^text\/javascript;/u,
  );
  assert.equal(workerResponse.headers.get("service-worker-allowed"), "/");
  assert.match(await workerResponse.text(), /self\.clients\.claim/u);

  const missingResponse = await fetch(`${origin}/missing`);
  assert.equal(missingResponse.status, 404);
} finally {
  if (child.exitCode === null) {
    child.kill();
    await once(child, "exit");
  }
}

async function fixtureOrigin(process) {
  process.stdout.setEncoding("utf8");
  return new Promise((resolveOrigin, reject) => {
    let output = "";
    const timer = setTimeout(() => {
      cleanup();
      reject(new Error(`Session fixture startup timed out: ${stderr}`));
    }, 10_000);
    const onData = (chunk) => {
      output += chunk;
      const match = output.match(
        /VeriSilo session fixture: (http:\/\/127\.0\.0\.1:\d+)/u,
      );
      if (match?.[1] !== undefined) {
        cleanup();
        resolveOrigin(match[1]);
      }
    };
    const onExit = (code) => {
      cleanup();
      reject(
        new Error(
          `Session fixture exited before startup (${String(code)}): ${stderr}`,
        ),
      );
    };
    const cleanup = () => {
      clearTimeout(timer);
      process.stdout.off("data", onData);
      process.off("exit", onExit);
    };
    process.stdout.on("data", onData);
    process.on("exit", onExit);
  });
}
