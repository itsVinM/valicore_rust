use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

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
    buf: Vec<u8>,
}

impl TcpInstrument {
    pub fn new(resource: &str, kind: &str, timeout_ms: u64) -> Self {
        let (addr, port) = parse_resource(resource);
        Self {
            resource: resource.to_string(),
            kind: kind.to_string(),
            addr,
            port,
            stream: None,
            timeout_ms,
            buf: Vec::with_capacity(4096),
        }
    }
}

#[async_trait]
impl SCPIInstrument for TcpInstrument {
    fn resource(&self) -> &str {
        &self.resource
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    async fn connect(&mut self) -> Result<(), InstrumentError> {
        if self.stream.is_some() {
            return Ok(());
        }
        let addr = format!("{}:{}", self.addr, self.port);
        let dur = Duration::from_millis(self.timeout_ms);
        let stream = timeout(dur, TcpStream::connect(&addr))
            .await
            .map_err(|_| InstrumentError::Timeout(format!("connect to {}", addr)))?
            .map_err(|e| InstrumentError::ConnectionFailed(format!("{}: {}", addr, e)))?;
        self.stream = Some(stream);
        Ok(())
    }

    async fn write(&mut self, cmd: &str) -> Result<(), InstrumentError> {
        let stream = self.stream.as_mut().ok_or(InstrumentError::Closed)?;
        let line = format!("{}\n", cmd);
        timeout(Duration::from_millis(self.timeout_ms), stream.write_all(line.as_bytes()))
            .await
            .map_err(|_| InstrumentError::Timeout(format!("write: {}", cmd)))?
            .map_err(|e| InstrumentError::WriteFailed(format!("{}: {}", cmd, e)))
    }

    async fn query(&mut self, cmd: &str) -> Result<String, InstrumentError> {
        self.write(cmd).await?;

        let stream = self.stream.as_mut().ok_or(InstrumentError::Closed)?;
        self.buf.clear();
        let dur = Duration::from_millis(self.timeout_ms);

        timeout(dur, read_line(stream, &mut self.buf))
            .await
            .map_err(|_| InstrumentError::Timeout(format!("query read: {}", cmd)))?
            .map_err(|e| InstrumentError::QueryFailed(format!("{}: {}", cmd, e)))?;

        let resp = String::from_utf8_lossy(&self.buf).trim().to_string();
        Ok(resp)
    }

    async fn close(&mut self) -> Result<(), InstrumentError> {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }
}

async fn read_line(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Result<usize, std::io::Error> {
    use tokio::io::AsyncReadExt;
    let mut tmp = [0u8; 1];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "connection closed"));
        }
        buf.push(tmp[0]);
        if tmp[0] == b'\n' {
            return Ok(buf.len());
        }
    }
}

fn parse_resource(resource: &str) -> (String, u16) {
    let parts: Vec<&str> = resource.split("::").collect();
    let addr = parts.get(1).unwrap_or(&"127.0.0.1").to_string();
    (addr, 5025)
}

pub fn create_instrument(kind: &str, resource: &str, timeout_ms: u64) -> Box<dyn SCPIInstrument> {
    Box::new(TcpInstrument::new(resource, kind, timeout_ms))
}
