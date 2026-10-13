use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use owl2_reasoner::{
    util::ontology_io::load_ontology_with_env, BranchPolicyMode, Class, ClassAssertionAxiom,
    ClassExpression, DisjointClassesAxiom, NamedIndividual, Ontology, OwlError, SchedulingMode,
    SpeculativeConfig, SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use owl2_reasoner::{Entity, IRI};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

const DEFAULT_TIMEOUT_MS: u64 = 2_000;
const MAX_TIMEOUT_MS: u64 = 60_000;
const MAX_STDIN_BYTES: u64 = 64 * 1024;
const MAX_SYNTHETIC_UNIONS: usize = 10_000;
const MAX_SYNTHETIC_OPERANDS: usize = 256;
const MAX_SYNTHETIC_PADDING: usize = 500_000;

#[derive(Debug, Deserialize)]
struct ProbeRequest {
    input: ProbeInput,
    mode: String,
    workers: usize,
    parallel_threshold: usize,
    cost_threshold_us: usize,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
enum ProbeInput {
    #[serde(rename = "synthetic")]
    Synthetic {
        unions: usize,
        operands: usize,
        padding: usize,
        conflict: bool,
    },
    #[serde(rename = "file")]
    File { path: String },
}

#[derive(Debug, Serialize)]
struct ProbeResponse {
    status: &'static str,
    verdict: ProbeVerdict,
    reasoning_ns: u128,
    features: ProbeFeatures,
    used_parallel: bool,
    boolean_certificate: Option<bool>,
    branches_created: usize,
    nogood_hits: usize,
    ontology_sha256: String,
}

#[derive(Debug, Serialize)]
struct ProbeVerdict {
    result: &'static str,
    error_class: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
struct ProbeFeatures {
    disjunctions: usize,
    branches: usize,
    cost_us: usize,
    workers: usize,
}

fn main() {
    let response = run_probe_from_stdin();
    match serde_json::to_writer(io::stdout(), &response) {
        Ok(()) => {
            let _ = writeln!(io::stdout());
            let _ = io::stdout().flush();
        }
        Err(err) => {
            let fallback = error_response("JsonError", err.to_string());
            let _ = serde_json::to_writer(io::stdout(), &fallback);
            let _ = writeln!(io::stdout());
            let _ = io::stdout().flush();
        }
    }
}

fn run_probe_from_stdin() -> ProbeResponse {
    let mut raw = String::new();
    let mut stdin = io::stdin().take(MAX_STDIN_BYTES + 1);
    if let Err(err) = stdin.read_to_string(&mut raw) {
        return error_response("IoError", err.to_string());
    }
    if raw.len() as u64 > MAX_STDIN_BYTES {
        return error_response(
            "RequestTooLarge",
            format!("stdin request must not exceed {MAX_STDIN_BYTES} bytes"),
        );
    }

    let request = match serde_json::from_str::<ProbeRequest>(&raw) {
        Ok(request) => request,
        Err(err) => return error_response("JsonError", err.to_string()),
    };

    match run_probe(request) {
        Ok(response) => response,
        Err((class, detail)) => error_response(&class, detail),
    }
}

fn run_probe(request: ProbeRequest) -> Result<ProbeResponse, (String, String)> {
    let scheduling_mode = parse_mode(&request.mode)?;
    validate_request(&request)?;

    prepare_safe_loader_env();

    let ontology = match &request.input {
        ProbeInput::Synthetic {
            unions,
            operands,
            padding,
            conflict,
        } => synthetic_ontology(*unions, *operands, *padding, *conflict)?,
        ProbeInput::File { path } => {
            let path = Path::new(path);
            if path.extension().map(|ext| ext == "owlbin").unwrap_or(false) {
                return Err((
                    "InvalidFileInput".to_string(),
                    ".owlbin inputs are disabled for the isolated prototype probe".to_string(),
                ));
            }
            load_ontology_with_env(path)
                .map_err(|err| (error_class(&err).to_string(), err.to_string()))?
        }
    };

    let ontology_sha256 = ontology_sha256(&ontology)?;
    let ontology = Arc::new(ontology);

    let mut config = SpeculativeConfig::default();
    config.num_workers = request.workers;
    config.scheduling_mode = scheduling_mode;
    config.branch_policy = BranchPolicyMode::Baseline;
    config.parallel_threshold = request.parallel_threshold;
    config.cost_threshold_us = request.cost_threshold_us;
    config.cost_per_operand_us = 50;
    config.cost_per_nesting_us = 15;
    config.enable_learning = true;
    config.speculative_timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

    let mut reasoner = SpeculativeTableauxReasoner::with_config_arc(ontology, config);

    let start = Instant::now();
    let result = reasoner.is_consistent();
    let reasoning_ns = start.elapsed().as_nanos();

    let stats = reasoner.get_stats();
    let (_, nogood_hits) = reasoner.get_nogood_stats();

    let (status, verdict) = match result {
        Ok(true) => (
            "success",
            ProbeVerdict {
                result: "consistent",
                error_class: None,
            },
        ),
        Ok(false) => (
            "success",
            ProbeVerdict {
                result: "inconsistent",
                error_class: None,
            },
        ),
        Err(err) => (
            "error",
            ProbeVerdict {
                result: "error",
                error_class: Some(error_class(&err).to_string()),
            },
        ),
    };
    let features = ProbeFeatures {
        disjunctions: stats.disjunctions_detected,
        branches: stats.estimated_branch_count,
        cost_us: stats.estimated_cost_us,
        workers: request.workers,
    };

    Ok(ProbeResponse {
        status,
        verdict,
        reasoning_ns,
        features,
        used_parallel: stats.used_parallel,
        boolean_certificate: stats.boolean_certificate,
        branches_created: stats.branches_created,
        nogood_hits: stats.nogood_hits.max(nogood_hits),
        ontology_sha256,
    })
}

fn parse_mode(mode: &str) -> Result<SchedulingMode, (String, String)> {
    match mode {
        "adaptive" => Ok(SchedulingMode::Adaptive),
        "sequential" => Ok(SchedulingMode::Sequential),
        "always_parallel" => Ok(SchedulingMode::AlwaysParallel),
        _ => Err((
            "InvalidMode".to_string(),
            "mode must be adaptive, sequential, or always_parallel".to_string(),
        )),
    }
}

fn validate_request(request: &ProbeRequest) -> Result<(), (String, String)> {
    if request.workers == 0 {
        return Err((
            "InvalidWorkers".to_string(),
            "workers must be greater than zero".to_string(),
        ));
    }
    if request.workers > 256 {
        return Err((
            "InvalidWorkers".to_string(),
            "workers must not exceed 256".to_string(),
        ));
    }
    if request
        .cost_threshold_us
        .checked_mul(request.workers)
        .is_none()
    {
        return Err((
            "InvalidCostThreshold".to_string(),
            "cost_threshold_us * workers overflows usize".to_string(),
        ));
    }
    if request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS) > MAX_TIMEOUT_MS {
        return Err((
            "InvalidTimeout".to_string(),
            format!("timeout_ms must not exceed {MAX_TIMEOUT_MS}"),
        ));
    }
    if let ProbeInput::Synthetic {
        unions,
        operands,
        padding,
        ..
    } = &request.input
    {
        if *unions > MAX_SYNTHETIC_UNIONS {
            return Err((
                "InvalidSyntheticInput".to_string(),
                format!("synthetic unions must not exceed {MAX_SYNTHETIC_UNIONS}"),
            ));
        }
        if *operands == 0 || *operands > MAX_SYNTHETIC_OPERANDS {
            return Err((
                "InvalidSyntheticInput".to_string(),
                format!("synthetic operands must be in 1..={MAX_SYNTHETIC_OPERANDS}"),
            ));
        }
        if *padding > MAX_SYNTHETIC_PADDING {
            return Err((
                "InvalidSyntheticInput".to_string(),
                format!("synthetic padding must not exceed {MAX_SYNTHETIC_PADDING}"),
            ));
        }
    }
    Ok(())
}

fn synthetic_ontology(
    unions: usize,
    operands: usize,
    padding: usize,
    conflict: bool,
) -> Result<Ontology, (String, String)> {
    let mut ontology = Ontology::new();

    for i in 0..unions {
        let trigger = class(&format!("http://example.org/probe/Trigger{i}"));
        ontology
            .add_class(trigger.clone())
            .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;

        let mut union_operands: SmallVec<[Box<ClassExpression>; 4]> = SmallVec::new();
        let mut operand_classes = Vec::with_capacity(operands);
        for j in 0..operands {
            let operand = class(&format!("http://example.org/probe/U{i}Operand{j}"));
            ontology
                .add_class(operand.clone())
                .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            union_operands.push(Box::new(ClassExpression::Class(operand.clone())));
            operand_classes.push(operand.clone());
        }

        let union = ClassExpression::ObjectUnionOf(union_operands);
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(trigger.clone()),
                union.clone(),
            ))
            .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;

        if conflict {
            let individual = NamedIndividual::new(format!("http://example.org/probe/ind{i}"));
            ontology
                .add_named_individual(individual.clone())
                .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    ClassExpression::Class(trigger),
                ))
                .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(individual.iri().clone(), union))
                .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            if operand_classes.len() >= 2 {
                ontology
                    .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                        operand_classes[0].iri().clone(),
                        operand_classes[1].iri().clone(),
                    ]))
                    .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            }
            for operand in operand_classes {
                ontology
                    .add_class_assertion(ClassAssertionAxiom::new(
                        individual.iri().clone(),
                        ClassExpression::Class(operand),
                    ))
                    .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            }
        }
    }

    if padding > 0 {
        let mut padding_classes = Vec::with_capacity(padding);
        for i in 0..padding {
            let iri = IRI::new_unchecked(format!("http://example.org/probe/Padding{i}"))
                .map_err(|err| (error_class(&err).to_string(), err.to_string()))?;
            padding_classes.push(<Class as Entity>::from_shared_iri(Arc::new(iri)));
        }
        ontology.add_classes_bulk_trusted(padding_classes);
    }

    Ok(ontology)
}

fn prepare_safe_loader_env() {
    std::env::set_var("OWL2_REASONER_FORCE_TEXT", "1");
    std::env::set_var("OWL2_REASONER_BIN_ONLY", "0");
    std::env::set_var("OWL2_REASONER_AUTO_CACHE", "0");
    std::env::set_var("OWL2_REASONER_LARGE_PROFILE_AUTO", "0");
}

fn class(iri: &str) -> Class {
    Class::new(iri.to_string())
}

fn ontology_sha256(ontology: &Ontology) -> Result<String, (String, String)> {
    let mut classes: Vec<String> = ontology
        .classes()
        .iter()
        .map(|class| class.iri().as_str().to_string())
        .collect();
    classes.sort();

    let mut axioms: Vec<String> = ontology
        .axioms()
        .iter()
        .map(|axiom| format!("{axiom:?}"))
        .collect();
    axioms.sort();

    let mut canonical = String::new();
    canonical.push_str("classes\n");
    for class in classes {
        canonical.push_str(&class);
        canonical.push('\n');
    }
    canonical.push_str("axioms\n");
    for axiom in axioms {
        canonical.push_str(&axiom);
        canonical.push('\n');
    }

    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| ("HashError".to_string(), err.to_string()))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(canonical.as_bytes())
            .map_err(|err| ("HashError".to_string(), err.to_string()))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| ("HashError".to_string(), err.to_string()))?;
    if !output.status.success() {
        return Err(("HashError".to_string(), "sha256sum failed".to_string()));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| ("HashError".to_string(), err.to_string()))?;
    stdout
        .split_whitespace()
        .next()
        .map(|hash| hash.to_string())
        .ok_or_else(|| {
            (
                "HashError".to_string(),
                "sha256sum returned no hash".to_string(),
            )
        })
}

fn error_class(error: &OwlError) -> &'static str {
    match error {
        OwlError::InvalidIRI(_) => "InvalidIRI",
        OwlError::IriParseError { .. } => "IriParseError",
        OwlError::IriCreationError { .. } => "IriCreationError",
        OwlError::UnknownPrefix(_) => "UnknownPrefix",
        OwlError::ParseError(_) => "ParseError",
        OwlError::ParseErrorWithLocation { .. } => "ParseErrorWithLocation",
        OwlError::SerializationError(_) => "SerializationError",
        OwlError::ReasoningError(_) => "ReasoningError",
        OwlError::UnsupportedFeature(_) => "UnsupportedFeature",
        OwlError::TableauxError { .. } => "TableauxError",
        OwlError::GraphError { .. } => "GraphError",
        OwlError::QueryError(_) => "QueryError",
        OwlError::StorageError(_) => "StorageError",
        OwlError::CacheError { .. } => "CacheError",
        OwlError::LockError { .. } => "LockError",
        OwlError::ValidationError(_) => "ValidationError",
        OwlError::EntityValidationError { .. } => "EntityValidationError",
        OwlError::AxiomValidationError { .. } => "AxiomValidationError",
        OwlError::OwlViolation(_) => "OwlViolation",
        OwlError::ProfileViolation { .. } => "ProfileViolation",
        OwlError::InconsistentOntology(_) => "InconsistentOntology",
        OwlError::ResourceLimitExceeded { .. } => "ResourceLimitExceeded",
        OwlError::TimeoutError { .. } => "TimeoutError",
        OwlError::ConfigError { .. } => "ConfigError",
        OwlError::ImportResolutionError { .. } => "ImportResolutionError",
        OwlError::IoError(_) => "IoError",
        OwlError::Utf8Error(_) => "Utf8Error",
        OwlError::JsonError(_) => "JsonError",
        OwlError::UnexpectedStructure(_) => "UnexpectedStructure",
        OwlError::ExpectedNamedObjectProperty => "ExpectedNamedObjectProperty",
        OwlError::ExpectedLiteralValue => "ExpectedLiteralValue",
        OwlError::ExpectedFunctionalPropertyAxiom => "ExpectedFunctionalPropertyAxiom",
        OwlError::ExpectedReflexivePropertyAxiom => "ExpectedReflexivePropertyAxiom",
        OwlError::ExpectedTransitivePropertyAxiom => "ExpectedTransitivePropertyAxiom",
        OwlError::ExpectedSubDataPropertyAxiom => "ExpectedSubDataPropertyAxiom",
        OwlError::ExpectedFunctionalDataPropertyAxiom => "ExpectedFunctionalDataPropertyAxiom",
        OwlError::ExpectedEquivalentDataPropertiesAxiom => "ExpectedEquivalentDataPropertiesAxiom",
        OwlError::ExpectedDisjointDataPropertiesAxiom => "ExpectedDisjointDataPropertiesAxiom",
        OwlError::ExpectedSameIndividualAxiom => "ExpectedSameIndividualAxiom",
        OwlError::ExpectedDifferentIndividualsAxiom => "ExpectedDifferentIndividualsAxiom",
        OwlError::ExpectedSubPropertyChainOfAxiom => "ExpectedSubPropertyChainOfAxiom",
        OwlError::ExpectedInverseObjectPropertiesAxiom => "ExpectedInverseObjectPropertiesAxiom",
        OwlError::Other(_) => "Other",
    }
}

fn error_response(error_class: &str, _detail: String) -> ProbeResponse {
    ProbeResponse {
        status: "error",
        verdict: ProbeVerdict {
            result: "error",
            error_class: Some(error_class.to_string()),
        },
        reasoning_ns: 0,
        features: ProbeFeatures::default(),
        used_parallel: false,
        boolean_certificate: None,
        branches_created: 0,
        nogood_hits: 0,
        ontology_sha256: String::new(),
    }
}
