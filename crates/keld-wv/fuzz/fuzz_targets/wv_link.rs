//! Raw legacy, compat and app-local WebView-link envelope fuzz target.

#![no_main]

// Compile the production decoder itself, not a copied model and not the full
// platform crate. This keeps fuzzing on the exact shipping parser while the
// standalone fuzz workspace stays free of WebView backend dependencies.
// This raw parser target compiles the exact production module; terminal
// output types are used by native owners, not constructed by this harness.
#[allow(dead_code)]
#[path = "../../src/wv_link.rs"]
mod production_wv_link;

use production_wv_link::{
    AppWvLinkEnvelope, MAX_APP_WV_LINK_ENVELOPE_LEN, MAX_RENDERER_PAYLOAD_LEN,
    MAX_WV_LINK_ENVELOPE_LEN, WvLinkEnvelope, WvLinkMode, decode_app_wv_link, decode_wv_link,
    decode_wv_link_for_mode,
};

fn assert_document_shape(document: &str) {
    assert_eq!(document.len(), 32);
    assert!(
        document
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
}

fn assert_compat_envelope(envelope: &WvLinkEnvelope) {
    match envelope {
        WvLinkEnvelope::Bind => {}
        WvLinkEnvelope::Invoke {
            document,
            request,
            channel,
            payload,
        } => {
            assert_document_shape(document);
            assert_ne!(request.get(), 0);
            assert_ne!(channel.get(), 0);
            assert!(payload.len() <= MAX_RENDERER_PAYLOAD_LEN);
        }
        WvLinkEnvelope::Send {
            document,
            message,
            channel,
            payload,
        } => {
            assert_document_shape(document);
            assert_ne!(message.get(), 0);
            assert!(channel.len() <= 256);
            assert!(payload.len() <= MAX_RENDERER_PAYLOAD_LEN);
        }
        WvLinkEnvelope::Consumed {
            document, delivery, ..
        } => {
            assert_document_shape(document);
            assert_ne!(delivery.get(), 0);
        }
    }
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(envelope) = decode_wv_link(data) {
        assert!(data.len() <= MAX_WV_LINK_ENVELOPE_LEN);
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
                assert!(payload.len() <= MAX_RENDERER_PAYLOAD_LEN);
            }
            WvLinkEnvelope::Send { .. } | WvLinkEnvelope::Consumed { .. } => {
                unreachable!("legacy v1 decoder must never emit v2 envelopes");
            }
        }
    }
    if let Ok(envelope) = decode_wv_link_for_mode(data, WvLinkMode::CompatV2) {
        assert!(data.len() <= MAX_WV_LINK_ENVELOPE_LEN);
        assert_compat_envelope(&envelope);
        assert!(decode_wv_link(data).is_err(), "v2 cannot downgrade to v1");
        assert!(matches!(
            decode_app_wv_link(data),
            Ok(AppWvLinkEnvelope::Transport(app)) if app == envelope
        ));
    }
    if let Ok(envelope) = decode_app_wv_link(data) {
        assert!(data.len() <= MAX_APP_WV_LINK_ENVELOPE_LEN);
        match envelope {
            AppWvLinkEnvelope::Transport(envelope) => {
                assert_compat_envelope(&envelope);
                assert_eq!(
                    decode_wv_link_for_mode(data, WvLinkMode::CompatV2),
                    Ok(envelope),
                );
            }
            AppWvLinkEnvelope::Call(call) => {
                assert_document_shape(&call.document);
                assert_ne!(call.call, 0);
                assert!(matches!(
                    call.method.as_str(),
                    "request" | "registerMsgListener" | "sendMessage" | "listenOnce"
                ));
                assert!(call.args.len() <= 16);
                assert!(decode_wv_link_for_mode(data, WvLinkMode::CompatV2).is_err());
            }
            AppWvLinkEnvelope::Registered { document, methods } => {
                assert_document_shape(&document);
                assert_eq!(
                    methods,
                    [
                        "request",
                        "registerMsgListener",
                        "sendMessage",
                        "listenOnce"
                    ]
                );
                assert!(decode_wv_link_for_mode(data, WvLinkMode::CompatV2).is_err());
            }
            AppWvLinkEnvelope::ReleaseCall { document, call }
            | AppWvLinkEnvelope::CallFinished { document, call } => {
                assert_document_shape(&document);
                assert_ne!(call, 0);
                assert!(decode_wv_link_for_mode(data, WvLinkMode::CompatV2).is_err());
            }
        }
    }
});
