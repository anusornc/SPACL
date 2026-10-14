//! OWL2 Reasoner CLI - Demo Application
//!
//! A command-line tool for reasoning with OWL2 ontologies.
//!
//! Usage:
//!   cargo run --bin owl2-reasoner -- <command> [options] <ontology_file>
//!
//! Commands:
//!   check       Check ontology consistency
//!   check-auto  Check with automatic reasoner selection
//!   convert     Convert OWL to binary format
//!   stats       Show ontology statistics
//!   compare     Compare Sequential vs SPACL performance
//!
//! Examples:
//!   cargo run --bin owl2-reasoner -- check tests/data/univ-bench.owl
//!   cargo run --bin owl2-reasoner -- stats tests/data/univ-bench.owl
//!   cargo run --bin owl2-reasoner -- compare tests/data/univ-bench.owl

use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;

use owl2_reasoner::{
    detect_profile, evaluate_consistency_with_config, evaluate_entailment,
    evaluate_inconsistency_with_config, select_consistency_reasoner,
    serializer::BinaryOntologyFormat, util::ontology_io::load_ontology_with_env, BranchPolicyMode,
    ConsistencyReasoner, EvaluationOperation, EvaluationResponse, Ontology,
    OntologyCharacteristics, OwlError, SchedulingMode, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner,
};
use owl2_reasoner::reasoner::evaluation::{EvaluationStatus, SemanticResult};

fn flush_stdout() {
    let _ = std::io::stdout().flush();
}

fn phase_log(label: &str, detail: &str) {
    eprintln!("[phase] {} {}", label, detail);
}

fn env_truthy(key: &str) -> bool {
    match env::var(key) {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            !(value.is_empty() || value == "0" || value == "false" || value == "no")
        }
        Err(_) => false,
    }
}

fn scheduling_mode_from_env() -> Result<SchedulingMode, String> {
    match env::var("SPACL_SCHED_MODE") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "adaptive" => Ok(SchedulingMode::Adaptive),
            "sequential" => Ok(SchedulingMode::Sequential),
            "always_parallel" | "always-parallel" | "parallel" | "forced_parallel" => {
                Ok(SchedulingMode::AlwaysParallel)
            }
            other => Err(format!(
                "unsupported SPACL_SCHED_MODE='{}' (expected adaptive|sequential|always_parallel)",
                other
            )),
        },
        Err(_) => Ok(SchedulingMode::Adaptive),
    }
}

fn branch_policy_from_env() -> Result<BranchPolicyMode, String> {
    match env::var("SPACL_BRANCH_POLICY") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "baseline" | "default" => Ok(BranchPolicyMode::Baseline),
            "heuristic" | "ranked" => Ok(BranchPolicyMode::Heuristic),
            "hybrid_rrn" | "hybrid-rrn" | "rrn" => Ok(BranchPolicyMode::HybridRrn),
            other => Err(format!(
                "unsupported SPACL_BRANCH_POLICY='{}' (expected baseline|heuristic|hybrid_rrn)",
                other
            )),
        },
        Err(_) => Ok(BranchPolicyMode::Baseline),
    }
}

fn speculative_config_from_env() -> Result<SpeculativeConfig, String> {
    let mut config = SpeculativeConfig::default();
    config.scheduling_mode = scheduling_mode_from_env()?;
    config.branch_policy = branch_policy_from_env()?;
    if let Ok(path) = env::var("SPACL_RRN_MODEL_PATH") {
        let path = path.trim();
        if !path.is_empty() {
            config.rrn_model_path = Some(path.to_string());
        }
    }
    if let Ok(path) = env::var("SPACL_BRANCH_SNAPSHOT_FILE") {
        let path = path.trim();
        if !path.is_empty() {
            config.branch_snapshot_path = Some(path.to_string());
        }
    }
    if env::var("SPACL_NOGOOD").is_ok() {
        config.enable_learning = env_truthy("SPACL_NOGOOD");
    }
    Ok(config)
}

fn emit_spacl_stats(reasoner: &SpeculativeTableauxReasoner) {
    if !env_truthy("SPACL_EMIT_STATS") {
        return;
    }
    let stats = reasoner.get_stats();
    eprintln!(
        "[spacl] mode={} branch_policy={} used_parallel={} branches_created={} work_items_expanded={} branches_pruned={} nogood_hits={} local_cache_hits={} global_cache_hits={} steal_attempts={} steal_successes={} policy_reordered_splits={} policy_fallbacks={} hybrid_policy_calls={} hybrid_model_calls={} branch_snapshots_written={}",
        stats.scheduling_mode,
        stats.branch_policy,
        stats.used_parallel,
        stats.branches_created,
        stats.work_items_expanded,
        stats.branches_pruned,
        stats.nogood_hits,
        stats.local_cache_hits,
        stats.global_cache_hits,
        stats.steal_attempts,
        stats.steal_successes,
        stats.policy_reordered_splits,
        stats.policy_fallbacks,
        stats.hybrid_policy_calls,
        stats.hybrid_model_calls,
        stats.branch_snapshots_written
    );
}

#[derive(Debug, Serialize)]
struct CheckAutoReport {
    schema_version: u32,
    report_type: &'static str,
    operation: &'static str,
    input_file: String,
    status: &'static str,
    reasoning_result: &'static str,
    correctness_status: &'static str,
    selected_reasoner: Option<&'static str>,
    parse_time_ms: Option<u128>,
    reason_time_ms: Option<u128>,
    duration_ms: u128,
    /// "skipped_size_gate" when the merge probe was skipped by the
    /// #168 size gate, or "budget_exhausted" when it entered and spent
    /// a budget mid-search (#171, ADR 0022) — both postures are
    /// fail-open on merge rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    merge_probe: Option<&'static str>,
    error: Option<String>,
}

impl CheckAutoReport {
    fn new(input_file: String, duration_ms: u128) -> Self {
        Self {
            schema_version: 1,
            report_type: "tableauxx_check_auto",
            operation: "consistency",
            input_file,
            status: "failed",
            reasoning_result: "error",
            correctness_status: "not_evaluated",
            selected_reasoner: None,
            parse_time_ms: None,
            reason_time_ms: None,
            duration_ms,
            merge_probe: None,
            error: None,
        }
    }
}

fn write_check_auto_report(mut file: File, report: &CheckAutoReport) -> Result<(), String> {
    serde_json::to_writer(&mut file, report).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())
}

fn canonical_parent_and_file_name(path: &Path) -> Option<(PathBuf, std::ffi::OsString)> {
    let file_name = path.file_name()?.to_os_string();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    Some((parent.canonicalize().ok()?, file_name))
}

fn paths_resolve_to_same_leaf(left: &str, right: &str) -> bool {
    let left = Path::new(left);
    let right = Path::new(right);
    if left == right {
        return true;
    }

    match (
        canonical_parent_and_file_name(left),
        canonical_parent_and_file_name(right),
    ) {
        (Some((left_parent, left_file)), Some((right_parent, right_file))) => {
            left_parent == right_parent && left_file == right_file
        }
        _ => false,
    }
}

fn check_auto_exit_for_error(error: &OwlError) -> (&'static str, &'static str, i32) {
    match error {
        OwlError::UnsupportedFeature(_) => ("unsupported", "unknown", 2),
        OwlError::TimeoutError { .. } => ("timeout", "unknown", 124),
        _ => ("failed", "error", 1),
    }
}

fn print_usage() {
    println!("OWL2 Reasoner CLI - Demo Application");
    println!();
    println!("Usage: owl2-reasoner <command> [options] <ontology_file>");
    println!();
    println!("Commands:");
    println!("  check <file>       Check ontology consistency");
    println!("  check-auto <file>  Check with automatic reasoner selection");
    println!("  convert <in> <out> Convert OWL to binary format (.owlbin)");
    println!("  stats <file>       Show ontology statistics");
    println!("  compare <file>     Compare Sequential vs SPACL performance");
    println!("  evaluate <op> ...  Emit machine-readable consistency/entailment JSON");
    println!("  materialize <in> <out>  Apply RL-profile reasoning, write the closure as N-Triples");
    println!("  help               Show this help message");
    println!();
    println!("Options:");
    println!("  -v, --verbose      Verbose output");
    println!("  --report-json <new-file>  check-auto create-new JSON report");
    println!("                            check-auto exits: 0 success, 1 failed, 2 unsupported, 124 timeout");
    println!();
    println!("Environment (large ontology / production):");
    println!("  OWL2_REASONER_LARGE_PARSE=1   Enable large-file parser mode");
    println!("  OWL2_REASONER_AUTO_CACHE=1    Parse text and write .owlbin cache");
    println!("  OWL2_REASONER_FORCE_TEXT=1    Ignore .owlbin and parse source text");
    println!("  OWL2_REASONER_BIN_ONLY=1      Require .owlbin input/cache");
    println!("  OWL2_REASONER_MAX_FILE_SIZE   Override parser max file size in bytes");
    println!("  OWL2_REASONER_LARGE_PROFILE_AUTO=0  Disable auto large-file text profile");
    println!(
        "  OWL2_REASONER_LARGE_PROFILE_THRESHOLD=<bytes>  Auto profile threshold (default 4MB)"
    );
    println!(
        "  OWL2_REASONER_STRUCTURAL_XML_AUTO=0  Disable auto structural RDF/XML for large files"
    );
    println!(
        "  OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD=<bytes>  Auto mode threshold (default 4MB)"
    );
    println!("  OWL2_REASONER_EXPERIMENTAL_XML_PARSER=1  Enable experimental RDF/XML pipeline");
    println!("  OWL2_REASONER_EXPERIMENTAL_XML_STRICT=1  Fail if unsupported experimental terms are skipped");
    println!("  OWL2_REASONER_BIN_FORMAT=v1|v2|v3|v4   Binary format (default v4); legacy writes reject unrepresentable semantics");
    println!("  SPACL_SCHED_MODE=adaptive|sequential|always_parallel");
    println!("  SPACL_BRANCH_POLICY=baseline|heuristic|hybrid_rrn");
    println!("  SPACL_RRN_MODEL_PATH=<path>   JSON model (linear or gbdt_stump_v1) for hybrid_rrn branch ranking");
    println!("  SPACL_BRANCH_SNAPSHOT_FILE=<path>   Export branch-level policy snapshots (.jsonl)");
    println!("  SPACL_NOGOOD=0|1             Disable/enable nogood learning for ablations");
    println!("  SPACL_EMIT_STATS=1           Emit one-line SPACL telemetry after reasoning");
    println!();
    println!("Examples:");
    println!("  OWL2_REASONER_LARGE_PARSE=1 owl2-reasoner check large.owl");
    println!("  OWL2_REASONER_AUTO_CACHE=1 owl2-reasoner check large.owl");
    println!("  owl2-reasoner check tests/data/univ-bench.owl");
    println!("  owl2-reasoner check-auto large.owl --report-json result.json");
    println!("  owl2-reasoner convert large.owl large.owlbin");
    println!("  owl2-reasoner check large.owlbin");
    println!("  owl2-reasoner stats tests/data/univ-bench.owl");
    println!("  owl2-reasoner compare tests/data/univ-bench.owl");
    println!("  owl2-reasoner evaluate consistency tests/data/univ-bench.owl");
    println!("  owl2-reasoner evaluate entailment premise.owl conclusion.owl");
}

fn load_ontology_for_evaluation(path: &str) -> Result<(Arc<Ontology>, u128), String> {
    let path_ref = Path::new(path);
    phase_log("parse_start", &format!("file={}", path_ref.display()));
    let start = Instant::now();
    let ontology = load_ontology_with_env(path_ref).map_err(|error| error.to_string())?;
    let parse_time_ms = start.elapsed().as_millis();
    phase_log("parse_done", &format!("ms={parse_time_ms}"));
    Ok((Arc::new(ontology), parse_time_ms))
}

fn emit_evaluation_response(mut response: EvaluationResponse, parse_time_ms: u128) {
    response.parse_time_ms = parse_time_ms;
    match serde_json::to_string(&response) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("{{\"status\":\"failed\",\"semantic_result\":\"error\",\"error\":{error:?}}}")
        }
    }
}

fn cmd_evaluate(args: &[String]) -> ExitCode {
    // Track B (ADR 0006): the FULL lane's opt-in routing. v0's
    // RDF-based evaluator answers the families it covers and falls
    // back to the Direct evaluation for everything else, so the flag
    // cannot regress currently-answered rows.
    let rdf_based = args.iter().any(|arg| arg == "--rdf-based");
    let args: Vec<&String> = args
        .iter()
        .filter(|arg| arg.as_str() != "--rdf-based")
        .collect();
    let args: &[&String] = &args;
    let operation_name = args.first().map(|value| value.as_str()).unwrap_or("unknown");
    let operation = EvaluationOperation::parse(operation_name);
    if !operation.is_supported() {
        let response = EvaluationResponse::unsupported(
            operation,
            String::new(),
            None,
            "supported operations are consistency, inconsistency, and entailment",
        );
        let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

        emit_evaluation_response(response, 0);

        return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    let expected_arg_count = match operation {
        EvaluationOperation::Consistency | EvaluationOperation::Inconsistency => 2,
        EvaluationOperation::Entailment => 3,
        EvaluationOperation::Unknown => unreachable!("unknown operation was rejected above"),
        _ => unreachable!("future operation was rejected above"),
    };
    if args.len() != expected_arg_count {
        let response = EvaluationResponse::failed(
            operation,
            args.get(1).map(|value| value.to_string()).unwrap_or_default(),
            format!(
                "{} expects {} argument(s) after the command",
                operation.as_str(),
                expected_arg_count - 1
            ),
        );
        let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

        emit_evaluation_response(response, 0);

        return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    let input_path: &str = args[1].as_str();
    let (premise, premise_parse_time_ms) = match load_ontology_for_evaluation(input_path) {
        Ok(value) => value,
        Err(error) => {
            let response = EvaluationResponse::failed(operation, input_path.to_string(), error);
            let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

            emit_evaluation_response(response, 0);

            return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
    };

    match operation {
        EvaluationOperation::Consistency => {
            let config = match speculative_config_from_env() {
                Ok(config) => config,
                Err(error) => {
                    let response = EvaluationResponse::failed(operation, input_path.to_string(), error);
                    let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

                    emit_evaluation_response(response, premise_parse_time_ms);

                    return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
                }
            };
            let response = evaluate_consistency_with_config(premise, input_path.clone(), config);
            let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);
            emit_evaluation_response(response, premise_parse_time_ms);
            return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
        EvaluationOperation::Inconsistency => {
            if rdf_based {
                let attempt = (|| -> Option<EvaluationResponse> {
                    let mut parser = owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser::new(
                        owl2_reasoner::parser::ParserConfig::default(),
                    );
                    let premise_graph = parser
                        .extract_rdf_graph(Path::new(args[1].as_str()))
                        .ok()?;
                    let start = Instant::now();
                    owl2_reasoner::reasoner::rdf_based::RdfBasedEvaluator::new(premise_graph)
                        .has_contradiction()
                        .filter(|inconsistent| *inconsistent)
                        .map(|_| {
                            EvaluationResponse::completed(
                                EvaluationOperation::Inconsistency,
                                args[1].clone(),
                                None,
                                SemanticResult::True,
                                start.elapsed().as_millis(),
                                Some("owl2_rdf_based_graph_closure"),
                                None,
                            )
                        })
                })();
                if let Some(response) = attempt {
                    let ok =
                        !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);
                    emit_evaluation_response(response, 0);
                    return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
                }
                // No visible violation: fall through to the Direct
                // evaluation (positive-only claims).
            }
            let config = match speculative_config_from_env() {
                Ok(config) => config,
                Err(error) => {
                    let response = EvaluationResponse::failed(operation, input_path.to_string(), error);
                    let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

                    emit_evaluation_response(response, premise_parse_time_ms);

                    return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
                }
            };
            let response = evaluate_inconsistency_with_config(premise, input_path.clone(), config);
            let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);
            emit_evaluation_response(response, premise_parse_time_ms);
            return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
        EvaluationOperation::Entailment => {
            let conclusion_path = &args[2];
            if rdf_based {
                let attempt = (|| -> Option<EvaluationResponse> {
                    let mut parser = owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser::new(
                        owl2_reasoner::parser::ParserConfig::default(),
                    );
                    let premise_graph = parser
                        .extract_rdf_graph(Path::new(args[1].as_str()))
                        .ok()?;
                    let conclusion_graph = parser
                        .extract_rdf_graph(Path::new(conclusion_path.as_str()))
                        .ok()?;
                    let start = Instant::now();
                    owl2_reasoner::reasoner::rdf_based::RdfBasedEvaluator::new(premise_graph)
                        .entails_graph(&conclusion_graph)
                        .map(|entailed| {
                            EvaluationResponse::completed(
                                EvaluationOperation::Entailment,
                                args[1].clone(),
                                Some(conclusion_path.to_string()),
                                if entailed {
                                    SemanticResult::True
                                } else {
                                    SemanticResult::False
                                },
                                start.elapsed().as_millis(),
                                Some("owl2_rdf_based_graph_closure"),
                                None,
                            )
                        })
                })();
                if let Some(response) = attempt {
                    let ok =
                        !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);
                    emit_evaluation_response(response, 0);
                    return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
                }
                // None (or a graph read failure) means the shape is
                // outside v0's families — fall through to the Direct
                // evaluation below.
            }
            let (conclusion, conclusion_parse_time_ms) =
                match load_ontology_for_evaluation(conclusion_path) {
                    Ok(value) => value,
                    Err(error) => {
                        let mut response =
                            EvaluationResponse::failed(operation, input_path.to_string(), error);
                        response.conclusion_ontology = Some(conclusion_path.to_string());
                        let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

                        emit_evaluation_response(response, premise_parse_time_ms);

                        return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
                    }
                };

            let response = evaluate_entailment(
                premise,
                conclusion,
                input_path.clone(),
                conclusion_path.clone(),
            );
            let ok = !matches!(response.status, EvaluationStatus::Failed | EvaluationStatus::Timeout);

            emit_evaluation_response(response, premise_parse_time_ms + conclusion_parse_time_ms);

            return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
        EvaluationOperation::Unknown => unreachable!("unknown operation was rejected above"),
        _ => unreachable!("future operation was rejected above"),
    }
}

fn load_ontology(path: &str) -> Result<Arc<Ontology>, String> {
    load_ontology_timed(path).map(|(ontology, _)| ontology)
}

fn load_ontology_timed(path: &str) -> Result<(Arc<Ontology>, u128), String> {
    let path = Path::new(path);
    println!("Loading ontology: {}", path.display());
    flush_stdout();
    phase_log("parse_start", &format!("file={}", path.display()));
    let start = Instant::now();
    let ontology = load_ontology_with_env(path).map_err(|e| format!("{}", e))?;
    let load_time = start.elapsed();
    println!("✓ Loaded in {:?}", load_time);
    flush_stdout();
    phase_log("parse_done", &format!("ms={}", load_time.as_millis()));
    Ok((Arc::new(ontology), load_time.as_millis()))
}

fn parse_check_auto_args(args: &[String]) -> Result<(&str, Option<&str>), String> {
    if args.is_empty() {
        return Err("No ontology file specified".to_string());
    }
    if args.len() == 1 {
        return Ok((&args[0], None));
    }
    if args.len() == 3 && args[1] == "--report-json" {
        return Ok((&args[0], Some(&args[2])));
    }
    Err("Usage: owl2-reasoner check-auto <ontology_file> [--report-json <new_file>]".to_string())
}

fn cmd_check_auto(args: &[String]) -> i32 {
    let cli_start = Instant::now();
    let (path, report_path) = match parse_check_auto_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("Error: {}", error);
            println!("Usage: owl2-reasoner check-auto <ontology_file> [--report-json <new_file>]");
            return 1;
        }
    };

    let report_file = match report_path {
        Some(report_path) => {
            if paths_resolve_to_same_leaf(path, report_path) {
                eprintln!("Error: --report-json path must not target the input file");
                return 1;
            }

            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(report_path)
            {
                Ok(file) => Some(file),
                Err(error) => {
                    eprintln!("Error creating report file: {}", error);
                    return 1;
                }
            }
        }
        None => None,
    };

    let mut report = CheckAutoReport::new(path.to_string(), 0);

    let (ontology, parse_time_ms) = match load_ontology_timed(path) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("Error: {}", e);
            report.error = Some(e);
            report.duration_ms = cli_start.elapsed().as_millis();
            return finish_check_auto(report_file, &report, 1);
        }
    };
    report.parse_time_ms = Some(parse_time_ms);

    let profile = detect_profile(&ontology);
    let analysis = OntologyCharacteristics::analyze(&ontology);
    let decision = select_consistency_reasoner(&ontology, profile);
    report.selected_reasoner = Some(decision.reasoner.as_str());

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║           ONTOLOGY ANALYSIS & REASONER SELECTION             ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Statistics:");
    println!("  Classes:          {}", analysis.class_count);
    println!("  Properties:       {}", analysis.property_count);
    println!("  Axioms:           {}", ontology.axioms().len());
    println!("  Disjunctions:     {}", analysis.disjunction_count);
    println!("  Complexity:       {}", analysis.complexity_description());
    println!();
    println!("Characteristics:");
    println!(
        "  Hierarchy fast path: {}",
        if owl2_reasoner::HierarchicalClassificationEngine::can_handle(&ontology) {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "  Detected profile: {}",
        profile.map_or("OWL2 DL".to_string(), |p| p.name().to_string())
    );
    println!();

    let start = Instant::now();
    phase_log(
        "reason_start",
        &format!("reasoner={}", decision.reasoner.as_str()),
    );
    let consistent = match decision.reasoner {
        ConsistencyReasoner::Simple => {
            println!(
                "Selected: {} ({})",
                decision.reasoner.as_str(),
                decision.rationale
            );
            let reasoner = SimpleReasoner::from_arc(Arc::clone(&ontology));
            reasoner.is_consistent()
        }
        ConsistencyReasoner::Speculative => {
            println!(
                "Selected: {} ({})",
                decision.reasoner.as_str(),
                decision.rationale
            );
            let config = match speculative_config_from_env() {
                Ok(config) => config,
                Err(err) => {
                    eprintln!("Error: {}", err);
                    report.error = Some(err);
                    report.duration_ms = cli_start.elapsed().as_millis();
                    return finish_check_auto(report_file, &report, 1);
                }
            };
            let mut reasoner =
                SpeculativeTableauxReasoner::with_config_arc(Arc::clone(&ontology), config);
            let result = reasoner.is_consistent();
            emit_spacl_stats(&reasoner);
            result
        }
    };

    let check_time = start.elapsed();

    // The gate can fire inside either lane (speculative workers build
    // their own SimpleReasoners), so the marker reads the process-wide
    // latch rather than a reasoner handle (#168).
    if owl2_reasoner::reasoner::simple::merge_probe_skipped_globally() {
        report.merge_probe = Some("skipped_size_gate");
        println!(
            "⚠ Merge probe skipped by the size gate: the verdict is fail-open on merge rules (OWL2_REASONER_MERGE_PROBE_MAX_NODES)"
        );
    } else if owl2_reasoner::reasoner::simple::merge_probe_exhausted_globally() {
        // A probe that entered and spent a budget mid-search (#171, ADR
        // 0022): same fail-open verdict, now observable — distinct from
        // the entry gate's skip.
        report.merge_probe = Some("budget_exhausted");
        println!(
            "⚠ Merge probe spent its budget mid-search: the verdict is fail-open on merge rules (OWL2_REASONER_MERGE_PROBE_WALK_NODES)"
        );
    }

    match consistent {
        Ok(result) => {
            println!("\n✓ Consistency check complete in {:?}", check_time);
            flush_stdout();
            phase_log(
                "reason_done",
                &format!(
                    "ms={} result={}",
                    check_time.as_millis(),
                    if result { "consistent" } else { "inconsistent" }
                ),
            );
            println!();
            if result {
                println!("Result: ✅ CONSISTENT");
            } else {
                println!("Result: ❌ INCONSISTENT");
            }
            report.status = "success";
            report.reasoning_result = if result { "consistent" } else { "inconsistent" };
            report.reason_time_ms = Some(check_time.as_millis());
            report.duration_ms = cli_start.elapsed().as_millis();
            finish_check_auto(report_file, &report, 0)
        }
        Err(e) => {
            eprintln!("Error during reasoning: {:?}", e);
            let (status, result, exit_code) = check_auto_exit_for_error(&e);
            report.status = status;
            report.reasoning_result = result;
            report.error = Some(e.to_string());
            report.duration_ms = cli_start.elapsed().as_millis();
            finish_check_auto(report_file, &report, exit_code)
        }
    }
}

fn finish_check_auto(report_file: Option<File>, report: &CheckAutoReport, exit_code: i32) -> i32 {
    if let Some(file) = report_file {
        if let Err(error) = write_check_auto_report(file, report) {
            eprintln!("Error writing report file: {}", error);
            return 1;
        }
    }
    exit_code
}

fn cmd_convert(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("Error: Need input and output files");
        return ExitCode::FAILURE;
        println!("Usage: owl2-reasoner convert <input.owl> <output.owlbin>");
        return ExitCode::FAILURE;
    }

    let input_path = &args[0];
    let output_path = &args[1];

    // Load ontology from OWL
    let ontology = match load_ontology(input_path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Serialize to binary
    println!("\nConverting to binary format...");
    let start = Instant::now();

    let mut file = match std::fs::File::create(output_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating output file: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match BinaryOntologyFormat::serialize(&ontology, &mut file) {
        Ok(()) => {
            let convert_time = start.elapsed();
            println!("✓ Conversion complete in {:?}", convert_time);

            // Show file size comparison
            let input_meta = std::fs::metadata(input_path).ok();
            let output_meta = std::fs::metadata(output_path).ok();

            if let (Some(in_meta), Some(out_meta)) = (input_meta, output_meta) {
                let in_size = in_meta.len();
                let out_size = out_meta.len();
                let ratio = out_size as f64 / in_size as f64;

                println!();
                println!("File sizes:");
                println!(
                    "  Input:  {} bytes ({:.2} MB)",
                    in_size,
                    in_size as f64 / 1_048_576.0
                );
                println!(
                    "  Output: {} bytes ({:.2} MB)",
                    out_size,
                    out_size as f64 / 1_048_576.0
                );
                println!("  Ratio:  {:.1}%", ratio * 100.0);
            }

            println!();
            println!("Usage: owl2-reasoner check {}", output_path);
        }
        Err(e) => {
            eprintln!("Error during conversion: {}", e);
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn cmd_check(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        return ExitCode::FAILURE;
        println!("Usage: owl2-reasoner check <ontology_file>");
        return ExitCode::FAILURE;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    println!("\nChecking consistency...");
    flush_stdout();
    let start = Instant::now();
    phase_log("reason_start", "reasoner=SpeculativeTableauxReasoner");

    let config = match speculative_config_from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error: {}", err);
            return ExitCode::FAILURE;
        }
    };
    let mut reasoner = SpeculativeTableauxReasoner::with_config_arc(ontology, config);
    match reasoner.is_consistent() {
        Ok(consistent) => {
            emit_spacl_stats(&reasoner);
            let check_time = start.elapsed();
            println!("✓ Consistency check complete in {:?}", check_time);
            flush_stdout();
            phase_log(
                "reason_done",
                &format!(
                    "ms={} result={}",
                    check_time.as_millis(),
                    if consistent {
                        "consistent"
                    } else {
                        "inconsistent"
                    }
                ),
            );
            println!();
            if consistent {
                println!("Result: ✅ CONSISTENT");
            } else {
                println!("Result: ❌ INCONSISTENT");
            }
        }
        Err(e) => {
            emit_spacl_stats(&reasoner);
            eprintln!("Error during reasoning: {:?}", e);
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn cmd_stats(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        return ExitCode::FAILURE;
        println!("Usage: owl2-reasoner stats <ontology_file>");
        return ExitCode::FAILURE;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let analysis = OntologyCharacteristics::analyze(&ontology);
    let profile = detect_profile(&ontology);
    let consistency_plan = select_consistency_reasoner(&ontology, profile);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                  ONTOLOGY STATISTICS                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Basic Counts:");
    println!("  Classes:             {}", ontology.classes().len());
    println!(
        "  Object Properties:   {}",
        ontology.object_properties().len()
    );
    println!(
        "  Data Properties:     {}",
        ontology.data_properties().len()
    );
    println!(
        "  Named Individuals:   {}",
        ontology.named_individuals().len()
    );
    println!("  Total Axioms:        {}", ontology.axioms().len());
    println!();

    println!("Analysis:");
    println!("  Disjunctions:        {}", analysis.disjunction_count);
    println!(
        "  Tree-like hierarchy: {}",
        if analysis.is_tree_like { "Yes" } else { "No" }
    );
    println!("  Complexity score:    {:.3}", analysis.complexity_score);
    println!(
        "  Detected profile:    {}",
        profile.map_or("OWL2 DL".to_string(), |p| p.name().to_string())
    );
    println!();

    println!(
        "Recommended consistency reasoner: {} ({})",
        consistency_plan.reasoner.as_str(),
        consistency_plan.rationale
    );
    ExitCode::SUCCESS
}

fn cmd_compare(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        return ExitCode::FAILURE;
        println!("Usage: owl2-reasoner compare <ontology_file>");
        return ExitCode::FAILURE;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              REASONER PERFORMANCE COMPARISON                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Shared ontology for both reasoners (no deep clone)
    let ontology = Arc::clone(&ontology);

    // SimpleReasoner
    println!("Running SimpleReasoner...");
    let start = Instant::now();
    let simple_reasoner = SimpleReasoner::from_arc(Arc::clone(&ontology));
    let simple_result = simple_reasoner.is_consistent();
    let simple_time = start.elapsed();
    println!("  Result: {:?} in {:?}", simple_result, simple_time);
    println!();

    // SpeculativeTableauxReasoner
    println!("Running SpeculativeTableauxReasoner (SPACL)...");
    let start = Instant::now();
    let config = match speculative_config_from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error: {}", err);
            return ExitCode::FAILURE;
        }
    };
    let mut speculative_reasoner =
        SpeculativeTableauxReasoner::with_config_arc(Arc::clone(&ontology), config);
    let speculative_result = speculative_reasoner.is_consistent();
    emit_spacl_stats(&speculative_reasoner);
    let speculative_time = start.elapsed();
    println!(
        "  Result: {:?} in {:?}",
        speculative_result, speculative_time
    );
    println!();

    // Summary
    println!("Summary:");
    println!("  SimpleReasoner:    {:?}", simple_time);
    println!("  SPACL:             {:?}", speculative_time);

    let speedup = simple_time.as_secs_f64() / speculative_time.as_secs_f64();
    if speedup > 1.0 {
        println!("  Speedup:           {:.2}x faster (SPACL)", speedup);
    } else {
        println!("  Speedup:           {:.2}x faster (Simple)", 1.0 / speedup);
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let command = &args[1];
    let command_args = &args[2..];

    match command.as_str() {
        "check" => return cmd_check(command_args),
        "check-auto" => return ExitCode::from(cmd_check_auto(command_args) as u8),
        "convert" => return cmd_convert(command_args),
        "stats" => return cmd_stats(command_args),
        "compare" => return cmd_compare(command_args),
        "evaluate" => return cmd_evaluate(command_args),
        "materialize" => return cmd_materialize(command_args),
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("Error: Unknown command '{}'", command);
            print_usage();
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn cmd_materialize(args: &[String]) -> ExitCode {
    // materialize <input> <output> [--profile RL|QL|EL|DL]
    let positional: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    let mut profile = String::from("RL");
    let mut index = positional.len();
    while index < args.len() {
        if args[index] == "--profile" {
            match args.get(index + 1) {
                Some(value) if ["RL", "QL", "EL", "DL"].contains(&value.as_str()) => {
                    profile = value.clone();
                    index += 2;
                    continue;
                }
                _ => {
                    eprintln!("Error: --profile expects RL, QL, EL, or DL");
                    return ExitCode::FAILURE;
                    println!("Usage: owl2-reasoner materialize <input> <output> [--profile RL|QL|EL|DL]");
                    return ExitCode::FAILURE;
                }
            }
        }
        eprintln!("Error: unrecognized materialize option {}", args[index]);
        return ExitCode::FAILURE;
        println!("Usage: owl2-reasoner materialize <input> <output> [--profile RL|QL|EL|DL]");
        return ExitCode::FAILURE;
    }
    if positional.len() != 2 {
        eprintln!("Error: materialize expects <input> <output>");
        return ExitCode::FAILURE;
        println!("Usage: owl2-reasoner materialize <input> <output> [--profile RL|QL|EL|DL]");
        return ExitCode::FAILURE;
    }
    let input_path = positional[0];
    let output_path = positional[1];

    let parse_started = Instant::now();
    let ontology = match load_ontology(input_path) {
        Ok(ontology) => ontology,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let parse_time_ms = parse_started.elapsed().as_millis();

    let materialize_started = Instant::now();
    let (closure, stats) = owl2_reasoner::reasoner::materialize::materialize_for_profile(
        &ontology, &profile,
    );
    let text = owl2_reasoner::reasoner::materialize::write_ntriples(&closure);
    let materialize_time_ms = materialize_started.elapsed().as_millis();

    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    };

    if let Err(e) = std::fs::write(output_path, &text) {
        eprintln!("Error writing output file: {}", e);
        return ExitCode::FAILURE;
    }

    println!(
        "{}",
        serde_json::json!({
            "command": "materialize",
            "profile": profile,
            "base_triples": stats.base_triples,
            "derived_triples": stats.derived_triples,
            "closure_triples": stats.closure_triples,
            "out_of_fragment_axioms": stats.out_of_fragment_axioms,
            "closure_sha256": digest,
            "parse_time_ms": parse_time_ms,
            "materialize_time_ms": materialize_time_ms,
        })
    );
    ExitCode::SUCCESS
}
