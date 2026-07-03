use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

// ── Error types ─────────────────────────────────────────────

#[derive(Debug)]
pub enum ScopeError {
    Connection(String),
    Acquisition(String),
    Config(String),
    Io(String),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScopeError::Connection(msg) => write!(f, "connection: {msg}"),
            ScopeError::Acquisition(msg) => write!(f, "acquisition: {msg}"),
            ScopeError::Config(msg) => write!(f, "config: {msg}"),
            ScopeError::Io(msg) => write!(f, "io: {msg}"),
        }
    }
}

impl std::error::Error for ScopeError {}

// ── YAML structure ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct IpConfig {
    port: u16,
}

#[derive(Debug, Deserialize)]
struct ScopeLibrary {
    ip_config: Option<IpConfig>,
    #[serde(rename = "OSCILLOSCOPES")]
    scopes: HashMap<String, ScopeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct ScopeSpec {
    description: String,
    default_ip: String,
    idn_pattern: String,
    endian: String,
    quirks: String,
    cmds: HashMap<String, String>,
}

// ── Settings/Getting dispatch maps ────────────────────────────
// Maps setting name → (command_key, [param_names])

const SETTINGS: &[(&str, &str, &[&str])] = &[
    ("vertical_scale", "set_v_scale", &["ch", "val"]),
    ("vertical_offset", "set_v_offset", &["ch", "val"]),
    ("coupling", "set_coupling", &["ch", "val"]),
    ("timebase", "set_h_scale", &["val"]),
    ("time_position", "set_h_pos", &["val"]),
    ("trigger_source", "set_trig_source", &["ch"]),
    ("trigger_level", "set_trig_level", &["ch", "val"]),
    ("trigger_slope", "set_trig_slope", &["ch", "val"]),
];

const GETTINGS: &[(&str, &str, &[&str])] = &[
    ("vertical_scale", "get_v_scale", &["ch"]),
    ("vertical_offset", "get_v_offset", &["ch"]),
    ("coupling", "get_coupling", &["ch"]),
    ("timebase", "get_h_scale", &[]),
    ("time_position", "get_h_pos", &[]),
    ("trigger_source", "get_trig_source", &["ch"]),
    ("trigger_level", "get_trig_level", &["ch"]),
    ("trigger_slope", "get_trig_slope", &["ch"]),
];

fn find_setting(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    SETTINGS.iter().find(|(n, _, _)| *n == name).map(|(_, c, p)| (*c, *p))
}

fn find_getting(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    GETTINGS.iter().find(|(n, _, _)| *n == name).map(|(_, c, p)| (*c, *p))
}

// ── Runtime oscilloscope ────────────────────────────────────

pub struct Oscilloscope {
    brand: String,
    spec: ScopeSpec,
    stream: Option<TcpStream>,
    buf: Vec<u8>,
    timeout_ms: u64,
    default_port: u16,
    active_channels: Vec<u8>,
    instrument_id: String,
}

impl Oscilloscope {
    fn load_library() -> Result<ScopeLibrary, String> {
        let yaml = include_str!("../valicore/specs/oscilloscope.yaml");
        serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse: {e}"))
    }

    pub fn brands() -> Vec<String> {
        let mut brands: Vec<_> = Self::load_library().unwrap().scopes.into_keys().collect();
        brands.sort();
        brands
    }

    pub fn info(brand: &str) -> Result<String, String> {
        let spec = Self::load_library()?
            .scopes
            .get(brand)
            .ok_or_else(|| format!("unknown brand '{brand}'"))?
            .clone();
        Ok(format!("{brand} — {}", spec.description))
    }

    /// Connect to an instrument, query *IDN?, and auto-detect its brand
    /// by matching against each brand's `idn_pattern`.
    pub async fn detect_brand(addr: &str, port: u16, timeout_ms: u64) -> Result<String, ScopeError> {
        let endpoint = format!("{addr}:{port}");
        let dur = Duration::from_millis(timeout_ms);
        let mut stream = timeout(dur, TcpStream::connect(&endpoint))
            .await
            .map_err(|_| ScopeError::Connection(format!("timeout to {endpoint}")))?
            .map_err(|e| ScopeError::Connection(format!("{endpoint}: {e}")))?;

        let _ = timeout(dur, stream.write_all(b"*IDN?\n")).await;
        let mut buf = Vec::new();
        let _ = timeout(dur, read_line(&mut stream, &mut buf)).await;
        let _ = stream.shutdown().await;
        let idn = String::from_utf8_lossy(&buf).trim().to_string();

        let lib = Self::load_library().map_err(ScopeError::Config)?;
        for (brand, spec) in &lib.scopes {
            if idn.to_uppercase().contains(&spec.idn_pattern.to_uppercase()) {
                return Ok(brand.clone());
            }
        }
        Err(ScopeError::Config(format!(
            "no brand matches *IDN? response: {idn}"
        )))
    }

    /// Create and connect an Oscilloscope with auto-detected brand.
    /// Port defaults to 5025 for probe, then to the detected brand's default port for connection.
    pub async fn from_ip(
        addr: &str,
        port: Option<u16>,
        timeout_ms: u64,
    ) -> Result<Self, ScopeError> {
        let probe_port = port.unwrap_or(5025);
        let brand = Self::detect_brand(addr, probe_port, timeout_ms).await?;
        let mut scope = Self::new(&brand, timeout_ms)?;
        let connect_port = port.unwrap_or(scope.default_port);
        scope.connect(addr, connect_port).await?;
        Ok(scope)
    }

    pub fn new(brand: &str, timeout_ms: u64) -> Result<Self, ScopeError> {
        let lib = Self::load_library().map_err(ScopeError::Config)?;
        let spec = lib
            .scopes
            .get(brand)
            .cloned()
            .ok_or_else(|| {
                let available = Oscilloscope::brands();
                ScopeError::Config(format!("unknown brand '{brand}', available: {}", available.join(", ")))
            })?;
        let default_port = lib.ip_config.as_ref().map(|c| c.port).unwrap_or(5025);
        Ok(Self {
            brand: brand.to_string(),
            spec,
            stream: None,
            buf: Vec::with_capacity(65536),
            timeout_ms,
            default_port,
            active_channels: Vec::new(),
            instrument_id: "OFFLINE".to_string(),
        })
    }

    pub fn brand(&self) -> &str {
        &self.brand
    }

    pub fn default_port(&self) -> u16 {
        self.default_port
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    pub fn active_channels(&self) -> &[u8] {
        &self.active_channels
    }

    pub fn instrument_id(&self) -> &str {
        &self.instrument_id
    }

    // ── Connection ──────────────────────────────────────────

    pub async fn connect(&mut self, addr: &str, port: u16) -> Result<(), ScopeError> {
        let endpoint = format!("{addr}:{port}");
        let dur = Duration::from_millis(self.timeout_ms);
        let s = timeout(dur, TcpStream::connect(&endpoint))
            .await
            .map_err(|_| ScopeError::Connection(format!("timeout to {endpoint}")))?
            .map_err(|e| ScopeError::Connection(format!("{endpoint}: {e}")))?;
        self.stream = Some(s);

        if !self.spec.quirks.is_empty() {
            let quirks = self.spec.quirks.clone();
            self.write(&quirks).await?;
        }

        let idn = self.query("*IDN?").await.unwrap_or_else(|_| "UNKNOWN".to_string());
        self.instrument_id = idn.clone();

        self.active_channels.clear();
        for ch in 1..=4 {
            let ch_str = ch.to_string();
            match self.cmd("check_ch", &[("ch", &ch_str)]) {
                Ok(cmd) => {
                    if let Ok(resp) = self.query(&cmd).await {
                        let t = resp.trim();
                        if t == "1" || t.eq_ignore_ascii_case("ON") {
                            self.active_channels.push(ch);
                        }
                    }
                }
                Err(_) => break,
            }
        }

        Ok(())
    }

    pub async fn close(&mut self) {
        if let Some(mut s) = self.stream.take() {
            let _ = s.shutdown().await;
        }
        self.active_channels.clear();
    }

    // ── Low-level I/O ──────────────────────────────────────

    pub async fn write(&mut self, cmd: &str) -> Result<(), ScopeError> {
        let stream = self.stream.as_mut().ok_or_else(|| ScopeError::Connection("not connected".into()))?;
        let line = format!("{cmd}\n");
        timeout(Duration::from_millis(self.timeout_ms), stream.write_all(line.as_bytes()))
            .await
            .map_err(|_| ScopeError::Io("write timeout".into()))?
            .map_err(|e| ScopeError::Io(format!("write: {e}")))
    }

    pub async fn query(&mut self, cmd: &str) -> Result<String, ScopeError> {
        self.write(cmd).await?;
        let stream = self.stream.as_mut().ok_or_else(|| ScopeError::Connection("not connected".into()))?;
        self.buf.clear();
        let dur = Duration::from_millis(self.timeout_ms);
        timeout(dur, read_line(stream, &mut self.buf))
            .await
            .map_err(|_| ScopeError::Io("read timeout".into()))?
            .map_err(|e| ScopeError::Io(format!("read: {e}")))?;
        Ok(String::from_utf8_lossy(&self.buf).trim().to_string())
    }

    pub async fn query_binary(&mut self, cmd: &str) -> Result<Vec<f64>, ScopeError> {
        self.write(cmd).await?;
        let stream = self.stream.as_mut().ok_or_else(|| ScopeError::Connection("not connected".into()))?;
        let dur = Duration::from_millis(self.timeout_ms);
        let raw = timeout(dur, read_binary_block(stream))
            .await
            .map_err(|_| ScopeError::Io("binary read timeout".into()))?
            .map_err(|e| ScopeError::Io(format!("binary: {e}")))?;

        let big_endian = self.spec.endian.eq_ignore_ascii_case("big");
        let samples: Vec<f64> = raw
            .chunks_exact(4)
            .map(|b| {
                let v = if big_endian {
                    f32::from_be_bytes([b[0], b[1], b[2], b[3]])
                } else {
                    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
                };
                v as f64
            })
            .collect();
        Ok(samples)
    }

    // ── Command dispatch ───────────────────────────────────

    pub fn cmd(&self, name: &str, subs: &[(&str, &str)]) -> Result<String, ScopeError> {
        let tpl = self
            .spec
            .cmds
            .get(name)
            .ok_or_else(|| ScopeError::Config(format!("command '{name}' not found for '{}'", self.brand)))?;
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

    /// Generic setter: looks up name in SETTINGS map, passes kwargs through to cmd().
    pub async fn setting(&mut self, name: &str, kwargs: &[(&str, &str)]) -> Result<(), ScopeError> {
        let (cmd_key, _) = find_setting(name)
            .ok_or_else(|| ScopeError::Config(format!("unknown setting '{name}'")))?;
        let cmd = self.cmd(cmd_key, kwargs)?;
        self.write(&cmd).await
    }

    /// Generic getter: looks up name in GETTINGS map, passes kwargs through to cmd().
    pub async fn getting(&mut self, name: &str, kwargs: &[(&str, &str)]) -> Result<String, ScopeError> {
        let (cmd_key, _) = find_getting(name)
            .ok_or_else(|| ScopeError::Config(format!("unknown getting '{name}'")))?;
        let cmd = self.cmd(cmd_key, kwargs)?;
        self.query(&cmd).await
    }

    pub fn available_settings() -> Vec<&'static str> {
        SETTINGS.iter().map(|(n, _, _)| *n).collect()
    }

    pub fn available_gettings() -> Vec<&'static str> {
        GETTINGS.iter().map(|(n, _, _)| *n).collect()
    }

    // ── Convenience setters ────────────────────────────────

    pub async fn set_v_scale(&mut self, channel: &str, val: f64) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_v_scale", &[("ch", channel), ("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_v_offset(&mut self, channel: &str, val: f64) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_v_offset", &[("ch", channel), ("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_coupling(&mut self, channel: &str, val: &str) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_coupling", &[("ch", channel), ("val", val)])?;
        self.write(&cmd).await
    }

    pub async fn set_h_scale(&mut self, val: f64) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_h_scale", &[("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_h_pos(&mut self, val: f64) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_h_pos", &[("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_trig_source(&mut self, channel: &str) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_trig_source", &[("ch", channel)])?;
        self.write(&cmd).await
    }

    pub async fn set_trig_level(&mut self, val: f64) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_trig_level", &[("val", &fmt_val(val))])?;
        self.write(&cmd).await
    }

    pub async fn set_trig_slope(&mut self, val: &str) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_trig_slope", &[("val", val)])?;
        self.write(&cmd).await
    }

    pub async fn set_ch_on(&mut self, channel: &str) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_ch_on", &[("ch", channel)])?;
        self.write(&cmd).await?;
        if let Ok(ch) = channel.parse::<u8>() {
            if !self.active_channels.contains(&ch) {
                self.active_channels.push(ch);
            }
        }
        Ok(())
    }

    pub async fn set_ch_off(&mut self, channel: &str) -> Result<(), ScopeError> {
        let cmd = self.cmd("set_ch_off", &[("ch", channel)])?;
        self.write(&cmd).await?;
        if let Ok(ch) = channel.parse::<u8>() {
            self.active_channels.retain(|&c| c != ch);
        }
        Ok(())
    }

    // ── Convenience getters ────────────────────────────────

    pub async fn get_v_scale(&mut self, channel: &str) -> Result<f64, ScopeError> {
        let cmd = self.cmd("get_v_scale", &[("ch", channel)])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| ScopeError::Acquisition(format!("parse '{resp}': {e}")))
    }

    pub async fn get_v_offset(&mut self, channel: &str) -> Result<f64, ScopeError> {
        let cmd = self.cmd("get_v_offset", &[("ch", channel)])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| ScopeError::Acquisition(format!("parse '{resp}': {e}")))
    }

    pub async fn get_coupling(&mut self, channel: &str) -> Result<String, ScopeError> {
        let cmd = self.cmd("get_coupling", &[("ch", channel)])?;
        self.query(&cmd).await
    }

    pub async fn get_h_scale(&mut self) -> Result<f64, ScopeError> {
        let cmd = self.cmd("get_h_scale", &[])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| ScopeError::Acquisition(format!("parse '{resp}': {e}")))
    }

    pub async fn get_h_pos(&mut self) -> Result<f64, ScopeError> {
        let cmd = self.cmd("get_h_pos", &[])?;
        let resp = self.query(&cmd).await?;
        resp.trim().parse().map_err(|e| ScopeError::Acquisition(format!("parse '{resp}': {e}")))
    }

    // ── Actions ──────────────────────────────────────────────

    pub async fn reset(&mut self) -> Result<(), ScopeError> {
        let cmd = self.cmd("reset", &[])?;
        self.write(&cmd).await
    }

    pub async fn autoset(&mut self) -> Result<(), ScopeError> {
        let cmd = self.cmd("autoset", &[])?;
        self.write(&cmd).await
    }

    pub async fn run(&mut self) -> Result<(), ScopeError> {
        let cmd = self.cmd("run", &[])?;
        self.write(&cmd).await
    }

    pub async fn stop(&mut self) -> Result<(), ScopeError> {
        let cmd = self.cmd("stop", &[])?;
        self.write(&cmd).await
    }

    pub async fn single(&mut self) -> Result<(), ScopeError> {
        let cmd = self.cmd("single", &[])?;
        self.write(&cmd).await
    }

    pub async fn check_ch(&mut self, channel: &str) -> Result<bool, ScopeError> {
        let cmd = self.cmd("check_ch", &[("ch", channel)])?;
        let resp = self.query(&cmd).await?;
        let t = resp.trim();
        Ok(t == "1" || t.eq_ignore_ascii_case("ON"))
    }

    // ── Waveform acquisition ──────────────────────────────────
    // ASCII waveform (universal fallback)

    pub async fn get_waveform(&mut self, channel: &str) -> Result<Vec<f64>, ScopeError> {
        if self.spec.cmds.contains_key("set_source") {
            let cmd = self.cmd("set_source", &[("ch", channel)])?;
            self.write(&cmd).await?;
        }
        let _ = self.write(":WAVeform:FORMat ASCII").await;
        let raw_cmd = self.cmd("get_raw", &[("ch", channel)])?;
        let raw = self.query(&raw_cmd).await?;
        parse_waveform_csv(&raw)
    }

    // Binary multi-channel acquisition (power-user path)

    pub async fn get_all_waveforms(&mut self) -> Result<WaveformResult, ScopeError> {
        if self.active_channels.is_empty() {
            return Err(ScopeError::Acquisition("no active channels".into()));
        }

        let sr_raw = self.query(":ACQuire:SRATe?").await?;
        let sample_rate: f64 = sr_raw
            .trim()
            .parse()
            .map_err(|e| ScopeError::Acquisition(format!("parse sample rate '{sr_raw}': {e}")))?;

        let mut data_matrix: Vec<Vec<f64>> = Vec::new();
        let mut failed_channels: Vec<u8> = Vec::new();
        let channels = self.active_channels.clone();

        for &ch in &channels {
            let ch_str = ch.to_string();

            if self.spec.cmds.contains_key("set_source") {
                match self.cmd("set_source", &[("ch", &ch_str)]) {
                    Ok(cmd) => { let _ = self.write(&cmd).await; }
                    Err(_) => {}
                }
            }

            match self.cmd("get_raw", &[("ch", &ch_str)]) {
                Ok(cmd) => match self.query_binary(&cmd).await {
                    Ok(data) => {
                        if !data.is_empty() {
                            data_matrix.push(data);
                        } else {
                            failed_channels.push(ch);
                        }
                    }
                    Err(_) => {
                        failed_channels.push(ch);
                    }
                },
                Err(_) => {
                    failed_channels.push(ch);
                }
            }
        }

        if data_matrix.is_empty() {
            return Err(ScopeError::Acquisition(
                format!("all channels failed: {failed_channels:?}"),
            ));
        }

        if !failed_channels.is_empty() {
            // partial — already collected
        }

        let min_len = data_matrix.iter().map(|d| d.len()).min().unwrap_or(0);
        let data_matrix: Vec<Vec<f64>> = data_matrix
            .into_iter()
            .map(|d| d[..min_len].to_vec())
            .collect();
        let time_axis: Vec<f64> = (0..min_len).map(|i| i as f64 / sample_rate).collect();

        let active_channels: Vec<String> = self.active_channels.iter().map(|c| c.to_string()).collect();
        let failed_channels_str: Vec<String> = failed_channels.iter().map(|c| c.to_string()).collect();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut metadata = HashMap::new();
        metadata.insert("sample_rate".to_string(), format!("{sample_rate:.0}"));
        metadata.insert(
            "channels".to_string(),
            active_channels.join(","),
        );
        metadata.insert(
            "failed_channels".to_string(),
            failed_channels_str.join(","),
        );
        metadata.insert("num_samples".to_string(), min_len.to_string());
        metadata.insert("timestamp".to_string(), ts.to_string());
        metadata.insert("instrument".to_string(), self.instrument_id.clone());

        Ok(WaveformResult { time_axis, data_matrix, metadata })
    }
}

// ── Data structures for multi-channel acquisition ──────────

pub struct WaveformResult {
    pub time_axis: Vec<f64>,
    pub data_matrix: Vec<Vec<f64>>,
    pub metadata: HashMap<String, String>,
}

// ── Helpers ─────────────────────────────────────────────────

fn fmt_val(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1e12 {
        format!("{:.0}", val)
    } else {
        format!("{:.6}", val)
    }
}

fn parse_waveform_csv(raw: &str) -> Result<Vec<f64>, ScopeError> {
    let body = raw.trim();
    let start = body
        .find(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
        .unwrap_or(body.len());
    let data = &body[start..];
    if data.is_empty() {
        return Ok(Vec::new());
    }
    data.split(',')
        .map(|s| s.trim().parse::<f64>().map_err(|e| ScopeError::Acquisition(format!("parse '{s}': {e}"))))
        .collect()
}

/// Read one line (text response) from TCP stream.
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

/// Read a SCPI binary block (`#<n><length><data>`).
async fn read_binary_block(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut byte = [0u8; 1];

    loop {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|e| format!("read header: {e}"))?;
        if byte[0] == b'#' {
            break;
        }
    }

    stream
        .read_exact(&mut byte)
        .await
        .map_err(|e| format!("read digit count: {e}"))?;
    let digit_count = (byte[0] - b'0') as usize;
    if digit_count == 0 || digit_count > 12 {
        return Err(format!("invalid digit count: {digit_count}"));
    }

    let mut len_buf = vec![0u8; digit_count];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("read length: {e}"))?;
    let len_str = String::from_utf8_lossy(&len_buf);
    let data_len: usize = len_str
        .parse()
        .map_err(|e| format!("parse length '{len_str}': {e}"))?;

    let mut data = vec![0u8; data_len];
    let mut offset = 0;
    while offset < data_len {
        let n = stream
            .read(&mut data[offset..])
            .await
            .map_err(|e| format!("read data: {e}"))?;
        if n == 0 {
            return Err("connection closed during binary read".into());
        }
        offset += n;
    }

    let _ = stream.read(&mut byte).await; // skip trailing \n
    Ok(data)
}
