// h2c logger for the transport-fingerprint diagnostic.
//
// Receives the decrypted plaintext HTTP/2 stream relayed by tls_observer.py,
// logs the client's initial SETTINGS frame (as exposed by Node's http2
// remoteSettings), every request's header order, and serves the report page.

const http2 = require("node:http2");
const fs = require("node:fs");
const path = require("node:path");

const LOG_PATH = path.resolve(__dirname, "..", "runtime", "h2-observations.jsonl");
const HTML_PATH = path.resolve(__dirname, "report_page.html");
const PORT = Number(process.env.H2C_PORT || 9443);

const logStream = fs.createWriteStream(LOG_PATH, { flags: "a" });
function log(event) {
  logStream.write(JSON.stringify({ ts: new Date().toISOString(), ...event }) + "\n");
}

const html = fs.readFileSync(HTML_PATH);

const server = http2.createServer();

server.on("session", (session) => {
  const initial = session.remoteSettings;
  log({ event: "session_open", initialRemoteSettings: initial });
  session.on("remoteSettings", (settings) => {
    log({ event: "remoteSettings_update", settings });
  });
  session.on("close", () => log({ event: "session_close" }));
  session.on("error", (error) => log({ event: "session_error", error: String(error) }));
});

server.on("stream", (stream, headers) => {
  log({ event: "stream", headers });
  if (headers[":path"] === "/favicon.ico") {
    stream.respond({ ":status": 404 });
    stream.end();
    return;
  }
  stream.respond({
    ":status": 200,
    "content-type": "text/html; charset=utf-8",
    "alt-svc": 'h3=":8447"; ma=3600',
  });
  stream.end(html);
});

server.on("sessionError", (error) => log({ event: "sessionError", error: String(error) }));
server.on("error", (error) => log({ event: "server_error", error: String(error) }));

server.listen(PORT, "127.0.0.1", () => log({ event: "listening_h2c", port: PORT }));
