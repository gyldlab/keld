//! KEL-98: byte-defined Unicode contracts shared with the TypeScript regressions.
//!
//! These are scalar-value strings, not text files: leading U+FEFF is data.

use keld_ipc::codec::{decode, encode};
use keld_ipc::echo::handle_echo;
use keld_ipc::{EchoRequest, EchoResponse, IpcError};

#[test]
fn echo_preserves_unicode_scalars_and_exact_postcard_bytes() -> Result<(), IpcError> {
    let vectors: &[(&str, &[u8])] = &[
        ("", &[0x00, 0x00]),
        ("\u{feff}a", &[0x04, 0xef, 0xbb, 0xbf, 0x61, 0x00]),
        (
            "\u{feff}\u{feff}",
            &[0x06, 0xef, 0xbb, 0xbf, 0xef, 0xbb, 0xbf, 0x00],
        ),
        ("\u{fffd}", &[0x03, 0xef, 0xbf, 0xbd, 0x00]),
        ("\0", &[0x01, 0x00, 0x00]),
        ("\u{10000}", &[0x04, 0xf0, 0x90, 0x80, 0x80, 0x00]),
        ("\u{10ffff}", &[0x04, 0xf4, 0x8f, 0xbf, 0xbf, 0x00]),
    ];
    for &(message, wire) in vectors {
        let request = EchoRequest {
            message: message.to_owned(),
            count: 0,
        };
        assert_eq!(encode(&request)?.as_slice(), wire);
        let output = handle_echo(wire)?;
        assert_eq!(output.as_slice(), wire);
        let response: EchoResponse = decode(&output)?;
        assert_eq!(response.message, message);
        assert_eq!(response.count, 0);
    }
    Ok(())
}

#[test]
fn echo_rejects_non_scalar_and_malformed_utf8() {
    // Each prefix declares the string's exact byte count; the last byte is count.
    let malformed: &[&[u8]] = &[
        &[0x01, 0xff, 0x00],
        &[0x01, 0x80, 0x00],
        &[0x02, 0xc0, 0xaf, 0x00],
        &[0x03, 0xed, 0xa0, 0x80, 0x00],
        &[0x04, 0xf4, 0x90, 0x80, 0x80, 0x00],
        &[0x02, 0xe2, 0x82, 0x00],
    ];
    for &wire in malformed {
        assert!(matches!(handle_echo(wire), Err(IpcError::Codec(_))));
    }
}
