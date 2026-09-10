//! KEL-145/KEL-148/KEL-190 documentary contract checks for atomic reasoning and merge authority.
//!
//! Root `AGENTS.md` owns the policy. This checker only pins its mandatory stage markers
//! and the narrower operational references. It is deliberately std-only and outside the
//! Cargo workspace. Compile with:
//! `rustc --edition=2024 -D warnings tools/atomic_protocol.rs`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod markdown_contract;
mod session_protocol;
use markdown_contract::{fence_marker, without_inline_code, without_struck_text};

const ROOT: &str = "AGENTS.md";
const WORKFLOW: &str = "docs/agents/workflow.md";
const RESEARCH: &str = ".agents/research.md";
const TESTING: &str = ".agents/testing.md";
const COORDINATION: &str = ".agents/coordination.md";
const DEPENDENCIES: &str = ".agents/dependencies.md";
const DOCS: &str = ".agents/docs.md";
const REVIEW: &str = ".agents/review.md";
const CI: &str = ".agents/ci.md";
const INDEX: &str = ".agents/index.md";
const CONTRIBUTING: &str = "CONTRIBUTING.md";
const MAINTAINERS: &str = "MAINTAINERS.md";
const BUG_TEMPLATE: &str = ".github/ISSUE_TEMPLATE/bug.yml";
const FEATURE_TEMPLATE: &str = ".github/ISSUE_TEMPLATE/feature.yml";
const TEMPLATE_CONFIG: &str = ".github/ISSUE_TEMPLATE/config.yml";
const JUSTFILE: &str = "justfile";
const DEVELOPMENT_GUIDE: &str = "docs/onboarding/05-development-guide.md";
const ROOT_HEADING: &str = "## Atomic problem-solving protocol (MUST)";
const RETIRED_HEADING: &str = "Failure decomposition protocol (MUST)";
const WORKFLOW_HEADING: &str = "## The loop (one issue, one agent, one concern)";
const PUBLIC_INTAKE_HEADING: &str = "## Public contributions";
const TESTING_HEADING: &str = "## Failure-first proof";
const DEPENDENCIES_AUTHORITATIVE_CHECKS_HEADING: &str = "## Authoritative checks";
const DOCS_DIAGRAM_SELECTION_HEADING: &str = "## Diagram selection and meaning";
const TESTING_MERMAID_GATE_HEADING: &str = "## Documentation and Mermaid render gate";
const MERGE_HEADING: &str = "## Standing autonomous merge delegation";
const MERGE_DEFAULT_PREFIX: &str = "Default eligible merge: ";
const PROMPT_TRACKER_HANDOFF_REQUIREMENT: &str = "Handoffs MUST follow Prompt Tracker `docs/06-graph-engineering.md` for system/client/exact-model identity.";
const CONTRIBUTING_LINK: &str = "https://github.com/gyldlab/keld/blob/main/CONTRIBUTING.md";
const MAINTAINER_REVIEW_START: &str = "- **Review:**";
const MAINTAINER_REVIEW_END: &str = "- **Direction:**";
const FORM_CONTRIBUTING_REQUIREMENT: &str = "Follow the [contribution guide](https://github.com/gyldlab/keld/blob/main/CONTRIBUTING.md) for public scope and submission.";
#[cfg(test)]
const FORM_CONTRIBUTING_LINE: &str = "        Follow the [contribution guide](https://github.com/gyldlab/keld/blob/main/CONTRIBUTING.md) for public scope and submission.";
#[cfg(test)]
const CONFIG_CONTRIBUTING_LINE: &str =
    "    url: https://github.com/gyldlab/keld/blob/main/CONTRIBUTING.md";
const CONFIG_CONTRIBUTING_BLOCK: &str = "contact_links:\n  - name: Contributing to Keld\n    url: https://github.com/gyldlab/keld/blob/main/CONTRIBUTING.md\n    about: Read the public scope, build, test, and pull-request process.";
const INDEX_HEADING: &str = "## Task routing";
const CURRENT_DOCUMENTATION_HEADING: &str = "## Current-documentation receipt";
const CURRENT_DOCUMENTATION_ROUTE: &str = "Material decision depends on current external OS/platform command, SDK/API, runtime, or external-tool semantics";
const CURRENT_DOCUMENTATION_LINK: &str = "[`.agents/research.md` § Current-documentation receipt](research.md#current-documentation-receipt)";
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

const CURRENT_DOCUMENTATION_RESEARCH_REQUIREMENTS: &[&str] = &[
    "Before deciding a material claim that depends on current external OS/platform-command, SDK/API, runtime, or external-tool semantics",
    "It does not apply to a pure local refactor whose decision does not rely on external semantics.",
    "When Context7 is available, resolve the relevant library and make a narrow query before deciding.",
    "Context7 is discovery, never the authority.",
    "Confirm every material claim with a current official primary source:",
    "If Context7 is unavailable or its query fails, record the exact failure",
    "If no relevant Context7 library applies, record that reason as not-applicable; primary confirmation is still required.",
    "leave the claim unknown or block the decision;",
    "Do not place private source text or sensitive queries there.",
];

const CURRENT_DOCUMENTATION_COORDINATION_REQUIREMENTS: &[&str] = &[
    "For each material external semantic selected by",
    "record these exact fields under the required receipt heading in the relevant Linear decision, OS handoff, or branch handoff.",
    "A pure local refactor does not need one.",
];

const CURRENT_DOCUMENTATION_RECEIPT_FIELDS: &[&str] = &[
    "- Applicability: applied:<external semantic> | not-applicable:<pure-local reason>",
    "- Context7: used:<library ID; query; retrieval date> | not-applicable:<reason> | unavailable:<exact tool failure>",
    "- Official primary: <URL or immutable source; applicable version/tag; retrieval date>",
    "- Supported claim: <exact decision-bearing claim>",
    "- Fallback/blocker: none | <unknown or blocked decision and next action>",
];

const CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS: &[&str] = &[
    "When a material decision depends on current external OS/platform-command, SDK/API, runtime, or external-tool semantics, create the receipt owned by",
    "Pure local refactors do not need the receipt.",
    "status, handoff, and current-documentation receipt rules owned by `.agents/coordination.md`",
];

const CURRENT_DOCUMENTATION_DEPENDENCIES_REQUIREMENTS: &[&str] = &[
    CURRENT_DOCUMENTATION_LINK,
    "to every current API or runtime semantic claim.",
    "It owns Context7 discovery, official confirmation, fallback, and the reviewable receipt; the registry and upstream sources above remain required dependency evidence.",
];

const CURRENT_DOCUMENTATION_DOCS_REQUIREMENTS: &[&str] = &[
    "For unfamiliar Mermaid syntax, apply",
    CURRENT_DOCUMENTATION_LINK,
    "official Mermaid docs remain the authority.",
];

const CURRENT_DOCUMENTATION_TESTING_REQUIREMENTS: &[&str] = &[
    "Before using unfamiliar Mermaid syntax, apply",
    CURRENT_DOCUMENTATION_LINK,
    "The official Mermaid docs are the primary syntax authority; Context7 remains discovery.",
];

const MERGE_OWNER_REQUIREMENTS: &[&str] = &[
    "The repository-owner standing delegation requires the issue agent to merge a Keld PR without another approval question only after every predicate below passes.",
    "Waive extra human review only if GitHub reports author and merger as `0monish` or `amishabenramani`; verify both before merge.",
    "Native bypass checks merger only; others need human review.",
    "Scope, winning claim, required approval artifacts, current base, dependencies and single-writer collisions are reconciled.",
    "Every owned acceptance criterion, including each required real OS/device observable, is passed rather than awaiting, failed or unrun.",
    "`just ci` and every applicable GitHub required check pass on the final tip.",
    "CodeRabbit reviewed the exact final tip or the isolated substitute passed, and every valid finding and review thread is fixed, independently refuted and resolved.",
    "Every applicable unsafe, public API, permission model, dependency addition and wire protocol gate has named independent security or architecture evidence on the exact final diff.",
    "The PR is mergeable and contains only the reviewed issue scope.",
    "A narrower explicit `do-not-merge`, missing approval artifact, or proposal whose acceptance is the decision itself overrides this delegation.",
    "This delegation authorizes only Keld PR merge; it does not authorize scope expansion, deployment, release, publication, production mutation, account administration or another repository.",
    "After merge, fetch main, verify the landed patch or tree and ancestor relation, post the execution artifact, complete only the owned acceptance unit, reconcile remaining parent criteria before marking its issue Done, release the claim and remove the clean worktree.",
];

const PUBLIC_INTAKE_OWNER_REQUIREMENTS: &[&str] = &[
    "This is Keld's canonical public-intake guide for external contributors.",
    "You do not need access to Linear, private research, or an agent-memory service.",
];

const WORKFLOW_PUBLIC_INTAKE_REQUIREMENTS: &[&str] = &[
    "External contributors follow [`CONTRIBUTING.md`](../../CONTRIBUTING.md), the canonical public-intake owner.",
    "Maintainers bridge accepted public scope into internal Linear linkage, spec and acceptance publication, claims, implementation and merge coordination.",
    "The internal loop below binds maintainers and agents acting with maintainer authority; external contributors do not acquire that authority.",
];

const MAINTAINER_REVIEW_REQUIREMENTS: &[&str] = &[
    "changes normally receive approval from a maintainer other than the author/latest pusher.",
    "CODEOWNERS requests the team; required CI and applicable architecture/security evidence must pass before merge.",
    "The narrow exception is owned by the internal [standing autonomous merge delegation](.agents/coordination.md#standing-autonomous-merge-delegation); all other actors keep normal human review.",
    "Agent review remains technical evidence and is never represented as human approval.",
];

const ROOT_MERGE_REQUIREMENT: &str = "Review gates require named independent security or architecture evidence under the standing repository-owner delegation in `.agents/coordination.md`; they do not require a human-only actor.";
const WORKFLOW_MERGE_REQUIREMENTS: &[&str] = &[
    "Apply `.agents/coordination.md` § Standing autonomous merge delegation after the branch handoff.",
    "When every predicate passes, merge without another approval question and complete its landed-verification sequence.",
];
const REVIEW_MERGE_REQUIREMENT: &str = "The terminal merge decision and post-merge verification are owned by `.agents/coordination.md` § Standing autonomous merge delegation.";
const CI_MERGE_REQUIREMENT: &str = "CI routing is an independently reviewed shared-file concern";

fn canonical_merge_owner() -> String {
    format!(
        "{MERGE_HEADING}\n{}\n{MERGE_DEFAULT_PREFIX}`merge-when-complete`.\n{}\n{}\n- {}\n- {}\n- {}\n- {}\n- {}\n- {}\n{}\n{}\n{}",
        MERGE_OWNER_REQUIREMENTS[0],
        MERGE_OWNER_REQUIREMENTS[1],
        MERGE_OWNER_REQUIREMENTS[2],
        MERGE_OWNER_REQUIREMENTS[3],
        MERGE_OWNER_REQUIREMENTS[4],
        MERGE_OWNER_REQUIREMENTS[5],
        MERGE_OWNER_REQUIREMENTS[6],
        MERGE_OWNER_REQUIREMENTS[7],
        MERGE_OWNER_REQUIREMENTS[8],
        MERGE_OWNER_REQUIREMENTS[9],
        MERGE_OWNER_REQUIREMENTS[10],
        MERGE_OWNER_REQUIREMENTS[11],
    )
}

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

fn yaml_markdown_scalars(text: &str) -> Vec<String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut scalars = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if *line != "  - type: markdown" {
            continue;
        }
        let item_end = lines[index + 1..]
            .iter()
            .position(|candidate| candidate.starts_with("  - "))
            .map_or(lines.len(), |offset| index + 1 + offset);
        let Some(value_offset) = lines[index + 1..item_end]
            .iter()
            .position(|candidate| *candidate == "      value: |")
        else {
            continue;
        };
        let value_start = index + 1 + value_offset + 1;
        let value_end = lines[value_start..item_end]
            .iter()
            .position(|candidate| {
                !candidate.trim().is_empty()
                    && candidate.len() - candidate.trim_start_matches(' ').len() < 8
            })
            .map_or(item_end, |offset| value_start + offset);
        let scalar = lines[value_start..value_end]
            .iter()
            .map(|candidate| candidate.strip_prefix("        ").unwrap_or(candidate))
            .collect::<Vec<_>>()
            .join("\n");
        scalars.push(scalar);
    }
    scalars
}

fn yaml_top_level_value(text: &str, key: &str) -> Option<String> {
    let mut found = false;
    let mut value = String::new();
    for line in text.lines() {
        if line == key {
            if found {
                return None;
            }
            found = true;
            continue;
        }
        if !found {
            continue;
        }
        if !line.trim().is_empty() && !line.starts_with(' ') {
            break;
        }
        value.push_str(line);
        value.push('\n');
    }
    found.then_some(value)
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

fn direct_section<'a>(text: &'a str, heading: &str, path: &str) -> Result<&'a str, String> {
    let canonical_section = section(text, heading, path)?;
    let mut offset = 0_usize;
    let mut previous_line_is_nonempty = false;
    for line in canonical_section.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker_width = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        let is_new_heading = marker_width >= 1
            && trimmed
                .as_bytes()
                .get(marker_width)
                .is_some_and(u8::is_ascii_whitespace);
        let setext = line.trim();
        let is_setext_heading = previous_line_is_nonempty
            && !setext.is_empty()
            && setext.bytes().all(|byte| byte == b'=' || byte == b'-');
        if offset != 0 && (is_new_heading || is_setext_heading) {
            return Ok(&canonical_section[..offset]);
        }
        previous_line_is_nonempty = !setext.is_empty();
        offset += line.len();
    }
    Ok(canonical_section)
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

fn canonical_task_routing_rows<'a>(visible: &'a str) -> Result<Vec<(&'a str, &'a str)>, String> {
    let routing = section(&visible, INDEX_HEADING, INDEX)?;
    let lines = routing
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let header = lines
        .iter()
        .position(|line| line.trim() == "| Task or path | Read |");
    let Some(header) = header else {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{INDEX}` must contain the canonical task-routing header in `{INDEX_HEADING}`."
        ));
    };
    if !lines
        .get(header + 1)
        .is_some_and(|line| line.trim() == "|---|---|")
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{INDEX}` must contain the canonical two-cell task-routing separator in `{INDEX_HEADING}`."
        ));
    }

    Ok(lines[header + 2..]
        .iter()
        .take_while(|line| line.trim().starts_with('|'))
        .filter_map(|line| {
            let row = line.trim().strip_prefix('|')?.strip_suffix('|')?;
            let mut cells = row.split('|').map(str::trim);
            let (Some(task), Some(read), None) = (cells.next(), cells.next(), cells.next()) else {
                return None;
            };
            Some((task, read))
        })
        .collect())
}

fn require_index_route(text: &str) -> Result<(), String> {
    let visible = visible_markdown(text);
    let route_is_in_table = canonical_task_routing_rows(&visible)?
        .iter()
        .any(|(task, read)| *task == INDEX_REQUIREMENTS[0] && read.contains(INDEX_REQUIREMENTS[1]));
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

fn require_current_documentation_consumer(
    root: &Path,
    path: &str,
    heading: &str,
    requirements: &[&str],
) -> Result<(), String> {
    let text = read(root, path)?;
    let rendered = visible_markdown(&text);
    let canonical_section = binding_prose(direct_section(&rendered, heading, path)?);
    let visible = binding_prose(&text);
    for requirement in requirements {
        require_normalized(&canonical_section, requirement, path)?;
        require_unique_normalized(&visible, requirement, path)?;
    }
    Ok(())
}

fn check_current_documentation_receipts(root: &Path) -> Result<(), String> {
    let research = read(root, RESEARCH)?;
    let research_rendered = visible_markdown(&research);
    let research_visible = binding_prose(&research);
    let receipt = binding_prose(direct_section(
        &research_rendered,
        CURRENT_DOCUMENTATION_HEADING,
        RESEARCH,
    )?);
    for requirement in CURRENT_DOCUMENTATION_RESEARCH_REQUIREMENTS {
        require_normalized(&receipt, requirement, RESEARCH)?;
        require_unique_normalized(&research_visible, requirement, RESEARCH)?;
    }

    let coordination = read(root, COORDINATION)?;
    let coordination_visible = visible_markdown(&coordination);
    let receipt = direct_section(
        &coordination_visible,
        CURRENT_DOCUMENTATION_HEADING,
        COORDINATION,
    )?;
    for requirement in CURRENT_DOCUMENTATION_COORDINATION_REQUIREMENTS {
        require_normalized(receipt, requirement, COORDINATION)?;
        require_unique_normalized(&coordination_visible, requirement, COORDINATION)?;
    }
    for field in CURRENT_DOCUMENTATION_RECEIPT_FIELDS {
        if !normalize(receipt).contains(&normalize(field)) {
            return Err(format!(
                "ATOMIC-PROTOCOL: coordination must retain receipt field {field} in its canonical section."
            ));
        }
        if normalized_occurrences(&coordination, field) != 1 {
            return Err(format!(
                "ATOMIC-PROTOCOL: coordination must retain receipt field {field} exactly once."
            ));
        }
    }

    for (path, heading, requirements) in [
        (
            DEPENDENCIES,
            DEPENDENCIES_AUTHORITATIVE_CHECKS_HEADING,
            CURRENT_DOCUMENTATION_DEPENDENCIES_REQUIREMENTS,
        ),
        (
            DOCS,
            DOCS_DIAGRAM_SELECTION_HEADING,
            CURRENT_DOCUMENTATION_DOCS_REQUIREMENTS,
        ),
        (
            TESTING,
            TESTING_MERMAID_GATE_HEADING,
            CURRENT_DOCUMENTATION_TESTING_REQUIREMENTS,
        ),
    ] {
        require_current_documentation_consumer(root, path, heading, requirements)?;
    }

    let index = read(root, INDEX)?;
    let index_visible = visible_markdown(&index);
    let route_is_in_table =
        canonical_task_routing_rows(&index_visible)?
            .iter()
            .any(|(task, read)| {
                *task == CURRENT_DOCUMENTATION_ROUTE
                    && read.contains("research.md")
                    && read.contains("Current-documentation receipt")
                    && read.contains("testing.md")
            });
    if !route_is_in_table
        || normalized_occurrences(&index_visible, CURRENT_DOCUMENTATION_ROUTE) != 1
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: index must route {CURRENT_DOCUMENTATION_ROUTE} to the research receipt and testing inside its canonical table."
        ));
    }

    let workflow = read(root, WORKFLOW)?;
    let workflow_visible = binding_prose(&workflow);
    let workflow_loop = section(&workflow_visible, WORKFLOW_HEADING, WORKFLOW)?;
    let pickup = line_block(
        workflow_loop,
        "1. **Pick up and refresh.**",
        Some("2. **Spec gate.**"),
        WORKFLOW,
    )?;
    for requirement in &CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS[..2] {
        require_normalized(pickup, requirement, WORKFLOW)?;
        require_unique_normalized(&workflow_visible, requirement, WORKFLOW)?;
    }
    let implementation = line_block(
        workflow_loop,
        "4. **Implement and coordinate.**",
        Some("5. **Verify**"),
        WORKFLOW,
    )?;
    let requirement = CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS[2];
    require_normalized(implementation, requirement, WORKFLOW)?;
    require_unique_normalized(&workflow_visible, requirement, WORKFLOW)?;
    Ok(())
}

fn check_autonomous_merge(root: &Path) -> Result<(), String> {
    let root_text = read(root, ROOT)?;
    let root_visible = binding_prose(&root_text);
    require_normalized(&root_visible, ROOT_MERGE_REQUIREMENT, ROOT)?;
    require_unique_normalized(&root_visible, ROOT_MERGE_REQUIREMENT, ROOT)?;

    let coordination = read(root, COORDINATION)?;
    let coordination_rendered = visible_markdown(&coordination);
    let merge_rendered = section(&coordination_rendered, MERGE_HEADING, COORDINATION)?;
    let coordination_visible = binding_prose(&coordination);
    let merge_owner = section(&coordination_visible, MERGE_HEADING, COORDINATION)?;
    require_normalized(
        &coordination_visible,
        PROMPT_TRACKER_HANDOFF_REQUIREMENT,
        COORDINATION,
    )?;
    require_unique_normalized(
        &coordination_visible,
        PROMPT_TRACKER_HANDOFF_REQUIREMENT,
        COORDINATION,
    )?;
    for requirement in MERGE_OWNER_REQUIREMENTS {
        require_normalized(merge_owner, requirement, COORDINATION)?;
        require_unique_normalized(&coordination_visible, requirement, COORDINATION)?;
    }
    if merge_rendered
        .lines()
        .any(|line| line.trim_start().starts_with("### "))
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{COORDINATION}` `{MERGE_HEADING}` must not hide active policy under a subheading. Keep one closed canonical owner."
        ));
    }
    let canonical_merge = canonical_merge_owner();
    if normalize_binding(merge_owner) != normalize_binding(&canonical_merge) {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{COORDINATION}` `{MERGE_HEADING}` contains extra, missing, moved, or contradictory binding prose. Restore its closed canonical rule set."
        ));
    }
    let defaults = merge_rendered
        .lines()
        .filter_map(|line| line.trim().strip_prefix(MERGE_DEFAULT_PREFIX))
        .map(|value| value.trim().trim_end_matches('.').trim_matches('`'))
        .collect::<Vec<_>>();
    if defaults.len() != 1 || !defaults[0].eq_ignore_ascii_case("merge-when-complete") {
        let declared = defaults.first().copied().unwrap_or("missing");
        return Err(format!(
            "ATOMIC-PROTOCOL: `{COORDINATION}` must declare one active `{MERGE_DEFAULT_PREFIX}` value of `merge-when-complete`; got `{declared}`. `human-decide` is not an eligible default."
        ));
    }

    let workflow = read(root, WORKFLOW)?;
    let workflow_visible = binding_prose(&workflow);
    let workflow_loop = section(&workflow_visible, WORKFLOW_HEADING, WORKFLOW)?;
    let handoff = line_block(workflow_loop, "7. **PR and handoff.**", None, WORKFLOW)?;
    for requirement in WORKFLOW_MERGE_REQUIREMENTS {
        require_normalized(handoff, requirement, WORKFLOW)?;
        require_unique_normalized(&workflow_visible, requirement, WORKFLOW)?;
    }
    if normalize_binding(handoff)
        .to_ascii_lowercase()
        .contains("human-decide")
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{WORKFLOW}` step 7 retains `human-decide` as an active merge state. Use `{MERGE_HEADING}` or `do-not-merge`."
        ));
    }

    let review = read(root, REVIEW)?;
    let review_visible = binding_prose(&review);
    require_normalized(&review_visible, REVIEW_MERGE_REQUIREMENT, REVIEW)?;
    require_unique_normalized(&review_visible, REVIEW_MERGE_REQUIREMENT, REVIEW)?;

    let ci = binding_prose(&read(root, CI)?);
    require_normalized(&ci, CI_MERGE_REQUIREMENT, CI)?;
    require_unique_normalized(&ci, CI_MERGE_REQUIREMENT, CI)?;

    let maintainers = binding_prose(&read(root, MAINTAINERS)?);
    let review = line_block(
        &maintainers,
        MAINTAINER_REVIEW_START,
        Some(MAINTAINER_REVIEW_END),
        MAINTAINERS,
    )?;
    let expected = format!(
        "{MAINTAINER_REVIEW_START} {}",
        MAINTAINER_REVIEW_REQUIREMENTS.join(" ")
    );
    if normalize_binding(review) != normalize_binding(&expected) {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{MAINTAINERS}` review policy must link the `{COORDINATION}` canonical owner without copying its exception."
        ));
    }
    let outside_review = maintainers.replacen(review, "", 1).to_ascii_lowercase();
    let words = outside_review
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let second_merge_owner = ["bypass", "waive", "waiver"]
        .iter()
        .any(|word| words.contains(word))
        || [
            "skip human",
            "without human",
            "review is optional",
            "approval is optional",
            "review not required",
            "approval not required",
            "merge without",
            "may merge",
            "authorized to merge",
            "merge exception",
            "review exception",
            "approval exception",
        ]
        .iter()
        .any(|phrase| outside_review.contains(phrase));
    if second_merge_owner {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{MAINTAINERS}` contains merge-authorization policy outside its owner-link review block. Keep one owner in `{COORDINATION}`."
        ));
    }

    Ok(())
}

fn check_public_intake(root: &Path) -> Result<(), String> {
    let contributing = binding_prose(&read(root, CONTRIBUTING)?);
    for requirement in PUBLIC_INTAKE_OWNER_REQUIREMENTS {
        require_normalized(&contributing, requirement, CONTRIBUTING)?;
        require_unique_normalized(&contributing, requirement, CONTRIBUTING)?;
    }

    let workflow = read(root, WORKFLOW)?;
    let rendered = visible_markdown(&workflow);
    let public = section(&rendered, PUBLIC_INTAKE_HEADING, WORKFLOW)?;
    if public
        .lines()
        .any(|line| line.trim_start().starts_with("### "))
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{WORKFLOW}` `{PUBLIC_INTAKE_HEADING}` must stay one compact owner link and maintainer boundary."
        ));
    }
    let expected = format!(
        "{PUBLIC_INTAKE_HEADING}\n{}",
        WORKFLOW_PUBLIC_INTAKE_REQUIREMENTS.join("\n")
    );
    if normalize_binding(public) != normalize_binding(&expected) {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{WORKFLOW}` `{PUBLIC_INTAKE_HEADING}` duplicates or drifts from the `{CONTRIBUTING}` owner. Restore the canonical owner link and maintainer boundary."
        ));
    }

    for consumer in [BUG_TEMPLATE, FEATURE_TEMPLATE] {
        let text = read(root, consumer)?;
        let body = (exact_line_offsets(&text, "body:").len() == 1)
            .then(|| yaml_top_level_value(&text, "body:"))
            .flatten()
            .ok_or_else(|| {
                format!(
                    "ATOMIC-PROTOCOL: `{consumer}` must contain one top-level `body:` sequence."
                )
            })?;
        let visible = yaml_markdown_scalars(&body)
            .into_iter()
            .map(|scalar| visible_markdown(&scalar))
            .collect::<Vec<_>>()
            .join("\n");
        if normalized_occurrences(&visible, FORM_CONTRIBUTING_REQUIREMENT) != 1
            || text.matches(CONTRIBUTING_LINK).count() != 1
        {
            return Err(format!(
                "ATOMIC-PROTOCOL: `{consumer}` must link the `{CONTRIBUTING}` owner exactly once."
            ));
        }
    }
    let config = read(root, TEMPLATE_CONFIG)?;
    if exact_line_offsets(&config, "contact_links:").len() != 1
        || config.matches(CONFIG_CONTRIBUTING_BLOCK).count() != 1
        || config.matches(CONTRIBUTING_LINK).count() != 1
    {
        return Err(format!(
            "ATOMIC-PROTOCOL: `{TEMPLATE_CONFIG}` must keep the `{CONTRIBUTING}` owner as the first `contact_links` entry."
        ));
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
    session_protocol::check(root)?;
    let root_text = read(root, ROOT)?;
    check_root(&root_text)?;
    check_references(root)?;
    check_current_documentation_receipts(root)?;
    check_autonomous_merge(root)?;
    check_public_intake(root)?;
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

    pub(super) struct TempDir {
        pub(super) path: PathBuf,
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
        ) + ROOT_MERGE_REQUIREMENT
            + "\n"
    }

    fn fixture_research() -> String {
        format!(
            "# Research\n\n{CURRENT_DOCUMENTATION_HEADING}\n\n{}\n\n## Next\n",
            CURRENT_DOCUMENTATION_RESEARCH_REQUIREMENTS.join("\n")
        )
    }

    fn fixture_current_documentation_receipt() -> String {
        format!(
            "{CURRENT_DOCUMENTATION_HEADING}\n\n{}\n\n{}\n",
            CURRENT_DOCUMENTATION_COORDINATION_REQUIREMENTS.join("\n"),
            CURRENT_DOCUMENTATION_RECEIPT_FIELDS.join("\n"),
        )
    }

    fn fixture_coordination() -> String {
        format!(
            "# Coordination\n\n{PROMPT_TRACKER_HANDOFF_REQUIREMENT}\n\n{}\n{}\n\n## Next\n",
            fixture_current_documentation_receipt(),
            canonical_merge_owner(),
        )
    }

    fn fixture_dependencies() -> String {
        format!(
            "# Dependencies\n\n{DEPENDENCIES_AUTHORITATIVE_CHECKS_HEADING}\n\n{}\n\n## Next\n",
            CURRENT_DOCUMENTATION_DEPENDENCIES_REQUIREMENTS.join("\n"),
        )
    }

    fn fixture_docs() -> String {
        format!(
            "# Docs\n\n{DOCS_DIAGRAM_SELECTION_HEADING}\n\n{}\n\n## Next\n",
            CURRENT_DOCUMENTATION_DOCS_REQUIREMENTS.join("\n"),
        )
    }

    fn fixture_testing() -> String {
        format!(
            "# Testing\n\n{TESTING_HEADING}\n\n{}\n\n{TESTING_MERMAID_GATE_HEADING}\n\n{}\n\n## Next\n",
            TESTING_REQUIREMENTS.join("\n"),
            CURRENT_DOCUMENTATION_TESTING_REQUIREMENTS.join("\n"),
        )
    }

    pub(super) fn fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(ROOT, &valid_root());
        temp.write(
            WORKFLOW,
            &format!("# Workflow\n\n{PUBLIC_INTAKE_HEADING}\n\n{}\n\n## The loop (one issue, one agent, one concern)\n\n1. **Pick up and refresh.** Fetch the Linear issue (team KELD, current milestone first),\nroot `AGENTS.md` § Atomic problem-solving protocol. The same first comment MUST record the decision-bearing atoms: owner, boundary and inputs/outputs, failure mode, observable contract, independence from the other atoms, and first falsifier.\n2. **Spec gate.** Larger than a bug fix and no spec? Write one from\n3. **Isolate.** Work separately.\n4. **Implement and coordinate.** Tests with the change (conformance entries *first* for\nA material-decision comment MUST also record every atom changed or added by the decision, its independence edges and first falsifier.\n5. **Verify** (the gate from root `AGENTS.md`): fmt + clippy `-D warnings` + full test\n7. **PR and handoff.** {} {}\n\n## Next\n", WORKFLOW_PUBLIC_INTAKE_REQUIREMENTS.join("\n"), WORKFLOW_MERGE_REQUIREMENTS[0], WORKFLOW_MERGE_REQUIREMENTS[1]),
        );
        let workflow = fs::read_to_string(temp.path.join(WORKFLOW)).expect("read workflow fixture");
        let workflow = workflow
            .replacen(
                "and first falsifier.\n2. **Spec gate.**",
                &format!(
                    "and first falsifier.\n{}\n{}\n2. **Spec gate.**",
                    CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS[0],
                    CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS[1],
                ),
                1,
            )
            .replacen(
                "and first falsifier.\n5. **Verify**",
                &format!(
                    "and first falsifier.\n{}\n5. **Verify**",
                    CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS[2],
                ),
                1,
            );
        temp.write(WORKFLOW, &workflow);
        temp.write(COORDINATION, &fixture_coordination());
        temp.write(RESEARCH, &fixture_research());
        temp.write(DEPENDENCIES, &fixture_dependencies());
        temp.write(DOCS, &fixture_docs());
        temp.write(
            CONTRIBUTING,
            &format!(
                "# Contributing\n\n{}\n",
                PUBLIC_INTAKE_OWNER_REQUIREMENTS.join("\n")
            ),
        );
        let form = format!(
            "body:\n  - type: markdown\n    attributes:\n      value: |\n{FORM_CONTRIBUTING_LINE}\n"
        );
        temp.write(BUG_TEMPLATE, &form);
        temp.write(FEATURE_TEMPLATE, &form);
        temp.write(TEMPLATE_CONFIG, &format!("{CONFIG_CONTRIBUTING_BLOCK}\n"));
        temp.write(
            MAINTAINERS,
            &format!(
                "# Maintainers\n\n{MAINTAINER_REVIEW_START} {}\n{MAINTAINER_REVIEW_END} project direction stays public.\n",
                MAINTAINER_REVIEW_REQUIREMENTS.join(" ")
            ),
        );
        temp.write(REVIEW, &format!("# Review\n\n{REVIEW_MERGE_REQUIREMENT}\n"));
        temp.write(CI, &format!("# CI\n\n{CI_MERGE_REQUIREMENT}.\n"));
        temp.write(TESTING, &fixture_testing());
        temp.write(
            INDEX,
            "# Index\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Any non-trivial design, diagnosis, review, or implementation | Root `AGENTS.md` § Atomic problem-solving protocol. |\n\n## Next\n",
        );
        temp.write(
            INDEX,
            &format!(
                "# Index\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| {} | {} |\n| {} | research.md Current-documentation receipt; testing.md when behavior is exercised |\n\n## Next\n",
                INDEX_REQUIREMENTS[0],
                INDEX_REQUIREMENTS[1],
                CURRENT_DOCUMENTATION_ROUTE,
            ),
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
        session_protocol::seed_fixture(&temp.path);
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

    #[test]
    fn current_documentation_receipt_bindings_fail_when_removed() {
        for (path, requirements) in [
            (RESEARCH, CURRENT_DOCUMENTATION_RESEARCH_REQUIREMENTS),
            (
                COORDINATION,
                CURRENT_DOCUMENTATION_COORDINATION_REQUIREMENTS,
            ),
            (WORKFLOW, CURRENT_DOCUMENTATION_WORKFLOW_REQUIREMENTS),
            (COORDINATION, CURRENT_DOCUMENTATION_RECEIPT_FIELDS),
            (
                DEPENDENCIES,
                CURRENT_DOCUMENTATION_DEPENDENCIES_REQUIREMENTS,
            ),
            (DOCS, CURRENT_DOCUMENTATION_DOCS_REQUIREMENTS),
            (TESTING, CURRENT_DOCUMENTATION_TESTING_REQUIREMENTS),
        ] {
            for requirement in requirements {
                let temp = fixture();
                replace_requirement(&temp, path, requirement);
                let error = check(&temp.path).expect_err("removed receipt binding must fail");
                assert!(error.contains(requirement), "{error}");
            }
        }

        let temp = fixture();
        replace_requirement(&temp, INDEX, CURRENT_DOCUMENTATION_ROUTE);
        let error = check(&temp.path).expect_err("removed current-documentation route must fail");
        assert!(error.contains("index"), "{error}");
    }

    #[test]
    fn current_documentation_route_requires_a_canonical_two_cell_task_routing_row() {
        let route_row = format!(
            "| {CURRENT_DOCUMENTATION_ROUTE} | research.md Current-documentation receipt; testing.md when behavior is exercised |"
        );

        let temp = fixture();
        let index = fs::read_to_string(temp.path.join(INDEX)).expect("read index fixture");
        let malformed = index.replacen(
            &route_row,
            &format!(
                "| {CURRENT_DOCUMENTATION_ROUTE} | research.md Current-documentation receipt; testing.md when behavior is exercised | decoy |"
            ),
            1,
        );
        temp.write(INDEX, &malformed);
        let error = check(&temp.path).expect_err("three-cell route row must fail");
        assert!(error.contains("index must route"), "{error}");

        let temp = fixture();
        let index = fs::read_to_string(temp.path.join(INDEX)).expect("read index fixture");
        let moved = index
            .replacen(&route_row, "| Other route | other.md |", 1)
            .replace(
                "## Next",
                &format!("## Historical routing\n\n{route_row}\n\n## Next"),
            );
        temp.write(INDEX, &moved);
        let error = check(&temp.path).expect_err("out-of-table route row must fail");
        assert!(error.contains("index must route"), "{error}");
    }

    #[test]
    fn current_documentation_consumer_directive_cannot_move_to_a_historical_heading() {
        for historical_heading in [
            "### Historical receipt requirement",
            "# Historical receipt requirement",
            "Historical receipt requirement\n---",
        ] {
            for (path, requirement) in [
                (
                    DEPENDENCIES,
                    CURRENT_DOCUMENTATION_DEPENDENCIES_REQUIREMENTS[0],
                ),
                (DOCS, CURRENT_DOCUMENTATION_DOCS_REQUIREMENTS[1]),
                (TESTING, CURRENT_DOCUMENTATION_TESTING_REQUIREMENTS[1]),
            ] {
                let temp = fixture();
                let source =
                    fs::read_to_string(temp.path.join(path)).expect("read consumer fixture");
                let moved = source
                    .replacen(requirement, "historical directive omitted", 1)
                    .replace(
                        "## Next",
                        &format!("{historical_heading}\n\n{requirement}\n\n## Next"),
                    );
                temp.write(path, &moved);
                let error = check(&temp.path)
                    .expect_err("historical directive must not satisfy the consumer binding");
                assert!(error.contains(requirement), "{error}");
            }
        }
    }

    #[test]
    fn receipt_field_moved_to_history_does_not_satisfy_the_canonical_section() {
        let temp = fixture();
        let field = CURRENT_DOCUMENTATION_RECEIPT_FIELDS[2];
        let coordination = fs::read_to_string(temp.path.join(COORDINATION))
            .expect("read coordination fixture")
            .replacen(field, "historical field omitted", 1)
            + "\n## Historical receipt\n\n"
            + field
            + "\n";
        temp.write(COORDINATION, &coordination);
        let error = check(&temp.path).expect_err("historical receipt field must not satisfy owner");
        assert!(error.contains(field), "{error}");
    }

    #[test]
    fn receipt_owner_requirements_cannot_move_to_a_nested_history_heading() {
        let temp = fixture();
        let requirement = CURRENT_DOCUMENTATION_RESEARCH_REQUIREMENTS[0];
        let research = fs::read_to_string(temp.path.join(RESEARCH))
            .expect("read research fixture")
            .replacen(requirement, "historical requirement omitted", 1)
            .replace(
                "## Next",
                &format!("### Historical receipt requirement\n\n{requirement}\n\n## Next"),
            );
        temp.write(RESEARCH, &research);
        let error = check(&temp.path)
            .expect_err("nested historical research requirement must not satisfy its owner");
        assert!(error.contains(requirement), "{error}");

        let temp = fixture();
        let field = CURRENT_DOCUMENTATION_RECEIPT_FIELDS[2];
        let coordination = fs::read_to_string(temp.path.join(COORDINATION))
            .expect("read coordination fixture")
            .replacen(field, "historical field omitted", 1)
            .replace(
                "## Standing autonomous merge delegation",
                &format!(
                    "### Historical receipt requirement\n\n{field}\n\n## Standing autonomous merge delegation"
                ),
            );
        temp.write(COORDINATION, &coordination);
        let error = check(&temp.path)
            .expect_err("nested historical coordination field must not satisfy its owner");
        assert!(error.contains(field), "{error}");
    }

    #[test]
    fn every_autonomous_merge_predicate_is_independently_enforced() {
        for requirement in MERGE_OWNER_REQUIREMENTS {
            let temp = fixture();
            replace_requirement(&temp, COORDINATION, requirement);
            let error = check(&temp.path).expect_err("removed merge predicate must fail");
            assert!(error.contains(requirement), "{error}");
        }
    }

    #[test]
    fn merge_identity_scope_and_fallback_are_enforced() {
        for (from, to) in [
            ("author and merger", "merger"),
            (
                "`0monish` or `amishabenramani`",
                "`0monish`, `amishabenramani`, or `another-actor`",
            ),
            ("verify both before merge", "verify after merge"),
            ("others need human review", "others may skip human review"),
        ] {
            let temp = fixture();
            let coordination = fs::read_to_string(temp.path.join(COORDINATION))
                .expect("read coordination fixture")
                .replacen(from, to, 1);
            temp.write(COORDINATION, &coordination);
            let error = check(&temp.path).expect_err("weakened merge identity rule must fail");
            assert!(error.contains(COORDINATION), "{error}");
        }

        for coordination in [
            fixture_coordination().replace(
                "others need human review.",
                "others need human review. Exception: `another-actor` may use the waiver.",
            ),
            fixture_coordination().replace(
                "others need human review.",
                "others need human review. For all other actors, human review is optional.",
            ),
            fixture_coordination().replace(
                MERGE_OWNER_REQUIREMENTS[1],
                &format!(
                    "Waive extra human review whenever the merger is `0monish`.\n\n### Historical wording\n\n{}",
                    MERGE_OWNER_REQUIREMENTS[1]
                ),
            ),
        ] {
            let temp = fixture();
            temp.write(COORDINATION, &coordination);
            let error = check(&temp.path)
                .expect_err("additive or historical merge identity contradiction must fail");
            assert!(error.contains(COORDINATION), "{error}");
        }
    }

    #[test]
    fn maintainer_review_links_one_merge_owner() {
        for (from, to) in [
            (
                ".agents/coordination.md#standing-autonomous-merge-delegation",
                ".agents/coordination.md#another-section",
            ),
            (
                MAINTAINER_REVIEW_REQUIREMENTS[2],
                "The narrow exception is maintained independently in this file.",
            ),
        ] {
            let temp = fixture();
            let maintainers = fs::read_to_string(temp.path.join(MAINTAINERS))
                .expect("read maintainer fixture")
                .replacen(from, to, 1);
            temp.write(MAINTAINERS, &maintainers);
            let error = check(&temp.path).expect_err("missing merge-owner link must fail");
            assert!(error.contains(MAINTAINERS), "{error}");
        }

        for insertion in [
            " PRs authored by `another-actor` may also skip human review.",
            &format!(
                "\n### Historical exception\n\n{}",
                MAINTAINER_REVIEW_REQUIREMENTS[2]
            ),
        ] {
            let temp = fixture();
            let maintainers = fs::read_to_string(temp.path.join(MAINTAINERS))
                .expect("read maintainer fixture")
                .replace(
                    MAINTAINER_REVIEW_END,
                    &format!("{insertion}\n{MAINTAINER_REVIEW_END}"),
                );
            temp.write(MAINTAINERS, &maintainers);
            let error = check(&temp.path).expect_err("duplicated merge policy must fail");
            assert!(error.contains(MAINTAINERS), "{error}");
        }

        let temp = fixture();
        let maintainers = fs::read_to_string(temp.path.join(MAINTAINERS))
            .expect("read maintainer fixture")
            .replace(
                MAINTAINER_REVIEW_REQUIREMENTS[2],
                &format!("<!-- {} -->", MAINTAINER_REVIEW_REQUIREMENTS[2]),
            );
        temp.write(MAINTAINERS, &maintainers);
        let error = check(&temp.path).expect_err("commented owner link must not count");
        assert!(error.contains(MAINTAINERS), "{error}");

        let temp = fixture();
        let maintainers = fs::read_to_string(temp.path.join(MAINTAINERS))
            .expect("read maintainer fixture")
            + "\n## Emergency merge exception\n\nPRs authored by `another-actor` may skip human review.\n";
        temp.write(MAINTAINERS, &maintainers);
        let error = check(&temp.path).expect_err("a second merge-policy owner must fail");
        assert!(error.contains(MAINTAINERS), "{error}");

        for rule in [
            "Another actor may waive review.",
            "Another actor can bypass human approval.",
            "Another actor may merge without human review.",
            "For another actor, approval is optional.",
        ] {
            let temp = fixture();
            let maintainers = fs::read_to_string(temp.path.join(MAINTAINERS))
                .expect("read maintainer fixture")
                + &format!("\n## Alternate policy\n\n{rule}\n");
            temp.write(MAINTAINERS, &maintainers);
            let error = check(&temp.path).expect_err("authorization prose outside owner must fail");
            assert!(error.contains(MAINTAINERS), "{error}");
        }

        let temp = fixture();
        let maintainers = fs::read_to_string(temp.path.join(MAINTAINERS))
            .expect("read maintainer fixture")
            + "\n## Community\n\nMaintainers merge community feedback into plans and document exception handling while reviewing proposals.\n";
        temp.write(MAINTAINERS, &maintainers);
        check(&temp.path).expect("unrelated maintainer prose remains allowed");
    }

    #[test]
    fn public_intake_owner_and_consumers_are_enforced() {
        for requirement in PUBLIC_INTAKE_OWNER_REQUIREMENTS {
            let temp = fixture();
            replace_requirement(&temp, CONTRIBUTING, requirement);
            let error = check(&temp.path).expect_err("missing public-intake owner rule must fail");
            assert!(error.contains(CONTRIBUTING), "{error}");
        }

        for consumer in [BUG_TEMPLATE, FEATURE_TEMPLATE, TEMPLATE_CONFIG] {
            let temp = fixture();
            replace_requirement(&temp, consumer, CONTRIBUTING_LINK);
            let error = check(&temp.path).expect_err("missing public-intake owner link must fail");
            assert!(error.contains(consumer), "{error}");

            let temp = fixture();
            let text = fs::read_to_string(temp.path.join(consumer))
                .expect("read public-intake consumer")
                .replace(CONTRIBUTING_LINK, "https://github.com/gyldlab/keld/issues");
            let decoy = format!(
                "description: |\n  - type: markdown\n    attributes:\n      value: |\n{FORM_CONTRIBUTING_LINE}\n"
            ) + &text;
            temp.write(consumer, &decoy);
            let error = check(&temp.path)
                .expect_err("a top-level scalar outside `body` must not supply the owner link");
            assert!(error.contains(consumer), "{error}");
        }

        for consumer in [BUG_TEMPLATE, FEATURE_TEMPLATE] {
            for replacement in [
                format!("# {CONTRIBUTING_LINK}"),
                format!("<!-- {CONTRIBUTING_LINK} -->"),
                format!("        <!--\n{FORM_CONTRIBUTING_LINE}\n        -->"),
            ] {
                let temp = fixture();
                let text = fs::read_to_string(temp.path.join(consumer))
                    .expect("read public-intake consumer")
                    .replace(FORM_CONTRIBUTING_LINE, &replacement);
                temp.write(consumer, &text);
                let error = check(&temp.path)
                    .expect_err("commented public-intake owner link must not count");
                assert!(error.contains(consumer), "{error}");
            }

            let temp = fixture();
            let text = fs::read_to_string(temp.path.join(consumer))
                .expect("read public-intake consumer")
                .replace(CONTRIBUTING_LINK, "https://github.com/gyldlab/keld/issues")
                + &format!("\ndecoy: |\n{FORM_CONTRIBUTING_LINE}\n");
            temp.write(consumer, &text);
            let error = check(&temp.path)
                .expect_err("a non-markdown literal-scalar link decoy must not count");
            assert!(error.contains(consumer), "{error}");
        }

        let temp = fixture();
        let config = fs::read_to_string(temp.path.join(TEMPLATE_CONFIG))
            .expect("read template config fixture")
            .replace(
                CONFIG_CONTRIBUTING_LINE,
                "    url: https://github.com/gyldlab/keld/issues",
            )
            + &format!("\ndecoy: |\n{CONFIG_CONTRIBUTING_LINE}\n");
        temp.write(TEMPLATE_CONFIG, &config);
        let error = check(&temp.path).expect_err("a literal-scalar config decoy must not count");
        assert!(error.contains(TEMPLATE_CONFIG), "{error}");

        let temp = fixture();
        let bug = fs::read_to_string(temp.path.join(BUG_TEMPLATE))
            .expect("read bug fixture")
            .replace(
                FORM_CONTRIBUTING_LINE,
                &format!("        # literal Markdown heading\n{FORM_CONTRIBUTING_LINE}"),
            );
        temp.write(BUG_TEMPLATE, &bug);
        check(&temp.path).expect("a hash inside block-scalar content is not a YAML comment");

        let temp = fixture();
        let contributing = fs::read_to_string(temp.path.join(CONTRIBUTING))
            .expect("read contribution fixture")
            .replace(
                PUBLIC_INTAKE_OWNER_REQUIREMENTS[1],
                "You must obtain private Linear, research, and agent-memory access.",
            );
        temp.write(CONTRIBUTING, &contributing);
        let error = check(&temp.path).expect_err("inverted public-access rule must fail");
        assert!(error.contains(CONTRIBUTING), "{error}");

        let temp = fixture();
        let workflow = fs::read_to_string(temp.path.join(WORKFLOW))
            .expect("read workflow fixture")
            .replace(
                "\n\n## The loop",
                " External contributors must also use private Linear.\n\n## The loop",
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(&temp.path).expect_err("duplicated public policy must fail");
        assert!(error.contains(WORKFLOW), "{error}");
    }

    #[test]
    fn autonomous_merge_consumers_and_override_are_enforced() {
        for (path, requirement) in [
            (ROOT, ROOT_MERGE_REQUIREMENT),
            (WORKFLOW, WORKFLOW_MERGE_REQUIREMENTS[0]),
            (WORKFLOW, WORKFLOW_MERGE_REQUIREMENTS[1]),
            (REVIEW, REVIEW_MERGE_REQUIREMENT),
            (CI, CI_MERGE_REQUIREMENT),
            (COORDINATION, PROMPT_TRACKER_HANDOFF_REQUIREMENT),
        ] {
            let temp = fixture();
            replace_requirement(&temp, path, requirement);
            let error = check(&temp.path).expect_err("removed merge consumer must fail");
            assert!(error.contains(path), "{error}");
        }

        for value in ["human-decide", "Human-Decide", "HUMAN-DECIDE"] {
            let temp = fixture();
            let coordination = fs::read_to_string(temp.path.join(COORDINATION))
                .expect("read coordination fixture")
                .replacen(
                    "Default eligible merge: `merge-when-complete`.",
                    &format!("Default eligible merge: `{value}`."),
                    1,
                );
            temp.write(COORDINATION, &coordination);
            let error = check(&temp.path).expect_err("human-decide default must fail");
            assert!(error.contains("human-decide"), "{error}");
        }

        for value in ["human-decide", "Human-Decide", "HUMAN-DECIDE"] {
            let temp = fixture();
            let workflow = fs::read_to_string(temp.path.join(WORKFLOW))
                .expect("read workflow fixture")
                .replace(
                    "\n## Next",
                    &format!("\nFallback merge state: {value}.\n\n## Next"),
                );
            temp.write(WORKFLOW, &workflow);
            let error = check(&temp.path).expect_err("human-decide workflow state must fail");
            assert!(error.contains("human-decide"), "{error}");
        }

        let temp = fixture();
        let coordination = fs::read_to_string(temp.path.join(COORDINATION))
            .expect("read coordination fixture")
            .replace(
                "\n## Next",
                "\n## Historical\n\nHistorical explanation: human-decide was the retired default.\n\n## Next",
            );
        temp.write(COORDINATION, &coordination);
        check(&temp.path).expect("explanatory text must not become the active default");

        let temp = fixture();
        let workflow = fs::read_to_string(temp.path.join(WORKFLOW)).expect("read workflow");
        let moved = workflow
            .replacen(WORKFLOW_MERGE_REQUIREMENTS[0], "retired handoff pointer", 1)
            .replacen(WORKFLOW_MERGE_REQUIREMENTS[1], "retired merge action", 1)
            .replace(
                "\n## Next",
                &format!(
                    "\n## Historical\n\n{} {}\n\n## Next",
                    WORKFLOW_MERGE_REQUIREMENTS[0], WORKFLOW_MERGE_REQUIREMENTS[1]
                ),
            );
        temp.write(WORKFLOW, &moved);
        check(&temp.path).expect_err("historical merge prose must not satisfy step 7");
    }

    #[test]
    fn hidden_merge_policy_cannot_satisfy_the_contract() {
        let requirement = MERGE_OWNER_REQUIREMENTS[2];
        for hidden in [
            format!("<!-- {requirement} -->"),
            format!("```text\n{requirement}\n```"),
            format!("> {requirement}"),
        ] {
            let temp = fixture();
            let coordination = fs::read_to_string(temp.path.join(COORDINATION))
                .expect("read coordination fixture")
                .replacen(requirement, &hidden, 1);
            temp.write(COORDINATION, &coordination);
            check(&temp.path).expect_err("hidden merge predicate must not count");
        }
    }
}
