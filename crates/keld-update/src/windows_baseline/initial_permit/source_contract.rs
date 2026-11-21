//! Source-only negative controls for D-46's one-use effect boundary.
//!
//! The native issuer is not commissioned, so an executable positive issuer
//! fixture would counterfeit the missing KEL-53 repository lease. These tests
//! inspect only the real permit methods and reject single-fault mutations.
//! They cannot establish real OS identity, cross-install exclusion, or journal
//! durability; those require the commissioned native issuer and native tests.

const SOURCE: &str = include_str!("../initial_permit.rs");

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
        // A macro's tokens are never an executed statement merely because
        // they occur inside the permit method's lexical source.
        if visible
            .get(open)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        {
            open += 1;
            while visible
                .get(open)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
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

#[test]
fn single_fault_permit_mutations_fail_before_a_native_issuer_exists() {
    let mutations = [
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

    for (label, before, after) in mutations {
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
