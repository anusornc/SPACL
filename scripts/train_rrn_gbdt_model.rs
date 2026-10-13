use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

const FEATURE_NAMES: [&str; 8] = [
    "union_nodes",
    "intersection_nodes",
    "complement_nodes",
    "some_values_nodes",
    "all_values_nodes",
    "atom_nodes",
    "max_depth",
    "total_nodes",
];

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
    feature_rows: Vec<[f64; 8]>,
    targets: Vec<f64>,
}

#[derive(Debug, Clone)]
struct Sample {
    features: [f64; 8],
    target: f64,
}

#[derive(Debug, Clone, Serialize)]
struct GbdtStump {
    feature: String,
    threshold: f64,
    left_value: f64,
    right_value: f64,
}

#[derive(Debug, Clone, Serialize)]
struct TrainedRrnGbdtModel {
    model_type: String,
    base_score: f64,
    learning_rate: f64,
    stumps: Vec<GbdtStump>,
    training_samples: usize,
    source_policy_mode: String,
    trees_requested: usize,
    trees_trained: usize,
    max_thresholds_per_feature: usize,
    min_leaf_size: usize,
    ranking_records: usize,
    ranking_pairs: usize,
    pairwise_accuracy: f64,
    mse: f64,
}

#[derive(Debug, Clone, Copy)]
struct BestStump {
    feature_idx: usize,
    threshold: f64,
    left_value: f64,
    right_value: f64,
    sse: f64,
}

fn print_usage() {
    eprintln!(
        "Usage: train_rrn_gbdt_model <snapshot.jsonl> <output_model.json> [policy_mode]\n\
         Example:\n\
         cargo run --bin train_rrn_gbdt_model -- \\\n\
           benchmarks/competitors/results/history/<RUN_ID>/branch_snapshots.jsonl \\\n\
           benchmarks/models/rrn_gbdt_stump_model.json heuristic"
    );
}

fn feature_vector(f: &SnapshotOperandFeatures) -> [f64; 8] {
    [
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

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn sample_thresholds(mut values: Vec<f64>, max_thresholds: usize) -> Vec<f64> {
    values.sort_by(|a, b| a.total_cmp(b));
    values.dedup_by(|a, b| (*a - *b).abs() <= f64::EPSILON);
    if values.len() <= 1 {
        return Vec::new();
    }

    let mut cuts = Vec::with_capacity(values.len().saturating_sub(1));
    for idx in 0..(values.len() - 1) {
        cuts.push((values[idx] + values[idx + 1]) * 0.5);
    }
    if cuts.len() <= max_thresholds {
        return cuts;
    }

    let mut sampled = Vec::with_capacity(max_thresholds);
    let last = cuts.len() - 1;
    for i in 0..max_thresholds {
        let pos = i * last / (max_thresholds - 1).max(1);
        sampled.push(cuts[pos]);
    }
    sampled.dedup_by(|a, b| (*a - *b).abs() <= f64::EPSILON);
    sampled
}

fn fit_best_stump(
    samples: &[Sample],
    residuals: &[f64],
    max_thresholds: usize,
    min_leaf_size: usize,
) -> Option<BestStump> {
    let mut best: Option<BestStump> = None;

    for feature_idx in 0..FEATURE_NAMES.len() {
        let thresholds = sample_thresholds(
            samples
                .iter()
                .map(|sample| sample.features[feature_idx])
                .collect(),
            max_thresholds,
        );

        for threshold in thresholds {
            let mut left_sum = 0.0;
            let mut right_sum = 0.0;
            let mut left_count = 0usize;
            let mut right_count = 0usize;

            for (sample, residual) in samples.iter().zip(residuals.iter()) {
                if sample.features[feature_idx] <= threshold {
                    left_sum += *residual;
                    left_count += 1;
                } else {
                    right_sum += *residual;
                    right_count += 1;
                }
            }

            if left_count < min_leaf_size || right_count < min_leaf_size {
                continue;
            }

            let left_value = left_sum / left_count as f64;
            let right_value = right_sum / right_count as f64;
            let mut sse = 0.0;
            for (sample, residual) in samples.iter().zip(residuals.iter()) {
                let pred = if sample.features[feature_idx] <= threshold {
                    left_value
                } else {
                    right_value
                };
                let err = residual - pred;
                sse += err * err;
            }

            let candidate = BestStump {
                feature_idx,
                threshold,
                left_value,
                right_value,
                sse,
            };
            if best.map(|b| candidate.sse < b.sse).unwrap_or(true) {
                best = Some(candidate);
            }
        }
    }

    best
}

fn score_with_model(
    base_score: f64,
    learning_rate: f64,
    stumps: &[GbdtStump],
    x: &[f64; 8],
) -> f64 {
    let mut score = base_score;
    for stump in stumps {
        let feature_idx = FEATURE_NAMES
            .iter()
            .position(|name| *name == stump.feature)
            .unwrap_or(FEATURE_NAMES.len() - 1);
        let delta = if x[feature_idx] <= stump.threshold {
            stump.left_value
        } else {
            stump.right_value
        };
        score += learning_rate * delta;
    }
    score
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

    let trees_requested: usize = env::var("RRN_GBDT_TREES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(64);
    let learning_rate: f64 = env::var("RRN_GBDT_LR")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(0.08);
    let max_thresholds_per_feature: usize = env::var("RRN_GBDT_MAX_THRESHOLDS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 1)
        .unwrap_or(32);
    let min_leaf_size: usize = env::var("RRN_GBDT_MIN_LEAF")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(8);

    let input_file = fs::File::open(input_path)?;
    let reader = BufReader::new(input_file);

    let mut samples: Vec<Sample> = Vec::new();
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
            samples.push(Sample {
                features: fv,
                target,
            });
            feature_rows.push(fv);
            targets.push(target);
        }

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

    let targets: Vec<f64> = samples.iter().map(|sample| sample.target).collect();
    let base_score = mean(&targets);
    let mut predictions = vec![base_score; samples.len()];
    let mut stumps = Vec::new();

    for _ in 0..trees_requested {
        let residuals: Vec<f64> = samples
            .iter()
            .zip(predictions.iter())
            .map(|(sample, pred)| sample.target - pred)
            .collect();

        let Some(best) = fit_best_stump(
            &samples,
            &residuals,
            max_thresholds_per_feature,
            min_leaf_size,
        ) else {
            break;
        };

        for (sample, pred) in samples.iter().zip(predictions.iter_mut()) {
            let delta = if sample.features[best.feature_idx] <= best.threshold {
                best.left_value
            } else {
                best.right_value
            };
            *pred += learning_rate * delta;
        }

        stumps.push(GbdtStump {
            feature: FEATURE_NAMES[best.feature_idx].to_string(),
            threshold: best.threshold,
            left_value: best.left_value,
            right_value: best.right_value,
        });
    }

    let mse = samples
        .iter()
        .zip(predictions.iter())
        .map(|(sample, pred)| {
            let err = sample.target - pred;
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
                let p_i =
                    score_with_model(base_score, learning_rate, &stumps, &record.feature_rows[i]);
                let p_j =
                    score_with_model(base_score, learning_rate, &stumps, &record.feature_rows[j]);
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

    let output = TrainedRrnGbdtModel {
        model_type: "gbdt_stump_v1".to_string(),
        base_score,
        learning_rate,
        stumps: stumps.clone(),
        training_samples: samples.len(),
        source_policy_mode: policy_mode,
        trees_requested,
        trees_trained: stumps.len(),
        max_thresholds_per_feature,
        min_leaf_size,
        ranking_records: ranking_records.len(),
        ranking_pairs: pair_total,
        pairwise_accuracy,
        mse,
    };

    if let Some(parent) = Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(output_path, serde_json::to_string_pretty(&output)?)?;

    eprintln!(
        "[rrn-gbdt-train] wrote model={} records={} used={} samples={} trees={}/{} pair_acc={:.4} mse={:.6}",
        output_path,
        records_seen,
        records_used,
        samples.len(),
        stumps.len(),
        trees_requested,
        pairwise_accuracy,
        mse
    );

    Ok(())
}
