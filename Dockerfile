FROM rust:1.86-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY pyproject.toml ./
COPY tests/ tests/
COPY examples/ examples/

RUN cargo build --release 2>/dev/null || true
RUN cargo build --release

FROM python:3.12-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    libpango-1.0-0 \
    libpangocairo-1.0-0 \
    libgdk-pixbuf-2.0-0 \
    libffi-dev \
    libcairo2 \
    libpangoft2-1.0-0 \
    shared-mime-info \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir maturin>=1.0

WORKDIR /app

COPY --from=builder /app/ .

RUN --mount=type=cache,target=/root/.cache/pip \
    maturin develop --release 2>&1

ENTRYPOINT ["valicore"]
CMD ["--help"]
