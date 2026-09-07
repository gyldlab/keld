# Contributing to Keld

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

Comment on the public issue before a substantial change so a maintainer can confirm
scope and avoid overlapping work. For a feature or architecture change, agree on its
contract and acceptance criteria before implementation. Maintainers publish the
relevant decision on GitHub and handle internal planning links.

Documentation corrections, independent demo reports, and small regression fixes are
valuable. The [product-status ledger](docs/engineering/product-status.md) distinguishes
working slices from planned work; [public project planning](https://github.com/gyldlab/keld/issues/167)
records the current foundation priorities.

## Build, change, and submit

1. Fork the repository and create a branch for one concern.
2. Follow the [quick-start](docs/onboarding/README.md) to build and run the current demo.
3. Read the root [engineering rules](AGENTS.md) and the nearest crate's `AGENTS.md`
   before editing code. Preserve permission checks and add a regression test for a bug.
4. Run the relevant tests while developing, then the full local gate:

   ```bash
   just ci
   ```

   The [development guide](docs/onboarding/05-development-guide.md) lists prerequisites
   and explains the gates. Include the actual results and any unavailable platform;
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
