//! Raw private WebView-link envelope fuzz target (KEL-142 AC7).

#![no_main]

use keld_wv::wv_link::{WvLinkEnvelope, decode_wv_link};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(envelope) = decode_wv_link(data) {
        match envelope {
            WvLinkEnvelope::Bind => {}
            WvLinkEnvelope::Invoke {
                request,
                channel,
                payload,
                ..
            } => {
                assert_ne!(request.get(), 0);
                assert_ne!(channel.get(), 0);
                assert!(payload.len() <= 4096);
            }
        }
    }
});
