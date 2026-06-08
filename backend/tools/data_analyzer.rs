// ─────────────────────────────────────────────────────────────
// Data Analyzer — Statistical Analysis Tool
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/data_analyzer.py

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DataSet {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<f64>>,
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub dataset: String,
    pub statistics: HashMap<String, ColumnStats>,
    pub correlations: Vec<Correlation>,
    pub anomalies: Vec<Anomaly>,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct ColumnStats {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub q25: f64,
    pub q75: f64,
}

#[derive(Debug, Clone)]
pub struct Correlation {
    pub column_a: String,
    pub column_b: String,
    pub coefficient: f64,
}

#[derive(Debug, Clone)]
pub struct Anomaly {
    pub row_index: usize,
    pub column: String,
    pub value: f64,
    pub z_score: f64,
}

/// Data analysis tool — computes descriptive statistics, correlations, and anomalies.
pub struct DataAnalyzer {
    anomaly_threshold: f64, // Z-score threshold
    total_analyses: u64,
}

impl DataAnalyzer {
    pub fn new() -> Self {
        Self {
            anomaly_threshold: 3.0,
            total_analyses: 0,
        }
    }

    /// Analyze a dataset.
    pub fn analyze(&mut self, dataset: &DataSet) -> AnalysisResult {
        self.total_analyses += 1;

        let mut statistics = HashMap::new();
        for (i, col) in dataset.columns.iter().enumerate() {
            let values: Vec<f64> = dataset
                .rows
                .iter()
                .filter_map(|row| row.get(i).copied())
                .collect();
            if !values.is_empty() {
                statistics.insert(col.clone(), self.compute_stats(&values));
            }
        }

        let correlations = self.compute_correlations(dataset);
        let anomalies = self.detect_anomalies(dataset, &statistics);

        let summary = format!(
            "Dataset '{}': {} rows × {} columns. Found {} anomalies.",
            dataset.name,
            dataset.rows.len(),
            dataset.columns.len(),
            anomalies.len()
        );

        AnalysisResult {
            dataset: dataset.name.clone(),
            statistics,
            correlations,
            anomalies,
            summary,
        }
    }

    fn compute_stats(&self, values: &[f64]) -> ColumnStats {
        let n = values.len();
        let mean = values.iter().sum::<f64>() / n as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
        let std_dev = variance.sqrt();

        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let median = if n % 2 == 0 {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        } else {
            sorted[n / 2]
        };

        ColumnStats {
            count: n,
            mean,
            median,
            std_dev,
            min: sorted[0],
            max: sorted[n - 1],
            q25: sorted[n / 4],
            q75: sorted[3 * n / 4],
        }
    }

    fn compute_correlations(&self, dataset: &DataSet) -> Vec<Correlation> {
        let mut correlations = Vec::new();
        let ncols = dataset.columns.len();
        for i in 0..ncols {
            for j in (i + 1)..ncols {
                let a: Vec<f64> = dataset
                    .rows
                    .iter()
                    .filter_map(|r| r.get(i).copied())
                    .collect();
                let b: Vec<f64> = dataset
                    .rows
                    .iter()
                    .filter_map(|r| r.get(j).copied())
                    .collect();
                if a.len() == b.len() && !a.is_empty() {
                    let coeff = self.pearson_r(&a, &b);
                    if coeff.abs() > 0.3 {
                        correlations.push(Correlation {
                            column_a: dataset.columns[i].clone(),
                            column_b: dataset.columns[j].clone(),
                            coefficient: coeff,
                        });
                    }
                }
            }
        }
        correlations
    }

    fn pearson_r(&self, a: &[f64], b: &[f64]) -> f64 {
        let n = a.len() as f64;
        let mean_a = a.iter().sum::<f64>() / n;
        let mean_b = b.iter().sum::<f64>() / n;
        let cov: f64 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - mean_a) * (y - mean_b))
            .sum::<f64>()
            / n;
        let std_a = (a.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>() / n).sqrt();
        let std_b = (b.iter().map(|x| (x - mean_b).powi(2)).sum::<f64>() / n).sqrt();
        if std_a == 0.0 || std_b == 0.0 {
            0.0
        } else {
            cov / (std_a * std_b)
        }
    }

    fn detect_anomalies(
        &self,
        dataset: &DataSet,
        stats: &HashMap<String, ColumnStats>,
    ) -> Vec<Anomaly> {
        let mut anomalies = Vec::new();
        for (i, col) in dataset.columns.iter().enumerate() {
            if let Some(s) = stats.get(col) {
                if s.std_dev > 0.0 {
                    for (row_idx, row) in dataset.rows.iter().enumerate() {
                        if let Some(&val) = row.get(i) {
                            let z = (val - s.mean).abs() / s.std_dev;
                            if z > self.anomaly_threshold {
                                anomalies.push(Anomaly {
                                    row_index: row_idx,
                                    column: col.clone(),
                                    value: val,
                                    z_score: z,
                                });
                            }
                        }
                    }
                }
            }
        }
        anomalies
    }

    pub fn total_analyses(&self) -> u64 {
        self.total_analyses
    }
}

impl Default for DataAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
