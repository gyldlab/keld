# Open-source governance evidence (KEL-179)

No product boundary change. Public work item: [#169](https://github.com/gyldlab/keld/issues/169), within [foundation #167](https://github.com/gyldlab/keld/issues/167).

## Observed administration

Observed 2026-09-07 UTC using the authenticated repository-owner GitHub API:

- `@gyldlab/keld-maintainers` is a visible team with explicit repository write access. Seven existing active organization members with repository admin access were added; the triage-only bot was not promoted. Existing direct permissions were preserved.
- Issues were already enabled. Discussions, private vulnerability reporting, Dependabot alerts/security updates, secret scanning and push protection were enabled and read back.
- Main retains its app-bound `gatekeeper`, `title-lint` and `CI required` checks, strict up-to-date requirement, administrator enforcement, conversation resolution, and no force-push/deletion. Required independent reviews changed from zero to one; CODEOWNER/latest-push review and stale-approval dismissal are enabled.
- The confirmed applicant is @0monish. Public GYLDLAB membership was enabled. The separate optional profile-company edit lacked OAuth scope; no broader token grant was requested and no profile edit is claimed.
- The existing owner-published contact address is `hello@gyldlab.com`; no mailbox or report was fabricated and no email was sent. Delivery, mailbox access controls and actual human response were not exercised.

[CodeQL alert rule 22483865](https://github.com/gyldlab/keld/rules/22483865) is prepared **disabled**, with no bypass actors. It requires CodeQL, security alerts at all levels and ordinary errors/warnings. Activation waits for the reviewed advanced workflow to land and produce actual main-baseline results; current PR scan/upload success does not prove active alert blocking. This avoids creating a bootstrap dependency on evidence only the not-yet-landed workflow can produce. After that predicate, activate and read back the effective branch rule. KEL-180 owns workflow and dependency admission.

## Instruction change

Public-intake owner: `docs/agents/workflow.md`, routed by implementation/coordination. `.agents/review.md` retains branch mechanics and now explicitly scopes its namespace rule to maintainer branches. External contributors and their tools use public GitHub intake/forks without private access; maintainers preserve internal linkage, spec approval, claims, testing and review. No root/nested always instructions or budgets changed. CONTRIBUTING and forms consume that owner rather than copy the internal procedure. The public contributor rewrite originally prepared under KEL-178 is included here so the owner and consumer land together; its planning link uses public issue #167 rather than a not-yet-landed ROADMAP file.

| Measurement | Before | After |
|---|---:|---:|
| UTF-8 bytes | 14703 | 15453 |
| tiktoken 0.12.0 / o200k_base tokens | 3337 | 3479 |

The branch-owner qualifier changes `.agents/review.md` from 1896 to 1907 bytes and 436 to 438 o200k_base tokens; its existing budget is unchanged.

Rejected alternative: require every external contributor to obtain private Linear/research or a specific agent harness. Required significant-feature spec approval remains before implementation; public intake is not a gate bypass. Preserving explicit critical CODEOWNERS paths maintains the existing review map while changing the owner to the verified team.

Representative checks cover: a new public contributor without Linear; an external contributor using an agent; a project-maintainer agent that still requires an internal claim; a feature without approved spec; a PR without human/CODEOWNER approval; and private security versus conduct reporting. Static checks prove delivery and links, not real model compliance or a human newcomer's experience. The public [stranger-test issue](https://github.com/gyldlab/keld/issues/177) collects genuine independent feedback; no such participation is invented.

## Verification and rollout

The governance diff passed `just ci`, including formatting, warning-denied workspace Clippy, 630 Rust cases (two ignored) and 41 TypeScript cases. CodeRabbit identified a conduct-report conflict: the named coordinator could receive a report about themself. The corrected policy avoids that inbox and requires a separate independent route; GitHub Support is an available private platform channel, not a claimed internal adjudicator. The independent review also caught the old CONTRIBUTING namespace requirement and ambiguous reconsideration channel. Both are corrected here: public intake is self-contained and appeals preserve the original conflict-safe route. Final revision checks and exact review identity/results are recorded in the PR. Settings are already applied as stated; file changes become public main only after CI and the newly required human approval.

Rollback of document behavior is a reviewed revert of this scoped PR. Before-state administrative JSON and applied request/readback records are preserved by the issue operator. Any settings rollback is separately reviewed; do not turn off review or protection just to merge this change. Keep the original primary-checkout ignored ROADMAP backup before pulling the public tracked roadmap.
