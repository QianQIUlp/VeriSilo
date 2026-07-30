import { createReadStream } from "node:fs";
import { createServer } from "node:http";
import { resolve } from "node:path";

const port = Number(process.env.PORT ?? 4173);
const fixture = resolve(import.meta.dirname, "index.html");

if (!Number.isInteger(port) || port < 0 || port > 65_535) {
  throw new Error("PORT must be an integer between 0 and 65535.");
}

const serviceWorker = `self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
`;

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://127.0.0.1:${port}`);

  if (url.pathname === "/health.json") {
    response
      .writeHead(200, {
        "cache-control": "no-store",
        "content-type": "application/json; charset=utf-8",
      })
      .end(
        `${JSON.stringify({
          fixture: "verisilo-session-site",
          schemaVersion: 1,
        })}\n`,
      );
    return;
  }

  if (url.pathname === "/service-worker.js") {
    response
      .writeHead(200, {
        "cache-control": "no-store",
        "content-type": "text/javascript; charset=utf-8",
        "service-worker-allowed": "/",
      })
      .end(serviceWorker);
    return;
  }

  if (url.pathname !== "/" && url.pathname !== "/index.html") {
    response.writeHead(404).end("Not found");
    return;
  }
  response.writeHead(200, {
    "cache-control": "no-store",
    "content-type": "text/html; charset=utf-8",
  });
  createReadStream(fixture).pipe(response);
});

server.listen(port, "127.0.0.1", () => {
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("The session fixture did not receive a TCP address.");
  }
  console.log(`VeriSilo session fixture: http://127.0.0.1:${address.port}`);
});

server.once("error", (error) => {
  console.error(`VeriSilo session fixture failed: ${error.message}`);
  process.exitCode = 1;
});
