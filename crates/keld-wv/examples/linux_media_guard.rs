//! Real Linux `WebKitGTK` media-permission probe for KEL-132.
//!
//! The example is test evidence, not a shipping binary. It serves one secure
//! localhost page, requests one mock capture kind, and exits only after the
//! page reports the observed result. The companion `LD_PRELOAD` fixture records
//! whether wry consumed Keld's callback through the `WebKitGTK` deny API.

#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::io::{ErrorKind, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use keld_wv::webkitgtk::{WebKitGtkEngine, prepare_gpu_safe_mode_process};
    use keld_wv::{
        AppWindowCommand, AppWindowEvent, LogicalSize, NavTarget, WebEngine, WebviewSpec,
    };

    const SERVER_DEADLINE: Duration = Duration::from_secs(20);
    const STREAM_DEADLINE: Duration = Duration::from_secs(2);
    const MAX_REQUEST_BYTES: usize = 16 * 1024;

    #[derive(Clone, Copy)]
    enum MediaKind {
        Camera,
        Microphone,
    }

    impl MediaKind {
        fn parse(value: &str) -> Result<Self, String> {
            match value {
                "camera" => Ok(Self::Camera),
                "microphone" => Ok(Self::Microphone),
                _ => Err(format!(
                    "unknown media kind `{value}`; use `camera` or `microphone`"
                )),
            }
        }

        const fn name(self) -> &'static str {
            match self {
                Self::Camera => "camera",
                Self::Microphone => "microphone",
            }
        }

        const fn constraints(self) -> &'static str {
            match self {
                Self::Camera => "{ audio: false, video: true }",
                Self::Microphone => "{ audio: true, video: false }",
            }
        }
    }

    #[derive(Clone, Copy)]
    enum ExpectedOutcome {
        Denied,
        Allowed,
    }

    impl ExpectedOutcome {
        fn parse(value: &str) -> Result<Self, String> {
            match value {
                "denied" => Ok(Self::Denied),
                "allowed" => Ok(Self::Allowed),
                _ => Err(format!(
                    "unknown expected outcome `{value}`; use `denied` or `allowed`"
                )),
            }
        }

        fn matches(self, outcome: &str) -> bool {
            match self {
                Self::Denied => matches!(outcome, "NotAllowedError" | "SecurityError"),
                Self::Allowed => matches!(outcome, "resolved"),
            }
        }
    }

    struct ProbeResult {
        secure_context: bool,
        outcome: String,
    }

    pub fn run() -> Result<(), String> {
        let mut args = env::args().skip(1);
        let kind = MediaKind::parse(
            &args
                .next()
                .ok_or_else(|| String::from("missing media kind"))?,
        )?;
        let expected = ExpectedOutcome::parse(
            &args
                .next()
                .ok_or_else(|| String::from("missing expected outcome"))?,
        )?;
        let nonce = args
            .next()
            .ok_or_else(|| String::from("missing run nonce"))?;
        if nonce.is_empty()
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(String::from(
                "run nonce must contain only ASCII letters, digits, and hyphens",
            ));
        }
        if env::var("KELD_MEDIA_NONCE").as_deref() != Ok(nonce.as_str()) {
            return Err(String::from(
                "KELD_MEDIA_NONCE must exactly match the run nonce argument",
            ));
        }
        if let Some(extra) = args.next() {
            return Err(format!("unexpected argument `{extra}`"));
        }

        let _ = prepare_gpu_safe_mode_process().map_err(|error| error.to_string())?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let (commands_tx, commands_rx) = mpsc::channel();
        let server_nonce = nonce.clone();
        let server = thread::Builder::new()
            .name(String::from("keld-media-probe-http"))
            .spawn(move || serve(&listener, kind, &server_nonce, &commands_tx))
            .map_err(|error| error.to_string())?;

        let mut engine = WebKitGtkEngine::new().map_err(|error| error.to_string())?;
        let (events_tx, _events_rx) = mpsc::channel::<AppWindowEvent>();
        let primer = WebviewSpec {
            title: format!("Keld Media Guard Primer {nonce}"),
            initial: NavTarget::Html(String::from(
                "<!doctype html><meta charset=utf-8><title>identity primer</title>",
            )),
            size: LogicalSize {
                width: 320.0,
                height: 240.0,
            },
        };
        let primer_id = engine.create(&primer).map_err(|error| error.to_string())?;
        engine
            .destroy(primer_id)
            .map_err(|error| error.to_string())?;
        let spec = WebviewSpec {
            title: format!("Keld Media Guard {} {nonce}", kind.name()),
            initial: NavTarget::Url(format!("http://{address}/{nonce}/")),
            size: LogicalSize {
                width: 640.0,
                height: 480.0,
            },
        };
        let media_id = engine
            .create_app(&spec, events_tx.clone())
            .map_err(|error| error.to_string())?;
        publish_media_id(media_id)?;
        engine
            .run_app_until_quit(commands_rx, events_tx)
            .map_err(|error| error.to_string())?;

        let result = server
            .join()
            .map_err(|_| String::from("media probe server thread panicked"))??;
        if !result.secure_context {
            return Err(String::from(
                "localhost page was not a secure context; media result is not a permission oracle",
            ));
        }
        if !expected.matches(&result.outcome) {
            return Err(format!(
                "{} expected a different result, observed `{}`",
                kind.name(),
                result.outcome
            ));
        }
        println!(
            "KELD_MEDIA_RESULT nonce={nonce} kind={} secure_context=true outcome={}",
            kind.name(),
            result.outcome
        );
        Ok(())
    }

    fn publish_media_id(id: keld_wv::WebviewId) -> Result<(), String> {
        let path = env::var_os("KELD_MEDIA_IDENTITY_RECEIPT")
            .ok_or_else(|| String::from("KELD_MEDIA_IDENTITY_RECEIPT is unset"))?;
        let mut receipt = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot publish media webview id: {error}"))?;
        writeln!(receipt, "{}", id.0).map_err(|error| error.to_string())
    }

    fn serve(
        listener: &TcpListener,
        kind: MediaKind,
        nonce: &str,
        commands: &mpsc::Sender<AppWindowCommand>,
    ) -> Result<ProbeResult, String> {
        let result = serve_until_result(listener, kind, nonce, commands);
        if result.is_err() {
            let _ = commands.send(AppWindowCommand::Fatal);
        }
        result
    }

    fn serve_until_result(
        listener: &TcpListener,
        kind: MediaKind,
        nonce: &str,
        commands: &mpsc::Sender<AppWindowCommand>,
    ) -> Result<ProbeResult, String> {
        let deadline = Instant::now() + SERVER_DEADLINE;
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let path = request_path(&mut stream)?;
                    if path == format!("/{nonce}/") {
                        respond_html(&mut stream, kind, nonce)?;
                    } else if path == format!("/{nonce}/ready") {
                        await_request_start()?;
                        respond(&mut stream, "204 No Content", "text/plain", b"")?;
                    } else if let Some(query) = path.strip_prefix(&format!("/{nonce}/result?")) {
                        let result = parse_result(query)?;
                        respond(&mut stream, "204 No Content", "text/plain", b"")?;
                        await_census()?;
                        commands
                            .send(AppWindowCommand::Quit)
                            .map_err(|_| String::from("window command receiver closed"))?;
                        return Ok(result);
                    } else {
                        respond(&mut stream, "404 Not Found", "text/plain", b"not found")?;
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => thread::yield_now(),
                Err(error) => return Err(error.to_string()),
            }
        }
        Err(String::from(
            "media page produced no result before the server deadline",
        ))
    }

    fn await_census() -> Result<(), String> {
        let ready = env::var_os("KELD_MEDIA_READY")
            .ok_or_else(|| String::from("KELD_MEDIA_READY is unset"))?;
        let release = env::var_os("KELD_MEDIA_RELEASE")
            .ok_or_else(|| String::from("KELD_MEDIA_RELEASE is unset"))?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&ready)
            .map_err(|error| format!("cannot publish census readiness: {error}"))?;
        let deadline = Instant::now() + SERVER_DEADLINE;
        while Instant::now() < deadline {
            if std::path::Path::new(&release).is_file() {
                return Ok(());
            }
            thread::yield_now();
        }
        Err(String::from(
            "window census did not release the media probe before its deadline",
        ))
    }

    fn await_request_start() -> Result<(), String> {
        let ready = env::var_os("KELD_MEDIA_PAGE_READY")
            .ok_or_else(|| String::from("KELD_MEDIA_PAGE_READY is unset"))?;
        let release = env::var_os("KELD_MEDIA_REQUEST_RELEASE")
            .ok_or_else(|| String::from("KELD_MEDIA_REQUEST_RELEASE is unset"))?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&ready)
            .map_err(|error| format!("cannot publish page readiness: {error}"))?;
        let deadline = Instant::now() + SERVER_DEADLINE;
        while Instant::now() < deadline {
            if std::path::Path::new(&release).is_file() {
                return Ok(());
            }
            thread::yield_now();
        }
        Err(String::from(
            "window census did not release the media request before its deadline",
        ))
    }

    fn request_path(stream: &mut TcpStream) -> Result<String, String> {
        stream
            .set_read_timeout(Some(STREAM_DEADLINE))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        while bytes.len() < MAX_REQUEST_BYTES {
            let read = stream
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        if bytes.len() >= MAX_REQUEST_BYTES {
            return Err(String::from("HTTP request exceeded 16 KiB"));
        }
        let request = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
        let line = request
            .lines()
            .next()
            .ok_or_else(|| String::from("HTTP request has no request line"))?;
        let mut fields = line.split_ascii_whitespace();
        if fields.next() != Some("GET") {
            return Err(String::from("media probe accepts only GET"));
        }
        fields
            .next()
            .map(str::to_owned)
            .ok_or_else(|| String::from("HTTP request has no path"))
    }

    fn respond_html(stream: &mut TcpStream, kind: MediaKind, nonce: &str) -> Result<(), String> {
        let html = format!(
            r#"<!doctype html><meta charset="utf-8"><title>Keld media probe</title>
<script>
const constraints = {};
const report = outcome => fetch(`/{nonce}/result?secure=${{String(window.isSecureContext)}}&outcome=${{encodeURIComponent(outcome)}}`);
(async () => {{
  await fetch(`/{nonce}/ready`);
  try {{
    const stream = await navigator.mediaDevices.getUserMedia(constraints);
    for (const track of stream.getTracks()) track.stop();
    await report("resolved");
  }} catch (error) {{
    await report(error && error.name ? error.name : "UnknownError");
  }}
}})();
</script>"#,
            kind.constraints()
        );
        respond(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            html.as_bytes(),
        )
    }

    fn parse_result(query: &str) -> Result<ProbeResult, String> {
        let mut secure = None;
        let mut outcome = None;
        for field in query.split('&') {
            if let Some(value) = field.strip_prefix("secure=") {
                secure = Some(value == "true");
            } else if let Some(value) = field.strip_prefix("outcome=") {
                outcome = Some(value.to_owned());
            }
        }
        Ok(ProbeResult {
            secure_context: secure.ok_or_else(|| String::from("result omitted secure state"))?,
            outcome: outcome.ok_or_else(|| String::from("result omitted outcome"))?,
        })
    }

    fn respond(
        stream: &mut TcpStream,
        status: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<(), String> {
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .map_err(|error| error.to_string())?;
        stream.write_all(body).map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = linux::run() {
        eprintln!("KELD_MEDIA_PROBE_FAIL: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux_media_guard is available only on Linux");
    std::process::exit(1);
}
