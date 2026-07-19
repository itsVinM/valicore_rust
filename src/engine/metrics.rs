use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub static SIGNAL_PROCESSING_CALLS: AtomicU64 = AtomicU64::new(0);
pub static SIGNAL_PROCESSING_TOTAL_US: AtomicU64 = AtomicU64::new(0);
pub static TCP_QUERIES: AtomicU64 = AtomicU64::new(0);
pub static TCP_QUERY_TOTAL_US: AtomicU64 = AtomicU64::new(0);
pub static INSTRUMENT_CONNECTIONS: AtomicU64 = AtomicU64::new(0);

pub fn record_signal_processing(duration_us: u64) {
    SIGNAL_PROCESSING_CALLS.fetch_add(1, Ordering::Relaxed);
    SIGNAL_PROCESSING_TOTAL_US.fetch_add(duration_us, Ordering::Relaxed);
}

pub fn record_tcp_query(duration_us: u64) {
    TCP_QUERIES.fetch_add(1, Ordering::Relaxed);
    TCP_QUERY_TOTAL_US.fetch_add(duration_us, Ordering::Relaxed);
}

pub fn record_instrument_connected() {
    INSTRUMENT_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
}

pub fn snapshot() -> HashMap<String, f64> {
    let calls = SIGNAL_PROCESSING_CALLS.load(Ordering::Relaxed);
    let sp_us = SIGNAL_PROCESSING_TOTAL_US.load(Ordering::Relaxed);
    let queries = TCP_QUERIES.load(Ordering::Relaxed);
    let tq_us = TCP_QUERY_TOTAL_US.load(Ordering::Relaxed);
    let conns = INSTRUMENT_CONNECTIONS.load(Ordering::Relaxed);

    let mut m = HashMap::with_capacity(5);
    m.insert("signal_processing_calls".into(), calls as f64);
    m.insert("signal_processing_avg_us".into(), if calls > 0 { sp_us as f64 / calls as f64 } else { 0.0 });
    m.insert("tcp_queries".into(), queries as f64);
    m.insert("tcp_query_avg_us".into(), if queries > 0 { tq_us as f64 / queries as f64 } else { 0.0 });
    m.insert("instrument_connections".into(), conns as f64);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_returns_all_keys() {
        let s = snapshot();
        assert!(s.contains_key("signal_processing_calls"));
        assert!(s.contains_key("signal_processing_avg_us"));
        assert!(s.contains_key("tcp_queries"));
        assert!(s.contains_key("tcp_query_avg_us"));
        assert!(s.contains_key("instrument_connections"));
    }

    #[test]
    fn record_signal_processing_increments() {
        let before = SIGNAL_PROCESSING_CALLS.load(Ordering::Relaxed);
        record_signal_processing(500);
        let after = SIGNAL_PROCESSING_CALLS.load(Ordering::Relaxed);
        assert_eq!(after, before + 1);
    }
}
