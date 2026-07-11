use std::f64::consts::PI;

// ── Const-generic window functions (compile-time size) ──

pub fn hamming_window<const N: usize>() -> [f64; N] {
    assert!(N > 1, "window size must be > 1");
    let mut w = [0.0; N];
    for (i, item) in w.iter_mut().enumerate() {
        *item = 0.54 - 0.46 * (2.0 * PI * i as f64 / (N - 1) as f64).cos();
    }
    w
}

pub fn hann_window<const N: usize>() -> [f64; N] {
    assert!(N > 1, "window size must be > 1");
    let mut w = [0.0; N];
    for (i, item) in w.iter_mut().enumerate() {
        *item = 0.5 * (1.0 - (2.0 * PI * i as f64 / (N - 1) as f64).cos());
    }
    w
}

pub fn blackman_window<const N: usize>() -> [f64; N] {
    assert!(N > 1, "window size must be > 1");
    let mut w = [0.0; N];
    for (i, item) in w.iter_mut().enumerate() {
        *item = 0.42 - 0.5 * (2.0 * PI * i as f64 / (N - 1) as f64).cos()
            + 0.08 * (4.0 * PI * i as f64 / (N - 1) as f64).cos();
    }
    w
}

pub fn flat_top_window<const N: usize>() -> [f64; N] {
    assert!(N > 1, "window size must be > 1");
    let mut w = [0.0; N];
    for (i, item) in w.iter_mut().enumerate() {
        *item = 0.21557895
            - 0.41663158 * (2.0 * PI * i as f64 / (N - 1) as f64).cos()
            + 0.277263158 * (4.0 * PI * i as f64 / (N - 1) as f64).cos()
            - 0.083578947 * (6.0 * PI * i as f64 / (N - 1) as f64).cos()
            + 0.006947368 * (8.0 * PI * i as f64 / (N - 1) as f64).cos();
    }
    w
}

// ── Runtime windowing (existing API) ──

pub fn apply_window(samples: &[f64], window_type: &str) -> Result<Vec<f64>, String> {
    if samples.is_empty() {
        return Err("empty signal".into());
    }
    let n = samples.len();
    let window: Vec<f64> = match window_type.to_lowercase().as_str() {
        "hann" | "hanning" => (0..n)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (n - 1) as f64).cos()))
            .collect(),
        "hamming" => (0..n)
            .map(|i| 0.54 - 0.46 * (2.0 * PI * i as f64 / (n - 1) as f64).cos())
            .collect(),
        "blackman" => (0..n)
            .map(|i| {
                0.42 - 0.5 * (2.0 * PI * i as f64 / (n - 1) as f64).cos()
                    + 0.08 * (4.0 * PI * i as f64 / (n - 1) as f64).cos()
            })
            .collect(),
        "flat_top" => (0..n)
            .map(|i| {
                0.21557895
                    - 0.41663158 * (2.0 * PI * i as f64 / (n - 1) as f64).cos()
                    + 0.277263158 * (4.0 * PI * i as f64 / (n - 1) as f64).cos()
                    - 0.083578947 * (6.0 * PI * i as f64 / (n - 1) as f64).cos()
                    + 0.006947368 * (8.0 * PI * i as f64 / (n - 1) as f64).cos()
            })
            .collect(),
        "none" | "rectangular" => vec![1.0; n],
        _ => return Err(format!("unknown window type: {window_type}")),
    };

    Ok(samples.iter().zip(window.iter()).map(|(s, w)| s * w).collect())
}

pub fn apply_filter(
    samples: &[f64],
    filter_type: &str,
    cutoff: f64,
    order: u32,
) -> Result<Vec<f64>, String> {
    if samples.is_empty() {
        return Err("empty signal".into());
    }
    if !(0.0..=1.0).contains(&cutoff) {
        return Err("cutoff must be in (0, 1] as fraction of Nyquist".into());
    }
    if order == 0 || order > 8 {
        return Err("filter order must be 1..=8".into());
    }

    match filter_type.to_lowercase().as_str() {
        "lowpass" => Ok(lowpass(samples, cutoff, order)),
        "highpass" => Ok(highpass(samples, cutoff, order)),
        _ => Err(format!("unknown filter type: {filter_type}")),
    }
}

fn lowpass(samples: &[f64], cutoff: f64, order: u32) -> Vec<f64> {
    let rc = 1.0 / (cutoff * 2.0 * PI);
    let dt = 1.0;
    let alpha = dt / (rc + dt);

    let mut result = samples.to_vec();
    for _ in 0..order {
        let mut y = result[0];
        for x in result.iter_mut() {
            y += alpha * (*x - y);
            *x = y;
        }
    }
    result
}

fn highpass(samples: &[f64], cutoff: f64, order: u32) -> Vec<f64> {
    let rc = 1.0 / (cutoff * 2.0 * PI);
    let dt = 1.0;
    let alpha = rc / (rc + dt);

    let mut result = samples.to_vec();
    for _ in 0..order {
        let mut y_prev = result[0];
        for i in 1..result.len() {
            let x = result[i];
            result[i] = alpha * (result[i - 1] + x - y_prev);
            y_prev = x;
        }
        result[0] = 0.0;
    }
    result
}

pub fn cross_correlation(a: &[f64], b: &[f64]) -> Vec<f64> {
    let n = a.len();
    let mut result = vec![0.0; n];
    let mean_a = a.iter().sum::<f64>() / n as f64;
    let mean_b = b.iter().sum::<f64>() / n as f64;

    for shift in 0..n {
        let mut sum = 0.0;
        let mut count = 0;
        for i in 0..(n - shift) {
            sum += (a[i] - mean_a) * (b[i + shift] - mean_b);
            count += 1;
        }
        if count > 0 {
            result[shift] = sum / count as f64;
        }
    }
    result
}
