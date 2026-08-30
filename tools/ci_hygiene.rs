//! KEL-39 contract check: CODEOWNERS, PR/issue templates, secret scan, Action SHAs.
//!
//! Not a Cargo member (no lockfile change). Compile with:
//! `rustc --edition=2024 -D warnings tools/ci_hygiene.rs`
//! Error text states the fix. Codes are not `KELD-*` so this file does not
//! collide with `docs/engineering/keld-error-codes.md` (owned by a parallel change).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CODEOWNERS: &str = ".github/CODEOWNERS";
const PR_TEMPLATE: &str = ".github/PULL_REQUEST_TEMPLATE.md";
const ISSUE_DIR: &str = ".github/ISSUE_TEMPLATE";
const WORKFLOW: &str = ".github/workflows/ci.yml";
const KELDBOT_WORKFLOW: &str = ".github/workflows/keldbot.yml";
const CI_REQUIRED_EVALUATOR: &str = "tools/ci_required.sh";
const GITIGNORE: &str = ".gitignore";
const NEXTEST_CONFIG: &str = ".config/nextest.toml";
const MERMAID_CHECKER: &str = "tools/mermaid_docs.rs";
const MERMAID_RENDERER: &str = "tools/mermaid_render_check.sh";
const MERMAID_CONFIG: &str = "tools/mermaid-render-config.json";
const MERMAID_IMAGE_DIGEST: &str =
    "sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf";

const REQUIRED_OWNER_PATHS: &[&str] = &[
    "crates/keld-guard",
    "crates/keld-ipc",
    "Cargo.toml",
    ".github",
    "AGENTS.md",
    ".agents",
    "docs/agents",
    "keld-error-codes.md",
    "justfile",
    "tools",
];

const PR_NEEDLES: &[&str] = &[
    "## Summary",
    "## Spec refs",
    "## Review gates",
    "## Tests",
    "## Platforms",
    "## Perf impact",
    "No boundary change",
    "agent/kel-",
    "cargo fmt",
    "clippy",
    "nextest",
    "mermaid-render-check",
];

const WORKFLOW_TEXT_NEEDLES: &[&str] = &[
    "gitleaks detect",
    "sha256sum -c",
    "--test tools/ci_hygiene.rs",
    "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb",
    "toolchain: 1.97.1",
    "tools/llms_docs.rs",
    "llms-docs check",
];

const WORKFLOW_RUN_NEEDLES: &[&str] = &[
    "--test tools/mermaid_docs.rs",
    "mermaid-docs check .",
    "tools/mermaid_render_check.sh",
];

fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "CI-HYGIENE: missing `{}`: {error}. Restore the KEL-39 file from git or recreate it.",
            path.display()
        )
    })
}

fn github_dir_is_ignored(gitignore: &str) -> bool {
    gitignore.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        // `/.github/*` ignores children of `.github/` (CODEOWNERS, workflows/,
        // ISSUE_TEMPLATE/), which is enough for GitHub to never see CI files.
        matches!(
            line,
            "/.github/"
                | "/.github"
                | ".github/"
                | ".github"
                | "/.github/**"
                | "/.github/*"
                | ".github/**"
                | ".github/*"
        )
    })
}

fn uncommented_codeowners_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    })
}

fn codeowners_covers(text: &str, needle: &str) -> bool {
    uncommented_codeowners_lines(text).any(|line| line.contains(needle) && line.contains('@'))
}

fn uncommented_line_contains(text: &str, needle: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains(needle)
    })
}

fn yaml_content(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let content = trimmed
        .split_once(" #")
        .map_or(trimmed, |(before, _)| before)
        .trim_end();
    Some((indent, content))
}

fn workflow_has_checkout_persist_credentials_false(text: &str) -> bool {
    let mut checkout_indent = None;
    let mut with_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content.starts_with("- uses:") {
            checkout_indent = content.contains("actions/checkout@").then_some(indent);
            with_indent = None;
            continue;
        }
        let Some(checkout) = checkout_indent else {
            continue;
        };
        if indent <= checkout {
            checkout_indent = None;
            with_indent = None;
            continue;
        }
        if content == "with:" {
            with_indent = Some(indent);
            continue;
        }
        if let Some(with) = with_indent {
            if indent <= with {
                with_indent = None;
            } else if let Some(value) = content.strip_prefix("persist-credentials:") {
                let value = value
                    .trim()
                    .trim_matches(|character| character == '\'' || character == '"');
                if value == "false" {
                    return true;
                }
            }
        }
    }
    false
}

fn workflow_has_checkout_fetch_depth_zero(text: &str) -> bool {
    let mut checkout_indent = None;
    let mut with_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content.starts_with("- uses:") {
            checkout_indent = content.contains("actions/checkout@").then_some(indent);
            with_indent = None;
            continue;
        }
        let Some(checkout) = checkout_indent else {
            continue;
        };
        if indent <= checkout {
            checkout_indent = None;
            with_indent = None;
            continue;
        }
        if content == "with:" {
            with_indent = Some(indent);
            continue;
        }
        if let Some(with) = with_indent {
            if indent <= with {
                with_indent = None;
            } else if let Some(value) = content.strip_prefix("fetch-depth:") {
                let value = value
                    .trim()
                    .trim_matches(|character| character == '\'' || character == '"');
                if value == "0" {
                    return true;
                }
            }
        }
    }
    false
}

fn workflow_has_unconditional_named_step(text: &str, step_name: &str, command: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let expected_command = format!("run: {command}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content.starts_with("if:") || content.starts_with("- if:") {
            return false;
        }
        if content == expected_command {
            return true;
        }
    }
    false
}

fn workflow_named_step_has_property(text: &str, step_name: &str, property: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content == property {
            return true;
        }
    }
    false
}

fn workflow_named_step_contains(text: &str, step_name: &str, needle: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content.contains(needle) {
            return true;
        }
    }
    false
}

fn workflow_job_block<'a>(text: &'a str, job_name: &str) -> Option<String> {
    let target = format!("{job_name}:");
    let mut found = false;
    let mut block = String::new();

    for line in text.lines() {
        let parsed = yaml_content(line);
        if !found {
            if matches!(parsed, Some((2, ref content)) if content == &target) {
                found = true;
                block.push_str(line);
                block.push('\n');
            }
            continue;
        }
        if matches!(parsed, Some((indent, _)) if indent <= 2) {
            break;
        }
        block.push_str(line);
        block.push('\n');
    }

    found.then_some(block)
}

fn workflow_job_level_if(block: &str) -> Option<String> {
    for line in block.lines() {
        let Some((_indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            return None;
        }
        if let Some(value) = content.strip_prefix("if:") {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn check_change_router_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "changes") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `changes` job. Restore the always-created change router; do not use workflow-level path filters for required CI."
        ));
    };
    if !uncommented_line_contains(&block, "name: change router") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` job is missing `name: change router`. Keep the router's owner visible in that job."
        ));
    }
    if !workflow_has_checkout_fetch_depth_zero(&block) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` job must use `actions/checkout` with `fetch-depth: 0`. The router needs both PR/push comparison commits; do not borrow a full-history checkout from another job."
        ));
    }
    for (step_name, command) in [
        ("Router contract tests", "tools/ci_changes_test.sh"),
        (
            "Classify changed-path ownership",
            "tools/ci_changes.sh github",
        ),
    ] {
        if !workflow_has_unconditional_named_step(&block, step_name, command) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `changes` job must have an unconditional `{step_name}` step whose exact command is `{command}`. Put it in the router job; a shell guard, GitHub `if`, or identical command in another job cannot prove routing."
            ));
        }
    }
    Ok(())
}

fn check_package_loop_shell(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `check` job. Restore the cross-platform package verification job."
        ));
    };
    for step_name in ["clippy (warnings deny)", "test"] {
        if !workflow_named_step_has_property(&block, step_name, "shell: bash") {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `{step_name}` must set `shell: bash`. Its package loop is Bash syntax; the Windows default PowerShell shell rejects it before Cargo runs."
            ));
        }
    }
    if !workflow_named_step_contains(&block, "test", "--no-tests=pass") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `test` must use `cargo nextest --no-tests=pass` for selected zero-test packages. Package routing must preserve the workspace suite's valid zero-test behavior, not fail before consumer tests run."
        ));
    }
    Ok(())
}

fn check_check_job_if_avoids_matrix(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `check` job. Restore the cross-platform package verification job."
        ));
    };
    let mut if_indent = None;
    let mut if_text = String::new();
    for line in block.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            break;
        }
        if let Some(if_at) = if_indent {
            if indent <= if_at {
                break;
            }
            if_text.push_str(content);
            if_text.push('\n');
            continue;
        }
        if let Some(value) = content.strip_prefix("if:") {
            if_indent = Some(indent);
            if_text.push_str(value.trim());
            if_text.push('\n');
        }
    }
    if if_text.contains("matrix.") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` job-level `if` must not use `matrix`. GitHub evaluates `jobs.<job_id>.if` before matrix expansion (contexts: github, needs, vars, inputs only). Referencing `matrix.os` invalidates the workflow so rustc never starts on any OS. Keep OS filters on steps, which do have `matrix`."
        ));
    }
    Ok(())
}

fn check_msrv_avoids_apt(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "msrv") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `msrv` job. Restore MSRV `cargo check`; do not drop the rustc version gate to avoid Ubuntu apt."
        ));
    };
    if !uncommented_line_contains(&block, "runs-on: macos-latest") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `msrv` must `runs-on: macos-latest`. MSRV is a rustc version gate; do not install WebKitGTK via `apt-get` on Ubuntu for it."
        ));
    }
    if uncommented_line_contains(&block, "apt-get") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `msrv` must not call `apt-get`. WebKitGTK belongs on Linux GUI smoke / Ubuntu clippy when `keld-wv` actually links, not on the MSRV rustc job."
        ));
    }
    Ok(())
}

/// Asserts the `bun-test` lane's decidable properties only: that it exists, is
/// gated on the router's own output, pins its Bun version, and does not open a
/// second live apt lane. Each of those is a fact about the workflow text.
///
/// What this deliberately does not assert — that the lane really runs the
/// suites the router selected — is tracked in KEL-115, because it is a
/// behavioural property and no reading of the text settles it.
fn check_bun_test_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "bun-test") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `bun-test` job. The `packages/` TypeScript suites need a lane of their own: the Bun install inside `check` exists so Rust tests can spawn Bun children and never runs `bun test`."
        ));
    };
    if !uncommented_line_contains(&block, "if: needs.changes.outputs.ts == 'true'") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must be gated on `if: needs.changes.outputs.ts == 'true'`. The router owns which diffs reach this lane; an ungated job wastes runners and a gate on another output silently never runs (contexts: needs, github, vars, inputs only — never `matrix`)."
        ));
    }
    if !uncommented_line_contains(&block, "bun-version: \"1.4.0\"") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must pin `bun-version: \"1.4.0\"` (KEL-77). A floating `latest` silently changes the runtime under the suite."
        ));
    }
    // Deliberately NOT checked here: that the lane actually executes the suite
    // over the router's selection. Whether a shell script runs a command is not
    // decidable from its text, and every attempt to assert it by pattern was
    // broken from outside the pattern's frame — a piped `echo`, `if: false`,
    // `continue-on-error`, `|| true`, a `for` loop whose body ignores the loop
    // variable, the selection variable reassigned inside the script being
    // parsed, and `continue` / `exit 0` making the suite unreachable. Each of
    // those passed a check written specifically to catch the previous one.
    //
    // A guard that looks authoritative while being bypassable is worse than an
    // absent one, because it is read as proof. KEL-115 replaces this with a
    // receipt: the lane reports which suites it ran, and that is compared
    // against the router's `ts_packages` output. Behaviour, not text.
    if uncommented_line_contains(&block, "apt-get") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must not call `apt-get`. Bun ships from `oven-sh/setup-bun`; a second live Ubuntu apt lane contends with Linux GUI smoke on the Azure mirrors."
        ));
    }
    Ok(())
}

fn check_required_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "required") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `required` job. Add one always-created merge-admission result that consumes every conditional lane plus unconditional gitleaks."
        ));
    };
    if !uncommented_line_contains(&block, "name: CI required") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` must publish the stable check name `CI required` for branch protection."
        ));
    }
    if workflow_job_level_if(&block).as_deref() != Some("${{ always() }}") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` must use job-level `if: ${{{{ always() }}}}`. Otherwise a failed/cancelled dependency skips the merge decision instead of making it fail."
        ));
    }
    for needed in [
        "- changes",
        "- fmt",
        "- check",
        "- bun-test",
        "- linux-gui-smoke",
        "- msrv",
        "- deny",
        "- secrets",
        "- hygiene",
    ] {
        if !uncommented_line_contains(&block, needed) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `required.needs` is missing `{needed}`. The required result must observe every routed lane plus unconditional gitleaks."
            ));
        }
    }
    for command in ["tools/ci_required.sh test", "tools/ci_required.sh check"] {
        if !workflow_named_step_contains(&block, "Verify required CI results", command) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `required` must execute `{command}` in `Verify required CI results`. Mentioning or running it in another job does not decide the exact head."
            ));
        }
    }
    for expression in [
        "KELD_RESULT_BUN: ${{ needs['bun-test'].result }}",
        "KELD_RESULT_GUI: ${{ needs['linux-gui-smoke'].result }}",
    ] {
        if !uncommented_line_contains(&block, expression) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `required` is missing `{expression}`. Hyphenated job ids require bracket access in GitHub expressions; do not let a syntax error erase the merge gate."
            ));
        }
    }
    if !workflow_has_checkout_persist_credentials_false(&block) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` checkout must set `persist-credentials: false`; the merge-decision job needs repository bytes, not push authority."
        ));
    }
    Ok(())
}

fn check_keldbot_workflow(root: &Path) -> Result<(), String> {
    let text = read(root, KELDBOT_WORKFLOW)?;
    for needle in [
        "pull_request_target:",
        "types: [opened, edited, synchronize]",
        "permissions: {}",
    ] {
        if !uncommented_line_contains(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` is missing `{needle}`. Keep final-head metadata validation on opened, edited, and synchronize with a default-deny token."
            ));
        }
    }
    if uncommented_line_contains(&text, "actions/checkout@")
        || uncommented_line_contains(&text, "run:")
    {
        return Err(format!(
            "CI-HYGIENE: `{KELDBOT_WORKFLOW}` uses `pull_request_target` with a repository checkout or shell `run:` step. Keep PR-controlled code off this secret-bearing workflow; validate proposed workflow bytes in unprivileged CI instead."
        ));
    }
    for job in ["gatekeeper", "title-lint"] {
        let Some(block) = workflow_job_block(&text, job) else {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` has no `{job}` job. Restore the required PR metadata check."
            ));
        };
        if let Some(condition) = workflow_job_level_if(&block) {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` `{job}` has job-level `if: {condition}`. Required metadata checks must run again on synchronize; a skipped job satisfies branch protection without revalidating the final head."
            ));
        }
    }
    for heading in [
        "Summary",
        "Spec refs",
        "Review gates",
        "Tests",
        "Platforms",
        "Perf impact",
    ] {
        if !text.contains(&format!("\"{heading}\"")) {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` gatekeeper omits required PR heading `{heading}`. Keep it aligned with `{PR_TEMPLATE}`."
            ));
        }
    }
    let unpinned = action_uses_unpinned(&text);
    if !unpinned.is_empty() {
        return Err(format!(
            "CI-HYGIENE: `{KELDBOT_WORKFLOW}` has unpinned `uses:` entries. Pin every action to a 40-character commit SHA; offenders: {unpinned:?}"
        ));
    }
    Ok(())
}

fn executable_run_fragment_contains(fragment: &str, needle: &str) -> bool {
    if !fragment.contains(needle) {
        return false;
    }
    let command = fragment.trim();
    !(command.starts_with("echo ") && !command.contains('|'))
        && !command.starts_with("Write-Output ")
}

fn workflow_has_executable_run_needle(text: &str, needle: &str) -> bool {
    let mut run_indent = None;
    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content.starts_with("- run:") || content.starts_with("run:") {
            run_indent = Some(indent);
            let command = content
                .strip_prefix("- run:")
                .or_else(|| content.strip_prefix("run:"))
                .unwrap_or_default();
            if executable_run_fragment_contains(command, needle) {
                return true;
            }
            continue;
        }
        if let Some(run) = run_indent {
            if indent <= run {
                run_indent = None;
            } else if executable_run_fragment_contains(content, needle) {
                return true;
            }
        }
    }
    false
}

fn action_uses_unpinned(workflow: &str) -> Vec<(usize, String)> {
    let mut bad = Vec::new();
    for (idx, line) in workflow.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = uses_spec(trimmed) else {
            continue;
        };
        if rest.starts_with("./") || rest.starts_with("docker://") {
            continue;
        }
        if !is_pinned_sha(rest) {
            bad.push((idx + 1, rest.to_owned()));
        }
    }
    bad
}

fn uses_spec(trimmed: &str) -> Option<&str> {
    let rest = trimmed
        .strip_prefix("- uses:")
        .or_else(|| trimmed.strip_prefix("uses:"))?;
    Some(rest.trim())
}

fn uses_action_ref(spec: &str) -> &str {
    let without_comment = spec
        .split_once('#')
        .map(|(before, _)| before)
        .unwrap_or(spec)
        .trim();
    without_comment
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
}

fn is_pinned_sha(spec: &str) -> bool {
    let action_ref = uses_action_ref(spec);
    let Some((_, sha)) = action_ref.rsplit_once('@') else {
        return false;
    };
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The verification gate AGENTS.md mandates is `--profile ci`, so that profile
/// must not retry: a retry reports a flaky test as green and hides the first
/// failure from the summary a human or agent reads (KEL-112).
///
/// Deliberately narrow, and its limits are the point.
///
/// It catches a plainly-spelled `retries` in the section family that binds the
/// gate — see [`classify_gate_section`]: either profile's own table, its
/// `[[...overrides]]`, or its `.retries` sub-table, with a trailing comment on
/// the header tolerated. It does NOT catch:
///
/// - `cargo nextest run --profile ci --retries 2`
/// - `NEXTEST_RETRIES=2` in the job's environment
/// - `--profile local` pointed at a profile that does retry
/// - a key spelled so as to evade a line reader: `"retries"`, an escape-encoded
///   `"\U00000072etries"`, or a header hidden inside a multi-line string
///
/// The first three were reproduced against this exact config with the guard
/// green, and no reading of this file can catch them: nextest resolves retries
/// from config UNION flag UNION env, flag and env winning, so the deciding
/// inputs are not in the file at all.
///
/// The enumeration above is what has been *measured*, not a proof of
/// completeness. nextest owns profile resolution, and a future version may add
/// another binding form; this guard would not know. That asymmetry is the
/// reason KEL-115 replaces it rather than growing it.
///
/// The fourth is decidable but deliberately not chased. Three rounds of
/// hardening a parser for it each produced a new spelling from outside the
/// previous frame, and an evasive spelling is not the regression this guard
/// exists to prevent — an ordinary edit re-adding `retries` is. KEL-115
/// replaces the whole approach with a behavioural check: run the real gate
/// command against a deliberately failing test and assert exactly one attempt.
///
/// Extend this only to a section that genuinely binds the gate and is verified
/// to retry. Do not extend it to chase an evasive spelling: that makes it look
/// more authoritative without making it more correct.
fn check_ci_profile_does_not_retry(root: &Path) -> Result<(), String> {
    let text = read(root, NEXTEST_CONFIG)?;
    if let Some((section, parent)) = gate_section_with_unfollowable_parent(&text) {
        return Err(format!(
            "CI-HYGIENE: `{NEXTEST_CONFIG}` has `{section}` inheriting from `{parent}`, and \
             this check cannot follow that chain to prove the mandated gate does not retry — \
             nextest inheritance is transitive, so `retries` anywhere up the chain binds \
             `--profile ci`. Inherit from `default` (the implicit parent) instead, or move \
             the settings into `{section}` directly."
        ));
    }
    if let Some(section) = gate_section_setting_retries(&text) {
        return Err(format!(
            "CI-HYGIENE: `{NEXTEST_CONFIG}` sets `retries` under `{section}`, which binds \
             `--profile ci` — the profile AGENTS.md mandates as the verification gate. A flaky \
             test would report green and its first failure would never be shown. Remove the \
             `retries` key and fix the non-deterministic test instead (await the condition, \
             bind port 0, use a temp dir)."
        ));
    }
    Ok(())
}

/// How a section header relates to the profile `--profile ci` resolves.
#[derive(PartialEq)]
enum GateSection {
    /// The section itself *is* the retries table: `[profile.ci.retries]`.
    IsRetries,
    /// A section whose `retries` key binds the gate.
    MayDeclareRetries,
    /// Not this contract.
    Unrelated,
}

/// Classifies a TOML section header against the profile the mandated gate uses.
///
/// Structural rather than a list of literal spellings, because the shapes that
/// bind the gate are a *family*, not a fixed set: `retries` is a key under the
/// profile, and also a legitimate sub-table (`[profile.ci.retries]`, nextest's
/// documented retries-with-backoff form), and both `ci` and `default` carry it
/// because `--profile ci` inherits `default`.
///
/// Verified against cargo-nextest 0.9.140 by counting `TRY` lines from a
/// deliberately failing test. Binding: `[profile.ci]`, `[profile.default]`,
/// either profile's `[[...overrides]]`, and either profile's `.retries`
/// sub-table. Not binding, and correctly ignored: `[profile.local]`,
/// `[profile.ci-extra]`, `[profile.default-miri]` — measured, all run a single
/// attempt under `--profile ci`.
fn classify_gate_section(header: &str) -> GateSection {
    // A trailing comment is ordinary TOML: `[profile.ci] # keep empty` is still
    // the `ci` section, and comparing the whole line would miss it.
    let header = header.split('#').next().unwrap_or(header).trim();
    let inner = match header
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
    {
        Some(inner) => inner,
        None => match header.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            Some(inner) => inner,
            None => return GateSection::Unrelated,
        },
    };
    let mut parts = inner.split('.').map(str::trim);
    if parts.next() != Some("profile") {
        return GateSection::Unrelated;
    }
    // `--profile ci` resolves against `ci` and the `default` it inherits.
    if !matches!(parts.next(), Some("ci" | "default")) {
        return GateSection::Unrelated;
    }
    match (parts.next(), parts.next()) {
        (None, _) | (Some("overrides"), None) => GateSection::MayDeclareRetries,
        (Some("retries"), None) => GateSection::IsRetries,
        _ => GateSection::Unrelated,
    }
}

/// A gate-binding section whose `inherits` points somewhere this check cannot
/// follow, as `(section, parent)`.
///
/// nextest lets any profile name a parent (`inherits = "shared"`), and the
/// chain is transitive: measured, `[profile.ci] inherits = "a"`, `[profile.a]
/// inherits = "b"`, `[profile.b] retries = 2` runs a failing test 3 times under
/// `--profile ci`. Following an arbitrary chain is more parsing than this guard
/// is willing to own, so an unfollowable parent FAILS CLOSED: silence about a
/// chain it cannot read must not be reported as "no retries".
///
/// `inherits = "default"` is allowed because it is the implicit parent anyway,
/// and `[profile.default]` is already classified.
fn gate_section_with_unfollowable_parent(text: &str) -> Option<(String, String)> {
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let header = line.split('#').next().unwrap_or(line).trim().to_owned();
            current = match classify_gate_section(line) {
                GateSection::Unrelated => None,
                _ => Some(header),
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "inherits" {
            continue;
        }
        let parent = value
            .split('#')
            .next()
            .unwrap_or(value)
            .trim()
            .trim_matches(['"', '\''])
            .to_owned();
        if parent != "default" {
            if let Some(section) = current.clone() {
                return Some((section, parent));
            }
        }
    }
    None
}

/// The gate-binding section that declares `retries`, if any.
///
/// Returns the header so the error can name the section it actually found: a
/// message hard-coding `[profile.ci]` sends a reader to the wrong line when the
/// key is under `[profile.default]`.
fn gate_section_setting_retries(text: &str) -> Option<String> {
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let header = line.split('#').next().unwrap_or(line).trim().to_owned();
            match classify_gate_section(line) {
                // The section header *is* the declaration; there is no key to find.
                GateSection::IsRetries => return Some(header),
                GateSection::MayDeclareRetries => current = Some(header),
                GateSection::Unrelated => current = None,
            }
            continue;
        }
        if line
            .split('=')
            .next()
            .is_some_and(|key| key.trim() == "retries")
        {
            if let Some(header) = current.clone() {
                return Some(header);
            }
        }
    }
    None
}

fn check_gitignore(root: &Path) -> Result<(), String> {
    let text = read(root, GITIGNORE)?;
    if github_dir_is_ignored(&text) {
        return Err(
            "CI-HYGIENE: `.gitignore` ignores `/.github/`, so GitHub never sees workflows, \
             CODEOWNERS, or templates. Remove that ignore rule (KEL-39 publishes CI)."
                .to_owned(),
        );
    }
    Ok(())
}

fn check_codeowners(root: &Path) -> Result<(), String> {
    let text = read(root, CODEOWNERS)?;
    for needle in REQUIRED_OWNER_PATHS {
        if !codeowners_covers(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{CODEOWNERS}` has no owned path containing `{needle}`. \
                 Add that path with at least one `@user` or `@org/team` owner."
            ));
        }
    }
    Ok(())
}

fn check_pr_template(root: &Path) -> Result<(), String> {
    let text = read(root, PR_TEMPLATE)?;
    for needle in PR_NEEDLES {
        if !text.contains(needle) {
            return Err(format!(
                "CI-HYGIENE: `{PR_TEMPLATE}` is missing `{needle}`. \
                 Restore the six required PR headings from AGENTS.md § Commits & PRs \
                 and the verification-gate commands from AGENTS.md § Commands & verification."
            ));
        }
    }
    Ok(())
}

fn check_issue_templates(root: &Path) -> Result<(), String> {
    let dir = root.join(ISSUE_DIR);
    let entries = fs::read_dir(&dir).map_err(|error| {
        format!(
            "CI-HYGIENE: missing `{ISSUE_DIR}`: {error}. \
             Add at least one GitHub issue template that mentions the verification gate."
        )
    })?;
    let mut found = false;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("CI-HYGIENE: cannot read `{ISSUE_DIR}`: {error}. Check directory permissions.")
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "config.yml" || name == "config.yaml" {
            continue;
        }
        if name.ends_with(".md") || name.ends_with(".yml") || name.ends_with(".yaml") {
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!(
            "CI-HYGIENE: `{ISSUE_DIR}` has no bug/feature template. \
             Add a `.yml` or `.md` template (not only `config.yml`)."
        ));
    }
    Ok(())
}

fn check_workflow(root: &Path) -> Result<(), String> {
    let text = read(root, WORKFLOW)?;
    let _required_evaluator = read(root, CI_REQUIRED_EVALUATOR)?;
    for needle in WORKFLOW_TEXT_NEEDLES {
        if !uncommented_line_contains(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` is missing `{needle}`. \
                 Restore the gitleaks job (checksummed CLI, not the org-licensed Action), \
                 `with: toolchain:` on dtolnay/rust-toolchain, \
                the hygiene job that compiles this file, \
                 generated-doc freshness, and the structural plus digest-pinned Mermaid render gates."
            ));
        }
    }
    if !workflow_has_checkout_persist_credentials_false(&text) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` must set `persist-credentials: false` inside an `actions/checkout` `with:` mapping. An echoed or unrelated YAML value does not protect checkout credentials."
        ));
    }
    check_change_router_job(&text)?;
    check_package_loop_shell(&text)?;
    check_check_job_if_avoids_matrix(&text)?;
    check_msrv_avoids_apt(&text)?;
    check_bun_test_job(&text)?;
    check_required_job(&text)?;
    for needle in WORKFLOW_RUN_NEEDLES {
        if !workflow_has_executable_run_needle(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` is missing executable Mermaid gate `{needle}`. Restore it in a `run:` step; comments and echo text do not execute the gate."
            ));
        }
    }
    let unpinned = action_uses_unpinned(&text);
    if !unpinned.is_empty() {
        let details: Vec<String> = unpinned
            .iter()
            .map(|(line, spec)| format!("line {line}: {spec}"))
            .collect();
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has unpinned `uses:` entries (need a 40-char commit SHA). \
             Pin each action and leave the tag in a trailing comment. Offenders: {}",
            details.join("; ")
        ));
    }
    Ok(())
}

fn check_mermaid_gate_files(root: &Path) -> Result<(), String> {
    let _checker = read(root, MERMAID_CHECKER)?;
    let renderer = read(root, MERMAID_RENDERER)?;
    for needle in [
        MERMAID_IMAGE_DIGEST,
        "--network none",
        "--read-only",
        "--cap-drop ALL",
        "--security-opt no-new-privileges",
        "--memory 2g",
        "--pids-limit 256",
        "run_with_timeout 120 docker run",
        "run_with_timeout 300 docker pull",
        "--pull never",
        "--jobs 2",
        ":/input/source.md:ro",
        "trap cleanup EXIT",
        "/tmp/keld-mermaid-render.",
    ] {
        if !uncommented_line_contains(&renderer, needle) {
            return Err(format!(
                "CI-HYGIENE: `{MERMAID_RENDERER}` is missing `{needle}`. Restore the pinned, network-disabled, read-only, resource-bounded renderer contract."
            ));
        }
    }
    let config = read(root, MERMAID_CONFIG)?;
    for needle in [
        "\"securityLevel\": \"strict\"",
        "\"maxTextSize\"",
        "\"maxEdges\"",
        "\"deterministicIds\": true",
    ] {
        if !config.contains(needle) {
            return Err(format!(
                "CI-HYGIENE: `{MERMAID_CONFIG}` is missing `{needle}`. Restore the strict deterministic resource-limit configuration."
            ));
        }
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    check_gitignore(root)?;
    check_ci_profile_does_not_retry(root)?;
    check_codeowners(root)?;
    check_pr_template(root)?;
    check_issue_templates(root)?;
    check_workflow(root)?;
    check_keldbot_workflow(root)?;
    check_mermaid_gate_files(root)?;
    Ok(())
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        "CI-HYGIENE: missing command. Run `ci-hygiene check [workspace]`.".to_owned()
    })?;
    let root = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if args.next().is_some() {
        return Err(
            "CI-HYGIENE: too many arguments. Run `ci-hygiene check [workspace]`.".to_owned(),
        );
    }
    match command.as_str() {
        "check" => check(&root),
        _ => Err(format!(
            "CI-HYGIENE: unknown command `{command}`. Use `check` to verify KEL-39 files."
        )),
    }
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
            let path = env::temp_dir().join(format!("keld-ci-hygiene-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create isolated fixture root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
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

    const PINNED_CHECKOUT: &str =
        "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0\n";

    fn valid_workflow() -> String {
        [
            "name: CI",
            "jobs:",
            "  changes:",
            "    name: change router",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          fetch-depth: 0",
            "      - name: Router contract tests",
            "        run: tools/ci_changes_test.sh",
            "      - name: Classify changed-path ownership",
            "        run: tools/ci_changes.sh github",
            "  check:",
            "    steps:",
            "      - name: clippy (warnings deny)",
            "        shell: bash",
            "        run: cargo clippy -p fixture --all-targets -- -D warnings",
            "      - name: test",
            "        shell: bash",
            "        run: cargo nextest run -p fixture --profile ci --no-tests=pass",
            "  bun-test:",
            "    if: needs.changes.outputs.ts == 'true'",
            "    steps:",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0",
            "        with:",
            "          bun-version: \"1.4.0\"",
            "      - name: bun test",
            "        shell: bash",
            "        env:",
            "          KELD_CI_TS_PACKAGES: ${{ needs.changes.outputs.ts_packages }}",
            "        run: cd fixture && bun test",
            "  msrv:",
            "    runs-on: macos-latest",
            "    steps:",
            "      - run: cargo +1.97 check -p fixture --all-targets",
            "  secrets:",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - uses: dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772 # master 2026-08-05",
            "        with:",
            "          toolchain: 1.97.1",
            "      - run: echo 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb | sha256sum -c -",
            "      - run: gitleaks detect --source . --exit-code 1",
            "  hygiene:",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "      - run: rustc --edition=2024 --test tools/ci_hygiene.rs",
            "      - run: rustc --edition=2024 tools/llms_docs.rs",
            "      - run: llms-docs check .",
            "      - run: rustc --edition=2024 --test tools/mermaid_docs.rs",
            "      - run: mermaid-docs check .",
            "      - run: tools/mermaid_render_check.sh # sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf",
            "  required:",
            "    name: CI required",
            "    if: ${{ always() }}",
            "    needs:",
            "      - changes",
            "      - fmt",
            "      - check",
            "      - bun-test",
            "      - linux-gui-smoke",
            "      - msrv",
            "      - deny",
            "      - secrets",
            "      - hygiene",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - name: Verify required CI results",
            "        env:",
            "          KELD_RESULT_BUN: ${{ needs['bun-test'].result }}",
            "          KELD_RESULT_GUI: ${{ needs['linux-gui-smoke'].result }}",
            "        run: |",
            "          tools/ci_required.sh test",
            "          tools/ci_required.sh check success skipped skipped skipped skipped skipped skipped success skipped",
            "",
        ]
        .join("\n")
    }

    fn valid_keldbot_workflow() -> String {
        [
            "name: KeldBot",
            "on:",
            "  pull_request_target:",
            "    types: [opened, edited, synchronize]",
            "permissions: {}",
            "jobs:",
            "  gatekeeper:",
            "    runs-on: ubuntu-latest",
            "    steps:",
            "      - uses: actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3 # v9.0.0",
            "        with:",
            "          script: |",
            "            const REQUIRED = [\"Summary\", \"Spec refs\", \"Review gates\", \"Tests\", \"Platforms\", \"Perf impact\"];",
            "  title-lint:",
            "    runs-on: ubuntu-latest",
            "    steps:",
            "      - uses: actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3 # v9.0.0",
            "        with:",
            "          script: |",
            "            core.info('title');",
            "",
        ]
        .join("\n")
    }

    fn valid_codeowners() -> &'static str {
        "/Cargo.toml @alice\n\
         /crates/keld-guard/ @alice\n\
         /crates/keld-ipc/ @alice\n\
         /.github/ @alice\n\
         /AGENTS.md @alice\n\
         /.agents/ @alice\n\
         /docs/agents/ @alice\n\
         /docs/engineering/keld-error-codes.md @alice\n\
         /justfile @alice\n\
         /tools/ @alice\n"
    }

    fn valid_pr() -> &'static str {
        "## Summary\n## Spec refs\nNo boundary change\n## Review gates\n\
         ## Tests\nRun cargo fmt, clippy, nextest, and mermaid-render-check.\n\
         ## Platforms\n## Perf impact\nagent/kel-\n"
    }

    fn complete_fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(".gitignore", "/target\n/.claude\n");
        temp.write(CODEOWNERS, valid_codeowners());
        temp.write(PR_TEMPLATE, valid_pr());
        temp.write(
            ".github/ISSUE_TEMPLATE/bug.yml",
            "name: Bug\nbody:\n  - type: markdown\n",
        );
        temp.write(WORKFLOW, &valid_workflow());
        temp.write(KELDBOT_WORKFLOW, &valid_keldbot_workflow());
        temp.write(CI_REQUIRED_EVALUATOR, "#!/usr/bin/env bash\nexit 0\n");
        temp.write(MERMAID_CHECKER, "fn main() {}\n");
        temp.write(NEXTEST_CONFIG, "[profile.ci]\n");
        temp.write(
            MERMAID_RENDERER,
            "sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf\n--network none\n--read-only\n--cap-drop ALL\n--security-opt no-new-privileges\n--memory 2g\n--pids-limit 256\nrun_with_timeout 120 docker run\nrun_with_timeout 300 docker pull\n--pull never\n--jobs 2\n:/input/source.md:ro\ntrap cleanup EXIT\n/tmp/keld-mermaid-render.\n",
        );
        temp.write(
            MERMAID_CONFIG,
            "{\"securityLevel\": \"strict\", \"maxTextSize\": 50000, \"maxEdges\": 500, \"deterministicIds\": true}\n",
        );
        temp
    }

    #[test]
    fn complete_fixture_passes() {
        let temp = complete_fixture();
        check(temp.path()).expect("complete KEL-39 fixture must pass");
    }

    #[test]
    fn required_result_job_is_mandatory_and_always_created() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("  required:\n", "  removed-required:\n", 1),
        );
        let error = check(temp.path()).expect_err("missing required result must fail");
        assert!(error.contains("required"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("    if: ${{ always() }}", "    if: ${{ success() }}", 1),
        );
        let error = check(temp.path()).expect_err("non-always required result must fail");
        assert!(error.contains("always"), "{error}");
    }

    #[test]
    fn required_result_must_observe_unconditional_gitleaks() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("      - secrets\n", "", 1),
        );
        let error = check(temp.path()).expect_err("missing gitleaks dependency must fail");
        assert!(error.contains("secrets"), "{error}");
    }

    #[test]
    fn keldbot_required_jobs_revalidate_synchronized_heads() {
        let temp = complete_fixture();
        temp.write(
            KELDBOT_WORKFLOW,
            &valid_keldbot_workflow().replacen(
                "  gatekeeper:\n",
                "  gatekeeper:\n    if: github.event.action != 'synchronize'\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("skipped gatekeeper must fail");
        assert!(error.contains("synchronize"), "{error}");

        temp.write(
            KELDBOT_WORKFLOW,
            &valid_keldbot_workflow().replace(", synchronize", ""),
        );
        let error = check(temp.path()).expect_err("missing synchronize trigger must fail");
        assert!(error.contains("synchronize"), "{error}");
    }

    #[test]
    fn keldbot_pull_request_target_never_checks_out_or_runs_pr_code() {
        let temp = complete_fixture();
        temp.write(
            KELDBOT_WORKFLOW,
            &valid_keldbot_workflow().replacen(
                "    steps:\n",
                "    steps:\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("secret-bearing checkout must fail");
        assert!(error.contains("checkout"), "{error}");
    }

    #[test]
    fn ci_profile_that_retries_is_rejected() {
        let temp = complete_fixture();
        temp.write(NEXTEST_CONFIG, "[profile.ci]\nretries = 1\n");
        let error = check(temp.path())
            .expect_err("the mandated verification profile must not retry (KEL-112)");
        assert!(error.contains("retries"), "{error}");
        assert!(error.contains(NEXTEST_CONFIG), "{error}");
    }

    #[test]
    fn the_inherited_default_profile_is_the_gate_too() {
        // `--profile ci` inherits `default`, and nextest's own documentation
        // puts `retries` there — so this is the likeliest accidental
        // regression, not an exotic one. Measured on cargo-nextest 0.9.140:
        // `[profile.default] retries = 2` with an empty `[profile.ci]` runs a
        // failing test 3 times under `--profile ci`.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.default]\nretries = 2\n\n[profile.ci]\n",
        );
        let error = check(temp.path())
            .expect_err("`ci` inherits `default`, so retries there bind the mandated gate");
        assert!(error.contains("retries"), "{error}");
    }

    #[test]
    fn an_override_that_retries_binds_the_gate() {
        // An override's `retries` applies to every test its filter matches.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n\n[[profile.ci.overrides]]\nfilter = \"all()\"\nretries = 2\n",
        );
        let error = check(temp.path())
            .expect_err("an override under the mandated profile still retries it");
        assert!(error.contains("retries"), "{error}");
    }

    #[test]
    fn the_retries_sub_table_binds_the_gate() {
        // nextest's documented retries-with-backoff form is a sub-table, not a
        // key. Measured: `[profile.ci.retries]` with `count = 2` runs a failing
        // test 3 times under `--profile ci`. A guard matching literal section
        // names missed it entirely, because the section *is* the declaration.
        for header in ["[profile.ci.retries]", "[profile.default.retries]"] {
            let temp = complete_fixture();
            temp.write(
                NEXTEST_CONFIG,
                &format!("[profile.ci]\n\n{header}\ncount = 2\nbackoff = \"fixed\"\n"),
            );
            let error =
                check(temp.path()).expect_err("a retries sub-table retries the mandated gate");
            assert!(error.contains(header), "{error}");
        }
    }

    #[test]
    fn a_trailing_comment_does_not_hide_the_section() {
        // `[profile.ci] # keep this empty` is ordinary TOML and still the `ci`
        // section. Comparing the whole trimmed line against a literal header
        // silently skipped it while nextest retried.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci] # keep this empty\nretries = 2\n",
        );
        let error =
            check(temp.path()).expect_err("a commented header is still the mandated profile");
        assert!(error.contains("[profile.ci]"), "{error}");
    }

    #[test]
    fn an_inherited_override_that_retries_binds_the_gate() {
        // The fourth binding section. Its absence from the tests was why the
        // doc comment could claim all four were verified while one was not.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n\n[[profile.default.overrides]]\nfilter = \"all()\"\nretries = 2\n",
        );
        let error = check(temp.path())
            .expect_err("an override on the inherited profile still retries the gate");
        assert!(error.contains("[[profile.default.overrides]]"), "{error}");
    }

    #[test]
    fn the_error_names_the_section_it_found() {
        // A message hard-coding `[profile.ci]` sends the reader to the wrong
        // line when the key is under the profile `ci` inherits.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.default]\nretries = 2\n\n[profile.ci]\n",
        );
        let error = check(temp.path()).expect_err("inherited retries bind the gate");
        assert!(error.contains("[profile.default]"), "{error}");
        assert!(
            !error.contains("under `[profile.ci]`"),
            "must not name a section it did not find: {error}"
        );
    }

    #[test]
    fn an_unfollowable_parent_fails_closed() {
        // nextest inheritance is transitive. Measured: `[profile.ci] inherits =
        // "a"`, `[profile.a] inherits = "b"`, `[profile.b] retries = 2` runs a
        // failing test 3 times under `--profile ci`. This guard does not follow
        // chains, so it must refuse rather than report silence as no-retries.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.shared]\nretries = 2\n\n[profile.ci]\ninherits = \"shared\"\n",
        );
        let error =
            check(temp.path()).expect_err("a parent this check cannot follow must fail closed");
        assert!(error.contains("shared"), "{error}");
        assert!(error.contains("cannot follow"), "{error}");
    }

    #[test]
    fn inheriting_the_implicit_default_parent_is_allowed() {
        // `default` is the implicit parent and is already classified, so naming
        // it explicitly must not be a false positive.
        let temp = complete_fixture();
        temp.write(NEXTEST_CONFIG, "[profile.ci]\ninherits = \"default\"\n");
        check(temp.path()).expect("inheriting the implicit parent is a no-op");
    }

    #[test]
    fn a_profile_that_does_not_bind_the_gate_may_retry() {
        // The no-false-positive side: measured, `[profile.local]` runs a single
        // attempt under `--profile ci`, so rejecting it would block a
        // legitimate local convenience for no gain.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n\n[profile.local]\nretries = 3\n",
        );
        check(temp.path()).expect("a profile the gate does not inherit is not this contract");
    }

    #[test]
    fn ci_profile_without_retries_passes() {
        let temp = complete_fixture();
        temp.write(NEXTEST_CONFIG, "[profile.ci]\nfail-fast = false\n");
        check(temp.path()).expect("a ci profile with no retries is the contract");
    }

    #[test]
    fn retries_under_a_different_profile_is_not_this_contract() {
        // Guards over-matching: only the profile AGENTS.md mandates is bound.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.local]\nretries = 3\n\n[profile.ci]\n",
        );
        check(temp.path()).expect("only `[profile.ci]` is the verification gate");
    }

    #[test]
    fn commented_out_retries_is_not_a_retry() {
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n# retries = 1 (removed, KEL-112)\n",
        );
        check(temp.path()).expect("a comment is documentation, not configuration");
    }

    #[test]
    fn router_test_in_another_job_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replacen("        run: tools/ci_changes_test.sh\n", "", 1)
            .replace(
                "  secrets:\n",
                "  unrelated:\n    steps:\n      - run: tools/ci_changes_test.sh\n  secrets:\n",
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("router test must run in changes job");
        assert!(error.contains("changes` job"), "{error}");
        assert!(error.contains("tools/ci_changes_test.sh"), "{error}");
    }

    #[test]
    fn router_command_in_another_job_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replacen("        run: tools/ci_changes.sh github\n", "", 1)
            .replace(
                "  secrets:\n",
                "  unrelated:\n    steps:\n      - run: tools/ci_changes.sh github\n  secrets:\n",
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("router command must run in changes job");
        assert!(error.contains("changes` job"), "{error}");
        assert!(error.contains("tools/ci_changes.sh github"), "{error}");
    }

    #[test]
    fn full_history_checkout_in_another_job_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replacen("          fetch-depth: 0\n", "", 1)
            .replacen(
                "          persist-credentials: false\n",
                "          persist-credentials: false\n          fetch-depth: 0\n",
                1,
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("changes job must fetch its own history");
        assert!(error.contains("changes` job"), "{error}");
        assert!(error.contains("fetch-depth: 0"), "{error}");
    }

    #[test]
    fn quoted_fetch_depth_in_changes_job_is_accepted() {
        for value in ["'0'", "\"0\""] {
            let temp = complete_fixture();
            temp.write(
                WORKFLOW,
                &valid_workflow().replacen("fetch-depth: 0", &format!("fetch-depth: {value}"), 1),
            );
            check(temp.path()).expect("quoted fetch depth is the same YAML scalar");
        }
    }

    #[test]
    fn shell_guarded_router_test_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "run: tools/ci_changes_test.sh",
                "run: false && tools/ci_changes_test.sh",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("disabled shell command must fail");
        assert!(error.contains("unconditional"), "{error}");
        assert!(error.contains("tools/ci_changes_test.sh"), "{error}");
    }

    #[test]
    fn github_if_guarded_router_command_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Classify changed-path ownership\n        run:",
                "      - name: Classify changed-path ownership\n        if: ${{ false }}\n        run:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("disabled GitHub step must fail");
        assert!(error.contains("unconditional"), "{error}");
        assert!(error.contains("tools/ci_changes.sh github"), "{error}");
    }

    #[test]
    fn package_loop_requires_bash_on_windows_matrix() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("        shell: bash\n", "", 1),
        );
        let error = check(temp.path()).expect_err("package loop needs Bash on Windows");
        assert!(error.contains("clippy (warnings deny)"), "{error}");
        assert!(error.contains("shell: bash"), "{error}");
    }

    #[test]
    fn selected_zero_test_package_must_not_fail_nextest() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(" --no-tests=pass", "", 1),
        );
        let error = check(temp.path()).expect_err("zero-test package policy must remain explicit");
        assert!(error.contains("--no-tests=pass"), "{error}");
    }

    #[test]
    fn check_job_if_must_not_use_matrix() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "  check:\n    steps:",
                "  check:\n    if: needs.changes.outputs.rust == 'true' && matrix.os != 'ubuntu-latest'\n    steps:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("job-level matrix.os must fail hygiene");
        assert!(error.contains("matrix"), "{error}");
        assert!(error.contains("check"), "{error}");
    }

    #[test]
    fn msrv_must_not_apt_get_on_ubuntu() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "    runs-on: macos-latest\n",
                "    runs-on: ubuntu-latest\n    steps:\n      - run: sudo apt-get update\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("MSRV apt-get must fail hygiene");
        assert!(error.contains("msrv"), "{error}");
        assert!(
            error.contains("macos-latest") || error.contains("apt-get"),
            "{error}"
        );
    }

    #[test]
    fn deleting_the_bun_lane_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow();
        let start = workflow
            .find("  bun-test:")
            .expect("fixture has the Bun lane");
        let end = workflow.find("  msrv:").expect("fixture has the MSRV lane");
        let mut without_lane = workflow.clone();
        without_lane.replace_range(start..end, "");
        temp.write(WORKFLOW, &without_lane);
        let error = check(temp.path()).expect_err("removing the only bun test lane must fail");
        assert!(error.contains("bun-test"), "{error}");
    }

    #[test]
    fn floating_bun_version_in_bun_lane_fails() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("bun-version: \"1.4.0\"", "bun-version: latest", 1),
        );
        let error = check(temp.path()).expect_err("an unpinned Bun runtime must fail");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("1.4.0"), "{error}");
    }

    #[test]
    fn bun_lane_gated_on_another_router_output_fails() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "    if: needs.changes.outputs.ts == 'true'\n",
                "    if: needs.changes.outputs.rust == 'true'\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("the Bun lane must follow its own router output");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("outputs.ts"), "{error}");
    }

    #[test]
    fn bun_lane_must_not_apt_get() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: bun test\n",
                "      - run: sudo apt-get update\n      - name: bun test\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("a second live apt lane must fail");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("apt-get"), "{error}");
    }

    #[test]
    fn ignoring_github_dir_fails() {
        let temp = complete_fixture();
        temp.write(".gitignore", "/target\n/.github/\n");
        let error = check(temp.path()).expect_err("ignored .github must fail");
        assert!(error.contains("CI-HYGIENE"), "{error}");
        assert!(error.contains("/.github/"), "{error}");
        assert!(error.contains("Remove that ignore"), "{error}");
    }

    #[test]
    fn ignoring_github_star_pattern_fails() {
        let temp = complete_fixture();
        temp.write(".gitignore", "/target\n/.github/*\n");
        let error = check(temp.path()).expect_err("/.github/* must count as ignoring .github");
        assert!(error.contains("CI-HYGIENE"), "{error}");
        assert!(error.contains("Remove that ignore"), "{error}");
    }

    #[test]
    fn github_dir_ignore_patterns() {
        for pattern in [
            "/.github/",
            "/.github",
            ".github/",
            ".github",
            "/.github/**",
            "/.github/*",
            ".github/**",
            ".github/*",
        ] {
            assert!(
                github_dir_is_ignored(&format!("/target\n{pattern}\n")),
                "{pattern} must be treated as ignoring .github"
            );
        }
        assert!(!github_dir_is_ignored("/target\n/.claude\n"));
        assert!(!github_dir_is_ignored("# /.github/*\n/target\n"));
        assert!(!github_dir_is_ignored("/.github/workflows/ci.yml\n"));
    }

    #[test]
    fn comment_mentioning_github_ignore_is_not_an_ignore_rule() {
        let temp = complete_fixture();
        temp.write(
            ".gitignore",
            "# formerly /.github/ — CI is tracked (KEL-39)\n/target\n",
        );
        check(temp.path()).expect("comment must not count as an ignore rule");
    }

    #[test]
    fn echoed_mermaid_gate_does_not_satisfy_workflow_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "- run: tools/mermaid_render_check.sh",
            "- run: echo tools/mermaid_render_check.sh",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("echoed gate must not satisfy hygiene");
        assert!(error.contains("executable Mermaid gate"), "{error}");
        assert!(error.contains("mermaid_render_check.sh"), "{error}");
    }

    #[test]
    fn inline_comment_mermaid_gate_does_not_satisfy_workflow_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "- run: tools/mermaid_render_check.sh",
            "- run: true # tools/mermaid_render_check.sh",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("commented gate must not satisfy hygiene");
        assert!(error.contains("executable Mermaid gate"), "{error}");
    }

    #[test]
    fn checkout_credentials_must_be_disabled_in_its_with_mapping() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "      persist-credentials: false\n",
            "      # persist-credentials: false\n",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("comment must not satisfy checkout hardening");
        assert!(error.contains("persist-credentials: false"), "{error}");
    }

    #[test]
    fn missing_guard_codeowners_path_fails() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            "/Cargo.toml @alice\n/crates/keld-ipc/ @alice\n/.github/ @alice\n",
        );
        let error = check(temp.path()).expect_err("missing guard path must fail");
        assert!(error.contains("keld-guard"), "{error}");
        assert!(error.contains("@user"), "{error}");
    }

    #[test]
    fn commented_codeowners_path_does_not_count() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            "# /crates/keld-guard/ @alice\n\
             /Cargo.toml @alice\n\
             /crates/keld-ipc/ @alice\n\
             /.github/ @alice\n",
        );
        let error = check(temp.path()).expect_err("commented owner line must fail");
        assert!(error.contains("keld-guard"), "{error}");
    }

    #[test]
    fn codeowners_path_without_owner_fails() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            "/Cargo.toml @alice\n\
             /crates/keld-guard/\n\
             /crates/keld-ipc/ @alice\n\
             /.github/ @alice\n",
        );
        let error = check(temp.path()).expect_err("path with no @owner must fail");
        assert!(error.contains("keld-guard"), "{error}");
    }

    #[test]
    fn missing_pr_gate_fails() {
        let temp = complete_fixture();
        temp.write(PR_TEMPLATE, "## Summary\n\nNo gates here.\n");
        let error = check(temp.path()).expect_err("incomplete PR template must fail");
        assert!(
            error.contains("Spec refs")
                || error.contains("Review gates")
                || error.contains("cargo fmt"),
            "{error}"
        );
        assert!(
            error.contains("verification-gate") || error.contains("required PR headings"),
            "{error}"
        );
    }

    #[test]
    fn missing_toolchain_pin_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace("toolchain: 1.97.1", "");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("workflow without toolchain pin must fail");
        assert!(error.contains("toolchain: 1.97.1"), "{error}");
    }

    #[test]
    fn missing_llms_docs_check_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replace("tools/llms_docs.rs", "")
            .replace("llms-docs check .", "");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("workflow without llms-docs check must fail");
        assert!(
            error.contains("tools/llms_docs.rs") || error.contains("llms-docs check"),
            "{error}"
        );
    }

    #[test]
    fn missing_mermaid_render_workflow_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replace("tools/mermaid_render_check.sh", "")
            .replace(MERMAID_IMAGE_DIGEST, "");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("workflow without pinned render must fail");
        assert!(
            error.contains("mermaid_render_check.sh") || error.contains(MERMAID_IMAGE_DIGEST),
            "{error}"
        );
    }

    #[test]
    fn commented_mermaid_render_workflow_does_not_pass() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "- run: tools/mermaid_render_check.sh",
            "# - run: tools/mermaid_render_check.sh",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("commented render step must fail");
        assert!(error.contains("mermaid_render_check.sh"), "{error}");
    }

    #[test]
    fn weakened_mermaid_renderer_isolation_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("--network none", ""),
        );
        let error = check(temp.path()).expect_err("network-enabled renderer must fail");
        assert!(error.contains("--network none"), "{error}");
    }

    #[test]
    fn commented_mermaid_isolation_flag_does_not_pass() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("--network none", "# --network none"),
        );
        let error = check(temp.path()).expect_err("commented network isolation must fail");
        assert!(error.contains("--network none"), "{error}");
    }

    #[test]
    fn weakened_mermaid_resource_config_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_CONFIG,
            &read(temp.path(), MERMAID_CONFIG)
                .expect("config fixture")
                .replace("\"maxEdges\"", "\"removedMaxEdges\""),
        );
        let error = check(temp.path()).expect_err("config without edge limit must fail");
        assert!(error.contains("maxEdges"), "{error}");
    }

    #[test]
    fn missing_tools_codeowner_fails() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            &valid_codeowners().replace("/tools/ @alice\n", ""),
        );
        let error = check(temp.path()).expect_err("gate tools without owner must fail");
        assert!(error.contains("tools"), "{error}");
    }

    #[test]
    fn workflow_mentioning_hygiene_file_without_test_flag_fails() {
        let temp = complete_fixture();
        let workflow =
            valid_workflow().replace("--test tools/ci_hygiene.rs", "tools/ci_hygiene.rs");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("compile-only hygiene step must fail");
        assert!(error.contains("--test tools/ci_hygiene.rs"), "{error}");
    }

    #[test]
    fn comment_at_sign_does_not_false_report_unpinned() {
        assert!(is_pinned_sha(
            "actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # pin @v4.4.0"
        ));
        let workflow = format!("jobs:\n  x:\n    steps:\n{PINNED_CHECKOUT}");
        let with_comment_at = workflow.replace("# v4.4.0", "# pin @v4.4.0");
        assert!(
            action_uses_unpinned(&with_comment_at).is_empty(),
            "{:?}",
            action_uses_unpinned(&with_comment_at)
        );
    }

    #[test]
    fn quoted_uses_spec_sha_is_pinned() {
        assert!(is_pinned_sha(
            r#""actions/checkout@11d5960a326750d5838078e36cf38b85af677262""#
        ));
        let workflow = "jobs:\n  x:\n    steps:\n      - uses: \"actions/checkout@11d5960a326750d5838078e36cf38b85af677262\"\n";
        assert!(
            action_uses_unpinned(workflow).is_empty(),
            "{:?}",
            action_uses_unpinned(workflow)
        );
    }

    #[test]
    fn missing_gitleaks_job_fails() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            "name: CI\njobs:\n  hygiene:\n    steps:\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262\n        run: rustc tools/ci_hygiene.rs\n",
        );
        let error = check(temp.path()).expect_err("workflow without gitleaks must fail");
        assert!(error.contains("gitleaks detect"), "{error}");
    }

    #[test]
    fn unpinned_action_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "actions/checkout@v4",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("floating action tag must fail");
        assert!(error.contains("unpinned"), "{error}");
        assert!(
            error.contains("@v4") || error.contains("checkout@v4"),
            "{error}"
        );
    }

    #[test]
    fn empty_issue_template_dir_fails() {
        let temp = complete_fixture();
        temp.write(
            ".github/ISSUE_TEMPLATE/config.yml",
            "blank_issues_enabled: true\n",
        );
        let bug = temp.path().join(".github/ISSUE_TEMPLATE/bug.yml");
        fs::remove_file(bug).expect("remove bug template");
        let error = check(temp.path()).expect_err("config-only issue templates must fail");
        assert!(error.contains("ISSUE_TEMPLATE"), "{error}");
    }

    #[test]
    fn deleting_codeowners_file_fails() {
        let temp = complete_fixture();
        fs::remove_file(temp.path().join(CODEOWNERS)).expect("remove CODEOWNERS");
        let error = check(temp.path()).expect_err("missing CODEOWNERS must fail");
        assert!(error.contains("CODEOWNERS"), "{error}");
        assert!(error.contains("Restore"), "{error}");
    }
}
