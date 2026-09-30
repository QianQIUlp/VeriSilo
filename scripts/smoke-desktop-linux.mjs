import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  rmSync,
  mkdirSync,
  writeFileSync,
  existsSync,
  lstatSync,
  readlinkSync,
  readFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { parseArgs } from "node:util";
import { setTimeout as delay } from "node:timers/promises";
import { Worker } from "node:worker_threads";

const { values } = parseArgs({
  options: {
    desktop: { type: "string" },
    cli: { type: "string" },
    browser: { type: "string", default: "/usr/bin/google-chrome" },
    managed: { type: "boolean", default: false },
  },
});
assert.equal(process.platform, "linux", "This smoke must run on native Linux.");
assert(values.desktop && values.cli, "--desktop and --cli are required.");
const root = mkdtempSync(join(tmpdir(), "verisilo-linux-smoke-"));
const env = { ...process.env, XDG_DATA_HOME: root };
const password = "linux smoke isolated vault passphrase";
const backupPassword = "linux smoke isolated backup passphrase";
let desktop;
let desktopLog = "";
let fixture;
let standard;
const evidence = {
  platform: "linux-x64",
  standard: false,
  managed: false,
  passed: false,
};

function cli(args, input, allowFailure = false) {
  const result = spawnSync(
    resolve(values.cli),
    ["--vault", "linux-smoke", "--json", ...args],
    {
      env,
      input,
      encoding: "utf8",
      timeout: 180_000,
    },
  );
  if (allowFailure && result.status !== 0) return null;
  assert.equal(
    result.status,
    0,
    `${args.join(" ")}: ${result.stderr || result.error || desktopLog}`,
  );
  return result.stdout.trim() ? JSON.parse(result.stdout) : null;
}

async function waitFor(check, label) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(100);
  }
  throw new Error(`${label} timed out. Desktop log: ${desktopLog}`);
}

async function startDesktop() {
  desktop = spawn(
    resolve(values.desktop),
    ["--vault", "linux-smoke", "--cli-background"],
    {
      env,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  desktop.on("error", (error) => {
    desktopLog += `${error}\n`;
  });
  for (const stream of [desktop.stdout, desktop.stderr]) {
    stream.on("data", (data) => {
      desktopLog = (desktopLog + data).slice(-16_384);
    });
  }
  const discoveryPath = join(
    root,
    "VeriSilo/vaults/linux-smoke/local-api.json",
  );
  await waitFor(async () => {
    if (!existsSync(discoveryPath)) return false;
    const discovery = JSON.parse(readFileSync(discoveryPath, "utf8"));
    try {
      const response = await fetch(new URL("v1/health", discovery.url), {
        signal: AbortSignal.timeout(500),
      });
      return response.ok;
    } catch {
      return false;
    }
  }, "desktop API startup");
}

async function stopDesktop() {
  cli(["service", "stop"]);
  await waitFor(() => desktop.exitCode !== null, "desktop shutdown");
}

async function closeStandardBrowser(silo) {
  // RC5 Standard browsers are closed by the user, not the Managed stop API.
  // Close only real windows belonging to this isolated smoke's browser PID.
  assert(resolve(silo.profileDirectory).startsWith(`${root}${sep}`));
  const record = JSON.parse(
    readFileSync(
      join(root, "VeriSilo/vaults/linux-smoke/runtime/browser-session.json"),
      "utf8",
    ),
  );
  assert.equal(record.siloId, silo.id);
  assert(Number.isSafeInteger(record.pid) && record.pid > 0);
  // Chromium's setproctitle joins argv with spaces on Linux. The generated
  // fixture Profile has no whitespace, so either representation is unambiguous.
  assert(!/\s/u.test(silo.profileDirectory));
  const args = readFileSync(`/proc/${record.pid}/cmdline`, "utf8").split(
    /[\0\s]+/u,
  );
  assert(
    args.includes(`--user-data-dir=${resolve(silo.profileDirectory)}`),
    `Recorded PID ${record.pid} is not this fixture browser: ${JSON.stringify(args)}`,
  );
  const closeWindow = String.raw`
import ctypes as C, json, sys
pid = int(sys.argv[1])
x = C.CDLL("libX11.so.6")
window = C.c_ulong
x.XOpenDisplay.restype = C.c_void_p
x.XOpenDisplay.argtypes = [C.c_char_p]
d = x.XOpenDisplay(None)
if not d:
    raise RuntimeError("X11 display is unavailable")
signatures = {
    "XDefaultRootWindow": (window, [C.c_void_p]),
    "XInternAtom": (window, [C.c_void_p, C.c_char_p, C.c_int]),
    "XQueryTree": (C.c_int, [C.c_void_p, window, C.POINTER(window), C.POINTER(window), C.POINTER(C.POINTER(window)), C.POINTER(C.c_uint)]),
    "XGetWindowProperty": (C.c_int, [C.c_void_p, window, window, C.c_long, C.c_long, C.c_int, window, C.POINTER(window), C.POINTER(C.c_int), C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ubyte))]),
    "XGetWMProtocols": (C.c_int, [C.c_void_p, window, C.POINTER(C.POINTER(window)), C.POINTER(C.c_int)]),
    "XFree": (C.c_int, [C.c_void_p]),
    "XSendEvent": (C.c_int, [C.c_void_p, window, C.c_int, C.c_long, C.c_void_p]),
    "XFlush": (C.c_int, [C.c_void_p]),
    "XCloseDisplay": (C.c_int, [C.c_void_p]),
}
for name, (result, arguments) in signatures.items():
    getattr(x, name).restype = result
    getattr(x, name).argtypes = arguments
class Data(C.Union):
    _fields_ = [("l", C.c_long * 5)]
class Message(C.Structure):
    _fields_ = [("type", C.c_int), ("serial", C.c_ulong), ("send_event", C.c_int), ("display", C.c_void_p), ("window", window), ("message_type", window), ("format", C.c_int), ("data", Data)]
class Event(C.Union):
    _fields_ = [("message", Message), ("pad", C.c_long * 24)]
pid_atom = x.XInternAtom(d, b"_NET_WM_PID", False)
delete_atom = x.XInternAtom(d, b"WM_DELETE_WINDOW", False)
protocol_atom = x.XInternAtom(d, b"WM_PROTOCOLS", False)
root, parent, children, count = window(), window(), C.POINTER(window)(), C.c_uint()
closed = []
try:
    if not x.XQueryTree(d, x.XDefaultRootWindow(d), C.byref(root), C.byref(parent), C.byref(children), C.byref(count)):
        raise RuntimeError("Cannot enumerate Xvfb top-level windows")
    for index in range(count.value):
        w = children[index]
        kind, fmt, items, remaining, value = window(), C.c_int(), C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ubyte)()
        x.XGetWindowProperty(d, w, pid_atom, 0, 1, False, 6, C.byref(kind), C.byref(fmt), C.byref(items), C.byref(remaining), C.byref(value))
        matches = kind.value == 6 and fmt.value == 32 and items.value == 1 and value and C.cast(value, C.POINTER(C.c_ulong))[0] == pid
        if value:
            x.XFree(value)
        if not matches:
            continue
        protocols, length = C.POINTER(window)(), C.c_int()
        supports_close = x.XGetWMProtocols(d, w, C.byref(protocols), C.byref(length)) and delete_atom in protocols[:length.value]
        if protocols:
            x.XFree(protocols)
        if supports_close:
            event = Event()
            event.message = Message(33, 0, True, d, w, protocol_atom, 32, Data())
            event.message.data.l[0] = delete_atom
            if not x.XSendEvent(d, w, False, 0, C.byref(event)):
                raise RuntimeError("WM_DELETE_WINDOW send failed")
            closed.append(w)
    x.XFlush(d)
finally:
    if children:
        x.XFree(children)
    x.XCloseDisplay(d)
print(json.dumps({"pid": pid, "windows": closed}))
`;
  await waitFor(() => {
    const result = spawnSync("python3", ["-c", closeWindow, String(record.pid)], {
      encoding: "utf8",
      timeout: 5_000,
    });
    assert.equal(result.status, 0, result.stderr || String(result.error));
    const closed = JSON.parse(result.stdout);
    assert.equal(closed.pid, record.pid);
    if (!closed.windows.length) return false;
    evidence.standardClose = closed;
    return true;
  }, "Standard browser's own X11 window");
}

function standardDiagnostics(silo) {
  const read = (path, limit = 4096) => {
    try {
      return readFileSync(path).subarray(0, limit).toString("utf8");
    } catch (error) {
      return { error: error.code };
    }
  };
  const record = read(
    join(root, "VeriSilo/vaults/linux-smoke/runtime/browser-session.json"),
    16_384,
  );
  let pid = null;
  try {
    const parsed = JSON.parse(record);
    if (Number.isSafeInteger(parsed.pid) && parsed.pid > 0) pid = parsed.pid;
  } catch {
    /* Keep partial or unreadable records in the diagnostic. */
  }
  return {
    lastStatus: evidence.standardClose?.lastStatus,
    record,
    process: pid
      ? {
          pid,
          status: read(`/proc/${pid}/status`),
          cmdline: read(`/proc/${pid}/cmdline`),
          wchan: read(`/proc/${pid}/wchan`, 128),
          children: read(`/proc/${pid}/task/${pid}/children`, 2048),
        }
      : null,
    sentinels: Object.fromEntries(
      ["SingletonLock", "SingletonCookie", "SingletonSocket"].map((name) => {
        try {
          const path = join(silo.profileDirectory, name);
          const metadata = lstatSync(path);
          return [name, metadata.isSymbolicLink() ? readlinkSync(path) : "regular"];
        } catch (error) {
          return [name, { error: error.code }];
        }
      }),
    ),
  };
}

function startManaged(siloId) {
  const activation = cli(["start", siloId]);
  (evidence.managedLaunches ??= []).push(activation);
  assert.equal(activation.state, "running");
  assert.equal(activation.engineEvidence?.verifiedAdapter, "camoufox");
  assert.equal(activation.engineEvidence?.packageVerification, "verified");
  assert.equal(activation.engineEvidence?.hostLaunch, "verified");
  assert.equal(activation.identityEvidence?.state, "matched");
  return activation;
}

try {
  await startDesktop();
  cli(["vault", "init"], `${password}\n${password}\n`);
  cli(["app", "open"]);
  await waitFor(
    () => cli(["app", "status"]).visible === true,
    "native WebView opens",
  );
  cli(["app", "hide"]);
  await waitFor(
    () => cli(["app", "status"]).visible === false,
    "native WebView hides",
  );

  const silo = cli([
    "create",
    "--standard",
    "--name",
    "linux-standard",
    "--browser-kind",
    "chrome",
    "--executable-path",
    resolve(values.browser),
  ]);
  assert(silo.id);
  standard = silo;
  assert.equal(cli(["start", silo.id]).state, "running");
  await closeStandardBrowser(silo);
  await waitFor(
    () => {
      const status = cli(["status", silo.id]).activation;
      evidence.standardClose.lastStatus = status;
      return status?.state === "stopped";
    },
    "Standard browser close reconciliation",
  );
  standard = null;
  assert(cli(["silos"]).some((item) => item.id === silo.id));
  evidence.standard = true;

  if (values.managed) {
    const managed = cli([
      "create",
      "--name",
      "linux-managed",
      "--network",
      "direct",
      "--preset",
      "balanced-en-us",
    ]);
    fixture = new Worker(
      `
      const { parentPort } = require("node:worker_threads");
      require("node:http").createServer((request, response) => {
        response.writeHead(200, { "Content-Type": "text/html" });
        response.end("<!doctype html><title>VeriSilo Linux smoke</title><p>Local profile fixture</p>");
      }).listen(0, "127.0.0.1", function () { parentPort.postMessage(this.address().port); });
    `,
      { eval: true },
    );
    const port = await new Promise((resolvePort, reject) => {
      fixture.once("message", resolvePort);
      fixture.once("error", reject);
    });
    const fixtureUrl = `http://127.0.0.1:${port}/`;
    const firstLaunch = startManaged(managed.id);
    cli(["page", managed.id, "goto", fixtureUrl]);
    assert.equal(
      cli(
        ["page", managed.id, "evaluate"],
        '() => { localStorage.setItem("smoke", "saved"); return localStorage.getItem("smoke"); }',
      ).value,
      "saved",
    );
    const concurrent = cli([
      "create",
      "--name",
      "linux-managed-concurrent",
      "--network",
      "direct",
      "--preset",
      "balanced-en-us",
    ]);
    const concurrentLaunch = startManaged(concurrent.id);
    assert.notEqual(
      concurrentLaunch.identityEvidence.runtimeId,
      firstLaunch.identityEvidence.runtimeId,
    );
    assert.notEqual(
      concurrentLaunch.identityEvidence.artifactId,
      firstLaunch.identityEvidence.artifactId,
    );
    cli(["page", concurrent.id, "goto", fixtureUrl]);
    assert.equal(
      cli(
        ["page", concurrent.id, "evaluate"],
        '() => { localStorage.setItem("smoke", "other-silo"); return localStorage.getItem("smoke"); }',
      ).value,
      "other-silo",
    );
    const windows = cli(["page", managed.id, "windows"]);
    assert.equal(
      windows.available,
      false,
      "Native OS window enumeration is unavailable on Linux.",
    );
    assert(
      windows.page?.innerWidth > 0,
      "Managed page metrics must be observed.",
    );
    const snapshot = cli(["page", managed.id, "snapshot"]);
    assert.equal(snapshot.url, fixtureUrl);
    const screenshot = cli(["page", managed.id, "screenshot"]);
    assert(
      existsSync(screenshot.path),
      "Managed screenshot must exist on disk.",
    );
    cli(["stop", managed.id]);
    const archive = join(root, "managed-backup.vsm");
    cli(
      ["backup", managed.id, archive],
      `${backupPassword}\n${backupPassword}\n`,
    );
    const inspection = cli(
      ["backup-inspect", managed.id, archive],
      `${backupPassword}\n`,
    );
    assert.match(inspection.archiveSha256, /^[a-f0-9]{64}$/);
    startManaged(managed.id);
    cli(["page", managed.id, "goto", fixtureUrl]);
    cli(
      ["page", managed.id, "evaluate"],
      '() => localStorage.setItem("smoke", "changed")',
    );
    cli(["stop", managed.id]);
    cli(
      [
        "backup-restore",
        managed.id,
        archive,
        "--sha256",
        inspection.archiveSha256,
        "--yes",
      ],
      `${backupPassword}\n`,
    );
    startManaged(managed.id);
    cli(["page", managed.id, "goto", fixtureUrl]);
    assert.equal(
      cli(
        ["page", managed.id, "evaluate"],
        '() => localStorage.getItem("smoke")',
      ).value,
      "saved",
      "Cold restore must restore the saved browser Profile.",
    );
    assert.equal(cli(["status", concurrent.id]).activation?.state, "running");
    assert.equal(
      cli(
        ["page", concurrent.id, "evaluate"],
        '() => localStorage.getItem("smoke")',
      ).value,
      "other-silo",
      "Stopping and restoring one Silo must preserve its concurrent peer.",
    );
    cli(["stop", managed.id]);
    assert.equal(cli(["status", concurrent.id]).activation?.state, "running");
    cli(["stop", concurrent.id]);
    evidence.managedConcurrency = true;
    evidence.managed = true;
  }

  await stopDesktop();
  await startDesktop();
  cli(["vault", "unlock"], `${password}\n`);
  assert(
    cli(["silos"]).some((item) => item.id === silo.id),
    "Silo persists across desktop restart.",
  );
  await stopDesktop();
  evidence.passed = true;
  console.log(
    `Native Linux desktop smoke passed: WebView, Standard lifecycle, Vault persistence${values.managed ? ", Managed identity/control/backup/restore" : ""}.`,
  );
} finally {
  if (standard) evidence.standardDiagnostics = standardDiagnostics(standard);
  evidence.completedAt = new Date().toISOString();
  const evidenceRoot = resolve("artifacts/linux-desktop-smoke");
  mkdirSync(evidenceRoot, { recursive: true });
  writeFileSync(
    join(evidenceRoot, "result.json"),
    `${JSON.stringify(evidence, null, 2)}\n`,
  );
  writeFileSync(join(evidenceRoot, "desktop.log"), desktopLog);
  if (fixture) await fixture.terminate();
  if (standard) {
    try {
      await closeStandardBrowser(standard);
    } catch {
      /* Already exited or no longer this fixture. */
    }
  }
  if (desktop?.exitCode === null) {
    cli(["service", "stop"], undefined, true);
    desktop.kill("SIGTERM");
    await waitFor(() => desktop.exitCode !== null, "cleanup shutdown").catch(
      () => desktop.kill("SIGKILL"),
    );
  }
  rmSync(root, { recursive: true, force: true });
}
