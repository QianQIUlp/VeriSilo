import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (relativePath) => readFile(path.join(root, relativePath), "utf8");

const [
  cargo,
  binary,
  listener,
  service,
  auth,
  agent,
  durable,
  provider,
  deployment,
  exampleRaw,
] = await Promise.all([
  read("crates/verisilo-remote-backend/Cargo.toml"),
  read("crates/verisilo-remote-backend/src/bin/verisilo-remote-agent.rs"),
  read("crates/verisilo-remote-backend/src/https_server.rs"),
  read("crates/verisilo-remote-backend/src/agent_service.rs"),
  read("crates/verisilo-remote-backend/src/auth_store.rs"),
  read("crates/verisilo-remote-backend/src/agent.rs"),
  read("crates/verisilo-remote-backend/src/durable_store.rs"),
  read("crates/verisilo-remote-backend/src/provider_bridge.rs"),
  read("crates/verisilo-remote-backend/DEPLOYMENT.md"),
  read("crates/verisilo-remote-backend/verisilo-remote-agent.example.json"),
]);

assert.match(
  cargo,
  /reqwest = \{ version = "[^"]+", default-features = false, features = \["blocking", "rustls"\] \}/u,
);
assert.match(cargo, /^rustls = "[^"]+"$/mu);
assert.doesNotMatch(cargo, /native-tls|openssl/u);
const agentVersion = cargo.match(/^version = "([^"]+)"$/mu)?.[1];
assert.match(agentVersion ?? "", /^\d+\.\d+\.\d+$/u);
if (process.env.VERISILO_REMOTE_AGENT_VERSION !== undefined) {
  assert.equal(agentVersion, process.env.VERISILO_REMOTE_AGENT_VERSION);
}

for (const invariant of [
  /#\[cfg\(not\(unix\)\)\][\s\S]+ExitCode::FAILURE/u,
  /std::io::stdout\(\)\.is_terminal\(\)/u,
  /OperatorCommand::InitToken/u,
  /OperatorCommand::RevokeAll/u,
  /OperatorCommand::Serve/u,
  /pairing_token=\{\}/u,
]) {
  assert.match(binary, invariant);
}
assert.doesNotMatch(binary, /https?:\/\/[A-Za-z0-9]/u);

for (const invariant of [
  /TcpListener::bind\(configuration\.listen_address\)/u,
  /ServerConfig::builder\(\)[\s\S]+with_single_cert/u,
  /alpn_protocols = vec!\[b"http\/1\.1"\.to_vec\(\)\]/u,
  /request\.method != Some\("POST"\)/u,
  /request\.path != Some\("\/"\)/u,
  /content-type[\s\S]+application\/json/u,
  /x-verisilo-protocol/u,
  /"transfer-encoding" \| "upgrade" \| "expect"/u,
  /MAX_MESSAGE_BYTES/u,
  /require_safe_config_permissions/u,
  /require_private_permissions/u,
  /valid_absolute_path/u,
  /reject_symlink/u,
  /handler\.maintenance_tick\(\)/u,
]) {
  assert.match(listener, invariant);
}
assert.doesNotMatch(listener, /TcpStream::connect|http:\/\//u);

for (const invariant of [
  /strict_json::<PairingRequestEnvelope>/u,
  /strict_json::<AuthenticatedWireRequest>/u,
  /redeem_pairing_token/u,
  /authenticate_operation/u,
  /TlsPinRotationAuthorizationRequestEnvelope/u,
  /authorize_pin_rotation/u,
  /credential_id: authenticated\.credential_id/u,
  /stored_record\.as_ref\(\)/u,
  /record\.deletion_proof_id != Some\(proof\.proof_id\)/u,
  /last_activity_at_unix_ms/u,
]) {
  assert.match(service, invariant);
}
assert.doesNotMatch(service, /println!|eprintln!|dbg!/u);
assert.doesNotMatch(
  service,
  /proof\.reason\s*!=|proof\.deleted_at_unix_ms\.abs_diff/u,
);

for (const invariant of [
  /DeletionResourceKind::ComputeInstance/u,
  /DeletionResourceKind::PersistentVolume/u,
  /DeletionResourceKind::Snapshot/u,
  /DeletionResourceKind::EphemeralKey/u,
  /DeletionResourceStatus::NotApplicable/u,
  /deletion_resources_are_bound/u,
  /record\.state == EnvironmentState::Deleted[\s\S]+if !confirm_destroy/u,
]) {
  assert.match(agent, invariant);
}
assert.doesNotMatch(agent, /resources_deleted|volume_key_destroyed/u);

for (const invariant of [
  /PAIRING_HASH_DOMAIN/u,
  /CREDENTIAL_HASH_DOMAIN/u,
  /digest_sha256/u,
  /Zeroizing<String>/u,
  /flock\(file\.as_raw_fd\(\), LOCK_EX \| LOCK_NB\)/u,
  /set_mode_0600/u,
  /file\.sync_all\(\)/u,
  /sync_parent/u,
  /last_request_sequence/u,
  /pin_rotation_authorizations/u,
  /redeem_pairing_token_for_rotation/u,
]) {
  assert.match(auth, invariant);
}
assert.doesNotMatch(auth, /pub\s+secret:\s+String/u);

for (const invariant of [
  /file\.sync_all\(\)/u,
  /fs::rename/u,
  /sync_parent/u,
  /deletion_proof/u,
  /sequences: HashMap<Uuid, u64>/u,
]) {
  assert.match(durable, invariant);
}

for (const invariant of [
  /Command::new\(&self\.executable_path\)/u,
  /\.arg\("--verisilo-provider-v1"\)/u,
  /sha256_file/u,
  /constant_time_eq/u,
  /MAX_MESSAGE_BYTES/u,
  /deny_unknown_fields/u,
  /resource_deletions/u,
]) {
  assert.match(provider, invariant);
}
assert.doesNotMatch(
  provider,
  /(?:Command::new\([^)]*(?:cmd|powershell|bash|sh)\b)|\.args\(|\b(?:eval|Invoke-Expression)\b/iu,
);

const example = JSON.parse(exampleRaw);
assert.equal(example.listenAddress, "0.0.0.0:8443");
assert.equal(example.provider?.mode, "unavailable");
assert.ok(Array.isArray(example.provider?.capabilities));
assert.deepEqual(
  new Set(example.provider.capabilities.map((item) => item.operation)),
  new Set([
    "create",
    "start",
    "stop",
    "pause",
    "snapshot",
    "destroy",
    "configureNetwork",
    "health",
    "logs",
  ]),
);
assert.ok(
  example.provider.capabilities.every(
    (item) =>
      item.availability?.availability === "unavailable" &&
      typeof item.availability.reason === "string" &&
      item.availability.reason.length > 0,
  ),
);
for (const key of [
  "tlsCertificateChainPath",
  "tlsPrivateKeyPath",
  "authStatePath",
  "agentStatePath",
]) {
  assert.equal(path.posix.isAbsolute(example[key]), true);
}
assert.doesNotMatch(
  exampleRaw,
  /subscription|password|bearer|secret|https?:\/\//iu,
);

for (const boundary of [
  "does not simulate a VM, browser",
  "no shell, caller path, arbitrary argument, or remote URL",
  "does not include\\s+or claim an implemented media stream",
  "operator-owned\\s+connection admission/rate limiting",
]) {
  assert.match(deployment, new RegExp(boundary, "u"));
}

process.stdout.write(
  "Remote Agent static source invariants passed; no TLS endpoint, provider, VM, browser, or media stream was exercised.\n",
);
