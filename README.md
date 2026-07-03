# valicore

YAML-driven validation orchestration with a Python CLI and Rust engine.

```bash
pip install valicore
valicore init -o campaign.yaml
valicore run campaign.yaml
```

## Features

- YAML campaigns — declare instruments, measurements, limits, test flow
- SCPI drivers — Keysight, Rigol, R&S (extensible)
- Rust engine — async I/O via tokio, signal processing (FFT, PSD, THD, filtering)
- HTML/PDF reports, JUnit CI hooks, Docker multi-stage build
- `--scpi python` fallback for debugging without compiled module

## Deploy

```bash
# K8s (CronJob)
docker build -t valicore .
kubectl apply -k k8s/

# Terraform (local Docker)
cd terraform && terraform init && terraform apply
```

## Development

```bash
pip install maturin
maturin develop --release
pytest
```

## License

MIT
