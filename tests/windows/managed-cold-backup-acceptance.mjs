// Native Windows, same-user/same-Vault Managed cold-backup acceptance.
// Run only against a dedicated development app tree with a bundled engine package.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { randomBytes, createHash } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
function option(name) {
  const index = args.indexOf(name);
  return index < 0 ? undefined : args[index + 1];
}
const cli = option("--cli");
const expectedSourceSha = option("--source-sha");
if (!cli || !existsSync(cli)) {
  throw new Error("Usage: node managed-cold-backup-acceptance.mjs --cli <normal-user development verisilo-cli.exe> --source-sha <committed HEAD> [--output-dir <new directory>]");
}
assert.match(expectedSourceSha ?? "", /^[0-9a-f]{40}$/, "--source-sha must identify the exact committed candidate");
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
function git(...arguments_) {
  const result = spawnSync("git", ["-C", repoRoot, ...arguments_], { encoding: "utf8", windowsHide: true });
  if (result.status !== 0) throw new Error(`git ${arguments_[0]} failed: ${result.stderr?.trim()}`);
  return result.stdout.trim();
}
assert.equal(git("rev-parse", "HEAD"), expectedSourceSha, "Candidate source SHA differs from worktree HEAD");
assert.equal(git("status", "--porcelain", "--untracked-files=normal"), "", "Candidate worktree must be clean");
const appRoot = path.dirname(path.resolve(cli));
const packageRoot = path.join(appRoot, "managed-browser", "engine-package");
if (appRoot.toLowerCase().includes(".verisilo-worktrees") ||
    !existsSync(path.join(appRoot, "verisilo.exe")) ||
    !existsSync(path.join(packageRoot, "browser", "camoufox.exe")) ||
    !existsSync(path.join(packageRoot, "engine-package.json"))) {
  throw new Error("The CLI must be beside a development desktop and complete Engine package outside a Codex worktree.");
}
const account = spawnSync("whoami", { encoding: "utf8", windowsHide: true });
assert.equal(account.status, 0, "Cannot determine Windows account");
assert.equal(account.stdout.trim().toLowerCase(), "telecaster\\qiu", "Native acceptance requires the normal qiu account");
const runId = randomBytes(4).toString("hex");
const vault = `coldbk-${runId}`;
const outputDir = path.resolve(option("--output-dir") ??
  path.join(process.env.LOCALAPPDATA, "VeriSiloAcceptance", `cold-backup-${runId}`));
if (existsSync(outputDir)) throw new Error(`Acceptance output already exists: ${outputDir}`);
const vaultRoot = path.join(process.env.LOCALAPPDATA, "io.verisilo.app", "vaults", vault);
if (existsSync(vaultRoot)) throw new Error(`Acceptance Vault already exists: ${vaultRoot}`);
await mkdir(outputDir, { recursive: true });
const reportPath = path.join(outputDir, "result.json");
const report = {
  schema: "verisilo-managed-cold-backup-acceptance/v1",
  runId, sourceSha: expectedSourceSha, account: account.stdout.trim(), vault,
  appRoot, packageRoot, outputDir, vaultRoot,
  startedAt: new Date().toISOString(), steps: [], result: "inconclusive",
};
let discovery;
let fixture;
let fixtureError = "";
let failure;
const passphrase = randomBytes(32).toString("hex");
const fixtureToken = randomBytes(32).toString("hex");
const markerA = `a-${randomBytes(8).toString("hex")}`;
const markerB = `b-${randomBytes(8).toString("hex")}`;
const markerMutated = `mutated-${randomBytes(8).toString("hex")}`;
const fixturePath = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures", "loopback-server.mjs");
function redact(message) {
  return String(message).replaceAll(passphrase, "[passphrase]")
    .replaceAll(fixtureToken, "[fixture-token]")
    .replaceAll(markerA, "[marker-A]")
    .replaceAll(markerB, "[marker-B]")
    .replaceAll(markerMutated, "[mutated-marker]");
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
async function api(method, route, body) {
  const response = await fetch(new URL(route, discovery.url), {
    method,
    headers: {
      Authorization: `Bearer ${discovery.token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(180_000),
  });
  const payload = await response.json();
  if (!response.ok || payload.ok !== true) {
    throw new ApiRejected(response.status, payload.code, payload.error ?? "Local API rejected request");
  }
  return payload.data;
}
async function rejected(method, route, body, name) {
  try {
    await api(method, route, body);
  } catch (error) {
    if (!(error instanceof ApiRejected)) throw error;
    step(name, { status: error.status, code: error.code ?? null });
    return;
  }
  throw new Error(`${name}: unexpectedly accepted`);
}
function siloProjection(silo) {
  const keys = ["id", "name", "color", "browser", "executionTarget", "profileDirectory",
    "networkProfile", "engine", "seedReference", "createdAt", "identityLockedAt", "archivedAt"];
  return Object.fromEntries(keys.map((key) => [key, silo[key] ?? null]));
}
async function silo(id) {
  return api("GET", `/v1/silos/${id}`);
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
  assert.equal(activation.activeSiloId, null);
}
async function page(id, action) {
  return api("POST", `/v1/silos/${id}/page`, action);
}
function fixtureUrl(query) {
  const url = new URL(`http://127.0.0.1:${report.fixturePort}/`);
  for (const [key, value] of Object.entries(query)) url.searchParams.set(key, value);
  url.searchParams.set("harnessToken", fixtureToken);
  url.searchParams.set("operationToken", randomBytes(16).toString("hex"));
  return url;
}
async function fixturePage(id, query) {
  const url = fixtureUrl(query);
  await page(id, { action: "goto", url: url.href });
  const expected = `VERISILO_E2E:PASS:${url.searchParams.get("operationToken")}:`;
  const failed = `VERISILO_E2E:FAIL:${url.searchParams.get("operationToken")}:`;
  for (let attempt = 0; attempt < 40; attempt++) {
    const snapshot = await page(id, { action: "snapshot" });
    if (snapshot.title?.startsWith(expected)) return;
    if (snapshot.title?.startsWith(failed)) throw new Error("Fixture page reported FAIL");
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("Fixture page did not report a result within 10 seconds");
}
async function freePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => server.listen(0, "127.0.0.1", resolve).once("error", reject));
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return port;
}
async function launchFixture() {
  report.fixturePort = await freePort();
  fixture = spawn(process.execPath, [fixturePath, "--host", "127.0.0.1", "--port",
    String(report.fixturePort), "--token", fixtureToken], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  fixture.stderr.on("data", (chunk) => { fixtureError = (fixtureError + chunk).slice(-2000); });
  fixture.stdout.resume();
  for (let attempt = 0; attempt < 40; attempt++) {
    if (fixture.exitCode !== null) throw new Error(`Fixture exited: ${fixtureError}`);
    try {
      const health = await fetch(`http://127.0.0.1:${report.fixturePort}/__health?harnessToken=${fixtureToken}`);
      const body = await health.json();
      if (body.schema === "urn:verisilo:windows-e2e-fixture-health:1" && body.token === fixtureToken) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 125));
  }
  throw new Error(`Fixture did not become healthy: ${fixtureError}`);
}

try {
  const cliBytes = await readFile(cli);
  const desktopBytes = await readFile(path.join(appRoot, "verisilo.exe"));
  report.cliSha256 = createHash("sha256").update(cliBytes).digest("hex");
  report.desktopSha256 = createHash("sha256").update(desktopBytes).digest("hex");
  const manifest = await readFile(path.join(packageRoot, "engine-package.json"));
  report.enginePackageManifestSha256 = createHash("sha256").update(manifest).digest("hex");
  const cliStatus = spawnSync(cli, ["--vault", vault, "--json", "status"], {
    encoding: "utf8", timeout: 30_000, windowsHide: true,
  });
  if (cliStatus.status !== 0) throw new Error(`Development CLI did not start its service: ${cliStatus.stderr?.trim()}`);
  discovery = JSON.parse(await readFile(path.join(vaultRoot, "local-api.json"), "utf8"));
  assert.equal(discovery.schema, "verisilo-local-api/v1");
  assert.equal(discovery.vaultName, vault);
  assert.match(discovery.url, /^http:\/\/127\.0\.0\.1:\d+\/?$/);
  step("normal-user development service started", { servicePid: discovery.pid });
  await api("POST", "/v1/vault/initialize", { passphrase });
  await launchFixture();
  step("isolated Vault and loopback fixture ready");

  const create = (name) => api("POST", "/v1/silos", {
    name, color: "#5b5ce2", identityPreset: "balanced-en-us", followNetworkExit: false,
    networkProfile: { mode: "direct", proxyRequired: false },
  });
  const a = await create(`cold-A-${runId}`);
  const b = await create(`cold-B-${runId}`);
  assert.notEqual(a.id, b.id);
  step("Managed A and B created", { aId: a.id, bId: b.id });

  const originalA = await start(a.id);
  await fixturePage(a.id, { op: "write", value: markerA });
  await stop(a.id);
  const originalB = await start(b.id);
  await fixturePage(b.id, { op: "write", value: markerB });
  await stop(b.id);
  const beforeA = siloProjection(await silo(a.id));
  const beforeB = siloProjection(await silo(b.id));
  step("A and B original browser markers persisted");

  const archive = path.join(outputDir, "managed-A.backup");
  const receipt = await api("POST", `/v1/silos/${a.id}/backup`, {
    destinationPath: archive, passphrase,
  });
  assert.equal(receipt.siloId, a.id);
  assert.equal(realpathSync.native(receipt.destinationPath), realpathSync.native(archive));
  assert.ok(receipt.bytes > 0 && receipt.profileBytes > 0 && receipt.fileCount > 0);
  const inspect = await api("POST", `/v1/silos/${a.id}/backup-inspect`, { sourcePath: archive, passphrase });
  assert.match(inspect.archiveSha256, /^[0-9a-f]{64}$/);
  assert.equal(inspect.siloId, a.id);
  assert.equal(inspect.siloName, a.name);
  assert.equal(inspect.artifactId, originalA.identityEvidence.artifactId);
  assert.equal(inspect.artifactSha256, originalA.identityEvidence.artifactFileSha256);
  assert.ok(inspect.engineVersion);
  step("stopped A backed up and inspected", {
    archiveSha256: inspect.archiveSha256, artifactId: inspect.artifactId,
    artifactSha256: inspect.artifactSha256, engineVersion: inspect.engineVersion,
    backupBytes: receipt.bytes, profileBytes: receipt.profileBytes, fileCount: receipt.fileCount,
  });

  await start(a.id);
  await fixturePage(a.id, { op: "write", value: markerMutated });
  await stop(a.id);
  const restoreBody = {
    sourcePath: archive, passphrase, expectedArchiveSha256: inspect.archiveSha256,
    confirmOverwrite: true,
  };
  await rejected("POST", `/v1/silos/${a.id}/backup-restore`,
    { ...restoreBody, expectedArchiveSha256: "0".repeat(64) }, "wrong archive hash rejected");
  await rejected("POST", `/v1/silos/${a.id}/backup-restore`,
    { ...restoreBody, passphrase: randomBytes(32).toString("hex") }, "wrong passphrase rejected");
  await start(a.id);
  await fixturePage(a.id, { op: "read-persistent", expectedPersistent: markerMutated });
  await stop(a.id);
  assert.deepEqual(siloProjection(await silo(a.id)), beforeA);
  assert.deepEqual(siloProjection(await silo(b.id)), beforeB);
  step("rejections preserved mutated A and untouched B metadata");

  const restored = await api("POST", `/v1/silos/${a.id}/backup-restore`, restoreBody);
  assert.equal(restored.id, a.id);
  assert.deepEqual(siloProjection(restored), beforeA);
  const restoredA = await start(a.id);
  assert.equal(restoredA.identityEvidence.artifactId, inspect.artifactId);
  assert.equal(restoredA.identityEvidence.artifactFileSha256, inspect.artifactSha256);
  await fixturePage(a.id, { op: "read-persistent", expectedPersistent: markerA });
  await stop(a.id);
  assert.deepEqual(siloProjection(await silo(a.id)), beforeA);
  step("original A Profile, Artifact binding, and configuration restored");

  const afterB = await start(b.id);
  assert.equal(afterB.identityEvidence.artifactId, originalB.identityEvidence.artifactId);
  await fixturePage(b.id, { op: "read-persistent", expectedPersistent: markerB });
  await stop(b.id);
  assert.deepEqual(siloProjection(await silo(b.id)), beforeB);
  step("B remained unchanged after A restore");
} catch (error) {
  failure = error;
  report.failure = { name: error.name, message: redact(error.message) };
} finally {
  if (discovery) {
    try {
      const status = await api("GET", "/v1/status");
      if (status.activation?.activeSiloId) await stop(status.activation.activeSiloId);
      await api("POST", "/v1/vault/lock");
      await api("POST", "/v1/service/stop");
    } catch (error) {
      report.cleanupError = redact(error.message);
      failure ??= error;
    }
  }
  if (fixture && fixture.exitCode === null) fixture.kill();
  report.finishedAt = new Date().toISOString();
  report.result = failure ? "failed" : "passed";
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${report.result}: ${reportPath}\n`);
  if (failure) process.exitCode = 1;
}
