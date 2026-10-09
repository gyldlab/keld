# Security CI coverage

The [CI workflow](../../.github/workflows/ci.yml) runs dependency review on every
pull request and push to `main`. CodeQL analyses every language on every push to
`main`, so the default branch always has a complete baseline. On a pull request the
change router (`tools/ci_changes.sh`) selects each CodeQL language only when that
language's analysed inputs changed: Rust for `*.rs` and Rust build inputs,
JavaScript/TypeScript for the source and data files its extractor reads, and Actions
for workflow and action files. Unknown inputs, all-lane fallbacks and workflow or
router edits select every language, and a push whose change router fails still
runs every language. `CI required` rejects missing, failed or
cancelled dependency review, a selected CodeQL language whose job is missing,
skipped, failed or cancelled, and an unselected language whose job ran. A passing
analysis is evidence that the tool ran; it is not a claim that Keld has no
vulnerabilities.

`ci-hygiene check` runs the parsed workflow security check through Bun, the same
runtime already required by `just ci`. `tools/ci_workflow_security.ts` owns checkout
and scanner semantics, the cargo-deny delivery contract, and the 15-minute step
timeout (inside a longer job timeout) on every `run` script that invokes
`apt`/`apt-get`; the Rust checker retains the other hygiene contracts.
Each CodeQL language has its own job (`codeql-rust`, `codeql-javascript-typescript`,
`codeql-actions`), because a job-level condition cannot read `matrix`. Each job must
need the router, run only on its own router output, keep its `/language:<language>`
upload category, and set no strategy. CodeQL steps in any other job are refused, so
a successful job cannot silently represent a missing or duplicated scan category.
Block, flow and aliased steps are inspected as objects. Missing, malformed,
multidocument, cyclic and unknown job/step structures are refused. Bun 1.4.2's
parser uses the last value for duplicate keys; this check does not claim to reject
duplicate-key syntax or prove full equivalence to GitHub's YAML implementation.
The selected parser controls run in CI against pinned Bun 1.4.2. See
[Bun's YAML documentation](https://bun.com/docs/runtime/yaml) for its conformance limits.

| Check | Input and coverage | Limits |
| --- | --- | --- |
| CodeQL Rust | Rust source, extracted on a GitHub-hosted macOS runner with the repository Rust pin | Static analysis; does not establish Windows/Linux containment or runtime correctness. `none` extraction still executes build scripts and procedural macros. |
| CodeQL JavaScript/TypeScript | Repository JavaScript and TypeScript source on Ubuntu | No Bun/Node behavior or Electron conformance claim. |
| CodeQL Actions | GitHub workflow source on Ubuntu | Does not audit organization settings, administrator bypasses or account access. |
| Dependency review | GitHub's dependency comparison for exact base/head commits; new known vulnerabilities of every severity and scope block | Only manifests recognized by GitHub's dependency graph. No claim of complete `bun.lock` transitive coverage. Existing vulnerabilities and unknown advisories require separate review. |
| cargo-deny | Cargo advisory, license and dependency policy in `deny.toml`, plus the updater helper's own ban list | Cargo policy does not cover npm dependencies. The live advisory database can change the result without a file diff. |
| gitleaks | Pull request: that pull request's own commits (event `base.sha..head.sha`, both resolved). Push to `main`: `main`'s full history. Unmerged branches and tags are not scanned by CI; GitHub secret scanning (provider patterns, all branches) is their only coverage | Secret detection does not establish revocation of an exposed credential. Merge-commit conflict resolutions are not diffed. |

gitleaks runs on every event. Its configuration (`.gitleaks.toml`) and fingerprint
ignores (`.gitleaksignore`) have no other reader, so a change to either selects no
other CI lane.

cargo-deny runs from the upstream `x86_64-unknown-linux-musl` release archive at an
exact version. The job downloads it from the cargo-deny GitHub release, and `sha256sum -c`
must match the SHA-256 recorded in the workflow before the binary is extracted; both
policy runs then invoke that binary by absolute path. The Docker-based
`EmbarkStudios/cargo-deny-action` it replaces built its image from a Docker Hub base
image, whose anonymous pull limit failed the lane (#676), and it downloaded cargo-deny
without verifying it. The workflow-security check pins the version, the checksum, the
step order and the exact policy arguments, refuses conditions and `continue-on-error`
on these steps, and admits no other action in the job. cargo-deny fetches the RustSec
advisory database itself, from its default `https://github.com/RustSec/advisory-db`.
To bump it, change the version and SHA-256 in `.github/workflows/ci.yml` and
`tools/ci_workflow_security.ts` together, after checking the new archive against the
release's published digest.

No CI step pulls from Docker Hub. The only container image CI uses is the Mermaid
renderer, pulled from `ghcr.io` by immutable digest in `tools/mermaid_render_check.sh`.

The Ubuntu WebKitGTK `.deb` set comes from a first-party `actions/cache` entry. Its key is
the runner image (`ImageOS`, `ImageVersion`) plus the SHA256 of the job's exact package list
(`tools/ci_webkitgtk_apt.sh key`). The cache is not trusted. apt accepts a file already in
its archive directory when only the size matches (the `pkgAcqArchive::pkgAcqArchive`
constructor in `apt-pkg/acquire-item.cc`, lines 3494-3512 in apt 2.7.14 on Ubuntu 24.04),
so the script:

1. still runs `apt-get update`, which verifies the signed InRelease metadata. Under apt's
   default `APT::Update::Error-Mode=persistent`, a transient fetch failure is only a warning
   and apt keeps the lists it verified earlier, while a signature failure is still an error
   (`AcquireUpdate` in `apt-pkg/update.cc`). The trust root is therefore always signed, but not
   guaranteed fresh. The default is kept deliberately: it tolerates mirror flakes, and
   stale-but-signed lists can only name older signed packages, which step 3 still binds;
2. lists the exact files with `apt-get install --print-uris -o Acquire::ForceHash=SHA256`;
3. stages a cached file only when its SHA256 and size match that signed index entry.

Every other package downloads and is hash-checked by apt. Index lists are never cached, and
the workflow-security check refuses any other `actions/cache` step. On a miss the step runs
the plain update and install behind the 15-minute step timeout, then saves the cache. A stuck
cache download aborts after 2 minutes (`SEGMENT_DOWNLOAD_TIMEOUT_MINS`) and proceeds as a
miss, and both cache steps have a 5-minute bound.

The main-scoped caches (this one and `rust-cache`) stay trustworthy only while default-branch
workflows never run pull-request code. `tools/ci_workflow_security.ts` therefore parses every
workflow with a `pull_request_target` trigger (today `keldbot.yml`), and refuses any
`actions/checkout`, `run:` step or reusable-workflow job in it.

Dependency review first checks every API response page for GitHub's incomplete
snapshot warning. Unavailable APIs, malformed refs or incomplete snapshots fail the
job; restore the dependency graph's base/head metadata and rerun the same head.
The upstream dependency-review action otherwise reports snapshot warnings without
failing. The metadata probe and action are separate reads of the same immutable refs;
they do not prove that GitHub indexes every package ecosystem or dependency.

The workflows use `pull_request` with ephemeral hosted runners, no user secrets,
read-only repository contents and disabled checkout credential persistence. Only
CodeQL receives `security-events: write` to upload results. This is not an assertion
that Rust extraction is execution-free or that a writable PR workflow independently
authenticates its own check; that trust boundary remains tracked in KEL-169.

CodeQL's analysis/upload job result and its alert result are different checks.
Maintainers must verify live scan results and configure GitHub's **Require code
scanning results** rule for CodeQL and the chosen alert thresholds before calling
alert-based merge blocking enabled. Before activating that rule, also verify how it
treats a pull request whose diff selected no CodeQL language; that behavior is not
established by this workflow file. Repository settings are not established by this
workflow file. Public work and outstanding acceptance are tracked in
[issue #170](https://github.com/gyldlab/keld/issues/170).

Pins verified on 2026-09-08 (Asia/Kolkata):

- CodeQL action v4.37.9, commit `cdf488f595d80d6e07e03d4674febd5ab45fa938`,
  released 2026-08-26; default CodeQL bundle 2.26.4.
- Dependency review v5.0.0, commit `a1d282b36b6f3519aa1f3fc636f609c47dddb294`,
  released 2026-05-08; requires a Node 24-capable Actions runner.

Primary sources: [Rust extraction behavior](https://docs.github.com/en/code-security/reference/code-scanning/codeql/build-options-for-compiled-languages),
[dependency graph formats](https://docs.github.com/en/code-security/reference/supply-chain-security/dependency-graph-supported-package-ecosystems),
[dependency-review action](https://github.com/actions/dependency-review-action/tree/a1d282b36b6f3519aa1f3fc636f609c47dddb294),
[CodeQL release](https://github.com/github/codeql-action/releases/tag/v4.37.9),
and [code scanning merge protection](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/manage-your-configuration/set-merge-protection).

Rollback is a reviewed revert of the security jobs and their required-result handoff
together, plus reconciliation of any separately enabled GitHub code-scanning ruleset.
A failing security check is investigated before any rollback; its evidence remains in
the issue and Actions logs.
