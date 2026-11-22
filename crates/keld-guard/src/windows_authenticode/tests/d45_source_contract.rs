//! D-45 callsite deletion oracle. These are Rust libtest cases, not a CI script.
//!
//! T-82 has no authenticated D-WIRE membership or native EKU issuer. No signed
//! positive v2 route can execute yet. These tests guard the currently present
//! production callsites against deletion or an inert-text replacement. They
//! cannot replace a real Windows signed-Host, revocation or CLOSE test.

use super::d45_source_lex::{compact, function_body, hide_inert_text};
use super::d45_source_mutations::MUTANTS;

const GUARD: &str = include_str!("../../windows_authenticode.rs");
const ROLE: &str = include_str!("../d45_role_policy.rs");

fn needs(body: &str, fragment: &str, failure: &'static str) -> Result<(), &'static str> {
    if body.contains(fragment) {
        Ok(())
    } else {
        Err(failure)
    }
}

fn count(
    body: &str,
    fragment: &str,
    expected: usize,
    failure: &'static str,
) -> Result<(), &'static str> {
    if body.matches(fragment).count() == expected {
        Ok(())
    } else {
        Err(failure)
    }
}

fn in_order(body: &str, fragments: &[&str], failure: &'static str) -> Result<(), &'static str> {
    let mut remaining = body;
    for fragment in fragments {
        let offset = remaining.find(fragment).ok_or(failure)?;
        remaining = &remaining[offset + fragment.len()..];
    }
    Ok(())
}

fn method(source: &str, name: &str) -> Result<String, &'static str> {
    function_body(source, name).map(compact)
}

fn contract(guard: &str, role: &str) -> Result<(), &'static str> {
    let guard = hide_inert_text(guard)?;
    let role = hide_inert_text(role)?;
    check_retained_ownership(&guard)?;
    check_wintrust_verify_close(&guard)?;
    check_primary_leaf_certificate_abi(&guard)?;
    check_native_issuer_policy(&guard, &role)
}

fn check_retained_ownership(guard: &str) -> Result<(), &'static str> {
    let opening = method(guard, "open")?;
    let preflight = method(guard, "observe_preflight")?;
    let full = method(guard, "verify")?;
    let bound = method(guard, "verify_full_bound")?;
    let identity = method(guard, "verified_windows_identity_from_image")?;
    needs(
        &opening,
        "letimage=open_windows_trust_image(&path)?;Ok(Self{path,image})",
        "Guard retained image open",
    )?;
    needs(
        &preflight,
        "windows_identity_from_image_with_policy(&self.path,&self.image,WindowsTrustPolicy::OfflinePreflight,)?",
        "preflight retained hFile",
    )?;
    needs(
        &preflight,
        "Ok(WindowsPreflightImage{source:self,",
        "preflight retained owner",
    )?;
    needs(
        &full,
        "verified_windows_identity_from_image(&self.path,&self.image)?;",
        "full verifier retained hFile",
    )?;
    needs(
        &full,
        "Ok(VerifiedWindowsImage{image:self.image,identity,})",
        "full verified owner",
    )?;
    needs(
        &identity,
        "windows_identity_from_image_with_policy(executable,image,WindowsTrustPolicy::Full)",
        "full independent policy entry",
    )?;
    in_order(
        &bound,
        &[
            "require_retained_host_source(&self.source.image,direct.expected_host_bytes,&direct.original_host_sha256,)?;",
            "letfull=self.source.verify()?;",
            "full.identity.publisher_scope!=self.observed_publisher_scope",
            "full.identity.app_id!=self.observed_app_id",
            "Ok(full)",
        ],
        "bound promotion reused pathname or skipped retained identity",
    )?;
    Ok(())
}

fn check_wintrust_verify_close(guard: &str) -> Result<(), &'static str> {
    let trust = method(guard, "windows_identity_from_image_with_policy")?;
    needs(
        &trust,
        "hFile:image.as_raw_handle().cast()",
        "WinTrust retained original hFile",
    )?;
    needs(&trust, "pFile:&rawmutfile_info", "WinTrust file pointer")?;
    needs(
        &trust,
        "dwUnionChoice:WTD_CHOICE_FILE",
        "WinTrust FILE union",
    )?;
    if trust.contains("File::open(") || trust.contains(".open(executable)") {
        return Err("WinTrust source path reopen");
    }
    needs(
        &trust,
        "WindowsTrustPolicy::Full=>(WTD_REVOKE_WHOLECHAIN,WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,)",
        "full whole-chain revocation",
    )?;
    needs(
        &trust,
        "WindowsTrustPolicy::OfflinePreflight=>(WTD_REVOKE_NONE,WTD_REVOCATION_CHECK_NONE|WTD_CACHE_ONLY_URL_RETRIEVAL,)",
        "preflight cache-only revocation",
    )?;
    needs(
        &trust,
        "fdwRevocationChecks:revocation_checks",
        "WinTrust revocation applied",
    )?;
    needs(
        &trust,
        "dwProvFlags:provider_flags",
        "WinTrust provider flags applied",
    )?;
    needs(
        &trust,
        "dwStateAction:WTD_STATEACTION_VERIFY",
        "WinTrust VERIFY state",
    )?;
    count(
        &trust,
        "WinVerifyTrust(",
        3,
        "WinTrust VERIFY/CLOSE call count",
    )?;
    count(
        &trust,
        "trust_data.dwStateAction=WTD_STATEACTION_CLOSE;",
        2,
        "WinTrust must CLOSE after failure and success",
    )?;
    needs(&trust, "iftrust_status!=0{", "WinTrust failure branch")?;
    needs(
        &trust,
        "if!trust_data.hWVTStateData.is_null(){",
        "WinTrust failure state CLOSE",
    )?;
    in_order(
        &trust,
        &[
            "letidentity_result=validate_windows_signature_cardinality(",
            ".and_then(|()|windows_identity_from_verified_trust_state(&trust_data,offline));",
            "trust_data.dwStateAction=WTD_STATEACTION_CLOSE;",
            "letclose_status=unsafe{WinVerifyTrust(",
            "match(identity_result,close_result)",
        ],
        "WinTrust successful VERIFY/extract/CLOSE order",
    )?;
    needs(
        &trust,
        "(Ok(_),Err(cleanup))=>Err(cleanup)",
        "WinTrust successful identity CLOSE failure propagation",
    )?;
    needs(
        &trust,
        "(Err(primary),Err(cleanup))=>Err(primary.with_close_failure(cleanup))",
        "WinTrust refusal CLOSE failure propagation",
    )?;
    needs(
        &trust,
        "returnErr(primary.with_close_failure(cleanup));",
        "WinTrust failed VERIFY CLOSE failure propagation",
    )?;
    Ok(())
}

// The live provider-leaf ABI check must be one uninterrupted function-entry
// sequence: reject signer error/empty chain, retain the original leaf, reject
// null, read only cbStruct, reject undersize, then access the remaining fields.
// An ordered-subsequence check can falsely accept an unreachable `if false`
// guard or an early successful return which fabricates a publisher scope.
// This is a source-shape deletion oracle, not a native trust-chain substitute.
const D45_PRIMARY_LEAF_ENTRY_PREFIX: &str = concat!(
    "letsigner_ref=unsafe{&*signer};",
    "ifsigner_ref.dwError!=0||signer_ref.csCertChain==0",
    "{returnErr(windows_identity_error(,,));}",
);
const D45_PRIMARY_LEAF_CALLSITE: &str = concat!(
    "letcertificate=unsafe{WTHelperGetProvCertFromChain(signer,0)};",
    "ifcertificate.is_null(){returnErr(windows_identity_error(,,));}",
    "letcertificate_size=unsafe{(*certificate).cbStruct};",
    "ifcertificate_size<windows_checked_abi_size::<",
    "windows_sys::Win32::Security::WinTrust::CRYPT_PROVIDER_CERT>",
    "(,)?{returnErr(windows_identity_error(,,));}",
    "letcertificate_ref=unsafe{&*certificate};",
);

fn check_primary_leaf_certificate_abi(guard: &str) -> Result<(), &'static str> {
    let leaf = method(guard, "windows_publisher_scope_from_signer")?;
    let refusal = "D-45 primary leaf provider ABI guard before field access";
    count(&leaf, D45_PRIMARY_LEAF_CALLSITE, 1, refusal)?;
    let (prefix, _) = leaf.split_once(D45_PRIMARY_LEAF_CALLSITE).ok_or(refusal)?;
    // The lexical mask already removes strings, comments and inert macro trees.
    // Freeze the executable prefix, not only brace depth. A conditionally
    // reachable early Ok() can otherwise bypass an intact ABI guard and forge
    // the returned publisher scope while passing an ordered-token test.
    if prefix != D45_PRIMARY_LEAF_ENTRY_PREFIX {
        return Err(refusal);
    }
    Ok(())
}

#[test]
fn unreachable_primary_leaf_abi_guard_fails_source_contract() {
    let before =
        "let certificate_size = unsafe { (*certificate).cbStruct };\n    if certificate_size";
    let after =
        "    }\n    // SAFETY: size and provider-owned lifetime are checked before other fields.";
    assert!(
        GUARD.contains(before) && GUARD.contains(after),
        "stale primary leaf mutation anchors"
    );
    let wrapped = GUARD.replacen(
        before,
        "let certificate_size = unsafe { (*certificate).cbStruct };\n    if false {\n        if certificate_size",
        1,
    );
    let wrapped = wrapped.replacen(
        after,
        "    }\n    }\n    // SAFETY: size and provider-owned lifetime are checked before other fields.",
        1,
    );
    assert_eq!(
        contract(&wrapped, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "a live, balanced but unreachable certificate-size check cannot satisfy the ABI source oracle"
    );
}

#[test]
fn conditional_early_success_before_primary_leaf_abi_guard_fails_source_contract() {
    let anchor = "let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };";
    assert_eq!(
        GUARD.matches(anchor).count(),
        1,
        "stale primary leaf anchor"
    );
    let bypass = GUARD.replacen(
        anchor,
        "if signer_ref.csCertChain > 0 { return Ok([0_u8; 32]); }\n    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };",
        1,
    );
    assert_eq!(
        contract(&bypass, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "an early successful return can forge publisher scope without verifying the leaf ABI"
    );
    let harmless = GUARD.replacen(anchor, "/* audit comment */\n    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };", 1);
    assert_eq!(
        contract(&harmless, ROLE),
        Ok(()),
        "inert comments cannot create a false-red ABI source contract"
    );
}

fn check_native_issuer_policy(guard: &str, role: &str) -> Result<(), &'static str> {
    check_native_chain_observations(guard)?;
    check_dwire_issuer_membership(guard, role)
}

fn check_native_chain_observations(guard: &str) -> Result<(), &'static str> {
    let verified_state = method(guard, "windows_identity_from_verified_trust_state")?;
    let chain = method(guard, "windows_d45_chain_facts_from_signer")?;
    let selected = method(guard, "windows_d45_selected_cert_contexts")?;
    let provider = method(guard, "windows_d45_match_provider_element")?;
    let hashes = method(guard, "windows_d45_hash_cert_contexts")?;
    needs(
        &verified_state,
        "WTHelperProvDataFromStateData(trust_data.hWVTStateData)",
        "live verified provider state",
    )?;
    needs(
        &verified_state,
        "WTHelperGetProvSignerFromChain(provider,0,0,0)",
        "live verified signer",
    )?;
    needs(
        &verified_state,
        "letpublisher_scope=windows_publisher_scope_from_signer(signer)?;",
        "verified signer publisher SPKI",
    )?;
    needs(
        &verified_state,
        "Some(windows_d45_chain_facts_from_signer(signer)?)",
        "preflight native D-45 chain observation",
    )?;
    in_order(
        &chain,
        &[
            "windows_d45_single_selected_chain(signer)?;",
            "windows_d45_selected_cert_contexts(signer,simple_chain,count)?;",
            "windows_d45_hash_cert_contexts(issuer,root)",
        ],
        "D-45 native chain source",
    )?;
    needs(
        &selected,
        "ifindex==1{issuer_context=Some(element.pCertContext);}",
        "D-45 immediate issuer slot",
    )?;
    needs(
        &selected,
        "ifindex+1==count{root_context=Some(element.pCertContext);}",
        "D-45 terminal root slot",
    )?;
    needs(
        &provider,
        "ifis_terminal_root&&(provider.fTrustedRoot==0||provider.fSelfSigned==0)",
        "D-45 trusted self-signed root",
    )?;
    needs(
        &hashes,
        "root.pbCertEncoded.is_null()",
        "D-45 original root DER",
    )?;
    needs(
        &hashes,
        "std::slice::from_raw_parts(root.pbCertEncoded,root_len)",
        "D-45 original root DER",
    )?;
    needs(
        &hashes,
        "windows_encoded_spki(&issuer_info.SubjectPublicKeyInfo,",
        "D-45 immediate issuer SPKI DER",
    )?;
    needs(&hashes, "Sha256::digest(root_der)", "D-45 root SHA256")?;
    needs(
        &hashes,
        "Sha256::digest(&issuer_spki)",
        "D-45 immediate issuer SPKI SHA256",
    )?;
    Ok(())
}

fn check_dwire_issuer_membership(guard: &str, role: &str) -> Result<(), &'static str> {
    let issuer = method(role, "require_issuer")?;
    let role_check = method(role, "require_d45_issuer_profile_role")?;
    // This pure predicate remains deliberately uncallable until T-82's native
    // issuer and Pack D-WIRE producers exist. Source checks cannot mint them.
    needs(
        &compact(role),
        "enumWindowsD45DWireMembershipBinding{}",
        "D-WIRE witness remains uninhabited",
    )?;
    needs(
        &compact(guard),
        "_dwire_issuer_membership:d45_role_policy::WindowsD45DWireMembershipBinding",
        "Guard requires an authenticated D-WIRE witness",
    )?;
    needs(
        &issuer,
        "ifpolicy.wire_membership_binding.is_none(){returnErr(",
        "D-WIRE issuer membership gate",
    )?;
    needs(
        &issuer,
        "if!policy.root_certificate_hashes.contains(&facts.root_certificate_sha256){returnErr(",
        "issuer policy native root match",
    )?;
    needs(
        &issuer,
        "if!policy.leaf_issuer_spki_hashes.contains(&facts.leaf_issuer_spki_sha256){returnErr(",
        "issuer policy native immediate SPKI match",
    )?;
    needs(
        &role_check,
        "require_issuer(&policy.source_issuer,facts)?;",
        "source issuer policy enforcement",
    )?;
    needs(
        &role_check,
        "require_issuer(&policy.destination_issuer,facts)?;",
        "destination issuer policy enforcement",
    )?;
    Ok(())
}

#[test]
fn live_d45_guard_source_contract_is_closed() {
    contract(GUARD, ROLE).expect("D-45 production callsite source contract");
}

#[test]
fn removed_guard_and_issuer_checks_fail_source_contract() {
    for &(label, original, replacement, expected, in_role) in MUTANTS {
        let source = if in_role { ROLE } else { GUARD };
        assert!(
            source.contains(original),
            "stale D-45 mutant anchor: {label}"
        );
        let modified = source.replacen(original, replacement, 1);
        let actual = if in_role {
            contract(GUARD, &modified)
        } else {
            contract(&modified, ROLE)
        };
        assert_eq!(
            actual,
            Err(expected),
            "{label} did not trigger its owner contract"
        );
    }
}

#[test]
fn inert_macros_and_literals_cannot_rescue_removed_calls() {
    let original = "hFile: image.as_raw_handle().cast(),";
    let removed = GUARD.replacen(original, "hFile: std::ptr::null_mut(),", 1);
    let anchor = "let mut file_info = WINTRUST_FILE_INFO {";
    let fake = concat!(
        "let _ = stringify!{ hFile: image.as_raw_handle().cast(), };\n",
        "macro_rules! dormant { () => { hFile: image.as_raw_handle().cast(), }; }\n",
        "let _ = r##\"hFile: image.as_raw_handle().cast(),\"##;\n",
        "/* nested /* inert */ hFile: image.as_raw_handle().cast(), */\n",
        "let mut file_info = WINTRUST_FILE_INFO {",
    );
    assert!(GUARD.contains(anchor), "the WinTrust FILE owner moved");
    let modified = removed.replacen(anchor, fake, 1);
    assert_eq!(
        contract(&modified, ROLE),
        Err("WinTrust retained original hFile"),
        "inactive text must not supply a verified hFile"
    );
}

#[test]
fn char_byte_char_and_inert_macro_braces_do_not_truncate_functions() {
    let anchor = "let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;";
    let inserted = concat!(
        "let _right_brace = '}'; let _byte_brace = b'}';\n",
        "let _escaped = '\\''; let _unicode = '\\u{7d}';\n",
        "let _utf8 = 'λ'; let _raw = br###\"} } }\"###;\n",
        "let _inert = stringify!([{ }]);\n",
        "macro_rules! r#dormant { () => { } }\n",
        "macro_rules! Δother { () => { } }\n",
        "let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;",
    );
    assert!(GUARD.contains(anchor));
    let with_inert_tokens = GUARD.replacen(anchor, inserted, 1);
    assert_eq!(
        contract(&with_inert_tokens, ROLE),
        Ok(()),
        "inert macro and Rust literal braces must not hide the live CLOSE path"
    );
}
