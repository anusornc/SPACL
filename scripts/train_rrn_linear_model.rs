use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
struct BranchSnapshotRecord {
    policy_mode: String,
    operand_scores: Vec<f64>,
    operand_features: Vec<SnapshotOperandFeatures>,
    #[serde(default)]
    ordered_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct RankingRecord {
    feature_rows: Vec<[f64; 9]>,
    targets: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrainObjective {
    Regression,
    Pairwise,
}

impl TrainObjective {
    fn as_str(&self) -> &'static str {
        match self {
            TrainObjective::Regression => "regression",
            TrainObjective::Pairwise => "pairwise",
        }
    }

    fn from_env() -> Self {
        match env::var("RRN_TRAIN_OBJECTIVE") {
            Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
                "pairwise" | "rank" | "ranking" => TrainObjective::Pairwise,
                _ => TrainObjective::Regression,
            },
            Err(_) => TrainObjective::Regression,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct TrainedRrnLinearWeights {
    bias: f64,
    union_weight: f64,
    intersection_weight: f64,
    complement_weight: f64,
    some_values_weight: f64,
    all_values_weight: f64,
    atom_weight: f64,
    depth_weight: f64,
    node_weight: f64,
    training_samples: usize,
    source_policy_mode: String,
    epochs: usize,
    learning_rate: f64,
    objective: String,
    ranking_records: usize,
    ranking_pairs: usize,
    pairwise_accuracy: f64,
}

fn print_usage() {
    eprintln!(
        "Usage: train_rrn_linear_model <snapshot.jsonl> <output_model.json> [policy_mode]\n\
         Example:\n\
         cargo run --bin train_rrn_linear_model -- \\\n           benchmarks/competitors/results/history/<RUN_ID>/branch_snapshots.jsonl \\\n           benchmarks/models/rrn_linear_model.json heuristic"
    );
}

fn feature_vector(f: &SnapshotOperandFeatures) -> [f64; 9] {
    [
        1.0,
        f.union_nodes as f64,
        f.intersection_nodes as f64,
        f.complement_nodes as f64,
        f.some_values_nodes as f64,
        f.all_values_nodes as f64,
        f.atom_nodes as f64,
        f.max_depth as f64,
        f.total_nodes as f64,
    ]
}

fn dot(weights: &[f64; 9], x: &[f64; 9]) -> f64 {
    weights.iter().zip(x.iter()).map(|(w, v)| w * v).sum()
}

fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        let z = (-x).exp();
        1.0 / (1.0 + z)
    } else {
        let z = x.exp();
        z / (1.0 + z)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        print_usage();
        std::process::exit(2);
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let policy_mode = if args.len() >= 4 {
        args[3].trim().to_string()
    } else {
        "heuristic".to_string()
    };

    let epochs: usize = env::var("RRN_TRAIN_EPOCHS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(400);
    let learning_rate: f64 = env::var("RRN_TRAIN_LR")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(0.00005);
    let objective = TrainObjective::from_env();
    let max_pairs_per_record: usize = env::var("RRN_PAIRWISE_MAX_PAIRS_PER_RECORD")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(128);

    let input_file = fs::File::open(input_path)?;
    let reader = BufReader::new(input_file);

    let mut samples: Vec<([f64; 9], f64)> = Vec::new();
    let mut ranking_records: Vec<RankingRecord> = Vec::new();
    let mut records_seen = 0usize;
    let mut records_used = 0usize;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        records_seen += 1;
        let record: BranchSnapshotRecord = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if record.policy_mode != policy_mode {
            continue;
        }
        records_used += 1;
        let n = record
            .operand_scores
            .len()
            .min(record.operand_features.len());
        if n == 0 {
            continue;
        }

        let mut feature_rows = Vec::with_capacity(n);
        let mut targets = Vec::with_capacity(n);
        for idx in 0..n {
            let fv = feature_vector(&record.operand_features[idx]);
            let target = record.operand_scores[idx];
            feature_rows.push(fv);
            targets.push(target);
            samples.push((
                fv,
                target,
            ));
        }
        // Keep ranking snapshots only when order metadata exists and is consistent.
        if !record.ordered_indices.is_empty() && record.ordered_indices.len() >= n {
            ranking_records.push(RankingRecord {
                feature_rows,
                targets,
            });
        }
    }

    if samples.is_empty() {
        return Err(format!(
            "no training samples found for policy_mode='{}' in {} records from {}",
            policy_mode, records_seen, input_path
        )
        .into());
    }

    let mut weights = [0.0_f64; 9];
    let mut ranking_pairs = 0usize;
    match objective {
        TrainObjective::Regression => {
            for _ in 0..epochs {
                for (x, y) in &samples {
                    let prediction = dot(&weights, x);
                    let err = prediction - y;
                    for j in 0..weights.len() {
                        weights[j] -= learning_rate * err * x[j];
                    }
                }
            }
        }
        TrainObjective::Pairwise => {
            if ranking_records.is_empty() {
                return Err(
                    "pairwise objective selected but no ranking records were available".into(),
                );
            }
            for _ in 0..epochs {
                for record in &ranking_records {
                    let n = record.feature_rows.len();
                    let mut pair_budget = 0usize;
                    for i in 0..n {
                        for j in (i + 1)..n {
                            let y_i = record.targets[i];
                            let y_j = record.targets[j];
                            if y_i <= y_j {
                                continue;
                            }
                            let mut diff = [0.0_f64; 9];
                            for k in 0..diff.len() {
                                diff[k] = record.feature_rows[i][k] - record.feature_rows[j][k];
                            }
                            let s = dot(&weights, &diff);
                            let grad_scale = sigmoid(-s) * (y_i - y_j).max(1.0);
                            for k in 0..weights.len() {
                                weights[k] += learning_rate * grad_scale * diff[k];
                            }
                            pair_budget += 1;
                            ranking_pairs += 1;
                            if pair_budget >= max_pairs_per_record {
                                break;
                            }
                        }
                        if pair_budget >= max_pairs_per_record {
                            break;
                        }
                    }
                }
            }
        }
    }

    let mse = samples
        .iter()
        .map(|(x, y)| {
            let err = dot(&weights, x) - y;
            err * err
        })
        .sum::<f64>()
        / samples.len() as f64;

    let mut pair_total = 0usize;
    let mut pair_correct = 0usize;
    for record in &ranking_records {
        let n = record.feature_rows.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let y_i = record.targets[i];
                let y_j = record.targets[j];
                if y_i <= y_j {
                    continue;
                }
                pair_total += 1;
                let p_i = dot(&weights, &record.feature_rows[i]);
                let p_j = dot(&weights, &record.feature_rows[j]);
                if p_i > p_j {
                    pair_correct += 1;
                }
            }
        }
    }
    let pairwise_accuracy = if pair_total == 0 {
        0.0
    } else {
        pair_correct as f64 / pair_total as f64
    };

    let output = TrainedRrnLinearWeights {
        bias: weights[0],
        union_weight: weights[1],
        intersection_weight: weights[2],
        complement_weight: weights[3],
        some_values_weight: weights[4],
        all_values_weight: weights[5],
        atom_weight: weights[6],
        depth_weight: weights[7],
        node_weight: weights[8],
        training_samples: samples.len(),
        source_policy_mode: policy_mode,
        epochs,
        learning_rate,
        objective: objective.as_str().to_string(),
        ranking_records: ranking_records.len(),
        ranking_pairs,
        pairwise_accuracy,
    };

    if let Some(parent) = Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(output_path, serde_json::to_string_pretty(&output)?)?;

    eprintln!(
        "[rrn-train] wrote model={} objective={} records={} used={} samples={} rank_records={} rank_pairs={} pair_acc={:.4} mse={:.6}",
        output_path,
        objective.as_str(),
        records_seen,
        records_used,
        samples.len(),
        ranking_records.len(),
        ranking_pairs,
        pairwise_accuracy,
        mse
    );

    Ok(())
}
