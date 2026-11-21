//! Single-fault callsite edits for the D-45 source-contract regression test.
//!
//! Fields: label, old production bytes, new bytes, expected refusal, role-source
//! selector. These replacements are applied to memory only, never to Git files.

pub(super) const MUTANTS: &[(&str, &str, &str, &str, bool)] = &[
    (
        "WinTrust hFile",
        "hFile: image.as_raw_handle().cast(),",
        "hFile: std::ptr::null_mut(),",
        "WinTrust retained original hFile",
        false,
    ),
    (
        "preflight path reopen",
        "&self.image,\n            WindowsTrustPolicy::OfflinePreflight,",
        "&File::open(&self.path).unwrap(),\n            WindowsTrustPolicy::OfflinePreflight,",
        "preflight retained hFile",
        false,
    ),
    (
        "full path reopen",
        "let full = self.source.verify()?;",
        "let full = WindowsAuthenticodeImage::open(&self.source.path)?.verify()?;",
        "bound promotion reused pathname or skipped retained identity",
        false,
    ),
    (
        "full revocation removed",
        "WindowsTrustPolicy::Full => (\n            WTD_REVOKE_WHOLECHAIN,",
        "WindowsTrustPolicy::Full => (\n            WTD_REVOKE_NONE,",
        "full whole-chain revocation",
        false,
    ),
    (
        "VERIFY replaced",
        "dwStateAction: WTD_STATEACTION_VERIFY,",
        "dwStateAction: WTD_STATEACTION_CLOSE,",
        "WinTrust VERIFY state",
        false,
    ),
    (
        "failed VERIFY CLOSE removed",
        "if !trust_data.hWVTStateData.is_null() {",
        "if false {",
        "WinTrust failure state CLOSE",
        false,
    ),
    (
        "CLOSE replaced",
        "trust_data.dwStateAction = WTD_STATEACTION_CLOSE;",
        "trust_data.dwStateAction = WTD_STATEACTION_VERIFY;",
        "WinTrust must CLOSE after failure and success",
        false,
    ),
    (
        "CLOSE failure ignored",
        "(Ok(_), Err(cleanup)) => Err(cleanup),",
        "(Ok(identity), Err(_cleanup)) => Ok(identity),",
        "WinTrust successful identity CLOSE failure propagation",
        false,
    ),
    (
        "native root DER replaced",
        "root.pbCertEncoded",
        "issuer.pbCertEncoded",
        "D-45 original root DER",
        false,
    ),
    (
        "issuer SPKI digest removed",
        "Sha256::digest(&issuer_spki)",
        "Sha256::digest(root_der)",
        "D-45 immediate issuer SPKI SHA256",
        false,
    ),
    (
        "D-WIRE binding bypass",
        "if policy.wire_membership_binding.is_none() {",
        "if false {",
        "D-WIRE issuer membership gate",
        true,
    ),
    (
        "native root membership bypass",
        ".contains(&facts.root_certificate_sha256)",
        ".contains(&facts.leaf_issuer_spki_sha256)",
        "issuer policy native root match",
        true,
    ),
    (
        "native SPKI membership bypass",
        ".contains(&facts.leaf_issuer_spki_sha256)",
        ".contains(&facts.root_certificate_sha256)",
        "issuer policy native immediate SPKI match",
        true,
    ),
    (
        "destination issuer removed",
        "require_issuer(&policy.destination_issuer, facts)?;",
        "let _ = &policy.destination_issuer;",
        "destination issuer policy enforcement",
        true,
    ),
    (
        "uninhabited witness weakened",
        "enum WindowsD45DWireMembershipBinding {}",
        "enum WindowsD45DWireMembershipBinding { Fabricated }",
        "D-WIRE witness remains uninhabited",
        true,
    ),
];
