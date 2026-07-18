use std::net::SocketAddr;
use std::sync::OnceLock;

use axum::{routing::get, Router};
use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::info;

// ── Metrics handle (singleton) ─────────────────────────────

pub static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub fn init_metrics() {
    // Describe metrics (called once)
    describe_counter!(
        "valicore_campaigns_total",
        "Total number of campaigns executed"
    );
    describe_counter!(
        "valicore_campaign_steps_total",
        "Total number of test steps executed"
    );
    describe_counter!(
        "valicore_instruments_connected_total",
        "Total number of instrument connections established"
    );
    describe_counter!(
        "valicore_tcp_queries_total",
        "Total number of SCPI queries sent"
    );
    describe_histogram!(
        "valicore_signal_processing_duration_seconds",
        "Duration of signal processing operations"
    );
    describe_histogram!(
        "valicore_tcp_query_duration_seconds",
        "Duration of SCPI TCP queries"
    );
    describe_gauge!(
        "valicore_active_instruments",
        "Number of currently active instrument connections"
    );
    describe_gauge!(
        "valicore_campaigns_in_progress",
        "Number of campaigns currently running"
    );

    // Set up Prometheus exporter
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    PROMETHEUS_HANDLE.set(handle).ok();
}

// ── Metric recording helpers ───────────────────────────────

pub fn record_campaign_started() {
    counter!("valicore_campaigns_total", "status" => "started").increment(1);
    gauge!("valicore_campaigns_in_progress").increment(1.0);
}

pub fn record_campaign_completed(success: bool) {
    let status = if success { "completed" } else { "failed" };
    counter!("valicore_campaigns_total", "status" => status).increment(1);
    gauge!("valicore_campaigns_in_progress").decrement(1.0);
}

pub fn record_step_completed(passed: bool) {
    let status = if passed { "passed" } else { "failed" };
    counter!("valicore_campaign_steps_total", "status" => status).increment(1);
}

pub fn record_instrument_connected() {
    counter!("valicore_instruments_connected_total").increment(1);
    gauge!("valicore_active_instruments").increment(1.0);
}

pub fn record_instrument_disconnected() {
    gauge!("valicore_active_instruments").decrement(1.0);
}

pub fn record_tcp_query(duration_secs: f64) {
    counter!("valicore_tcp_queries_total").increment(1);
    histogram!("valicore_tcp_query_duration_seconds").record(duration_secs);
}

pub fn record_signal_processing(duration_secs: f64, operation: &str) {
    histogram!(
        "valicore_signal_processing_duration_seconds",
        "operation" => operation.to_owned()
    )
    .record(duration_secs);
}

// ── Health & Metrics HTTP server ───────────────────────────

async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn metrics_handler() -> String {
    let handle = PROMETHEUS_HANDLE.get().expect("metrics not initialized");
    handle.render()
}

pub fn start_http_server(addr: SocketAddr) {
    tokio::spawn(async move {
        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/metrics", get(metrics_handler));

        info!("observability server listening on {}", addr);
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_descriptions_registered() {
        init_metrics();
        // If we got here without panicking, descriptions were registered
    }
}
