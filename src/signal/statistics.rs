use std::collections::HashMap;

pub fn compute_stats(samples: &[f64]) -> HashMap<String, f64> {
    let n = samples.len() as f64;
    let sum: f64 = samples.iter().sum();
    let mean = sum / n;

    let variance = samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();

    let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let rms = (samples.iter().map(|v| v.powi(2)).sum::<f64>() / n).sqrt();
    let crest = if rms > 0.0 { max.abs() / rms } else { 0.0 };

    let skewness = if variance > 0.0 {
        samples
            .iter()
            .map(|v| (v - mean).powi(3))
            .sum::<f64>()
            / (n * std_dev.powi(3))
    } else {
        0.0
    };

    let kurtosis = if variance > 0.0 {
        samples
            .iter()
            .map(|v| (v - mean).powi(4))
            .sum::<f64>()
            / (n * variance.powi(2))
            - 3.0
    } else {
        -3.0
    };

    let mut result = HashMap::new();
    result.insert("count".to_string(), n);
    result.insert("mean".to_string(), mean);
    result.insert("std".to_string(), std_dev);
    result.insert("variance".to_string(), variance);
    result.insert("min".to_string(), min);
    result.insert("max".to_string(), max);
    result.insert("peak_to_peak".to_string(), max - min);
    result.insert("rms".to_string(), rms);
    result.insert("crest_factor".to_string(), crest);
    result.insert("skewness".to_string(), skewness);
    result.insert("kurtosis".to_string(), kurtosis);
    result
}
