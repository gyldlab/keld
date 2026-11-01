//! Structural regression guard for every public-FS fixture source file.
//!
//! It rejects the known ways a diagnostic or assertion failure can render an
//! untrusted page/app record, captured child `Output`/stream, or a private
//! token, nonce or endpoint: inline captures, positional, indexed and named
//! format arguments, positional Debug, any `dbg!`, `assert_eq!`/`assert_ne!`
//! (and `debug_` variants) operands that name a private field, and any
//! equality operand or format/message argument that calls a local closure
//! (`let`, `let mut`, typed `let x: T =`, `move`) or fn whose body indexes or
//! `.get`s a private key literal (best effort, by name).
//!
//! It is a textual check, not a Rust parser or a comprehensive secret scan.
//! Known shapes it cannot see, which review and the sentinel tests must cover:
//! - a rename before printing (`let r = report; eprintln!("{r}")`), including
//!   a renamed private value used as an equality operand;
//! - raw byte writes (`stderr().write_all(report.as_bytes())`);
//! - `Result::expect`/`unwrap` panics that render an `Err`'s `Debug`;
//! - accessors that index a private value through a variable key, or that
//!   are defined outside the public-FS fixture.
//!
//! The sentinel formatter tests in the parent module remain the behavioural
//! proof of the fixed diagnostics.

use std::fs;
use std::path::{Path, PathBuf};

/// Bindings that hold untrusted records, captured child output or private
/// values in the public-FS fixture.
const UNTRUSTED: [&str; 34] = [
    "report",
    "reports",
    "result",
    "started",
    "admitted",
    "ready",
    "late_report",
    "output",
    "click",
    "stderr",
    "stdout",
    "executable",
    "ps_output",
    "reply",
    "boot",
    "endpoint",
    "token",
    "nonce",
    "pairs",
    "bun_pipe",
    "child_pipe",
    "host_rows",
    "bun_rows",
    "app_link",
    "old_link",
    "successor_link",
    "old_token",
    "old_endpoint",
    "initial_document",
    "raw_bytes",
    "path",
    // Kernel socket addresses (application-link census identities).
    "identity",
    "client_peer",
    "client_socket",
];

/// Macros whose arguments are rendered into a message or log line.
const FORMAT_MACROS: [&str; 16] = [
    "dbg!",
    "debug_assert!",
    "debug_assert_eq!",
    "debug_assert_ne!",
    "eprintln!",
    "println!",
    "eprint!",
    "print!",
    "panic!",
    "assert!",
    "assert_eq!",
    "assert_ne!",
    "format!",
    "write!",
    "writeln!",
    "unreachable!",
];

/// Owners that reduce an untrusted value to a fixed marker or counts.
const SAFE_WRAPPERS: [&str; 6] = [
    "output_summary(",
    "route_summary(",
    "host_exit_summary(",
    "observed_marker(",
    "PrivatePhase::",
    "Phase::",
];

/// Scalar projections that render no payload bytes.
const SCALAR_PROJECTIONS: [&str; 5] = [
    ".len()",
    ".is_empty()",
    ".success()",
    ".code()",
    ".signal()",
];

/// Equality operands naming one of these render a private value on failure.
/// `server_socket` is only an equality operand here, not an untrusted format
/// binding: `format!("->{server_socket}")` legitimately builds `client_peer`.
/// Kernel PIDs and process images are not listed: the fixture's process
/// custody checks assert them with `assert_eq!` by design.
const PRIVATE_FIELDS: [&str; 10] = [
    "nonce",
    "token",
    "endpoint",
    "[key]",
    // Kernel socket addresses (application-link census identities).
    ".identity",
    "client_peer",
    "client_socket",
    "server_socket",
    // Kernel stdout pipe identities of the descendant fixture.
    "bun_pipe",
    "child_pipe",
];

fn untrusted_interpolations(source: &str) -> Vec<String> {
    untrusted_interpolations_with(source, &private_accessors(source))
}

/// `accessors` are local closures/fns whose body indexes a private key; the
/// fixture-wide check passes the union over every public-FS source.
fn untrusted_interpolations_with(source: &str, accessors: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for binding in UNTRUSTED {
        for shape in [format!("{{{binding}}}"), format!("{{{binding}:")] {
            if source.contains(&shape) {
                found.push(shape);
            }
        }
    }
    // Positional Debug is rejected outright; a fixed diagnostic names fields.
    for positional in ["{:?}", "{:#?}"] {
        if source.contains(positional) {
            found.push(positional.to_owned());
        }
    }
    for (name, args) in invocations(source) {
        if name == "dbg!" {
            // dbg! renders the Debug of every argument to stderr.
            found.push(format!("dbg!({})", args.trim()));
            continue;
        }
        let parts = split_top_level(args);
        if matches!(
            name,
            "assert_eq!" | "assert_ne!" | "debug_assert_eq!" | "debug_assert_ne!"
        ) {
            for operand in parts.iter().take(2) {
                if names_private_field(operand) || calls_any(operand, accessors) {
                    found.push(format!("{name} operand `{}`", operand.trim()));
                }
            }
        }
        let Some(format_at) = parts.iter().position(|part| is_literal(part)) else {
            continue;
        };
        for part in &parts[format_at + 1..] {
            let expression = without_argument_name(part).trim();
            if renders_untrusted(expression) || calls_any(expression, accessors) {
                found.push(format!("{name} argument `{expression}`"));
            }
        }
    }
    found
}

fn renders_untrusted(expression: &str) -> bool {
    if is_literal(expression)
        || SAFE_WRAPPERS
            .iter()
            .any(|wrapper| expression.trim_start_matches('&').starts_with(wrapper))
        || SCALAR_PROJECTIONS
            .iter()
            .any(|projection| expression.ends_with(projection))
    {
        return false;
    }
    identifiers(expression)
        .iter()
        .any(|identifier| UNTRUSTED.contains(identifier))
}

fn names_private_field(operand: &str) -> bool {
    let operand = operand.trim();
    if SCALAR_PROJECTIONS
        .iter()
        .any(|projection| operand.ends_with(projection))
    {
        return false;
    }
    let lower = operand.to_ascii_lowercase();
    PRIVATE_FIELDS.iter().any(|field| lower.contains(field))
}

/// Private key suffixes whose literal index (`["documentNonce"]`) or
/// `.get("documentNonce")` marks a local closure/fn as a private accessor.
const PRIVATE_KEYS: [&str; 3] = ["nonce", "token", "endpoint"];

/// Whether lowercase `body` indexes or `.get`s a private key literal.
fn reads_private_key(body: &str) -> bool {
    PRIVATE_KEYS.iter().any(|key| {
        body.contains(&format!("{key}\"]"))
            || body.match_indices(".get(\"").any(|(at, open)| {
                let literal = &body[at + open.len()..];
                literal.find('"').is_some_and(|end| {
                    literal[..end].ends_with(key) && literal[end..].starts_with("\")")
                })
            })
    })
}

/// Names of `let name = |..| ..;` closures and `fn name(..) {..}` items whose
/// body indexes a private key literal (for example `["documentNonce"]`).
fn private_accessors(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if let Some(end) = opaque_end(bytes, at) {
            at = end;
            continue;
        }
        let boundary = at == 0 || !is_ident(bytes[at - 1]);
        let keyword = [&b"let "[..], &b"fn "[..]]
            .into_iter()
            .find(|keyword| boundary && bytes[at..].starts_with(keyword));
        let Some(keyword) = keyword else {
            at += 1;
            continue;
        };
        let mut name_start = at + keyword.len();
        if keyword == b"let " && bytes[name_start..].starts_with(b"mut ") {
            name_start += 4;
        }
        let name_end = bytes[name_start..]
            .iter()
            .position(|byte| !is_ident(*byte))
            .map_or(bytes.len(), |end| name_start + end);
        let name = &source[name_start..name_end];
        let rest = source[name_end..].trim_start();
        let body = if keyword == b"fn " {
            source[name_end..].find('{').map(|open| {
                let open = name_end + open;
                &source[open..matching_close(bytes, open)]
            })
        } else if let Some(closure) = closure_initializer(rest) {
            let closure = closure.trim_start();
            let closure = closure.strip_prefix("move ").unwrap_or(closure);
            closure.starts_with('|').then(|| statement(closure))
        } else {
            None
        };
        if let Some(body) = body {
            let lower = body.to_ascii_lowercase();
            if !name.is_empty() && reads_private_key(&lower) {
                found.push(name);
            }
        }
        at = name_end.max(at + 1);
    }
    found
}

/// The initializer after `= ` in `let name = ..` or a typed
/// `let name: Type = ..` binding (the type has no `=` before the initializer).
fn closure_initializer(rest: &str) -> Option<&str> {
    let rest = match rest.strip_prefix(':') {
        Some(typed) => {
            let statement_end = typed.find(';').unwrap_or(typed.len());
            let equals = typed[..statement_end].find('=')?;
            &typed[equals..]
        }
        None => rest,
    };
    let initializer = rest.strip_prefix('=')?;
    (!initializer.starts_with('=')).then_some(initializer)
}

/// Text up to the first top-level `;`.
fn statement(text: &str) -> &str {
    let bytes = text.as_bytes();
    let (mut depth, mut at) = (0_usize, 0);
    while at < bytes.len() {
        if let Some(end) = opaque_end(bytes, at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => return &text[..at],
            _ => {}
        }
        at += 1;
    }
    text
}

/// Whether `operand` calls one of `names` (`name(` at an identifier boundary).
fn calls_any(operand: &str, names: &[&str]) -> bool {
    names.iter().any(|name| {
        operand.match_indices(name).any(|(at, _)| {
            let bytes = operand.as_bytes();
            let before = at == 0 || !is_ident(bytes[at - 1]);
            let after = operand[at + name.len()..].trim_start();
            before && after.starts_with('(')
        })
    })
}

/// `name = expression` (an explicitly named format argument) yields `expression`.
fn without_argument_name(part: &str) -> &str {
    let trimmed = part.trim_start();
    let name_end = trimmed
        .bytes()
        .position(|byte| !is_ident(byte))
        .unwrap_or(trimmed.len());
    let rest = trimmed[name_end..].trim_start();
    if name_end > 0 && rest.starts_with('=') && !rest.starts_with("==") {
        &rest[1..]
    } else {
        part
    }
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_literal(part: &str) -> bool {
    let bytes = part.trim().as_bytes();
    !bytes.is_empty() && opaque_end(bytes, 0) == Some(bytes.len())
}

/// End of a comment, string literal or char literal starting at `at`.
fn opaque_end(bytes: &[u8], at: usize) -> Option<usize> {
    let rest = &bytes[at..];
    if rest.starts_with(b"//") {
        let line = rest.iter().position(|byte| *byte == b'\n');
        return Some(line.map_or(bytes.len(), |end| at + end));
    }
    if rest.starts_with(b"/*") {
        let close = rest[2..].windows(2).position(|pair| pair == b"*/");
        return Some(close.map_or(bytes.len(), |end| at + 2 + end + 2));
    }
    if at > 0 && is_ident(bytes[at - 1]) {
        return None;
    }
    let mut quote = at;
    if bytes.get(quote) == Some(&b'b') {
        quote += 1;
    }
    let raw = bytes.get(quote) == Some(&b'r');
    let mut hashes = 0;
    if raw {
        quote += 1;
        while bytes.get(quote) == Some(&b'#') {
            hashes += 1;
            quote += 1;
        }
    }
    if bytes.get(quote) == Some(&b'"') {
        let mut end = quote + 1;
        while end < bytes.len() {
            match bytes[end] {
                b'\\' if !raw => end += 2,
                b'"' if bytes
                    .get(end + 1..end + 1 + hashes)
                    .is_some_and(|tail| tail.iter().all(|byte| *byte == b'#')) =>
                {
                    return Some(end + 1 + hashes);
                }
                _ => end += 1,
            }
        }
        return Some(bytes.len());
    }
    if bytes[at] == b'\'' {
        if bytes.get(at + 1) == Some(&b'\\') {
            let close = bytes[at + 2..].iter().position(|byte| *byte == b'\'');
            return close.map(|end| at + 2 + end + 1);
        }
        if bytes.get(at + 2) == Some(&b'\'') {
            return Some(at + 3);
        }
    }
    None
}

/// Every `name!(...)` invocation of a format macro, with its argument text.
fn invocations(source: &str) -> Vec<(&'static str, &str)> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if let Some(end) = opaque_end(bytes, at) {
            at = end;
            continue;
        }
        let boundary = at == 0 || !is_ident(bytes[at - 1]);
        if let Some(name) = FORMAT_MACROS
            .iter()
            .find(|name| boundary && bytes[at..].starts_with(name.as_bytes()))
        {
            let mut open = at + name.len();
            while bytes.get(open).is_some_and(u8::is_ascii_whitespace) {
                open += 1;
            }
            if bytes.get(open) == Some(&b'(') {
                let close = matching_close(bytes, open);
                found.push((*name, &source[open + 1..close]));
                at = open + 1;
                continue;
            }
        }
        at += 1;
    }
    found
}

fn matching_close(bytes: &[u8], open: usize) -> usize {
    let mut depth = 0_usize;
    let mut at = open;
    while at < bytes.len() {
        if let Some(end) = opaque_end(bytes, at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
        at += 1;
    }
    bytes.len()
}

fn split_top_level(args: &str) -> Vec<&str> {
    let bytes = args.as_bytes();
    let mut parts = Vec::new();
    let (mut depth, mut start, mut at) = (0_usize, 0, 0);
    while at < bytes.len() {
        if let Some(end) = opaque_end(bytes, at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(&args[start..at]);
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    if !args[start..].trim().is_empty() {
        parts.push(&args[start..]);
    }
    parts
}

fn identifiers(expression: &str) -> Vec<&str> {
    let bytes = expression.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if let Some(end) = opaque_end(bytes, at) {
            at = end;
            continue;
        }
        if is_ident(bytes[at]) && !bytes[at].is_ascii_digit() {
            let start = at;
            while bytes.get(at).copied().is_some_and(is_ident) {
                at += 1;
            }
            found.push(&expression[start..at]);
            continue;
        }
        at += 1;
    }
    found
}

/// Every Rust source of the public-FS fixture except this guard, whose
/// negative-control literals deliberately contain the rejected shapes.
fn public_fs_sources() -> Vec<(PathBuf, String)> {
    let macos = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/no_flag/macos");
    let mut pending = vec![macos.join("public_fs")];
    let mut files = vec![macos.join("public_fs.rs")];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("public-FS fixture directory") {
            let path = entry.expect("public-FS fixture entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && !path.ends_with("private_diagnostics/source_guard.rs")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).expect("public-FS fixture source");
            (path, source)
        })
        .collect()
}

#[test]
fn no_public_fs_fixture_source_renders_an_untrusted_or_private_value() {
    let sources = public_fs_sources();
    // Non-vacuity: the walk must reach the parent owner and the nested
    // observer server whose raw report print this guard was widened for.
    for required in ["macos/public_fs.rs", "public_fs/observer/server.rs"] {
        assert!(
            sources.iter().any(|(path, _)| path.ends_with(required)),
            "guard walk missed {required}"
        );
    }
    let accessors: Vec<&str> = sources
        .iter()
        .flat_map(|(_, source)| private_accessors(source))
        .collect();
    let leaks: Vec<String> = sources
        .iter()
        .flat_map(|(path, source)| {
            untrusted_interpolations_with(source, &accessors)
                .into_iter()
                .map(move |leak| format!("{}: {leak}", path.display()))
        })
        .collect();
    assert!(
        leaks.is_empty(),
        "an untrusted fixture record, Output, Host stream or private value was \
         reintroduced in a test diagnostic: {leaks:#?}"
    );
}

#[test]
fn untrusted_interpolation_guard_sees_every_known_binding_shape() {
    // Negative control: each shape that once leaked, or that the earlier
    // inline-only guard could not see, must be reported.
    for leaked in [
        // Inline captures (original shapes).
        r#"assert_eq!(a, b, "{started}");"#,
        r#"assert_eq!(a, b, "{admitted}");"#,
        r#"assert_eq!(a, b, "{ready}");"#,
        r#"assert!(ok, "{stderr}");"#,
        r#"panic!("late: {late_report:?}");"#,
        r#"eprintln!("host_output={output:?}");"#,
        r#"eprintln!("{report}");"#,
        // The observer server's raw page report (token and nonces).
        r#"eprintln!("KELD_KEL140_OBSERVATION {report}");"#,
        r#"eprintln!("ARM ready={ready} stdout={bun_pipe:?}");"#,
        // Positional Debug.
        r#"assert!(ok, "{:?}", evidence.click);"#,
        r#"eprintln!("rows={:#?}", rows);"#,
        // Positional Display with an untrusted argument.
        r#"assert!(ok, "{}", stderr);"#,
        r#"writeln!(err, "ack {}", reply);"#,
        r#"eprintln!("a, (b {}", output.stdout.escape_ascii());"#,
        // Indexed arguments.
        r#"eprintln!("{0} {0}", report);"#,
        r#"panic!("{1}: {0}", result["requests"], case);"#,
        // Inline-named arguments.
        r#"eprintln!("{x}", x = stderr);"#,
        r#"panic!("{r}", r = result["token"]);"#,
        // Equality operands naming a private field.
        r#"assert_eq!(ready["token"], self.token);"#,
        r#"assert_eq!(report[key], other[key], "same {key}");"#,
        r#"assert_ne!(old_endpoint, endpoint, "fresh endpoint");"#,
        r#"assert_eq!(admitted["nonce"], started["requests"][2]["nonce"]);"#,
        // Any dbg!.
        "dbg!(report);",
        "dbg!(&evidence.reports[0]);",
        // A local closure or fn returning a private field (the D1 shape).
        r#"let document = |evidence: &LaunchEvidence| {
            evidence.reports[0]["documentNonce"].clone()
        };
        assert_ne!(document(&recovered), document(&relaunched), "new document");"#,
        r#"let document = move |e: &E| e.page["javascriptToken"].clone();
        assert_eq!(document(&a), b);"#,
        r#"fn first_request(report: &Value) -> &Value { &report["requests"][0]["nonce"] }
        fn check(a: &Value, b: &Value) { assert_eq!(first_request(a), b); }"#,
        // Accessor results rendered as format or message arguments (N1).
        r#"let document = |e: &E| e.page["documentNonce"].clone();
        panic!("document {}", document(&recovered));"#,
        r#"let document = |e: &E| e.page["documentNonce"].clone();
        assert_eq!(a, b, "renamed {}", document(&relaunched));"#,
        // Typed closure binding (N2).
        r#"let typed: fn(&LaunchEvidence) -> Value = |e| e.reports[0]["documentNonce"].clone();
        assert_ne!(typed(&a), typed(&b));"#,
        // `.get` accessor (N2).
        r#"let document = |e: &E| e.page.get("documentNonce").cloned();
        assert_ne!(document(&a), document(&b));"#,
        r#"fn session(e: &E) -> Option<&Value> { e.boot.get("endpoint") }
        fn check(a: &E, b: &E) { assert_eq!(session(a), session(b)); }"#,
        // `let mut` closure binding.
        r#"let mut document = |e: &E| e.page["javascriptToken"].clone();
        assert_eq!(document(&a), x);"#,
        // debug_assert variants (N2).
        "debug_assert_ne!(document_nonce, other);",
        r#"let document = |e: &E| e.page["documentNonce"].clone();
        debug_assert_ne!(document(&recovered), document(&relaunched));"#,
        r#"debug_assert!(ok, "{}", stderr);"#,
        // Route paths carry a pending-handler nonce.
        r#"panic!("unexpected route {path}");"#,
        r#"panic!("unexpected route {}", path);"#,
        // Application-link kernel socket identities (#1015).
        r#"assert_eq!(row.identity, client_peer, "client FD clone changed its connected peer");"#,
        r#"assert_eq!(peer, client_socket.as_str(), "host connected-peer symmetry");"#,
        "assert_ne!(server_socket, other);",
        r#"eprintln!("peer={client_peer}");"#,
        r#"panic!("socket {}", row.identity);"#,
        // Descendant stdout pipe identities (#1012).
        r#"assert_eq!(bun_pipe, child_pipe, "descendant must inherit exact Bun stdout endpoint");"#,
        "debug_assert_ne!(child_pipe, other);",
    ] {
        assert!(
            !untrusted_interpolations(leaked).is_empty(),
            "guard missed {leaked}"
        );
    }
    // Fixed markers, counts and fixture-owned literals are not flagged.
    for fixed in [
        r#"eprintln!("{}", marker); "{phase} {key}""#,
        r#"eprintln!("{}", observed_marker(&report, PrivatePhase::Observation));"#,
        r#"eprintln!("{}", host_exit_summary(&output, evidence.reports.len()));"#,
        r#"assert!(ok, "{case}: {}", output_summary(&output));"#,
        r#"assert_eq!(output.status.code(), Some(1), "{case}");"#,
        r#"assert_eq!(nonce.len(), 32, "nonce length");"#,
        r#"format!("recovery-{}", phase.as_str())"#,
        r#"eprintln!("calls={n}", n = quit_calls);"#,
        r#"// eprintln!("{}", report) is not an invocation inside a comment"#,
        r#"fn pid(report: &Value, key: &str) -> u32 { report[key].as_u64().unwrap() as u32 }
        fn check(a: &Value) { assert_eq!(pid(a, "pid"), 7); }"#,
        r#"let document = |e: &E| e.page["phase"].clone();
        assert_eq!(document(&a), "page-ready");"#,
        r#"panic!("unexpected route; {}", route_summary(path));"#,
        r#"let phase = |e: &E| e.page.get("phase").cloned();
        assert_eq!(phase(&a), b, "phase {}", phase(&a).is_some());"#,
        r#"fn fresh(bytes: &mut String) { bytes.push_str("x"); let _ = "fixture nonce"; }
        fn check(a: &str) { assert_eq!(fresh_len(a), 32, "{}", fresh_len(a)); }"#,
        // The server socket builds the expected client peer; socket counts
        // and process-custody PIDs are not private socket identities.
        r#"let client_peer = format!("->{server_socket}");"#,
        r#"assert_eq!(pairs.len(), 1, "one connected application socket pair");"#,
        r#"assert_private_eq(&row.identity, &client_peer, "client FD clone changed its connected peer");"#,
        r#"assert_eq!(parent_process(guardian), host, "owned guardian parent");"#,
        r#"let bun_pipe = pipe_identity(family.bun, "1");"#,
    ] {
        assert!(
            untrusted_interpolations(fixed).is_empty(),
            "guard flagged fixed diagnostic {fixed}: {:?}",
            untrusted_interpolations(fixed)
        );
    }
}
