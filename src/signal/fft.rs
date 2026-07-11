use num_complex::Complex;
use rustfft::FftPlanner;
use wide::f64x4;

// ── Shared FFT prep ───────────────────────────────────────

struct FftResult {
    buf: Vec<Complex<f64>>,
    freq_res: f64,
    half_n: usize,
}

fn fft_forward(samples: &[f64], sample_rate: f64) -> Result<FftResult, String> {
    if samples.is_empty() { return Err("empty signal".into()); }
    if sample_rate <= 0.0 { return Err("sample_rate must be positive".into()); }

    let n = samples.len();
    let mut buf: Vec<Complex<f64>> = samples.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::new();
    planner.plan_fft_forward(n).process(&mut buf);

    Ok(FftResult { buf, freq_res: sample_rate / n as f64, half_n: n / 2 })
}

// ── Public API ────────────────────────────────────────────

pub fn fft_analysis(samples: &[f64], sample_rate: f64) -> Result<Vec<Vec<f64>>, String> {
    let r = fft_forward(samples, sample_rate)?;
    let n = r.buf.len();
    let (mut freqs, mut mags) = (Vec::with_capacity(r.half_n), Vec::with_capacity(r.half_n));
    for i in 0..r.half_n {
        freqs.push(i as f64 * r.freq_res);
        mags.push(r.buf[i].norm() / n as f64);
    }
    Ok(vec![freqs, mags])
}

pub fn psd(samples: &[f64], sample_rate: f64) -> Result<Vec<Vec<f64>>, String> {
    let r = fft_forward(samples, sample_rate)?;
    let scale = 2.0 / (sample_rate * r.buf.len() as f64);
    let (mut freqs, mut power) = (Vec::with_capacity(r.half_n), Vec::with_capacity(r.half_n));
    for i in 0..r.half_n {
        freqs.push(i as f64 * r.freq_res);
        let p = r.buf[i].norm_sqr() * scale;
        power.push(if i == 0 { p * 0.5 } else { p });
    }
    Ok(vec![freqs, power])
}

pub fn psd_vectorized(samples: &[f64], sample_rate: f64) -> Result<Vec<Vec<f64>>, String> {
    let r = fft_forward(samples, sample_rate)?;
    let scale = 2.0 / (sample_rate * r.buf.len() as f64);
    let h = r.half_n;
    let mut freqs = Vec::with_capacity(h);
    let mut power = Vec::with_capacity(h);

    // DC bin
    if h > 0 {
        freqs.push(0.0);
        power.push(r.buf[0].norm_sqr() * scale * 0.5);
    }

    // SIMD: 4 bins at a time
    let vs = f64x4::splat(scale);
    let mut i = 1;
    while i + 4 <= h {
        let re = f64x4::new([r.buf[i].re, r.buf[i+1].re, r.buf[i+2].re, r.buf[i+3].re]);
        let im = f64x4::new([r.buf[i].im, r.buf[i+1].im, r.buf[i+2].im, r.buf[i+3].im]);
        let ps = ((re * re) + (im * im)) * vs;
        let a = ps.to_array();
        freqs.extend_from_slice(&[
            i as f64 * r.freq_res, (i+1) as f64 * r.freq_res,
            (i+2) as f64 * r.freq_res, (i+3) as f64 * r.freq_res,
        ]);
        power.extend_from_slice(&a);
        i += 4;
    }
    for j in i..h {
        freqs.push(j as f64 * r.freq_res);
        power.push(r.buf[j].norm_sqr() * scale);
    }
    Ok(vec![freqs, power])
}

pub fn thd(samples: &[f64], fundamental_hz: f64, sample_rate: f64) -> Result<f64, String> {
    if fundamental_hz <= 0.0 || sample_rate <= 0.0 { return Err("frequencies must be positive".into()); }
    let r = fft_forward(samples, sample_rate)?;
    let n = r.buf.len();
    let fund_bin = (fundamental_hz / r.freq_res).round() as usize;
    if fund_bin == 0 || fund_bin >= n / 2 {
        return Err("fundamental out of range".into());
    }

    let peak = |bin: usize, w: usize| -> f64 {
        let lo = bin.saturating_sub(w);
        let hi = (bin + w + 1).min(n / 2);
        (lo..hi).map(|i| r.buf[i].norm_sqr()).fold(0.0_f64, f64::max)
    };

    let fp = peak(fund_bin, 2);
    if fp == 0.0 { return Ok(0.0); }

    let hp: f64 = (2..=10).map(|h| {
        let b = (h as f64 * fundamental_hz / r.freq_res).round() as usize;
        if b < n / 2 { peak(b, 2) } else { 0.0 }
    }).sum();

    Ok((hp / fp).sqrt() * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, sr: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sr).sin()).collect()
    }

    #[test]
    fn fft_peak_at_freq() {
        let sr = 1000.0;
        let s = sine(50.0, sr, 1024);
        let r = fft_analysis(&s, sr).unwrap();
        let peak = r[1].iter().enumerate().max_by(|a,b| a.1.partial_cmp(b.1).unwrap()).unwrap();
        let peak_freq = r[0][peak.0];
        assert!((peak_freq - 50.0).abs() < 5.0, "peak at {peak_freq}");
    }

    #[test]
    fn psd_sum_positive() {
        let s = sine(100.0, 1000.0, 512);
        let r = psd(&s, 1000.0).unwrap();
        assert!(r[1].iter().all(|&p| p >= 0.0));
    }

    #[test]
    fn thd_sine_is_near_zero() {
        let s = sine(100.0, 1000.0, 1024);
        let t = thd(&s, 100.0, 1000.0).unwrap();
        assert!(t < 1.0, "THD of pure sine: {t}");
    }

    #[test]
    fn empty_signal_error() {
        assert!(fft_analysis(&[], 1000.0).is_err());
        assert!(psd(&[], 1000.0).is_err());
        assert!(thd(&[], 100.0, 1000.0).is_err());
    }
}
