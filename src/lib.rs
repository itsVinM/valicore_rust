mod engine;
mod signal;

use std::collections::HashMap;

use pyo3::prelude::*;

use engine::campaign::TestCampaign;
use engine::runner;

// ── tokio runtime (lazy, shared across PyO3 calls) ──────────

fn get_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("failed to create tokio runtime"))
}

// ── PyO3 module ─────────────────────────────────────────────

#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Signal processing
    m.add_function(wrap_pyfunction!(compute_fft, m)?)?;
    m.add_function(wrap_pyfunction!(compute_psd, m)?)?;
    m.add_function(wrap_pyfunction!(compute_stats, m)?)?;
    m.add_function(wrap_pyfunction!(apply_window, m)?)?;
    m.add_function(wrap_pyfunction!(apply_filter, m)?)?;
    m.add_function(wrap_pyfunction!(compute_thd, m)?)?;
    m.add_function(wrap_pyfunction!(cross_correlate, m)?)?;

    // Campaign engine
    m.add_function(wrap_pyfunction!(py_campaign_info, m)?)?;
    m.add_function(wrap_pyfunction!(py_run_campaign, m)?)?;

    Ok(())
}

// ── Signal processing (unchanged API) ──────────────────────

#[pyfunction]
fn compute_fft(samples: Vec<f64>, sample_rate: f64) -> PyResult<Vec<Vec<f64>>> {
    let result = signal::fft_analysis(&samples, sample_rate)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?;
    Ok(result)
}

#[pyfunction]
fn compute_psd(samples: Vec<f64>, sample_rate: f64) -> PyResult<Vec<Vec<f64>>> {
    let result = signal::psd(&samples, sample_rate)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?;
    Ok(result)
}

#[pyfunction]
fn compute_stats(samples: Vec<f64>) -> PyResult<HashMap<String, f64>> {
    Ok(signal::compute_stats(&samples))
}

#[pyfunction]
fn apply_window(samples: Vec<f64>, window_type: &str) -> PyResult<Vec<f64>> {
    signal::apply_window(&samples, window_type)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))
}

#[pyfunction]
fn apply_filter(
    samples: Vec<f64>,
    filter_type: &str,
    cutoff: f64,
    order: u32,
) -> PyResult<Vec<f64>> {
    signal::apply_filter(&samples, filter_type, cutoff, order)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))
}

#[pyfunction]
fn compute_thd(samples: Vec<f64>, fundamental_hz: f64, sample_rate: f64) -> PyResult<f64> {
    signal::thd(&samples, fundamental_hz, sample_rate)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))
}

#[pyfunction]
fn cross_correlate(a: Vec<f64>, b: Vec<f64>) -> PyResult<Vec<f64>> {
    if a.len() != b.len() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "signals must have equal length",
        ));
    }
    Ok(signal::cross_correlation(&a, &b))
}

// ── Campaign engine (new) ─────────────────────────────────

#[pyfunction]
fn py_campaign_info(path: &str) -> PyResult<String> {
    let campaign = TestCampaign::from_yaml(path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

    let info = serde_json::json!({
        "title": campaign.title,
        "version": campaign.version,
        "instruments": campaign.instruments.keys().collect::<Vec<_>>(),
        "groups": campaign.groups.keys().collect::<Vec<_>>(),
        "total_steps": campaign.total_steps(),
    });
    Ok(serde_json::to_string(&info).unwrap_or_default())
}

#[pyfunction]
fn py_run_campaign(path: &str) -> PyResult<String> {
    let campaign = TestCampaign::from_yaml(path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

    let rt = get_runtime();
    let results = rt
        .block_on(runner::run_campaign(&campaign))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

    Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
}
