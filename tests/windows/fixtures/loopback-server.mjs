import { createServer } from "node:http";

function readArgument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

const host = readArgument("--host") ?? "127.0.0.1";
const port = Number(readArgument("--port") ?? process.env.PORT ?? 4173);

if (host !== "127.0.0.1") {
  throw new Error("The Windows E2E fixture may bind only to 127.0.0.1.");
}
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error("--port must be a TCP port between 1 and 65535.");
}

const events = [];

function page() {
  return `<!doctype html>
<meta charset="utf-8">
<title>VERISILO_E2E:LOADING</title>
<pre id="result">loading</pre>
<script type="module">
  const params = new URL(location.href).searchParams;
  const operation = params.get("op");
  const value = params.get("value") ?? "";
  const expected = params.get("expected") ?? "";
  const result = document.querySelector("#result");

  function finish(ok, details) {
    const prefix = ok ? "PASS" : "FAIL";
    document.title = "VERISILO_E2E:" + prefix + ":" + details;
    result.textContent = document.title;
  }

  function cookies() {
    return Object.fromEntries(
      document.cookie.split("; ").filter(Boolean).map((item) => {
        const separator = item.indexOf("=");
        return [item.slice(0, separator), decodeURIComponent(item.slice(separator + 1))];
      }),
    );
  }

  function indexedDbValue() {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open("verisilo-windows-e2e", 1);
      request.onupgradeneeded = () => request.result.createObjectStore("values");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const database = request.result;
        const transaction = database.transaction("values", "readonly");
        const get = transaction.objectStore("values").get("marker");
        get.onerror = () => reject(get.error);
        get.onsuccess = () => resolve(get.result ?? "");
        transaction.oncomplete = () => database.close();
      };
    });
  }

  function writeIndexedDb(marker) {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open("verisilo-windows-e2e", 1);
      request.onupgradeneeded = () => request.result.createObjectStore("values");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const database = request.result;
        const transaction = database.transaction("values", "readwrite");
        transaction.objectStore("values").put(marker, "marker");
        transaction.onerror = () => reject(transaction.error);
        transaction.oncomplete = () => {
          database.close();
          resolve();
        };
      };
    });
  }

  try {
    if (operation === "write") {
      localStorage.setItem("marker", value);
      sessionStorage.setItem("marker", value);
      document.cookie = "verisilo_e2e=" + encodeURIComponent(value) + "; Path=/; SameSite=Lax";
      await writeIndexedDb(value);
      finish(true, "written");
    } else if (operation === "read") {
      const actual = [
        localStorage.getItem("marker") ?? "",
        sessionStorage.getItem("marker") ?? "",
        cookies().verisilo_e2e ?? "",
        await indexedDbValue(),
      ];
      finish(actual.every((item) => item === expected), actual.join(","));
    } else {
      finish(false, "unknown-operation");
    }
  } catch (error) {
    finish(false, "exception-" + String(error));
  }
</script>`;
}

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://${host}:${port}`);
  if (url.pathname === "/__reset" && request.method === "POST") {
    events.length = 0;
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/__events") {
    response.writeHead(200, {
      "content-type": "application/json; charset=utf-8",
    });
    response.end(`${JSON.stringify(events)}\n`);
    return;
  }
  if (url.pathname !== "/") {
    response.writeHead(404).end("Not found");
    return;
  }

  events.push({ method: request.method, path: `${url.pathname}${url.search}` });
  response.writeHead(200, {
    "cache-control": "no-store",
    "content-type": "text/html; charset=utf-8",
  });
  response.end(page());
});

server.listen(port, host, () => {
  process.stdout.write(
    `VeriSilo Windows E2E fixture: http://${host}:${port}\n`,
  );
});

server.once("error", (error) => {
  process.stderr.write(
    `VeriSilo Windows E2E fixture could not bind ${host}:${port}: ${error.message}\n`,
  );
  process.exitCode = 1;
});
