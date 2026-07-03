# valicore

**Validation Orchestration Framework** — YAML-driven test campaigns with a Python CLI and Rust engine.

Run instrument-based validation suites from a single YAML file. Plug in DMMs, oscilloscopes, power supplies via SCPI. Generate HTML/PDF reports. Validate in CI.

```bash
pip install valicore
valicore init -o campaign.yaml   # create a template
valicore validate campaign.yaml  # check YAML without hardware
valicore run campaign.yaml        # execute against real instruments
```

## Features

- **YAML campaigns** — declare instruments, measurements, limits, and test flow in one file
- **SCPI instrument drivers** — Keysight, Rigol, Rohde & Schwarz (extensible via `base.py`)
- **Rust engine** — async instrument I/O via tokio, pass/fail evaluation, campaign orchestration
- **Python fallback** — switch backends with `--scpi python` for debugging without the compiled module
- **Signal processing** — FFT, PSD, THD, windowing (Hann/Hamming/Blackman), filtering (lowpass/highpass/bandpass), cross-correlation — all in Rust
- **Reports** — HTML and PDF output via Jinja2 + WeasyPrint
- **CI/CD hooks** — JUnit XML output for integration with test dashboards
- **Validation** — dry-run mode validates YAML structure and instrument references without touching hardware
- **Docker** — multi-stage container image with the full stack pre-built

## Example Campaign

```yaml
title: "Power Supply Validation - Rev B"
version: "1.0"

instruments:
  dmm:
    kind: keysight_34460a
    resource: "TCPIP0::192.168.1.100::inst0::INSTR"
    timeout: 5000
  scope:
    kind: rigol_ds1000z
    resource: "USB0::0x1AB1::0x04B0::DS1ZA123456::INSTR"
  psu:
    kind: rs_nga100
    resource: "TCPIP0::192.168.1.101::inst0::INSTR"

groups:
  voltage_outputs:
    description: "Measure output voltages"
    steps:
      - name: "3.3V rail"
        instrument: dmm
        measurements:
          - name: "Vout_3v3"
            type: "volt:dc"
            limits:
              - op: within
                value: 3.3
                tolerance: 0.05

      - name: "5.0V rail ripple"
        instrument: scope
        measurements:
          - name: "Vripple_5v0"
            type: "volt:ac"
            limits:
              - op: lt
                value: 0.050
        post_processing:
          window: hann
          filter_type: lowpass
          filter_cutoff: 0.3
          filter_order: 4

output:
  formats:
    - html
    - pdf

ci_hooks:
  junit: true
  junit_output: "test-results.xml"
```

## CLI

```
Usage: valicore [OPTIONS] COMMAND [ARGS]...

  valicore — Validation Orchestration Framework

Options:
  --version  Show the version and exit.
  --help     Show this message and exit.

Commands:
  init            Create a template campaign YAML file.
  validate        Validate a campaign YAML file without executing.
  run             Execute a test campaign from a YAML file.
  list-campaigns  List all campaigns found in a directory.
```

### Run Options

| Flag | Description |
|---|---|
| `--scpi rust` | Use the Rust tokio-based SCPI backend (default) |
| `--scpi python` | Use pyvisa-based SCPI backend |
| `--instrument DMM=RESOURCE` | Override instrument VISA resource at runtime |
| `--format html` | Generate HTML report |
| `--format pdf` | Generate PDF report |
| `--json` | Emit structured JSON results to stdout |
| `--output DIR` | Report output directory |

## Architecture

```
┌─────────────────────────────────────┐
│  CLI (click)                        │
│  valicore run/validate/init         │
├─────────────────────────────────────┤
│  Python                             │
│  ┌───────────┐ ┌──────────┐         │
│  │ Campaign  │ │ Reporting│         │
│  │ Loader    │ │ HTML/PDF │         │
│  │ Model     │ │ Jinja2   │         │
│  │ Pydantic  │ │ WeasyPrint│        │
│  └─────┬─────┘ └──────────┘         │
├───────┼─────────────────────────────┤
│ Rust  │  (PyO3 extension module)    │
│  ┌────┴────┐ ┌──────────────────┐   │
│  │ Engine  │ │ Signal Processing│   │
│  │ Campaign│ │ FFT / PSD / THD  │   │
│  │ Runner  │ │ Windowing / Filter│  │
│  │ tokio   │ │ Cross-correlation│   │
│  └─────────┘ └──────────────────┘   │
└─────────────────────────────────────┘
```

The Rust extension compiles to `valicore._rust` via PyO3 and maturin. Both the engine and signal processing run in Rust with a Python fallback for every function.

## Reports

HTML reports render campaign results with pass/fail status per step and measurement. PDF reports use the same template via WeasyPrint.

```
valicore run campaign.yaml -f html -f pdf -o reports/
```

## Docker

```bash
docker build -t valicore .
docker run --rm valicore --help
```

Multi-stage build: Rust dependencies compile in a `rust:1.86` image, then the runtime uses `python:3.12-slim` with only WeasyPrint system deps.

## Development

```bash
# Install maturin
pip install maturin

# Build and install the Rust extension in development mode
maturin develop --release

# Run tests
pytest

# Build release wheel
maturin build --release
```

## Instrument Drivers

Built-in drivers in `valicore/instruments/`:

| Driver | Class |
|---|---|
| Keysight 34460A DMM | `keysight_34460a` |
| Rigol DS1000Z scope | `rigol_ds1000z` |
| Rohde & Schwarz NGA100 PSU | `rs_nga100` |

Add new instruments by subclassing `BaseInstrument` in `instruments/base.py` and registering the `kind` name.

## CI Integration

The JUnit hook produces `test-results.xml` compatible with Jenkins, GitLab CI, GitHub Actions, and other CI dashboards:

```yaml
ci_hooks:
  junit: true
  junit_output: "test-results.xml"
```

## Deploy

### Kubernetes (CronJob)

Run valicore as a scheduled validation job on any K8s cluster (Minikube, kind, production):

```bash
kubectl apply -k k8s/
```

Validates the smoke-test campaign every weekday at 6 AM. Campaigns are mounted via ConfigMap.

### Terraform (Docker provider)

Build and run valicore locally with infrastructure-as-code:

```bash
cd terraform
terraform init
terraform apply
```

Builds the Docker image and runs `valicore validate` against the example campaign in a container.

## License

MIT
