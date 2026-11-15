//! Contract tests for the KEL-19 `.keldeai` host container (container spec §3, §7).
//!
//! Fixtures are synthetic PE32+ images built in test code (`fixture`); no binary is
//! committed. `writer` covers AC1, AC3–AC5 and AC7, `malformed` AC6, `reader` AC9 and
//! the bounded-read property, and `platform` AC10.

use super::*;

mod fixture;
mod malformed;
mod platform;
mod reader;
mod writer;

/// Runs the writer on an admissible fixture with the golden payload.
fn embed(host: &[u8]) -> Vec<u8> {
    embed_host_identity(host, &fixture::golden_payload())
        .unwrap_or_else(|error| panic!("fixture must be admissible: {error}"))
}

/// The error's code and, when the variant has one, its static detail.
fn refusal<T: std::fmt::Debug>(result: Result<T, PackError>) -> (&'static str, &'static str) {
    match result {
        Ok(value) => panic!("expected a refusal, got {value:?}"),
        Err(error) => {
            let detail = match error {
                PackError::HostImageInvalid { detail }
                | PackError::HostImageNotPristine { detail }
                | PackError::HostImageNoRoom { detail }
                | PackError::IdentityContainerInvalid { detail }
                | PackError::ExpectedIdentityInvalid { detail }
                | PackError::InvalidMetadata { detail } => detail,
                _ => "",
            };
            (error.code(), detail)
        }
    }
}

#[test]
fn new_errors_have_stable_codes_and_fix_guidance() {
    let cases = [
        (
            PackError::HostImageInvalid { detail: "d" },
            "KELD-PACK-006",
            "unmodified prebuilt `keld-host.exe` or `keld-updater-helper.exe`",
        ),
        (
            PackError::IdentityContainerMissing,
            "KELD-PACK-007",
            "Rebuild with `keld build`",
        ),
        (
            PackError::IdentityContainerDuplicate,
            "KELD-PACK-008",
            "never re-run embedding on its output",
        ),
        (
            PackError::HostImageNotPristine { detail: "d" },
            "KELD-PACK-009",
            "never embed into a signed image",
        ),
        (
            PackError::HostImageNoRoom { detail: "d" },
            "KELD-PACK-010",
            "Keld build defect to report",
        ),
        (
            PackError::IdentityContainerInvalid { detail: "d" },
            "KELD-PACK-011",
            "reinstall the signed package or rebuild with `keld build`",
        ),
        (
            PackError::IdentityContainerRead {
                source: std::io::Error::other("injected"),
            },
            "KELD-PACK-012",
            "reinstall the signed package if the failure persists",
        ),
    ];
    for (error, code, fix) in cases {
        assert_eq!(error.code(), code);
        let text = error.to_string();
        assert!(text.starts_with(&format!("{code}: ")), "{text}");
        assert!(text.contains(fix), "{text}");
        // The same container serves keld-host.exe and keld-updater-helper.exe (KEL-19
        // container spec §1), so no text names the host alone.
        assert!(
            !text.replace("keld-host.exe", "").contains("host"),
            "{code} names only the host: {text}"
        );
    }
    let read = PackError::IdentityContainerRead {
        source: std::io::Error::other("injected"),
    };
    let source = std::error::Error::source(&read).map(ToString::to_string);
    assert_eq!(source.as_deref(), Some("injected"));
    assert!(
        PackError::HostImageInvalid {
            detail: "missing MZ signature"
        }
        .to_string()
        .contains("(missing MZ signature)")
    );
}
