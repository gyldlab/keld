//! One-use, test-owned real TCP witness for Bun's completed HTTP /quit body read.
//!
//! The listener is armed before Host spawn. The parent polls it only while the
//! real Host Child is live; it is not a Host stderr marker, protocol reply or
//! runtime/process authority.
use std::fs::File;
use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

pub(super) struct QuitBodyReceipt {
    listener: TcpListener,
    token: String,
    pending: Option<(TcpStream, Vec<u8>)>,
    observed: bool,
    invalid: bool,
}

impl QuitBodyReceipt {
    pub(super) fn arm() -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("test-owned Quit body receipt listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking receipt accept");
        // Never print or otherwise publish this one-use test-only capability.
        let mut random = [0_u8; 16];
        File::open("/dev/urandom")
            .expect("macOS native test randomness")
            .read_exact(&mut random)
            .expect("16 bytes of independent native test nonce");
        let mut token = String::with_capacity(32);
        for byte in random {
            use std::fmt::Write as _;
            write!(&mut token, "{byte:02x}").expect("append fixed native nonce bytes");
        }
        Self {
            listener,
            token,
            pending: None,
            observed: false,
            invalid: false,
        }
    }

    pub(super) fn port(&self) -> u16 {
        self.listener
            .local_addr()
            .expect("owned receipt port")
            .port()
    }

    pub(super) fn token(&self) -> &str {
        &self.token
    }

    /// Each poll is nonblocking; no private request data crosses assertions.
    /// The callback belongs to the original Host 20-second Child wait.
    pub(super) fn poll(&mut self, deadline: Instant) {
        if self.observed || self.invalid || Instant::now() >= deadline {
            return;
        }
        if self.pending.is_none() {
            match self.listener.accept() {
                Ok((socket, _peer)) => {
                    socket
                        .set_nonblocking(true)
                        .expect("nonblocking receipt reader");
                    self.pending = Some((socket, Vec::new()));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.invalid = true;
                    return;
                }
            }
        }
        let (mut socket, mut data) = self.pending.take().expect("one owned accepted peer");
        let mut chunk = [0_u8; 512];
        match socket.read(&mut chunk) {
            Ok(0) => {
                self.invalid = true;
                return;
            }
            Ok(count) => data.extend_from_slice(&chunk[..count]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                self.pending = Some((socket, data));
                return;
            }
            Err(_) => {
                self.invalid = true;
                return;
            }
        }
        if data.len() > 2048 {
            self.invalid = true;
            return;
        }
        if !data.windows(4).any(|window| window == b"\r\n\r\n") {
            self.pending = Some((socket, data));
            return;
        }
        let expected = format!("GET /keld-body-verified/{} HTTP/1.1\r\n", self.token);
        if !data.starts_with(expected.as_bytes()) {
            self.invalid = true;
            return;
        }
        // An ACK is deliberately required by Bun before app.quit(). It is
        // sent only after the *real native* request is read and validated.
        socket
            .set_nonblocking(false)
            .expect("bounded local receipt response");
        socket
            .set_write_timeout(Some(std::time::Duration::from_millis(100)))
            .expect("bounded receipt write");
        if socket.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 3\r\nConnection: close\r\n\r\nack"
        ).is_err() {
            self.invalid = true;
            return;
        }
        // The native write may briefly block; a post-read, post-write clock
        // fence forbids accepting a receipt beyond the original Host limit.
        if Instant::now() >= deadline {
            self.invalid = true;
            return;
        }
        self.observed = true;
    }

    pub(super) fn stage(&self) -> &'static str {
        if self.invalid {
            "invalid_bun_receipt"
        } else if self.observed {
            "bun_body_verified"
        } else {
            "no_bun_body_receipt"
        }
    }

    pub(super) fn assert_verified(&self) {
        assert!(
            self.observed && !self.invalid,
            "missing actual Bun post-/quit-body native receipt"
        );
    }
}

#[cfg(test)]
mod native_bun_reply_tests {
    use super::QuitBodyReceipt;
    use crate::support::EVENT_DEADLINE;
    use std::io::{ErrorKind, Read as _, Write as _};
    use std::net::TcpListener;
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Instant;

    /// Never let a failed negative leave a standalone Bun process unowned.
    struct ChildOwner(Option<Child>);

    impl Drop for ChildOwner {
        fn drop(&mut self) {
            if let Some(child) = self.0.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Stop/join the only native response worker even if the test's original
    /// clock assertion panics. No detached server can outlive its test.
    struct QuitServerOwner {
        stop: Arc<AtomicBool>,
        join: Option<thread::JoinHandle<()>>,
    }

    impl QuitServerOwner {
        fn start(listener: TcpListener, body: &'static str) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = Arc::clone(&stop);
            let join = thread::spawn(move || serve_real_quit(&listener, body, &stopped));
            Self {
                stop,
                join: Some(join),
            }
        }

        fn stop_and_join(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(join) = self.join.take() {
                let result = join.join();
                if !thread::panicking() {
                    result.expect("native Quit server worker joined");
                }
            }
        }
    }

    impl Drop for QuitServerOwner {
        fn drop(&mut self) {
            self.stop_and_join();
        }
    }

    /// Parses real native HTTP request bytes, with no synthetic observer
    /// state. The only variable is the reply body, not the Bun source code.
    fn serve_real_quit(listener: &TcpListener, body: &'static str, stop: &AtomicBool) {
        let deadline = Instant::now() + EVENT_DEADLINE;
        let mut peer = loop {
            if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
                return;
            }
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::park_timeout(std::time::Duration::from_millis(1));
                }
                Err(_) => return,
            }
        };
        peer.set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .expect("finite HTTP GET byte-poll");
        let mut request = Vec::new();
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            if stop.load(Ordering::Acquire) || Instant::now() >= deadline || request.len() >= 2048 {
                return;
            }
            let mut piece = [0_u8; 512];
            match peer.read(&mut piece) {
                Ok(0) => return,
                Ok(count) => request.extend_from_slice(&piece[..count]),
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return,
            }
        }
        assert!(
            request.starts_with(b"GET /quit HTTP/1.1\r\n"),
            "native Bun client must request the actual Quit route"
        );
        peer.set_write_timeout(Some(std::time::Duration::from_millis(100)))
            .expect("bounded HTTP response writer");
        write!(peer, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
            .expect("real native Quit HTTP body response");
    }

    /// Real Bun fetch + the exact helper text used by the staged Keld app.
    /// The standalone path isolates the first HTTP causal hop from Mac GUI TCC.
    fn run_native_quit_body_probe(body: &'static str) -> (bool, bool) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("native owned Quit server");
        let port = listener.local_addr().expect("native Quit port").port();
        listener.set_nonblocking(true).expect("bounded quit accept");
        let mut server = QuitServerOwner::start(listener, body);
        let mut receipt = QuitBodyReceipt::arm();
        let source = include_str!("../../../fixtures/public_fs_app.ts");
        let body_function = source
            .split_once("// KELD_QUIT_BODY_FUNCTION_BEGIN\n")
            .expect("shared real Bun body-validation function begins")
            .1
            .split_once("// KELD_QUIT_BODY_FUNCTION_END")
            .expect("shared real Bun body-validation function ends")
            .0;
        let script = format!(
            "{body_function}\nconst reply = await fetch('http://127.0.0.1:{port}/quit');\nawait acceptActualQuitBody(reply, {{port:{},nonce:'{}'}});\nconsole.log('KELD_NATIVE_BODY_VERIFIED');",
            receipt.port(),
            receipt.token()
        );
        let mut child = ChildOwner(Some(
            Command::new("bun")
                .args(["-e", &script])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("real Bun native HTTP Quit verifier"),
        ));
        let deadline = Instant::now() + EVENT_DEADLINE;
        let status = loop {
            receipt.poll(deadline);
            if let Some(status) = child
                .0
                .as_mut()
                .expect("owned Bun child")
                .try_wait()
                .expect("inspect native Bun exit")
            {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "Bun native Quit verifier exceeded original deadline"
            );
            thread::yield_now();
        };
        server.stop_and_join();
        let output = child
            .0
            .take()
            .expect("retained Bun child")
            .wait_with_output()
            .expect("collect exact native Bun child output");
        assert_eq!(status, output.status);
        let verified_stdout = output.stdout == b"KELD_NATIVE_BODY_VERIFIED\n";
        (
            status.success() && verified_stdout,
            receipt.stage() == "bun_body_verified",
        )
    }

    #[test]
    fn shipping_test_fixture_calls_real_body_verifier_before_app_quit() {
        let source = include_str!("../../../fixtures/public_fs_app.ts");
        let (implementation, _) = source
            .split_once("// KELD_QUIT_BODY_FUNCTION_END")
            .expect("shared test fixture owns verified body function");
        assert!(
            implementation.contains("if (await quit.text() !== \"quit\") throw"),
            "real Bun body validation must remain before native receipt"
        );
        let after_function = source
            .split_once("// KELD_QUIT_BODY_FUNCTION_END")
            .expect("shared source function boundary")
            .1;
        let verified = after_function
            .find("await acceptActualQuitBody(quit, quitBodyReceipt);")
            .expect("the shipping test app must call the shared real body verifier");
        let quit = after_function
            .find("const pendingQuit = app.quit();")
            .expect("original shipping app lifecycle Quit remains");
        assert!(
            verified < quit,
            "verified native body receipt must precede lifecycle Quit"
        );
        // Preserve the existing no-receipt route, rather than inserting an
        // extra await/microtask into every unrelated real Host shutdown.
        assert!(after_function.contains(
            "} else {\n  // Keep the original no-receipt fixture path inline: no extra async hop.\n  if (!quit.ok) throw new Error(\"invalid sample quit command\");\n  console.error(\"KELD_KEL140_QUIT_HTTP_HEADERS_OK\");\n  if (await quit.text() !== \"quit\") throw new Error(\"invalid sample quit command\");\n  console.error(\"KELD_KEL140_QUIT_HTTP_BODY_OK\");\n}"
        ), "default public-FS quit byte/marker path must stay inline");
    }

    #[test]
    fn real_bun_verified_quit_body_sends_exact_native_receipt() {
        let (completed, native_receipt) = run_native_quit_body_probe("quit");
        assert!(
            completed,
            "real Bun must consume correct Quit body and finish"
        );
        assert!(
            native_receipt,
            "native OS receipt must come from post-body Bun"
        );
    }

    #[test]
    fn real_bun_wrong_quit_body_cannot_mint_native_receipt() {
        let (completed, native_receipt) = run_native_quit_body_probe("wrong");
        assert!(!completed, "real Bun must refuse the wrong HTTP Quit body");
        assert!(
            !native_receipt,
            "wrong HTTP body must never publish a native receipt"
        );
    }
}
