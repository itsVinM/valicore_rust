# valicore

**Rust-backed oscilloscope driver and signal analysis for Python.**

Control 10 oscilloscope brands through a single YAML-driven API, capture waveforms over async TCP, and analyze signals with SIMD-accelerated DSP — all from Python.

## Install

```bash
pip install maturin
maturin develop --release
```

## Quick start

```python
from valicore import Oscilloscope, RustSignalProcessor

# Auto-detect brand and connect
scope = Oscilloscope.from_ip("192.168.1.15")
print(scope.brand())  # "RIGOL"

# Configure and capture
scope.setting("vertical_scale", [("ch", "1"), ("val", "0.5")])
data = scope.get_waveform("1")
scope.close()

# Analyze
stats = RustSignalProcessor.stats(data)
fft = RustSignalProcessor.fft(data, sample_rate=1e6)
```

## Oscilloscope control

```python
scope = Oscilloscope("RS")
scope.connect("192.168.1.10")

# Settings — YAML resolves to brand-specific SCPI
scope.setting("vertical_scale", [("ch", "1"), ("val", "0.5")])
scope.setting("trigger_level", [("ch", "1"), ("val", "0.0")])
scope.setting("coupling", [("ch", "1"), ("val", "DC")])

# Actions
scope.reset()
scope.autoset()
scope.run()

# Waveform
data = scope.get_waveform("1")
time_axis, data_matrix, metadata = scope.get_all_waveforms()
```

**Supported brands:** Rohde & Schwarz, Tektronix, Keysight, Rigol, Yokogawa, Siglent, LeCroy, GW Instek, Prodigit, Owon

## Signal analysis

```python
from valicore import RustSignalProcessor as sp

fft = sp.fft(data, sample_rate=1e6)          # FFT
psd = sp.psd(data, sample_rate=1e6)          # Power spectral density
stats = sp.stats(data)                       # 11 metrics (mean, std, rms, etc.)
windowed = sp.window(data, "blackman")       # Windowing
filtered = sp.filter(data, "lowpass", 0.3)   # IIR filtering
thd = sp.thd(data, 1000, 1e6)               # THD %
corr = sp.cross_correlate(a, b)             # Cross-correlation
```

## Test campaigns

```yaml
title: "Power Supply Validation"
instruments:
  dmm:
    kind: keysight_34460a
    resource: "TCPIP0::192.168.1.100::inst0::INSTR"
groups:
  voltage_accuracy:
    steps:
      - name: "3.3V rail"
        instrument: dmm
        command: "MEAS:VOLT:DC? 10,0.001"
        measurements:
          - name: Vout_3v3
            limits:
              - op: within
                value: 3.3
                tolerance: 0.1
```

```python
from valicore._rust import py_run_campaign
results = py_run_campaign("campaign.yaml")
```

## Observability

```bash
# Structured logging
RUST_LOG=valicore_rs=debug python script.py

# Prometheus metrics (set before import)
export VALICORE_METRICS_ADDR="0.0.0.0:9090"
python -c "from valicore import Oscilloscope"

# Health/metrics
curl http://localhost:9090/health
curl http://localhost:9090/metrics
```

**Metrics:** `valicore_campaigns_total`, `valicore_campaign_steps_total`, `valicore_tcp_queries_total`, `valicore_signal_processing_duration_seconds`, `valicore_active_instruments`

## Docker

```bash
maturin develop --release
docker compose up -d
docker compose exec valicore bash
```

## Testing

```bash
cargo test           # Rust unit tests
pytest tests/ -v     # Python integration tests
```

## License

MIT
