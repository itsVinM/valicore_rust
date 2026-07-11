use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::campaign::{Limit, TestCampaign, TestStep};
use super::instrument::{create_instrument, SCPIInstrument};

fn check_limit(value: f64, limit: &Limit) -> bool {
    let tol = limit.tolerance.unwrap_or(1e-9);
    match limit.op.as_str() {
        "eq" => (value - limit.value).abs() <= tol,
        "ne" => (value - limit.value).abs() > tol,
        "lt" => value < limit.value,
        "le" => value <= limit.value,
        "gt" => value > limit.value,
        "ge" => value >= limit.value,
        "within" => (value - limit.value).abs() <= tol,
        "outside" => (value - limit.value).abs() > tol,
        _ => true,
    }
}

fn evaluate_limits(value: f64, limits: &[Limit]) -> &'static str {
    if limits.iter().all(|l| check_limit(value, l)) {
        "passed"
    } else {
        "failed"
    }
}

fn parse_measurement(response: &str) -> f64 {
    if let Ok(v) = response.trim().parse::<f64>() {
        return v;
    }
    response
        .split(',')
        .next()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

type InstrumentMap = Arc<Mutex<HashMap<String, Box<dyn SCPIInstrument>>>>;

async fn ensure_instrument(
    step_instr_name: &str,
    campaign: &TestCampaign,
    pool: &InstrumentMap,
) -> Result<(), String> {
    let mut map = pool.lock().await;
    if map.contains_key(step_instr_name) {
        return Ok(());
    }
    let config = campaign
        .instruments
        .get(step_instr_name)
        .ok_or_else(|| format!("instrument '{}' not defined in campaign", step_instr_name))?;

    let timeout = config.timeout.unwrap_or(5000);
    let mut instr = create_instrument(&config.kind, &config.resource, timeout);
    instr
        .connect()
        .await
        .map_err(|e| format!("connect {}: {}", step_instr_name, e))?;
    map.insert(step_instr_name.to_string(), instr);
    Ok(())
}

async fn query_instrument(
    step_instr_name: &str,
    cmd: &str,
    pool: &InstrumentMap,
) -> Result<String, String> {
    let mut map = pool.lock().await;
    let instr = map
        .get_mut(step_instr_name)
        .ok_or_else(|| format!("instrument '{}' not connected", step_instr_name))?;
    instr.query(cmd).await.map_err(|e| format!("query error: {}", e))
}

async fn run_step(
    step: &TestStep,
    campaign: &TestCampaign,
    pool: &InstrumentMap,
) -> Value {
    let mut step_result = json!({
        "name": step.name,
        "description": step.description,
        "status": "passed",
        "measurements": [],
        "error": null,
    });

    if let Err(e) = ensure_instrument(&step.instrument, campaign, pool).await {
        if let Some(obj) = step_result.as_object_mut() {
            obj.insert("status".into(), json!("failed"));
            obj.insert("error".into(), json!(e));
        }
        return step_result;
    }

    for _ in 0..step.repeat {
        if step.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(step.delay_ms)).await;
        }

        let cmd = step.command.as_deref().unwrap_or("*IDN?");
        let response = match query_instrument(&step.instrument, cmd, pool).await {
            Ok(r) => r,
            Err(e) => {
                if let Some(obj) = step_result.as_object_mut() {
                    obj.insert("status".into(), json!("failed"));
                    obj.insert("error".into(), json!(e));
                }
                return step_result;
            }
        };

        for meas in &step.measurements {
            let value = parse_measurement(&response);
            let verdict = meas
                .limits
                .as_ref()
                .map_or("passed", |l| evaluate_limits(value, l));

            let meas_result = json!({
                "name": meas.name,
                "value": value,
                "limits": meas.limits.as_ref().map(|l| {
                    json!(l.iter().map(|lim| {
                        json!({
                            "op": lim.op,
                            "value": lim.value,
                            "tolerance": lim.tolerance,
                        })
                    }).collect::<Vec<_>>())
                }),
                "verdict": verdict,
            });

            if let Some(arr) = step_result.get_mut("measurements").and_then(|v| v.as_array_mut()) {
                arr.push(meas_result);
            }

            if verdict == "failed" {
                if let Some(obj) = step_result.as_object_mut() {
                    obj.insert("status".into(), json!("failed"));
                }
            }
        }
    }

    step_result
}

pub async fn run_campaign(campaign: &TestCampaign) -> Result<Value, String> {
    let pool: InstrumentMap = Arc::new(Mutex::new(HashMap::new()));
    let mut groups = serde_json::Map::new();

    for (group_name, group) in &campaign.groups {
        let mut steps = Vec::new();
        for step in &group.steps {
            let result = run_step(step, campaign, &pool).await;
            steps.push(result);
        }

        let status = if steps.iter().any(|s| s["status"] == "failed") {
            "failed"
        } else {
            "passed"
        };

        groups.insert(
            group_name.clone(),
            json!({
                "name": group.name,
                "description": group.description,
                "status": status,
                "steps": steps,
            }),
        );
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let results = json!({
        "title": campaign.title,
        "version": campaign.version.as_deref().unwrap_or("1.0"),
        "timestamp": now.to_string(),
        "groups": groups,
    });

    Ok(results)
}

pub fn format_summary(results: &Value) -> String {
    let total: usize = results
        .pointer("/groups")
        .and_then(|v| v.as_object())
        .map(|g| {
            g.values()
                .filter_map(|v| v.get("steps").and_then(|s| s.as_array()))
                .map(|s| s.len())
                .sum()
        })
        .unwrap_or(0);
    let failed: usize = results
        .pointer("/groups")
        .and_then(|v| v.as_object())
        .map(|g| {
            g.values()
                .filter_map(|v| v.get("steps").and_then(|s| s.as_array()))
                .flatten()
                .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("failed"))
                .count()
        })
        .unwrap_or(0);
    format!("{}/{} passed, {} failed", total - failed, total, failed)
}

use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::Stream;

pub fn run_campaign_stream(campaign: TestCampaign) -> impl Stream<Item = (String, Value)> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::spawn(async move {
        let pool: InstrumentMap = Arc::new(Mutex::new(HashMap::new()));

        for (group_name, group) in &campaign.groups {
            let mut steps = Vec::new();
            for step in &group.steps {
                let result = run_step(step, &campaign, &pool).await;
                steps.push(result);
            }

            let status = if steps.iter().any(|s| s["status"] == "failed") {
                "failed"
            } else {
                "passed"
            };

            let group_result = json!({
                "name": group.name,
                "description": group.description,
                "status": status,
                "steps": steps,
            });

            let _ = tx.send((group_name.clone(), group_result));
        }
    });

    UnboundedReceiverStream::new(rx)
}
