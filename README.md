# valicore

Rust-backed oscilloscope driver and signal analysis for Python. One YAML file defines SCPI commands for 10 brands — auto-detect or specify explicitly, then control and capture through a single generic API.

## Install

```bash
pip install maturin
maturin develop --release
```

Or with the Python fallback (no Rust build needed):

```bash
pip install pyvisa pyvisa-py
```

## Quick start

```python
from valicore import Oscilloscope, RustSignalProcessor

# Auto-detect brand from *IDN? response
scope = Oscilloscope.from_ip("192.168.1.15")
print(scope.brand())         # "RIGOL"
print(scope.instrument_id()) # "*IDN? response"

# Capture
data = scope.get_waveform("1")
scope.close()

# Analyze
stats = RustSignalProcessor.stats(data)
# → {"count": 10000, "mean": 0.0012, "rms": 0.342, "min": -1.0, "max": 1.0, ...}

fft = RustSignalProcessor.fft(data, sample_rate=1e6)
peak_freq = fft["frequencies_hz"][fft["magnitudes"].index(max(fft["magnitudes"]))]
# → 1000.0 Hz
```

## Oscilloscope control

```python
scope = Oscilloscope("RS")           # explicit brand
scope.connect("192.168.1.10")

# Generic dispatch — one API for all brands
scope.setting("vertical_scale", [("ch", "1"), ("val", "0.5")])
scope.setting("trigger_level",  [("ch", "1"), ("val", "1.5")])
val = scope.getting("vertical_scale", [("ch", "1")])

# Channel management
scope.set_ch_on("1")
scope.set_ch_off("2")
scope.check_ch("1")  # → True

# Actions
scope.reset()
scope.autoset()
scope.run()
scope.stop()
scope.single()

# Multi-channel capture
scope.set_ch_on("1")
scope.set_ch_on("3")
time_axis, data_matrix, meta = scope.get_all_waveforms()
# time_axis:  [0.0, 1e-9, 2e-9, ...]
# data_matrix: [[ch1 samples], [ch3 samples]]
# meta: {"sample_rate": "1000000000", "channels": "1,3", ...}

scope.close()
```

**Available settings:** `vertical_scale`, `vertical_offset`, `coupling`, `timebase`, `time_position`, `trigger_source`, `trigger_level`, `trigger_slope`

Adding a new SCPI command = one line in `oscilloscope.yaml`. No code changes.

## Signal analysis

```python
from valicore import RustSignalProcessor as sp

data = [...]  # your samples

# FFT
fft = sp.fft(data, sample_rate=1e6)
# → {"frequencies_hz": [...], "magnitudes": [...]}

# Power spectral density
psd = sp.psd(data, sample_rate=1e6)
# → {"frequencies_hz": [...], "power_density": [...]}

# Statistics (single-pass, O(n))
stats = sp.stats(data)
# → count, mean, std, variance, min, max, peak_to_peak, rms, crest_factor, skewness, kurtosis

# Windowing
windowed = sp.window(data, "blackman")  # hann, hamming, blackman, flat_top, none

# Filtering
filtered = sp.filter(data, "lowpass", cutoff=0.3, order=2)

# THD
thd_pct = sp.thd(data, fundamental_hz=1000, sample_rate=1e6)

# Cross-correlation
corr = sp.cross_correlate(signal_a, signal_b)

# Moving average
smooth = sp.moving_average(data, window_size=10)
```

## Supported brands

| Brand | Series | Default IP |
|-------|--------|------------|
| `RS` | Rohde & Schwarz RTO/RTM/RTC | 192.168.1.10 |
| `TEKTRONIX` | TDS/MSO/DPO | 192.168.1.20 |
| `KEYSIGHT` | InfiniiVision / Infiniium | 192.168.1.30 |
| `RIGOL` | DS1000Z/DS2000/DS4000 | 192.168.1.15 |
| `YOKOGAWA` | DLM / DL | 192.168.1.40 |
| `SIGLENT` | SDS1000X/SDS2000X/SDS5000X | 192.168.1.50 |
| `LECROY` | WaveSurfer/WaveRunner/WavePro | 192.168.1.60 |
| `GW_INSTEK` | GDS | 192.168.1.70 |
| `PRODIGIT` | oscilloscope series | 192.168.1.80 |
| `OWON` | SDS | 192.168.1.90 |

## Adding a new brand

Add to `src/valicore/driver/oscilloscope.yaml`:

```yaml
OSCILLOSCOPES:
  MY_BRAND:
    description: "My scope series"
    default_ip: "192.168.1.99"
    idn_pattern: "MY_BRAND"     # matched against *IDN? response
    endian: little
    quirks: "*CLS"              # sent once after connection
    cmds:
      reset: "*RST"
      set_v_scale: ":CHANnel{ch}:SCALe {val}"
      get_v_scale: ":CHANnel{ch}:SCALe?"
      get_raw: ":WAVeform:DATA? {ch}"
      # ... add more as needed
```

Placeholders `{ch}`, `{val}`, etc. are substituted at runtime. The YAML is compiled into the Rust binary — no rebuild of Python code needed, just `maturin develop`.

## Architecture

```
oscilloscope.yaml ──→ Oscilloscope (Rust) ──→ PyO3 ──→ Python
                         ↕ TCP/IP               ↑
                    ChannelMask (u8 bitfield)    │
                                            Python fallback
                                            (pyvisa, when Rust
                                             extension missing)
```

- **Rust core**: async TCP/SCPI, YAML-driven command dispatch, SIMD PSD
- **Python fallback**: same API via pyvisa, used when the compiled extension isn't available
- **Generic dispatch**: `setting(name, kwargs)` / `getting(name, kwargs)` — no per-brand methods

## Testing

```bash
# Rust unit tests
cargo test

# Python integration
pip install pytest
pytest tests/ -v
```
