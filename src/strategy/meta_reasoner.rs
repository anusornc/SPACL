//! Meta-reasoner for intelligent selection of reasoning strategies
//!
//! This module implements a machine learning-based meta-reasoner that
//! dynamically selects the most appropriate reasoning approach based on
//! ontology characteristics and reasoning tasks.

use crate::reasoner::{
    ComplexityLevel, ExpressivenessLevel as ExpressionLevel, OntologyFeatures, ReasoningTask,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reasoning strategy selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningStrategy {
    /// Use tableaux-based reasoning
    Tableaux,
    /// Use saturation-based reasoning (for EL profiles)
    Saturation,
    /// Use transformation-based reasoning (for EL++)
    Transformation,
    /// Use hybrid approach with fallback
    Hybrid,
}

/// Meta-reasoner for strategy selection
pub struct MetaReasoner {
    /// Decision tree for strategy selection
    decision_tree: DecisionTree,
    /// Performance history for learning
    performance_history: HashMap<String, PerformanceRecord>,
    /// Configuration parameters
    config: MetaReasonerConfig,
}

/// Configuration for meta-reasoner
#[derive(Debug, Clone)]
pub struct MetaReasonerConfig {
    /// Enable learning from performance history
    pub enable_learning: bool,
    /// Minimum samples before using learned preferences
    pub min_samples_for_learning: usize,
    /// Weight for performance history in decision making
    pub history_weight: f64,
}

impl Default for MetaReasonerConfig {
    fn default() -> Self {
        MetaReasonerConfig {
            enable_learning: true,
            min_samples_for_learning: 10,
            history_weight: 0.3,
        }
    }
}

/// Performance record for learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecord {
    pub strategy: ReasoningStrategy,
    pub execution_time_ms: u64,
    pub memory_usage_mb: f64,
    pub success: bool,
    pub ontology_hash: String,
}

/// Simple decision tree for strategy selection
pub struct DecisionTree {
    /// Rules for strategy selection
    rules: Vec<DecisionRule>,
}

/// Decision rule
#[derive(Debug, Clone)]
pub struct DecisionRule {
    pub condition: RuleCondition,
    pub strategy: ReasoningStrategy,
    pub confidence: f64,
}

/// Rule condition
#[derive(Debug, Clone)]
pub enum RuleCondition {
    /// Check expressiveness level
    ExpressionLevel(ExpressionLevel),
    /// Check complexity level
    ComplexityLevel(ComplexityLevel),
    /// Check number of classes
    NumClasses { min: usize, max: Option<usize> },
    /// Check if has nominals
    HasNominals(bool),
    /// Combined conditions
    And(Vec<RuleCondition>),
    /// Alternative conditions
    Or(Vec<RuleCondition>),
}

impl MetaReasoner {
    /// Create a new meta-reasoner
    pub fn new() -> anyhow::Result<Self> {
        let decision_tree = DecisionTree::new();
        let performance_history = HashMap::new();
        let config = MetaReasonerConfig::default();

        Ok(MetaReasoner {
            decision_tree,
            performance_history,
            config,
        })
    }

    /// Select the best reasoning strategy
    pub fn select_reasoning_strategy(
        &self,
        features: &OntologyFeatures,
        task: ReasoningTask,
    ) -> anyhow::Result<ReasoningStrategy> {
        // First, try to use learned preferences if available
        if self.config.enable_learning {
            if let Some(learned_strategy) = self.get_learned_preference(features, task) {
                return Ok(learned_strategy);
            }
        }

        // Fall back to rule-based selection
        let strategy = self.decision_tree.select_strategy(features, task);
        Ok(strategy)
    }

    /// Get learned preference from performance history
    fn get_learned_preference(
        &self,
        features: &OntologyFeatures,
        _task: ReasoningTask,
    ) -> Option<ReasoningStrategy> {
        let ontology_hash = self.compute_ontology_hash(features);

        // Find similar ontologies in history
        let similar_records: Vec<_> = self
            .performance_history
            .values()
            .filter(|record| record.success && record.ontology_hash == ontology_hash)
            .collect();

        if similar_records.len() < self.config.min_samples_for_learning {
            return None;
        }

        // Find the strategy with best average performance
        let mut strategy_performance: HashMap<ReasoningStrategy, (u64, usize)> = HashMap::new();

        for record in similar_records {
            let entry = strategy_performance
                .entry(record.strategy)
                .or_insert((0, 0));
            entry.0 += record.execution_time_ms;
            entry.1 += 1;
        }

        // Select strategy with lowest average execution time
        strategy_performance
            .iter()
            .min_by_key(|(_, (total_time, count))| total_time / *count as u64)
            .map(|(strategy, _)| *strategy)
    }

    /// Compute a hash for ontology features
    fn compute_ontology_hash(&self, features: &OntologyFeatures) -> String {
        format!(
            "{}_{}_{}_{:?}_{}_{}_{:?}",
            features.num_classes,
            features.num_properties,
            features.num_individuals,
            features.expressiveness_level,
            features.has_nominals,
            features.has_cardinality_restrictions,
            features.estimated_complexity
        )
    }

    /// Record performance for learning
    pub fn record_performance(
        &mut self,
        features: &OntologyFeatures,
        strategy: ReasoningStrategy,
        execution_time_ms: u64,
        memory_usage_mb: f64,
        success: bool,
    ) {
        let ontology_hash = self.compute_ontology_hash(features);
        let record = PerformanceRecord {
            strategy,
            execution_time_ms,
            memory_usage_mb,
            success,
            ontology_hash: ontology_hash.clone(),
        };

        self.performance_history.insert(
            format!("{}_{}", ontology_hash, chrono::Utc::now().timestamp()),
            record,
        );
    }
}

impl DecisionTree {
    /// Create a new decision tree with default rules
    pub fn new() -> Self {
        let rules = vec![
            // EL profile -> Saturation reasoning
            DecisionRule {
                condition: RuleCondition::ExpressionLevel(ExpressionLevel::EL),
                strategy: ReasoningStrategy::Saturation,
                confidence: 0.9,
            },
            // Small ontologies with low complexity -> Transformation
            DecisionRule {
                condition: RuleCondition::And(vec![
                    RuleCondition::NumClasses {
                        min: 0,
                        max: Some(1000),
                    },
                    RuleCondition::ComplexityLevel(ComplexityLevel::Low),
                ]),
                strategy: ReasoningStrategy::Transformation,
                confidence: 0.8,
            },
            // Medium complexity -> Hybrid approach
            DecisionRule {
                condition: RuleCondition::ComplexityLevel(ComplexityLevel::Medium),
                strategy: ReasoningStrategy::Hybrid,
                confidence: 0.7,
            },
            // High complexity or has nominals -> Tableaux
            DecisionRule {
                condition: RuleCondition::Or(vec![
                    RuleCondition::ComplexityLevel(ComplexityLevel::High),
                    RuleCondition::ComplexityLevel(ComplexityLevel::VeryHigh),
                    RuleCondition::HasNominals(true),
                ]),
                strategy: ReasoningStrategy::Tableaux,
                confidence: 0.85,
            },
            // Default fallback -> Hybrid
            DecisionRule {
                condition: RuleCondition::NumClasses { min: 0, max: None },
                strategy: ReasoningStrategy::Hybrid,
                confidence: 0.5,
            },
        ];

        DecisionTree { rules }
    }

    /// Select strategy based on rules
    pub fn select_strategy(
        &self,
        features: &OntologyFeatures,
        _task: ReasoningTask,
    ) -> ReasoningStrategy {
        // Find the first rule that matches with highest confidence
        let mut best_match = None;
        let mut best_confidence = 0.0;

        for rule in &self.rules {
            if self.evaluate_condition(&rule.condition, features)
                && rule.confidence > best_confidence
            {
                best_match = Some(rule.strategy);
                best_confidence = rule.confidence;
            }
        }

        best_match.unwrap_or(ReasoningStrategy::Hybrid)
    }

    /// Evaluate a rule condition
    fn evaluate_condition(&self, condition: &RuleCondition, features: &OntologyFeatures) -> bool {
        match condition {
            RuleCondition::ExpressionLevel(level) => {
                std::mem::discriminant(&features.expressiveness_level)
                    == std::mem::discriminant(level)
            }
            RuleCondition::ComplexityLevel(level) => {
                std::mem::discriminant(&features.estimated_complexity)
                    == std::mem::discriminant(level)
            }
            RuleCondition::NumClasses { min, max } => {
                features.num_classes >= *min
                    && max.map_or(true, |max_val| features.num_classes <= max_val)
            }
            RuleCondition::HasNominals(expected) => features.has_nominals == *expected,
            RuleCondition::And(conditions) => conditions
                .iter()
                .all(|cond| self.evaluate_condition(cond, features)),
            RuleCondition::Or(conditions) => conditions
                .iter()
                .any(|cond| self.evaluate_condition(cond, features)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_reasoner_creation() {
        let meta_reasoner = MetaReasoner::new();
        assert!(meta_reasoner.is_ok());
    }

    #[test]
    fn test_strategy_selection() {
        let meta_reasoner = MetaReasoner::new().unwrap();

        let features = OntologyFeatures {
            num_classes: 100,
            num_properties: 50,
            num_individuals: 200,
            expressiveness_level: ExpressionLevel::EL,
            has_nominals: false,
            has_cardinality_restrictions: false,
            estimated_complexity: ComplexityLevel::Low,
        };

        let strategy =
            meta_reasoner.select_reasoning_strategy(&features, ReasoningTask::ConsistencyCheck);
        assert!(strategy.is_ok());
        assert_eq!(strategy.unwrap(), ReasoningStrategy::Saturation);
    }

    #[test]
    fn test_decision_tree() {
        let tree = DecisionTree::new();

        let features = OntologyFeatures {
            num_classes: 500,
            num_properties: 100,
            num_individuals: 1000,
            expressiveness_level: ExpressionLevel::SROIQ,
            has_nominals: true,
            has_cardinality_restrictions: true,
            estimated_complexity: ComplexityLevel::High,
        };

        let strategy = tree.select_strategy(&features, ReasoningTask::ConsistencyCheck);
        assert_eq!(strategy, ReasoningStrategy::Tableaux);
    }
}
