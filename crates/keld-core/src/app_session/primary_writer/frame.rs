//! Encoding and explicit value-free CONTROL classification.
//! An arbitrary payload or a consumer-authored error can never obtain CONTROL.

use super::{
    Arc, CONTROL_BYTES, ChannelId, Class, CorrelationId, FrameHeader, FrameKind, HostAppError,
    Instant, Packet, PreparedFrame, PrimaryWriter, Reservation, WriterLink, WriterRefusal,
};
#[cfg(test)]
use super::{PrimaryCallerContext, SessionShutdownState};
use keld_ipc::compat_control::{
    CompatControlEvent, CompatControlResponse, DispatchCertainty, TerminalDisposition,
};
use keld_ipc::{CallError, LifecycleEvent, LifecycleResponse};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app_session) enum ValueFreeTerminal {
    Consumed,
    NoListener,
    Stale,
    Retired(DispatchCertainty),
}
impl ValueFreeTerminal {
    fn into_wire(self) -> TerminalDisposition {
        match self {
            Self::Consumed => TerminalDisposition::Consumed,
            Self::NoListener => TerminalDisposition::NoListener,
            Self::Stale => TerminalDisposition::Stale,
            Self::Retired(certainty) => TerminalDisposition::RouteRetired { certainty },
        }
    }
}

/// Closed classification; strings, paths, values, and consumer errors are absent.
#[derive(Debug)]
pub(in crate::app_session) enum ControlMessage {
    Ping {
        channel: ChannelId,
        correlation: CorrelationId,
    },
    LifecycleReply {
        correlation: CorrelationId,
        response: LifecycleResponse,
    },
    LifecycleEvent(LifecycleEvent),
    WindowReply {
        correlation: CorrelationId,
        response: keld_ipc::window::WindowResponse,
    },
    CompatReleased {
        correlation: CorrelationId,
    },
    CompatTerminal {
        correlation: CorrelationId,
        disposition: ValueFreeTerminal,
    },
    DocumentBound {
        webview: u32,
        navigation: u32,
        window_id: u32,
        window_incarnation: u64,
    },
    DocumentRetired {
        webview: u32,
        navigation: u32,
    },
    Capacity {
        channel: ChannelId,
        correlation: CorrelationId,
    },
    GenerationRetired {
        channel: ChannelId,
        correlation: CorrelationId,
    },
    QuitDrained {
        channel: ChannelId,
        correlation: CorrelationId,
    },
}

impl PrimaryWriter {
    #[cfg(test)]
    pub(in crate::app_session) fn prepare_control(
        &self,
        caller: &PrimaryCallerContext,
        shutdown: &SessionShutdownState,
        target: Arc<WriterLink>,
        message: ControlMessage,
        deadline: Instant,
    ) -> Result<PreparedFrame, HostAppError> {
        let reservation = self
            .reserve(Class::Control, Some((caller, shutdown)), deadline)
            .map_err(|error| error.into_host_error("primary CONTROL reservation"))?;
        message.encode(reservation, target)
    }

    #[cfg(test)]
    pub(in crate::app_session) fn try_prepare_control(
        &self,
        target: Arc<WriterLink>,
        message: ControlMessage,
        deadline: Instant,
    ) -> Result<PreparedFrame, HostAppError> {
        let reservation = self
            .reserve(Class::Control, None, deadline)
            .map_err(|error| error.into_host_error("primary CONTROL reservation"))?;
        message.encode(reservation, target)
    }
}

impl Reservation {
    /// Borrowed input retains its existing upstream charge. The sole new encoded
    /// Box is counted, allocated and encoded while this position is reserved.
    pub(in crate::app_session) fn encode_value<T: serde::Serialize + ?Sized>(
        self,
        target: Arc<WriterLink>,
        kind: FrameKind,
        channel: ChannelId,
        correlation: CorrelationId,
        value: &T,
        payload_limit: usize,
    ) -> Result<PreparedFrame, HostAppError> {
        if self.class != Class::Value || kind == FrameKind::Err {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary value encoding"));
        }
        self.encode(target, kind, channel, correlation, value, payload_limit)
    }

    /// A previously encoded inbound value is copied once into its charged writer
    /// allocation before admission. Enqueue and dequeue never clone that Box.
    pub(in crate::app_session) fn copy_value(
        self,
        target: Arc<WriterLink>,
        kind: FrameKind,
        channel: ChannelId,
        correlation: CorrelationId,
        bytes: &[u8],
    ) -> Result<PreparedFrame, HostAppError> {
        if self.class != Class::Value || kind == FrameKind::Err || bytes.len() > self.class.limit()
        {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary value copy"));
        }
        self.finish(target, kind, channel, correlation, bytes.into())
    }

    fn encode<T: serde::Serialize + ?Sized>(
        self,
        target: Arc<WriterLink>,
        kind: FrameKind,
        channel: ChannelId,
        correlation: CorrelationId,
        value: &T,
        payload_limit: usize,
    ) -> Result<PreparedFrame, HostAppError> {
        let length = keld_ipc::codec::encoded_len(value)
            .map_err(|source| super::super::app_ipc("primary frame count", &source))?;
        if length > self.class.limit().min(payload_limit) || Instant::now() >= self.deadline {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary frame bound"));
        }
        let mut bytes = vec![0; length].into_boxed_slice();
        let encoded = keld_ipc::codec::encode_into(value, &mut bytes)
            .map_err(|source| super::super::app_ipc("primary bounded encoding", &source))?;
        if encoded.len() != length {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary encoding stability"));
        }
        self.finish(target, kind, channel, correlation, bytes)
    }

    fn finish(
        self,
        target: Arc<WriterLink>,
        kind: FrameKind,
        channel: ChannelId,
        correlation: CorrelationId,
        payload: Box<[u8]>,
    ) -> Result<PreparedFrame, HostAppError> {
        if payload.len() > self.class.limit() || Instant::now() >= self.deadline {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary encoded frame"));
        }
        let len = u32::try_from(payload.len())
            .map_err(|_| WriterRefusal::InvalidFrame.into_host_error("primary frame length"))?;
        let deadline = self.deadline;
        Ok(PreparedFrame {
            reservation: self,
            packet: Some(Packet {
                header: FrameHeader {
                    kind,
                    flags: 0,
                    channel,
                    corr: correlation,
                    len,
                },
                payload,
                target,
                deadline,
            }),
        })
    }

    /// Ordinary value-bearing errors retain the sanctioned `CallError` encoder;
    /// a path, consumer message or page-derived error never borrows CONTROL.
    pub(in crate::app_session) fn encode_error_value(
        self,
        target: Arc<WriterLink>,
        channel: ChannelId,
        correlation: CorrelationId,
        error: &CallError,
    ) -> Result<PreparedFrame, HostAppError> {
        if self.class != Class::Value {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary value error"));
        }
        self.encode_error(target, channel, correlation, error)
    }

    fn fixed_error(
        self,
        target: Arc<WriterLink>,
        channel: ChannelId,
        correlation: CorrelationId,
        error: &CallError,
    ) -> Result<PreparedFrame, HostAppError> {
        // Only the three canonical, fixed framework constructors below can use
        // this path. Consumer/path-bearing errors use the ordinary value lane.
        self.encode_error(target, channel, correlation, error)
    }

    fn encode_error(
        self,
        target: Arc<WriterLink>,
        channel: ChannelId,
        correlation: CorrelationId,
        error: &CallError,
    ) -> Result<PreparedFrame, HostAppError> {
        let length = keld_ipc::codec::encoded_len(error)
            .map_err(|source| super::super::app_ipc("primary error count", &source))?;
        if length > self.class.limit() || Instant::now() >= self.deadline {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary error bound"));
        }
        let mut bytes = vec![0; length].into_boxed_slice();
        let used = keld_ipc::encode_call_error_into(error, &mut bytes)
            .map_err(|source| super::super::app_ipc("primary error encoding", &source))?;
        if used.len() != length {
            return Err(WriterRefusal::InvalidFrame.into_host_error("primary error stability"));
        }
        self.finish(target, FrameKind::Err, channel, correlation, bytes)
    }
}

impl ControlMessage {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the complete closed control encoding dispatch in one prepaid reservation owner."
    )]
    pub(in crate::app_session) fn encode(
        self,
        reservation: Reservation,
        target: Arc<WriterLink>,
    ) -> Result<PreparedFrame, HostAppError> {
        let compat = keld_ipc::channel_table::COMPAT_CONTROL.id();
        match self {
            Self::Ping {
                channel,
                correlation,
            } => reservation.finish(target, FrameKind::Ping, channel, correlation, Box::new([])),
            Self::LifecycleReply {
                correlation,
                response,
            } => reservation.encode(
                target,
                FrameKind::Reply,
                keld_ipc::LIFECYCLE_CHANNEL,
                correlation,
                &response,
                CONTROL_BYTES,
            ),
            Self::WindowReply {
                correlation,
                response,
            } => {
                // Exhaustive fixed-shape classification, not a second wire schema.
                // Adding any value-bearing field must reopen this CONTROL review.
                use keld_ipc::window::{WindowRef, WindowResponse};
                match &response {
                    WindowResponse::Created { window } | WindowResponse::Adopted { window } => {
                        let _: WindowRef = *window;
                    }
                    WindowResponse::CloseRequested { close_seq }
                    | WindowResponse::Merged { close_seq } => {
                        let _: u64 = *close_seq;
                    }
                    WindowResponse::Accepted
                    | WindowResponse::AlreadyClosing
                    | WindowResponse::ReplyApplied
                    | WindowResponse::ReplyStale
                    | WindowResponse::Destroyed
                    | WindowResponse::Loaded => {}
                }
                reservation.encode(
                    target,
                    FrameKind::Reply,
                    keld_ipc::channel_table::WINDOW.id(),
                    correlation,
                    &response,
                    CONTROL_BYTES,
                )
            }
            Self::LifecycleEvent(event) => reservation.encode(
                target,
                FrameKind::Event,
                keld_ipc::LIFECYCLE_CHANNEL,
                CorrelationId(0),
                &event,
                CONTROL_BYTES,
            ),
            Self::CompatReleased { correlation } => reservation.encode(
                target,
                FrameKind::Reply,
                compat,
                correlation,
                &CompatControlResponse::Released,
                CONTROL_BYTES,
            ),
            Self::CompatTerminal {
                correlation,
                disposition,
            } => reservation.encode(
                target,
                FrameKind::Reply,
                compat,
                correlation,
                &CompatControlResponse::Deliver {
                    disposition: disposition.into_wire(),
                },
                CONTROL_BYTES,
            ),
            Self::DocumentBound {
                webview,
                navigation,
                window_id,
                window_incarnation,
            } => reservation.encode(
                target,
                FrameKind::Event,
                compat,
                CorrelationId(0),
                &CompatControlEvent::DocumentBound {
                    webview,
                    navigation,
                    window_id,
                    window_incarnation,
                },
                CONTROL_BYTES,
            ),
            Self::DocumentRetired {
                webview,
                navigation,
            } => reservation.encode(
                target,
                FrameKind::Event,
                compat,
                CorrelationId(0),
                &CompatControlEvent::DocumentRetired {
                    webview,
                    navigation,
                },
                CONTROL_BYTES,
            ),
            Self::Capacity {
                channel,
                correlation,
            } => reservation.fixed_error(
                target,
                channel,
                correlation,
                &CallError::admission_capacity(),
            ),
            Self::GenerationRetired {
                channel,
                correlation,
            } => reservation.fixed_error(
                target,
                channel,
                correlation,
                &CallError::generation_retired(),
            ),
            Self::QuitDrained {
                channel,
                correlation,
            } => reservation.fixed_error(target, channel, correlation, &CallError::quit_drained()),
        }
    }
}
