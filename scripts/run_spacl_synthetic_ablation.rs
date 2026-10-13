use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use chrono::Local;
use owl2_reasoner::{
    BranchPolicyMode, Class, ClassExpression, DisjointClassesAxiom, Ontology, SchedulingMode,
    SpeculativeConfig, SpeculativeTableauxReasoner, SubClassOfAxiom,
};

#[derive(Clone)]
struct Workload {
    name: String,
    ontology: Ontology,
}

#[derive(Clone, Copy)]
struct ModeConfig {
    name: &'static str,
    scheduling_mode: SchedulingMode,
    nogood_enabled: bool,
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn branch_policy_from_env() -> BranchPolicyMode {
    match std::env::var("SPACL_BRANCH_POLICY") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "baseline" | "default" => BranchPolicyMode::Baseline,
            "heuristic" | "ranked" => BranchPolicyMode::Heuristic,
            "hybrid_rrn" | "hybrid-rrn" | "rrn" => BranchPolicyMode::HybridRrn,
            _ => BranchPolicyMode::Baseline,
        },
        Err(_) => BranchPolicyMode::Baseline,
    }
}

fn apply_env_overrides(config: &mut SpeculativeConfig) {
    if let Ok(value) = std::env::var("SPACL_SYNTH_PARALLEL_THRESHOLD") {
        if let Ok(parsed) = value.trim().parse::<usize>() {
            config.parallel_threshold = parsed;
        }
    }
    if let Ok(value) = std::env::var("SPACL_SYNTH_COST_THRESHOLD_US") {
        if let Ok(parsed) = value.trim().parse::<usize>() {
            config.cost_threshold_us = parsed;
        }
    }
    if let Ok(value) = std::env::var("SPACL_SYNTH_COST_PER_OPERAND_US") {
        if let Ok(parsed) = value.trim().parse::<usize>() {
            config.cost_per_operand_us = parsed;
        }
    }
    if let Ok(value) = std::env::var("SPACL_SYNTH_COST_PER_NESTING_US") {
        if let Ok(parsed) = value.trim().parse::<usize>() {
            config.cost_per_nesting_us = parsed;
        }
    }
    config.branch_policy = branch_policy_from_env();
    if let Ok(path) = std::env::var("SPACL_RRN_MODEL_PATH") {
        let path = path.trim();
        if !path.is_empty() {
            config.rrn_model_path = Some(path.to_string());
        }
    }
    if let Ok(path) = std::env::var("SPACL_BRANCH_SNAPSHOT_FILE") {
        let path = path.trim();
        if !path.is_empty() {
            config.branch_snapshot_path = Some(path.to_string());
        }
    }
}

fn env_list(key: &str) -> Option<Vec<String>> {
    std::env::var(key).ok().map(|value| {
        value
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect()
    })
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

fn create_inconsistent_disjunctive_ontology(size: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..size {
        let class = Class::new(format!("http://example.org/C{}", i));
        ontology.add_class(class).unwrap();
    }

    for i in (0..size).step_by(5) {
        if i + 1 < size {
            let c1 = Class::new(format!("http://example.org/C{}", i));
            let c2 = Class::new(format!("http://example.org/C{}", i + 1));

            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    c1.iri().clone(),
                    c2.iri().clone(),
                ]))
                .unwrap();

            let subclass = Class::new(format!("http://example.org/D{}", i));
            ontology.add_class(subclass.clone()).unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(subclass),
                    ClassExpression::ObjectUnionOf(
                        vec![
                            Box::new(ClassExpression::Class(c1)),
                            Box::new(ClassExpression::Class(c2)),
                        ]
                        .into(),
                    ),
                ))
                .unwrap();
        }
    }

    ontology
}

fn create_reused_conflict_ontology(num_unions: usize) -> Ontology {
    let mut ontology = Ontology::new();

    let left = Class::new("http://example.org/ConflictLeft");
    let right = Class::new("http://example.org/ConflictRight");
    ontology.add_class(left.clone()).unwrap();
    ontology.add_class(right.clone()).unwrap();
    ontology
        .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
            left.iri().clone(),
            right.iri().clone(),
        ]))
        .unwrap();

    for i in 0..num_unions {
        let subclass = Class::new(format!("http://example.org/R{}", i));
        let tag = Class::new(format!("http://example.org/Tag{}", i));
        ontology.add_class(subclass.clone()).unwrap();
        ontology.add_class(tag.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(subclass),
                ClassExpression::ObjectUnionOf(
                    vec![
                        Box::new(ClassExpression::Class(left.clone())),
                        Box::new(ClassExpression::Class(right.clone())),
                        Box::new(ClassExpression::Class(tag)),
                    ]
                    .into(),
                ),
            ))
            .unwrap();
    }

    ontology
}

fn create_mixed_operand_ontology(num_unions: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..num_unions {
        let head = Class::new(format!("http://example.org/M{}", i));
        ontology.add_class(head.clone()).unwrap();

        let atomic = Class::new(format!("http://example.org/M{}_A", i));
        let b = Class::new(format!("http://example.org/M{}_B", i));
        let c = Class::new(format!("http://example.org/M{}_C", i));
        let d = Class::new(format!("http://example.org/M{}_D", i));
        let e = Class::new(format!("http://example.org/M{}_E", i));
        let f = Class::new(format!("http://example.org/M{}_F", i));
        let g = Class::new(format!("http://example.org/M{}_G", i));
        let h = Class::new(format!("http://example.org/M{}_H", i));
        let j = Class::new(format!("http://example.org/M{}_J", i));
        let k = Class::new(format!("http://example.org/M{}_K", i));

        for class in [&atomic, &b, &c, &d, &e, &f, &g, &h, &j, &k] {
            ontology.add_class((*class).clone()).unwrap();
        }

        let op_atomic = ClassExpression::Class(atomic);
        let op_intersection = ClassExpression::ObjectIntersectionOf(
            vec![
                Box::new(ClassExpression::Class(b)),
                Box::new(ClassExpression::Class(c)),
            ]
            .into(),
        );
        let op_complement = ClassExpression::ObjectComplementOf(Box::new(ClassExpression::Class(d)));
        let op_union = ClassExpression::ObjectUnionOf(
            vec![
                Box::new(ClassExpression::Class(e)),
                Box::new(ClassExpression::Class(f)),
                Box::new(ClassExpression::ObjectComplementOf(Box::new(
                    ClassExpression::Class(g),
                ))),
            ]
            .into(),
        );
        let op_nested = ClassExpression::ObjectIntersectionOf(
            vec![
                Box::new(ClassExpression::ObjectUnionOf(
                    vec![
                        Box::new(ClassExpression::Class(h)),
                        Box::new(ClassExpression::Class(j)),
                    ]
                    .into(),
                )),
                Box::new(ClassExpression::Class(k)),
            ]
            .into(),
        );

        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(head),
                ClassExpression::ObjectUnionOf(
                    vec![
                        Box::new(op_atomic),
                        Box::new(op_intersection),
                        Box::new(op_complement),
                        Box::new(op_union),
                        Box::new(op_nested),
                    ]
                    .into(),
                ),
            ))
            .unwrap();
    }

    ontology
}

fn workloads() -> Vec<Workload> {
    let requested = env_list("SPACL_SYNTH_ABLATION_WORKLOADS");
    let selected = |name: &str| match &requested {
        Some(filter) => filter.iter().any(|requested_name| requested_name == name),
        None => true,
    };

    let mut workloads = Vec::new();

    if selected("union_2x10") {
        workloads.push(Workload {
            name: "union_2x10".to_string(),
            ontology: create_union_ontology(2, 10),
        });
    }
    if selected("union_4x6") {
        workloads.push(Workload {
            name: "union_4x6".to_string(),
            ontology: create_union_ontology(4, 6),
        });
    }
    if selected("union_4x10") {
        workloads.push(Workload {
            name: "union_4x10".to_string(),
            ontology: create_union_ontology(4, 10),
        });
    }
    if selected("union_6x6") {
        workloads.push(Workload {
            name: "union_6x6".to_string(),
            ontology: create_union_ontology(6, 6),
        });
    }
    if selected("union_7x6") {
        workloads.push(Workload {
            name: "union_7x6".to_string(),
            ontology: create_union_ontology(7, 6),
        });
    }
    if selected("union_6x10") {
        workloads.push(Workload {
            name: "union_6x10".to_string(),
            ontology: create_union_ontology(6, 10),
        });
    }
    if selected("union_8x6") {
        workloads.push(Workload {
            name: "union_8x6".to_string(),
            ontology: create_union_ontology(8, 6),
        });
    }
    if selected("union_8x10") {
        workloads.push(Workload {
            name: "union_8x10".to_string(),
            ontology: create_union_ontology(8, 10),
        });
    }
    if selected("union_10x6") {
        workloads.push(Workload {
            name: "union_10x6".to_string(),
            ontology: create_union_ontology(10, 6),
        });
    }
    if selected("union_10x10") {
        workloads.push(Workload {
            name: "union_10x10".to_string(),
            ontology: create_union_ontology(10, 10),
        });
    }
    if selected("union_12x10") {
        workloads.push(Workload {
            name: "union_12x10".to_string(),
            ontology: create_union_ontology(12, 10),
        });
    }
    if selected("union_16x10") {
        workloads.push(Workload {
            name: "union_16x10".to_string(),
            ontology: create_union_ontology(16, 10),
        });
    }
    if selected("union_32x10") {
        workloads.push(Workload {
            name: "union_32x10".to_string(),
            ontology: create_union_ontology(32, 10),
        });
    }
    if selected("union_64x10") {
        workloads.push(Workload {
            name: "union_64x10".to_string(),
            ontology: create_union_ontology(64, 10),
        });
    }
    if selected("union_2500x2") {
        workloads.push(Workload {
            name: "union_2500x2".to_string(),
            ontology: create_union_ontology(2500, 2),
        });
    }
    if selected("inconsistent_100") {
        workloads.push(Workload {
            name: "inconsistent_100".to_string(),
            ontology: create_inconsistent_disjunctive_ontology(100),
        });
    }
    if selected("inconsistent_500") {
        workloads.push(Workload {
            name: "inconsistent_500".to_string(),
            ontology: create_inconsistent_disjunctive_ontology(500),
        });
    }
    if selected("inconsistent_10000") {
        workloads.push(Workload {
            name: "inconsistent_10000".to_string(),
            ontology: create_inconsistent_disjunctive_ontology(10_000),
        });
    }
    if selected("reused_conflict_8") {
        workloads.push(Workload {
            name: "reused_conflict_8".to_string(),
            ontology: create_reused_conflict_ontology(8),
        });
    }
    if selected("reused_conflict_12") {
        workloads.push(Workload {
            name: "reused_conflict_12".to_string(),
            ontology: create_reused_conflict_ontology(12),
        });
    }
    if selected("reused_conflict_16") {
        workloads.push(Workload {
            name: "reused_conflict_16".to_string(),
            ontology: create_reused_conflict_ontology(16),
        });
    }
    if selected("mixed_operands_16") {
        workloads.push(Workload {
            name: "mixed_operands_16".to_string(),
            ontology: create_mixed_operand_ontology(16),
        });
    }
    if selected("mixed_operands_8") {
        workloads.push(Workload {
            name: "mixed_operands_8".to_string(),
            ontology: create_mixed_operand_ontology(8),
        });
    }
    if selected("mixed_operands_32") {
        workloads.push(Workload {
            name: "mixed_operands_32".to_string(),
            ontology: create_mixed_operand_ontology(32),
        });
    }

    workloads
}

fn modes() -> Vec<ModeConfig> {
    let mut modes = vec![
        ModeConfig {
            name: "sequential",
            scheduling_mode: SchedulingMode::Sequential,
            nogood_enabled: false,
        },
        ModeConfig {
            name: "adaptive_no_nogood",
            scheduling_mode: SchedulingMode::Adaptive,
            nogood_enabled: false,
        },
        ModeConfig {
            name: "adaptive",
            scheduling_mode: SchedulingMode::Adaptive,
            nogood_enabled: true,
        },
        ModeConfig {
            name: "always_parallel",
            scheduling_mode: SchedulingMode::AlwaysParallel,
            nogood_enabled: false,
        },
    ];

    if let Some(filter) = env_list("SPACL_SYNTH_ABLATION_MODES") {
        modes.retain(|mode| filter.iter().any(|name| name == mode.name));
    }

    modes
}

fn main() {
    let repeats = env_usize("SPACL_SYNTH_ABLATION_REPEATS", 3);
    let warmups = env_usize("SPACL_SYNTH_ABLATION_WARMUPS", 1);
    let run_id = format!(
        "spacl_synthetic_ablation_{}",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let output_dir = PathBuf::from("benchmarks/competitors/results/history").join(&run_id);
    fs::create_dir_all(&output_dir).unwrap();

    let mut csv = BufWriter::new(File::create(output_dir.join("results.csv")).unwrap());
    writeln!(
        csv,
        "workload,mode,nogood_enabled,branch_policy,repeat,wall_time_ms,reasoning_time_ms,used_parallel,disjunctions_detected,estimated_branch_count,branches_created,work_items_expanded,branches_pruned,nogood_hits,local_cache_hits,global_cache_hits,steal_attempts,steal_successes,policy_reordered_splits,policy_fallbacks,hybrid_policy_calls,hybrid_model_calls,branch_snapshots_written"
    )
    .unwrap();
    csv.flush().unwrap();

    let mut readme = BufWriter::new(File::create(output_dir.join("README.md")).unwrap());
    writeln!(readme, "# SPACL Synthetic Scheduler Ablation").unwrap();
    writeln!(readme, "- Repeats: `{}`", repeats).unwrap();
    writeln!(readme, "- Warmups: `{}`", warmups).unwrap();
    writeln!(readme, "- Workloads: `{}`", workloads().len()).unwrap();
    writeln!(readme, "- Modes: `{}`", modes().len()).unwrap();
    writeln!(readme).unwrap();
    writeln!(
        readme,
        "This artifact isolates scheduling and nogood policy on in-memory synthetic workloads."
    )
    .unwrap();
    readme.flush().unwrap();

    for workload in workloads() {
        for mode in modes() {
            for _ in 0..warmups {
                let mut config = SpeculativeConfig::default();
                config.scheduling_mode = mode.scheduling_mode;
                config.enable_learning = mode.nogood_enabled;
                apply_env_overrides(&mut config);
                let mut reasoner =
                    SpeculativeTableauxReasoner::with_config(workload.ontology.clone(), config);
                let _ = reasoner.is_consistent();
            }

            for repeat in 1..=repeats {
                let mut config = SpeculativeConfig::default();
                config.scheduling_mode = mode.scheduling_mode;
                config.enable_learning = mode.nogood_enabled;
                apply_env_overrides(&mut config);

                let start = Instant::now();
                let mut reasoner =
                    SpeculativeTableauxReasoner::with_config(workload.ontology.clone(), config);
                let _ = reasoner.is_consistent();
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                let stats = reasoner.get_stats();

                writeln!(
                    csv,
                    "{},{},{},{},{},{:.3},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    workload.name,
                    mode.name,
                    mode.nogood_enabled,
                    stats.branch_policy,
                    repeat,
                    wall_ms,
                    stats.reasoning_time_ms,
                    stats.used_parallel,
                    stats.disjunctions_detected,
                    stats.estimated_branch_count,
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
                )
                .unwrap();
                csv.flush().unwrap();
            }
        }
    }

    csv.flush().unwrap();
    readme.flush().unwrap();
    println!("{}", output_dir.display());
}
