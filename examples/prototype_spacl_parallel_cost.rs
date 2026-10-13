//! Diagnostic overhead cases, not evidence of useful parallel search.
use owl2_reasoner::{
    Class, ClassAssertionAxiom, ClassExpression, DisjointClassesAxiom, NamedIndividual, Ontology,
    SchedulingMode, SpeculativeConfig, SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;

fn ontology(conflict: bool, unions: usize) -> Ontology {
    let mut o = Ontology::new();
    for i in 0..unions {
        let trigger = Class::new(format!("http://example.org/cost/Trigger{i}"));
        let a = Class::new(format!("http://example.org/cost/A{i}"));
        let b = Class::new(format!("http://example.org/cost/B{i}"));
        for c in [&trigger, &a, &b] {
            o.add_class(c.clone()).unwrap();
        }
        o.add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(trigger),
            ClassExpression::ObjectUnionOf(
                vec![
                    Box::new(ClassExpression::Class(a)),
                    Box::new(ClassExpression::Class(b)),
                ]
                .into(),
            ),
        ))
        .unwrap();
    }
    if conflict {
        let a = Class::new("http://example.org/cost/ClashA");
        let b = Class::new("http://example.org/cost/ClashB");
        let x = NamedIndividual::new("http://example.org/cost/x");
        o.add_class(a.clone()).unwrap();
        o.add_class(b.clone()).unwrap();
        o.add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
            a.iri().clone(),
            b.iri().clone(),
        ]))
        .unwrap();
        o.add_named_individual(x.clone()).unwrap();
        for expr in [ClassExpression::Class(a.clone()), ClassExpression::Class(b)] {
            o.add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr))
                .unwrap();
        }
    }
    o
}

fn option(args: &[String], key: &str, default: &str) -> String {
    args.windows(2)
        .find(|p| p[0] == key)
        .map(|p| p[1].clone())
        .unwrap_or(default.into())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = option(&args, "--mode", "sequential");
    let case = option(&args, "--case", "sat");
    let learning = option(&args, "--learning", "1") == "1";
    let warm = args.iter().any(|a| a == "--warm");
    let samples: usize = option(&args, "--samples", "1").parse().unwrap();
    let workers: usize = option(&args, "--workers", "2").parse().unwrap();
    let unions: usize = option(&args, "--unions", "6").parse().unwrap();
    let depth: usize = option(&args, "--depth", "10").parse().unwrap();
    let timeout_ms: u64 = option(&args, "--timeout-ms", "5000").parse().unwrap();
    assert!(unions <= 24 && depth <= 24);
    assert!((1..=16).contains(&samples) && (1..=8).contains(&workers));
    assert!(
        warm || samples == 1,
        "cold samples require separate processes"
    );
    assert!(case == "sat" || case == "unsat");
    let scheduling_mode = match mode.as_str() {
        "sequential" => SchedulingMode::Sequential,
        "parallel" => SchedulingMode::AlwaysParallel,
        _ => panic!("invalid mode"),
    };
    let base = Arc::new(ontology(case == "unsat", unions));
    let mut rows = Vec::<Value>::new();
    let mut lifecycle_verified = !warm;
    for index in 0..samples + usize::from(warm) {
        let clone_start = Instant::now();
        let clone = (*base).clone();
        let clone_ns = clone_start.elapsed().as_nanos();
        drop(clone);
        let mut config = SpeculativeConfig::default();
        config.scheduling_mode = scheduling_mode;
        config.num_workers = workers;
        config.enable_learning = learning;
        config.speculative_timeout_ms = timeout_ms;
        config.max_speculative_depth = depth;
        let setup = Instant::now();
        let mut reasoner = SpeculativeTableauxReasoner::with_config_arc(Arc::clone(&base), config);
        let setup_ns = setup.elapsed().as_nanos();
        let start = Instant::now();
        let verdict = reasoner.is_consistent();
        let reason_ns = start.elapsed().as_nanos();
        let stats = reasoner.get_stats();
        let report = stats.report();
        let complete_line = format!("Workers: {workers} started, {workers} completed");
        if warm && index == 0 {
            if verdict
                .as_ref()
                .map(|v| *v != (case == "sat"))
                .unwrap_or(true)
            {
                println!(
                    "{}",
                    json!({"status":"blocked_semantics","case":case,"mode":mode})
                );
                return;
            }
            lifecycle_verified = !stats.used_parallel || report.contains(&complete_line);
            if !lifecycle_verified {
                println!(
                    "{}",
                    json!({"status":"blocked_lifecycle","case":case,"mode":mode})
                );
                return;
            }
            continue;
        }
        let expected = case == "sat";
        let matches = verdict.as_ref().map(|v| *v == expected).unwrap_or(false);
        rows.push(json!({
            "case":case,"mode":mode,"learning":learning,"warm":warm,"sample":rows.len(),
            "expected":expected,"verdict":verdict.as_ref().ok(),"matches_oracle":matches,
            "error":verdict.as_ref().err().map(ToString::to_string),
            "unions":unions,"depth":depth,"timeout_ms":timeout_ms,
            "reasoning_ns":reason_ns,"constructor_ns":setup_ns,"ontology_clone_ns":clone_ns,
            "used_parallel":stats.used_parallel,"branches_created":stats.branches_created,
            "work_items_expanded":stats.work_items_expanded,"nogoods_learned":stats.nogoods_learned,
            "nogood_database_size":reasoner.get_nogood_stats().0,"report":report
        }));
        if !matches {
            break;
        }
    }
    println!(
        "{}",
        json!({"status":"complete","warm_worker_completion_verified":warm && lifecycle_verified,"rows":rows})
    );
}
