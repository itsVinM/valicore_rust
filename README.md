# valicore

**Rust-backed oscilloscope driver and signal analysis for Python.**

Control 10 oscilloscope brands through a single YAML-driven API, capture waveforms over async TCP, and analyze signals with SIMD-accelerated DSP — all from Python.

[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![Python](https://img.shields.io/badge/Python-3.10+-blue)](https://python.org)
[![PyO3](https://img.shields.io/badge/PyO3-0.23-green)](https://pyo3.rs)

---

## Why this exists

Test engineers and hardware automation developers face a recurring problem: every oscilloscope brand speaks a slightly different dialect of SCPI, and the tooling to talk to them is fragmented across vendor-specific libraries, PyVISA wrappers, and ad-hoc scripts. Signal analysis lives in a separate ecosystem (SciPy, MATLAB) with its own data marshalling overhead.

**valicore** collapses this into a single layer:

- **One YAML file** defines SCPI commands for every supported scope. Adding a new brand is a data change, not a code change.
- **A Rust core** handles async TCP I/O, binary waveform parsing, and SIMD-accelerated signal processing with zero heap allocation on the hot path.
- **Python via PyO3** gives you the ergonomics of Python with the performance of compiled Rust — no subprocess calls, no C extensions to maintain.
- **A test campaign engine** orchestrates multi-instrument test sequences with YAML-defined limits and pass/fail verdicts.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Python Layer                             │
│  Oscilloscope   RustSignalProcessor   CampaignRunner   save_*  │
└──────────┬──────────────┬──────────────────┬──────────┬────────┘
           │              │                  │          │
     ┌─────▼──────────────▼──────────────────▼──────────▼─────┐
     │                    PyO3 FFI Boundary                    │
     └─────┬──────────────┬──────────────────┬──────────┬─────┘
           │              │                  │          │
┌──────────▼──────────────▼──────────────────▼──────────▼─────┐
│                      Rust Core (src/)                       │
│                                                             │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────────┐  │
│  │ Oscilloscope│  │   Signal     │  │  Campaign Engine   │  │
│  │             │  │  Processing  │  │                    │  │
│  │ YAML cmds   │  │  FFT / PSD   │  │  YAML schema       │  │
│  │ TCP I/O     │  │  Statistics   │  │  Limit checking    │  │
│  │ Binary parse│  │  Windowing    │  │  Connection pool   │  │
│  │ ChannelMask │  │  Filtering    │  │  Streaming results │  │
│  │ Auto-detect │  │  THD / Xcorr  │  │  CSV / HDF5 export │  │
│  └──────┬──────┘  └──────────────┘  └────────────────────┘  │
│         │                                                    │
│  ┌──────▼──────────────────────────────────────────────────┐  │
│  │              Tokio Async Runtime (shared)               │  │
│  │         OnceLock<Runtime> — one instance, global        │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
         │
    ┌────▼────┐
    │ TCP/IP  │  ← SCPI instruments (oscilloscopes, DMMs, etc.)
    └─────────┘
```

## Quick start

### Install

```bash
pip install maturin
maturin develop --release
```

Or use the pure-Python fallback (no Rust build required):

```bash
pip install pyvisa pyvisa-py
```

### First connection

```python
from valicore import Oscilloscope, RustSignalProcessor

# Auto-detect brand via *IDN? response
scope = Oscilloscope.from_ip("192.168.1.15")
print(scope.brand())         # "RIGOL"
print(scope.instrument_id()) # "RIGOL TECHNOLOGIES,DS1054Z,..."

# Configure and capture
scope.setting("vertical_scale", [("ch", "1"), ("val", "0.5")])
scope.setting("trigger_level",  [("ch", "1"), ("val", "0.0")])
data = scope.get_waveform("1")
scope.close()

# Analyze
stats = RustSignalProcessor.stats(data)
fft = RustSignalProcessor.fft(data, sample_rate=1e6)
```

## Oscilloscope control

### Generic dispatch

Every brand is controlled through the same `setting()` / `getting()` interface. The mapping from logical names to brand-specific SCPI commands is resolved at runtime from YAML.

```python
scope = Oscilloscope("RS")
scope.connect("192.168.1.10")

# Write any setting by name — the YAML resolves the SCPI command
scope.setting("vertical_scale", [("ch", "1"), ("val", "0.5")])
scope.setting("vertical_offset", [("ch", "1"), ("val", "-0.25")])
scope.setting("coupling",       [("ch", "1"), ("val", "DC")])
scope.setting("timebase",       [("val", "1E-3")])
scope.setting("trigger_source", [("ch", "1")])
scope.setting("trigger_level",  [("ch", "1"), ("val", "0.0")])
scope.setting("trigger_slope",  [("ch", "1"), ("val", "POS")])

# Read back
val = scope.getting("vertical_scale", [("ch", "1")])

# Actions
scope.reset()
scope.autoset()
scope.run()
scope.stop()
scope.single()
```

### Channel management

Channels are tracked with a `u8` bitfield (`ChannelMask`) — bits 0-7 map to channels 1-8. No heap allocation per query.

```python
scope.set_ch_on("1")
scope.set_ch_on("3")
print(scope.check_ch("1"))     # True
print(scope.active_channels()) # [1, 3]
```

### Waveform acquisition

Binary block parsing handles the SCPI `#NDDDD...` format with endian-aware `f32` conversion. Multi-channel capture returns aligned time and data matrices.

```python
# Single channel
data = scope.get_waveform("1")

# All active channels (binary transfer)
time_axis, data_matrix, metadata = scope.get_all_waveforms()
# time_axis:  [0.0, 1e-9, 2e-9, ...]
# data_matrix: [[ch1 samples], [ch3 samples]]
# metadata: {"sample_rate": "1000000000", "channels": "1,3", ...}
```

### Brand auto-detection

```python
brand = Oscilloscope.detect_brand("192.168.1.15")
# Queries *IDN? and matches against idn_pattern in YAML
```

**Available settings:** `vertical_scale`, `vertical_offset`, `coupling`, `timebase`, `time_position`, `trigger_source`, `trigger_level`, `trigger_slope`, `channel_on`, `channel_off`

## Signal analysis

All signal processing functions run in Rust via PyO3. The `RustSignalProcessor` class is a stateless interface to the compiled DSP pipeline.

```python
from valicore import RustSignalProcessor as sp

data = [...]  # float64 samples

# FFT — returns frequencies and magnitudes
fft = sp.fft(data, sample_rate=1e6)
# {"frequencies_hz": [0, 976.5, 1953.1, ...], "magnitudes": [0.0012, 0.89, ...]}

# Power spectral density (V²/Hz)
psd = sp.psd(data, sample_rate=1e6)

# Single-pass O(n) statistics
stats = sp.stats(data)
# count, mean, std, variance, min, max, peak_to_peak,
# rms, crest_factor, skewness, kurtosis

# Windowing
windowed = sp.window(data, "blackman")  # hann, hamming, blackman, flat_top, none

# Lowpass / highpass filtering (cascaded IIR, order 1-8)
filtered = sp.filter(data, "lowpass", cutoff=0.3, order=2)

# Total harmonic distortion (THD) — percentage
thd_pct = sp.thd(data, fundamental_hz=1000, sample_rate=1e6)

# Cross-correlation
corr = sp.cross_correlate(signal_a, signal_b)

# Moving average (convolution)
smooth = sp.moving_average(data, window_size=10)
```

### SIMD-accelerated PSD

The PSD computation vectorizes power calculations using `wide::f64x4` (4-wide f64 SIMD), processing 4 frequency bins per CPU cycle:

```rust
// Processes 4 bins simultaneously
let re = f64x4::new([buf[i].re, buf[i+1].re, buf[i+2].re, buf[i+3].re]);
let im = f64x4::new([buf[i].im, buf[i+1].im, buf[i+2].im, buf[i+3].im]);
let power = ((re * re) + (im * im)) * scale;
```

### Statistics engine

Single-pass O(n) computation of 11 statistical measures — no data copies, no multi-pass scans:

| Metric | Description |
|--------|-------------|
| `count` | Number of samples |
| `mean` | Arithmetic mean |
| `std` | Standard deviation |
| `variance` | Population variance |
| `min` / `max` | Extremes |
| `peak_to_peak` | Max - Min |
| `rms` | Root mean square |
| `crest_factor` | Peak / RMS |
| `skewness` | Distribution asymmetry |
| `kurtosis` | Distribution tail weight (excess) |

## Test campaigns

Define multi-instrument test sequences in YAML with limit checking and pass/fail verdicts.

### Campaign YAML schema

```yaml
title: "Power Supply Validation"
version: "1.0"

instruments:
  dmm:
    kind: keysight_34460a
    resource: "TCPIP0::192.168.1.100::inst0::INSTR"
    timeout: 5000

groups:
  voltage_accuracy:
    description: "Check all rails within spec"
    steps:
      - name: "3.3V rail"
        instrument: dmm
        command: "MEAS:VOLT:DC? 10,0.001"
        measurements:
          - name: Vout_3v3
            type: volt:dc
            limits:
              - op: within
                value: 3.3
                tolerance: 0.1
              - op: gt
                value: 3.0
              - op: lt
                value: 3.6

  ripple_test:
    description: "AC ripple on 5V rail"
    depends_on: ["voltage_accuracy"]
    steps:
      - name: "5V ripple"
        instrument: scope
        command: ":WAVeform:DATA? 1"
        measurements:
          - name: Vpp_ripple
            type: peak_to_peak
            limits:
              - op: lt
                value: 0.05
```

### Running campaigns

```python
import json
from valicore._rust import py_campaign_info, py_run_campaign

# Inspect campaign structure
info = json.loads(py_campaign_info("campaign.yaml"))
print(f"{info['total_steps']} steps across {len(info['groups'])} groups")

# Execute
results = json.loads(py_run_campaign("campaign.yaml"))
print(results)
```

### Limit operators

| Operator | Description | Tolerance |
|----------|-------------|-----------|
| `eq` | Equal within tolerance | Required |
| `ne` | Not equal beyond tolerance | Required |
| `lt` | Strictly less than | — |
| `le` | Less than or equal | — |
| `gt` | Strictly greater than | — |
| `ge` | Greater than or equal | — |
| `within` | Absolute deviation ≤ tolerance | Required |
| `outside` | Absolute deviation > tolerance | Required |

### Streaming results

The campaign runner also supports streaming via `run_campaign_stream()`, yielding group-by-group results as they complete:

```rust
pub fn run_campaign_stream(campaign: TestCampaign) -> impl Stream<Item = (String, Value)>
```

## Supported brands

| Brand | Identifier | Series | Default Port |
|-------|-----------|--------|--------------|
| Rohde & Schwarz | `RS` | RTO / RTM / RTC | 5025 |
| Tektronix | `TEKTRONIX` | TDS / MSO / DPO | 5025 |
| Keysight | `KEYSIGHT` | InfiniiVision / Infiniium | 5025 |
| Rigol | `RIGOL` | DS1000Z / DS2000 / DS4000 | 5025 |
| Yokogawa | `YOKOGAWA` | DLM / DL | 5025 |
| Siglent | `SIGLENT` | SDS1000X / SDS2000X / SDS5000X | 5025 |
| LeCroy | `LECROY` | WaveSurfer / WaveRunner / WavePro | 5025 |
| GW Instek | `GW_INSTEK` | GDS | 5025 |
| Prodigit | `PRODIGIT` | Oscilloscope series | 5025 |
| Owon | `OWON` | SDS | 5025 |

## Adding a new brand

Add a single entry to `src/valicore/driver/oscilloscope.yaml`:

```yaml
OSCILLOSCOPES:
  MY_BRAND:
    description: "My scope series"
    default_ip: "192.168.1.99"
    idn_pattern: "MY_BRAND"       # matched against *IDN? response
    endian: little                 # little | big
    quirks: "*CLS"                 # sent once after connection
    cmds:
      reset: "*RST"
      autoset: ":AUToset"
      run: ":RUN"
      stop: ":STOP"
      single: ":SINGle"
      set_v_scale: ":CHANnel{ch}:SCALe {val}"
      get_v_scale: ":CHANnel{ch}:SCALe?"
      set_v_offset: ":CHANnel{ch}:OFFSet {val}"
      get_v_offset: ":CHANnel{ch}:OFFSet?"
      set_coupling: ":CHANnel{ch}:COUPling {val}"
      get_coupling: ":CHANnel{ch}:COUPling?"
      set_h_scale: ":TIMebase:SCALe {val}"
      get_h_scale: ":TIMebase:SCALe?"
      set_h_pos: ":TIMebase:POSition {val}"
      get_h_pos: ":TIMebase:POSition?"
      set_trig_source: ":TRIGger:EDGE:SOURce {ch}"
      get_trig_source: ":TRIGger:EDGE:SOURce?"
      set_trig_level: ":TRIGger:EDGE:LEVel {val}"
      get_trig_level: ":TRIGger:EDGE:LEVel?"
      set_trig_slope: ":TRIGger:EDGE:SLOPe {val}"
      get_trig_slope: ":TRIGger:EDGE:SLOPe?"
      set_source: ":WAVeform:SOURce {ch}"
      get_raw: ":WAVeform:DATA?"
      check_ch: ":CHANnel{ch}:DISplay?"
```

Placeholders `{ch}`, `{val}` are substituted at runtime. The YAML is compiled into the Rust binary via `include_str!` — zero runtime file I/O. Rebuild with `maturin develop` to pick up changes.

## Architecture deep-dive

### Zero-allocation I/O

The `Oscilloscope` struct carries a stack-allocated 4 KB read buffer (`buf: [u8; 4096]`), avoiding heap allocation on every SCPI query. The `fmt_endpoint()` function formats TCP addresses into a fixed 48-byte stack buffer using integer-to-ASCII conversion without `format!()` or `String`.

### ChannelMask bitfield

Active channels are tracked as a `u8` bitfield where bit *n* represents channel *n+1*. This gives O(1) set/clear/test operations and enables compact CSV formatting for metadata without allocation:

```rust
struct ChannelMask(u8);
impl ChannelMask {
    fn set(&mut self, ch: u8) { self.0 |= 1 << (ch - 1); }
    fn has(self, ch: u8) -> bool { self.0 & (1 << (ch - 1)) != 0 }
    fn iter(self) -> impl Iterator<Item = u8> { (1..=8).filter(move |ch| self.has(*ch)) }
}
```

### Lazy Tokio runtime

A single `OnceLock<Runtime>` provides the async runtime shared across all PyO3 calls. Python threads calling into Rust all share the same Tokio multi-thread runtime, with `block_on()` bridging the sync/async boundary:

```rust
fn get_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to create tokio runtime"))
}
```

### YAML command dispatch

The `setting()` / `getting()` methods map logical names to brand-specific SCPI templates through a compile-time lookup table. The `cmd()` method performs placeholder substitution (`{ch}`, `{val}`) with pre-allocated capacity:

```rust
const SETTINGS: &[(&str, &str, &[&str])] = &[
    ("vertical_scale", "set_v_scale", &["ch", "val"]),
    ("trigger_level",  "set_trig_level", &["ch", "val"]),
    // ...
];
```

### Binary waveform parsing

SCPI binary block transfer (`#NDDDD...` format) is parsed with a streaming state machine that handles partial reads. Endianness is determined per-brand from the YAML `endian` field, converting raw `f32` bytes to `f64` samples for Python consumption.

### Connection pooling

The campaign runner maintains an `Arc<Mutex<HashMap<String, Box<dyn SCPIInstrument>>>>` pool. Instruments are connected lazily on first use and reused across steps, avoiding redundant TCP handshakes in long test sequences.

### File export

- **CSV**: Pre-allocated `String` with estimated capacity, zero intermediate allocations per row
- **HDF5**: Optional feature (`--features hdf5`), writes time axis, channel datasets, and metadata group

## Project structure

```
valicore/
├── Cargo.toml                    # Rust dependencies, cdylib output
├── pyproject.toml                # Maturin build config, Python metadata
├── src/
│   ├── lib.rs                    # PyO3 module definition, runtime singleton
│   ├── engine/
│   │   ├── oscilloscope.rs       # SCPI driver, YAML dispatch, TCP I/O, ChannelMask
│   │   ├── campaign.rs           # Test campaign YAML schema
│   │   ├── runner.rs             # Async campaign executor, limit evaluation
│   │   ├── save.rs               # CSV and HDF5 export
│   │   └── mod.rs
│   ├── signal/
│   │   ├── fft.rs                # FFT, PSD, THD, SIMD-vectorized PSD
│   │   ├── windowing.rs          # Window functions, filters, cross-correlation
│   │   ├── statistics.rs         # Single-pass O(n) statistics
│   │   └── mod.rs
│   └── valicore/
│       ├── __init__.py           # Public API exports
│       ├── core.py               # Python wrapper, fallback logic
│       └── driver/
│           ├── oscilloscope.yaml # Brand definitions (compiled into binary)
│           ├── _fallback.py      # PyVISA-based Python fallback
│           └── _oscilloscope_driver.py
└── tests/
    ├── test_oscilloscope.py      # Oscilloscope driver tests
    └── test_signal_processing.py # Signal processing tests
```

## Testing

```bash
# Rust unit tests (all modules)
cargo test

# Python integration tests
pip install pytest
pytest tests/ -v
```

Rust tests cover: command dispatch, channel mask operations, endpoint formatting, YAML parsing, limit evaluation, FFT accuracy, PSD positivity, THD of pure signals, window function correctness, filter validation, CSV round-trips, and more.

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `pyo3` | 0.23 | Python bindings (abi3, stable ABI) |
| `rustfft` | 6 | FFT computation |
| `wide` | 0.7 | SIMD primitives (`f64x4`) |
| `tokio` | 1 | Async runtime, TCP, timeouts |
| `serde` / `serde_yaml` / `serde_json` | 1 | Serialization |
| `async-trait` | 0.1 | Async trait objects |
| `anyhow` / `thiserror` | 1 / 2 | Error handling |
| `hdf5` | 0.8 | HDF5 export (optional) |

## License

MIT
