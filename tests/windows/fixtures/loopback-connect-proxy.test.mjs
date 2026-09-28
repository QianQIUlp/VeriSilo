import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { createServer as createHttpServer } from "node:http";
import { connect, createServer as createNetServer } from "node:net";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";

async function freePort() {
  const server = createNetServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function tunnel(port, target, request) {
  return new Promise((resolve, reject) => {
    const socket = connect(port, "127.0.0.1");
    let response = "";
    let sent = false;
    socket.setTimeout(3000, () => socket.destroy(new Error("proxy fixture timed out")));
    socket.on("error", reject);
    socket.on("data", (chunk) => {
      response += chunk.toString();
      if (!sent && response.includes("\r\n\r\n")) {
        sent = true;
        if (request && response.startsWith("HTTP/1.1 200")) socket.write(request);
        else socket.end();
      }
      if (response.includes("fixture-ok") || response.startsWith("HTTP/1.1 403")) {
        socket.destroy();
        resolve(response);
      }
    });
    socket.on("connect", () => socket.write(`CONNECT ${target} HTTP/1.1\r\nHost: ${target}\r\n\r\n`));
  });
}

test("loopback CONNECT proxy maps only the reserved test address and records route tokens", async () => {
  const site = createHttpServer((_request, response) => response.end("fixture-ok"));
  await new Promise((resolve) => site.listen(0, "127.0.0.1", resolve));
  after(() => site.close());
  const sitePort = site.address().port;
  const proxyPort = await freePort();
  const token = randomBytes(32).toString("hex");
  const proxy = spawn(process.execPath, [fileURLToPath(new URL("./loopback-connect-proxy.mjs", import.meta.url)),
    "--port", String(proxyPort), "--site-port", String(sitePort), "--token", token,
    "--allow-exit-observation"],
  { stdio: "ignore", windowsHide: true });
  after(() => proxy.kill());
  const control = `http://127.0.0.1:${proxyPort}`;
  let ready = false;
  for (let attempt = 0; attempt < 40; attempt++) {
    try {
      ready = (await fetch(`${control}/__health?harnessToken=${token}`)).ok;
      if (ready) break;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.ok(ready, "proxy fixture did not start");
  const operationToken = randomBytes(16).toString("hex");
  const target = `198.51.100.9:${sitePort}`;
  assert.match(await tunnel(proxyPort, target,
    `GET /?operationToken=${operationToken} HTTP/1.1\r\nHost: ${target}\r\nConnection: close\r\n\r\n`),
  /fixture-ok/);
  assert.match(await tunnel(proxyPort, "example.com:80"), /^HTTP\/1\.1 403/);
  const allowed = async (target) => (await (await fetch(
    `${control}/__allowed?harnessToken=${token}&target=${encodeURIComponent(target)}`)).json()).allowed;
  for (const observation of ["ipwho.is:443", "api.ipify.org:443", "api.ip.sb:443"])
    assert.equal(await allowed(observation), true, `${observation} is an existing Host check`);
  for (const forbidden of ["api.ipify.org:80", "api.ip.sb:80", "example.com:443"])
    assert.equal(await allowed(forbidden), false, `${forbidden} must remain blocked`);
  const events = await (await fetch(`${control}/__events?harnessToken=${token}`)).json();
  assert.deepEqual(events, [{ target, operationTokens: [operationToken] }]);
});
