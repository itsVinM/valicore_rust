use std::f64::consts::PI;

// ── Single window function with a macro ────────────────────
// Every window is just: f(i / (N-1)). The macro eliminates
// the copy-paste between const-generic and runtime versions.

macro_rules! window_fn {
    ($name:ident, $t:ident, $expr:expr) => {
        pub fn $name<const N: usize>() -> [f64; N] {
            assert!(N > 1);
            let mut w = [0.0; N];
            let n1 = (N - 1) as f64;
            for (i, v) in w.iter_mut().enumerate() {
                let $t = i as f64 / n1;
                *v = $expr;
            }
            w
        }
    };
}

window_fn!(hamming_window, t, 0.54 - 0.46 * (2.0 * PI * t).cos());
window_fn!(hann_window, t, 0.5 * (1.0 - (2.0 * PI * t).cos()));
window_fn!(blackman_window, t, 0.42 - 0.5 * (2.0 * PI * t).cos() + 0.08 * (4.0 * PI * t).cos());
window_fn!(flat_top_window, t, 0.21557895
    - 0.41663158 * (2.0 * PI * t).cos()
    + 0.277263158 * (4.0 * PI * t).cos()
    - 0.083578947 * (6.0 * PI * t).cos()
    + 0.006947368 * (8.0 * PI * t).cos());

// ── Runtime windowing ─────────────────────────────────────
// Same formula, just evaluated at runtime for arbitrary N.

type WinFn = fn(usize, usize) -> f64;

fn hann(i: usize, n: usize) -> f64 { 0.5 * (1.0 - (2.0 * PI * i as f64 / (n - 1) as f64).cos()) }
fn hamming(i: usize, n: usize) -> f64 { 0.54 - 0.46 * (2.0 * PI * i as f64 / (n - 1) as f64).cos() }
fn blackman(i: usize, n: usize) -> f64 {
    let t = 2.0 * PI * i as f64 / (n - 1) as f64;
    0.42 - 0.5 * t.cos() + 0.08 * (2.0 * t).cos()
}
fn flat_top(i: usize, n: usize) -> f64 {
    let t = 2.0 * PI * i as f64 / (n - 1) as f64;
    0.21557895 - 0.41663158 * t.cos() + 0.277263158 * (2.0*t).cos()
        - 0.083578947 * (3.0*t).cos() + 0.006947368 * (4.0*t).cos()
}
fn rect(_i: usize, _n: usize) -> f64 { 1.0 }

fn resolve_window(name: &str) -> Result<WinFn, String> {
    match name.to_lowercase().as_str() {
        "hann" | "hanning" => Ok(hann),
        "hamming" => Ok(hamming),
        "blackman" => Ok(blackman),
        "flat_top" => Ok(flat_top),
        "none" | "rectangular" => Ok(rect),
        _ => Err(format!("unknown window type: {name}")),
    }
}

pub fn apply_window(samples: &[f64], window_type: &str) -> Result<Vec<f64>, String> {
    if samples.is_empty() { return Err("empty signal".into()); }
    let f = resolve_window(window_type)?;
    let n = samples.len();
    Ok(samples.iter().enumerate().map(|(i, &s)| s * f(i, n)).collect())
}

// ── Filter ────────────────────────────────────────────────

pub fn apply_filter(samples: &[f64], filter_type: &str, cutoff: f64, order: u32) -> Result<Vec<f64>, String> {
    if samples.is_empty() { return Err("empty signal".into()); }
    if !(0.0..=1.0).contains(&cutoff) { return Err("cutoff must be in (0, 1]".into()); }
    if order == 0 || order > 8 { return Err("filter order must be 1..=8".into()); }

    match filter_type.to_lowercase().as_str() {
        "lowpass" => { let rc = 1.0 / (cutoff * 2.0 * PI); let alpha = 1.0 / (rc + 1.0); Ok(filter_pass(samples, order, alpha, true)) }
        "highpass" => { let rc = 1.0 / (cutoff * 2.0 * PI); let alpha = rc / (rc + 1.0); Ok(filter_pass(samples, order, alpha, false)) }
        _ => Err(format!("unknown filter type: {filter_type}")),
    }
}

fn filter_pass(samples: &[f64], order: u32, alpha: f64, lowpass: bool) -> Vec<f64> {
    let mut r = samples.to_vec();
    for _ in 0..order {
        if lowpass {
            let mut y = r[0];
            for x in r.iter_mut() { y += alpha * (*x - y); *x = y; }
        } else {
            let mut yp = r[0];
            for i in 1..r.len() {
                let x = r[i];
                r[i] = alpha * (r[i - 1] + x - yp);
                yp = x;
            }
            r[0] = 0.0;
        }
    }
    r
}

// ── Cross-correlation ─────────────────────────────────────

pub fn cross_correlation(a: &[f64], b: &[f64]) -> Vec<f64> {
    let n = a.len();
    let mean_a = a.iter().sum::<f64>() / n as f64;
    let mean_b = b.iter().sum::<f64>() / n as f64;
    let mut result = vec![0.0; n];

    // Precompute centered b once
    let bc: Vec<f64> = b.iter().map(|v| v - mean_b).collect();

    for shift in 0..n {
        let len = n - shift;
        let sum: f64 = (0..len)
            .map(|i| (a[i] - mean_a) * bc[i + shift])
            .sum();
        result[shift] = sum / len as f64;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool { (a - b).abs() < 1e-10 }

    #[test]
    fn hann_window_length() {
        let w = hann_window::<64>();
        assert_eq!(w.len(), 64);
        assert!(close(w[0], 0.0));
        assert!(close(w[63], 0.0));
        // Peak at i=31 for even N=64: 0.5*(1+cos(pi/63)) ≈ 0.99938
        assert!(w[31] > 0.99);
        assert!(w[32] > 0.99);
    }

    #[test]
    fn hamming_window_endpoints() {
        let w = hamming_window::<8>();
        assert!(close(w[0], 0.08));
        assert!(close(w[7], 0.08));
    }

    #[test]
    fn apply_window_none() {
        let s = vec![1.0, 2.0, 3.0];
        assert_eq!(apply_window(&s, "none").unwrap(), s);
    }

    #[test]
    fn apply_window_unknown() {
        assert!(apply_window(&[1.0], "foo").is_err());
    }

    #[test]
    fn lowpass_smoothing() {
        let s = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let r = apply_filter(&s, "lowpass", 0.5, 1).unwrap();
        // After lowpass the transition should be smoothed
        assert!(r[2] < r[3]);
    }

    #[test]
    fn cross_correlation_peak_at_zero() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r = cross_correlation(&a, &a);
        // Autocorrelation peak at shift=0
        assert!(r[0] >= r[1]);
    }

    #[test]
    fn filter_validation() {
        assert!(apply_filter(&[1.0], "lowpass", -0.1, 1).is_err());
        assert!(apply_filter(&[1.0], "lowpass", 0.5, 0).is_err());
        assert!(apply_filter(&[1.0], "lowpass", 0.5, 9).is_err());
        assert!(apply_filter(&[1.0], "bandpass", 0.5, 1).is_err());
    }
}
