//! Speculative Parallel Tableaux with Adaptive Conflict Learning (SPACL)
//!
//! Implements speculative branch exploration, optional nogood learning from
//! failed branches, and configurable branch-priority policies.
//!
//! # Current scheduling boundary
//!
//! `SchedulingMode::Adaptive` currently evaluates fixed branch-count and
//! estimated-cost thresholds once before reasoning. The name is retained for
//! API compatibility; the active execution path does not update thresholds
//! from runtime feedback.
//!
//! `AdaptiveTuner` is a disconnected experimental helper. It is not invoked by
//! `SpeculativeTableauxReasoner::is_consistent`, and the separate evolutionary
//! strategy module is not part of this scheduling decision.
//!
//! # Mechanisms
//!
//! - Work-stealing exploration of admitted disjunction branches.
//! - Optional nogood retention and subsumption-based branch pruning.
//! - Baseline, deterministic heuristic, and model-backed branch ordering.
//! - Telemetry for scheduling decisions, work, pruning, and policy activity.
//!
//! Semantic preservation is a verification objective, not a formal theorem in
//! this repository. Performance must be established by paired benchmark runs;
//! the `SpeculativeStats::speedup` field is only a legacy utilization proxy.

#![allow(dead_code)]

use super::boolean_consistency::check_boolean_consistency;
use super::qualified_consistency::check_qualified_consistency;
use super::simple::SimpleReasoner;
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::class_expressions::ClassExpression;
use crossbeam::channel::{unbounded, Receiver, Sender};
use crossbeam::deque::{Stealer, Worker};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

/// Global rayon thread pool for SPACL workers
use rayon::{ThreadPool, ThreadPoolBuilder};

lazy_static::lazy_static! {
    /// Global thread pool shared across all SPACL instances
    static ref SPACL_THREAD_POOL: ThreadPool = ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .thread_name(|i| format!("spacl-worker-{}", i))
        .build()
        .expect("Failed to create SPACL thread pool");
}

/// Unique identifier for a speculative branch
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(u64);

impl BranchId {
    fn new(id: u64) -> Self {
        Self(id)
    }

    fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

/// A learned conflict (nogood) - represents a set of assertions that lead to contradiction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nogood {
    /// The set of class expressions that together are unsatisfiable
    pub assertions: HashSet<ClassExpression>,
    /// The size of the nogood (for prioritization)
    pub size: usize,
    /// How many times this nogood has been used to prune
    pub hit_count: usize,
}

impl Nogood {
    pub fn new(assertions: HashSet<ClassExpression>) -> Self {
        let size = assertions.len();
        Self {
            assertions,
            size,
            hit_count: 0,
        }
    }

    /// Check if this nogood subsumes a set of assertions
    pub fn subsumes(&self, assertions: &HashSet<ClassExpression>) -> bool {
        self.assertions.is_subset(assertions)
    }

    /// Record a hit (when this nogood prevented exploration)
    pub fn record_hit(&mut self) {
        self.hit_count += 1;
    }
}

/// Statistics for speculative reasoning
#[derive(Debug, Default, Clone)]
pub struct SpeculativeStats {
    /// Effective scheduling mode for the run
    pub scheduling_mode: String,
    /// Effective branch-priority policy for the run
    pub branch_policy: String,
    /// Number of unique disjunction expressions detected for gating
    pub disjunctions_detected: usize,
    /// Heuristic branch-count estimate used by the parallel gate
    pub estimated_branch_count: usize,
    /// Heuristic cost estimate (microseconds) used by the parallel gate
    pub estimated_cost_us: usize,
    /// Configured branch threshold used by the gate
    pub branch_threshold: usize,
    /// Effective cost threshold used by the gate (already multiplied by workers)
    pub cost_gate_threshold_us: usize,
    /// Whether the branch-count gate passed
    pub branch_gate_passed: bool,
    /// Whether the cost gate passed
    pub cost_gate_passed: bool,
    /// Whether this run actually entered speculative parallel execution
    pub used_parallel: bool,
    /// Exact Boolean verdict that bypassed admitted speculative work this run.
    /// None means no SPACL shortcut, not that the sequential checker was unused.
    pub boolean_certificate: Option<bool>,
    /// Exact qualified object-cardinality/hasSelf verdict that bypassed
    /// speculative artificial witnesses for the original ontology.
    pub qualified_consistency_certificate: Option<bool>,
    /// Work items that expanded into child work
    pub work_items_expanded: usize,
    /// Total branches created
    pub branches_created: usize,
    /// Branches pruned by nogoods
    pub branches_pruned: usize,
    /// Branches that led to contradictions
    pub contradictions_found: usize,
    /// Branches that completed successfully
    pub successful_branches: usize,
    /// Nogoods learned
    pub nogoods_learned: usize,
    /// Nogood hits (from any source)
    pub nogood_hits: usize,
    /// Local cache hits (thread-local nogood hits)
    pub local_cache_hits: usize,
    /// Global cache hits (from synchronized nogoods)
    pub global_cache_hits: usize,
    /// Total nogood checks performed
    pub nogood_checks: usize,
    /// Times a worker attempted to steal work
    pub steal_attempts: usize,
    /// Successful steal operations
    pub steal_successes: usize,
    /// Number of worker tasks started for speculative execution
    pub workers_started: usize,
    /// Number of worker tasks that completed their loop before return
    pub workers_completed: usize,
    /// Splits where policy produced an order different from ontology order
    pub policy_reordered_splits: usize,
    /// Policy fallbacks (e.g., hybrid mode without a loaded model)
    pub policy_fallbacks: usize,
    /// Number of branch-split decisions routed through hybrid policy mode
    pub hybrid_policy_calls: usize,
    /// Hybrid policy decisions where a model ranking was actually used
    pub hybrid_model_calls: usize,
    /// Number of branch snapshots exported for offline training
    pub branch_snapshots_written: usize,
    /// Time spent in speculative work that was later cancelled
    pub wasted_time_ms: u64,
    /// Total reasoning time in milliseconds
    pub reasoning_time_ms: u64,
    /// Legacy parallel-utilization proxy; not measured speedup against a baseline
    pub speedup: f64,
}

impl SpeculativeStats {
    /// Calculate nogood hit rate
    pub fn nogood_hit_rate(&self) -> f64 {
        if self.nogood_checks == 0 {
            0.0
        } else {
            self.nogood_hits as f64 / self.nogood_checks as f64
        }
    }

    /// Calculate pruning effectiveness
    pub fn pruning_effectiveness(&self) -> f64 {
        if self.branches_created == 0 {
            0.0
        } else {
            self.branches_pruned as f64 / self.branches_created as f64
        }
    }

    /// Get detailed report
    pub fn report(&self) -> String {
        format!(
            "SPACL Statistics:\n\
            - Policy: schedule={}, branch_policy={}\n\
            - Policy telemetry: hybrid_calls={}, model_calls={}, fallbacks={}, reorders={}\n\
            - Branches: {} created, {} pruned ({:.1}%), {} successful\n\
            - Contradictions: {} found\n\
            - Nogoods: {} learned, {} hits ({:.1}%)\n\
            - Cache hits: {} local, {} global\n\
            - Workers: {} started, {} completed\n\
            - Parallel-utilization proxy: {:.2}x",
            self.scheduling_mode,
            self.branch_policy,
            self.hybrid_policy_calls,
            self.hybrid_model_calls,
            self.policy_fallbacks,
            self.policy_reordered_splits,
            self.branches_created,
            self.branches_pruned,
            self.pruning_effectiveness() * 100.0,
            self.successful_branches,
            self.contradictions_found,
            self.nogoods_learned,
            self.nogood_hits,
            self.nogood_hit_rate() * 100.0,
            self.local_cache_hits,
            self.global_cache_hits,
            self.workers_started,
            self.workers_completed,
            self.speedup
        )
    }
}

/// Work item for speculative execution with branch constraints
#[derive(Debug, Clone)]
struct WorkItem {
    /// The branch this work belongs to
    branch_id: BranchId,
    /// Constraints assumed on this branch (e.g., A for left branch of A ⊔ B)
    constraints: Vec<ClassExpression>,
    /// Class expressions being tested (for nogood learning)
    test_expressions: HashSet<ClassExpression>,
    /// Current depth
    depth: usize,
    /// Shared disjunction list selected for speculative expansion
    disjunctions: Arc<Vec<ClassExpression>>,
    /// Next disjunction index to branch on
    next_disjunction_idx: usize,
}

#[derive(Debug, Clone, Copy)]
struct BranchPolicyContext {
    branch_id: BranchId,
    depth: usize,
    disjunction_index: usize,
}

/// Result from speculative execution
#[derive(Debug, Clone)]
enum WorkResult {
    /// Branch completed successfully (satisfiable)
    Success { branch_id: BranchId },
    /// Branch led to contradiction - includes nogood
    Contradiction { branch_id: BranchId, nogood: Nogood },
    /// Partial result - needs more work
    Partial {
        branch_id: BranchId,
        new_work: Vec<WorkItem>,
    },
}

#[derive(Debug, Clone, Copy)]
struct ParallelDecision {
    disjunctions_detected: usize,
    estimated_branch_count: usize,
    estimated_cost_us: usize,
    branch_threshold: usize,
    cost_gate_threshold_us: usize,
    branch_gate_passed: bool,
    cost_gate_passed: bool,
    use_parallel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingMode {
    /// One-shot fixed branch-and-cost admission gate (legacy variant name)
    Adaptive,
    Sequential,
    AlwaysParallel,
}

impl SchedulingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SchedulingMode::Adaptive => "adaptive",
            SchedulingMode::Sequential => "sequential",
            SchedulingMode::AlwaysParallel => "always_parallel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchPolicyMode {
    /// Use ontology operand order (current baseline behavior)
    Baseline,
    /// Use deterministic structural heuristic ranking
    Heuristic,
    /// Learned ranking policy loaded from model file; safely falls back when unavailable
    HybridRrn,
}

impl BranchPolicyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            BranchPolicyMode::Baseline => "baseline",
            BranchPolicyMode::Heuristic => "heuristic",
            BranchPolicyMode::HybridRrn => "hybrid_rrn",
        }
    }
}

/// Configuration for speculative parallelism
#[derive(Debug, Clone)]
pub struct SpeculativeConfig {
    /// Number of worker threads
    pub num_workers: usize,
    /// Scheduling policy for speculative branch exploration
    pub scheduling_mode: SchedulingMode,
    /// Branch-priority policy for disjunction operand ordering
    pub branch_policy: BranchPolicyMode,
    /// Optional path to a learned policy model (linear or gbdt_stump_v1) used in `hybrid_rrn` mode
    pub rrn_model_path: Option<String>,
    /// Optional JSONL output path for branch-level policy snapshots
    pub branch_snapshot_path: Option<String>,
    /// Maximum speculative depth (to prevent explosion)
    pub max_speculative_depth: usize,
    /// Threshold for parallelizing disjunctions (min branch size)
    pub parallel_threshold: usize,
    /// Whether to use nogood learning
    pub enable_learning: bool,
    /// Maximum number of nogoods to store
    pub max_nogoods: usize,
    /// Timeout for speculative work
    pub speculative_timeout_ms: u64,
    /// Whether the static estimated-cost gate participates in `Adaptive` mode
    ///
    /// This legacy field name does not enable feedback-driven threshold updates.
    pub adaptive_tuning: bool,
    /// Fixed per-worker cost threshold in microseconds
    /// Only parallelize if estimated work reaches the effective threshold.
    pub cost_threshold_us: usize,
    /// Estimated microseconds per disjunction operand
    pub cost_per_operand_us: usize,
    /// Estimated microseconds per nested expression
    pub cost_per_nesting_us: usize,
}

impl Default for SpeculativeConfig {
    fn default() -> Self {
        Self {
            num_workers: num_cpus::get(),
            scheduling_mode: SchedulingMode::Adaptive,
            branch_policy: BranchPolicyMode::Baseline,
            rrn_model_path: None,
            branch_snapshot_path: None,
            max_speculative_depth: 10,
            // Calibrated conservatively so branch-count gate avoids pathological
            // parallelization on medium disjunctive workloads while still leaving
            // room for larger branch-heavy cases to opt in.
            parallel_threshold: 2000,
            enable_learning: true,
            max_nogoods: 10000,
            speculative_timeout_ms: 30000,
            adaptive_tuning: true,
            cost_threshold_us: 500, // 500µs minimum to justify parallelization
            cost_per_operand_us: 50, // 50µs per operand
            cost_per_nesting_us: 15, // Conservative refinement from calibration; keeps default in the 2000+ plateau
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RrnLinearWeights {
    bias: f64,
    union_weight: f64,
    intersection_weight: f64,
    complement_weight: f64,
    some_values_weight: f64,
    all_values_weight: f64,
    atom_weight: f64,
    depth_weight: f64,
    node_weight: f64,
}

impl Default for RrnLinearWeights {
    fn default() -> Self {
        Self {
            bias: 0.0,
            union_weight: 5.0,
            intersection_weight: 2.5,
            complement_weight: 1.0,
            some_values_weight: 1.5,
            all_values_weight: 1.5,
            atom_weight: 0.3,
            depth_weight: 0.7,
            node_weight: 0.1,
        }
    }
}

impl RrnLinearWeights {
    fn score(&self, fv: &ExprFeatureVector) -> f64 {
        self.bias
            + self.union_weight * fv.union_nodes as f64
            + self.intersection_weight * fv.intersection_nodes as f64
            + self.complement_weight * fv.complement_nodes as f64
            + self.some_values_weight * fv.some_values_nodes as f64
            + self.all_values_weight * fv.all_values_nodes as f64
            + self.atom_weight * fv.atom_nodes as f64
            + self.depth_weight * fv.max_depth as f64
            + self.node_weight * fv.total_nodes as f64
    }
}

#[derive(Debug, Clone, Default)]
struct ExprFeatureVector {
    union_nodes: usize,
    intersection_nodes: usize,
    complement_nodes: usize,
    some_values_nodes: usize,
    all_values_nodes: usize,
    atom_nodes: usize,
    total_nodes: usize,
    max_depth: usize,
}

impl ExprFeatureVector {
    fn combine_with(&mut self, other: &ExprFeatureVector) {
        self.union_nodes += other.union_nodes;
        self.intersection_nodes += other.intersection_nodes;
        self.complement_nodes += other.complement_nodes;
        self.some_values_nodes += other.some_values_nodes;
        self.all_values_nodes += other.all_values_nodes;
        self.atom_nodes += other.atom_nodes;
        self.total_nodes += other.total_nodes;
        self.max_depth = self.max_depth.max(other.max_depth);
    }

    fn from_expression(expr: &ClassExpression, depth: usize) -> Self {
        let mut out = Self {
            total_nodes: 1,
            max_depth: depth,
            ..Self::default()
        };

        match expr {
            ClassExpression::ObjectUnionOf(operands) => {
                out.union_nodes = 1;
                for op in operands {
                    out.combine_with(&Self::from_expression(op, depth + 1));
                }
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                out.intersection_nodes = 1;
                for op in operands {
                    out.combine_with(&Self::from_expression(op, depth + 1));
                }
            }
            ClassExpression::ObjectComplementOf(inner) => {
                out.complement_nodes = 1;
                out.combine_with(&Self::from_expression(inner, depth + 1));
            }
            ClassExpression::ObjectSomeValuesFrom(_, inner) => {
                out.some_values_nodes = 1;
                out.combine_with(&Self::from_expression(inner, depth + 1));
            }
            ClassExpression::ObjectAllValuesFrom(_, inner) => {
                out.all_values_nodes = 1;
                out.combine_with(&Self::from_expression(inner, depth + 1));
            }
            _ => {
                out.atom_nodes = 1;
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotOperandFeatures {
    union_nodes: usize,
    intersection_nodes: usize,
    complement_nodes: usize,
    some_values_nodes: usize,
    all_values_nodes: usize,
    atom_nodes: usize,
    total_nodes: usize,
    max_depth: usize,
}

impl From<ExprFeatureVector> for SnapshotOperandFeatures {
    fn from(value: ExprFeatureVector) -> Self {
        Self {
            union_nodes: value.union_nodes,
            intersection_nodes: value.intersection_nodes,
            complement_nodes: value.complement_nodes,
            some_values_nodes: value.some_values_nodes,
            all_values_nodes: value.all_values_nodes,
            atom_nodes: value.atom_nodes,
            total_nodes: value.total_nodes,
            max_depth: value.max_depth,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RrnGbdtStump {
    feature: String,
    threshold: f64,
    left_value: f64,
    right_value: f64,
}

impl Default for RrnGbdtStump {
    fn default() -> Self {
        Self {
            feature: "total_nodes".to_string(),
            threshold: 0.0,
            left_value: 0.0,
            right_value: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RrnGbdtModel {
    model_type: String,
    base_score: f64,
    learning_rate: f64,
    stumps: Vec<RrnGbdtStump>,
}

impl Default for RrnGbdtModel {
    fn default() -> Self {
        Self {
            model_type: "gbdt_stump_v1".to_string(),
            base_score: 0.0,
            learning_rate: 0.1,
            stumps: Vec::new(),
        }
    }
}

impl RrnGbdtModel {
    fn feature_value(feature: &str, fv: &ExprFeatureVector) -> f64 {
        match feature {
            "union_nodes" | "union" => fv.union_nodes as f64,
            "intersection_nodes" | "intersection" => fv.intersection_nodes as f64,
            "complement_nodes" | "complement" => fv.complement_nodes as f64,
            "some_values_nodes" | "some_values" | "some" => fv.some_values_nodes as f64,
            "all_values_nodes" | "all_values" | "all" => fv.all_values_nodes as f64,
            "atom_nodes" | "atom" => fv.atom_nodes as f64,
            "max_depth" | "depth" => fv.max_depth as f64,
            "total_nodes" | "node_count" | "nodes" => fv.total_nodes as f64,
            _ => 0.0,
        }
    }

    fn score(&self, fv: &ExprFeatureVector) -> f64 {
        let mut score = self.base_score;
        for stump in &self.stumps {
            let x = Self::feature_value(&stump.feature, fv);
            let delta = if x <= stump.threshold {
                stump.left_value
            } else {
                stump.right_value
            };
            score += self.learning_rate * delta;
        }
        score
    }
}

#[derive(Debug, Clone)]
enum RrnPolicyModel {
    Linear(RrnLinearWeights),
    Gbdt(RrnGbdtModel),
}

impl RrnPolicyModel {
    fn from_path(path: &str) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| format!("failed reading RRN model file '{}': {}", path, err))?;
        let value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|err| format!("failed parsing RRN model '{}': {}", path, err))?;
        let model_type = value
            .get("model_type")
            .and_then(|v| v.as_str())
            .unwrap_or("linear")
            .to_string();
        if model_type.eq_ignore_ascii_case("gbdt_stump_v1") {
            let model: RrnGbdtModel = serde_json::from_value(value)
                .map_err(|err| format!("failed parsing GBDT policy model '{}': {}", path, err))?;
            Ok(Self::Gbdt(model))
        } else {
            let weights: RrnLinearWeights = serde_json::from_value(value).map_err(|err| {
                format!(
                    "failed parsing linear policy model '{}': {} (model_type='{}')",
                    path, err, model_type
                )
            })?;
            Ok(Self::Linear(weights))
        }
    }

    fn score_expression(&self, expr: &ClassExpression) -> f64 {
        let fv = ExprFeatureVector::from_expression(expr, 0);
        match self {
            RrnPolicyModel::Linear(weights) => weights.score(&fv),
            RrnPolicyModel::Gbdt(model) => model.score(&fv),
        }
    }
}

#[derive(Debug, Clone)]
struct PolicyRanking {
    ordered_operands: Vec<ClassExpression>,
    ordered_indices: Vec<usize>,
    scores: Vec<f64>,
    reordered: bool,
    used_model: bool,
    used_fallback: bool,
}

#[derive(Debug, Clone)]
struct BranchPolicyEngine {
    mode: BranchPolicyMode,
    rrn_model: Option<RrnPolicyModel>,
}

impl BranchPolicyEngine {
    fn new(config: &SpeculativeConfig) -> Self {
        if !matches!(config.branch_policy, BranchPolicyMode::HybridRrn) {
            return Self {
                mode: config.branch_policy,
                rrn_model: None,
            };
        }

        let rrn_model = config
            .rrn_model_path
            .as_deref()
            .and_then(|path| RrnPolicyModel::from_path(path).ok());
        Self {
            mode: config.branch_policy,
            rrn_model,
        }
    }

    fn heuristic_score(expr: &ClassExpression) -> f64 {
        match expr {
            ClassExpression::ObjectUnionOf(operands) => {
                20.0 + operands.len() as f64 * 4.0
                    + operands
                        .iter()
                        .map(|op| Self::heuristic_score(op))
                        .sum::<f64>()
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                10.0 + operands.len() as f64 * 2.0
                    + operands
                        .iter()
                        .map(|op| Self::heuristic_score(op))
                        .sum::<f64>()
            }
            ClassExpression::ObjectComplementOf(inner) => 3.0 + Self::heuristic_score(inner),
            ClassExpression::ObjectSomeValuesFrom(_, inner) => 5.0 + Self::heuristic_score(inner),
            ClassExpression::ObjectAllValuesFrom(_, inner) => 5.0 + Self::heuristic_score(inner),
            _ => 1.0,
        }
    }

    fn rank(&self, operands: &[ClassExpression]) -> PolicyRanking {
        let mut scored: Vec<(f64, usize, ClassExpression)> = match self.mode {
            BranchPolicyMode::Baseline => operands
                .iter()
                .cloned()
                .enumerate()
                .map(|(idx, expr)| (idx as f64, idx, expr))
                .collect(),
            BranchPolicyMode::Heuristic => operands
                .iter()
                .cloned()
                .enumerate()
                .map(|(idx, expr)| (Self::heuristic_score(&expr), idx, expr))
                .collect(),
            BranchPolicyMode::HybridRrn => {
                if let Some(model) = &self.rrn_model {
                    operands
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(idx, expr)| (model.score_expression(&expr), idx, expr))
                        .collect()
                } else {
                    operands
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(idx, expr)| (Self::heuristic_score(&expr), idx, expr))
                        .collect()
                }
            }
        };

        if !matches!(self.mode, BranchPolicyMode::Baseline) {
            scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }

        let ordered_indices: Vec<usize> = scored.iter().map(|(_, idx, _)| *idx).collect();
        let scores: Vec<f64> = scored.iter().map(|(score, _, _)| *score).collect();
        let ordered_operands: Vec<ClassExpression> =
            scored.into_iter().map(|(_, _, expr)| expr).collect();

        let reordered = ordered_indices
            .iter()
            .enumerate()
            .any(|(pos, idx)| pos != *idx);
        let used_model =
            matches!(self.mode, BranchPolicyMode::HybridRrn) && self.rrn_model.is_some();
        let used_fallback = matches!(self.mode, BranchPolicyMode::HybridRrn) && !used_model;

        PolicyRanking {
            ordered_operands,
            ordered_indices,
            scores,
            reordered,
            used_model,
            used_fallback,
        }
    }
}

#[derive(Debug, Serialize)]
struct BranchSnapshotRecord {
    branch_id: u64,
    depth: usize,
    disjunction_index: usize,
    policy_mode: String,
    operand_count: usize,
    ordered_indices: Vec<usize>,
    operand_scores: Vec<f64>,
    operand_features: Vec<SnapshotOperandFeatures>,
    reordered: bool,
    hybrid_model_used: bool,
    hybrid_fallback_used: bool,
}

struct BranchSnapshotWriter {
    writer: Mutex<BufWriter<std::fs::File>>,
}

impl BranchSnapshotWriter {
    fn from_path(path: &str) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| format!("failed opening branch snapshot file '{}': {}", path, err))?;
        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    fn write_record(&self, record: &BranchSnapshotRecord) -> Result<(), String> {
        let mut guard = self
            .writer
            .lock()
            .map_err(|_| "failed to lock branch snapshot writer".to_string())?;
        serde_json::to_writer(&mut *guard, record)
            .map_err(|err| format!("failed serializing branch snapshot: {}", err))?;
        guard
            .write_all(b"\n")
            .map_err(|err| format!("failed writing branch snapshot newline: {}", err))?;
        guard
            .flush()
            .map_err(|err| format!("failed flushing branch snapshot writer: {}", err))
    }
}

/// Global nogood database shared across all workers
#[derive(Debug)]
struct NogoodDatabase {
    nogoods: RwLock<Vec<Nogood>>,
    max_size: usize,
}

impl NogoodDatabase {
    fn new(max_size: usize) -> Self {
        Self {
            nogoods: RwLock::new(Vec::new()),
            max_size,
        }
    }

    /// Check if any nogood subsumes the given assertions
    fn check_conflict(&self, assertions: &HashSet<ClassExpression>) -> Option<Nogood> {
        if let Ok(nogoods) = self.nogoods.read() {
            for nogood in nogoods.iter() {
                if nogood.subsumes(assertions) {
                    return Some(nogood.clone());
                }
            }
        }
        None
    }

    /// Add a new nogood, maintaining size limit
    fn add_nogood(&self, nogood: Nogood) {
        if let Ok(mut nogoods) = self.nogoods.write() {
            nogoods.push(nogood);

            // If too many nogoods, prune least useful ones
            if nogoods.len() > self.max_size {
                nogoods.sort_by(|a, b| {
                    // Prefer smaller nogoods with more hits
                    let a_score = a.hit_count as f64 / a.size as f64;
                    let b_score = b.hit_count as f64 / b.size as f64;
                    b_score.partial_cmp(&a_score).unwrap()
                });
                nogoods.truncate(self.max_size * 9 / 10); // Keep 90%
            }
        }
    }

    /// Get statistics
    fn stats(&self) -> (usize, usize) {
        if let Ok(nogoods) = self.nogoods.read() {
            let total_hits: usize = nogoods.iter().map(|n| n.hit_count).sum();
            (nogoods.len(), total_hits)
        } else {
            (0, 0)
        }
    }

    /// Access the internal nogoods for cache synchronization
    fn get_nogoods(&self) -> Option<std::sync::RwLockReadGuard<'_, Vec<Nogood>>> {
        self.nogoods.read().ok()
    }
}

/// Thread-local nogood cache to reduce synchronization overhead
///
/// Each worker thread maintains its own local cache of nogoods.
/// The cache is periodically synced to the global database.
struct ThreadLocalNogoodCache {
    /// Local nogoods discovered by this thread
    local_nogoods: Vec<Nogood>,
    /// Copy of nogoods from global database (stale but useful)
    cached_nogoods: Vec<Nogood>,
    /// Number of checks since last sync
    checks_since_sync: usize,
    /// Sync interval
    sync_interval: usize,
    /// Hit count for statistics
    local_hits: usize,
}

impl ThreadLocalNogoodCache {
    fn new(sync_interval: usize) -> Self {
        Self {
            local_nogoods: Vec::new(),
            cached_nogoods: Vec::new(),
            checks_since_sync: 0,
            sync_interval,
            local_hits: 0,
        }
    }

    /// Check for conflicts using local cache first, then global
    fn check_conflict(
        &mut self,
        global: &NogoodDatabase,
        assertions: &HashSet<ClassExpression>,
    ) -> Option<Nogood> {
        self.checks_since_sync += 1;

        // Check local nogoods first (fastest)
        for nogood in &self.local_nogoods {
            if nogood.subsumes(assertions) {
                self.local_hits += 1;
                return Some(nogood.clone());
            }
        }

        // Check cached nogoods
        for nogood in &self.cached_nogoods {
            if nogood.subsumes(assertions) {
                return Some(nogood.clone());
            }
        }

        // Sync with global database periodically
        if self.checks_since_sync >= self.sync_interval {
            self.sync_with_global(global);
            self.checks_since_sync = 0;

            // Re-check with updated cache
            for nogood in &self.cached_nogoods {
                if nogood.subsumes(assertions) {
                    return Some(nogood.clone());
                }
            }
        }

        None
    }

    /// Add a nogood to local cache
    fn add_nogood(&mut self, nogood: Nogood) {
        self.local_nogoods.push(nogood);
    }

    /// Sync cached nogoods from global database
    fn sync_with_global(&mut self, global: &NogoodDatabase) {
        if let Ok(nogoods) = global.nogoods.read() {
            // Update cache with any new nogoods
            if nogoods.len() != self.cached_nogoods.len() {
                self.cached_nogoods = nogoods.clone();
            }
        }
    }

    /// Flush local nogoods to global database
    fn flush_to_global(&mut self, global: &NogoodDatabase) {
        for nogood in self.local_nogoods.drain(..) {
            global.add_nogood(nogood);
        }
    }
}

/// The SPACL reasoner
pub struct SpeculativeTableauxReasoner {
    /// The ontology being reasoned about
    ontology: Arc<Ontology>,
    /// Configuration
    config: SpeculativeConfig,
    /// Nogood database
    nogoods: Arc<NogoodDatabase>,
    /// Statistics
    stats: Arc<Mutex<SpeculativeStats>>,
    /// Worker thread handles
    workers: Option<Vec<thread::JoinHandle<()>>>,
    /// Work queue
    work_queue: Option<Worker<WorkItem>>,
    /// Stealers for work-stealing
    stealers: Option<Vec<Stealer<WorkItem>>>,
    /// Result channel
    result_sender: Option<Sender<WorkResult>>,
    result_receiver: Option<Receiver<WorkResult>>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Early termination flag - set when SAT found
    solution_found: Arc<AtomicBool>,
}

impl SpeculativeTableauxReasoner {
    /// Create a new speculative reasoner
    pub fn new(ontology: Ontology) -> Self {
        let config = SpeculativeConfig::default();
        Self::with_config(ontology, config)
    }

    /// Create a new speculative reasoner from a shared ontology
    pub fn from_arc(ontology: Arc<Ontology>) -> Self {
        let config = SpeculativeConfig::default();
        Self::with_config_arc(ontology, config)
    }

    /// Create with custom configuration
    pub fn with_config(ontology: Ontology, config: SpeculativeConfig) -> Self {
        Self::with_config_arc(Arc::new(ontology), config)
    }

    /// Create with custom configuration from a shared ontology
    pub fn with_config_arc(ontology: Arc<Ontology>, config: SpeculativeConfig) -> Self {
        Self {
            ontology,
            config,
            nogoods: Arc::new(NogoodDatabase::new(10000)),
            stats: Arc::new(Mutex::new(SpeculativeStats::default())),
            workers: None,
            work_queue: None,
            stealers: None,
            result_sender: None,
            result_receiver: None,
            shutdown: Arc::new(AtomicBool::new(false)),
            solution_found: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Estimate the number of branches needed for reasoning
    ///
    /// This is a rough estimate based on:
    /// - Number of disjunctions (unions) in the ontology
    /// - Number of existential restrictions
    /// - Number of classes with multiple parents
    fn estimate_branch_count(&self, disjunctions_detected: usize) -> usize {
        let mut disjoint_axioms = 0usize;

        // Count disjunctive axioms (each adds branching)
        for axiom in self.ontology.axioms() {
            if let crate::logic::axioms::Axiom::DisjointClasses(_) = axiom.as_ref() {
                disjoint_axioms += 1;
            }
        }

        // Scale by number of classes (rough heuristic)
        let class_count = self.ontology.classes().len();
        (disjunctions_detected + 2 * disjoint_axioms + class_count / 10).max(1)
    }

    /// Check ontology consistency using sequential processing
    ///
    /// Used as a fallback for small ontologies where parallel overhead
    /// exceeds the benefit.
    fn is_consistent_sequential(&self) -> OwlResult<bool> {
        // Use SimpleReasoner for minimal overhead on trivial cases
        let reasoner = SimpleReasoner::from_arc(Arc::clone(&self.ontology));
        reasoner.is_consistent()
    }

    /// Find all disjunctions (unions) in the ontology
    /// These are candidates for parallel branch splitting
    fn find_disjunctions(&self) -> Vec<ClassExpression> {
        let mut disjunctions = Vec::new();

        for axiom in self.ontology.axioms() {
            // Check subclass axioms for disjunctions in superclass position
            if let crate::logic::axioms::Axiom::SubClassOf(sub) = axiom.as_ref() {
                self.find_disjunctions_in_expression(sub.super_class(), &mut disjunctions);
            }
            // Note: EquivalentClasses axioms could also be checked here
        }

        // Remove duplicates
        disjunctions.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        disjunctions.dedup_by(|a, b| format!("{:?}", a) == format!("{:?}", b));

        disjunctions
    }

    /// Recursively find disjunctions in a class expression
    fn find_disjunctions_in_expression(
        &self,
        expr: &ClassExpression,
        disjunctions: &mut Vec<ClassExpression>,
    ) {
        match expr {
            ClassExpression::ObjectUnionOf(operands) if operands.len() >= 2 => {
                disjunctions.push(expr.clone());
                // Also check operands recursively
                for op in operands {
                    self.find_disjunctions_in_expression(op, disjunctions);
                }
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                for op in operands {
                    self.find_disjunctions_in_expression(op, disjunctions);
                }
            }
            ClassExpression::ObjectComplementOf(inner) => {
                self.find_disjunctions_in_expression(inner, disjunctions);
            }
            ClassExpression::ObjectSomeValuesFrom(_, class)
            | ClassExpression::ObjectAllValuesFrom(_, class) => {
                self.find_disjunctions_in_expression(class, disjunctions);
            }
            ClassExpression::ObjectMinCardinality(_, _, Some(qualifier))
            | ClassExpression::ObjectMaxCardinality(_, _, Some(qualifier))
            | ClassExpression::ObjectExactCardinality(_, _, Some(qualifier)) => {
                self.find_disjunctions_in_expression(qualifier, disjunctions);
            }
            _ => {} // Atomic classes, no recursion needed
        }
    }

    /// Estimate the computational cost of processing a class expression
    /// Returns estimated microseconds of work
    fn estimate_expression_cost(&self, expr: &ClassExpression) -> usize {
        match expr {
            ClassExpression::ObjectUnionOf(operands) => {
                // Base cost + cost per operand + nested cost
                let base_cost = self.config.cost_per_operand_us * operands.len();
                let nested_cost: usize = operands
                    .iter()
                    .map(|op| self.estimate_expression_cost(op))
                    .sum();
                base_cost + nested_cost
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                let base_cost = self.config.cost_per_operand_us * operands.len() / 2; // Cheaper than union
                let nested_cost: usize = operands
                    .iter()
                    .map(|op| self.estimate_expression_cost(op))
                    .sum();
                base_cost + nested_cost
            }
            ClassExpression::ObjectComplementOf(inner) => {
                self.config.cost_per_nesting_us + self.estimate_expression_cost(inner)
            }
            ClassExpression::ObjectSomeValuesFrom(_, class) => {
                self.config.cost_per_nesting_us * 2 + self.estimate_expression_cost(class)
            }
            ClassExpression::ObjectAllValuesFrom(_, class) => {
                self.config.cost_per_nesting_us * 2 + self.estimate_expression_cost(class)
            }
            ClassExpression::ObjectMinCardinality(_, _, Some(qualifier))
            | ClassExpression::ObjectMaxCardinality(_, _, Some(qualifier))
            | ClassExpression::ObjectExactCardinality(_, _, Some(qualifier)) => {
                self.config.cost_per_nesting_us * 2 + self.estimate_expression_cost(qualifier)
            }
            _ => 10, // Atomic classes are cheap
        }
    }

    /// Estimate total cost of all disjunctions in the ontology
    fn estimate_total_cost(&self, disjunctions: &[ClassExpression]) -> usize {
        disjunctions
            .iter()
            .map(|d| self.estimate_expression_cost(d))
            .sum()
    }

    /// Evaluate the implementation-aligned parallel gating decision.
    fn evaluate_parallel_decision(&self, disjunctions: &[ClassExpression]) -> ParallelDecision {
        let disjunctions_detected = disjunctions.len();
        let estimated_branch_count = self.estimate_branch_count(disjunctions_detected);
        let branch_threshold = self.config.parallel_threshold;
        let estimated_cost_us = if disjunctions.is_empty() {
            0
        } else {
            self.estimate_total_cost(disjunctions)
        };
        let cost_gate_threshold_us = self.config.cost_threshold_us * self.config.num_workers;

        if disjunctions.is_empty() {
            return ParallelDecision {
                disjunctions_detected,
                estimated_branch_count,
                estimated_cost_us,
                branch_threshold,
                cost_gate_threshold_us,
                branch_gate_passed: false,
                cost_gate_passed: false,
                use_parallel: false,
            };
        }

        let branch_gate_passed = estimated_branch_count >= branch_threshold;
        let cost_gate_passed = if self.config.adaptive_tuning {
            estimated_cost_us >= cost_gate_threshold_us
        } else {
            true
        };
        let adaptive_parallel = branch_gate_passed && cost_gate_passed;
        let use_parallel = match self.config.scheduling_mode {
            SchedulingMode::Adaptive => adaptive_parallel,
            SchedulingMode::Sequential => false,
            SchedulingMode::AlwaysParallel => !disjunctions.is_empty(),
        };

        ParallelDecision {
            disjunctions_detected,
            estimated_branch_count,
            estimated_cost_us,
            branch_threshold,
            cost_gate_threshold_us,
            branch_gate_passed,
            cost_gate_passed,
            use_parallel,
        }
    }

    /// Create root work item for recursive speculative expansion.
    fn create_branch_work_items(
        &self,
        disjunctions: Arc<Vec<ClassExpression>>,
        test_expressions: &HashSet<ClassExpression>,
    ) -> Vec<WorkItem> {
        vec![WorkItem {
            branch_id: BranchId::new(0),
            constraints: Vec::new(),
            test_expressions: test_expressions.clone(),
            depth: 0,
            disjunctions,
            next_disjunction_idx: 0,
        }]
    }

    /// Build assertion set for nogood checking from ontology-level test expressions
    /// plus branch-specific constraints.
    fn branch_assertions(item: &WorkItem) -> HashSet<ClassExpression> {
        let mut assertions = item.test_expressions.clone();
        for expr in &item.constraints {
            assertions.insert(expr.clone());
        }
        assertions
    }

    fn prioritize_operands(
        operands: &[Box<ClassExpression>],
        context: BranchPolicyContext,
        policy_engine: &BranchPolicyEngine,
        stats: &Arc<Mutex<SpeculativeStats>>,
        snapshot_writer: Option<&Arc<BranchSnapshotWriter>>,
    ) -> Vec<ClassExpression> {
        let original: Vec<ClassExpression> = operands.iter().map(|op| (**op).clone()).collect();
        let ranking = policy_engine.rank(&original);

        if let Ok(mut s) = stats.lock() {
            if ranking.reordered {
                s.policy_reordered_splits += 1;
            }
            if matches!(policy_engine.mode, BranchPolicyMode::HybridRrn) {
                s.hybrid_policy_calls += 1;
                if ranking.used_model {
                    s.hybrid_model_calls += 1;
                }
                if ranking.used_fallback {
                    s.policy_fallbacks += 1;
                }
            }
        }

        if let Some(writer) = snapshot_writer {
            let operand_features: Vec<SnapshotOperandFeatures> = ranking
                .ordered_operands
                .iter()
                .map(|expr| {
                    SnapshotOperandFeatures::from(ExprFeatureVector::from_expression(expr, 0))
                })
                .collect();
            let record = BranchSnapshotRecord {
                branch_id: context.branch_id.0,
                depth: context.depth,
                disjunction_index: context.disjunction_index,
                policy_mode: policy_engine.mode.as_str().to_string(),
                operand_count: original.len(),
                ordered_indices: ranking.ordered_indices.clone(),
                operand_scores: ranking.scores.clone(),
                operand_features,
                reordered: ranking.reordered,
                hybrid_model_used: ranking.used_model,
                hybrid_fallback_used: ranking.used_fallback,
            };
            if writer.write_record(&record).is_ok() {
                if let Ok(mut s) = stats.lock() {
                    s.branch_snapshots_written += 1;
                }
            }
        }

        ranking.ordered_operands
    }

    /// Expand a work item by splitting on the next selected disjunction.
    fn expand_work_item(
        item: &WorkItem,
        max_depth: usize,
        branch_counter: &AtomicUsize,
        policy_engine: &BranchPolicyEngine,
        stats: &Arc<Mutex<SpeculativeStats>>,
        snapshot_writer: Option<&Arc<BranchSnapshotWriter>>,
    ) -> Option<Vec<WorkItem>> {
        if item.depth >= max_depth || item.next_disjunction_idx >= item.disjunctions.len() {
            return None;
        }

        let disjunction = item.disjunctions.get(item.next_disjunction_idx)?;
        if let ClassExpression::ObjectUnionOf(operands) = disjunction {
            if operands.is_empty() {
                return None;
            }

            let context = BranchPolicyContext {
                branch_id: item.branch_id,
                depth: item.depth,
                disjunction_index: item.next_disjunction_idx,
            };
            let prioritized = Self::prioritize_operands(
                operands.as_ref(),
                context,
                policy_engine,
                stats,
                snapshot_writer,
            );
            let mut new_work = Vec::with_capacity(prioritized.len());
            for operand in prioritized {
                let mut constraints = item.constraints.clone();
                constraints.push(operand.clone());

                let mut test_expressions = item.test_expressions.clone();
                test_expressions.insert(operand);

                let next_id = branch_counter.fetch_add(1, Ordering::SeqCst) as u64;
                new_work.push(WorkItem {
                    branch_id: BranchId::new(next_id),
                    constraints,
                    test_expressions,
                    depth: item.depth + 1,
                    disjunctions: Arc::clone(&item.disjunctions),
                    next_disjunction_idx: item.next_disjunction_idx + 1,
                });
            }

            return Some(new_work);
        }

        // If a selected expression is no longer a disjunction, advance index and continue.
        Some(vec![WorkItem {
            branch_id: item.branch_id,
            constraints: item.constraints.clone(),
            test_expressions: item.test_expressions.clone(),
            depth: item.depth + 1,
            disjunctions: Arc::clone(&item.disjunctions),
            next_disjunction_idx: item.next_disjunction_idx + 1,
        }])
    }

    /// Check ontology consistency using speculative parallel reasoning
    ///
    /// Automatically selects between sequential and parallel processing
    /// based on the estimated problem complexity.
    pub fn is_consistent(&mut self) -> OwlResult<bool> {
        let start_time = Instant::now();
        if let Ok(mut stats) = self.stats.lock() {
            stats.scheduling_mode = self.config.scheduling_mode.as_str().to_string();
            stats.branch_policy = self.config.branch_policy.as_str().to_string();
            stats.used_parallel = false;
            stats.boolean_certificate = None;
            stats.qualified_consistency_certificate = None;
        }

        match check_qualified_consistency(&self.ontology) {
            Ok(Some(verdict)) => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.qualified_consistency_certificate = Some(verdict);
                    stats.reasoning_time_ms = start_time.elapsed().as_millis() as u64;
                }
                return Ok(verdict);
            }
            // The checker declining its fragment (mixed axioms) is a
            // pass, not a verdict; the sequential layers below decide
            // (#113). Budget exhaustion still surfaces — it is a guard
            // on the checker's own correctness, not a fragment
            // boundary.
            Err(OwlError::UnsupportedFeature(_)) => {}
            Err(error) => return Err(error),
            Ok(None) => {}
        }

        // Find disjunctions first to estimate cost
        let disjunctions = self.find_disjunctions();
        let decision = self.evaluate_parallel_decision(&disjunctions);

        if let Ok(mut stats) = self.stats.lock() {
            stats.scheduling_mode = self.config.scheduling_mode.as_str().to_string();
            stats.branch_policy = self.config.branch_policy.as_str().to_string();
            stats.disjunctions_detected = decision.disjunctions_detected;
            stats.estimated_branch_count = decision.estimated_branch_count;
            stats.estimated_cost_us = decision.estimated_cost_us;
            stats.branch_threshold = decision.branch_threshold;
            stats.cost_gate_threshold_us = decision.cost_gate_threshold_us;
            stats.branch_gate_passed = decision.branch_gate_passed;
            stats.cost_gate_passed = decision.cost_gate_passed;
            stats.used_parallel = false;
            stats.boolean_certificate = None;
            stats.qualified_consistency_certificate = None;
        }

        // Apply the one-shot static gate before selecting the execution path.
        if !decision.use_parallel {
            // Cost is too low - use sequential to avoid parallel overhead
            let start_time = Instant::now();
            let result = self.is_consistent_sequential();
            if let Ok(mut stats) = self.stats.lock() {
                stats.reasoning_time_ms = start_time.elapsed().as_millis() as u64;
            }
            return result;
        }

        // Legacy branches assert selected operands on an artificial witness.
        // An empty class need not have a witness: certify the original exact
        // Boolean fragment before that branch construction can change its truth.
        let certificate = check_boolean_consistency(&self.ontology)?;
        match certificate {
            Some(verdict) => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.boolean_certificate = Some(verdict);
                    stats.reasoning_time_ms = start_time.elapsed().as_millis() as u64;
                }
                return Ok(verdict);
            }
            None => {}
        }
        if let Ok(mut stats) = self.stats.lock() {
            stats.used_parallel = true;
        }

        // Reset flags
        self.shutdown.store(false, Ordering::SeqCst);
        self.solution_found.store(false, Ordering::SeqCst);

        // Initialize work queue and channels
        let work_queue = Worker::new_fifo();
        let (result_tx, result_rx) = unbounded();
        let (worker_done_tx, worker_done_rx) = unbounded();
        let outstanding_work = Arc::new(AtomicUsize::new(0));
        let branch_counter = Arc::new(AtomicUsize::new(1)); // branch 0 is root

        self.result_sender = Some(result_tx.clone());
        self.result_receiver = Some(result_rx);

        // Create stealers for all workers
        let mut stealers = Vec::new();
        for _ in 0..self.config.num_workers {
            stealers.push(work_queue.stealer());
        }
        self.stealers = Some(stealers.clone());

        // Wrap work queue in Arc<Mutex<>> for sharing
        let shared_queue = Arc::new(Mutex::new(work_queue));

        // Collect class expressions for nogood learning
        // Note: disjunctions already computed earlier for threshold check
        let mut test_expressions = HashSet::new();
        for axiom in self.ontology.axioms() {
            if let crate::logic::axioms::Axiom::SubClassOf(sub) = axiom.as_ref() {
                test_expressions.insert(sub.sub_class().clone());
                test_expressions.insert(sub.super_class().clone());
            }
        }

        let disjunctions = Arc::new(disjunctions);
        let work_items =
            self.create_branch_work_items(Arc::clone(&disjunctions), &test_expressions);

        // Push work items to queue
        for work_item in work_items {
            if let Ok(queue) = shared_queue.lock() {
                queue.push(work_item.clone());
            }
            outstanding_work.fetch_add(1, Ordering::SeqCst);

            if let Ok(mut stats) = self.stats.lock() {
                stats.branches_created += 1;
            }
        }

        // Use global rayon thread pool instead of spawning threads
        // This eliminates thread creation overhead (~200-300µs per call)
        let pool = &*SPACL_THREAD_POOL;
        let policy_engine = Arc::new(BranchPolicyEngine::new(&self.config));
        let snapshot_writer = self
            .config
            .branch_snapshot_path
            .as_deref()
            .and_then(|path| BranchSnapshotWriter::from_path(path).ok())
            .map(Arc::new);

        // Spawn workers on the thread pool after work is enqueued.
        // This avoids a race where workers observe outstanding_work == 0 and exit early.
        for worker_id in 0..self.config.num_workers {
            let stealer = stealers[worker_id].clone();
            let queue_ref = Arc::clone(&shared_queue);

            let nogoods = Arc::clone(&self.nogoods);
            let stats = Arc::clone(&self.stats);
            let shutdown = Arc::clone(&self.shutdown);
            let solution_found = Arc::clone(&self.solution_found);
            let result_tx = result_tx.clone();
            let worker_done_tx = worker_done_tx.clone();
            let ontology = Arc::clone(&self.ontology);
            let config = self.config.clone();
            let outstanding = Arc::clone(&outstanding_work);
            let branch_counter = Arc::clone(&branch_counter);
            let policy_engine = Arc::clone(&policy_engine);
            let snapshot_writer = snapshot_writer.as_ref().map(Arc::clone);

            // Spawn on global thread pool instead of creating new thread
            pool.spawn(move || {
                Self::worker_loop(
                    worker_id,
                    queue_ref,
                    stealer,
                    nogoods,
                    stats,
                    shutdown,
                    solution_found,
                    result_tx,
                    ontology,
                    config,
                    outstanding,
                    branch_counter,
                    policy_engine,
                    snapshot_writer,
                    worker_done_tx,
                );
            });
        }
        drop(worker_done_tx);

        // Store None since we're using thread pool (not managing threads directly)
        self.workers = None;

        // Wait for results (thread pool workers will check shutdown flag)
        let result = self.collect_results(&outstanding_work);

        // Signal workers to shutdown
        self.shutdown.store(true, Ordering::SeqCst);
        let completion_result =
            Self::wait_for_worker_completion(&worker_done_rx, self.config.num_workers);

        let elapsed = start_time.elapsed();
        if let Ok(mut stats) = self.stats.lock() {
            stats.reasoning_time_ms = elapsed.as_millis() as u64;
            // Calculate the legacy utilization proxy from completed branch work.
            if stats.branches_created > 0 {
                // This is not a paired measurement against sequential execution.
                let worker_count = self.config.num_workers;
                let completed = stats.successful_branches + stats.contradictions_found;
                if completed > 0 {
                    stats.speedup =
                        completed as f64 / (completed as f64 / worker_count as f64 + 1.0);
                }
            }
        }

        match (result, completion_result) {
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
            (Ok(value), Ok(())) => Ok(value),
        }
    }

    /// Worker thread main loop using SimpleReasoner
    fn worker_loop(
        worker_id: usize,
        shared_queue: Arc<Mutex<Worker<WorkItem>>>,
        stealer: Stealer<WorkItem>,
        nogoods: Arc<NogoodDatabase>,
        stats: Arc<Mutex<SpeculativeStats>>,
        shutdown: Arc<AtomicBool>,
        solution_found: Arc<AtomicBool>,
        result_tx: Sender<WorkResult>,
        ontology: Arc<Ontology>,
        config: SpeculativeConfig,
        outstanding_work: Arc<AtomicUsize>,
        branch_counter: Arc<AtomicUsize>,
        policy_engine: Arc<BranchPolicyEngine>,
        snapshot_writer: Option<Arc<BranchSnapshotWriter>>,
        worker_done_tx: Sender<()>,
    ) {
        if let Ok(mut s) = stats.lock() {
            s.workers_started += 1;
        }

        // Create thread-local nogood cache for reduced synchronization
        let mut local_cache = if config.enable_learning {
            Some(ThreadLocalNogoodCache::new(100)) // Sync every 100 checks
        } else {
            None
        };

        loop {
            if shutdown.load(Ordering::SeqCst) {
                // Flush local nogoods to global before exiting
                if let Some(ref mut cache) = local_cache {
                    cache.flush_to_global(&nogoods);
                }
                break;
            }

            // Check if solution already found by another worker
            if solution_found.load(Ordering::SeqCst) {
                // Early termination - another worker found SAT
                if let Some(ref mut cache) = local_cache {
                    cache.flush_to_global(&nogoods);
                }
                break;
            }

            // Try to get work
            let work_item = if let Ok(queue) = shared_queue.try_lock() {
                if let Some(local) = queue.pop() {
                    Some(local)
                } else {
                    if let Ok(mut s) = stats.lock() {
                        s.steal_attempts += 1;
                    }
                    let stolen = stealer.steal().success();
                    if stolen.is_some() {
                        if let Ok(mut s) = stats.lock() {
                            s.steal_successes += 1;
                        }
                    }
                    stolen
                }
            } else {
                if let Ok(mut s) = stats.lock() {
                    s.steal_attempts += 1;
                }
                let stolen = stealer.steal().success();
                if stolen.is_some() {
                    if let Ok(mut s) = stats.lock() {
                        s.steal_successes += 1;
                    }
                }
                stolen
            };

            let work_item = match work_item {
                Some(w) => w,
                None => {
                    if outstanding_work.load(Ordering::SeqCst) == 0 {
                        if let Some(ref mut cache) = local_cache {
                            cache.flush_to_global(&nogoods);
                        }
                        break;
                    }
                    thread::yield_now();
                    continue;
                }
            };

            // Check nogoods before processing (using thread-local cache)
            if config.enable_learning {
                let assertions = Self::branch_assertions(&work_item);

                // Track whether hit was from local or global cache
                let (conflict, from_local) = if let Some(ref mut cache) = local_cache {
                    let had_local = !cache.local_nogoods.is_empty();
                    let hit = cache.check_conflict(&nogoods, &assertions);
                    let from_local = hit.is_some() && had_local && cache.local_hits > 0;
                    (hit, from_local)
                } else {
                    (nogoods.check_conflict(&assertions), false)
                };

                if let Some(nogood) = conflict {
                    // Pruned by nogood
                    if let Ok(mut s) = stats.lock() {
                        s.branches_pruned += 1;
                        s.nogood_hits += 1;
                        s.nogood_checks += 1;
                        if from_local {
                            s.local_cache_hits += 1;
                        } else {
                            s.global_cache_hits += 1;
                        }
                    }
                    let previous = outstanding_work.fetch_sub(1, Ordering::SeqCst);
                    if previous == 1 {
                        let _ = result_tx.send(WorkResult::Contradiction {
                            branch_id: work_item.branch_id,
                            nogood,
                        });
                    }
                    continue;
                } else {
                    // No hit, but we still performed a check
                    if let Ok(mut s) = stats.lock() {
                        s.nogood_checks += 1;
                    }
                }
            }

            // Process the work item using SimpleReasoner
            let result = Self::process_work_item_simple(
                work_item,
                &ontology,
                &nogoods,
                &stats,
                &config,
                local_cache.as_mut(),
                worker_id,
                &branch_counter,
                policy_engine.as_ref(),
                snapshot_writer.as_ref(),
            );

            match result {
                WorkResult::Partial { new_work, .. } => {
                    // Parent item was expanded into child items.
                    if let Ok(mut s) = stats.lock() {
                        s.work_items_expanded += 1;
                        s.branches_created += new_work.len();
                    }

                    if let Ok(queue) = shared_queue.lock() {
                        for child in new_work {
                            outstanding_work.fetch_add(1, Ordering::SeqCst);
                            queue.push(child);
                        }
                    }
                    outstanding_work.fetch_sub(1, Ordering::SeqCst);
                }
                WorkResult::Success { .. } => {
                    solution_found.store(true, Ordering::SeqCst);
                    let _ = result_tx.send(result);
                    outstanding_work.fetch_sub(1, Ordering::SeqCst);
                }
                WorkResult::Contradiction { .. } => {
                    outstanding_work.fetch_sub(1, Ordering::SeqCst);
                    let _ = result_tx.send(result);
                }
            }
        }

        if let Ok(mut s) = stats.lock() {
            s.workers_completed += 1;
        }
        let _ = worker_done_tx.send(());
    }

    fn wait_for_worker_completion(
        worker_done_rx: &Receiver<()>,
        expected_workers: usize,
    ) -> OwlResult<()> {
        let mut completed = 0;

        while completed < expected_workers {
            // `speculative_timeout_ms` applies to result collection, not cleanup.
            // After shutdown, the caller drains every finite worker loop so a
            // timeout/error cannot return while old worker tasks still mutate
            // shared per-run state.
            match worker_done_rx.recv() {
                Ok(()) => completed += 1,
                Err(crossbeam::channel::RecvError) => {
                    return Err(OwlError::ReasoningError(format!(
                        "Speculative worker completion channel disconnected: completed {completed}/{expected_workers}"
                    )));
                }
            }
        }

        Ok(())
    }

    fn ontology_with_constraints(
        ontology: &Ontology,
        branch_id: BranchId,
        constraints: &[ClassExpression],
    ) -> Ontology {
        let mut branch_ontology = ontology.clone();
        if constraints.is_empty() {
            return branch_ontology;
        }

        let test_ind_str = format!("http://tableauxx.org/branch#test_{}", branch_id.0);
        let test_individual = crate::core::entities::NamedIndividual::new(test_ind_str.as_str());
        let _ = branch_ontology.add_named_individual(test_individual.clone());

        for constraint in constraints {
            let assertion = crate::logic::axioms::ClassAssertionAxiom::new(
                test_individual.iri().clone(),
                constraint.clone(),
            );
            let _ = branch_ontology.add_class_assertion(assertion);
        }

        branch_ontology
    }

    fn is_inconsistent_with_constraints(
        ontology: &Ontology,
        branch_id: BranchId,
        constraints: &[ClassExpression],
    ) -> bool {
        let branch_ontology = Self::ontology_with_constraints(ontology, branch_id, constraints);
        let reasoner = SimpleReasoner::new(branch_ontology);
        reasoner.is_consistent().map(|ok| !ok).unwrap_or(false)
    }

    fn minimize_conflicting_constraints(
        ontology: &Ontology,
        branch_id: BranchId,
        constraints: &[ClassExpression],
    ) -> Vec<ClassExpression> {
        if constraints.len() <= 1 {
            return constraints.to_vec();
        }

        let mut core = constraints.to_vec();
        let mut idx = 0;
        while idx < core.len() {
            let mut trial = core.clone();
            trial.remove(idx);

            // Never allow empty learned nogood from minimization.
            if trial.is_empty() {
                idx += 1;
                continue;
            }

            if Self::is_inconsistent_with_constraints(ontology, branch_id, &trial) {
                core = trial;
            } else {
                idx += 1;
            }
        }

        core
    }

    /// Build a pruning-authorized nogood from branch constraints.
    ///
    /// A nogood is admitted only if contradiction is explicitly re-validated
    /// after minimization. If no verified non-empty core is found, return empty.
    fn verified_nogood_from_constraints(
        ontology: &Ontology,
        branch_id: BranchId,
        constraints: &[ClassExpression],
    ) -> HashSet<ClassExpression> {
        let minimized = Self::minimize_conflicting_constraints(ontology, branch_id, constraints);
        if minimized.is_empty() {
            return HashSet::new();
        }

        if !Self::is_inconsistent_with_constraints(ontology, branch_id, &minimized) {
            return HashSet::new();
        }

        minimized.into_iter().collect()
    }

    /// Process a single work item using SimpleReasoner.
    fn process_work_item_simple(
        item: WorkItem,
        ontology: &Arc<Ontology>,
        nogoods: &Arc<NogoodDatabase>,
        stats: &Arc<Mutex<SpeculativeStats>>,
        config: &SpeculativeConfig,
        local_cache: Option<&mut ThreadLocalNogoodCache>,
        _worker_id: usize,
        branch_counter: &Arc<AtomicUsize>,
        policy_engine: &BranchPolicyEngine,
        snapshot_writer: Option<&Arc<BranchSnapshotWriter>>,
    ) -> WorkResult {
        if let Some(new_work) = Self::expand_work_item(
            &item,
            config.max_speculative_depth,
            branch_counter,
            policy_engine,
            stats,
            snapshot_writer,
        ) {
            if !new_work.is_empty() {
                return WorkResult::Partial {
                    branch_id: item.branch_id,
                    new_work,
                };
            }
        }

        // Evaluate this branch with all accumulated constraints.
        let branch_ontology =
            Self::ontology_with_constraints(ontology, item.branch_id, &item.constraints);
        let reasoner = SimpleReasoner::new(branch_ontology);
        let is_consistent = reasoner.is_consistent().unwrap_or(true);

        if !is_consistent {
            let nogood = if config.enable_learning {
                // Learn only verified branch-level cores for pruning safety.
                let nogood_assertions = Self::verified_nogood_from_constraints(
                    ontology,
                    item.branch_id,
                    &item.constraints,
                );
                let nogood = Nogood::new(nogood_assertions);

                // Add to local cache if available, otherwise to global (skip empty nogoods).
                if !nogood.assertions.is_empty() {
                    if let Some(cache) = local_cache {
                        cache.add_nogood(nogood.clone());
                    } else {
                        nogoods.add_nogood(nogood.clone());
                    }
                }
                nogood
            } else {
                Nogood::new(HashSet::new())
            };

            if let Ok(mut stats) = stats.lock() {
                stats.contradictions_found += 1;
                if config.enable_learning && !nogood.assertions.is_empty() {
                    stats.nogoods_learned += 1;
                }
            }

            return WorkResult::Contradiction {
                branch_id: item.branch_id,
                nogood,
            };
        }

        // Success - this branch is consistent
        if let Ok(mut stats) = stats.lock() {
            stats.successful_branches += 1;
        }

        WorkResult::Success {
            branch_id: item.branch_id,
        }
    }

    /// Collect and combine results from all workers
    fn collect_results(&self, outstanding_work: &Arc<AtomicUsize>) -> OwlResult<bool> {
        let receiver = self.result_receiver.as_ref().unwrap();
        let start = Instant::now();

        loop {
            if self.solution_found.load(Ordering::SeqCst) {
                return Ok(true);
            }

            if outstanding_work.load(Ordering::SeqCst) == 0 {
                return Ok(self.solution_found.load(Ordering::SeqCst));
            }

            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(WorkResult::Success { .. }) => {
                    self.solution_found.store(true, Ordering::SeqCst);
                    return Ok(true);
                }
                Ok(WorkResult::Contradiction { .. }) => {
                    // Continue until we either find SAT or exhaust all work.
                }
                Ok(WorkResult::Partial { .. }) => {
                    // Partial work is redistributed directly in worker loop.
                }
                Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                    if start.elapsed().as_millis() > self.config.speculative_timeout_ms as u128 {
                        return Err(OwlError::ReasoningError(
                            "Speculative reasoning timeout".to_string(),
                        ));
                    }
                }
                Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                    if outstanding_work.load(Ordering::SeqCst) == 0 {
                        return Ok(self.solution_found.load(Ordering::SeqCst));
                    }
                    return Err(OwlError::ReasoningError(
                        "Speculative worker channel disconnected before completion".to_string(),
                    ));
                }
            }
        }
    }

    /// Check class satisfiability
    pub fn is_satisfiable(&mut self, _class: &IRI) -> OwlResult<bool> {
        // Add class assertion and check consistency
        let test_ontology = (*self.ontology).clone();
        // Would add the class assertion here
        let mut reasoner = Self::with_config(test_ontology, self.config.clone());
        reasoner.is_consistent()
    }

    /// Get current statistics
    pub fn get_stats(&self) -> SpeculativeStats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Get nogood database stats
    pub fn get_nogood_stats(&self) -> (usize, usize) {
        self.nogoods.stats()
    }

    /// Enable/disable the fixed estimated-cost gate used by `Adaptive` mode
    ///
    /// This does not connect the standalone `AdaptiveTuner` to execution.
    pub fn set_adaptive_tuning(&mut self, enabled: bool) {
        self.config.adaptive_tuning = enabled;
    }

    /// Override the scheduling policy for ablations and controlled experiments.
    pub fn set_scheduling_mode(&mut self, mode: SchedulingMode) {
        self.config.scheduling_mode = mode;
    }

    /// Enable/disable nogood learning for ablations and controlled experiments.
    pub fn set_learning_enabled(&mut self, enabled: bool) {
        self.config.enable_learning = enabled;
    }
}

/// Disconnected experimental threshold helper using bounded hill climbing
///
/// This public utility is retained for compatibility. The active SPACL
/// execution path does not construct or call it.
pub struct AdaptiveTuner {
    /// Current parallelism threshold
    current_threshold: AtomicUsize,
    /// Performance history
    history: Arc<Mutex<Vec<(usize, f64)>>>, // (threshold, speedup)
}

impl AdaptiveTuner {
    pub fn new(initial_threshold: usize) -> Self {
        Self {
            current_threshold: AtomicUsize::new(initial_threshold),
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get current threshold
    pub fn get_threshold(&self) -> usize {
        self.current_threshold.load(Ordering::SeqCst)
    }

    /// Update threshold based on observed performance
    pub fn update(&self, speedup: f64) {
        let current = self.get_threshold();

        if let Ok(mut history) = self.history.lock() {
            history.push((current, speedup));

            // Simple hill-climbing adjustment
            if history.len() >= 5 {
                let recent: Vec<_> = history.iter().rev().take(5).collect();
                let avg_speedup: f64 = recent.iter().map(|(_, s)| s).sum::<f64>() / 5.0;

                // If speedup > 2, increase parallelism (lower threshold)
                // If speedup < 1, decrease parallelism (raise threshold)
                let adjustment = if avg_speedup > 2.0 {
                    -10i32
                } else if avg_speedup < 1.0 {
                    10i32
                } else {
                    0
                };

                let new_threshold = (current as i32 + adjustment).max(10).min(1000) as usize;
                self.current_threshold
                    .store(new_threshold, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::Class;
    use crate::core::ontology::Ontology;
    use crate::logic::axioms::{DisjointClassesAxiom, SubClassOfAxiom};
    use tempfile::NamedTempFile;

    #[test]
    fn test_nogood_creation() {
        let mut assertions = HashSet::new();
        assertions.insert(ClassExpression::Class(Class::new("http://example.org/A")));

        let nogood = Nogood::new(assertions);
        assert_eq!(nogood.size, 1);
        assert_eq!(nogood.hit_count, 0);
    }

    #[test]
    fn test_nogood_subsumption() {
        let mut assertions1 = HashSet::new();
        assertions1.insert(ClassExpression::Class(Class::new("http://example.org/A")));
        let nogood = Nogood::new(assertions1.clone());

        let mut assertions2 = HashSet::new();
        assertions2.insert(ClassExpression::Class(Class::new("http://example.org/A")));
        assertions2.insert(ClassExpression::Class(Class::new("http://example.org/B")));

        assert!(nogood.subsumes(&assertions2));
        assert!(nogood.subsumes(&assertions1)); // Equal sets - nogood still subsumes
    }

    #[test]
    fn last_pruned_work_item_notifies_result_collector() {
        let class_a = ClassExpression::Class(Class::new("http://example.org/A"));
        let class_b = ClassExpression::Class(Class::new("http://example.org/B"));

        let worker = Worker::new_fifo();
        let stealer = worker.stealer();
        for id in 0..99 {
            let mut test_expressions = HashSet::new();
            test_expressions.insert(class_b.clone());
            worker.push(WorkItem {
                branch_id: BranchId::new(id),
                constraints: Vec::new(),
                test_expressions,
                depth: 0,
                disjunctions: Arc::new(Vec::new()),
                next_disjunction_idx: 0,
            });
        }
        let mut pruned_assertions = HashSet::new();
        pruned_assertions.insert(class_a.clone());
        worker.push(WorkItem {
            branch_id: BranchId::new(99),
            constraints: Vec::new(),
            test_expressions: pruned_assertions.clone(),
            depth: 0,
            disjunctions: Arc::new(Vec::new()),
            next_disjunction_idx: 0,
        });
        let shared_queue = Arc::new(Mutex::new(worker));

        let nogoods = Arc::new(NogoodDatabase::new(16));
        nogoods.add_nogood(Nogood::new(pruned_assertions));

        let stats = Arc::new(Mutex::new(SpeculativeStats::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let solution_found = Arc::new(AtomicBool::new(false));
        let (result_tx, result_rx) = unbounded();
        let mut ontology = Ontology::new();
        let clash = Class::new("http://example.org/Clash");
        let other = Class::new("http://example.org/OtherClash");
        let individual =
            crate::core::entities::NamedIndividual::new("http://example.org/clash-witness");
        ontology.add_class(clash.clone()).unwrap();
        ontology.add_class(other.clone()).unwrap();
        ontology.add_named_individual(individual.clone()).unwrap();
        ontology
            .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                clash.iri().clone(),
                other.iri().clone(),
            ]))
            .unwrap();
        for class in [clash, other] {
            ontology
                .add_class_assertion(crate::logic::axioms::ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    ClassExpression::Class(class),
                ))
                .unwrap();
        }
        let ontology = Arc::new(ontology);
        let config = SpeculativeConfig::default();
        let outstanding_work = Arc::new(AtomicUsize::new(100));
        let branch_counter = Arc::new(AtomicUsize::new(1));
        let policy_engine = Arc::new(BranchPolicyEngine::new(&config));
        let (worker_done_tx, worker_done_rx) = unbounded();

        SpeculativeTableauxReasoner::worker_loop(
            0,
            shared_queue,
            stealer,
            Arc::clone(&nogoods),
            Arc::clone(&stats),
            shutdown,
            solution_found,
            result_tx,
            ontology,
            config,
            Arc::clone(&outstanding_work),
            branch_counter,
            policy_engine,
            None,
            worker_done_tx,
        );

        assert!(worker_done_rx.try_recv().is_ok());
        assert_eq!(outstanding_work.load(Ordering::SeqCst), 0);

        let pruned_result = result_rx.try_iter().find_map(|result| match result {
            WorkResult::Contradiction { branch_id, nogood } if branch_id == BranchId::new(99) => {
                Some(nogood)
            }
            _ => None,
        });
        assert!(
            pruned_result.is_some(),
            "last pruned work must notify collector"
        );
        assert_eq!(pruned_result.unwrap().size, 1);
    }

    #[test]
    fn test_speculative_config_default() {
        let config = SpeculativeConfig::default();
        assert!(config.enable_learning);
        assert_eq!(config.max_speculative_depth, 10);
        assert_eq!(config.scheduling_mode, SchedulingMode::Adaptive);
    }

    #[test]
    fn test_parallel_decision_respects_forced_modes() {
        let mut ontology = Ontology::new();
        let a = Class::new("http://example.org/A");
        let b = Class::new("http://example.org/B");
        let c = Class::new("http://example.org/C");
        ontology.add_class(a.clone()).unwrap();
        ontology.add_class(b.clone()).unwrap();
        ontology.add_class(c.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(a),
                ClassExpression::ObjectUnionOf(
                    vec![
                        Box::new(ClassExpression::Class(b)),
                        Box::new(ClassExpression::Class(c)),
                    ]
                    .into(),
                ),
            ))
            .unwrap();

        let mut config = SpeculativeConfig::default();
        config.parallel_threshold = usize::MAX;
        let reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config.clone());
        let disjunctions = reasoner.find_disjunctions();
        assert!(
            !reasoner
                .evaluate_parallel_decision(&disjunctions)
                .use_parallel
        );

        config.scheduling_mode = SchedulingMode::AlwaysParallel;
        let reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config.clone());
        assert!(
            reasoner
                .evaluate_parallel_decision(&disjunctions)
                .use_parallel
        );

        config.scheduling_mode = SchedulingMode::Sequential;
        let reasoner = SpeculativeTableauxReasoner::with_config(ontology, config);
        assert!(
            !reasoner
                .evaluate_parallel_decision(&disjunctions)
                .use_parallel
        );
    }

    #[test]
    fn test_expand_work_item_generates_all_operands() {
        let a = ClassExpression::Class(Class::new("http://example.org/A"));
        let b = ClassExpression::Class(Class::new("http://example.org/B"));
        let c = ClassExpression::Class(Class::new("http://example.org/C"));
        let disjunction = ClassExpression::ObjectUnionOf(
            vec![
                Box::new(a.clone()),
                Box::new(b.clone()),
                Box::new(c.clone()),
            ]
            .into(),
        );

        let mut test_expressions = HashSet::new();
        test_expressions.insert(a.clone());

        let item = WorkItem {
            branch_id: BranchId::new(0),
            constraints: Vec::new(),
            test_expressions,
            depth: 0,
            disjunctions: Arc::new(vec![disjunction]),
            next_disjunction_idx: 0,
        };
        let counter = AtomicUsize::new(1);
        let config = SpeculativeConfig::default();
        let policy_engine = BranchPolicyEngine::new(&config);
        let stats = Arc::new(Mutex::new(SpeculativeStats::default()));
        let expanded = SpeculativeTableauxReasoner::expand_work_item(
            &item,
            10,
            &counter,
            &policy_engine,
            &stats,
            None,
        )
        .expect("expansion");

        assert_eq!(expanded.len(), 3);
        for child in expanded {
            assert_eq!(child.constraints.len(), 1);
            assert_eq!(child.next_disjunction_idx, 1);
            assert_eq!(child.depth, 1);
        }
    }

    #[test]
    fn heuristic_policy_reorders_operands_and_updates_stats() {
        let a = ClassExpression::Class(Class::new("http://example.org/A"));
        let b = ClassExpression::Class(Class::new("http://example.org/B"));
        let c = ClassExpression::Class(Class::new("http://example.org/C"));
        let nested =
            ClassExpression::ObjectUnionOf(vec![Box::new(b.clone()), Box::new(c.clone())].into());

        let item = WorkItem {
            branch_id: BranchId::new(0),
            constraints: Vec::new(),
            test_expressions: HashSet::new(),
            depth: 0,
            disjunctions: Arc::new(vec![ClassExpression::ObjectUnionOf(
                vec![Box::new(a.clone()), Box::new(nested.clone())].into(),
            )]),
            next_disjunction_idx: 0,
        };

        let counter = AtomicUsize::new(1);
        let mut config = SpeculativeConfig::default();
        config.branch_policy = BranchPolicyMode::Heuristic;
        let policy_engine = BranchPolicyEngine::new(&config);
        let stats = Arc::new(Mutex::new(SpeculativeStats::default()));

        let expanded = SpeculativeTableauxReasoner::expand_work_item(
            &item,
            10,
            &counter,
            &policy_engine,
            &stats,
            None,
        )
        .expect("expansion");

        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].constraints.first(), Some(&nested));
        assert_eq!(stats.lock().unwrap().policy_reordered_splits, 1);
    }

    #[test]
    fn hybrid_rrn_mode_uses_safe_fallback_telemetry() {
        let a = ClassExpression::Class(Class::new("http://example.org/A"));
        let b = ClassExpression::Class(Class::new("http://example.org/B"));
        let c = ClassExpression::Class(Class::new("http://example.org/C"));

        let item = WorkItem {
            branch_id: BranchId::new(0),
            constraints: Vec::new(),
            test_expressions: HashSet::new(),
            depth: 0,
            disjunctions: Arc::new(vec![ClassExpression::ObjectUnionOf(
                vec![
                    Box::new(a.clone()),
                    Box::new(ClassExpression::ObjectUnionOf(
                        vec![Box::new(b), Box::new(c)].into(),
                    )),
                ]
                .into(),
            )]),
            next_disjunction_idx: 0,
        };

        let counter = AtomicUsize::new(1);
        let mut config = SpeculativeConfig::default();
        config.branch_policy = BranchPolicyMode::HybridRrn;
        let policy_engine = BranchPolicyEngine::new(&config);
        let stats = Arc::new(Mutex::new(SpeculativeStats::default()));

        let expanded = SpeculativeTableauxReasoner::expand_work_item(
            &item,
            10,
            &counter,
            &policy_engine,
            &stats,
            None,
        )
        .expect("expansion");
        assert_eq!(expanded.len(), 2);

        let snapshot = stats.lock().unwrap().clone();
        assert_eq!(snapshot.hybrid_policy_calls, 1);
        assert_eq!(snapshot.hybrid_model_calls, 0);
        assert_eq!(snapshot.policy_fallbacks, 1);
    }

    #[test]
    fn branch_snapshot_export_writes_jsonl() {
        let a = ClassExpression::Class(Class::new("http://example.org/A"));
        let b = ClassExpression::Class(Class::new("http://example.org/B"));

        let item = WorkItem {
            branch_id: BranchId::new(0),
            constraints: Vec::new(),
            test_expressions: HashSet::new(),
            depth: 0,
            disjunctions: Arc::new(vec![ClassExpression::ObjectUnionOf(
                vec![Box::new(a), Box::new(b)].into(),
            )]),
            next_disjunction_idx: 0,
        };

        let counter = AtomicUsize::new(1);
        let config = SpeculativeConfig::default();
        let policy_engine = BranchPolicyEngine::new(&config);
        let stats = Arc::new(Mutex::new(SpeculativeStats::default()));

        let snapshot_file = NamedTempFile::new().expect("temp file");
        let snapshot_path = snapshot_file.path().to_string_lossy().to_string();
        let writer = Arc::new(BranchSnapshotWriter::from_path(&snapshot_path).expect("writer"));

        let expanded = SpeculativeTableauxReasoner::expand_work_item(
            &item,
            10,
            &counter,
            &policy_engine,
            &stats,
            Some(&writer),
        )
        .expect("expansion");
        assert_eq!(expanded.len(), 2);

        let text = std::fs::read_to_string(&snapshot_path).expect("snapshot read");
        assert!(
            !text.trim().is_empty(),
            "snapshot file must contain at least one JSONL row"
        );
        let first_line = text.lines().next().expect("first line");
        let value: serde_json::Value = serde_json::from_str(first_line).expect("valid json");
        assert_eq!(value["branch_id"].as_u64(), Some(0));
        assert_eq!(value["operand_count"].as_u64(), Some(2));
        assert_eq!(
            value["operand_features"].as_array().map(|a| a.len()),
            Some(2)
        );
        assert_eq!(stats.lock().unwrap().branch_snapshots_written, 1);
    }

    #[test]
    fn test_adaptive_tuner() {
        let tuner = AdaptiveTuner::new(100);
        assert_eq!(tuner.get_threshold(), 100);

        tuner.update(2.5);
        // Threshold should decrease for good speedup
    }

    #[test]
    fn contradictory_branch_without_constraints_does_not_learn_fallback_nogood() {
        // Build a small ontology with a verified contradiction that does not
        // come from branch constraints.
        let mut ontology = Ontology::new();
        let a = Class::new("http://example.org/A");
        let b = Class::new("http://example.org/B");
        ontology.add_class(a.clone()).unwrap();
        ontology.add_class(b.clone()).unwrap();
        ontology
            .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                a.iri().clone(),
                b.iri().clone(),
            ]))
            .unwrap();
        let witness = Arc::new(IRI::new("http://example.org/witness").unwrap());
        for class in [&a, &b] {
            ontology
                .add_class_assertion(crate::logic::axioms::ClassAssertionAxiom::new(
                    witness.clone(),
                    ClassExpression::Class(class.clone()),
                ))
                .unwrap();
        }

        let item = WorkItem {
            branch_id: BranchId::new(0),
            constraints: Vec::new(),
            test_expressions: {
                let mut set = HashSet::new();
                set.insert(ClassExpression::Class(a));
                set.insert(ClassExpression::Class(b));
                set
            },
            depth: 0,
            disjunctions: Arc::new(Vec::new()),
            next_disjunction_idx: 0,
        };

        let nogoods = Arc::new(NogoodDatabase::new(16));
        let stats = Arc::new(Mutex::new(SpeculativeStats::default()));
        let config = SpeculativeConfig::default();
        let branch_counter = Arc::new(AtomicUsize::new(1));
        let policy_engine = BranchPolicyEngine::new(&config);

        let result = SpeculativeTableauxReasoner::process_work_item_simple(
            item,
            &Arc::new(ontology),
            &nogoods,
            &stats,
            &config,
            None,
            0,
            &branch_counter,
            &policy_engine,
            None,
        );

        match result {
            WorkResult::Contradiction { nogood, .. } => {
                assert!(
                    nogood.assertions.is_empty(),
                    "must not learn fallback nogood when no verified non-empty core exists"
                );
            }
            other => panic!("expected contradiction result, got {:?}", other),
        }

        let s = stats.lock().unwrap();
        assert_eq!(s.nogoods_learned, 0);
        assert_eq!(s.contradictions_found, 1);
    }
}
