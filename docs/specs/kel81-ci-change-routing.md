# Spec: CI change-based dependency routing

Status: approved
Linear: KEL-81 · Owner: GYLDLAB · Updated: 2026-08-19

## 1. Goal & non-goals

Run costly CI jobs only when a changed input can affect their observable contract,
without weakening required checks or security coverage. The workflow itself always
exists for every pull request and push; a repository-owned router controls jobs.

Non-goals:

- Removing the KEL-28 Linux GUI smoke contract.
- Using workflow-level `paths` / `paths-ignore` filters for required CI.
- Guessing ownership from a static crate list, adding a third-party path-filter action,
  or changing Keld product behavior.

## 2. Spec refs

- `AGENTS.md` § Commands & verification, § CI dependency routing
- `docs/agents/workflow.md` § Review: CI is the arbiter, humans are the architects
- `.agents/testing.md` § CI tiers

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a pull-request diff that changes only `keld-runtime`, when CI classifies it,
   then Rust/MSRV run, Linux GUI smoke is a successful skipped job, and Ubuntu clippy
   does not `apt-get` WebKitGTK (`webkitgtk=false`; `keld-cli` is omitted from
   `nongtk_packages`).
2. Given a change under Keld's actual `keld-host` local dependency closure, when CI
   classifies it, then the Linux GUI smoke runs. An IPC-only change does not also
   install WebKitGTK on Ubuntu clippy.
3. Given a docs-only or `keld-compat`-only diff, when CI classifies it, then
   documentation contracts (docs) or macOS/Windows clippy (compat) run while Ubuntu
   WebKitGTK apt and GUI smoke are successful skips.
4. Given an unknown, workspace-graph, or unavailable comparison base, when CI
   classifies it, then every potentially affected non-security job runs, including
   Ubuntu WebKitGTK apt. A workflow/router edit still creates every job and runs GUI
   smoke apt, but does not duplicate `apt-get update` onto Ubuntu clippy or MSRV.
5. Given every PR or push, when CI starts, then gitleaks runs regardless of classifier
   output, and the workflow itself was not skipped by a path trigger.
6. Given an empty, pull-request or push diff, when the router runs, then its emitted
   outputs match the classifier contract tests.
7. Given a selected workspace package with no tests, when package-scoped nextest runs,
   then it succeeds and the remaining selected package tests still run, matching the
   original workspace-suite behavior.

## 4. Design

**No boundary change.** The router owns only CI scheduling; it cannot alter product
authority, process ownership, permissions, wire bytes, or performance claims.

- The existing GitHub workflow is reused. GitHub documents that workflow-level path
  filtering leaves required checks pending, while job-level skipped checks report
  success. Therefore the router is an always-created Ubuntu job and every expensive job
  consumes its outputs through a job-level condition.
- `tools/ci_changes.sh` is the single classifier. It reads NUL-delimited paths and
  derives both (a) the `keld-host` local dependency closure for the graphical smoke and
  (b) the changed package's Cargo reverse-dependent consumers for Rust checks, from
  `cargo metadata --no-deps`. It also derives whether those selected package closures
  compile `keld-wv` so Ubuntu clippy can run GTK-free packages without apt. Live
  `apt-get` for WebKitGTK on Ubuntu clippy is owned by changed `keld-wv` / `keld-core` /
  `keld-host` paths (or unknown/lockfile fail-safe), not by every reverse-dependent that
  happens to compile `keld-core`. MSRV runs on macOS so rustc-version checking never
  waits on Azure Ubuntu mirrors. Linux GUI smoke remains the job that always apts when
  the host graphical contract is affected.
- Build graph, workflow, router and unknown inputs fail safe to all affected lanes.
  `gitleaks` is unconditional because secret ownership includes every changed file.
- A missing comparison commit enables all conditional lanes. If Cargo metadata or `jq`
  is unavailable, the router fails before it can emit a partial/empty package selection;
  it never turns a routing fault into a skipped-green result.
- Performance: no product-performance claim. Reduced CI work is an operational outcome;
  its correctness baseline is the existing full lane set.

## 5. Boundaries

- Implement in: `.github/workflows/ci.yml`, `tools/ci_changes.sh`,
  `tools/ci_changes_test.sh`, `AGENTS.md`, `.agents/testing.md`.
- Must not touch: Keld product crates, Cargo dependency declarations, wire protocol,
  permission model, or webview runtime behavior.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1: Add tested, fail-safe path classification with Cargo-derived host closure.
- [x] T2: Make job-level CI routing consume the classifier while keeping gitleaks
  unconditional and the workflow always created.
- [x] T3: Record the agent-facing ownership, fallback and review policy.

## 7. Test plan

`tools/ci_changes_test.sh` exercises empty, runtime-only, docs-only, hygiene-only,
host-dependency, manifest, workflow, unknown, pull-request base/head, push before/head,
and unavailable-comparison-base cases. It also checks the live Cargo metadata closure
contains the actual host packages. The router uses NUL-delimited Git output so spaces or
unusual file names cannot split a classification record.

## 8. Review gates triggered

- unsafe: none
- public API: none
- permission model: none
- dependency addition: none
- wire protocol: none
- CI workflow / CODEOWNERS: human sign-off required

## 9. Perf impact

No application performance impact. CI avoids unnecessary platform dependency installs;
the full lane set remains the fallback whenever ownership is not proven.

## 10. Open questions

None. This records the user's 2026-08-19 approval to replace blanket platform-job
execution with fail-safe job-level routing.
