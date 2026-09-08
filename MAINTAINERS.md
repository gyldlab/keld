# Keld maintainers

[@0monish (Monish Khandelwal)](https://github.com/0monish) is Keld's founding
maintainer and the applicant for open-source support programs on behalf of
[GYLDLAB](https://github.com/gyldlab). This identifies responsibility; it does not
claim acceptance into any program.

The [keld-maintainers team](https://github.com/orgs/gyldlab/teams/keld-maintainers)
owns code review and public triage. Its initial membership comes from existing
repository administrators: @0monish, @nixn7, @rahul-tumma, @hp282000,
@amishabenramani, @Rajkakadiya07 and @rpipaliya. Team membership and repository
permissions are the live ownership record; this list records the founding team.

## Responsibilities and decisions

- **Triage:** maintainers classify public issues, reproduce defects, publish scope
  and acceptance criteria, and identify the implementation owner. `triage` means
  a report still needs that review; `help wanted` and `good first issue` identify
  scoped work, not a promise of immediate assistance.
- **Review:** changes normally receive approval from a maintainer other than the
  author/latest pusher. CODEOWNERS requests the team; required CI and applicable
  architecture/security evidence must pass before merge. For PRs both authored and
  merged through @0monish or @amishabenramani, the internal standing delegation may
  waive an additional human approval only after every documented quality predicate
  passes. GitHub's native allowance keys on the merger, so the agent separately verifies
  both identities. Other actors keep normal review. Agent review remains technical
  evidence and is never represented as human approval.
- **Direction:** proposals and consequential decisions are discussed in public
  issues or pull requests and reflected in approved specifications. Private
  research is optional supporting material, not a prerequisite for contributing.
- **Security and conduct:** @0monish coordinates the processes in
  [SECURITY.md](SECURITY.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
  A maintainer involved in a conduct report does not decide that report; another
  uninvolved maintainer handles it.
- **Releases:** @0monish coordinates release readiness with the team. A release
  needs a reproducible distributable, documented support scope and evidence;
  the absence of a release must not be represented as a successful alpha.

Contributors become maintainers through sustained contributions and review,
public nomination, and agreement from the existing team. Membership changes and
scope are recorded publicly without disclosing private incident information.
Disagreements first receive a written technical rationale; unresolved decisions
are recorded by the founding maintainer with the reasons and remaining objections.

Start with [Contributing](CONTRIBUTING.md), [public issues](https://github.com/gyldlab/keld/issues)
or [Discussions](https://github.com/gyldlab/keld/discussions). Internal maintainer
coordination stays in [the agent workflow](docs/agents/workflow.md).
