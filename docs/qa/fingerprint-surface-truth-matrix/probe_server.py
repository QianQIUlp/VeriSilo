# QA-only observation server for the fingerprint surface truth matrix.
# Serves header-echo and cross-realm probe pages on loopback and records every
# request's headers. It is not part of the product and never ships.
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

HERE = Path(__file__).parent
RAW = HERE / "raw"
RAW.mkdir(exist_ok=True)
HEADER_LOG = RAW / "request-headers.jsonl"

REALM_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>realm-probe</title></head>
<body>
<iframe id="f" src="/frame.html" width="10" height="10"></iframe>
<script>
function bundle(tag) {
  let gl = null;
  try {
    const c = document.createElement("canvas");
    const ctx = c.getContext("webgl");
    const ext = ctx.getExtension("WEBGL_debug_renderer_info");
    gl = { vendor: ctx.getParameter(ext.UNMASKED_VENDOR_WEBGL),
           renderer: ctx.getParameter(ext.UNMASKED_RENDERER_WEBGL),
           version: ctx.getParameter(ctx.VERSION) };
  } catch (e) { gl = { error: String(e) }; }
  return {
    realm: tag,
    userAgent: navigator.userAgent,
    platform: navigator.platform,
    oscpu: navigator.oscpu || null,
    language: navigator.language,
    languages: navigator.languages,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    utcOffsetMinutes: -new Date().getTimezoneOffset(),
    hardwareConcurrency: navigator.hardwareConcurrency,
    maxTouchPoints: navigator.maxTouchPoints,
    devicePixelRatio: window.devicePixelRatio,
    screenWidth: screen.width,
    screenHeight: screen.height,
    webdriver: navigator.webdriver,
    globalPrivacyControl: navigator.globalPrivacyControl,
    doNotTrack: navigator.doNotTrack,
    deviceMemory: navigator.deviceMemory || null,
    userAgentData: navigator.userAgentData || null,
    webgl: gl,
  };
}
window.__realms = { top: bundle("top") };
const workerCode = `self.onmessage=e=>{` +
  `let gl=null;` +
  `try{const c=new OffscreenCanvas(4,4);const x=c.getContext('webgl');` +
  `const ext=x.getExtension('WEBGL_debug_renderer_info');` +
  `gl={vendor:x.getParameter(ext.UNMASKED_VENDOR_WEBGL),renderer:x.getParameter(ext.UNMASKED_RENDERER_WEBGL)};}catch(err){gl={error:String(err)};}` +
  `self.postMessage({realm:'dedicated-worker',userAgent:navigator.userAgent,platform:navigator.platform,` +
  `language:navigator.language,languages:navigator.languages,` +
  `timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,` +
  `utcOffsetMinutes:-new Date().getTimezoneOffset(),` +
  `hardwareConcurrency:navigator.hardwareConcurrency,` +
  `webgl:gl});};`;
const blob = new Blob([workerCode], { type: "text/javascript" });
const w = new Worker(URL.createObjectURL(blob));
w.onmessage = (e) => { window.__realms.dedicatedWorker = e.data; };
w.postMessage("go");
const f = document.getElementById("f");
f.onload = () => { window.__realms.iframe = f.contentWindow.__frameBundle || null; };
</script></body></html>
"""

FRAME_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"></head><body>
<script>
window.__frameBundle = (function () {
  return {
    realm: "iframe",
    userAgent: navigator.userAgent,
    platform: navigator.platform,
    oscpu: navigator.oscpu || null,
    language: navigator.language,
    languages: navigator.languages,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    utcOffsetMinutes: -new Date().getTimezoneOffset(),
    hardwareConcurrency: navigator.hardwareConcurrency,
    maxTouchPoints: navigator.maxTouchPoints,
    devicePixelRatio: window.devicePixelRatio,
    screenWidth: screen.width,
    screenHeight: screen.height,
    webdriver: navigator.webdriver,
    globalPrivacyControl: navigator.globalPrivacyControl,
  };
})();
</script></body></html>
"""


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def _record(self):
        entry = {
            "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "method": self.command,
            "path": self.path,
            "client": self.client_address[0],
            "headers": {k.lower(): v for k, v in self.headers.items()},
        }
        with HEADER_LOG.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
        return entry

    def _send(self, body, ctype="text/html; charset=utf-8"):
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        entry = self._record()
        if self.path.startswith("/echo"):
            body = json.dumps({"seenRequestHeaders": entry["headers"]},
                              ensure_ascii=False).encode()
            self._send(body, "application/json")
        elif self.path.startswith("/realm.html"):
            self._send(REALM_PAGE.encode())
        elif self.path.startswith("/frame.html"):
            self._send(FRAME_PAGE.encode())
        else:
            self._send(b"ok", "text/plain")


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18923
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
