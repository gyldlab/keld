//! D-45 callsite deletion oracle. These are Rust libtest cases, not a CI script.
//!
//! T-82 has no authenticated D-WIRE membership or native EKU issuer. No signed
//! positive v2 route can execute yet. These tests guard the currently present
//! production callsites against deletion or an inert-text replacement. They
//! cannot replace a real Windows signed-Host, revocation or CLOSE test.

use super::d45_source_lex::{compact, function_body, hide_comments_and_literals, hide_inert_text};
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
    let raw_guard = guard;
    let macro_visible_guard = hide_comments_and_literals(guard)?;
    let guard = hide_inert_text(guard)?;
    let role = hide_inert_text(role)?;
    check_retained_ownership(&guard)?;
    check_wintrust_verify_close(&guard)?;
    check_primary_leaf_scope_domain(raw_guard, &guard)?;
    check_primary_leaf_certificate_abi(&guard, &macro_visible_guard)?;
    check_native_issuer_policy(&guard, &macro_visible_guard, &role)
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
const D45_PRIMARY_LEAF_IDENTITY_TAIL: &str = concat!(
    "ifcertificate_ref.dwError!=0||certificate_ref.pCert.is_null()",
    "{returnErr(windows_identity_error(,,));}",
    "letcertificate_context=unsafe{&*certificate_ref.pCert};",
    "ifcertificate_context.pCertInfo.is_null()",
    "{returnErr(windows_identity_error(,,));}",
    "letcertificate_info=unsafe{&*certificate_context.pCertInfo};",
    "letspki_der=windows_encoded_spki(&certificate_info.SubjectPublicKeyInfo,)?;",
    "letmutpublisher_hasher=Sha256::new();",
    "publisher_hasher.update(WINDOWS_PUBLISHER_SCOPE_DOMAIN);",
    "publisher_hasher.update(&spki_der);",
    "letpublisher_digest=publisher_hasher.finalize();",
    "letmutpublisher_scope=[0_u8;32];",
    "publisher_scope.copy_from_slice(&publisher_digest);",
    "Ok(publisher_scope)",
);

fn check_primary_leaf_scope_domain(raw: &str, visible: &str) -> Result<(), &'static str> {
    let refusal = "D-45 publisher scope domain separator";
    let marker = "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = ";
    let expected =
        "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"keld.publisher.windows/v1\\0\";";
    // Only the sole declaration and its original signer-hash use may name
    // this active identity domain. A cfg(test) const plus cfg(not(test))
    // replacement static would otherwise preserve this test's raw slice.
    count(visible, "WINDOWS_PUBLISHER_SCOPE_DOMAIN", 2, refusal)?;
    count(visible, marker, 1, refusal)?;
    let at = visible.find(marker).ok_or(refusal)?;
    // The preceding *specified* source item must be immediately adjacent
    // in the live token stream. Searching only for the last semicolon falsely
    // accepts an inserted macro definition/invocation with its own semicolon,
    // even when that macro generates a production-only identity override.
    let visible_compact = compact(visible);
    let (_, before_domain) = visible_compact
        .split_once("constWINDOWS_SIGNED_PROGRAM_NAME_MAX_UNITS:usize=WINDOWS_SIGNED_APP_ID_PREFIX.len()+255;")
        .ok_or(refusal)?;
    if !before_domain.starts_with("constWINDOWS_PUBLISHER_SCOPE_DOMAIN:&[u8]=") {
        return Err(refusal);
    }
    if raw.get(at..at + expected.len()) != Some(expected) {
        return Err(refusal);
    }
    Ok(())
}

fn check_primary_leaf_certificate_abi(
    guard: &str,
    macro_visible_guard: &str,
) -> Result<(), &'static str> {
    let leaf = method(guard, "windows_publisher_scope_from_signer")?;
    let refusal = "D-45 primary leaf provider ABI guard before field access";
    // One actual function owner, not a test-only fake followed by a cfg-hidden
    // production alternative (including the Rust raw-identifier spelling).
    let live_source = compact(guard);
    let declaration = "fnwindows_publisher_scope_from_signer(";
    count(&live_source, declaration, 1, refusal)?;
    if live_source.contains("fnr#windows_publisher_scope_from_signer(") {
        return Err(refusal);
    }
    // An approved original method must be unconditionally compiled; a
    // cfg(test) shadow of a macro-generated production issuer is not valid.
    let (preceding, _) = live_source.split_once(declaration).ok_or(refusal)?;
    let (_, attributes) = preceding.rsplit_once('}').ok_or(refusal)?;
    if attributes != "#[allow(unsafe_code)]" {
        return Err(refusal);
    }
    // Unlike the ordinary source-contract mask, do NOT hide live macro
    // invocations here: a macro can expand to an early Ok(forged scope).
    // Both masks keep byte offsets unchanged, so this selects the same method.
    let method_at = guard
        .find("fn windows_publisher_scope_from_signer(")
        .ok_or(refusal)?;
    let macro_visible_method = function_body(
        macro_visible_guard.get(method_at..).ok_or(refusal)?,
        "windows_publisher_scope_from_signer",
    )?;
    for (index, &byte) in macro_visible_method.as_bytes().iter().enumerate() {
        if byte == b'!' && macro_visible_method.as_bytes().get(index + 1) != Some(&b'=') {
            return Err(refusal);
        }
    }
    count(&leaf, D45_PRIMARY_LEAF_CALLSITE, 1, refusal)?;
    let (prefix, _) = leaf.split_once(D45_PRIMARY_LEAF_CALLSITE).ok_or(refusal)?;
    // The lexical mask already removes strings, comments and inert macro trees.
    // Freeze the executable prefix, not only brace depth. A conditionally
    // reachable early Ok() can otherwise bypass an intact ABI guard and forge
    // the returned publisher scope while passing an ordered-token test.
    if prefix != D45_PRIMARY_LEAF_ENTRY_PREFIX {
        return Err(refusal);
    }
    // The verified leaf certificate must actually contribute the returned
    // publisher scope. Reject conditional early Ok(), shadowed SPKI and digest
    // substitution after a correct ABI guard. No unexpected live tail allowed.
    let after_abi = leaf
        .strip_prefix(D45_PRIMARY_LEAF_ENTRY_PREFIX)
        .and_then(|rest| rest.strip_prefix(D45_PRIMARY_LEAF_CALLSITE))
        .ok_or(refusal)?;
    if after_abi != D45_PRIMARY_LEAF_IDENTITY_TAIL {
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

#[test]
fn conditional_early_success_after_primary_leaf_abi_guard_fails_source_contract() {
    let anchor = "let certificate_ref = unsafe { &*certificate };";
    assert_eq!(
        GUARD.matches(anchor).count(),
        1,
        "stale leaf identity anchor"
    );
    let bypass = GUARD.replacen(
        anchor,
        concat!(
            "let certificate_ref = unsafe { &*certificate };\n",
            "    if certificate_ref.dwError == 0 && !certificate_ref.pCert.is_null() {\n",
            "        return Ok([0_u8; 32]);\n",
            "    }"
        ),
        1,
    );
    assert_eq!(
        contract(&bypass, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "an early successful return after ABI check can forge publisher scope without hashing original SPKI"
    );
    let harmless = GUARD.replacen(
        anchor,
        "let certificate_ref = unsafe { &*certificate };\n    /* an inert comment before checking the native leaf */",
        1,
    );
    assert_eq!(
        contract(&harmless, ROLE),
        Ok(()),
        "inert comments after ABI guard must not turn the identity source contract red"
    );
}

#[test]
fn primary_leaf_publisher_scope_digest_substitution_fails_source_contract() {
    let original = "publisher_scope.copy_from_slice(&publisher_digest);";
    assert_eq!(GUARD.matches(original).count(), 1, "stale digest anchor");
    let forged = GUARD.replacen(original, "publisher_scope.copy_from_slice(&[0_u8; 32]);", 1);
    assert_eq!(
        contract(&forged, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "publisher scope must use the native original leaf SPKI digest"
    );
}

#[test]
fn publisher_scope_domain_separation_removal_fails_source_contract() {
    let original =
        "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"keld.publisher.windows/v1\\0\";";
    assert_eq!(GUARD.matches(original).count(), 1, "stale identity domain");
    let changed = GUARD.replacen(
        original,
        "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"\";",
        1,
    );
    assert_eq!(
        contract(&changed, ROLE),
        Err("D-45 publisher scope domain separator"),
        "the signed leaf SPKI digest must retain KEL-135 publisher domain separation"
    );
    let harmless = GUARD.replacen(
        original,
        "/* inert source note */\nconst WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"keld.publisher.windows/v1\\0\";",
        1,
    );
    assert_eq!(contract(&harmless, ROLE), Ok(()));
}

#[test]
fn cfg_shadowed_publisher_scope_domain_fails_source_contract() {
    let original =
        "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"keld.publisher.windows/v1\\0\";";
    assert_eq!(GUARD.matches(original).count(), 1, "stale domain owner");
    let shadow = GUARD.replacen(
        original,
        concat!(
            "#[cfg(test)]\n",
            "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"keld.publisher.windows/v1\\0\";\n",
            "#[cfg(not(test))]\n",
            "static WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"\";"
        ),
        1,
    );
    assert_eq!(
        contract(&shadow, ROLE),
        Err("D-45 publisher scope domain separator"),
        "a cfg-selected production static cannot override the signed leaf identity domain"
    );
    let alternate_macro = GUARD.replacen(
        original,
        concat!(
            "macro_rules! forge_publisher_domain {\n",
            "    () => { static WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"\"; };\n",
            "}\n",
            "#[cfg(not(test))] forge_publisher_domain!();\n",
            "const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b\"keld.publisher.windows/v1\\0\";"
        ),
        1,
    );
    assert_eq!(
        contract(&alternate_macro, ROLE),
        Err("D-45 publisher scope domain separator"),
        "a macro-generated alternate domain cannot disappear from the source census"
    );
}

#[test]
fn cfg_shadowed_primary_leaf_publisher_owner_fails_source_contract() {
    let owner = "fn windows_publisher_scope_from_signer(";
    assert_eq!(GUARD.matches(owner).count(), 1, "stale primary owner");
    let test_only = GUARD.replacen(
        owner,
        "#[cfg(test)]\nfn windows_publisher_scope_from_signer(",
        1,
    );
    let alternate = concat!(
        "\n#[cfg(not(test))]\n",
        "fn windows_publisher_scope_from_signer(",
        "_signer: *mut CRYPT_PROVIDER_SGNR)",
        " -> Result<[u8; 32], WindowsAuthenticodeError> {",
        " Ok([0_u8; 32])",
        "}\n"
    );
    for production_owner in [
        alternate.to_owned(),
        alternate.replace(
            "fn windows_publisher_scope_from_signer(",
            "fn r#windows_publisher_scope_from_signer(",
        ),
    ] {
        let duplicate = format!("{test_only}{production_owner}");
        assert_eq!(
            contract(&duplicate, ROLE),
            Err("D-45 primary leaf provider ABI guard before field access"),
            "a cfg-hidden forged release owner cannot hide behind the genuine test-only function"
        );
    }
}

#[test]
fn active_macro_in_primary_leaf_publisher_owner_fails_source_contract() {
    let anchor = "let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };";
    assert_eq!(
        GUARD.matches(anchor).count(),
        1,
        "stale signer owner anchor"
    );
    let injected = GUARD.replacen(
        anchor,
        "forge_publisher_scope!();\n    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };",
        1,
    );
    let forged = format!(
        "{}{}",
        "macro_rules! forge_publisher_scope { () => { return Ok([0_u8; 32]); } }\n", injected
    );
    assert_eq!(
        contract(&forged, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "an active macro invocation cannot disappear from the verified publisher-scope body"
    );
}

#[test]
fn cfg_generated_alternate_primary_leaf_publisher_owner_fails_source_contract() {
    let owner = "fn windows_publisher_scope_from_signer(";
    assert_eq!(GUARD.matches(owner).count(), 1, "stale original owner");
    let good_only_in_tests = GUARD.replacen(
        owner,
        "#[cfg(test)]\nfn windows_publisher_scope_from_signer(",
        1,
    );
    let forged_release = concat!(
        "\n#[cfg(not(test))]\n",
        "macro_rules! forged_release_publisher {\n",
        "    () => {\n",
        "        fn windows_publisher_scope_from_signer(",
        "_signer: *mut CRYPT_PROVIDER_SGNR)",
        " -> Result<[u8; 32], WindowsAuthenticodeError> {\n",
        "            Ok([0_u8; 32])\n",
        "        }\n",
        "    };\n",
        "}\n",
        "#[cfg(not(test))]\n",
        "forged_release_publisher!();\n",
    );
    let alternate = format!("{good_only_in_tests}{forged_release}");
    assert_eq!(
        contract(&alternate, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "test-only original owner cannot conceal a macro-generated forged release signer"
    );
}

#[test]
fn cfg_before_unsafe_attribute_cannot_hide_original_leaf_owner() {
    let original = "#[allow(unsafe_code)] // bounded read and DER encoding of the live WinTrust leaf certificate";
    assert_eq!(
        GUARD.matches(original).count(),
        1,
        "stale native owner attributes"
    );
    let hidden = GUARD.replacen(
        original,
        "#[cfg(test)]\n#[allow(unsafe_code)] // bounded read and DER encoding of the live WinTrust leaf certificate",
        1,
    );
    assert_eq!(
        contract(&hidden, ROLE),
        Err("D-45 primary leaf provider ABI guard before field access"),
        "an earlier cfg(test) attribute must not hide the real release signer even if the last attribute is unchanged"
    );
}

const D45_VERIFIED_OWNER_REFUSAL: &str = "D-45 verified-state owner and caller";

// Keep the verified-state extractor and its VERIFY/CLOSE caller as single,
// unconditional Rust function items. A genuine #[cfg(test)] first definition
// followed by a forged release definition otherwise satisfies method(), which
// extracts only the first matching body.
fn require_unique_unconditional_owner(
    guard: &str,
    name: &str,
    required_attributes: &str,
) -> Result<(), &'static str> {
    let signature = format!("fn{name}(");
    count(guard, &signature, 1, D45_VERIFIED_OWNER_REFUSAL)?;
    if guard.contains(&format!("fnr#{name}(")) {
        return Err(D45_VERIFIED_OWNER_REFUSAL);
    }
    let (preceding, _) = guard
        .split_once(&signature)
        .ok_or(D45_VERIFIED_OWNER_REFUSAL)?;
    let (_, attributes) = preceding
        .rsplit_once('}')
        .ok_or(D45_VERIFIED_OWNER_REFUSAL)?;
    if attributes != required_attributes {
        return Err(D45_VERIFIED_OWNER_REFUSAL);
    }
    Ok(())
}

// The regular source mask intentionally hides macro token trees. Read the
// second, offset-preserving mask at the already selected function owner so a
// block-scoped macro generating a forged local verifier cannot disappear.
fn visible_owner_body(
    guard: &str,
    macro_visible_guard: &str,
    name: &str,
) -> Result<String, &'static str> {
    let signature = format!("fn {name}(");
    let at = guard.find(&signature).ok_or(D45_VERIFIED_OWNER_REFUSAL)?;
    let source = macro_visible_guard
        .get(at..)
        .ok_or(D45_VERIFIED_OWNER_REFUSAL)?;
    function_body(source, name)
        .map(str::to_owned)
        .map_err(|_| D45_VERIFIED_OWNER_REFUSAL)
}

fn d45_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

fn d45_ident_at(source: &str, at: usize) -> (&str, usize) {
    let bytes = source.as_bytes();
    let start = if bytes.get(at..at + 2) == Some(b"r#") {
        at + 2
    } else {
        at
    };
    let mut end = start;
    while bytes.get(end).is_some_and(|&byte| d45_ident_byte(byte)) {
        end += 1;
    }
    (&source[start..end], end)
}

fn d45_skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn d45_macro_tree_end(bytes: &[u8], opening: usize) -> Result<usize, &'static str> {
    let closing = |byte| match byte {
        b'(' => Some(b')'),
        b'[' => Some(b']'),
        b'{' => Some(b'}'),
        _ => None,
    };
    let mut stack = vec![
        bytes
            .get(opening)
            .copied()
            .and_then(closing)
            .ok_or(D45_VERIFIED_OWNER_REFUSAL)?,
    ];
    for (at, &byte) in bytes.iter().enumerate().skip(opening + 1) {
        if let Some(close) = closing(byte) {
            stack.push(close);
        } else if matches!(byte, b')' | b']' | b'}') {
            if stack.pop() != Some(byte) {
                return Err(D45_VERIFIED_OWNER_REFUSAL);
            }
            if stack.is_empty() {
                return Ok(at + 1);
            }
        }
    }
    Err(D45_VERIFIED_OWNER_REFUSAL)
}

// Read *active* tokens from the ordinary mask: comments, strings and macro
// arguments are already blank. A module import affects both release owners,
// and a local import can shadow matches!/format! without changing either call.
fn d45_import_binds_identity_macro(import: &str, allowed: &[&str]) -> bool {
    let bytes = import.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'*' {
            // A glob can introduce any of the approved macro spellings.
            return true;
        }
        if !d45_ident_byte(bytes[at]) {
            at += 1;
            continue;
        }
        let (name, end) = d45_ident_at(import, at);
        let after = d45_skip_space(bytes, end);
        if allowed.contains(&name) && matches!(bytes.get(after), None | Some(b',' | b'}' | b';')) {
            return true;
        }
        at = end.max(at + 1);
    }
    false
}

// Check attributes in the macro-hidden mask: an inert stringify! or an unused
// macro_rules! body cannot grant #[macro_use], but a release-only cfg_attr can.
fn d45_reject_macro_use_attributes(source: &str) -> Result<(), &'static str> {
    let bytes = source.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'#' {
            at += 1;
            continue;
        }
        // Rust permits whitespace (including masked comments) between #,
        // the optional inner-attribute !, and [. Do not let # [macro_use]
        // silently import a release-only replacement for matches!.
        let after_hash = d45_skip_space(bytes, at + 1);
        let opening = if bytes.get(after_hash) == Some(&b'!') {
            d45_skip_space(bytes, after_hash + 1)
        } else {
            after_hash
        };
        let opening = (bytes.get(opening) == Some(&b'[')).then_some(opening);
        if let Some(opening) = opening {
            let end = d45_macro_tree_end(bytes, opening)?;
            let contents = &source[opening + 1..end - 1];
            let attribute = contents.as_bytes();
            if contents.match_indices("macro_use").any(|(pos, _)| {
                (pos == 0 || !d45_ident_byte(attribute[pos - 1]))
                    && attribute
                        .get(pos + "macro_use".len())
                        .is_none_or(|&byte| !d45_ident_byte(byte))
            }) {
                return Err(D45_VERIFIED_OWNER_REFUSAL);
            }
            at = end;
        } else {
            at += 1;
        }
    }
    Ok(())
}

fn d45_reject_active_macro_bindings(
    source: &str,
    allowed: &[&str],
    module_level_only: bool,
) -> Result<(), &'static str> {
    let bytes = source.as_bytes();
    let mut at = 0;
    let mut depth = 0_usize;
    while at < bytes.len() {
        if bytes[at] == b'{' {
            depth += 1;
        } else if bytes[at] == b'}' {
            depth = depth.saturating_sub(1);
        }
        if (module_level_only && depth != 0) || !bytes[at].is_ascii_alphabetic() {
            at += 1;
            continue;
        }
        if at > 0 && d45_ident_byte(bytes[at - 1]) {
            at += 1;
            continue;
        }
        if bytes.get(at..at + 2) == Some(b"r#") {
            // r#use is an ordinary identifier, never an import keyword.
            // Consume the full raw identifier before scanning its suffix.
            let (_, end) = d45_ident_at(source, at);
            at = end.max(at + 2);
            continue;
        }
        if at >= 2 && bytes.get(at - 2..at) == Some(b"r#") {
            // A raw identifier r#use is not the Rust use keyword.
            at += 1;
            continue;
        }
        let (word, end) = d45_ident_at(source, at);
        if word == "use" {
            let semicolon = bytes[end..]
                .iter()
                .position(|&byte| byte == b';')
                .map(|offset| end + offset)
                .ok_or(D45_VERIFIED_OWNER_REFUSAL)?;
            if d45_import_binds_identity_macro(&source[end..=semicolon], allowed) {
                return Err(D45_VERIFIED_OWNER_REFUSAL);
            }
            // A grouped import's braces do not change the surrounding scope.
            at = semicolon + 1;
            continue;
        }
        if word == "macro_rules" {
            let bang = d45_skip_space(bytes, end);
            if bytes.get(bang) == Some(&b'!') {
                let name_at = d45_skip_space(bytes, bang + 1);
                let (name, _) = d45_ident_at(source, name_at);
                if allowed.contains(&name) {
                    return Err(D45_VERIFIED_OWNER_REFUSAL);
                }
            }
        }
        at = end.max(at + 1);
    }
    Ok(())
}

fn d45_reject_module_macro_declarations(source: &str) -> Result<(), &'static str> {
    let bytes = source.as_bytes();
    let mut depth = 0_usize;
    let mut parentheses = 0_usize;
    let mut brackets = 0_usize;
    let mut test_probe_macros = 0_usize;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'{' {
            depth += 1;
        } else if bytes[at] == b'}' {
            depth = depth.saturating_sub(1);
        } else if bytes[at] == b'(' {
            parentheses += 1;
        } else if bytes[at] == b')' {
            parentheses = parentheses.saturating_sub(1);
        } else if bytes[at] == b'[' {
            brackets += 1;
        } else if bytes[at] == b']' {
            brackets = brackets.saturating_sub(1);
        }
        if depth == 0
            && parentheses == 0
            && brackets == 0
            && bytes[at..].starts_with(b"macro_rules")
        {
            let end = at + "macro_rules".len();
            if (at == 0 || !d45_ident_byte(bytes[at - 1]))
                && bytes.get(end).is_none_or(|&byte| !d45_ident_byte(byte))
            {
                let bang = d45_skip_space(bytes, end);
                if bytes.get(bang) == Some(&b'!') {
                    let name_at = d45_skip_space(bytes, bang + 1);
                    let (name, name_end) = d45_ident_at(source, name_at);
                    if ["matches", "format", "stringify", "thread_local"].contains(&name) {
                        return Err(D45_VERIFIED_OWNER_REFUSAL);
                    }
                    let opening = d45_skip_space(bytes, name_end);
                    at = d45_macro_tree_end(bytes, opening)?;
                    continue;
                }
            }
        }
        if depth == 0 && parentheses == 0 && brackets == 0 && bytes[at] == b'!' {
            // A crate inner attribute #![...] is not a macro call.
            if at > 0 && bytes[at - 1] == b'#' {
                at += 1;
                continue;
            }
            let opening = d45_skip_space(bytes, at + 1);
            if matches!(bytes.get(opening), Some(b'(' | b'[' | b'{')) {
                let mut end = at;
                while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                    end -= 1;
                }
                let mut start = end;
                while start > 0 && d45_ident_byte(bytes[start - 1]) {
                    start -= 1;
                }
                if start >= 2 && bytes.get(start - 2..start) == Some(b"r#") {
                    start -= 2;
                }
                let name = source[start..end].trim_start_matches("r#");
                if name.is_empty() {
                    // Module-level const _: bool = !(false) uses unary !,
                    // not a macro invocation.
                    at += 1;
                    continue;
                }
                // The single original std thread_local! test probe is inert
                // in shipping code. All other module item macro expansions
                // could inject a release-only import, owner or return path.
                if name != "thread_local"
                    || test_probe_macros != 0
                    || !source[..start].trim_end().ends_with("#[cfg(test)]")
                {
                    return Err(D45_VERIFIED_OWNER_REFUSAL);
                }
                let macro_end = d45_macro_tree_end(bytes, opening)?;
                if !source[opening..macro_end].contains("WINDOWS_D45_TEST_PROBE") {
                    return Err(D45_VERIFIED_OWNER_REFUSAL);
                }
                test_probe_macros += 1;
                at = macro_end;
                continue;
            }
        }
        at += 1;
    }
    Ok(())
}

// The comment/literal mask keeps live macro invocations visible. Skip only
// non-executing token trees (macro_rules! definitions and stringify! input).
// In particular format!(forged!()) still contains an executable forged! call.
fn require_expected_identity_macros(body: &str, allowed: &[&str]) -> Result<(), &'static str> {
    let bytes = body.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'!' {
            at += 1;
            continue;
        }
        let opening = d45_skip_space(bytes, at + 1);
        let has_opening = matches!(bytes.get(opening), Some(b'(' | b'[' | b'{'));
        let mut end = at;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && d45_ident_byte(bytes[start - 1]) {
            start -= 1;
        }
        if start >= 2 && bytes.get(start - 2..start) == Some(b"r#") {
            start -= 2;
        }
        let name = body[start..end].trim_start_matches("r#");
        if name == "macro_rules" {
            // Its body is inert until invoked. The exact declared name is
            // checked here; the module-scoped form is checked separately.
            let name_at = d45_skip_space(bytes, at + 1);
            let (declared, name_end) = d45_ident_at(body, name_at);
            if ["matches", "format", "stringify"].contains(&declared) {
                return Err(D45_VERIFIED_OWNER_REFUSAL);
            }
            let macro_open = d45_skip_space(bytes, name_end);
            at = d45_macro_tree_end(bytes, macro_open)?;
            continue;
        }
        // Unary !(value), if !(value) and != are ordinary operators.
        if !has_opening
            || name.is_empty()
            || matches!(name, "if" | "while" | "return" | "let" | "else" | "break")
        {
            at += 1;
            continue;
        }
        if start > 0 && bytes[start - 1] == b':' {
            return Err(D45_VERIFIED_OWNER_REFUSAL);
        }
        if !allowed.contains(&name) {
            return Err(D45_VERIFIED_OWNER_REFUSAL);
        }
        if name == "stringify" {
            at = d45_macro_tree_end(bytes, opening)?;
            continue;
        }
        at = opening + 1;
    }
    Ok(())
}

fn check_verified_state_owner_and_caller(
    guard: &str,
    macro_visible_guard: &str,
) -> Result<(), &'static str> {
    let live = compact(guard);
    require_unique_unconditional_owner(
        &live,
        "windows_identity_from_image_with_policy",
        "#[allow(unsafe_code,clippy::too_many_lines)]",
    )?;
    require_unique_unconditional_owner(
        &live,
        "windows_identity_from_verified_trust_state",
        "#[allow(unsafe_code)]",
    )?;
    // The only live mentions are the function declaration and the one call
    // from the WinVerifyTrust VERIFY/CLOSE owner. This also rejects local
    // bindings, imports, and cfg-selected raw-identifier alternatives.
    count(
        &live,
        "windows_identity_from_verified_trust_state",
        2,
        D45_VERIFIED_OWNER_REFUSAL,
    )?;
    let caller = method(guard, "windows_identity_from_image_with_policy")?;
    count(
        &caller,
        "windows_identity_from_verified_trust_state(&trust_data,offline)",
        1,
        D45_VERIFIED_OWNER_REFUSAL,
    )?;
    let extractor = method(guard, "windows_identity_from_verified_trust_state")?;
    let test_capture = "#[cfg(test)]windows_d45_capture_chain_for_test(signer);";
    count(&extractor, test_capture, 1, D45_VERIFIED_OWNER_REFUSAL)?;
    if caller.contains("#[cfg") || extractor.replacen(test_capture, "", 1).contains("#[cfg") {
        return Err(D45_VERIFIED_OWNER_REFUSAL);
    }

    // Calls to other macros can expand into local functions at the caller.
    // Preserve the existing harmless stringify! and dormant macro_rules!
    // controls; reject invocations outside the three known standard macros.
    let caller_visible = visible_owner_body(
        guard,
        macro_visible_guard,
        "windows_identity_from_image_with_policy",
    )?;
    let extractor_visible = visible_owner_body(
        guard,
        macro_visible_guard,
        "windows_identity_from_verified_trust_state",
    )?;
    require_expected_identity_macros(&caller_visible, &["matches", "format", "stringify"])?;
    require_expected_identity_macros(&extractor_visible, &["format", "stringify"])?;
    let identity_macros = ["matches", "format", "stringify"];
    d45_reject_macro_use_attributes(guard)?;
    d45_reject_active_macro_bindings(guard, &identity_macros, true)?;
    d45_reject_active_macro_bindings(
        function_body(guard, "windows_identity_from_image_with_policy")?,
        &identity_macros,
        false,
    )?;
    d45_reject_active_macro_bindings(
        function_body(guard, "windows_identity_from_verified_trust_state")?,
        &identity_macros,
        false,
    )?;
    d45_reject_module_macro_declarations(macro_visible_guard)?;
    Ok(())
}

fn check_native_issuer_policy(
    guard: &str,
    macro_visible_guard: &str,
    role: &str,
) -> Result<(), &'static str> {
    check_native_chain_observations(guard, macro_visible_guard)?;
    check_dwire_issuer_membership(guard, role)
}

fn check_native_chain_observations(
    guard: &str,
    macro_visible_guard: &str,
) -> Result<(), &'static str> {
    check_verified_state_owner_and_caller(guard, macro_visible_guard)?;
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
fn cfg_shadowed_verified_state_owner_fails_source_contract() {
    let original = "fn windows_identity_from_verified_trust_state(";
    assert_eq!(GUARD.matches(original).count(), 1, "stale verified owner");
    let test_only = GUARD.replacen(
        original,
        "#[cfg(test)]\nfn windows_identity_from_verified_trust_state(",
        1,
    );
    let forged_release = concat!(
        "\n#[cfg(not(test))]\n",
        "fn windows_identity_from_verified_trust_state(",
        "_trust_data: &WINTRUST_DATA, _observe_d45_chain: bool",
        ") -> Result<WindowsTrustObservation, WindowsAuthenticodeError> {\n",
        "    Ok(WindowsTrustObservation {\n",
        "        identity: WindowsAuthenticodeIdentity {\n",
        "            publisher_scope: [0_u8; 32], app_id: \"forged\".into(),\n",
        "        },\n",
        "        d45_chain_facts: None,\n",
        "    })\n",
        "}\n",
    );
    for release_owner in [
        forged_release.to_owned(),
        forged_release.replace(
            "fn windows_identity_from_verified_trust_state(",
            "fn r#windows_identity_from_verified_trust_state(",
        ),
    ] {
        assert_eq!(
            contract(&format!("{test_only}{release_owner}"), ROLE),
            Err(D45_VERIFIED_OWNER_REFUSAL),
            "a test-only genuine extractor must not conceal a forged release extractor"
        );
    }
}

#[test]
fn block_scoped_verified_state_function_shadow_fails_source_contract() {
    let callsite = "    let identity_result = validate_windows_signature_cardinality(";
    assert_eq!(GUARD.matches(callsite).count(), 1, "stale VERIFY callsite");
    let shadow = GUARD.replacen(
        callsite,
        concat!(
            "    #[cfg(not(test))]\n",
            "    fn windows_identity_from_verified_trust_state(",
            "_: &WINTRUST_DATA, _: bool",
            ") -> Result<WindowsTrustObservation, WindowsAuthenticodeError> {\n",
            "        Err(windows_identity_error(\"forged\", \"forged\"))\n",
            "    }\n",
            "    let identity_result = validate_windows_signature_cardinality("
        ),
        1,
    );
    assert_eq!(
        contract(&shadow, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "a local release-only function must not shadow the verified module owner"
    );
}

#[test]
fn block_scoped_macro_verified_state_shadow_fails_source_contract() {
    let callsite = "    let identity_result = validate_windows_signature_cardinality(";
    assert_eq!(GUARD.matches(callsite).count(), 1, "stale VERIFY callsite");
    let shadow = GUARD.replacen(
        callsite,
        concat!(
            "    #[cfg(not(test))]\n",
            "    macro_rules! forged_verified_state {\n",
            "        ($name:ident) => {\n",
            "            fn $name(_: &WINTRUST_DATA, _: bool)",
            " -> Result<WindowsTrustObservation, WindowsAuthenticodeError> {\n",
            "                Err(windows_identity_error(\"forged\", \"forged\"))\n",
            "            }\n",
            "        };\n",
            "    }\n",
            "    #[cfg(not(test))]\n",
            "    forged_verified_state!(windows_identity_from_verified_trust_state);\n",
            "    let identity_result = validate_windows_signature_cardinality("
        ),
        1,
    );
    assert_eq!(
        contract(&shadow, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "a macro may create a local release-only function invisible to the normal source mask"
    );
    let always_shadow = shadow.replace("    #[cfg(not(test))]\n", "");
    assert_ne!(always_shadow, shadow, "stale cfg shadow anchor");
    assert_eq!(
        contract(&always_shadow, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "even without cfg attributes an active local macro must fail the caller oracle"
    );
}

#[test]
fn block_scoped_standard_macro_redefinition_fails_source_contract() {
    let anchor = "let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);";
    assert_eq!(GUARD.matches(anchor).count(), 1, "stale policy owner");
    let forged = GUARD.replacen(
        anchor,
        concat!(
            "macro_rules! matches { ($($arg:tt)*) => {\n",
            "        return Err(windows_identity_error(\"forged\", \"forged\"));\n",
            "    }; }\n",
            "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);"
        ),
        1,
    );
    assert_eq!(
        contract(&forged, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "a local override of an otherwise approved macro must not bypass native VERIFY"
    );
}

#[test]
fn release_only_import_alias_to_forged_macro_fails_source_contract() {
    let preceding_owner = "fn verified_windows_identity_from_image(";
    let offline = "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);";
    assert_eq!(
        GUARD.matches(preceding_owner).count(),
        1,
        "stale preceding owner"
    );
    assert_eq!(GUARD.matches(offline).count(), 1, "stale policy callsite");
    let exported_forgery = concat!(
        "#[macro_export]\n",
        "macro_rules! d45_forge {\n",
        "    ($($input:tt)*) => {\n",
        "        return Ok(WindowsTrustObservation {\n",
        "            identity: WindowsAuthenticodeIdentity {\n",
        "                publisher_scope: [0_u8; 32], app_id: \"forged\".into(),\n",
        "            },\n",
        "            d45_chain_facts: None,\n",
        "        })\n",
        "    };\n",
        "}\n",
    );
    let forged_macro_only = GUARD.replacen(
        preceding_owner,
        &format!("{exported_forgery}{preceding_owner}"),
        1,
    );
    assert_eq!(
        contract(&forged_macro_only, ROLE),
        Ok(()),
        "an unused, differently named macro must not change the verified caller"
    );
    for prefix in [
        "    use crate::d45_forge as matches;\n",
        "    #[cfg(not(test))] use crate::d45_forge as matches;\n",
    ] {
        let local_alias = forged_macro_only.replacen(offline, &format!("{prefix}{offline}"), 1);
        assert_eq!(
            contract(&local_alias, ROLE),
            Err(D45_VERIFIED_OWNER_REFUSAL),
            "a local macro alias can fabricate Ok before WinVerifyTrust: {prefix}"
        );
        let active = hide_inert_text(&local_alias).expect("well-formed alias mutation");
        assert_eq!(
            d45_reject_active_macro_bindings(
                function_body(&active, "windows_identity_from_image_with_policy")
                    .expect("live VERIFY owner"),
                &["matches", "format", "stringify"],
                false,
            ),
            Err(D45_VERIFIED_OWNER_REFUSAL),
            "the import scanner itself must reject both active and release-only aliases"
        );
    }
    for import in [
        "use crate::d45_forge as matches;",
        "#[cfg(not(test))] use crate::d45_forge as matches;",
        "use crate::{d45_forge as r#matches};",
        "use crate::d45_forge as format;",
    ] {
        let module_alias =
            forged_macro_only.replacen(preceding_owner, &format!("{import}\n{preceding_owner}"), 1);
        assert_eq!(
            contract(&module_alias, ROLE),
            Err(D45_VERIFIED_OWNER_REFUSAL),
            "module-level macro alias must be refused: {import}"
        );
    }
}

#[test]
fn inactive_macro_import_text_and_different_binding_remain_green() {
    let offline = "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);";
    assert_eq!(GUARD.matches(offline).count(), 1, "stale policy callsite");
    let inert = GUARD.replacen(
        offline,
        concat!(
            "    /* use crate::d45_forge as matches; */\n",
            "    let _inert = stringify!(use crate::d45_forge as matches;);\n",
            "    let _nested = stringify!(unknown!());\n",
            "    let _nested_rules = stringify!(macro_rules! matches { () => {} });\n",
            "    let _unary = !(true);\n",
            "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);"
        ),
        1,
    );
    assert_eq!(
        contract(&inert, ROLE),
        Ok(()),
        "comments, inert macro arguments, nested stringify and unary ! are harmless"
    );
    let macro_owner = "fn verified_windows_identity_from_image(";
    let named_macro = GUARD.replacen(
        macro_owner,
        &format!("#[macro_export]\nmacro_rules! d45_forge {{ () => {{ false }}; }}\n{macro_owner}"),
        1,
    );
    let renamed = named_macro.replacen(
        offline,
        concat!(
            "    use crate::d45_forge as matches_helper;\n",
            "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);"
        ),
        1,
    );
    assert_eq!(
        contract(&renamed, ROLE),
        Ok(()),
        "importing an unrelated name cannot shadow the approved matches! macro"
    );
}

#[test]
fn release_only_module_macro_use_shadow_fails_source_contract() {
    let owner = "fn verified_windows_identity_from_image(";
    assert_eq!(GUARD.matches(owner).count(), 1, "stale WinTrust owner");
    let injected = GUARD.replacen(
        owner,
        concat!(
            "#[cfg(not(test))]\n",
            "#[macro_use]\n",
            "mod d45_shadow {\n",
            "    macro_rules! matches { ($($input:tt)*) => {\n",
            "        return Ok(WindowsTrustObservation {\n",
            "            identity: WindowsAuthenticodeIdentity {\n",
            "                publisher_scope: [0_u8; 32], app_id: \"forged\".into(),\n",
            "            },\n",
            "            d45_chain_facts: None,\n",
            "        })\n",
            "    }; }\n",
            "}\n",
            "fn verified_windows_identity_from_image("
        ),
        1,
    );
    assert_eq!(
        contract(&injected, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "release-only #[macro_use] must not bind a forged matches! before WinTrust"
    );
    let attr_shadow = injected.replace(
        "#[cfg(not(test))]\n#[macro_use]\n",
        "#[cfg_attr(not(test), macro_use)]\n",
    );
    assert_eq!(
        contract(&attr_shadow, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "cfg_attr must not create a release-only macro import"
    );
    let spaced_shadow = injected.replacen("#[macro_use]", "# [macro_use]", 1);
    assert_eq!(
        contract(&spaced_shadow, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "Rust-valid whitespace between # and [ must not hide macro_use"
    );
    let spaced_cfg_attr = injected.replace(
        "#[cfg(not(test))]\n#[macro_use]\n",
        "# [cfg_attr(not(test), macro_use)]\n",
    );
    assert_eq!(
        contract(&spaced_cfg_attr, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "spaced cfg_attr must not hide a release-only macro import"
    );
}

#[test]
fn release_only_module_macro_expansion_import_fails_source_contract() {
    let owner = "fn verified_windows_identity_from_image(";
    assert_eq!(GUARD.matches(owner).count(), 1, "stale WinTrust owner");
    let injected = GUARD.replacen(
        owner,
        concat!(
            "#[cfg(not(test))]\n",
            "#[macro_export]\n",
            "macro_rules! d45_forge { ($($input:tt)*) => {\n",
            "    return Err(windows_identity_error(\"forged\", \"forged\"));\n",
            "}; }\n",
            "#[cfg(not(test))]\n",
            "macro_rules! d45_import { () => {\n",
            "    use crate::d45_forge as matches;\n",
            "}; }\n",
            "#[cfg(not(test))]\n",
            "d45_import!();\n",
            "fn verified_windows_identity_from_image("
        ),
        1,
    );
    assert_eq!(
        contract(&injected, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "a release-only item macro must not expand to a hidden macro import"
    );
    let include_expansion = GUARD.replacen(
        owner,
        "#[cfg(not(test))]\ninclude!(\"unreviewed-release-owner.rs\");\nfn verified_windows_identity_from_image(",
        1,
    );
    assert_eq!(
        contract(&include_expansion, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "unknown item macro expansion must not silently extend the verified owner"
    );
}

#[test]
fn raw_keyword_identifier_and_inert_module_macro_text_remain_green() {
    let offline = "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);";
    assert_eq!(
        GUARD.matches(offline).count(),
        1,
        "stale WinTrust policy call"
    );
    let benign = GUARD.replacen(
        offline,
        concat!(
            "    let r#use = 2 * 3; let _audit_number = r#use;\n",
            "    let _inert_attr = stringify!(#[macro_use] mod bogus {});\n",
            "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);"
        ),
        1,
    );
    assert_eq!(
        contract(&benign, ROLE),
        Ok(()),
        "r#use is an identifier, multiplication is not an import glob, and stringify! is inert"
    );
}

#[test]
fn module_level_unary_negation_is_not_an_item_macro() {
    let benign =
        format!("{GUARD}\n#[cfg(not(test))]\nconst _D45_AUDIT_NO_MACRO: bool = !(false);\n");
    assert_eq!(
        contract(&benign, ROLE),
        Ok(()),
        "ordinary unary ! in a module-level const must not appear to be a macro"
    );
}

#[test]
fn exact_macro_names_and_nested_calls_are_distinguished() {
    let preceding_owner = "fn verified_windows_identity_from_image(";
    let offline = "    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);";
    assert_eq!(
        GUARD.matches(preceding_owner).count(),
        1,
        "stale preceding owner"
    );
    assert_eq!(GUARD.matches(offline).count(), 1, "stale policy callsite");
    let helper = GUARD.replacen(
        preceding_owner,
        &format!("macro_rules!matches_helper {{ () => {{ true }}; }}\n{preceding_owner}"),
        1,
    );
    assert_eq!(
        contract(&helper, ROLE),
        Ok(()),
        "macro_rules!matches_helper must not match the exact macro_rules!matches name"
    );
    let shadow = GUARD.replacen(
        preceding_owner,
        &format!("macro_rules! matches {{ () => {{ true }}; }}\n{preceding_owner}"),
        1,
    );
    assert_eq!(
        contract(&shadow, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "module-scoped macro_rules!matches is an active name shadow"
    );
    let helper = helper.replacen(
        offline,
        &format!("    macro_rules! format_helper {{ () => {{ true }}; }}\n{offline}"),
        1,
    );
    assert_eq!(
        contract(&helper, ROLE),
        Ok(()),
        "an unused local macro definition cannot substitute the approved macro"
    );
    let active_unknown = GUARD.replacen(
        offline,
        &format!("    let _dangerous = format!(unknown!());\n{offline}"),
        1,
    );
    assert_eq!(
        contract(&active_unknown, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "format! evaluates its arguments, unlike inert stringify! arguments"
    );
}

#[test]
fn cfg_verified_caller_and_extractor_branch_fail_source_contract() {
    let caller = "fn windows_identity_from_image_with_policy(";
    assert_eq!(GUARD.matches(caller).count(), 1, "stale VERIFY owner");
    let test_only = GUARD.replacen(
        caller,
        "#[cfg(test)]\nfn windows_identity_from_image_with_policy(",
        1,
    );
    assert_eq!(
        contract(&test_only, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "the verified-state caller cannot be compiled only for tests"
    );
    let provider =
        "let provider = unsafe { WTHelperProvDataFromStateData(trust_data.hWVTStateData) };";
    assert_eq!(GUARD.matches(provider).count(), 1, "stale provider owner");
    let release_bypass = GUARD.replacen(
        provider,
        concat!(
            "#[cfg(not(test))]\n",
            "    return Err(windows_identity_error(\"forged\", \"forged\"));\n",
            "    let provider = unsafe { WTHelperProvDataFromStateData(trust_data.hWVTStateData) };"
        ),
        1,
    );
    assert_eq!(
        contract(&release_bypass, ROLE),
        Err(D45_VERIFIED_OWNER_REFUSAL),
        "a release-only early return cannot leave the good test-only provider calls as evidence"
    );
}

#[test]
fn inert_verified_owner_names_and_macro_definitions_remain_green() {
    let callsite = "    let identity_result = validate_windows_signature_cardinality(";
    assert_eq!(GUARD.matches(callsite).count(), 1, "stale VERIFY callsite");
    let harmless = GUARD.replacen(
        callsite,
        concat!(
            "    /* fn windows_identity_from_verified_trust_state() { } */\n",
            "    let _audit_note = r#\"fn windows_identity_from_verified_trust_state()\"#;\n",
            "    let _inert = stringify!(fn windows_identity_from_verified_trust_state() {});\n",
            "    macro_rules! r#dormant_verified { () => { } }\n",
            "    let identity_result = validate_windows_signature_cardinality("
        ),
        1,
    );
    assert_eq!(
        contract(&harmless, ROLE),
        Ok(()),
        "comments, literals, stringify and unused local macros are not alternate live verifier owners"
    );
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
