# Open-source governance evidence (KEL-179)

No product boundary change. Public work item: [#169](https://github.com/gyldlab/keld/issues/169), within [foundation #167](https://github.com/gyldlab/keld/issues/167).

## Observed administration

Observed 2026-09-07 UTC using the authenticated repository-owner GitHub API:

- `@gyldlab/keld-maintainers` is a visible team with explicit repository write access. Seven existing active organization members with repository admin access were added; the triage-only bot was not promoted. Existing direct permissions were preserved.
- Issues were already enabled. Discussions, private vulnerability reporting, Dependabot alerts/security updates, secret scanning and push protection were enabled and read back.
- Main retains its app-bound `gatekeeper`, `title-lint` and `CI required` checks, strict up-to-date requirement, administrator enforcement, conversation resolution, and no force-push/deletion. Required independent reviews changed from zero to one; CODEOWNER/latest-push review and stale-approval dismissal are enabled. A separate no-bypass PR rule remains active, while the native review allowance names only `0monish` and `amishabenramani`; GitHub keys that allowance on the merger, not the author.
- The confirmed applicant is @0monish. Public GYLDLAB membership was enabled. The separate optional profile-company edit lacked OAuth scope; no broader token grant was requested and no profile edit is claimed.
- The existing owner-published contact address is `hello@gyldlab.com`; no mailbox or report was fabricated and no email was sent. Delivery, mailbox access controls and actual human response were not exercised.

[CodeQL alert rule 22483865](https://github.com/gyldlab/keld/rules/22483865) is prepared **disabled**, with no bypass actors. It requires CodeQL, security alerts at all levels and ordinary errors/warnings. Activation waits for the reviewed advanced workflow to land and produce actual main-baseline results; current PR scan/upload success does not prove active alert blocking. This avoids creating a bootstrap dependency on evidence only the not-yet-landed workflow can produce. After that predicate, activate and read back the effective branch rule. KEL-180 owns workflow and dependency admission.

## Instruction change

Public intake is owned by `CONTRIBUTING.md`. `docs/agents/workflow.md`, routed by implementation/coordination, links that owner and states only the maintainer transition into private linkage, claims and merge authority. The issue forms link CONTRIBUTING and retain field-specific prompts. `.agents/review.md` retains branch mechanics and now explicitly scopes its namespace rule to maintainer branches. `.agents/coordination.md` owns the narrower internal merge exception: both PR author and authenticated merger must be `0monish` or `amishabenramani`, and every quality predicate remains required. Its Prompt Tracker link consumes the existing task-handoff identity owner rather than copying that model policy. No root/nested always instructions changed. The public contributor rewrite originally prepared under KEL-178 is included here so the owner and consumer land together; its planning link uses public issue #167 rather than a not-yet-landed ROADMAP file.

| Measurement | Before | After |
|---|---:|---:|
| UTF-8 bytes | 14703 | 15091 |
| tiktoken 0.12.0 / o200k_base tokens | 3337 | 3405 |

The branch-owner qualifier changes `.agents/review.md` from 1896 to 1907 bytes and 436 to 438 o200k_base tokens; its existing budget is unchanged.
The merge-identity and Prompt Tracker owner-link rules change `.agents/coordination.md` from 3030 to 3326 bytes and 658 to 729 o200k_base tokens (cross-checked with pinned `js-tiktoken` 1.0.21); its routed cap changes from 3072 to 3328 under KEL-190. Automatic root/nested chains are unchanged.

Rejected alternative: require every external contributor to obtain private Linear/research or a specific agent harness. Required significant-feature spec approval remains before implementation; public intake is not a gate bypass. Preserving explicit critical CODEOWNERS paths maintains the existing review map while changing the owner to the verified team.

Representative checks cover: a new public contributor without Linear; an external contributor using an agent; a project-maintainer agent that still requires an internal claim; a feature without approved spec; eligible and ineligible author/merger pairs; a PR missing any quality predicate; and private security versus conduct reporting. Static checks prove delivery and links, not real model compliance or a human newcomer's experience. The public [stranger-test issue](https://github.com/gyldlab/keld/issues/177) collects genuine independent feedback; no such participation is invented.

## Instruction evaluation

Read-only baseline/head evaluations ran on physical Ubuntu with Codex CLI 0.150.1, requested `gpt-5.6-terra`/high, an ephemeral read-only sandbox and network use forbidden by the prompt. The CLI did not expose a distinct served-model revision. Baseline is `65e5036c0d7945427c349a881d490c95f06cfef4`; evaluated instruction head is `1398902cb20f5e816fcea7e50b8e83c2d591a063`. Each cell is one sample; token/latency differences include cache and tool-output variation and are not a performance or model-compliance claim. The CLI emitted no billing value, so cost is unknown.

| Seed | Baseline decision; files/tools; tokens; ms | Head decision; files/tools; tokens; ms | Contract observation |
|---|---|---|---|
| security/path | stop; 8/10; 265285; 86325 | proceed; 5/7; 303878; 105608 | Both preserve spec, permission/public/wire review and real deny/no-side-effect falsifiers; decision variance is not scored. |
| CI | ask; 4/6; 249558; 81295 | ask; 5/5; 229157; 79382 | Both require routed applicability, `CI required`, failure-preserving negative controls and exact gates. |
| docs/Mermaid | proceed; 5/4; 164581; 70468 | proceed; 5/3; 174230; 56931 | Both route docs/testing and require generated corpus plus pinned render/visual evidence. |
| research | proceed; 10/9; 419619; 120154 | stop; 5/4; 139717; 66356 | Both preserve nested-repo/labels/pins/publication gates; head honestly stops because that worktree lacks the nested checkout. |
| PR review | ask; 7/5; 168254; 90048 | stop; 6/5; 163160; 56580 | Head requires issue ownership plus GitHub-verified author and merger identities before merge. |
| ordinary CLI bug | proceed; 5/5; 157337; 68024 | proceed; 5/7; 274992; 96063 | Both route the CLI owner, failure-first integration test and full gate without public/architecture expansion. |
| public intake | ask; 10/3; 125736; 60257 | proceed; 8/5; 149143; 69108 | Baseline asks how an external user gets a KEL branch; head uses public issue/fork/PR and forbids invented private access. |

Across the seven rows, baseline used 1,550,370 input+output tokens and 42 tool calls; head used 1,434,277 and 36. No efficiency conclusion follows from one sample. Three paired semantic controls are decisive: (1) an inverted CONTRIBUTING private-access rule changes public intake from head `proceed` to `stop`; (2) head refuses an `another-actor` author+merger, while an additive waiver mutation proceeds; (3) head loads Prompt Tracker and returns system/device, agent/client, exact requested model and effort, while deleting the owner link falls back to generic claim fields. The same three source mutations each make `just atomic-protocol` exit 1. Root and all eight nested prompt-input traces contain one complete root chain and the expected nested marker.

The immutable raw bundle is `keld-kel179-instruction-evals.tar.gz`, 876597 bytes, SHA-256 `e8b82c72692efc11aee77de39e973dc28f50a18c56ac6465f37ae2cd34ce16e1`, with 159 checksum-covered files. It retains prompts, schema, event streams, responses, timing, usage, static mutations, reviewer/refuter reports and the first failed summarizer assertion. Its portable verifier checks 14 baseline/head rows, five semantic controls, three static mutations and nine prompt traces. Publication in the PR/Linear evidence does not make the samples a behavioral guarantee.

That bundle froze when `1398902` was the evaluated instruction head, so its statement that only the evidence record would follow was true at freeze but is incomplete for the final branch. Later changes are confined to this evidence record, `tools/atomic_protocol.rs` checker hardening, and a `MAINTAINERS.md` owner link that removes duplicated policy without changing eligibility. The routed instruction bytes and evaluated target semantics are unchanged. Archived static mutation logs prove the `1398902` checker; final-tip gates and review, recorded in the PR, prove the later checker. The semantic responses remain evidence for the unchanged instruction inputs, not for the checker implementation.

## Verification and rollout

The governance diff passed `just ci`, including formatting, warning-denied workspace Clippy, 630 Rust cases (two ignored) and 41 TypeScript cases. CodeRabbit identified a conduct-report conflict: the named coordinator could receive a report about themself. The corrected policy avoids that inbox and requires a separate independent route; GitHub Support is an available private platform channel, not a claimed internal adjudicator. The independent review also caught the old CONTRIBUTING namespace requirement and ambiguous reconsideration channel. Both are corrected here: public intake is self-contained and appeals preserve the original conflict-safe route. Final revision checks and exact review identity/results are recorded in the PR. Settings are already applied as stated; file changes become public main only after CI and applicable review. A successful eligible merge with both identities verified is the first live effectiveness check for the two-account exception; this document does not claim it before landed verification.

Rollback of document behavior is a reviewed revert of this scoped PR. Before-state administrative JSON and applied request/readback records are preserved by the issue operator. Any settings rollback is separately reviewed; do not turn off review or protection just to merge this change. Keep the original primary-checkout ignored ROADMAP backup before pulling the public tracked roadmap.
