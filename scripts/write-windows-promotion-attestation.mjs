import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";

const sha256Pattern = /^[0-9a-f]{64}$/u;
const revisionPattern = /^[0-9a-f]{40}$/u;

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function countStatuses(results) {
  const counts = { PASS: 0, FAIL: 0, SKIP: 0, BLOCKED: 0 };
  for (const entry of results) {
    if (
      entry === null ||
      typeof entry !== "object" ||
      typeof entry.name !== "string" ||
      typeof entry.detail !== "string" ||
      !Object.hasOwn(counts, entry.status)
    ) {
      throw new Error("Harness summary contains a malformed result.");
    }
    counts[entry.status] += 1;
  }
  return counts;
}

function requiredEvidenceNames(browser) {
  return [
    "windows_matrix_target",
    `${browser}_default_profile_unchanged`,
    `${browser}_temporary_A_B_storage_cookie_isolation`,
    `${browser}_browser_profile_lock_refusal`,
    `${browser}_loopback_proxy_fail_closed`,
    `${browser}_extension_absent_browser_baseline`,
    `${browser}_desktop_vault_init_unlock_silo_create`,
    `${browser}_desktop_isolated_user_data_dir`,
    "vault_locked_sensitive_operation_refusal",
    "verisilo_profile_lock_safe_refusal",
    "extension_absent_desktop_degradation",
    "desktop_recovery_after_exception",
    "native_host_current_user_registration_and_messages",
  ];
}

function acceptanceResultNames(browser) {
  return [
    `${browser}_desktop_vault_init_unlock_silo_create`,
    `${browser}_desktop_isolated_user_data_dir`,
    "vault_locked_sensitive_operation_refusal",
    "verisilo_profile_lock_safe_refusal",
    "extension_absent_desktop_degradation",
    "desktop_recovery_after_exception",
  ];
}

function hasExactKeys(value, keys) {
  return (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    JSON.stringify(Object.keys(value).sort()) ===
      JSON.stringify([...keys].sort())
  );
}

function validateAcceptanceReceipt(receipt, options) {
  const expectedNames = acceptanceResultNames(options.browser).sort();
  const actualNames = Array.isArray(receipt?.results)
    ? receipt.results.map((entry) => entry?.name).sort()
    : [];
  if (
    !hasExactKeys(receipt, [
      "browser",
      "candidate",
      "driverBuild",
      "result",
      "results",
      "safety",
      "schema",
      "schemaVersion",
    ]) ||
    !hasExactKeys(receipt.candidate, [
      "artifactId",
      "artifactSha256",
      "repository",
      "sourceRevision",
    ]) ||
    !hasExactKeys(receipt.driverBuild, [
      "cargoFeature",
      "credentialTransport",
      "sourceRevision",
    ]) ||
    !hasExactKeys(receipt.browser, [
      "companionState",
      "isolatedUserDataDir",
      "kind",
      "version",
    ]) ||
    !hasExactKeys(receipt.safety, [
      "exactRuntimeTermination",
      "osTemporaryRootValidated",
      "productionRootsRefused",
      "profilePreserved",
      "randomSentinelValidated",
      "unrelatedProcessSurvived",
    ]) ||
    receipt?.schema !== "urn:verisilo:windows-acceptance-receipt:1" ||
    receipt.schemaVersion !== 1 ||
    receipt.result !== "PASS" ||
    receipt.candidate?.repository !== options.repository ||
    receipt.candidate.artifactId !== options.artifactId ||
    receipt.candidate.artifactSha256 !== options.artifactSha256 ||
    receipt.candidate.sourceRevision !== options.sourceRevision ||
    receipt.driverBuild?.sourceRevision !== options.sourceRevision ||
    receipt.driverBuild.cargoFeature !== "acceptance-tests" ||
    receipt.driverBuild.credentialTransport !== "anonymous-stdin-pipe" ||
    receipt.browser?.kind !== options.browser ||
    typeof receipt.browser.version !== "string" ||
    receipt.browser.version.length === 0 ||
    receipt.browser.isolatedUserDataDir !== true ||
    receipt.browser.companionState !== "not_connected_no_extension_evidence" ||
    receipt.safety?.osTemporaryRootValidated !== true ||
    receipt.safety.randomSentinelValidated !== true ||
    receipt.safety.productionRootsRefused !== true ||
    receipt.safety.exactRuntimeTermination !== true ||
    receipt.safety.unrelatedProcessSurvived !== true ||
    receipt.safety.profilePreserved !== true ||
    JSON.stringify(actualNames) !== JSON.stringify(expectedNames) ||
    receipt.results.some(
      (entry) =>
        !hasExactKeys(entry, ["detail", "name", "status"]) ||
        entry.status !== "PASS" ||
        typeof entry.detail !== "string" ||
        entry.detail.length === 0,
    )
  ) {
    throw new Error(
      "desktop acceptance receipt is incomplete or candidate/source/browser mismatched",
    );
  }
}

async function writeAttestation(options) {
  const failures = [];
  let descriptor;
  let descriptorMatches = false;
  let descriptorSha256 = null;
  let results = [];
  let summarySha256 = null;
  let acceptanceReceiptSha256 = null;
  let acceptanceReceiptMatches = false;
  let acceptanceReceipt = null;
  if (options.candidateOutcome !== "success") {
    failures.push(
      `candidate verification step outcome was ${options.candidateOutcome}`,
    );
  }
  try {
    const descriptorBytes = await readFile(options.descriptorPath);
    descriptorSha256 = createHash("sha256")
      .update(descriptorBytes)
      .digest("hex");
    descriptor = JSON.parse(descriptorBytes.toString("utf8"));
    descriptorMatches =
      descriptor?.schema === "urn:verisilo:windows-promotion-candidate:1" &&
      descriptor.schemaVersion === 1 &&
      descriptor.repository === options.repository &&
      descriptor.artifactId === options.artifactId &&
      descriptor.artifactSha256 === options.artifactSha256 &&
      descriptor.sourceRevision === options.sourceRevision &&
      descriptor.acceptanceDriver?.sourceRevision === options.sourceRevision &&
      descriptor.acceptanceDriver.cargoFeature === "acceptance-tests" &&
      descriptor.acceptanceDriver.cargoTarget ===
        "verisilo-acceptance-driver" &&
      sha256Pattern.test(descriptor.checksumManifestSha256);
    if (!descriptorMatches) {
      failures.push("candidate descriptor does not match the promotion input");
    }
  } catch {
    failures.push("verified candidate descriptor is absent or unreadable");
  }
  try {
    const receiptBytes = await readFile(options.acceptanceReceiptPath);
    acceptanceReceiptSha256 = createHash("sha256")
      .update(receiptBytes)
      .digest("hex");
    acceptanceReceipt = JSON.parse(receiptBytes.toString("utf8"));
    validateAcceptanceReceipt(acceptanceReceipt, options);
    acceptanceReceiptMatches = true;
  } catch (error) {
    failures.push(error.message ?? "desktop acceptance receipt is unreadable");
  }
  if (options.harnessOutcome !== "success") {
    failures.push(
      `RequireAll harness step outcome was ${options.harnessOutcome}`,
    );
  }
  try {
    const summaryBytes = await readFile(options.summaryPath);
    summarySha256 = createHash("sha256").update(summaryBytes).digest("hex");
    const parsed = JSON.parse(summaryBytes.toString("utf8"));
    results = Array.isArray(parsed) ? parsed : [parsed];
  } catch {
    failures.push("real E2E summary is absent or unreadable");
  }

  let counts = { PASS: 0, FAIL: 0, SKIP: 0, BLOCKED: 0 };
  try {
    counts = countStatuses(results);
  } catch (error) {
    failures.push(error.message);
  }
  if (results.length === 0) {
    failures.push("real E2E summary has no results");
  }
  if (counts.FAIL > 0 || counts.SKIP > 0 || counts.BLOCKED > 0) {
    failures.push(
      "RequireAll evidence contains FAIL, SKIP, or BLOCKED results",
    );
  }
  const requiredEvidence = requiredEvidenceNames(options.browser).map(
    (name) => {
      const matches = results.filter((entry) => entry?.name === name);
      return {
        name,
        status:
          matches.length === 1 && matches[0].status === "PASS"
            ? "PASS"
            : "MISSING_OR_NON_PASS",
      };
    },
  );
  if (requiredEvidence.some((entry) => entry.status !== "PASS")) {
    failures.push(
      "RequireAll summary is missing canonical real desktop/browser/Native Host evidence",
    );
  }
  const osEvidence = results.find(
    (entry) => entry?.name === "windows_matrix_target",
  );
  if (osEvidence?.status !== "PASS") {
    failures.push("the declared Windows matrix target did not pass");
  }
  const browserPrefix = `${options.browser}_`;
  const browserEvidence = results.filter((entry) =>
    entry?.name?.startsWith(browserPrefix),
  );
  if (
    browserEvidence.length === 0 ||
    browserEvidence.some((entry) => entry.status !== "PASS")
  ) {
    failures.push(
      `the ${options.browser} matrix has missing or non-PASS evidence`,
    );
  }

  const uniqueFailures = [...new Set(failures)];
  const attestation = {
    schema: "urn:verisilo:windows-promotion-attestation:2",
    schemaVersion: 2,
    generatedAt: new Date().toISOString().replace(".000Z", "Z"),
    repository: options.repository,
    artifactId: options.artifactId,
    candidateDigest: options.artifactSha256,
    sourceRevision: options.sourceRevision,
    matrix: {
      expectedOs: options.expectedOs,
      architecture: "x64",
      browser: options.browser,
    },
    gates: {
      sameRepositoryArtifactVerified:
        options.candidateOutcome === "success" && descriptorMatches,
      candidateDescriptorSha256: descriptorSha256,
      checksumManifestSha256: descriptor?.checksumManifestSha256 ?? null,
      acceptanceDriverReceiptVerified: acceptanceReceiptMatches,
      acceptanceDriverReceiptSha256: acceptanceReceiptSha256,
      acceptanceDriverSourceRevision:
        acceptanceReceipt?.driverBuild?.sourceRevision ?? null,
      requireAll: true,
      harnessStepOutcome: options.harnessOutcome,
      harnessSummarySha256: summarySha256,
      statusCounts: counts,
      osEvidence: osEvidence
        ? { status: osEvidence.status, detail: osEvidence.detail }
        : null,
      browserEvidence: browserEvidence.map((entry) => ({
        name: entry.name,
        status: entry.status,
      })),
      requiredEvidence,
    },
    result: uniqueFailures.length === 0 ? "PASS" : "FAIL",
    failureReasons: uniqueFailures,
  };
  await mkdir(path.dirname(options.outputPath), { recursive: true });
  await writeFile(options.outputPath, stableJson(attestation), "utf8");
  return attestation;
}

function validateInputs(options) {
  if (
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(options.repository) ||
    !Number.isSafeInteger(options.artifactId) ||
    options.artifactId < 1 ||
    !sha256Pattern.test(options.artifactSha256) ||
    /^0{64}$/u.test(options.artifactSha256) ||
    !revisionPattern.test(options.sourceRevision) ||
    !["Windows 10", "Windows 11"].includes(options.expectedOs) ||
    !["Chrome", "Edge"].includes(options.browser) ||
    !["success", "failure", "cancelled", "skipped"].includes(
      options.candidateOutcome,
    ) ||
    !["success", "failure", "cancelled", "skipped"].includes(
      options.harnessOutcome,
    )
  ) {
    throw new Error("Promotion attestation inputs are invalid.");
  }
}

async function selfTest() {
  const temporaryRoot = await mkdtemp(
    path.join(tmpdir(), "verisilo-promotion-attestation-"),
  );
  const descriptorPath = path.join(temporaryRoot, "descriptor.json");
  const summaryPath = path.join(temporaryRoot, "summary.json");
  const acceptanceReceiptPath = path.join(
    temporaryRoot,
    "desktop-acceptance.json",
  );
  const outputPath = path.join(temporaryRoot, "attestation.json");
  const options = {
    repository: "QianQIUlp/VeriSilo",
    artifactId: 123,
    artifactSha256: "a".repeat(64),
    sourceRevision: "b".repeat(40),
    expectedOs: "Windows 11",
    browser: "Chrome",
    candidateOutcome: "success",
    harnessOutcome: "success",
    descriptorPath,
    summaryPath,
    acceptanceReceiptPath,
    outputPath,
  };
  try {
    await writeFile(
      descriptorPath,
      stableJson({
        schema: "urn:verisilo:windows-promotion-candidate:1",
        schemaVersion: 1,
        repository: options.repository,
        artifactId: options.artifactId,
        artifactSha256: options.artifactSha256,
        sourceRevision: options.sourceRevision,
        checksumManifestSha256: "c".repeat(64),
        acceptanceDriver: {
          sourceRevision: options.sourceRevision,
          cargoFeature: "acceptance-tests",
          cargoTarget: "verisilo-acceptance-driver",
        },
      }),
    );
    await writeFile(
      summaryPath,
      stableJson(
        requiredEvidenceNames(options.browser).map((name) => ({
          name,
          status: "PASS",
          detail: "fixture",
        })),
      ),
    );
    await writeFile(
      acceptanceReceiptPath,
      stableJson({
        schema: "urn:verisilo:windows-acceptance-receipt:1",
        schemaVersion: 1,
        result: "PASS",
        candidate: {
          repository: options.repository,
          artifactId: options.artifactId,
          artifactSha256: options.artifactSha256,
          sourceRevision: options.sourceRevision,
        },
        driverBuild: {
          sourceRevision: options.sourceRevision,
          cargoFeature: "acceptance-tests",
          credentialTransport: "anonymous-stdin-pipe",
        },
        browser: {
          kind: options.browser,
          version: "fixture-browser-version",
          isolatedUserDataDir: true,
          companionState: "not_connected_no_extension_evidence",
        },
        safety: {
          osTemporaryRootValidated: true,
          randomSentinelValidated: true,
          productionRootsRefused: true,
          exactRuntimeTermination: true,
          unrelatedProcessSurvived: true,
          profilePreserved: true,
        },
        results: acceptanceResultNames(options.browser).map((name) => ({
          name,
          status: "PASS",
          detail: "fixture",
        })),
      }),
    );
    validateInputs(options);
    const pass = await writeAttestation(options);
    if (pass.result !== "PASS") {
      throw new Error(
        "Attestation self-test did not accept complete PASS evidence.",
      );
    }
    await writeFile(
      summaryPath,
      stableJson(
        requiredEvidenceNames(options.browser).map((name) => ({
          name,
          status:
            name === `${options.browser}_loopback_proxy_fail_closed`
              ? "SKIP"
              : "PASS",
          detail: "fixture",
        })),
      ),
    );
    const failed = await writeAttestation(options);
    if (failed.result !== "FAIL" || failed.gates.statusCounts.SKIP !== 1) {
      throw new Error(
        "Attestation self-test accepted missing browser evidence.",
      );
    }
    await writeFile(
      summaryPath,
      stableJson(
        requiredEvidenceNames(options.browser).map((name) => ({
          name,
          status: "PASS",
          detail: "fixture",
        })),
      ),
    );
    const mismatchedReceipt = JSON.parse(
      await readFile(acceptanceReceiptPath, "utf8"),
    );
    mismatchedReceipt.candidate.sourceRevision = "d".repeat(40);
    await writeFile(acceptanceReceiptPath, stableJson(mismatchedReceipt));
    const mismatched = await writeAttestation(options);
    if (
      mismatched.result !== "FAIL" ||
      mismatched.gates.acceptanceDriverReceiptVerified !== false
    ) {
      throw new Error(
        "Attestation self-test accepted a source-mismatched driver receipt.",
      );
    }
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  process.stdout.write(
    "Windows promotion attestation self-test passed (SKIP was preserved as FAIL).\n",
  );
}

if (process.argv.includes("--self-test")) {
  await selfTest();
} else if (process.argv.includes("--enforce")) {
  const value = argument("--enforce");
  if (value === undefined) {
    throw new Error("Usage: --enforce <attestation.json>.");
  }
  const attestation = JSON.parse(await readFile(path.resolve(value), "utf8"));
  if (
    attestation?.schema !== "urn:verisilo:windows-promotion-attestation:2" ||
    attestation.schemaVersion !== 2 ||
    attestation.result !== "PASS" ||
    !Number.isSafeInteger(attestation.artifactId) ||
    attestation.artifactId < 1 ||
    !sha256Pattern.test(attestation.candidateDigest) ||
    /^0{64}$/u.test(attestation.candidateDigest) ||
    !revisionPattern.test(attestation.sourceRevision) ||
    attestation.gates?.sameRepositoryArtifactVerified !== true ||
    attestation.gates.acceptanceDriverReceiptVerified !== true ||
    !sha256Pattern.test(attestation.gates.acceptanceDriverReceiptSha256) ||
    attestation.gates.acceptanceDriverSourceRevision !==
      attestation.sourceRevision ||
    attestation.gates.requireAll !== true ||
    attestation.gates.harnessStepOutcome !== "success" ||
    attestation.gates.statusCounts?.FAIL !== 0 ||
    attestation.gates.statusCounts.SKIP !== 0 ||
    attestation.gates.statusCounts.BLOCKED !== 0 ||
    !Array.isArray(attestation.gates.requiredEvidence) ||
    attestation.gates.requiredEvidence.length === 0 ||
    attestation.gates.requiredEvidence.some(
      (entry) => entry?.status !== "PASS",
    ) ||
    !Array.isArray(attestation.failureReasons) ||
    attestation.failureReasons.length !== 0
  ) {
    throw new Error(
      "Windows promotion is denied by the machine-readable attestation.",
    );
  }
  process.stdout.write(
    "Windows promotion attestation permits this exact matrix cell.\n",
  );
} else {
  const artifactIdValue = argument("--artifact-id");
  const options = {
    repository: argument("--repository"),
    artifactId: Number(artifactIdValue),
    artifactSha256: argument("--artifact-sha256"),
    sourceRevision: argument("--source-revision"),
    expectedOs: argument("--expected-os"),
    browser: argument("--browser"),
    candidateOutcome: argument("--candidate-outcome"),
    harnessOutcome: argument("--harness-outcome"),
    descriptorPath: path.resolve(argument("--descriptor") ?? ""),
    summaryPath: path.resolve(argument("--summary") ?? ""),
    acceptanceReceiptPath: path.resolve(argument("--acceptance-receipt") ?? ""),
    outputPath: path.resolve(argument("--out") ?? ""),
  };
  if (!/^[1-9][0-9]{0,15}$/u.test(artifactIdValue ?? "")) {
    throw new Error("Promotion attestation artifact ID is invalid.");
  }
  validateInputs(options);
  const attestation = await writeAttestation(options);
  process.stdout.write(
    `Wrote ${attestation.result} Windows promotion attestation for ${options.expectedOs}/${options.browser}.\n`,
  );
}
