# valicore

SCPI oscilloscope driver + signal analysis in Python and Rust.

```python
from valicore.instruments import RigolDS1000Z
from valicore.core import RustSignalProcessor

scope = RigolDS1000Z("USB0::0x1AB1::0x04B0::DS1ZA123456::INSTR")
samples = scope.get_waveform("CH1")              # grab from hardware

stats = RustSignalProcessor.stats(samples)        # mean, rms, peak
freqs, mags = RustSignalProcessor.fft(samples, 1e6)  # FFT
```

## Supported scopes

- Rigol DS1000Z series
- Keysight 34460A DMM (measurement data)
- Rohde & Schwarz NGA100 PSU (voltage/current reads)

## Signal processing (Rust)

| Function | Description |
|---|---|
| `fft(samples, rate)` | Magnitude spectrum |
| `psd(samples, rate)` | Power spectral density |
| `thd(samples, fundamental, rate)` | Total harmonic distortion |
| `stats(samples)` | Mean, RMS, min, max, variance |
| `window(samples, type)` | Hann, Hamming, Blackman |
| `filter(samples, type, cutoff, order)` | Lowpass, highpass, bandpass |
| `cross_correlate(a, b)` | Time-domain correlation |

All run in Rust via PyO3. Pure-Python fallback available.

## Deploy

```bash
# K8s (Job)
docker build -t valicore . && kubectl apply -k k8s/

# Terraform (local Docker)
cd terraform && terraform init && terraform apply
```

## CLI

```bash
valicore scope --resource "TCPIP::..." --channel CH1 --samples 10000
```

## Development

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install maturin
maturin develop --release
```
