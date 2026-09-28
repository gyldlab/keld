//! HTTP request intake contracts for the shared Windows renderer fixture.

use std::io::Read as _;

use crate::windows_renderer_http::{
    RENDERER_REQUEST_HEADER_LIMIT, RENDERER_REQUEST_LINE_LIMIT, RendererRequestRead,
    read_renderer_request_line,
};

#[test]
fn renderer_beacon_accumulates_a_fragmented_request_line() {
    let mut chunks = [
        b"G".as_slice(),
        b"ET /ready.png HTTP/1.1\r".as_slice(),
        b"\nHost: 127.0.0.1\r\n\r\n".as_slice(),
    ]
    .into_iter();
    let mut request = Vec::new();
    let result = read_renderer_request_line(&mut request, |buffer| {
        let chunk = chunks.next().unwrap_or_default();
        buffer[..chunk.len()].copy_from_slice(chunk);
        Ok(chunk.len())
    })
    .expect("read fragmented renderer request");

    assert_eq!(
        result,
        RendererRequestRead::Complete(b"GET /ready.png HTTP/1.1".to_vec())
    );
}

#[test]
fn renderer_request_line_waits_for_the_complete_header_block() {
    let mut request = Vec::new();
    let mut first = true;
    let partial = read_renderer_request_line(&mut request, |buffer| {
        if first {
            first = false;
            let bytes = b"GET /app HTTP/1.1\r\n";
            buffer[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        } else {
            Err(std::io::ErrorKind::WouldBlock.into())
        }
    })
    .expect("incomplete headers are pending, not an I/O failure");
    assert_eq!(partial, RendererRequestRead::Pending);
    let complete = read_renderer_request_line(&mut request, |buffer| {
        let bytes = b"Host: 127.0.0.1\r\nConnection: close\r\n\r\n";
        buffer[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    })
    .expect("complete request headers");
    assert_eq!(
        complete,
        RendererRequestRead::Complete(b"GET /app HTTP/1.1".to_vec())
    );
}

#[test]
fn renderer_request_header_budget_is_independent_of_the_request_line_budget() {
    assert_eq!(RENDERER_REQUEST_LINE_LIMIT, 2048);
    assert_eq!(RENDERER_REQUEST_HEADER_LIMIT, 8192);
    for total in [8192, 8193] {
        let mut wire = b"GET /app HTTP/1.1\r\nX-Fixture: ".to_vec();
        wire.resize(total - 4, b'a');
        wire.extend_from_slice(b"\r\n\r\n");
        let mut cursor = std::io::Cursor::new(wire);
        let result = read_renderer_request_line(&mut Vec::new(), |buffer| cursor.read(buffer));
        if total == 8192 {
            assert_eq!(
                result.expect("exact header budget admits"),
                RendererRequestRead::Complete(b"GET /app HTTP/1.1".to_vec())
            );
        } else {
            assert_eq!(
                result.err().as_deref(),
                Some("renderer beacon request headers exceeded 8192 bytes")
            );
        }
    }
}

#[test]
fn renderer_beacon_request_line_enforces_exact_maximum_and_maximum_plus_one() {
    let exact = vec![b'a'; RENDERER_REQUEST_LINE_LIMIT - 2];
    let mut exact_wire = exact.clone();
    exact_wire.extend_from_slice(b"\r\n\r\n");
    let mut exact_cursor = std::io::Cursor::new(exact_wire);
    let mut request = Vec::new();
    let result = read_renderer_request_line(&mut request, |buffer| exact_cursor.read(buffer))
        .expect("exact-limit request line");
    assert_eq!(result, RendererRequestRead::Complete(exact));

    let mut oversized_wire = vec![b'b'; RENDERER_REQUEST_LINE_LIMIT - 1];
    oversized_wire.extend_from_slice(b"\r\n");
    let mut oversized_cursor = std::io::Cursor::new(oversized_wire);
    let mut request = Vec::new();
    let error = read_renderer_request_line(&mut request, |buffer| oversized_cursor.read(buffer))
        .expect_err("maximum-plus-one request line must fail");
    assert_eq!(
        error,
        format!("renderer beacon request line exceeded {RENDERER_REQUEST_LINE_LIMIT} bytes")
    );
}

#[test]
fn renderer_beacon_ignores_only_idle_or_reset_before_request_bytes() {
    for kind in [
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::ConnectionAborted,
    ] {
        let mut request = Vec::new();
        let empty = read_renderer_request_line(&mut request, |_| {
            Err(std::io::Error::new(kind, "empty preconnect ended"))
        })
        .expect("pre-request close is an empty preconnect");
        assert_eq!(empty, RendererRequestRead::Empty);
    }
    for kind in [std::io::ErrorKind::TimedOut, std::io::ErrorKind::WouldBlock] {
        let mut request = Vec::new();
        let idle = read_renderer_request_line(&mut request, |_| {
            Err(std::io::Error::new(kind, "empty preconnect ended"))
        })
        .expect("pre-request idle remains pending");
        assert_eq!(idle, RendererRequestRead::Pending);
    }

    let mut first = true;
    let mut request = Vec::new();
    let partial = read_renderer_request_line(&mut request, |buffer| {
        if first {
            first = false;
            buffer[..3].copy_from_slice(b"GET");
            Ok(3)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "partial request reset",
            ))
        }
    })
    .expect_err("a reset after request bytes must remain a failure");
    assert!(
        partial.starts_with("renderer beacon request reset after 3 bytes:"),
        "{partial}"
    );
}
