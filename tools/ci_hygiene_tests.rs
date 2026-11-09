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

fn windows_media_step() -> String {
    let commands = WINDOWS_MEDIA_ACCEPTANCE_COMMANDS
        .iter()
        .map(|command| format!("          {command}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "      - name: Windows media guard acceptance (KEL-132)\n        shell: pwsh\n        run: |\n{commands}"
    )
}

fn release_helper_step() -> String {
    let mut lines = vec![
        format!("      - name: {RELEASE_HELPER_STEP}"),
        format!("        if: {RELEASE_HELPER_CONDITION}"),
        "        shell: bash".to_owned(),
        "        env:".to_owned(),
    ];
    lines.extend(
        RELEASE_HELPER_ENV
            .iter()
            .map(|(key, value)| format!("          {key}: {value}")),
    );
    lines.push("        run: |".to_owned());
    lines.extend(
        RELEASE_HELPER_COMMANDS
            .iter()
            .map(|command| format!("          {command}")),
    );
    lines.join("\n")
}

// Fixture for Rust-owned contracts; parsed security cases use the real workflow in Bun.
fn gitleaks_scan_step() -> String {
    let mut lines = vec!["      - name: Scan".to_owned(), "        env:".to_owned()];
    lines.extend(
        GITLEAKS_SCAN_ENV
            .iter()
            .map(|(key, value)| format!("          {key}: {value}")),
    );
    lines.push("        run: |".to_owned());
    lines.extend(
        GITLEAKS_SCAN_COMMANDS
            .iter()
            .map(|command| format!("          {command}")),
    );
    lines.join("\n")
}

/// The live `deny` job, verbatim from `.github/workflows/ci.yml`. Its steps'
/// contract is parsed by tools/ci_workflow_security.ts (#676), so the fixture
/// copies no deny step text that could drift from the workflow (#677).
fn live_deny_job() -> String {
    let lines: Vec<&str> = include_str!("../.github/workflows/ci.yml")
        .lines()
        .collect();
    let start = lines
        .iter()
        .position(|line| *line == "  deny:")
        .expect("ci.yml declares a top-level `deny` job");
    // The job ends at the next line indented less than its body (the next job key).
    let end = lines[start + 1..]
        .iter()
        .position(|line| !line.trim().is_empty() && !line.starts_with("    "))
        .map_or(lines.len(), |offset| start + 1 + offset);
    lines[start..end].join("\n").trim_end().to_owned()
}

fn valid_workflow() -> String {
    let windows_media_step = windows_media_step();
    let deny_job = live_deny_job();
    [
            "name: CI",
            "env:",
            "  CARGO_TERM_COLOR: always",
            "  RUST_BACKTRACE: 1",
            "  CARGO_INCREMENTAL: \"0\"",
            "jobs:",
            "  changes:",
            "    name: change router",
            "    outputs:",
            "      mermaid: ${{ steps.classify.outputs.mermaid }}",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          fetch-depth: 0",
            "          persist-credentials: false",
            "      - name: Classify changed-path ownership",
            "        run: tools/ci_changes.sh github",
            "      - name: Agent instruction inventory (agents-md)",
            "        run: tools/agents_md.sh",
            "      - name: Product status contract",
            "        run: |",
            "          mkdir -p target/product-status",
            "          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
            "          target/product-status/product-status-test",
            "          rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status",
            "          target/product-status/product-status check .",
            "  router-contracts:",
            "    name: Router contract tests",
            "    runs-on: ubuntu-latest",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          fetch-depth: 0",
            "          persist-credentials: false",
            "      - name: Router contract tests",
            "        run: tools/ci_changes_test.sh",
            "  check:",
            "    strategy:",
            "      matrix:",
            "        os: ${{ fromJSON(needs.changes.outputs.check_os) }}",
            "    steps:",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6",
            "        with:",
            "          bun-version: '1.4.4'",
            "      - name: Check keld-ipc fuzz workspace",
            "        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
            "        run: |",
            "          cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml",
            "      - name: Test keld-ipc fuzzing hook",
            "        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
            "        run: |",
            "          cargo test -p keld-ipc --features fuzzing --lib attempt::records",
            "      - name: Check keld-wv fuzz workspace",
            "        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
            "        run: |",
            "          cargo check --manifest-path crates/keld-wv/fuzz/Cargo.toml",
            "      - name: Check keld-update fuzz workspace",
            "        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
            "        run: |",
            "          cargo check --manifest-path crates/keld-update/fuzz/Cargo.toml",
            "      - name: clippy (warnings deny)",
            "        shell: bash",
            "        run: cargo clippy -p fixture --all-targets -- -D warnings",
            "      - name: test",
            "        shell: bash",
            "        run: cargo nextest run -p fixture --profile ci --no-tests=pass",
            "      - name: Product status Windows path contracts",
            "        if: matrix.os == 'windows-latest'",
            "        shell: bash",
            "        run: |",
            "          mkdir -p target/product-status",
            "          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
            "          target/product-status/product-status-test",
            &release_helper_step(),
            "  windows-media:",
            "    runs-on: windows-latest",
            "    needs: changes",
            "    if: needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv')",
            "    steps:",
            windows_media_step.as_str(),
            "  bun-test:",
            "    runs-on: ${{ matrix.os }}",
            "    if: needs.changes.outputs.ts == 'true'",
            "    strategy:",
            "      fail-fast: false",
            "      matrix:",
            "        os: [ubuntu-latest, macos-latest, windows-latest]",
            "    steps:",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0",
            "        with:",
            "          bun-version: \"1.4.4\"",
            "      - name: bun test",
            "        shell: bash",
            "        env:",
            "          KELD_CI_TS_PACKAGES: ${{ needs.changes.outputs.ts_packages }}",
            "        run: cd fixture && bun test",
            "  linux-gui-smoke:",
            "    steps:",
            "      - name: Build Linux media guard probe",
            "        run: |",
            "          cargo build -p keld-wv --example linux_media_guard",
            "          cc -shared -fPIC -Wall -Wextra -Werror -Wpedantic $(pkg-config --cflags webkit2gtk-4.1) crates/keld-wv/tests/fixtures/linux_media_interpose.c -o \"$RUNNER_TEMP/linux_media_interpose.so\" -ldl $(pkg-config --libs webkit2gtk-4.1)",
            "      - name: Xvfb GUI smoke test — title, controls, and clean close",
            "        run: xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host",
            "  msrv:",
            "    runs-on: macos-latest",
            "    steps:",
            "      - run: cargo +1.99.1 check -p fixture --all-targets",
            deny_job.as_str(),
            "  secrets:",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - uses: dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772 # master 2026-08-05",
            "        with:",
            "          toolchain: 1.99.1",
            "      - run: echo 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb | sha256sum -c -",
            &gitleaks_scan_step(),
            "  hygiene:",
            "    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          fetch-depth: 0",
            "          persist-credentials: false",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0",
            "        with:",
            "          bun-version: \"1.4.4\"",
            "      - name: Native JavaScript formatting",
            "        run: |",
            "          bun packages/@keld/kipc/scripts/echo-codegen.ts check",
            "          bun fmt --no-script --check \"**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}\"",
            "      - name: Documentation placeholder contracts",
            "        run: |",
            "          mkdir -p target/doc-placeholders",
            "          rustc --edition=2024 -D warnings --test tools/doc_placeholders.rs -o target/doc-placeholders/doc-placeholders-test",
            "          target/doc-placeholders/doc-placeholders-test",
            "          rustc --edition=2024 -D warnings tools/doc_placeholders.rs -o target/doc-placeholders/doc-placeholders",
            "          target/doc-placeholders/doc-placeholders check .",
            "      - name: Hello command contract",
            "        run: |",
            "          python3 -B tools/test_hello_command.py",
            "      - name: Atomic problem-solving protocol contract",
            "        run: |",
            "          mkdir -p target/atomic-protocol",
            "          rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test",
            "          target/atomic-protocol/atomic-protocol-test",
            "          rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol",
            "          target/atomic-protocol/atomic-protocol check .",
            "      - name: Agent instruction context budget",
            "        run: |",
            "          mkdir -p target/agent-context",
            "          rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test",
            "          target/agent-context/agent-context-test",
            "          rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context",
            "          target/agent-context/agent-context check .",
            "          python3 -B tools/test_session_closeout.py",
            "          python3 -B tools/test_session_closeout_hook.py",
            "          python3 -B tools/workspace.py check",
            "          python3 -B tools/test_workspace.py",
            "      - run: rustc --edition=2024 --test tools/ci_hygiene.rs",
            "      - name: Check this checkout",
            "        if: needs.changes.outputs.hygiene == 'true'",
            "        env:",
            "          KELD_CI_BASE_REF: ${{ github.event.pull_request.base.sha || github.event.before }}",
            "        run: |",
            "          rustc --edition=2024 -D warnings tools/ci_hygiene.rs -o target/ci-hygiene/ci-hygiene",
            "          target/ci-hygiene/ci-hygiene check .",
            "      - run: rustc --edition=2024 --test tools/product_status.rs",
            "      - run: product-status check .",
            "      - name: Public audit registry contracts",
            "        if: needs.changes.outputs.docs == 'true'",
            "        run: |",
            "          python3 -B docs/audits/verify.py",
            "          python3 -B tools/windows_architecture_closeout.py --check",
            "          python3 -B docs/audits/test_verify.py",
            "          python3 -B tools/test_windows_architecture_closeout.py",
            "      - run: rustc --edition=2024 --test tools/llms_docs.rs",
            "      - run: rustc --edition=2024 tools/llms_docs.rs",
            "      - run: llms-docs check .",
            "  doctest:",
            "    runs-on: ${{ matrix.os }}",
            "    needs: changes",
            "    if: needs.changes.outputs.doctest == 'true'",
            "    strategy:",
            "      matrix:",
            "        os: [macos-latest, windows-latest]",
            "    steps:",
            "      - name: cargo test --doc",
            "        shell: bash",
            "        env:",
            "          KELD_CI_DOCTEST_PACKAGES: ${{ needs.changes.outputs.doctest_packages }}",
            "        run: |",
            "          if [ -z \"$KELD_CI_DOCTEST_PACKAGES\" ]; then",
            "            echo \"::error::doctest lane ran with no package; check the router's doctest_packages output\"",
            "            exit 1",
            "          fi",
            "          for package in $KELD_CI_DOCTEST_PACKAGES; do",
            "            cargo test -p \"$package\" --doc",
            "          done",
            "  workspace-contracts:",
            "    runs-on: ${{ matrix.os }}",
            "    needs: changes",
            "    if: needs.changes.outputs.workspace == 'true'",
            "    strategy:",
            "      matrix:",
            "        os: [ubuntu-latest, macos-latest, windows-latest]",
            "    steps:",
            "      - name: Local workspace path and process contracts",
            "        shell: bash",
            "        run: |",
            "          if [ \"${{ matrix.os }}\" = windows-latest ]; then",
            "            python -B tools/test_workspace.py",
            "          else",
            "            python3 -B tools/test_workspace.py",
            "          fi",
            "  mermaid:",
            "    needs:",
            "      - changes",
            "    if: needs.changes.outputs.mermaid == 'true'",
            "    steps:",
            "      - name: Mermaid source contracts",
            "        run: |",
            "          mkdir -p target/mermaid-docs",
            "          rustc --edition=2024 -D warnings --test tools/mermaid_docs.rs -o target/mermaid-docs/mermaid-docs-test",
            "          target/mermaid-docs/mermaid-docs-test",
            "          rustc --edition=2024 -D warnings tools/mermaid_docs.rs -o target/mermaid-docs/mermaid-docs",
            "          target/mermaid-docs/mermaid-docs check .",
            "      - name: Render Mermaid docs in pinned isolated container",
            "        run: tools/mermaid_render_check.sh",
            "  required:",
            "    name: CI required",
            "    if: ${{ always() }}",
            "    needs:",
            "      - changes",
            "      - router-contracts",
            "      - fmt",
            "      - check",
            "      - bun-test",
            "      - linux-gui-smoke",
            "      - msrv",
            "      - deny",
            "      - secrets",
            "      - hygiene",
            "      - mermaid",
            "      - codeql-rust",
            "      - codeql-javascript-typescript",
            "      - codeql-actions",
            "      - dependency-review",
            "      - workspace-contracts",
            "      - doctest",
            "      - windows-media",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - name: Verify required CI results",
            "        env:",
            "          KELD_RESULT_CHANGES: ${{ needs.changes.result }}",
            "          KELD_RESULT_ROUTER_CONTRACTS: ${{ needs['router-contracts'].result }}",
            "          KELD_RESULT_FMT: ${{ needs.fmt.result }}",
            "          KELD_RESULT_CHECK: ${{ needs.check.result }}",
            "          KELD_RESULT_BUN: ${{ needs['bun-test'].result }}",
            "          KELD_RESULT_GUI: ${{ needs['linux-gui-smoke'].result }}",
            "          KELD_RESULT_MSRV: ${{ needs.msrv.result }}",
            "          KELD_RESULT_DENY: ${{ needs.deny.result }}",
            "          KELD_RESULT_SECRETS: ${{ needs.secrets.result }}",
            "          KELD_RESULT_HYGIENE: ${{ needs.hygiene.result }}",
            "          KELD_RESULT_MERMAID: ${{ needs.mermaid.result }}",
            "          KELD_ROUTE_RUST: ${{ needs.changes.outputs.rust }}",
            "          KELD_ROUTE_TS: ${{ needs.changes.outputs.ts }}",
            "          KELD_ROUTE_GUI: ${{ needs.changes.outputs.gui }}",
            "          KELD_ROUTE_MSRV: ${{ needs.changes.outputs.msrv }}",
            "          KELD_ROUTE_DENY: ${{ needs.changes.outputs.deny }}",
            "          KELD_ROUTE_HYGIENE: ${{ needs.changes.outputs.hygiene }}",
            "          KELD_ROUTE_DOCS: ${{ needs.changes.outputs.docs }}",
            "          KELD_ROUTE_MERMAID: ${{ needs.changes.outputs.mermaid }}",
            "          KELD_RESULT_CODEQL_RUST: ${{ needs['codeql-rust'].result }}",
            "          KELD_RESULT_CODEQL_JAVASCRIPT_TYPESCRIPT: ${{ needs['codeql-javascript-typescript'].result }}",
            "          KELD_RESULT_CODEQL_ACTIONS: ${{ needs['codeql-actions'].result }}",
            "          KELD_ROUTE_CODEQL_RUST: ${{ needs.changes.outputs.codeql_rust }}",
            "          KELD_ROUTE_CODEQL_JAVASCRIPT_TYPESCRIPT: ${{ needs.changes.outputs.codeql_javascript_typescript }}",
            "          KELD_ROUTE_CODEQL_ACTIONS: ${{ needs.changes.outputs.codeql_actions }}",
            "          KELD_RESULT_DEPENDENCY_REVIEW: ${{ needs['dependency-review'].result }}",
            "          KELD_RESULT_WORKSPACE: ${{ needs['workspace-contracts'].result }}",
            "          KELD_ROUTE_WORKSPACE: ${{ needs.changes.outputs.workspace }}",
            "          KELD_EVENT_NAME: ${{ github.event_name }}",
            "          KELD_ROUTE_RUST_DOCUMENTATION_ONLY: ${{ needs.changes.outputs.rust_documentation_only }}",
            "          KELD_ROUTE_CHECK_OS: ${{ needs.changes.outputs.check_os }}",
            "          KELD_RESULT_DOCTEST: ${{ needs.doctest.result }}",
            "          KELD_ROUTE_DOCTEST: ${{ needs.changes.outputs.doctest }}",
            "          KELD_RESULT_MEDIA: ${{ needs['windows-media'].result }}",
            "          KELD_ROUTE_MEDIA: ${{ needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv') }}",
            "        run: |",
            "          tools/ci_required.sh test",
            "          tools/ci_required.sh check \\",
            "            \"$KELD_RESULT_CHANGES\" \"$KELD_RESULT_FMT\" \"$KELD_RESULT_CHECK\" \\",
            "            \"$KELD_RESULT_BUN\" \"$KELD_RESULT_GUI\" \"$KELD_RESULT_MSRV\" \\",
            "            \"$KELD_RESULT_DENY\" \"$KELD_RESULT_SECRETS\" \"$KELD_RESULT_HYGIENE\" \\",
            "            \"$KELD_RESULT_MERMAID\" \\",
            "            \"$KELD_ROUTE_RUST\" \"$KELD_ROUTE_TS\" \"$KELD_ROUTE_GUI\" \\",
            "            \"$KELD_ROUTE_MSRV\" \"$KELD_ROUTE_DENY\" \\",
            "            \"$KELD_ROUTE_HYGIENE\" \"$KELD_ROUTE_DOCS\" \\",
            "            \"$KELD_ROUTE_MERMAID\" \\",
            "            \"$KELD_RESULT_CODEQL_RUST\" \"$KELD_RESULT_CODEQL_JAVASCRIPT_TYPESCRIPT\" \\",
            "            \"$KELD_RESULT_CODEQL_ACTIONS\" \\",
            "            \"$KELD_ROUTE_CODEQL_RUST\" \"$KELD_ROUTE_CODEQL_JAVASCRIPT_TYPESCRIPT\" \\",
            "            \"$KELD_ROUTE_CODEQL_ACTIONS\" \\",
            "            \"$KELD_RESULT_DEPENDENCY_REVIEW\" \\",
            "            \"$KELD_RESULT_WORKSPACE\" \"$KELD_ROUTE_WORKSPACE\" \\",
            "            \"$KELD_EVENT_NAME\" \"$KELD_ROUTE_RUST_DOCUMENTATION_ONLY\" \"$KELD_ROUTE_CHECK_OS\" \\",
            "            \"$KELD_RESULT_DOCTEST\" \"$KELD_ROUTE_DOCTEST\" \\",
            "            \"$KELD_RESULT_MEDIA\" \"$KELD_ROUTE_MEDIA\" \\",
            "            \"$KELD_RESULT_ROUTER_CONTRACTS\"",
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
         /crates/keld-updater-helper/ @alice\n\
         /.github/ @alice\n\
         /AGENTS.md @alice\n\
         /.agents/ @alice\n\
         /.codex/ @alice\n\
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
    temp.write(ATOMIC_PROTOCOL_CHECKER, "fn main() {}\n");
    temp.write(AGENT_CONTEXT_CHECKER, "fn main() {}\n");
    temp.write(
            JUSTFILE,
            concat!(
                "ci: audit-docs mermaid-ci js-fmt-check fmt-check clippy test doc deny\n",
                "audit-docs:\n",
                "    {{python_command}} -B docs/audits/verify.py\n",
                "    {{python_command}} -B tools/windows_architecture_closeout.py --check\n",
                "    {{python_command}} -B docs/audits/test_verify.py\n",
                "    {{python_command}} -B tools/test_windows_architecture_closeout.py\n",
                "ci-full: audit-docs mermaid-full js-fmt-check fmt-check clippy test doc deny\n",
                "js-fmt-check:\n    bun packages/@keld/kipc/scripts/echo-codegen.ts check\n    bun fmt --no-script --check \"**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}\"\n",
                "mermaid-full: mermaid-test mermaid-check mermaid-render-check\n",
                "mermaid-ci:\n",
                "    #!/usr/bin/env bash\n",
                "    set -euo pipefail\n",
                "    selected=$(tools/ci_changes.sh local | sed -n 's/^mermaid=//p')\n",
                "    case \"$selected\" in\n",
                "        true) just mermaid-full ;;\n",
                "        false) echo \"Mermaid route: skipped; no diagram or renderer input changed.\" ;;\n",
                "        *) exit 1 ;;\n",
                "    esac\n",
                "test:\n",
                "    #!/usr/bin/env bash\n",
                "    set -euo pipefail\n",
                "    if [[ \"$(uname -s)\" == \"Linux\" ]]; then\n",
                "        xvfb-run -a cargo nextest run --workspace --profile ci\n",
                "    else\n",
                "        cargo nextest run --workspace --profile ci\n",
                "    fi\n"
            ),
        );
    temp.write(MERMAID_CHECKER, "fn main() {}\n");
    temp.write(NEXTEST_CONFIG, "[profile.ci]\n");
    temp.write(MERMAID_RENDERER, include_str!("mermaid_render_check.sh"));
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
fn gates_inside_parallel_groups_count_for_ci_and_ci_full() {
    let temp = complete_fixture();
    let grouped = read(temp.path(), JUSTFILE)
            .expect("just fixture")
            .replacen(
                "ci: audit-docs mermaid-ci js-fmt-check fmt-check clippy test doc deny\n",
                "ci: policy js-fmt-check fmt-check clippy test doc deny\n\n[parallel]\npolicy: audit-docs mermaid-ci\n",
                1,
            )
            .replacen(
                "ci-full: audit-docs mermaid-full js-fmt-check fmt-check clippy test doc deny\n",
                "ci-full: policy-full js-fmt-check fmt-check clippy test doc deny\n\n[parallel]\npolicy-full: audit-docs mermaid-full\n",
                1,
            );
    temp.write(JUSTFILE, &grouped);
    check(temp.path()).expect("grouped gates still satisfy the local gate contract");

    for (needle, replacement, expected) in [
        (
            "policy: audit-docs mermaid-ci",
            "policy: mermaid-ci",
            "audit-docs",
        ),
        (
            "policy: audit-docs mermaid-ci",
            "policy: audit-docs mermaid-test",
            "routed `mermaid-ci`",
        ),
        (
            "policy-full: audit-docs mermaid-full",
            "policy-full: audit-docs",
            "force the full Mermaid path",
        ),
        (
            "policy-full: audit-docs mermaid-full",
            "policy-full: mermaid-full",
            "force the full Mermaid path",
        ),
        (
            "policy: audit-docs mermaid-ci\n",
            "policy: audit-docs mermaid-ci test\n",
            "one at a time",
        ),
        (
            "ci: policy js-fmt-check fmt-check clippy test doc deny",
            "ci: policy fmt-check clippy doc deny",
            "one at a time",
        ),
        (
            "ci: policy js-fmt-check fmt-check clippy test doc deny\n",
            "ci: policy rust deny\n\n[parallel]\nrust: fmt-check clippy test doc\n",
            "one at a time",
        ),
        (
            "ci: policy js-fmt-check fmt-check clippy test doc deny\n",
            "[parallel]\nci: policy js-fmt-check fmt-check clippy test doc deny\n",
            "one at a time",
        ),
        (
            "policy: audit-docs mermaid-ci",
            "policy: mermaid-ci # audit-docs",
            "audit-docs",
        ),
    ] {
        temp.write(JUSTFILE, &grouped.replacen(needle, replacement, 1));
        let error = check(temp.path()).expect_err("a weakened group must fail");
        assert!(error.contains(expected), "{needle}: {error}");
    }
}

#[test]
fn decorated_parallel_attributes_cannot_hide_rust_gates() {
    for attribute in [
        "[parallel] # scheduler",
        "[parallel]   ",
        "[parallel]\t",
        r#"[doc("literal # hash, parallel"), parallel] # scheduler"#,
    ] {
        for recipe in ["ci", "ci-full", "parallel-helper"] {
            let temp = complete_fixture();
            let original = read(temp.path(), JUSTFILE).expect("just fixture");
            check_serial_rust_gates(temp.path()).expect("serial fixture is valid");
            let mutated = if recipe == "parallel-helper" {
                format!("{original}\n{attribute}\nparallel-helper: test\n")
            } else {
                original.replacen(&format!("{recipe}:"), &format!("{attribute}\n{recipe}:"), 1)
            };
            temp.write(JUSTFILE, &mutated);
            let error = check_serial_rust_gates(temp.path())
                .expect_err("decorated parallel attribute must remain effective");
            assert!(error.contains("one at a time"), "{error}");
            check(temp.path()).expect_err("the root checker must invoke the serial guard");
        }
    }
    let temp = complete_fixture();
    let original = read(temp.path(), JUSTFILE).expect("just fixture");
    temp.write(
        JUSTFILE,
        &original.replacen(
            "ci:",
            "[doc(\"literal # hash, parallel\"), private] # parallel\nci:",
            1,
        ),
    );
    check(temp.path()).expect("quoted and commented parallel words are not attributes");
}

#[test]
fn bodyful_dependencies_cannot_hide_transitive_parallel_rust_gates() {
    for dependency in ["test", "middle"] {
        let temp = complete_fixture();
        let original = read(temp.path(), JUSTFILE).expect("just fixture");
        let harmless = format!(
            "{original}\n[parallel]\nparallel-helper: helper\n\nhelper: harmless\n    true\n\nharmless:\n    true\n\nmiddle: test\n    true\n"
        );
        temp.write(JUSTFILE, &harmless);
        check_serial_rust_gates(temp.path()).expect("bodyful safe helper is valid");
        check(temp.path()).expect("safe helper does not alter the gate inventory");
        temp.write(
            JUSTFILE,
            &harmless.replace("helper: harmless", &format!("helper: {dependency}")),
        );
        let error = check_serial_rust_gates(temp.path())
            .expect_err("the parallel dependency graph reaches a Rust gate");
        assert!(error.contains("reaches `test`"), "{error}");
        check(temp.path()).expect_err("the root checker must invoke the serial guard");
    }
}

#[test]
fn typescript_cannot_run_inside_the_parallel_policy_batch() {
    for dependency in ["typescript", "helper"] {
        let temp = complete_fixture();
        let original = read(temp.path(), JUSTFILE).expect("fixture");
        temp.write(JUSTFILE, &format!(
                "{original}\n[parallel]\npolicy-probe: {dependency}\n\nhelper: typescript\n    true\n\ntypescript:\n    true\n"
            ));
        let error =
            check_serial_rust_gates(temp.path()).expect_err("compiler load must be isolated");
        assert!(error.contains("reaches `typescript`"), "{error}");
    }
}

#[test]
fn rust_suffix_order_and_policy_before_rust_are_enforced() {
    for replacement in [
        "clippy fmt-check test doc deny",
        "js-fmt-check fmt-check clippy test doc deny audit-docs",
    ] {
        let temp = complete_fixture();
        let original = read(temp.path(), JUSTFILE).expect("just fixture");
        check_serial_rust_gates(temp.path()).expect("original suffix is valid");
        temp.write(
            JUSTFILE,
            &original.replace("js-fmt-check fmt-check clippy test doc deny", replacement),
        );
        let error = check_serial_rust_gates(temp.path())
            .expect_err("the Rust gates must be the ordered serial suffix");
        assert!(error.contains("must end with"), "{error}");
        check(temp.path()).expect_err("the root checker must invoke the serial guard");
    }
}

#[test]
fn public_audit_gate_is_mandatory_locally_and_in_docs_ci() {
    let temp = complete_fixture();
    temp.write(
        JUSTFILE,
        &read(temp.path(), JUSTFILE).expect("just fixture").replacen(
            "ci: audit-docs mermaid-ci",
            "ci: mermaid-ci",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("root ci must own public audit gate");
    assert!(error.contains("audit-docs"), "{error}");

    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen("          python3 -B docs/audits/test_verify.py\n", "", 1),
    );
    let error = check(temp.path()).expect_err("audit mutation suite must run in docs CI");
    assert!(error.contains("Public audit registry contracts"), "{error}");

    // #942: the Windows closeout ledger check and its tests run in both lanes.
    for command in [
        "python3 -B tools/windows_architecture_closeout.py --check",
        "python3 -B tools/test_windows_architecture_closeout.py",
    ] {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(&format!("          {command}\n"), "", 1),
        );
        let error = check(temp.path()).expect_err("closeout ledger must run in docs CI");
        assert!(error.contains("Public audit registry contracts"), "{error}");

        let temp = complete_fixture();
        let local = command.replacen("python3 ", "{{python_command}} ", 1);
        temp.write(
            JUSTFILE,
            &read(temp.path(), JUSTFILE).expect("just fixture").replacen(
                &format!("    {local}\n"),
                "",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("closeout ledger must run in audit-docs");
        assert!(error.contains("audit-docs"), "{error}");
    }

    let temp = complete_fixture();
    let workflow = valid_workflow().replacen(
            "          fetch-depth: 0\n          persist-credentials: false\n      - uses: oven-sh/setup-bun",
            "          persist-credentials: false\n      - uses: oven-sh/setup-bun",
            1,
        );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("audit CI needs full history");
    assert!(error.contains("full history"), "{error}");
}

#[test]
fn root_test_recipe_requires_linux_xvfb_without_weakening_other_platforms() {
    for (needle, replacement) in [
        ("xvfb-run -a ", ""),
        (
            "if [[ \"$(uname -s)\" == \"Linux\" ]]; then",
            "if false; then",
        ),
        ("cargo nextest run --workspace --profile ci", "true"),
    ] {
        let temp = complete_fixture();
        temp.write(
            JUSTFILE,
            &read(temp.path(), JUSTFILE)
                .expect("root test fixture")
                .replacen(needle, replacement, 1),
        );
        let error = check(temp.path()).expect_err("display contract must remain executable");
        assert!(error.contains("xvfb-run"), "{needle}: {error}");
    }
}

#[test]
fn linux_media_guard_commands_are_mandatory_in_the_gui_lane() {
    for (needle, replacement) in [
        (
            "          cargo build -p keld-wv --example linux_media_guard\n",
            "          removed-media-build\n",
        ),
        (
            "crates/keld-wv/tests/fixtures/linux_media_interpose.c",
            "removed-media-interposer.c",
        ),
        ("-Wall -Wextra -Werror -Wpedantic", "-Wall -Wextra"),
        (
            "xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host",
            "removed-media-run",
        ),
    ] {
        let temp = complete_fixture();
        let workflow = valid_workflow().replacen(needle, replacement, 1);
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("missing media command must fail");
        assert!(error.contains("KEL-132"), "{needle}: {error}");
    }
}

#[test]
fn linux_media_guard_rejects_inert_or_conditional_commands() {
    for (needle, replacement) in [
        (
            "          cargo build -p keld-wv --example linux_media_guard\n",
            "          echo cargo build -p keld-wv --example linux_media_guard\n",
        ),
        (
            "      - name: Build Linux media guard probe\n        run: |\n",
            "      - name: Build Linux media guard probe\n        if: false\n        run: |\n",
        ),
        (
            "        run: xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n",
            "        run: |\n          exit\n          xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n",
        ),
        (
            "        run: xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n",
            "        run: |\n          if [ 1 -eq 0 ]; then\n            xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n          fi\n",
        ),
    ] {
        let temp = complete_fixture();
        let workflow = valid_workflow().replacen(needle, replacement, 1);
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("inert media command must fail");
        assert!(error.contains("KEL-132"), "{needle}: {error}");
    }
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
fn doc_placeholder_and_hello_steps_run_whenever_the_hygiene_job_runs() {
    check_doc_placeholders_step(&valid_workflow()).expect("fixture placeholder step passes");
    check_hello_command_step(&valid_workflow()).expect("fixture hello step passes");
    for (old, new, label, needle) in [
        (
            "      - name: Documentation placeholder contracts\n",
            "      - name: Removed placeholder step\n",
            "missing placeholder step",
            "Documentation placeholder contracts",
        ),
        (
            "      - name: Documentation placeholder contracts\n        run:",
            "      - name: Documentation placeholder contracts\n        if: needs.changes.outputs.rust == 'true'\n        run:",
            "placeholder step gated off the docs lane",
            "only its `run` key",
        ),
        (
            "          target/doc-placeholders/doc-placeholders check .",
            "          target/doc-placeholders/doc-placeholders check . || true",
            "suppressed placeholder check",
            "without wrappers",
        ),
        (
            "          target/doc-placeholders/doc-placeholders-test\n",
            "",
            "placeholder self-tests dropped",
            "without wrappers",
        ),
        (
            "      - name: Hello command contract\n",
            "      - name: Removed hello step\n",
            "missing hello step",
            "Hello command contract",
        ),
        (
            "          python3 -B tools/test_hello_command.py\n",
            "          echo python3 -B tools/test_hello_command.py\n",
            "echoed hello contract",
            "without wrappers",
        ),
    ] {
        assert!(valid_workflow().contains(old), "{label}");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(old, new, 1));
        let error = check(temp.path()).expect_err(label);
        assert!(error.contains(needle), "{label}: {error}");
    }
}

#[test]
fn agents_md_runs_unconditionally_in_the_router_job() {
    for (old, new, label) in [
        (
            "      - name: Agent instruction inventory (agents-md)\n        run: tools/agents_md.sh\n",
            "",
            "missing agents-md step",
        ),
        (
            "      - name: Agent instruction inventory (agents-md)\n        run: tools/agents_md.sh\n",
            "      - name: Agent instruction inventory (agents-md)\n        if: needs.changes.outputs.hygiene == 'true'\n        run: tools/agents_md.sh\n",
            "agents-md gated off crates changes",
        ),
        (
            "        run: tools/agents_md.sh\n",
            "        run: echo tools/agents_md.sh\n",
            "echoed agents-md",
        ),
        (
            "      - name: Agent instruction inventory (agents-md)\n        run: tools/agents_md.sh\n",
            "      - name: Agent instruction inventory (agents-md)\n        run: tools/agents_md.sh\n        if: ${{ false }}\n",
            "condition after run",
        ),
        (
            "      - name: Agent instruction inventory (agents-md)\n        run: tools/agents_md.sh\n",
            "      - name: Agent instruction inventory (agents-md)\n        run: tools/agents_md.sh\n        \"if\": false\n",
            "quoted condition after run",
        ),
    ] {
        assert!(valid_workflow().contains(old), "{label}");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(old, new, 1));
        let error = check(temp.path()).expect_err(label);
        assert!(
            error.contains("Agent instruction inventory"),
            "{label}: {error}"
        );
    }
}

#[test]
fn atomic_protocol_step_is_mandatory_and_failure_preserving() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "      - name: Atomic problem-solving protocol contract\n",
            "      - name: Removed atomic step\n",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("missing atomic step must fail");
    assert!(error.contains("Atomic problem-solving"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          target/atomic-protocol/atomic-protocol check .",
            "          echo target/atomic-protocol/atomic-protocol check .",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("echoed atomic check must fail");
    assert!(error.contains("without wrappers"), "{error}");

    temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Atomic problem-solving protocol contract\n        run:",
                "      - name: Atomic problem-solving protocol contract\n        if: ${{ false }}\n        run:",
                1,
            ),
        );
    let error = check(temp.path()).expect_err("skipped atomic step must fail");
    assert!(error.contains("may contain only"), "{error}");

    temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
                "    if: needs.changes.outputs.hygiene == 'true'",
                1,
            ),
        );
    let error = check(temp.path()).expect_err("docs-only atomic skip must fail");
    assert!(error.contains("both hygiene and docs"), "{error}");

    temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "  hygiene:\n    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
                "  hygiene:\n    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'\n    continue-on-error: true",
                1,
            ),
        );
    let error = check(temp.path()).expect_err("hygiene failure suppression must fail");
    assert!(error.contains("continue-on-error"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "  hygiene:\n    if:",
            "  hygiene:\n    \"continue-on-error\": true\n    if:",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("quoted failure suppression must fail");
    assert!(error.contains("continue-on-error"), "{error}");

    for (needle, replacement, label) in [
        (
            "  hygiene:\n    if:",
            "  hygiene:\n    defaults:\n      run:\n        shell: bash {0} || true\n    if:",
            "job inherited shell",
        ),
        (
            "jobs:\n",
            "defaults:\n  run:\n    shell: bash {0} || true\njobs:\n",
            "workflow inherited shell",
        ),
    ] {
        temp.write(WORKFLOW, &valid_workflow().replacen(needle, replacement, 1));
        let error = check(temp.path()).expect_err(label);
        assert!(error.contains("defaults"), "{error}");
    }

    let real_step = concat!(
        "      - name: Atomic problem-solving protocol contract\n",
        "        run: |\n",
        "          mkdir -p target/atomic-protocol\n",
        "          rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test\n",
        "          target/atomic-protocol/atomic-protocol-test\n",
        "          rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol\n",
        "          target/atomic-protocol/atomic-protocol check .\n",
    );
    let decoy_step = concat!(
        "      - name: Disabled decoy\n",
        "        if: ${{ false }}\n",
        "        run: |\n",
        "          - name: Atomic problem-solving protocol contract\n",
        "            run: |\n",
        "              mkdir -p target/atomic-protocol\n",
        "              rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test\n",
        "              target/atomic-protocol/atomic-protocol-test\n",
        "              rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol\n",
        "              target/atomic-protocol/atomic-protocol check .\n",
    );
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(real_step, decoy_step, 1),
    );
    let error = check(temp.path()).expect_err("nested decoy step must fail");
    assert!(error.contains("exactly one"), "{error}");
}

#[test]
fn agent_context_step_is_mandatory_and_failure_preserving() {
    let temp = complete_fixture();
    let context_step = concat!(
        "      - name: Agent instruction context budget\n",
        "        run: |\n",
        "          mkdir -p target/agent-context\n",
        "          rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test\n",
        "          target/agent-context/agent-context-test\n",
        "          rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context\n",
        "          target/agent-context/agent-context check .\n",
        "          python3 -B tools/test_session_closeout.py\n",
        "          python3 -B tools/test_session_closeout_hook.py\n",
        "          python3 -B tools/workspace.py check\n",
        "          python3 -B tools/test_workspace.py\n",
    );
    temp.write(WORKFLOW, &valid_workflow().replacen(context_step, "", 1));
    let error = check(temp.path()).expect_err("missing context gate must fail");
    assert!(
        error.contains("Agent instruction context budget"),
        "{error}"
    );

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          target/agent-context/agent-context check .",
            "          echo target/agent-context/agent-context check .",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("echoed context gate must fail");
    assert!(error.contains("without wrappers"), "{error}");
}

#[test]
fn hosted_closeout_suites_cannot_be_omitted_or_echoed() {
    for command in [
        "python3 -B tools/test_session_closeout.py",
        "python3 -B tools/test_session_closeout_hook.py",
        "python3 -B tools/workspace.py check",
        "python3 -B tools/test_workspace.py",
    ] {
        for replacement in [
            String::new(),
            format!("echo {command}"),
            format!("{command} || true"),
        ] {
            let temp = complete_fixture();
            temp.write(
                WORKFLOW,
                &valid_workflow().replacen(command, &replacement, 1),
            );
            let error =
                check(temp.path()).expect_err("missing or swallowed closeout tests must fail");
            assert!(error.contains("without wrappers"), "{error}");
        }
    }
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

    temp.write(
            WORKFLOW,
            &valid_workflow()
                .replacen("      - secrets\n", "", 1)
                .replacen(
                    "      - name: Verify required CI results\n        env:\n",
                    "      - name: Verify required CI results\n        env:\n          SPOOF: '- secrets'\n",
                    1,
                ),
        );
    let error = check(temp.path()).expect_err("needs text in env must not count");
    assert!(error.contains("secrets"), "{error}");
}

#[test]
fn required_result_must_receive_router_applicability() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          KELD_ROUTE_RUST: ${{ needs.changes.outputs.rust }}\n",
            "",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("missing router output must fail");
    assert!(error.contains("KELD_ROUTE_RUST"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen("\"$KELD_ROUTE_TS\"", "false", 1),
    );
    let error = check(temp.path()).expect_err("unused router output must fail");
    assert!(error.contains("35-argument"), "{error}");
}

#[test]
fn required_result_must_observe_security_jobs() {
    for job in [
        "codeql-rust",
        "codeql-javascript-typescript",
        "codeql-actions",
        "dependency-review",
        "workspace-contracts",
        "doctest",
        "windows-media",
    ] {
        let workflow = valid_workflow().replacen(&format!("      - {job}\n"), "", 1);
        let error = check_required_job(&workflow).expect_err("missing security job must fail");
        assert!(error.contains(job), "{error}");
    }
}

#[test]
fn required_result_must_receive_security_results_without_spoofing() {
    for (key, expression) in [
        (
            "KELD_RESULT_CODEQL_RUST",
            "${{ needs['codeql-rust'].result }}",
        ),
        (
            "KELD_RESULT_CODEQL_JAVASCRIPT_TYPESCRIPT",
            "${{ needs['codeql-javascript-typescript'].result }}",
        ),
        (
            "KELD_RESULT_CODEQL_ACTIONS",
            "${{ needs['codeql-actions'].result }}",
        ),
        (
            "KELD_ROUTE_CODEQL_RUST",
            "${{ needs.changes.outputs.codeql_rust }}",
        ),
        (
            "KELD_ROUTE_CODEQL_JAVASCRIPT_TYPESCRIPT",
            "${{ needs.changes.outputs.codeql_javascript_typescript }}",
        ),
        (
            "KELD_ROUTE_CODEQL_ACTIONS",
            "${{ needs.changes.outputs.codeql_actions }}",
        ),
        (
            "KELD_RESULT_DEPENDENCY_REVIEW",
            "${{ needs['dependency-review'].result }}",
        ),
    ] {
        let workflow = valid_workflow().replacen(expression, "success", 1);
        let error = check_required_job(&workflow).expect_err("spoofed security result must fail");
        assert!(error.contains(key), "{error}");
        let workflow = valid_workflow().replacen(&format!("\"${key}\""), "success", 1);
        let error = check_required_job(&workflow).expect_err("unused security result must fail");
        assert!(error.contains("35-argument"), "{error}");
    }
}

#[test]
fn check_job_os_matrix_comes_only_from_the_router() {
    let router = "        os: ${{ fromJSON(needs.changes.outputs.check_os) }}\n";
    assert!(valid_workflow().contains(router));
    check_check_job_os_matrix(&valid_workflow()).expect("router-owned OS matrix passes");
    for (replacement, label) in [
        (
            "        os: [ubuntu-latest, macos-latest, windows-latest]\n",
            "literal list",
        ),
        ("        os: [ubuntu-latest]\n", "literal single OS"),
        (
            "        os: ${{ fromJSON(needs.changes.outputs.packages) }}\n",
            "other output",
        ),
        ("", "missing matrix"),
        (
            "        os: ${{ fromJSON(needs.changes.outputs.check_os) }}\n        os: [windows-latest]\n",
            "duplicate key",
        ),
    ] {
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(router, replacement, 1));
        let error = check(temp.path()).expect_err(label);
        assert!(error.contains("check_os"), "{label}: {error}");
    }
}

#[test]
fn doctest_job_is_routed_two_os_and_executed() {
    check_doctest_job(&valid_workflow()).expect("fixture doctest job passes");
    for (old, new, label) in [
        (
            "    if: needs.changes.outputs.doctest == 'true'\n",
            "    if: needs.changes.outputs.rust == 'true'\n",
            "gated on another output",
        ),
        (
            "    if: needs.changes.outputs.doctest == 'true'\n",
            "",
            "ungated",
        ),
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.doctest == 'true'\n",
            "    runs-on: macos-latest\n    needs: changes\n    if: needs.changes.outputs.doctest == 'true'\n",
            "fixed runner runs every matrix row on macOS",
        ),
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.doctest == 'true'\n",
            "    needs: changes\n    if: needs.changes.outputs.doctest == 'true'\n",
            "missing runner",
        ),
        (
            "        os: [macos-latest, windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "        os: [macos-latest]\n    steps:\n      - name: cargo test --doc\n",
            "macOS alone never compiles a cfg(windows) doctest",
        ),
        (
            "        os: [macos-latest, windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "        os: [windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "Windows alone skips the ungated doctests' macOS proof",
        ),
        (
            "        os: [macos-latest, windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "        os: [ubuntu-latest, macos-latest, windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "Ubuntu runner needs GTK apt",
        ),
        (
            "    if: needs.changes.outputs.doctest == 'true'\n    strategy:\n      matrix:\n        os: [macos-latest, windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "    if: needs.changes.outputs.doctest == 'true'\n    env:\n      NOTE: 'os: [macos-latest, windows-latest]'\n    strategy:\n      matrix:\n        os: [macos-latest]\n    steps:\n      - name: cargo test --doc\n",
            "single-OS matrix with the two-OS text elsewhere",
        ),
        (
            "    if: needs.changes.outputs.doctest == 'true'\n    strategy:\n      matrix:\n        os: [macos-latest, windows-latest]\n    steps:\n      - name: cargo test --doc\n",
            "    if: needs.changes.outputs.doctest == 'true'\n    steps:\n      - name: cargo test --doc\n",
            "no matrix",
        ),
        (
            "          KELD_CI_DOCTEST_PACKAGES: ${{ needs.changes.outputs.doctest_packages }}\n",
            "          KELD_CI_DOCTEST_PACKAGES: ${{ needs.changes.outputs.packages }}\n",
            "unfiltered package list includes bin-only packages",
        ),
        (
            "            cargo test -p \"$package\" --doc\n",
            "            cargo test -p \"$package\" --doc || true\n",
            "suppressed failure",
        ),
        (
            "            exit 1\n          fi\n          for package in $KELD_CI_DOCTEST_PACKAGES",
            "            exit 0\n          fi\n          for package in $KELD_CI_DOCTEST_PACKAGES",
            "empty selection passes",
        ),
        (
            "      - name: cargo test --doc\n        shell: bash\n",
            "      - name: cargo test --doc\n        if: false\n        shell: bash\n",
            "skipped step",
        ),
        ("  doctest:\n", "  doctests:\n", "renamed job"),
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.doctest == 'true'\n",
            "    runs-on: ${{ matrix.os }}\n    if: needs.changes.outputs.doctest == 'true'\n",
            "missing router dependency",
        ),
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.doctest == 'true'\n",
            "    runs-on: ${{ matrix.os }}\n    needs: fmt\n    if: needs.changes.outputs.doctest == 'true'\n",
            "wrong dependency",
        ),
    ] {
        assert!(valid_workflow().contains(old), "{label}");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(old, new, 1));
        let error = check(temp.path()).expect_err(label);
        assert!(error.contains("doctest"), "{label}: {error}");
    }
}

#[test]
fn workspace_contracts_job_is_routed_cross_os_and_executed() {
    check_workspace_contracts_job(&valid_workflow()).expect("fixture workspace job passes");
    for (old, new, label) in [
        (
            "    if: needs.changes.outputs.workspace == 'true'\n",
            "    if: needs.changes.outputs.rust == 'true'\n",
            "gated on another output",
        ),
        (
            "    if: needs.changes.outputs.workspace == 'true'\n",
            "",
            "ungated",
        ),
        (
            "        os: [ubuntu-latest, macos-latest, windows-latest]\n    steps:\n      - name: Local workspace",
            "        os: [ubuntu-latest]\n    steps:\n      - name: Local workspace",
            "single OS",
        ),
        (
            "      - name: Local workspace path and process contracts\n        shell: bash\n",
            "      - name: Local workspace path and process contracts\n        if: false\n        shell: bash\n",
            "skipped step",
        ),
        (
            "            python3 -B tools/test_workspace.py\n",
            "            echo python3 -B tools/test_workspace.py\n",
            "echoed command",
        ),
        (
            "  workspace-contracts:\n",
            "  workspace-contract:\n",
            "renamed job",
        ),
        // The doctest job precedes this one in the fixture with the same runner and
        // dependency lines, so each mutation names the workspace job's `if` too.
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.workspace == 'true'\n",
            "    runs-on: ${{ matrix.os }}\n    if: needs.changes.outputs.workspace == 'true'\n",
            "missing router dependency",
        ),
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.workspace == 'true'\n",
            "    runs-on: ubuntu-latest\n    needs: changes\n    if: needs.changes.outputs.workspace == 'true'\n",
            "fixed runner",
        ),
        (
            "    runs-on: ${{ matrix.os }}\n    needs: changes\n    if: needs.changes.outputs.workspace == 'true'\n",
            "    needs: changes\n    if: needs.changes.outputs.workspace == 'true'\n",
            "missing runner",
        ),
        (
            "    if: needs.changes.outputs.workspace == 'true'\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-latest, windows-latest]\n",
            "    if: needs.changes.outputs.workspace == 'true'\n    env:\n      NOTE: 'os: [ubuntu-latest, macos-latest, windows-latest]'\n    strategy:\n      matrix:\n        os: [ubuntu-latest]\n",
            "single-OS matrix with the three-OS text elsewhere",
        ),
    ] {
        assert!(valid_workflow().contains(old), "{label}");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(old, new, 1));
        let error = check(temp.path()).expect_err(label);
        assert!(error.contains("workspace-contracts"), "{label}: {error}");
    }
}

#[test]
fn required_evaluator_text_must_be_executed() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          tools/ci_required.sh check \\",
            "          echo tools/ci_required.sh check \\",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("echoed evaluator text must fail");
    assert!(error.contains("run block"), "{error}");
}

#[test]
fn required_evaluator_rejects_false_green_shell_shapes() {
    let temp = complete_fixture();

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          KELD_RESULT_SECRETS: ${{ needs.secrets.result }}",
            "          KELD_RESULT_SECRETS: success",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("literal gitleaks success must fail");
    assert!(error.contains("KELD_RESULT_SECRETS"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "            \"$KELD_ROUTE_HYGIENE\" \"$KELD_ROUTE_DOCS\"",
            "            \"$KELD_ROUTE_HYGIENE\" \"$KELD_ROUTE_DOCS\" || true",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("exit-status suppression must fail");
    assert!(error.contains("run block"), "{error}");

    temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Verify required CI results\n        env:",
                "      - name: Verify required CI results\n        continue-on-error: true\n        env:",
                1,
            ),
        );
    let error = check(temp.path()).expect_err("continue-on-error must fail");
    assert!(error.contains("continue-on-error"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "  required:\n    name: CI required",
            "  required:\n    name: CI required\n    continue-on-error: true",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("job continue-on-error must fail");
    assert!(error.contains("continue-on-error"), "{error}");

    for (property, label) in [
        ("if: ${{ false }}", "skipped evaluator"),
        ("shell: bash {0} || true", "failure-suppressing shell"),
        ("\"if\": ${{ false }}", "quoted skipped evaluator"),
        (
            "\"shell\": bash {0} || true",
            "quoted failure-suppressing shell",
        ),
    ] {
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Verify required CI results\n        env:",
                &format!(
                    "      - name: Verify required CI results\n        {property}\n        env:"
                ),
                1,
            ),
        );
        let error = check(temp.path()).expect_err(label);
        assert!(error.contains("only direct"), "{error}");
    }

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          tools/ci_required.sh check \\",
            "          if false; then\n          tools/ci_required.sh check \\",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("unreachable check must fail");
    assert!(error.contains("run block"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(
            "          tools/ci_required.sh check \\",
            "          KELD_RESULT_SECRETS=success\n          tools/ci_required.sh check \\",
            1,
        ),
    );
    let error = check(temp.path()).expect_err("result reassignment must fail");
    assert!(error.contains("run block"), "{error}");
}

#[test]
fn required_evaluator_refuses_unreviewed_step_environment() {
    let baseline = valid_workflow();
    check_required_job(&baseline).expect("reviewed required result baseline");
    let needle = "      - name: Verify required CI results\n        env:\n";
    assert!(baseline.contains(needle));
    for (label, injection) in [
        (
            "shell startup override",
            "          BASH_ENV: ./tools/ignored.sh",
        ),
        ("path shadow", "          PATH: ./untrusted/bin"),
        (
            "GitHub environment file",
            "          GITHUB_ENV: ./untrusted/out",
        ),
        ("ordinary unreviewed binding", "          UNREVIEWED: value"),
    ] {
        let mutated = baseline.replacen(needle, &format!("{needle}{injection}\n"), 1);
        let error = check_required_job(&mutated).expect_err(label);
        assert!(error.contains("env"), "{label}: {error}");
    }
}

#[test]
fn required_job_refuses_inherited_environment() {
    let baseline = valid_workflow();
    check_required_job(&baseline).expect("reviewed required job baseline");
    let needle = "  required:\n    name: CI required\n";
    assert!(baseline.contains(needle));
    for (label, injection) in [
        (
            "shell startup override",
            "    env:\n      BASH_ENV: ./tools/ignored.sh\n",
        ),
        ("path shadow", "    env:\n      PATH: ./untrusted/bin\n"),
        ("flow mapping", "    env: {BASH_ENV: ./tools/ignored.sh}\n"),
    ] {
        let mutated = baseline.replacen(needle, &format!("{needle}{injection}"), 1);
        let error = check_required_job(&mutated).expect_err(label);
        assert!(error.contains("env"), "{label}: {error}");
    }
}

#[test]
fn required_job_preserves_only_reviewed_workflow_environment() {
    let baseline = valid_workflow();
    check_required_job(&baseline).expect("reviewed workflow environment baseline");
    for (label, old, replacement) in [
        (
            "additional Bash startup authority",
            "env:\n",
            "env:\n  BASH_ENV: ./tools/ignored.sh\n",
        ),
        (
            "additional GitHub environment writer",
            "env:\n",
            "env:\n  GITHUB_ENV: ./untrusted/out\n",
        ),
        ("mutable PATH", "env:\n", "env:\n  PATH: ./untrusted/bin\n"),
        (
            "missing reviewed compiler environment",
            "  CARGO_INCREMENTAL: \"0\"\n",
            "",
        ),
        (
            "changed reviewed compiler environment",
            "  CARGO_INCREMENTAL: \"0\"\n",
            "  CARGO_INCREMENTAL: \"1\"\n",
        ),
        (
            "duplicate workflow environment overrides the reviewed map",
            "jobs:\n",
            "env:\n  BASH_ENV: ./tools/ignored.sh\njobs:\n",
        ),
    ] {
        assert!(baseline.contains(old), "{label}: mutation input must exist");
        let mutated = baseline.replacen(old, replacement, 1);
        let error = check_required_job(&mutated).expect_err(label);
        assert!(error.contains("env"), "{label}: {error}");
    }
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
fn ci_profile_that_retries_is_rejected() {
    let temp = complete_fixture();
    temp.write(NEXTEST_CONFIG, "[profile.ci]\nretries = 1\n");
    let error =
        check(temp.path()).expect_err("the mandated verification profile must not retry (KEL-112)");
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
    let error =
        check(temp.path()).expect_err("an override under the mandated profile still retries it");
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
        let error = check(temp.path()).expect_err("a retries sub-table retries the mandated gate");
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
    let error = check(temp.path()).expect_err("a commented header is still the mandated profile");
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
    let error = check(temp.path()).expect_err("a parent this check cannot follow must fail closed");
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
fn router_test_in_another_job_does_not_satisfy_independent_contract() {
    let temp = complete_fixture();
    let workflow = valid_workflow()
        .replacen("        run: tools/ci_changes_test.sh\n", "", 1)
        .replace(
            "  secrets:\n",
            "  unrelated:\n    steps:\n      - run: tools/ci_changes_test.sh\n  secrets:\n",
        );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("router contract test must run in its dedicated job");
    assert!(error.contains("router-contracts`"), "{error}");
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
                "  secrets:\n    steps:\n",
                &format!("  secrets:\n    steps:\n{PINNED_CHECKOUT}        with:\n          persist-credentials: false\n          fetch-depth: 0\n"),
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
                "  check:\n    strategy:",
                "  check:\n    if: needs.changes.outputs.rust == 'true' && matrix.os != 'ubuntu-latest'\n    strategy:",
                1,
            ),
        );
    let error = check(temp.path()).expect_err("job-level matrix.os must fail hygiene");
    assert!(error.contains("matrix"), "{error}");
    assert!(error.contains("check"), "{error}");
}

#[test]
fn missing_fuzz_workspace_step_fails() {
    let temp = complete_fixture();
    let step = "      - name: Check keld-ipc fuzz workspace\n        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'\n        run: |\n          cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml\n";
    temp.write(WORKFLOW, &valid_workflow().replace(step, ""));
    let error = check(temp.path()).expect_err("missing fuzz workspace check must fail");
    assert!(error.contains("Check keld-ipc fuzz workspace"), "{error}");
}

#[test]
fn missing_ipc_fuzzing_hook_test_step_fails() {
    let temp = complete_fixture();
    let step = "      - name: Test keld-ipc fuzzing hook
        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'
        run: |
          cargo test -p keld-ipc --features fuzzing --lib attempt::records
";
    temp.write(WORKFLOW, &valid_workflow().replace(step, ""));
    let error = check(temp.path()).expect_err("missing fuzzing hook test must fail");
    assert!(error.contains("Test keld-ipc fuzzing hook"), "{error}");
}

#[test]
fn missing_wv_fuzz_workspace_step_fails() {
    let temp = complete_fixture();
    let step = "      - name: Check keld-wv fuzz workspace\n        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'\n        run: |\n          cargo check --manifest-path crates/keld-wv/fuzz/Cargo.toml\n";
    temp.write(WORKFLOW, &valid_workflow().replace(step, ""));
    let error = check(temp.path()).expect_err("missing keld-wv fuzz workspace check must fail");
    assert!(error.contains("Check keld-wv fuzz workspace"), "{error}");
}

#[test]
fn missing_update_fuzz_workspace_step_fails() {
    let temp = complete_fixture();
    let step = "      - name: Check keld-update fuzz workspace\n        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'\n        run: |\n          cargo check --manifest-path crates/keld-update/fuzz/Cargo.toml\n";
    temp.write(WORKFLOW, &valid_workflow().replace(step, ""));
    let error = check(temp.path()).expect_err("missing keld-update fuzz workspace check must fail");
    assert!(
        error.contains("Check keld-update fuzz workspace"),
        "{error}"
    );
}

#[test]
fn fuzz_workspace_step_must_be_ubuntu_and_rust_routed() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replace(
            "matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
            "matrix.os == 'ubuntu-latest'",
        ),
    );
    let error = check(temp.path()).expect_err("fuzz workspace step needs both routing gates");
    assert!(error.contains("exact condition"), "{error}");
}

#[test]
fn fuzz_workspace_step_must_run_the_exact_stable_check() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replace(
            "cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml",
            "cargo check",
        ),
    );
    let error = check(temp.path()).expect_err("fuzz workspace step needs its direct command");
    assert!(
        error.contains("stable fuzz-workspace cargo check"),
        "{error}"
    );
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
fn bun_lane_runs_on_every_os() {
    let workflow = valid_workflow();
    check_bun_test_job(&workflow).expect("the three-OS Bun lane passes");
    let lane = "    if: needs.changes.outputs.ts == 'true'\n    strategy:\n      fail-fast: false\n      matrix:\n";
    for (old, new, label) in [
        (
            format!("{lane}        os: [ubuntu-latest, macos-latest, windows-latest]\n"),
            format!("{lane}        os: [ubuntu-latest]\n"),
            "Ubuntu only",
        ),
        (
            format!("{lane}        os: [ubuntu-latest, macos-latest, windows-latest]\n"),
            format!("{lane}        os: [ubuntu-latest, macos-latest]\n"),
            "no Windows",
        ),
        (
            String::from("  bun-test:\n    runs-on: ${{ matrix.os }}\n"),
            String::from("  bun-test:\n    runs-on: ubuntu-latest\n"),
            "fixed runner",
        ),
    ] {
        assert!(
            workflow.contains(&old),
            "fixture lacks the {label} mutation target"
        );
        let changed = workflow.replace(&old, &new);
        let error = check_bun_test_job(&changed).expect_err(label);
        assert!(error.contains("bun-test"), "{label}: {error}");
    }
}

#[test]
fn bun_lane_rejects_matrix_exclusions_and_includes() {
    let workflow = valid_workflow();
    let lane = "  bun-test:\n    runs-on: ${{ matrix.os }}\n    if: needs.changes.outputs.ts == 'true'\n    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-latest, macos-latest, windows-latest]\n";
    assert!(workflow.contains(lane), "fixture lacks the Bun matrix");
    for (extra, label) in [
        (
            "        exclude:\n          - os: windows-latest\n",
            "exclude Windows",
        ),
        (
            "        exclude:\n          - os: macos-latest\n",
            "exclude macOS",
        ),
        (
            "        exclude:\n          - os: ubuntu-latest\n",
            "exclude Ubuntu",
        ),
        (
            "        include:\n          - os: windows-latest\n",
            "matrix include",
        ),
        (
            "        os: [ubuntu-latest, macos-latest, windows-latest]\n",
            "duplicate os key",
        ),
    ] {
        let changed = workflow.replacen(lane, &format!("{lane}{extra}"), 1);
        let error = check_bun_test_job(&changed).expect_err(label);
        assert!(error.contains("bun-test"), "{label}: {error}");
    }
}

#[test]
fn bun_lane_requires_the_actual_job_level_routing_condition() {
    let workflow = valid_workflow();
    let route = "    if: needs.changes.outputs.ts == 'true'\n    strategy:";
    let step = "      - name: bun test\n";
    assert!(workflow.contains(route), "fixture lacks the Bun route");
    assert!(workflow.contains(step), "fixture lacks the Bun test step");

    // A step-level condition cannot substitute for the job-level router gate.
    for (job_if, label) in [
        (
            "    if: needs.changes.outputs.rust == 'true'\n",
            "wrong job route",
        ),
        ("", "missing job route"),
    ] {
        let changed = workflow
            .replacen(route, &format!("{job_if}    strategy:"), 1)
            .replacen(
                step,
                "      - name: bun test\n        if: needs.changes.outputs.ts == 'true'\n",
                1,
            );
        let error = check_bun_test_job(&changed).expect_err(label);
        assert!(error.contains("bun-test"), "{label}: {error}");
    }

    // Duplicate mapping keys are ambiguous: never trust a matching first key.
    let duplicated = workflow.replacen(
        route,
        "    if: needs.changes.outputs.ts == 'true'\n    if: needs.changes.outputs.rust == 'true'\n    strategy:",
        1,
    );
    let error = check_bun_test_job(&duplicated).expect_err("duplicate job route");
    assert!(error.contains("bun-test"), "{error}");
}

#[test]
fn bun_ci_accepts_requested_144_and_rejects_other_versions() {
    let requested = valid_workflow();
    assert!(requested.contains("bun-version: \"1.4.4\""));
    check_bun_test_job(&requested).expect("the requested stable CI pin must pass");
    for version in ["1.4.0", "1.4.2", "latest", "1.4.4-canary.1", "1.4.40"] {
        let changed = requested.replace("\"1.4.4\"", &format!("\"{version}\""));
        assert!(
            check_bun_test_job(&changed).is_err(),
            "unexpected runtime {version} must not satisfy the CI pin"
        );
    }
}

#[test]
fn native_js_formatting_cannot_be_removed_or_skipped() {
    let step = concat!(
        "      - name: Native JavaScript formatting\n        run: |\n",
        "          bun packages/@keld/kipc/scripts/echo-codegen.ts check\n",
        "          bun fmt --no-script --check \"**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}\"\n",
    );
    for replacement in [
        String::new(),
        step.replace("bun fmt", "echo bun fmt"),
        step.replace("        run:", "        if: false\n        run:"),
        step.replace(
            "        run:",
            "        continue-on-error: true\n        run:",
        ),
        step.replace(
            "          bun packages/@keld/kipc/scripts/echo-codegen.ts check\n",
            "",
        ),
        step.lines()
            .filter(|line| !line.contains("bun fmt"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    ] {
        let temp = complete_fixture();
        let workflow = valid_workflow();
        assert!(workflow.contains(step));
        temp.write(WORKFLOW, &workflow.replace(step, &replacement));
        assert!(
            check(temp.path()).is_err(),
            "missing/skipped formatter passed"
        );
    }
}

#[test]
fn native_js_formatting_allows_explanatory_comments() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replace(
            "        run: |\n          bun packages/@keld/kipc/scripts/echo-codegen.ts check\n",
            "        run: |\n          # Freshness before format.\n          bun packages/@keld/kipc/scripts/echo-codegen.ts check\n",
        ),
    );
    check(temp.path()).expect("comments do not disable a native check");
}

#[test]
fn local_native_js_formatting_cannot_leave_the_ci_inventory() {
    for recipe in ["ci", "ci-full"] {
        let temp = complete_fixture();
        let justfile = read(temp.path(), JUSTFILE).expect("justfile");
        let header = justfile
            .lines()
            .find(|line| line.starts_with(&format!("{recipe}:")))
            .expect("CI header");
        let altered = justfile.replacen(header, &header.replace(" js-fmt-check", ""), 1);
        assert_ne!(altered, justfile);
        temp.write(JUSTFILE, &altered);
        assert!(check(temp.path()).is_err());
    }
}

#[test]
fn bun_step_pins_accept_named_actions_and_ignore_job_strategy() {
    let workflow = valid_workflow()
            .replace("    strategy:\n      matrix:\n        os: ${{ fromJSON(needs.changes.outputs.check_os) }}\n", "    strategy:\n      fail-fast: false\n      matrix:\n        os: ${{ fromJSON(needs.changes.outputs.check_os) }}\n")
            .replace("      - uses: oven-sh/setup-bun@", "      - name: install Bun\n        uses: oven-sh/setup-bun@");
    let temp = complete_fixture();
    temp.write(WORKFLOW, &workflow);
    check(temp.path()).expect("only direct steps own action inputs");
}

#[test]
fn bun_pin_is_bound_to_the_action_input_not_job_text() {
    let correct = "          bun-version: \"1.4.4\"\n";
    for replacement in [
        "          bun-version: latest\n",
        "          # missing bun-version\n",
        "        env:\n          bun-version: \"1.4.4\"\n",
    ] {
        let workflow = valid_workflow().replace(correct, replacement).replace(
            "      - name: bun test\n",
            "      - run: |\n          echo 'bun-version: \"1.4.4\"'\n      - name: bun test\n",
        );
        let temp = complete_fixture();
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("unrelated text cannot pin an action");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("1.4.4"), "{error}");
    }
}

#[test]
fn every_bun_setup_in_each_bun_consumer_requires_its_own_pin() {
    for job in ["check", "bun-test", "hygiene"] {
        let workflow = valid_workflow();
        let block = workflow_job_block(&workflow, job).expect("fixture job");
        for extra in [
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: latest\n",
            "      - name: another Bun setup\n        uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n",
            "      - name: folded Bun action\n        'uses': >-\n          oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: latest\n",
            "      - uses: \"oven-sh/setup-b\\u0075n@0c5077e51419868618aeaa5fe8019c62421857d6\"\n        with:\n          bun-version: latest\n",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: '1.4.4'\n          \"bun-\\u0076ersion\": latest\n",
            "      - ? uses\n        : oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: latest\n",
        ] {
            let temp = complete_fixture();
            temp.write(
                WORKFLOW,
                &workflow.replace(&block, &format!("{block}{extra}")),
            );
            let error = check(temp.path()).expect_err("every actual setup must be pinned");
            assert!(error.contains(job), "{error}");
            assert!(error.contains("1.4.4"), "{error}");
        }
    }
}

#[test]
fn every_bun_consumer_requires_a_setup_action() {
    for job in ["check", "bun-test", "hygiene"] {
        let workflow = valid_workflow();
        let block = workflow_job_block(&workflow, job).expect("fixture job");
        let changed = block.replace("oven-sh/setup-bun@", "some-other/action@");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &workflow.replace(&block, &changed));
        let error = check(temp.path()).expect_err("a version on another action is not Bun");
        assert!(error.contains(job), "{error}");
    }
}

#[test]
fn floating_bun_version_in_bun_lane_fails() {
    let temp = complete_fixture();
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen("bun-version: \"1.4.4\"", "bun-version: latest", 1),
    );
    let error = check(temp.path()).expect_err("an unpinned Bun runtime must fail");
    assert!(error.contains("bun-test"), "{error}");
    assert!(error.contains("1.4.4"), "{error}");
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
fn fixture_deny_job_is_the_live_ci_job() {
    let job = live_deny_job();
    assert!(job.starts_with("  deny:\n"), "{job}");
    assert!(job.contains("\n      - name: cargo-deny check\n"), "{job}");
    assert!(
        !job.contains("\n  secrets:"),
        "the job must stop at the next job: {job}"
    );
    assert!(valid_workflow().contains(&format!("{job}\n  secrets:\n")));
}

#[test]
fn echoed_mermaid_gate_does_not_satisfy_workflow_contract() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "        run: tools/mermaid_render_check.sh",
        "        run: echo tools/mermaid_render_check.sh",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("echoed gate must not satisfy hygiene");
    assert!(error.contains("renderer directly"), "{error}");
}

#[test]
fn inline_comment_mermaid_gate_does_not_satisfy_workflow_contract() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "        run: tools/mermaid_render_check.sh",
        "        run: true # tools/mermaid_render_check.sh",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("commented gate must not satisfy hygiene");
    assert!(error.contains("renderer directly"), "{error}");
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
fn missing_updater_helper_codeowners_path_fails() {
    let temp = complete_fixture();
    temp.write(
        CODEOWNERS,
        &valid_codeowners().replace("/crates/keld-updater-helper/ @alice\n", ""),
    );
    let error = check(temp.path()).expect_err("missing updater helper path must fail");
    assert!(error.contains("crates/keld-updater-helper"), "{error}");
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
fn missing_llms_docs_check_fails() {
    let temp = complete_fixture();
    let workflow = valid_workflow()
        .replace("--test tools/llms_docs.rs", "")
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
fn missing_product_status_check_fails() {
    let temp = complete_fixture();
    let workflow = valid_workflow()
        .replace("--test tools/product_status.rs", "")
        .replace("product-status check .", "");
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("workflow without product-status check must fail");
    assert!(
        error.contains("tools/product_status.rs") || error.contains("product-status check"),
        "{error}"
    );
}

#[test]
fn conditional_product_status_step_fails() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "      - name: Product status contract\n        run: |",
        "      - name: Product status contract\n        if: ${{ false }}\n        run: |",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("conditional product status must fail");
    assert!(error.contains("Product status contract"), "{error}");
}

#[test]
fn echoed_product_status_command_fails() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "          target/product-status/product-status check .",
        "          echo 'target/product-status/product-status check .'",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("echoed product status must fail");
    assert!(error.contains("Product status contract"), "{error}");
}

#[test]
fn missing_product_status_windows_step_fails() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
            "      - name: Product status Windows path contracts\n        if: matrix.os == 'windows-latest'\n        shell: bash\n        run: |\n          mkdir -p target/product-status\n          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test\n          target/product-status/product-status-test\n",
            "",
        );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("missing Windows status step must fail");
    assert!(error.contains("Windows path contracts"), "{error}");
}

#[test]
fn product_status_windows_step_requires_bash() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "        if: matrix.os == 'windows-latest'\n        shell: bash\n        run: |",
        "        if: matrix.os == 'windows-latest'\n        run: |",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("the POSIX command block requires Bash");
    assert!(error.contains("shell: bash"), "{error}");
}

#[test]
fn duplicate_product_status_windows_step_fails() {
    let temp = complete_fixture();
    let step = "      - name: Product status Windows path contracts\n        if: matrix.os == 'windows-latest'\n        shell: bash\n        run: |\n          mkdir -p target/product-status\n          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test\n          target/product-status/product-status-test\n";
    let workflow = valid_workflow().replace(step, &format!("{step}{step}"));
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("duplicate Windows status steps must fail");
    assert!(error.contains("exactly one"), "{error}");
}

#[test]
fn broadened_product_status_windows_condition_fails() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "if: matrix.os == 'windows-latest'",
        "if: matrix.os == 'windows-latest' || true",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("broadened Windows condition must fail");
    assert!(error.contains("exact condition"), "{error}");
}

#[test]
fn windows_media_acceptance_step_is_unique_and_direct() {
    let temp = complete_fixture();
    let windows_media_step = windows_media_step();
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(&windows_media_step, "", 1),
    );
    let error = check(temp.path()).expect_err("missing Windows media step must fail");
    assert!(error.contains("KEL-132"), "{error}");

    let duplicated = format!("{windows_media_step}\n{windows_media_step}");
    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(&windows_media_step, &duplicated, 1),
    );
    let error = check(temp.path()).expect_err("duplicate Windows media step must fail");
    assert!(error.contains("exactly one"), "{error}");

    temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "        shell: pwsh\n        run: |\n          cargo clippy -p keld-wv",
                "        continue-on-error: true\n        shell: pwsh\n        run: |\n          cargo clippy -p keld-wv",
                1,
            ),
        );
    let error = check(temp.path()).expect_err("failure suppression must fail");
    assert!(
        error.contains("KEL-132") || error.contains("continue-on-error"),
        "{error}"
    );
}

#[test]
fn release_updater_helper_step_is_unique_and_direct() {
    let step = release_helper_step();
    let temp = complete_fixture();
    temp.write(WORKFLOW, &valid_workflow().replacen(&step, "", 1));
    let error = check(temp.path()).expect_err("missing release helper step must fail");
    assert!(error.contains("KEL-53 T4d S9c"), "{error}");

    temp.write(
        WORKFLOW,
        &valid_workflow().replacen(&step, &format!("{step}\n{step}"), 1),
    );
    let error = check(temp.path()).expect_err("duplicate release helper step must fail");
    assert!(error.contains("exactly one"), "{error}");
}

#[test]
fn release_updater_helper_step_cannot_be_narrowed_or_made_inert() {
    for (needle, replacement) in [
        (
            "contains(needs.changes.outputs.packages, 'keld-updater-helper') || ",
            "",
        ),
        ("        shell: bash\n        env:", "        env:"),
        (
            "          KELD_PACK_REAL_HOST: ${{ github.workspace }}/target/release/keld-updater-helper.exe\n",
            "",
        ),
        ("cargo build --release -p keld-updater-helper", "true"),
        ("--test release_image -- --ignored", "--test release_image"),
        ("ok\\. 2 passed;", "ok\\."),
        (
            "          set -euo pipefail\n",
            "          set -euo pipefail\n          exit 0\n",
        ),
        // The AC2 launch of the embedded release helper cannot be dropped or redirected.
        (
            "          KELD_PACK_EMBEDDED_HELPER: ${{ runner.temp }}/ac2-helper/keld-updater-helper.exe\n",
            "",
        ),
        (
            "KELD_PACK_REAL_HELPER: ${{ github.workspace }}/target/release/keld-updater-helper.exe",
            "KELD_PACK_REAL_HELPER: ${{ github.workspace }}/target/release/keld-host.exe",
        ),
        (
            "-- --ignored --exact launch::ac2_embedded_release_helper_launches_to_its_self_anchor_refusal",
            "-- --ignored --exact launch::ac2_embedded_release_host_launches_to_the_identity_refusal",
        ),
        ("          mkdir -p \"$RUNNER_TEMP/ac2-helper\"\n", ""),
    ] {
        let workflow = valid_workflow();
        assert!(workflow.contains(needle), "fixture holds {needle}");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &workflow.replacen(needle, replacement, 1));
        let error = check(temp.path()).expect_err("weakened release helper step must fail");
        assert!(error.contains("KEL-53 T4d S9c"), "{needle}: {error}");
    }
}

#[test]
fn parallel_windows_media_job_preserves_required_admission() {
    let base = valid_workflow();
    for (needle, replacement) in [
        (
            "  windows-media:\n    runs-on: windows-latest\n",
            "  windows-media:\n    runs-on: ubuntu-latest\n",
        ),
        (
            "  windows-media:\n    runs-on: windows-latest\n    needs: changes\n",
            "  windows-media:\n    runs-on: windows-latest\n",
        ),
        (
            "  windows-media:\n    runs-on: windows-latest\n",
            "  windows-media:\n    continue-on-error: true\n    runs-on: windows-latest\n",
        ),
    ] {
        assert!(base.contains(needle));
        let temp = complete_fixture();
        temp.write(WORKFLOW, &base.replacen(needle, replacement, 1));
        let error = check(temp.path()).expect_err("media job must reject weakened scope");
        assert!(
            error.contains("KEL-132") || error.contains("continue-on-error"),
            "{error}"
        );
    }
    for (needle, replacement) in [
        ("      - windows-media\n", ""),
        (
            "KELD_RESULT_MEDIA: ${{ needs['windows-media'].result }}",
            "KELD_RESULT_MEDIA: success",
        ),
        (
            "KELD_ROUTE_MEDIA: ${{ needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv') }}",
            "KELD_ROUTE_MEDIA: false",
        ),
        (
            "\"$KELD_RESULT_MEDIA\" \"$KELD_ROUTE_MEDIA\"",
            "\"$KELD_RESULT_MEDIA\" false",
        ),
    ] {
        assert!(base.contains(needle), "fixture holds {needle}");
        let error = check_required_job(&base.replacen(needle, replacement, 1))
            .expect_err("media gate must reject dropped/spoofed status or applicability");
        assert!(
            error.contains("windows-media")
                || error.contains("MEDIA")
                || error.contains("35-argument"),
            "{needle}: {error}"
        );
    }
}

#[test]
fn windows_media_acceptance_condition_and_shell_are_exact() {
    for (needle, replacement) in [
        (
            "needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv')",
            "needs.changes.outputs.rust == 'true'",
        ),
        ("        shell: pwsh", "        shell: bash"),
    ] {
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(needle, replacement, 1));
        let error = check(temp.path()).expect_err("weakened Windows media routing must fail");
        assert!(error.contains("KEL-132"), "{needle}: {error}");
    }
}

#[test]
fn windows_media_acceptance_commands_cannot_be_removed_or_made_inert() {
    for (needle, replacement) in [
        ("--features media-acceptance", "--features default"),
        (
            "& $powerShellHost -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File crates/keld-wv/tests/windows_media_guard.ps1",
            "Write-Output crates/keld-wv/tests/windows_media_guard.ps1",
        ),
        (
            "& $powerShellHost -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File crates/keld-wv/tests/windows_media_guard.ps1",
            "& crates/keld-wv/tests/windows_media_guard.ps1",
        ),
        (
            "if ($LASTEXITCODE -ne 0) { throw 'media-acceptance Clippy failed' }",
            "Write-Output 'ignored Clippy status'",
        ),
        ("$fixture.Count -ne 1", "$fixture.Count -lt 0"),
        (
            "Test-Path -LiteralPath $resultPath -PathType Leaf",
            "Test-Path -LiteralPath $resultPath -PathType Container",
        ),
        (
            "Test-Path -LiteralPath $evidenceRoot",
            "Test-Path -LiteralPath $resultPath",
        ),
        (
            "@($result.rows).Count -ne 10",
            "@($result.rows).Count -lt 0",
        ),
        ("$result.rows -isnot [array]", "$false"),
        (
            "$result.source_executable -cne $fixturePath",
            "$result.source_executable -cne $result.source_executable",
        ),
        (
            "$result.executable_sha256 -cne $fixtureHash",
            "$result.executable_sha256 -cne $result.executable_sha256",
        ),
        (
            "$result.watchdog_probe.exit_code -ne 124",
            "$result.watchdog_probe.exit_code -ne $result.watchdog_probe.exit_code",
        ),
        ("$result.watchdog_probe.exit_code -isnot [int]", "$false"),
        (
            "$result.outer_deadline_probe.outer_timed_out -ne $true",
            "$result.outer_deadline_probe.outer_timed_out -eq $true",
        ),
        (
            "$result.outer_deadline_probe.exit_code -gt [int]::MaxValue",
            "$false",
        ),
        (
            "Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath",
            "Get-FileHash -Algorithm SHA256 -LiteralPath $evidenceExecutable",
        ),
        (
            "$watchdogStderr.Contains('KELD_MEDIA_TIMEOUT: fixture exceeded 0.1 seconds')",
            "$watchdogStderr.Contains('anything')",
        ),
        (
            "microphone/removed-guard/app-grants",
            "camera/removed-guard/app-grants",
        ),
        (
            "$_.profile_removed -ne $true",
            "$_.profile_removed -eq $true",
        ),
        ("Sort-Object -Unique -CaseSensitive", "Sort-Object -Unique"),
        ("Compare-Object -CaseSensitive", "Compare-Object"),
        (
            "$result.schema -cne 'keld.windows-media-fixture/v1'",
            "$result.schema -ne 'keld.windows-media-fixture/v1'",
        ),
        (
            "$result.capture_device -cne 'WebView2 synthetic development device'",
            "$result.capture_device -cne $result.capture_device",
        ),
        (
            "$result.system -cne [Environment]::OSVersion.VersionString",
            "$result.system -cne $result.system",
        ),
        (
            "$result.device -cne [Environment]::MachineName",
            "$result.device -cne $result.device",
        ),
        (
            "$_.receipt -cnotlike 'KELD_MEDIA_RESULT * case_ok=true'",
            "$_.receipt -notlike 'KELD_MEDIA_RESULT * case_ok=true'",
        ),
        (
            "$row.stderr_sha256 -cnotmatch $hashPattern",
            "$row.stderr_sha256 -cmatch $hashPattern",
        ),
        ("$row.$field -isnot [int]", "$false"),
        ("$row.profile_removed -isnot [bool]", "$false"),
        (
            "\"$($logReceipts[0])\" -cne $row.receipt",
            "\"$($logReceipts[0])\" -cne \"$($logReceipts[0])\"",
        ),
        (
            "Test-Path -LiteralPath $row.profile_path",
            "Test-Path -LiteralPath $resultPath",
        ),
        (
            "$profileReceipts.Count -ne 1",
            "$profileReceipts.Count -lt 0",
        ),
        ("(?<nonce>[1-9][0-9]*)", "(?<nonce>[0-9]+)"),
        ("(?<adapter_tid>0|[1-9][0-9]*)", "(?<adapter_tid>[0-9]+)"),
        (
            "$originPort -gt 65535",
            "$originPort -gt [uint32]::MaxValue",
        ),
        (
            "$fixtureTempRoot.StartsWith($sharedProfileRoot, [StringComparison]::OrdinalIgnoreCase)",
            "$false",
        ),
        (
            "$row.receipt -cmatch $receiptPattern",
            "$row.receipt -cnotmatch $receiptPattern",
        ),
        (
            "[int]$receiptFields.permission_kind -ne $row.permission_kind",
            "[int]$receiptFields.permission_kind -ne [int]$receiptFields.permission_kind",
        ),
        ("$seenNonces.ContainsKey($receiptFields.nonce)", "$false"),
        (
            "$row.permission_kind -ne $expectedPermissionKind",
            "$row.permission_kind -ne $row.permission_kind",
        ),
        (
            "$row.outcome -cnotmatch $expectedOutcome",
            "$row.outcome -cnotmatch $row.outcome",
        ),
    ] {
        let temp = complete_fixture();
        temp.write(WORKFLOW, &valid_workflow().replacen(needle, replacement, 1));
        let error = check(temp.path()).expect_err("inert Windows media command must fail");
        assert!(error.contains("KEL-132"), "{needle}: {error}");
    }
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
        error.contains("renderer directly")
            || error.contains("pinned render step")
            || error.contains(MERMAID_IMAGE_DIGEST),
        "{error}"
    );
}

#[test]
fn unknown_mermaid_applicability_must_fail_closed_locally() {
    let temp = complete_fixture();
    let justfile = read(temp.path(), JUSTFILE).expect("just fixture");
    temp.write(
        JUSTFILE,
        &justfile.replace("        *) exit 1 ;;", "        *) echo skipped ;;"),
    );
    let error = check(temp.path()).expect_err("unknown Mermaid route cannot become green");
    assert!(error.contains("unknown applicability"), "{error}");
}

#[test]
fn commented_mermaid_render_workflow_does_not_pass() {
    let temp = complete_fixture();
    let workflow = valid_workflow().replace(
        "        run: tools/mermaid_render_check.sh",
        "        run: echo tools/mermaid_render_check.sh",
    );
    temp.write(WORKFLOW, &workflow);
    let error = check(temp.path()).expect_err("commented render step must fail");
    assert!(error.contains("renderer directly"), "{error}");
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
#[cfg(unix)]
fn empty_mermaid_manifest_skips_docker_and_render_output() {
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    let temp = TempDir::new();
    let checkout = temp.path().join("checkout with spaces");
    fs::create_dir_all(checkout.join("tools")).expect("fixture checkout");
    init_fixture_repository(&checkout);
    fs::write(
        checkout.join("README.md"),
        "The word Mermaid is not a diagram.\n",
    )
    .expect("write prose-only Markdown");
    fs::write(
        checkout.join("tools/mermaid_docs.rs"),
        include_str!("mermaid_docs.rs"),
    )
    .expect("copy the canonical block parser");
    assert!(
        Command::new("git")
            .args(["add", "README.md", "tools/mermaid_docs.rs"])
            .current_dir(&checkout)
            .status()
            .expect("track fixture inputs")
            .success()
    );
    temp.write("renderer.sh", include_str!("mermaid_render_check.sh"));
    let docker_marker = temp.path().join("docker-was-called");
    let docker = temp.path().join("bin/docker");
    fs::create_dir_all(docker.parent().expect("docker fake parent"))
        .expect("create fake docker directory");
    fs::write(
        &docker,
        "#!/usr/bin/env bash\nprintf called >\"$KELD_TEST_DOCKER_MARKER\"\nexit 91\n",
    )
    .expect("write fail-fast Docker oracle");
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o700))
        .expect("make Docker oracle executable");
    let mut paths = vec![temp.path().join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").expect("tool PATH")));
    let output = Command::new("bash")
        .arg(temp.path().join("renderer.sh"))
        .current_dir(&checkout)
        .env("PATH", env::join_paths(paths).expect("fixture PATH"))
        .env("KELD_TEST_DOCKER_MARKER", &docker_marker)
        .output()
        .expect("execute renderer with empty manifest");
    assert!(
        output.status.success(),
        "empty manifest should be a successful no-op: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("no Mermaid blocks"),
        "missing explicit no-op evidence: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!docker_marker.exists(), "zero candidates queried Docker");
    assert!(
        !checkout.join("target/keld-mermaid-render").exists(),
        "zero candidates created render output"
    );
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_rejects_target_symlink_before_creating_output() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::process::Command;

    let temp = TempDir::new();
    let checkout = temp.path().join("checkout with spaces");
    let external = temp.path().join("external");
    fs::create_dir(&checkout).expect("checkout directory");
    fs::create_dir(&external).expect("external directory");
    symlink(&external, checkout.join("target")).expect("escaping target symlink");
    init_fixture_repository(&checkout);
    fs::write(
            checkout.join("diagram.md"),
            "```mermaid\nflowchart LR\naccTitle: Renderer test\naccDescr: A fixture diagram for renderer cleanup tests.\nA --> B\n```\n",
        )
        .expect("tracked diagram");
    assert!(
        Command::new("git")
            .args(["add", "diagram.md"])
            .current_dir(&checkout)
            .status()
            .expect("track diagram")
            .success()
    );
    fs::create_dir(checkout.join("tools")).expect("fixture tools");
    fs::write(checkout.join(MERMAID_CONFIG), "{}").expect("config fixture");
    temp.write("renderer.sh", include_str!("mermaid_render_check.sh"));
    temp.write("bin/docker", "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image) exit 0 ;;\ncontext) printf 'unix:///unused-kel188.sock\\n' ;;\n*) exit 42 ;;\nesac\n");
    let docker = temp.path().join("bin/docker");
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o700)).expect("driver executable");
    let mut paths = vec![temp.path().join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").expect("tool PATH")));
    let output = Command::new("bash")
        .arg(temp.path().join("renderer.sh"))
        .current_dir(&checkout)
        .env("PATH", env::join_paths(paths).expect("fixture PATH"))
        .output()
        .expect("execute real renderer script");
    assert!(!output.status.success());
    assert!(
        fs::read_dir(&external)
            .expect("external observation")
            .next()
            .is_none(),
        "renderer created output outside its checkout"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("non-symlink directory"),
        "wrong failure boundary: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[cfg(windows)]
fn mermaid_native_dacl_does_not_depend_on_path_whoami() {
    use std::process::Command;

    let temp = TempDir::new();
    let parent = temp.path().join("target");
    let output_dir = parent.join("keld-mermaid-render.fixture");
    fs::create_dir_all(output_dir.join("child")).expect("retained directory fixture");
    fs::write(output_dir.join("child/diagnostic.txt"), "diagnostic")
        .expect("retained file fixture");
    // Exercise the owning script with real Windows ACL APIs. A colliding GNU-shaped
    // executable must not supply identity to the native restoration process.
    let renderer = include_str!("mermaid_render_check.sh");
    let start = renderer.find("running_under_msys() {").expect("MSYS owner");
    let end = renderer
        .find("\nrun_with_timeout 8 docker info")
        .expect("helper boundary");
    let script = format!(
        "set -euo pipefail\n{}\nrender_parent=$(cd \"$1\" && pwd -P)\nwhoami.exe() {{ echo 'GNU whoami rejects Windows arguments' >&2; return 64; }}\nrestore_docker_output_dir \"$render_parent/keld-mermaid-render.fixture\"\n",
        &renderer[start..end]
    );
    temp.write("native-dacl-restore.sh", &script);
    let output = Command::new("bash")
        .arg(temp.path().join("native-dacl-restore.sh"))
        .arg(&parent)
        .output()
        .expect("execute native restoration");
    assert!(
        output.status.success(),
        "native restoration failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle = r#"
$ErrorActionPreference = 'Stop'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$root = Get-Item -LiteralPath $env:KELD_TEST_OUTPUT
$items = @($root) + @(Get-ChildItem -LiteralPath $root.FullName -Recurse -Force)
if ($items.Count -ne 3) { throw 'incomplete native census' }
foreach ($item in $items) {
    $acl = Get-Acl -LiteralPath $item.FullName
    $rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    if (-not $acl.AreAccessRulesProtected -or $rules.Count -ne 1) { throw 'DACL not protected single-owner' }
    $rule = $rules[0]
    $inheritance = if ($item.PSIsContainer) { 3 } else { 0 }
    if ($rule.IdentityReference -ne $sid -or $rule.IsInherited -or
        $rule.AccessControlType -ne 'Allow' -or $rule.FileSystemRights -ne 'FullControl' -or
        [int]$rule.InheritanceFlags -ne $inheritance -or [int]$rule.PropagationFlags -ne 0) {
        throw 'native ACE differs from owner-only contract'
    }
}
'native DACL census: 3/3'
"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", oracle])
        .env("KELD_TEST_OUTPUT", &output_dir)
        .output()
        .expect("read native DACL inventory");
    assert!(
        output.status.success(),
        "native DACL oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("native DACL census: 3/3"));
}

#[cfg(unix)]
fn write_msys_renderer_fixture(
    temp: &TempDir,
    docker: &str,
    chmod: Option<&str>,
    remove: Option<&str>,
    native_dacl: Option<&str>,
) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    let checkout = temp.path().join("checkout");
    fs::create_dir(&checkout).expect("checkout directory");
    init_fixture_repository(&checkout);
    fs::write(
            checkout.join("diagram.md"),
            "```mermaid\nflowchart LR\naccTitle: Renderer test\naccDescr: A fixture diagram for renderer cleanup tests.\nA --> B\n```\n",
        )
        .expect("tracked diagram");
    assert!(
        Command::new("git")
            .args(["add", "diagram.md"])
            .current_dir(&checkout)
            .status()
            .expect("track diagram")
            .success()
    );
    fs::create_dir(checkout.join("tools")).expect("fixture tools");
    fs::write(
        checkout.join("tools/mermaid_docs.rs"),
        include_str!("mermaid_docs.rs"),
    )
    .expect("copy canonical Mermaid parser");
    fs::write(checkout.join(MERMAID_CONFIG), "{}\n").expect("renderer config");
    temp.write("renderer.sh", include_str!("mermaid_render_check.sh"));
    temp.write("bin/docker", docker);
    temp.write(
        "bin/uname",
        "#!/usr/bin/env bash\nprintf 'MSYS_NT-10.0\\n'\n",
    );
    temp.write(
            "bin/cygpath",
            "#!/usr/bin/env bash\n[[ \"$1\" == '-am' || \"$1\" == '-aw' ]] || exit 64\nprintf '%s\\n' \"$2\"\n",
        );
    temp.write(
        "bin/whoami.exe",
        "#!/usr/bin/env bash\necho 'GNU whoami rejects Windows arguments' >&2\nexit 64\n",
    );
    temp.write(
        "bin/powershell.exe",
        native_dacl.unwrap_or("#!/usr/bin/env bash\nexit 0\n"),
    );
    temp.write(
            "bin/chmod",
            chmod.unwrap_or(
                "#!/usr/bin/env bash\nmode=$1\nshift\n[[ \"${1:-}\" == -- ]] && shift\nexec /bin/chmod \"$mode\" \"$@\"\n",
            ),
        );
    if let Some(remove) = remove {
        temp.write("bin/rm", remove);
    }
    for shim in [
        "docker",
        "uname",
        "cygpath",
        "whoami.exe",
        "powershell.exe",
        "chmod",
    ] {
        let path = temp.path().join("bin").join(shim);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("shim executable");
    }
    for shim in ["rm"] {
        let path = temp.path().join("bin").join(shim);
        if path.exists() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("shim executable");
        }
    }
    checkout
}

#[cfg(unix)]
fn run_msys_renderer_fixture(temp: &TempDir, checkout: &Path) -> std::process::Output {
    use std::process::Command;

    let mut paths = vec![temp.path().join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").expect("tool PATH")));
    Command::new("bash")
        .arg(temp.path().join("renderer.sh"))
        .current_dir(checkout)
        .env_remove("MSYSTEM")
        .env("PATH", env::join_paths(paths).expect("fixture PATH"))
        .output()
        .expect("execute real renderer script")
}

#[cfg(unix)]
fn retained_render_dirs(checkout: &Path) -> Vec<PathBuf> {
    fs::read_dir(checkout.join("target"))
        .expect("checkout-local output directory")
        .map(|entry| entry.expect("retained output entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("keld-mermaid-render."))
        })
        .collect()
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_bounds_stalled_owned_container_cleanup() {
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    // Specific custody for this regression's real renderer child. Closing
    // its owned stdin releases only the fake cleanup's builtin read; the
    // failure backstop never signals a Docker daemon or a cached PID.
    struct CleanupChild(Option<Child>);
    impl Drop for CleanupChild {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                drop(child.stdin.take());
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    let temp = TempDir::new();
    let checkout = write_msys_renderer_fixture(
        &temp,
        r#"#!/usr/bin/env bash
case "$1" in
info|image) exit 0 ;;
context) printf 'unix:///unused-gh705.sock\n' ;;
run) exit 42 ;;
rm)
  printf '%s\n' "$@" >"$KELD_TEST_DOCKER_RM_RECORD"
  IFS= read -r release || :
  printf released >"$KELD_TEST_DOCKER_RM_RELEASED"
  exit 0
  ;;
*) exit 99 ;;
esac
"#,
        None,
        None,
        None,
    );
    let removal_record = temp.path().join("cleanup-request");
    let fallback_record = temp.path().join("cleanup-released");
    let stdout_path = temp.path().join("renderer.stdout");
    let stderr_path = temp.path().join("renderer.stderr");
    let stdout = fs::File::create(&stdout_path).expect("owned renderer stdout");
    let stderr = fs::File::create(&stderr_path).expect("owned renderer stderr");
    let mut paths = vec![temp.path().join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").expect("tool PATH")));
    let child = Command::new("bash")
        .arg(temp.path().join("renderer.sh"))
        .current_dir(&checkout)
        .env_remove("MSYSTEM")
        .env("PATH", env::join_paths(paths).expect("fixture PATH"))
        .env("KELD_TEST_DOCKER_RM_RECORD", &removal_record)
        .env("KELD_TEST_DOCKER_RM_RELEASED", &fallback_record)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn actual renderer under fake Docker");
    let renderer_pid = child.id();
    let mut custody = CleanupChild(Some(child));
    let mut deadline = Instant::now() + Duration::from_secs(45);
    let mut cleanup_entered = false;
    let mut intervened = false;
    let status = loop {
        let child = custody.0.as_mut().expect("owned unreaped renderer");
        if let Some(status) = child.try_wait().expect("observe renderer exit") {
            break status;
        }
        if !cleanup_entered && removal_record.exists() {
            cleanup_entered = true;
            // Arm the completion backstop after the real cleanup entry;
            // compiling the canonical parser is not cleanup-time evidence.
            deadline = Instant::now() + Duration::from_secs(15);
        }
        if Instant::now() >= deadline {
            if intervened {
                let _ = child.kill();
                break child.wait().expect("reap backstop-stopped renderer");
            }
            // Record failure before fallback. EOF lets the original
            // unbounded fake cleanup return, so the baseline cannot hang.
            intervened = true;
            drop(child.stdin.take());
            deadline = Instant::now() + Duration::from_secs(5);
        }
        // Poll actual entry/exit conditions; no delay establishes order.
        std::thread::yield_now();
    };
    drop(custody.0.take()); // The exact child was reaped before capture/cleanup.
    let stdout = fs::read_to_string(&stdout_path).expect("renderer stdout capture");
    let stderr = fs::read_to_string(&stderr_path).expect("renderer stderr capture");
    assert!(
        cleanup_entered,
        "cleanup was not reached: {stdout}\n{stderr}"
    );
    assert_eq!(
        fs::read_to_string(&removal_record).expect("actual fake cleanup argv"),
        format!("rm\n--force\nkeld-mermaid-{renderer_pid}-1\n"),
        "cleanup selected a different container or a broad operation"
    );
    assert!(
        !intervened,
        "owned Docker cleanup exceeded its completion backstop; original status={status:?}\n{stdout}\n{stderr}"
    );
    assert_eq!(
        status.code(),
        Some(1),
        "cleanup replaced the render failure"
    );
    assert!(
        !fallback_record.exists(),
        "fake cleanup needed an EOF/release rather than its owning deadline"
    );
    assert!(stderr.contains("container cleanup incomplete"), "{stderr}");
    assert!(stderr.contains("exit 124"), "{stderr}");
    assert!(
        stderr.contains(&format!("keld-mermaid-{renderer_pid}-1")),
        "cleanup diagnostic lost its exact owned name: {stderr}"
    );
    assert!(stderr.contains("absence is unverified"), "{stderr}");
    assert!(stderr.contains("pinned Mermaid render failed"), "{stderr}");
    assert!(stderr.contains("failed-render output retained"), "{stderr}");
    let retained = retained_render_dirs(&checkout);
    assert_eq!(retained.len(), 1, "lost failed-render evidence");
    assert_eq!(
        fs::metadata(&retained[0])
            .expect("retained failure mode")
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "bounded cleanup changed the existing owner-only restore"
    );
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_preserves_render_failure_when_container_removal_fails() {
    let temp = TempDir::new();
    let checkout = write_msys_renderer_fixture(
        &temp,
        "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image) exit 0 ;;\ncontext) printf 'unix:///unused-gh705.sock\\n' ;;\nrun) exit 42 ;;\nrm) exit 62 ;;\n*) exit 99 ;;\nesac\n",
        None,
        None,
        None,
    );
    let output = run_msys_renderer_fixture(&temp, &checkout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "cleanup masked primary status"
    );
    assert!(stderr.contains("container cleanup incomplete"), "{stderr}");
    assert!(stderr.contains("exit 62"), "{stderr}");
    assert!(stderr.contains("absence is unverified"), "{stderr}");
    assert!(stderr.contains("failed-render output retained"), "{stderr}");
    assert_eq!(retained_render_dirs(&checkout).len(), 1);
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_retains_msys_failure_output_with_owner_only_mode() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = TempDir::new();
    let checkout = write_msys_renderer_fixture(
        &temp,
        "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image|rm) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrun) exit 42 ;;\n*) exit 99 ;;\nesac\n",
        None,
        None,
        None,
    );
    let output = run_msys_renderer_fixture(&temp, &checkout);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed-render output retained"),
        "missing retained-output receipt: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("with owner-only host access"),
        "native restoration must not depend on PATH whoami: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let retained = retained_render_dirs(&checkout);
    assert_eq!(retained.len(), 1, "retained output: {retained:?}");
    assert!(
        retained[0]
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("keld-mermaid-render.")),
        "wrong retained output path: {:?}",
        retained[0]
    );
    assert_eq!(
        fs::metadata(&retained[0])
            .expect("retained output metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_does_not_claim_owner_only_when_restoration_fails() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = TempDir::new();
    let checkout = write_msys_renderer_fixture(
        &temp,
        "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image|rm) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrun) exit 42 ;;\n*) exit 99 ;;\nesac\n",
        Some(
            "#!/usr/bin/env bash\nmode=$1\nshift\n[[ \"${1:-}\" == -- ]] && shift\nif [[ \"$mode\" == 0700 ]]; then exit 44; fi\nexec /bin/chmod \"$mode\" \"$@\"\n",
        ),
        None,
        None,
    );
    let output = run_msys_renderer_fixture(&temp, &checkout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("owner-only restoration failed"), "{stderr}");
    assert!(!stderr.contains("with owner-only host access"), "{stderr}");
    let retained = retained_render_dirs(&checkout);
    assert_eq!(retained.len(), 1, "retained output: {retained:?}");
    assert_eq!(
        fs::metadata(&retained[0])
            .expect("retained output metadata")
            .permissions()
            .mode()
            & 0o777,
        0o777
    );
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_does_not_claim_owner_only_when_native_dacl_restoration_fails() {
    let temp = TempDir::new();
    let checkout = write_msys_renderer_fixture(
        &temp,
        "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image|rm) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrun) exit 42 ;;\n*) exit 99 ;;\nesac\n",
        None,
        None,
        Some("#!/usr/bin/env bash\nexit 45\n"),
    );
    let output = run_msys_renderer_fixture(&temp, &checkout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("owner-only restoration failed"), "{stderr}");
    assert!(!stderr.contains("with owner-only host access"), "{stderr}");
}

#[test]
#[cfg(unix)]
fn mermaid_renderer_restores_output_when_success_cleanup_cannot_remove_it() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = TempDir::new();
    let checkout = write_msys_renderer_fixture(
        &temp,
        "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrm) exit 0 ;;\nrun)\n  out=''\n  shift\n  while [[ $# -gt 0 ]]; do\n    if [[ \"$1\" == --volume ]]; then\n      case \"$2\" in *:/out) out=\"${2%:/out}\" ;; esac\n      shift 2\n    else\n      shift\n    fi\n  done\n  [[ -n \"$out\" ]] || exit 64\n  printf '<svg><title>fixture</title><desc>fixture</desc></svg>' >\"$out/fixture.svg\"\n  ;;\n*) exit 99 ;;\nesac\n",
        None,
        Some("#!/usr/bin/env bash\nexit 66\n"),
        None,
    );
    let output = run_msys_renderer_fixture(&temp, &checkout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("successful-render output could not be removed; retained"),
        "{stderr}"
    );
    let retained = retained_render_dirs(&checkout);
    assert_eq!(retained.len(), 1, "retained output: {retained:?}");
    assert_eq!(
        fs::metadata(&retained[0])
            .expect("retained output metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn mermaid_output_outside_the_shared_checkout_fails() {
    let temp = complete_fixture();
    temp.write(
        MERMAID_RENDERER,
        &read(temp.path(), MERMAID_RENDERER)
            .expect("renderer fixture")
            .replace(
                "$render_parent/keld-mermaid-render.",
                "/tmp/keld-mermaid-render.",
            ),
    );
    let error = check(temp.path()).expect_err("output must share the checkout mount boundary");
    assert!(error.contains("render_dir"), "{error}");
}
#[test]
fn missing_mermaid_msys_path_exclusion_fails() {
    let temp = complete_fixture();
    temp.write(
        MERMAID_RENDERER,
        &read(temp.path(), MERMAID_RENDERER)
            .expect("renderer fixture")
            .replace("MSYS2_ARG_CONV_EXCL='*'", ""),
    );
    let error = check(temp.path()).expect_err("MSYS path rewriting must stay disabled");
    assert!(error.contains("MSYS2_ARG_CONV_EXCL"), "{error}");
}

#[test]
fn missing_mermaid_msys_shell_detection_fails() {
    let temp = complete_fixture();
    temp.write(
        MERMAID_RENDERER,
        &read(temp.path(), MERMAID_RENDERER)
            .expect("renderer fixture")
            .replace("running_under_msys", ""),
    );
    let error = check(temp.path()).expect_err("Git Bash detection must remain explicit");
    assert!(error.contains("running_under_msys"), "{error}");
}

#[test]
fn missing_mermaid_msys_output_permission_fix_fails() {
    let temp = complete_fixture();
    temp.write(
        MERMAID_RENDERER,
        &read(temp.path(), MERMAID_RENDERER)
            .expect("renderer fixture")
            .replace("prepare_docker_output_dir \"$render_dir\"", ""),
    );
    let error = check(temp.path()).expect_err("MSYS output bind must remain writable");
    assert!(error.contains("prepare_docker_output_dir"), "{error}");
}

#[test]
fn missing_mermaid_msys_permission_restoration_fails() {
    let temp = complete_fixture();
    temp.write(
        MERMAID_RENDERER,
        &read(temp.path(), MERMAID_RENDERER)
            .expect("renderer fixture")
            .replace("if restore_docker_output_dir \"$render_dir\"; then", ""),
    );
    let error = check(temp.path()).expect_err("retained output must return to owner-only access");
    assert!(error.contains("restore_docker_output_dir"), "{error}");
}

#[test]
fn mermaid_permission_restoration_must_stay_bounded_and_preserve_status() {
    for (old, replacement) in [
        ("if ! running_under_msys; then", "if false; then"),
        ("\"$render_parent\"/keld-mermaid-render.*) ;;", "*) ;;"),
        ("restore_windows_owner_only_dacl \"$path\"", "true"),
        ("MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'", "true"),
        ("$acl.SetAccessRuleProtection($true, $false)", "true"),
        (
            "chmod 0700 -- \"$path\" || {",
            "chmod 0777 -- \"$path\" || {",
        ),
        ("local cleanup_status=$?", "local cleanup_status=0"),
        ("trap - EXIT", "true"),
        ("exit \"$cleanup_status\"", "exit 0"),
    ] {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace(old, replacement),
        );
        let error = check(temp.path())
            .expect_err("retained-output restoration must remain bounded and failure-safe");
        assert!(error.contains("CI-HYGIENE"), "{old}: {error}");
    }
}

#[test]
fn inert_or_reordered_mermaid_msys_shell_text_fails() {
    for (old, replacement) in [
        ("running_under_msys() {", "echo 'running_under_msys() {'"),
        (
            "prepare_docker_output_dir \"$render_dir\"",
            "echo 'prepare_docker_output_dir \"$render_dir\"'",
        ),
        (
            "export MSYS2_ARG_CONV_EXCL='*'",
            "echo \"export MSYS2_ARG_CONV_EXCL='*'\"",
        ),
        (
            "prepare_docker_output_dir \"$render_dir\"\ndocker_render_dir=$(docker_host_path \"$render_dir\")",
            "docker_render_dir=$(docker_host_path \"$render_dir\")\nprepare_docker_output_dir \"$render_dir\"",
        ),
    ] {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace(old, replacement),
        );
        check(temp.path()).expect_err("inert or reordered MSYS shell text must fail");
    }
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
    let workflow = valid_workflow().replace("--test tools/ci_hygiene.rs", "tools/ci_hygiene.rs");
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

fn assert_gitleaks_scan_refused(from: &str, to: &str, what: &str) {
    let workflow = valid_workflow();
    assert!(workflow.contains(from), "fixture lacks `{from}`");
    let temp = complete_fixture();
    temp.write(WORKFLOW, &workflow.replacen(from, to, 1));
    let error = check(temp.path()).expect_err(what);
    assert!(error.contains("gitleaks `Scan`"), "{what}: {error}");
}

#[test]
fn unscoped_gitleaks_scan_fails() {
    assert_gitleaks_scan_refused(
        " --log-opts=\"$KELD_GITLEAKS_RANGE\"",
        "",
        "an all-refs gitleaks scan must fail",
    );
}

#[test]
fn trailing_comment_cannot_spoof_the_gitleaks_scope() {
    assert_gitleaks_scan_refused(
        " --log-opts=\"$KELD_GITLEAKS_RANGE\"",
        " # --log-opts=\"$KELD_GITLEAKS_RANGE\"",
        "a commented-out scope must fail",
    );
}

#[test]
fn unresolved_pull_request_sha_guard_is_required() {
    assert_gitleaks_scan_refused(
        "if [ -z \"$sha\" ] || ! git cat-file -e \"${sha}^{commit}\"; then",
        "if [ -z \"$sha\" ]; then",
        "a range whose SHAs are not resolved must fail",
    );
}

#[test]
fn empty_gitleaks_range_fails() {
    assert_gitleaks_scan_refused(
        "KELD_GITLEAKS_RANGE=\"$KELD_GITLEAKS_PR_BASE..$KELD_GITLEAKS_PR_HEAD\"",
        "KELD_GITLEAKS_RANGE=\"$KELD_GITLEAKS_PR_HEAD..$KELD_GITLEAKS_PR_HEAD\"",
        "an empty pull-request range must fail",
    );
}

#[test]
fn unknown_event_must_refuse_a_gitleaks_range() {
    assert_gitleaks_scan_refused(
        "refusing to choose a scan range\" >&2\n          exit 1",
        "refusing to choose a scan range\" >&2\n          KELD_GITLEAKS_RANGE=HEAD",
        "an unknown event that falls back to a scan must fail",
    );
}

#[test]
fn gitleaks_scan_refusals_exit_nonzero() {
    let next = |needle: &str| {
        GITLEAKS_SCAN_COMMANDS
            .iter()
            .position(|command| command.contains(needle))
            .and_then(|index| GITLEAKS_SCAN_COMMANDS.get(index + 1).copied())
    };
    assert_eq!(next("refusing an unresolvable range"), Some("exit 1"));
    assert_eq!(next("refusing to choose a scan range"), Some("exit 1"));
}

#[test]
fn gitleaks_range_inputs_come_from_the_event() {
    assert_gitleaks_scan_refused(
        "KELD_GITLEAKS_PR_BASE: ${{ github.event.pull_request.base.sha }}",
        "KELD_GITLEAKS_PR_BASE: ${{ github.event.pull_request.head.sha }}",
        "a spoofed base SHA must fail",
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
fn channel_allocations_step_must_bind_the_comparison_base() {
    let workflow = valid_workflow();
    let env_line = "          KELD_CI_BASE_REF: ${{ github.event.pull_request.base.sha || github.event.before }}\n";
    assert!(workflow.contains(env_line), "fixture binds the base");
    check_channel_allocations_step(&workflow).expect("fixture step passes");
    let mutations = [
            ("removed env", workflow.replacen(&format!("        env:\n{env_line}"), "", 1), "only its `if`, `env` and `run`"),
            (
                "PR base only",
                workflow.replacen(" || github.event.before", "", 1),
                "never with itself",
            ),
            (
                "default base",
                workflow.replacen("${{ github.event.pull_request.base.sha || github.event.before }}", "origin/main", 1),
                "never with itself",
            ),
            (
                "always-run step",
                workflow.replacen(
                    "      - name: Check this checkout\n        if: needs.changes.outputs.hygiene == 'true'",
                    "      - name: Check this checkout\n        if: always()",
                    1,
                ),
                "exactly when the router selects hygiene",
            ),
            (
                "suppressed exit",
                workflow.replacen("          target/ci-hygiene/ci-hygiene check .", "          target/ci-hygiene/ci-hygiene check . || true", 1),
                "without wrappers",
            ),
            ("missing step", workflow.replacen("      - name: Check this checkout\n", "      - name: Check another checkout\n", 1), "exactly one"),
        ];
    for (label, mutated, needle) in mutations {
        assert_ne!(mutated, workflow, "{label}: mutation applied");
        let error = check_channel_allocations_step(&mutated).expect_err(label);
        assert!(error.contains(needle), "{label}: {error}");
    }
}

#[test]
fn channel_allocations_prefix_rule_admits_only_appends() {
    let base = "echo 1\nfs 2\nlifecycle 3\n";
    check_allocations_prefix(Some(base), Some(base), "b").expect("unchanged passes");
    check_allocations_prefix(
        Some(base),
        Some("echo 1\nfs 2\nlifecycle 3\nprobe 4\n"),
        "b",
    )
    .expect("an append passes");
    check_allocations_prefix(None, Some(base), "b").expect("introducing the file passes");
    check_allocations_prefix(None, None, "b").expect("no file anywhere passes");
    // Negative controls (spec criterion 3a): a both-sides renumber the
    // in-crate table check cannot see, a deleted line, a reorder, removal.
    for (current, line) in [
        ("echo 1\nfs 4\nlifecycle 3\n", 2),
        ("echo 1\nlifecycle 3\n", 2),
        ("fs 2\necho 1\nlifecycle 3\n", 1),
        ("echo 1\nfs 2\n", 3),
    ] {
        let error = check_allocations_prefix(Some(base), Some(current), "b")
            .expect_err("only appends are admitted");
        assert!(error.contains(&format!("line {line} ")), "{error}");
        assert!(
            error.contains("append-only") || error.contains("append new"),
            "{error}"
        );
    }
    let removed = check_allocations_prefix(Some(base), None, "b").expect_err("removal fails");
    assert!(removed.contains("was removed"), "{removed}");
}

fn git_fixture(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .expect("run git fixture command");
    assert!(status.success(), "git {args:?}");
}

/// `git init` for a throwaway fixture with automatic maintenance off, so no
/// detached `git maintenance` repack outlives a command and races cleanup (#670).
fn init_fixture_repository(root: &Path) {
    git_fixture(root, &["init", "--quiet"]);
    git_fixture(root, &["config", "maintenance.auto", "false"]);
}

#[test]
fn channel_allocations_rule_reads_the_merge_base_and_fails_closed() {
    let temp = TempDir::new();
    let root = temp.path();
    init_fixture_repository(root);
    temp.write("README.md", "fixture\n");
    git_fixture(root, &["add", "README.md"]);
    git_fixture(root, &["commit", "--quiet", "-m", "before the baseline"]);
    git_fixture(root, &["branch", "before"]);
    // The PR that introduces the file passes against a base without it.
    temp.write(CHANNEL_ALLOCATIONS, "echo 1\nfs 2\nlifecycle 3\n");
    check_channel_allocations_append_only(root, "before").expect("introduction passes");

    git_fixture(root, &["add", CHANNEL_ALLOCATIONS]);
    git_fixture(root, &["commit", "--quiet", "-m", "baseline"]);
    git_fixture(root, &["branch", "base"]);
    git_fixture(root, &["checkout", "--quiet", "-b", "feature"]);
    temp.write(CHANNEL_ALLOCATIONS, "echo 1\nfs 2\nlifecycle 3\nprobe 4\n");
    check_channel_allocations_append_only(root, "base").expect("an append passes");

    temp.write(CHANNEL_ALLOCATIONS, "echo 1\nfs 4\nlifecycle 3\n");
    let renumbered = check_channel_allocations_append_only(root, "base")
        .expect_err("a renumber in the baseline fails against the merge base");
    assert!(renumbered.contains("line 2 `fs 2`"), "{renumbered}");

    // A base that moved on after the fork still compares at the fork point.
    git_fixture(root, &["checkout", "--quiet", "base"]);
    temp.write("README.md", "main moved\n");
    git_fixture(root, &["commit", "--quiet", "-am", "main moved"]);
    git_fixture(root, &["checkout", "--quiet", "feature"]);
    temp.write(CHANNEL_ALLOCATIONS, "echo 1\nlifecycle 3\n");
    let deleted =
        check_channel_allocations_append_only(root, "base").expect_err("a deleted line fails");
    assert!(deleted.contains("line 2 `fs 2`"), "{deleted}");

    let unresolved = check_channel_allocations_append_only(root, "no-such-base")
        .expect_err("an unresolvable base fails closed");
    assert!(unresolved.contains("cannot resolve"), "{unresolved}");
    assert!(unresolved.contains("never passes"), "{unresolved}");
    for absent in ["", "0000000000000000000000000000000000000000"] {
        let error = check_channel_allocations_append_only(root, absent)
            .expect_err("an empty or first-push base fails closed");
        assert!(error.contains("all-zero"), "{error}");
    }
}

#[test]
fn deleting_codeowners_file_fails() {
    let temp = complete_fixture();
    fs::remove_file(temp.path().join(CODEOWNERS)).expect("remove CODEOWNERS");
    let error = check(temp.path()).expect_err("missing CODEOWNERS must fail");
    assert!(error.contains("CODEOWNERS"), "{error}");
    assert!(error.contains("Restore"), "{error}");
}

#[test]
fn job_level_keys_after_steps_are_found_and_nested_if_is_ignored() {
    let block = "  job:\n    runs-on: ubuntu-latest\n    steps:\n      - name: one\n        if: ${{ nested }}\n        run: echo\n    if: ${{ always() }}\n    continue-on-error: true\n";
    assert_eq!(
        workflow_job_level_if(block).as_deref(),
        Some("${{ always() }}")
    );
    assert_eq!(
        workflow_job_level_property(block, "continue-on-error").as_deref(),
        Some("true")
    );
    let nested_only = "  job:\n    steps:\n      - name: one\n        if: ${{ nested }}\n";
    assert_eq!(workflow_job_level_if(nested_only), None);
}
