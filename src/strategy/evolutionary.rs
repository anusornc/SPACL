//! Evolutionary optimization for reasoning algorithms
//!
//! This module implements evolutionary algorithms to optimize reasoning
//! heuristics and meta-reasoner decision making.

use crate::strategy::meta_reasoner::ReasoningStrategy;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Evolutionary optimizer for reasoning algorithms
pub struct EvolutionaryOptimizer {
    /// Population of reasoning strategies
    population: Vec<EvolutionaryStrategy>,
    /// Population size
    population_size: usize,
    /// Mutation rate
    mutation_rate: f64,
    /// Crossover rate
    crossover_rate: f64,
    /// Current generation
    generation: usize,
    /// Random number generator
    rng: ThreadRng,
}

/// Evolutionary strategy representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionaryStrategy {
    /// Strategy parameters
    pub parameters: StrategyParameters,
    /// Fitness score
    pub fitness: f64,
    /// Performance history
    pub performance_history: Vec<PerformanceRecord>,
}

/// Strategy parameters that can be evolved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParameters {
    /// Weights for different ontology features in decision making
    pub feature_weights: HashMap<String, f64>,
    /// Thresholds for strategy selection
    pub selection_thresholds: HashMap<String, f64>,
    /// Reasoning timeouts
    pub timeouts: HashMap<ReasoningStrategy, u64>,
    /// Cache configuration
    pub cache_config: CacheConfig,
}

/// Cache configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Primary cache size
    pub primary_cache_size: usize,
    /// Secondary cache size
    pub secondary_cache_size: usize,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Eviction policy weight
    pub eviction_weight: f64,
}

/// Performance record for fitness evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecord {
    pub execution_time_ms: u64,
    pub memory_usage_mb: f64,
    pub success: bool,
    pub ontology_features: String, // Serialized features
    pub reasoning_task: String,
}

impl EvolutionaryOptimizer {
    /// Create a new evolutionary optimizer
    pub fn new(population_size: usize) -> Self {
        let mut optimizer = EvolutionaryOptimizer {
            population: Vec::new(),
            population_size,
            mutation_rate: 0.1,
            crossover_rate: 0.7,
            generation: 0,
            rng: thread_rng(),
        };

        // Initialize random population
        optimizer.initialize_population();

        optimizer
    }

    /// Initialize random population
    fn initialize_population(&mut self) {
        self.population.clear();

        for _ in 0..self.population_size {
            let strategy = self.create_random_strategy();
            self.population.push(strategy);
        }
    }

    /// Create a random strategy
    fn create_random_strategy(&mut self) -> EvolutionaryStrategy {
        let mut feature_weights = HashMap::new();
        feature_weights.insert("num_classes".to_string(), self.rng.gen_range(0.0..1.0));
        feature_weights.insert("num_properties".to_string(), self.rng.gen_range(0.0..1.0));
        feature_weights.insert("num_individuals".to_string(), self.rng.gen_range(0.0..1.0));
        feature_weights.insert("expressiveness".to_string(), self.rng.gen_range(0.0..1.0));
        feature_weights.insert("complexity".to_string(), self.rng.gen_range(0.0..1.0));

        let mut selection_thresholds = HashMap::new();
        selection_thresholds.insert(
            "tableaux_threshold".to_string(),
            self.rng.gen_range(0.3..0.9),
        );
        selection_thresholds.insert(
            "saturation_threshold".to_string(),
            self.rng.gen_range(0.2..0.8),
        );
        selection_thresholds.insert(
            "transformation_threshold".to_string(),
            self.rng.gen_range(0.1..0.7),
        );

        let mut timeouts = HashMap::new();
        timeouts.insert(ReasoningStrategy::Tableaux, self.rng.gen_range(5000..60000));
        timeouts.insert(
            ReasoningStrategy::Saturation,
            self.rng.gen_range(1000..30000),
        );
        timeouts.insert(
            ReasoningStrategy::Transformation,
            self.rng.gen_range(2000..45000),
        );
        timeouts.insert(ReasoningStrategy::Hybrid, self.rng.gen_range(10000..90000));

        let cache_config = CacheConfig {
            primary_cache_size: self.rng.gen_range(100..2000),
            secondary_cache_size: self.rng.gen_range(500..5000),
            cache_ttl_seconds: self.rng.gen_range(300..3600),
            eviction_weight: self.rng.gen_range(0.1..0.9),
        };

        let parameters = StrategyParameters {
            feature_weights,
            selection_thresholds,
            timeouts,
            cache_config,
        };

        EvolutionaryStrategy {
            parameters,
            fitness: 0.0,
            performance_history: Vec::new(),
        }
    }

    /// Evolve the population for one generation
    pub fn evolve_generation(&mut self) -> anyhow::Result<()> {
        // Selection
        self.selection();

        // Crossover
        self.crossover()?;

        // Mutation
        self.mutation();

        self.generation += 1;

        Ok(())
    }

    /// Selection phase - keep the fittest individuals
    fn selection(&mut self) {
        // Sort by fitness (descending)
        self.population
            .sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());

        // Keep top 50% of population
        let keep_size = self.population_size / 2;
        self.population.truncate(keep_size);
    }

    /// Crossover phase - create offspring from parents
    fn crossover(&mut self) -> anyhow::Result<()> {
        let current_size = self.population.len();
        let mut offspring = Vec::new();

        while offspring.len() + current_size < self.population_size {
            if self.rng.gen::<f64>() < self.crossover_rate {
                // Select two parents randomly
                let parent1_idx = self.rng.gen_range(0..current_size);
                let parent2_idx = self.rng.gen_range(0..current_size);

                if parent1_idx != parent2_idx {
                    // Clone parents to avoid borrow issues
                    let parent1 = self.population[parent1_idx].clone();
                    let parent2 = self.population[parent2_idx].clone();
                    let child = self.create_offspring(&parent1, &parent2)?;
                    offspring.push(child);
                }
            }
        }

        self.population.extend(offspring);
        Ok(())
    }

    /// Create offspring from two parents
    fn create_offspring(
        &mut self,
        parent1: &EvolutionaryStrategy,
        parent2: &EvolutionaryStrategy,
    ) -> anyhow::Result<EvolutionaryStrategy> {
        let mut child_params = parent1.parameters.clone();

        // Crossover feature weights
        for (key, value1) in &parent1.parameters.feature_weights {
            if let Some(value2) = parent2.parameters.feature_weights.get(key) {
                let alpha = self.rng.gen::<f64>();
                let new_value = alpha * value1 + (1.0 - alpha) * value2;
                child_params.feature_weights.insert(key.clone(), new_value);
            }
        }

        // Crossover selection thresholds
        for (key, value1) in &parent1.parameters.selection_thresholds {
            if let Some(value2) = parent2.parameters.selection_thresholds.get(key) {
                let alpha = self.rng.gen::<f64>();
                let new_value = alpha * value1 + (1.0 - alpha) * value2;
                child_params
                    .selection_thresholds
                    .insert(key.clone(), new_value);
            }
        }

        // Crossover timeouts
        for (strategy, timeout1) in &parent1.parameters.timeouts {
            if let Some(timeout2) = parent2.parameters.timeouts.get(strategy) {
                let alpha = self.rng.gen::<f64>();
                let new_timeout =
                    (alpha * (*timeout1 as f64) + (1.0 - alpha) * (*timeout2 as f64)) as u64;
                child_params.timeouts.insert(*strategy, new_timeout);
            }
        }

        // Crossover cache config
        let alpha = self.rng.gen::<f64>();
        child_params.cache_config.primary_cache_size = ((alpha
            * parent1.parameters.cache_config.primary_cache_size as f64)
            + ((1.0 - alpha) * parent2.parameters.cache_config.primary_cache_size as f64))
            as usize;

        child_params.cache_config.secondary_cache_size = ((alpha
            * parent1.parameters.cache_config.secondary_cache_size as f64)
            + ((1.0 - alpha) * parent2.parameters.cache_config.secondary_cache_size as f64))
            as usize;

        Ok(EvolutionaryStrategy {
            parameters: child_params,
            fitness: 0.0,
            performance_history: Vec::new(),
        })
    }

    /// Mutation phase - introduce random changes
    fn mutation(&mut self) {
        // Use indices to avoid borrow issues
        for i in 0..self.population.len() {
            if self.rng.gen::<f64>() < self.mutation_rate {
                self.mutate_strategy_at_index(i);
            }
        }
    }

    /// Mutate a strategy at a specific index
    fn mutate_strategy_at_index(&mut self, index: usize) {
        if index >= self.population.len() {
            return;
        }

        let strategy = &mut self.population[index];

        // Mutate feature weights
        for (_, weight) in &mut strategy.parameters.feature_weights {
            if self.rng.gen::<f64>() < 0.3 {
                *weight += self.rng.gen_range(-0.1..0.1);
                *weight = weight.clamp(0.0, 1.0);
            }
        }

        // Mutate selection thresholds
        for (_, threshold) in &mut strategy.parameters.selection_thresholds {
            if self.rng.gen::<f64>() < 0.3 {
                *threshold += self.rng.gen_range(-0.1..0.1);
                *threshold = threshold.clamp(0.0, 1.0);
            }
        }

        // Mutate timeouts
        for (_, timeout) in &mut strategy.parameters.timeouts {
            if self.rng.gen::<f64>() < 0.3 {
                let change = self.rng.gen_range(-5000..5000);
                *timeout = (*timeout as i64 + change).max(1000) as u64;
            }
        }

        // Mutate cache config
        if self.rng.gen::<f64>() < 0.3 {
            let change = self.rng.gen_range(-100..100);
            strategy.parameters.cache_config.primary_cache_size =
                (strategy.parameters.cache_config.primary_cache_size as i32 + change).max(50)
                    as usize;
        }
    }

    /// Evaluate fitness of a strategy based on performance records
    pub fn evaluate_fitness(&mut self, strategy_idx: usize, performance: &PerformanceRecord) {
        if strategy_idx < self.population.len() {
            let strategy = &mut self.population[strategy_idx];
            strategy.performance_history.push(performance.clone());

            // Calculate fitness based on performance
            let time_score = if performance.execution_time_ms > 0 {
                1000.0 / performance.execution_time_ms as f64
            } else {
                0.0
            };

            let memory_score = if performance.memory_usage_mb > 0.0 {
                100.0 / performance.memory_usage_mb
            } else {
                0.0
            };

            let success_score = if performance.success { 100.0 } else { 0.0 };

            // Weighted fitness calculation
            let new_fitness = 0.4 * time_score + 0.3 * memory_score + 0.3 * success_score;

            // Update fitness with moving average
            if strategy.fitness == 0.0 {
                strategy.fitness = new_fitness;
            } else {
                strategy.fitness = 0.8 * strategy.fitness + 0.2 * new_fitness;
            }
        }
    }

    /// Get the best strategy from current population
    pub fn get_best_strategy(&self) -> Option<&EvolutionaryStrategy> {
        self.population
            .iter()
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap())
    }

    /// Get current generation number
    pub fn get_generation(&self) -> usize {
        self.generation
    }

    /// Get population statistics
    pub fn get_population_stats(&self) -> PopulationStats {
        if self.population.is_empty() {
            return PopulationStats::default();
        }

        let fitnesses: Vec<f64> = self.population.iter().map(|s| s.fitness).collect();
        let max_fitness = fitnesses.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let min_fitness = fitnesses.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let avg_fitness = fitnesses.iter().sum::<f64>() / fitnesses.len() as f64;

        PopulationStats {
            generation: self.generation,
            population_size: self.population.len(),
            max_fitness,
            min_fitness,
            avg_fitness,
        }
    }
}

/// Population statistics
#[derive(Debug, Default)]
pub struct PopulationStats {
    pub generation: usize,
    pub population_size: usize,
    pub max_fitness: f64,
    pub min_fitness: f64,
    pub avg_fitness: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolutionary_optimizer_creation() {
        let optimizer = EvolutionaryOptimizer::new(20);
        assert_eq!(optimizer.population.len(), 20);
        assert_eq!(optimizer.generation, 0);
    }

    #[test]
    fn test_strategy_creation() {
        let mut optimizer = EvolutionaryOptimizer::new(1);
        let strategy = optimizer.create_random_strategy();

        assert!(!strategy.parameters.feature_weights.is_empty());
        assert!(!strategy.parameters.selection_thresholds.is_empty());
        assert!(!strategy.parameters.timeouts.is_empty());
        assert_eq!(strategy.fitness, 0.0);
    }

    #[test]
    fn test_evolution_generation() {
        let mut optimizer = EvolutionaryOptimizer::new(10);

        // Set some fitness values
        for i in 0..optimizer.population.len() {
            optimizer.population[i].fitness = (i as f64) * 10.0;
        }

        let result = optimizer.evolve_generation();
        assert!(result.is_ok());
        assert_eq!(optimizer.generation, 1);
    }

    #[test]
    fn test_fitness_evaluation() {
        let mut optimizer = EvolutionaryOptimizer::new(5);

        let performance = PerformanceRecord {
            execution_time_ms: 1000,
            memory_usage_mb: 50.0,
            success: true,
            ontology_features: "test".to_string(),
            reasoning_task: "consistency".to_string(),
        };

        optimizer.evaluate_fitness(0, &performance);
        assert!(optimizer.population[0].fitness > 0.0);
        assert_eq!(optimizer.population[0].performance_history.len(), 1);
    }
}
