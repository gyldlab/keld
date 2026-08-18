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
const GITIGNORE: &str = ".gitignore";
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
    "Review gates",
    "cargo fmt",
    "clippy",
    "nextest",
    "mermaid-render-check",
];

const WORKFLOW_NEEDLES: &[&str] = &[
    "gitleaks detect",
    "sha256sum -c",
    "--test tools/ci_hygiene.rs",
    "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb",
    "toolchain: 1.97.1",
    "tools/llms_docs.rs",
    "llms-docs check",
    "--test tools/mermaid_docs.rs",
    "mermaid-docs check .",
    "tools/mermaid_render_check.sh",
    "persist-credentials: false",
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
                 Restore the verification-gate checklist from AGENTS.md."
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
    for needle in WORKFLOW_NEEDLES {
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
    check_codeowners(root)?;
    check_pr_template(root)?;
    check_issue_templates(root)?;
    check_workflow(root)?;
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
        format!(
            "name: CI\n\
             jobs:\n\
               secrets:\n\
                 steps:\n\
             {PINNED_CHECKOUT}\
                   - uses: dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772 # master 2026-08-05\n\
                     with:\n\
                       toolchain: 1.97.1\n\
                   - run: echo 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb | sha256sum -c -\n\
                   - run: gitleaks detect --source . --exit-code 1\n\
                   - run: echo persist-credentials: false\n\
               hygiene:\n\
                 steps:\n\
             {PINNED_CHECKOUT}\
                   - run: rustc --edition=2024 --test tools/ci_hygiene.rs\n\
                   - run: rustc --edition=2024 tools/llms_docs.rs\n\
                   - run: llms-docs check .\n\
                   - run: rustc --edition=2024 --test tools/mermaid_docs.rs\n\
                   - run: mermaid-docs check .\n\
                   - run: tools/mermaid_render_check.sh # sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf\n"
        )
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
        "## Review gates\n\nRun cargo fmt, clippy, nextest, and mermaid-render-check.\n"
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
        temp.write(MERMAID_CHECKER, "fn main() {}\n");
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
        let error = check(temp.path()).expect_err("PR template without gates must fail");
        assert!(
            error.contains("Review gates") || error.contains("cargo fmt"),
            "{error}"
        );
        assert!(error.contains("verification-gate"), "{error}");
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
