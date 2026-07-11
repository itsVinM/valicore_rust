use std::collections::HashMap;

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
}
