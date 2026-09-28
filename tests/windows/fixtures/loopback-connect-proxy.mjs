// Controlled HTTP CONNECT upstream for native Managed concurrency acceptance.
// Bind to loopback; only the local test site and the Managed Host's fixed
// exit-observation endpoints can leave this process.
import { createServer } from "node:http";
import { connect } from "node:net";

function argument(name) {
  const index = process.argv.indexOf(name);
  return index < 0 ? undefined : process.argv[index + 1];
}

const port = Number(argument("--port"));
const sitePort = Number(argument("--site-port"));
const token = argument("--token");
const allowExitObservation = process.argv.includes("--allow-exit-observation");
const exitObservationTargets = new Set(["ipwho.is:443", "api.ipify.org:443", "api.ip.sb:443"]);
if (![port, sitePort].every((value) => Number.isInteger(value) && value > 0 && value <= 65535) ||
    !/^[0-9a-f]{64}$/.test(token ?? "")) {
  throw new Error("Usage: node loopback-connect-proxy.mjs --port <port> --site-port <port> --token <64 hex> [--allow-exit-observation]");
}

function destination(target) {
  if (target === `198.51.100.9:${sitePort}`) return { host: "127.0.0.1", port: sitePort, site: true };
  if (allowExitObservation && exitObservationTargets.has(target))
    return { host: target.slice(0, -4), port: 443, site: false };
  return null;
}

const events = [];
const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://127.0.0.1:${port}`);
  if (url.searchParams.get("harnessToken") !== token) {
    response.writeHead(403).end();
    return;
  }
  if (request.method === "GET" && url.pathname === "/__health") {
    response.writeHead(200, { "content-type": "application/json" })
      .end(`${JSON.stringify({ schema: "urn:verisilo:loopback-connect-proxy:1", sitePort })}\n`);
  } else if (request.method === "GET" && url.pathname === "/__events") {
    response.writeHead(200, { "content-type": "application/json" }).end(`${JSON.stringify(events)}\n`);
  } else if (request.method === "GET" && url.pathname === "/__allowed") {
    response.writeHead(200, { "content-type": "application/json" })
      .end(`${JSON.stringify({ allowed: destination(url.searchParams.get("target")) !== null })}\n`);
  } else if (request.method === "POST" && url.pathname === "/__reset") {
    events.length = 0;
    response.writeHead(204).end();
  } else {
    response.writeHead(404).end();
  }
});

server.on("connect", (request, client, head) => {
  const target = request.url ?? "";
  // A reserved test address prevents Firefox's implicit localhost proxy bypass.
  // The proxy maps it to the loopback site; no packet goes to that address.
  const route = destination(target);
  if (!route) {
    client.end("HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n");
    return;
  }
  const event = { target, operationTokens: [] };
  events.push(event);
  const upstream = connect(route.port, route.host);
  let established = false;
  upstream.once("connect", () => {
    established = true;
    client.write("HTTP/1.1 200 Connection Established\r\n\r\n");
    let tail = "";
    const seen = new Set();
    const observe = (chunk) => {
      const content = tail + chunk.toString("latin1");
      for (const match of content.matchAll(/operationToken=([0-9a-f]{32})/g)) {
        if (!seen.has(match[1])) {
          seen.add(match[1]);
          event.operationTokens.push(match[1]);
        }
      }
      tail = content.slice(-64);
    };
    if (route.site) {
      if (head.length) observe(head);
      client.on("data", observe);
    }
    if (head.length) upstream.write(head);
    client.pipe(upstream).pipe(client);
  });
  upstream.once("error", () => {
    if (!established) client.write("HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n");
    client.destroy();
  });
  client.once("error", () => upstream.destroy());
  client.once("close", () => upstream.destroy());
  upstream.once("close", () => client.destroy());
});

server.listen(port, "127.0.0.1", () => {
  process.stdout.write(`VeriSilo loopback CONNECT proxy listening on 127.0.0.1:${port}\n`);
});
