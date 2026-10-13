use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::Local;
use owl2_reasoner::util::ontology_io::load_ontology_with_env;
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig, SpeculativeStats,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};

#[derive(Clone, Debug)]
struct CalibrationConfig {
    parallel_threshold: usize,
    cost_threshold_us: usize,
    cost_per_operand_us: usize,
    cost_per_nesting_us: usize,
}

#[derive(Clone)]
struct CalibrationCase {
    name: String,
    ontology: Arc<Ontology>,
}

#[derive(Clone, Debug)]
struct RunResult {
    median_ms: f64,
    stats: SpeculativeStats,
}

#[derive(Clone, Debug)]
struct StageASummary {
    config: CalibrationConfig,
    average_regret: f64,
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn env_truthy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            !(value.is_empty() || value == "0" || value == "false" || value == "no")
        }
        Err(_) => false,
    }
}

fn env_usize_list(key: &str, default: &[usize]) -> Vec<usize> {
    match std::env::var(key) {
        Ok(value) => {
            let parsed: Vec<usize> = value
                .split(',')
                .filter_map(|part| part.trim().parse::<usize>().ok())
                .filter(|v| *v > 0)
                .collect();
            if parsed.is_empty() {
                default.to_vec()
            } else {
                parsed
            }
        }
        Err(_) => default.to_vec(),
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn create_union_ontology(num_unions: usize, operands_per_union: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..num_unions {
        let subclass = Class::new(format!("http://example.org/U{}", i));
        ontology.add_class(subclass.clone()).unwrap();

        let union_operands: Vec<_> = (0..operands_per_union)
            .map(|j| {
                let op_class = Class::new(format!("http://example.org/U{}O{}", i, j));
                ontology.add_class(op_class.clone()).unwrap();
                Box::new(ClassExpression::Class(op_class))
            })
            .collect();

        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(subclass),
                ClassExpression::ObjectUnionOf(union_operands.into_iter().collect()),
            ))
            .unwrap();
    }

    ontology
}

fn create_complement_union_ontology(num_unions: usize, operands_per_union: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..num_unions {
        let subclass = Class::new(format!("http://example.org/C{}", i));
        ontology.add_class(subclass.clone()).unwrap();

        let union_operands: Vec<_> = (0..operands_per_union)
            .map(|j| {
                let op_class = Class::new(format!("http://example.org/C{}N{}", i, j));
                ontology.add_class(op_class.clone()).unwrap();
                Box::new(ClassExpression::ObjectComplementOf(Box::new(
                    ClassExpression::Class(op_class),
                )))
            })
            .collect();

        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(subclass),
                ClassExpression::ObjectUnionOf(union_operands.into_iter().collect()),
            ))
            .unwrap();
    }

    ontology
}

fn load_case(path: &str, name: &str) -> Option<CalibrationCase> {
    let ontology = load_ontology_with_env(PathBuf::from(path).as_path()).ok()?;
    Some(CalibrationCase {
        name: name.to_string(),
        ontology: Arc::new(ontology),
    })
}

fn run_simple(ontology: &Arc<Ontology>, repeats: usize) -> f64 {
    let mut durations = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        let reasoner = SimpleReasoner::from_arc(Arc::clone(ontology));
        let _ = reasoner.is_consistent();
        durations.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    median(&mut durations)
}

fn run_speculative(
    ontology: &Arc<Ontology>,
    config: &CalibrationConfig,
    repeats: usize,
) -> RunResult {
    let mut durations = Vec::with_capacity(repeats);
    let mut final_stats = SpeculativeStats::default();

    for _ in 0..repeats {
        let mut reasoner_config = SpeculativeConfig::default();
        reasoner_config.parallel_threshold = config.parallel_threshold;
        reasoner_config.cost_threshold_us = config.cost_threshold_us;
        reasoner_config.cost_per_operand_us = config.cost_per_operand_us;
        reasoner_config.cost_per_nesting_us = config.cost_per_nesting_us;

        let start = Instant::now();
        let mut reasoner = SpeculativeTableauxReasoner::with_config_arc(
            Arc::clone(ontology),
            reasoner_config,
        );
        let _ = reasoner.is_consistent();
        durations.push(start.elapsed().as_secs_f64() * 1000.0);
        final_stats = reasoner.get_stats();
    }

    RunResult {
        median_ms: median(&mut durations),
        stats: final_stats,
    }
}

fn run_forced_parallel(ontology: &Arc<Ontology>, repeats: usize) -> f64 {
    let forced = CalibrationConfig {
        parallel_threshold: 1,
        cost_threshold_us: 1,
        cost_per_operand_us: 50,
        cost_per_nesting_us: 30,
    };
    let mut durations = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let mut reasoner_config = SpeculativeConfig::default();
        reasoner_config.parallel_threshold = forced.parallel_threshold;
        reasoner_config.cost_threshold_us = forced.cost_threshold_us;
        reasoner_config.cost_per_operand_us = forced.cost_per_operand_us;
        reasoner_config.cost_per_nesting_us = forced.cost_per_nesting_us;
        reasoner_config.adaptive_tuning = false;

        let start = Instant::now();
        let mut reasoner = SpeculativeTableauxReasoner::with_config_arc(
            Arc::clone(ontology),
            reasoner_config,
        );
        let _ = reasoner.is_consistent();
        durations.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    median(&mut durations)
}

fn stage_a_cases() -> Vec<CalibrationCase> {
    let mut cases = vec![
        CalibrationCase {
            name: "synthetic_union_1x4".to_string(),
            ontology: Arc::new(create_union_ontology(1, 4)),
        },
        CalibrationCase {
            name: "synthetic_union_2x8".to_string(),
            ontology: Arc::new(create_union_ontology(2, 8)),
        },
        CalibrationCase {
            name: "synthetic_complement_4x4".to_string(),
            ontology: Arc::new(create_complement_union_ontology(4, 4)),
        },
        CalibrationCase {
            name: "synthetic_union_6x8".to_string(),
            ontology: Arc::new(create_union_ontology(6, 8)),
        },
        CalibrationCase {
            name: "synthetic_complement_6x6".to_string(),
            ontology: Arc::new(create_complement_union_ontology(6, 6)),
        },
    ];

    if let Some(case) = load_case("tests/data/hierarchy_100.owl", "hierarchy_100") {
        cases.push(case);
    }
    if let Some(case) = load_case("tests/data/univ-bench.owl", "univ_bench") {
        cases.push(case);
    }
    if env_truthy("SPACL_CALIBRATION_INCLUDE_DISJUNCTIVE_5K") {
        if let Some(case) = load_case(
            "benchmarks/ontologies/disjunctive/disjunctive_5k.ofn",
            "disjunctive_5k",
        ) {
            cases.push(case);
        }
    }
    if env_truthy("SPACL_CALIBRATION_INCLUDE_DISJUNCTIVE_10K") {
        if let Some(case) = load_case(
            "benchmarks/ontologies/disjunctive/disjunctive_10k.ofn",
            "disjunctive_10k",
        ) {
            cases.push(case);
        }
    }

    cases
}

fn stage_b_cases() -> Vec<CalibrationCase> {
    let mut cases = Vec::new();
    if let Some(case) = load_case("benchmarks/ontologies/other/doid.owl", "doid") {
        cases.push(case);
    }
    if let Some(case) = load_case("benchmarks/ontologies/other/go-basic.owl", "go_basic") {
        cases.push(case);
    }
    if env_truthy("SPACL_CALIBRATION_INCLUDE_UBERON") {
        if let Some(case) = load_case("benchmarks/ontologies/other/uberon.owl", "uberon") {
            cases.push(case);
        }
    }
    cases
}

fn candidate_configs() -> Vec<CalibrationConfig> {
    let mut configs = Vec::new();
    let parallel_thresholds = env_usize_list("SPACL_CALIBRATION_PARALLEL_THRESHOLDS", &[10, 50, 100]);
    let cost_thresholds = env_usize_list("SPACL_CALIBRATION_COST_THRESHOLDS", &[1, 5, 25]);
    let operand_costs = env_usize_list("SPACL_CALIBRATION_OPERAND_COSTS", &[25, 50]);
    let nesting_costs = env_usize_list("SPACL_CALIBRATION_NESTING_COSTS", &[15, 30]);

    for parallel_threshold in parallel_thresholds {
        for cost_threshold_us in &cost_thresholds {
            for cost_per_operand_us in &operand_costs {
                for cost_per_nesting_us in &nesting_costs {
                    configs.push(CalibrationConfig {
                        parallel_threshold,
                        cost_threshold_us: *cost_threshold_us,
                        cost_per_operand_us: *cost_per_operand_us,
                        cost_per_nesting_us: *cost_per_nesting_us,
                    });
                }
            }
        }
    }
    configs
}

fn write_header<W: Write>(writer: &mut W, header: &str) {
    writeln!(writer, "{header}").unwrap();
}

fn main() {
    std::env::set_var("OWL2_REASONER_MAX_FILE_SIZE", "0");
    std::env::set_var("OWL2_REASONER_LARGE_PARSE", "1");

    let stage_a_repeats = env_usize("SPACL_CALIBRATION_STAGE_A_REPEATS", 3);
    let stage_b_repeats = env_usize("SPACL_CALIBRATION_STAGE_B_REPEATS", 1);
    let top_k = env_usize("SPACL_CALIBRATION_TOP_K", 5);

    let run_id = format!(
        "adaptive_calibration_{}",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let output_dir = PathBuf::from("results/history").join(&run_id);
    fs::create_dir_all(&output_dir).unwrap();

    let stage_a_cases = stage_a_cases();
    let stage_b_cases = stage_b_cases();
    let configs = candidate_configs();

    let mut stage_a_writer = BufWriter::new(File::create(output_dir.join("stage_a.csv")).unwrap());
    let mut summary_writer = BufWriter::new(File::create(output_dir.join("summary.csv")).unwrap());
    let mut stage_b_writer = BufWriter::new(File::create(output_dir.join("stage_b.csv")).unwrap());

    write_header(
        &mut stage_a_writer,
        "case,parallel_threshold,cost_threshold_us,cost_per_operand_us,cost_per_nesting_us,sequential_ms,forced_parallel_ms,adaptive_ms,best_baseline_ms,regret,used_parallel,disjunctions_detected,estimated_branch_count,estimated_cost_us,branch_threshold,cost_gate_threshold_us,branch_gate_passed,cost_gate_passed,branches_created,branches_pruned,nogood_hits",
    );
    write_header(
        &mut summary_writer,
        "parallel_threshold,cost_threshold_us,cost_per_operand_us,cost_per_nesting_us,average_regret",
    );
    write_header(
        &mut stage_b_writer,
        "case,parallel_threshold,cost_threshold_us,cost_per_operand_us,cost_per_nesting_us,sequential_ms,forced_parallel_ms,adaptive_ms,best_baseline_ms,regret,used_parallel,disjunctions_detected,estimated_branch_count,estimated_cost_us,branch_threshold,cost_gate_threshold_us,branch_gate_passed,cost_gate_passed,branches_created,branches_pruned,nogood_hits",
    );

    println!("Adaptive calibration run: {}", run_id);
    println!("Stage A cases: {}", stage_a_cases.len());
    println!("Stage B cases: {}", stage_b_cases.len());
    println!("Candidate configs: {}", configs.len());

    let stage_a_baselines: Vec<(f64, f64)> = stage_a_cases
        .iter()
        .map(|case| {
            (
                run_simple(&case.ontology, stage_a_repeats),
                run_forced_parallel(&case.ontology, stage_a_repeats),
            )
        })
        .collect();
    let stage_b_baselines: Vec<(f64, f64)> = stage_b_cases
        .iter()
        .map(|case| {
            (
                run_simple(&case.ontology, stage_b_repeats),
                run_forced_parallel(&case.ontology, stage_b_repeats),
            )
        })
        .collect();

    let mut stage_a_summaries = Vec::new();

    for config in &configs {
        let mut regrets = Vec::new();
        for (idx, case) in stage_a_cases.iter().enumerate() {
            let (sequential_ms, forced_parallel_ms) = stage_a_baselines[idx];
            let adaptive = run_speculative(&case.ontology, config, stage_a_repeats);
            let best_baseline_ms = sequential_ms.min(forced_parallel_ms);
            let regret = if best_baseline_ms > 0.0 {
                adaptive.median_ms / best_baseline_ms
            } else {
                1.0
            };
            regrets.push(regret);

            writeln!(
                stage_a_writer,
                "{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.6},{},{},{},{},{},{},{},{},{},{},{}",
                case.name,
                config.parallel_threshold,
                config.cost_threshold_us,
                config.cost_per_operand_us,
                config.cost_per_nesting_us,
                sequential_ms,
                forced_parallel_ms,
                adaptive.median_ms,
                best_baseline_ms,
                regret,
                adaptive.stats.used_parallel,
                adaptive.stats.disjunctions_detected,
                adaptive.stats.estimated_branch_count,
                adaptive.stats.estimated_cost_us,
                adaptive.stats.branch_threshold,
                adaptive.stats.cost_gate_threshold_us,
                adaptive.stats.branch_gate_passed,
                adaptive.stats.cost_gate_passed,
                adaptive.stats.branches_created,
                adaptive.stats.branches_pruned,
                adaptive.stats.nogood_hits,
            )
            .unwrap();
        }
        stage_a_writer.flush().unwrap();

        let average_regret = regrets.iter().sum::<f64>() / regrets.len() as f64;
        stage_a_summaries.push(StageASummary {
            config: config.clone(),
            average_regret,
        });
    }

    stage_a_summaries.sort_by(|a, b| {
        a.average_regret
            .partial_cmp(&b.average_regret)
            .unwrap_or(Ordering::Equal)
    });

    for summary in &stage_a_summaries {
        writeln!(
            summary_writer,
            "{},{},{},{},{:.6}",
            summary.config.parallel_threshold,
            summary.config.cost_threshold_us,
            summary.config.cost_per_operand_us,
            summary.config.cost_per_nesting_us,
            summary.average_regret,
        )
        .unwrap();
    }
    summary_writer.flush().unwrap();

    println!("Top stage-A configs:");
    for summary in stage_a_summaries.iter().take(top_k) {
        println!(
            "  pt={} ct={} op={} nest={} avg_regret={:.4}",
            summary.config.parallel_threshold,
            summary.config.cost_threshold_us,
            summary.config.cost_per_operand_us,
            summary.config.cost_per_nesting_us,
            summary.average_regret
        );
    }

    for summary in stage_a_summaries.iter().take(top_k) {
        for (idx, case) in stage_b_cases.iter().enumerate() {
            let (sequential_ms, forced_parallel_ms) = stage_b_baselines[idx];
            let adaptive = run_speculative(&case.ontology, &summary.config, stage_b_repeats);
            let best_baseline_ms = sequential_ms.min(forced_parallel_ms);
            let regret = if best_baseline_ms > 0.0 {
                adaptive.median_ms / best_baseline_ms
            } else {
                1.0
            };

            writeln!(
                stage_b_writer,
                "{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.6},{},{},{},{},{},{},{},{},{},{},{}",
                case.name,
                summary.config.parallel_threshold,
                summary.config.cost_threshold_us,
                summary.config.cost_per_operand_us,
                summary.config.cost_per_nesting_us,
                sequential_ms,
                forced_parallel_ms,
                adaptive.median_ms,
                best_baseline_ms,
                regret,
                adaptive.stats.used_parallel,
                adaptive.stats.disjunctions_detected,
                adaptive.stats.estimated_branch_count,
                adaptive.stats.estimated_cost_us,
                adaptive.stats.branch_threshold,
                adaptive.stats.cost_gate_threshold_us,
                adaptive.stats.branch_gate_passed,
                adaptive.stats.cost_gate_passed,
                adaptive.stats.branches_created,
                adaptive.stats.branches_pruned,
                adaptive.stats.nogood_hits,
            )
            .unwrap();
        }
        stage_b_writer.flush().unwrap();
    }

    println!("Wrote calibration artifacts to {}", output_dir.display());
}
