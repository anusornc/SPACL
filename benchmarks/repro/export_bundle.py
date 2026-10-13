#!/usr/bin/env python3
"""Assemble the ADR 0028 publishable-reproduction export bundle (issue #203).

The bundle is the curated content tree for the public companion repository:
(a) tracked repository files named by the committed selection manifest
(`export_selection.json`), copied at their repo-relative paths, and (b)
generated evidence exports copied from local gitignored run directories into
an ``evidence/`` tree, where each lane directory carries a README header with
its run identity and the verbatim ADR claim sentence(s) that govern what the
lane may claim (ADR 0002/0024-0027). Every exported artifact is bound to its
sha256 and source in ``hash_inventory.json`` so every published number traces
to a run id or a tracked path.

Fail-closed contract: the export aborts on a vanished or untracked selection
entry, a run directory or headline artifact that is absent, a run-identity
file that is missing or lacks its run-id fields, a ladder whose metadata
references aggregation runs the manifest does not list, a claim sentence that
is not verbatim in its governing ADR, and any file that violates the
pointers-not-blobs rule (forbidden suffix or size cap). ``--self-check``
re-hashes a produced bundle against its inventory and verifies completeness.

Redistribution is pointers, not blobs (ADR 0028): no corpora, no generated
datasets, no engine binaries.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
DEFAULT_MAX_ARTIFACT_BYTES = 8 * 1024 * 1024
INVENTORY_NAME = "hash_inventory.json"
LANE_README_NAME = "README.md"
SOURCE_KINDS = ("tracked_selection", "run_dir", "generated")

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG = Path(__file__).resolve().parent / "export_selection.json"


class ExportError(RuntimeError):
    """The export bundle cannot satisfy its fail-closed contract."""


def log_info(message: str) -> None:
    print(f"[info] {message}")


def log_ok(message: str) -> None:
    print(f"[ok] {message}")


def log_err(message: str) -> None:
    print(f"[error] {message}", file=sys.stderr)


def collapse(text: str) -> str:
    """Collapse whitespace so hard-wrapped markdown compares word-for-word."""
    return re.sub(r"\s+", " ", text).strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def posix_parts(value: str) -> tuple[str, ...]:
    return tuple(part for part in value.split("/") if part != "")


def safe_relative(value: str, what: str) -> Path:
    """Require a literal repo-relative POSIX path that cannot escape its base."""
    if not value or value.startswith("/") or "\\" in value:
        raise ExportError(f"unsafe {what}: {value!r}")
    if any(char in value for char in "*?["):
        # git pathspecs treat these as globs; the manifest is an explicit
        # list, so wildcard entries would silently over-select.
        raise ExportError(f"unsafe {what} (wildcards are not explicit): {value!r}")
    parts = posix_parts(value)
    if any(part == ".." for part in parts):
        raise ExportError(f"unsafe {what}: {value!r}")
    return Path(*parts)


def run_git(repo_root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise ExportError(
            f"git {' '.join(args)} failed: {result.stderr.strip() or result.returncode}"
        )
    return result.stdout


def load_config(config_path: Path) -> dict[str, Any]:
    if not config_path.is_file():
        raise ExportError(f"selection manifest not found: {config_path}")
    try:
        config = json.loads(config_path.read_text())
    except json.JSONDecodeError as error:
        raise ExportError(f"selection manifest is not valid JSON: {config_path}: {error}") from error
    if config.get("schema_version") != SCHEMA_VERSION:
        raise ExportError(
            f"selection manifest schema_version must be {SCHEMA_VERSION}: {config_path}"
        )
    if not config.get("bundle_name"):
        raise ExportError("selection manifest lacks bundle_name")
    if not config.get("tracked_selection"):
        raise ExportError("selection manifest lacks tracked_selection entries")
    if not config.get("lanes"):
        raise ExportError("selection manifest lacks lanes")
    return config


def resolve_repo_root(repo_root_arg: str | None) -> Path:
    root = Path(repo_root_arg).resolve() if repo_root_arg else REPO_ROOT
    try:
        toplevel = Path(run_git(root, "rev-parse", "--show-toplevel").strip())
    except ExportError as error:
        raise ExportError(f"not a git repository: {root}: {error}") from error
    if Path(toplevel).resolve() != root:
        raise ExportError(f"repo root is not the git toplevel: {root} vs {toplevel}")
    return root


def source_commit(repo_root: Path) -> str:
    commit = run_git(repo_root, "rev-parse", "HEAD").strip()
    if not commit:
        raise ExportError("repository has no HEAD commit")
    return commit


def expand_selection(config: dict[str, Any], repo_root: Path) -> list[str]:
    """Expand manifest entries to git-tracked files, failing closed."""
    selected: list[str] = []
    seen: set[str] = set()
    for entry in config["tracked_selection"]:
        if not isinstance(entry, str):
            raise ExportError(f"tracked_selection entry must be a string: {entry!r}")
        rel = safe_relative(entry, "tracked_selection entry")
        if not rel.parts:
            raise ExportError(
                f"selection entry must name a concrete path, not the repository root: {entry}"
            )
        target = repo_root / rel
        if not target.exists():
            raise ExportError(
                f"selection entry vanished from disk (not tracked or deleted): {entry}"
            )
        listing = run_git(repo_root, "ls-files", "-z", "--", entry).split("\0")
        tracked = sorted({item for item in listing if item})
        if not tracked:
            raise ExportError(f"selection entry is not git-tracked: {entry}")
        if not target.is_dir() and str(rel) not in tracked:
            raise ExportError(f"selection entry is not git-tracked: {entry}")
        for item in tracked:
            item_path = repo_root / item
            if item_path.is_symlink():
                raise ExportError(f"tracked selection contains a symlink (blob risk): {item}")
            if item not in seen:
                seen.add(item)
                selected.append(item)
    return selected


def verify_claims(lane: dict[str, Any], repo_root: Path, selection: set[str]) -> list[dict[str, str]]:
    claims = lane.get("claims") or []
    if not claims:
        raise ExportError(f"lane {lane['name']} carries no claim sentence")
    for claim in claims:
        adr = safe_relative(claim.get("adr", ""), "claim adr")
        sentence = claim.get("sentence", "")
        if not sentence or not sentence.strip():
            raise ExportError(f"lane {lane['name']} has an empty claim sentence")
        if str(adr) not in selection:
            raise ExportError(
                f"lane {lane['name']}: governing ADR {adr} is not in the tracked selection"
            )
        adr_path = repo_root / adr
        if not adr_path.is_file():
            raise ExportError(f"lane {lane['name']}: governing ADR missing on disk: {adr}")
        if collapse(sentence) not in collapse(adr_path.read_text()):
            raise ExportError(
                f"lane {lane['name']}: claim sentence is not verbatim in {adr}"
            )
    return claims


def extract_run_ids(export: dict[str, Any], repo_root: Path, lane_name: str) -> list[str]:
    source_dir = safe_relative(export.get("source_dir", ""), "source_dir")
    identity = export.get("identity") or {}
    identity_file = identity.get("file")
    if not identity_file:
        raise ExportError(f"lane {lane_name}: export {source_dir} lacks an identity file")
    identity_path = repo_root / source_dir / identity_file
    if not identity_path.is_file():
        raise ExportError(
            f"lane {lane_name}: run identity file absent: {source_dir}/{identity_file}"
        )
    try:
        metadata = json.loads(identity_path.read_text())
    except json.JSONDecodeError as error:
        raise ExportError(
            f"lane {lane_name}: run identity file is not valid JSON: "
            f"{source_dir}/{identity_file}: {error}"
        ) from error
    run_ids: list[str] = [source_dir.name]
    for field in identity.get("run_id_fields", []):
        if field not in metadata:
            raise ExportError(
                f"lane {lane_name}: identity file {source_dir}/{identity_file} "
                f"lacks run id field {field!r}"
            )
        value = metadata[field]
        values = value if isinstance(value, list) else [value]
        if not values or any(not isinstance(item, str) or not item for item in values):
            raise ExportError(
                f"lane {lane_name}: run id field {field!r} in "
                f"{source_dir}/{identity_file} is empty or not a string"
            )
        for item in values:
            if item not in run_ids:
                run_ids.append(item)
    return run_ids


def resolve_lane_exports(
    lane: dict[str, Any], repo_root: Path
) -> list[dict[str, Any]]:
    name = lane.get("name", "")
    lane_rel = safe_relative(name, "lane name")
    if len(lane_rel.parts) != 1:
        raise ExportError(f"lane name must be a single path component: {name!r}")
    resolved: list[dict[str, Any]] = []
    seen_dests: set[str] = set()
    for export in lane.get("exports") or []:
        source_dir = safe_relative(export.get("source_dir", ""), "source_dir")
        dest = safe_relative(export.get("dest", ""), "dest")
        source_path = repo_root / source_dir
        if not source_path.is_dir():
            raise ExportError(f"lane {name}: run directory absent: {source_dir}")
        artifacts = export.get("artifacts") or []
        if not artifacts:
            raise ExportError(f"lane {name}: export {source_dir} lists no artifacts")
        for artifact in artifacts:
            artifact_rel = safe_relative(artifact, "artifact")
            if artifact_rel.name == LANE_README_NAME:
                raise ExportError(
                    f"lane {name}: artifact name {LANE_README_NAME} is reserved for the lane header"
                )
            artifact_path = source_path / artifact_rel
            if not artifact_path.is_file():
                raise ExportError(
                    f"lane {name}: headline artifact absent: {source_dir}/{artifact}"
                )
        dest_key = str(dest)
        if dest_key in seen_dests:
            raise ExportError(f"lane {name}: duplicate export dest {dest_key}")
        seen_dests.add(dest_key)
        resolved.append(
            {
                "source_dir": str(source_dir),
                "dest": dest_key,
                "artifacts": [str(safe_relative(a, "artifact")) for a in artifacts],
                "run_ids": extract_run_ids(export, repo_root, name),
            }
        )
    return resolved


def cross_check_ladder(lane: dict[str, Any], exports: list[dict[str, Any]], repo_root: Path) -> None:
    """Ladder metadata must reference exactly the configured aggregations."""
    metadata_export = None
    for export in exports:
        if "ladder_metadata.json" in export["artifacts"]:
            metadata_export = export
            break
    if metadata_export is None:
        raise ExportError(f"lane {lane['name']}: no ladder_metadata.json export configured")
    metadata_path = repo_root / metadata_export["source_dir"] / "ladder_metadata.json"
    try:
        metadata = json.loads(metadata_path.read_text())
    except json.JSONDecodeError as error:
        raise ExportError(
            f"lane {lane['name']}: ladder_metadata.json is not valid JSON: {error}"
        ) from error
    referenced: set[str] = set()
    profiles = metadata.get("profiles") or {}
    if not profiles:
        raise ExportError(f"lane {lane['name']}: ladder_metadata.json lists no profiles")
    for profile, info in profiles.items():
        for point in (info or {}).get("points") or []:
            aggregation_dir = point.get("aggregation_dir")
            if not aggregation_dir:
                raise ExportError(
                    f"lane {lane['name']}: ladder profile {profile} point lacks aggregation_dir"
                )
            referenced.add(str(aggregation_dir))
    configured = {
        Path(export["source_dir"]).name
        for export in exports
        if export is not metadata_export
    }
    if not configured:
        raise ExportError(f"lane {lane['name']}: no aggregation exports configured for the ladder")
    missing = sorted(referenced - configured)
    extra = sorted(configured - referenced)
    if missing or extra:
        raise ExportError(
            f"lane {lane['name']}: ladder metadata and manifest disagree "
            f"(missing from manifest: {missing}; not referenced by metadata: {extra})"
        )


def render_lane_readme(
    lane: dict[str, Any],
    exports: list[dict[str, Any]],
) -> str:
    lines: list[str] = []
    lines.append(f"# Evidence lane: {lane.get('title', lane['name'])}")
    lines.append("")
    lines.append(
        f"Generated by `benchmarks/repro/export_bundle.py` from the selection "
        f"manifest `benchmarks/repro/export_selection.json` (issue #203, ADR "
        f"0028). Artifacts in this directory are byte-identical copies from "
        f"gitignored run directories of the private monorepo; their sha256 "
        f"values and sources are recorded in `{INVENTORY_NAME}` at the bundle "
        f"root. No corpora, generated datasets, or engine binaries are "
        f"redistributed."
    )
    lines.append("")
    lines.append("## Claim (verbatim)")
    lines.append("")
    for claim in lane["claims"]:
        lines.append(f"> {claim['sentence']}")
        lines.append(">")
        lines.append(f"> — `{claim['adr']}`")
        lines.append("")
    lines.append("## Run identity")
    lines.append("")
    lines.append("| Export | Source run directory | Run ids |")
    lines.append("|---|---|---|")
    for export in exports:
        ids = ", ".join(f"`{run_id}`" for run_id in export["run_ids"])
        lines.append(f"| {export['dest']} | `{export['source_dir']}` | {ids} |")
    lines.append("")
    lines.append("## Artifacts")
    lines.append("")
    lines.append("| Artifact | Source | Run ids |")
    lines.append("|---|---|---|")
    for export in exports:
        ids = ", ".join(f"`{run_id}`" for run_id in export["run_ids"])
        for artifact in export["artifacts"]:
            source = f"`{export['source_dir']}/{artifact}`"
            lines.append(f"| {export['dest']}/{artifact} | {source} | {ids} |")
    lines.append("")
    return "\n".join(lines)


def render_bundle_readme(
    config: dict[str, Any],
    lanes: list[dict[str, Any]],
    generated_at: str,
    commit: str,
) -> str:
    lines: list[str] = []
    lines.append(f"# {config['bundle_name']} export bundle")
    lines.append("")
    lines.append(f"- Generated at: {generated_at}")
    lines.append(f"- Source commit: `{commit}`")
    lines.append(
        "- Generated by: `benchmarks/repro/export_bundle.py` from "
        "`benchmarks/repro/export_selection.json` (issue #203, ADR 0028)"
    )
    lines.append("")
    if config.get("bundle_claim"):
        lines.append("## Scope (verbatim, ADR 0028)")
        lines.append("")
        lines.append(f"> {config['bundle_claim']['sentence']}")
        lines.append(">")
        lines.append(f"> — `{config['bundle_claim']['adr']}`")
        lines.append("")
    lines.append("## Evidence lanes")
    lines.append("")
    lines.append("| Lane | Directory | Claim ADRs | Run ids |")
    lines.append("|---|---|---|---|")
    for lane in lanes:
        adrs = ", ".join(f"`{claim['adr']}`" for claim in lane["claims"])
        run_ids = sorted(
            {run_id for export in lane["exports"] for run_id in export["run_ids"]}
        )
        ids = ", ".join(f"`{run_id}`" for run_id in run_ids)
        lines.append(f"| {lane['title']} | `evidence/{lane['name']}/` | {adrs} | {ids} |")
    lines.append("")
    lines.append("## Integrity")
    lines.append("")
    lines.append(
        f"Every file in this bundle is bound to its sha256, source path, and "
        f"run ids in `{INVENTORY_NAME}` at the bundle root. To re-verify this "
        f"bundle against its inventory:"
    )
    lines.append("")
    lines.append(
        "```bash"
    )
    lines.append(
        "python3 benchmarks/repro/export_bundle.py --self-check <bundle-dir>"
    )
    lines.append("```")
    lines.append("")
    return "\n".join(lines)


def guard_pointers_not_blobs(bundle_dir: Path, config: dict[str, Any]) -> None:
    forbidden = tuple(
        suffix.lower() for suffix in config.get("forbidden_suffixes", []) if suffix
    )
    allowlist = set(config.get("forbidden_suffix_allowlist") or [])
    # A trailing-slash allowlist entry is a directory-prefix exception (for
    # example project-authored test fixtures), so it applies to every bundle
    # path under that prefix; the size cap still applies per file.
    allowlist_prefixes = tuple(e for e in allowlist if e.endswith("/"))
    exact_allowlist = allowlist - set(allowlist_prefixes)
    max_bytes = int(config.get("max_artifact_bytes", DEFAULT_MAX_ARTIFACT_BYTES))
    offenders: list[str] = []
    for path in sorted(bundle_dir.rglob("*")):
        if not path.is_file() or path.is_symlink():
            if path.is_symlink():
                offenders.append(f"symlink in bundle: {path}")
            continue
        rel = path.relative_to(bundle_dir).as_posix()
        if path.parent == bundle_dir and path.name == INVENTORY_NAME:
            # The inventory is the manifest that travels (ADR 0028); it is
            # generated by this script and never a redistributed blob.
            continue
        allowed = rel in exact_allowlist or rel.startswith(allowlist_prefixes)
        if forbidden and path.suffix.lower() in forbidden and not allowed:
            offenders.append(f"forbidden suffix {path.suffix}: {rel}")
        size = path.stat().st_size
        if size > max_bytes:
            offenders.append(f"size {size} exceeds cap {max_bytes}: {rel}")
    if offenders:
        raise ExportError(
            "pointers-not-blobs guard failed (ADR 0028; no corpora, datasets, "
            "or engine binaries in the bundle): " + "; ".join(offenders)
        )


def validate_allowlist(config: dict[str, Any], selection: set[str]) -> None:
    """Every allowlisted path must actually be selected; no dead exceptions.

    A trailing-slash entry names a selected directory whose contents are
    project-authored fixtures (not corpora or datasets); it is valid when the
    directory itself is selected.
    """
    allowlist = config.get("forbidden_suffix_allowlist") or []
    for entry in allowlist:
        stem = entry.rstrip("/")
        covered = stem in selection or any(
            selected.startswith(stem + "/") for selected in selection
        )
        if not covered:
            raise ExportError(
                "forbidden_suffix_allowlist entry is not in the tracked selection "
                f"(dead exception): {entry}"
            )


def prepare_output_dir(output: Path, force: bool) -> None:
    if output.exists():
        if not output.is_dir():
            raise ExportError(f"output path exists and is not a directory: {output}")
        if any(output.iterdir()):
            if not force:
                raise ExportError(
                    f"output directory exists and is not empty (use --force to "
                    f"replace a previously generated bundle): {output}"
                )
            if not (output / INVENTORY_NAME).is_file():
                raise ExportError(
                    f"refusing --force: {output} is not a previously generated bundle "
                    f"(no {INVENTORY_NAME})"
                )
            shutil.rmtree(output)
    output.mkdir(parents=True)


def build_bundle(
    config: dict[str, Any],
    repo_root: Path,
    output: Path,
    force: bool,
) -> dict[str, Any]:
    commit = source_commit(repo_root)
    log_info(f"source commit {commit}")

    selection = expand_selection(config, repo_root)
    validate_allowlist(config, set(selection))
    log_ok(f"selection: {len(selection)} tracked files from "
           f"{len(config['tracked_selection'])} manifest entries")

    selection_set = set(selection)
    lanes: list[dict[str, Any]] = []
    lane_names: set[str] = set()
    for lane in config["lanes"]:
        if lane["name"] in lane_names:
            raise ExportError(f"duplicate lane name: {lane['name']}")
        lane_names.add(lane["name"])
        claims = verify_claims(lane, repo_root, selection_set)
        exports = resolve_lane_exports(lane, repo_root)
        if lane.get("cross_check_ladder"):
            cross_check_ladder(lane, exports, repo_root)
        lanes.append(
            {
                "name": lane["name"],
                "title": lane.get("title", lane["name"]),
                "claims": claims,
                "exports": exports,
            }
        )
    evidence_count = sum(len(export["artifacts"]) for lane in lanes for export in lane["exports"])
    log_ok(f"lanes: {len(lanes)} configured, {evidence_count} evidence artifacts")

    # Tracked destinations and evidence destinations must not collide.
    tracked_dests = {str(item) for item in selection}
    evidence_dests = {
        f"evidence/{lane['name']}/{export['dest']}/{artifact}".replace("/./", "/")
        for lane in lanes
        for export in lane["exports"]
        for artifact in export["artifacts"]
    }
    collisions = tracked_dests & evidence_dests
    if collisions:
        raise ExportError(f"tracked selection and evidence exports collide: {sorted(collisions)}")

    prepare_output_dir(output, force)
    generated_at = datetime.now(timezone.utc).isoformat(timespec="seconds")

    inventory_entries: list[dict[str, Any]] = []

    for rel in selection:
        destination = output / rel
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(repo_root / rel, destination)
        inventory_entries.append(
            {
                "path": str(rel),
                "sha256": sha256_file(repo_root / rel),
                "source_kind": "tracked_selection",
                "source_path": str(rel),
                "run_ids": [],
            }
        )
    log_ok(f"copied {len(selection)} tracked files")

    for lane in lanes:
        lane_dir = output / "evidence" / lane["name"]
        for export in lane["exports"]:
            export_dir = lane_dir / export["dest"] if export["dest"] != "." else lane_dir
            for artifact in export["artifacts"]:
                source = repo_root / export["source_dir"] / artifact
                destination = export_dir / artifact
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
                artifact_rel = (
                    destination.relative_to(output).as_posix()
                )
                inventory_entries.append(
                    {
                        "path": artifact_rel,
                        "sha256": sha256_file(source),
                        "source_kind": "run_dir",
                        "source_path": f"{export['source_dir']}/{artifact}",
                        "run_ids": list(export["run_ids"]),
                    }
                )
        readme = lane_dir / LANE_README_NAME
        readme.parent.mkdir(parents=True, exist_ok=True)
        readme.write_text(render_lane_readme(lane, lane["exports"]))
        inventory_entries.append(
            {
                "path": readme.relative_to(output).as_posix(),
                "sha256": sha256_file(readme),
                "source_kind": "generated",
                "source_path": None,
                "run_ids": sorted(
                    {run_id for export in lane["exports"] for run_id in export["run_ids"]}
                ),
            }
        )
    log_ok(f"copied {evidence_count} evidence artifacts and wrote {len(lanes)} lane headers")

    bundle_readme = output / LANE_README_NAME
    bundle_readme.write_text(
        render_bundle_readme(config, lanes, generated_at, commit)
    )
    inventory_entries.append(
        {
            "path": LANE_README_NAME,
            "sha256": sha256_file(bundle_readme),
            "source_kind": "generated",
            "source_path": None,
            "run_ids": [],
        }
    )

    guard_pointers_not_blobs(output, config)
    log_ok("pointers-not-blobs guard passed (suffix + size cap)")

    entries = sorted(inventory_entries, key=lambda entry: entry["path"])
    counts = {"total": len(entries)}
    for kind in SOURCE_KINDS:
        counts[kind] = sum(1 for entry in entries if entry["source_kind"] == kind)
    inventory = {
        "schema_version": SCHEMA_VERSION,
        "bundle_name": config["bundle_name"],
        "generated_at": generated_at,
        "source_commit": commit,
        "counts": counts,
        "lanes": [
            {
                "name": lane["name"],
                "claims": [dict(claim) for claim in lane["claims"]],
                "run_ids": sorted(
                    {run_id for export in lane["exports"] for run_id in export["run_ids"]}
                ),
            }
            for lane in lanes
        ],
        "entries": entries,
    }
    (output / INVENTORY_NAME).write_text(json.dumps(inventory, indent=2) + "\n")
    log_ok(f"inventory written: {output / INVENTORY_NAME} ({counts['total']} entries)")
    log_ok(f"bundle complete: {output}")
    return inventory


def self_check(bundle_dir: Path, config: dict[str, Any] | None) -> int:
    inventory_path = bundle_dir / INVENTORY_NAME
    if not inventory_path.is_file():
        raise ExportError(f"bundle has no {INVENTORY_NAME}: {bundle_dir}")
    try:
        inventory = json.loads(inventory_path.read_text())
    except json.JSONDecodeError as error:
        raise ExportError(f"{INVENTORY_NAME} is not valid JSON: {error}") from error
    if inventory.get("schema_version") != SCHEMA_VERSION:
        raise ExportError(f"inventory schema_version must be {SCHEMA_VERSION}")
    entries = inventory.get("entries") or []
    if not entries:
        raise ExportError("inventory lists no entries")

    listed: dict[str, dict[str, Any]] = {}
    for entry in entries:
        for key in ("path", "sha256", "source_kind", "source_path", "run_ids"):
            if key not in entry:
                raise ExportError(f"inventory entry missing {key!r}: {entry}")
        if entry["source_kind"] not in SOURCE_KINDS:
            raise ExportError(f"inventory entry has unknown source_kind: {entry}")
        rel = entry["path"]
        if not isinstance(rel, str):
            raise ExportError(f"inventory entry path is not a string: {entry}")
        parts = posix_parts(rel)
        if not parts or any(part in ("..", "") for part in parts):
            raise ExportError(f"unsafe inventory path: {rel!r}")
        if rel in listed:
            raise ExportError(f"duplicate inventory path: {rel}")
        listed[rel] = entry
        artifact = bundle_dir / Path(*parts)
        if not artifact.is_file() or artifact.is_symlink():
            raise ExportError(f"inventoried file missing from bundle: {rel}")
        digest = sha256_file(artifact)
        if digest != entry["sha256"]:
            raise ExportError(
                f"sha256 mismatch for {rel}: inventory {entry['sha256']} vs bundle {digest}"
            )
        if entry["source_kind"] == "run_dir" and not entry["run_ids"]:
            raise ExportError(f"run_dir entry without run ids: {rel}")

    on_disk: set[str] = set()
    for path in bundle_dir.rglob("*"):
        if path.is_file() and not path.is_symlink():
            on_disk.add(path.relative_to(bundle_dir).as_posix())
    unlisted_files = sorted(on_disk - set(listed) - {INVENTORY_NAME})
    if unlisted_files:
        raise ExportError(f"bundle files missing from inventory: {unlisted_files}")

    if config is not None:
        inventory_lanes = {lane.get("name"): lane for lane in inventory.get("lanes") or []}
        config_lane_names = {lane["name"] for lane in config["lanes"]}
        if config_lane_names != set(inventory_lanes):
            raise ExportError(
                f"config lanes and inventory lanes disagree: "
                f"{sorted(config_lane_names)} vs {sorted(inventory_lanes)}"
            )
        for lane in config["lanes"]:
            lane_readme = bundle_dir / "evidence" / lane["name"] / LANE_README_NAME
            if not lane_readme.is_file():
                raise ExportError(f"lane README header absent: evidence/{lane['name']}/")
            readme_text = lane_readme.read_text()
            collapsed = collapse(readme_text)
            for claim in lane.get("claims") or []:
                if collapse(claim["sentence"]) not in collapsed:
                    raise ExportError(
                        f"lane {lane['name']} README lacks its verbatim claim sentence"
                    )
                bundle_adr = bundle_dir / claim["adr"]
                if not bundle_adr.is_file():
                    raise ExportError(
                        f"lane {lane['name']}: governing ADR absent from bundle: {claim['adr']}"
                    )
                if collapse(claim["sentence"]) not in collapse(bundle_adr.read_text()):
                    raise ExportError(
                        f"lane {lane['name']}: claim sentence is not verbatim in the "
                        f"bundled copy of {claim['adr']}"
                    )
            lane_run_ids = {
                run_id
                for rel_path, entry in listed.items()
                if rel_path.startswith(f"evidence/{lane['name']}/")
                for run_id in entry["run_ids"]
            }
            inventory_lane_ids = set(inventory_lanes[lane["name"]].get("run_ids") or [])
            if lane_run_ids != inventory_lane_ids:
                raise ExportError(
                    f"lane {lane['name']}: run ids in inventory entries "
                    f"({sorted(lane_run_ids)}) disagree with the lane record "
                    f"({sorted(inventory_lane_ids)})"
                )
            for run_id in sorted(lane_run_ids):
                if run_id not in readme_text:
                    raise ExportError(
                        f"lane {lane['name']} README does not list run id {run_id}"
                    )
        if config.get("bundle_claim"):
            bundle_readme = bundle_dir / LANE_README_NAME
            if not bundle_readme.is_file():
                raise ExportError("bundle README absent")
            collapsed = collapse(bundle_readme.read_text())
            if collapse(config["bundle_claim"]["sentence"]) not in collapsed:
                raise ExportError("bundle README lacks the ADR 0028 pointers-not-blobs claim")

    guard_pointers_not_blobs(bundle_dir, config or {})
    counts = inventory.get("counts") or {}
    log_ok(
        f"self-check passed: {len(listed)} inventoried files verified, "
        f"{len(on_disk)} files on disk, counts={counts}"
    )
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Assemble or verify the ADR 0028 reproduction export bundle."
    )
    parser.add_argument(
        "--config",
        default=str(DEFAULT_CONFIG),
        help="selection manifest JSON (default: %(default)s)",
    )
    parser.add_argument(
        "--repo-root",
        default=None,
        help="repository root (default: the repository containing this script)",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="bundle output directory (default: benchmarks/repro/export/<bundle_name>)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="replace a previously generated bundle directory (must contain an inventory)",
    )
    parser.add_argument(
        "--self-check",
        dest="self_check",
        default=None,
        metavar="BUNDLE_DIR",
        help="verify a produced bundle against its hash inventory",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    config_path = Path(args.config).resolve()
    try:
        if args.self_check:
            config = load_config(config_path)
            return self_check(Path(args.self_check).resolve(), config)
        config = load_config(config_path)
        repo_root = resolve_repo_root(args.repo_root)
        output = (
            Path(args.output).resolve()
            if args.output
            else repo_root / "benchmarks" / "repro" / "export" / config["bundle_name"]
        )
        build_bundle(config, repo_root, output, args.force)
        return 0
    except ExportError as error:
        log_err(str(error))
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
