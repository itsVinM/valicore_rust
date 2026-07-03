# valicore

YAML-driven SCPI oscilloscope driver + signal analysis in Rust (PyO3).

The YAML spec at `src/valicore/specs/oscilloscope.yaml` defines SCPI commands for 10 brands.
The Rust oscilloscope loads the YAML at compile time — the YAML is the single source of truth.

```python
from valicore import Oscilloscope, RustSignalProcessor

scope = Oscilloscope("RIGOL")
scope.connect("192.168.1.15")
data = scope.get_waveform("CH1")

stats = RustSignalProcessor.stats(data)
```

## Supported brands

| Brand | Description |
|-------|-------------|
| `RIGOL` | DS1000Z / DS2000 / DS4000 |
| `RS` | Rohde & Schwarz RTO/RTM/RTC |
| `TEKTRONIX` | TDS/MSO/DPO |
| `KEYSIGHT` | InfiniiVision / Infiniium |
| `YOKOGAWA` | DLM / DL |
| `SIGLENT` | SDS1000X / SDS2000X / SDS5000X |
| `LECROY` | WaveSurfer / WaveRunner / WavePro |
| `GW_INSTEK` | GDS |
| `PRODIGIT` | Prodigit |
| `OWON` | SDS |

## Architecture

```
oscilloscope.yaml
       │  include_str! (compile-time)
       ▼
┌───────────────┐     ┌──────────────────┐
│  Oscilloscope │────▶│  PyO3            │
│  (Rust)       │◀────│  Oscilloscope    │
│               │     │  (Python class)  │
│  cmd(name,    │     └──────────────────┘
│    subs[])    │              │
│  setting()    │     valicore capture --brand RIGOL ...
│  getting()    │     valicore resources
│  get_waveform │              │
│  query_binary │     ┌──────────────────┐
│  _SETTINGS[]  │────▶│  TCP/IP          │
│  _GETTINGS[]  │     │  (:5025)         │
└───────────────┘     └──────────────────┘
```

The `_SETTINGS` and `_GETTINGS` maps provide generic dispatch:
```python
scope.setting("vertical_scale", [("ch", "CH1"), ("val", "1.0")])
# writes :CH1:SCAL 1.0 (RIGOL)

scope.getting("vertical_scale", [("ch", "1")])
# queries CHANnel1:SCALe? (RS)
```

## YAML spec format

Each brand under `OSCILLOSCOPES:` defines:
- `description` — human-readable name
- `ip_address` — default IP (for reference)
- `endian` — `little` or `big` (for binary waveform data)
- `quirks` — initialization command(s) sent on connect
- `cmds` — key-value map of `{command_name}: "{SCPI template}"`

Templates support `{ch}`, `{val}`, `{edge}` placeholders:
```yaml
RIGOL:
  endian: little
  quirks: ":SYSTem:LANGuage EN"
  cmds:
    reset: "*RST"
    autoset: ":AUToscale"
    set_v_scale: ":{ch}:SCAL {val}"
    get_v_scale: ":{ch}:SCAL?"
    get_raw: ":WAVeform:DATA? {ch}"
```

## Python API

### `Oscilloscope(brand, timeout_ms=5000)`

**Statics:**
- `Oscilloscope.brands()` — list available brand keys
- `Oscilloscope.available_settings()` — list setting names
- `Oscilloscope.available_gettings()` — list getting names

**Connection:**
- `connect(addr, port=5025)` — TCP connect, apply quirks, query IDN + active channels
- `close()` — disconnect
- `is_connected()` — bool
- `brand()` — brand name
- `instrument_id()` — `*IDN?` response
- `active_channels()` — list of active channel numbers

**Low-level:**
- `write(cmd)` — write SCPI string
- `query(cmd)` — write + read response
- `query_binary(cmd)` — write + read SCPI binary block `#n...` → `list[float]`
- `cmd(name, subs)` — generate SCPI from YAML template with substitutions: `[("ch","CH1"), ("val","1.0")]`
- `commands()` — list all available command keys for this brand

**Generic dispatch:**
- `setting(name, kwargs)` — look up `_SETTINGS[name]`, substitute kwargs, write
- `getting(name, kwargs)` — look up `_GETTINGS[name]`, substitute kwargs, query

**Convenience setters:**
- `set_v_scale(channel, val)`, `set_v_offset(channel, val)`
- `set_coupling(channel, val)`, `set_h_scale(val)`, `set_h_pos(val)`
- `set_trig_source(channel)`, `set_trig_level(val)`, `set_trig_slope(val)`
- `set_ch_on(channel)`, `set_ch_off(channel)`

**Convenience getters:**
- `get_v_scale(channel)`, `get_v_offset(channel)`, `get_coupling(channel)`
- `get_h_scale()`, `get_h_pos()`

**Actions:** `reset()`, `autoset()`, `run()`, `stop()`, `single()`, `check_ch(channel)`

**Waveform:**
- `get_waveform(channel)` — ASCII CSV capture → `list[float]`
- `get_all_waveforms()` — binary multi-channel → `(time_axis, data_matrix, metadata)`
  - `time_axis`: `list[float]` in seconds
  - `data_matrix`: `list[list[float]]` channels × samples (truncated to shortest)
  - `metadata`: `dict` with sample_rate, channels, num_samples, timestamp, instrument

## Signal processing (Rust via PyO3)

| Function | Returns | Description |
|---|---|---|
| `RustSignalProcessor.fft(samples, rate)` | `{frequencies_hz, magnitudes}` | Magnitude spectrum |
| `RustSignalProcessor.psd(samples, rate)` | `{frequencies_hz, power_density}` | Power spectral density |
| `RustSignalProcessor.stats(samples)` | `{mean, rms, min, max, variance, ...}` | Statistics |
| `RustSignalProcessor.thd(samples, fundamental, rate)` | `float` | Total harmonic distortion % |
| `RustSignalProcessor.window(samples, type)` | `list[float]` | Hann/Hamming/Blackman window |
| `RustSignalProcessor.filter(samples, type, cutoff, order)` | `list[float]` | Lowpass/highpass/bandpass |
| `RustSignalProcessor.cross_correlate(a, b)` | `list[float]` | Cross-correlation |
| `RustSignalProcessor.moving_average(samples, window)` | `list[float]` | Moving average filter |

## CLI

```bash
# List oscilloscope brands and VISA resources
valicore resources

# Capture waveform from a scope
valicore capture --brand RIGOL -r 192.168.1.15 -c CH1 --fft --stats -o capture.csv

# Analyze a saved CSV
valicore analyze capture.csv --fft --thd 1000
```

### capture options

| Option | Default | Description |
|---|---|---|
| `--brand, -b` | `RIGOL` | Brand from YAML spec |
| `--resource, -r` | (required) | IP address or VISA resource string |
| `--port, -p` | `5025` | TCP port |
| `--channel, -c` | `CH1` | Channel (CH1 or 1 depending on brand) |
| `--output, -o` | — | Save to CSV |
| `--fft` | — | Print FFT peak frequency |
| `--stats` | — | Print signal statistics |

## Deploy

```bash
# Docker + K8s
docker build -t valicore . && kubectl apply -k k8s/

# Terraform (local Docker provider)
cd terraform && terraform init && terraform apply
```

## Development

```bash
# Local (Rust + Python)
python3 -m venv .venv && source .venv/bin/activate
pip install maturin
maturin develop
pytest tests/ -v

# Local Docker (build the wheel, run tests in container)
docker build -t valicore-dev -f- . <<'DOCKERFILE'
FROM python:3.12-slim
WORKDIR /app
COPY . .
RUN pip install maturin && maturin develop && python -m pytest tests/
DOCKERFILE

# Or step by step:
docker build -t valicore .
docker run --rm valicore valicore resources
docker run --rm --network host valicore valicore capture --brand RIGOL -r 192.168.1.15
```
