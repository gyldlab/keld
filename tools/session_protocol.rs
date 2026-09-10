//! Documentary ownership checks; actual receipt validation is session_closeout.py.

use std::path::Path;

use super::{
    binding_prose, canonical_task_routing_rows, direct_section, normalize_binding, read,
    require_normalized, visible_markdown,
};

const OWNER: &str = "docs/agents/workflow.md";
const HEADING: &str = "## Session continuity and closeout";
const RULES: &[&str] = &[
    "including research-only audits",
    "A merged follow-up MUST NOT erase its originating audit objective.",
    "Every material finding MUST have a disposition",
    "then re-fetch and verify their content",
    "Unresolved work requires a bounded handoff",
    "Run `just session-closeout <receipt>` after the final state change.",
    "the validator cannot discover omitted work or authenticate an agent-written remote receipt",
];
const ROUTE: &str = "Starting, resuming, or closing non-trivial repository work";
const RESEARCH_LINK: &str = "(../docs/agents/workflow.md#session-continuity-and-closeout)";
const REPAIR_RULES: &[&str] = &[
    "At affected milestones, session closeout and after protocol changes",
    "capture a counterexample; search/reuse the current owner before adding a rule.",
    "prove a failing control before editing",
    "independently review/evaluate with all applicable gates",
    "then adopt or roll back against acceptance.",
    "Retire ineffective/duplicate rules through the same reviewed process",
    "No authority expansion, weakened gates or unbounded rules.",
];

pub fn check(root: &Path) -> Result<(), String> {
    let text = read(root, OWNER)?;
    let visible = visible_markdown(&text);
    let section = binding_prose(direct_section(&visible, HEADING, OWNER)?);
    for rule in RULES {
        require_normalized(&section, rule, OWNER)?;
        for other in [
            "AGENTS.md",
            ".agents/coordination.md",
            ".agents/research.md",
        ] {
            if normalize_binding(&read(root, other)?).contains(&normalize_binding(rule)) {
                return Err(format!(
                    "SESSION-PROTOCOL: {other} duplicates {OWNER}: {rule}"
                ));
            }
        }
    }
    let router = visible_markdown(&read(root, ".agents/index.md")?);
    if !canonical_task_routing_rows(&router)?
        .iter()
        .any(|(task, target)| {
            task.contains(ROUTE) && target.contains("(../docs/agents/workflow.md)")
        })
    {
        return Err("SESSION-PROTOCOL: missing all-session lifecycle route".into());
    }
    require_normalized(
        &binding_prose(&read(root, ".agents/research.md")?),
        RESEARCH_LINK,
        ".agents/research.md",
    )?;
    let instructions = visible_markdown(&read(root, ".agents/instructions.md")?);
    let repair = direct_section(
        &instructions,
        "## Feedback and repair",
        ".agents/instructions.md",
    )?;
    for rule in REPAIR_RULES {
        require_normalized(repair, rule, ".agents/instructions.md")?;
    }
    if !canonical_task_routing_rows(&router)?
        .iter()
        .any(|(task, target)| {
            task.contains("Observed protocol failure") && target.contains("(instructions.md)")
        })
    {
        return Err("SESSION-PROTOCOL: missing protocol-failure repair route".into());
    }
    Ok(())
}

#[cfg(test)]
pub fn seed_fixture(root: &Path) {
    use std::fs;
    for (path, extra) in [
        (OWNER, format!("\n{HEADING}\n\n{}\n", RULES.join("\n"))),
        (".agents/research.md", format!("\n{RESEARCH_LINK}\n")),
    ] {
        let file = root.join(path);
        let mut text = fs::read_to_string(&file).expect("fixture owner exists");
        text.push_str(&extra);
        fs::write(file, text).expect("seed session owner");
    }
    let file = root.join(".agents/index.md");
    let text = fs::read_to_string(&file)
        .expect("fixture router exists")
        .replace(
            "## Next",
            &format!("| {ROUTE} | [workflow](../docs/agents/workflow.md) |\n\n## Next"),
        );
    fs::write(file, text).expect("seed session route");
    fs::write(
        root.join(".agents/instructions.md"),
        format!(
            "# Instructions\n\n## Feedback and repair\n\n{}\n",
            REPAIR_RULES.join("\n")
        ),
    )
    .expect("seed repair owner");
    let file = root.join(".agents/index.md");
    let text = fs::read_to_string(&file).expect("read router").replace(
        "## Next",
        "| Observed protocol failure | [instructions](instructions.md) |\n\n## Next",
    );
    fs::write(file, text).expect("seed repair route");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_commented_or_duplicated_obligation_is_rejected() {
        for rule in RULES {
            for mode in ["missing", "comment", "duplicate"] {
                let temp = super::super::tests::fixture();
                let path = temp.path.join(OWNER);
                let text = fs::read_to_string(&path).expect("read owner");
                let replaced = match mode {
                    "comment" => text.replace(rule, &format!("<!-- {rule} -->")),
                    "duplicate" => {
                        let other = temp.path.join(".agents/research.md");
                        let text = fs::read_to_string(&other).expect("read consumer");
                        fs::write(other, format!("{text}\n{rule}\n")).expect("duplicate rule");
                        text
                    }
                    _ => text.replace(rule, ""),
                };
                if mode != "duplicate" {
                    fs::write(path, replaced).expect("mutate owner");
                }
                assert!(check(&temp.path).is_err(), "survived {mode}: {rule}");
            }
        }
    }

    #[test]
    fn missing_or_hidden_audit_route_is_rejected() {
        for hide in [false, true] {
            let temp = super::super::tests::fixture();
            let path = temp.path.join(".agents/index.md");
            let text = fs::read_to_string(&path).expect("read route");
            let text = text
                .lines()
                .map(|line| {
                    if line.contains(ROUTE) {
                        if hide {
                            format!("<!-- {line} -->")
                        } else {
                            String::new()
                        }
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(path, text).expect("mutate route");
            assert!(check(&temp.path).is_err());
        }
    }

    #[test]
    fn repair_cannot_drop_failure_proof_review_or_rollback() {
        for rule in REPAIR_RULES {
            let temp = super::super::tests::fixture();
            let path = temp.path.join(".agents/instructions.md");
            let text = fs::read_to_string(&path).expect("read repair owner");
            fs::write(path, text.replace(rule, "")).expect("remove repair guard");
            assert!(check(&temp.path).is_err(), "survived removal: {rule}");
        }
    }
}
