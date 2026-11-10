//! Owned loopback observer for trusted inert pages; no product IPC or authority.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tao::event_loop::EventLoopProxy;

use super::{WkUserEvent, WvError, failure};

pub(super) struct UaObservation {
    pub(super) deadline: Instant,
    root: String,
    incoming: mpsc::Receiver<Result<Value, String>>,
    rows: BTreeMap<String, Value>,
    server: Server,
}
impl UaObservation {
    pub(super) fn new(wake: EventLoopProxy<WkUserEvent>) -> Result<Self, WvError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| failure(error.to_string()))?;
        let address = listener
            .local_addr()
            .map_err(|error| failure(error.to_string()))?;
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|error| failure(error.to_string()))?;
        let prefix = nonce
            .iter()
            .fold(String::with_capacity(32), |mut text, byte| {
                let _ = write!(&mut text, "{byte:02x}");
                text
            });
        let root = format!("http://{address}/{prefix}");
        let (outgoing, incoming) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name(String::from("keld-ua-fixture-observer"))
            .spawn(move || {
                for _ in 0..2 {
                    let result = receive(&listener, &prefix);
                    let terminal = result.is_err();
                    if outgoing.send(result).is_err() {
                        break;
                    }
                    let _ = wake.send_event(WkUserEvent::ApplicationNameFixtureWake);
                    if terminal {
                        break;
                    }
                }
            })
            .map_err(|error| failure(error.to_string()))?;
        Ok(Self {
            deadline: Instant::now() + Duration::from_secs(20),
            root,
            incoming,
            rows: BTreeMap::new(),
            server: Server {
                address,
                worker: Some(worker),
            },
        })
    }
    pub(super) fn page(&self, path: &str) -> String {
        // Fixed first-party observation code. It reads navigator data; it does
        // not replace any page global or compute an expected UA from metadata.
        format!(
            "<!doctype html><title>First-party UA fixture</title><p>Inert {path} page</p><script>const probe=new Image();probe.src='{}/{path}?ua='+Array.from(navigator.userAgent,c=>c.charCodeAt(0).toString(16).padStart(2,'0')).join('');</script>",
            self.root
        )
    }
    pub(super) fn observe(&mut self) -> Result<(), WvError> {
        for row in self.incoming.try_iter() {
            let row = row.map_err(failure)?;
            let path = row
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| failure("observer path absent"))?
                .to_owned();
            if self.rows.insert(path, row).is_some() {
                return Err(failure("duplicate native observation path"));
            }
        }
        Ok(())
    }
    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
    pub(super) fn finish(mut self) -> Result<Vec<Value>, WvError> {
        self.server.finish()?;
        if self.rows.len() != 2
            || !self.rows.contains_key("ordinary")
            || !self.rows.contains_key("managed")
        {
            return Err(failure("both actual native paths are required"));
        }
        Ok(std::mem::take(&mut self.rows).into_values().collect())
    }
}

struct Server {
    address: SocketAddr,
    worker: Option<JoinHandle<()>>,
}
impl Server {
    fn finish(&mut self) -> Result<(), WvError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        // Wake a blocked accept on failure/timeout. This is fixture-owned
        // cancellation, never a product cleanup or native quiescence receipt.
        if !worker.is_finished() {
            let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(1));
        }
        worker
            .join()
            .map_err(|_| failure("owned UA observer thread panicked"))
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn receive(listener: &TcpListener, prefix: &str) -> Result<Value, String> {
    let (mut stream, peer) = listener.accept().map_err(|error| error.to_string())?;
    if !peer.ip().is_loopback() {
        return Err(String::from("non-loopback observer peer"));
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 12 * 1024 {
            return Err(String::from("observer request exceeds12KiB"));
        }
        let mut byte = [0_u8; 1];
        if stream.read(&mut byte).map_err(|error| error.to_string())? != 1 {
            return Err(String::from("incomplete observer request"));
        }
        bytes.push(byte[0]);
    }
    let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    let target = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET "))
        .and_then(|line| line.strip_suffix(" HTTP/1.1"))
        .ok_or("unexpected observer request line")?;
    let target = target
        .strip_prefix(&format!("/{prefix}/"))
        .ok_or("observer nonce mismatch")?;
    let (path, hex) = target.split_once("?ua=").ok_or("UA observation absent")?;
    if !matches!(path, "ordinary" | "managed")
        || hex.is_empty()
        || hex.len() > 8192
        || !hex.len().is_multiple_of(2)
    {
        return Err(String::from("malformed native UA observation"));
    }
    let navigator = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            std::str::from_utf8(pair)
                .map_err(|error| error.to_string())
                .and_then(|pair| u8::from_str_radix(pair, 16).map_err(|error| error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let navigator = String::from_utf8(navigator).map_err(|error| error.to_string())?;
    let header = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            name.eq_ignore_ascii_case("User-Agent")
                .then(|| value.trim())
        })
        .ok_or("native HTTP User-Agent absent")?;
    if navigator.is_empty() || navigator.len() > 4096 || navigator != header {
        return Err(String::from("actual HTTP and navigator UA disagree"));
    }
    stream
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .map_err(|error| error.to_string())?;
    Ok(json!({"path":path,"navigator_user_agent":navigator,"http_user_agent":header}))
}
