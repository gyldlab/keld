//! Bounded HTTP request intake shared by native Windows renderer fixtures.

use std::net::{TcpListener, TcpStream};

pub(crate) const RENDERER_CONNECTION_LIMIT: usize = 16;
pub(crate) const RENDERER_REQUEST_LINE_LIMIT: usize = 2048;
pub(crate) const RENDERER_REQUEST_HEADER_LIMIT: usize = 8192;

pub(crate) struct PendingRendererRequest {
    pub(crate) stream: TcpStream,
    pub(crate) request: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RendererRequestRead {
    Pending,
    Empty,
    Complete(Vec<u8>),
}

pub(crate) fn read_renderer_request_line(
    request: &mut Vec<u8>,
    mut read_chunk: impl FnMut(&mut [u8]) -> std::io::Result<usize>,
) -> Result<RendererRequestRead, String> {
    loop {
        if request.len() == RENDERER_REQUEST_HEADER_LIMIT {
            return Err(format!(
                "renderer beacon request headers exceeded {RENDERER_REQUEST_HEADER_LIMIT} bytes"
            ));
        }
        let mut chunk = [0_u8; 256];
        let available = (RENDERER_REQUEST_HEADER_LIMIT - request.len()).min(chunk.len());
        let read = match read_chunk(&mut chunk[..available]) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(RendererRequestRead::Pending);
            }
            Err(error) if request.is_empty() && error.kind() == std::io::ErrorKind::TimedOut => {
                return Ok(RendererRequestRead::Pending);
            }
            Err(error)
                if request.is_empty()
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) =>
            {
                return Ok(RendererRequestRead::Empty);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                return Err(format!(
                    "renderer beacon request reset after {} bytes: {error}",
                    request.len()
                ));
            }
            Err(error) => return Err(format!("read renderer beacon request: {error}")),
        };
        if read == 0 {
            return if request.is_empty() {
                Ok(RendererRequestRead::Empty)
            } else {
                Err(format!(
                    "renderer beacon request ended before the header terminator: {}",
                    String::from_utf8_lossy(request)
                ))
            };
        }
        request.extend_from_slice(&chunk[..read]);
        if let Some(line_end) = request.windows(2).position(|bytes| bytes == b"\r\n") {
            if line_end + 2 > RENDERER_REQUEST_LINE_LIMIT {
                return Err(format!(
                    "renderer beacon request line exceeded {RENDERER_REQUEST_LINE_LIMIT} bytes"
                ));
            }
            // Consume the complete request headers before a Connection: close
            // response. Closing with unread incoming bytes can reset the socket
            // and discard a larger HTML/worker response in the browser.
            if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut complete = std::mem::take(request);
                complete.truncate(line_end);
                return Ok(RendererRequestRead::Complete(complete));
            }
        } else if request.len() >= RENDERER_REQUEST_LINE_LIMIT {
            return Err(format!(
                "renderer beacon request line exceeded {RENDERER_REQUEST_LINE_LIMIT} bytes"
            ));
        }
    }
}

/// Admit at most one connection after the caller has reaped completed/closed peers.
/// Returns whether a new stream was accepted; no request waits on another stream.
pub(crate) fn accept_renderer_connection(
    listener: &TcpListener,
    pending: &mut Vec<PendingRendererRequest>,
    context: &str,
) -> Result<bool, String> {
    match listener.accept() {
        Ok((stream, _)) => {
            if pending.len() == RENDERER_CONNECTION_LIMIT {
                return Err(format!(
                    "{context} exceeded {RENDERER_CONNECTION_LIMIT} pending connections"
                ));
            }
            stream
                .set_nonblocking(true)
                .map_err(|error| format!("set {context} stream nonblocking: {error}"))?;
            pending.push(PendingRendererRequest {
                stream,
                request: Vec::new(),
            });
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(format!("accept {context}: {error}")),
    }
}
