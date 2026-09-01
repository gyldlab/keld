//! Raw header/session decode fuzz target (KEL-133 spec section 3 criterion 12).
//!
//! Property set: for every v0 receive policy, feeding arbitrary bytes into the
//! validated read path terminates without panic, forged lengths never allocate
//! past `MAX_FRAME_LEN`, and any admitted frame satisfies its policy's
//! declared rules. Handler effects and credential disclosure are structurally
//! impossible here: no handler or token exists in the harness.

#![no_main]

use std::io::Cursor;

use keld_ipc::link::read_validated_frame;
use keld_ipc::receive::{validate_received_header, ReceivePolicy};
use keld_ipc::{ChannelId, CorrelationId, FrameHeader, FrameKind, HEADER_LEN};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let policies = [
        ReceivePolicy::primary_app_receiver(),
        ReceivePolicy::server_pre_auth_hello(),
        ReceivePolicy::client_await_hello(),
        ReceivePolicy::echo_receiver(),
        ReceivePolicy::echo_reply_waiter(CorrelationId(7)),
        ReceivePolicy::lifecycle_receiver(),
        ReceivePolicy::lifecycle_event_receiver(),
        ReceivePolicy::lifecycle_reply_waiter(CorrelationId(7)),
        ReceivePolicy::privileged_call_receiver(ChannelId(2)),
    ];
    for policy in &policies {
        let mut cursor = Cursor::new(data);
        match read_validated_frame(&mut cursor, policy) {
            Ok((_admitted, payload)) => {
                // Admission implies policy conformance, re-derived from bytes.
                let header_bytes: &[u8; HEADER_LEN] =
                    &data[..HEADER_LEN].try_into().expect("admitted implies a full header");
                let header = FrameHeader::decode(header_bytes).expect("admitted implies syntax");
                let revalidated =
                    validate_received_header(policy, header).expect("admitted implies semantics");
                assert_eq!(u64::from(revalidated.len()), payload.len() as u64);
                if revalidated.kind() == FrameKind::Ping {
                    assert!(policy.allow_ping);
                    assert_eq!(revalidated.flags(), 0);
                    assert!(payload.is_empty());
                } else {
                    assert!(policy.kinds.contains(revalidated.kind()));
                    assert_eq!(revalidated.flags(), 0);
                    assert_eq!(revalidated.channel(), policy.channel);
                }
            }
            Err(err) => {
                // Terminates with a classified `KELD-IPC-*` error, never a panic.
                let text = err.to_string();
                assert!(text.starts_with("KELD-IPC-0"), "unclassified: {text}");
            }
        }
    }
});
