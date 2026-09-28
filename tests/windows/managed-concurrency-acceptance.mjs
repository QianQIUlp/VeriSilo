// Native Windows acceptance of two local Managed sessions in one named Vault.
// Run against a committed candidate copied into a separate normal-user app tree.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

const arguments_ = process.argv.slice(2);
function option(name) {
  const index = arguments_.indexOf(name);
  return index < 0 ? undefined : arguments_[index + 1];
}
const cli = option("--cli");
const sourceSha = option("--source-sha");
const uiCheckpoint = arguments_.includes("--ui-checkpoint");
const continuationPath = option("--continue-after-direct");
if (!cli || !existsSync(cli) || !/^[0-9a-f]{40}$/.test(sourceSha ?? "")) {
  throw new Error("Usage: node managed-concurrency-acceptance.mjs --cli <independent normal-user verisilo-cli.exe> --source-sha <committed SHA> [--output-dir <new directory>] [--ui-checkpoint] [--continue-after-direct <prior failed result.json>]");
}
if (continuationPath && uiCheckpoint) throw new Error("Continuation keeps UI verification separate from backend acceptance.");
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
function git(...args) {
  const result = spawnSync("git", ["-C", repoRoot, ...args], { encoding: "utf8", windowsHide: true });
  if (result.status !== 0) throw new Error(`git ${args[0]} failed: ${result.stderr?.trim()}`);
  return result.stdout.trim();
}
assert.equal(git("rev-parse", "HEAD"), sourceSha, "Candidate source SHA differs from worktree HEAD");
assert.equal(git("status", "--porcelain", "--untracked-files=normal"), "", "Candidate worktree must be clean");
const appRoot = path.dirname(path.resolve(cli));
const packageRoot = path.join(appRoot, "managed-browser", "engine-package");
if (appRoot.toLowerCase().includes(".verisilo-worktrees") ||
    appRoot.toLowerCase().includes("managed-cold-backup-880904") ||
    !existsSync(path.join(appRoot, "verisilo.exe")) ||
    !existsSync(path.join(packageRoot, "browser", "camoufox.exe")) ||
    !existsSync(path.join(packageRoot, "engine-package.json"))) {
  throw new Error("CLI must be beside a development desktop and fixed Engine package in a new normal-user app tree.");
}
const account = spawnSync("whoami", { encoding: "utf8", windowsHide: true });
assert.equal(account.status, 0);
assert.equal(account.stdout.trim().toLowerCase(), "telecaster\\qiu", "Native acceptance requires the normal qiu account");
const runId = randomBytes(4).toString("hex");
const vault = `managed2-${runId}`;
const outputDir = path.resolve(option("--output-dir") ??
  path.join(process.env.LOCALAPPDATA, "VeriSiloAcceptance", `managed-concurrency-${runId}`));
const vaultRoot = path.join(process.env.LOCALAPPDATA, "io.verisilo.app", "vaults", vault);
if (existsSync(outputDir) || existsSync(vaultRoot)) throw new Error("Acceptance output or Vault already exists");
await mkdir(outputDir, { recursive: true });
const reportPath = path.join(outputDir, "result.json");
const fixedEngineManifestSha256 = "7bfb701935221b523027738d0c7abaa386851471e99f08eec7eb2bf09917278e";
const report = {
  schema: "verisilo-managed-concurrency-acceptance/v1", runId, sourceSha,
  account: account.stdout.trim(), vault, appRoot, packageRoot, outputDir, vaultRoot,
  startedAt: new Date().toISOString(), steps: [], result: "inconclusive",
};
const passphrase = randomBytes(32).toString("hex");
const fixtureToken = randomBytes(32).toString("hex");
const markers = Object.fromEntries(["A", "B", "P", "Q", "mutated"].map((name) =>
  [name, `${name.toLowerCase()}-${randomBytes(8).toString("hex")}`]));
const fixtureDir = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");
let discovery;
let serviceRunning = false;
let site;
const proxies = new Map();
let failure;

function redact(message) {
  let result = String(message).replaceAll(passphrase, "[passphrase]").replaceAll(fixtureToken, "[fixture-token]");
  for (const marker of Object.values(markers)) result = result.replaceAll(marker, "[marker]");
  if (discovery?.token) result = result.replaceAll(discovery.token, "[local-api-token]");
  return result;
}
function step(name, detail = {}) {
  report.steps.push({ name, at: new Date().toISOString(), ...detail });
  process.stdout.write(`${name}\n`);
}
class ApiRejected extends Error {
  constructor(status, code, message) {
    super(message);
    this.status = status;
    this.code = code;
  }
}
async function api(method, route, body, timeout = 180_000) {
  const response = await fetch(new URL(route, discovery.url), {
    method,
    headers: { Authorization: `Bearer ${discovery.token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }) },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeout),
  }).catch((error) => {
    throw new Error(`${method} ${route}: ${error.cause?.code ?? error.name}: ${error.cause?.message ?? error.message}`);
  });
  const payload = await response.json();
  if (!response.ok || payload.ok !== true) {
    throw new ApiRejected(response.status, payload.code, payload.error ?? "Local API rejected request");
  }
  return payload.data;
}
async function rejected(method, route, body, name) {
  try { await api(method, route, body); }
  catch (error) {
    if (!(error instanceof ApiRejected)) throw error;
    step(name, { status: error.status, code: error.code ?? null });
    return;
  }
  throw new Error(`${name}: unexpectedly accepted`);
}
async function runtime(id) {
  const session = await api("GET", `/v1/silos/${id}/runtime`);
  assert.equal(session.siloId, id);
  return session;
}
async function recent(id) {
  const record = await api("GET", `/v1/silos/${id}/recent-run`);
  assert.equal(record?.siloId, id);
  return record;
}
async function sessions() {
  const result = await api("GET", "/v1/sessions");
  assert.ok(Array.isArray(result));
  assert.equal(new Set(result.map((session) => session.siloId)).size, result.length);
  return result;
}
async function assertRunning(...ids) {
  const all = await sessions();
  for (const id of ids) {
    const session = all.find((entry) => entry.siloId === id);
    assert.equal(session?.activation?.state, "running", `${id} must be running`);
    assert.equal(session.activation.activeSiloId, id);
  }
  return all;
}
async function start(id) {
  const activation = await api("POST", `/v1/silos/${id}/start`);
  assert.equal(activation.state, "running");
  assert.equal(activation.activeSiloId, id);
  assert.equal(activation.identityEvidence?.state, "matched");
  return activation;
}
async function stop(id) {
  const activation = await api("POST", `/v1/silos/${id}/stop`);
  assert.equal(activation.state, "stopped");
  assert.equal(activation.activeSiloId, null);
  return activation;
}
async function page(id, action) { return api("POST", `/v1/silos/${id}/page`, action); }
async function recheck(id) { return api("POST", `/v1/silos/${id}/recheck`); }
async function freePort() {
  const listener = net.createServer();
  await new Promise((resolve, reject) => listener.listen(0, "127.0.0.1", resolve).once("error", reject));
  const port = listener.address().port;
  await new Promise((resolve) => listener.close(resolve));
  return port;
}
async function launchChild(file, args, healthUrl, schema) {
  const child = spawn(process.execPath, [path.join(fixtureDir, file), ...args],
    { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr = (stderr + chunk).slice(-2000); });
  child.stdout.resume();
  for (let attempt = 0; attempt < 60; attempt++) {
    if (child.exitCode !== null) throw new Error(`${file} exited: ${stderr}`);
    try {
      const response = await fetch(healthUrl, { signal: AbortSignal.timeout(500) });
      if (response.ok && (await response.json()).schema === schema) return child;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  child.kill();
  throw new Error(`${file} did not become healthy: ${stderr}`);
}
async function launchSite() {
  report.fixturePort = await freePort();
  site = await launchChild("loopback-server.mjs", ["--host", "127.0.0.1", "--port",
    String(report.fixturePort), "--token", fixtureToken],
  `http://127.0.0.1:${report.fixturePort}/__health?harnessToken=${fixtureToken}`,
  "urn:verisilo:windows-e2e-fixture-health:1");
}
async function launchProxy(name) {
  const port = await freePort();
  const child = await launchChild("loopback-connect-proxy.mjs", ["--port", String(port),
    "--site-port", String(report.fixturePort), "--token", fixtureToken, "--allow-ipwhois"],
  `http://127.0.0.1:${port}/__health?harnessToken=${fixtureToken}`,
  "urn:verisilo:loopback-connect-proxy:1");
  proxies.set(name, { port, child });
  return port;
}
async function events(port) {
  const response = await fetch(`http://127.0.0.1:${port}/__events?harnessToken=${fixtureToken}`);
  assert.equal(response.status, 200);
  return response.json();
}
function fixtureUrl(host, query) {
  const url = new URL(`http://${host}:${report.fixturePort}/`);
  for (const [key, value] of Object.entries(query)) url.searchParams.set(key, value);
  url.searchParams.set("harnessToken", fixtureToken);
  url.searchParams.set("operationToken", randomBytes(16).toString("hex"));
  return url;
}
async function fixturePage(id, host, query) {
  const url = fixtureUrl(host, query);
  await page(id, { action: "goto", url: url.href });
  const expected = `VERISILO_E2E:PASS:${url.searchParams.get("operationToken")}:`;
  const failed = `VERISILO_E2E:FAIL:${url.searchParams.get("operationToken")}:`;
  for (let attempt = 0; attempt < 40; attempt++) {
    const snapshot = await page(id, { action: "snapshot" });
    if (snapshot.title?.startsWith(expected)) return url.searchParams.get("operationToken");
    if (snapshot.title?.startsWith(failed)) throw new Error("Fixture page reported FAIL");
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("Fixture page did not report a result within 10 seconds");
}
function assertRoute(proxyEvents, operationToken, name) {
  assert.ok(proxyEvents.some((event) => event.operationTokens.includes(operationToken)),
    `${name} proxy did not carry its browser operation`);
}
function recentBinding(record) {
  return { siloId: record.siloId, runId: record.runId, runtimeId: record.runtimeId,
    profileSiloId: record.profileSiloId, artifactBinding: record.artifactBinding,
    engineAdapter: record.engineAdapter, networkPolicy: record.networkPolicy,
    identityArtifactId: record.identityEvidence?.artifactId };
}
function cliStatus() {
  const result = spawnSync(cli, ["--vault", vault, "--json", "status"],
    { encoding: "utf8", timeout: 30_000, windowsHide: true });
  if (result.status !== 0) throw new Error(`Development CLI failed to start service: ${result.stderr?.trim()}`);
  return JSON.parse(result.stdout);
}
async function waitForUiCheckpoint(aId, bId) {
  if (!uiCheckpoint) return;
  const checkpointId = randomBytes(16).toString("hex");
  const checkpointPath = path.join(outputDir, "ui-checkpoint.json");
  const continuePath = path.join(outputDir, "ui-continue.json");
  const before = await Promise.all([runtime(aId), runtime(bId)]);
  const runtimeIds = before.map((session) => session.activation.networkEvidence?.runtimeId);
  assert.ok(runtimeIds.every(Boolean) && runtimeIds[0] !== runtimeIds[1]);
  const opened = spawnSync(cli, ["--vault", vault, "app", "open"],
    { encoding: "utf8", timeout: 30_000, windowsHide: true });
  if (opened.status !== 0) throw new Error(`Could not open the existing desktop UI: ${redact(opened.stderr?.trim())}`);
  await writeFile(checkpointPath, `${JSON.stringify({
    schema: "verisilo-managed-concurrency-ui-checkpoint/v1", checkpointId, sourceSha, vault,
    servicePid: discovery.pid, a: { siloId: aId, runtimeId: runtimeIds[0] },
    b: { siloId: bId, runtimeId: runtimeIds[1] },
    createdAt: new Date().toISOString(), continuePath,
  }, null, 2)}\n`, { flag: "wx" });
  step("UI checkpoint ready: switch A/B selection in the live desktop", { checkpointPath, continuePath });
  const deadline = Date.now() + 10 * 60_000;
  let continuation;
  while (Date.now() < deadline) {
    if (existsSync(continuePath)) {
      continuation = JSON.parse(await readFile(continuePath, "utf8"));
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  assert.equal(continuation?.schema, "verisilo-managed-concurrency-ui-continue/v1",
    "UI checkpoint was not completed within 10 minutes");
  assert.equal(continuation.checkpointId, checkpointId, "UI continuation did not match this run");
  assert.equal(continuation.result, "passed", "UI selection check did not pass");
  const after = await Promise.all([runtime(aId), runtime(bId)]);
  assert.deepEqual(after.map((session) => session.activation.networkEvidence?.runtimeId), runtimeIds,
    "UI selection changed a browser's running identity");
  await assertRunning(aId, bId);
  step("live UI selection left both runtime identities unchanged", { checkpointPath, continuePath });
}
async function readDiscovery() {
  discovery = JSON.parse(await readFile(path.join(vaultRoot, "local-api.json"), "utf8"));
  assert.equal(discovery.schema, "verisilo-local-api/v1");
  assert.equal(discovery.vaultName, vault);
  assert.match(discovery.url, /^http:\/\/127\.0\.0\.1:\d+\/?$/);
}

try {
  report.cliSha256 = createHash("sha256").update(await readFile(cli)).digest("hex");
  report.desktopSha256 = createHash("sha256").update(await readFile(path.join(appRoot, "verisilo.exe"))).digest("hex");
  report.enginePackageManifestSha256 = createHash("sha256")
    .update(await readFile(path.join(packageRoot, "engine-package.json"))).digest("hex");
  assert.equal(report.enginePackageManifestSha256, fixedEngineManifestSha256,
    "Native acceptance must use the previously verified fixed Engine package");
  if (continuationPath) {
    const previousPath = path.resolve(continuationPath);
    const previousBytes = await readFile(previousPath);
    const previous = JSON.parse(previousBytes.toString("utf8"));
    assert.equal(previous.schema, report.schema);
    assert.equal(previous.result, "failed");
    assert.match(previous.failure?.message ?? "", /^UI checkpoint was not completed within 10 minutes/);
    assert.ok(previous.steps?.some((entry) => entry.name === "two Direct Managed browsers and Profiles isolated"),
      "Previous attempt must contain the completed direct A/B evidence");
    assert.equal(previous.desktopSha256, report.desktopSha256, "Desktop binary changed since direct A/B evidence");
    assert.equal(previous.cliSha256, report.cliSha256, "CLI binary changed since direct A/B evidence");
    assert.equal(previous.enginePackageManifestSha256, report.enginePackageManifestSha256,
      "Engine package changed since direct A/B evidence");
    report.continuedFrom = { resultPath: previousPath,
      resultSha256: createHash("sha256").update(previousBytes).digest("hex"),
      runId: previous.runId, sourceSha: previous.sourceSha,
      completedStep: "two Direct Managed browsers and Profiles isolated" };
  }
  cliStatus();
  await readDiscovery();
  serviceRunning = true;
  await api("GET", "/v1/status");
  await api("POST", "/v1/vault/initialize", { passphrase });
  await launchSite();
  step("normal-user named Vault and test site ready", { servicePid: discovery.pid });

  const createDirect = (name) => api("POST", "/v1/silos", {
    name: `${name}-${runId}`, color: "#5b5ce2", identityPreset: "balanced-en-us",
    followNetworkExit: false, networkProfile: { mode: "direct", proxyRequired: false },
  });
  const a = await createDirect("managed-A");
  const b = await createDirect("managed-B");
  const c = await createDirect("managed-third");
  assert.equal(new Set([a.id, b.id, c.id]).size, 3);
  assert.notEqual(a.profileDirectory, b.profileDirectory);
  const aFirst = await start(a.id);
  await fixturePage(a.id, "127.0.0.1", { op: "write", value: markers.A });
  if (continuationPath) {
    await start(b.id);
    await fixturePage(b.id, "127.0.0.1", { op: "write", value: markers.B });
    await assertRunning(a.id, b.id);
    step("A/B fixture ready for remaining acceptance; prior direct isolation evidence reused",
      { aId: a.id, bId: b.id, thirdId: c.id });
    const opened = spawnSync(cli, ["--vault", vault, "app", "open"],
      { encoding: "utf8", timeout: 30_000, windowsHide: true });
    report.nonblockingUiOpen = { success: opened.status === 0,
      error: opened.status === 0 ? null : redact(opened.stderr?.trim() || opened.error?.message) };
    process.stdout.write(`UI selection fixture (nonblocking): vault=${vault} A=${a.id} B=${b.id}\n`);
  } else {
    assert.equal((await api("GET", "/v1/status")).activation.activeSiloId, a.id);
    assert.equal(cliStatus().activation.activeSiloId, a.id);
    let bFinished = false;
    const bStart = start(b.id).finally(() => { bFinished = true; });
    const probeStarted = Date.now();
    const aDuringB = await Promise.race([
      Promise.all([runtime(a.id), page(a.id, { action: "snapshot" })]),
      new Promise((_, reject) => setTimeout(() => reject(new Error("A became unresponsive during B launch")), 8000)),
    ]);
    const probeMs = Date.now() - probeStarted;
    const aProbeFinishedBeforeB = !bFinished;
    assert.equal(aDuringB[0].activation.state, "running");
    assert.match(aDuringB[1].title ?? "", /^VERISILO_E2E:PASS:/);
    await bStart;
    await fixturePage(b.id, "127.0.0.1", { op: "write", value: markers.B });
    const statusPair = await api("GET", "/v1/status");
    assert.equal(statusPair.managedSessionLimit, 2);
    assert.ok(![a.id, b.id].includes(statusPair.activation?.activeSiloId),
      "legacy activation cannot select an arbitrary session while A/B both run");
    assert.ok(![a.id, b.id].includes(cliStatus().activation?.activeSiloId),
      "legacy CLI status cannot select an arbitrary session while A/B both run");
    await assertRunning(a.id, b.id);
    assert.equal((await api("GET", `/v1/silos/${a.id}/diagnose`)).active, true);
    assert.equal((await api("GET", `/v1/silos/${b.id}/diagnose`)).active, true);
    assert.notEqual((await runtime(a.id)).activation.networkEvidence?.runtimeId,
      (await runtime(b.id)).activation.networkEvidence?.runtimeId);
    await fixturePage(a.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.A });
    await fixturePage(b.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.B });
    step("two Direct Managed browsers and Profiles isolated", { aId: a.id, bId: b.id,
      thirdId: c.id, aProbeDuringBMs: probeMs, aProbeFinishedBeforeB });
    await waitForUiCheckpoint(a.id, b.id);
  }

  await rejected("POST", `/v1/silos/${a.id}/start`, undefined, "duplicate A start rejected");
  await rejected("POST", `/v1/silos/${c.id}/start`, undefined, "third Managed start rejected");
  await assertRunning(a.id, b.id);
  const beforeB = recentBinding(await recent(b.id));
  const bRuntimeId = (await runtime(b.id)).activation.networkEvidence?.runtimeId;
  const reobservedA = await recheck(a.id);
  assert.equal(reobservedA.activeSiloId, a.id);
  assert.deepEqual(recentBinding(await recent(b.id)), beforeB);
  assert.equal((await runtime(b.id)).activation.networkEvidence?.runtimeId, bRuntimeId);
  await stop(a.id);
  await fixturePage(b.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.B });
  const aSecond = await start(a.id);
  assert.equal(aSecond.identityEvidence.artifactId, aFirst.identityEvidence.artifactId);
  await fixturePage(a.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.A });
  await assertRunning(a.id, b.id);
  step("A reobserve, stop, and restart left B live and attributed");

  await stop(a.id);
  const archive = path.join(outputDir, "managed-A.backup");
  await rejected("POST", `/v1/silos/${b.id}/backup`,
    { destinationPath: path.join(outputDir, "running-B.backup"), passphrase },
    "running B backup rejected");
  assert.equal(existsSync(path.join(outputDir, "running-B.backup")), false);
  const receipt = await api("POST", `/v1/silos/${a.id}/backup`, { destinationPath: archive, passphrase });
  assert.equal(receipt.siloId, a.id);
  assert.equal(realpathSync.native(receipt.destinationPath), realpathSync.native(archive));
  const inspection = await api("POST", `/v1/silos/${a.id}/backup-inspect`, { sourcePath: archive, passphrase });
  assert.equal(inspection.siloId, a.id);
  assert.equal(inspection.archiveSha256, createHash("sha256").update(await readFile(archive)).digest("hex"));
  const restoreBody = { sourcePath: archive, passphrase,
    expectedArchiveSha256: inspection.archiveSha256, confirmOverwrite: true };
  await start(a.id);
  await fixturePage(a.id, "127.0.0.1", { op: "write", value: markers.mutated });
  await rejected("POST", `/v1/silos/${a.id}/backup-restore`, restoreBody,
    "running A restore rejected");
  await fixturePage(a.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.mutated });
  await stop(a.id);
  await api("POST", `/v1/silos/${a.id}/backup-restore`, restoreBody);
  const aRestored = await start(a.id);
  assert.equal(aRestored.identityEvidence.artifactId, inspection.artifactId);
  await fixturePage(a.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.A });
  await fixturePage(b.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.B });
  assert.deepEqual(recentBinding(await recent(b.id)), beforeB);
  await assertRunning(a.id, b.id);
  step("A cold backup and explicit restore completed while B ran", {
    archiveSha256: inspection.archiveSha256, archiveBytes: receipt.bytes, fileCount: receipt.fileCount });

  await stop(a.id);
  await stop(b.id);
  const pPort = await launchProxy("P");
  const qPort = await launchProxy("Q");
  const createProxy = (name, port) => api("POST", "/v1/silos", {
    name: `${name}-${runId}`, color: "#5b5ce2", identityPreset: "balanced-en-us",
    followNetworkExit: false,
    networkProfile: { mode: "fixed_proxy", proxyRequired: true, scheme: "http",
      host: "127.0.0.1", port, bypassList: [] },
  });
  const p = await createProxy("managed-proxy-P", pPort);
  const q = await createProxy("managed-proxy-Q", qPort);
  await start(p.id);
  await start(q.id);
  const pToken = await fixturePage(p.id, "198.51.100.9", { op: "write", value: markers.P });
  const qToken = await fixturePage(q.id, "198.51.100.9", { op: "write", value: markers.Q });
  const pEvents = await events(pPort);
  const qEvents = await events(qPort);
  assertRoute(pEvents, pToken, "P");
  assertRoute(qEvents, qToken, "Q");
  assert.ok(!pEvents.some((event) => event.operationTokens.includes(qToken)));
  assert.ok(!qEvents.some((event) => event.operationTokens.includes(pToken)));
  await assertRunning(p.id, q.id);
  step("two fixed proxy sessions used distinct local CONNECT routes", {
    pId: p.id, qId: q.id, pSiteConnections: pEvents.filter((entry) => entry.operationTokens.length).length,
    qSiteConnections: qEvents.filter((entry) => entry.operationTokens.length).length });

  proxies.get("P").child.kill();
  await new Promise((resolve) => proxies.get("P").child.once("exit", resolve));
  // Keep the failed request on the same proxied destination. Firefox may
  // bypass proxies for loopback URLs even when the Silo policy says fixed.
  const failedToken = fixtureUrl("198.51.100.9", { op: "read-persistent", expectedPersistent: markers.P });
  try { await page(p.id, { action: "goto", url: failedToken.href }); }
  catch (error) { if (!(error instanceof ApiRejected)) throw error; }
  await new Promise((resolve) => setTimeout(resolve, 500));
  const siteEvents = await events(report.fixturePort);
  assert.ok(!siteEvents.some((event) => event.path.includes(failedToken.searchParams.get("operationToken"))),
    "failed P proxy leaked its browser request directly to the local site");
  try { await recheck(p.id); }
  catch (error) { if (!(error instanceof ApiRejected)) throw error; }
  const pFailed = await runtime(p.id);
  assert.ok(["verification_failed", "recovery_required", "stopped", "failed"].includes(pFailed.activation.state) ||
    pFailed.activation.networkEvidence?.browserRouting === "failed", "P failure was not attributed");
  const qAfterFailure = await fixturePage(q.id, "198.51.100.9",
    { op: "read-persistent", expectedPersistent: markers.Q });
  assertRoute(await events(qPort), qAfterFailure, "Q after P failure");
  assert.equal((await runtime(q.id)).activation.state, "running");
  step("P proxy failure closed P route while Q remained usable", { pState: pFailed.activation.state });

  await stop(p.id);
  await start(a.id);
  await fixturePage(a.id, "127.0.0.1", { op: "read-persistent", expectedPersistent: markers.A });
  const qMixedToken = await fixturePage(q.id, "198.51.100.9",
    { op: "read-persistent", expectedPersistent: markers.Q });
  assertRoute(await events(qPort), qMixedToken, "Q with Direct A running");
  await assertRunning(a.id, q.id);
  step("Direct A and fixed proxy Q ran together");

  await api("POST", "/v1/vault/lock");
  assert.equal((await api("GET", "/v1/status")).vault.state, "locked");
  await rejected("POST", `/v1/silos/${a.id}/page`, { action: "snapshot" }, "locked A page rejected");
  await rejected("POST", `/v1/silos/${q.id}/page`, { action: "snapshot" }, "locked Q page rejected");
  await api("POST", "/v1/vault/unlock", { passphrase });
  for (const id of [a.id, q.id]) {
    if ((await runtime(id)).activation.activeSiloId === id) await stop(id);
  }
  step("Vault lock blocked both page controls; both sessions remained manageable after unlock");

  await start(a.id);
  await start(q.id);
  await api("POST", "/v1/service/stop");
  const oldPid = discovery.pid;
  let oldServiceGone = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    try { await fetch(new URL("/v1/health", discovery.url), { signal: AbortSignal.timeout(250) }); }
    catch { oldServiceGone = true; break; }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.equal(oldServiceGone, true, "service did not exit after stopping both Managed sessions");
  serviceRunning = false;
  cliStatus();
  await readDiscovery();
  serviceRunning = true;
  assert.notEqual(discovery.pid, oldPid, "service did not restart");
  await api("POST", "/v1/vault/unlock", { passphrase });
  const afterRestartA = await runtime(a.id);
  const afterRestartQ = await runtime(q.id);
  assert.equal(afterRestartA.siloId, a.id);
  assert.equal(afterRestartQ.siloId, q.id);
  assert.notEqual(afterRestartA.activation.state, "running");
  assert.notEqual(afterRestartQ.activation.state, "running");
  assert.equal((await recent(a.id)).siloId, a.id);
  assert.equal((await recent(q.id)).siloId, q.id);
  step("service exit and restart retained per-Silo recovery attribution", {
    aState: afterRestartA.activation.state, qState: afterRestartQ.activation.state,
    oldPid, newPid: discovery.pid });
} catch (error) {
  failure = error;
  report.failure = { name: error.name, message: redact(error.message) };
} finally {
  if (serviceRunning) {
    const cleanupErrors = [];
    try {
      for (const session of await sessions()) {
        if (session.activation.activeSiloId === session.siloId) {
          try { await stop(session.siloId); } catch (error) { cleanupErrors.push(error); }
        }
      }
    } catch (error) { cleanupErrors.push(error); }
    try { await api("POST", "/v1/vault/lock"); } catch (error) { cleanupErrors.push(error); }
    try { await api("POST", "/v1/service/stop"); } catch (error) { cleanupErrors.push(error); }
    if (cleanupErrors.length) {
      report.cleanupError = cleanupErrors.map((error) => redact(error.message)).join("; ");
      failure ??= cleanupErrors[0];
    }
  }
  for (const proxy of proxies.values()) if (proxy.child.exitCode === null) proxy.child.kill();
  if (site?.exitCode === null) site.kill();
  report.finishedAt = new Date().toISOString();
  report.result = failure ? "failed" : "passed";
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${report.result}: ${reportPath}\n`);
  if (failure) process.exitCode = 1;
}
