//! KEL-145 documentary contract check for Keld's atomic problem-solving protocol.
//!
//! Root `AGENTS.md` owns the policy. This checker only pins its mandatory stage markers
//! and the narrower operational references. It is deliberately std-only and outside the
//! Cargo workspace. Compile with:
//! `rustc --edition=2024 -D warnings tools/atomic_protocol.rs`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ROOT: &str = "AGENTS.md";
const WORKFLOW: &str = "docs/agents/workflow.md";
const TESTING: &str = ".agents/testing.md";
const INDEX: &str = ".agents/index.md";
const ROOT_HEADING: &str = "## Atomic problem-solving protocol (MUST)";
const RETIRED_HEADING: &str = "Failure decomposition protocol (MUST)";

const STAGES: &[&str] = &[
    "1. **Decompose before deciding (MUST).**",
    "2. **State the logical component (MUST).**",
    "3. **Validate independence (MUST).**",
    "4. **Verify correctness (MUST).**",
    "5. **Synthesize only after proof (MUST).**",
];

const ROOT_SEMANTICS: &[&str] = &[
    "Before selecting a design, answer or fix",
    "Each atom MUST name its owner, boundary and inputs/outputs, failure mode, and observable contract",
    "Hidden coupling MUST be promoted into its own atom or an explicit edge between atoms.",
    "direct evidence or a falsifiable test or negative control",
    "until every decision-bearing atom is passed, explicitly unknown, or named as a blocker",
    "If the synthesis contradicts a passed atom, agents MUST stop and correct the model",
    "Performance decompositions MUST separate census, work, queue/copy, clock, statistic and artifact.",
    "Security decompositions MUST separate identity, authentication, authorization, OS containment, lifecycle/revocation and evidence provenance.",
];

const WORKFLOW_REQUIREMENTS: &[&str] = &[
    "root `AGENTS.md` § Atomic problem-solving protocol",
    "same first comment MUST record the decision-bearing atoms",
    "owner, boundary and inputs/outputs, failure mode, observable contract, independence from the other atoms, and first falsifier",
    "A material-decision comment MUST also record every atom changed or added by the decision, its independence edges and first falsifier",
];

const TESTING_REQUIREMENTS: &[&str] = &[
    "Root `AGENTS.md` § Atomic problem-solving protocol owns the decomposition.",
    "bind it to one named atom's observable contract",
    "state why its oracle is independent of the implementation and the other atoms",
    "Every negative control MUST name the one fault or mutation that falsifies that atom",
];

const INDEX_REQUIREMENTS: &[&str] = &[
    "Any non-trivial design, diagnosis, review, or implementation",
    "Root `AGENTS.md` § Atomic problem-solving protocol",
];

fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "ATOMIC-PROTOCOL: cannot read `{}`: {error}. Restore the KEL-145 contract file.",
            path.display()
        )
    })
}

fn without_html_comments(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut remainder = text;
    while let Some(start) = remainder.find("<!--") {
        visible.push_str(&remainder[..start]);
        let comment = &remainder[start + "<!--".len()..];
        let Some(end) = comment.find("-->") else {
            return visible;
        };
        remainder = &comment[end + "-->".len()..];
    }
    visible.push_str(remainder);
    visible
}

fn visible_markdown(text: &str) -> String {
    let without_comments = without_html_comments(text);
    let mut visible = String::with_capacity(without_comments.len());
    let mut fence: Option<&str> = None;

    for line in without_comments.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(active) = fence {
            if marker == Some(active) {
                fence = None;
            }
            continue;
        }
        if let Some(opening) = marker {
            fence = Some(opening);
            continue;
        }
        visible.push_str(line);
        visible.push('\n');
    }
    visible
}

fn normalize(text: &str) -> String {
    visible_markdown(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn section<'a>(text: &'a str, heading: &str) -> Result<&'a str, String> {
    let heading_offsets = text
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let current = *offset;
            *offset += line.len();
            Some((current, line.trim_end_matches(['\r', '\n'])))
        })
        .filter_map(|(offset, line)| (line == heading).then_some(offset))
        .collect::<Vec<_>>();
    if heading_offsets.len() != 1 {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{ROOT}` must contain exactly one `{heading}` section. Restore the canonical owner instead of copying or deleting it."
        ));
    }
    let start = heading_offsets[0];
    let after_heading = start + heading.len();
    let end = text[after_heading..]
        .find("\n## ")
        .map_or(text.len(), |offset| after_heading + offset);
    Ok(&text[start..end])
}

fn require_normalized(haystack: &str, needle: &str, path: &str) -> Result<(), String> {
    if normalize(haystack).contains(&normalize(needle)) {
        return Ok(());
    }
    Err(format!(
        "ATOMIC-PROTOCOL: `{path}` is missing or weakens `{needle}`. Restore the binding KEL-145 wording."
    ))
}

fn require_ordered(haystack: &str, needles: &[&str], path: &str) -> Result<(), String> {
    let visible = visible_markdown(haystack);
    let lines = visible.lines().collect::<Vec<_>>();
    let mut cursor = 0_usize;
    for needle in needles {
        let Some(offset) = lines[cursor..]
            .iter()
            .position(|line| line.starts_with(needle))
        else {
            return Err(format!(
                "ATOMIC-PROTOCOL: `{path}` is missing or reorders mandatory stage `{needle}`. Restore all stages in canonical order."
            ));
        };
        cursor += offset + 1;
    }
    Ok(())
}

fn check_root(text: &str) -> Result<(), String> {
    let visible = visible_markdown(text);
    if visible.contains(RETIRED_HEADING) {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{ROOT}` still contains retired duplicate `{RETIRED_HEADING}`. Reconcile failures into `{ROOT_HEADING}`."
        ));
    }
    let protocol = section(&visible, ROOT_HEADING)?;
    require_ordered(protocol, STAGES, ROOT)?;
    for requirement in ROOT_SEMANTICS {
        require_normalized(protocol, requirement, ROOT)?;
    }
    Ok(())
}

fn require_index_route(text: &str) -> Result<(), String> {
    let visible = visible_markdown(text);
    let routed = visible.lines().any(|line| {
        let cells = line
            .strip_prefix('|')
            .and_then(|line| line.strip_suffix('|'))
            .map(|line| line.split('|').map(str::trim).collect::<Vec<_>>());
        cells.is_some_and(|cells| {
            cells.len() == 2
                && cells[0] == INDEX_REQUIREMENTS[0]
                && cells[1].contains(INDEX_REQUIREMENTS[1])
        })
    });
    if routed {
        return Ok(());
    }
    Err(format!(
        "ATOMIC-PROTOCOL: `{INDEX}` must route `{}` to `{}` in one task-routing table row. Plain prose is not a route.",
        INDEX_REQUIREMENTS[0], INDEX_REQUIREMENTS[1]
    ))
}

fn check_references(root: &Path) -> Result<(), String> {
    let workflow = read(root, WORKFLOW)?;
    for requirement in WORKFLOW_REQUIREMENTS {
        require_normalized(&workflow, requirement, WORKFLOW)?;
    }

    let testing = read(root, TESTING)?;
    for requirement in TESTING_REQUIREMENTS {
        require_normalized(&testing, requirement, TESTING)?;
    }

    let index = read(root, INDEX)?;
    require_index_route(&index)?;

    for (path, text) in [(WORKFLOW, workflow), (TESTING, testing), (INDEX, index)] {
        for stage in STAGES {
            if normalize(&text).contains(&normalize(stage)) {
                return Err(format!(
                    "ATOMIC-PROTOCOL: `{path}` copies canonical stage `{stage}`. Reference `{ROOT_HEADING}` and keep only path-specific operations."
                ));
            }
        }
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    let root_text = read(root, ROOT)?;
    check_root(&root_text)?;
    check_references(root)
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        "ATOMIC-PROTOCOL: missing command. Run `atomic-protocol check [workspace]`.".to_owned()
    })?;
    let root = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if args.next().is_some() {
        return Err(
            "ATOMIC-PROTOCOL: too many arguments. Run `atomic-protocol check [workspace]`."
                .to_owned(),
        );
    }
    if command != "check" {
        return Err(format!(
            "ATOMIC-PROTOCOL: unknown command `{command}`. Use `check`."
        ));
    }
    check(&root)
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("keld-atomic-protocol-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create isolated fixture root");
            Self { path }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().expect("fixture path has parent"))
                .expect("create fixture parent");
            fs::write(path, contents).expect("write fixture");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn valid_root() -> String {
        format!(
            "# Rules\n\n{ROOT_HEADING}\n\nBefore selecting a design, answer or fix.\n\n{}\n{} Each atom MUST name its owner, boundary and inputs/outputs, failure mode, and observable contract.\n{} Hidden coupling MUST be promoted into its own atom or an explicit edge between atoms.\n{} Each atom needs direct evidence or a falsifiable test or negative control.\n{} Do not synthesize until every decision-bearing atom is passed, explicitly unknown, or named as a blocker. If the synthesis contradicts a passed atom, agents MUST stop and correct the model.\n\nPerformance decompositions MUST separate census, work, queue/copy, clock, statistic and artifact. Security decompositions MUST separate identity, authentication, authorization, OS containment, lifecycle/revocation and evidence provenance.\n\n## Next\n",
            STAGES[0], STAGES[1], STAGES[2], STAGES[3], STAGES[4]
        )
    }

    fn fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(ROOT, &valid_root());
        temp.write(
            WORKFLOW,
            "root `AGENTS.md` § Atomic problem-solving protocol\nThe same first comment MUST record the decision-bearing atoms: owner, boundary and inputs/outputs, failure mode, observable contract, independence from the other atoms, and first falsifier. A material-decision comment MUST also record every atom changed or added by the decision, its independence edges and first falsifier.\n",
        );
        temp.write(
            TESTING,
            "Root `AGENTS.md` § Atomic problem-solving protocol owns the decomposition. The author MUST bind it to one named atom's observable contract and state why its oracle is independent of the implementation and the other atoms. Every negative control MUST name the one fault or mutation that falsifies that atom.\n",
        );
        temp.write(
            INDEX,
            "| Task or path | Read |\n|---|---|\n| Any non-trivial design, diagnosis, review, or implementation | Root `AGENTS.md` § Atomic problem-solving protocol. |\n",
        );
        temp
    }

    fn replace_requirement(temp: &TempDir, path: &str, requirement: &str) {
        let contents = fs::read_to_string(temp.path.join(path)).expect("read fixture document");
        assert!(
            contents.contains(requirement),
            "fixture must contain `{requirement}`"
        );
        temp.write(path, &contents.replacen(requirement, "", 1));
    }

    #[test]
    fn complete_contract_passes() {
        let temp = fixture();
        check(&temp.path).expect("complete atomic protocol fixture must pass");
    }

    #[test]
    fn every_mandatory_stage_fails_when_removed_or_weakened() {
        for stage in STAGES {
            let temp = fixture();
            temp.write(ROOT, &valid_root().replacen(stage, "", 1));
            let error = check(&temp.path).expect_err("removed stage must fail");
            assert!(error.contains(stage), "{error}");

            let temp = fixture();
            let weakened = stage.replace("(MUST)", "(SHOULD)");
            temp.write(ROOT, &valid_root().replacen(stage, &weakened, 1));
            let error = check(&temp.path).expect_err("weakened stage must fail");
            assert!(error.contains(stage), "{error}");
        }
    }

    #[test]
    fn synthesis_contradiction_rule_is_mandatory() {
        let temp = fixture();
        temp.write(
            ROOT,
            &valid_root().replace(
                "If the synthesis contradicts a passed atom, agents MUST stop and correct the model.",
                "",
            ),
        );
        let error = check(&temp.path).expect_err("missing contradiction stop must fail");
        assert!(error.contains("contradicts"), "{error}");
    }

    #[test]
    fn every_root_semantic_fails_when_removed() {
        for requirement in ROOT_SEMANTICS {
            let temp = fixture();
            replace_requirement(&temp, ROOT, requirement);
            let error = check(&temp.path).expect_err("removed root semantic must fail");
            assert!(error.contains(requirement), "{error}");
        }
    }

    #[test]
    fn hidden_stage_text_cannot_satisfy_the_contract() {
        for hidden in [
            format!("<!-- {} -->", STAGES[0]),
            format!("```text\n{}\n```", STAGES[0]),
        ] {
            let temp = fixture();
            temp.write(ROOT, &valid_root().replacen(STAGES[0], &hidden, 1));
            let error = check(&temp.path).expect_err("hidden stage must not count as policy");
            assert!(error.contains(STAGES[0]), "{error}");
        }
    }

    #[test]
    fn canonical_structure_and_normative_strength_are_mandatory() {
        let temp = fixture();
        temp.write(
            ROOT,
            &valid_root().replacen(ROOT_HEADING, "Atomic problem-solving protocol (MUST)", 1),
        );
        let error = check(&temp.path).expect_err("ordinary prose heading must fail");
        assert!(error.contains("exactly one"), "{error}");

        let temp = fixture();
        temp.write(
            ROOT,
            &valid_root().replacen(
                "Each atom MUST name its owner",
                "Each atom MAY name its owner",
                1,
            ),
        );
        let error = check(&temp.path).expect_err("MAY must not satisfy MUST");
        assert!(error.contains("Each atom MUST name"), "{error}");

        let temp = fixture();
        temp.write(
            INDEX,
            "Any non-trivial design, diagnosis, review, or implementation reads Root `AGENTS.md` § Atomic problem-solving protocol.\n",
        );
        let error = check(&temp.path).expect_err("plain prose is not task routing");
        assert!(error.contains("table row"), "{error}");
    }

    #[test]
    fn old_failure_protocol_cannot_remain_as_a_second_owner() {
        let temp = fixture();
        temp.write(
            ROOT,
            &format!("{}\n**{RETIRED_HEADING}:** duplicate", valid_root()),
        );
        let error = check(&temp.path).expect_err("duplicate owner must fail");
        assert!(error.contains("retired duplicate"), "{error}");
    }

    #[test]
    fn operational_references_are_required_but_must_not_copy_the_stages() {
        let temp = fixture();
        temp.write(WORKFLOW, "material decision without atoms\n");
        let error = check(&temp.path).expect_err("missing workflow binding must fail");
        assert!(error.contains(WORKFLOW), "{error}");

        let temp = fixture();
        let copied = format!(
            "root `AGENTS.md` § Atomic problem-solving protocol\nThe same first comment MUST record the decision-bearing atoms: owner, boundary and inputs/outputs, failure mode, observable contract, independence from the other atoms, and first falsifier. A material-decision comment MUST also record every atom changed or added by the decision, its independence edges and first falsifier.\n{}\n",
            STAGES[0]
        );
        temp.write(WORKFLOW, &copied);
        let error = check(&temp.path).expect_err("copied canonical stages must fail");
        assert!(error.contains("copies canonical stage"), "{error}");
    }

    #[test]
    fn every_operational_requirement_is_independently_enforced() {
        for (path, requirements) in [
            (WORKFLOW, WORKFLOW_REQUIREMENTS),
            (TESTING, TESTING_REQUIREMENTS),
            (INDEX, INDEX_REQUIREMENTS),
        ] {
            for requirement in requirements {
                let temp = fixture();
                replace_requirement(&temp, path, requirement);
                let error = check(&temp.path).expect_err("removed reference must fail");
                assert!(error.contains(path), "{error}");
                assert!(error.contains(requirement), "{error}");
            }
        }
    }
}
