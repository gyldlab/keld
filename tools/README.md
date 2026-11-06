# Tools directory

Run repository tasks from the root with [`just`](../justfile); `just --list` shows the available recipes. The [`ci.yml`](../.github/workflows/ci.yml) workflow is the source for hosted-only commands. Files below include entrypoints, private modules imported by those entrypoints, test fixtures, and optional maintainer utilities. A file appearing here does not mean it runs in every CI job.

Use [`CONTRIBUTING.md`](../CONTRIBUTING.md) for contributor steps, [`docs/agents/workflow.md`](../docs/agents/workflow.md) for maintainer lifecycle, and [`.agents/index.md`](../.agents/index.md) for routed checks. The tables identify implementation ownership and invocation; they do not replace those contracts.

## CI routing, formatting, and repository hygiene

| Files | Responsibility and verified entrypoint |
| --- | --- |
| [`ci_local.py`](ci_local.py), [`test_ci_local.py`](test_ci_local.py) | Executes the Just inventory with local changed-input selection; `just ci` or `just ci-full` (`ci-router-test` exercises selection). |
| [`ci_changes.sh`](ci_changes.sh), [`ci_changes_test.sh`](ci_changes_test.sh) | Shared local/GitHub changed-path lane router and its independent fixtures; `just ci-router-test` or the hosted router job. |
| [`ci_inputs.py`](ci_inputs.py), [`ci-inputs.json`](ci-inputs.json) | Reviewed reader scopes, membership, and content fingerprints for routing. `just ci-inputs-rebind` updates digests only after reviewing changed reader inputs; normal consumers check for drift. |
| [`ci_required.sh`](ci_required.sh) | Hosted required-job admission and its self-test; invoked from `ci.yml`. |
| [`ci_webkitgtk_apt.sh`](ci_webkitgtk_apt.sh) | Hosted Ubuntu WebKitGTK apt/deb-cache preparation with signed-package verification; invoked from `ci.yml`. |
| [`dependency_review_metadata.sh`](dependency_review_metadata.sh) | Hosted dependency-review metadata completeness check; invoked from `ci.yml`. |
| [`ci_workflow_security.ts`](ci_workflow_security.ts), [`ci_workflow_security.test.ts`](ci_workflow_security.test.ts) | Bun contracts for GitHub workflow security; `just hygiene` runs them. The existing KIPC no-emit compiler checks these sources through `just typescript`. |
| [`ci_hygiene.rs`](ci_hygiene.rs), [`ci_hygiene_tests.rs`](ci_hygiene_tests.rs), [`ci_hygiene_workflow_access.rs`](ci_hygiene_workflow_access.rs) | Repository/CI-policy validator with private test and workflow-reading modules; `just hygiene`. |
| [`fmt_included.sh`](fmt_included.sh) | Rust formatting coverage for Git-tracked standalone tools, included files, and macro-declared modules missed by normal Cargo discovery; `just fmt` / `just fmt-check`. |
| [`test_hello_command.py`](test_hello_command.py) | Tests the platform-appropriate native hello recipe; `just hygiene`. |

## Instruction, path, and test-layout contracts

| Files | Responsibility and verified entrypoint |
| --- | --- |
| [`agents_md.sh`](agents_md.sh) | Agent routing and nearest-crate unsafe-instruction coverage; `just agents-md`. |
| [`atomic_protocol.rs`](atomic_protocol.rs) | Checks the documented atomic-reasoning stages and ownership references; `just atomic-protocol`. |
| [`agent_context.rs`](agent_context.rs), [`markdown_contract.rs`](markdown_contract.rs), [`repo_path_contract.rs`](repo_path_contract.rs) | Instruction routing and budget validator with shared Markdown and repository-path parsers; `just agent-context` / `just agent-context-test`. The path helper is also shared with standalone documentation checkers. |
| [`session_protocol.rs`](session_protocol.rs) | Private session-continuity/closeout policy checks compiled into `atomic_protocol.rs` by `just atomic-protocol`; receipt validation remains with the Python closeout tool below. |
| [`justfile_contract.rs`](justfile_contract.rs) | Private Just recipe-inventory parser reused by standalone repository checkers; not a separate CLI. |
| [`test_import_guard.py`](test_import_guard.py), [`test_test_import_guard.py`](test_test_import_guard.py) | Incremental wildcard-import checker for owner-local Rust test modules and its negative controls; invoked directly by `ci.yml`. |

## Documentation checks and generated views

| Files | Responsibility and verified entrypoint |
| --- | --- |
| [`llms_docs.rs`](llms_docs.rs) | Generates/checks the checked-in `llms.txt` and `llms-full.txt` documentation corpus; `just llms`, `just llms-check`, `just llms-test`. |
| [`product_status.rs`](product_status.rs) | Checks the authoritative `docs/engineering/product-status.tsv` ledger and renders its Markdown view; `just product-status`, `just product-status-check`, `just product-status-test`. |
| [`doc_placeholders.rs`](doc_placeholders.rs) | Finds unresolved placeholders in checked-in prose; `just doc-placeholders-check` / `just doc-placeholders-test`. |
| [`mermaid_docs.rs`](mermaid_docs.rs) | Structural/accessibility checks for Mermaid documentation; `just mermaid-check` / `just mermaid-test`. |
| [`mermaid_render_check.sh`](mermaid_render_check.sh), [`mermaid-render-config.json`](mermaid-render-config.json) | Digest-pinned isolated Mermaid renderer and its configuration; `just mermaid-render-check` (and `just mermaid-full`). |

## Local task workspaces, caches, hooks, and closeout

| Files | Responsibility and verified entrypoint |
| --- | --- |
| [`research_checkout.sh`](research_checkout.sh), [`test_research_checkout.sh`](test_research_checkout.sh) | Maintainer nested-research Git sync/push under the existing `reference-run` lock. `just research-sync`, `just research-push`, `just research-test`; tests use only disposable local bare remotes. |
| [`workspace.py`](workspace.py), [`test_workspace.py`](test_workspace.py) | Managed Git worktrees and session scratch; `just work-start`, `work-status`, `work-run`, `work-finish`, `work-clean`, `work-check`, and `work-test`. This is cooperative task coordination, not a filesystem sandbox. |
| [`worktree_cache.rs`](worktree_cache.rs), [`worktree_cache_delete.rs`](worktree_cache_delete.rs), [`worktree_cache_delete_tests.rs`](worktree_cache_delete_tests.rs) | Git-registered Cargo cache inventory and guarded opt-in deletion, with included native tests; `just cache-check`, `just cache-delete`, `just cache-check-test`. |
| [`cache_tool_provenance_test.sh`](cache_tool_provenance_test.sh) | Ensures cache commands use newly compiled, verified source rather than a retained binary; `just cache-check-test`. |
| [`hooks_install.sh`](hooks_install.sh), [`hooks_test.sh`](hooks_test.sh) | Installs reviewed local checkout hooks and checks their incoming-revision isolation; `just hooks-install` / `just hooks-test`. |
| [`session_closeout.py`](session_closeout.py), [`session_closeout_hook.py`](session_closeout_hook.py) | Read-only baseline/closeout receipt validator and hash-bound native-client hook adapters; `just session-closeout <receipt>` validates a supplied receipt, without authenticating remote claims. |
| [`test_session_closeout.py`](test_session_closeout.py), [`test_session_closeout_hook.py`](test_session_closeout_hook.py) | Real Git/filesystem and hook-admission negative controls; included in `just agent-context-test`. |

## Source inventories, review evidence, and operator utilities

| Files | Responsibility and verified entrypoint |
| --- | --- |
| [`source_census.py`](source_census.py), [`test_source_census.py`](test_source_census.py) | Reproducible category/size inventory of pinned **Git blobs** and its negative controls; invoked directly for source audits (tests also run in `ci.yml`). |
| [`source_budget_report.py`](source_budget_report.py), [`test_source_budget_report.py`](test_source_budget_report.py) | Report-only comparison of hypothetical size budgets using the census classifier; the hosted report/test does **not** enforce file-size limits. |
| [`windows_architecture_ledger.py`](windows_architecture_ledger.py), [`test_windows_architecture_ledger.py`](test_windows_architecture_ledger.py) | Deterministic Windows-related source review queue and classifier tests; queue membership is not a security signoff. |
| [`retained_fs_artifact.py`](retained_fs_artifact.py), [`retained_fs_artifact_test.py`](retained_fs_artifact_test.py) | Evidence-binding validator for the retained-filesystem milestone and its exact-field tests; an external trusted caller must authenticate provenance. |
| [`windows_signed_installed_acceptance.ps1`](windows_signed_installed_acceptance.ps1), [`windows_signed_installed_acceptance_test.ps1`](windows_signed_installed_acceptance_test.ps1) | Native Windows operator verification for an *existing signed installed release* and its negative controls; neither script builds or signs a release. |
| [`competitors_sync.rs`](competitors_sync.rs) | Shallow sync of opt-in gitignored reference trees from `competitors.lock.toml`; `just competitors-sync` is **local maintainer-only** and not a CI gate. |

For direct scripts with required arguments, follow their source and the verified invocation in `ci.yml` or the Just recipe. Generated-output and routing-fingerprint changes must go through their owning generator/review path; this map does not authorize manual edits to generated artifacts or digest records.
