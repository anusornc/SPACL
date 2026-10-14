#!/usr/bin/env python3
"""Lane-local helpers for the W3C OWL 2 approved conformance harness."""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import shutil
import sys
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path


SEMANTIC_TYPES = {
    "ConsistencyTest": ("consistency", "true", "consistent", "premise"),
    "InconsistencyTest": ("inconsistency", "true", "true", "premise"),
    "PositiveEntailmentTest": ("entailment", "true", "true", "conclusion"),
    "NegativeEntailmentTest": ("entailment", "false", "false", "non_conclusion"),
}

SYNTAX_TYPES = {"ProfileIdentificationTest", "SyntaxTranslationTest"}
MANIFEST_FIELDS = [
    "case_id",
    "source_manifest",
    "operation",
    "expected_semantic_result",
    "w3c_expected_result",
    "lane_status",
    "unsupported_reason",
    "oracle_status",
    "oracle_source",
    "oracle_case_id",
    "premise_path",
    "conclusion_path",
    "test_type",
    "approval_status",
    "semantics",
    "normative_syntax",
    "species",
    "profiles",
]
STAGED_SENTINEL = ".w3c-owl2-owned"
RDF_NS = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
XML_BASE = "{http://www.w3.org/XML/1998/namespace}base"


def clean_token(value: str) -> str:
    return value.split("#")[-1].split("/")[-1].strip(": \t\r\n")


def safe_id(value: str) -> str:
    value = re.sub(r"[^A-Za-z0-9._-]+", "_", value.strip())
    return value.strip("._-") or "case"


def unquote_functional_literal(value: str) -> str:
    return bytes(value, "utf-8").decode("unicode_escape")


def one_line(value: str) -> str:
    return re.sub(r"\s+", " ", value.strip())


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1].split("#")[-1]


def rdf_attr(elem: ET.Element, name: str) -> str:
    return elem.attrib.get(f"{{{RDF_NS}}}{name}") or elem.attrib.get(name)


def discover_files(source_dir: Path) -> list[Path]:
    candidates = []
    for path in source_dir.rglob("*"):
        if not path.is_file():
            continue
        if path.suffix.lower() in {".ofn", ".omn", ".rdf", ".owl", ".xml", ".ttl", ".nt"}:
            candidates.append(path)
    return sorted(candidates)


def parse_functional(path: Path, text: str) -> dict[str, object]:
    types = {
        clean_token(match)
        for match in re.findall(
            r"(?:ClassAssertion|Declaration)\s*\(\s*:?(ConsistencyTest|InconsistencyTest|PositiveEntailmentTest|NegativeEntailmentTest|ProfileIdentificationTest|SyntaxTranslationTest)\b",
            text,
            flags=re.I,
        )
    }
    types.update(
        clean_token(match)
        for match in re.findall(
            r"rdf:type\s+:?(ConsistencyTest|InconsistencyTest|PositiveEntailmentTest|NegativeEntailmentTest|ProfileIdentificationTest|SyntaxTranslationTest)\b",
            text,
            flags=re.I,
        )
    )
    object_values: dict[str, set[str]] = {}
    for prop, value in re.findall(
        r"ObjectPropertyAssertion\s*\(\s*:?(status|semantics|normativeSyntax|species|profile)\s+\S+\s+:?([A-Za-z][A-Za-z0-9_-]*)\s*\)",
        text,
        flags=re.I,
    ):
        object_values.setdefault(prop.lower(), set()).add(clean_token(value))
    for prop in ("status", "semantics", "normativeSyntax", "species", "profile"):
        for line in text.splitlines():
            if "ObjectPropertyAssertion" in line:
                continue
            for value in re.findall(rf":{prop}\s+:?([A-Za-z][A-Za-z0-9_-]*)", line, flags=re.I):
                object_values.setdefault(prop.lower(), set()).add(clean_token(value))
    data_values: dict[str, list[str]] = {}
    literal_re = re.compile(
        r"DataPropertyAssertion\s*\(\s*:?(identifier|expectedResult|expectedSemanticResult|checkerResult|result|[A-Za-z]*InputOntology|[A-Za-z]*PremiseOntology|[A-Za-z]*ConclusionOntology|[A-Za-z]*NonConclusionOntology)\s+\S+\s+\"((?:[^\"\\]|\\.)*)\"",
        flags=re.I | re.S,
    )
    for prop, value in literal_re.findall(text):
        data_values.setdefault(prop, []).append(unquote_functional_literal(value))
    return {"types": types, "object_values": object_values, "data_values": data_values}


def parse_xml_element(root: ET.Element) -> dict[str, object]:
    types: set[str] = set()
    object_values: dict[str, set[str]] = {}
    data_values: dict[str, list[str]] = {}
    import_refs: set[str] = set()
    for elem in root.iter():
        name = local_name(elem.tag)
        resource = rdf_attr(elem, "resource") or rdf_attr(elem, "about")
        if name == "type" and resource:
            token = clean_token(resource)
            if token in SEMANTIC_TYPES or token in SYNTAX_TYPES:
                types.add(token)
        elif name == "importedOntology":
            if resource:
                import_refs.add(resource)
        elif name in {"status", "semantics", "normativeSyntax", "species", "profile"}:
            if resource:
                object_values.setdefault(name.lower(), set()).add(clean_token(resource))
            elif elem.text and elem.text.strip():
                object_values.setdefault(name.lower(), set()).add(clean_token(elem.text))
        elif name.endswith("Ontology") or name in {
            "identifier",
            "expectedResult",
            "expectedSemanticResult",
            "checkerResult",
            "result",
        }:
            if elem.text and elem.text.strip():
                data_values.setdefault(name, []).append(elem.text)
            elif resource:
                data_values.setdefault(name, []).append(resource)
    return {
        "types": types,
        "object_values": object_values,
        "data_values": data_values,
        "import_refs": import_refs,
    }


def build_import_registry(root: ET.Element) -> dict[str, dict[str, str]]:
    """Map `test:importedOntology` node IRIs to their imported document content.

    W3C aggregate manifests embed each imported ontology in a sibling node
    (any element carrying `test:importedOntologyIRI` + `test:rdfXmlInputOntology`),
    outside every `test:TestCase`, so plain case materialization never sees it.
    """
    registry: dict[str, dict[str, str]] = {}
    for elem in root.iter():
        if len(elem) == 0:
            continue
        children = {local_name(child.tag): child for child in elem}
        iri_child = children.get("importedOntologyIRI")
        content_child = children.get("rdfXmlInputOntology")
        if iri_child is None or content_child is None:
            continue
        about = rdf_attr(elem, "about")
        imported_iri = rdf_attr(iri_child, "resource")
        content = (content_child.text or "").strip()
        if about and imported_iri and content:
            registry[about] = {"iri": imported_iri, "content": content}
    return registry


def register_common_namespaces() -> None:
    for prefix, uri in (
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ):
        ET.register_namespace(prefix, uri)


def inline_imported_ontologies(
    premise_path: Path, import_refs: set[str], registry: dict[str, dict[str, str]]
) -> int:
    """Append referenced imported documents into a staged premise file.

    Each appended top-level element carries its own `xml:base` so relative IRIs
    (`rdf:ID`, `#fragment`) resolve against the imported document, not the
    premise — relocating W3C fixtures must preserve cross-file references.
    The reasoner CLI runs with import resolution off, so the union closure of
    the W3C test's premise documents has to exist in this one file.
    """
    pending = []
    resolved: set[str] = set()
    for ref in sorted(import_refs):
        entry = registry.get(ref)
        if entry is None:
            continue
        pending.append(entry)
        resolved.add(ref)
    unresolved = import_refs - resolved
    if unresolved:
        print(
            f"[w3c-owl2-prepare][warn] imported ontology nodes not found in manifest, "
            f"staging without them: {sorted(unresolved)}",
            file=sys.stderr,
        )
    if not pending:
        return 0
    try:
        premise_root = ET.parse(premise_path).getroot()
    except (ET.ParseError, ValueError) as error:
        print(
            f"[w3c-owl2-prepare][warn] premise not XML, skipping import inlining: {premise_path} ({error})",
            file=sys.stderr,
        )
        return 0
    inlined = 0
    for entry in pending:
        try:
            imported_root = ET.fromstring(entry["content"])
        except (ET.ParseError, ValueError) as error:
            print(
                f"[w3c-owl2-prepare][warn] imported ontology not XML, skipping: {entry['iri']} ({error})",
                file=sys.stderr,
            )
            continue
        base = imported_root.get(XML_BASE) or entry["iri"]
        for child in list(imported_root):
            if child.get(XML_BASE) is None:
                child.set(XML_BASE, base)
            premise_root.append(child)
        inlined += 1
    register_common_namespaces()
    ET.indent(premise_root)
    premise_path.write_text(ET.tostring(premise_root, encoding="unicode") + "\n", encoding="utf-8")
    return inlined


def parse_xml(path: Path) -> list[dict[str, object]]:
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError:
        return []

    import_registry = build_import_registry(root)
    test_cases = [elem for elem in root.iter() if local_name(elem.tag) == "TestCase"]
    if test_cases:
        parsed_cases = [parse_xml_element(test_case) for test_case in test_cases]
    else:
        parsed_cases = [parse_xml_element(root)]
    for parsed in parsed_cases:
        parsed["_import_registry"] = import_registry
    return parsed_cases


def parse_case(path: Path) -> list[dict[str, object]]:
    text = path.read_text(encoding="utf-8", errors="replace")
    parsed_cases = parse_xml(path) if text.lstrip().startswith("<") else [parse_functional(path, text)]
    valid_cases = []
    for parsed in parsed_cases:
        if not parsed["types"]:
            fallback = parse_functional(path, text)
            parsed["types"] = set(parsed["types"]) | set(fallback["types"])
            for key, values in fallback["object_values"].items():
                parsed["object_values"].setdefault(key, set()).update(values)
            for key, values in fallback["data_values"].items():
                parsed["data_values"].setdefault(key, []).extend(values)
        if parsed["types"]:
            valid_cases.append(parsed)
    return valid_cases


def first_value(data_values: dict[str, list[str]], names: list[str]) -> tuple[str, str]:
    lower_map = {key.lower(): (key, values) for key, values in data_values.items()}
    for name in names:
        found = lower_map.get(name.lower())
        if found and found[1]:
            return found[0], found[1][0]
    return "", ""


def normalize_declared_result(value: str) -> str:
    token = clean_token(value).lower()
    return token if token in {"true", "false", "unknown", "error"} else ""


def semantic_result_for(operation: str, declared_result: str, fallback: str) -> str:
    if not declared_result:
        return fallback
    if operation == "consistency":
        return {"true": "consistent", "false": "inconsistent"}.get(
            declared_result, declared_result
        )
    return declared_result


def materialize_input(
    value: str,
    prop_name: str,
    case_dir: Path,
    source_file: Path,
    source_dir: Path,
    role: str,
) -> str:
    if not value:
        return ""
    ext = ".owl"
    if prop_name.lower().startswith("fs"):
        ext = ".ofn"
    elif prop_name.lower().startswith("owlxml"):
        ext = ".owx"
    target = case_dir / f"{role}{ext}"

    stripped = value.strip()
    source_candidates = []
    if not stripped.startswith("<") and "\n" not in stripped and len(stripped) < 500:
        source_candidates.extend([source_file.parent / stripped, source_dir / stripped])
    for candidate in source_candidates:
        if candidate.is_file():
            shutil.copyfile(candidate, target)
            return str(target)

    target.write_text(stripped + "\n", encoding="utf-8")
    return str(target)


def classify_case(
    parsed: dict[str, object], semantic_type: str | None = None
) -> tuple[str, str, str, str, str]:
    types = set(parsed["types"])
    object_values = parsed["object_values"]
    approval = next(iter(sorted(object_values.get("status", []))), "unknown")
    semantics = ",".join(sorted(object_values.get("semantics", []))) or "unknown"
    normative = ",".join(sorted(object_values.get("normativesyntax", []))) or "unknown"
    species = ",".join(sorted(object_values.get("species", []))) or "unknown"

    semantic_type = semantic_type or next((name for name in SEMANTIC_TYPES if name in types), "")
    syntax_type = next((name for name in SYNTAX_TYPES if name in types), "")
    test_type = semantic_type or syntax_type or ",".join(sorted(types))

    if not semantic_type:
        return test_type, "unsupported", "syntax_or_profile_test", approval, semantics
    if approval.lower() != "approved":
        return test_type, "unsupported", f"approval_{approval.lower()}", approval, semantics
    if "Direct" not in semantics and "DIRECT" not in semantics:
        return test_type, "unsupported", "not_direct_semantics", approval, semantics
    if species != "unknown" and "DL" not in species:
        return test_type, "unsupported", "not_owl2_dl_direct_semantics", approval, semantics
    if "RDFXML" not in normative:
        return test_type, "unsupported", "no_normative_rdfxml", approval, semantics
    return test_type, "baseline", "", approval, semantics


def prepare(args: argparse.Namespace) -> int:
    source_dir = Path(args.source_dir).resolve()
    staged_dir = Path(args.staged_dir).resolve()
    manifest = Path(args.manifest).resolve()
    if not source_dir.is_dir():
        print(f"[w3c-owl2-prepare][error] source directory not found: {source_dir}", file=sys.stderr)
        return 1

    if args.clean:
        expected_manifest = staged_dir / "manifest.tsv"
        if manifest != expected_manifest:
            print(
                "[w3c-owl2-prepare][error] clean mode requires the manifest inside the staged directory",
                file=sys.stderr,
            )
            return 1
        if staged_dir.exists():
            if not staged_dir.is_dir():
                print(
                    f"[w3c-owl2-prepare][error] staged path is not a directory: {staged_dir}",
                    file=sys.stderr,
                )
                return 1
            sentinel = staged_dir / STAGED_SENTINEL
            existing_entries = [
                entry for entry in staged_dir.iterdir() if entry.name != STAGED_SENTINEL
            ]
            if existing_entries and not sentinel.is_file():
                print(
                    f"[w3c-owl2-prepare][error] refusing to clean unstamped staged directory: {staged_dir}",
                    file=sys.stderr,
                )
                return 1
            if sentinel.is_file():
                shutil.rmtree(staged_dir / "cases", ignore_errors=True)
                for generated in (manifest, staged_dir / "prepare_summary.json"):
                    generated.unlink(missing_ok=True)
    staged_dir.mkdir(parents=True, exist_ok=True)
    (staged_dir / STAGED_SENTINEL).touch()
    (staged_dir / "cases").mkdir(exist_ok=True)

    rows = []
    index = 0
    import_inline_cases = 0
    for path in discover_files(source_dir):
        for parsed in parse_case(path):
            semantic_types = [name for name in SEMANTIC_TYPES if name in parsed["types"]]
            if not semantic_types:
                semantic_types = [""]
            for semantic_type in semantic_types:
                index += 1
                case_parsed = dict(parsed)
                if semantic_type:
                    case_parsed["types"] = {semantic_type}
                data_values = case_parsed["data_values"]
                test_type, lane_status, unsupported_reason, approval, semantics = classify_case(
                    case_parsed, semantic_type or None
                )
                operation, w3c_expected, expected_semantic, conclusion_role = SEMANTIC_TYPES.get(
                    test_type, ("unsupported", "unknown", "unknown", "")
                )
                _, declared_result_value = first_value(
                    data_values,
                    ["expectedResult", "expectedSemanticResult", "checkerResult", "result"],
                )
                declared_result = normalize_declared_result(declared_result_value)
                if declared_result:
                    w3c_expected = declared_result
                    expected_semantic = semantic_result_for(operation, declared_result, expected_semantic)
                identifier = first_value(data_values, ["identifier"])[1]
                base_case_id = safe_id(identifier or path.relative_to(source_dir).with_suffix("").as_posix())
                if len(semantic_types) > 1 and semantic_type:
                    base_case_id = f"{base_case_id}_{safe_id(operation)}"
                case_id = f"{index:04d}_{base_case_id}"
                case_dir = staged_dir / "cases" / case_id
                case_dir.mkdir(parents=True, exist_ok=True)

                premise_prop, premise_value = first_value(
                    data_values,
                    ["rdfXMLPremiseOntology", "premiseOntology", "rdfXMLInputOntology", "inputOntology"],
                )
                premise_path = materialize_input(
                    premise_value, premise_prop, case_dir, path, source_dir, "premise"
                )
                if premise_path and parsed.get("import_refs"):
                    inlined = inline_imported_ontologies(
                        Path(premise_path),
                        set(parsed["import_refs"]),
                        dict(parsed.get("_import_registry") or {}),
                    )
                    if inlined:
                        import_inline_cases += 1
                conclusion_path = ""
                if conclusion_role == "conclusion":
                    prop, value = first_value(data_values, ["rdfXMLConclusionOntology", "conclusionOntology"])
                    conclusion_path = materialize_input(value, prop, case_dir, path, source_dir, "conclusion")
                elif conclusion_role == "non_conclusion":
                    prop, value = first_value(
                        data_values, ["rdfXMLNonConclusionOntology", "nonConclusionOntology"]
                    )
                    conclusion_path = materialize_input(
                        value, prop, case_dir, path, source_dir, "non_conclusion"
                    )

                if lane_status == "baseline":
                    if not premise_path:
                        lane_status = "unsupported"
                        unsupported_reason = "missing_premise_ontology"
                    elif operation == "entailment" and not conclusion_path:
                        lane_status = "unsupported"
                        unsupported_reason = "missing_conclusion_ontology"

                object_values = case_parsed["object_values"]
                rows.append(
                    {
                        "case_id": case_id,
                        "source_manifest": str(path),
                        "operation": operation,
                        "expected_semantic_result": expected_semantic,
                        "w3c_expected_result": w3c_expected,
                        "lane_status": lane_status,
                        "unsupported_reason": unsupported_reason or "-",
                        "oracle_status": "available" if lane_status == "baseline" else "not_applicable",
                        "oracle_source": "w3c-owl2-approved",
                        "oracle_case_id": case_id,
                        "premise_path": premise_path or "-",
                        "conclusion_path": conclusion_path or "-",
                        "test_type": test_type,
                        "approval_status": approval,
                        "semantics": semantics,
                        "normative_syntax": ",".join(sorted(object_values.get("normativesyntax", [])))
                        or "unknown",
                        "species": ",".join(sorted(object_values.get("species", []))) or "unknown",
                        "profiles": ",".join(sorted(object_values.get("profile", []))) or "none",
                    }
                )

    with manifest.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=MANIFEST_FIELDS, delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)

    counts = Counter(row["lane_status"] for row in rows)
    type_counts = Counter(row["test_type"] for row in rows)
    summary = {
        "source_dir": str(source_dir),
        "staged_dir": str(staged_dir),
        "manifest": str(manifest),
        "total_cases": len(rows),
        "lane_status_counts": dict(sorted(counts.items())),
        "test_type_counts": dict(sorted(type_counts.items())),
        "import_inline_cases": import_inline_cases,
    }
    (staged_dir / "prepare_summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"[w3c-owl2-prepare] discovered {len(rows)} manifest case(s)")
    print(f"[w3c-owl2-prepare] baseline={counts.get('baseline', 0)} unsupported={counts.get('unsupported', 0)}")
    print(f"[w3c-owl2-prepare] manifest={manifest}")
    return 0 if rows else 1


def report(args: argparse.Namespace) -> int:
    run_dir = Path(args.run_dir).resolve()
    result_files = sorted(
        path
        for path in run_dir.glob("*.json")
        if path.name not in {"run_metadata.json", "summary.json"}
    )
    if not result_files:
        print(f"[w3c-owl2-report][error] no result JSON files in {run_dir}", file=sys.stderr)
        return 1

    rows = []
    for path in result_files:
        with path.open(encoding="utf-8") as handle:
            result = json.load(handle)
        rows.append(result)

    by_result = Counter(row.get("result_status", "unknown") for row in rows)
    by_operation = Counter(row.get("operation", "unknown") for row in rows)
    by_w3c = Counter(row.get("w3c_expected_result", "unknown") for row in rows)
    by_type = Counter(row.get("w3c_test_type", "unknown") for row in rows)
    baseline_rows = [row for row in rows if row.get("w3c_lane_status") == "baseline"]
    by_baseline_type = Counter(row.get("w3c_test_type", "unknown") for row in baseline_rows)
    summary = {
        "run_dir": str(run_dir),
        "total_results": len(rows),
        "result_status_counts": dict(sorted(by_result.items())),
        "operation_counts": dict(sorted(by_operation.items())),
        "w3c_expected_result_counts": dict(sorted(by_w3c.items())),
        "w3c_test_type_counts": dict(sorted(by_type.items())),
        "baseline_w3c_test_type_counts": dict(sorted(by_baseline_type.items())),
    }
    (run_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    csv_path = run_dir / "results.csv"
    fields = [
        "case_id",
        "operation",
        "execution_status",
        "result_status",
        "correctness_status",
        "semantic_result",
        "expected_semantic_result",
        "w3c_expected_result",
        "w3c_test_type",
        "w3c_lane_status",
        "w3c_unsupported_reason",
        "oracle_status",
        "wall_time_ms",
        "reported_time_ms",
        "parse_time_ms",
        "reason_time_ms",
    ]
    with csv_path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            oracle = row.get("oracle") or {}
            writer.writerow(
                {
                    "case_id": row.get("case_id", ""),
                    "operation": row.get("operation", ""),
                    "execution_status": row.get("execution_status", ""),
                    "result_status": row.get("result_status", ""),
                    "correctness_status": row.get("correctness_status", ""),
                    "semantic_result": row.get("semantic_result", ""),
                    "expected_semantic_result": oracle.get("expected"),
                    "w3c_expected_result": row.get("w3c_expected_result", ""),
                    "w3c_test_type": row.get("w3c_test_type", ""),
                    "w3c_lane_status": row.get("w3c_lane_status", ""),
                    "w3c_unsupported_reason": row.get("w3c_unsupported_reason", ""),
                    "oracle_status": oracle.get("status", ""),
                    "wall_time_ms": row.get("wall_time_ms", ""),
                    "reported_time_ms": row.get("reported_time_ms", ""),
                    "parse_time_ms": row.get("parse_time_ms", ""),
                    "reason_time_ms": row.get("reason_time_ms", ""),
                }
            )

    md_path = run_dir / "report.md"
    lines = [
        "# W3C OWL 2 Approved Conformance Report",
        "",
        f"- Results: `{run_dir}`",
        f"- Cases: {len(rows)}",
        "",
        "## Result Status",
        "",
    ]
    for key, count in sorted(by_result.items()):
        lines.append(f"- `{key}`: {count}")
    lines.extend(["", "## Operations", ""])
    for key, count in sorted(by_operation.items()):
        lines.append(f"- `{key}`: {count}")
    lines.extend(["", "## W3C Expected Values", ""])
    for key, count in sorted(by_w3c.items()):
        lines.append(f"- `{key}`: {count}")
    lines.extend(["", "## Baseline Test Types", ""])
    for key, count in sorted(by_baseline_type.items()):
        lines.append(f"- `{key}`: {count}")
    md_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"[w3c-owl2-report] summary={run_dir / 'summary.json'}")
    print(f"[w3c-owl2-report] csv={csv_path}")
    print(f"[w3c-owl2-report] report={md_path}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--source-dir", required=True)
    prepare_parser.add_argument("--staged-dir", required=True)
    prepare_parser.add_argument("--manifest", required=True)
    prepare_parser.add_argument("--clean", action="store_true")
    prepare_parser.set_defaults(func=prepare)

    report_parser = subparsers.add_parser("report")
    report_parser.add_argument("--run-dir", required=True)
    report_parser.set_defaults(func=report)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
