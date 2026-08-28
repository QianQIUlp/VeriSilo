#!/usr/bin/env python3
"""FP3-1b native-Windows Host adapter for the exact Formal-v3 candidate.

This is an evidence-only Host entrypoint.  It keeps the production Host
protocol intact, binds its asset hooks to the already-qualified Formal-v3
runtime tree, and appends one bounded network observation to ``observed.json``.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit


REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

import browser_tree
import host_v1
import run_spike


FP2_RESULT = (
    HOST_DIR
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp2-result.json"
)
ASSET_LOCK = (
    REPO_ROOT
    / "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-runtime-asset-lock.json"
)
TREE_MANIFEST = (
    REPO_ROOT
    / "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-browser-tree-manifest.json"
)
BROWSER_ROOT = REPO_ROOT / "artifacts/camoufox-fp2-formal-r1-attempt-8/browser"
EXECUTABLE = BROWSER_ROOT / "camoufox.exe"

FP2_RESULT_SHA256 = "caa5ed4005c3e9c392c76a5d264d3d7d4d30cb741ac675fd27803c7f5fa06fa6"
ASSET_LOCK_SHA256 = "81e73a69347272d0b770bfa3c9b3eb07449bb165efb0c16948eece2e5a0678ce"
TREE_MANIFEST_SHA256 = "8434ab9925bf0f7d95cc4ff06fe94b7dcf9963a0691f37638469d68cda58ace2"
TREE_MANIFEST_CANONICAL_SHA256 = (
    "68d78d0f414d90545691560858b46ed179ee163b7258306c44f0d850bcde6204"
)
ARCHIVE_SHA256 = "032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5"
EXECUTABLE_SHA256 = "b147602826db5bf852e5777f56cd56036dc04e8ea8868a8e55f8b08744f142a6"
ARCHIVE_SIZE = 493_493_005
TREE_FILE_COUNT = 503
TREE_TOTAL_BYTES = 982_405_560
RELEASE = "v152.0.4-beta.28"
ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3"
RUNTIME_LOCK_SCHEMA = "verisilo-camoufox-fp1-r1-runtime-asset/v1"
STUN_URL = "stun:stun.cloudflare.com:3478"
IP_OBSERVATION_URL = "https://api.country.is/"

_LOCK_VIEW: dict[str, Any] | None = None
_TREE_RECEIPT: dict[str, Any] | None = None


class FP3HostError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def strict_json(path: Path) -> dict[str, Any]:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise FP3HostError(f"duplicate JSON key in {path.name}: {key}")
            result[key] = value
        return result

    try:
        value = json.loads(
            path.read_bytes().decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=lambda token: (_ for _ in ()).throw(
                FP3HostError(f"invalid JSON constant in {path.name}: {token}")
            ),
        )
    except FP3HostError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise FP3HostError(f"invalid JSON {path}: {type(exc).__name__}") from exc
    if type(value) is not dict:
        raise FP3HostError(f"JSON root is not an object: {path}")
    return value


def require_file(path: Path, expected_sha256: str) -> None:
    if not path.is_file() or path.is_symlink():
        raise FP3HostError(f"missing or irregular file: {path}")
    if sha256_file(path) != expected_sha256:
        raise FP3HostError(f"SHA-256 mismatch: {path}")


def canonical_json_sha256(value: object) -> str:
    payload = json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def validate_formal_inputs() -> dict[str, Any]:
    require_file(FP2_RESULT, FP2_RESULT_SHA256)
    result = strict_json(FP2_RESULT)
    candidate = result.get("formalCandidate")
    if (
        result.get("status") != "passed"
        or result.get("verified") is not False
        or type(candidate) is not dict
        or candidate.get("engineRevision") != ENGINE_REVISION
        or candidate.get("archive")
        != {"sha256": ARCHIVE_SHA256, "sizeBytes": ARCHIVE_SIZE}
        or candidate.get("executable", {}).get("sha256") != EXECUTABLE_SHA256
        or candidate.get("runtimeAssetLock")
        != {
            "path": "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-runtime-asset-lock.json",
            "sha256": ASSET_LOCK_SHA256,
        }
        or candidate.get("runtimeTree")
        != {
            "path": "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-browser-tree-manifest.json",
            "rawSha256": TREE_MANIFEST_SHA256,
            "canonicalSha256": TREE_MANIFEST_CANONICAL_SHA256,
            "fileCount": TREE_FILE_COUNT,
            "totalBytes": TREE_TOTAL_BYTES,
        }
    ):
        raise FP3HostError("accepted FP2 Formal-v3 binding is not exact")

    require_file(ASSET_LOCK, ASSET_LOCK_SHA256)
    lock = strict_json(ASSET_LOCK)
    archive = lock.get("archive")
    tree = lock.get("runtimeTree")
    if (
        lock.get("schema") != RUNTIME_LOCK_SCHEMA
        or lock.get("assetKind") != "self-built"
        or lock.get("verified") is not False
        or lock.get("evidenceClass") != "compiled-not-runtime-verified"
        or lock.get("release") != RELEASE
        or lock.get("platform") != "windows-x86_64"
        or lock.get("engineRevision") != ENGINE_REVISION
        or type(archive) is not dict
        or archive.get("sha256") != ARCHIVE_SHA256
        or archive.get("sizeBytes") != ARCHIVE_SIZE
        or archive.get("camoufoxExeSha256") != EXECUTABLE_SHA256
        or archive.get("executableRelativePath") != "camoufox.exe"
        or type(tree) is not dict
        or tree.get("root")
        != "artifacts/camoufox-fp2-formal-r1-attempt-8/browser"
        or tree.get("manifest", {}).get("path")
        != "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-browser-tree-manifest.json"
        or tree.get("manifest", {}).get("sha256") != TREE_MANIFEST_SHA256
        or tree.get("manifest", {}).get("canonicalSha256")
        != TREE_MANIFEST_CANONICAL_SHA256
        or tree.get("fileCount") != TREE_FILE_COUNT
        or tree.get("totalBytes") != TREE_TOTAL_BYTES
    ):
        raise FP3HostError("Formal-v3 runtime asset lock is not exact")
    return {
        **lock,
        "sha256": ARCHIVE_SHA256,
        "sizeBytes": ARCHIVE_SIZE,
        "executableRelativePath": "camoufox.exe",
    }


def verify_runtime_tree_once() -> dict[str, Any]:
    global _TREE_RECEIPT
    if _TREE_RECEIPT is not None:
        return _TREE_RECEIPT
    require_file(TREE_MANIFEST, TREE_MANIFEST_SHA256)
    manifest = browser_tree.load_tree_manifest(TREE_MANIFEST)
    if (
        manifest.get("treeRootLabel") != "browser"
        or manifest.get("fileCount") != TREE_FILE_COUNT
        or manifest.get("totalBytes") != TREE_TOTAL_BYTES
        or canonical_json_sha256(manifest) != TREE_MANIFEST_CANONICAL_SHA256
    ):
        raise FP3HostError("Formal-v3 runtime tree manifest is not exact")
    if BROWSER_ROOT.resolve(strict=True) != BROWSER_ROOT:
        raise FP3HostError("Formal-v3 browser root is not canonical")
    receipt = browser_tree.verify_tree(BROWSER_ROOT, manifest)
    require_file(EXECUTABLE, EXECUTABLE_SHA256)
    _TREE_RECEIPT = receipt
    return receipt


def exact_path(value: Path | str | None, expected: Path, label: str) -> Path:
    if value is None:
        raise FP3HostError(f"{label} is required")
    selected = Path(value).resolve(strict=True)
    if selected != expected.resolve(strict=True):
        raise FP3HostError(f"{label} is not the frozen FP3 input")
    return selected


def resolve_asset_lock_path(path: Path | str | None = None) -> Path:
    return exact_path(path, ASSET_LOCK, "asset lock")


def load_asset_lock(path: Path | str | None = None) -> dict[str, Any]:
    global _LOCK_VIEW
    resolve_asset_lock_path(path)
    if _LOCK_VIEW is None:
        _LOCK_VIEW = validate_formal_inputs()
    return _LOCK_VIEW


def asset_kind(lock: dict[str, Any]) -> str:
    if _LOCK_VIEW is None or lock != _LOCK_VIEW:
        raise FP3HostError("non-FP3 asset lock reached the Host adapter")
    return "self-built"


def ensure_browser_asset(
    lock: dict[str, Any],
    allow_download: bool = True,
    *,
    browser_root: Path | str | None = None,
    tree_manifest: Path | str | None = None,
    verify_tree_contents: bool = True,
) -> Path:
    if allow_download or not verify_tree_contents:
        raise FP3HostError("Formal-v3 FP3 runtime requires one exact local tree check")
    asset_kind(lock)
    exact_path(browser_root, BROWSER_ROOT, "browser root")
    exact_path(tree_manifest, TREE_MANIFEST, "tree manifest")
    verify_runtime_tree_once()
    return EXECUTABLE


def verify_self_built_browser_root(
    lock: dict[str, Any],
    browser_root: Path | str,
    *,
    repo_root: Path | str,
    tree_manifest_path: Path | str | None = None,
    verify_tree_contents: bool = True,
) -> tuple[Path, dict[str, Any]]:
    asset_kind(lock)
    if Path(repo_root).resolve(strict=True) != REPO_ROOT:
        raise FP3HostError("repository root differs from the frozen FP3 input")
    exact_path(browser_root, BROWSER_ROOT, "browser root")
    exact_path(tree_manifest_path, TREE_MANIFEST, "tree manifest")
    return EXECUTABLE, verify_runtime_tree_once()


PUBLIC_EXIT_SCRIPT = r"""
async ({ipUrl}) => {
  const response = await fetch(ipUrl, {cache: "no-store", credentials: "omit"});
  const payload = await response.json();
  return {
    success:
      response.ok &&
      typeof payload?.ip === "string" &&
      typeof payload?.country === "string",
    httpStatus: response.status,
    ip: typeof payload?.ip === "string" ? payload.ip : null,
    countryCode:
      typeof payload?.country === "string" ? payload.country : null,
  };
}
"""

GEOLOCATION_SCRIPT = r"""
({timeoutMs}) => new Promise((resolve) => {
  if (!navigator.geolocation) {
    resolve({status: "unavailable"});
    return;
  }
  navigator.geolocation.getCurrentPosition(
    (position) => resolve({
      status: "observed",
      latitude: position.coords.latitude,
      longitude: position.coords.longitude,
      accuracy: position.coords.accuracy,
      timestamp: position.timestamp,
    }),
    (error) => resolve({
      status: "failed",
      code: error.code,
      message: String(error.message ?? "").slice(0, 120),
    }),
    {enableHighAccuracy: false, maximumAge: 0, timeout: timeoutMs},
  );
})
"""

ICE_SCRIPT = r"""
({stunUrl, timeoutMs}) => new Promise((resolve) => {
  const candidates = [];
  let settled = false;
  let timer = null;
  const finish = (completed, timedOut, error = null) => {
    if (settled) return;
    settled = true;
    if (timer !== null) clearTimeout(timer);
    const result = {
      completed,
      timedOut,
      candidateCount: candidates.length,
      candidates,
    };
    if (error) result.error = error;
    resolve(result);
  };
  timer = setTimeout(() => finish(false, true, "TimeoutError"), timeoutMs);
  try {
    const peer = new RTCPeerConnection({iceServers: [{urls: [stunUrl]}]});
    peer.createDataChannel("fp3");
    peer.addEventListener("icecandidate", (event) => {
      if (!event.candidate) {
        finish(true, false);
        return;
      }
      const candidate = event.candidate;
      const raw = candidate.candidate ?? "";
      const parts = raw.trim().split(/\s+/u);
      candidates.push({
        candidate: raw,
        address:
          typeof candidate.address === "string"
            ? candidate.address
            : (parts.length > 4 ? parts[4] : null),
        candidateType:
          typeof candidate.type === "string"
            ? candidate.type
            : (parts[6] === "typ" ? parts[7] : null),
        protocol:
          typeof candidate.protocol === "string"
            ? candidate.protocol
            : (parts.length > 2 ? parts[2].toLowerCase() : null),
        port: Number.isInteger(candidate.port)
          ? candidate.port
          : (parts.length > 5 ? Number(parts[5]) : null),
      });
    });
    peer.addEventListener("icegatheringstatechange", () => {
      if (peer.iceGatheringState === "complete") finish(true, false);
    });
    peer.createOffer()
      .then((offer) => peer.setLocalDescription(offer))
      .catch((error) => finish(false, false, error?.name ?? "Error"));
  } catch (error) {
    finish(false, false, error?.name ?? "Error");
  }
})
"""

STAGE_SETUP_TIMEOUT_SECONDS = 8.0
STAGE_CLOSE_TIMEOUT_SECONDS = 2.0
STAGE_EVALUATION_TIMEOUTS = {
    "publicExit": 20.0,
    "geolocation": 18.0,
    "ice": 12.0,
}


def persist_observation(
    observed_path: Path, payload: dict[str, Any], observation: dict[str, Any]
) -> None:
    payload["fp3NetworkObservation"] = observation
    raw = (json.dumps(payload, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    temporary = observed_path.with_name("observed.fp3.tmp")
    if temporary.exists():
        raise FP3HostError("stale FP3 observation temporary file")
    temporary.write_bytes(raw)
    os.replace(temporary, observed_path)


async def observe_stage(
    context: Any,
    probe_url: str,
    script: str,
    argument: dict[str, Any],
    *,
    setup_timeout: float,
    evaluation_timeout: float,
    close_timeout: float,
) -> dict[str, Any]:
    stage_page = None

    async def prepare_page() -> None:
        nonlocal stage_page
        stage_page = await context.new_page()
        await stage_page.goto(
            probe_url,
            wait_until="domcontentloaded",
            timeout=int(setup_timeout * 1000),
        )

    try:
        await asyncio.wait_for(prepare_page(), timeout=setup_timeout)
        value = await asyncio.wait_for(
            stage_page.evaluate(script, argument), timeout=evaluation_timeout
        )
        if type(value) is not dict:
            raise FP3HostError("FP3 observation stage returned a non-object")
        result: dict[str, Any] = {"status": "observed", "value": value}
    except Exception as exc:  # noqa: BLE001 - only the bounded type enters evidence
        result = {"status": "failed", "errorType": type(exc).__name__[:64]}
    if stage_page is None:
        result["pageClose"] = {"status": "not_present"}
    else:
        result["pageClose"] = (
            await host_v1.close_context_bounded(stage_page, close_timeout)
        ).as_dict()
    return result


async def collect_network_observation(
    context: Any,
    probe_url: str,
    observed_path: Path,
    payload: dict[str, Any],
    permission: dict[str, Any],
    stun_url: str,
    *,
    setup_timeout: float = STAGE_SETUP_TIMEOUT_SECONDS,
    evaluation_timeouts: dict[str, float] | None = None,
    close_timeout: float = STAGE_CLOSE_TIMEOUT_SECONDS,
) -> dict[str, Any]:
    base = payload.get("observedFull")
    if type(base) is not dict or type(base.get("session")) is not dict:
        raise FP3HostError("base Host observation is unavailable")
    timeouts = evaluation_timeouts or STAGE_EVALUATION_TIMEOUTS
    observation: dict[str, Any] = {
        "status": "completed",
        "observedAtUtc": run_spike.utcnow(),
        "ipObservationUrl": IP_OBSERVATION_URL,
        "stunUrl": stun_url,
        "timezone": base["session"].get("timezone"),
        "locale": base.get("language"),
        "languages": base.get("languages"),
        "publicExit": None,
        "geolocation": None,
        "ice": None,
        "stages": {},
        "errors": [],
        "geolocationPermission": permission,
        "verified": False,
    }
    persist_observation(observed_path, payload, observation)
    specs = (
        ("publicExit", PUBLIC_EXIT_SCRIPT, {"ipUrl": IP_OBSERVATION_URL}),
        (
            "geolocation",
            GEOLOCATION_SCRIPT,
            {"timeoutMs": 15_000},
        ),
        (
            "ice",
            ICE_SCRIPT,
            {"stunUrl": stun_url, "timeoutMs": 10_000},
        ),
    )
    for name, script, argument in specs:
        stage = await observe_stage(
            context,
            probe_url,
            script,
            argument,
            setup_timeout=setup_timeout,
            evaluation_timeout=timeouts[name],
            close_timeout=close_timeout,
        )
        value = stage.pop("value", None)
        observation["stages"][name] = stage
        observation[name] = value
        if stage["status"] != "observed":
            observation["errors"].append(f"{name}:{stage.get('errorType', 'Error')}")
        elif name == "publicExit" and value.get("success") is not True:
            observation["errors"].append("publicExit:failed")
        elif name == "geolocation" and value.get("status") != "observed":
            observation["errors"].append(
                f"geolocation:{value.get('status', 'failed')}"
            )
        elif name == "ice" and (
            value.get("completed") is not True or value.get("candidateCount", 0) == 0
        ):
            observation["errors"].append("ice:incomplete")
        persist_observation(observed_path, payload, observation)
    return observation


class FP3ManagedHost(host_v1.CamoufoxHost):
    def _verify_browser_binding_for_launch(self, artifact: dict) -> None:
        if self.executable != EXECUTABLE or _TREE_RECEIPT is None:
            raise host_v1.ArtifactIntegrityError(
                "Formal-v3 runtime tree was not verified during Host preparation"
            )
        require_file(EXECUTABLE, EXECUTABLE_SHA256)
        host_v1.verify_browser_binding(
            artifact, self.lock, self.executable, host_v1.installed_versions()
        )

    async def _launch_browser(self, session: dict, artifact: dict) -> None:
        await super()._launch_browser(session, artifact)
        page = session.get("page")
        context = session.get("ctx")
        observed_path = Path(session["sessionDir"]) / "observed.json"
        payload = strict_json(observed_path)
        permission = {"status": "failed", "origin": None}
        try:
            if page is None or context is None:
                raise FP3HostError("live Host page/context is unavailable")
            parsed = urlsplit(page.url)
            if parsed.scheme != "http" or parsed.hostname != "127.0.0.1" or not parsed.port:
                raise FP3HostError("FP3 observation is not on the exact loopback probe origin")
            origin = f"http://127.0.0.1:{parsed.port}"
            await context.grant_permissions(["geolocation"], origin=origin)
            permission = {"status": "granted", "origin": origin}
            stun_url = os.environ.get("VERISILO_FP3_STUN_URL", STUN_URL)
            if stun_url != STUN_URL:
                raise FP3HostError("STUN URL differs from the frozen FP3 input")
            await collect_network_observation(
                context,
                page.url,
                observed_path,
                payload,
                permission,
                stun_url,
            )
        except Exception as exc:  # noqa: BLE001 - bounded type only enters evidence
            payload = strict_json(observed_path)
            existing = payload.get("fp3NetworkObservation")
            observation = existing if type(existing) is dict else {}
            observation.update(
                {
                    "status": "failed",
                    "errorType": type(exc).__name__[:64],
                    "geolocationPermission": permission,
                    "verified": False,
                }
            )
            persist_observation(observed_path, payload, observation)


def patch_host() -> None:
    host_v1.resolve_asset_lock_path = resolve_asset_lock_path
    host_v1.load_asset_lock = load_asset_lock
    host_v1.ensure_browser_asset = ensure_browser_asset
    host_v1.asset_kind = asset_kind
    host_v1.verify_self_built_browser_root = verify_self_built_browser_root
    host_v1.CamoufoxHost = FP3ManagedHost
    run_spike.resolve_asset_lock_path = resolve_asset_lock_path
    run_spike.load_asset_lock = load_asset_lock
    run_spike.asset_kind = asset_kind


def main() -> int:
    if os.name != "nt":
        raise FP3HostError("FP3-1b requires native Windows")
    if "--child-host" in sys.argv:
        sys.argv.remove("--child-host")
    patch_host()
    return host_v1.main()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FP3HostError as exc:
        raise SystemExit(f"FP3 Host adapter rejected input: {exc}") from exc
