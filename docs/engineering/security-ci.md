# Security CI coverage

The [CI workflow](../../.github/workflows/ci.yml) runs CodeQL and dependency review
on every pull request and push to `main`. `CI required` rejects missing, skipped,
failed or cancelled security jobs. A passing analysis is evidence that the tool ran;
it is not a claim that Keld has no vulnerabilities.

| Check | Input and coverage | Limits |
| --- | --- | --- |
| CodeQL Rust | Rust source, extracted on a GitHub-hosted macOS runner with the repository Rust pin | Static analysis; does not establish Windows/Linux containment or runtime correctness. `none` extraction still executes build scripts and procedural macros. |
| CodeQL JavaScript/TypeScript | Repository JavaScript and TypeScript source on Ubuntu | No Bun/Node behavior or Electron conformance claim. |
| CodeQL Actions | GitHub workflow source on Ubuntu | Does not audit organization settings, administrator bypasses or account access. |
| Dependency review | GitHub's dependency comparison for exact base/head commits; new known vulnerabilities of every severity and scope block | Only manifests recognized by GitHub's dependency graph. No claim of complete `bun.lock` transitive coverage. Existing vulnerabilities and unknown advisories require separate review. |
| cargo-deny | Cargo advisory, license and dependency policy in `deny.toml` | Cargo policy does not cover npm dependencies. |
| gitleaks | Repository history | Secret detection does not establish revocation of an exposed credential. |

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
alert-based merge blocking enabled. Repository settings are not established by this
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
