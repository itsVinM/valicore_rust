# valicore

YAML-driven SCPI oscilloscope driver + signal analysis in Rust (PyO3).

The YAML spec (`src/valicore/specs/oscilloscope.yaml`) defines SCPI commands for 10 brands — loaded at compile time, single source of truth.

```python
from valicore import Oscilloscope, RustSignalProcessor
scope = Oscilloscope("RIGOL")
scope.connect("192.168.1.15")
data = scope.get_waveform("CH1")
stats = RustSignalProcessor.stats(data)
```

## Supported brands

`RIGOL` · `RS` · `TEKTRONIX` · `KEYSIGHT` · `YOKOGAWA` · `SIGLENT` · `LECROY` · `GW_INSTEK` · `PRODIGIT` · `OWON`

## Architecture

```
oscilloscope.yaml → Oscilloscope (Rust) → PyO3 → CLI / Python
                       ↕ TCP/IP (:5025)
```

`_SETTINGS` / `_GETTINGS` dispatch maps drive generic `setting(name, kwargs)` / `getting(name, kwargs)` calls — no brand-specific Python code needed for new commands.

## YAML spec format

```yaml
RIGOL:
  endian: little
  quirks: ":SYSTem:LANGuage EN"
  cmds:
    set_v_scale: ":{ch}:SCAL {val}"
    get_v_scale: ":{ch}:SCAL?"
    get_raw: ":WAVeform:DATA? {ch}"
```

Placeholders (`{ch}`, `{val}`, `{edge}`) are substituted at runtime. Any brand goes in one file.

## CLI

```
valicore resources                         # list brands
valicore capture --brand RIGOL -r 192.168.1.15 -c CH1 --fft --stats -o out.csv
valicore analyze out.csv --fft --thd 1000
```

## Development

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install maturin && maturin develop
pytest tests/ -v

# Docker
docker build -t valicore .
docker run --rm --network host valicore valicore capture --brand RIGOL -r 192.168.1.15
```
