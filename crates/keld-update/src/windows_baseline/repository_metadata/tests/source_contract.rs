//! TEST ONLY: D-23/D-90 native writer guard callsites, not just helper behavior.
//!
//! The real D-90 repository owner has no qualified witness constructor.
//! These source-shape checks cannot exercise the full protected writer, prove
//! control-flow reachability, or certify filesystem durability. They prevent
//! a single omitted retained-directory check from leaving helper-only tests
//! green. A real T-82 issuer and native crash-cut tests remain mandatory.

#[path = "source_contract_lex.rs"]
mod lex;

const SOURCE: &str = include_str!("../../repository_metadata.rs");
const JOURNAL_SOURCE: &str = include_str!("../../initial_admission_journal.rs");
const EMPTY: &str = "require_empty_generation_namespace(self.generations)?;";
const GENESIS: &str = "require_only_genesis_generation(self.generations)?;";
const PENDING: &str = "require_pending_generation_namespace(self.generations, current.is_some())?;";
const GENESIS_MATCH: &str = "require_only_genesis_generation(self.generations)?,";
const SUCCESSOR: &str = "require_only_genesis_and_successor(self.generations)?;";
const ROOT_GUARD: &str = "require_no_repository_pending_siblings(self.repository)?;";
const PENDING_READ: &str =
    "read_optional_record(self.repository, PENDING_OPERATION, self.profile)?";
const CURRENT_READ: &str = "read_optional_record(self.repository, CURRENT, self.profile)?";

fn body(visible: &str, name: &str) -> Result<String, &'static str> {
    let range = lex::method_span(visible, name)?;
    Ok(lex::compact(&visible[range]))
}

fn exactly(body: &str, call: &str, count: usize) -> Result<(), &'static str> {
    if body.matches(call).count() != count {
        return Err("D23 source method lost or duplicated a native census call");
    }
    Ok(())
}

fn ordered(body: &str, required: &[&str]) -> Result<(), &'static str> {
    let mut remainder = body;
    for step in required {
        let position = remainder
            .find(step)
            .ok_or("D23 native census moved outside its required write boundary")?;
        remainder = &remainder[position + step.len()..];
    }
    Ok(())
}

fn inspect_contract(visible: &str) -> Result<(), &'static str> {
    let actual = body(visible, "inspect")?;
    exactly(&actual, EMPTY, 1)?;
    exactly(&actual, GENESIS, 1)?;
    exactly(&actual, PENDING, 1)?;
    ordered(
        &actual,
        &[
            "None if current.is_none() => {",
            EMPTY,
            "Some(anchor) if anchor.enrollment_state == EnrollmentState::Pending",
            PENDING,
            "Ok(NativeMetadataInspection::EnrollmentPending {",
            "Some(anchor) => {",
            GENESIS,
            "Ok(NativeMetadataInspection::Established {",
        ],
    )
}

fn initial_anchor_contract(visible: &str) -> Result<(), &'static str> {
    let actual = body(visible, "publish_pending_anchor")?;
    exactly(&actual, EMPTY, 1)?;
    ordered(
        &actual,
        &[
            "if self.read_anchor()?.is_some() || self.read_current()?.is_some()",
            EMPTY,
            "let bytes = encode_metadata_anchor(anchor)?;",
            "publish_new_record(",
        ],
    )
}

fn generation_contract(visible: &str) -> Result<(), &'static str> {
    let actual = body(visible, "publish_immutable_generation")?;
    exactly(&actual, EMPTY, 1)?;
    exactly(&actual, GENESIS, 1)?;
    exactly(&actual, GENESIS_MATCH, 1)?;
    exactly(&actual, SUCCESSOR, 1)?;
    ordered(
        &actual,
        &[
            "match anchor.enrollment_state {",
            "EnrollmentState::Pending => {",
            EMPTY,
            "EnrollmentState::Established => {",
            GENESIS,
            "publish_new_record(",
            "match anchor.enrollment_state {",
            "EnrollmentState::Pending =>",
            GENESIS_MATCH,
            "EnrollmentState::Established => {",
            SUCCESSOR,
            "Ok(digest)",
        ],
    )
}

fn initial_current_contract(visible: &str) -> Result<(), &'static str> {
    let actual = body(visible, "publish_initial_current")?;
    exactly(&actual, GENESIS, 1)?;
    ordered(
        &actual,
        &[
            "if self.read_current()?.is_some() {",
            GENESIS,
            "let generation = self.require_selected_generation(current)?;",
            "publish_new_record(",
        ],
    )
}

fn check_contract(source: &str) -> Result<(), &'static str> {
    let visible = lex::hide_inert_text(source)?;
    inspect_contract(&visible)?;
    initial_anchor_contract(&visible)?;
    generation_contract(&visible)?;
    initial_current_contract(&visible)
}

/// Bind the same retained-root negative guard to both production D-90 entry
/// points. Inert comments, literals and macro trees cannot supply an anchor.
/// This source shape is not an OS writer lease or a D-90 census witness.
fn d90_callsite_contract(metadata: &str, journal: &str) -> Result<(), &'static str> {
    let visible = lex::hide_inert_text(metadata)?;
    // Every metadata entry that can read CURRENT or publish a protected record
    // must refuse pending/root aliases before its first live statement.
    for method in [
        "inspect",
        "publish_pending_anchor",
        "publish_immutable_generation",
        "publish_initial_current",
    ] {
        let publisher = body(&visible, method)?;
        exactly(&publisher, "self.require_no_pending()?;", 1)?;
        // method_span already starts after the function's opening brace.
        if !publisher
            .trim_start()
            .starts_with("self.require_no_pending()?;")
        {
            return Err("D90 publisher bypasses first-effect root namespace guard");
        }
    }
    let restart = body(&visible, "inspect_restart")?;
    ordered(
        &restart,
        &["journal.inspect_pointed_attempt()?", "self.inspect()"],
    )?;
    let inspect = body(&visible, "inspect")?;
    ordered(
        &inspect,
        &[
            "self.require_no_pending()?;",
            "let anchor = self.read_anchor()?;",
            "let current = self.read_current()?;",
        ],
    )?;
    let current = body(&visible, "read_current")?;
    exactly(&current, CURRENT_READ, 1)?;

    let no_pending = body(&visible, "require_no_pending")?;
    d90_exact_root_guard(&no_pending)?;
    ordered(
        &no_pending,
        &[
            ROOT_GUARD,
            PENDING_READ,
            "require_only_pointed_repository_member(",
        ],
    )?;

    let visible = lex::hide_inert_text(journal)?;
    let pointed = body(&visible, "inspect_pointed_attempt")?;
    exactly(&pointed, "self.read_pending()?", 1)?;
    let pending = body(&visible, "read_pending")?;
    d90_exact_root_guard(&pending)?;
    ordered(
        &pending,
        &[
            ROOT_GUARD,
            PENDING_READ,
            "decode_pending_repository_operation(&bytes)?",
        ],
    )
}

fn d90_exact_root_guard(body: &str) -> Result<(), &'static str> {
    if body.matches(ROOT_GUARD).count() != 1 {
        return Err("D90 source lost or duplicated the retained root guard");
    }
    Ok(())
}

// Mutation replacement is made ONLY inside the actual selected method body
// after comments, literals and inert macro trees have been excluded. This
// simulates one omitted production guard without editing production source.
fn replace_one_call_in(source: &str, method: &str, call: &str, replacement: &str) -> String {
    let visible = lex::hide_inert_text(source).expect("source masking");
    let span = lex::method_span(&visible, method).expect("method body");
    let offsets = visible[span.clone()]
        .match_indices(call)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "{method}: one exact active guard anchor");
    let start = span.start + offsets[0];
    let end = start + call.len();
    assert_eq!(source.get(start..end), Some(call));
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, replacement);
    mutant
}

fn replace_one_call(method: &str, call: &str, replacement: &str) -> String {
    replace_one_call_in(SOURCE, method, call, replacement)
}

#[test]
fn retained_d23_writer_callsite_contract_matches_actual_source() {
    check_contract(SOURCE).expect("every native D23 callsite and cut order");
}

#[test]
fn d90_production_current_and_pending_reader_callsites_are_guarded() {
    d90_callsite_contract(SOURCE, JOURNAL_SOURCE)
        .expect("native D23 current and D68 pending reads require retained-root inspection");
}

#[test]
fn removing_only_repository_metadata_root_guard_goes_red() {
    d90_callsite_contract(SOURCE, JOURNAL_SOURCE).expect("unmodified source is green");
    let mutant = replace_one_call_in(
        SOURCE,
        "require_no_pending",
        ROOT_GUARD,
        "let _ = self.repository;",
    );
    assert_eq!(
        d90_callsite_contract(&mutant, JOURNAL_SOURCE),
        Err("D90 source lost or duplicated the retained root guard")
    );
}

#[test]
fn removing_only_initial_admission_pending_root_guard_goes_red() {
    d90_callsite_contract(SOURCE, JOURNAL_SOURCE).expect("unmodified source is green");
    let mutant = replace_one_call_in(
        JOURNAL_SOURCE,
        "read_pending",
        ROOT_GUARD,
        "let _ = self.repository;",
    );
    assert_eq!(
        d90_callsite_contract(SOURCE, &mutant),
        Err("D90 source lost or duplicated the retained root guard")
    );
}

#[test]
fn removing_any_current_publisher_root_fence_goes_red() {
    d90_callsite_contract(SOURCE, JOURNAL_SOURCE).expect("baseline publisher guard must pass");
    for method in [
        "inspect",
        "publish_pending_anchor",
        "publish_immutable_generation",
        "publish_initial_current",
    ] {
        let mutant = replace_one_call_in(
            SOURCE,
            method,
            "self.require_no_pending()?;",
            "let _ = self.repository;",
        );
        assert!(
            d90_callsite_contract(&mutant, JOURNAL_SOURCE).is_err(),
            "{method}: current or protected publication bypassed the first root check"
        );
    }
}

#[test]
fn every_single_d23_guard_removal_goes_red() {
    const SITES: &[(&str, &str)] = &[
        ("inspect", EMPTY),
        ("inspect", GENESIS),
        ("inspect", PENDING),
        ("publish_pending_anchor", EMPTY),
        ("publish_immutable_generation", EMPTY),
        ("publish_immutable_generation", GENESIS),
        ("publish_immutable_generation", GENESIS_MATCH),
        ("publish_immutable_generation", SUCCESSOR),
        ("publish_initial_current", GENESIS),
    ];
    for &(method, call) in SITES {
        let replacement = if call.ends_with(',') {
            "(),"
        } else {
            "let _ = self.generations;"
        };
        let altered = replace_one_call(method, call, replacement);
        assert!(
            check_contract(&altered).is_err(),
            "{method}: removal of {call} left the native callsite oracle green"
        );
    }
}

#[test]
fn inert_comments_literals_and_macro_tokens_cannot_replace_a_guard() {
    const REPLACEMENTS: &[&str] = &[
        "/* require_only_genesis_generation(self.generations)?; */",
        "/* nested /* source */ require_only_genesis_generation(self.generations)?; */",
        "let _ = \"require_only_genesis_generation(self.generations)?;\";",
        "let _ = r##\"require_only_genesis_generation(self.generations)?;\"##;",
        "let _ = br##\"require_only_genesis_generation(self.generations)?;\"##;",
        "let _ = stringify!(require_only_genesis_generation(self.generations)?;);",
        "let _ = stringify ! { require_only_genesis_generation(self.generations)?; };",
        "macro_rules! inert { () => { require_only_genesis_generation(self.generations)?; }; }",
        "macro_rules /* gap */ ! r#inert { () => { require_only_genesis_generation(self.generations)?; }; }",
    ];
    for &inert in REPLACEMENTS {
        let altered = replace_one_call("publish_initial_current", GENESIS, inert);
        assert!(
            check_contract(&altered).is_err(),
            "inert text was mistaken for a D23 production guard: {inert}"
        );
    }
}
