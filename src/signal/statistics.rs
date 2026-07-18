use std::collections::HashMap;

use rayon::prelude::*;

pub fn compute_stats(samples: &[f64]) -> HashMap<String, f64> {
    let n = samples.len();
    if n == 0 {
        return HashMap::new();
    }
    let n_f = n as f64;

    // Single pass: accumulate sum, sum_sq, min, max, sum^3, sum^4
    let (sum, sum_sq, min, max, _sum3, _sum4) =
        samples
            .iter()
            .fold((0.0, 0.0, f64::INFINITY, f64::NEG_INFINITY, 0.0, 0.0), |(s, sq, mn, mx, s3, s4), &v| {
                let _d = v;
                (s + v, sq + v * v, mn.min(v), mx.max(v), s3 + v * v * v, s4 + v * v * v * v)
            });

    let mean = sum / n_f;
    let variance = sum_sq / n_f - mean * mean;
    let std_dev = variance.sqrt();
    let rms = (sum_sq / n_f).sqrt();
    let crest = if rms > 0.0 { max.abs() / rms } else { 0.0 };

    // Skewness & kurtosis using centralized moments (single-pass from raw sums)
    let (skewness, kurtosis) = if std_dev > 0.0 {
        let m3 = samples.iter().map(|v| {
            let d = v - mean;
            d * d * d
        }).sum::<f64>() / (n_f * std_dev * std_dev * std_dev);
        let m4 = samples.iter().map(|v| {
            let d = v - mean;
            d * d * d * d
        }).sum::<f64>() / (n_f * variance * variance) - 3.0;
        (m3, m4)
    } else {
        (0.0, -3.0)
    };

    let mut r = HashMap::with_capacity(11);
    r.insert("count".into(), n as f64);
    r.insert("mean".into(), mean);
    r.insert("std".into(), std_dev);
    r.insert("variance".into(), variance);
    r.insert("min".into(), min);
    r.insert("max".into(), max);
    r.insert("peak_to_peak".into(), max - min);
    r.insert("rms".into(), rms);
    r.insert("crest_factor".into(), crest);
    r.insert("skewness".into(), skewness);
    r.insert("kurtosis".into(), kurtosis);
    r
}

// ── Parallel stats (Rayon) ─────────────────────────────────

struct ChunkAccum {
    count: usize,
    sum: f64,
    sum_sq: f64,
    min: f64,
    max: f64,
    m3: f64,
    m4: f64,
}

fn merge_chunks(a: ChunkAccum, b: ChunkAccum) -> ChunkAccum {
    let n_a = a.count as f64;
    let n_b = b.count as f64;
    let n = a.count + b.count;
    if n == 0 {
        return ChunkAccum { count: 0, sum: 0.0, sum_sq: 0.0, min: f64::INFINITY, max: f64::NEG_INFINITY, m3: 0.0, m4: 0.0 };
    }
    let n_f = n as f64;
    let delta = b.sum / n_b - a.sum / n_a;
    let sum = a.sum + b.sum;
    let sum_sq = a.sum_sq + b.sum_sq;

    // Parallel skewness/kurtosis merge (approximate from chunk moments)
    let m3 = a.m3 + b.m3 + delta * delta * delta * n_a * n_b * (n_a - n_b) / (n_f * n_f * n_f);
    let m4 = a.m4 + b.m4;

    ChunkAccum { count: n, sum, sum_sq, min: a.min.min(b.min), max: a.max.max(b.max), m3, m4 }
}

pub fn compute_stats_parallel(samples: &[f64]) -> HashMap<String, f64> {
    let n = samples.len();
    if n == 0 {
        return HashMap::new();
    }

    let chunk_size = (n / rayon::current_num_threads().max(1)).max(1024);

    let acc = samples
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut s = 0.0;
            let mut sq = 0.0;
            let mut mn = f64::INFINITY;
            let mut mx = f64::NEG_INFINITY;
            for &v in chunk {
                s += v;
                sq += v * v;
                mn = mn.min(v);
                mx = mx.max(v);
            }
            ChunkAccum { count: chunk.len(), sum: s, sum_sq: sq, min: mn, max: mx, m3: 0.0, m4: 0.0 }
        })
        .reduce(|| ChunkAccum { count: 0, sum: 0.0, sum_sq: 0.0, min: f64::INFINITY, max: f64::NEG_INFINITY, m3: 0.0, m4: 0.0 },
                merge_chunks);

    let n_f = n as f64;
    let mean = acc.sum / n_f;
    let variance = acc.sum_sq / n_f - mean * mean;
    let std_dev = variance.abs().sqrt();
    let rms = (acc.sum_sq / n_f).sqrt();
    let crest = if rms > 0.0 { acc.max.abs() / rms } else { 0.0 };

    // Recompute skewness/kurtosis from raw data with known mean (parallel)
    let (m3_sum, m4_sum): (f64, f64) = samples
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut s3 = 0.0;
            let mut s4 = 0.0;
            for &v in chunk {
                let d = v - mean;
                s3 += d * d * d;
                s4 += d * d * d * d;
            }
            (s3, s4)
        })
        .reduce(|| (0.0, 0.0), |(a3, b3), (a4, b4)| (a3 + a4, b3 + b4));

    let (skewness, kurtosis) = if std_dev > 0.0 {
        let skew = m3_sum / (n_f * std_dev * std_dev * std_dev);
        let kurt = m4_sum / (n_f * variance * variance) - 3.0;
        (skew, kurt)
    } else {
        (0.0, -3.0)
    };

    let mut r = HashMap::with_capacity(11);
    r.insert("count".into(), n as f64);
    r.insert("mean".into(), mean);
    r.insert("std".into(), std_dev);
    r.insert("variance".into(), variance);
    r.insert("min".into(), acc.min);
    r.insert("max".into(), acc.max);
    r.insert("peak_to_peak".into(), acc.max - acc.min);
    r.insert("rms".into(), rms);
    r.insert("crest_factor".into(), crest);
    r.insert("skewness".into(), skewness);
    r.insert("kurtosis".into(), kurtosis);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10
    }

    #[test]
    fn basic_stats() {
        let s = compute_stats(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(s["count"], 5.0);
        assert!(close(s["mean"], 3.0));
        assert!(close(s["min"], 1.0));
        assert!(close(s["max"], 5.0));
        assert!(close(s["peak_to_peak"], 4.0));
        assert!(close(s["variance"], 2.0));
    }

    #[test]
    fn empty_input() {
        let s = compute_stats(&[]);
        assert!(s.is_empty());
    }

    #[test]
    fn single_value() {
        let s = compute_stats(&[42.0]);
        assert!(close(s["mean"], 42.0));
        assert!(close(s["std"], 0.0));
        assert!(close(s["rms"], 42.0));
    }

    #[test]
    fn rms_known() {
        // [0, 1] → rms = sqrt(0.5)
        let s = compute_stats(&[0.0, 1.0]);
        assert!(close(s["rms"], (0.5_f64).sqrt()));
    }

    #[test]
    fn parallel_matches_sequential() {
        let data: Vec<f64> = (0..10000).map(|i| i as f64 * 0.1).collect();
        let seq = compute_stats(&data);
        let par = compute_stats_parallel(&data);
        assert_eq!(seq["count"], par["count"]);
        assert!(close(seq["mean"], par["mean"]));
        assert!(close(seq["std"], par["std"]));
        assert!(close(seq["min"], par["min"]));
        assert!(close(seq["max"], par["max"]));
        assert!(close(seq["rms"], par["rms"]));
    }

    #[test]
    fn parallel_empty() {
        let s = compute_stats_parallel(&[]);
        assert!(s.is_empty());
    }

    #[test]
    fn parallel_single_value() {
        let s = compute_stats_parallel(&[42.0]);
        assert!(close(s["mean"], 42.0));
        assert!(close(s["std"], 0.0));
    }
}
