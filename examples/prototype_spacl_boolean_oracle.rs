use owl2_reasoner::{
    Class, ClassAssertionAxiom, ClassExpression, NamedIndividual, Ontology, OwlResult,
    SchedulingMode, SimpleReasoner, SpeculativeConfig, SpeculativeStats,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::process::ExitCode;

const BASE: &str = "http://tableauxx.org/prototype/spacl-boolean-oracle#";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct LiteralSpec {
    class: &'static str,
    negated: bool,
}

impl LiteralSpec {
    const fn positive(class: &'static str) -> Self {
        Self {
            class,
            negated: false,
        }
    }

    const fn negative(class: &'static str) -> Self {
        Self {
            class,
            negated: true,
        }
    }

    fn eval(&self, assignment: &BTreeMap<&'static str, bool>) -> bool {
        let value = assignment.get(self.class).copied().unwrap_or(false);
        if self.negated {
            !value
        } else {
            value
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct IndividualClauses {
    individual: &'static str,
    clauses: Vec<Vec<LiteralSpec>>,
}

#[derive(Debug, Clone, Serialize)]
struct CaseSpec {
    name: &'static str,
    description: &'static str,
    tbox_clauses: Vec<Vec<LiteralSpec>>,
    individual_clauses: Vec<IndividualClauses>,
}

#[derive(Debug, Serialize)]
struct OracleResult {
    is_consistent: bool,
    witnesses: BTreeMap<String, Option<BTreeMap<String, bool>>>,
}

#[derive(Debug, Serialize)]
struct ReasonerObservation {
    reasoner: &'static str,
    scheduling_mode: Option<&'static str>,
    num_workers: Option<usize>,
    learning_enabled: Option<bool>,
    status: &'static str,
    is_consistent: Option<bool>,
    error: Option<String>,
    branching_counters: Option<BranchingCounters>,
}

#[derive(Debug, Serialize)]
struct BranchingCounters {
    disjunctions_detected: usize,
    estimated_branch_count: usize,
    estimated_cost_us: usize,
    branch_threshold: usize,
    cost_gate_threshold_us: usize,
    branch_gate_passed: bool,
    cost_gate_passed: bool,
    used_parallel: bool,
    boolean_certificate: Option<bool>,
    work_items_expanded: usize,
    branches_created: usize,
    branches_pruned: usize,
    contradictions_found: usize,
    successful_branches: usize,
    nogoods_learned: usize,
    nogood_hits: usize,
    nogood_checks: usize,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    case: CaseSpec,
    oracle: OracleResult,
    observations: Vec<ReasonerObservation>,
    semantic_failed: bool,
    unavailable_verdicts: Vec<String>,
    mismatches: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Report {
    diagnostic: &'static str,
    note: &'static str,
    cases: Vec<CaseReport>,
    semantic_failed: bool,
}

fn main() -> ExitCode {
    let assert_correct = env::args().any(|arg| arg == "--assert-correct");
    let report = build_report();
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("failed to serialize diagnostic report: {error}");
            return ExitCode::from(2);
        }
    }

    if assert_correct && report.semantic_failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn build_report() -> Report {
    let cases: Vec<CaseReport> = cases().into_iter().map(run_case).collect();
    let semantic_failed = cases.iter().any(|case| case.semantic_failed);

    Report {
        diagnostic: "spacl_boolean_consistency_oracle",
        note: "Independent truth-table CNF oracle over named-class clauses compared with public SimpleReasoner and SpeculativeTableauxReasoner APIs.",
        cases,
        semantic_failed,
    }
}

fn run_case(case: CaseSpec) -> CaseReport {
    let oracle = evaluate_case(&case);
    let mut observations = Vec::new();
    observations.push(run_simple(&case));

    for scheduling_mode in [SchedulingMode::Sequential, SchedulingMode::AlwaysParallel] {
        for num_workers in [1, 2] {
            for learning_enabled in [false, true] {
                observations.push(run_speculative(
                    &case,
                    scheduling_mode,
                    num_workers,
                    learning_enabled,
                ));
            }
        }
    }

    let mut unavailable_verdicts = Vec::new();
    let mut mismatches = Vec::new();
    for observation in &observations {
        if observation.status != "ok" || observation.is_consistent.is_none() {
            unavailable_verdicts.push(format!(
                "{} mode={:?} workers={:?} learning={:?} unavailable status={} error={:?}",
                observation.reasoner,
                observation.scheduling_mode,
                observation.num_workers,
                observation.learning_enabled,
                observation.status,
                observation.error
            ));
        } else if observation.is_consistent != Some(oracle.is_consistent) {
            mismatches.push(format!(
                "{} mode={:?} workers={:?} learning={:?} observed={:?} expected={}",
                observation.reasoner,
                observation.scheduling_mode,
                observation.num_workers,
                observation.learning_enabled,
                observation.is_consistent,
                oracle.is_consistent
            ));
        }
    }
    let semantic_failed = !unavailable_verdicts.is_empty() || !mismatches.is_empty();

    CaseReport {
        case,
        oracle,
        observations,
        semantic_failed,
        unavailable_verdicts,
        mismatches,
    }
}

fn run_simple(case: &CaseSpec) -> ReasonerObservation {
    let result = build_ontology(case).and_then(|ontology| {
        let reasoner = SimpleReasoner::new(ontology);
        reasoner.is_consistent()
    });

    observation_from_result("SimpleReasoner", None, None, None, result, None)
}

fn run_speculative(
    case: &CaseSpec,
    scheduling_mode: SchedulingMode,
    num_workers: usize,
    learning_enabled: bool,
) -> ReasonerObservation {
    let mut config = SpeculativeConfig::default();
    config.scheduling_mode = scheduling_mode;
    config.num_workers = num_workers;
    config.enable_learning = learning_enabled;
    config.parallel_threshold = 1;
    config.adaptive_tuning = false;

    match build_ontology(case) {
        Ok(ontology) => {
            let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology, config);
            let result = reasoner.is_consistent();
            let stats = reasoner.get_stats();
            observation_from_result(
                "SpeculativeTableauxReasoner",
                Some(scheduling_mode.as_str()),
                Some(num_workers),
                Some(learning_enabled),
                result,
                Some(stats),
            )
        }
        Err(error) => observation_from_result(
            "SpeculativeTableauxReasoner",
            Some(scheduling_mode.as_str()),
            Some(num_workers),
            Some(learning_enabled),
            Err(error),
            None,
        ),
    }
}

fn observation_from_result(
    reasoner: &'static str,
    scheduling_mode: Option<&'static str>,
    num_workers: Option<usize>,
    learning_enabled: Option<bool>,
    result: OwlResult<bool>,
    stats: Option<SpeculativeStats>,
) -> ReasonerObservation {
    match result {
        Ok(is_consistent) => ReasonerObservation {
            reasoner,
            scheduling_mode,
            num_workers,
            learning_enabled,
            status: "ok",
            is_consistent: Some(is_consistent),
            error: None,
            branching_counters: stats.map(branching_counters),
        },
        Err(error) => ReasonerObservation {
            reasoner,
            scheduling_mode,
            num_workers,
            learning_enabled,
            status: "error",
            is_consistent: None,
            error: Some(error.to_string()),
            branching_counters: stats.map(branching_counters),
        },
    }
}

fn branching_counters(stats: SpeculativeStats) -> BranchingCounters {
    BranchingCounters {
        disjunctions_detected: stats.disjunctions_detected,
        estimated_branch_count: stats.estimated_branch_count,
        estimated_cost_us: stats.estimated_cost_us,
        branch_threshold: stats.branch_threshold,
        cost_gate_threshold_us: stats.cost_gate_threshold_us,
        branch_gate_passed: stats.branch_gate_passed,
        cost_gate_passed: stats.cost_gate_passed,
        used_parallel: stats.used_parallel,
        boolean_certificate: stats.boolean_certificate,
        work_items_expanded: stats.work_items_expanded,
        branches_created: stats.branches_created,
        branches_pruned: stats.branches_pruned,
        contradictions_found: stats.contradictions_found,
        successful_branches: stats.successful_branches,
        nogoods_learned: stats.nogoods_learned,
        nogood_hits: stats.nogood_hits,
        nogood_checks: stats.nogood_checks,
    }
}

fn evaluate_case(case: &CaseSpec) -> OracleResult {
    let mut witnesses = BTreeMap::new();

    for individual in &case.individual_clauses {
        let witness = satisfying_assignment(&individual.clauses);
        witnesses.insert(
            individual.individual.to_string(),
            witness.map(|assignment| {
                assignment
                    .into_iter()
                    .map(|(class, value)| (class.to_string(), value))
                    .collect()
            }),
        );
    }

    OracleResult {
        is_consistent: witnesses.values().all(Option::is_some),
        witnesses,
    }
}

fn satisfying_assignment(clauses: &[Vec<LiteralSpec>]) -> Option<BTreeMap<&'static str, bool>> {
    if clauses.iter().any(Vec::is_empty) {
        return None;
    }

    let variables: Vec<_> = clauses
        .iter()
        .flat_map(|clause| clause.iter().map(|literal| literal.class))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    if variables.is_empty() {
        return Some(BTreeMap::new());
    }

    let total_assignments = 1usize << variables.len();
    for mask in 0..total_assignments {
        let assignment: BTreeMap<_, _> = variables
            .iter()
            .enumerate()
            .map(|(index, variable)| (*variable, (mask & (1 << index)) != 0))
            .collect();

        if clauses
            .iter()
            .all(|clause| clause.iter().any(|literal| literal.eval(&assignment)))
        {
            return Some(assignment);
        }
    }

    None
}

fn build_ontology(case: &CaseSpec) -> OwlResult<Ontology> {
    let mut ontology = Ontology::new();
    let mut known_classes = BTreeSet::new();

    for clause in case.tbox_clauses.iter().chain(
        case.individual_clauses
            .iter()
            .flat_map(|item| item.clauses.iter()),
    ) {
        for literal in clause {
            if known_classes.insert(literal.class) {
                ontology.add_class(class(literal.class))?;
            }
        }
    }

    for (index, clause) in case.tbox_clauses.iter().enumerate() {
        add_clause_subclass_axiom(
            &mut ontology,
            &format!("{}_tbox_clause_{index}", case.name),
            clause,
        )?;
    }

    for individual in &case.individual_clauses {
        let named_individual = NamedIndividual::new(iri(individual.individual));
        ontology.add_named_individual(named_individual.clone())?;

        for (index, clause) in individual.clauses.iter().enumerate() {
            let anchor_name = format!("{}_{}_clause_{index}", case.name, individual.individual);
            let anchor = add_clause_subclass_axiom(&mut ontology, &anchor_name, clause)?;
            ontology.add_class_assertion(ClassAssertionAxiom::new(
                named_individual.iri().clone(),
                ClassExpression::Class(anchor),
            ))?;
        }
    }

    Ok(ontology)
}

fn add_clause_subclass_axiom(
    ontology: &mut Ontology,
    anchor_name: &str,
    clause: &[LiteralSpec],
) -> OwlResult<Class> {
    let anchor = class(anchor_name);
    ontology.add_class(anchor.clone())?;
    ontology.add_subclass_axiom(SubClassOfAxiom::new(
        ClassExpression::Class(anchor.clone()),
        union_expr(clause),
    ))?;
    Ok(anchor)
}

fn union_expr(clause: &[LiteralSpec]) -> ClassExpression {
    ClassExpression::ObjectUnionOf(
        clause
            .iter()
            .map(|literal| Box::new(literal_expr(*literal)))
            .collect(),
    )
}

fn literal_expr(literal: LiteralSpec) -> ClassExpression {
    let expression = ClassExpression::Class(class(literal.class));
    if literal.negated {
        ClassExpression::ObjectComplementOf(Box::new(expression))
    } else {
        expression
    }
}

fn class(name: &str) -> Class {
    Class::new(iri(name))
}

fn iri(name: &str) -> String {
    format!("{BASE}{name}")
}

fn cases() -> Vec<CaseSpec> {
    let a = LiteralSpec::positive("A");
    let not_a = LiteralSpec::negative("A");
    let b = LiteralSpec::positive("B");
    let not_b = LiteralSpec::negative("B");
    let c = LiteralSpec::positive("C");
    let not_c = LiteralSpec::negative("C");
    let d = LiteralSpec::positive("D");
    let not_d = LiteralSpec::negative("D");

    vec![
        CaseSpec {
            name: "two_var_four_clauses_unsat",
            description: "One asserted individual constrained by all four binary clauses over A/B; exhaustive truth table has no satisfying assignment.",
            tbox_clauses: Vec::new(),
            individual_clauses: vec![IndividualClauses {
                individual: "x",
                clauses: vec![
                    vec![a, b],
                    vec![a, not_b],
                    vec![not_a, b],
                    vec![not_a, not_b],
                ],
            }],
        },
        CaseSpec {
            name: "two_var_three_clauses_sat",
            description: "One asserted individual constrained by three binary clauses over A/B; at least A=true,B=true satisfies the CNF.",
            tbox_clauses: Vec::new(),
            individual_clauses: vec![IndividualClauses {
                individual: "x",
                clauses: vec![vec![a, b], vec![a, not_b], vec![not_a, b]],
            }],
        },
        CaseSpec {
            name: "vacuous_tbox_four_clauses_sat",
            description: "The same four contradictory-looking clauses are present only as TBox subclass axioms with no asserted individuals, so empty class extensions make the ontology satisfiable.",
            tbox_clauses: vec![
                vec![a, b],
                vec![a, not_b],
                vec![not_a, b],
                vec![not_a, not_b],
            ],
            individual_clauses: Vec::new(),
        },
        CaseSpec {
            name: "two_independent_witnesses_sat",
            description: "Two named individuals carry independent clause sets over disjoint variable pairs; each has its own satisfying witness.",
            tbox_clauses: Vec::new(),
            individual_clauses: vec![
                IndividualClauses {
                    individual: "x",
                    clauses: vec![vec![a, b], vec![not_a, b]],
                },
                IndividualClauses {
                    individual: "y",
                    clauses: vec![vec![c, d], vec![c, not_d], vec![not_c, d]],
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truth_table_oracle_rejects_all_four_binary_clauses() {
        let clauses = vec![
            vec![LiteralSpec::positive("A"), LiteralSpec::positive("B")],
            vec![LiteralSpec::positive("A"), LiteralSpec::negative("B")],
            vec![LiteralSpec::negative("A"), LiteralSpec::positive("B")],
            vec![LiteralSpec::negative("A"), LiteralSpec::negative("B")],
        ];

        assert!(satisfying_assignment(&clauses).is_none());
    }

    #[test]
    fn truth_table_oracle_accepts_three_binary_clauses() {
        let clauses = vec![
            vec![LiteralSpec::positive("A"), LiteralSpec::positive("B")],
            vec![LiteralSpec::positive("A"), LiteralSpec::negative("B")],
            vec![LiteralSpec::negative("A"), LiteralSpec::positive("B")],
        ];

        let witness = satisfying_assignment(&clauses).expect("expected satisfiable CNF");
        assert!(clauses
            .iter()
            .all(|clause| clause.iter().any(|literal| literal.eval(&witness))));
    }

    #[test]
    fn truth_table_oracle_rejects_empty_clause() {
        let empty_cnf: Vec<Vec<LiteralSpec>> = Vec::new();
        assert_eq!(satisfying_assignment(&empty_cnf), Some(BTreeMap::new()));

        let clauses = vec![Vec::new()];
        assert!(satisfying_assignment(&clauses).is_none());
    }

    #[test]
    fn truth_table_oracle_accepts_tautology_with_witness() {
        let clauses = vec![
            vec![LiteralSpec::positive("A"), LiteralSpec::negative("A")],
            vec![LiteralSpec::positive("B")],
        ];

        let witness = satisfying_assignment(&clauses).expect("expected tautological CNF witness");
        assert_eq!(witness.get("B"), Some(&true));
        assert!(clauses
            .iter()
            .all(|clause| clause.iter().any(|literal| literal.eval(&witness))));
    }

    #[test]
    fn no_asserted_individuals_are_vacuously_consistent() {
        let case = CaseSpec {
            name: "vacuous",
            description: "no asserted individuals",
            tbox_clauses: vec![vec![LiteralSpec::positive("A"), LiteralSpec::negative("A")]],
            individual_clauses: Vec::new(),
        };

        let oracle = evaluate_case(&case);
        assert!(oracle.is_consistent);
        assert!(oracle.witnesses.is_empty());
    }
}
