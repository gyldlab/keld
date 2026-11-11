//! Observe clean discovery-only closure without replacing rmcp's protocol parser.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use rmcp::RoleServer;
use rmcp::model::{
    ClientJsonRpcMessage, ClientRequest, RequestId, ServerJsonRpcMessage, ServerResult,
};
use rmcp::transport::Transport;
use tokio::io::{AsyncRead, ReadBuf};

#[cfg(test)]
mod tests;

/// rmcp reports bootstrap EOF as ConnectionClosed. A successful discovery and
/// an actual input EOF distinguish normal closure from an input or send fault.
#[derive(Debug, Default)]
pub(super) struct DiscoveryEof {
    clean_eof: AtomicBool,
    discovery_replied: AtomicBool,
}

impl DiscoveryEof {
    pub(super) fn is_clean(&self) -> bool {
        self.discovery_replied.load(Ordering::Relaxed) && self.clean_eof.load(Ordering::Relaxed)
    }
}

/// Observe IO completion only. Framing, malformed-input recovery, JSON,
/// versions and metadata validation remain inside the existing SDK transport.
pub(super) struct ObservedRead<R> {
    inner: R,
    observation: Arc<DiscoveryEof>,
}

impl<R> ObservedRead<R> {
    pub(super) fn new(inner: R, observation: Arc<DiscoveryEof>) -> Self {
        Self { inner, observation }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ObservedRead<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let had_capacity = buf.remaining() != 0;
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        match &result {
            Poll::Ready(Ok(())) if had_capacity => this
                .observation
                .clean_eof
                .store(buf.filled().len() == before, Ordering::Relaxed),
            Poll::Ready(Err(_)) => this.observation.clean_eof.store(false, Ordering::Relaxed),
            _ => {}
        }
        result
    }
}

pub(super) struct ObservedTransport<T> {
    inner: T,
    observation: Arc<DiscoveryEof>,
    discovery_id: Option<RequestId>,
}

impl<T> ObservedTransport<T> {
    pub(super) fn new(inner: T, observation: Arc<DiscoveryEof>) -> Self {
        Self {
            inner,
            observation,
            discovery_id: None,
        }
    }
}

impl<T: Transport<RoleServer>> Transport<RoleServer> for ObservedTransport<T> {
    type Error = T::Error;

    fn send(
        &mut self,
        item: ServerJsonRpcMessage,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let discovery = matches!(&item, ServerJsonRpcMessage::Response(response)
            if matches!(&response.result, ServerResult::DiscoverResult(_))
                && self.discovery_id.as_ref() == Some(&response.id));
        let rejected = matches!(&item, ServerJsonRpcMessage::Error(_));
        let sent = self.inner.send(item);
        let observation = self.observation.clone();
        async move {
            sent.await?;
            if discovery || rejected {
                observation
                    .discovery_replied
                    .store(discovery, Ordering::Relaxed);
            }
            Ok(())
        }
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        let message = self.inner.receive().await;
        if let Some(message) = &message {
            self.discovery_id = match message {
                ClientJsonRpcMessage::Request(request)
                    if matches!(&request.request, ClientRequest::DiscoverRequest(_)) =>
                {
                    Some(request.id.clone())
                }
                _ => None,
            };
        }
        message
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}
