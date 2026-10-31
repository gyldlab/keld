# Contributing to Keld

This is Keld's canonical public-intake guide for external contributors.
You can contribute through public GitHub issues and pull requests. You do not need
access to Linear, private research, or an agent-memory service. Please follow the
[Code of Conduct](CODE_OF_CONDUCT.md); [MAINTAINERS.md](MAINTAINERS.md) identifies
who can help.

## Find a useful change

Check [open issues](https://github.com/gyldlab/keld/issues) before starting. Look for
[good first issues](https://github.com/gyldlab/keld/labels/good%20first%20issue) or
[help wanted](https://github.com/gyldlab/keld/labels/help%20wanted), or report a
reproducible problem. Include your Keld commit, OS/architecture, exact command,
expected result, actual output and relevant tool versions: `rustc --version` and
`cargo --version` for Rust reports, `cargo nextest --version` for workspace tests,
and `bun --revision` for Bun-dependent behavior. Remove secrets and private paths from
anything you publish. Report security vulnerabilities through
[SECURITY.md](SECURITY.md).

For a feature or architecture change, agree on its contract and acceptance criteria
before implementation. Maintainers publish the relevant decision on GitHub and
handle internal planning links.

Documentation corrections, independent demo reports, and small regression fixes are
valuable. The [product-status ledger](docs/engineering/product-status.md) distinguishes
working slices from planned work; [public project planning](https://github.com/gyldlab/keld/issues/167)
records the current foundation priorities.

## Claim an issue before editing

Before editing for an existing public issue, post a **Public work claim** on that
issue, including for a small fix or when using a coding agent. Read the issue,
comments, assignee and linked PRs first. Reading, investigating and discussing do
not need a claim. After posting, refresh the issue, comments, assignee and linked
PRs before editing; if another claim or overlapping work appeared, use the
coordination outcomes below. A direct typo or similarly small documentation PR without an
existing issue may proceed without creating an issue or claim; explain its scope
in the PR. Do not create retrospective claims for work already submitted.

Post through your GitHub account. An agent may post only through its contributor's
authorized account; without posting authorization or access, it must give the
contributor the comment to post and wait before editing. Agent assistance may be
disclosed but is optional. Use this example, replacing the example issue, scope
and paths; omit the branch/PR line until one exists:

```text
## Public work claim
- Contributor: @your-github-handle
- Issue: #123
- Scope: clarify the quick-start prerequisites
- Expected files: docs/onboarding/README.md
- Branch/PR: https://github.com/your-github-handle/keld/tree/docs-prerequisites
```

Include only public information: no device hostname, credentials, private Linear
records or security-sensitive paths. A claim records intent, not an exclusive lock,
assignment, repository/CI permission or maintainer authorization. Maintainer-authorized
agents still follow the [internal claim process](docs/agents/workflow.md#agent-claim-mandatory-before-any-edit).

Use these outcomes to coordinate work:

| Issue state | Next action |
| --- | --- |
| Open, unassigned, no active claim or overlapping PR | Post your claim, then start within the published scope. A feature or architecture change still needs the agreement described above. |
| Another active public claim or overlapping PR | Comment with your proposed scope and ask the contributor and a maintainer to split the work or choose a lead. Wait before overlapping edits; a second claim cannot displace the first. |
| Assigned or explicitly reserved by a maintainer, including an internal agent claim | Ask a maintainer on the public issue for a separate scope or handoff before editing. A public claim cannot override that decision. |
| Multiple interested contributors | A maintainer acknowledges a lead or distinct scopes in a public comment and may assign the issue. Comment order alone does not select a winner. Until that decision, pause overlapping edits. |
| At least 14 calendar days since the claimant's latest public progress/blocker update (or initial claim) | The public intent claim is inactive. Post a fresh claim; if the issue is unassigned with no overlapping open PR, work may start. An assignment, maintainer reservation or open PR still needs a maintainer handoff. |
| Withdrawn claim or closed, unmerged PR | Post that you are stopping and link useful work; this ends your intent claim. A new contributor may claim an open issue, subject to any remaining assignment, reservation or overlapping PR. Ask a maintainer to clear an obsolete assignment. |
| Completed work | Link the merged PR and say whether scope remains. A closed issue needs a maintainer to reopen it or identify a follow-up before new implementation. Remaining work on an open issue may be claimed under the rules above. |

While working, post a public progress/blocker update before the inactivity
threshold above; link your PR when available. For a handoff, name the remaining
scope and link the branch/PR, then have the next contributor post their own claim.
Maintainers record changed assignments or scope decisions publicly. If coordination
stalls, choose another unclaimed issue or ask a [maintainer](MAINTAINERS.md) to resolve
it; silence is not approval to overlap reserved work.

## Build, change, and submit

1. Fork the repository and create a branch for one concern.
2. Follow the [quick-start](docs/onboarding/README.md) to build and run the current demo.
3. Read the root [engineering rules](AGENTS.md) and the nearest crate's `AGENTS.md`
   before editing code. Preserve permission checks and add a regression test for a bug.
4. Run the relevant tests while developing, then the routine local gate. Mermaid is
   selected only when its diagram/renderer inputs change; use `just ci-full` to force a
   full-corpus Mermaid render:

   ```bash
   just ci
   ```

   The [development guide](docs/onboarding/05-development-guide.md) lists prerequisites
   and explains the gates; see [Discovering and running `just` commands](docs/onboarding/05-development-guide.md#33-discovering-and-running-just-commands)
   for a task-oriented recipe guide. Include the actual results and any unavailable platform;
   do not claim a desktop behavior from a compile-only result.
5. Open a pull request linked to the public issue. Use the provided template to explain
   the change, contract, tests, platforms, review gates, and performance impact. A draft
   PR is welcome when an acceptance check still needs maintainer help.

For an included documentation source, regenerate with `just llms`, then run
`just llms-test` and `just llms-check`. Keep generated files with their source change.

## Maintainer procedures

Maintainers and assigned agents use the [internal workflow](docs/agents/workflow.md)
for Linear linkage, claims, worktrees, independent review, and integration. Contributors
use the public intake above; maintainers own the internal coordination. Optional local
research and checkout-hook setup is owned by the [justfile](justfile) recipes
`research-sync` and `hooks-install`; neither is a prerequisite for an external
contribution.

Contributions are licensed under MIT OR Apache-2.0, as declared in
[LICENSE](LICENSE), [LICENSE-MIT](LICENSE-MIT), and [LICENSE-APACHE](LICENSE-APACHE).
