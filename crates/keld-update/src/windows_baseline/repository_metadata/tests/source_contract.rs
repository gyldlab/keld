//! TEST ONLY: D-23 native writer guard callsites, not just helper behavior.
//!
//! The real D-90 repository owner has no qualified witness constructor.
//! These source-shape checks cannot exercise the full protected writer, prove
//! control-flow reachability, or certify filesystem durability. They prevent
//! a single omitted retained-directory check from leaving helper-only tests
//! green. A real T-82 issuer and native crash-cut tests remain mandatory.

#[path = "source_contract_lex.rs"]
mod lex;

const SOURCE: &str = include_str!("../../repository_metadata.rs");
const EMPTY: &str = "require_empty_generation_namespace(self.generations)?;";
const GENESIS: &str = "require_only_genesis_generation(self.generations)?;";
const GENESIS_MATCH: &str = "require_only_genesis_generation(self.generations)?,";
const SUCCESSOR: &str = "require_only_genesis_and_successor(self.generations)?;";

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
    ordered(
        &actual,
        &[
            "None if current.is_none() => {",
            EMPTY,
            "Some(anchor) if anchor.enrollment_state == EnrollmentState::Pending",
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

// Mutation replacement is made ONLY inside the actual selected method body
// after comments, literals and inert macro trees have been excluded. This
// simulates one omitted production guard without editing production source.
fn replace_one_call(method: &str, call: &str, replacement: &str) -> String {
    let visible = lex::hide_inert_text(SOURCE).expect("source masking");
    let span = lex::method_span(&visible, method).expect("method body");
    let offsets = visible[span.clone()]
        .match_indices(call)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "{method}: one exact active guard anchor");
    let start = span.start + offsets[0];
    let end = start + call.len();
    assert_eq!(SOURCE.get(start..end), Some(call));
    let mut mutant = SOURCE.to_owned();
    mutant.replace_range(start..end, replacement);
    mutant
}

#[test]
fn retained_d23_writer_callsite_contract_matches_actual_source() {
    check_contract(SOURCE).expect("every native D23 callsite and cut order");
}

#[test]
fn every_single_d23_guard_removal_goes_red() {
    const SITES: &[(&str, &str)] = &[
        ("inspect", EMPTY),
        ("inspect", GENESIS),
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
