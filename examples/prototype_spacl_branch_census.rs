use owl2_reasoner::logic::axioms::{Axiom, ClassExpression};
use owl2_reasoner::util::ontology_io::load_ontology_with_env;
use owl2_reasoner::{
    Ontology, OwlError, SchedulingMode, SpeculativeConfig, SpeculativeTableauxReasoner,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const INPUT_DIR_ENV: &str = "BRANCH_CENSUS_INPUT_DIR";
const OUTPUT_PATH_ENV: &str = "BRANCH_CENSUS_OUTPUT_PATH";
const DEFAULT_OUTPUT_PATH: &str =
    "benchmarks/prototypes/spacl_parallel_diagnosis/branch_census.json";
const PROVENANCE_PATH: &str = "benchmarks/prototypes/spacl_feedback/corpus_provenance.json";
const CLEARED_ENV_VARS: &[&str] = &[
    "OWL2_REASONER_LARGE_PARSE",
    "OWL2_REASONER_MAX_FILE_SIZE",
    "OWL2_REASONER_AUTO_CACHE",
    "OWL2_REASONER_FORCE_TEXT",
    "OWL2_REASONER_BIN_ONLY",
    "OWL2_REASONER_DISABLE_PARSE_FALLBACK",
    "OWL2_REASONER_ENABLE_PARSE_FALLBACK",
    "OWL2_REASONER_PARSE_PROGRESS_EVERY",
    "OWL2_REASONER_PARSE_IO_PROGRESS_BYTES",
    "OWL2_REASONER_EXPERIMENTAL_XML_PARSER",
    "OWL2_REASONER_EXPERIMENTAL_XML_STRICT",
    "OWL2_REASONER_EXPERIMENTAL_XML_BATCH",
    "OWL2_REASONER_EXPERIMENTAL_XML_QUEUE",
    "OWL2_REASONER_EXPERIMENTAL_XML_WORKERS",
    "OWL2_REASONER_EXPERIMENTAL_XML_CACHE",
    "OWL2_REASONER_STRUCTURAL_XML_PARSER",
    "OWL2_REASONER_STRUCTURAL_XML_AUTO",
    "OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD",
    "OWL2_REASONER_STRUCTURAL_XML_INTERNER",
    "OWL2_REASONER_STRUCTURAL_BREAKDOWN",
    "OWL2_REASONER_LARGE_PROFILE_AUTO",
    "OWL2_REASONER_LARGE_PROFILE_THRESHOLD",
    "OWL2_REASONER_STAGE_TIMING",
    "SPACL_SCHED_MODE",
    "SPACL_BRANCH_POLICY",
    "SPACL_RRN_MODEL_PATH",
    "SPACL_BRANCH_SNAPSHOT_FILE",
    "SPACL_NOGOOD",
    "SPACL_EMIT_STATS",
];

const PHASE1_BASELINE_ENV: &[(&str, &str)] = &[
    ("OWL2_REASONER_FORCE_TEXT", "1"),
    ("OWL2_REASONER_AUTO_CACHE", "0"),
    ("OWL2_REASONER_MAX_FILE_SIZE", "268435456"),
    ("OWL2_REASONER_LARGE_PROFILE_AUTO", "0"),
];

const FIXTURE_FILENAME: &str = "fixture_equiv_and_subclass_union.owl";
const FIXTURE_CONTENT: &str = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#"
         xmlns:ex="http://example.org/branch-census#">
  <owl:Ontology rdf:about="http://example.org/branch-census"/>

  <owl:Class rdf:about="http://example.org/branch-census#Sub">
    <rdfs:subClassOf>
      <owl:Class>
        <owl:unionOf rdf:parseType="Collection">
          <owl:Class rdf:about="http://example.org/branch-census#A"/>
          <owl:Class rdf:about="http://example.org/branch-census#B"/>
        </owl:unionOf>
      </owl:Class>
    </rdfs:subClassOf>
  </owl:Class>

  <owl:Class rdf:about="http://example.org/branch-census#Eq">
    <owl:equivalentClass>
      <owl:Class>
        <owl:unionOf rdf:parseType="Collection">
          <owl:Class rdf:about="http://example.org/branch-census#C"/>
          <owl:Class rdf:about="http://example.org/branch-census#D"/>
        </owl:unionOf>
      </owl:Class>
    </owl:equivalentClass>
  </owl:Class>
</rdf:RDF>
"#;

#[derive(Debug, Serialize)]
struct Report {
    schema_version: u32,
    purpose: &'static str,
    generated_by: &'static str,
    loader_contract: LoaderContract,
    source_references: SourceReferences,
    model_notes: ModelNotes,
    fixture: CorpusCaseReport,
    corpus: Vec<CorpusCaseReport>,
    unavailable_cases: Vec<UnavailableCase>,
    conclusion: Conclusion,
}

#[derive(Debug, Serialize)]
struct UnavailableCase {
    id: String,
    filename: String,
    expected_sha256: String,
    error: String,
}

#[derive(Debug, Serialize)]
struct LoaderContract {
    variant: &'static str,
    ontology_loader: &'static str,
    requested_env: BTreeMap<&'static str, &'static str>,
    cleared_env_vars: Vec<&'static str>,
    effective_env: BTreeMap<&'static str, Option<String>>,
}

#[derive(Debug, Serialize)]
struct SourceReferences {
    corpus_provenance_json: &'static str,
    parser_branch_signal_refs: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct ModelNotes {
    equivalent_classes_representation: &'static str,
    scanned_expression_scope: &'static str,
    public_disjunction_stats_scope: &'static str,
}

#[derive(Debug, Serialize)]
struct CorpusCaseReport {
    id: String,
    filename: String,
    expected_sha256: String,
    actual_sha256: String,
    sha256_matches: bool,
    syntax_counts: SyntaxCounts,
    ontology_counts: OntologyCounts,
    typed_expression_counts: TypedExpressionCounts,
    spacl_sequential_stats: SpaclStatsSubset,
    interpretation: Interpretation,
}

#[derive(Debug, Default, Serialize)]
struct SyntaxCounts {
    rdf_xml_union_of_occurrences: usize,
    rdf_xml_intersection_of_occurrences: usize,
    rdf_xml_complement_of_occurrences: usize,
}

#[derive(Debug, Serialize)]
struct OntologyCounts {
    classes: usize,
    axioms: usize,
    subclass_axioms: usize,
    equivalent_classes_axioms: usize,
    class_assertions: usize,
}

#[derive(Debug, Default, Serialize)]
struct TypedExpressionCounts {
    scanned_positions: ExpressionKindCounts,
    subclass_subclass: ExpressionKindCounts,
    subclass_superclass: ExpressionKindCounts,
    class_assertion: ExpressionKindCounts,
    equivalent_classes_expression_slots_available: bool,
    equivalent_classes_named_class_pairs: usize,
}

#[derive(Debug, Default, Serialize)]
struct ExpressionKindCounts {
    union: usize,
    intersection: usize,
    complement: usize,
    some_values_from: usize,
    all_values_from: usize,
    qualified_object_cardinality: usize,
    qualified_data_cardinality: usize,
    has_self: usize,
}

#[derive(Debug, Serialize)]
struct SpaclStatsSubset {
    execution_status: &'static str,
    semantic_result: Option<bool>,
    error: Option<String>,
    detector_observed: bool,
    scheduling_mode: String,
    disjunctions_detected: usize,
    estimated_branch_count: usize,
    estimated_cost_us: usize,
    branch_gate_passed: bool,
    cost_gate_passed: bool,
    used_parallel: bool,
}

#[derive(Debug, Serialize)]
struct Interpretation {
    syntax_union_retained_as_typed_union: bool,
    retained_union_reaches_public_spacl_detector: Option<bool>,
    primary_gap: &'static str,
}

#[derive(Debug, Serialize)]
struct Conclusion {
    real_corpus_disjunctions_detected_zero_reason: &'static str,
    fixture_discriminator: &'static str,
    no_speed_claims: bool,
}

#[derive(Debug, serde::Deserialize)]
struct CorpusProvenance {
    inputs: Vec<CorpusInput>,
}

#[derive(Debug, serde::Deserialize)]
struct CorpusInput {
    id: String,
    filename: String,
    sha256: String,
}

fn main() -> Result<(), OwlError> {
    configure_loader_env();
    let loader_contract = loader_contract();

    let input_dir = required_input_dir()?;
    let output_path =
        std::env::var(OUTPUT_PATH_ENV).unwrap_or_else(|_| DEFAULT_OUTPUT_PATH.to_string());
    let provenance = read_provenance(Path::new(PROVENANCE_PATH))?;

    let fixture = run_fixture_case()?;
    let mut corpus = Vec::new();
    let mut unavailable_cases = Vec::new();
    for input in &provenance.inputs {
        match run_corpus_case(input, Path::new(&input_dir)) {
            Ok(case) => corpus.push(case),
            Err(error) => unavailable_cases.push(UnavailableCase {
                id: input.id.clone(),
                filename: input.filename.clone(),
                expected_sha256: input.sha256.clone(),
                error: error.to_string(),
            }),
        }
    }

    let report = Report {
        schema_version: 3,
        purpose: "Compare RDF/XML unionOf syntax counts with retained typed class-expression branch signals and public SPACL disjunction stats.",
        generated_by: "cargo run --example prototype_spacl_branch_census",
        loader_contract,
        source_references: SourceReferences {
            corpus_provenance_json: PROVENANCE_PATH,
            parser_branch_signal_refs: vec![
                "src/reasoner/speculative.rs find_disjunctions only inspects Axiom::SubClassOf(super_class)",
                "src/logic/axioms/class_axioms.rs EquivalentClassesAxiom stores Vec<Arc<IRI>>, not ClassExpression slots",
                "src/parser/rdf_xml_streaming.rs materialize_pending_class_edges lowers anonymous equivalence into two SubClassOf implications and preserves anonymous subclass expressions",
                "src/strategy/ontology_analysis.rs OntologyCharacteristics counts disjunctions only from SubClassOf superclass",
            ],
        },
        model_notes: ModelNotes {
            equivalent_classes_representation: "EquivalentClassesAxiom has named-class IRI storage only; supported anonymous equivalence is represented by two SubClassOf implications.",
            scanned_expression_scope: "Typed expression counts scan SubClassOf sub_class/super_class and ClassAssertion class_expr, recursively including object-cardinality class qualifiers. Data qualifier presence and hasSelf are counted, but data-range internals and other Axiom variants are not a complete coverage claim.",
            public_disjunction_stats_scope: "SpeculativeStats.disjunctions_detected is based on unique ObjectUnionOf expressions found under SubClassOf.super_class only.",
        },
        fixture,
        corpus,
        unavailable_cases,
        conclusion: Conclusion {
            real_corpus_disjunctions_detected_zero_reason: "Interpret this from each case's syntax_counts, typed_expression_counts.scanned_positions, and primary_gap fields. Cases with raw unionOf syntax but zero scanned unions indicate no union retained in the audited typed-expression slots; cases with no raw unionOf syntax are reported separately as no_union_syntax_observed.",
            fixture_discriminator: "The small RDF/XML fixture contains anonymous EquivalentClasses and SubClassOf unionOf syntax. Compare its syntax, scanned-expression, and detector counts to locate loss; the report does not assume loss when retained expressions are present. EquivalentClasses has no expression-valued axiom slots.",
            no_speed_claims: true,
        },
    };

    write_report(Path::new(&output_path), &report)?;
    if report.unavailable_cases.is_empty() {
        Ok(())
    } else {
        Err(OwlError::ValidationError(format!(
            "census retained {} unavailable case(s); they are not zero-union successes",
            report.unavailable_cases.len()
        )))
    }
}

fn required_input_dir() -> Result<String, OwlError> {
    std::env::var(INPUT_DIR_ENV).map_err(|_| OwlError::ConfigError {
        parameter: INPUT_DIR_ENV.to_string(),
        message: format!(
            "{INPUT_DIR_ENV} must point to the directory containing pinned corpus copies"
        ),
    })
}

fn configure_loader_env() {
    for name in CLEARED_ENV_VARS {
        std::env::remove_var(name);
    }
    for (name, value) in PHASE1_BASELINE_ENV {
        std::env::set_var(name, value);
    }
}

fn loader_contract() -> LoaderContract {
    let effective_env = CLEARED_ENV_VARS
        .iter()
        .map(|name| (*name, std::env::var(name).ok()))
        .collect();

    LoaderContract {
        variant: "phase1_baseline",
        ontology_loader: "owl2_reasoner::util::ontology_io::load_ontology_with_env",
        requested_env: PHASE1_BASELINE_ENV.iter().copied().collect(),
        cleared_env_vars: CLEARED_ENV_VARS.to_vec(),
        effective_env,
    }
}

fn read_provenance(path: &Path) -> Result<CorpusProvenance, OwlError> {
    let content = fs::read_to_string(path).map_err(|err| {
        OwlError::StorageError(format!("failed to read {}: {}", path.display(), err))
    })?;
    serde_json::from_str(&content)
        .map_err(|err| OwlError::ParseError(format!("failed to parse {}: {}", path.display(), err)))
}

fn run_fixture_case() -> Result<CorpusCaseReport, OwlError> {
    let fixture_path = unique_fixture_path()?;
    fs::write(&fixture_path, FIXTURE_CONTENT).map_err(|err| {
        OwlError::StorageError(format!(
            "failed to write fixture {}: {}",
            fixture_path.display(),
            err
        ))
    })?;
    let fixture_sha256 = file_sha256(&fixture_path)?;
    let ontology = load_ontology_with_env(&fixture_path)?;
    let report = case_report(
        "fixture_equiv_and_subclass_union",
        FIXTURE_FILENAME,
        &fixture_sha256,
        &fixture_sha256,
        FIXTURE_CONTENT,
        &ontology,
    );
    let _ = fs::remove_file(&fixture_path);
    Ok(report)
}

fn run_corpus_case(input: &CorpusInput, input_dir: &Path) -> Result<CorpusCaseReport, OwlError> {
    let path = input_dir.join(&input.filename);
    let actual_sha256 = file_sha256(&path)?;
    if actual_sha256 != input.sha256 {
        return Err(OwlError::StorageError(format!(
            "sha256 mismatch for {}: expected {}, got {}",
            input.filename, input.sha256, actual_sha256
        )));
    }
    let content = fs::read_to_string(&path).map_err(|err| {
        OwlError::StorageError(format!("failed to read {}: {}", path.display(), err))
    })?;
    let ontology = load_ontology_with_env(&path)?;
    Ok(case_report(
        &input.id,
        &input.filename,
        &input.sha256,
        &actual_sha256,
        &content,
        &ontology,
    ))
}

fn case_report(
    id: &str,
    filename: &str,
    expected_sha256: &str,
    actual_sha256: &str,
    content: &str,
    ontology: &Ontology,
) -> CorpusCaseReport {
    let typed_expression_counts = typed_expression_counts(ontology);
    let spacl_sequential_stats = spacl_sequential_stats(ontology);
    let syntax_counts = syntax_counts(content);
    let syntax_union_count = syntax_counts.rdf_xml_union_of_occurrences;
    let retained_union_count = typed_expression_counts.scanned_positions.union;
    let detector_union_count = spacl_sequential_stats.disjunctions_detected;
    let detector_observed = spacl_sequential_stats.detector_observed;

    CorpusCaseReport {
        id: id.to_string(),
        filename: filename.to_string(),
        expected_sha256: expected_sha256.to_string(),
        actual_sha256: actual_sha256.to_string(),
        sha256_matches: expected_sha256 == actual_sha256,
        syntax_counts,
        ontology_counts: OntologyCounts {
            classes: ontology.classes().len(),
            axioms: ontology.axioms().len(),
            subclass_axioms: ontology.subclass_axioms().len(),
            equivalent_classes_axioms: ontology.equivalent_classes_axioms().len(),
            class_assertions: ontology.class_assertions().len(),
        },
        typed_expression_counts,
        spacl_sequential_stats,
        interpretation: Interpretation {
            syntax_union_retained_as_typed_union: retained_union_count > 0,
            retained_union_reaches_public_spacl_detector: detector_observed
                .then_some(detector_union_count > 0),
            primary_gap: if !detector_observed {
                "reasoning_preflight_or_error_no_detector_conclusion"
            } else if syntax_union_count == 0 {
                "no_union_syntax_observed"
            } else if retained_union_count == 0 {
                "no_union_in_audited_positions"
            } else if detector_union_count == 0 {
                "public_extractor_scope_gap"
            } else {
                "in_scope_subclass_superclass_union_detected"
            },
        },
    }
}

fn syntax_counts(content: &str) -> SyntaxCounts {
    SyntaxCounts {
        rdf_xml_union_of_occurrences: count_token(content, "unionOf"),
        rdf_xml_intersection_of_occurrences: count_token(content, "intersectionOf"),
        rdf_xml_complement_of_occurrences: count_token(content, "complementOf"),
    }
}

fn count_token(content: &str, needle: &str) -> usize {
    content.match_indices(needle).count()
}

fn typed_expression_counts(ontology: &Ontology) -> TypedExpressionCounts {
    let mut counts = TypedExpressionCounts::default();

    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::SubClassOf(sub) => {
                let sub_counts = count_expression(sub.sub_class());
                let super_counts = count_expression(sub.super_class());
                counts.subclass_subclass.add(&sub_counts);
                counts.subclass_superclass.add(&super_counts);
                counts.scanned_positions.add(&sub_counts);
                counts.scanned_positions.add(&super_counts);
            }
            Axiom::ClassAssertion(assertion) => {
                let assertion_counts = count_expression(assertion.class_expr());
                counts.class_assertion.add(&assertion_counts);
                counts.scanned_positions.add(&assertion_counts);
            }
            Axiom::EquivalentClasses(equivalent) => {
                counts.equivalent_classes_named_class_pairs += equivalent.classes().len();
            }
            _ => {}
        }
    }

    counts
}

fn unique_fixture_path() -> Result<std::path::PathBuf, OwlError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| OwlError::StorageError(format!("system clock error: {}", err)))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "tableauxx_branch_census_{}_{}_{}",
        std::process::id(),
        nanos,
        FIXTURE_FILENAME
    )))
}

fn file_sha256(path: &Path) -> Result<String, OwlError> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|err| {
            OwlError::StorageError(format!(
                "failed to execute sha256sum for {}: {}",
                path.display(),
                err
            ))
        })?;
    if !output.status.success() {
        return Err(OwlError::StorageError(format!(
            "sha256sum failed for {} with status {}",
            path.display(),
            output.status
        )));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|err| {
        OwlError::StorageError(format!(
            "sha256sum output was not UTF-8 for {}: {}",
            path.display(),
            err
        ))
    })?;
    stdout
        .split_whitespace()
        .next()
        .map(|value| value.to_string())
        .ok_or_else(|| {
            OwlError::StorageError(format!(
                "sha256sum produced no digest for {}",
                path.display()
            ))
        })
}

fn count_expression(expr: &ClassExpression) -> ExpressionKindCounts {
    let mut counts = ExpressionKindCounts::default();
    match expr {
        ClassExpression::ObjectUnionOf(operands) => {
            counts.union += 1;
            for operand in operands {
                counts.add(&count_expression(operand));
            }
        }
        ClassExpression::ObjectIntersectionOf(operands) => {
            counts.intersection += 1;
            for operand in operands {
                counts.add(&count_expression(operand));
            }
        }
        ClassExpression::ObjectComplementOf(inner) => {
            counts.complement += 1;
            counts.add(&count_expression(inner));
        }
        ClassExpression::ObjectSomeValuesFrom(_, inner) => {
            counts.some_values_from += 1;
            counts.add(&count_expression(inner));
        }
        ClassExpression::ObjectAllValuesFrom(_, inner) => {
            counts.all_values_from += 1;
            counts.add(&count_expression(inner));
        }
        ClassExpression::ObjectMinCardinality(_, _, Some(inner))
        | ClassExpression::ObjectMaxCardinality(_, _, Some(inner))
        | ClassExpression::ObjectExactCardinality(_, _, Some(inner)) => {
            counts.qualified_object_cardinality += 1;
            counts.add(&count_expression(inner));
        }
        ClassExpression::DataMinCardinality(_, _, Some(_))
        | ClassExpression::DataMaxCardinality(_, _, Some(_))
        | ClassExpression::DataExactCardinality(_, _, Some(_)) => {
            counts.qualified_data_cardinality += 1;
        }
        ClassExpression::ObjectHasSelf(_) => counts.has_self += 1,
        _ => {}
    }
    counts
}

impl ExpressionKindCounts {
    fn add(&mut self, other: &Self) {
        self.union += other.union;
        self.intersection += other.intersection;
        self.complement += other.complement;
        self.some_values_from += other.some_values_from;
        self.all_values_from += other.all_values_from;
        self.qualified_object_cardinality += other.qualified_object_cardinality;
        self.qualified_data_cardinality += other.qualified_data_cardinality;
        self.has_self += other.has_self;
    }
}

fn spacl_sequential_stats(ontology: &Ontology) -> SpaclStatsSubset {
    let mut config = SpeculativeConfig::default();
    config.scheduling_mode = SchedulingMode::Sequential;

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let outcome = reasoner.is_consistent();
    let stats = reasoner.get_stats();
    // Qualified certificates and unsupported preflight results can return
    // before disjunction discovery. Default zero counters are not a census.
    let detector_observed = outcome.is_ok() && stats.qualified_consistency_certificate.is_none();
    let execution_status = match &outcome {
        Ok(_) => "success",
        Err(OwlError::UnsupportedFeature(_)) => "unsupported",
        Err(_) => "error",
    };

    SpaclStatsSubset {
        execution_status,
        semantic_result: outcome.as_ref().ok().copied(),
        error: outcome.err().map(|error| error.to_string()),
        detector_observed,
        scheduling_mode: stats.scheduling_mode,
        disjunctions_detected: stats.disjunctions_detected,
        estimated_branch_count: stats.estimated_branch_count,
        estimated_cost_us: stats.estimated_cost_us,
        branch_gate_passed: stats.branch_gate_passed,
        cost_gate_passed: stats.cost_gate_passed,
        used_parallel: stats.used_parallel,
    }
}

fn write_report(path: &Path, report: &Report) -> Result<(), OwlError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            OwlError::StorageError(format!("failed to create {}: {}", parent.display(), err))
        })?;
    }
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| OwlError::StorageError(format!("failed to serialize report: {}", err)))?;
    fs::write(path, format!("{}\n", json)).map_err(|err| {
        OwlError::StorageError(format!("failed to write {}: {}", path.display(), err))
    })
}
