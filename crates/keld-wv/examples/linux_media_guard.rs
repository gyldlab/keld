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
        AppWindowCommand, AppWindowEvent, LogicalSize, NavTarget, WebEngine, WebviewId,
        WebviewSpec, WvError,
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

        const fn track_kind(self) -> &'static str {
            match self {
                Self::Camera => "video",
                Self::Microphone => "audio",
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

        fn matches(self, kind: MediaKind, result: &ProbeResult) -> bool {
            match self {
                Self::Denied => {
                    matches!(result.outcome.as_str(), "NotAllowedError" | "SecurityError")
                        && result.track_kind == "none"
                        && result.track_count == 0
                        && !result.live_before_stop
                        && !result.ended_after_stop
                }
                Self::Allowed => {
                    result.outcome == "resolved"
                        && result.track_kind == kind.track_kind()
                        && result.track_count > 0
                        && result.live_before_stop
                        && result.ended_after_stop
                }
            }
        }
    }

    struct ProbeResult {
        secure_context: bool,
        outcome: String,
        track_kind: String,
        track_count: usize,
        live_before_stop: bool,
        ended_after_stop: bool,
    }

    struct ProbeArgs {
        kind: MediaKind,
        expected: ExpectedOutcome,
        nonce: String,
        primer_count: usize,
    }

    pub fn run() -> Result<(), String> {
        let ProbeArgs {
            kind,
            expected,
            nonce,
            primer_count,
        } = parse_args()?;

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
        let primer_ids = destroy_primers(&mut engine, kind, &nonce, primer_count)?;
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
        if primer_ids.contains(&media_id)
            || primer_ids
                .last()
                .is_some_and(|primer| media_id.0 <= primer.0)
        {
            return Err(String::from(
                "fresh media view reused or preceded a destroyed primer id",
            ));
        }
        publish_media_identity(kind, &nonce, &primer_ids, media_id)?;
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
        if !expected.matches(kind, &result) {
            return Err(format!(
                "{} expected a different result, observed outcome={} track_kind={} track_count={} live_before_stop={} ended_after_stop={}",
                kind.name(),
                result.outcome,
                result.track_kind,
                result.track_count,
                result.live_before_stop,
                result.ended_after_stop
            ));
        }
        println!(
            "KELD_MEDIA_RESULT nonce={nonce} kind={} secure_context=true outcome={} track_kind={} track_count={} live_before_stop={} ended_after_stop={}",
            kind.name(),
            result.outcome,
            result.track_kind,
            result.track_count,
            result.live_before_stop,
            result.ended_after_stop
        );
        Ok(())
    }

    fn parse_args() -> Result<ProbeArgs, String> {
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
        let primer_count_text = args
            .next()
            .ok_or_else(|| String::from("missing primer count"))?;
        let primer_count = parse_primer_count(&primer_count_text)?;
        if let Some(extra) = args.next() {
            return Err(format!("unexpected argument `{extra}`"));
        }
        Ok(ProbeArgs {
            kind,
            expected,
            nonce,
            primer_count,
        })
    }

    fn parse_primer_count(value: &str) -> Result<usize, String> {
        let count = value
            .parse::<usize>()
            .map_err(|_| String::from("primer count must be a canonical positive integer"))?;
        if count == 0 || count > 16 || value != count.to_string() {
            return Err(String::from(
                "primer count must be a canonical integer in 1..=16",
            ));
        }
        Ok(count)
    }

    fn destroy_primers(
        engine: &mut WebKitGtkEngine,
        kind: MediaKind,
        nonce: &str,
        count: usize,
    ) -> Result<Vec<WebviewId>, String> {
        let mut ids = Vec::with_capacity(count);
        for index in 0..count {
            let primer = WebviewSpec {
                title: format!("Keld Media Guard Primer {} {nonce}", index + 1),
                initial: NavTarget::Html(String::from(
                    "<!doctype html><meta charset=utf-8><title>identity primer</title>",
                )),
                size: LogicalSize {
                    width: 320.0,
                    height: 240.0,
                },
            };
            let id = engine.create(&primer).map_err(|error| error.to_string())?;
            if ids
                .last()
                .is_some_and(|previous: &WebviewId| id.0 <= previous.0)
            {
                return Err(String::from("primer ids were not strictly increasing"));
            }
            engine.destroy(id).map_err(|error| error.to_string())?;
            let error = match engine.navigate(id, NavTarget::Html(String::new())) {
                Ok(()) => {
                    return Err(format!(
                        "destroyed primer {} still accepted navigation",
                        id.0
                    ));
                }
                Err(error) => error,
            };
            let message = error.to_string();
            if !matches!(error, WvError::UnknownWebview { id: stale } if stale == id.0)
                || !message.starts_with("KELD-WV-007:")
            {
                return Err(format!(
                    "destroyed primer {} returned the wrong error: {message}",
                    id.0
                ));
            }
            println!(
                "KELD_MEDIA_STALE nonce={nonce} kind={} ordinal={} primer_id={} code=KELD-WV-007",
                kind.name(),
                index + 1,
                id.0
            );
            ids.push(id);
        }
        Ok(ids)
    }

    fn publish_media_identity(
        kind: MediaKind,
        nonce: &str,
        primer_ids: &[WebviewId],
        media_id: WebviewId,
    ) -> Result<(), String> {
        let path = env::var_os("KELD_MEDIA_IDENTITY_RECEIPT")
            .ok_or_else(|| String::from("KELD_MEDIA_IDENTITY_RECEIPT is unset"))?;
        let mut receipt = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot publish media webview id: {error}"))?;
        let first = primer_ids
            .first()
            .ok_or_else(|| String::from("at least one primer id is required"))?;
        let last = primer_ids
            .last()
            .ok_or_else(|| String::from("at least one primer id is required"))?;
        writeln!(
            receipt,
            "KELD_MEDIA_IDENTITY nonce={nonce} kind={} primer_count={} primer_first={} primer_last={} stale_count={} stale_code=KELD-WV-007 media_id={} fresh=true",
            kind.name(),
            primer_ids.len(),
            first.0,
            last.0,
            primer_ids.len(),
            media_id.0
        )
        .map_err(|error| error.to_string())
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
const report = (outcome, trackKind = "none", trackCount = 0, liveBeforeStop = false, endedAfterStop = false) => fetch(`/{nonce}/result?secure=${{String(window.isSecureContext)}}&outcome=${{encodeURIComponent(outcome)}}&track_kind=${{trackKind}}&track_count=${{trackCount}}&live_before_stop=${{liveBeforeStop}}&ended_after_stop=${{endedAfterStop}}`);
(async () => {{
  await fetch(`/{nonce}/ready`);
  try {{
    const stream = await navigator.mediaDevices.getUserMedia(constraints);
    const tracks = stream.getTracks().filter(track => track.kind === "{}");
    const liveBeforeStop = tracks.length > 0 && tracks.every(track => track.readyState === "live");
    for (const track of tracks) track.stop();
    const endedAfterStop = tracks.length > 0 && tracks.every(track => track.readyState === "ended");
    await report("resolved", "{}", tracks.length, liveBeforeStop, endedAfterStop);
  }} catch (error) {{
    await report(error && error.name ? error.name : "UnknownError");
  }}
}})();
</script>"#,
            kind.constraints(),
            kind.track_kind(),
            kind.track_kind()
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
        let mut track_kind = None;
        let mut track_count = None;
        let mut live_before_stop = None;
        let mut ended_after_stop = None;
        for field in query.split('&') {
            if let Some(value) = field.strip_prefix("secure=") {
                secure = Some(value == "true");
            } else if let Some(value) = field.strip_prefix("outcome=") {
                outcome = Some(value.to_owned());
            } else if let Some(value) = field.strip_prefix("track_kind=") {
                track_kind = Some(value.to_owned());
            } else if let Some(value) = field.strip_prefix("track_count=") {
                track_count = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| String::from("result track_count was not an integer"))?,
                );
            } else if let Some(value) = field.strip_prefix("live_before_stop=") {
                live_before_stop = Some(parse_bool(value, "live_before_stop")?);
            } else if let Some(value) = field.strip_prefix("ended_after_stop=") {
                ended_after_stop = Some(parse_bool(value, "ended_after_stop")?);
            }
        }
        Ok(ProbeResult {
            secure_context: secure.ok_or_else(|| String::from("result omitted secure state"))?,
            outcome: outcome.ok_or_else(|| String::from("result omitted outcome"))?,
            track_kind: track_kind
                .ok_or_else(|| String::from("result omitted requested track kind"))?,
            track_count: track_count
                .ok_or_else(|| String::from("result omitted requested track count"))?,
            live_before_stop: live_before_stop
                .ok_or_else(|| String::from("result omitted live-before-stop state"))?,
            ended_after_stop: ended_after_stop
                .ok_or_else(|| String::from("result omitted ended-after-stop state"))?,
        })
    }

    fn parse_bool(value: &str, field: &str) -> Result<bool, String> {
        match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("result {field} was not an exact Boolean")),
        }
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

    #[cfg(test)]
    mod tests {
        use super::{ExpectedOutcome, MediaKind, ProbeResult, parse_bool, parse_primer_count};

        fn allowed_result(kind: &str, count: usize, live: bool, ended: bool) -> ProbeResult {
            ProbeResult {
                secure_context: true,
                outcome: String::from("resolved"),
                track_kind: kind.to_owned(),
                track_count: count,
                live_before_stop: live,
                ended_after_stop: ended,
            }
        }

        #[test]
        fn primer_count_is_canonical_positive_and_bounded() {
            assert_eq!(parse_primer_count("1").expect("one primer"), 1);
            assert_eq!(parse_primer_count("16").expect("maximum primers"), 16);
            for invalid in ["", "0", "01", "17", "-1", "one"] {
                assert!(parse_primer_count(invalid).is_err(), "{invalid}");
            }
        }

        #[test]
        fn allow_requires_requested_live_tracks_that_end_after_stop() {
            assert!(
                ExpectedOutcome::Allowed
                    .matches(MediaKind::Camera, &allowed_result("video", 1, true, true))
            );
            for result in [
                allowed_result("audio", 1, true, true),
                allowed_result("video", 0, true, true),
                allowed_result("video", 1, false, true),
                allowed_result("video", 1, true, false),
            ] {
                assert!(!ExpectedOutcome::Allowed.matches(MediaKind::Camera, &result));
            }
        }

        #[test]
        fn result_booleans_are_exact() {
            assert!(parse_bool("true", "field").expect("true"));
            assert!(!parse_bool("false", "field").expect("false"));
            for invalid in ["True", "0", "garbage"] {
                assert!(parse_bool(invalid, "field").is_err(), "{invalid}");
            }
        }
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
