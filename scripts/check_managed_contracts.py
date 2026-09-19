"""Cross-layer managed-identity contract consistency gate.

The managed-identity tables (presets, timezones, GPU profiles, core counts)
are hand-mirrored across the Host (source of record), the Rust core, and the
desktop UI. This gate parses each layer and fails the build on the first
drift, so a layer can no longer be missed silently (the engine.rs allowlist
and the host timezone whitelist both drifted this way historically).

Runs as part of every `agent-task verify`; exits 1 with a layer-labeled diff.
"""

import ast
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    print(f"managed-contract MISMATCH: {message}")
    sys.exit(1)


def read(relative_path: str) -> str:
    return (REPO / relative_path).read_text(encoding="utf-8")


def rust_block(source: str, header: str) -> str:
    """Return the balanced-brace block that starts at `header`."""
    start = source.index(header)
    open_brace = source.index("{", start)
    depth = 0
    for index in range(open_brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    fail(f"unbalanced braces after {header!r}")


def string_literals(text: str) -> set[str]:
    return set(re.findall(r'"([^"]+)"', text))


def parse_host_tables() -> tuple[set[str], set[str], set[str], dict[str, str], dict[str, str]]:
    """Return (presets, timezones, gpu_ids, preset_locale, preset_timezone)."""
    tree = ast.parse(read("apps/camoufox-host/provision_artifact.py"))
    tables: dict[str, ast.expr] = {}
    for node in tree.body:
        target = None
        if isinstance(node, ast.Assign) and len(node.targets) == 1:
            target = node.targets[0]
        elif isinstance(node, ast.AnnAssign):
            target = node.target
        if (
            isinstance(target, ast.Name)
            and target.id
            in ("PROVISION_PRESETS", "SUPPORTED_TIMEZONES", "GPU_PRESETS")
        ):
            tables[target.id] = node.value
    missing = {"PROVISION_PRESETS", "SUPPORTED_TIMEZONES", "GPU_PRESETS"} - set(tables)
    if missing:
        fail(f"provision_artifact.py no longer defines {sorted(missing)} at module level")

    presets = {key.value for key in tables["PROVISION_PRESETS"].keys}
    timezones = {element.value for element in tables["SUPPORTED_TIMEZONES"].elts}
    gpu_ids = {key.value for key in tables["GPU_PRESETS"].keys}

    preset_locale: dict[str, str] = {}
    preset_timezone: dict[str, str] = {}
    for key, value in zip(
        [k.value for k in tables["PROVISION_PRESETS"].keys],
        tables["PROVISION_PRESETS"].values,
    ):
        if isinstance(value, ast.Call) and value.func.id == "_balanced_preset":
            locale_arg, timezone_arg = value.args
            preset_locale[key] = locale_arg.value
            preset_timezone[key] = timezone_arg.value
            if timezone_arg.value not in timezones:
                fail(
                    f"preset {key} defaults to timezone {timezone_arg.value} "
                    "which SUPPORTED_TIMEZONES rejects"
                )
    return presets, timezones, gpu_ids, preset_locale, preset_timezone


def parse_domain_rs() -> tuple[set[str], set[str], set[str], dict[str, str], dict[str, str]]:
    """Return (preset_names, timezones, gpu_allowlist, variant_locale, variant_to_wire)."""
    source = read("apps/desktop/src-tauri/src/domain.rs")
    impl_block = rust_block(source, "impl ManagedIdentityPreset")
    preset_names = set(re.findall(r'Self::\w+ => "([a-z0-9-]+)"', impl_block))
    timezones = string_literals(
        rust_block(source, "fn is_supported_managed_timezone")
    )
    gpu_allowlist = string_literals(rust_block(source, "fn is_supported_gpu_preset"))
    variant_locale = {
        variant: locale
        for variant, locale in re.findall(r'Self::(\w+) => Some\("([^"]+)"\)', impl_block)
    }
    variant_to_wire = {
        variant: wire
        for variant, wire in re.findall(r'Self::(\w+) => "([a-z0-9-]+)"', impl_block)
    }
    return preset_names, timezones, gpu_allowlist, variant_locale, variant_to_wire


def main() -> int:
    presets, timezones, gpu_ids, preset_locale, preset_timezone = parse_host_tables()

    # --- Preset names across every hand-mirrored layer ---
    (
        rust_presets,
        rust_timezones,
        rust_gpu,
        variant_locale,
        variant_to_wire,
    ) = parse_domain_rs()
    if rust_presets != presets:
        fail(f"preset names domain.rs vs host: rust-only={sorted(rust_presets - presets)} host-only={sorted(presets - rust_presets)}")

    engine_source = read("apps/desktop/src-tauri/src/engine.rs")
    allowlist_match = re.search(r"if !matches!\(\s*preset,(.*?)\)\s*\{", engine_source, re.DOTALL)
    if allowlist_match is None:
        fail("engine.rs provision allowlist (if !matches!(preset, ...)) not found")
    engine_allowlist = string_literals(allowlist_match.group(1))
    if engine_allowlist != presets:
        fail(f"preset allowlist engine.rs vs host: engine-only={sorted(engine_allowlist - presets)} host-only={sorted(presets - engine_allowlist)}")

    api_source = read("apps/desktop/src/desktop-api.ts")
    union_match = re.search(r"export type ManagedIdentityPreset =\s*(.*?);", api_source, re.DOTALL)
    if union_match is None:
        fail("desktop-api.ts ManagedIdentityPreset union not found")
    union_names = string_literals(union_match.group(1))
    if union_names != presets:
        fail(f"preset union desktop-api.ts vs host: ts-only={sorted(union_names - presets)} host-only={sorted(presets - union_names)}")

    form_source = read("apps/desktop/src/features/identity/ManagedSiloForm.tsx")
    option_names = set(re.findall(r'<option value="(balanced-[a-z0-9-]+)">', form_source))
    balanced_presets = set(preset_locale)
    if option_names != balanced_presets:
        fail(
            "ManagedSiloForm country options vs host balanced presets: "
            f"form-only={sorted(option_names - balanced_presets)} "
            f"host-only={sorted(balanced_presets - option_names)}"
        )

    # --- Timezone whitelist across layers ---
    if rust_timezones != timezones:
        fail(f"timezones domain.rs vs host: rust-only={sorted(rust_timezones - timezones)} host-only={sorted(timezones - rust_timezones)}")
    tz_source = read("apps/desktop/src/timezone-presets.ts")
    preset_block = re.search(r"export const TIMEZONE_PRESETS = \[(.*?)\] as const;", tz_source, re.DOTALL)
    if preset_block is None:
        fail("timezone-presets.ts TIMEZONE_PRESETS not found")
    ui_timezones = set(re.findall(r'id: "([^"]+)"', preset_block.group(1)))
    if ui_timezones != timezones:
        fail(f"timezones timezone-presets.ts vs host: ui-only={sorted(ui_timezones - timezones)} host-only={sorted(timezones - ui_timezones)}")

    # --- Per-preset defaults: UI timezone default and Rust locale mirror the host table ---
    default_fn = re.search(
        r"export function defaultTimezoneForPreset\(.*?\{(.*)\}", tz_source, re.DOTALL
    )
    if default_fn is None:
        fail("timezone-presets.ts defaultTimezoneForPreset not found")
    arms = dict(re.findall(r'case "([a-z0-9-]+)":\s*\n\s*return "([^"]+)"', default_fn.group(1)))
    fallback = re.search(r"default:\s*\n\s*return \"([^\"]+)\"", default_fn.group(1))
    if fallback is None:
        fail("timezone-presets.ts defaultTimezoneForPreset lost its default arm")
    for preset, timezone in preset_timezone.items():
        effective = arms.get(preset, fallback.group(1))
        if effective != timezone:
            fail(f"defaultTimezoneForPreset({preset}) = {effective}, host table says {timezone}")

    for variant, locale in variant_locale.items():
        wire = variant_to_wire.get(variant)
        if wire in preset_locale and preset_locale[wire] != locale:
            fail(f"domain.rs locale({variant}) = {locale}, host table says {preset_locale[wire]}")

    # --- GPU profile ids ---
    gpu_ts = read("apps/desktop/src/gpu-presets.ts")
    gpu_block = re.search(r"export const GPU_PRESETS = \[(.*?)\] as const;", gpu_ts, re.DOTALL)
    if gpu_block is None:
        fail("gpu-presets.ts GPU_PRESETS not found")
    ui_gpu = set(re.findall(r'id: "([^"]+)"', gpu_block.group(1)))
    if ui_gpu - {"auto"} != gpu_ids:
        fail(f"gpu ids gpu-presets.ts vs host: ui-only={sorted(ui_gpu - {'auto'} - gpu_ids)} host-only={sorted(gpu_ids - ui_gpu)}")
    if rust_gpu != gpu_ids | {"auto"}:
        fail(f"gpu allowlist domain.rs vs host: rust-only={sorted(rust_gpu - gpu_ids - {'auto'})} host-only={sorted(gpu_ids - rust_gpu)}")

    # --- Hardware concurrency choices ---
    cores_host = re.search(
        r"SUPPORTED_HARDWARE_CONCURRENCY = \(([^)]*)\)",
        read("apps/camoufox-host/provision_artifact.py"),
    )
    if cores_host is None:
        fail("provision_artifact.py SUPPORTED_HARDWARE_CONCURRENCY not found")
    cores_pattern = ", ".join(part.strip() for part in cores_host.group(1).split(",") if part.strip())
    cores_regex = re.escape(cores_pattern).replace(",", r",\s*")
    for relative_path in (
        "apps/desktop/src-tauri/src/engine.rs",
        "apps/desktop/src-tauri/src/vault.rs",
        "apps/desktop/src-tauri/src/domain.rs",
        "apps/desktop/src/shared/defaults.ts",
    ):
        if not re.search(cores_regex, read(relative_path)):
            fail(f"core choices {cores_pattern} not found in {relative_path}")

    print(
        "managed contracts consistent: "
        f"{len(presets)} presets, {len(timezones)} timezones, "
        f"{len(gpu_ids)} gpu profiles, cores={cores_pattern}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
