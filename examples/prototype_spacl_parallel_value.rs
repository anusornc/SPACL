//! PROTOTYPE ONLY: diagnostic CLI for bounded SPACL parallel-value experiments.
//! Not production scheduler, reasoner, or benchmark harness code.

pub use owl2_reasoner::{core, logic};

mod qualified_adapter {
    include!("../src/reasoner/qualified_consistency.rs");

    #[derive(Debug)]
    pub struct PreparedProbe {
        facts: QualifiedFacts,
        targets: Vec<String>,
        order: Vec<usize>,
    }

    impl PreparedProbe {
        pub fn targets_len(&self) -> usize {
            self.targets.len()
        }

        pub fn order_vertex_ids(&self) -> Vec<usize> {
            self.order
                .iter()
                .filter_map(|index| vertex_id(&self.targets[*index]))
                .collect()
        }

        pub fn legal_prefixes(&self, color_count: usize, depth: usize) -> Vec<Vec<Option<usize>>> {
            let depth = depth.min(self.order.len());
            let mut prefixes = Vec::new();
            let mut colors = vec![None; self.targets.len()];
            self.enumerate_prefix(color_count, depth, 0, &mut colors, &mut prefixes);
            prefixes
        }

        fn enumerate_prefix(
            &self,
            color_count: usize,
            depth: usize,
            position: usize,
            colors: &mut [Option<usize>],
            prefixes: &mut Vec<Vec<Option<usize>>>,
        ) {
            if position == depth {
                prefixes.push(colors.to_vec());
                return;
            }
            let target_index = self.order[position];
            for color in 0..color_count {
                if self.can_assign(target_index, color, colors) {
                    colors[target_index] = Some(color);
                    self.enumerate_prefix(color_count, depth, position + 1, colors, prefixes);
                    colors[target_index] = None;
                }
            }
        }

        fn can_assign(&self, target_index: usize, color: usize, colors: &[Option<usize>]) -> bool {
            !colors.iter().enumerate().any(|(other_index, other_color)| {
                other_color.is_some_and(|other_color| {
                    other_color == color
                        && are_semantically_distinct(
                            &self.facts,
                            &self.targets[target_index],
                            &self.targets[other_index],
                        )
                })
            })
        }

        pub fn run_kernel_from_prefix(
            &self,
            color_count: usize,
            prefix: &[Option<usize>],
            start_position: usize,
        ) -> Result<(bool, usize, Option<Vec<usize>>), String> {
            let mut colors = prefix.to_vec();
            let mut steps = 0_usize;
            let result = color_distinctness_graph(
                &self.facts,
                &self.targets,
                &self.order,
                &mut colors,
                start_position,
                color_count,
                &mut steps,
            )
            .map_err(|error| error.to_string())?;
            let witness = if result {
                Some(
                    colors
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| {
                            "kernel returned SAT with incomplete color assignment".to_string()
                        })?,
                )
            } else {
                None
            };
            Ok((result, steps, witness))
        }
    }

    pub fn prepare(ontology: &Ontology) -> Result<PreparedProbe, String> {
        validate_expression_bounds(ontology).map_err(|error| error.to_string())?;
        reject_unsupported_axioms(ontology).map_err(|error| error.to_string())?;
        let facts = collect_facts(ontology).map_err(|error| error.to_string())?;
        let cardinality = facts
            .cardinalities
            .iter()
            .find(|cardinality| cardinality.maximum.is_some())
            .ok_or_else(|| "ontology contains no maximum cardinality assertion".to_string())?;
        let targets = qualifying_targets(&facts, cardinality);
        let mut order: Vec<usize> = (0..targets.len()).collect();
        order.sort_by_key(|index| {
            std::cmp::Reverse(
                targets
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != *index)
                    .filter(|(_, target)| {
                        are_semantically_distinct(&facts, &targets[*index], target)
                    })
                    .count(),
            )
        });
        Ok(PreparedProbe {
            facts,
            targets,
            order,
        })
    }

    pub fn qualified_certificate(ontology: &Ontology) -> Result<Option<bool>, String> {
        check_qualified_consistency(ontology).map_err(|error| error.to_string())
    }

    fn vertex_id(iri: &str) -> Option<usize> {
        iri.rsplit_once("/v")?.1.parse().ok()
    }
}

use owl2_reasoner::{
    Axiom, Class, ClassAssertionAxiom, ClassExpression, DifferentIndividualsAxiom, NamedIndividual,
    ObjectProperty, ObjectPropertyExpression, Ontology, PropertyAssertionAxiom, SchedulingMode,
    SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner,
};
use rayon::ThreadPoolBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

const BASE: &str = "http://tableauxx.org/prototype/spacl-parallel-value";
const PREFIX_DEPTH: usize = 2;
const MAX_STDIN_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
struct Request {
    case: CaseInput,
    mode: Mode,
    workers: usize,
    warmups: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CaseInput {
    id: String,
    vertices: usize,
    edges: Vec<[usize; 2]>,
    colors: usize,
    role: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Audit,
    SerialDirect,
    SerialFrontier,
    ParallelFrontier,
}

#[derive(Debug, Serialize)]
struct PublicObservation {
    reasoner: &'static str,
    scheduling_mode: Option<&'static str>,
    status: &'static str,
    result: Option<bool>,
    error: Option<String>,
    actual_workers_started: Option<usize>,
    actual_workers_completed: Option<usize>,
    qualified_certificate: Option<bool>,
    production_preflight: bool,
}

#[derive(Debug, Serialize, Clone)]
struct TaskRow {
    task_id: usize,
    prefix: Vec<Option<usize>>,
    result: bool,
    search_nodes: usize,
    witness: Option<Vec<usize>>,
    worker_id: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Measurement {
    mode: Mode,
    reasoning_ns: u128,
    tasks_started: usize,
    tasks_completed: usize,
    workers_used: Vec<usize>,
    workers_used_count: usize,
    search_nodes: usize,
    tasks: Vec<TaskRow>,
    witness: Option<Vec<usize>>,
    result: bool,
}

fn main() -> ExitCode {
    let response = match run_from_stdin() {
        Ok(value) => value,
        Err(error) => error_envelope("InputError", error),
    };
    println!("{response}");
    ExitCode::SUCCESS
}

fn run_from_stdin() -> Result<Value, String> {
    let mut input = String::new();
    io::stdin()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_string(&mut input)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    if input.len() as u64 > MAX_STDIN_BYTES {
        return Err("stdin JSON exceeds 64 KiB limit".to_string());
    }
    let request: Request =
        serde_json::from_str(&input).map_err(|error| format!("invalid request JSON: {error}"))?;
    validate_request(&request)?;

    let ontology = build_graph_ontology(&request.case)?;
    let warmups = request.warmups.unwrap_or(0);
    let mut warmup_rows = Vec::new();
    let pool = if request.mode == Mode::ParallelFrontier {
        Some(
            ThreadPoolBuilder::new()
                .num_threads(request.workers)
                .build()
                .map_err(|error| format!("failed to build rayon pool: {error}"))?,
        )
    } else {
        None
    };

    for warmup_index in 0..warmups {
        let prepared = qualified_adapter::prepare(&ontology)?;
        let warmup = measure(&prepared, request.mode, request.case.colors, pool.as_ref())?;
        warmup_rows.push(json!({
            "warmup": warmup_index,
            "result": warmup.result,
            "tasks_started": warmup.tasks_started,
            "tasks_completed": warmup.tasks_completed,
            "search_nodes": warmup.search_nodes
        }));
    }

    let prepared = qualified_adapter::prepare(&ontology)?;
    let order = prepared.order_vertex_ids();
    let measurement = measure(&prepared, request.mode, request.case.colors, pool.as_ref())?;
    let result_label = if measurement.result {
        "consistent"
    } else {
        "inconsistent"
    };
    let public_audit = if request.mode == Mode::Audit {
        Some(run_public_audit(&ontology, request.workers))
    } else {
        None
    };

    Ok(json!({
        "status": "success",
        "verdict": {
            "result": result_label,
            "error_class": Value::Null
        },
        "reasoning_ns": measurement.reasoning_ns,
        "mode": request.mode,
        "case_id": request.case.id,
        "vertices": request.case.vertices,
        "colors": request.case.colors,
        "order": order,
        "public_audit": public_audit,
        "tasks_started": measurement.tasks_started,
        "tasks_completed": measurement.tasks_completed,
        "workers_used": measurement.workers_used,
        "workers_used_count": measurement.workers_used_count,
        "search_nodes": measurement.search_nodes,
        "tasks": measurement.tasks,
        "witness": measurement.witness,
        "warmup_rows": warmup_rows
    }))
}

fn validate_request(request: &Request) -> Result<(), String> {
    if request.case.vertices == 0 || request.case.vertices > 13 {
        return Err("case.vertices must be in 1..=13".to_string());
    }
    if request.case.colors == 0 || request.case.colors > request.case.vertices {
        return Err("case.colors must be in 1..=vertices".to_string());
    }
    if request.workers != 2 {
        return Err("workers must be exactly 2 for this prototype".to_string());
    }
    if request.warmups.unwrap_or(0) > 1 {
        return Err("warmups must be 0 or 1".to_string());
    }
    if request.case.role.trim().is_empty() {
        return Err("case.role must be non-empty".to_string());
    }
    let mut seen = BTreeSet::new();
    for [left, right] in &request.case.edges {
        if left == right {
            return Err("self-loop edges are invalid".to_string());
        }
        if *left >= request.case.vertices || *right >= request.case.vertices {
            return Err("edge endpoint is outside vertices".to_string());
        }
        let edge = if left < right {
            (*left, *right)
        } else {
            (*right, *left)
        };
        if !seen.insert(edge) {
            return Err("duplicate undirected edges are invalid".to_string());
        }
    }
    Ok(())
}

fn build_graph_ontology(case: &CaseInput) -> Result<Ontology, String> {
    let mut ontology = Ontology::new();
    let target_class = Class::new(format!("{BASE}/{}", iri_segment(&case.id, "C")));
    let owner = NamedIndividual::new(format!("{BASE}/{}/owner", iri_segment(&case.id, "case")));
    let role = ObjectProperty::new(format!(
        "{BASE}/{}/{}",
        iri_segment(&case.id, "role"),
        case.role
    ));
    ontology
        .add_class(target_class.clone())
        .map_err(to_string)?;
    ontology
        .add_named_individual(owner.clone())
        .map_err(to_string)?;
    ontology
        .add_object_property(role.clone())
        .map_err(to_string)?;

    let mut vertices = Vec::with_capacity(case.vertices);
    for index in 0..case.vertices {
        let vertex = NamedIndividual::new(format!(
            "{BASE}/{}/v{index:02}",
            iri_segment(&case.id, "case")
        ));
        ontology
            .add_named_individual(vertex.clone())
            .map_err(to_string)?;
        ontology
            .add_property_assertion(PropertyAssertionAxiom::new(
                owner.iri().clone(),
                role.iri().clone(),
                vertex.iri().clone(),
            ))
            .map_err(to_string)?;
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                vertex.iri().clone(),
                ClassExpression::Class(target_class.clone()),
            ))
            .map_err(to_string)?;
        vertices.push(vertex);
    }

    for [left, right] in &case.edges {
        ontology
            .add_axiom(Axiom::DifferentIndividuals(Box::new(
                DifferentIndividualsAxiom::new(vec![
                    vertices[*left].iri().clone(),
                    vertices[*right].iri().clone(),
                ]),
            )))
            .map_err(to_string)?;
    }

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            owner.iri().clone(),
            ClassExpression::ObjectMaxCardinality(
                case.colors as u32,
                Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(role))),
                Some(Box::new(ClassExpression::Class(target_class))),
            ),
        ))
        .map_err(to_string)?;

    Ok(ontology)
}

fn measure(
    prepared: &qualified_adapter::PreparedProbe,
    mode: Mode,
    color_count: usize,
    pool: Option<&rayon::ThreadPool>,
) -> Result<Measurement, String> {
    let start = Instant::now();
    if prepared.targets_len() <= color_count {
        let witness = (0..prepared.targets_len()).collect();
        let reasoning_ns = start.elapsed().as_nanos();
        return Ok(Measurement {
            mode,
            reasoning_ns,
            tasks_started: 0,
            tasks_completed: 0,
            workers_used: Vec::new(),
            workers_used_count: 0,
            search_nodes: 0,
            tasks: Vec::new(),
            witness: Some(witness),
            result: true,
        });
    }

    match mode {
        Mode::Audit | Mode::SerialDirect => {
            measure_serial_direct(prepared, mode, color_count, start)
        }
        Mode::SerialFrontier => measure_serial_frontier(prepared, mode, color_count, start),
        Mode::ParallelFrontier => {
            let pool = pool.ok_or_else(|| "parallel_frontier requires a rayon pool".to_string())?;
            measure_parallel_frontier(prepared, mode, color_count, pool, start)
        }
    }
}

fn measure_serial_direct(
    prepared: &qualified_adapter::PreparedProbe,
    mode: Mode,
    color_count: usize,
    start: Instant,
) -> Result<Measurement, String> {
    let prefix = vec![None; prepared.targets_len()];
    let (result, search_nodes, witness) =
        prepared.run_kernel_from_prefix(color_count, &prefix, 0)?;
    let task = TaskRow {
        task_id: 0,
        prefix,
        result,
        search_nodes,
        witness: witness.clone(),
        worker_id: None,
    };
    let mut measurement = Measurement {
        mode,
        reasoning_ns: 0,
        tasks_started: 1,
        tasks_completed: 1,
        workers_used: Vec::new(),
        workers_used_count: 0,
        search_nodes,
        tasks: vec![task],
        witness,
        result,
    };
    measurement.reasoning_ns = start.elapsed().as_nanos();
    Ok(measurement)
}

fn measure_serial_frontier(
    prepared: &qualified_adapter::PreparedProbe,
    mode: Mode,
    color_count: usize,
    start: Instant,
) -> Result<Measurement, String> {
    let prefixes = prepared.legal_prefixes(color_count, PREFIX_DEPTH);
    let mut tasks = Vec::with_capacity(prefixes.len());
    for (task_id, prefix) in prefixes.into_iter().enumerate() {
        let (result, search_nodes, witness) =
            prepared.run_kernel_from_prefix(color_count, &prefix, PREFIX_DEPTH)?;
        tasks.push(TaskRow {
            task_id,
            prefix,
            result,
            search_nodes,
            witness,
            worker_id: None,
        });
    }
    let mut measurement = measurement_from_tasks(mode, 0, tasks, Vec::new());
    measurement.reasoning_ns = start.elapsed().as_nanos();
    Ok(measurement)
}

fn measure_parallel_frontier(
    prepared: &qualified_adapter::PreparedProbe,
    mode: Mode,
    color_count: usize,
    pool: &rayon::ThreadPool,
    start: Instant,
) -> Result<Measurement, String> {
    let prefixes = prepared.legal_prefixes(color_count, PREFIX_DEPTH);
    let rows: Vec<Result<TaskRow, String>> = pool.install(|| {
        use rayon::prelude::*;
        prefixes
            .into_par_iter()
            .enumerate()
            .map(|(task_id, prefix)| {
                let worker_id = rayon::current_thread_index();
                let (result, search_nodes, witness) =
                    prepared.run_kernel_from_prefix(color_count, &prefix, PREFIX_DEPTH)?;
                Ok(TaskRow {
                    task_id,
                    prefix,
                    result,
                    search_nodes,
                    witness,
                    worker_id,
                })
            })
            .collect()
    });
    let mut tasks = Vec::with_capacity(rows.len());
    for row in rows {
        tasks.push(row?);
    }
    tasks.sort_by_key(|task| task.task_id);
    let workers_used = tasks
        .iter()
        .filter_map(|task| task.worker_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut measurement = measurement_from_tasks(mode, 0, tasks, workers_used);
    measurement.reasoning_ns = start.elapsed().as_nanos();
    Ok(measurement)
}

fn measurement_from_tasks(
    mode: Mode,
    reasoning_ns: u128,
    tasks: Vec<TaskRow>,
    workers_used: Vec<usize>,
) -> Measurement {
    let result = tasks.iter().any(|task| task.result);
    let search_nodes = tasks.iter().map(|task| task.search_nodes).sum();
    let witness = tasks
        .iter()
        .find(|task| task.result)
        .and_then(|task| task.witness.clone());
    Measurement {
        mode,
        reasoning_ns,
        tasks_started: tasks.len(),
        tasks_completed: tasks.len(),
        workers_used_count: workers_used.len(),
        workers_used,
        search_nodes,
        tasks,
        witness,
        result,
    }
}

fn run_public_audit(ontology: &Ontology, workers: usize) -> Vec<PublicObservation> {
    vec![
        run_simple_audit(ontology),
        run_spacl_audit(ontology, SchedulingMode::Sequential, "sequential", workers),
        run_spacl_audit(
            ontology,
            SchedulingMode::AlwaysParallel,
            "always_parallel",
            workers,
        ),
    ]
}

fn run_simple_audit(ontology: &Ontology) -> PublicObservation {
    let result = SimpleReasoner::new(ontology.clone()).is_consistent();
    PublicObservation {
        reasoner: "SimpleReasoner",
        scheduling_mode: None,
        status: if result.is_ok() { "ok" } else { "error" },
        result: result.as_ref().ok().copied(),
        error: result.err().map(|error| error.to_string()),
        actual_workers_started: None,
        actual_workers_completed: None,
        qualified_certificate: qualified_adapter::qualified_certificate(ontology)
            .ok()
            .flatten(),
        production_preflight: true,
    }
}

fn run_spacl_audit(
    ontology: &Ontology,
    scheduling_mode: SchedulingMode,
    mode_name: &'static str,
    workers: usize,
) -> PublicObservation {
    let mut config = SpeculativeConfig::default();
    config.scheduling_mode = scheduling_mode;
    config.num_workers = workers;
    config.parallel_threshold = 1;
    config.adaptive_tuning = false;
    let mut reasoner =
        SpeculativeTableauxReasoner::with_config_arc(Arc::new(ontology.clone()), config);
    let result = reasoner.is_consistent();
    let stats = reasoner.get_stats();
    PublicObservation {
        reasoner: "SpeculativeTableauxReasoner",
        scheduling_mode: Some(mode_name),
        status: if result.is_ok() { "ok" } else { "error" },
        result: result.as_ref().ok().copied(),
        error: result.err().map(|error| error.to_string()),
        actual_workers_started: Some(stats.workers_started),
        actual_workers_completed: Some(stats.workers_completed),
        qualified_certificate: stats.qualified_consistency_certificate,
        production_preflight: stats.workers_started == 0,
    }
}

fn error_envelope(error_class: &str, message: String) -> Value {
    json!({
        "status": "error",
        "verdict": {
            "result": "error",
            "error_class": error_class
        },
        "error": message
    })
}

fn to_string<T: ToString>(value: T) -> String {
    value.to_string()
}

fn iri_segment(value: &str, fallback: &str) -> String {
    let filtered: String = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .collect();
    if filtered.is_empty() {
        fallback.to_string()
    } else {
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(id: &str, vertices: usize, edges: Vec<[usize; 2]>, colors: usize) -> CaseInput {
        CaseInput {
            id: id.to_string(),
            vertices,
            edges,
            colors,
            role: "r".to_string(),
        }
    }

    fn prepared(case: &CaseInput) -> qualified_adapter::PreparedProbe {
        let ontology = build_graph_ontology(case).unwrap();
        qualified_adapter::prepare(&ontology).unwrap()
    }

    #[test]
    fn audit_known_c5_sat_and_unsat() {
        let edges = vec![[0, 1], [1, 2], [2, 3], [3, 4], [4, 0]];
        let sat = build_graph_ontology(&case("c5-sat", 5, edges.clone(), 3)).unwrap();
        let unsat = build_graph_ontology(&case("c5-unsat", 5, edges, 2)).unwrap();
        assert_eq!(
            qualified_adapter::qualified_certificate(&sat).unwrap(),
            Some(true)
        );
        assert_eq!(
            qualified_adapter::qualified_certificate(&unsat).unwrap(),
            Some(false)
        );
    }

    #[test]
    fn serial_and_parallel_frontier_have_equivalent_task_trace() {
        let graph = case("trace", 5, vec![[0, 1], [0, 2], [1, 3], [2, 4], [3, 4]], 3);
        let prepared = prepared(&graph);
        let serial = measure(&prepared, Mode::SerialFrontier, graph.colors, None).unwrap();
        let pool = ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        let parallel =
            measure(&prepared, Mode::ParallelFrontier, graph.colors, Some(&pool)).unwrap();
        assert_eq!(serial.result, parallel.result);
        assert_eq!(serial.tasks_started, parallel.tasks_started);
        assert_eq!(serial.search_nodes, parallel.search_nodes);
        let serial_prefixes: Vec<_> = serial.tasks.iter().map(|task| &task.prefix).collect();
        let parallel_prefixes: Vec<_> = parallel.tasks.iter().map(|task| &task.prefix).collect();
        assert_eq!(serial_prefixes, parallel_prefixes);
    }

    #[test]
    fn complete_prefix_coverage_has_no_symmetry_pruning() {
        let graph = case("empty", 4, vec![], 3);
        let prepared = prepared(&graph);
        let prefixes = prepared.legal_prefixes(3, PREFIX_DEPTH);
        assert_eq!(prefixes.len(), 9);
        assert!(prefixes
            .iter()
            .any(|prefix| prefix.iter().filter(|c| c == &&Some(0)).count() == 2));
    }

    #[test]
    fn invalid_input_is_rejected() {
        let request = Request {
            case: case("bad", 3, vec![[0, 1], [1, 0]], 2),
            mode: Mode::SerialDirect,
            workers: 2,
            warmups: Some(1),
        };
        assert!(validate_request(&request)
            .unwrap_err()
            .contains("duplicate"));
    }

    #[test]
    fn independent_input_edge_validation_rejects_self_loop_and_bounds() {
        let self_loop = Request {
            case: case("self", 3, vec![[1, 1]], 2),
            mode: Mode::SerialDirect,
            workers: 2,
            warmups: Some(0),
        };
        assert!(validate_request(&self_loop)
            .unwrap_err()
            .contains("self-loop"));

        let out_of_bounds = Request {
            case: case("bounds", 3, vec![[1, 3]], 2),
            mode: Mode::SerialDirect,
            workers: 2,
            warmups: Some(0),
        };
        assert!(validate_request(&out_of_bounds)
            .unwrap_err()
            .contains("outside vertices"));
    }

    #[test]
    fn eleven_vertex_prefixes_and_witnesses_use_original_vertex_order() {
        let graph = case("n11", 11, vec![[0, 10]], 10);
        let prepared = prepared(&graph);
        let order = prepared.order_vertex_ids();
        assert_eq!(order[0], 0);
        assert_eq!(order[1], 10);
        assert_eq!(order.iter().copied().collect::<BTreeSet<_>>().len(), 11);

        let measurement = measure(&prepared, Mode::SerialFrontier, graph.colors, None).unwrap();
        assert!(measurement.result);
        assert_eq!(measurement.tasks[0].prefix.len(), 11);
        assert_eq!(measurement.tasks[0].prefix[0], Some(0));
        assert_eq!(measurement.tasks[0].prefix[1], None);
        assert_eq!(measurement.tasks[0].prefix[10], Some(1));
        let witness = measurement.witness.as_ref().unwrap();
        assert_eq!(witness.len(), 11);
        assert_ne!(witness[0], witness[10]);
    }

    #[test]
    fn trivial_shortcut_has_zero_search_work() {
        let graph = case("trivial", 3, vec![[0, 1], [1, 2]], 3);
        let prepared = prepared(&graph);
        let measurement = measure(&prepared, Mode::ParallelFrontier, graph.colors, None).unwrap();
        assert!(measurement.result);
        assert!(measurement.reasoning_ns > 0);
        assert_eq!(measurement.tasks_started, 0);
        assert_eq!(measurement.search_nodes, 0);
    }
}
