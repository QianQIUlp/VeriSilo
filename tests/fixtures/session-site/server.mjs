import { createReadStream } from "node:fs";
import { createServer } from "node:http";
import { resolve } from "node:path";

const port = Number(process.env.PORT ?? 4173);
const fixture = resolve(import.meta.dirname, "index.html");

createServer((request, response) => {
  if (request.url !== "/" && request.url !== "/index.html") {
    response.writeHead(404).end("Not found");
    return;
  }
  response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  createReadStream(fixture).pipe(response);
}).listen(port, "127.0.0.1", () => {
  console.log(`VeriSilo session fixture: http://127.0.0.1:${port}`);
});
