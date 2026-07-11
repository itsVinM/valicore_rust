use num_complex::Complex;
use rustfft::FftPlanner;
use wide::f64x4;

pub fn fft_analysis(samples: &[f64], sample_rate: f64) -> Result<Vec<Vec<f64>>, String> {
    if samples.is_empty() {
        return Err("empty signal".into());
    }
    if sample_rate <= 0.0 {
        return Err("sample_rate must be positive".into());
    }

    let n = samples.len();
    let mut buffer: Vec<Complex<f64>> = samples.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    let freq_resolution = sample_rate / n as f64;
    let half_n = n / 2;

    let mut freqs = Vec::with_capacity(half_n);
    let mut magnitudes = Vec::with_capacity(half_n);

    for i in 0..half_n {
        let freq = i as f64 * freq_resolution;
        let mag = buffer[i].norm() / n as f64;
        freqs.push(freq);
        magnitudes.push(mag);
    }

    Ok(vec![freqs, magnitudes])
}

pub fn psd(samples: &[f64], sample_rate: f64) -> Result<Vec<Vec<f64>>, String> {
    if samples.is_empty() {
        return Err("empty signal".into());
    }
    if sample_rate <= 0.0 {
        return Err("sample_rate must be positive".into());
    }

    let n = samples.len();
    let mut buffer: Vec<Complex<f64>> = samples.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    let freq_resolution = sample_rate / n as f64;
    let half_n = n / 2;
    let scale = 2.0 / (sample_rate * n as f64);

    let mut freqs = Vec::with_capacity(half_n);
    let mut power = Vec::with_capacity(half_n);

    for i in 0..half_n {
        let freq = i as f64 * freq_resolution;
        let p = buffer[i].norm_sqr() * scale;
        if i == 0 {
            power.push(p * 0.5);
        } else {
            power.push(p);
        }
        freqs.push(freq);
    }

    Ok(vec![freqs, power])
}

pub fn psd_vectorized(samples: &[f64], sample_rate: f64) -> Result<Vec<Vec<f64>>, String> {
    if samples.is_empty() {
        return Err("empty signal".into());
    }
    if sample_rate <= 0.0 {
        return Err("sample_rate must be positive".into());
    }

    let n = samples.len();
    let mut buffer: Vec<Complex<f64>> = samples.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    let freq_resolution = sample_rate / n as f64;
    let half_n = n / 2;
    let scale = 2.0 / (sample_rate * n as f64);

    let mut freqs = Vec::with_capacity(half_n);
    let mut power = Vec::with_capacity(half_n);

    // DC bin (i=0): halved
    if half_n > 0 {
        freqs.push(0.0);
        power.push(buffer[0].norm_sqr() * scale * 0.5);
    }

    // Process 4 bins at once with SIMD
    let simd_scale = f64x4::splat(scale);
    let mut i = 1;
    while i + 4 <= half_n {
        let re = f64x4::new([buffer[i].re, buffer[i + 1].re, buffer[i + 2].re, buffer[i + 3].re]);
        let im = f64x4::new([buffer[i].im, buffer[i + 1].im, buffer[i + 2].im, buffer[i + 3].im]);
        let ns = (re * re) + (im * im);
        let ps = ns * simd_scale;
        let a = ps.to_array();
        freqs.extend_from_slice(&[
            i as f64 * freq_resolution,
            (i + 1) as f64 * freq_resolution,
            (i + 2) as f64 * freq_resolution,
            (i + 3) as f64 * freq_resolution,
        ]);
        power.extend_from_slice(&a);
        i += 4;
    }

    // Remainder
    for j in i..half_n {
        freqs.push(j as f64 * freq_resolution);
        power.push(buffer[j].norm_sqr() * scale);
    }

    Ok(vec![freqs, power])
}

pub fn thd(samples: &[f64], fundamental_hz: f64, sample_rate: f64) -> Result<f64, String> {
    if samples.is_empty() {
        return Err("empty signal".into());
    }
    if sample_rate <= 0.0 || fundamental_hz <= 0.0 {
        return Err("frequencies must be positive".into());
    }

    let n = samples.len();
    let mut buffer: Vec<Complex<f64>> = samples.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    let freq_resolution = sample_rate / n as f64;
    let fund_bin = (fundamental_hz / freq_resolution).round() as usize;

    if fund_bin == 0 || fund_bin >= n / 2 {
        return Err("fundamental frequency too low or too high for FFT resolution".into());
    }

    let peak_power = |bin: usize, width: usize| -> f64 {
        let start = bin.saturating_sub(width);
        let end = (bin + width + 1).min(n / 2);
        (start..end)
            .map(|i| buffer[i].norm_sqr())
            .fold(f64::default(), f64::max)
    };

    let fundamental_power = peak_power(fund_bin, 2);
    if fundamental_power == 0.0 {
        return Ok(0.0);
    }

    let mut harmonic_power = 0.0;
    for h in 2..=10 {
        let harmonic_bin = (h as f64 * fundamental_hz / freq_resolution).round() as usize;
        if harmonic_bin < n / 2 {
            harmonic_power += peak_power(harmonic_bin, 2);
        }
    }

    Ok((harmonic_power / fundamental_power).sqrt() * 100.0)
}
