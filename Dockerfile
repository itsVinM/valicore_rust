FROM rust:1.86-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY pyproject.toml ./

RUN cargo build --release

FROM python:3.12-slim-bookworm

RUN pip install --no-cache-dir maturin>=1.0

WORKDIR /app
COPY --from=builder /app/ .

RUN --mount=type=cache,target=/root/.cache/pip \
    maturin develop --release 2>&1

ENTRYPOINT ["valicore"]
CMD ["--help"]
