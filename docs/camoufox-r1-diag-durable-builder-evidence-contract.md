# R1-diag Durable Builder Evidence & Rehydration Contract

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical builder contract**: the diagnostic build/provenance is closed, and this
> document is no longer a current execution constraint. Read it only when auditing the
> corresponding builder evidence. For the current route, see the
> [status page](camoufox-program-status.md).

- Status: **Accepted for implementation**
- Date formed: 2026-08-23
- Starting checkpoint: `f267bb4ff3f00115a37546bbe0649d0db889a7d3`
- Starting tree: `d923535f105169b40d3ae091b9b925c864a086d7`
- Execution boundary: offline / no-engine / no-browser

This document freezes the durable retention of the Phase B builder identity and the Docker
image rehydration contract. It does not change the R1 diagnostic engine recipe, the patch
series, or the runtime semantics, and it does not authorize Phase B-2, Phase C-2, Phase D,
Browser, or V1–V4.

## 1. History and current state

`r1diag-builder-20260823t0504z` legitimately produced the image
`sha256:f46ec076dcde9b3759007c3683c07e5a3c563f9145475b335b6f40a82bb6732c`, which was correctly
written into the v2 source lock at `f267bb4`. After the host was deallocated/rebooted, Azure
Temporary Storage was reformatted by cloud-init, and the Phase-B material evidence and the
Docker content store were permanently lost at the same time. Therefore:

```text
Phase B-1 / Phase C-1 identity history: Accepted and immutable
Phase C-1 operational consumability:   Superseded / unavailable
Current builder binding:                must return to unbound
```

`f267bb4` is neither deleted nor rewritten. The new checkpoint preserves this fact through Git
history and the operational lineage pointer inside the lock. It does not pass off the old
result, archive, or image as currently consumable material. The historically successful run
`r1diag-builder-20260823t0504z` and the earlier failed run `r1diag-builder-20260823t0435z` are
both permanently retired. A future Phase B-2 must refuse to reuse these two run IDs, even if it
sits on a brand-new scratch/durable filesystem.

## 2. v2 compatibility decision

The current Dockerfile, the embedded strict driver, and the diagnostic gate all read these
fixed values:

```text
camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json
schema verisilo-r1-diag-source-binding/v2
engineRevision verisilo-camoufox-152.0.4-beta.28-r1-diag-v2
```

This task forbids modifying these container recipe bytes. So no v3 execution schema that the
strict driver cannot consume is created. The new Phase-A checkpoint updates the
host-evidence contract of the current v2 lock, and keeps the Dockerfile, strict driver, diag
gate, engine revision, and the six patch bytes unchanged. The historical v2 binding is
preserved by `f267bb4` and by the lineage fields inside the lock.

## 3. Layers of scratch and durable evidence

```text
scratch root:
  /mnt/camoufox-build
  usable for rebuildable source/build/work/output
  may be lost on host restart

durable evidence root:
  /var/lib/verisilo/camoufox-build-evidence
  stores only the builder evidence bundle and the durable-root qualification
  must survive host restarts
```

Both paths are frozen as launcher constants. The launcher accepts no environment variable, no
arbitrary lock path, no arbitrary evidence path, and no CLI root override. The durable root
must be created in advance by the host and must allow the launcher to write to it. The
launcher must not run `chmod`, `chown`, or a privileged filesystem repair.

Before each use of the durable root, the following must be verified fail-closed:

1. The root is a real directory and not a symlink.
2. The `st_dev` of the root and of the scratch root differ.
3. Record the mount target/source, filesystem type, and UUID from `findmnt`.
4. The current mount identity exactly matches the reboot qualification.
5. The launcher can directly create, fsync, and re-read the evidence, without using `sudo` to
   repair file permissions.

## 4. Reboot qualification

Before Phase B-2, one versioned qualification must be completed:

```text
stage-durable-root-qualification
  → exclusive qualification directory
  → sentinel + request JSON
  → fsync file and directory

host reboot/deallocate/restart

verify-durable-root-qualification
  → boot ID must differ
  → mount identity must be the same
  → sentinel size/SHA must be the same
  → write an exclusive qualified-after-reboot result
  → fsync + re-read
```

The qualification ID may only select one direct child directory under the fixed root. It must
not become a path-injection channel. The Phase B `prepare-image` must explicitly bind one
qualification ID that already passed. A missing one, the same boot, mount drift, sentinel
drift, or result drift is rejected before the Docker build.

## 5. Durable Phase-B bundle

Each new Phase-B run corresponds to:

```text
/var/lib/verisilo/camoufox-build-evidence/<run-id>/
  retention-preflight.json
  builder-image.tar
  builder-image-result.json
  builder-image-inspect.json
  builder-build-context.tar
  buildx.log
  buildx-metadata.json
  docker-save.log
  durable-manifest.json
  retention-receipt.json
```

Before any Docker build, the launcher must create the bundle directory exclusively with a new
run-id, write and fsync/re-read `retention-preflight.json`, bind the qualification result and
the current mount identity, and prove that the launcher can still directly
create/write/fsync/read. If the reservation fails or the directory already exists, it must not
enter Docker. After a successful reservation, the reservation is kept permanently even if
later steps fail, and a retry under the same run-id is forbidden.

The other seven payload files must be copied from the scratch provenance with exclusive create.
The launcher writes each target file, flushes it, runs `fsync`, closes it, and then re-hashes
and re-stats it. If the bundle directory pre-exists (except for the exact preflight reservation
that this launcher just created and verified), reject: do not overwrite, do not delete, and do
not retry under the same run-id. If a copy fails, the partial bundle may be kept, but no
accepted manifest may be generated.

`durable-manifest.json` is written after the seven payloads and contains at least:

- schema, runId, writtenAt, `retained=true`, `fsyncCompleted=true`;
- source commit/tree/lock, imageId, binding proposal canonical SHA;
- the durable qualification ID/result SHA and the mount identity;
- the exact basename, size, and SHA-256 of the preflight and of the other seven payload files;
- the manifest canonical SHA-256.

The input of the manifest canonical hash is defined as "the compact/sorted-key UTF-8 JSON
object after removing the `manifestCanonicalSha256` field". The manifest itself is not listed
in files, to avoid self-reference. After the manifest is written and fsynced, the launcher
must reopen from the durable root and fully verify the payload, the manifest, the
qualification, and the image identity. Only after that re-read passes is the final
exclusive+fsync `retention-receipt.json` written. The receipt binds at least the manifest
raw/canonical SHA, the builder result SHA, the proposal canonical SHA, the qualification result
SHA, and the source lineage, and declares `retained=true / reReadable=true`. The receipt
itself uses the same canonical JSON rule after removing `receiptCanonicalSha256`, and after it
is written it must also be fully re-read again.

The manifest is the payload commit marker. The receipt is the transaction marker that "the
payload has been re-read and closed from the durable root". If the manifest exists but the
receipt is missing, drifted, or cannot be re-read, both Phase B and Phase C must reject, so
that "ready to re-read" is not misreported as "already re-read".

`builder-image-result.json` is generated before retention, with the status
`prepared-awaiting-durable-retention`. It does not by itself represent an accepted Phase B. The
`durably-retained-reread-verified-awaiting-source-lock-binding` status of
`retention-receipt.json` is the transaction boundary that Phase C-2 can accept.

After Phase B obtains the immutable config digest from the buildx metadata and completes the
exact inspect, `docker image save` must save by that ID. It must not keep saving by the mutable
tag. The launcher must also parse the `manifest.json` and the config member of the Docker
archive and verify that the SHA-256 of the config bytes is exactly the proposal image ID. A
successful save command alone cannot prove that the tar has the same identity as the inspected
image.

BuildKit must not keep reading a mutable live checkout directory after the recipe is verified.
The launcher must freeze the locked bytes of the Dockerfile, the strict driver, the diag gate,
and the host launcher into a deterministic `builder-build-context.tar`, and use the same
`O_NOFOLLOW` FD for the pre-hash, the buildx binary stdin, and the post-hash. This tar enters
the durable bundle, and the Phase C evidence binds its SHA/size. After buildx completes, the
image identity is taken from the immutable `containerimage.config.digest` of the metadata, and
then inspected by that digest. The inspect ID and the four frozen recipe-source labels must all
match. The mutable tag is used only for the local `--load` naming and must not become the
inspect/save/binding authority.

## 6. Bound evidence and rehydration

A future Phase C-2 must freeze all of the following together:

- the exact `builderImageBinding`;
- the builder result SHA;
- the binding proposal canonical SHA;
- the durable manifest raw/canonical SHA;
- the retention receipt raw/canonical SHA and `reReadable=true`;
- the deterministic build-context tar SHA/size;
- the durable qualification ID/result SHA;
- `retained=true` and the source commit/tree/lock lineage;
- `builderOperationalLineage.current` must advance atomically from the exact unbound state to
  `bindingState=bound / durableEvidence=retained-and-reread / phaseB2=accepted /
reasonCodes=[]`. It must not leave the contradiction "the top level is bound while the
  lineage is still unbound".

`prepare-bound-image` no longer consumes an arbitrary `--source-run-root`. It reads the bundle
from the fixed durable root only, by source run ID. It first fully verifies the
manifest/files/result/binding, and then checks the Docker store:

```text
bound image ID already exists and the inspect ID is exactly equal
  → action = already-present

bound image ID does not exist
  → docker image load (the verified tar as binary stdin)
  → the load exit must be 0
  → inspect the immutable image ID again
  → the loaded ID must equal the lock binding
```

Rehydration only deserializes the already-frozen image archive. It is not a new build. The
following are forbidden: network pull, `docker build`, tag authority, shell redirection,
`--entrypoint`, driver injection, and manual filesystem repair. The load output does not
constitute identity. The immutable inspect ID after the load is what constitutes the Gate.

The load path must open one regular-file archive FD with `O_NOFOLLOW` (where the platform
supports it). The pre-load hash/fstat, the Docker stdin, and the post-load hash/fstat use that
same open FD, to avoid the TOCTOU of "verifying one path and loading other bytes". Any
inode/size/hash drift is rejected.

`build-engine` accepts only a preparation record whose status is
`prepared-from-durable-builder-binding`, with `retained=true`, and whose proposal/manifest
exactly equal the bound lock. This record is not an independent authority: the engine path
must, by its source run ID, re-read the manifest, the receipt, and all payloads from the fixed
durable root again and compare them exactly with the bound lock. Only then, before
`docker run`, does it inspect the exact image ID again. If the image is lost between prepare
and build, the build fails closed and does not implicitly load/pull/build in the build path.
`docker run` must explicitly carry `--pull=never` and must not rely on Docker's implicit
resolution behavior for an image-ID reference.

All Docker invocations use the frozen absolute executable path and a minimal sanitized
environment. `DOCKER_HOST`, `DOCKER_CONTEXT`, proxy, an arbitrary `PATH`, or an inherited
environment must not change the daemon, the network, or the command resolution. Evidence JSON
must reject duplicate keys. An exclusive JSON write must use atomic exclusive-create semantics
and must not use the TOCTOU pattern of an ordinary overwrite write after `exists()`.

## 7. No-browser regressions that must be covered

Cover at least:

1. A durable root on the same filesystem as scratch, a symlink, or mount drift is rejected.
2. A qualification with the same boot, sentinel drift, or result drift is rejected.
3. When the durable reservation/preflight fails before Docker, the build does not start. When
   the ephemeral part succeeds but any step of the durable copy/fsync/re-read/receipt fails,
   Phase B fails.
4. A manifest with a missing file, an unknown file, path traversal, or hash/size drift is
   rejected.
5. A pre-existing bundle is not overwritten. A partial bundle, or one with only a manifest and
   no receipt, is not acceptable.
6. Re-read from the durable root after copy/fsync. It can still be verified after a simulated
   reboot.
7. Image absent + valid tar goes into load. Already present + exact ID does not load.
8. Tar drift is rejected before load. A nonzero load exit or a loaded ID mismatch is rejected.
9. The rehydration path has no chmod/chown, pull, build, or tag trust.
10. Phase C/Phase D consume only evidence with `retained=true / reReadable=true` that is
    manifest+receipt-bound.
11. The historical `f46ec076…`/run exists only in the superseded lineage and does not become
    the current binding.
12. The bytes of the Dockerfile, the strict driver, the diag gate, and 0000–0004/9000 stay
    unchanged.
13. A tag redirected after inspect, a tar config digest not equal to the image ID, a JSON
    duplicate key, and a hostile Docker environment are all rejected.
14. The two historical Phase-B run IDs are permanently refused for reuse. A bound/unbound lock
    that is inconsistent with the current state of the operational lineage is rejected.

## 8. Allowed, forbidden, and stop conditions

Allowed to modify:

```text
apps/camoufox-host/build/r1-diag-v1/build_host.py
apps/camoufox-host/test_r1_diag_*.py
apps/camoufox-host/lock/camoufox-...-r1-diag-v2-source.json
docs/camoufox-program-status.md
this document
```

Forbidden to modify or execute: the Dockerfile, strict_build.py, diag_gate.py, 0000–0004, 9000,
the engine source, the Artifact, the probe, the FP2 comparator, Phase B-2, Phase C-2, a
real-machine consumption of prepare-bound-image, Phase D, Browser, V1–V4.

The implementation must close with all the no-browser tests, `py_compile`,
`git diff --check`, the scope diff, and a clean checkpoint, then stop and return to the lead
agent Gate. The actual durable-root sentinel/reboot qualification is an independent host
qualification after the checkpoint is Accepted, and is not secretly executed in this offline
implementation.
