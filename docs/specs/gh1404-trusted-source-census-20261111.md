# GH-1404: #1472 immutable file census and candidate trusted review stages

**Status: REVIEW EVIDENCE ONLY — DO NOT MERGE OR TREAT THESE AS INTEGRATED FEATURE COMMITS.**

This file contains an exact filename census of draft [#1472](https://github.com/gyldlab/keld/pull/1472) and a **candidate inventory allocation** for independent staged review. It does not implement a protected-file edit, a CI waiver, a historical-secret decision, or an accepted signed application. The separate docs-only design draft is [#1473](https://github.com/gyldlab/keld/pull/1473) at `e0c0c8db863098f63aa6f6d4e6f80bd2a0b083b9`.

## Immutable source and coverage receipts

- Repository: `gyldlab/keld`; target base: `f49bc07e16ffb64c7060bc47b05ad755bd2ae4b9`; #1472 feature head: `aac5060602aceb63f521bcaf9164d2a6b6337979`, confirmed with GitHub PR metadata at census time (2026-11-11).
- GitHub paginated `list_pr_changed_filenames`: **451 unique paths**; 451 matching `fetch_pr_patch` file records. Of those, **374 contain patch hunks and 77 have no patch returned**. Do not infer the omitted patch content.
- GitHub unified `get_pr_diff` returned **HTTP 406**, explicitly refusing more than 300 changed files. This is a full **path census**, not a complete readable unified patch. Before approving any stage, reviewers must obtain the actual immutable stage diff, including the paths with missing source hunks.
- Root `.github/CODEOWNERS` at this base has 18 owner patterns, matching **40 distinct changed paths** in this source. The separate default-main `.github/workflows/ci-provenance.yml` CI-authority rule matches the **16 identified inputs** in [#1473's proposed trust plan](gh1404-ci-trust-integration-plan.md): **5 overlap** CODEOWNERS and **11 are CI-authority-only**, giving **51 unique paths** needing at least one of the two authority reviews.
- This file is an audit of the source candidate; the 451 rows below are **never evidence of a passing CI gate**. Source changes to any protected input must be authorized and executed by its designated owner, and the resulting stage must be re-enumerated against the *actual* immutable main base/head after every predecessor lands.

## Candidate staging and dependency gates

| Candidate gate | Exact source-path allocation | CODEOWNERS matches | CI authority inputs | Missing GitHub patches | Required predecessor and disposition |
| --- | ---: | ---: | ---: | ---: | --- |
| G0 — trusted CI authority | 4 | 4 | 4 | 0 | **Owner decision only.** Workflow and tools changes are escrowed. Do not place them in an ordinary feature PR or modify the verifier to self-approve. Trusted-base maintainers choose a separately reviewed revocation-aware route. |
| F1 — Guard/KIPC foundation | 64 | 34 | 2 | 7 | After G0 trust decision and independent Guard/IPC security/wire review. Contains Guard/IPC Rust, `@keld/kipc`, central error registry and its generated corpus; preserve source/consumer/test closure. |
| F2 — runtime/Core/WebView/native | 147 | 2 | 4 | 23 | After an admitted F1. Contains shared Cargo graph and owned Rust runtime/Core/WebView/native code; require Rust OS-specific/unsafe, permission and dependency evidence. |
| F3 — Host/Pack/CLI/API installed producer | 123 | 0 | 3 | 47 | After F1+F2 contracts. Contains Host, packager, CLI, `@keld/api`, original-entry and signed-graph producer. Requires qualified app/provider pins and negative installation controls. |
| F4 — Electron/update/compat consumers | 113 | 0 | 3 | 0 | After F3 published API/facts semantics. Contains `@keld/electron`, updater, compat data and related docs; require consumer compatibility tests. |
| **Total** | **451** | **40** | **16** | **77** | **All staging units remain proposed and do not satisfy merge gates merely by staying below 300 files.** |

**Dependency map:** Guard → IPC → runtime/Core → Host; Guard → Pack → CLI; `@keld/kipc` → `@keld/api` → `@keld/electron` and updater consumers. Work through actual producers and downstream callers before admitting each stage. These groups partition inventory exactly and all have fewer than 300 files; they **have not** been built or demonstrated independently deployable. Where one group requires an API/export, generated contract, test or docs from another group, split or move the complete boundary unit into the earlier independently passing PR and recheck every count. Protect the default main verifier from self-approval. Adjacent repair PRs [#1475](https://github.com/gyldlab/keld/pull/1475), [#1477](https://github.com/gyldlab/keld/pull/1477), and [#1480](https://github.com/gyldlab/keld/pull/1480) remain separately owned; reconcile their exact final diffs rather than duplicating or overwriting them.

## Exact 16 CI-authority inputs

- `.github/workflows/ci.yml` — also CODEOWNERS
- `Cargo.toml` — also CODEOWNERS
- `crates/keld-cli/Cargo.toml`
- `crates/keld-core/Cargo.toml`
- `crates/keld-native/Cargo.toml`
- `crates/keld-pack/Cargo.toml`
- `crates/keld-wv/Cargo.toml`
- `packages/@keld/api/package.json`
- `packages/@keld/electron-updater/fixtures/package.json`
- `packages/@keld/electron-updater/package.json`
- `packages/@keld/electron/package.json`
- `packages/@keld/kipc/package.json`
- `packages/@keld/kipc/scripts/echo-codegen.ts`
- `tools/ci_workflow_security.test.ts` — also CODEOWNERS
- `tools/ci_workflow_security.ts` — also CODEOWNERS
- `tools/test_ci_local.py` — also CODEOWNERS

## Exact 40 changed CODEOWNERS-covered paths

- `.github/workflows/ci.yml` — also CI authority
- `Cargo.lock`
- `Cargo.toml` — also CI authority
- `crates/keld-guard/src/admit.rs`
- `crates/keld-guard/src/lib.rs`
- `crates/keld-guard/src/manifest_profile.rs`
- `crates/keld-guard/src/renderer_channels.rs`
- `crates/keld-guard/src/verified_manifest.rs`
- `crates/keld-guard/src/window_csp.rs`
- `crates/keld-guard/tests/declared_profile.rs`
- `crates/keld-guard/tests/native_system_grants.rs`
- `crates/keld-guard/tests/renderer_channels.rs`
- `crates/keld-guard/tests/window_csp_parser.rs`
- `crates/keld-guard/tests/window_csp_verified.rs`
- `crates/keld-ipc/channel_allocations.txt`
- `crates/keld-ipc/src/bootstrap.rs`
- `crates/keld-ipc/src/channel_table.rs`
- `crates/keld-ipc/src/compat_control.rs`
- `crates/keld-ipc/src/guard_dispatch.rs`
- `crates/keld-ipc/src/lib.rs`
- `crates/keld-ipc/src/lifecycle.rs`
- `crates/keld-ipc/src/link.rs`
- `crates/keld-ipc/src/receive.rs`
- `crates/keld-ipc/src/window.rs`
- `crates/keld-ipc/src/window/tests.rs`
- `crates/keld-ipc/src/window/tests/desktop.rs`
- `crates/keld-ipc/tests/dialog_channel.rs`
- `crates/keld-ipc/tests/dialog_guard.rs`
- `crates/keld-ipc/tests/fixtures/desktop-v0.tsv`
- `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`
- `crates/keld-ipc/tests/fixtures/window-v0.tsv`
- `crates/keld-ipc/tests/guard_allow_allocation.rs`
- `crates/keld-ipc/tests/receiver_corpus.rs`
- `crates/keld-ipc/tests/renderer_channel_dispatch.rs`
- `crates/keld-ipc/tests/support/receiver_corpus.rs`
- `crates/keld-ipc/tests/worker_link.rs`
- `docs/engineering/keld-error-codes.md`
- `tools/ci_workflow_security.test.ts` — also CI authority
- `tools/ci_workflow_security.ts` — also CI authority
- `tools/test_ci_local.py` — also CI authority

## Independent approval and product gates

1. **Private security incident owner:** Independently assess the historical GHSA-46g3-v38r-mw9x / ancestral Gitleaks classification and any necessary credential revocation/remediation privately under `SECURITY.md`. This document deliberately contains no credential value or incident finding and never waives scanner or changes history. A normal passing exact-head Gitleaks scan remains mandatory.
2. **Trusted-base maintainers:** Decide the implementation of revocation-aware, immutable base/head and complete-file-census approval for each of the 16 CI authority paths. Default-branch `ci-provenance.yml` currently denies both 300+ files and any protected CI authority change. A feature PR may not authenticate its own protected edits. Record the decision and evidence on the owning trust/security tracker, then re-run it against each stage's actual refs.
3. **Independent `@gyldlab/keld-maintainers` reviewers:** Review the 40 changed CODEOWNERS paths, their tests and source consumers in the exact final stages. Root Cargo, Guard and IPC need the repository-designated single writer. Check unsafe, public API, permission model, dependency addition and wire protocol gates with named independent evidence; a draft or CODEOWNERS request is not an approval.
4. **Windows regression-file owner:** Commit or explicitly transfer the strict migration of `packages/@keld/api/src/runtime-preload.test.ts`, preserving Init/ACK, both subscriptions, Ready, one authenticated link, error/refusal/tamper assertions and existing deadlines. Preserve the owner's other uncommitted files; passing a different test does not close this one.
5. **Original graph/source owners:** Publish immutable qualified Host/Bun/provider/app/policy inputs from #499/#502/#553/#452/#570. The [single-graph amendment](macos-original-single-graph-amendment.md) is **not approved** for changed `import.meta.main`, `Bun.main` and `process.argv[1]` semantics by a passing bootstrap fixture.
6. **Native signed-app owner and independent reviewer:** Qualify actual relocated signed `.app`/DMG, original pristine draw.io Open/Edit/Save XML readback, Cancel/Discard/Quit/reopen, lifecycle/descendant cleanup and signed/generated-source tamper refusals. Only actual native acceptance plus full current-head Rust/Bun/Gitleaks/provenance/VeriSentry/aggregate CI, review and author/merger gates can close product readiness.

## Exhaustive 451-path review allocation

This list is generated from one complete paginated PR-file census. “owner” means a CODEOWNERS-covered path, “CI” means an input governed by the stricter protected default-branch CI-authority rule, and “no patch” means GitHub's per-file API did not return unified hunk text. Classification is for preparation only, not permission to modify or merge.

| Changed path at #1472 source head | Candidate | CODEOWNERS | CI authority | Patch evidence |
| --- | --- | --- | --- | --- |
| `.github/workflows/ci.yml` | G0 | owner | CI | per-file hunk |
| `Cargo.lock` | F2 | owner | — | per-file hunk |
| `Cargo.toml` | F2 | owner | CI | per-file hunk |
| `crates/keld-cli/Cargo.toml` | F3 | — | CI | per-file hunk |
| `crates/keld-cli/src/boot.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/boot/permissions_tests.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/build.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/build/config.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/create.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/dev.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/doctor.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/lib.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/main.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/mcp/server.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/src/verb.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/tests/build_macos.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/tests/channel_table_scan.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/tests/cli_kel29.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/tests/doctor_mcp.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/tests/error_registry.rs` | F3 | — | — | per-file hunk |
| `crates/keld-cli/tests/profile_doctor.rs` | F3 | — | — | per-file hunk |
| `crates/keld-compat/fixtures/app-corpus/corpus.json` | F4 | — | — | per-file hunk |
| `crates/keld-compat/fixtures/app-corpus/denominator.json` | F4 | — | — | per-file hunk |
| `crates/keld-compat/fixtures/native-corpus/corpus.json` | F4 | — | — | per-file hunk |
| `crates/keld-compat/fixtures/native-corpus/denominator.json` | F4 | — | — | per-file hunk |
| `crates/keld-core/AGENTS.md` | F2 | — | — | per-file hunk |
| `crates/keld-core/Cargo.toml` | F2 | — | CI | per-file hunk |
| `crates/keld-core/src/app_session.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/auth_router_tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/boot_contract.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/boot_contract/installed_graph.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/compat_router.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/compat_router/availability_tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/compat_router/availability_tests/original_recovery.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_custody.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_custody_contract.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_session.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_session/host_route.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_session/native_ui.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_session/publication_tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/dialog_session/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/error.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/graph_contract.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/graph_contract/installed_tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/launch_facts.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/macos_installed.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/macos_profile_selection.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/menu_router.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/menu_router/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/original_webapp.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/profile_preflight.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/single_instance.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/single_instance/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/accepted_primary_identity.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/app_identity_and_shared_probes.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/boot_schema_and_preflight.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/failed_ready_and_cli_lease.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/filesystem_guard_and_recovery.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/filesystem_quiesce.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/generation_correlation_and_auth.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/gh499_graph.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/guarded_router_and_quit.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/profile_admission.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/ready_recovery.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/recovered_events_and_echo.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/role_shutdown_and_worker.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/router_lifecycle.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/successor_single_flight.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/terminal_cleanup_and_windows_shutdown.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/tests/worker_link_e2e.rs` | F2 | — | — | per-file hunk |
| `crates/keld-core/src/app_session/window_registry.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_registry/tests.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_registry/tests/original_document_destroy.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_registry/tests/original_load_context.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_registry/tests/original_recovery.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router/tests.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router/tests/close_failures.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router/tests/close_policy.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router/tests/desktop.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router/tests/original_document_destroy.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/app_session/window_router/tests/original_load.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/hello.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/hello/auth_config.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/hello/auth_config/literal.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/hello/auth_config/source_scan.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/hello/auth_config/tests.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/hello_session.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/lib.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/lifecycle.rs` | F2 | — | — | no patch |
| `crates/keld-core/src/macos_profile_identity.rs` | F2 | — | — | no patch |
| `crates/keld-core/tests/hello_profile.rs` | F2 | — | — | no patch |
| `crates/keld-guard/src/admit.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/src/lib.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/src/manifest_profile.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/src/renderer_channels.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/src/verified_manifest.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/src/window_csp.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/tests/declared_profile.rs` | F1 | owner | — | no patch |
| `crates/keld-guard/tests/native_system_grants.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-guard/tests/renderer_channels.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-guard/tests/window_csp_parser.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-guard/tests/window_csp_verified.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-host/src/main.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/facade_profile_recovery.ts` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos-installed-launch-facts/README.md` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos-installed-launch-facts/early.ts` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos-installed-launch-facts/main.ts` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos-installed-launch-facts/ready-witness.ts` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos-installed-launch-facts/transport-preload-observer.ts` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos_local_package_acceptance.py` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos_local_package_crash_witness.ts` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos_local_package_crashes.py` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos_local_package_identity_tests.py` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos_local_package_launch_facts.py` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/fixtures/macos_local_package_security.py` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/boot_admission.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/facade_ready.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/actual_auth_app.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/actual_auth_app/quit_diagnostics.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/facade_profile_recovery.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/facade_profile_recovery_fixture.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/policy_startup.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/profile_admission.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/project.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/public_fs/retirement.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/macos/renderer_bridge.rs` | F3 | — | — | per-file hunk |
| `crates/keld-host/tests/no_flag/windows/staging.rs` | F3 | — | — | per-file hunk |
| `crates/keld-ipc/channel_allocations.txt` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/bootstrap.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/channel_table.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/compat_control.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/guard_dispatch.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/lib.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/lifecycle.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/link.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/receive.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/window.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/window/tests.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/src/window/tests/desktop.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/dialog_channel.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/dialog_guard.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/fixtures/desktop-v0.tsv` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/fixtures/window-v0.tsv` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/guard_allow_allocation.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/receiver_corpus.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/renderer_channel_dispatch.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/support/receiver_corpus.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-ipc/tests/worker_link.rs` | F1 | owner | — | per-file hunk |
| `crates/keld-native/AGENTS.md` | F2 | — | — | per-file hunk |
| `crates/keld-native/Cargo.toml` | F2 | — | CI | per-file hunk |
| `crates/keld-native/src/dialog.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/dialog/file_filter.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/dialog/macos_file.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/dialog/macos_file/delegate.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/dialog/macos_file/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/dialog/macos_message.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/lib.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/menu.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/menu/macos.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/menu/macos/target.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/src/menu/macos/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/tests/dialog_file_filter.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/tests/dialog_wire.rs` | F2 | — | — | per-file hunk |
| `crates/keld-native/tests/menu_wire.rs` | F2 | — | — | per-file hunk |
| `crates/keld-pack/Cargo.toml` | F3 | — | CI | per-file hunk |
| `crates/keld-pack/src/lib.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos/assemble.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos/capture.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos/graph_tests.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos/tests.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos/tool_output.rs` | F3 | — | — | per-file hunk |
| `crates/keld-pack/src/macos_metadata.rs` | F3 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/bootstrap.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/generation_host.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/guarded_lease/tests/own_child_reap_callers.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/guarded_primary.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/process_group.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/supervision_runner.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/active_generation_shutdown.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/bootstrap_shutdown_race.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/generation_host.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/generation_host/clear_authority.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/guarded_primary_shutdown.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/guardian_crash_pre_registration.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/process_group_failsafe.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/process_group_failsafe/exiting_member_race.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/macos_guardian/tests/startup_selection.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/primary.rs` | F2 | — | — | per-file hunk |
| `crates/keld-runtime/src/role.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/AGENTS.md` | F2 | — | — | per-file hunk |
| `crates/keld-wv/Cargo.toml` | F2 | — | CI | per-file hunk |
| `crates/keld-wv/examples/macos_original_custody.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/fuzz/fuzz_targets/wv_link.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/engine.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/engine/application_name_for_user_agent_tests.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/error.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/lib.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/media.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/profile.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/profile/identity.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/application_name_fixture.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/application_name_fixture/observer.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_bridge.rs` | F2 | — | — | no patch |
| `crates/keld-wv/src/wkwebview/macos_bridge/boot_diagnostic.js` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_bridge/boot_diagnostic.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_bridge/boot_diagnostic_common.js` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_bridge/boot_diagnostic_page.js` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_document.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_document/recovery_fixture.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_navigation_owner.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_preload_runtime.generated.js` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_profile.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_resource_intake.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_resource_intake/native_pending_policy_contract.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_resource_native.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_resource_source.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/macos_termination.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/managed_windows.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/mod.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/preload_native_fixture.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/s1_resource_fixture.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/s1_resource_lifecycle.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wkwebview/s1_resource_lifecycle_observer.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/src/wv_link.rs` | F2 | — | — | per-file hunk |
| `crates/keld-wv/tests/fixtures/drawio-original-preload.json` | F2 | — | — | per-file hunk |
| `crates/keld-wv/tests/fixtures/generic-original-preload.json` | F2 | — | — | per-file hunk |
| `docs/architecture/03-security.md` | F1 | — | — | per-file hunk |
| `docs/architecture/04-electron-compat.md` | F4 | — | — | per-file hunk |
| `docs/architecture/06-runtime-and-tooling.md` | F2 | — | — | per-file hunk |
| `docs/engineering/keld-error-codes.md` | F1 | owner | — | per-file hunk |
| `docs/specs/gh1287-sdk-package-artifacts.md` | F3 | — | — | per-file hunk |
| `docs/specs/gh1404-existing-migration-reconciliation.md` | F2 | — | — | per-file hunk |
| `docs/specs/gh1404-original-recovery-startup.md` | F2 | — | — | per-file hunk |
| `docs/specs/gh553-empty-stylesheet-policy.md` | F3 | — | — | per-file hunk |
| `docs/specs/gh564-macos-sync-message-box.md` | F4 | — | — | per-file hunk |
| `docs/specs/kel78-explicit-legacy-profile.md` | F2 | — | — | per-file hunk |
| `docs/specs/kel78-strict-profile-sandbox.md` | F2 | — | — | per-file hunk |
| `docs/specs/macos-installed-graph-source.md` | F3 | — | — | per-file hunk |
| `docs/specs/macos-installed-original-runtime-facts.md` | F3 | — | — | per-file hunk |
| `docs/specs/macos-local-app-packaging.md` | F3 | — | — | per-file hunk |
| `docs/specs/macos-original-single-graph-amendment.md` | F3 | — | — | per-file hunk |
| `llms-full.txt` | F1 | — | — | per-file hunk |
| `packages/@keld/api/bun.lock` | F3 | — | — | per-file hunk |
| `packages/@keld/api/fixtures/package-lifecycle.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/fixtures/staged-api-bundle.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/package.json` | F3 | — | CI | per-file hunk |
| `packages/@keld/api/src/app.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/auth.generated.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/auth.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/auth.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/channels.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/channels.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/compat-source-link.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/compat-source-link.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/compat-source.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/compat-source.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/dialog-sync-consumer.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/dialog.generated.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/echo-call.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/echo-call.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/fs.generated.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/fs.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/fs.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/installed-runtime-entry.mjs` | F3 | — | — | no patch |
| `packages/@keld/api/src/link-last-window.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/link-last-window.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/link-quit-release.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/link-quit-release.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/link-quit-requested.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/link-quit-requested.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/link.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/menu-codec.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/menu-link.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/menu-link.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/menu.generated.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/mirror.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/package-artifact.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/package-build-noemit.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/ready-desktop-platform.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/ready-desktop-platform.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/ready-publication.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/runtime-packed-startup.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/runtime-preload-entry.fixture.mjs` | F3 | — | — | no patch |
| `packages/@keld/api/src/runtime-preload-observer.fixture.mjs` | F3 | — | — | no patch |
| `packages/@keld/api/src/runtime-preload.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/runtime-preload.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-close-receipts.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-close-receipts.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-codec.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-codec.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-desktop-source.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-desktop-state.test-role.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-desktop-state.test.ts` | F3 | — | — | no patch |
| `packages/@keld/api/src/window-navigation-codec.test.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-original-document-codec.test.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source-capacity.test.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source-diagnostic-release.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source-link.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source-release.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source-semantic-release.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source.test.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-source.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-state-phase.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-state-phase.test.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/src/window-veto-ack.test-role.ts` | F3 | — | — | per-file hunk |
| `packages/@keld/api/tsconfig.build.json` | F3 | — | — | per-file hunk |
| `packages/@keld/electron-updater/bun.lock` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/fixtures/import_only.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/fixtures/missing_facts.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/fixtures/package.json` | F4 | — | CI | per-file hunk |
| `packages/@keld/electron-updater/fixtures/probe_preload.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/fixtures/probe_self_check.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/fixtures/replay.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/package.json` | F4 | — | CI | per-file hunk |
| `packages/@keld/electron-updater/src/app-updater.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/app-updater.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/electron-app.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/error.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/import-shape.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/index.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/replay.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/src/types.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron-updater/tsconfig.json` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/bun.lock` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/app_ready.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/app_session_facts_decoy_env.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/app_surface_locale_after_ready.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/bindings_no_link.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/dialog_sync_before_ready.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias-data.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias-regression.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias-types.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/consumer.mts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/duplicate.mjs` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/independent-worker.mjs` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/matrix.mjs` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/original-ipc-consumer.mts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/original-ipc.mjs` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-alias/worker-realm.mjs` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed-lifecycle.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed_launch_early_dependency.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed_launch_early_entry.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed_launch_facts_probe.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed_launch_sidecar_probe.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/installed_original_early_dependency.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/module_export_mutation_probe.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/module_process_probe.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/module_resolve_probe.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/native_quit_requested.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/original_quit_handoff_unqualified.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/quit_before_event_preflight.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/quit_stale_last_window_closed.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/fixtures/session_recording_model.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/package.json` | F4 | — | CI | per-file hunk |
| `packages/@keld/electron/scripts/preload-runtime-codegen.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/app-session-facts.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/app-session-facts.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/app-surface.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/app.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/app.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/browser-window-ownership.test-role.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/browser-window-ownership.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/browser-window-recovery.test-role.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/browser-window-recovery.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/browser-window.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/corpus.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog-option-validation.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog-options.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog-options.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog-parent.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog-sync-consumer.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/dialog.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/entry-identity.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/file-dialog-options.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/file-dialog-options.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/index.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/installed-launch-facts.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/installed-launch-preload.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/installed-launch-preload.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/installed-lifecycle.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/installed-original-facts.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/ipc-main-delivery.test-role.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/ipc-main-delivery.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/ipc-main.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/launch-facts.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/link.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/menu.test-fixture.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/menu.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/menu.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/module-bindings.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/native-quit-requested.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/native-surface.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/native-theme.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/original-ipc.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/original-quit-handoff.component.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/original-recovery-scope.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/original-recovery-startup.test-role.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/original-recovery-startup.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/package-artifact.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/preload-runtime.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/preload-runtime.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/process-default-app.test-role.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/process-default-app.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/process-facts.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/screen.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/session-unsupported.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/session-web-request.recording.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/session.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/single-instance-facts.test.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/unsupported-bindings.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/src/unsupported.ts` | F4 | — | — | per-file hunk |
| `packages/@keld/electron/tsconfig.build.json` | F4 | — | — | per-file hunk |
| `packages/@keld/kipc/package.json` | F1 | — | CI | per-file hunk |
| `packages/@keld/kipc/scripts/channel-table.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/dialog-codegen.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/dialog-wire.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/echo-codegen.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/echo-codegen.ts` | F1 | — | CI | per-file hunk |
| `packages/@keld/kipc/scripts/launch-facts-codegen.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/menu-codegen.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/menu-codegen.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/package-artifacts.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/scripts/window-codegen.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/compat-control.generated.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/compat-control.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/endpoint-contract-boundaries.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/endpoint-value-codec.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/host-resource-policy.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/installed-original-launch-facts.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/launch-facts.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/launch-facts.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/release-build.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/runtime-process.generated.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/runtime-process.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/transport-source-ownership.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/transport.test.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/src/transport.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/test/fixtures/installed-launch-facts-v7.json` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/test/worker-link-role.ts` | F1 | — | — | per-file hunk |
| `packages/@keld/kipc/tsconfig.build.json` | F1 | — | — | per-file hunk |
| `tools/ci_workflow_security.test.ts` | G0 | owner | CI | per-file hunk |
| `tools/ci_workflow_security.ts` | G0 | owner | CI | per-file hunk |
| `tools/test_ci_local.py` | G0 | owner | CI | per-file hunk |
