//! One registry meaning per `KELD-*` code emitted by the shipping host (GH-1118).
//!
//! `src/main.rs` emits its codes from inline strings behind per-OS `cfg` gates, so no
//! single native build runs every site. This contract reads the source text instead
//! and binds every emitted literal to exactly one registry code by the condition its
//! message names. The expected mapping below is written from the registry meanings in
//! `docs/engineering/keld-error-codes.md`; it is not derived from the code under test.
//!
//! Mutation that must turn this red: put `KELD-CORE-037` back on a probe, sentinel or
//! guardian-argv line, or `KELD-WV-009` back on a fixture line.

/// The host source as compiled; a moved emit site changes this text, not the test.
const HOST_MAIN: &str = include_str!("../src/main.rs");

/// Registry meaning per message family. One row per distinct condition, so a line
/// that names a condition and carries another family's code fails here.
const EXPECTED: &[(&str, &str)] = &[
    // KELD-CLI-044: unknown CLI flag (shared with keld-cli's flag parser).
    ("unknown hello flag", "KELD-CLI-044"),
    ("unknown host argument", "KELD-CLI-044"),
    // KELD-CORE-037: the no-flag authenticated app session failed, including its
    // ordered cleanup and its supervised Bun guardian.
    ("dev-stage cleanup failed for", "KELD-CORE-037"),
    ("supervised Bun guardian failed", "KELD-CORE-037"),
    ("guardian stderr failed", "KELD-CORE-037"),
    // KELD-CORE-044: a private host role invoked outside its argv/build contract.
    ("fixture invocation", "KELD-CORE-044"),
    ("profile test-root probe", "KELD-CORE-044"),
    ("dev-stage cleanup invocation is malformed", "KELD-CORE-044"),
    ("dev-stage cleanup PID is invalid", "KELD-CORE-044"),
    ("guardian invocation is malformed", "KELD-CORE-044"),
    ("invalid private entry device", "KELD-CORE-044"),
    ("invalid private entry inode", "KELD-CORE-044"),
    // KELD-CORE-045: the guardian refused to run outside its validated dev stage.
    ("guardian stage identity failed", "KELD-CORE-045"),
    (
        "not executing from a validated private dev stage",
        "KELD-CORE-045",
    ),
    ("guardian stage argument does not match", "KELD-CORE-045"),
    // KELD-CORE-046: the Windows dev-stage cleanup sentinel failed a step.
    ("cleanup Job admission failed", "KELD-CORE-046"),
    ("dev-stage cleanup readiness failed", "KELD-CORE-046"),
    ("attempt process-family cleanup failed", "KELD-CORE-046"),
    ("malformed cleanup release was refused", "KELD-CORE-046"),
    ("cleanup control pipe failed", "KELD-CORE-046"),
];

/// Codes whose owner is another crate's typed error; the host must not borrow them
/// for its own conditions.
const FOREIGN_OWNED: &[&str] = &["KELD-WV-009"];

/// Pulls `KELD-AREA-001` / `KELD-AREA001` tokens (3+ digits), the registry grammar.
///
/// This mirrors `extract_keld_codes` in `crates/keld-cli/tests/error_registry.rs` on
/// purpose: that function lives in another crate's integration test, which no crate can
/// import, and the alternatives (a shared dev-dependency crate for fifteen lines, or a
/// cross-crate `#[path]` include) are the include-based pseudo-extraction that
/// `.agents/test-layout.md` forbids. Both copies pin the same grammar vector
/// (`code_grammar_matches_the_registry_scanner` here,
/// `extract_accepts_hyphenated_and_compact_forms` there), so a divergence fails one of them.
fn keld_codes(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 5 < bytes.len() {
        if &bytes[i..i + 5] != b"KELD-" {
            i += 1;
            continue;
        }
        let mut j = i + 5;
        while j < bytes.len() && bytes[j].is_ascii_uppercase() {
            j += 1;
        }
        if j == i + 5 {
            i += 1;
            continue;
        }
        if j < bytes.len() && bytes[j] == b'-' {
            j += 1;
        }
        let digits = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j - digits >= 3 {
            out.push(&line[i..j]);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn emit_sites() -> Vec<(usize, &'static str, &'static str)> {
    HOST_MAIN
        .lines()
        .enumerate()
        .flat_map(|(index, line)| {
            keld_codes(line)
                .into_iter()
                .map(move |code| (index + 1, code, line))
        })
        .collect()
}

#[test]
fn every_host_emit_site_carries_the_registry_code_for_its_condition() {
    let sites = emit_sites();
    assert!(
        sites.len() >= 20,
        "the host source lost most of its emit sites; re-read src/main.rs: {sites:?}"
    );
    let mut seen: Vec<&str> = Vec::new();
    for (line_number, code, line) in &sites {
        let families: Vec<&(&str, &str)> = EXPECTED
            .iter()
            .filter(|(phrase, _)| line.contains(phrase))
            .collect();
        assert_eq!(
            families.len(),
            1,
            "src/main.rs:{line_number} must name exactly one classified condition \
             (add a row to EXPECTED with its registry meaning): {line}"
        );
        let (_, expected) = families[0];
        assert_eq!(
            code, expected,
            "src/main.rs:{line_number} carries {code} for a condition the registry \
             documents as {expected}: {line}"
        );
        if !seen.contains(expected) {
            seen.push(expected);
        }
    }
    // Negative control on the table itself: every classified code is still emitted,
    // so a row can never pass by matching nothing.
    for (_, expected) in EXPECTED {
        assert!(
            seen.contains(expected),
            "{expected} is classified but no longer emitted by src/main.rs"
        );
    }
}

#[test]
fn host_does_not_borrow_codes_owned_by_other_crates() {
    for (line_number, code, line) in emit_sites() {
        assert!(
            !FOREIGN_OWNED.contains(&code),
            "src/main.rs:{line_number} borrows {code}, whose meaning is owned by another \
             crate's typed error: {line}"
        );
    }
}

#[test]
fn code_grammar_matches_the_registry_scanner() {
    assert_eq!(
        keld_codes("KELD-CLI-044: x KELD-MCP001 y KEL-35 KELD-FOO KELD-X12"),
        vec!["KELD-CLI-044", "KELD-MCP001"]
    );
}
