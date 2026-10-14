#!/usr/bin/env python3
"""Aggregate W3C, ORE, OWL2Bench, and query-lane artifacts safely."""

from __future__ import annotations

import argparse
import csv
import json
import re
from datetime import datetime, timezone
from pathlib import Path


QUERY_PROFILES = ("DL", "EL", "QL", "RL")
QUERY_PROFILE_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
QUERY_CASE_COUNT = sum(QUERY_PROFILE_COUNTS.values())
SHA256_RE = re.compile(r"^(?:sha256:)?[0-9a-fA-F]{64}$")
RECORDED_MS_RE = re.compile(r"^[0-9]+$")
# ADR 0024/0027: the campaign correctness conjunction covers
# correctness-kind suites only; the ORE and OWL2Bench stage suites are
# performance lanes that adjudicate no correctness and can never block or
# pass the campaign gate.
SUITE_LANE_KINDS = {
    "w3c": "correctness",
    "owl2bench": "performance",
    "owl2bench-query": "correctness",
    "ore": "performance",
}


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def valid_sha256(value: object) -> bool:
    return isinstance(value, str) and SHA256_RE.fullmatch(value) is not None


def validate_split_identity_list(
    value: object,
    *,
    identity_field: str,
    required_profiles: set[str],
    label: str,
    errors: list[str],
) -> None:
    if not isinstance(value, list) or not value:
        errors.append(f"{label} identities are missing")
        return
    observed_profiles: set[str] = set()
    for identity in value:
        if not isinstance(identity, dict):
            errors.append(f"{label} identity is malformed")
            continue
        profile = identity.get("profile")
        if not isinstance(profile, str) or not profile:
            errors.append(f"{label} identity profile is missing")
        else:
            observed_profiles.add(profile)
        if not valid_sha256(identity.get(identity_field)):
            errors.append(f"{label} identity hash is invalid")
    if observed_profiles != required_profiles:
        missing = ",".join(sorted(required_profiles - observed_profiles)) or "none"
        extra = ",".join(sorted(observed_profiles - required_profiles)) or "none"
        errors.append(f"{label} identity profile coverage mismatch (missing={missing}, extra={extra})")


def artifact_paths(run_dir: Path) -> tuple[list[str], list[str]]:
    names = [
        "run_metadata.json",
        "summary.json",
        "report.md",
        "results.csv",
        "ore_consistency_report.md",
        "ore_consistency_details.tsv",
        "owl2bench_stage_report.md",
        "owl2bench_stage_results.csv",
    ]
    aggregate = [str(run_dir / name) for name in names if (run_dir / name).exists()]
    raw = [
        str(path)
        for path in sorted(run_dir.glob("*.json"))
        if path.name not in {"run_metadata.json", "summary.json", "ore_metadata.json"}
    ]
    raw.extend(str(path) for path in sorted((run_dir / "raw").rglob("*")) if path.is_file())
    for raw_dir in (run_dir / "raw", run_dir / "ontologies"):
        if raw_dir.exists():
            raw.append(str(raw_dir))
    return aggregate, raw


def run_metadata(run_dir: Path) -> dict:
    metadata = {}
    for name in ("run_metadata.json", "ore_metadata.json"):
        path = run_dir / name
        if path.exists():
            metadata[name] = load_json(path)
    return metadata


def w3c_suite(run_dir: Path) -> dict:
    cases = []
    for path in sorted(run_dir.glob("*.json")):
        if path.name in {"run_metadata.json", "summary.json"}:
            continue
        result = load_json(path)
        if result.get("schema_version") == 1:
            cases.append(result)
    baseline_cases = [case for case in cases if case.get("w3c_lane_status") == "baseline"]
    counts: dict[str, int] = {}
    for case in cases:
        status = case.get("result_status", "unknown")
        counts[status] = counts.get(status, 0) + 1
    baseline_counts: dict[str, int] = {}
    for case in baseline_cases:
        status = case.get("result_status", "unknown")
        baseline_counts[status] = baseline_counts.get(status, 0) + 1
    gate = "passed" if baseline_cases and all(case.get("result_status") == "passed" for case in baseline_cases) else "blocked"
    aggregate, raw = artifact_paths(run_dir)
    return {
        "status": gate,
        "correctness_gate": gate,
        "case_count": len(cases),
        "baseline_case_count": len(baseline_cases),
        "result_status_counts": counts,
        "baseline_result_status_counts": baseline_counts,
        "run_dir": str(run_dir),
        "artifacts": aggregate,
        "raw_result_artifacts": raw,
        "metadata": run_metadata(run_dir),
    }


def ore_suite(run_dir: Path) -> dict:
    """Load the ORE run as an informational performance lane (ADR 0024).

    No public oracle exists for the ORE 2015 consistency sample, so the lane
    declares no correctness gate: rows keep their honest oracle_unavailable
    status and the suite never blocks or passes the campaign gate.
    """
    details = run_dir / "ore_consistency_details.tsv"
    rows = []
    if details.exists():
        with details.open(newline="", encoding="utf-8") as handle:
            rows = list(csv.DictReader(handle, delimiter="\t"))
    counts: dict[str, int] = {}
    for row in rows:
        status = row.get("result_status", "unknown")
        counts[status] = counts.get(status, 0) + 1
    aggregate, raw = artifact_paths(run_dir)
    return {
        "status": "informational",
        "correctness_gate": "not_applicable",
        "case_count": len(rows),
        "result_status_counts": counts,
        "run_dir": str(run_dir),
        "artifacts": aggregate,
        "raw_result_artifacts": raw,
        "metadata": run_metadata(run_dir),
    }


def stage_suite(run_dir: Path) -> dict:
    """Load the OWL2Bench stage run as an informational performance lane (ADR 0027).

    No oracle exists for OWL2Bench consistency, so the lane declares no
    correctness gate: correctness rows keep their honest oracle_unavailable
    status while loads, consistency executions, and recorded timing stay
    visible as execution evidence. The suite never blocks or passes the
    campaign gate.
    """
    results = run_dir / "owl2bench_stage_results.csv"
    rows = []
    if results.exists():
        with results.open(newline="", encoding="utf-8") as handle:
            rows = list(csv.DictReader(handle))
    result_counts: dict[str, int] = {}
    execution_counts: dict[str, int] = {}
    timing_recorded = {"wall_time_ms": 0, "parse_time_ms": 0, "reason_time_ms": 0}
    for row in rows:
        status = row.get("result_status", "unknown")
        result_counts[status] = result_counts.get(status, 0) + 1
        execution = row.get("execution_status", "unknown")
        execution_counts[execution] = execution_counts.get(execution, 0) + 1
        for field in timing_recorded:
            # Timing fields may be recorded as -1 (not recorded); only
            # plain non-negative integers count as recorded evidence.
            if RECORDED_MS_RE.fullmatch(row.get(field) or ""):
                timing_recorded[field] += 1
    aggregate, raw = artifact_paths(run_dir)
    return {
        "status": "informational",
        "correctness_gate": "not_applicable",
        "case_count": len(rows),
        "result_status_counts": result_counts,
        "execution_status_counts": execution_counts,
        "timing_evidence": {"unit": "ms", "recorded_counts": timing_recorded},
        "run_dir": str(run_dir),
        "artifacts": aggregate,
        "raw_result_artifacts": raw,
        "metadata": run_metadata(run_dir),
    }


def generic_summary(run_dir: Path) -> dict:
    summary_path = run_dir / "summary.json"
    if summary_path.exists():
        summary = load_json(summary_path)
        gate = summary.get("correctness_gate", "blocked")
        profile_gates = summary.get("profile_gates")
        if isinstance(profile_gates, dict):
            required_profiles = ("DL", "EL", "QL", "RL")
            profile_conjunction = (
                "passed"
                if all(profile_gates.get(profile) == "passed" for profile in required_profiles)
                else "blocked"
            )
            # A profile-qualified query report is authoritative only when its
            # declared query/correctness gates agree with the four-way AND.
            # This prevents a stale or hand-edited top-level pass from
            # bypassing a blocked profile.
            if (
                profile_conjunction != "passed"
                or summary.get("query_gate") != "passed"
                or summary.get("correctness_gate") != "passed"
            ):
                gate = "blocked"
            else:
                gate = "passed"
    else:
        # Fail-closed: without summary.json the profile-qualified query
        # contract cannot be checked, so the suite stays blocked.
        gate = "blocked"
        summary = {}
        results_csv = run_dir / "results.csv"
        if results_csv.exists():
            counts: dict[str, int] = {}
            case_count = 0
            with results_csv.open(newline="", encoding="utf-8") as handle:
                for row in csv.DictReader(handle):
                    status = row.get("result_status") or row.get("status") or "unknown"
                    counts[status] = counts.get(status, 0) + 1
                    case_count += 1
            summary = {
                "case_count": case_count,
                "result_status_counts": counts,
            }
    counts = summary.get("result_status_counts", {})
    case_count = summary.get(
        "query_count", summary.get("case_count", sum(counts.values()))
    )
    aggregate, raw = artifact_paths(run_dir)
    return {
        "status": "passed" if gate == "passed" else "blocked",
        "correctness_gate": "passed" if gate == "passed" else "blocked",
        "case_count": case_count,
        "result_status_counts": counts,
        "summary": summary,
        "run_dir": str(run_dir),
        "artifacts": aggregate,
        "raw_result_artifacts": raw,
        "metadata": run_metadata(run_dir),
    }


def query_summary(run_dir: Path) -> dict:
    """Load a query report only when its full four-profile contract is present."""

    result = generic_summary(run_dir)
    summary = result.get("summary", {})
    errors: list[str] = []
    if summary.get("query_count") != QUERY_CASE_COUNT:
        errors.append(f"query_count must be {QUERY_CASE_COUNT}")
    if summary.get("profile_case_counts") != QUERY_PROFILE_COUNTS:
        errors.append("profile_case_counts must be DL=22, EL=10, QL=10, RL=18")
    split_stack = summary.get("split_stack_requested") is True
    if not split_stack:
        errors.append("split-stack declaration is required for the OWL2Bench query campaign")

    profile_gates = summary.get("profile_gates")
    if not isinstance(profile_gates, dict) or set(profile_gates) != set(QUERY_PROFILES):
        errors.append("profile_gates must contain exactly DL, EL, QL, and RL")
        profile_conjunction = "blocked"
    else:
        profile_conjunction = (
            "passed"
            if all(profile_gates[profile] == "passed" for profile in QUERY_PROFILES)
            else "blocked"
        )
        if summary.get("query_gate") != profile_conjunction:
            errors.append("query_gate must equal the four-profile conjunction")
        if summary.get("correctness_gate") != profile_conjunction:
            errors.append("correctness_gate must equal the four-profile conjunction")

    profile_reports = summary.get("profiles")
    if not isinstance(profile_reports, dict) or set(profile_reports) != set(QUERY_PROFILES):
        errors.append("profiles must contain the four generated profile summaries")
    else:
        if split_stack:
            if summary.get("component_status_counts") != {"available": QUERY_CASE_COUNT}:
                errors.append("split-stack component status counts are incomplete")
            if summary.get("materialization_status_counts") != {"complete": QUERY_CASE_COUNT}:
                errors.append("split-stack materialization status counts are incomplete")
            validate_split_identity_list(
                summary.get("component_identities"),
                identity_field="manifest_identity",
                required_profiles=set(QUERY_PROFILES),
                label="split-stack component",
                errors=errors,
            )
            validate_split_identity_list(
                summary.get("materialization_identities"),
                identity_field="materialization_identity",
                required_profiles=set(QUERY_PROFILES),
                label="split-stack materialization",
                errors=errors,
            )
        generated_profiles_dir = run_dir / "profiles"
        if not generated_profiles_dir.is_dir():
            errors.append("generated profile report directory is missing")
        for profile in QUERY_PROFILES:
            profile_report = profile_reports.get(profile)
            expected_count = QUERY_PROFILE_COUNTS[profile]
            if not isinstance(profile_report, dict):
                errors.append(f"{profile} profile summary is missing")
                continue
            profile_summary_path = generated_profiles_dir / profile / "summary.json"
            profile_report_path = generated_profiles_dir / profile / "report.md"
            if not profile_summary_path.is_file() or not profile_report_path.is_file():
                errors.append(f"{profile} generated profile report artifacts are missing")
            else:
                try:
                    generated_profile = load_json(profile_summary_path)
                except (OSError, json.JSONDecodeError):
                    generated_profile = {}
                if not isinstance(generated_profile, dict):
                    errors.append(f"{profile} generated profile summary is malformed")
                else:
                    for field, expected_value in profile_report.items():
                        if generated_profile.get(field) != expected_value:
                            errors.append(f"{profile} embedded profile summary does not match its artifact")
                            break
            if profile_report.get("profile") != profile:
                errors.append(f"{profile} profile identity is missing")
            if split_stack:
                if profile_report.get("split_stack_requested") is not True:
                    errors.append(f"{profile} split-stack declaration is missing")
                if profile_report.get("component_status_counts") != {"available": expected_count}:
                    errors.append(f"{profile} component status counts are incomplete")
                if profile_report.get("materialization_status_counts") != {"complete": expected_count}:
                    errors.append(f"{profile} materialization status counts are incomplete")
                validate_split_identity_list(
                    profile_report.get("component_identities"),
                    identity_field="manifest_identity",
                    required_profiles={profile},
                    label=f"{profile} component",
                    errors=errors,
                )
                validate_split_identity_list(
                    profile_report.get("materialization_identities"),
                    identity_field="materialization_identity",
                    required_profiles={profile},
                    label=f"{profile} materialization",
                    errors=errors,
                )
            if profile_report.get("case_count") != expected_count:
                errors.append(f"{profile} profile summary has the wrong case count")
            if profile_report.get("expected_case_count") != expected_count:
                errors.append(f"{profile} profile summary has the wrong expected count")
            if profile_report.get("correctness_gate") != "passed":
                errors.append(f"{profile} profile summary is not passed")
            if profile_report.get("result_status_counts") != {"passed": expected_count}:
                errors.append(f"{profile} result status evidence is incomplete")
            if profile_report.get("execution_status_counts") != {"success": expected_count}:
                errors.append(f"{profile} execution status evidence is incomplete")
            if profile_report.get("correctness_status_counts") != {"passed": expected_count}:
                errors.append(f"{profile} correctness status evidence is incomplete")
            if profile_report.get("oracle_status_counts") != {"available": expected_count}:
                errors.append(f"{profile} oracle status evidence is incomplete")
            for field in (
                "source_hashes",
                "oracle_hashes",
                "oracle_provenance_hashes",
                "oracle_provenance_artifact_hashes",
                "source_identities",
                "dataset_identities",
                "engine_identities",
                "canonicalizer_identities",
                "oracle_identities",
            ):
                if not isinstance(profile_report.get(field), list) or not profile_report[field]:
                    errors.append(f"{profile} {field} evidence is missing")

            for field in (
                "source_hashes",
                "oracle_hashes",
                "oracle_provenance_hashes",
                "oracle_provenance_artifact_hashes",
            ):
                if not all(valid_sha256(value) for value in profile_report.get(field, [])):
                    errors.append(f"{profile} {field} contains an invalid hash")

            for source in profile_report.get("source_identities", []):
                if not isinstance(source, dict) or not valid_sha256(source.get("source_query_sha256")):
                    errors.append(f"{profile} source identity contains an invalid hash")
            for dataset in profile_report.get("dataset_identities", []):
                if (
                    not isinstance(dataset, dict)
                    or not dataset.get("dataset_id")
                    or not valid_sha256(dataset.get("dataset_sha256"))
                ):
                    errors.append(f"{profile} dataset identity contains an invalid hash")

            timing = profile_report.get("timing_provenance")
            if (
                not isinstance(timing, dict)
                or timing.get("unit") != "ms"
                or not isinstance(timing.get("load"), dict)
                or not isinstance(timing.get("query"), dict)
            ):
                errors.append(f"{profile} timing provenance is missing")
            else:
                for timing_name in ("load", "query"):
                    if timing[timing_name].get("recorded_count") != expected_count:
                        errors.append(
                            f"{profile} {timing_name} timing evidence is incomplete"
                        )

            for oracle in profile_report.get("oracle_identities", []):
                if not isinstance(oracle, dict):
                    errors.append(f"{profile} oracle identity is malformed")
                    continue
                if oracle.get("class") not in {"official", "derived"}:
                    errors.append(f"{profile} oracle class is not accepted")
                if oracle.get("match_reason") != "accepted":
                    errors.append(f"{profile} oracle acceptance evidence is missing")
                for field in (
                    "oracle_id",
                    "provenance",
                    "provenance_sha256",
                    "provenance_artifact",
                    "provenance_artifact_sha256",
                    "artifact_sha256",
                    "canonical_result_hash",
                ):
                    if not oracle.get(field):
                        errors.append(f"{profile} oracle {field} evidence is missing")
                for field in (
                    "provenance_sha256",
                    "provenance_artifact_sha256",
                    "artifact_sha256",
                    "canonical_result_hash",
                ):
                    if not valid_sha256(oracle.get(field)):
                        errors.append(f"{profile} oracle {field} contains an invalid hash")
                if oracle.get("class") == "derived":
                    for field in (
                        "proof_artifact",
                        "proof_artifact_sha256",
                        "audit_artifact",
                        "audit_artifact_sha256",
                        "audit_status",
                        "oracle_pipeline_version",
                    ):
                        if not oracle.get(field):
                            errors.append(f"{profile} derived oracle {field} evidence is missing")
                    for field in ("proof_artifact_sha256", "audit_artifact_sha256"):
                        if not valid_sha256(oracle.get(field)):
                            errors.append(f"{profile} derived oracle {field} contains an invalid hash")
                    if oracle.get("derivation_deterministic") is not True:
                        errors.append(f"{profile} derived oracle is not deterministic")
                    if oracle.get("sound_complete") is not True:
                        errors.append(f"{profile} derived oracle is not sound/complete")

    if errors:
        result["status"] = "blocked"
        result["correctness_gate"] = "blocked"
        result["strict_query_gate_errors"] = errors
    return result


def unavailable(name: str, path: str, lane_kind: str) -> dict:
    if lane_kind == "performance":
        # ADR 0024: a missing performance-lane run is informational and does
        # not block the campaign correctness gate.
        return {
            "status": "not_available",
            "correctness_gate": "not_applicable",
            "note": (
                f"{name} performance lane run directory was not supplied or does not exist "
                f"(reported informationally; does not block the campaign gate): {path or '<unset>'}"
            ),
            "run_dir": path,
            "artifacts": [],
            "raw_result_artifacts": [],
            "metadata": {},
        }
    return {
        "status": "not_available",
        "correctness_gate": "blocked",
        "error": f"{name} run directory was not supplied or does not exist: {path or '<unset>'}",
        "run_dir": path,
        "artifacts": [],
        "raw_result_artifacts": [],
        "metadata": {},
    }


def apply_lane(name: str, suite: dict) -> dict:
    """Declare each suite's lane kind and per-suite performance permission.

    ADR 0024 scopes the performance-after-correctness rule per suite: a
    correctness lane may report performance only after its own gate passes,
    while a declared performance lane may report execution performance once
    execution evidence exists.
    """
    lane_kind = SUITE_LANE_KINDS[name]
    suite["lane_kind"] = lane_kind
    if lane_kind == "performance":
        suite["performance_claims_allowed"] = suite.get("case_count", 0) > 0
    else:
        suite["performance_claims_allowed"] = suite.get("correctness_gate") == "passed"
    return suite


def load_suite(name: str, path_value: str) -> dict:
    run_dir = Path(path_value).resolve() if path_value else None
    lane_kind = SUITE_LANE_KINDS[name]
    if run_dir is None or not run_dir.is_dir():
        return apply_lane(name, unavailable(name, path_value, lane_kind))
    if name == "w3c":
        return apply_lane(name, w3c_suite(run_dir))
    if name == "ore":
        return apply_lane(name, ore_suite(run_dir))
    if name == "owl2bench":
        return apply_lane(name, stage_suite(run_dir))
    if name == "owl2bench-query":
        return apply_lane(name, query_summary(run_dir))
    return apply_lane(name, generic_summary(run_dir))


def write_report(output_dir: Path, suites: dict[str, dict]) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    # ADR 0024/0027: the campaign correctness gate is the conjunction over
    # the correctness-kind suites only (W3C AND owl2bench-query); the ORE
    # and OWL2Bench stage performance lanes are informational and never
    # block or pass it. The campaign-level performance-claim permission
    # rolls up the per-suite permissions.
    correctness_suites = {
        name: suite
        for name, suite in suites.items()
        if suite.get("lane_kind", "correctness") == "correctness"
    }
    all_passed = all(suite["correctness_gate"] == "passed" for suite in correctness_suites.values())
    performance_allowed = all(
        suite.get("performance_claims_allowed", False) for suite in suites.values()
    )
    summary = {
        "schema_version": 2,
        "campaign": "owl2-reasoning-benchmarks",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "correctness_gate": "passed" if all_passed else "blocked",
        "performance_claims_allowed": performance_allowed,
        "suites": suites,
    }
    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

    with (output_dir / "report.md").open("w", encoding="utf-8") as handle:
        handle.write("# OWL 2 Benchmark Campaign Report\n\n")
        handle.write(f"- Correctness gate: **{summary['correctness_gate']}**\n")
        handle.write(
            f"- Performance/scalability claims allowed: **{summary['performance_claims_allowed']}**\n\n"
        )
        handle.write("| Suite | Status | Cases | Run directory | Artifacts | Lane |\n")
        handle.write("|---|---|---:|---|---|---|\n")
        for name, suite in suites.items():
            counts = suite.get("result_status_counts", {})
            case_count = suite.get("case_count", sum(counts.values()))
            artifacts = ", ".join(f"`{path}`" for path in suite.get("artifacts", [])) or "none"
            handle.write(
                f"| {name} | {suite.get('status', 'blocked')} | {case_count} | "
                f"`{suite.get('run_dir', '')}` | {artifacts} | {suite.get('lane_kind', 'correctness')} |\n"
            )
        handle.write("\n## Interpretation\n\n")
        if all_passed:
            handle.write("All correctness suites passed their gates; performance claims may be interpreted from their suite reports.\n")
        else:
            handle.write("At least one correctness suite is missing or blocked; performance and scalability claims are suppressed for those correctness lanes until their own gates pass.\n")
        handle.write(
            "Performance-claim permission is scoped per suite (ADR 0024): a correctness lane may "
            "report performance only after its own gate passes; the ore and owl2bench performance "
            "lanes may report execution performance only, and each adjudicates no correctness, "
            "because no public oracle exists for its consistency workload.\n"
        )
        handle.write("\n## Reproducibility metadata\n\n")
        for name, suite in suites.items():
            handle.write(f"### {name}\n\n")
            metadata = suite.get("metadata", {})
            if metadata:
                handle.write("```json\n")
                handle.write(json.dumps(metadata, indent=2, sort_keys=True))
                handle.write("\n```\n\n")
            else:
                handle.write("No run metadata was available.\n\n")
            raw = suite.get("raw_result_artifacts", [])
            if raw:
                handle.write("Raw artifacts:\n\n")
                for path in raw:
                    handle.write(f"- `{path}`\n")
                handle.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--w3c-dir", default="")
    parser.add_argument("--ore-dir", default="")
    parser.add_argument("--owl2bench-dir", default="")
    parser.add_argument("--query-dir", default="")
    args = parser.parse_args()

    suite_paths = {
        "w3c": args.w3c_dir,
        "ore": args.ore_dir,
        "owl2bench": args.owl2bench_dir,
        "owl2bench-query": args.query_dir,
    }
    suites = {name: load_suite(name, path) for name, path in suite_paths.items()}
    write_report(Path(args.output_dir).resolve(), suites)
    print(f"campaign report: {Path(args.output_dir).resolve() / 'report.md'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
