//! Raw private WebView-link envelope fuzz target (KEL-142 AC7).

#![no_main]

// Compile the production decoder itself, not a copied model and not the full
// platform crate. This keeps fuzzing on the exact shipping parser while the
// standalone fuzz workspace stays free of WebView backend dependencies.
#[path = "../../src/wv_link.rs"]
mod production_wv_link;

use production_wv_link::{WvLinkEnvelope, decode_wv_link};

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
