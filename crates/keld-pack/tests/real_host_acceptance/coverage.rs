//! AC8: the Authenticode image hash covers the container's payload, padding and section
//! header, and the `CheckSum` is the excluded control (§7 row 8).

use super::{
    APP_ID, TRUST_E_NOSIGNATURE, WIN_VERIFY_TRUST_REJECTED, env_path, fixture_payload, sha256_hex,
};
use keld_guard::{WindowsAuthenticodeError, WindowsAuthenticodeImage};
use keld_pack::{ExpectedAppIdentityPayload, embed_host_identity, read_host_identity};

/// `TRUST_E_BAD_DIGEST`: "The digital signature of the object did not verify."
const TRUST_E_BAD_DIGEST: u32 = 0x8009_6010;

fn u16_at(image: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes(
        image[at..at + 2].try_into().expect("two bytes"),
    ))
}

fn u32_at(image: &[u8], at: usize) -> usize {
    usize::try_from(u32::from_le_bytes(
        image[at..at + 4].try_into().expect("four bytes"),
    ))
    .expect("u32 fits usize")
}

/// What the KEL-135 verifier owner observed on one fresh copy of an image.
#[derive(Debug, PartialEq)]
enum Trust {
    /// `WinVerifyTrust` returned zero, the image has one primary and no secondary
    /// signature, and the program name decoded; the container was then read through the
    /// verified pinned handle.
    Verified {
        app_id: String,
        payload: Result<ExpectedAppIdentityPayload, String>,
    },
    /// `WinVerifyTrust` returned this non-zero status.
    Rejected(u32),
    /// Any other refusal, including a status the verifier reported with a close failure.
    Refused(String),
}

impl Trust {
    /// The exact status, and for a verified image what its signer and reader returned.
    fn describe(&self) -> String {
        match self {
            Self::Verified { app_id, payload } => {
                let reader = payload.as_ref().map_or_else(
                    |error| format!("err({error})"),
                    |payload| {
                        format!(
                            "ok({}/{}/{})",
                            payload.app_id(),
                            payload.channel(),
                            payload.target()
                        )
                    },
                );
                format!("status=0x00000000 signed_app_id={app_id} reader={reader}")
            }
            Self::Rejected(status) => format!("status={status:#010x}"),
            Self::Refused(detail) => format!("status=refused({detail})"),
        }
    }
}

/// The exact `WinVerifyTrust` status in a `keld-guard` refusal, when that is all it says.
fn rejected_status(error: &WindowsAuthenticodeError) -> Option<u32> {
    let hex = error.detail().strip_prefix(WIN_VERIFY_TRUST_REJECTED)?;
    if hex.len() != 8 || error.close_failure().is_some() {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Writes `image` to a fresh directory and verifies it once with the KEL-135 policy.
fn verify_fresh_copy(image: &[u8]) -> Trust {
    let directory = tempfile::tempdir().expect("fresh copy directory");
    let path = directory.path().join("keld-host.exe");
    std::fs::write(&path, image).expect("write the fresh copy");
    let trust =
        match WindowsAuthenticodeImage::open(&path).and_then(WindowsAuthenticodeImage::verify) {
            Ok(verified) => Trust::Verified {
                app_id: verified.identity().app_id().to_owned(),
                payload: read_host_identity(verified.file()).map_err(|error| error.to_string()),
            },
            Err(error) => rejected_status(&error)
                .map_or_else(|| Trust::Refused(error.to_string()), Trust::Rejected),
        };
    // The verified handle shares only reads; it is dropped above, so removal is observed.
    directory.close().expect("remove the fresh copy");
    trust
}

/// A fresh copy of `image` whose byte at `offset` is XOR `0x01`.
fn flipped(image: &[u8], offset: usize) -> Vec<u8> {
    let mut copy = image.to_vec();
    copy[offset] ^= 0x01;
    copy
}

/// The AC8 falsifier oracle: exactly `TRUST_E_BAD_DIGEST`, and nothing else, passes.
fn bad_digest_row(trust: &Trust) -> Result<(), String> {
    match trust {
        Trust::Rejected(TRUST_E_BAD_DIGEST) => Ok(()),
        other => Err(format!(
            "expected {TRUST_E_BAD_DIGEST:#010x}, observed {other:?}"
        )),
    }
}

/// The writer's output signed once verifies and decodes; each covered flip is exactly
/// `TRUST_E_BAD_DIGEST`; a `CheckSum` flip still verifies.
#[test]
#[ignore = "needs KELD_PACK_REAL_HOST and the operator-signed KELD_PACK_SIGNED_HOST"]
fn ac8_signed_container_coverage_falsifier() {
    let host = std::fs::read(env_path("KELD_PACK_REAL_HOST")).expect("read the release host");
    let signed = std::fs::read(env_path("KELD_PACK_SIGNED_HOST")).expect("read the signed host");
    let payload = fixture_payload();
    let embedded = embed_host_identity(&host, &payload).expect("the release host is admissible");

    // The signed image is the writer's output signed once: the attribute certificate table
    // starts at the container's raw end and ends at end of file, and every byte before it
    // equals the writer's output except the CheckSum and the Certificate Table entry.
    let nt = u32_at(&signed, 0x3C);
    let optional = nt + 24;
    let checksum = optional + 64;
    let certificate_entry = optional + 144;
    let certificate = u32_at(&signed, certificate_entry);
    let certificate_bytes = u32_at(&signed, certificate_entry + 4);
    assert_eq!(certificate, embedded.len(), "certificate table start");
    assert_eq!(
        certificate + certificate_bytes,
        signed.len(),
        "certificate table end"
    );
    let signer_owned = [
        checksum..checksum + 4,
        certificate_entry..certificate_entry + 8,
    ];
    for (offset, (written, signed_byte)) in embedded.iter().zip(&signed).enumerate() {
        assert!(
            written == signed_byte || signer_owned.iter().any(|range| range.contains(&offset)),
            "the signer changed byte {offset:#x} outside the CheckSum and Certificate Table entry"
        );
    }

    // Falsifier offsets from the PE Format layout of the signed image, not from the writer.
    let sections = u16_at(&signed, nt + 6);
    let header = optional + 240 + (sections - 1) * 40;
    assert_eq!(
        &signed[header..header + 8],
        b".keldeai",
        "last section header"
    );
    let length = u32_at(&signed, header + 8);
    let raw_bytes = u32_at(&signed, header + 16);
    let pointer = u32_at(&signed, header + 20);
    assert!(raw_bytes > length, "the container has padding");
    let falsifiers = [
        ("payload-byte", pointer),
        ("padding-byte", pointer + raw_bytes - 1),
        ("virtual-size-low-byte", header + 8),
    ];

    let intact = verify_fresh_copy(&signed);
    let unsigned = verify_fresh_copy(&embedded);
    let rows: Vec<(&str, usize, Trust)> = falsifiers
        .iter()
        .map(|&(row, offset)| (row, offset, verify_fresh_copy(&flipped(&signed, offset))))
        .collect();
    let checksum_control = verify_fresh_copy(&flipped(&signed, checksum));

    println!(
        "KELD_PACK_AC8 input_sha256={} embedded_sha256={} signed_sha256={} signed_bytes={} \
         signed_checksum={:#010x} certificate_table={certificate:#x}+{certificate_bytes:#x} \
         container_header={header:#x} container_raw={pointer:#x}+{raw_bytes:#x} \
         payload_bytes={length}",
        sha256_hex(&host),
        sha256_hex(&embedded),
        sha256_hex(&signed),
        signed.len(),
        u32_at(&signed, checksum)
    );
    println!("KELD_PACK_AC8 row=intact {}", intact.describe());
    println!("KELD_PACK_AC8 row=unsigned-control {}", unsigned.describe());
    for (row, offset, trust) in &rows {
        println!(
            "KELD_PACK_AC8 row={row} offset={offset:#x} xor=0x01 {}",
            trust.describe()
        );
    }
    println!(
        "KELD_PACK_AC8 row=checksum-control offset={checksum:#x} xor=0x01 {}",
        checksum_control.describe()
    );

    let verified = Trust::Verified {
        app_id: APP_ID.to_owned(),
        payload: Ok(payload),
    };
    assert_eq!(intact, verified, "the intact signed image");
    // Negative controls for the oracle: an unflipped image and another non-zero status
    // must each fail a falsifier row.
    assert!(bad_digest_row(&intact).is_err(), "an unflipped row passed");
    assert_eq!(
        unsigned,
        Trust::Rejected(TRUST_E_NOSIGNATURE),
        "the unsigned control"
    );
    assert!(
        bad_digest_row(&unsigned).is_err(),
        "another status passed a row"
    );
    for (row, offset, trust) in &rows {
        if let Err(failure) = bad_digest_row(trust) {
            panic!("AC8 row {row} at {offset:#x}: {failure}");
        }
    }
    assert_eq!(checksum_control, verified, "the CheckSum control");
}
