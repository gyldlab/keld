//! KEL-145 documentary contract check for Keld's atomic problem-solving protocol.
//!
//! Root `AGENTS.md` owns the policy. This checker only pins its mandatory stage markers
//! and the narrower operational references. It is deliberately std-only and outside the
//! Cargo workspace. Compile with:
//! `rustc --edition=2024 -D warnings tools/atomic_protocol.rs`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod markdown_contract;
use markdown_contract::{fence_marker, without_inline_code, without_struck_text};

const ROOT: &str = "AGENTS.md";
const WORKFLOW: &str = "docs/agents/workflow.md";
const TESTING: &str = ".agents/testing.md";
const INDEX: &str = ".agents/index.md";
const JUSTFILE: &str = "justfile";
const DEVELOPMENT_GUIDE: &str = "docs/onboarding/05-development-guide.md";
const ROOT_HEADING: &str = "## Atomic problem-solving protocol (MUST)";
const RETIRED_HEADING: &str = "Failure decomposition protocol (MUST)";
const WORKFLOW_HEADING: &str = "## The loop (one issue, one agent, one concern)";
const TESTING_HEADING: &str = "## Failure-first proof";
const INDEX_HEADING: &str = "## Task routing";
const DEVELOPMENT_GUIDE_CI_ROW: &str = "| `just ci` | Full local gate; the `justfile` `ci` recipe is the sole source of its inventory and order. |";
const ENFORCEMENT_LINE_PREFIX: &str =
    "Enforcement: `just atomic-protocol` validates the canonical stages";
const ATOMIC_RECIPE_COMMANDS: &[&str] = &[
    "mkdir -p target/atomic-protocol",
    "rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test",
    "target/atomic-protocol/atomic-protocol-test",
    "rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol",
    "target/atomic-protocol/atomic-protocol check .",
];

const STAGES: &[&str] = &[
    "1. **Decompose before deciding (MUST).**",
    "2. **State the logical component (MUST).**",
    "3. **Validate independence (MUST).**",
    "4. **Verify correctness (MUST).**",
    "5. **Synthesize only after proof (MUST).**",
];

const INTRO_SEMANTICS: &[&str] = &["Before selecting a design, answer or fix"];

const STAGE_SEMANTICS: &[&[&str]] = &[
    &["Split the problem into decision-bearing atoms"],
    &[
        "Each atom MUST name its owner, boundary and inputs/outputs, failure mode, and observable contract",
    ],
    &[
        "Changing or falsifying one atom MUST NOT silently alter another",
        "Hidden coupling MUST be promoted into its own atom or an explicit edge between atoms.",
    ],
    &[
        "Each atom MUST have direct evidence or a falsifiable test or negative control",
        "Prose, comments, mocks, or another atom's pass are not proof of that atom.",
    ],
    &[
        "until every decision-bearing atom is passed, explicitly unknown, or named as a blocker",
        "If the synthesis contradicts a passed atom, agents MUST stop and correct the model",
    ],
];

const FOOTER_SEMANTICS: &[&str] = &[
    "Performance decompositions MUST separate census, work, queue/copy, clock, statistic and artifact.",
    "Security decompositions MUST separate identity, authentication, authorization, OS containment, lifecycle/revocation and evidence provenance.",
    "Enforcement: `just atomic-protocol` validates the canonical stages",
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
    let mut fence: Option<(u8, usize)> = None;
    let mut block_quote = false;

    for line in without_comments.lines() {
        let trimmed = line.trim_start();
        let marker = fence_marker(line);
        if let Some((active_marker, active_width)) = fence {
            if marker.is_some_and(|(candidate, width, closing)| {
                candidate == active_marker && width >= active_width && closing
            }) {
                fence = None;
            }
            continue;
        }
        if line.starts_with('\t') || line.len() - trimmed.len() >= 4 {
            continue;
        }
        if let Some((opening, width, _)) = marker {
            fence = Some((opening, width));
            continue;
        }
        if trimmed.is_empty() {
            block_quote = false;
            visible.push('\n');
            continue;
        }
        if trimmed.starts_with('>') {
            block_quote = true;
            continue;
        }
        if block_quote {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.split_once("]:").is_some() {
            continue;
        }
        visible.push_str(&without_struck_text(line));
        visible.push('\n');
    }
    visible
}

fn binding_prose(text: &str) -> String {
    visible_markdown(text)
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.contains('|') && (!trimmed.starts_with('#') || trimmed.starts_with("## "))
        })
        .fold(String::new(), |mut prose, line| {
            prose.push_str(&without_inline_code(line));
            prose.push('\n');
            prose
        })
}

fn normalize(text: &str) -> String {
    visible_markdown(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_binding(text: &str) -> String {
    binding_prose(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized_occurrences(haystack: &str, needle: &str) -> usize {
    normalize(haystack).matches(&normalize(needle)).count()
}

fn section<'a>(text: &'a str, heading: &str, path: &str) -> Result<&'a str, String> {
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
            "ATOMIC-PROTOCOL: `{path}` must contain exactly one `{heading}` section. Restore the canonical owner instead of copying or deleting it."
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
    if normalize_binding(haystack).contains(&normalize_binding(needle)) {
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

fn exact_line_offsets(text: &str, marker: &str) -> Vec<usize> {
    text.split_inclusive('\n')
        .scan(0, |offset, line| {
            let current = *offset;
            *offset += line.len();
            Some((current, line.trim_end_matches(['\r', '\n'])))
        })
        .filter_map(|(offset, line)| (line == marker).then_some(offset))
        .collect()
}

fn line_prefix_offsets(text: &str, marker: &str) -> Vec<usize> {
    text.split_inclusive('\n')
        .scan(0, |offset, line| {
            let current = *offset;
            *offset += line.len();
            Some((current, line.trim_end_matches(['\r', '\n'])))
        })
        .filter_map(|(offset, line)| line.starts_with(marker).then_some(offset))
        .collect()
}

fn require_unique_normalized(haystack: &str, needle: &str, path: &str) -> Result<(), String> {
    let count = normalize_binding(haystack)
        .matches(&normalize_binding(needle))
        .count();
    if count == 1 {
        return Ok(());
    }
    Err(format!(
        "ATOMIC-PROTOCOL: `{path}` must contain binding wording `{needle}` exactly once; found {count}. Remove decoy, historical, or duplicate policy text."
    ))
}

fn check_root(text: &str) -> Result<(), String> {
    let rendered = visible_markdown(text);
    let visible = binding_prose(text);
    if visible.contains(RETIRED_HEADING) {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{ROOT}` still contains retired duplicate `{RETIRED_HEADING}`. Reconcile failures into `{ROOT_HEADING}`."
        ));
    }
    let protocol = section(&visible, ROOT_HEADING, ROOT)?;
    let rendered_protocol = section(&rendered, ROOT_HEADING, ROOT)?;
    if line_prefix_offsets(rendered_protocol, ENFORCEMENT_LINE_PREFIX).len() != 1
        || line_prefix_offsets(&rendered, ENFORCEMENT_LINE_PREFIX).len() != 1
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{ROOT}` must contain one `{ENFORCEMENT_LINE_PREFIX}` line in `{ROOT_HEADING}`."
        ));
    }
    require_ordered(protocol, STAGES, ROOT)?;

    let mut offsets = Vec::with_capacity(STAGES.len());
    for stage in STAGES {
        let stage_offsets = line_prefix_offsets(protocol, stage);
        if stage_offsets.len() != 1 || normalized_occurrences(&visible, stage) != 1 {
            return Err(format!(
                "ATOMIC-PROTOCOL: `{ROOT}` must contain mandatory stage `{stage}` exactly once as its own line in `{ROOT_HEADING}`."
            ));
        }
        offsets.push(stage_offsets[0]);
    }

    let intro = &protocol[..offsets[0]];
    for requirement in INTRO_SEMANTICS {
        require_normalized(intro, requirement, ROOT)?;
        require_unique_normalized(&visible, requirement, ROOT)?;
    }
    for (index, requirements) in STAGE_SEMANTICS.iter().enumerate() {
        let end = offsets.get(index + 1).copied().unwrap_or(protocol.len());
        let body = &protocol[offsets[index]..end];
        for requirement in *requirements {
            require_normalized(body, requirement, ROOT)?;
            require_unique_normalized(&visible, requirement, ROOT)?;
        }
    }
    for requirement in FOOTER_SEMANTICS {
        require_normalized(protocol, requirement, ROOT)?;
        require_unique_normalized(&visible, requirement, ROOT)?;
    }

    let outside = visible.replacen(protocol, "", 1).to_ascii_lowercase();
    let duplicate_owner = outside.split("\n## ").any(|candidate_section| {
        let signature_count = ["logical component", "independ", "correct", "synthes"]
            .iter()
            .filter(|signature| candidate_section.contains(*signature))
            .count();
        candidate_section.contains("atomic") && signature_count >= 3
    });
    if duplicate_owner {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{ROOT}` contains a second protocol owner outside `{ROOT_HEADING}`. Reconcile its stages into the canonical section."
        ));
    }
    Ok(())
}

fn line_block<'a>(
    text: &'a str,
    start_marker: &str,
    end_marker: Option<&str>,
    path: &str,
) -> Result<&'a str, String> {
    let starts = line_prefix_offsets(text, start_marker);
    if starts.len() != 1 {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{path}` must contain `{start_marker}` exactly once in its owning section."
        ));
    }
    let start = starts[0];
    let end = if let Some(marker) = end_marker {
        let ends = line_prefix_offsets(text, marker);
        if ends.len() != 1 || ends[0] <= start {
            return Err(format!(
                "ATOMIC-PROTOCOL: `{path}` must contain `{marker}` once after `{start_marker}`."
            ));
        }
        ends[0]
    } else {
        text.len()
    };
    Ok(&text[start..end])
}

fn require_index_route(text: &str) -> Result<(), String> {
    let visible = visible_markdown(text);
    let routing = section(&visible, INDEX_HEADING, INDEX)?;
    let lines = routing
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let header = lines
        .iter()
        .position(|line| line.trim() == "| Task or path | Read |");
    let route_is_in_table = header.is_some_and(|header| {
        lines
            .get(header + 1)
            .is_some_and(|line| line.trim() == "|---|---|")
            && lines[header + 2..]
                .iter()
                .take_while(|line| line.trim().starts_with('|'))
                .any(|line| {
                    line.trim()
                        .strip_prefix('|')
                        .and_then(|line| line.strip_suffix('|'))
                        .map(|line| line.split('|').map(str::trim).collect::<Vec<_>>())
                        .is_some_and(|cells| {
                            cells.len() == 2
                                && cells[0] == INDEX_REQUIREMENTS[0]
                                && cells[1].contains(INDEX_REQUIREMENTS[1])
                        })
                })
    });
    if route_is_in_table && normalized_occurrences(&visible, INDEX_REQUIREMENTS[0]) == 1 {
        return Ok(());
    }
    Err(format!(
        "ATOMIC-PROTOCOL: `{INDEX}` must route `{}` to `{}` inside the canonical table in `{INDEX_HEADING}`. Plain prose or an isolated decoy table is not a route.",
        INDEX_REQUIREMENTS[0], INDEX_REQUIREMENTS[1]
    ))
}

fn check_references(root: &Path) -> Result<(), String> {
    let workflow = read(root, WORKFLOW)?;
    let workflow_visible = binding_prose(&workflow);
    let workflow_loop = section(&workflow_visible, WORKFLOW_HEADING, WORKFLOW)?;
    let pickup = line_block(
        workflow_loop,
        "1. **Pick up and refresh.**",
        Some("2. **Spec gate.**"),
        WORKFLOW,
    )?;
    let implementation = line_block(
        workflow_loop,
        "4. **Implement and coordinate.**",
        Some("5. **Verify**"),
        WORKFLOW,
    )?;
    for requirement in &WORKFLOW_REQUIREMENTS[..3] {
        require_normalized(pickup, requirement, WORKFLOW)?;
        require_unique_normalized(&workflow_visible, requirement, WORKFLOW)?;
    }
    for requirement in &WORKFLOW_REQUIREMENTS[3..] {
        require_normalized(implementation, requirement, WORKFLOW)?;
        require_unique_normalized(&workflow_visible, requirement, WORKFLOW)?;
    }

    let testing = read(root, TESTING)?;
    let testing_visible = binding_prose(&testing);
    let failure_first = section(&testing_visible, TESTING_HEADING, TESTING)?;
    for requirement in TESTING_REQUIREMENTS {
        require_normalized(failure_first, requirement, TESTING)?;
        require_unique_normalized(&testing_visible, requirement, TESTING)?;
    }

    let index = read(root, INDEX)?;
    require_index_route(&index)?;

    for (path, text) in [
        (WORKFLOW, binding_prose(&workflow)),
        (TESTING, binding_prose(&testing)),
        (INDEX, binding_prose(&index)),
    ] {
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

fn check_justfile_and_development_guide(root: &Path) -> Result<(), String> {
    let justfile = read(root, JUSTFILE)?;
    let Some(ci_line) = justfile.lines().find(|line| line.starts_with("ci:")) else {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{JUSTFILE}` has no `ci:` recipe. Restore the sole local-gate inventory."
        ));
    };
    if ci_line
        .split_whitespace()
        .filter(|word| *word == "atomic-protocol")
        .count()
        != 1
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{JUSTFILE}` `ci:` must include `atomic-protocol` exactly once."
        ));
    }
    if exact_line_offsets(&justfile, "atomic-protocol:").len() != 1 {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{JUSTFILE}` must define the `atomic-protocol:` recipe exactly once."
        ));
    }
    let mut in_recipe = false;
    let mut recipe_commands = Vec::new();
    for line in justfile.lines() {
        if line == "atomic-protocol:" {
            in_recipe = true;
            continue;
        }
        if !in_recipe {
            continue;
        }
        if !line.trim().is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        let command = line.trim();
        if !command.is_empty() && !command.starts_with('#') {
            recipe_commands.push(command);
        }
    }
    if recipe_commands != ATOMIC_RECIPE_COMMANDS {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{JUSTFILE}` `atomic-protocol:` must compile and run the checker tests and real-check commands exactly; got `{}`.",
            recipe_commands.join(" | ")
        ));
    }

    let guide = visible_markdown(&read(root, DEVELOPMENT_GUIDE)?);
    if exact_line_offsets(&guide, DEVELOPMENT_GUIDE_CI_ROW).len() != 1 {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{DEVELOPMENT_GUIDE}` must point `just ci` at the `justfile` as the sole inventory instead of copying a stale gate list."
        ));
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    let root_text = read(root, ROOT)?;
    check_root(&root_text)?;
    check_references(root)?;
    check_justfile_and_development_guide(root)
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
            "# Rules\n\n{ROOT_HEADING}\n\nBefore selecting a design, answer or fix.\n\n{} Split the problem into decision-bearing atoms.\n{} Each atom MUST name its owner, boundary and inputs/outputs, failure mode, and observable contract.\n{} Changing or falsifying one atom MUST NOT silently alter another. Hidden coupling MUST be promoted into its own atom or an explicit edge between atoms.\n{} Each atom MUST have direct evidence or a falsifiable test or negative control. Prose, comments, mocks, or another atom's pass are not proof of that atom.\n{} Do not synthesize until every decision-bearing atom is passed, explicitly unknown, or named as a blocker. If the synthesis contradicts a passed atom, agents MUST stop and correct the model.\n\nPerformance decompositions MUST separate census, work, queue/copy, clock, statistic and artifact. Security decompositions MUST separate identity, authentication, authorization, OS containment, lifecycle/revocation and evidence provenance.\n\nEnforcement: `just atomic-protocol` validates the canonical stages.\n\n## Next\n",
            STAGES[0], STAGES[1], STAGES[2], STAGES[3], STAGES[4]
        )
    }

    fn fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(ROOT, &valid_root());
        temp.write(
            WORKFLOW,
            "# Workflow\n\n## The loop (one issue, one agent, one concern)\n\n1. **Pick up and refresh.** Fetch the Linear issue (team KELD, current milestone first),\nroot `AGENTS.md` § Atomic problem-solving protocol. The same first comment MUST record the decision-bearing atoms: owner, boundary and inputs/outputs, failure mode, observable contract, independence from the other atoms, and first falsifier.\n2. **Spec gate.** Larger than a bug fix and no spec? Write one from\n3. **Isolate.** Work separately.\n4. **Implement and coordinate.** Tests with the change (conformance entries *first* for\nA material-decision comment MUST also record every atom changed or added by the decision, its independence edges and first falsifier.\n5. **Verify** (the gate from root `AGENTS.md`): fmt + clippy `-D warnings` + full test\n\n## Next\n",
        );
        temp.write(
            TESTING,
            "# Testing\n\n## Failure-first proof\n\nRoot `AGENTS.md` § Atomic problem-solving protocol owns the decomposition. The author MUST bind it to one named atom's observable contract and state why its oracle is independent of the implementation and the other atoms. Every negative control MUST name the one fault or mutation that falsifies that atom.\n\n## Next\n",
        );
        temp.write(
            INDEX,
            "# Index\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Any non-trivial design, diagnosis, review, or implementation | Root `AGENTS.md` § Atomic problem-solving protocol. |\n\n## Next\n",
        );
        temp.write(
            JUSTFILE,
            &format!(
                "ci: atomic-protocol fmt-check\n\natomic-protocol:\n    {}\n",
                ATOMIC_RECIPE_COMMANDS.join("\n    ")
            ),
        );
        temp.write(
            DEVELOPMENT_GUIDE,
            &format!("# Development\n\n{DEVELOPMENT_GUIDE_CI_ROW}\n"),
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
        let requirements = INTRO_SEMANTICS
            .iter()
            .copied()
            .chain(
                STAGE_SEMANTICS
                    .iter()
                    .flat_map(|items| items.iter().copied()),
            )
            .chain(FOOTER_SEMANTICS.iter().copied());
        for requirement in requirements {
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
            format!("```text\n{}\n```\n", STAGES[0]),
        ] {
            let temp = fixture();
            temp.write(ROOT, &valid_root().replacen(STAGES[0], &hidden, 1));
            let error = check(&temp.path).expect_err("hidden stage must not count as policy");
            assert!(error.contains(STAGES[0]), "{error}");
        }
    }

    #[test]
    fn decoy_markdown_cannot_satisfy_binding_sections() {
        let requirement = STAGE_SEMANTICS[1][0];
        for decoy in [
            format!("| Historical | {requirement} |"),
            format!("Historical | {requirement}\n--- | ---"),
            format!("[//]: # ({requirement})"),
            format!("[atomic]: # ({requirement})"),
            format!("~~{requirement}~~"),
            format!("> {requirement}"),
            format!("> Historical quote\n{requirement}"),
            format!("### Historical: {requirement}"),
            format!("`{requirement}`"),
        ] {
            let temp = fixture();
            let weakened = valid_root()
                .replacen(requirement, "Each atom MAY name only a component.", 1)
                .replacen("\n## Next", &format!("\n{decoy}\n\n## Next"), 1);
            temp.write(ROOT, &weakened);
            check(&temp.path).expect_err("decoy text must not restore a weakened stage body");
        }

        let temp = fixture();
        let in_stage_table = valid_root().replacen(
            requirement,
            &format!("Each atom MAY name only a component.\n| Historical | {requirement} |"),
            1,
        );
        temp.write(ROOT, &in_stage_table);
        assert!(
            !binding_prose(&in_stage_table).contains(requirement),
            "table decoy must be absent from binding prose"
        );
        check(&temp.path).expect_err("a table inside the stage body is still a decoy");

        let temp = fixture();
        let in_stage_code = valid_root().replacen(
            requirement,
            &format!("Each atom MAY name only a component. `{requirement}`"),
            1,
        );
        temp.write(ROOT, &in_stage_code);
        check(&temp.path).expect_err("inline code inside the stage body is still a decoy");

        let temp = fixture();
        let wide_fence = valid_root().replacen(
            requirement,
            &format!(
                "Each atom MAY name only a component.\n````text\n```\n{requirement}\n````\n    ```"
            ),
            1,
        );
        temp.write(ROOT, &wide_fence);
        check(&temp.path).expect_err("a shorter inner fence must not expose hidden policy");

        let temp = fixture();
        let workflow = fs::read_to_string(temp.path.join(WORKFLOW)).expect("read workflow");
        let moved = workflow
            .replacen(WORKFLOW_REQUIREMENTS[1], "weakened historical pointer", 1)
            .replace(
                "## Next",
                &format!(
                    "## Historical wording\n\n{}\n\n## Next",
                    WORKFLOW_REQUIREMENTS[1]
                ),
            );
        temp.write(WORKFLOW, &moved);
        check(&temp.path).expect_err("historical section must not own workflow policy");

        let temp = fixture();
        let index = fs::read_to_string(temp.path.join(INDEX)).expect("read index");
        let route_row = format!("| {} | {}. |", INDEX_REQUIREMENTS[0], INDEX_REQUIREMENTS[1]);
        let moved = index
            .replacen(&route_row, "| Unrelated | No route. |", 1)
            .replace("## Next", &format!("## Next\n\n{route_row}"));
        temp.write(INDEX, &moved);
        check(&temp.path).expect_err("isolated table row must not become task routing");
    }

    #[test]
    fn duplicate_stage_and_renamed_protocol_owner_fail() {
        let temp = fixture();
        temp.write(
            ROOT,
            &format!("{}\n{} duplicate\n", valid_root(), STAGES[0]),
        );
        check(&temp.path).expect_err("duplicate mandatory stage must fail");

        let temp = fixture();
        temp.write(
            ROOT,
            &format!(
                "{}\n## Alternate atomic rules\n\nDecompose the units and state each logical component.\n\nValidate independence.\n\nVerify correctness.\n\nThen synthesize the final answer.\n",
                valid_root()
            ),
        );
        let error = check(&temp.path).expect_err("renamed duplicate owner must fail");
        assert!(error.contains("second protocol owner"), "{error}");
    }

    #[test]
    fn justfile_and_development_guide_cannot_drift() {
        let temp = fixture();
        temp.write(JUSTFILE, "ci: fmt-check\n\natomic-protocol:\n    true\n");
        check(&temp.path).expect_err("local ci must retain atomic protocol gate");

        let temp = fixture();
        temp.write(
            JUSTFILE,
            "ci: atomic-protocol fmt-check\n\nfake-atomic-protocol:\n    true\n",
        );
        check(&temp.path).expect_err("renamed recipe header must fail");

        let temp = fixture();
        temp.write(
            JUSTFILE,
            "ci: atomic-protocol fmt-check\n\natomic-protocol:\n    true\n",
        );
        check(&temp.path).expect_err("no-op recipe body must fail");

        let temp = fixture();
        temp.write(
            DEVELOPMENT_GUIDE,
            "| `just ci` | copied list: fmt-check clippy test |\n",
        );
        check(&temp.path).expect_err("development guide must not copy gate inventory");
    }

    #[test]
    fn harmless_workflow_reflow_and_route_reordering_pass() {
        let temp = fixture();
        let workflow = fs::read_to_string(temp.path.join(WORKFLOW)).expect("read workflow");
        temp.write(
            WORKFLOW,
            &workflow.replacen(
                "1. **Pick up and refresh.** Fetch the Linear issue",
                "1. **Pick up and refresh.**\n   Fetch the Linear issue",
                1,
            ),
        );
        check(&temp.path).expect("list-item prose reflow must keep ownership");

        let index = fs::read_to_string(temp.path.join(INDEX)).expect("read index");
        temp.write(
            INDEX,
            &index.replacen(
                "| Any non-trivial design",
                "| A harmless earlier route | Read something else. |\n| Any non-trivial design",
                1,
            ),
        );
        check(&temp.path).expect("route order inside the canonical table is not policy");
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
        assert!(error.contains(INDEX), "{error}");
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
        let workflow = fs::read_to_string(temp.path.join(WORKFLOW)).expect("read workflow");
        let copied = workflow.replacen(
            "2. **Spec gate.**",
            &format!("{}\n2. **Spec gate.**", STAGES[0]),
            1,
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
