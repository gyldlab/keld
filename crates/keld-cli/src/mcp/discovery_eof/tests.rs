use super::*;

const DISCOVER: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}"#,
    "\n"
);

fn serve_bytes(
    input: Vec<u8>,
    read_error: bool,
    write_error: bool,
    chunk_size: usize,
) -> Result<(), super::super::McpServeError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(super::super::serve_io(
        None,
        FaultRead {
            input: std::io::Cursor::new(input),
            read_error,
            chunk_size,
        },
        FaultWrite { write_error },
    ))
}

#[test]
fn complete_discovery_eof_includes_lf_crlf_batched_and_split_reads() {
    for chunk_size in [7, 4096] {
        for input in [
            DISCOVER.to_owned(),
            DISCOVER.replace('\n', "\r\n"),
            format!("\r\n{DISCOVER}\n{DISCOVER}\r\n"),
        ] {
            assert!(serve_bytes(input.into_bytes(), false, false, chunk_size).is_ok());
        }
    }
}

#[test]
fn discovery_eof_preserves_sdk_malformed_and_truncated_input_recovery() {
    let tools = DISCOVER.replace("server/discover", "tools/list");
    for suffix in ["not json\n", "{}\n", "{", "\r", " "] {
        let discovery = serve_bytes(
            format!("{DISCOVER}{suffix}").into_bytes(),
            false,
            false,
            4096,
        );
        let ordinary = serve_bytes(format!("{tools}{suffix}").into_bytes(), false, false, 4096);
        assert!(ordinary.is_ok(), "ordinary SDK recovery: {suffix:?}");
        assert!(discovery.is_ok(), "discovery SDK recovery: {suffix:?}");
    }
}

#[test]
fn eof_without_a_complete_discovery_is_an_error() {
    for input in ["", "not json\n", "{", "{}\n", DISCOVER.trim_end()] {
        assert!(
            serve_bytes(input.as_bytes().to_vec(), false, false, 4096).is_err(),
            "input={input:?}"
        );
    }
}

#[test]
fn discovery_eof_preserves_rejected_metadata_and_initialization_errors() {
    for suffix in [
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"server/discover\",\"params\":{\"_meta\":{}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"initialize\",\"params\":{}}\n",
    ] {
        assert!(
            serve_bytes(
                format!("{DISCOVER}{suffix}").into_bytes(),
                false,
                false,
                4096
            )
            .is_err(),
            "suffix={suffix}"
        );
    }
}

#[test]
fn discovery_eof_preserves_read_or_write_failure() {
    assert!(serve_bytes(DISCOVER.as_bytes().to_vec(), true, false, 4096).is_err());
    assert!(serve_bytes(DISCOVER.as_bytes().to_vec(), false, true, 4096).is_err());
}

struct FaultRead {
    input: std::io::Cursor<Vec<u8>>,
    read_error: bool,
    chunk_size: usize,
}

impl AsyncRead for FaultRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut bytes = [0; 4096];
        let limit = buf.remaining().min(bytes.len()).min(self.chunk_size);
        let count = std::io::Read::read(&mut self.input, &mut bytes[..limit])?;
        if count == 0 && self.read_error {
            return Poll::Ready(Err(io::Error::other("injected read failure")));
        }
        buf.put_slice(&bytes[..count]);
        Poll::Ready(Ok(()))
    }
}

struct FaultWrite {
    write_error: bool,
}

impl tokio::io::AsyncWrite for FaultWrite {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.write_error {
            Poll::Ready(Err(io::Error::other("injected write failure")))
        } else {
            Poll::Ready(Ok(bytes.len()))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
