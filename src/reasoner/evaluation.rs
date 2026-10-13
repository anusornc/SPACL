//! Machine-readable evaluation operations for benchmark adapters.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;

use crate::core::error::OwlError;
use crate::core::entities::Entity;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{
    Axiom, ClassExpression, ObjectPropertyExpression, PropertyAssertionObject,
};
use crate::logic::datatypes::value_space::{self, NumericInterval};

use super::simple::SimpleReasoner;
use super::speculative::{SpeculativeConfig, SpeculativeTableauxReasoner};

/// Stable response returned by the public evaluation contract.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct EvaluationResponse {
    pub schema_version: u32,
    pub operation: EvaluationOperation,
    pub status: EvaluationStatus,
    pub execution_status: EvaluationExecutionStatus,
    pub result_status: EvaluationResultStatus,
    pub correctness_status: EvaluationCorrectnessStatus,
    pub semantic_result: SemanticResult,
    pub input_ontology: String,
    pub conclusion_ontology: Option<String>,
    pub parse_time_ms: u128,
    pub reason_time_ms: Option<u128>,
    pub supported_scope: Option<String>,
    pub error: Option<String>,
    pub unsupported_fragment: Option<UnsupportedFragment>,
    pub runtime_metadata: RuntimeMetadata,
}

/// Evaluation operation exposed by the public contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum EvaluationOperation {
    Consistency,
    Inconsistency,
    Entailment,
    Unknown,
}

impl EvaluationOperation {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "consistency" => Self::Consistency,
            "inconsistency" => Self::Inconsistency,
            "entailment" => Self::Entailment,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Consistency => "consistency",
            Self::Inconsistency => "inconsistency",
            Self::Entailment => "entailment",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn is_supported(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// Execution status for an evaluation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    NotEvaluated,
    Passed,
    Wrong,
    Unsupported,
    NotAvailable,
    Failed,
    Timeout,
    OracleUnavailable,
}

impl EvaluationStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not_evaluated",
            Self::Passed => "passed",
            Self::Wrong => "wrong",
            Self::Unsupported => "unsupported",
            Self::NotAvailable => "not_available",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::OracleUnavailable => "oracle_unavailable",
        }
    }
}

/// Execution status for the native evaluation operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum EvaluationExecutionStatus {
    Success,
    Unsupported,
    NotAvailable,
    Failed,
    Timeout,
}

/// Overall result status before an external suite oracle is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum EvaluationResultStatus {
    NotEvaluated,
    Passed,
    Wrong,
    Unsupported,
    NotAvailable,
    Failed,
    Timeout,
    OracleUnavailable,
}

/// Correctness status before an external suite oracle is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCorrectnessStatus {
    NotEvaluated,
    Passed,
    Wrong,
    Unsupported,
    OracleUnavailable,
}

/// Semantic result produced by the reasoner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum SemanticResult {
    Consistent,
    Inconsistent,
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    Unknown,
    Error,
}

/// Identifies which side of an entailment request is outside the supported fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedFragmentSide {
    Operation,
    Premise,
    Conclusion,
}

/// Structured explanation for an unsupported operation or fragment.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct UnsupportedFragment {
    pub side: UnsupportedFragmentSide,
    pub reason: String,
}

impl SemanticResult {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Consistent => "consistent",
            Self::Inconsistent => "inconsistent",
            Self::True => "true",
            Self::False => "false",
            Self::Unknown => "unknown",
            Self::Error => "error",
        }
    }
}

/// Runtime information needed to reproduce an evaluation result.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct RuntimeMetadata {
    pub reasoner: String,
    pub source_revision: Option<String>,
    pub flags: BTreeMap<String, String>,
    pub resource_limits: BTreeMap<String, String>,
    pub source_metadata: BTreeMap<String, String>,
    pub supported_restrictions: Vec<String>,
    pub effective_config: BTreeMap<String, String>,
}

impl EvaluationResponse {
    pub fn completed(
        operation: EvaluationOperation,
        input_ontology: String,
        conclusion_ontology: Option<String>,
        semantic_result: SemanticResult,
        reason_time_ms: u128,
        supported_scope: Option<&str>,
        effective_config: Option<&SpeculativeConfig>,
    ) -> Self {
        Self {
            schema_version: 1,
            operation,
            status: EvaluationStatus::NotEvaluated,
            execution_status: EvaluationExecutionStatus::Success,
            result_status: EvaluationResultStatus::NotEvaluated,
            correctness_status: EvaluationCorrectnessStatus::NotEvaluated,
            semantic_result,
            input_ontology,
            conclusion_ontology,
            parse_time_ms: 0,
            reason_time_ms: Some(reason_time_ms),
            supported_scope: supported_scope.map(str::to_string),
            error: None,
            unsupported_fragment: None,
            runtime_metadata: runtime_metadata(
                if operation == EvaluationOperation::Entailment {
                    "SimpleReasoner"
                } else {
                    "SpeculativeTableauxReasoner"
                },
                supported_scope,
                effective_config,
            ),
        }
    }

    pub fn unsupported(
        operation: EvaluationOperation,
        input_ontology: String,
        conclusion_ontology: Option<String>,
        message: &str,
    ) -> Self {
        Self::unsupported_fragment(
            operation,
            input_ontology,
            conclusion_ontology,
            UnsupportedFragmentSide::Operation,
            message,
        )
    }

    pub fn unsupported_fragment(
        operation: EvaluationOperation,
        input_ontology: String,
        conclusion_ontology: Option<String>,
        side: UnsupportedFragmentSide,
        message: &str,
    ) -> Self {
        Self {
            schema_version: 1,
            operation,
            status: EvaluationStatus::Unsupported,
            execution_status: EvaluationExecutionStatus::Unsupported,
            result_status: EvaluationResultStatus::Unsupported,
            correctness_status: EvaluationCorrectnessStatus::Unsupported,
            semantic_result: SemanticResult::Unknown,
            input_ontology,
            conclusion_ontology,
            parse_time_ms: 0,
            reason_time_ms: None,
            supported_scope: None,
            error: Some(message.to_string()),
            unsupported_fragment: Some(UnsupportedFragment {
                side,
                reason: message.to_string(),
            }),
            runtime_metadata: runtime_metadata("evaluation_contract", None, None),
        }
    }

    pub fn failed(operation: EvaluationOperation, input_ontology: String, error: String) -> Self {
        Self {
            schema_version: 1,
            operation,
            status: EvaluationStatus::Failed,
            execution_status: EvaluationExecutionStatus::Failed,
            result_status: EvaluationResultStatus::Failed,
            correctness_status: EvaluationCorrectnessStatus::NotEvaluated,
            semantic_result: SemanticResult::Error,
            input_ontology,
            conclusion_ontology: None,
            parse_time_ms: 0,
            reason_time_ms: None,
            supported_scope: None,
            error: Some(error),
            unsupported_fragment: None,
            runtime_metadata: runtime_metadata("evaluation_contract", None, None),
        }
    }
}

fn runtime_metadata(
    reasoner: &str,
    supported_scope: Option<&str>,
    effective_config: Option<&SpeculativeConfig>,
) -> RuntimeMetadata {
    const FLAG_KEYS: &[&str] = &[
        "OWL2_REASONER_LARGE_PARSE",
        "OWL2_REASONER_FORCE_TEXT",
        "OWL2_REASONER_BIN_ONLY",
        "OWL2_REASONER_EXPERIMENTAL_XML_PARSER",
        "OWL2_REASONER_EXPERIMENTAL_XML_STRICT",
        "OWL2_REASONER_STRUCTURAL_XML_PARSER",
        "OWL2_REASONER_STRUCTURAL_XML_AUTO",
        "OWL2_REASONER_STRUCTURAL_XML_INTERNER",
        "OWL2_REASONER_STAGE_TIMING",
        "SPACL_SCHED_MODE",
        "SPACL_BRANCH_POLICY",
        "SPACL_NOGOOD",
    ];
    const RESOURCE_KEYS: &[&str] = &[
        "OWL2_REASONER_MAX_FILE_SIZE",
        "TIMEOUT_SECONDS",
        "RAYON_NUM_THREADS",
        "MEMORY_LIMIT_BYTES",
    ];

    let mut flags = BTreeMap::new();
    for key in FLAG_KEYS {
        flags.insert(
            (*key).to_string(),
            env::var(key).unwrap_or_else(|_| "unset".to_string()),
        );
    }

    let mut resource_limits = BTreeMap::new();
    for key in RESOURCE_KEYS {
        resource_limits.insert(
            (*key).to_string(),
            env::var(key).unwrap_or_else(|_| "unset".to_string()),
        );
    }

    let mut source_metadata = BTreeMap::new();
    source_metadata.insert("package".to_string(), env!("CARGO_PKG_NAME").to_string());
    source_metadata.insert(
        "package_version".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );
    if let Some(scope) = supported_scope {
        source_metadata.insert("supported_scope".to_string(), scope.to_string());
    }

    let mut effective_config_values = BTreeMap::new();
    if let Some(config) = effective_config {
        effective_config_values.insert("num_workers".to_string(), config.num_workers.to_string());
        effective_config_values.insert(
            "scheduling_mode".to_string(),
            format!("{:?}", config.scheduling_mode),
        );
        effective_config_values.insert(
            "branch_policy".to_string(),
            format!("{:?}", config.branch_policy),
        );
        effective_config_values.insert(
            "max_speculative_depth".to_string(),
            config.max_speculative_depth.to_string(),
        );
        effective_config_values.insert(
            "parallel_threshold".to_string(),
            config.parallel_threshold.to_string(),
        );
        effective_config_values.insert(
            "enable_learning".to_string(),
            config.enable_learning.to_string(),
        );
        effective_config_values.insert("max_nogoods".to_string(), config.max_nogoods.to_string());
        effective_config_values.insert(
            "speculative_timeout_ms".to_string(),
            config.speculative_timeout_ms.to_string(),
        );
        effective_config_values.insert(
            "adaptive_tuning".to_string(),
            config.adaptive_tuning.to_string(),
        );
        effective_config_values.insert(
            "cost_threshold_us".to_string(),
            config.cost_threshold_us.to_string(),
        );
        effective_config_values.insert(
            "cost_per_operand_us".to_string(),
            config.cost_per_operand_us.to_string(),
        );
        effective_config_values.insert(
            "cost_per_nesting_us".to_string(),
            config.cost_per_nesting_us.to_string(),
        );
    }

    RuntimeMetadata {
        reasoner: reasoner.to_string(),
        source_revision: env::var("SOURCE_REVISION")
            .ok()
            .or_else(|| env::var("GIT_COMMIT").ok()),
        flags,
        resource_limits,
        source_metadata,
        supported_restrictions: supported_scope
            .map(|scope| vec![scope.to_string()])
            .unwrap_or_default(),
        effective_config: effective_config_values,
    }
}

fn supports_named_subsumption_fragment(ontology: &Ontology) -> bool {
    ontology.axioms().iter().all(|axiom| match axiom.as_ref() {
        Axiom::SubClassOf(subclass_axiom) => matches!(
            (subclass_axiom.sub_class(), subclass_axiom.super_class()),
            (ClassExpression::Class(_), ClassExpression::Class(_))
        ),
        Axiom::AnnotationAssertion(_) => true,
        _ => false,
    })
}

/// Evaluate whether an ontology is consistent.
pub fn evaluate_consistency(
    ontology: Arc<Ontology>,
    input_ontology: impl Into<String>,
) -> EvaluationResponse {
    evaluate_consistency_with_config(ontology, input_ontology, SpeculativeConfig::default())
}

/// Evaluate consistency using an explicit effective speculative configuration.
pub fn evaluate_consistency_with_config(
    ontology: Arc<Ontology>,
    input_ontology: impl Into<String>,
    config: SpeculativeConfig,
) -> EvaluationResponse {
    let input_ontology = input_ontology.into();
    let start = Instant::now();
    // Recorded single-list oneOf frames lower here too (#155's
    // recorded hardening, #158): the consistency input plays the
    // premise role, so the members-only constraint presses in this
    // lane exactly as it does premise-side in entailment. An empty
    // channel costs nothing.
    let ontology = if ontology.object_enumeration_frames().is_empty() {
        ontology
    } else {
        let mut lowered = (*ontology).clone();
        apply_object_enumeration_frames(&mut lowered);
        Arc::new(lowered)
    };
    let mut reasoner = SpeculativeTableauxReasoner::with_config_arc(ontology, config.clone());

    match reasoner.is_consistent() {
        Ok(consistent) => EvaluationResponse::completed(
            EvaluationOperation::Consistency,
            input_ontology,
            None,
            if consistent {
                SemanticResult::Consistent
            } else {
                SemanticResult::Inconsistent
            },
            start.elapsed().as_millis(),
            Some("owl2_consistency"),
            Some(&config),
        ),
        Err(OwlError::UnsupportedFeature(message)) => EvaluationResponse::unsupported_fragment(
            EvaluationOperation::Consistency,
            input_ontology,
            None,
            UnsupportedFragmentSide::Operation,
            &message,
        ),
        Err(error) => EvaluationResponse::failed(
            EvaluationOperation::Consistency,
            input_ontology,
            error.to_string(),
        ),
    }
}

/// Evaluate whether an ontology is inconsistent.
pub fn evaluate_inconsistency(
    ontology: Arc<Ontology>,
    input_ontology: impl Into<String>,
) -> EvaluationResponse {
    evaluate_inconsistency_with_config(ontology, input_ontology, SpeculativeConfig::default())
}

/// Evaluate inconsistency using an explicit effective speculative configuration.
pub fn evaluate_inconsistency_with_config(
    ontology: Arc<Ontology>,
    input_ontology: impl Into<String>,
    config: SpeculativeConfig,
) -> EvaluationResponse {
    let mut response = evaluate_consistency_with_config(ontology, input_ontology, config);
    response.operation = EvaluationOperation::Inconsistency;
    if response.execution_status == EvaluationExecutionStatus::Success {
        response.semantic_result = if response.semantic_result == SemanticResult::Inconsistent {
            SemanticResult::True
        } else {
            SemanticResult::False
        };
    }
    response
}

/// Evaluate entailment by consistency reduction (#112): the premise
/// together with the negated conclusion is inconsistent exactly when the
/// conclusion is entailed. Every conclusion axiom kind maps to a
/// negated form — a complemented class assertion, a negated property
/// assertion, the complementary individual (in)equality, or a witness
/// instance for axiom-level conclusions (C ⊓ ¬D for subsumption) — and
/// each reduction runs through the same consistency lane the
/// consistency operation uses. A reduction the reasoner cannot decide
/// fails open: no inconsistency proof means no entailment claimed, so a
/// `false` is as sound as the consistency checker it rides on.
pub fn evaluate_entailment(
    premise: Arc<Ontology>,
    conclusion: Arc<Ontology>,
    input_ontology: impl Into<String>,
    conclusion_ontology: impl Into<String>,
) -> EvaluationResponse {
    let input_ontology = input_ontology.into();
    let conclusion_ontology_name = conclusion_ontology.into();
    let conclusion_ontology_for_error = Some(conclusion_ontology_name.clone());
    let start = Instant::now();

    // Domain and range axioms become role-membership GCIs —
    // rdfs:domain(p, C) is (exists p. Top) sqsubset C — so the
    // consistency lane sees them through the existing GCI machinery.
    let mut lowered_premise = lower_domain_range_axioms(&premise);

    // Recorded single-list oneOf frames lower PREMISE-side into the
    // bidirectional SubClassOf pair the multiple-enumeration flush
    // emits (#155): the frame pins A's members and puts them in A.
    // Conclusion-side frames stay unconsumed — lowering them
    // wholesale is what regressed unionOf-004 (#141), because the
    // parser cannot see premise/conclusion roles and this lane can.
    apply_object_enumeration_frames(&mut lowered_premise);

    let premise = Arc::new(lowered_premise);

    // Blank-node conclusions fold each cluster's typings and chain
    // edges into its root assertion's negation; the member axioms are
    // decided there and skipped below (#122).
    let consumed = consumed_blank_nodes(&conclusion);
    let mut entailed = true;
    for axiom in conclusion.axioms() {
        if mentions_consumed_blank_node(axiom.as_ref(), &consumed) {
            continue;
        }
        match EntailmentReduction::of(axiom.as_ref(), &premise, &conclusion) {
            ReductionOutcome::Entailed => {}
            ReductionOutcome::NotEntailed => {
                entailed = false;
            }
            ReductionOutcome::Unsupported(reason) => {
                return EvaluationResponse::unsupported_fragment(
                    EvaluationOperation::Entailment,
                    input_ontology,
                    conclusion_ontology_for_error,
                    UnsupportedFragmentSide::Conclusion,
                    &reason,
                );
            }
        }
    }

    EvaluationResponse::completed(
        EvaluationOperation::Entailment,
        input_ontology,
        Some(conclusion_ontology_name),
        if entailed {
            SemanticResult::True
        } else {
            SemanticResult::False
        },
        start.elapsed().as_millis(),
        Some("owl2_entailment_consistency_reduction"),
        None,
    )
}

/// How one conclusion axiom answers under the consistency reduction.
enum ReductionOutcome {
    Entailed,
    NotEntailed,
    Unsupported(String),
}

/// The negated form of one conclusion axiom: closure(s) that extend a
/// copy of the premise; every closure must force an inconsistency for
/// the axiom to count as entailed.
enum EntailmentReduction {
    /// Negated forms whose union with the premise must be inconsistent.
    Negations(Vec<Box<dyn FnOnce(&mut Ontology) + Send>>),
}

impl EntailmentReduction {
    fn of(axiom: &Axiom, premise: &Ontology, conclusion: &Ontology) -> ReductionOutcome {
        // A conclusion the premise already states is entailed outright.
        if premise
            .axioms()
            .iter()
            .any(|existing| existing.as_ref() == axiom)
        {
            return ReductionOutcome::Entailed;
        }
        match axiom {
            Axiom::ClassAssertion(assertion) => {
                // Vocabulary truth: XSD built-ins are rdfs:Datatype
                // instances in every interpretation (#118, W3C
                // I5.8-011).
                if let ClassExpression::Class(class) = assertion.class_expr() {
                    if class.iri().as_str() == RDFS_DATATYPE_IRI
                        && value_space::is_builtin_datatype_iri(assertion.individual().as_str())
                    {
                        return ReductionOutcome::Entailed;
                    }
                    // The conclusion document's `<owl:Ontology/>` header
                    // parses as an anonymous individual typed
                    // owl:Ontology — serialization, not a conclusion
                    // condition, so negating it must not sink the whole
                    // entailment (#133, W3C description-logic-661/665/
                    // 667's oiled headers). An explicit `<rdf:List>`
                    // element typing its own list node is the same kind
                    // of scaffolding (#140, W3C I5.5-005's singleton
                    // unionOf). A NAMED subject keeps the normal
                    // reduction: an ontology- or list-individual claim
                    // is a real assertion.
                    if (class.iri().as_str() == OWL_ONTOLOGY_IRI
                        || class.iri().as_str() == RDF_LIST_IRI)
                        && assertion.individual().as_str().starts_with("_:")
                    {
                        return ReductionOutcome::Entailed;
                    }
                }
                let individual = assertion.individual().clone();
                let negated = negate_class_expression(assertion.class_expr().clone());
                let assertion_individual = individual.clone();
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let _ = ontology.add_named_individual(
                        crate::core::entities::NamedIndividual::from_shared_iri(
                            assertion_individual,
                        ),
                    );
                    let _ = ontology.add_class_assertion(
                        crate::logic::axioms::ClassAssertionAxiom::new(individual, negated),
                    );
                })])
                .decide(premise)
            }
            Axiom::SubClassOf(axiom) => {
                let sub = axiom.sub_class().clone();
                let sup = axiom.super_class().clone();
                let definitions = premise_definitions(premise);
                Self::Negations(vec![negate_subclass(sub, sup, &definitions)]).decide(premise)
            }
            Axiom::EquivalentClasses(axiom) => {
                // Equivalence is the conjunction of both subsumption
                // directions; each needs its own witness.
                let classes: Vec<ClassExpression> = axiom
                    .classes()
                    .iter()
                    .map(|class| ClassExpression::Class(crate::core::entities::Class::from_shared_iri(class.clone())))
                    .collect();
                let definitions = premise_definitions(premise);
                let mut negations: Vec<Box<dyn FnOnce(&mut Ontology) + Send>> = Vec::new();
                for window in classes.windows(2) {
                    negations.push(negate_subclass(window[0].clone(), window[1].clone(), &definitions));
                    negations.push(negate_subclass(window[1].clone(), window[0].clone(), &definitions));
                }
                Self::Negations(negations).decide(premise)
            }
            Axiom::PropertyAssertion(assertion) => {
                if is_annotation_property(premise, assertion.property()) {
                    // An annotation assertion carries no model-theoretic
                    // content under Direct Semantics, so the conclusion
                    // axiom is satisfied in every interpretation — the
                    // W3C's own -Direct family tests exactly this
                    // inertness (I4.6-005-Direct: "the direct semantics
                    // ignore annotations in the conclusion ontology";
                    // #157, reversing #126's r31 boundary).
                    return ReductionOutcome::Entailed;
                }
                let subject = assertion.subject().clone();
                let property = assertion.property().clone();
                let Some(object) = assertion.object_iri().cloned() else {
                    // A blank-node object is an existential claim over
                    // the bnode's local formula (its typings plus the
                    // existentials along its outgoing edges); the
                    // negation is a universal complement over that
                    // formula (#122, W3C somevaluesfrom2bnode /
                    // allValuesFrom-002 / someValuesFrom-003). A
                    // single fresh ¬p edge would under-decide: no
                    // premise can name the anonymous witness.
                    if property.as_str() == OWL_SAME_AS_IRI {
                        return ReductionOutcome::Unsupported(
                            "blank-node sameAs conclusions are unsupported".to_string(),
                        );
                    }
                    let PropertyAssertionObject::Anonymous(inner) = assertion.object() else {
                        return ReductionOutcome::Unsupported(
                            "blank-node property assertion conclusions are unsupported"
                                .to_string(),
                        );
                    };
                    let Some(filler) = blank_node_complement(conclusion, inner.node_id(), 0)
                    else {
                        return ReductionOutcome::Unsupported(
                            "blank-node property assertion conclusions are unsupported"
                                .to_string(),
                        );
                    };
                    let property = ObjectPropertyExpression::ObjectProperty(Box::new(
                        crate::core::entities::ObjectProperty::from_shared_iri(property),
                    ));
                    return Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                        let _ = ontology.add_class_assertion(
                            crate::logic::axioms::ClassAssertionAxiom::new(
                                subject,
                                ClassExpression::ObjectAllValuesFrom(
                                    Box::new(property),
                                    Box::new(filler),
                                ),
                            ),
                        );
                    })])
                    .decide(premise);
                };
                // owl:sameAs stays a property assertion through the
                // parser (the closure lane's representation), but as a
                // conclusion it is an identity claim: its negation is
                // differentFrom, which the key rules and the
                // different/same clash checks read (#116, W3C
                // Keys-001/003).
                if property.as_str() == OWL_SAME_AS_IRI {
                    return Self::Negations(vec![negate_to_different(subject, object)])
                        .decide(premise);
                }
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let _ = ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                        crate::logic::axioms::NegativeObjectPropertyAssertionAxiom::new(
                            subject, property, object,
                        ),
                    )));
                })])
                .decide(premise)
            }
            Axiom::DataPropertyAssertion(assertion) => {
                if is_annotation_property(premise, assertion.property()) {
                    // Same inertness as the object lane above: an
                    // annotation's literal value constrains nothing
                    // under Direct Semantics (miscellaneous-302-Direct
                    // concludes "bar" from "foo" and expects true —
                    // #157).
                    return ReductionOutcome::Entailed;
                }
                let axiom =
                    crate::logic::axioms::NegativeDataPropertyAssertionAxiom::new(
                        assertion.subject().clone(),
                        assertion.property().clone(),
                        assertion.value().clone(),
                    );
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let _ = ontology
                        .add_axiom(Axiom::NegativeDataPropertyAssertion(Box::new(axiom)));
                })])
                .decide(premise)
            }
            Axiom::SameIndividual(axiom) => {
                let individuals: Vec<_> = axiom.individuals().to_vec();
                let mut negations: Vec<Box<dyn FnOnce(&mut Ontology) + Send>> = Vec::new();
                for index in 0..individuals.len() {
                    for other in index + 1..individuals.len() {
                        negations.push(negate_to_different(
                            individuals[index].clone(),
                            individuals[other].clone(),
                        ));
                    }
                }
                Self::Negations(negations).decide(premise)
            }
            Axiom::DifferentIndividuals(axiom) => {
                let individuals: Vec<_> = axiom.individuals().to_vec();
                let mut negations: Vec<Box<dyn FnOnce(&mut Ontology) + Send>> = Vec::new();
                for index in 0..individuals.len() {
                    for other in index + 1..individuals.len() {
                        let left = individuals[index].clone();
                        let right = individuals[other].clone();
                        negations.push(Box::new(move |ontology: &mut Ontology| {
                            let _ = ontology.add_axiom(Axiom::SameIndividual(Box::new(
                                crate::logic::axioms::SameIndividualAxiom::new(vec![
                                    left, right,
                                ]),
                            )));
                        }));
                    }
                }
                Self::Negations(negations).decide(premise)
            }
            Axiom::DisjointClasses(axiom) => {
                // Not-disjoint is a member of every pair at once.
                let classes: Vec<ClassExpression> = axiom
                    .classes()
                    .iter()
                    .map(|class| ClassExpression::Class(crate::core::entities::Class::from_shared_iri(class.clone())))
                    .collect();
                let conjunction = ClassExpression::ObjectIntersectionOf(
                    classes.into_iter().map(Box::new).collect(),
                );
                witness_reduction(conjunction).decide(premise)
            }
            Axiom::ObjectPropertyDomain(axiom) => {
                // A non-domain witness uses the property but sits outside C.
                let property = axiom.property().clone();
                let domain =
                    ClassExpression::Class(crate::core::entities::Class::from_shared_iri(
                        axiom.domain().clone(),
                    ));
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let (subject, object) = fresh_pair(ontology);
                    let _ = ontology.add_class_assertion(
                        crate::logic::axioms::ClassAssertionAxiom::new(
                            subject.iri().clone(),
                            ClassExpression::ObjectComplementOf(Box::new(domain)),
                        ),
                    );
                    let _ = ontology.add_property_assertion(
                        crate::logic::axioms::PropertyAssertionAxiom::new(
                            subject.iri().clone(),
                            property,
                            object.iri().clone(),
                        ),
                    );
                })])
                .decide(premise)
            }
            Axiom::ObjectPropertyRange(axiom) => {
                // Datatype-property ranges ride this axiom kind with
                // an XSD IRI as the range class; when both sides stay
                // inside the numeric lattice the value spaces decide
                // outright (#118, W3C I5.8-006/-008/-009). Anything
                // else falls through to the witness reduction.
                if let Some(verdict) = datatype_range_verdict(axiom, premise) {
                    return verdict;
                }
                // A non-range filler: an edge whose object is outside C.
                let property = axiom.property().clone();
                let range =
                    ClassExpression::Class(crate::core::entities::Class::from_shared_iri(
                        axiom.range().clone(),
                    ));
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let (subject, object) = fresh_pair(ontology);
                    let _ = ontology.add_class_assertion(
                        crate::logic::axioms::ClassAssertionAxiom::new(
                            object.iri().clone(),
                            ClassExpression::ObjectComplementOf(Box::new(range)),
                        ),
                    );
                    let _ = ontology.add_property_assertion(
                        crate::logic::axioms::PropertyAssertionAxiom::new(
                            subject.iri().clone(),
                            property,
                            object.iri().clone(),
                        ),
                    );
                })])
                .decide(premise)
            }
            Axiom::SubObjectProperty(axiom) => {
                // A non-subproperty witness: p-edge plus a negated q-edge.
                let sub = axiom.sub_property().clone();
                let sup = axiom.super_property().clone();
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let (subject, object) = fresh_pair(ontology);
                    let _ = ontology.add_property_assertion(
                        crate::logic::axioms::PropertyAssertionAxiom::new(
                            subject.iri().clone(),
                            sub,
                            object.iri().clone(),
                        ),
                    );
                    let _ = ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                        crate::logic::axioms::NegativeObjectPropertyAssertionAxiom::new(
                            subject.iri().clone(),
                            sup,
                            object.iri().clone(),
                        ),
                    )));
                })])
                .decide(premise)
            }
            Axiom::EquivalentObjectProperties(axiom) => {
                let properties: Vec<_> = axiom.properties().to_vec();
                let mut negations: Vec<Box<dyn FnOnce(&mut Ontology) + Send>> = Vec::new();
                for index in 0..properties.len() {
                    for other in index + 1..properties.len() {
                        let sub = properties[index].clone();
                        let sup = properties[other].clone();
                        negations.push(Box::new(move |ontology: &mut Ontology| {
                            let (subject, object) = fresh_pair(ontology);
                            let _ = ontology.add_property_assertion(
                                crate::logic::axioms::PropertyAssertionAxiom::new(
                                    subject.iri().clone(),
                                    sub,
                                    object.iri().clone(),
                                ),
                            );
                            let _ =
                                ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                                    crate::logic::axioms::NegativeObjectPropertyAssertionAxiom::new(
                                        subject.iri().clone(),
                                        sup,
                                        object.iri().clone(),
                                    ),
                                )));
                        }));
                    }
                }
                Self::Negations(negations).decide(premise)
            }
            Axiom::TransitiveProperty(axiom) => {
                // "Not transitive" is two p-edges whose joining edge is
                // missing: fresh x, y, z with p(x, y), p(y, z) and a
                // negated p(x, z). The consistency lane's chain
                // composition closes the triangle exactly when
                // p ∘ p ⊑ p holds (#117, W3C chain2trans1 and the
                // ObjectPropertyChain-BJP-004 non-entailment).
                let property = axiom.property().clone();
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let subject = fresh_individual(ontology, "transitivity-subject");
                    let middle = fresh_individual(ontology, "transitivity-middle");
                    let target = fresh_individual(ontology, "transitivity-object");
                    for (left, right) in [
                        (subject.iri().clone(), middle.iri().clone()),
                        (middle.iri().clone(), target.iri().clone()),
                    ] {
                        let _ = ontology.add_property_assertion(
                            crate::logic::axioms::PropertyAssertionAxiom::new(
                                left,
                                property.clone(),
                                right,
                            ),
                        );
                    }
                    let _ = ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                        crate::logic::axioms::NegativeObjectPropertyAssertionAxiom::new(
                            subject.iri().clone(),
                            property.clone(),
                            target.iri().clone(),
                        ),
                    )));
                })])
                .decide(premise)
            }
            Axiom::SymmetricProperty(axiom) => {
                // "Not symmetric" is one p-edge whose reverse is
                // missing: fresh x, y with p(x, y) and a negated
                // p(y, x). The consistency lane's symmetric closure
                // closes the pair exactly when the premise forces the
                // reverse edge (#121, the #117 transitivity pattern's
                // sibling; W3C WebOnt-SymmetricProperty-002).
                let property = axiom.property().clone();
                Self::Negations(vec![Box::new(move |ontology: &mut Ontology| {
                    let subject = fresh_individual(ontology, "symmetry-subject");
                    let object = fresh_individual(ontology, "symmetry-object");
                    let _ = ontology.add_property_assertion(
                        crate::logic::axioms::PropertyAssertionAxiom::new(
                            subject.iri().clone(),
                            property.clone(),
                            object.iri().clone(),
                        ),
                    );
                    let _ = ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                        crate::logic::axioms::NegativeObjectPropertyAssertionAxiom::new(
                            object.iri().clone(),
                            property.clone(),
                            subject.iri().clone(),
                        ),
                    )));
                })])
                .decide(premise)
            }
            _ => ReductionOutcome::Unsupported(format!(
                "conclusion axiom kind {:?} is unsupported by the entailment reduction",
                axiom.axiom_type()
            )),
        }
    }

    fn decide(self, premise: &Ontology) -> ReductionOutcome {
        match self {
            Self::Negations(negations) => {
                for negate in negations {
                    let mut extended = premise.clone();
                    negate(&mut extended);
                    match SimpleReasoner::new(extended).is_consistent() {
                        Ok(true) => return ReductionOutcome::NotEntailed,
                        Ok(false) => {}
                        Err(OwlError::UnsupportedFeature(message)) => {
                            return ReductionOutcome::Unsupported(message);
                        }
                        Err(error) => {
                            return ReductionOutcome::Unsupported(error.to_string());
                        }
                    }
                }
                ReductionOutcome::Entailed
            }
        }
    }
}

/// The negated form of an identity claim a = b: the pair is different.
fn negate_to_different(
    left: Arc<crate::core::iri::IRI>,
    right: Arc<crate::core::iri::IRI>,
) -> Box<dyn FnOnce(&mut Ontology) + Send> {
    Box::new(move |ontology: &mut Ontology| {
        let _ = ontology.add_axiom(Axiom::DifferentIndividuals(Box::new(
            crate::logic::axioms::DifferentIndividualsAxiom::new(vec![left, right]),
        )));
    })
}

/// The rdfs:Datatype class IRI — built-in datatype vocabulary
/// conclusions land as class assertions on it (#118).
const RDFS_DATATYPE_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";

/// The rdfs:Literal datatype IRI — the one data qualifier that is
/// provably trivial, since every data filler is a literal (#129).
const RDFS_LITERAL_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

/// The XSD-numeric verdict for a range conclusion: every premise
/// range of the same property bounds its extension, so the conclusion
/// holds exactly when the intersection of those bounds fits the
/// conclusion's datatype (#118, W3C I5.8-006/-008/-009). `None` when
/// either side leaves the numeric lattice or the premise states no
/// lattice range for the property — the witness reduction then keeps
/// today's behavior.
fn datatype_range_verdict(
    axiom: &crate::logic::axioms::ObjectPropertyRangeAxiom,
    premise: &Ontology,
) -> Option<ReductionOutcome> {
    let target = value_space::integer_interval(axiom.range().as_str())?;
    // No premise range means nothing bounds the extension; an
    // unconstrained property is not known to be integer-valued, so
    // the witness reduction keeps its say (never a free Entailed).
    let mut bounds: Option<NumericInterval> = None;
    for premise_axiom in premise.object_property_range_axioms() {
        if premise_axiom.property().as_ref() != axiom.property().as_ref() {
            continue;
        }
        let interval = value_space::integer_interval(premise_axiom.range().as_str())?;
        bounds = Some(match bounds {
            Some(existing) => existing.intersect(interval),
            None => interval,
        });
    }
    let bounds = bounds?;
    Some(if target.contains(bounds) {
        ReductionOutcome::Entailed
    } else {
        ReductionOutcome::NotEntailed
    })
}

/// The negated form of a conclusion expression, pushed one step into
/// the shapes the kernel decides (#129): a negated complement
/// collapses, a cardinality negation flips the bound (the qualifier
/// travels unchanged), an exact cardinality becomes the disjunction of
/// its two violated readings, an existential negates to the
/// universal over the negated filler (#136), and a universal negates
/// to the existential over the negated filler (#139). Every rewrite is a
/// logical equivalence; anything else keeps the single complement the
/// callers build today. A qualified DATA bound flips only under an
/// rdfs:Literal qualifier — provably trivial, since every data filler
/// is a literal (an xsd:string qualifier would drop non-string
/// fillers unsoundly).
fn negate_class_expression(expression: ClassExpression) -> ClassExpression {
    fn complement(inner: ClassExpression) -> ClassExpression {
        ClassExpression::ObjectComplementOf(Box::new(inner))
    }
    fn data_qualifier_trivial(
        qualifier: &Option<Box<crate::logic::axioms::class_expressions::DataRange>>,
    ) -> bool {
        match qualifier {
            None => true,
            Some(range) => matches!(
                range.as_ref(),
                crate::logic::axioms::class_expressions::DataRange::Datatype(datatype)
                    if datatype.as_str() == RDFS_LITERAL_IRI
            ),
        }
    }
    match expression {
        // not-(not X) is X.
        ClassExpression::ObjectComplementOf(inner) => *inner,
        // A singleton intersection is its one conjunct (the W3C
        // cardinality-004 conclusion wraps exactCard in one).
        ClassExpression::ObjectIntersectionOf(operands) if operands.len() == 1 => {
            negate_class_expression(*operands.into_iter().next().unwrap())
        }
        // not-(exists r.C) is forall r.(not C).
        ClassExpression::ObjectSomeValuesFrom(property, filler) => {
            ClassExpression::ObjectAllValuesFrom(
                property,
                Box::new(negate_class_expression(*filler)),
            )
        }
        // not-(forall r.C) is exists r.(not C) (#139, W3C I5.24-003's
        // range conclusion).
        ClassExpression::ObjectAllValuesFrom(property, filler) => {
            ClassExpression::ObjectSomeValuesFrom(
                property,
                Box::new(negate_class_expression(*filler)),
            )
        }
        // not-(>= n) is <= n-1; not-(<= n) is >= n+1.
        ClassExpression::ObjectMinCardinality(minimum, property, qualifier) => {
            if minimum > 0 {
                ClassExpression::ObjectMaxCardinality(minimum - 1, property, qualifier)
            } else {
                complement(ClassExpression::ObjectMinCardinality(minimum, property, qualifier))
            }
        }
        ClassExpression::ObjectMaxCardinality(maximum, property, qualifier) => {
            match maximum.checked_add(1) {
                Some(minimum) => {
                    ClassExpression::ObjectMinCardinality(minimum, property, qualifier)
                }
                None => complement(ClassExpression::ObjectMaxCardinality(
                    maximum, property, qualifier,
                )),
            }
        }
        // not-(= n) is >= n+1 OR <= n-1; exact 0 negates to the min
        // reading alone (the max reading is bottom).
        ClassExpression::ObjectExactCardinality(exact, property, qualifier) => {
            let min_reading = exact.checked_add(1).map(|minimum| {
                Box::new(ClassExpression::ObjectMinCardinality(
                    minimum,
                    property.clone(),
                    qualifier.clone(),
                ))
            });
            let max_reading = (exact > 0).then(|| {
                Box::new(ClassExpression::ObjectMaxCardinality(
                    exact - 1,
                    property.clone(),
                    qualifier.clone(),
                ))
            });
            match (min_reading, max_reading) {
                (Some(minimum), Some(maximum)) => {
                    ClassExpression::ObjectUnionOf(smallvec::smallvec![minimum, maximum])
                }
                (Some(minimum), None) => *minimum,
                // Only the bound's overflow lands here (u32::MAX);
                // today's single complement keeps the fail-open path.
                (None, _) => complement(ClassExpression::ObjectExactCardinality(
                    exact, property, qualifier,
                )),
            }
        }
        ClassExpression::DataExactCardinality(exact, property, qualifier) => {
            if !data_qualifier_trivial(&qualifier) {
                return complement(ClassExpression::DataExactCardinality(
                    exact, property, qualifier,
                ));
            }
            let min_reading = exact.checked_add(1).map(|minimum| {
                Box::new(ClassExpression::DataMinCardinality(
                    minimum,
                    property.clone(),
                    None,
                ))
            });
            let max_reading = (exact > 0).then(|| {
                Box::new(ClassExpression::DataMaxCardinality(
                    exact - 1,
                    property.clone(),
                    None,
                ))
            });
            match (min_reading, max_reading) {
                (Some(minimum), Some(maximum)) => {
                    ClassExpression::ObjectUnionOf(smallvec::smallvec![minimum, maximum])
                }
                (Some(minimum), None) => *minimum,
                (None, _) => complement(ClassExpression::DataExactCardinality(
                    exact, property, qualifier,
                )),
            }
        }
        ClassExpression::DataMinCardinality(minimum, property, qualifier) => {
            if data_qualifier_trivial(&qualifier) && minimum > 0 {
                ClassExpression::DataMaxCardinality(minimum - 1, property, None)
            } else {
                complement(ClassExpression::DataMinCardinality(minimum, property, qualifier))
            }
        }
        ClassExpression::DataMaxCardinality(maximum, property, qualifier) => {
            match (data_qualifier_trivial(&qualifier), maximum.checked_add(1)) {
                (true, Some(minimum)) => {
                    ClassExpression::DataMinCardinality(minimum, property, None)
                }
                _ => complement(ClassExpression::DataMaxCardinality(maximum, property, qualifier)),
            }
        }
        other => complement(other),
    }
}

/// The negated form of a subsumption C ⊑ D: a fresh witness in C ⊓ ¬D,
/// with the complement pushed into the kernel's decidable shapes.
/// Classes the premise defines through bidirectional SubClassOf (how
/// the parser lowers equivalences): C -> the defining expression D
/// with both `C ⊑ D` and `D ⊑ C` stated. One-directional subclasses
/// never appear — pushing a negation through them would be unsound
/// (#137).
fn premise_definitions(premise: &Ontology) -> BTreeMap<String, ClassExpression> {
    let mut down: Vec<(&str, &ClassExpression)> = Vec::new();
    let mut up: Vec<(&str, &ClassExpression)> = Vec::new();
    for axiom in premise.subclass_axioms() {
        match (axiom.sub_class(), axiom.super_class()) {
            (ClassExpression::Class(sub), sup) if !matches!(sup, ClassExpression::Class(_)) => {
                down.push((sub.iri().as_str(), sup))
            }
            (sub, ClassExpression::Class(sup)) if !matches!(sub, ClassExpression::Class(_)) => {
                up.push((sup.iri().as_str(), sub))
            }
            _ => {}
        }
    }
    let mut definitions = BTreeMap::new();
    for (name, expression) in down {
        // The two lowered directions of one equivalence are the same
        // expression — anything else is an unrelated coincidence of
        // shape, not a definition (derived equality; ClassExpression
        // and everything nested in it derive PartialEq).
        if up.iter().any(|(up_name, up_expression)| {
            up_name == &name && *up_expression == expression
        }) {
            definitions
                .entry(name.to_string())
                .or_insert_with(|| expression.clone());
        }
    }
    definitions
}

/// One premise-aware push: a negation that landed on a defined class
/// rewrites through its defining expression once (#137, W3C
/// equivalentClass-004); everything else keeps the pure-#129 shape.
fn negate_through_definitions(
    definitions: &BTreeMap<String, ClassExpression>,
    negated: ClassExpression,
) -> ClassExpression {
    if let ClassExpression::ObjectComplementOf(inner) = &negated {
        if let ClassExpression::Class(class) = inner.as_ref() {
            if let Some(definition) = definitions.get(class.iri().as_str()) {
                return negate_class_expression(definition.clone());
            }
        }
    }
    negated
}

fn negate_subclass(
    sub: ClassExpression,
    sup: ClassExpression,
    definitions: &BTreeMap<String, ClassExpression>,
) -> Box<dyn FnOnce(&mut Ontology) + Send> {
    let negated = negate_through_definitions(definitions, negate_class_expression(sup));
    Box::new(move |ontology: &mut Ontology| {
        let witness = fresh_individual(ontology, "subsumption-witness");
        let _ = ontology.add_class_assertion(crate::logic::axioms::ClassAssertionAxiom::new(
            witness.iri().clone(),
            ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                Box::new(sub),
                Box::new(negated),
            ]),
        ));
    })
}

/// A witness instance for an expression the premise must refute.
fn witness_reduction(expression: ClassExpression) -> EntailmentReduction {
    EntailmentReduction::Negations(vec![Box::new(move |ontology: &mut Ontology| {
        let witness = fresh_individual(ontology, "witness");
        let _ = ontology.add_class_assertion(crate::logic::axioms::ClassAssertionAxiom::new(
            witness.iri().clone(),
            expression,
        ));
    })])
}

/// How many conclusion property assertions point at one blank node.
fn incoming_blank_edges(conclusion: &Ontology, node: &str) -> usize {
    conclusion
        .property_assertions()
        .iter()
        .filter(|assertion| {
            matches!(assertion.object(), PropertyAssertionObject::Anonymous(inner)
                if inner.node_id() == node)
        })
        .count()
}

/// The chain depth a blank-node complement still folds; deeper
/// conclusions keep today's unsupported answer.
const BLANK_NODE_COMPLEMENT_DEPTH: usize = 8;

/// The complement of one blank node's local formula — the negation
/// filler for the universal that refutes the root assertion. Typings
/// contribute their complements (owl:Thing's is bottom and absorbs);
/// each outgoing edge contributes a universal over the target's own
/// complement (a named target becomes complement-of-oneOf). `None`
/// keeps the shape unsupported: chains past the depth bound, or a
/// blank node a second edge points at — a shared existential cannot
/// be negated per-axiom without claiming more than the premise
/// proves (#122).
fn blank_node_complement(
    conclusion: &Ontology,
    node: &str,
    depth: usize,
) -> Option<ClassExpression> {
    if depth == BLANK_NODE_COMPLEMENT_DEPTH {
        return None;
    }
    if incoming_blank_edges(conclusion, node) != 1 {
        return None;
    }
    use crate::core::entities::{Class, Individual, NamedIndividual, ObjectProperty};

    let mut disjuncts: Vec<Box<ClassExpression>> = Vec::new();
    for assertion in conclusion.class_assertions() {
        if assertion.individual().as_str() != node {
            continue;
        }
        if let ClassExpression::Class(class) = assertion.class_expr() {
            if class.iri().as_str() == OWL_THING_IRI {
                continue;
            }
        }
        disjuncts.push(Box::new(ClassExpression::ObjectComplementOf(Box::new(
            assertion.class_expr().clone(),
        ))));
    }
    for assertion in conclusion.property_assertions() {
        if assertion.subject().as_str() != node {
            continue;
        }
        let filler = match assertion.object() {
            PropertyAssertionObject::Anonymous(inner) => {
                blank_node_complement(conclusion, inner.node_id(), depth + 1)?
            }
            PropertyAssertionObject::Named(target) => {
                ClassExpression::ObjectComplementOf(Box::new(ClassExpression::ObjectOneOf(
                    Box::new(smallvec::smallvec![Individual::Named(
                        NamedIndividual::from_shared_iri(target.clone()),
                    )]),
                )))
            }
        };
        disjuncts.push(Box::new(ClassExpression::ObjectAllValuesFrom(
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::from_shared_iri(assertion.property().clone()),
            ))),
            Box::new(filler),
        )));
    }
    Some(match disjuncts.len() {
        0 => ClassExpression::ObjectComplementOf(Box::new(ClassExpression::Class(
            Class::owl_thing(),
        ))),
        1 => *disjuncts.remove(0),
        _ => ClassExpression::ObjectUnionOf(disjuncts.into_iter().collect()),
    })
}

/// Blank nodes folded into some root assertion's negation: the
/// objects of named-subject (root) assertions plus the nodes their
/// chains reach. Member axioms (typings, chain edges) are decided by
/// their root and skipped in the per-axiom loop (#122). Chains hang
/// off a named root only — a rootless cycle or orphan blank node is
/// never consumed, so its axioms still run per-axiom (and stay
/// unentailed) instead of silently passing as entailed. Guard
/// failures surface as Unsupported from the root's own reduction,
/// which short-circuits the evaluation before any skip could matter.
fn consumed_blank_nodes(conclusion: &Ontology) -> BTreeSet<String> {
    fn collect_chain(
        conclusion: &Ontology,
        node: &str,
        remaining: usize,
        consumed: &mut BTreeSet<String>,
    ) {
        if remaining == 0 || !consumed.insert(node.to_string()) {
            return;
        }
        for assertion in conclusion.property_assertions() {
            if assertion.subject().as_str() == node {
                if let PropertyAssertionObject::Anonymous(inner) = assertion.object() {
                    collect_chain(conclusion, inner.node_id(), remaining - 1, consumed);
                }
            }
        }
    }

    // Every blank node the conclusion names, as an object or as an
    // edge subject — a root's subject must stand outside this set.
    let blank_nodes: BTreeSet<&str> = conclusion
        .property_assertions()
        .iter()
        .filter_map(|assertion| match assertion.object() {
            PropertyAssertionObject::Anonymous(inner) => Some(inner.node_id()),
            PropertyAssertionObject::Named(_) => None,
        })
        .collect();

    let mut consumed = BTreeSet::new();
    for assertion in conclusion.property_assertions() {
        let is_root = match assertion.object() {
            PropertyAssertionObject::Anonymous(_) => {
                !blank_nodes.contains(assertion.subject().as_str())
            }
            PropertyAssertionObject::Named(_) => false,
        };
        if is_root {
            if let PropertyAssertionObject::Anonymous(inner) = assertion.object() {
                collect_chain(
                    conclusion,
                    inner.node_id(),
                    BLANK_NODE_COMPLEMENT_DEPTH,
                    &mut consumed,
                );
            }
        }
    }
    consumed
}

/// Whether the axiom only states structure of a consumed blank node
/// — decided by the cluster root, not on its own (#122).
fn mentions_consumed_blank_node(axiom: &Axiom, consumed: &BTreeSet<String>) -> bool {
    match axiom {
        Axiom::ClassAssertion(assertion) => {
            consumed.contains(assertion.individual().as_str())
        }
        Axiom::PropertyAssertion(assertion) => consumed.contains(assertion.subject().as_str()),
        _ => false,
    }
}

/// A fresh individual that collides with nothing in the ontology.
fn fresh_individual(ontology: &Ontology, role: &str) -> crate::core::entities::NamedIndividual {
    let mut counter = 0usize;
    loop {
        let candidate = crate::core::entities::NamedIndividual::new(format!(
            "urn:tableauxx:entailment:{role}:{counter}"
        ));
        if !ontology.contains_named_individual_iri(candidate.iri()) {
            return candidate;
        }
        counter += 1;
    }
}

/// A fresh (subject, object) pair for property-based witnesses.
fn fresh_pair(ontology: &Ontology) -> (crate::core::entities::NamedIndividual, crate::core::entities::NamedIndividual) {
    (
        fresh_individual(ontology, "subject"),
        fresh_individual(ontology, "object"),
    )
}

/// The individual a negated class assertion lands on: the conclusion's
/// own individual, re-created in the extended premise.
fn negation_subject(individual: &Arc<crate::core::iri::IRI>) -> Arc<crate::core::iri::IRI> {
    individual.clone()
}

/// The parser's representation of an identity assertion — owl:sameAs
/// rides the property-assertion lane for the closure engine, so the
/// kernel recognizes it by IRI (#116).
const OWL_SAME_AS_IRI: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_THING_IRI: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_ONTOLOGY_IRI: &str = "http://www.w3.org/2002/07/owl#Ontology";
const RDF_LIST_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#List";

/// Built-in annotation vocabulary: conclusions over these properties
/// are metadata entailments, not DL entailments (W3C I4.6-005,
/// sameAs-001).
const KNOWN_ANNOTATION_PROPERTIES: [&str; 9] = [
    "http://www.w3.org/2000/01/rdf-schema#comment",
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2000/01/rdf-schema#seeAlso",
    "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
    "http://www.w3.org/2002/07/owl#versionInfo",
    "http://www.w3.org/2002/07/owl#backwardCompatibleWith",
    "http://www.w3.org/2002/07/owl#incompatibleWith",
    "http://www.w3.org/2002/07/owl#priorVersion",
    "http://www.w3.org/2002/07/owl#deprecated",
];

/// Whether the IRI names an annotation property the premise declares —
/// built-in vocabulary, an annotation-property entity, or the RDF/XML
/// parser's vocabulary typing of a declaration. Conclusions over such
/// properties are metadata with no Direct-Semantics content: the
/// reduction reads them as satisfied in every interpretation (#157;
/// previously declined as unsupported, #126's r31 boundary, extended
/// from 0261/0270 to 0474/0495/0501 before that).
fn is_annotation_property(premise: &Ontology, property: &Arc<crate::core::iri::IRI>) -> bool {
    KNOWN_ANNOTATION_PROPERTIES.contains(&property.as_str())
        || premise
            .annotation_properties()
            .iter()
            .any(|candidate| candidate.iri().as_ref() == property.as_ref())
        || premise_types_annotation_property(premise, property)
}

/// Whether the premise types the IRI as owl:AnnotationProperty in the
/// RDF/XML parser's vocabulary-typing representation — declarations
/// never become annotation-property entities there.
fn premise_types_annotation_property(
    premise: &Ontology,
    property: &Arc<crate::core::iri::IRI>,
) -> bool {
    const OWL_ANNOTATION_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
    premise.class_assertions().iter().any(|assertion| {
        assertion.individual().as_ref() == property.as_ref()
            && matches!(assertion.class_expr(), ClassExpression::Class(class)
                if class.iri().as_str() == OWL_ANNOTATION_PROPERTY_IRI)
    })
}

/// Lower the premise's recorded single-list oneOf frames into the
/// bidirectional SubClassOf pair the parser's multiple-enumeration
/// flush emits (#155): A ⊑ oneOf{...} pins A's members to the
/// enumeration and oneOf{...} ⊑ A puts them in A. The channel is
/// role-agnostic — only this premise-side consumption reads it, so
/// conclusion documents keep their vacuous behavior. Rides the copy
/// `lower_domain_range_axioms` already made: an empty channel costs
/// nothing.
fn apply_object_enumeration_frames(premise: &mut Ontology) {
    use crate::core::entities::{Class, Individual, NamedIndividual};
    use smallvec::SmallVec;

    let frames: Vec<_> = premise.object_enumeration_frames().to_vec();
    for frame in frames {
        let mut members: SmallVec<[Individual; 8]> = SmallVec::new();
        for member in &frame.members {
            members.push(Individual::Named(NamedIndividual::from_shared_iri(member.clone())));
        }
        let named = ClassExpression::Class(Class::from_shared_iri(frame.class.clone()));
        let enumeration = ClassExpression::ObjectOneOf(Box::new(members));
        let _ = premise.add_subclass_axiom(crate::logic::axioms::SubClassOfAxiom::new(
            named.clone(),
            enumeration.clone(),
        ));
        let _ = premise.add_subclass_axiom(crate::logic::axioms::SubClassOfAxiom::new(
            enumeration, named,
        ));
    }
}

/// Copy the premise with rdfs:domain/range axioms lowered to
/// role-membership subclass axioms the consistency lane understands.
fn lower_domain_range_axioms(premise: &Ontology) -> Ontology {
    use crate::core::entities::{Class, Entity as _, ObjectProperty};
    let mut lowered = premise.clone();
    let mut additions: Vec<ClassExpression> = Vec::new();
    for axiom in premise.axioms() {
        match axiom.as_ref() {
            Axiom::ObjectPropertyDomain(axiom) => {
                let property = ObjectProperty::from_shared_iri(axiom.property().clone());
                additions.push(ClassExpression::ObjectSomeValuesFrom(
                    Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(property))),
                    Box::new(ClassExpression::Class(Class::new(
                        "http://www.w3.org/2002/07/owl#Thing",
                    ))),
                ));
                additions.push(ClassExpression::Class(Class::from_shared_iri(
                    axiom.domain().clone(),
                )));
            }
            Axiom::ObjectPropertyRange(axiom) => {
                let property = ObjectProperty::from_shared_iri(axiom.property().clone());
                additions.push(ClassExpression::ObjectSomeValuesFrom(
                    Box::new(ObjectPropertyExpression::ObjectInverseOf(Box::new(
                        ObjectPropertyExpression::ObjectProperty(Box::new(property)),
                    ))),
                    Box::new(ClassExpression::Class(Class::new(
                        "http://www.w3.org/2002/07/owl#Thing",
                    ))),
                ));
                additions.push(ClassExpression::Class(Class::from_shared_iri(
                    axiom.range().clone(),
                )));
            }
            _ => {}
        }
    }
    let mut index = 0;
    while index + 1 < additions.len() {
        let subject = additions[index].clone();
        let consequence = additions[index + 1].clone();
        let _ = lowered.add_subclass_axiom(crate::logic::axioms::SubClassOfAxiom::new(
            subject,
            consequence,
        ));
        index += 2;
    }
    lowered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::{Class, DataProperty, Literal, NamedIndividual};
    use crate::logic::axioms::{
        Axiom, DataPropertyAssertionAxiom, HasKeyAxiom, PropertyAssertionAxiom,
    };

    /// Black-box entailment through the public evaluation contract.
    fn entail(premise: Ontology, conclusion: Ontology) -> SemanticResult {
        evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        )
        .semantic_result
    }

    fn same_as_iri() -> Arc<crate::core::iri::IRI> {
        crate::core::iri::IRI::new_optimized(OWL_SAME_AS_IRI)
            .expect("owl:sameAs IRI is valid")
    }

    /// The Keys premise: P keyed on Thing (no scope) or on a class
    /// that a and b carry; every individual shares the value "v".
    fn key_premise(scope: Option<&str>) -> Ontology {
        let mut premise = Ontology::new();
        let a = NamedIndividual::new("http://example.org/a");
        let b = NamedIndividual::new("http://example.org/b");
        let outsider = NamedIndividual::new("http://example.org/outsider");
        for individual in [&a, &b, &outsider] {
            premise.add_named_individual(individual.clone()).unwrap();
        }
        let p = DataProperty::new("http://example.org/P");
        let key_class = match scope {
            Some(class) => {
                let scope = Class::new(class.to_string());
                for individual in [&a, &b] {
                    premise
                        .add_class_assertion(crate::logic::axioms::ClassAssertionAxiom::new(
                            individual.iri().clone(),
                            ClassExpression::Class(scope.clone()),
                        ))
                        .unwrap();
                }
                scope
            }
            None => Class::new("http://www.w3.org/2002/07/owl#Thing".to_string()),
        };
        premise
            .add_axiom(Axiom::HasKey(Box::new(HasKeyAxiom::new(
                key_class.iri().clone(),
                vec![p.iri().clone()],
            ))))
            .unwrap();
        for individual in [&a, &b, &outsider] {
            premise
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(
                        individual.iri().clone(),
                        p.iri().clone(),
                        Literal::simple("v"),
                    ),
                )))
                .unwrap();
        }
        premise
    }

    fn same_as_conclusion(
        left: &NamedIndividual,
        right: &NamedIndividual,
    ) -> Ontology {
        let mut conclusion = Ontology::new();
        conclusion
            .add_axiom(Axiom::PropertyAssertion(Box::new(
                PropertyAssertionAxiom::new(left.iri().clone(), same_as_iri(), right.iri().clone()),
            )))
            .unwrap();
        conclusion
    }

    /// #116 (W3C Keys-001): the RDF/XML parser keeps owl:sameAs as a
    /// property assertion (the closure lane's representation), so the
    /// entailment kernel must read that conclusion as an identity
    /// claim — negate it to differentFrom, which the key rule then
    /// blocks: both individuals share the key value.
    #[test]
    fn same_as_property_conclusion_is_identity_for_entailment() {
        let premise = key_premise(None);
        let a = NamedIndividual::new("http://example.org/a");
        let b = NamedIndividual::new("http://example.org/b");
        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(same_as_conclusion(&a, &b)),
            "premise.owl",
            "conclusion.owl",
        );
        assert!(
            matches!(response.semantic_result, SemanticResult::True),
            "same key tuple under a Thing-scoped key entails the identity: {:?}",
            response.semantic_result
        );
    }

    /// #116 (W3C Keys-003 guard): the identity is only forced where
    /// the key applies — with a key-class-scoped premise, the outsider
    /// shares the value but not the class, so its identity is NOT
    /// entailed.
    #[test]
    fn same_as_property_conclusion_respects_key_scope() {
        let premise = key_premise(Some("http://example.org/C"));
        let a = NamedIndividual::new("http://example.org/a");
        let outsider = NamedIndividual::new("http://example.org/outsider");
        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(same_as_conclusion(&a, &outsider)),
            "premise.owl",
            "conclusion.owl",
        );
        assert!(
            matches!(response.semantic_result, SemanticResult::False),
            "the outsider is outside the key class; the identity must not be entailed"
        );
    }

    /// Shared fixture helper: parse RDF/XML through the streaming
    /// parser (#161's dedup item — five nested modules carried this
    /// verbatim; reviewers accepted the sibling convention twice).
    #[cfg(feature = "rio-xml")]
    fn parse_ontology(content: &str) -> Ontology {
        let mut parser = crate::parser::rdf_xml_streaming::RdfXmlStreamingParser::new(
            crate::parser::ParserConfig::default(),
        );
        parser
            .parse_content(content)
            .expect("fixture RDF/XML must parse")
    }

    mod chains_117 {
        //! Fixtures for #117: property chains and transitivity
        //! conclusions in the entailment lane. Anchors: W3C
        //! New-Feature-ObjectPropertyChain-001 (0065), -BJP-003
        //! (0067), chain2trans1 (0004), -BJP-004 non-entailment
        //! (0069).

        use super::entail;
        use super::super::*;
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            PropertyAssertionAxiom, SubPropertyChainOfAxiom, TransitivePropertyAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn individual(ontology: &mut Ontology, name: &str) {
            ontology
                .add_named_individual(crate::core::entities::NamedIndividual::new(
                    format!("http://example.org/{name}"),
                ))
                .expect("individual registers");
        }

        fn edge(ontology: &mut Ontology, subject: &str, property: &str, object: &str) {
            let _ = ontology.add_property_assertion(PropertyAssertionAxiom::new(
                iri(subject),
                iri(property),
                iri(object),
            ));
        }

        fn chain(ontology: &mut Ontology, first: &str, second: &str, super_property: &str) {
            let _ = ontology.add_axiom(Axiom::SubPropertyChainOf(Box::new(
                SubPropertyChainOfAxiom::new(vec![iri(first), iri(second)], iri(super_property)),
            )));
        }

        fn assertion_conclusion(subject: &str, property: &str, object: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            individual(&mut conclusion, subject);
            individual(&mut conclusion, object);
            edge(&mut conclusion, subject, property, object);
            conclusion
        }

        fn transitive_conclusion(property: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            let _ = conclusion.add_axiom(Axiom::TransitiveProperty(Box::new(
                TransitivePropertyAxiom::new(iri(property)),
            )));
            conclusion
        }

        /// ObjectPropertyChain-001: hasMother ∘ hasSister ⊑ hasAunt
        /// with both steps asserted composes the super-property edge.
        #[test]
        fn chain_composes_asserted_steps_into_super_property() {
            let mut premise = Ontology::new();
            for name in ["Stewie", "Lois", "Carol"] {
                individual(&mut premise, name);
            }
            edge(&mut premise, "Stewie", "hasMother", "Lois");
            edge(&mut premise, "Lois", "hasSister", "Carol");
            chain(&mut premise, "hasMother", "hasSister", "hasAunt");

            let result = entail(
                premise,
                assertion_conclusion("Stewie", "hasAunt", "Carol"),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "the chain composes hasAunt(Stewie, Carol): {result:?}"
            );
        }

        /// Without the first step the chain derives nothing: Lois has
        /// no hasMother edge, so hasAunt(Lois, Carol) stays open.
        #[test]
        fn chain_does_not_compose_without_the_first_step() {
            let mut premise = Ontology::new();
            for name in ["Stewie", "Lois", "Carol"] {
                individual(&mut premise, name);
            }
            edge(&mut premise, "Stewie", "hasMother", "Lois");
            edge(&mut premise, "Lois", "hasSister", "Carol");
            chain(&mut premise, "hasMother", "hasSister", "hasAunt");

            let result = entail(premise, assertion_conclusion("Lois", "hasAunt", "Carol"));
            assert!(
                matches!(result, SemanticResult::False),
                "no composed edge for Lois: {result:?}"
            );
        }

        /// BJP-003 plus a longer path: p ∘ p ⊑ p composes a→b→c→d
        /// into a→d through the fixpoint (chains of chains).
        #[test]
        fn self_composing_chain_reaches_transitive_edges() {
            let mut premise = Ontology::new();
            for name in ["a", "b", "c", "d"] {
                individual(&mut premise, name);
            }
            edge(&mut premise, "a", "p", "b");
            edge(&mut premise, "b", "p", "c");
            edge(&mut premise, "c", "p", "d");
            chain(&mut premise, "p", "p", "p");

            let result = entail(premise, assertion_conclusion("a", "p", "d"));
            assert!(
                matches!(result, SemanticResult::True),
                "the fixpoint composes p(a, d): {result:?}"
            );
        }

        /// chain2trans1: p ∘ p ⊑ p is transitivity stated as a chain,
        /// so the property-characteristic conclusion is entailed.
        #[test]
        fn transitive_property_conclusion_follows_from_self_chain() {
            let mut premise = Ontology::new();
            chain(&mut premise, "p", "p", "p");

            let result = entail(premise, transitive_conclusion("p"));
            assert!(
                matches!(result, SemanticResult::True),
                "p ∘ p ⊑ p makes p transitive: {result:?}"
            );
        }

        /// BJP-004 guard: p ∘ q ⊑ p with only q-edges does not make p
        /// transitive — the conclusion must stay unentailed.
        #[test]
        fn transitive_property_conclusion_rejects_mixed_chain() {
            let mut premise = Ontology::new();
            for name in ["a", "b", "c"] {
                individual(&mut premise, name);
            }
            edge(&mut premise, "a", "q", "b");
            edge(&mut premise, "b", "q", "c");
            chain(&mut premise, "p", "q", "p");

            let result = entail(premise, transitive_conclusion("p"));
            assert!(
                matches!(result, SemanticResult::False),
                "q-edges alone never compose a p-step: {result:?}"
            );
        }
    }

    mod symmetry_121 {
        //! Fixtures for #121: SymmetricProperty conclusions in the
        //! entailment lane. The negation is the #117 transitivity
        //! pattern's sibling — one p-edge whose reverse is missing —
        //! and the consistency lane's symmetric closure refutes it
        //! exactly when the premise forces the reverse edge. Anchor:
        //! W3C WebOnt-SymmetricProperty-002 (0323).

        use super::entail;
        use super::super::*;
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            EquivalentObjectPropertiesAxiom, PropertyAssertionAxiom,
            SubObjectPropertyAxiom, SymmetricPropertyAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn symmetric(ontology: &mut Ontology, property: &str) {
            let _ = ontology.add_axiom(Axiom::SymmetricProperty(Box::new(
                SymmetricPropertyAxiom::new(iri(property)),
            )));
        }

        fn equivalent(ontology: &mut Ontology, left: &str, right: &str) {
            let _ = ontology.add_axiom(Axiom::EquivalentObjectProperties(Box::new(
                EquivalentObjectPropertiesAxiom::new(vec![iri(left), iri(right)]),
            )));
        }

        fn subproperty(ontology: &mut Ontology, sub: &str, sup: &str) {
            let _ = ontology.add_axiom(Axiom::SubObjectProperty(Box::new(
                SubObjectPropertyAxiom::new(iri(sub), iri(sup)),
            )));
        }

        fn edge(ontology: &mut Ontology, subject: &str, property: &str, object: &str) {
            let _ = ontology.add_property_assertion(PropertyAssertionAxiom::new(
                iri(subject),
                iri(property),
                iri(object),
            ));
        }

        fn symmetric_conclusion(property: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            symmetric(&mut conclusion, property);
            conclusion
        }

        /// A property equivalent to a symmetric one is symmetric:
        /// q(x, y) climbs to p, symmetry reverses it, and the climb
        /// back down to q clashes the negated reverse edge. Today the
        /// conclusion kind is unsupported outright.
        #[test]
        fn symmetric_property_conclusion_follows_from_equivalent_symmetry() {
            let mut premise = Ontology::new();
            symmetric(&mut premise, "p");
            equivalent(&mut premise, "p", "q");

            let result = entail(premise, symmetric_conclusion("q"));
            assert!(
                matches!(result, SemanticResult::True),
                "p ≡ q and p symmetric make q symmetric: {result:?}"
            );
        }

        /// A property nothing links to the symmetric one stays
        /// undecided: the witness pair keeps a model, so the verdict
        /// must be false — not unsupported.
        #[test]
        fn symmetric_property_conclusion_rejects_unlinked_property() {
            let mut premise = Ontology::new();
            symmetric(&mut premise, "p");
            edge(&mut premise, "a", "q", "b");

            let result = entail(premise, symmetric_conclusion("q"));
            assert!(
                matches!(result, SemanticResult::False),
                "no premise forces q's reverse edges: {result:?}"
            );
        }

        /// Subproperty of a symmetric property is not symmetric: the
        /// closure may only label reverses with the declared
        /// symmetric property, never with a subproperty's name.
        #[test]
        fn symmetric_property_conclusion_rejects_subproperty_of_symmetric() {
            let mut premise = Ontology::new();
            symmetric(&mut premise, "p");
            subproperty(&mut premise, "q", "p");
            edge(&mut premise, "a", "q", "b");

            let result = entail(premise, symmetric_conclusion("q"));
            assert!(
                matches!(result, SemanticResult::False),
                "q ⊑ p with p symmetric leaves q's symmetry open: {result:?}"
            );
        }
    }

    mod enumeration_frames_155 {
        //! Fixtures for #155: single-list owl:oneOf frames on named
        //! classes, recorded at parse into the ontology's side channel
        //! and lowered PREMISE-side by the entailment lane — the
        //! #109 narrow gate keeps them out of the axiom stream, and
        //! the roles the parser cannot see are distinct here: the
        //! premise's frame becomes nominal constraints while a
        //! conclusion's channel entry stays unconsumed (unionOf-004's
        //! vacuous pass must survive). Anchor: W3C
        //! WebOnt-SymmetricProperty-002 (0323); bonus
        //! rdfbased-sem-enum-inst-included (0140) rides the same
        //! lowering through the members-are-in-the-class direction.
        use super::parse_ontology;

        use super::entail;
        use super::super::*;
        use crate::core::entities::Class;
        use crate::core::iri::IRI;
        use crate::logic::axioms::ClassAssertionAxiom;

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        #[cfg(feature = "rio-xml")]
        /// 0323's premise, inlined: the IFP's range names A, whose
        /// single-list oneOf{a,b} frame reaches the kernel only
        /// through the recorded channel. Dropping the oneOf element
        /// yields the no-frame control.
        #[cfg(feature = "rio-xml")]
        fn symmetric_property_premise(frame: bool) -> Ontology {
            let one_of = if frame {
                r#"<owl:oneOf rdf:parseType="Collection">
          <owl:Thing rdf:about="http://example.org/a"/>
          <owl:Thing rdf:about="http://example.org/b"/>
        </owl:oneOf>"#
            } else {
                ""
            };
            parse_ontology(&format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:ex="http://example.org/">
  <owl:InverseFunctionalProperty rdf:about="http://example.org/equalityOnA">
    <rdfs:range>
      <owl:Class rdf:about="http://example.org/A">{one_of}</owl:Class>
    </rdfs:range>
  </owl:InverseFunctionalProperty>
  <owl:Thing rdf:about="http://example.org/a">
    <ex:equalityOnA rdf:resource="http://example.org/a"/>
  </owl:Thing>
  <owl:Thing rdf:about="http://example.org/b">
    <ex:equalityOnA rdf:resource="http://example.org/b"/>
  </owl:Thing>
  <owl:Thing rdf:about="http://example.org/c"/>
</rdf:RDF>"#
            ))
        }

        /// 0323's conclusion, inlined: symmetry plus the domain
        /// enumeration oneOf{a,b,c} and the (a,a) assertion.
        #[cfg(feature = "rio-xml")]
        fn symmetric_domain_conclusion() -> Ontology {
            parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:ex="http://example.org/">
  <owl:SymmetricProperty rdf:about="http://example.org/equalityOnA">
    <rdfs:domain>
      <owl:Class>
        <owl:oneOf rdf:parseType="Collection">
          <owl:Thing rdf:about="http://example.org/a"/>
          <owl:Thing rdf:about="http://example.org/b"/>
          <owl:Thing rdf:about="http://example.org/c"/>
        </owl:oneOf>
      </owl:Class>
    </rdfs:domain>
  </owl:SymmetricProperty>
  <owl:Thing rdf:about="http://example.org/a">
    <ex:equalityOnA rdf:resource="http://example.org/a"/>
  </owl:Thing>
</rdf:RDF>"#,
            )
        }

        /// The frame is the missing link: every equalityOnA-object
        /// sits in the range A, the lowered A ⊑ {a,b} pins it to a
        /// nominal, and the IFP then identifies each subject with
        /// the assertion's subject — so the domain enumeration and
        /// symmetry conclusions close (W3C 0323, #155).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn inlined_frame_premise_entails_symmetric_domain() {
            let result = entail(symmetric_property_premise(true), symmetric_domain_conclusion());
            assert!(
                matches!(result, SemanticResult::True),
                "oneOf frame + IFP range must force the symmetric domain: {result:?}"
            );
        }

        /// Without the frame the channel is empty and the premise
        /// leaves the domain enumeration open — today's answer.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn inlined_frameless_premise_keeps_domain_open() {
            let result = entail(symmetric_property_premise(false), symmetric_domain_conclusion());
            assert!(
                matches!(result, SemanticResult::False),
                "no enumeration means no forced domain membership: {result:?}"
            );
        }

        /// 0140's shape: the frame alone must put its members in the
        /// enumerated class ({a,b} ⊑ A direction of the lowering).
        #[test]
        fn recorded_frame_members_are_in_the_class() {
            let mut premise = Ontology::new();
            premise.record_object_enumeration_frame(iri("A"), vec![iri("a"), iri("b")]);

            let mut conclusion = Ontology::new();
            let _ = conclusion.add_axiom(Axiom::ClassAssertion(Box::new(
                ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::Class(Class::from_shared_iri(iri("A"))),
                ),
            )));

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "enumeration members belong to the enumerated class: {result:?}"
            );
        }
    }

    mod complement_pairs_156 {
        //! Fixtures for #156 (ADR 0021): cardinality complement pairs
        //! — X ≡ (p carries at most 0 values) with X.comp ≡ (p
        //! carries at least 1), one shared property — are exact
        //! complements under full OWL semantics. The consistency
        //! lane registers the pairs at index build, flips a recorded
        //! class into a complement note, reads the pairs inside
        //! classes_disjoint, and closes the fact1.1 shape (an
        //! instance of a class whose every exposed disjunct embeds a
        //! disjoint pair) through a bounded contrapositive assist.
        //! Anchor: W3C description-logic-601 (0404), the last
        //! baseline-wrong case; HermiT pins the reference
        //! (InconsistentOntologyException).
        use super::parse_ontology;

        use super::super::*;
        use crate::core::ontology::Ontology;

        fn consistent(ontology: Ontology) -> bool {
            matches!(
                evaluate_consistency(Arc::new(ontology), "fixture.owl").semantic_result,
                SemanticResult::Consistent
            )
        }

        #[cfg(feature = "rio-xml")]
        /// The disjointness half of one pair: an individual in both
        /// sides of X/X.comp clashes through the two cardinality
        /// definitions (works pre-#156 — the green anchor).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn pair_disjointness_half_clashes() {
            let ontology = parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="http://example.org/b">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/P3"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#unsignedByte">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/b.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P3"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#unsignedByte">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <rdf:Description rdf:about="http://example.org/x">
    <rdf:type rdf:resource="http://example.org/b"/>
    <rdf:type rdf:resource="http://example.org/b.comp"/>
  </rdf:Description>
</rdf:RDF>"#,
            );
            assert!(
                !consistent(ontology),
                "x in both pair halves must clash (disjointness half)"
            );
        }

        /// description-logic-601's fact1.1 shape, inlined: a, b, c
        /// pairwise disjoint through the complement machinery, and an
        /// instance of Unsatisfiable, whose exposed normal form is
        /// (a⊓b) ⊔ (a⊓c) ⊔ (b⊓c) — every disjunct embeds a disjoint
        /// pair, so the ontology is inconsistent.
        #[cfg(feature = "rio-xml")]
        fn fact_1_11_ontology(instance: bool, b_below_c_comp: bool) -> Ontology {
            let instance_element = if instance {
                r#"<rdf:Description rdf:about="http://example.org/x">
    <rdf:type rdf:resource="http://example.org/Unsat"/>
  </rdf:Description>"#
            } else {
                ""
            };
            let b_sub = if b_below_c_comp {
                r#"<rdfs:subClassOf rdf:resource="http://example.org/c.comp"/>"#
            } else {
                ""
            };
            parse_ontology(&format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:Class rdf:about="http://example.org/a">
    <rdfs:subClassOf>
      <owl:Class rdf:about="http://example.org/C1"/>
    </rdfs:subClassOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C1">
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/b.comp"/>
      <owl:Class rdf:about="http://example.org/c.comp"/>
    </owl:intersectionOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/b">
    {b_sub}
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/P3"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#unsignedByte">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/b.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P3"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#unsignedByte">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/c">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:ObjectProperty rdf:about="http://example.org/P2"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#short">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/c.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P2"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#short">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C6">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:ObjectProperty rdf:about="http://example.org/P6"/></owl:onProperty>
        <owl:maxCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#byte">0</owl:maxCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C6.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P6"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#byte">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/a"/>
      <owl:Class rdf:about="http://example.org/b"/>
    </owl:intersectionOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C7">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/P7"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C7.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P7"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/a"/>
      <owl:Class rdf:about="http://example.org/c"/>
    </owl:intersectionOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C8">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:ObjectProperty rdf:about="http://example.org/P8"/></owl:onProperty>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#int">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C8.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P8"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#int">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/b"/>
      <owl:Class rdf:about="http://example.org/c"/>
    </owl:intersectionOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/Unsat">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/P5"/></owl:onProperty>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#decimal">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/Unsat.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P5"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#decimal">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/C6"/>
      <owl:Class rdf:about="http://example.org/C7"/>
      <owl:Class rdf:about="http://example.org/C8"/>
    </owl:intersectionOf>
  </owl:Class>
  {instance_element}
</rdf:RDF>"#
            ))
        }

        /// The instance of Unsatisfiable refutes the ontology: every
        /// disjunct of its exposed normal form embeds a pair-disjoint
        /// pair (W3C description-logic-601).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn fact_1_11_instance_refutes() {
            assert!(
                !consistent(fact_1_11_ontology(true, true)),
                "the fact1.1 instance must refute the ontology"
            );
        }

        /// The TBox alone is fine: no instance, no refutation.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn fact_1_11_without_instance_stays_consistent() {
            assert!(
                consistent(fact_1_11_ontology(false, true)),
                "the TBox alone keeps a model"
            );
        }

        /// Break one disjointness edge (b ⊑ c.comp gone) and the
        /// (b⊓c) disjunct survives — fail-open, still consistent.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn fact_1_11_broken_chain_stays_consistent() {
            assert!(
                consistent(fact_1_11_ontology(true, false)),
                "a surviving disjunct keeps the ontology consistent"
            );
        }

        /// The assist's premise direction (#156's review round): a
        /// node excluding S is constrained only by intersections
        /// declared UNDER S (D ⊑ S). A one-way `Y ⊑ C6 ⊓ C7`
        /// licenses nothing for such a node — every avoidance may
        /// die while a model remains (x sits in C6 and C7 without
        /// being in Y), so the ontology must stay consistent.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn one_way_intersection_super_side_stays_consistent() {
            let ontology = parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:Class rdf:about="http://example.org/X">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/PP"/></owl:onProperty>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#int">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/Y">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/PP"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#int">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <rdfs:subClassOf>
      <owl:Class>
        <owl:intersectionOf rdf:parseType="Collection">
          <owl:Class rdf:about="http://example.org/C6"/>
          <owl:Class rdf:about="http://example.org/C7"/>
        </owl:intersectionOf>
      </owl:Class>
    </rdfs:subClassOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C6">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:ObjectProperty rdf:about="http://example.org/P6"/></owl:onProperty>
        <owl:maxCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#byte">0</owl:maxCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C6.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P6"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#byte">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/a"/>
      <owl:Class rdf:about="http://example.org/b"/>
    </owl:intersectionOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C7">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/P7"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/C7.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P7"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Class rdf:about="http://example.org/a"/>
      <owl:Class rdf:about="http://example.org/c"/>
    </owl:intersectionOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/a">
    <rdfs:subClassOf>
      <owl:Class rdf:about="http://example.org/b.comp"/>
    </rdfs:subClassOf>
    <rdfs:subClassOf rdf:resource="http://example.org/c.comp"/>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/b">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:DatatypeProperty rdf:about="http://example.org/P3"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#unsignedByte">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/b.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P3"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#unsignedByte">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/c">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty><owl:ObjectProperty rdf:about="http://example.org/P2"/></owl:onProperty>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#short">1</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <owl:Class rdf:about="http://example.org/c.comp">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/P2"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#short">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
  <rdf:Description rdf:about="http://example.org/x">
    <rdf:type rdf:resource="http://example.org/X"/>
  </rdf:Description>
</rdf:RDF>"#,
            );
            assert!(
                consistent(ontology),
                "a one-way super-side intersection must not refute (x models into C6⊓C7 outside Y)"
            );
        }
    }

    mod datatypes_118 {
        //! Fixtures for #118: XSD built-in semantics consulted by the
        //! entailment kernel. The parser lowers datatype-property
        //! ranges onto ObjectPropertyRange with the XSD IRI as the
        //! range class, so range conclusions decide by value-space
        //! containment and rdfs:Datatype membership is vocabulary.
        //! Anchors: W3C I5.8-006/-008/-009/-011 (0302/0306/0308/
        //! 0312).

        use super::entail;
        use super::super::*;
        use crate::core::iri::IRI;
        use crate::logic::axioms::{ClassAssertionAxiom, ObjectPropertyRangeAxiom};

        const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
        const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        fn range_axiom(property: &str, datatype: &str) -> Axiom {
            Axiom::ObjectPropertyRange(Box::new(ObjectPropertyRangeAxiom::new(
                iri(&format!("http://example.org/{property}")),
                iri(&format!("{XSD}{datatype}")),
            )))
        }

        fn range_premise(datatypes: &[&str]) -> Ontology {
            let mut premise = Ontology::new();
            for datatype in datatypes {
                premise.add_axiom(range_axiom("p", datatype)).unwrap();
            }
            premise
        }

        fn range_conclusion(datatype: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            conclusion.add_axiom(range_axiom("p", datatype)).unwrap();
            conclusion
        }

        fn datatype_member_conclusion(datatype: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            conclusion
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(&format!("{XSD}{datatype}")),
                    ClassExpression::Class(crate::core::entities::Class::from_shared_iri(
                        iri(RDFS_DATATYPE),
                    )),
                ))))
                .unwrap();
            conclusion
        }

        /// I5.8-006: byte ⊑ short, so the narrower premise range
        /// entails the wider conclusion.
        #[test]
        fn range_conclusion_follows_narrower_datatype_range() {
            let result = entail(range_premise(&["byte"]), range_conclusion("short"));
            assert!(
                matches!(result, SemanticResult::True),
                "byte ⊑ short: {result:?}"
            );
        }

        /// Guard: the direction does not invert — a wider premise
        /// range never entails the narrower conclusion.
        #[test]
        fn range_conclusion_rejects_wider_than_premise() {
            let result = entail(range_premise(&["short"]), range_conclusion("byte"));
            assert!(
                matches!(result, SemanticResult::False),
                "short ⊄ byte: {result:?}"
            );
        }

        /// I5.8-008: short ∩ unsignedInt = [0, 32767] ⊑ unsignedShort.
        #[test]
        fn range_conclusion_intersects_premise_ranges() {
            let result = entail(
                range_premise(&["short", "unsignedInt"]),
                range_conclusion("unsignedShort"),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "short ∩ unsignedInt ⊑ unsignedShort: {result:?}"
            );
        }

        /// I5.8-009: nonNegativeInteger ∩ nonPositiveInteger = {0} ⊑
        /// short.
        #[test]
        fn range_conclusion_intersects_to_singleton() {
            let result = entail(
                range_premise(&["nonNegativeInteger", "nonPositiveInteger"]),
                range_conclusion("short"),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "[0, 0] ⊑ short: {result:?}"
            );
        }

        /// Guard: an unrelated property's ranges do not bound p.
        #[test]
        fn range_conclusion_ignores_other_properties() {
            let mut premise = range_premise(&["byte"]);
            premise.add_axiom(range_axiom("q", "byte")).unwrap();
            let result = entail(premise, range_conclusion("short"));
            assert!(
                matches!(result, SemanticResult::True),
                "byte on p still bounds p: {result:?}"
            );
        }

        /// Guard (code-review soundness fix): with no premise range
        /// bounding p at all, even the widest conclusion stays
        /// unentailed — an unconstrained property is not known to be
        /// integer-valued.
        #[test]
        fn range_conclusion_never_entails_without_premise_bounds() {
            let result = entail(Ontology::new(), range_conclusion("integer"));
            assert!(
                matches!(result, SemanticResult::False),
                "nothing bounds p; xsd:integer must not be a free entailment: {result:?}"
            );
        }

        /// I5.8-011: built-in datatypes are rdfs:Datatype instances
        /// in every interpretation, from the empty premise.
        #[test]
        fn builtin_datatypes_are_rdf_datatypes() {
            for datatype in ["integer", "string"] {
                let result = entail(Ontology::new(), datatype_member_conclusion(datatype));
                assert!(
                    matches!(result, SemanticResult::True),
                    "xsd:{datatype} is an rdfs:Datatype: {result:?}"
                );
            }
        }

        /// Guard: vocabulary membership is not free for arbitrary
        /// IRIs — a random class is not an rdfs:Datatype.
        #[test]
        fn arbitrary_iri_is_not_an_rdf_datatype() {
            let mut conclusion = Ontology::new();
            conclusion
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("http://example.org/NotADatatype"),
                    ClassExpression::Class(crate::core::entities::Class::from_shared_iri(
                        iri(RDFS_DATATYPE),
                    )),
                ))))
                .unwrap();
            let result = entail(Ontology::new(), conclusion);
            assert!(
                matches!(result, SemanticResult::False),
                "nothing forces a random IRI into rdfs:Datatype: {result:?}"
            );
        }
    }

    mod unions_119 {
        //! Fixtures for #119: union choices deciding entailments. A
        //! pressed union whose disjuncts all die under the negated
        //! conclusion refutes it; a surviving disjunct does not.
        //! Anchor shape: W3C New-Feature-DisjointUnion-001 minus the
        //! parser piece (Child ⊑ Boy ⊔ Girl, #120 adds the axiom).

        use super::entail;
        use super::super::*;
        use crate::core::entities::{Class, NamedIndividual};
        use crate::logic::axioms::{ClassAssertionAxiom, SubClassOfAxiom};

        fn class(name: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(format!("http://example.org/{name}")))
        }

        fn not_class(name: &str) -> ClassExpression {
            ClassExpression::ObjectComplementOf(Box::new(class(name)))
        }

        fn union(parts: &[ClassExpression]) -> ClassExpression {
            ClassExpression::ObjectUnionOf(parts.iter().cloned().map(Box::new).collect())
        }

        /// Child ⊑ Boy ⊔ Girl; Stewie ∈ Child ⊓ ¬Girl.
        fn union_premise() -> Ontology {
            let mut premise = Ontology::new();
            premise
                .add_subclass_axiom(SubClassOfAxiom::new(
                    class("Child"),
                    union(&[class("Boy"), class("Girl")]),
                ))
                .unwrap();
            let stewie = NamedIndividual::new("http://example.org/Stewie");
            premise.add_named_individual(stewie.clone()).unwrap();
            premise
                .add_class_assertion(ClassAssertionAxiom::new(
                    stewie.iri().clone(),
                    class("Child"),
                ))
                .unwrap();
            premise
                .add_class_assertion(ClassAssertionAxiom::new(
                    stewie.iri().clone(),
                    not_class("Girl"),
                ))
                .unwrap();
            premise
        }

        fn member_conclusion(class_name: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            let stewie = NamedIndividual::new("http://example.org/Stewie");
            conclusion.add_named_individual(stewie.clone()).unwrap();
            conclusion
                .add_class_assertion(ClassAssertionAxiom::new(
                    stewie.iri().clone(),
                    class(class_name),
                ))
                .unwrap();
            conclusion
        }

        #[test]
        fn dead_disjuncts_under_negated_conclusion_entail() {
            let result = entail(union_premise(), member_conclusion("Boy"));
            assert!(
                matches!(result, SemanticResult::True),
                "¬Girl kills Girl and the negated ¬Boy kills Boy: {result:?}"
            );
        }

        #[test]
        fn surviving_disjunct_stays_unentailed() {
            let result = entail(union_premise(), member_conclusion("Girl"));
            assert!(
                matches!(result, SemanticResult::False),
                "Boy survives under the doubly-negated Girl: {result:?}"
            );
        }
    }

    mod bnode_122 {
        //! Fixtures for #122: blank-node property assertions in the
        //! entailment lane. The negation is a universal complement
        //! over the bnode's local formula — typings plus existentials
        //! along its outgoing edges. Anchors: W3C
        //! somevaluesfrom2bnode (0258), allValuesFrom-002 (0333,
        //! negative), someValuesFrom-003 (0503, nested).

        use super::entail;
        use super::super::*;
        use crate::core::entities::{AnonymousIndividual, Class, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            ClassAssertionAxiom, PropertyAssertionAxiom, SubClassOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        fn class(name: &str) -> Box<ClassExpression> {
            Box::new(ClassExpression::Class(Class::new(format!(
                "http://example.org/{name}",
            ))))
        }

        fn thing() -> Box<ClassExpression> {
            Box::new(ClassExpression::Class(Class::owl_thing()))
        }

        fn property(name: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(format!("http://example.org/{name}")),
            )))
        }

        fn some(role: &str, filler: Box<ClassExpression>) -> ClassExpression {
            ClassExpression::ObjectSomeValuesFrom(property(role), filler)
        }

        fn all(role: &str, filler: Box<ClassExpression>) -> ClassExpression {
            ClassExpression::ObjectAllValuesFrom(property(role), filler)
        }

        fn not(expression: Box<ClassExpression>) -> Box<ClassExpression> {
            Box::new(ClassExpression::ObjectComplementOf(expression))
        }

        fn typed(ontology: &mut Ontology, individual: &str, expression: ClassExpression) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(&format!("http://example.org/{individual}")),
                    expression,
                ))))
                .unwrap();
        }

        /// p(subject, _:node) — the blank-edge conclusion shape. The
        /// subject may itself be a blank node id (a chain's middle).
        fn anonymous_edge(ontology: &mut Ontology, subject: &str, node: &str) {
            let subject_iri = if subject.starts_with('_') {
                iri(subject)
            } else {
                iri(&format!("http://example.org/{subject}"))
            };
            ontology
                .add_axiom(Axiom::PropertyAssertion(Box::new(
                    PropertyAssertionAxiom::new_with_anonymous(
                        subject_iri,
                        iri("http://example.org/p"),
                        AnonymousIndividual::new(node),
                    ),
                )))
                .unwrap();
        }

        /// _:node: expression — a typing on a blank node, matched by
        /// the parser's node-id-as-IRI representation.
        fn blank_typed(ontology: &mut Ontology, node: &str, expression: ClassExpression) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(node),
                    expression,
                ))))
                .unwrap();
        }

        /// somevaluesfrom2bnode: a forced existential witness makes
        /// the blank edge entailed — the universal complement's filler
        /// clashes on the spawned witness.
        #[test]
        fn blank_node_conclusion_follows_forced_witness() {
            let mut premise = Ontology::new();
            typed(&mut premise, "a", some("p", thing()));

            let mut conclusion = Ontology::new();
            anonymous_edge(&mut conclusion, "a", "_:w");

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "a: some(p, Thing) forces a p-filler: {result:?}"
            );
        }

        /// Without a forced filler the negated universal keeps a
        /// model: the blank edge stays unentailed, decided as false —
        /// not unsupported.
        #[test]
        fn blank_node_conclusion_stays_open_without_witness() {
            let mut premise = Ontology::new();
            typed(&mut premise, "a", *thing());

            let mut conclusion = Ontology::new();
            anonymous_edge(&mut conclusion, "a", "_:w");

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::False),
                "nothing forces a p-filler for a: {result:?}"
            );
        }

        /// allValuesFrom-002 (negative): the universal complement of
        /// an unforced typing stays consistent next to an all-values
        /// GCI, so the conclusion is correctly not entailed.
        #[test]
        fn blank_node_conclusion_negative_verdict_under_universal() {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    *class("r"),
                    all("p", class("c")),
                ))))
                .unwrap();
            typed(&mut premise, "i", *class("r"));
            typed(&mut premise, "i", *thing());

            let mut conclusion = Ontology::new();
            anonymous_edge(&mut conclusion, "i", "_:o");
            blank_typed(&mut conclusion, "_:o", *class("c"));
            blank_typed(&mut conclusion, "_:o", *thing());

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::False),
                "r ⊑ ∀p.c never forces a p-filler: {result:?}"
            );
        }

        /// A typing the premise never forces on the witness must not
        /// be smuggled in by the fold: the complement's ¬c disjunct
        /// keeps a model where the witness is merely Thing.
        #[test]
        fn blank_node_conclusion_rejects_unforced_typing() {
            let mut premise = Ontology::new();
            typed(&mut premise, "a", some("p", thing()));

            let mut conclusion = Ontology::new();
            anonymous_edge(&mut conclusion, "a", "_:w");
            blank_typed(&mut conclusion, "_:w", *class("c"));

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::False),
                "the forced witness is only Thing, never c: {result:?}"
            );
        }

        /// A conclusion that is only a blank-node cycle has no root:
        /// nothing is consumed, the per-axiom fold meets the chain
        /// depth bound, and the verdict stays unsupported — never
        /// True (review counterexample: the skip pass must never let
        /// an unforced cycle pass as entailed).
        #[test]
        fn rootless_blank_node_cycle_stays_unentailed() {
            let premise = Ontology::new();

            let mut conclusion = Ontology::new();
            for (subject, property, object) in [("_:x", "http://example.org/q", "_:y"),
                                                ("_:y", "http://example.org/r", "_:x")] {
                conclusion
                    .add_axiom(Axiom::PropertyAssertion(Box::new(
                        PropertyAssertionAxiom::new_with_anonymous(
                            iri(subject),
                            iri(property),
                            AnonymousIndividual::new(object),
                        ),
                    )))
                    .unwrap();
            }

            let result = entail(premise, conclusion);
            assert!(
                !matches!(result, SemanticResult::True),
                "an unforced blank-node cycle is never entailed: {result:?}"
            );
        }

        /// #123: a GCI-forced witness is a Thing — the propagated
        /// ¬Thing of the fold's negation must clash against the class
        /// the walk recorded on the witness, and owl:Thing is top.
        #[test]
        fn gci_witness_typing_thing_entailed() {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    *class("person"),
                    some("p", class("person")),
                ))))
                .unwrap();
            typed(&mut premise, "fred", *class("person"));

            let mut conclusion = Ontology::new();
            anonymous_edge(&mut conclusion, "fred", "_:w");
            blank_typed(&mut conclusion, "_:w", *thing());

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "the GCI witness the premise forces is a Thing: {result:?}"
            );
        }

        /// #123 (W3C someValuesFrom-003 shape): the two-level chain —
        /// fred's filler has a filler of its own, both typed Thing —
        /// refutes through the nested universal once ¬Thing clashes
        /// with the class on the second witness.
        #[test]
        fn nested_chain_witness_typing_thing_entailed() {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    *class("person"),
                    some("p", class("person")),
                ))))
                .unwrap();
            premise
                .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    some("p", class("person")),
                    *class("person"),
                ))))
                .unwrap();
            typed(&mut premise, "fred", *class("person"));

            let mut conclusion = Ontology::new();
            anonymous_edge(&mut conclusion, "_:b1", "_:b2");
            anonymous_edge(&mut conclusion, "fred", "_:b1");
            blank_typed(&mut conclusion, "_:b1", *thing());
            blank_typed(&mut conclusion, "_:b2", *thing());

            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "both forced witnesses are Things: {result:?}"
            );
        }
    }

    mod data_witness_124 {
        //! Fixtures for #124: data-existential witnesses pinned to
        //! one concrete value by the premise's lattice ranges. Anchor:
        //! W3C I5.8-010 (0310) — ∃p.nonPositive under range nonNeg
        //! forces the value 0.

        use super::entail;
        use super::super::*;
        use crate::core::entities::{DataProperty, Literal};
        use crate::core::iri::IRI;
        use crate::logic::axioms::class_expressions::DataRange;
        use crate::logic::axioms::property_expressions::DataPropertyExpression;
        use crate::logic::axioms::{
            ClassAssertionAxiom, DataPropertyAssertionAxiom, ObjectPropertyRangeAxiom,
        };

        const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        fn xsd(name: &str) -> Arc<IRI> {
            iri(&format!("{XSD}{name}"))
        }

        /// range(p, xsd-name) — the shape datatype ranges parse to.
        fn range(ontology: &mut Ontology, property: &str, datatype: &str) {
            ontology
                .add_axiom(Axiom::ObjectPropertyRange(Box::new(
                    ObjectPropertyRangeAxiom::new(
                        iri(&format!("http://example.org/{property}")),
                        xsd(datatype),
                    ),
                )))
                .unwrap();
        }

        /// individual ∈ ∃property.xsd-name — the data existential.
        fn data_some(ontology: &mut Ontology, individual: &str, property: &str, datatype: &str) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(&format!("http://example.org/{individual}")),
                    ClassExpression::DataSomeValuesFrom(
                        Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                            format!("http://example.org/{property}"),
                        ))),
                        Box::new(DataRange::Datatype((*xsd(datatype)).clone())),
                    ),
                ))))
                .unwrap();
        }

        fn data_assertion(
            ontology: &mut Ontology,
            individual: &str,
            property: &str,
            lexical: &str,
            datatype: &str,
        ) {
            ontology
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(
                        iri(&format!("http://example.org/{individual}")),
                        iri(&format!("http://example.org/{property}")),
                        Literal::typed(lexical, xsd(datatype)),
                    ),
                )))
                .unwrap();
        }

        fn data_conclusion(individual: &str, property: &str, lexical: &str, datatype: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            data_assertion(&mut conclusion, individual, property, lexical, datatype);
            conclusion
        }

        /// I5.8-010: ∃p.nonPositive under a nonNeg range pins the
        /// witness to 0, so the zero assertion is entailed.
        #[test]
        fn pinned_witness_materializes_zero() {
            let mut premise = Ontology::new();
            range(&mut premise, "p", "nonNegativeInteger");
            data_some(&mut premise, "john", "p", "nonPositiveInteger");

            let result = entail(
                premise,
                data_conclusion("john", "p", "0", "int"),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "the only value both ranges allow is 0: {result:?}"
            );
        }

        /// positiveInteger under nonNeg does not pin: any positive
        /// value is a model, so a concrete assertion stays unentailed.
        #[test]
        fn unpinned_witness_stays_open() {
            let mut premise = Ontology::new();
            range(&mut premise, "p", "nonNegativeInteger");
            data_some(&mut premise, "john", "p", "positiveInteger");

            let result = entail(
                premise,
                data_conclusion("john", "p", "1", "int"),
            );
            assert!(
                matches!(result, SemanticResult::False),
                "the witness may be any positive value: {result:?}"
            );
        }

        /// Without a range nothing bounds the witness: −5 models the
        /// existential, so zero is not forced.
        #[test]
        fn witness_without_range_stays_open() {
            let mut premise = Ontology::new();
            data_some(&mut premise, "john", "p", "nonPositiveInteger");

            let result = entail(
                premise,
                data_conclusion("john", "p", "0", "int"),
            );
            assert!(
                matches!(result, SemanticResult::False),
                "an unbounded nonPositive witness can be −5: {result:?}"
            );
        }

        /// Cross-subtype value equality: a byte zero is an int zero.
        #[test]
        fn cross_subtype_zero_literals_are_equal() {
            let mut premise = Ontology::new();
            data_assertion(&mut premise, "john", "p", "0", "byte");

            let result = entail(
                premise,
                data_conclusion("john", "p", "0", "int"),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "0 as byte and 0 as int are one value: {result:?}"
            );
        }
    }

    mod data_hasvalue_125 {
        //! Fixtures for #125: DataHasValue restrictions materialize as
        //! data positives the settled cardinality check can spend.
        //! Anchor: W3C I5.21-002 (0281) — reptile families each
        //! ⊑ hasValue(family-name, own name) under
        //! Reptile ⊑ =1 family-name are pairwise disjoint.

        use super::entail;
        use super::super::*;
        use crate::core::entities::{Class, DataProperty, Literal};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::DataPropertyExpression;
        use crate::logic::axioms::{DisjointClassesAxiom, SubClassOfAxiom};
        use std::sync::Arc;

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        fn class(name: &str) -> ClassExpression {
            ClassExpression::Class(Class::from_shared_iri(iri(&format!(
                "http://example.org/{name}"
            ))))
        }

        fn property(name: &str) -> Box<DataPropertyExpression> {
            Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                format!("http://example.org/{name}"),
            )))
        }

        fn has_value(name: &str, lexical: &str) -> ClassExpression {
            ClassExpression::DataHasValue(property(name), Literal::simple(lexical))
        }

        fn sub(ontology: &mut Ontology, sub: ClassExpression, sup: ClassExpression) {
            ontology
                .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(sub, sup))))
                .unwrap();
        }

        /// The I5.21-002 shape: both families under a Reptile that
        /// allows exactly one family-name, each pinned to its own
        /// literal.
        fn family_premise(first_value: &str, second_value: &str) -> Ontology {
            let mut premise = Ontology::new();
            sub(
                &mut premise,
                class("Reptile"),
                ClassExpression::DataExactCardinality(1, property("family-name"), None),
            );
            sub(&mut premise, class("A"), class("Reptile"));
            sub(&mut premise, class("A"), has_value("family-name", first_value));
            sub(&mut premise, class("B"), class("Reptile"));
            sub(&mut premise, class("B"), has_value("family-name", second_value));
            premise
        }

        fn disjoint(first: &str, second: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            conclusion
                .add_axiom(Axiom::DisjointClasses(Box::new(DisjointClassesAxiom::new(
                    vec![
                        iri(&format!("http://example.org/{first}")),
                        iri(&format!("http://example.org/{second}")),
                    ],
                ))))
                .unwrap();
            conclusion
        }

        /// Two distinct family names under a =1 bound leave no room
        /// for an animal in both families.
        #[test]
        fn distinct_hasvalues_under_exact_one_are_disjoint() {
            let result = entail(
                family_premise("Amphisbaenidae", "Agamidae"),
                disjoint("A", "B"),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "two distinct forced values violate the =1 bound: {result:?}"
            );
        }

        /// The same literal twice is one value: no clash, so the
        /// classes may overlap (value identity, not pair counting).
        #[test]
        fn same_hasvalue_under_exact_one_stays_open() {
            let result = entail(
                family_premise("Amphisbaenidae", "Amphisbaenidae"),
                disjoint("A", "B"),
            );
            assert!(
                matches!(result, SemanticResult::False),
                "one value satisfies the =1 bound: {result:?}"
            );
        }

        /// Without a max/exact bound two forced values are simply two
        /// assertions — no clash, no disjointness.
        #[test]
        fn hasvalue_without_bound_stays_open() {
            let mut premise = Ontology::new();
            sub(&mut premise, class("A"), has_value("family-name", "Amphisbaenidae"));
            sub(&mut premise, class("B"), has_value("family-name", "Agamidae"));

            let result = entail(premise, disjoint("A", "B"));
            assert!(
                matches!(result, SemanticResult::False),
                "no bound caps the values: {result:?}"
            );
        }
    }

    mod annotation_inertness_157 {
        //! Fixtures for #157: an annotation assertion in a conclusion
        //! carries no model-theoretic content under Direct Semantics
        //! — the reduction reads it as satisfied in every
        //! interpretation. The W3C's own -Direct family tests exactly
        //! this inertness (I4.6-005-Direct: "the direct semantics
        //! ignore annotations in the conclusion ontology";
        //! miscellaneous-302-Direct concludes "bar" from "foo" and
        //! expects true). Reverses #126's r31 boundary (0261/0270/
        //! 0474/0495/0501) toward the spec's zero-unsupported
        //! destination.
        use super::parse_ontology;

        use super::entail;
        use super::super::*;
        use crate::core::entities::{Class, Literal};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{ClassAssertionAxiom, DataPropertyAssertionAxiom};
        use std::sync::Arc;

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        /// The premise shape the RDF/XML parser produces for an
        /// `<owl:AnnotationProperty rdf:ID="annotate"/>` declaration: a
        /// vocabulary typing, never an annotation-property entity.
        fn typed_premise(property: &str) -> Ontology {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(&format!("http://example.org/{property}")),
                    ClassExpression::Class(Class::from_shared_iri(iri(
                        "http://www.w3.org/2002/07/owl#AnnotationProperty",
                    ))),
                ))))
                .unwrap();
            premise
        }

        fn data_conclusion(subject: &str, property: &str, lexical: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            conclusion
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(
                        iri(&format!("http://example.org/{subject}")),
                        iri(&format!("http://example.org/{property}")),
                        Literal::simple(lexical),
                    ),
                )))
                .unwrap();
            conclusion
        }

        fn evaluate(premise: Ontology, conclusion: Ontology) -> EvaluationResponse {
            evaluate_entailment(
                Arc::new(premise),
                Arc::new(conclusion),
                "premise.owl",
                "conclusion.owl",
            )
        }

        /// The premise's typing is the declaration: the annotation
        /// conclusion is metadata, decided as entailed — the literal
        /// value constrains nothing (miscellaneous-302-Direct).
        #[test]
        fn premise_typed_annotation_conclusion_is_entailed() {
            let response = evaluate(
                typed_premise("annotate"),
                data_conclusion("c2", "annotate", "description of c1"),
            );
            assert!(
                matches!(
                    response.execution_status,
                    EvaluationExecutionStatus::Success
                ) && matches!(response.semantic_result, SemanticResult::True),
                "an annotation conclusion is inert and entailed: {:?} / {:?}",
                response.execution_status,
                response.semantic_result
            );
        }

        /// An ordinary data property the premise never types stays a
        /// real decision — the inertness reading claims nothing else.
        #[test]
        fn untyped_data_conclusion_stays_decided() {
            let response = evaluate(Ontology::new(), data_conclusion("x", "name", "v"));
            assert!(
                matches!(
                    response.execution_status,
                    EvaluationExecutionStatus::Success
                ) && matches!(response.semantic_result, SemanticResult::False),
                "an ordinary data conclusion is decided, not skipped: {:?} / {:?}",
                response.execution_status,
                response.semantic_result
            );
        }

        /// The object-valued lane reads the same inertness: an
        /// annotation with a named-individual object is metadata.
        #[test]
        fn object_annotation_conclusion_is_entailed() {
            let mut conclusion = Ontology::new();
            conclusion
                .add_axiom(Axiom::PropertyAssertion(Box::new(
                    crate::logic::axioms::PropertyAssertionAxiom::new(
                        iri("http://example.org/x"),
                        iri("http://example.org/annotate"),
                        iri("http://example.org/y"),
                    ),
                )))
                .unwrap();
            let response = evaluate(typed_premise("annotate"), conclusion);
            assert!(
                matches!(
                    response.execution_status,
                    EvaluationExecutionStatus::Success
                ) && matches!(response.semantic_result, SemanticResult::True),
                "an object annotation conclusion is inert and entailed: {:?} / {:?}",
                response.execution_status,
                response.semantic_result
            );
        }

        /// Inertness never spreads: an annotation assertion MIXED
        /// with a real unentailed class assertion leaves the verdict
        /// to the real content (W3C 0501's conclusion shape is
        /// annotation-only; this pin keeps the boundary honest).
        #[test]
        fn annotation_conclusion_does_not_cover_real_axioms() {
            let mut conclusion = data_conclusion("x", "annotate", "v");
            conclusion
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("http://example.org/x"),
                    ClassExpression::Class(Class::from_shared_iri(iri(
                        "http://example.org/Foreign",
                    ))),
                ))))
                .unwrap();
            let result = entail(typed_premise("annotate"), conclusion);
            assert!(
                matches!(result, SemanticResult::False),
                "the real assertion decides, the annotation does not mask it: {result:?}"
            );
        }

        #[cfg(feature = "rio-xml")]
        /// miscellaneous-302-Direct, inlined: the premise annotates a
        /// with "foo", the conclusion re-annotates with "bar" —
        /// inert under Direct Semantics, so true (0495).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn miscellaneous_302_direct_shape_answers_true() {
            let premise = parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:first="http://example.org/">
  <owl:AnnotationProperty rdf:about="http://example.org/prop"/>
  <owl:Thing rdf:about="http://example.org/a">
    <first:prop>foo</first:prop>
  </owl:Thing>
</rdf:RDF>"#,
            );
            let conclusion = parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:first="http://example.org/">
  <owl:AnnotationProperty rdf:about="http://example.org/prop"/>
  <owl:Thing rdf:about="http://example.org/a">
    <first:prop>bar</first:prop>
  </owl:Thing>
</rdf:RDF>"#,
            );
            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "the annotation's literal value is not semantic content: {result:?}"
            );
        }

        /// I4.6-005-Direct, inlined: the comment moves across
        /// equivalent classes — ignored in the conclusion, so true
        /// (0270).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn i4_6_005_direct_shape_answers_true() {
            let premise = parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:Class rdf:about="http://example.org/C1">
    <rdfs:comment>An example class.</rdfs:comment>
    <owl:equivalentClass>
      <owl:Class rdf:about="http://example.org/C2"/>
    </owl:equivalentClass>
  </owl:Class>
</rdf:RDF>"#,
            );
            let conclusion = parse_ontology(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:Class rdf:about="http://example.org/C2">
    <rdfs:comment>An example class.</rdfs:comment>
  </owl:Class>
</rdf:RDF>"#,
            );
            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "the moved comment is inert in the conclusion: {result:?}"
            );
        }
    }

    mod enumeration_consistency_158 {
        //! Fixtures for #158: the consistency lane consumes the
        //! enumeration-frames channel (#155's recorded hardening) —
        //! the consistency input plays the premise role, so a
        //! single-list oneOf frame's members-only constraint presses
        //! there exactly as it does premise-side in entailment. The
        //! clash rides the #114 oneOf-constraint machinery: a named
        //! node under C ⊑ oneOf{a,b} whose every member is blocked
        //! (differentFrom) contradicts.
        use super::parse_ontology;

        use super::super::*;
        use crate::core::ontology::Ontology;

        fn consistent(ontology: Ontology) -> bool {
            matches!(
                evaluate_consistency(Arc::new(ontology), "fixture.owl").semantic_result,
                SemanticResult::Consistent
            )
        }

        #[cfg(feature = "rio-xml")]
        /// A's single-list oneOf{a,b} frame reaches the kernel only
        /// through the channel; `blocked` adds x's differentFrom
        /// against both members.
        #[cfg(feature = "rio-xml")]
        fn frame_ontology(blocked: bool) -> Ontology {
            let differences = if blocked {
                r#"<owl:differentFrom rdf:resource="http://example.org/a"/>
    <owl:differentFrom rdf:resource="http://example.org/b"/>"#
            } else {
                ""
            };
            parse_ontology(&format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="http://example.org/A">
    <owl:oneOf rdf:parseType="Collection">
      <owl:Thing rdf:about="http://example.org/a"/>
      <owl:Thing rdf:about="http://example.org/b"/>
    </owl:oneOf>
  </owl:Class>
  <rdf:Description rdf:about="http://example.org/x">
    <rdf:type rdf:resource="http://example.org/A"/>
    {differences}
  </rdf:Description>
</rdf:RDF>"#
            ))
        }

        /// x ∈ A ⊑ {a,b} with both members differentFrom-blocked is a
        /// contradiction: the frame must press in this lane too.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn frame_with_blocked_members_refutes() {
            assert!(
                !consistent(frame_ontology(true)),
                "the members-only constraint must clash x against the blocked pair"
            );
        }

        /// Without the blocking the model stands: x is a or b.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn frame_without_blocking_stays_consistent() {
            assert!(
                consistent(frame_ontology(false)),
                "an unblocked member keeps the model"
            );
        }
    }

    mod property_reflexivity_159 {
        //! Fixtures for #159 (#148/#149 hardening): one property
        //! declared both reflexive and irreflexive is contradictory
        //! under Direct Semantics — reflexivity forces a self-loop on
        //! every domain element, irreflexivity forbids every one, and
        //! domains are non-empty. The declared-shape check fires
        //! without any assertion (no node exists to spawn or ban a
        //! loop on).
        use super::parse_ontology;

        use super::super::*;
        use crate::core::ontology::Ontology;

        fn consistent(ontology: Ontology) -> bool {
            matches!(
                evaluate_consistency(Arc::new(ontology), "fixture.owl").semantic_result,
                SemanticResult::Consistent
            )
        }

        #[cfg(feature = "rio-xml")]
        #[cfg(feature = "rio-xml")]
        fn declarations(reflexive_on: Option<&str>, irreflexive_on: Option<&str>) -> Ontology {
            let typing = |kind: &str, property: Option<&str>| {
                property.map_or_else(String::new, |p| {
                    format!(
                        r#"<owl:ObjectProperty rdf:about="http://example.org/{p}">
    <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#{kind}"/>
  </owl:ObjectProperty>"#
                    )
                })
            };
            parse_ontology(&format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  {}
  {}
</rdf:RDF>"#,
                typing("ReflexiveProperty", reflexive_on),
                typing("IrreflexiveProperty", irreflexive_on),
            ))
        }

        /// Both declarations on one property, nothing asserted: the
        /// declared shape itself contradicts (non-empty domain).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn both_declarations_on_one_property_refute() {
            assert!(
                !consistent(declarations(Some("p"), Some("p"))),
                "reflexive ∧ irreflexive on one property has no model"
            );
        }

        /// Either declaration alone keeps a model.
        #[cfg(feature = "rio-xml")]
        #[test]
        fn single_declarations_stay_consistent() {
            assert!(
                consistent(declarations(Some("p"), None)),
                "reflexive alone is fine"
            );
            assert!(
                consistent(declarations(None, Some("p"))),
                "irreflexive alone is fine"
            );
        }

        /// The conflict is per property: declarations on different
        /// properties leave a model (scope pin — no closure).
        #[cfg(feature = "rio-xml")]
        #[test]
        fn declarations_on_different_properties_stay_consistent() {
            assert!(
                consistent(declarations(Some("p"), Some("q"))),
                "reflexive p and irreflexive q do not interact"
            );
        }
    }

    mod intersection_complements_160 {
        //! Fixtures for #160: a node carrying ¬(D1 ⊓ … ⊓ Dn) whose
        //! every conjunct is verifiably satisfied is inconsistent —
        //! the complement is violated outright. Named conjuncts
        //! verify through recorded entailment, hasSelf through a
        //! realized self-loop, existentials through a witnessed
        //! successor. The check lives in the check phase on purpose:
        //! installing a standing ban from the disjunction would be
        //! unsound whenever another conjunct fails (the #148/#149
        //! backlog's intersection-complement gap).

        use super::entail;
        use super::super::*;
        use crate::core::entities::{Class, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::ClassAssertionAxiom;
        use crate::logic::axioms::PropertyAssertionAxiom;
        use smallvec::SmallVec;

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn role(name: &str) -> crate::logic::axioms::ObjectPropertyExpression {
            crate::logic::axioms::ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::from_shared_iri(iri(name)),
            ))
        }

        /// Premise: x : C plus optionally the self-loop; the
        /// successor variant carries y : <filler> and x—r→y.
        fn premise(self_loop: bool, successor: Option<&str>) -> Ontology {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("x"),
                    ClassExpression::Class(Class::from_shared_iri(iri("C"))),
                ))))
                .unwrap();
            if self_loop {
                premise
                    .add_property_assertion(PropertyAssertionAxiom::new(
                        iri("x"),
                        iri("r"),
                        iri("x"),
                    ))
                    .unwrap();
            }
            if let Some(filler) = successor {
                premise
                    .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                        iri("y"),
                        ClassExpression::Class(Class::from_shared_iri(iri(filler))),
                    ))))
                    .unwrap();
                premise
                    .add_property_assertion(PropertyAssertionAxiom::new(
                        iri("x"),
                        iri("r"),
                        iri("y"),
                    ))
                    .unwrap();
            }
            premise
        }

        /// Conclusion: x : <first> ⊓ <shape> — the reduction negates
        /// it into exactly the recorded intersection complement.
        fn intersection_conclusion(first: &str, shape: ClassExpression) -> Ontology {
            let mut conclusion = Ontology::new();
            let mut operands: SmallVec<[Box<ClassExpression>; 4]> = SmallVec::new();
            operands.push(Box::new(ClassExpression::Class(Class::from_shared_iri(iri(first)))));
            operands.push(Box::new(shape));
            conclusion
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("x"),
                    ClassExpression::ObjectIntersectionOf(operands),
                ))))
                .unwrap();
            conclusion
        }

        /// x : C with a self-loop satisfies C ⊓ hasSelf(r): the
        /// complement's every conjunct holds against it: true.
        #[test]
        fn satisfied_self_conjunction_refutes() {
            let result = entail(
                premise(true, None),
                intersection_conclusion(
                    "C",
                    ClassExpression::ObjectHasSelf(Box::new(role("r"))),
                ),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "C recorded and the loop realized: the complement cannot hold: {result:?}"
            );
        }

        /// Without the recorded named conjunct the other conjunct may
        /// fail — no verification, no refutation: false.
        #[test]
        fn unrecorded_conjunct_stays_open() {
            let result = entail(
                premise(true, None),
                intersection_conclusion(
                    "D",
                    ClassExpression::ObjectHasSelf(Box::new(role("r"))),
                ),
            );
            assert!(
                matches!(result, SemanticResult::False),
                "an unverified conjunct keeps the complement satisfiable: {result:?}"
            );
        }

        /// x : C with a witnessed successor satisfies C ⊓ ∃r.E: true.
        #[test]
        fn satisfied_existential_conjunction_refutes() {
            let result = entail(
                premise(false, Some("E")),
                intersection_conclusion(
                    "C",
                    ClassExpression::ObjectSomeValuesFrom(
                        Box::new(role("r")),
                        Box::new(ClassExpression::Class(Class::from_shared_iri(iri("E")))),
                    ),
                ),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "C recorded and the r-successor witnessed in E: {result:?}"
            );
        }

        /// The successor typed F does not witness E: the existential
        /// conjunct stays unverified, no refutation: false.
        #[test]
        fn unwitnessed_existential_stays_open() {
            let result = entail(
                premise(false, Some("F")),
                intersection_conclusion(
                    "C",
                    ClassExpression::ObjectSomeValuesFrom(
                        Box::new(role("r")),
                        Box::new(ClassExpression::Class(Class::from_shared_iri(iri("E")))),
                    ),
                ),
            );
            assert!(
                matches!(result, SemanticResult::False),
                "the successor sits in F, not E: the complement keeps a model: {result:?}"
            );
        }
    }

    mod sameas_canonicalization_128 {
        //! Fixtures for #128: asserted owl:sameAs renames one
        //! individual several ways, and the conflict pre-passes compare
        //! names through that rename. Anchors: W3C disjointWith-001
        //! (0453), DisjointDataProperties-002 (0043),
        //! DisjointObjectProperties-001 (0045). The 0043/0047 corpus
        //! rows additionally need an owl:AllDisjointProperties parser
        //! arm — these pins cover the kernel side only.

        use super::entail;
        use super::super::*;
        use crate::core::entities::{Class, Literal};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            ClassAssertionAxiom, DataPropertyAssertionAxiom,
            DifferentIndividualsAxiom, DisjointClassesAxiom,
            DisjointDataPropertiesAxiom, DisjointObjectPropertiesAxiom,
            PropertyAssertionAxiom,
        };
        use std::sync::Arc;

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        fn class(name: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(format!("http://example.org/{name}")))
        }

        fn typed(ontology: &mut Ontology, name: &str, class_name: &str) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(&format!("http://example.org/{name}")),
                    class(class_name),
                ))))
                .unwrap();
        }

        fn different(first: &str, second: &str) -> Ontology {
            let mut conclusion = Ontology::new();
            conclusion
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![
                        iri(&format!("http://example.org/{first}")),
                        iri(&format!("http://example.org/{second}")),
                    ]),
                )))
                .unwrap();
            conclusion
        }

        /// disjointWith-001: a ∈ A, b ∈ B, A disjoint B — so a ≠ b;
        /// the negation merges a and b into one member of both.
        #[test]
        fn disjoint_classes_make_members_different() {
            let mut premise = Ontology::new();
            premise
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    iri("http://example.org/A"),
                    iri("http://example.org/B"),
                ]))
                .unwrap();
            typed(&mut premise, "a", "A");
            typed(&mut premise, "b", "B");

            let result = entail(premise, different("a", "b"));
            assert!(
                matches!(result, SemanticResult::True),
                "one individual cannot sit in two disjoint classes: {result:?}"
            );
        }

        /// Without the disjointness the individuals may share a name —
        /// the rename must not invent distinctness.
        #[test]
        fn without_disjointness_stays_open() {
            let mut premise = Ontology::new();
            typed(&mut premise, "a", "A");
            typed(&mut premise, "b", "B");

            let result = entail(premise, different("a", "b"));
            assert!(
                matches!(result, SemanticResult::False),
                "nothing separates a and b: {result:?}"
            );
        }

        /// DisjointObjectProperties-001: s p1 x, s p2 y with p1, p2
        /// disjoint — so x ≠ y; the merged pair puts one object under
        /// both properties.
        #[test]
        fn disjoint_object_properties_make_fillers_different() {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::DisjointObjectProperties(Box::new(
                    DisjointObjectPropertiesAxiom::new(vec![
                        iri("http://example.org/p1"),
                        iri("http://example.org/p2"),
                    ]),
                )))
                .unwrap();
            premise
                .add_axiom(Axiom::PropertyAssertion(Box::new(
                    PropertyAssertionAxiom::new(
                        iri("http://example.org/s"),
                        iri("http://example.org/p1"),
                        iri("http://example.org/x"),
                    ),
                )))
                .unwrap();
            premise
                .add_axiom(Axiom::PropertyAssertion(Box::new(
                    PropertyAssertionAxiom::new(
                        iri("http://example.org/s"),
                        iri("http://example.org/p2"),
                        iri("http://example.org/y"),
                    ),
                )))
                .unwrap();

            let result = entail(premise, different("x", "y"));
            assert!(
                matches!(result, SemanticResult::True),
                "disjoint properties cannot share one object: {result:?}"
            );
        }

        /// DisjointDataProperties-002: two disjoint data properties
        /// carrying the same literal on (merged) one subject.
        #[test]
        fn disjoint_data_properties_make_subjects_different() {
            let mut premise = Ontology::new();
            premise
                .add_axiom(Axiom::DisjointDataProperties(Box::new(
                    DisjointDataPropertiesAxiom::new(vec![
                        iri("http://example.org/q1"),
                        iri("http://example.org/q2"),
                    ]),
                )))
                .unwrap();
            premise
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(
                        iri("http://example.org/a"),
                        iri("http://example.org/q1"),
                        Literal::simple("v"),
                    ),
                )))
                .unwrap();
            premise
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(
                        iri("http://example.org/b"),
                        iri("http://example.org/q2"),
                        Literal::simple("v"),
                    ),
                )))
                .unwrap();

            let result = entail(premise, different("a", "b"));
            assert!(
                matches!(result, SemanticResult::True),
                "disjoint data properties cannot carry one value: {result:?}"
            );
        }
    }

    mod negation_normalization_129 {
        //! Fixtures for #129: the negated conclusion lands in the
        //! shapes the kernel decides — double negations collapse,
        //! cardinality complements flip the bound, exact becomes the
        //! union of its two violated readings. Anchors: W3C
        //! ObjectQCR-001/-002 (0071/0073), cardinality-002 (0338).

        use super::entail;
        use super::super::*;
        use crate::core::entities::{Class, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            ClassAssertionAxiom, DifferentIndividualsAxiom,
            PropertyAssertionAxiom, SubClassOfAxiom,
        };
        use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
        use std::sync::Arc;

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(name).expect("test IRI is valid")
        }

        fn class(name: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(format!("http://example.org/{name}")))
        }

        fn role(name: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(format!("http://example.org/{name}")),
            )))
        }

        fn typed(ontology: &mut Ontology, name: &str, expression: ClassExpression) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(&format!("http://example.org/{name}")),
                    expression,
                ))))
                .unwrap();
        }

        fn edge(ontology: &mut Ontology, subject: &str, property: &str, object: &str) {
            ontology
                .add_axiom(Axiom::PropertyAssertion(Box::new(
                    PropertyAssertionAxiom::new(
                        iri(&format!("http://example.org/{subject}")),
                        iri(&format!("http://example.org/{property}")),
                        iri(&format!("http://example.org/{object}")),
                    ),
                )))
                .unwrap();
        }

        fn different(ontology: &mut Ontology, first: &str, second: &str) {
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![
                        iri(&format!("http://example.org/{first}")),
                        iri(&format!("http://example.org/{second}")),
                    ]),
                )))
                .unwrap();
        }

        fn sub(ontology: &mut Ontology, sub: ClassExpression, sup: ClassExpression) {
            ontology
                .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(sub, sup))))
                .unwrap();
        }

        fn conclusion_assertion(name: &str, expression: ClassExpression) -> Ontology {
            let mut conclusion = Ontology::new();
            typed(&mut conclusion, name, expression);
            conclusion
        }

        /// ObjectQCR-002: one Woman slot under a qualified max, two
        /// distinct candidates, so Stewie cannot be the second Woman.
        #[test]
        fn double_complement_conclusion_collapses() {
            let mut premise = Ontology::new();
            edge(&mut premise, "peter", "fatherOf", "stewie");
            edge(&mut premise, "peter", "fatherOf", "meg");
            typed(&mut premise, "meg", class("Woman"));
            different(&mut premise, "stewie", "meg");
            typed(
                &mut premise,
                "peter",
                ClassExpression::ObjectMaxCardinality(1, role("fatherOf"), Some(Box::new(class("Woman")))),
            );

            let result = entail(
                premise,
                conclusion_assertion(
                    "stewie",
                    ClassExpression::ObjectComplementOf(Box::new(class("Woman"))),
                ),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "not-not-Woman is Woman, and two Women break the max: {result:?}"
            );
        }

        /// ObjectQCR-001: two distinct Man fillers satisfy a qualified
        /// min of 2; the negation is a qualified max of 1.
        #[test]
        fn qualified_min_negates_to_qualified_max() {
            let mut premise = Ontology::new();
            edge(&mut premise, "peter", "fatherOf", "stewie");
            edge(&mut premise, "peter", "fatherOf", "chris");
            typed(&mut premise, "stewie", class("Man"));
            typed(&mut premise, "chris", class("Man"));
            different(&mut premise, "stewie", "chris");

            let result = entail(
                premise,
                conclusion_assertion(
                    "peter",
                    ClassExpression::ObjectMinCardinality(
                        2,
                        role("fatherOf"),
                        Some(Box::new(class("Man"))),
                    ),
                ),
            );
            assert!(
                matches!(result, SemanticResult::True),
                "two distinct Man fillers meet the qualified min: {result:?}"
            );
        }

        /// cardinality-002: min 1 and max 1 together mean exactly 1;
        /// the negation is the union of min-2 and max-0, and each
        /// branch dies against the pooled bounds.
        #[test]
        fn exact_conclusion_negates_to_union() {
            let mut premise = Ontology::new();
            sub(
                &mut premise,
                class("C"),
                ClassExpression::ObjectMaxCardinality(1, role("p"), None),
            );
            sub(
                &mut premise,
                class("C"),
                ClassExpression::ObjectMinCardinality(1, role("p"), None),
            );

            let mut conclusion = Ontology::new();
            sub(
                &mut conclusion,
                class("C"),
                ClassExpression::ObjectExactCardinality(1, role("p"), None),
            );
            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                ">=1 and <=1 leave no room for anything but exactly 1: {result:?}"
            );
        }

        /// Without the min, a C-instance may carry zero fillers — the
        /// exact conclusion must stay unentailed.
        #[test]
        fn exact_without_min_stays_open() {
            let mut premise = Ontology::new();
            sub(
                &mut premise,
                class("C"),
                ClassExpression::ObjectMaxCardinality(1, role("p"), None),
            );

            let mut conclusion = Ontology::new();
            sub(
                &mut conclusion,
                class("C"),
                ClassExpression::ObjectExactCardinality(1, role("p"), None),
            );
            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::False),
                "zero fillers satisfy the max but not the exact: {result:?}"
            );
        }

        /// The data twin of cardinality-002: an unqualified data
        /// exact negates into the same two-reading union.
        #[test]
        fn data_exact_conclusion_negates_to_union() {
            use crate::core::entities::DataProperty;
            use crate::logic::axioms::property_expressions::DataPropertyExpression;
            fn data_role(name: &str) -> Box<DataPropertyExpression> {
                Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                    format!("http://example.org/{name}"),
                )))
            }
            let mut premise = Ontology::new();
            sub(
                &mut premise,
                class("C"),
                ClassExpression::DataMaxCardinality(1, data_role("P"), None),
            );
            sub(
                &mut premise,
                class("C"),
                ClassExpression::DataMinCardinality(1, data_role("P"), None),
            );

            let mut conclusion = Ontology::new();
            sub(
                &mut conclusion,
                class("C"),
                ClassExpression::DataExactCardinality(1, data_role("P"), None),
            );
            let result = entail(premise, conclusion);
            assert!(
                matches!(result, SemanticResult::True),
                "data min 1 and max 1 leave only exactly 1: {result:?}"
            );
        }
    }
}
