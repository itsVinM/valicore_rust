use std::collections::HashMap;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

// ── YAML structure ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ScopeLibrary {
    #[serde(rename = "OSCILLOSCOPES")]
    scopes: HashMap<String, ScopeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct ScopeSpec {
    description: String,
    ip_address: String,
    endian: String,
    quirks: String,
    cmds: HashMap<String, String>,
}

// ── Runtime oscilloscope ────────────────────────────────────

pub struct Oscilloscope {
    brand: String,
    spec: ScopeSpec,
    stream: Option<TcpStream>,
    buf: Vec<u8>,
    timeout_ms: u64,
}

impl Oscilloscope {
    fn load_library() -> Result<ScopeLibrary, String> {
        let yaml = include_str!("../valicore/specs/oscilloscope.yaml");
        serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse: {e}"))
    }

    pub fn brands() -> Vec<String> {
        let lib = Self::load_library().unwrap();
        let mut brands: Vec<_> = lib.scopes.into_keys().collect();
        brands.sort();
        brands
    }

    pub fn info(brand: &str) -> Result<String, String> {
        let lib = Self::load_library()?;
        let spec = lib
            .scopes
            .get(brand)
            .ok_or_else(|| format!("unknown brand '{brand}'"))?;
        Ok(format!("{} — {}", brand, spec.description))
    }

    pub fn new(brand: &str, timeout_ms: u64) -> Result<Self, String> {
        let lib = Self::load_library()?;
        let spec = lib
            .scopes
            .get(brand)
            .ok_or_else(|| {
                let mut keys: Vec<_> = lib.scopes.into_keys().collect();
                keys.sort();
                format!("unknown brand '{brand}', available: {}", keys.join(", "))
            })?
            .clone();
        Ok(Self { brand: brand.to_string(), spec, stream: None, buf: Vec::with_capacity(4096), timeout_ms })
    }

    pub fn brand(&self) -> &str {
        &self.brand
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    pub async fn connect(&mut self, addr: &str, port: u16) -> Result<(), String> {
        let endpoint = format!("{addr}:{port}");
        let dur = Duration::from_millis(self.timeout_ms);
        let s = timeout(dur, TcpStream::connect(&endpoint))
            .await
            .map_err(|_| format!("connect timeout to {endpoint}"))?
            .map_err(|e| format!("connect failed: {e}"))?;
        self.stream = Some(s);
        if !self.spec.quirks.is_empty() {
            self.write(&self.spec.quirks).await?;
        }
        Ok(())
    }

    pub async fn close(&mut self) {
        if let Some(mut s) = self.stream.take() {
            let _ = s.shutdown().await;
        }
    }

    pub async fn write(&mut self, cmd: &str) -> Result<(), String> {
        let stream = self.stream.as_mut().ok_or_else(|| "not connected".to_string())?;
        let line = format!("{cmd}\n");
        timeout(Duration::from_millis(self.timeout_ms), stream.write_all(line.as_bytes()))
            .await
            .map_err(|_| "write timeout".to_string())?
            .map_err(|e| format!("write failed: {e}"))
    }

    pub async fn query(&mut self, cmd: &str) -> Result<String, String> {
        self.write(cmd).await?;
        let stream = self.stream.as_mut().ok_or_else(|| "not connected".to_string())?;
        self.buf.clear();
        let dur = Duration::from_millis(self.timeout_ms);
        timeout(dur, read_line(stream, &mut self.buf))
            .await
            .map_err(|_| "read timeout".to_string())?
            .map_err(|e| format!("read failed: {e}"))?;
        Ok(String::from_utf8_lossy(&self.buf).trim().to_string())
    }

    /// Build a command by substituting `{key}` placeholders in the YAML template.
    pub fn cmd(&self, name: &str, subs: &[(&str, &str)]) -> Result<String, String> {
        let tpl = self
            .spec
            .cmds
            .get(name)
            .ok_or_else(|| format!("command '{name}' not found for '{}'", self.brand))?;
        let mut s = tpl.clone();
        for (k, v) in subs {
            s = s.replace(&format!("{{{k}}}"), v);
        }
        Ok(s)
    }

    pub fn commands(&self) -> Vec<String> {
        let mut keys: Vec<_> = self.spec.cmds.keys().cloned().collect();
        keys.sort();
        keys
    }

    // ── Acquire ──────────────────────────────────────────────

    pub async fn get_waveform(&mut self, channel: &str) -> Result<Vec<f64>, String> {
        if self.spec.cmds.contains_key("set_source") {
            let cmd = self.cmd("set_source", &[("ch", channel)])?;
            self.write(&cmd).await?;
        }
        let _ = self.write(":WAVeform:FORMat ASCII").await;
        let raw_cmd = self.cmd("get_raw", &[("ch", channel)])?;
        let raw = self.query(&raw_cmd).await?;
        parse_waveform_csv(&raw)
    }

    // ── Setters ──────────────────────────────────────────────

    pub async fn set_v_scale(&mut self, channel: &str, val: f64) -> Result<(), String> {
        let cmd = self.cmd("set_v_scale", &[("ch", channel), ("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_v_offset(&mut self, channel: &str, val: f64) -> Result<(), String> {
        let cmd = self.cmd("set_v_offset", &[("ch", channel), ("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_coupling(&mut self, channel: &str, val: &str) -> Result<(), String> {
        let cmd = self.cmd("set_coupling", &[("ch", channel), ("val", val)])?;
        self.write(&cmd).await
    }

    pub async fn set_h_scale(&mut self, val: f64) -> Result<(), String> {
        let cmd = self.cmd("set_h_scale", &[("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_h_pos(&mut self, val: f64) -> Result<(), String> {
        let cmd = self.cmd("set_h_pos", &[("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_trig_source(&mut self, channel: &str, edge: Option<&str>) -> Result<(), String> {
        let mut subs = vec![("ch", channel)];
        if let Some(e) = edge {
            subs.push(("edge", e));
        }
        let cmd = self.cmd("set_trig_source", &subs)?;
        self.write(&cmd).await
    }

    pub async fn set_trig_level(&mut self, val: f64, edge: Option<&str>) -> Result<(), String> {
        let mut subs = vec![("val", &fmt_val(val))];
        if let Some(e) = edge {
            subs.push(("edge", e));
        }
        let cmd = self.cmd("set_trig_level", &subs)?;
        self.write(&cmd).await
    }

    pub async fn set_trig_slope(&mut self, val: &str, edge: Option<&str>) -> Result<(), String> {
        let mut subs = vec![("val", val)];
        if let Some(e) = edge {
            subs.push(("edge", e));
        }
        let cmd = self.cmd("set_trig_slope", &subs)?;
        self.write(&cmd).await
    }

    pub async fn set_ch_on(&mut self, channel: &str) -> Result<(), String> {
        let cmd = self.cmd("set_ch_on", &[("ch", channel)])?;
        self.write(&cmd).await
    }

    pub async fn set_ch_off(&mut self, channel: &str) -> Result<(), String> {
        let cmd = self.cmd("set_ch_off", &[("ch", channel)])?;
        self.write(&cmd).await
    }

    // ── Getters ──────────────────────────────────────────────

    pub async fn get_v_scale(&mut self, channel: &str) -> Result<f64, String> {
        let cmd = self.cmd("get_v_scale", &[("ch", channel)])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| format!("parse '{resp}': {e}"))
    }

    pub async fn get_v_offset(&mut self, channel: &str) -> Result<f64, String> {
        let cmd = self.cmd("get_v_offset", &[("ch", channel)])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| format!("parse '{resp}': {e}"))
    }

    pub async fn get_coupling(&mut self, channel: &str) -> Result<String, String> {
        let cmd = self.cmd("get_coupling", &[("ch", channel)])?;
        self.query(&cmd).await
    }

    pub async fn get_h_scale(&mut self) -> Result<f64, String> {
        let cmd = self.cmd("get_h_scale", &[])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| format!("parse '{resp}': {e}"))
    }

    pub async fn get_h_pos(&mut self) -> Result<f64, String> {
        let cmd = self.cmd("get_h_pos", &[])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| format!("parse '{resp}': {e}"))
    }

    pub async fn get_trig_source(&mut self) -> Result<String, String> {
        let cmd = self.cmd("get_trig_source", &[])?;
        self.query(&cmd).await
    }

    pub async fn get_trig_level(&mut self) -> Result<f64, String> {
        let cmd = self.cmd("get_trig_level", &[])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| format!("parse '{resp}': {e}"))
    }

    pub async fn get_trig_slope(&mut self) -> Result<String, String> {
        let cmd = self.cmd("get_trig_slope", &[])?;
        self.query(&cmd).await
    }

    // ── Actions ──────────────────────────────────────────────

    pub async fn reset(&mut self) -> Result<(), String> {
        let cmd = self.cmd("reset", &[])?;
        self.write(&cmd).await
    }

    pub async fn autoset(&mut self) -> Result<(), String> {
        let cmd = self.cmd("autoset", &[])?;
        self.write(&cmd).await
    }

    pub async fn run(&mut self) -> Result<(), String> {
        let cmd = self.cmd("run", &[])?;
        self.write(&cmd).await
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        let cmd = self.cmd("stop", &[])?;
        self.write(&cmd).await
    }

    pub async fn single(&mut self) -> Result<(), String> {
        let cmd = self.cmd("single", &[])?;
        self.write(&cmd).await
    }

    pub async fn check_ch(&mut self, channel: &str) -> Result<bool, String> {
        let cmd = self.cmd("check_ch", &[("ch", channel)])?;
        let resp = self.query(&cmd).await?;
        let trimmed = resp.trim();
        Ok(trimmed == "1" || trimmed.eq_ignore_ascii_case("ON"))
    }
}

// ── Helpers ─────────────────────────────────────────────────

fn fmt_val(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1e12 {
        format!("{:.0}", val)
    } else {
        format!("{:.6}", val)
    }
}

fn parse_waveform_csv(raw: &str) -> Result<Vec<f64>, String> {
    let body = raw.trim();
    let start = body
        .find(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
        .unwrap_or(body.len());
    let data = &body[start..];
    if data.is_empty() {
        return Ok(Vec::new());
    }
    data.split(',')
        .map(|s| s.trim().parse::<f64>().map_err(|e| format!("parse '{s}': {e}")))
        .collect()
}

async fn read_line(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Result<usize, std::io::Error> {
    let mut tmp = [0u8; 1];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "closed"));
        }
        buf.push(tmp[0]);
        if tmp[0] == b'\n' {
            return Ok(buf.len());
        }
    }
}
