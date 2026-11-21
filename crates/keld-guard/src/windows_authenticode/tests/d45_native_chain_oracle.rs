//! Operator-only D-45 native chain oracle using two real foreign Authenticode signers.
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
//! Git for Windows uses an unrelated four-element chain. The locally installed
//! Git image had a valid embedded timestamped signature; the leaf is expired
//! at the present date, so independent .NET `X509Chain.Build` was also checked
//! at the leaf's NotBefore + one day (true, four certificates) for DER bytes.
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

const GIT_IMAGE_SHA256: [u8; 32] = [
    0x3b, 0x64, 0x04, 0x3b, 0x9e, 0x4d, 0xc2, 0x25, 0x22, 0x5f, 0x43, 0xff, 0x9c, 0x11, 0x73, 0xb5,
    0x74, 0x93, 0xff, 0x72, 0xe5, 0xb7, 0x29, 0xac, 0xcf, 0xae, 0x05, 0x1e, 0xb1, 0x60, 0xcc, 0xa3,
];

const GIT_TRUST_ROOT_DER_SHA256: [u8; 32] = [
    0x53, 0x67, 0xf2, 0x0c, 0x7a, 0xde, 0x0e, 0x2b, 0xca, 0x79, 0x09, 0x15, 0x05, 0x6d, 0x08, 0x6b,
    0x72, 0x0c, 0x33, 0xc1, 0xfa, 0x2a, 0x26, 0x61, 0xac, 0xf7, 0x87, 0xe3, 0x29, 0x2e, 0x12, 0x70,
];

const GIT_LEAF_ISSUER_SPKI_SHA256: [u8; 32] = [
    0x27, 0xc5, 0x6b, 0xa8, 0xbe, 0xce, 0x26, 0x36, 0x5e, 0x02, 0x6b, 0x61, 0x9f, 0x7f, 0x76, 0xc4,
    0x6b, 0xd8, 0xa5, 0xb0, 0x0c, 0x89, 0xc1, 0xad, 0x88, 0xe7, 0x19, 0x1f, 0xd9, 0x0c, 0x5a, 0x5e,
];

/// Requires the exact embedded-signed Brave executable and the real Windows
/// certificate chain. The task operator runs this selectively with
/// `--ignored --exact`; regular CI never installs roots for this fixture.
#[test]
#[ignore = "requires exact external embedded-signed Brave binary and native Windows trust"]
fn brave_embedded_signer_chain_matches_independent_root_and_issuer_oracles() {
    verify_foreign_native_chain(
        installed_foreign_fixture(
            "KELD_D45_BRAVE_CERT_ORACLE",
            "BraveSoftware/Brave-Browser/Application/brave.exe",
        ),
        BRAVE_IMAGE_SHA256,
        BRAVE_TRUST_ROOT_DER_SHA256,
        BRAVE_LEAF_ISSUER_SPKI_SHA256,
    );
}

/// The second independent embedded signature is Git for Windows, with four
/// certificates and a distinct Microsoft root and immediate issuer.
#[test]
#[ignore = "requires exact external embedded-signed Git binary and native Windows trust"]
fn git_embedded_signer_chain_matches_independent_root_and_issuer_oracles() {
    verify_foreign_native_chain(
        installed_foreign_fixture("KELD_D45_GIT_CERT_ORACLE", "Git/cmd/git.exe"),
        GIT_IMAGE_SHA256,
        GIT_TRUST_ROOT_DER_SHA256,
        GIT_LEAF_ISSUER_SPKI_SHA256,
    );
}

fn installed_foreign_fixture(env_key: &str, relative_path: &str) -> PathBuf {
    std::env::var_os(env_key).map_or_else(
        || {
            PathBuf::from(
                std::env::var_os("ProgramFiles").expect("native Windows ProgramFiles is set"),
            )
            .join(relative_path)
        },
        PathBuf::from,
    )
}

/// Own the test-thread observer through all returns and unwinding paths.
/// The live production verifier never knows this guard or its captured value.
struct WindowsD45ProbeScope;

impl WindowsD45ProbeScope {
    fn arm() -> Self {
        WINDOWS_D45_TEST_PROBE.with(|probe| {
            let mut state = probe.borrow_mut();
            assert!(
                matches!(*state, WindowsD45TestProbe::Disabled),
                "the D-45 native observation requires a fresh test-thread probe"
            );
            *state = WindowsD45TestProbe::Armed;
        });
        Self
    }

    fn take(&mut self) -> WindowsD45TestProbe {
        WINDOWS_D45_TEST_PROBE.with(|probe| {
            std::mem::replace(&mut *probe.borrow_mut(), WindowsD45TestProbe::Disabled)
        })
    }
}

impl Drop for WindowsD45ProbeScope {
    fn drop(&mut self) {
        // Never cause a second panic while unwinding an unexpected native
        // failure. No test borrow is intentionally held across verification.
        let _ = WINDOWS_D45_TEST_PROBE.try_with(|probe| {
            if let Ok(mut state) = probe.try_borrow_mut() {
                *state = WindowsD45TestProbe::Disabled;
            }
        });
    }
}

#[test]
fn native_chain_test_probe_releases_armed_state_on_unwind() {
    let failure = std::panic::catch_unwind(|| {
        let _probe = WindowsD45ProbeScope::arm();
        WINDOWS_D45_TEST_PROBE.with(|slot| {
            assert!(matches!(*slot.borrow(), WindowsD45TestProbe::Armed));
        });
        panic!("intentional test-only probe unwind");
    });
    assert!(failure.is_err(), "the fault injection must have panicked");
    WINDOWS_D45_TEST_PROBE.with(|slot| {
        assert!(
            matches!(*slot.borrow(), WindowsD45TestProbe::Disabled),
            "the probe must be disabled after unwinding"
        );
    });
}

fn verify_foreign_native_chain(
    path: PathBuf,
    expected_image_sha256: [u8; 32],
    expected_root_der_sha256: [u8; 32],
    expected_issuer_spki_sha256: [u8; 32],
) {
    let image = WindowsAuthenticodeImage::open(&path)
        .expect("open the original existing embedded-signed binary once");
    let length = image.image.metadata().expect("retained image length").len();
    assert_eq!(
        sha256_retained_windows_image(&image.image, length)
            .expect("read the same original retained executable bytes"),
        expected_image_sha256,
        "the independent signed-binary fixture changed; re-pin after reviewing its signer"
    );

    let mut probe = WindowsD45ProbeScope::arm();

    // This is the exact production VERIFY→provider read→CLOSE ceremony with
    // whole-chain revocation. The test-only probe reads chain facts before
    // the foreign signer's missing Keld app-ID is correctly refused.
    let refusal = image
        .verify()
        .expect_err("a valid foreign signer never grants Keld Host identity");
    let captured = probe.take();
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
        facts._root_certificate_sha256, expected_root_der_sha256,
        "root DER must match the independently observed selected root certificate"
    );
    assert_eq!(
        facts._leaf_issuer_spki_sha256, expected_issuer_spki_sha256,
        "immediate issuer SPKI must match independent certificate DER re-encoding"
    );
}
