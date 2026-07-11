mod engine;
mod signal;

use std::collections::HashMap;

use pyo3::prelude::*;

use engine::campaign::TestCampaign;
use engine::oscilloscope::Oscilloscope;
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

    // Oscilloscope
    m.add_class::<PyOscilloscope>()?;

    Ok(())
}

// ── Oscilloscope (YAML-driven, matching Python ScopeAutomation) ─

use engine::oscilloscope::ScopeError;

fn to_pyerr(e: ScopeError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
}

#[pyclass(name = "Oscilloscope")]
struct PyOscilloscope {
    inner: Oscilloscope,
}

#[pymethods]
impl PyOscilloscope {
    #[new]
    #[pyo3(signature = (brand, timeout_ms=None))]
    fn new(brand: &str, timeout_ms: Option<u64>) -> PyResult<Self> {
        let inner = Oscilloscope::new(brand, timeout_ms.unwrap_or(5000)).map_err(to_pyerr)?;
        Ok(Self { inner })
    }

    fn brand(&self) -> String {
        self.inner.brand().to_string()
    }

    fn default_port(&self) -> u16 {
        self.inner.default_port()
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn active_channels(&self) -> Vec<u8> {
        self.inner.active_channels().to_vec()
    }

    fn instrument_id(&self) -> String {
        self.inner.instrument_id().to_string()
    }

    #[pyo3(signature = (addr, port=None))]
    fn connect(&mut self, addr: &str, port: Option<u16>) -> PyResult<()> {
        let p = port.unwrap_or_else(|| self.inner.default_port());
        get_runtime().block_on(self.inner.connect(addr, p)).map_err(to_pyerr)
    }

    fn close(&mut self) {
        get_runtime().block_on(self.inner.close())
    }

    fn write(&mut self, cmd: &str) -> PyResult<()> {
        get_runtime().block_on(self.inner.write(cmd)).map_err(to_pyerr)
    }

    fn query(&mut self, cmd: &str) -> PyResult<String> {
        get_runtime().block_on(self.inner.query(cmd)).map_err(to_pyerr)
    }

    fn query_binary(&mut self, cmd: &str) -> PyResult<Vec<f64>> {
        get_runtime().block_on(self.inner.query_binary(cmd)).map_err(to_pyerr)
    }

    fn cmd(&self, name: &str, subs: Vec<(String, String)>) -> PyResult<String> {
        let refs: Vec<(&str, &str)> = subs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.inner.cmd(name, &refs).map_err(to_pyerr)
    }

    fn commands(&self) -> Vec<String> {
        self.inner.commands()
    }

    // ── Generic dispatch (maps from SETTINGS/GETTINGS) ────

    #[staticmethod]
    fn brands() -> Vec<String> {
        Oscilloscope::brands()
    }

    #[staticmethod]
    #[pyo3(signature = (addr, port=None, timeout_ms=None))]
    fn detect_brand(addr: &str, port: Option<u16>, timeout_ms: Option<u64>) -> PyResult<String> {
        get_runtime()
            .block_on(Oscilloscope::detect_brand(
                addr,
                port.unwrap_or(5025),
                timeout_ms.unwrap_or(5000),
            ))
            .map_err(to_pyerr)
    }

    #[staticmethod]
    #[pyo3(signature = (addr, port=None, timeout_ms=None))]
    fn from_ip(addr: &str, port: Option<u16>, timeout_ms: Option<u64>) -> PyResult<Self> {
        let inner = get_runtime()
            .block_on(Oscilloscope::from_ip(
                addr,
                port,
                timeout_ms.unwrap_or(5000),
            ))
            .map_err(to_pyerr)?;
        Ok(Self { inner })
    }

    #[staticmethod]
    fn available_settings() -> Vec<&'static str> {
        Oscilloscope::available_settings()
    }

    #[staticmethod]
    fn available_gettings() -> Vec<&'static str> {
        Oscilloscope::available_gettings()
    }

    fn setting(&mut self, name: &str, kwargs: Vec<(String, String)>) -> PyResult<()> {
        let refs: Vec<(&str, &str)> = kwargs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        get_runtime().block_on(self.inner.setting(name, &refs)).map_err(to_pyerr)
    }

    fn getting(&mut self, name: &str, kwargs: Vec<(String, String)>) -> PyResult<String> {
        let refs: Vec<(&str, &str)> = kwargs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        get_runtime().block_on(self.inner.getting(name, &refs)).map_err(to_pyerr)
    }

    // ── Channel management (side-effects on active_channels) ──

    fn set_ch_on(&mut self, channel: &str) -> PyResult<()> {
        get_runtime().block_on(self.inner.set_ch_on(channel)).map_err(to_pyerr)
    }

    fn set_ch_off(&mut self, channel: &str) -> PyResult<()> {
        get_runtime().block_on(self.inner.set_ch_off(channel)).map_err(to_pyerr)
    }

    fn check_ch(&mut self, channel: &str) -> PyResult<bool> {
        get_runtime().block_on(self.inner.check_ch(channel)).map_err(to_pyerr)
    }

    // ── Actions (zero-arg commands) ──────────────────────

    fn reset(&mut self) -> PyResult<()> {
        get_runtime().block_on(self.inner.reset()).map_err(to_pyerr)
    }

    fn autoset(&mut self) -> PyResult<()> {
        get_runtime().block_on(self.inner.autoset()).map_err(to_pyerr)
    }

    fn run(&mut self) -> PyResult<()> {
        get_runtime().block_on(self.inner.run()).map_err(to_pyerr)
    }

    fn stop(&mut self) -> PyResult<()> {
        get_runtime().block_on(self.inner.stop()).map_err(to_pyerr)
    }

    fn single(&mut self) -> PyResult<()> {
        get_runtime().block_on(self.inner.single()).map_err(to_pyerr)
    }

    // ── Waveform acquisition ─────────────────────────────

    fn get_waveform(&mut self, channel: &str) -> PyResult<Vec<f64>> {
        get_runtime().block_on(self.inner.get_waveform(channel)).map_err(to_pyerr)
    }

    fn get_all_waveforms(&mut self) -> PyResult<(Vec<f64>, Vec<Vec<f64>>, HashMap<String, String>)> {
        let wf = get_runtime().block_on(self.inner.get_all_waveforms()).map_err(to_pyerr)?;
        Ok((wf.time_axis, wf.data_matrix, wf.metadata))
    }
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
