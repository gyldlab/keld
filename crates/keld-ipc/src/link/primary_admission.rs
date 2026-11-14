//! Same primary header/clock reader with one trusted preallocation custody cut.
//! No local reservation, body limit, role identity, or wire field is minted here.
#![forbid(unsafe_code)]

use super::{
    APP_LINK_IO_DEADLINE, AtomicBool, ChannelEntry, CorrelationId, FrameKind, GuardedCallAdmission,
    IpcError, PrimaryAppFrame, Read, ValidatedFrameHeader, read_frame_interruptible_validated_with,
    validate_primary_app_header_with_privileged_calls,
};

/// Read one primary frame after canonical semantic validation and a caller-owned
/// body-storage admission. The additional callback runs exactly once after all
/// existing kind/flags/channel/correlation rules and before body allocation.
/// It may retain the original operation's storage owner until this call's body
/// and subsequent decode are physically disposed; it supplies no wire authority.
///
/// Existing users delegate with a no-op callback and keep their global envelope
/// policy unchanged. A numeric renderer owner may additionally refuse a body
/// that cannot fit its already admitted native result carrier.
///
/// # Errors
/// The original syntax/envelope/semantic/clock/I/O errors, or the trusted owner's
/// refusal before body allocation/consumption. There is no partial-body retry.
pub fn read_primary_app_frame_interruptible_with_payload_admission<S: Read>(
    stream: &mut S,
    stop: &AtomicBool,
    pending_echo_reply: impl Fn() -> Option<CorrelationId>,
    privileged_calls: &[&'static ChannelEntry],
    admission: impl Fn(&'static ChannelEntry, CorrelationId) -> GuardedCallAdmission,
    mut payload_admission: impl FnMut(&ValidatedFrameHeader) -> Result<(), IpcError>,
) -> Result<Option<PrimaryAppFrame>, IpcError> {
    read_frame_interruptible_validated_with(
        stream,
        stop,
        None,
        None,
        APP_LINK_IO_DEADLINE,
        |header| {
            let validated = validate_primary_app_header_with_privileged_calls(
                pending_echo_reply(),
                privileged_calls,
                header,
            )?;
            let outcome = if header.kind == FrameKind::Call {
                privileged_calls
                    .iter()
                    .copied()
                    .find(|entry| header.channel == entry.id())
                    .map_or(GuardedCallAdmission::Admit, |entry| {
                        admission(entry, header.corr)
                    })
            } else {
                GuardedCallAdmission::Admit
            };
            if outcome == GuardedCallAdmission::RejectSession {
                return Err(IpcError::Protocol {
                    detail: "privileged Call is already outstanding for this session",
                });
            }
            payload_admission(&validated)?;
            Ok((validated, outcome))
        },
    )
    .map(|frame| {
        frame.map(|((header, admission), payload)| PrimaryAppFrame {
            header,
            payload,
            admission,
        })
    })
}

#[cfg(test)]
mod tests;
