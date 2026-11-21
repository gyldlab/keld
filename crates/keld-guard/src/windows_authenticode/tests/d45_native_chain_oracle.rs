//! Operator-only D-45 native chain oracle using a real foreign Authenticode signer.
//!
//! This tests the existing pinned-image `WinVerifyTrust` VERIFY/CLOSE lifecycle.
//! The signer has no Keld app-ID and never creates a Host role, permit, or trusted
//! Keld signing authority. This test does not write the Windows trust store.
//!
//! The expected bytes were computed *independently* from a locally installed,
//! embedded-signed Brave executable on Rushil Windows (2026-11-21):
//! `Get-AuthenticodeSignature` reported Valid and `SignatureType` Authenticode;
//! .NET `X509Chain.Build` returned true with exactly three certificates.
//! Root DER SHA-256 is from `X509Chain.ChainElements[2].Certificate.RawData`.
//! Leaf issuer SPKI SHA-256 is from `ChainElements[1].Certificate.RawData`,
//! decoded and re-encoded using Python cryptography 50.0.2 (also checked by
//! independent OpenSSL). These facts are *certificate bytes*, not approval.
//! A different binary or a different selected `WinTrust` chain must fail.

use std::path::PathBuf;

use super::{
    WINDOWS_D45_TEST_PROBE, WindowsAuthenticodeImage, WindowsD45TestProbe,
    sha256_retained_windows_image,
};

const BRAVE_IMAGE_SHA256: [u8; 32] = [
    0xeb, 0x7b, 0xdd, 0x67, 0x94, 0xfe, 0xef, 0x54, 0x3c, 0x70, 0xed, 0x73, 0xdb, 0x5a, 0x40, 0x86,
    0x36, 0xb9, 0xf0, 0xb8, 0x6f, 0x69, 0x77, 0x60, 0x9e, 0x80, 0x34, 0xb1, 0xf6, 0xda, 0x93, 0x2f,
];

const BRAVE_TRUST_ROOT_DER_SHA256: [u8; 32] = [
    0x55, 0x2f, 0x7b, 0xdc, 0xf1, 0xa7, 0xaf, 0x9e, 0x6c, 0xe6, 0x72, 0x01, 0x7f, 0x4f, 0x12, 0xab,
    0xf7, 0x72, 0x40, 0xc7, 0x8e, 0x76, 0x1a, 0xc2, 0x03, 0xd1, 0xd9, 0xd2, 0x0a, 0xc8, 0x99, 0x88,
];

const BRAVE_LEAF_ISSUER_SPKI_SHA256: [u8; 32] = [
    0x44, 0x48, 0xd3, 0x01, 0xcc, 0xe6, 0x84, 0x50, 0x00, 0xda, 0x99, 0xfa, 0xf2, 0xb7, 0x0b, 0xaf,
    0x36, 0x37, 0x60, 0x1e, 0x9e, 0x1f, 0x94, 0x01, 0x37, 0x94, 0x3f, 0xe8, 0x11, 0xa9, 0xab, 0x32,
];

/// Requires the exact embedded-signed Brave executable and the real Windows
/// certificate chain. The task operator runs this selectively with
/// `--ignored --exact`; regular CI never installs roots for this fixture.
#[test]
#[ignore = "requires exact external embedded-signed Brave binary and native Windows trust"]
fn brave_embedded_signer_chain_matches_independent_root_and_issuer_oracles() {
    let path = std::env::var_os("KELD_D45_BRAVE_CERT_ORACLE").map_or_else(
        || {
            PathBuf::from(
                std::env::var_os("ProgramFiles").expect("native Windows ProgramFiles is set"),
            )
            .join("BraveSoftware/Brave-Browser/Application/brave.exe")
        },
        PathBuf::from,
    );
    let image = WindowsAuthenticodeImage::open(&path)
        .expect("open the existing Brave binary once for retained-handle observation");
    let length = image.image.metadata().expect("retained image length").len();
    assert_eq!(
        sha256_retained_windows_image(&image.image, length)
            .expect("read the same original retained executable bytes"),
        BRAVE_IMAGE_SHA256,
        "the independent signed-binary fixture changed; re-pin after reviewing its signer"
    );

    let previous = WINDOWS_D45_TEST_PROBE
        .with(|probe| std::mem::replace(&mut *probe.borrow_mut(), WindowsD45TestProbe::Armed));
    assert!(
        matches!(previous, WindowsD45TestProbe::Disabled),
        "the D-45 native observation must own a fresh test-thread probe"
    );

    // This is the exact production VERIFY→provider read→CLOSE ceremony with
    // whole-chain revocation. The test-only probe reads chain facts before
    // the foreign signer's missing Keld app-ID is correctly refused.
    let refusal = image
        .verify()
        .expect_err("a valid Brave signer never grants Keld Host identity");
    let captured = WINDOWS_D45_TEST_PROBE
        .with(|probe| std::mem::replace(&mut *probe.borrow_mut(), WindowsD45TestProbe::Disabled));
    assert!(
        matches!(
            refusal.detail(),
            "the verified Authenticode signature has no Keld app-id description"
                | "the verified signature has no authenticated Keld app-id attribute"
        ),
        "foreign signer must fail at the authenticated Keld app-ID boundary: {refusal}"
    );
    assert!(
        refusal.close_failure().is_none(),
        "WinTrust state CLOSE must succeed after foreign signer refusal: {refusal}"
    );
    let facts = match captured {
        WindowsD45TestProbe::Captured(Ok(facts)) => facts,
        WindowsD45TestProbe::Captured(Err(error)) => {
            panic!("real WinTrust chain facts refused the independent signed fixture: {error}")
        }
        WindowsD45TestProbe::Armed | WindowsD45TestProbe::Disabled => {
            panic!("test reached no real WinTrust signer chain; never count an empty oracle")
        }
    };
    assert_eq!(
        facts._root_certificate_sha256, BRAVE_TRUST_ROOT_DER_SHA256,
        "root DER must be the independently observed DigiCert Trusted Root G4"
    );
    assert_eq!(
        facts._leaf_issuer_spki_sha256, BRAVE_LEAF_ISSUER_SPKI_SHA256,
        "immediate issuer SPKI must match independent certificate DER re-encoding"
    );
}
