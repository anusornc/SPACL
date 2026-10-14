#!/usr/bin/env python3
"""Generate the OWL2Bench scalability ladder datasets under ADR 0026.

This command owns scalability input preparation only. It drives the pinned
official OWL2Bench generator once per (profile, universities) ladder point,
runs every generation twice in isolated workspaces, and requires the two
outputs to be byte-identical before staging anything. The one-university
seed-1 correctness anchors (ADR 0015/0017) are reused by hash as the first
ladder point and are never regenerated. It never reasons over the datasets
and never creates an answer key or oracle.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

PROFILES = ("DL", "EL", "QL", "RL")
SCHEMA_VERSION = 1
SEED = 1
ANCHOR_UNIVERSITIES = 1
DEFAULT_SIZES = (2, 5, 10, 20, 50)
DEFAULT_TIMEOUT_SECONDS = 3600
DEFAULT_TIME_BIN = "/usr/bin/time"
GENERATOR_COMMAND = "java -jar OWL2Bench.jar <universities> <PROFILE> <seed>"
MANIFEST_FILENAME = "ladder-manifest.json"
GENERATOR_ASSETS = (
    "OWL2Bench.jar",
    "config.properties",
    "RandomNames.xlsx",
    "UNIV-BENCH-OWL2DL.owl",
    "UNIV-BENCH-OWL2EL.owl",
    "UNIV-BENCH-OWL2QL.owl",
    "UNIV-BENCH-OWL2RL.owl",
)
ANCHOR_INVOCATION = re.compile(
    r"^kracr/owl2bench@(?P<revision>[0-9a-f]{40}):OWL2Bench\.jar"
    r"\s+(?P<universities>\d+)\s+(?P<profile>DL|EL|QL|RL)\s+(?P<seed>\d+)$"
)
RSS_LINE = re.compile(r"Maximum resident set size \(kbytes\):\s*(\d+)")

# Pins from ADR 0015/0017: the preserved official generator and the staged
# seed-1 university-1 anchor bytes. Generation fails closed if either drifts.
PINNED_REVISION = "5a27536c8e7f7621b34c61c8a57ed9eb63e7829a"
PINNED_JAR_SHA256 = "c2bacf1d80e91ec05625b4c458f899a626ef92b85c60c6bac1f0a519060839ff"
PINNED_ANCHOR_SHA256 = {
    "DL": "fce9e2c1fa58ffd8bd5deb7de224c1f4a7d8c0eec0c49dce0d42491e1854ed77",
    "EL": "49ac036c92153c0995f009ffea03d9843372eef5ffa29fd93f6533f6165f62c1",
    "QL": "d2c5037f49f8a71488b3e2e2b8d82057b18bda931319eb4b7cf3d09a41e17d9c",
    "RL": "78d7a0780e8cf7bf5d375d2a13b30c6bad8fa5c0763854770c6ae7f6fb9104f3",
}
PINNED_ANCHOR_ADR = {
    "DL": "docs/adr/0017-seed-generated-anchors-for-el-ql-dl.md",
    "EL": "docs/adr/0017-seed-generated-anchors-for-el-ql-dl.md",
    "QL": "docs/adr/0017-seed-generated-anchors-for-el-ql-dl.md",
    "RL": "docs/adr/0015-seed-generated-rl-correctness-anchor.md",
}

REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_OUTPUT_DIR = REPO_ROOT / "benchmarks/ontologies/external/owl2bench/scalability"
DEFAULT_ANCHORS_MANIFEST = (
    REPO_ROOT / "benchmarks/ontologies/external/owl2bench/staged/manifest.tsv"
)


class LadderError(RuntimeError):
    """The scalability ladder cannot satisfy its reproducibility contract."""


class SourceUnavailableError(LadderError):
    """An external benchmark source or prerequisite cannot be verified."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def sha256_text(value: str) -> str:
    return "sha256:" + hashlib.sha256(value.encode("utf-8")).hexdigest()


def json_digest(value: Any) -> str:
    payload = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return sha256_text(payload)


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def normalize_hash(value: str) -> str:
    return value.strip().removeprefix("sha256:").lower()


def require_file(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file() or not os.access(resolved, os.R_OK):
        raise SourceUnavailableError(f"{label} is missing or unreadable: {resolved}")
    return resolved


def parse_profiles(value: str) -> tuple[str, ...]:
    profiles = tuple(part.strip().upper() for part in value.split(",") if part.strip())
    if not profiles or any(profile not in PROFILES for profile in profiles):
        raise LadderError(f"profiles must contain only {','.join(PROFILES)}")
    if len(set(profiles)) != len(profiles):
        raise LadderError("profiles must not contain duplicates")
    return profiles


def parse_sizes(value: str) -> tuple[int, ...]:
    sizes: list[int] = []
    for part in value.split(","):
        part = part.strip()
        if not part:
            continue
        try:
            parsed = int(part)
        except ValueError as error:
            raise LadderError("sizes must be positive integers") from error
        if parsed <= 0:
            raise LadderError("sizes must be positive integers")
        sizes.append(parsed)
    if not sizes:
        raise LadderError("at least one generation size is required")
    if len(set(sizes)) != len(sizes):
        raise LadderError("sizes must not contain duplicates")
    if ANCHOR_UNIVERSITIES in sizes:
        raise LadderError(
            "universities=1 is the anchor ladder point, reused by hash from the "
            "staged seed-1 anchors (ADR 0015/0017); it is never regenerated"
        )
    return tuple(sorted(sizes))


def parse_anchor_pins(pairs: list[str] | None) -> dict[str, str]:
    pins: dict[str, str] = dict(PINNED_ANCHOR_SHA256)
    for pair in pairs or []:
        profile, separator, value = pair.partition("=")
        profile = profile.strip().upper()
        if not separator or profile not in PROFILES or not value.strip():
            raise LadderError(f"anchor pin must be PROFILE=<sha256>, got: {pair}")
        pins[profile] = normalize_hash(value)
    return pins


def resolve_revision(source_dir: Path, explicit: str) -> str:
    if explicit.strip():
        return explicit.strip()
    try:
        result = subprocess.run(
            ["git", "-C", str(source_dir), "rev-parse", "HEAD"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise SourceUnavailableError("cannot determine OWL2Bench source revision") from error
    revision = result.stdout.strip()
    if result.returncode != 0 or not revision:
        raise SourceUnavailableError(
            "OWL2Bench source is not a repository; pass --source-revision explicitly"
        )
    return revision


def locate_asset(source_dir: Path, filename: str) -> Path:
    candidates = (source_dir / filename, source_dir / "OWL2Bench" / filename)
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    raise SourceUnavailableError(
        f"official OWL2Bench asset {filename} was not found under {source_dir}"
    )


def verify_generator_identity(
    source_dir: Path, explicit_revision: str, expected_revision: str, expected_jar_sha256: str
) -> tuple[Path, str, dict[str, Path]]:
    assets = {name: locate_asset(source_dir, name) for name in GENERATOR_ASSETS}
    jar = assets["OWL2Bench.jar"]
    actual_jar = normalize_hash(sha256_file(jar))
    expected_jar = normalize_hash(expected_jar_sha256)
    if actual_jar != expected_jar:
        raise SourceUnavailableError(
            "pinned OWL2Bench generator jar hash mismatch: expected "
            f"sha256:{expected_jar}, got sha256:{actual_jar} ({jar})"
        )
    revision = resolve_revision(source_dir, explicit_revision)
    if expected_revision.strip() and revision != expected_revision.strip():
        raise SourceUnavailableError(
            f"OWL2Bench source revision mismatch: expected {expected_revision.strip()},"
            f" got {revision}"
        )
    return jar, revision, assets


def load_anchor_rows(anchors_manifest: Path, profiles: tuple[str, ...], seed: int, revision: str) -> dict[str, dict[str, Any]]:
    anchors_manifest = require_file(anchors_manifest, "staged anchors manifest")
    rows: dict[str, dict[str, Any]] = {}
    with anchors_manifest.open("r", encoding="utf-8", newline="") as handle:
        for row in csv.reader(handle, delimiter="\t"):
            if len(row) < 7:
                continue
            match = ANCHOR_INVOCATION.match(row[1].strip())
            if not match:
                continue
            if match.group("universities") != str(ANCHOR_UNIVERSITIES):
                continue
            if match.group("seed") != str(seed):
                continue
            if row[6].strip() != "correctness":
                continue
            profile = match.group("profile")
            if row[2].strip().upper() != profile:
                raise LadderError(
                    f"staged anchor row profile column disagrees with its invocation: {row}"
                )
            if match.group("revision") != revision:
                raise LadderError(
                    f"staged anchor row was generated by revision "
                    f"{match.group('revision')}, not the verified revision {revision}"
                )
            if profile in rows:
                raise LadderError(f"multiple staged anchor rows for profile {profile}")
            rows[profile] = {
                "path": Path(row[0].strip()),
                "invocation": row[1].strip(),
                "manifest": anchors_manifest,
            }
    missing = [profile for profile in profiles if profile not in rows]
    if missing:
        raise SourceUnavailableError(
            f"staged anchors manifest has no seed-{seed} university-{ANCHOR_UNIVERSITIES} "
            f"anchor row for: {','.join(missing)}"
        )
    return rows


def anchor_point(
    profile: str,
    row: dict[str, Any],
    seed: int,
    revision: str,
    jar_sha256: str,
    pin: str,
) -> dict[str, Any]:
    path = require_file(row["path"], f"staged {profile} university-1 anchor")
    actual = normalize_hash(sha256_file(path))
    if actual != normalize_hash(pin):
        raise LadderError(
            f"staged {profile} university-1 anchor hash drift: expected "
            f"sha256:{normalize_hash(pin)}, got sha256:{actual} ({path})"
        )
    staged_name = row["path"].name
    return {
        "profile": profile,
        "universities": ANCHOR_UNIVERSITIES,
        "seed": seed,
        "origin": "anchor-reuse",
        "generated": False,
        "stage": "scalability",
        "invocation": f"kracr/owl2bench@{revision}:OWL2Bench.jar {ANCHOR_UNIVERSITIES} {profile} {seed}",
        "generator_revision": revision,
        "generator_jar_sha256": jar_sha256,
        "output": {
            "path": str(path),
            "repo_relative_path": repo_relative(path),
            "sha256": sha256_file(path),
            "bytes": path.stat().st_size,
        },
        "reused_from": {
            "anchors_manifest": str(row["manifest"]),
            "staged_name": staged_name,
            "pinned_sha256": "sha256:" + normalize_hash(pin),
            "adr": PINNED_ANCHOR_ADR[profile],
        },
        "determinism": None,
        "actuals": None,
        "resumed": False,
    }


def repo_relative(path: Path) -> str | None:
    resolved = path.expanduser().resolve()
    try:
        return str(resolved.relative_to(REPO_ROOT))
    except ValueError:
        return None


def staged_name(profile: str, universities: int, seed: int) -> str:
    return f"UNIV-BENCH-OWL2{profile}-seed{seed}-universities{universities}.owl"


def build_workspace(assets: dict[str, Path], workspace: Path) -> dict[str, Path]:
    workspace.mkdir(parents=True, exist_ok=True)
    copied: dict[str, Path] = {}
    for name, source in assets.items():
        destination = workspace / name
        shutil.copy2(source, destination)
        copied[name] = destination
    return copied


def run_once(
    java_bin: str,
    time_bin: str | None,
    assets: dict[str, Path],
    profile: str,
    universities: int,
    seed: int,
    timeout_seconds: int,
    diagnostics_path: Path,
    time_report_path: Path,
) -> tuple[Path, float, int | None]:
    """Run the generator once in a fresh workspace and return its output."""

    diagnostics_path.parent.mkdir(parents=True, exist_ok=True)
    time_report_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix=f"owl2bench-ladder-{profile.lower()}-{universities}-"
    ) as temporary:
        workspace = Path(temporary)
        copied = build_workspace(assets, workspace)
        jar_name = copied["OWL2Bench.jar"].name
        generator_command = [
            java_bin,
            "-jar",
            jar_name,
            str(universities),
            profile,
            str(seed),
        ]
        if time_bin:
            command = [time_bin, "-v", "-o", str(time_report_path), *generator_command]
        else:
            command = generator_command
        started = time.monotonic()
        process = subprocess.Popen(
            command,
            cwd=workspace,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            start_new_session=True,
        )
        try:
            output, _ = process.communicate(timeout=timeout_seconds)
        except subprocess.TimeoutExpired:
            os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            output, _ = process.communicate()
            diagnostics_path.parent.mkdir(parents=True, exist_ok=True)
            diagnostics_path.write_text(output or "", encoding="utf-8")
            raise LadderError(
                f"generation for {profile} universities={universities} exceeded the "
                f"{timeout_seconds}s per-run budget; see {diagnostics_path}"
            ) from None
        wall_seconds = round(time.monotonic() - started, 3)
        diagnostics_path.parent.mkdir(parents=True, exist_ok=True)
        diagnostics_path.write_text(output or "", encoding="utf-8")
        if process.returncode != 0:
            raise LadderError(
                f"official OWL2Bench generator failed for {profile} "
                f"universities={universities}: exit {process.returncode}; see {diagnostics_path}"
            )
        expected = workspace / f"OWL2{profile}-{universities}.owl"
        if not expected.is_file():
            candidates = sorted(workspace.glob(f"OWL2{profile}-{universities}.*"))
            if len(candidates) != 1:
                raise LadderError(
                    f"generator did not produce one expected {profile} "
                    f"universities={universities} dataset; see {diagnostics_path}"
                )
            expected = candidates[0]
        retained = diagnostics_path.parent / f"{diagnostics_path.stem}-dataset-{expected.name}"
        shutil.copy2(expected, retained)
        peak_rss_kib: int | None = None
        if time_bin and time_report_path.is_file():
            match = RSS_LINE.search(time_report_path.read_text(encoding="utf-8", errors="replace"))
            if match:
                peak_rss_kib = int(match.group(1))
        return retained.resolve(), wall_seconds, peak_rss_kib


def generate_point(
    java_bin: str,
    time_bin: str | None,
    assets: dict[str, Path],
    profile: str,
    universities: int,
    seed: int,
    timeout_seconds: int,
    output_dir: Path,
    logs_dir: Path,
    revision: str,
    jar_sha256: str,
) -> dict[str, Any]:
    stem = staged_name(profile, universities, seed)
    run1, wall1, rss1 = run_once(
        java_bin,
        time_bin,
        assets,
        profile,
        universities,
        seed,
        timeout_seconds,
        logs_dir / f"{stem}-run1.log",
        logs_dir / f"{stem}-run1.time.txt",
    )
    run2, wall2, rss2 = run_once(
        java_bin,
        time_bin,
        assets,
        profile,
        universities,
        seed,
        timeout_seconds,
        logs_dir / f"{stem}-run2.log",
        logs_dir / f"{stem}-run2.time.txt",
    )
    hash1 = sha256_file(run1)
    hash2 = sha256_file(run2)
    if hash1 != hash2:
        raise LadderError(
            f"byte-determinism mismatch for {profile} universities={universities}: "
            f"run1 {hash1} != run2 {hash2}; nothing staged for this dataset"
        )
    final = output_dir / stem
    final.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(run1, final)
    staged_hash = sha256_file(final)
    if staged_hash != hash1:
        raise LadderError(
            f"staged copy of {profile} universities={universities} does not match the "
            f"verified run output: {staged_hash} != {hash1}"
        )
    rss_recorded = rss1 is not None and rss2 is not None
    return {
        "profile": profile,
        "universities": universities,
        "seed": seed,
        "origin": "generated",
        "generated": True,
        "stage": "scalability",
        "invocation": f"kracr/owl2bench@{revision}:OWL2Bench.jar {universities} {profile} {seed}",
        "generator_revision": revision,
        "generator_jar_sha256": jar_sha256,
        "output": {
            "path": str(final.resolve()),
            "repo_relative_path": repo_relative(final),
            "sha256": staged_hash,
            "bytes": final.stat().st_size,
        },
        "determinism": {
            "runs": 2,
            "run1_sha256": hash1,
            "run2_sha256": hash2,
            "status": "byte-identical",
        },
        "actuals": {
            "wall_time_seconds": {"run1": wall1, "run2": wall2},
            "peak_rss_kib": {"run1": rss1, "run2": rss2},
            "rss_recorded": rss_recorded,
            "timeout_seconds_per_run": timeout_seconds,
        },
        "diagnostics": {
            "run1_log": str((logs_dir / f"{stem}-run1.log").resolve()),
            "run2_log": str((logs_dir / f"{stem}-run2.log").resolve()),
        },
        "resumed": False,
    }


def previous_verified_points(manifest_path: Path) -> dict[tuple[str, int], dict[str, Any]]:
    if not manifest_path.is_file():
        return {}
    try:
        previous = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return {}
    points: dict[tuple[str, int], dict[str, Any]] = {}
    for point in previous.get("points", []) if isinstance(previous, dict) else []:
        if not isinstance(point, dict):
            continue
        determinism = point.get("determinism")
        if point.get("origin") != "generated" or not isinstance(determinism, dict):
            continue
        if determinism.get("status") != "byte-identical":
            continue
        key = (point.get("profile"), point.get("universities"))
        if isinstance(key[0], str) and isinstance(key[1], int):
            points[key] = point
    return points


def build_manifest(
    revision: str,
    jar_sha256: str,
    seed: int,
    profiles: tuple[str, ...],
    sizes: tuple[int, ...],
    points: list[dict[str, Any]],
    timeout_seconds: int,
    time_bin: str | None,
    source_dir: Path,
) -> dict[str, Any]:
    manifest: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "manifest_id": f"owl2bench-scalability-ladder:seed={seed}",
        "adr": "docs/adr/0026-owl2bench-multisize-consistency-performance-lane.md",
        "status": "complete",
        "source_corpus": "owl2bench",
        "generator": {
            "name": "OWL2Bench",
            "version": "official-jar-artifact",
            "revision": revision,
            "jar_sha256": jar_sha256,
            "command": GENERATOR_COMMAND,
            "seed": seed,
            "source_directory": str(source_dir),
            "coverage": {
                "profiles": list(profiles),
                "generated_sizes": list(sizes),
                "anchor_universities": ANCHOR_UNIVERSITIES,
            },
        },
        "budget": {
            "timeout_seconds_per_run": timeout_seconds,
            "wall_time_source": "time.monotonic around each generator run",
            "peak_rss_source": (
                f"{time_bin} -v (Maximum resident set size)"
                if time_bin
                else "unavailable: no GNU time binary"
            ),
        },
        "points": points,
        "no_answer_keys": True,
    }
    manifest["identity_sha256"] = json_digest(
        {
            "manifest_id": manifest["manifest_id"],
            "generator": {"revision": revision, "jar_sha256": jar_sha256},
            "seed": seed,
            "points": [
                {
                    "profile": point["profile"],
                    "universities": point["universities"],
                    "origin": point["origin"],
                    "output_sha256": point["output"]["sha256"],
                    "determinism_status": (point["determinism"] or {}).get("status"),
                }
                for point in points
            ],
        }
    )
    return manifest


def generate(args: argparse.Namespace) -> int:
    source_dir = require_file(args.source_dir / "README.md", "OWL2Bench source README").parent
    profiles = parse_profiles(args.profiles)
    sizes = parse_sizes(args.sizes)
    seed = args.seed
    if seed < 0:
        raise LadderError("seed must be a non-negative integer")
    if args.timeout_seconds <= 0:
        raise LadderError("timeout-seconds must be positive")
    jar, revision, assets = verify_generator_identity(
        source_dir, args.source_revision, args.expected_revision, args.expected_jar_sha256
    )
    jar_sha256 = sha256_file(jar)
    anchor_pins = parse_anchor_pins(args.anchor_pin)
    anchors = load_anchor_rows(args.anchors_manifest, profiles, seed, revision)

    output_dir = args.output_dir.expanduser().resolve()
    manifest_path = (
        args.manifest.expanduser().resolve() if args.manifest else output_dir / MANIFEST_FILENAME
    )
    logs_dir = output_dir / "logs"
    previous = {} if args.force else previous_verified_points(manifest_path)

    points: list[dict[str, Any]] = []
    generated = 0
    resumed = 0
    reused = 0
    for profile in profiles:
        anchor = anchor_point(
            profile, anchors[profile], seed, revision, jar_sha256, anchor_pins[profile]
        )
        points.append(anchor)
        reused += 1
        for universities in sizes:
            key = (profile, universities)
            candidate = previous.get(key)
            final = output_dir / staged_name(profile, universities, seed)
            if (
                candidate is not None
                and final.is_file()
                and sha256_file(final) == candidate.get("output", {}).get("sha256")
            ):
                resumed_entry = dict(candidate)
                resumed_entry["resumed"] = True
                points.append(resumed_entry)
                resumed += 1
                continue
            points.append(
                generate_point(
                    args.java_bin,
                    args.time_bin or None,
                    assets,
                    profile,
                    universities,
                    seed,
                    args.timeout_seconds,
                    output_dir,
                    logs_dir,
                    revision,
                    jar_sha256,
                )
            )
            generated += 1
    points.sort(key=lambda point: (PROFILES.index(point["profile"]), point["universities"]))

    manifest = build_manifest(
        revision,
        jar_sha256,
        seed,
        profiles,
        sizes,
        points,
        args.timeout_seconds,
        args.time_bin or None,
        source_dir,
    )
    write_json(manifest_path, manifest)
    validation = validate_manifest(
        manifest_path,
        expect_profiles=profiles,
        expect_sizes=sizes,
        expect_revision=revision,
        expect_jar_sha256=jar_sha256,
        anchor_pins=anchor_pins,
        shape_only=False,
    )
    status = "complete" if validation["status"] == "complete" else "failed"
    print(
        json.dumps(
            {
                "status": status,
                "manifest": str(manifest_path),
                "generated": generated,
                "resumed": resumed,
                "anchor_reuse": reused,
                "validation_errors": validation["errors"],
            },
            sort_keys=True,
        )
    )
    return 0 if status == "complete" else 1


def resolve_point_file(point: dict[str, Any]) -> Path | None:
    path = point.get("output", {}).get("path")
    if isinstance(path, str) and Path(path).is_file():
        return Path(path)
    relative = point.get("output", {}).get("repo_relative_path")
    if isinstance(relative, str):
        candidate = REPO_ROOT / relative
        if candidate.is_file():
            return candidate
    if isinstance(path, str):
        return Path(path)
    if isinstance(relative, str):
        return REPO_ROOT / relative
    return None


def validate_manifest(
    manifest_path: Path,
    expect_profiles: tuple[str, ...],
    expect_sizes: tuple[int, ...],
    expect_revision: str,
    expect_jar_sha256: str,
    anchor_pins: dict[str, str],
    shape_only: bool,
) -> dict[str, Any]:
    manifest_path = require_file(manifest_path, "ladder manifest")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise LadderError(f"ladder manifest is malformed: {error}") from error
    if not isinstance(manifest, dict):
        raise LadderError("ladder manifest root must be an object")

    errors: list[str] = []
    if manifest.get("schema_version") != SCHEMA_VERSION:
        errors.append("schema_version_invalid")
    if manifest.get("no_answer_keys") is not True:
        errors.append("answer_key_policy_missing")
    generator = manifest.get("generator")
    if not isinstance(generator, dict):
        errors.append("generator_missing")
        generator = {}
    revision = generator.get("revision")
    jar_sha256 = generator.get("jar_sha256")
    seed = generator.get("seed")
    if not isinstance(revision, str) or not revision:
        errors.append("generator_revision_missing")
    if not isinstance(jar_sha256, str) or not re.fullmatch(
        r"sha256:[0-9a-f]{64}", normalize_hash_with_prefix(jar_sha256)
    ):
        errors.append("generator_jar_sha256_invalid")
    if not isinstance(seed, int) or seed < 0:
        errors.append("generator_seed_invalid")
    if isinstance(revision, str) and expect_revision.strip() and revision != expect_revision.strip():
        errors.append("generator_revision_mismatch")
    if isinstance(jar_sha256, str) and expect_jar_sha256.strip():
        if normalize_hash(jar_sha256) != normalize_hash(expect_jar_sha256):
            errors.append("generator_jar_sha256_mismatch")
    if generator.get("command") != GENERATOR_COMMAND:
        errors.append("generator_command_mismatch")

    points = manifest.get("points")
    if not isinstance(points, list):
        errors.append("points_missing")
        points = []

    seen: set[tuple[str, int]] = set()
    for point in points:
        if not isinstance(point, dict):
            errors.append("point_not_an_object")
            continue
        profile = point.get("profile")
        universities = point.get("universities")
        label = f"{profile}:{universities}"
        if not isinstance(profile, str) or profile not in PROFILES:
            errors.append(f"point_profile_invalid:{label}")
            continue
        if not isinstance(universities, int) or universities <= 0:
            errors.append(f"point_universities_invalid:{label}")
            continue
        key = (profile, universities)
        if key in seen:
            errors.append(f"point_duplicate:{label}")
            continue
        seen.add(key)
        if point.get("seed") != seed:
            errors.append(f"point_seed_mismatch:{label}")
        expected_invocation = (
            f"kracr/owl2bench@{revision}:OWL2Bench.jar {universities} {profile} {seed}"
        )
        if point.get("invocation") != expected_invocation:
            errors.append(f"invocation_mismatch:{label}")
        if point.get("generator_revision") != revision:
            errors.append(f"generator_revision_mismatch:{label}")
        if normalize_hash(str(point.get("generator_jar_sha256", ""))) != normalize_hash(
            str(jar_sha256 or "")
        ):
            errors.append(f"generator_jar_mismatch:{label}")
        output = point.get("output")
        output_sha: str | None = None
        if not isinstance(output, dict) or not isinstance(output.get("sha256"), str):
            errors.append(f"output_sha256_missing:{label}")
        else:
            output_sha = output["sha256"]
            if not re.fullmatch(r"[0-9a-f]{64}", normalize_hash(output_sha)):
                errors.append(f"output_sha256_invalid:{label}")
        if not shape_only and isinstance(output, dict):
            candidate = resolve_point_file(point)
            if candidate is None or not candidate.is_file():
                errors.append(f"output_missing:{label}")
            elif output_sha is not None and sha256_file(candidate) != output_sha:
                errors.append(f"output_hash_mismatch:{label}")
        if universities == ANCHOR_UNIVERSITIES:
            if point.get("origin") != "anchor-reuse" or point.get("generated") is not False:
                errors.append(f"anchor_origin_invalid:{label}")
            if point.get("actuals") is not None:
                errors.append(f"anchor_actuals_present:{label}")
            reused = point.get("reused_from")
            if not isinstance(reused, dict) or not reused.get("anchors_manifest"):
                errors.append(f"anchor_reuse_record_missing:{label}")
            if profile in anchor_pins and output_sha is not None:
                if normalize_hash(output_sha) != normalize_hash(anchor_pins[profile]):
                    errors.append(f"anchor_pin_mismatch:{profile}")
        else:
            if point.get("origin") != "generated" or point.get("generated") is not True:
                errors.append(f"generated_origin_invalid:{label}")
                continue
            determinism = point.get("determinism")
            if not isinstance(determinism, dict):
                errors.append(f"determinism_missing:{label}")
            else:
                if determinism.get("runs") != 2:
                    errors.append(f"determinism_runs:{label}")
                if determinism.get("status") != "byte-identical":
                    errors.append(f"determinism_status:{label}")
                run1 = determinism.get("run1_sha256")
                run2 = determinism.get("run2_sha256")
                if run1 != run2 or (output_sha is not None and run1 != output_sha):
                    errors.append(f"determinism_mismatch:{label}")
            actuals = point.get("actuals")
            if not isinstance(actuals, dict):
                errors.append(f"actuals_missing:{label}")
            else:
                walls = actuals.get("wall_time_seconds")
                if (
                    not isinstance(walls, dict)
                    or not isinstance(walls.get("run1"), (int, float))
                    or not isinstance(walls.get("run2"), (int, float))
                    or walls.get("run1") <= 0
                    or walls.get("run2") <= 0
                ):
                    errors.append(f"actuals_wall_time_invalid:{label}")
                rss = actuals.get("peak_rss_kib")
                rss_recorded = actuals.get("rss_recorded")
                if rss_recorded:
                    if (
                        not isinstance(rss, dict)
                        or not isinstance(rss.get("run1"), int)
                        or not isinstance(rss.get("run2"), int)
                        or rss.get("run1") <= 0
                        or rss.get("run2") <= 0
                    ):
                        errors.append(f"actuals_peak_rss_invalid:{label}")

    expected_points = {
        (profile, ANCHOR_UNIVERSITIES) for profile in expect_profiles
    } | {
        (profile, universities) for profile in expect_profiles for universities in expect_sizes
    }
    for profile, universities in sorted(expected_points - seen):
        errors.append(f"point_missing:{profile}:{universities}")
    for profile, universities in sorted(seen - expected_points):
        errors.append(f"point_unexpected:{profile}:{universities}")

    return {
        "schema_version": SCHEMA_VERSION,
        "manifest": str(manifest_path),
        "status": "complete" if not errors else "failed",
        "errors": errors,
        "point_count": len(seen),
        "shape_only": shape_only,
    }


def normalize_hash_with_prefix(value: str) -> str:
    return "sha256:" + normalize_hash(value)


def detect_time_bin(value: str) -> str | None:
    if value == "auto":
        return DEFAULT_TIME_BIN if Path(DEFAULT_TIME_BIN).is_file() else None
    if not value.strip():
        return None
    if not Path(value).is_file():
        raise SourceUnavailableError(f"time binary is missing or unreadable: {value}")
    return value


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    generate_parser = subparsers.add_parser("generate")
    generate_parser.add_argument("--source-dir", type=Path, required=True)
    generate_parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    generate_parser.add_argument("--manifest", type=Path)
    generate_parser.add_argument("--anchors-manifest", type=Path, default=DEFAULT_ANCHORS_MANIFEST)
    generate_parser.add_argument("--profiles", default=",".join(PROFILES))
    generate_parser.add_argument("--sizes", default=",".join(str(size) for size in DEFAULT_SIZES))
    generate_parser.add_argument("--seed", type=int, default=SEED)
    generate_parser.add_argument("--java-bin", default="java")
    generate_parser.add_argument("--time-bin", default="auto")
    generate_parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=int(os.environ.get("OWL2BENCH_LADDER_TIMEOUT_SECONDS", DEFAULT_TIMEOUT_SECONDS)),
    )
    generate_parser.add_argument("--source-revision", default="")
    generate_parser.add_argument("--expected-revision", default=PINNED_REVISION)
    generate_parser.add_argument("--expected-jar-sha256", default=PINNED_JAR_SHA256)
    generate_parser.add_argument("--anchor-pin", action="append")
    generate_parser.add_argument("--force", action="store_true")

    validate_parser = subparsers.add_parser("validate")
    validate_parser.add_argument("--manifest", type=Path, required=True)
    validate_parser.add_argument("--expect-profiles", default=",".join(PROFILES))
    validate_parser.add_argument(
        "--expect-sizes", default=",".join(str(size) for size in DEFAULT_SIZES)
    )
    validate_parser.add_argument("--expect-revision", default=PINNED_REVISION)
    validate_parser.add_argument("--expect-jar-sha256", default=PINNED_JAR_SHA256)
    validate_parser.add_argument("--anchor-pin", action="append")
    validate_parser.add_argument("--shape-only", action="store_true")

    args = parser.parse_args(argv)
    try:
        if args.command == "generate":
            args.time_bin = detect_time_bin(args.time_bin)
            return generate(args)
        if args.command == "validate":
            result = validate_manifest(
                args.manifest,
                expect_profiles=parse_profiles(args.expect_profiles),
                expect_sizes=parse_sizes(args.expect_sizes),
                expect_revision=args.expect_revision,
                expect_jar_sha256=args.expect_jar_sha256,
                anchor_pins=parse_anchor_pins(args.anchor_pin),
                shape_only=args.shape_only,
            )
            print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
            return 0 if result["status"] == "complete" else 1
    except SourceUnavailableError as error:
        print(
            json.dumps(
                {
                    "status": "not_available",
                    "reason": str(error),
                    "answer_key_policy": "not_permitted",
                },
                ensure_ascii=False,
                sort_keys=True,
            )
        )
        print(f"ladder dataset generator not_available: {error}", file=sys.stderr)
        return 125
    except LadderError as error:
        print(f"ladder dataset generator failed: {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"ladder dataset generator failed: {error}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
