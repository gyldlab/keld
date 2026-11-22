//! Source-only negative controls for D-46's one-use effect boundary.
//!
//! The native issuer is not commissioned, so an executable positive issuer
//! fixture would counterfeit the missing KEL-53 repository lease. These tests
//! inspect only the real permit methods and reject single-fault mutations.
//! They cannot establish real OS identity, cross-install exclusion, or journal
//! durability; those require the commissioned native issuer and native tests.

const SOURCE: &str = include_str!("../initial_permit.rs");
const LEGACY_INITIALIZER: &str = include_str!("../initialize.rs");

// This source-only boundary check does NOT prove a token change can be
// injected on Windows. It protects the already-authorized v1 native Guard
// observation at the real KEL-53 FILE_CREATE cut until the D-42 issuer exists.
fn legacy_effect_authority_contract(source: &str) -> Result<(), &'static str> {
    let visible = hide_non_code(source)?;
    let helper_start = visible
        .find("fn require_native_baseline_authority(")
        .ok_or("native v1 token helper missing")?;
    let initializer_start = visible
        .find("fn initialize_with_authority_and_token_check(")
        .ok_or("sole KEL-53 initializer missing")?;
    if helper_start >= initializer_start {
        return Err("native v1 token helper moved out of the entry boundary");
    }
    let helper = code(&visible[helper_start..initializer_start]);
    for guard in [
        "BaselineAuthority::MachineSystem => keld_guard::require_windows_system_token()",
        "BaselineAuthority::PerUser => keld_guard::require_windows_non_system_token()",
        "BaselineAuthority::MachineUac => { keld_guard::require_windows_non_system_token()?; keld_guard::require_windows_machine_uac_owner_token()",
    ] {
        if !helper.contains(guard) {
            return Err("legacy native mode guard removed or replaced");
        }
    }
    // Seeing all three guard names does not prove they execute: an earlier
    // unconditional return would leave those strings intact. Constrain this
    // small private native helper to its current exhaustive three-way match.
    let helper_compact = helper.split_whitespace().collect::<String>();
    let native_mode_helper: &str = concat!(
        "fnrequire_native_baseline_authority(authority:BaselineAuthority)->io::Result<()>{",
        "matchauthority{",
        "BaselineAuthority::MachineSystem=>keld_guard::require_windows_system_token(),",
        "BaselineAuthority::PerUser=>keld_guard::require_windows_non_system_token(),",
        "BaselineAuthority::MachineUac=>{",
        "keld_guard::require_windows_non_system_token()?;",
        "keld_guard::require_windows_machine_uac_owner_token()",
        "}}}",
    );
    if helper_compact != native_mode_helper {
        return Err("unreviewed native v1 guard control flow");
    }
    let end = visible[initializer_start..]
        .find("\nmod publication;")
        .ok_or("KEL-53 publication boundary missing")?;
    let function = &visible[initializer_start..initializer_start + end];
    require_exact_initial_authorization_prelude(function)?;
    let initializer = code(function);
    required_order(
        &initializer,
        &[
            "token_check(authority)",
            "publication::ValidatedBaselineStageSource::open_legacy(verified, archive)?;",
            "open_roots(trust, true)",
            "require_native_baseline_authority(authority)",
            "let mut lock = acquire_bootstrap_lock(&roots)?;",
        ],
    )?;
    if initializer
        .matches("require_native_baseline_authority(authority)")
        .count()
        != 1
    {
        return Err("bootstrap recheck must be exactly one native call");
    }
    require_legacy_effect_order(&initializer)
}

// A mere ordered subsequence is insufficient: an extra FILE_CREATE can occur
// before the recheck and leave the old anchors present. This closed source
// slice permits only the observed read-only operations from the initial token
// check through the first native effect; any additional live statement fails.
fn require_legacy_effect_order(initializer: &str) -> Result<(), &'static str> {
    let compact = initializer.split_whitespace().collect::<String>();
    let prefix_before_effect: &str = concat!(
        "token_check(authority).map_err(|cause|error(,cause))?;",
        "crate::provenance::match_identity(&trust.installation,verified.installation())?;",
        "letmutretained_source=",
        "publication::ValidatedBaselineStageSource::open_legacy(verified,archive)?;",
        "letroots=open_roots(trust,true).map_err(|cause|error(,cause))?;",
        "require_native_baseline_authority(authority).map_err(|cause|error(,cause))?;",
    );
    let protected_effects: &[&str] = &[
        "letmutlock=acquire_bootstrap_lock(&roots)?;",
        "seed_activation_lock(&roots)?;",
        "publish_baseline_version(&mutretained_source,&roots,authority,&mutobserve)?;",
        "seed_baseline_pointers(verified,&roots,&mutobserve)?;",
        "commit_baseline(&roots,&mutlock,&mutobserve)?;",
    ];
    let precheck_start = compact
        .find("token_check(authority).map_err(|cause|error(,cause))?;")
        .ok_or("initial native token check missing")?;
    let first_effect = compact
        .find(protected_effects[0])
        .ok_or("KEL-53 bootstrap creation missing")?;
    if compact.get(precheck_start..first_effect) != Some(prefix_before_effect) {
        return Err("unreviewed statement or effect before native bootstrap recheck");
    }
    // The entire initializer is the scope, not just the admitted prefix.
    // Each real protected call must appear once and in the established order;
    // a duplicate before the guard cannot be hidden behind its later copy.
    let mut remaining = &compact[first_effect..];
    for &effect in protected_effects {
        if compact.matches(effect).count() != 1 {
            return Err("missing or repeated KEL-53 protected effect");
        }
        let offset = remaining
            .find(effect)
            .ok_or("KEL-53 protected effect reordered before bootstrap recheck")?;
        remaining = &remaining[offset + effect.len()..];
    }
    Ok(())
}

// Freeze the executable initializer *from its body entry*, not just a
// subsequence beginning at token_check. The only admitted earlier statements
// are the existing direct-owner and mode refusals; any additional statement
// could create filesystem state before the first native authority check.
// This lexical contract is not a Rust control-flow proof or native OS oracle.
fn require_exact_initial_authorization_prelude(function: &str) -> Result<(), &'static str> {
    const AUTHORIZED_PRELUDE: &str = r#"
    trust.require_direct_owner()?;
    if trust.installation.install_mode != authority.mode() {
        return Err(error(
            "installer mode",
            format!(
                "initializer requires explicit {} mode, found {}",
                authority.mode().as_str(),
                trust.installation.install_mode.as_str()
            ),
        ));
    }
    token_check(authority).map_err(|cause| error("installer authority", cause))?;
    "#;
    let body_start = function
        .find('{')
        .ok_or("KEL-53 initializer body missing")?
        + 1;
    let token_start = function
        .find("token_check(authority)")
        .ok_or("initial installer token observation missing")?;
    let token_end = token_start
        + function[token_start..]
            .find(';')
            .ok_or("initial token observation not terminated")?
        + 1;
    if token_start <= body_start
        || function.matches("token_check(authority)").count() != 1
        || code(&function[body_start..token_end]) != code(&hide_non_code(AUTHORIZED_PRELUDE)?)
    {
        return Err("unreviewed statement or effect before initial installer authorization");
    }
    Ok(())
}

#[test]
fn all_three_legacy_modes_recheck_native_guard_before_first_effect() {
    legacy_effect_authority_contract(LEGACY_INITIALIZER)
        .expect("KEL-53 v1 calls the actual Guard token check after source/root admission");
}

#[test]
fn native_effect_recheck_single_fault_mutations_are_refused() {
    let recheck = "require_native_baseline_authority(authority)\n        .map_err(|cause| error(\"bootstrap authority recheck\", cause))?;";
    for (label, before, after) in [
        ("recheck deleted", recheck, "let _ = authority;"),
        (
            "effect occurs before native recheck",
            "require_native_baseline_authority(authority)\n        .map_err(|cause| error(\"bootstrap authority recheck\", cause))?;\n    let mut lock = acquire_bootstrap_lock(&roots)?;",
            "let mut lock = acquire_bootstrap_lock(&roots)?;\n    require_native_baseline_authority(authority)\n        .map_err(|cause| error(\"bootstrap authority recheck\", cause))?;",
        ),
        (
            "SYSTEM guard replaced by unconditional acceptance",
            "BaselineAuthority::MachineSystem => keld_guard::require_windows_system_token(),",
            "BaselineAuthority::MachineSystem => Ok(()),",
        ),
        (
            "PerUser guard replaced by unconditional acceptance",
            "BaselineAuthority::PerUser => keld_guard::require_windows_non_system_token(),",
            "BaselineAuthority::PerUser => Ok(()),",
        ),
        (
            "UAC owner guard replaced by unconditional acceptance",
            "keld_guard::require_windows_machine_uac_owner_token()",
            "Ok(())",
        ),
        (
            "native mode helper returns before all checks",
            "fn require_native_baseline_authority(authority: BaselineAuthority) -> io::Result<()> {\n    match authority {",
            "fn require_native_baseline_authority(authority: BaselineAuthority) -> io::Result<()> {\n    return Ok(());\n    match authority {",
        ),
        (
            "activation creation replaced with inert quoted call",
            "seed_activation_lock(&roots)?;",
            "let _ = \"seed_activation_lock(&roots)?;\";",
        ),
        (
            "activation creation replaced with inert macro tokens",
            "seed_activation_lock(&roots)?;",
            "let _ = stringify!(seed_activation_lock(&roots)?;);",
        ),
    ] {
        assert_eq!(
            LEGACY_INITIALIZER.matches(before).count(),
            1,
            "{label}: anchor"
        );
        let mutant = LEGACY_INITIALIZER.replacen(before, after, 1);
        assert!(
            legacy_effect_authority_contract(&mutant).is_err(),
            "{label}: missing native first-effect authority was not detected"
        );
    }

    // Worker46's counterexample: the old subsequence check admitted this
    // additional *real* creation between read-only root admission and the
    // native guard. No journal/metadata witness can excuse such an effect.
    let root_open = "let roots =\n        open_roots(trust, true).map_err(|cause| error(\"private scaffold admission\", cause))?;";
    assert_eq!(LEGACY_INITIALIZER.matches(root_open).count(), 1);
    let early_effect = LEGACY_INITIALIZER.replacen(
        root_open,
        &format!("{root_open}\n    seed_activation_lock(&roots)?;"),
        1,
    );
    assert_eq!(
        early_effect
            .matches("seed_activation_lock(&roots)?;")
            .count(),
        2
    );
    assert!(
        legacy_effect_authority_contract(&early_effect).is_err(),
        "an early activation.lock creation must refuse even when the native recheck and later effect still exist"
    );
    let unexpected_effect = LEGACY_INITIALIZER.replacen(
        root_open,
        &format!(
            "{root_open}\n    let _ = std::fs::create_dir(trust.installation.install_root.join(\"rogue\"));"
        ),
        1,
    );
    assert!(
        legacy_effect_authority_contract(&unexpected_effect).is_err(),
        "an unlisted filesystem effect before native recheck must refuse"
    );
}

#[test]
fn pre_token_effect_fails_with_owner_mode_and_later_effect_checks_intact() {
    legacy_effect_authority_contract(LEGACY_INITIALIZER)
        .expect("unchanged v1 owner/mode checks and KEL-53 bootstrap order are accepted");
    let initial_token: &str =
        "    token_check(authority).map_err(|cause| error(\"installer authority\", cause))?;";
    assert_eq!(LEGACY_INITIALIZER.matches(initial_token).count(), 1);
    let premature_create = LEGACY_INITIALIZER.replacen(
        initial_token,
        &format!(
            "    let _ = std::fs::create_dir(&trust.installation.install_root);\n{initial_token}"
        ),
        1,
    );
    assert_eq!(
        premature_create.matches("token_check(authority)").count(),
        1,
        "the mutation must retain the original token observation"
    );
    assert!(
        legacy_effect_authority_contract(&premature_create).is_err(),
        "a real filesystem write before the first token check must fail the source oracle"
    );
    for (label, before, after) in [
        (
            "direct owner dropped",
            "    trust.require_direct_owner()?;\n",
            "",
        ),
        (
            "mode refusal replaced by unconditional result",
            "    if trust.installation.install_mode != authority.mode() {",
            "    if false {",
        ),
    ] {
        assert_eq!(
            LEGACY_INITIALIZER.matches(before).count(),
            1,
            "{label}: anchor"
        );
        let mutant = LEGACY_INITIALIZER.replacen(before, after, 1);
        assert!(
            legacy_effect_authority_contract(&mutant).is_err(),
            "{label}: owner or mode admission was bypassed before the token check"
        );
    }
}

// Keep offsets unchanged so the existing method/field slicers operate on
// actual Rust tokens. Quoted text, comments and inert macro token trees can
// contain convincing fake permit calls. Mask them before checking the calls.
// This bounded lexical check does not prove control-flow reachability or OS
// effects; native execution must prove those once an issuer exists.
fn hide_non_code(source: &str) -> Result<String, &'static str> {
    let bytes = source.as_bytes();
    let mut visible = bytes.to_vec();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let end = if bytes[at..].starts_with(b"//") {
            at = (at + 2..bytes.len())
                .find(|&index| bytes[index] == b'\n')
                .unwrap_or(bytes.len());
            Some(at)
        } else if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1_usize;
            while at < bytes.len() && depth != 0 {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
            if depth != 0 {
                return Err("unterminated D-46 source comment");
            }
            Some(at)
        } else {
            let raw_after_prefix = match bytes[at..] {
                [b'b' | b'c', b'r', ..] => Some(at + 2),
                [b'r', ..] => Some(at + 1),
                _ => None,
            };
            let mut raw = None;
            if let Some(hash_start) = raw_after_prefix {
                let mut quote = hash_start;
                while bytes.get(quote) == Some(&b'#') {
                    quote += 1;
                }
                if bytes.get(quote) == Some(&b'"') {
                    let hashes = quote - hash_start;
                    at = quote + 1;
                    while at < bytes.len() {
                        if bytes[at] == b'"'
                            && bytes
                                .get(at + 1..at + 1 + hashes)
                                .is_some_and(|suffix| suffix.iter().all(|&byte| byte == b'#'))
                        {
                            raw = Some(at + 1 + hashes);
                            break;
                        }
                        at += 1;
                    }
                    if raw.is_none() {
                        return Err("unterminated D-46 raw string");
                    }
                }
            }
            if let Some(end) = raw {
                at = end;
                Some(end)
            } else if bytes[at] == b'"' || matches!(bytes[at..], [b'b' | b'c', b'"', ..]) {
                let quote = if bytes[at] == b'"' { at } else { at + 1 };
                at = quote + 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\\' {
                        at += 1;
                    }
                    at += 1;
                }
                if at >= bytes.len() {
                    return Err("unterminated D-46 string");
                }
                at += 1;
                Some(at)
            } else {
                let quote = if bytes[at] == b'\'' {
                    Some(at)
                } else if matches!(bytes[at..], [b'b', b'\'', ..]) {
                    Some(at + 1)
                } else {
                    None
                };
                if let Some(end) = quote.and_then(|quote| char_literal_end(source, quote)) {
                    at = end;
                    Some(end)
                } else {
                    at += 1;
                    None
                }
            }
        };
        if let Some(end) = end {
            for byte in &mut visible[start..end] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
        }
    }
    hide_macro_token_trees(&mut visible)?;
    String::from_utf8(visible).map_err(|_| "invalid D-46 source encoding")
}

// Exclude the argument tokens of macro invocations: `stringify !(call)` contains
// valid Rust call tokens but never executes the call. The uninvoked definition
// `macro_rules! name { ... }` also contains inert code. Rust permits whitespace
// and comments on both sides of `!`, so mask both delimited forms. This can
// refuse a valid future source edit; it cannot manufacture positive effect proof.
fn hide_macro_token_trees(visible: &mut [u8]) -> Result<(), &'static str> {
    let mut at = 0;
    while at < visible.len() {
        if visible[at] != b'!' {
            at += 1;
            continue;
        }
        let mut open = at + 1;
        while visible.get(open).is_some_and(u8::is_ascii_whitespace) {
            open += 1;
        }
        // Macro definitions put an identifier between `!` and the body.
        // Accept raw `r#name` and conservatively treat UTF-8 bytes as name
        // constituents too, rather than leaving Unicode macro bodies visible.
        // This masks a superset of Rust identifiers; it is not a parser.
        if visible.get(open) == Some(&b'r') && visible.get(open + 1) == Some(&b'#') {
            open += 2;
        }
        if visible
            .get(open)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 0x80)
        {
            open += 1;
            while visible
                .get(open)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte >= 0x80)
            {
                open += 1;
            }
            while visible.get(open).is_some_and(u8::is_ascii_whitespace) {
                open += 1;
            }
        }
        let close = match visible.get(open).copied() {
            Some(b'(') => b')',
            Some(b'[') => b']',
            Some(b'{') => b'}',
            _ => {
                at += 1;
                continue;
            }
        };
        let mut stack = vec![close];
        let mut cursor = open + 1;
        while let Some(&byte) = visible.get(cursor) {
            match byte {
                b'(' => stack.push(b')'),
                b'[' => stack.push(b']'),
                b'{' => stack.push(b'}'),
                b')' | b']' | b'}' => {
                    if stack.pop() != Some(byte) {
                        return Err("invalid D-46 macro token tree");
                    }
                    if stack.is_empty() {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if !stack.is_empty() {
            return Err("unterminated D-46 macro token tree");
        }
        for byte in &mut visible[at..=cursor] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        at = cursor + 1;
    }
    Ok(())
}

fn char_literal_end(source: &str, quote: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let first = quote + 1;
    let end = if bytes.get(first) == Some(&b'\\') {
        match bytes.get(first + 1)? {
            b'u' if bytes.get(first + 2) == Some(&b'{') => {
                let close = (first + 3..bytes.len()).find(|&at| bytes[at] == b'}')?;
                close + 1
            }
            b'x' => first + 4,
            _ => first + 2,
        }
    } else {
        first + source.get(first..)?.chars().next()?.len_utf8()
    };
    (bytes.get(end) == Some(&b'\'')).then_some(end + 1)
}

fn code(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn method<'a>(source: &'a str, name: &str) -> Result<(&'a str, &'a str), &'static str> {
    let signature = format!("pub(super) fn {name}(");
    let start = source.find(&signature).ok_or("D-46 method missing")?;
    let open = source[start..]
        .find('{')
        .map(|at| start + at)
        .ok_or("D-46 method body missing")?;
    let mut depth = 0;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&source[start..open], &source[open + 1..open + offset]));
                }
            }
            _ => {}
        }
    }
    Err("D-46 method body incomplete")
}

fn fields<'a>(source: &'a str, name: &str) -> Result<&'a str, &'static str> {
    let signature = format!("pub(super) struct {name}<'lease> {{");
    let start = source.find(&signature).ok_or("D-46 affine type missing")?;
    let open = start + signature.len();
    let close = source[open..]
        .find('}')
        .map(|at| open + at)
        .ok_or("D-46 affine type incomplete")?;
    Ok(&source[open..close])
}

fn required_order(body: &str, steps: &[&str]) -> Result<(), &'static str> {
    let mut remaining = body;
    for step in steps {
        let at = remaining.find(step).ok_or("D-46 boundary order changed")?;
        remaining = &remaining[at + step.len()..];
    }
    Ok(())
}

fn check_boundary(source: &str) -> Result<(), &'static str> {
    let visible = hide_non_code(source)?;
    let source = visible.as_str();
    for name in ["WindowsInitialInstallPermit", "CreationArmedEffectGate"] {
        let actual = code(fields(source, name)?);
        if !actual.contains("issuer: &'lease mut dyn NativeInitialAdmissionIssuer")
            || !actual.contains("_worker_only: PhantomData<Rc<()>>")
        {
            return Err("D-46 gate lost the unique writer borrow or thread bound");
        }
    }

    let (journal_signature, journal_body) = method(source, "consume_for_admission_journal")?;
    if !code(journal_signature).contains("( self, )") {
        return Err("D-46 permit is not consumed at journal entry");
    }
    let journal = code(journal_body);
    required_order(
        &journal,
        &[
            "thread::current().id() != self.worker",
            "self.issuer.revalidate_under_repository_lease()?;",
            "self.issuer.publish_and_read_back_creation_armed()?;",
            "Ok(CreationArmedEffectGate {",
        ],
    )?;
    if journal
        .matches("self.issuer.revalidate_under_repository_lease()?;")
        .count()
        != 1
        || journal.contains("acquire_bootstrap_lock")
    {
        return Err("D-46 journal entry can perform an effect or skip its only recheck");
    }

    let (effect_signature, effect_body) = method(source, "enter_bootstrap_effect")?;
    if !code(effect_signature).contains("(self, roots: &Roots)") {
        return Err("D-46 armed effect gate is reusable");
    }
    let effect = code(effect_body);
    required_order(
        &effect,
        &[
            "thread::current().id() != self.worker",
            "self.issuer.revalidate_under_repository_lease()?;",
            "self.issuer.require_durable_creation_armed()?;",
            "self.issuer.require_bound_existing_scaffold(roots)?;",
            "self.issuer.revalidate_under_repository_lease()?;",
            "let lock = initialize::acquire_bootstrap_lock(roots)?;",
            "initialize::recheck_bootstrap_scaffold(roots)?;",
            "self.issuer.revalidate_bootstrap_lock(&lock)?;",
            "initialize::recheck_bootstrap_scaffold(roots)?;",
            "self.issuer.publish_and_read_back_bootstrap_owned(&lock)?;",
        ],
    )?;
    if effect
        .matches("self.issuer.revalidate_under_repository_lease()?;")
        .count()
        != 2
        || effect
            .matches("initialize::acquire_bootstrap_lock(roots)?;")
            .count()
            != 1
        || effect
            .matches("initialize::recheck_bootstrap_scaffold(roots)?;")
            .count()
            != 2
        || !effect.contains(
            "self.issuer.revalidate_under_repository_lease()?; let lock = initialize::acquire_bootstrap_lock(roots)?;",
        )
    {
        return Err("D-46 effect-time checks, final handoff or single bootstrap create changed");
    }
    Ok(())
}

#[test]
fn permit_boundary_is_one_use_thread_bound_and_checked_at_effect_time() {
    check_boundary(SOURCE).expect("closed D-46 source contract");
}

// Keep the negative controls as data rather than a long test function. Every
// mutation has an exact source anchor and an independent refusal assertion.
const SOURCE_MUTATIONS: &[(&str, &str, &str)] = &[
    (
        "journal permit replay",
        "pub(super) fn consume_for_admission_journal(\n        self,",
        "pub(super) fn consume_for_admission_journal(\n        &self,",
    ),
    (
        "journal publication removed",
        "self.issuer.publish_and_read_back_creation_armed()?;",
        "let _ = &self.issuer;",
    ),
    (
        "effect gate replay",
        "pub(super) fn enter_bootstrap_effect(self, roots: &Roots)",
        "pub(super) fn enter_bootstrap_effect(&mut self, roots: &Roots)",
    ),
    (
        "durable fence readback removed",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = &self.issuer;",
    ),
    (
        "durable fence replaced with inert ordinary string",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = \"self.issuer.require_durable_creation_armed()?;\";",
    ),
    (
        "durable fence replaced with inert raw string",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = r#\"self.issuer.require_durable_creation_armed()?;\"#;",
    ),
    (
        "durable fence replaced with inert byte string",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = b\"self.issuer.require_durable_creation_armed()?;\";",
    ),
    (
        "durable fence replaced with inert multi-hash raw byte string",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = br##\"self.issuer.require_durable_creation_armed()?;\"##;",
    ),
    (
        "durable fence replaced with inert stringify call",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = stringify!(self.issuer.require_durable_creation_armed()?;);",
    ),
    (
        "durable fence replaced with inert stringify braces",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = stringify! { self.issuer.require_durable_creation_armed()?; };",
    ),
    (
        "durable fence replaced with inert stringify brackets",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = stringify![self.issuer.require_durable_creation_armed()?;];",
    ),
    (
        "durable fence replaced with whitespace-bang nested stringify",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = stringify ! ([{ self.issuer.require_durable_creation_armed()?; }]);",
    ),
    (
        "durable fence replaced with comment-gap stringify",
        "self.issuer.require_durable_creation_armed()?;",
        "let _ = stringify /* inert macro */ ! { self.issuer.require_durable_creation_armed()?; };",
    ),
    (
        "durable fence replaced with dormant macro definition",
        "self.issuer.require_durable_creation_armed()?;",
        "macro_rules! dormant_fence { () => { self.issuer.require_durable_creation_armed()?; }; }",
    ),
    (
        "durable fence replaced with spaced dormant macro definition",
        "self.issuer.require_durable_creation_armed()?;",
        "macro_rules /* inert */ ! dormant_fence { () => { self.issuer.require_durable_creation_armed()?; }; }",
    ),
    (
        "durable fence replaced with raw-identifier dormant macro definition",
        "self.issuer.require_durable_creation_armed()?;",
        "macro_rules! r#dormant_fence { () => { self.issuer.require_durable_creation_armed()?; }; }",
    ),
    (
        "durable fence replaced with Unicode-identifier dormant macro definition",
        "self.issuer.require_durable_creation_armed()?;",
        "macro_rules! \u{0394}fence { () => { self.issuer.require_durable_creation_armed()?; }; }",
    ),
    (
        "durable fence replaced with inert block comment",
        "self.issuer.require_durable_creation_armed()?;",
        "/* self.issuer.require_durable_creation_armed()?; */",
    ),
    (
        "journal publication replaced with nested block comment",
        "self.issuer.publish_and_read_back_creation_armed()?;",
        "/* retired /* nested */ self.issuer.publish_and_read_back_creation_armed()?; */",
    ),
    (
        "foreign scaffold unchecked",
        "self.issuer.require_bound_existing_scaffold(roots)?;",
        "let _ = roots;",
    ),
    (
        "final recheck moved after creation",
        "self.issuer.revalidate_under_repository_lease()?;\n        let lock = initialize::acquire_bootstrap_lock(roots)?;",
        "let lock = initialize::acquire_bootstrap_lock(roots)?;\n        self.issuer.revalidate_under_repository_lease()?;",
    ),
    (
        "bootstrap owned readback removed",
        "self.issuer.publish_and_read_back_bootstrap_owned(&lock)?;",
        "let _ = &lock;",
    ),
    (
        "worker identity inversion at effect",
        "thread::current().id() != self.worker {\n            return Err(error(\n                \"initial install admission\",\n                \"actual worker changed before first protected effect\"",
        "thread::current().id() == self.worker {\n            return Err(error(\n                \"initial install admission\",\n                \"actual worker changed before first protected effect\"",
    ),
    (
        "callback inserted after final revalidation",
        "self.issuer.revalidate_under_repository_lease()?;\n        let lock = initialize::acquire_bootstrap_lock(roots)?;",
        "self.issuer.revalidate_under_repository_lease()?;\n        unrelated_callback();\n        let lock = initialize::acquire_bootstrap_lock(roots)?;",
    ),
];

#[test]
fn single_fault_permit_mutations_fail_before_a_native_issuer_exists() {
    for &(label, before, after) in SOURCE_MUTATIONS {
        assert_eq!(
            SOURCE.matches(before).count(),
            1,
            "{label}: exact mutation anchor"
        );
        let altered = SOURCE.replacen(before, after, 1);
        assert_ne!(altered, SOURCE, "{label}: mutation must change real source");
        assert!(
            check_boundary(&altered).is_err(),
            "{label}: mutation was admitted"
        );
    }
}
