use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

// Errors 
#[derive(Debug, thiserror::Error)]
pub enum InstrumentError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("SCPI write failed: {0}")]
    WriteFailed(String),
    #[error("SCPI query failed: {0}")]
    QueryFailed(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("instrument closed")]
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("connection: {0}")]
    Connection(String),
    #[error("acquisition: {0}")]
    Acquisition(String),
    #[error("config: {0}")]
    Config(String),
    #[error("io: {0}")]
    Io(String),
}

pub fn fmt_endpoint(addr: &str, port: u16, buf: &mut [u8; 48]) -> usize {
    let ab = addr.as_bytes();
    buf[..ab.len()].copy_from_slice(ab);
    buf[ab.len()] = b':';
    let mut tmp = [0u8; 5];
    let mut n = 0;
    let mut p = port;
    if p == 0 { tmp[n] = b'0'; n += 1; }
    while p > 0 { tmp[n] = b'0' + (p % 10) as u8; p /= 10; n += 1; }
    let start = ab.len() + 1;
    for i in 0..n { buf[start + i] = tmp[n - 1 - i]; }
    start + n
}

// Stack-based line reader 

async fn read_line(stream: &mut TcpStream, buf: &mut [u8], pos: &mut usize) -> Result<(), std::io::Error> {
    let mut tmp = [0u8; 1];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 { return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "closed")); }
        if *pos < buf.len() { buf[*pos] = tmp[0]; }
        *pos += 1;
        if tmp[0] == b'\n' { return Ok(()); }
    }
}

//SCPI trait (used by runner for any instrument) 

#[async_trait]
pub trait SCPIInstrument: Send {
    async fn connect(&mut self) -> Result<(), InstrumentError>;
    async fn write(&mut self, cmd: &str) -> Result<(), InstrumentError>;
    async fn query(&mut self, cmd: &str) -> Result<String, InstrumentError>;
    async fn close(&mut self) -> Result<(), InstrumentError>;
    fn resource(&self) -> &str;
    fn kind(&self) -> &str;
}

pub struct TcpInstrument {
    resource: String,
    kind: String,
    addr: String,
    port: u16,
    stream: Option<TcpStream>,
    timeout_ms: u64,
    buf: [u8; 4096],
    pos: usize,
}

impl TcpInstrument {
    pub fn new(resource: &str, kind: &str, timeout_ms: u64) -> Self {
        let (addr, port) = parse_resource(resource);
        Self { resource: resource.to_string(), kind: kind.to_string(), addr, port, stream: None, timeout_ms, buf: [0u8; 4096], pos: 0 }
    }
}

#[async_trait]
impl SCPIInstrument for TcpInstrument {
    fn resource(&self) -> &str { &self.resource }
    fn kind(&self) -> &str { &self.kind }

    async fn connect(&mut self) -> Result<(), InstrumentError> {
        if self.stream.is_some() { return Ok(()); }
        let dur = Duration::from_millis(self.timeout_ms);
        let mut ep_buf = [0u8; 48];
        let ep_len = fmt_endpoint(&self.addr, self.port, &mut ep_buf);
        let ep = core::str::from_utf8(&ep_buf[..ep_len]).unwrap_or("?:?");
        let stream = timeout(dur, TcpStream::connect(ep)).await
            .map_err(|_| InstrumentError::Timeout(format!("connect to {ep}")))?
            .map_err(|e| InstrumentError::ConnectionFailed(format!("{ep}: {e}")))?;
        self.stream = Some(stream);
        Ok(())
    }

    async fn write(&mut self, cmd: &str) -> Result<(), InstrumentError> {
        let stream = self.stream.as_mut().ok_or(InstrumentError::Closed)?;
        let dur = Duration::from_millis(self.timeout_ms);
        timeout(dur, async { stream.write_all(cmd.as_bytes()).await?; stream.write_all(b"\n").await })
            .await.map_err(|_| InstrumentError::Timeout(format!("write: {cmd}")))?
            .map_err(|e| InstrumentError::WriteFailed(format!("{cmd}: {e}")))
    }

    async fn query(&mut self, cmd: &str) -> Result<String, InstrumentError> {
        self.write(cmd).await?;
        let stream = self.stream.as_mut().ok_or(InstrumentError::Closed)?;
        self.pos = 0;
        let dur = Duration::from_millis(self.timeout_ms);
        timeout(dur, read_line(stream, &mut self.buf, &mut self.pos))
            .await.map_err(|_| InstrumentError::Timeout(format!("query read: {cmd}")))?
            .map_err(|e| InstrumentError::QueryFailed(format!("{cmd}: {e}")))?;
        Ok(String::from_utf8_lossy(&self.buf[..self.pos]).trim().to_string())
    }

    async fn close(&mut self) -> Result<(), InstrumentError> {
        if let Some(mut stream) = self.stream.take() { let _ = stream.shutdown().await; }
        Ok(())
    }
}

fn parse_resource(resource: &str) -> (String, u16) {
    let mut parts: [&str; 6] = [""; 6];
    let mut n = 0;
    let mut remaining = resource;
    while let Some(idx) = remaining.find("::") {
        if n < parts.len() { parts[n] = &remaining[..idx]; n += 1; }
        remaining = &remaining[idx + 2..];
    }
    if n < parts.len() { parts[n] = remaining; n += 1; }
    let addr = if n >= 2 { parts[1].to_string() } else { "127.0.0.1".into() };
    (addr, 5025)
}

pub fn create_instrument(kind: &str, resource: &str, timeout_ms: u64) -> Box<dyn SCPIInstrument> {
    Box::new(TcpInstrument::new(resource, kind, timeout_ms))
}

// YAML structure 
#[derive(Debug, Clone, Deserialize)]
struct ScopeLibrary {
    ip_config: Option<IpConfig>,
    #[serde(rename = "OSCILLOSCOPES")]
    scopes: HashMap<String, ScopeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct IpConfig { port: u16 }

#[derive(Debug, Clone, Deserialize)]
struct ScopeSpec {
    description: String,
    default_ip: String,
    idn_pattern: String,
    endian: String,
    quirks: String,
    cmds: HashMap<String, String>,
}

// Settings/Getting dispatch 

const SETTINGS: &[(&str, &str, &[&str])] = &[
    ("channel_on", "set_ch_on", &["ch"]),
    ("channel_off", "set_ch_off", &["ch"]),
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
    ("channel_check", "check_ch", &["ch"]),
    ("vertical_scale", "get_v_scale", &["ch"]),
    ("vertical_offset", "get_v_offset", &["ch"]),
    ("coupling", "get_coupling", &["ch"]),
    ("timebase", "get_h_scale", &[]),
    ("time_position", "get_h_pos", &[]),
    ("trigger_source", "get_trig_source", &["ch"]),
    ("trigger_level", "get_trig_level", &["ch"]),
    ("trigger_slope", "get_trig_slope", &["ch"]),
];

fn find_in<'a>(table: &'a [(&str, &str, &[&str])], name: &str) -> Option<(&'a str, &'a [&'a str])> {
    table.iter().find(|(n, _, _)| *n == name).map(|(_, c, p)| (*c, *p))
}

// Channel bitfield 
#[derive(Debug, Clone, Copy, Default)]
struct ChannelMask(u8);

impl ChannelMask {
    fn set(&mut self, ch: u8) { self.0 |= 1 << (ch - 1); }
    fn clear(&mut self, ch: u8) { self.0 &= !(1 << (ch - 1)); }
    fn has(self, ch: u8) -> bool { self.0 & (1 << (ch - 1)) != 0 }
    fn is_empty(self) -> bool { self.0 == 0 }
    fn iter(self) -> impl Iterator<Item = u8> { (1..=8).filter(move |ch| self.has(*ch)) }

    fn to_array(self) -> ([u8; 8], usize) {
        let mut arr = [0u8; 8];
        let mut n = 0;
        for ch in self.iter() { arr[n] = ch; n += 1; }
        (arr, n)
    }

    fn fmt_csv(self, buf: &mut [u8; 32]) -> usize {
        let mut pos = 0;
        let mut first = true;
        for ch in self.iter() {
            if !first { buf[pos] = b','; pos += 1; }
            buf[pos] = b'0' + ch; pos += 1;
            first = false;
        }
        pos
    }
}

impl IntoIterator for ChannelMask {
    type Item = u8;
    type IntoIter = ChannelMaskIter;
    fn into_iter(self) -> Self::IntoIter { ChannelMaskIter(self, 0) }
}

pub struct ChannelMaskIter(ChannelMask, u8);

impl Iterator for ChannelMaskIter {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        while self.1 < 8 {
            self.1 += 1;
            if self.0.has(self.1) { return Some(self.1); }
        }
        None
    }
}

// Oscilloscope 

pub struct Oscilloscope {
    brand: String,
    spec: ScopeSpec,
    stream: Option<TcpStream>,
    buf: [u8; 4096],
    pos: usize,
    timeout_ms: u64,
    default_port: u16,
    active: ChannelMask,
    instrument_id: String,
}

impl Oscilloscope {
    fn load_library() -> Result<ScopeLibrary, String> {
        let yaml = include_str!("../valicore/driver/oscilloscope.yaml");
        serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse: {e}"))
    }

    pub fn brands() -> Result<Vec<String>, ScopeError> {
        let mut b: Vec<_> = Self::load_library()
            .map_err(ScopeError::Config)?
            .scopes
            .into_keys()
            .collect();
        b.sort();
        Ok(b)
    }

    pub fn info(brand: &str) -> Result<String, String> {
        let spec = Self::load_library()?.scopes.get(brand)
            .ok_or_else(|| format!("unknown brand '{brand}'"))?.clone();
        Ok(format!("{brand} — {}", spec.description))
    }

    pub async fn detect_brand(addr: &str, port: u16, timeout_ms: u64) -> Result<String, ScopeError> {
        let mut ep_buf = [0u8; 48];
        let ep_len = fmt_endpoint(addr, port, &mut ep_buf);
        let ep = core::str::from_utf8(&ep_buf[..ep_len]).unwrap_or("?:?");

        let dur = Duration::from_millis(timeout_ms);
        let mut stream = timeout(dur, TcpStream::connect(ep)).await
            .map_err(|_| ScopeError::Connection(format!("timeout to {ep}")))?
            .map_err(|e| ScopeError::Connection(format!("{ep}: {e}")))?;

        let _ = timeout(dur, stream.write_all(b"*IDN?\n")).await;

        let mut idn_buf = [0u8; 256];
        let mut idn_pos = 0;
        let mut tmp = [0u8; 1];
        loop {
            match timeout(dur, stream.read(&mut tmp)).await {
                Ok(Ok(1)) => {
                    if idn_pos < idn_buf.len() { idn_buf[idn_pos] = tmp[0]; }
                    idn_pos += 1;
                    if tmp[0] == b'\n' { break; }
                }
                _ => break,
            }
        }
        let _ = stream.shutdown().await;
        let idn_upper = String::from_utf8_lossy(&idn_buf[..idn_pos]).trim().to_uppercase();

        let lib = Self::load_library().map_err(ScopeError::Config)?;
        lib.scopes.iter()
            .find(|(_, spec)| idn_upper.contains(&spec.idn_pattern.to_uppercase()))
            .map(|(brand, _)| brand.clone())
            .ok_or_else(|| ScopeError::Config(format!("no brand matches *IDN? response: {idn_upper}")))
    }

    pub async fn from_ip(addr: &str, port: Option<u16>, timeout_ms: u64) -> Result<Self, ScopeError> {
        let probe = port.unwrap_or(5025);
        let brand = Self::detect_brand(addr, probe, timeout_ms).await?;
        let mut scope = Self::new(&brand, timeout_ms)?;
        scope.connect(addr, port.unwrap_or(scope.default_port)).await?;
        Ok(scope)
    }

    pub fn new(brand: &str, timeout_ms: u64) -> Result<Self, ScopeError> {
        let lib = Self::load_library().map_err(ScopeError::Config)?;
        let spec = lib.scopes.get(brand).cloned().ok_or_else(|| {
            let mut avail: Vec<_> = lib.scopes.keys().cloned().collect();
            avail.sort();
            ScopeError::Config(format!("unknown brand '{brand}', available: {}", avail.join(", ")))
        })?;
        let port = lib.ip_config.as_ref().map(|c| c.port).unwrap_or(5025);
        Ok(Self { brand: brand.to_string(), spec, stream: None, buf: [0u8; 4096], pos: 0, timeout_ms, default_port: port, active: ChannelMask::default(), instrument_id: "OFFLINE".into() })
    }

    pub fn brand(&self) -> &str { &self.brand }
    pub fn default_port(&self) -> u16 { self.default_port }
    pub fn is_connected(&self) -> bool { self.stream.is_some() }

    pub fn active_channels(&self) -> Vec<u8> {
        let (arr, n) = self.active.to_array();
        arr[..n].to_vec()
    }

    pub fn instrument_id(&self) -> &str { &self.instrument_id }

    //Connection 

    pub async fn connect(&mut self, addr: &str, port: u16) -> Result<(), ScopeError> {
        let mut ep_buf = [0u8; 48];
        let ep_len = fmt_endpoint(addr, port, &mut ep_buf);
        let ep = core::str::from_utf8(&ep_buf[..ep_len]).unwrap_or("?:?");

        let dur = Duration::from_millis(self.timeout_ms);
        self.stream = Some(
            timeout(dur, TcpStream::connect(ep)).await
                .map_err(|_| ScopeError::Connection(format!("timeout to {ep}")))?
                .map_err(|e| ScopeError::Connection(format!("{ep}: {e}")))?
        );

        if !self.spec.quirks.is_empty() {
            let q = self.spec.quirks.clone();
            self.write_cmd(&q).await?;
        }

        self.instrument_id = self.query("*IDN?").await.unwrap_or_else(|_| "UNKNOWN".into());

        self.active = ChannelMask::default();
        for ch in 1..=4 {
            let cs = [b'0' + ch];
            if let Ok(ch_str) = core::str::from_utf8(&cs) {
                if let Ok(cmd) = self.cmd("check_ch", &[("ch", ch_str)]) {
                    if let Ok(resp) = self.query(&cmd).await {
                        let t = resp.trim();
                        if t == "1" || t.eq_ignore_ascii_case("ON") {
                            self.active.set(ch);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn close(&mut self) {
        if let Some(mut s) = self.stream.take() { let _ = s.shutdown().await; }
        self.active = ChannelMask::default();
    }

    // Low-level I/O (stack buffers) 

    async fn write_cmd(&mut self, cmd: &str) -> Result<(), ScopeError> {
        let s = self.stream.as_mut().ok_or_else(|| ScopeError::Connection("not connected".into()))?;
        timeout(Duration::from_millis(self.timeout_ms), async {
            s.write_all(cmd.as_bytes()).await?;
            s.write_all(b"\n").await
        })
        .await.map_err(|_| ScopeError::Io("write timeout".into()))?
        .map_err(|e| ScopeError::Io(format!("write: {e}")))
    }

    pub async fn write(&mut self, cmd: &str) -> Result<(), ScopeError> {
        self.write_cmd(cmd).await
    }

    pub async fn query(&mut self, cmd: &str) -> Result<String, ScopeError> {
        self.write_cmd(cmd).await?;
        let s = self.stream.as_mut().ok_or_else(|| ScopeError::Connection("not connected".into()))?;
        self.pos = 0;
        let dur = Duration::from_millis(self.timeout_ms);
        timeout(dur, read_line(s, &mut self.buf, &mut self.pos))
            .await.map_err(|_| ScopeError::Io("read timeout".into()))?
            .map_err(|e| ScopeError::Io(format!("read: {e}")))?;
        Ok(String::from_utf8_lossy(&self.buf[..self.pos]).trim().to_string())
    }

    pub async fn query_binary(&mut self, cmd: &str) -> Result<Vec<f64>, ScopeError> {
        self.write_cmd(cmd).await?;
        let s = self.stream.as_mut().ok_or_else(|| ScopeError::Connection("not connected".into()))?;
        let dur = Duration::from_millis(self.timeout_ms);
        let raw = timeout(dur, read_binary_block(s))
            .await.map_err(|_| ScopeError::Io("binary read timeout".into()))?
            .map_err(|e| ScopeError::Io(format!("binary: {e}")))?;

        let big = self.spec.endian.eq_ignore_ascii_case("big");
        Ok(raw.chunks_exact(4).map(|b| {
            let v = if big { f32::from_be_bytes([b[0], b[1], b[2], b[3]]) }
                   else { f32::from_le_bytes([b[0], b[1], b[2], b[3]]) };
            v as f64
        }).collect())
    }

    // Command dispatch 

    pub fn cmd(&self, name: &str, subs: &[(&str, &str)]) -> Result<String, ScopeError> {
        let tpl = self.spec.cmds.get(name)
            .ok_or_else(|| ScopeError::Config(format!("command '{name}' not found for '{}'", self.brand)))?;
        if subs.is_empty() { return Ok(tpl.clone()); }
        let est = tpl.len() + subs.iter().map(|(k, v)| v.len().saturating_sub(k.len() + 2)).sum::<usize>();
        let mut s = String::with_capacity(est.max(tpl.len()));
        s.push_str(tpl);
        for (k, v) in subs {
            let pat_start = s.find(&format!("{{{k}}}"));
            if let Some(start) = pat_start {
                let end = start + k.len() + 2;
                s.replace_range(start..end, v);
            }
        }
        Ok(s)
    }

    pub fn commands(&self) -> Vec<String> { let mut k: Vec<_> = self.spec.cmds.keys().cloned().collect(); k.sort(); k }


    pub async fn setting(&mut self, name: &str, kwargs: &[(&str, &str)]) -> Result<(), ScopeError> {
        let (ck, _) = find_in(SETTINGS, name).ok_or_else(|| ScopeError::Config(format!("unknown setting '{name}'")))?;
        let cmd = self.cmd(ck, kwargs)?;
        self.write_cmd(&cmd).await?;
        // Channel on/off side-effect: update the active bitfield
        match name {
            "channel_on" => {
                if let Some((_, v)) = kwargs.iter().find(|(k, _)| *k == "ch") {
                    if let Ok(ch) = v.parse::<u8>() { self.active.set(ch); }
                }
            }
            "channel_off" => {
                if let Some((_, v)) = kwargs.iter().find(|(k, _)| *k == "ch") {
                    if let Ok(ch) = v.parse::<u8>() { self.active.clear(ch); }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn getting(&mut self, name: &str, kwargs: &[(&str, &str)]) -> Result<String, ScopeError> {
        let (ck, _) = find_in(GETTINGS, name).ok_or_else(|| ScopeError::Config(format!("unknown getting '{name}'")))?;
        let cmd = self.cmd(ck, kwargs)?;
        let resp = self.query(&cmd).await?;
        match name {
            "channel_check" => {
                let t = resp.trim();
                Ok(if t == "1" || t.eq_ignore_ascii_case("ON") { "1".into() } else { "0".into() })
            }
            _ => Ok(resp),
        }
    }

    pub fn available_settings() -> Vec<&'static str> { SETTINGS.iter().map(|(n, _, _)| *n).collect() }
    pub fn available_gettings() -> Vec<&'static str> { GETTINGS.iter().map(|(n, _, _)| *n).collect() }

    // Actions
    pub async fn reset(&mut self) -> Result<(), ScopeError> { let c = self.cmd("reset", &[])?; self.write_cmd(&c).await }
    pub async fn autoset(&mut self) -> Result<(), ScopeError> { let c = self.cmd("autoset", &[])?; self.write_cmd(&c).await }
    pub async fn run(&mut self) -> Result<(), ScopeError> { let c = self.cmd("run", &[])?; self.write_cmd(&c).await }
    pub async fn stop(&mut self) -> Result<(), ScopeError> { let c = self.cmd("stop", &[])?; self.write_cmd(&c).await }
    pub async fn single(&mut self) -> Result<(), ScopeError> { let c = self.cmd("single", &[])?; self.write_cmd(&c).await }

    // Waveform acquisition
    pub async fn get_waveform(&mut self, channel: &str) -> Result<Vec<f64>, ScopeError> {
        if self.spec.cmds.contains_key("set_source") {
            let cmd = self.cmd("set_source", &[("ch", channel)])?;
            let _ = self.write_cmd(&cmd).await;
        }
        let _ = self.write_cmd(":WAVeform:FORMat ASCII").await;
        let raw_cmd = self.cmd("get_raw", &[("ch", channel)])?;
        let raw = self.query(&raw_cmd).await?;
        parse_waveform_csv(&raw)
    }

    pub async fn get_all_waveforms(&mut self) -> Result<WaveformResult, ScopeError> {
        if self.active.is_empty() { return Err(ScopeError::Acquisition("no active channels".into())); }

        let sr = self.query(":ACQuire:SRATe?").await?;
        let sample_rate: f64 = sr.trim().parse()
            .map_err(|e| ScopeError::Acquisition(format!("parse sample rate '{sr}': {e}")))?;

        let mut data_matrix = Vec::new();
        let mut failed = [0u8; 8];
        let mut n_failed = 0usize;

        for ch in self.active {
            let cs = [b'0' + ch];
            let ch_str = match core::str::from_utf8(&cs) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if self.spec.cmds.contains_key("set_source") {
                if let Ok(cmd) = self.cmd("set_source", &[("ch", ch_str)]) { let _ = self.write_cmd(&cmd).await; }
            }
            match self.cmd("get_raw", &[("ch", ch_str)]) {
                Ok(cmd) => match self.query_binary(&cmd).await {
                    Ok(d) if !d.is_empty() => data_matrix.push(d),
                    _ => { if n_failed < 8 { failed[n_failed] = ch; n_failed += 1; } }
                },
                Err(_) => { if n_failed < 8 { failed[n_failed] = ch; n_failed += 1; } }
            }
        }

        if data_matrix.is_empty() {
            return Err(ScopeError::Acquisition(format!("all channels failed: {failed:?}")));
        }

        let min_len = data_matrix.iter().map(|d| d.len()).min().unwrap_or(0);
        let data_matrix: Vec<Vec<f64>> = data_matrix.into_iter().map(|d| d[..min_len].to_vec()).collect();
        let time_axis: Vec<f64> = (0..min_len).map(|i| i as f64 / sample_rate).collect();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        let mut ch_buf = [0u8; 32];
        let ch_len = self.active.fmt_csv(&mut ch_buf);

        let mut fail_buf = [0u8; 32];
        let mut fail_len = 0;
        for i in 0..n_failed {
            if i > 0 { fail_buf[fail_len] = b','; fail_len += 1; }
            fail_buf[fail_len] = b'0' + failed[i]; fail_len += 1;
        }

        let mut meta = HashMap::new();
        meta.insert("sample_rate".into(), format!("{sample_rate:.0}"));
        meta.insert("channels".into(), String::from_utf8_lossy(&ch_buf[..ch_len]).into_owned());
        meta.insert("failed_channels".into(), String::from_utf8_lossy(&fail_buf[..fail_len]).into_owned());
        meta.insert("num_samples".into(), min_len.to_string());
        meta.insert("timestamp".into(), ts.to_string());
        meta.insert("instrument".into(), self.instrument_id.clone());

        Ok(WaveformResult { time_axis, data_matrix, metadata: meta })
    }
}

pub struct WaveformResult {
    pub time_axis: Vec<f64>,
    pub data_matrix: Vec<Vec<f64>>,
    pub metadata: HashMap<String, String>,
}

// Helpers 

fn parse_waveform_csv(raw: &str) -> Result<Vec<f64>, ScopeError> {
    let body = raw.trim();
    let start = body.find(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '+').unwrap_or(body.len());
    let data = &body[start..];
    if data.is_empty() { return Ok(Vec::new()); }
    data.split(',').map(|s| s.trim().parse::<f64>().map_err(|e| ScopeError::Acquisition(format!("parse '{s}': {e}")))).collect()
}

async fn read_binary_block(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).await.map_err(|e| format!("read header: {e}"))?;
        if byte[0] == b'#' { break; }
    }

    stream.read_exact(&mut byte).await.map_err(|e| format!("read digit count: {e}"))?;
    let dc = (byte[0] - b'0') as usize;
    if dc == 0 || dc > 12 { return Err(format!("invalid digit count: {dc}")); }

    let mut len_buf = [0u8; 12];
    stream.read_exact(&mut len_buf[..dc]).await.map_err(|e| format!("read length: {e}"))?;
    let data_len: usize = core::str::from_utf8(&len_buf[..dc])
        .map_err(|e| format!("parse length: {e}"))?
        .parse()
        .map_err(|e| format!("parse length: {e}"))?;

    let mut data = vec![0u8; data_len];
    let mut off = 0;
    while off < data_len {
        let n = stream.read(&mut data[off..]).await.map_err(|e| format!("read data: {e}"))?;
        if n == 0 { return Err("connection closed during binary read".into()); }
        off += n;
    }
    let _ = stream.read(&mut byte).await;
    Ok(data)
}

// Tests 
#[cfg(test)]
mod tests {
    use super::*;

    // Channel mask
    #[test]
    fn channel_mask_basics() {
        let mut m = ChannelMask::default();
        assert!(m.is_empty());
        m.set(1); m.set(3); m.set(5);
        assert!(m.has(1)); assert!(!m.has(2)); assert!(m.has(3));
        let (arr, n) = m.to_array();
        assert_eq!(&arr[..n], &[1, 3, 5]);
        m.clear(3);
        assert!(!m.has(3));
        let (arr, n) = m.to_array();
        assert_eq!(&arr[..n], &[1, 5]);
    }

    #[test]
    fn channel_mask_fmt_csv() {
        let mut m = ChannelMask::default();
        m.set(1); m.set(3);
        let mut buf = [0u8; 32];
        let len = m.fmt_csv(&mut buf);
        assert_eq!(&buf[..len], b"1,3");
    }

    #[test]
    fn channel_mask_all() {
        let mut m = ChannelMask::default();
        for ch in 1..=8 { m.set(ch); }
        let (arr, n) = m.to_array();
        assert_eq!(&arr[..n], &[1,2,3,4,5,6,7,8]);
        m.clear(4);
        let (arr, n) = m.to_array();
        assert_eq!(&arr[..n], &[1,2,3,5,6,7,8]);
    }

    // Endpoint formatting
    #[test]
    fn fmt_endpoint_basic() {
        let mut buf = [0u8; 48];
        let len = fmt_endpoint("192.168.1.10", 5025, &mut buf);
        assert_eq!(core::str::from_utf8(&buf[..len]).unwrap(), "192.168.1.10:5025");
    }

    // Scope library
    #[test]
    fn brands_load() {
        let b = Oscilloscope::brands().unwrap();
        assert!(b.len() > 0);
        assert!(b.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn unknown_brand_error() {
        assert!(Oscilloscope::new("FAKE_BRAND", 5000).is_err());
    }

    // Waveform CSV parsing
    #[test]
    fn parse_waveform_csv_ok() {
        assert_eq!(parse_waveform_csv("1.0,2.0,3.0").unwrap(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn parse_waveform_csv_empty() {
        assert!(parse_waveform_csv("").unwrap().is_empty());
    }

    // Command dispatch
    #[test]
    fn cmd_format() {
        let scope = Oscilloscope::new("RS", 5000).unwrap();
        assert_eq!(scope.cmd("set_v_scale", &[("ch", "1"), ("val", "0.5")]).unwrap(), "CHANnel1:SCALe 0.5");
    }

    #[test]
    fn cmd_no_alloc() {
        let scope = Oscilloscope::new("RS", 5000).unwrap();
        assert_eq!(scope.cmd("reset", &[]).unwrap(), "*RST");
    }

    #[test]
    fn cmd_missing() {
        assert!(Oscilloscope::new("RS", 5000).unwrap().cmd("nonexistent", &[]).is_err());
    }

    #[test]
    fn available_settings_not_empty() {
        assert!(!Oscilloscope::available_settings().is_empty());
        assert!(!Oscilloscope::available_gettings().is_empty());
    }

    // TcpInstrument / parse_resource
    #[test]
    fn parse_resource_standard() {
        let (addr, port) = parse_resource("TCPIP0::192.168.1.10::inst0::INSTR");
        assert_eq!(addr, "192.168.1.10");
        assert_eq!(port, 5025);
    }

    #[test]
    fn parse_resource_missing() {
        let (addr, port) = parse_resource("garbage");
        assert_eq!(addr, "127.0.0.1");
        assert_eq!(port, 5025);
    }

    #[test]
    fn create_instrument_kind() {
        let instr = create_instrument("dmm", "TCPIP0::1.2.3.4::inst0::INSTR", 5000);
        assert_eq!(instr.kind(), "dmm");
        assert_eq!(instr.resource(), "TCPIP0::1.2.3.4::inst0::INSTR");
    }

    #[test]
    fn tcp_instrument_initial_state() {
        let instr = TcpInstrument::new("TCPIP0::10.0.0.1::INSTR", "scope", 3000);
        assert_eq!(instr.kind(), "scope");
        assert!(instr.stream.is_none());
    }
}
