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
  readdirSync,
  openSync,
  fstatSync,
  readSync,
  closeSync,
  constants,
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
let managedRestart;
const evidence = {
  platform: "linux-x64",
  standard: false,
  managed: false,
  passed: false,
};

function cli(args, input, allowFailure = false, timeout = 180_000) {
  const result = spawnSync(
    resolve(values.cli),
    ["--vault", "linux-smoke", "--json", ...args],
    {
      env,
      input,
      encoding: "utf8",
      timeout,
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
  await waitFor(() => {
    const observed = standardBrowserWindows(record.pid, true);
    evidence.standardWindowFirst ??= observed;
    evidence.standardClose = observed;
    return observed.windows.length > 0;
  }, "Standard browser's own visible X11 window");
}

function standardBrowserWindows(pid, close) {
  const closeWindow = String.raw`
import ctypes as C, json, sys
pid = int(sys.argv[1])
send_close = sys.argv[2] == "1"
x = C.CDLL("libX11.so.6")
window = C.c_ulong
class Attributes(C.Structure):
    _fields_ = [(name, C.c_int) for name in ("x", "y", "width", "height", "border_width", "depth")] + [
        ("visual", C.c_void_p), ("root", window), ("window_class", C.c_int),
        ("bit_gravity", C.c_int), ("win_gravity", C.c_int), ("backing_store", C.c_int),
        ("backing_planes", C.c_ulong), ("backing_pixel", C.c_ulong), ("save_under", C.c_int),
        ("colormap", window), ("map_installed", C.c_int), ("map_state", C.c_int),
        ("all_event_masks", C.c_long), ("your_event_mask", C.c_long), ("do_not_propagate_mask", C.c_long),
        ("override_redirect", C.c_int), ("screen", C.c_void_p),
    ]
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
    "XGetWindowAttributes": (C.c_int, [C.c_void_p, window, C.POINTER(Attributes)]),
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
type_atom = x.XInternAtom(d, b"_NET_WM_WINDOW_TYPE", False)
normal_atom = x.XInternAtom(d, b"_NET_WM_WINDOW_TYPE_NORMAL", False)
root, parent, children, count = window(), window(), C.POINTER(window)(), C.c_uint()
closed, owned = [], []
try:
    if not x.XQueryTree(d, x.XDefaultRootWindow(d), C.byref(root), C.byref(parent), C.byref(children), C.byref(count)):
        raise RuntimeError("Cannot enumerate Xvfb top-level windows")
    for index in range(min(count.value, 128)):
        w = children[index]
        kind, fmt, items, remaining, value = window(), C.c_int(), C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ubyte)()
        x.XGetWindowProperty(d, w, pid_atom, 0, 1, False, 6, C.byref(kind), C.byref(fmt), C.byref(items), C.byref(remaining), C.byref(value))
        matches = kind.value == 6 and fmt.value == 32 and items.value == 1 and value and C.cast(value, C.POINTER(C.c_ulong))[0] == pid
        if value:
            x.XFree(value)
        if not matches:
            continue
        attributes = Attributes()
        if not x.XGetWindowAttributes(d, w, C.byref(attributes)):
            continue
        value = C.POINTER(C.c_ubyte)()
        x.XGetWindowProperty(d, w, type_atom, 0, 16, False, 4, C.byref(kind), C.byref(fmt), C.byref(items), C.byref(remaining), C.byref(value))
        types = list(C.cast(value, C.POINTER(C.c_ulong))[:items.value]) if value and kind.value == 4 and fmt.value == 32 else []
        if value:
            x.XFree(value)
        is_normal = normal_atom in types
        owned.append({"id": w, "types": types, "normal": is_normal, "mapState": attributes.map_state,
                      "geometry": [attributes.x, attributes.y, attributes.width, attributes.height],
                      "overrideRedirect": bool(attributes.override_redirect)})
        # PID and WM_DELETE_WINDOW also occur on Chromium's hidden startup
        # windows. A user can close only a mapped, normal browser window.
        if not send_close or not is_normal or attributes.map_state != 2 or attributes.override_redirect or attributes.width <= 0 or attributes.height <= 0:
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
print(json.dumps({"pid": pid, "windows": closed, "ownedWindows": owned}))
`;
  const result = spawnSync(
    "python3",
    ["-c", closeWindow, String(pid), close ? "1" : "0"],
    {
      encoding: "utf8",
      timeout: 5_000,
    },
  );
  assert.equal(result.status, 0, result.stderr || String(result.error));
  const observed = JSON.parse(result.stdout);
  assert.equal(observed.pid, pid);
  return observed;
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
  let windows = null;
  if (pid) {
    try {
      windows = standardBrowserWindows(pid, false);
    } catch (error) {
      windows = { error: String(error) };
    }
  }
  return {
    lastStatus: evidence.standardClose?.lastStatus,
    record,
    windows,
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
  const engine = activation.engineEvidence;
  assert.equal(engine?.configuredAdapter, "camoufox");
  assert.equal(engine?.launchedAdapter, "camoufox");
  // RC5 authenticates the package at the installer build boundary. Loading
  // the installed package does not re-hash or CMS-verify the entire tree.
  assert.equal(engine?.verifiedAdapter, null);
  assert.equal(engine?.packageVerification, "not_requested");
  assert.equal(engine?.hostLaunch, "observed");
  const binding = engine?.packageVerificationDetails;
  assert.equal(binding?.verifierId, "installed-package");
  assert.equal(binding?.digestVerified, false);
  assert.equal(binding?.signatureVerified, false);
  assert.equal(
    binding?.engineRevision,
    "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3",
  );
  for (const field of [
    "artifactSha256",
    "packageManifestSha256",
    "packageTreeSha256",
    "hostSha256",
    "signerCertificateSha256",
  ]) {
    assert.match(binding[field], /^[a-f0-9]{64}$/u, field);
  }
  assert.equal(binding.artifactSha256, binding.hostSha256);
  assert(
    (process.env.VERISILO_ENGINE_SIGNER_SHA256 ?? "")
      .split(",")
      .map((pin) => pin.trim())
      .includes(binding.signerCertificateSha256),
    "Installed package must retain this build's exact signer certificate pin.",
  );
  assert.equal(activation.identityEvidence?.state, "matched");
  assert.equal(activation.identityEvidence?.siloId, siloId);
  assert.equal(activation.identityEvidence?.engineAdapter, "camoufox");
  return activation;
}

function saveFailedHostLogs(evidenceRoot) {
  const diagnose = (path, error) => {
    (evidence.hostLogCopyErrors ??= []).push({
      path,
      error: error.code ?? String(error),
    });
  };
  try {
    let silosRoot = root;
    for (const name of ["VeriSilo", "vaults", "linux-smoke", "silos"]) {
      silosRoot = join(silosRoot, name);
      const metadata = lstatSync(silosRoot);
      if (!metadata.isDirectory() || metadata.isSymbolicLink()) return;
    }
    for (const entry of readdirSync(silosRoot, { withFileTypes: true })) {
      if (
        !/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/u.test(
          entry.name,
        ) ||
        !entry.isDirectory() ||
        entry.isSymbolicLink()
      ) {
        continue;
      }
      const stateRoot = join(silosRoot, entry.name, "engine-state");
      try {
        const silo = lstatSync(join(silosRoot, entry.name));
        if (!silo.isDirectory() || silo.isSymbolicLink()) continue;
        const state = lstatSync(stateRoot);
        if (!state.isDirectory() || state.isSymbolicLink()) continue;
      } catch (error) {
        if (error.code !== "ENOENT") diagnose(entry.name, error);
        continue;
      }
      for (const name of ["host-stderr.log", "host-stderr.log.1"]) {
        let fd;
        try {
          const path = join(stateRoot, name);
          const metadata = lstatSync(path);
          if (!metadata.isFile() || metadata.isSymbolicLink()) continue;
          fd = openSync(
            path,
            constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
          );
          const opened = fstatSync(fd);
          if (!opened.isFile()) continue;
          const size = Math.min(opened.size, 128 * 1024);
          const tail = Buffer.alloc(size);
          const bytes = readSync(fd, tail, 0, size, opened.size - size);
          const destination = join(evidenceRoot, "host-logs", entry.name);
          mkdirSync(destination, { recursive: true });
          writeFileSync(join(destination, name), tail.subarray(0, bytes));
        } catch (error) {
          if (error.code !== "ENOENT") diagnose(`${entry.name}/${name}`, error);
        } finally {
          if (fd !== undefined) {
            try {
              closeSync(fd);
            } catch (error) {
              diagnose(`${entry.name}/${name}`, error);
            }
          }
        }
      }
    }
  } catch (error) {
    diagnose("silos", error);
  }
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
  evidence.standardClose.after = standardBrowserWindows(
    evidence.standardClose.pid,
    false,
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
    managedRestart = { siloId: managed.id, url: fixtureUrl };
  }

  await stopDesktop();
  await startDesktop();
  cli(["vault", "unlock"], `${password}\n`);
  assert(
    cli(["silos"]).some((item) => item.id === silo.id),
    "Silo persists across desktop restart.",
  );
  if (managedRestart) {
    startManaged(managedRestart.siloId);
    cli(["page", managedRestart.siloId, "goto", managedRestart.url]);
    assert.equal(
      cli(
        ["page", managedRestart.siloId, "evaluate"],
        '() => localStorage.getItem("smoke")',
      ).value,
      "saved",
      "Managed identity and restored Profile must survive desktop restart.",
    );
    cli(["stop", managedRestart.siloId]);
    evidence.managedDesktopRestart = true;
    evidence.managed = true;
  }
  await stopDesktop();
  evidence.passed = true;
  console.log(
    `Native Linux desktop smoke passed: WebView, Standard lifecycle, Vault persistence${values.managed ? ", Managed identity/control/backup/restore" : ""}.`,
  );
} finally {
  if (standard) evidence.standardDiagnostics = standardDiagnostics(standard);
  if (!evidence.passed && desktop?.exitCode === null) {
    const threads = spawnSync(
      "ps",
      ["-L", "-p", String(desktop.pid), "-o", "pid,tid,ppid,stat,wchan:32,comm"],
      { encoding: "utf8", timeout: 5_000 },
    );
    evidence.desktopThreadDiagnostics = {
      status: threads.status,
      output: threads.stdout,
      error: threads.stderr || threads.error?.message,
    };
  }
  if (!evidence.passed && values.managed) {
    evidence.managedFailureStatuses = [];
    const ids = new Set(
      (evidence.managedLaunches ?? []).map((launch) => launch.activeSiloId),
    );
    for (const siloId of ids) {
      try {
        const status = cli(["status", siloId], undefined, true, 15_000);
        evidence.managedFailureStatuses.push({
          siloId,
          state: status?.activation?.state,
          message: status?.activation?.message,
          engineEvidence: status?.activation?.engineEvidence,
        });
      } catch (error) {
        evidence.managedFailureStatuses.push({ siloId, error: String(error) });
      }
    }
  }
  evidence.completedAt = new Date().toISOString();
  const evidenceRoot = resolve("artifacts/linux-desktop-smoke");
  mkdirSync(evidenceRoot, { recursive: true });
  if (!evidence.passed) saveFailedHostLogs(evidenceRoot);
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
