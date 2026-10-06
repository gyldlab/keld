## Summary

- Records the owner decision #517 once, in `docs/agents/workflow.md` § Tracker issue. The tracker issue is the KELD Linear issue. When no live Linear issue covers the work and Linear cannot take a new one, it is instead the GitHub issue whose body names itself tracker of record. That issue is linked from the nearest live Linear issue, and agents never re-derive the choice.
- Linear scope, claim, status, comment and handoff duties here and in routed playbooks move to the tracker issue. Claims are posted and ordered only there, by creation time. A claim for the same work posted anywhere else never wins and counts as a conflict. Root `AGENTS.md` now requires "a tracker issue".
- `.agents/coordination.md` § GitHub tracker issue holds the GitHub mechanics:
  - task and receipt ids become `gh-<n>`/`GH-n`;
  - only comments and changes (edits, labels, assignees, relationships, state) from push-access accounts count, because the issue is public;
  - GitHub issue states map, first match wins, to Done, Canceled, Blocked, In Progress and Todo (`ready-for-agent`);
  - nothing private or security-sensitive is posted there.
- Enforcement now matches the rule. `just work-start gh-<n>` works (`tools/workspace.py`). `tools/session_closeout.py` accepts `GH-n` only when bound to `https://github.com/gyldlab/keld/issues/n`. The `.codex/hooks.json` pins are regenerated.
- Statements that said Linear owns live state now name the tracker issue: the KEL-245 spec, architecture 01, the generated status ledger and `llms` description, the branch contract (`.agents/review.md`), and the onboarding guides.

## Spec refs

No boundary change. This is an agent process contract only. `docs/specs/kel245-local-agent-workspace.md` §4 (claim authority and task-slug grammar) and architecture 01 §1 (owner of execution state) are aligned with the code and the rule. Linear-tracked work keeps the same meaning.

## Review gates

none

## Tests

- **Instruction budgets** (pinned `tiktoken 0.12.0` / `o200k_base`). Only coordination.md's cap changes, 4352 → 5120, to hold the GitHub mechanics. This closes the claim-forgery and split-arbiter review findings. workflow.md is at the 16 KiB routed hard cap, and room was made by retiring three passages that mirrored their owners: root's five-gate list, and `.agents/ci.md`'s PR-heading and `apt-get` rules. The gitleaks sentence moved into `.agents/ci.md`. The root and largest nested chains grow by 2 bytes (13,315 → 13,317; keld-wv 17,411 → 17,413; cap 24,576).

| File | Bytes | Cap | Tokens |
|---|---|---|---|
| `AGENTS.md` | 13305 → 13307 | 13312 → 13312 | 2952 → 2950 |
| `docs/agents/workflow.md` | 16353 → 16382 | 16384 → 16384 | 3586 → 3592 |
| `.agents/coordination.md` | 4297 → 5110 | 4352 → 5120 | 922 → 1127 |
| `.agents/index.md` | 4056 → 4066 | 4096 → 4096 | 898 → 900 |
| `.agents/ci.md` | 3936 → 4002 | 4096 → 4096 | 869 → 887 |
| `.agents/review.md` | 1907 → 1958 | 2304 → 2304 | 438 → 456 |
| `docs/agents/spec-template.md` | 2732 → 2750 | 3072 → 3072 | 662 → 672 |

- **Gates passed:** `just agent-context-test` (checker 15, closeout 37, hook 19, workspace 58), `target/agent-context/agent-context check .`, `just atomic-protocol` (40), `just llms-test` (8), `just llms`, `just llms-check`, `just product-status`, `just product-status-check`, `just agents-md` and `just hygiene`.
  - The last step of `just agent-context`, `tools/workspace.py check`, fails on this machine because six other sessions' active task records have missing trees (kel-102/133/139/142×2/260). The origin/main tool fails identically, and CI has no live records.
- **Mutation controls.** Each of these makes the new tests fail:
  - a kel-only issue regex;
  - a loosened task-name regex;
  - a kel-only receipt id;
  - a missing receipt host check;
  - a loosened receipt path.
- **Prompt loading.** `codex debug prompt-input` (codex-cli 0.160.1) ran at root and in all ten nested AGENTS directories, at baseline and at head. In all 22 runs the root and nested text loaded exactly, the terminal marker came after both, and nothing was truncated.
- **Representative evals.** Fresh-context read-only Opus agents were each given one revision's files, at baseline, at head, and at a negative control (head with both new owner sections deleted). Four review rounds ran; the final scenarios:

| Scenario | Baseline | Head | Negative control |
|---|---|---|---|
| Uncovered GitHub item | refuses | claims on the GitHub issue with `gh-445` | unclear |
| Linear-covered item | claims on Linear, `kel-79` | same | same (Linear default unchanged) |
| Claim forged by a non-write-access user | — | ignored | unclear |
| Same-work claim on the linked Linear issue | — | conflict, stop | — |
| GitHub issue with no tracker declaration | — | not a tracker issue | — |
| Closeout | `GH-445` rejected | `GH-445` + its URL accepted | — |

  These are single samples; they do not prove future model compliance.
- **Isolated adversarial review.** Two lenses (ownership/contradiction/budget; execution safety/tools) ran each round, with an independent refuter per finding. Rounds 1–4 confirmed 8, 11, 7 and 2 findings, all fixed in this diff. The last two were assignee and relationship changes in the trust filter, and a claim refresh on #520 (comment 6026281394). Targeted head evals: a forged Agent Brief from a non-push account is ignored, and a closed duplicate maps to Canceled and is not picked up.
- **Rust gates.**
  - `cargo fmt --all --check`: passed.
  - `cargo clippy --workspace --all-targets -- -D warnings`: passed.
  - `cargo nextest run --workspace --profile ci --no-fail-fast`: 934 run, 933 passed, 12 skipped, 1 failed (`keld-compat::lifecycle_corpus lifecycle_corpus_typescript_oracles_execute`). The same test fails identically at origin/main 80143cd0, and passes 6/6 with `CLAUDECODE` unset. Under that variable Bun 1.4.2 condenses `bun test` output, so the oracle cannot see per-test lines. Reported on KEL-237.
- **`just ci`:** not green locally. Its router `tools/ci_changes.sh` aborts under macOS `/bin/bash` 3.2 (`contract_options[@]: unbound variable`), identically at origin/main. Its gate components were run directly, as above. The GitHub `CI required` result will be at the PR head.

## Platforms

macOS 26.5.1 (CENTILLIONAIREs-Mac-mini) ran the instruction gates, tool tests, prompt loading, evals and Rust gates. Product OS behaviour is unchanged, and no real OS/device acceptance applies. Windows and Linux are covered by CI only.

## Perf impact

none

## Linear

GH-520 is the tracker of record (#517). Linear references:
- KEL-124 holds `shared:agent-workflow`; coordination notice 69c0457d.
- KEL-31 owns Linear conventions.

## Rollback

Revert the squash commit. After merging or reverting, re-trust `.codex/hooks.json` in the Codex `/hooks` UI and regenerate local Claude/Cursor hook registrations, because the hashed tool sources changed.
