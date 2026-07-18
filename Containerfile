FROM python:3.11-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir pyvisa numpy

WORKDIR /app
ENV RUST_LOG="valicore_rs=info"
ENV VALICORE_METRICS_ADDR="0.0.0.0:9090"
EXPOSE 9090

CMD ["sleep", "infinity"]
