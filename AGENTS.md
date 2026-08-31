# AGENTS.md — Keld invariant floor

Desktop framework: Rust host (windows/webviews/native); JS/TS main and named compat
roles on supervised Bun children; kipc IPC; generated, host-enforced default-deny;
Electron compatibility through `@keld/electron` + `keld migrate`.

**MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** use RFC 2119 meaning
in agent-facing files. Architecture specs remain prose.

## Ground truth

- Specs: `docs/architecture/01..07-*.md`; research: nested `docs/research/` checkout.
- [`llms.txt`](llms.txt) is the compact generated index; [`llms-full.txt`](llms-full.txt)
  is its ordered corpus. Included-source changes MUST pass `just llms-check`.
- Code/spec mismatch is a bug in one. Fix both in the same PR or state the blocker;
  MUST NOT drift silently.
- Features require an approved `docs/agents/spec-template.md` spec plus Linear (KELD).
  `docs/agents/workflow.md` owns execution; routed review/coordination playbooks own
  their operational rules.
- The nearest crate `AGENTS.md` adds path invariants and MUST be read before editing it.

## Instruction loading and routing (MUST)

- `.agents/index.md` is the only task router. Load only matching playbooks.
- Load classes are `always` (root/nested AGENTS), `routed` (exact task trigger), and
  `evidence` (search/slice only; never mandatory full-read).
- Before implementation or review, query only relevant areas in
  `docs/agents/learnings.md`; use bounded query/slices only.
- Any new or changed agent instruction MUST follow `.agents/instructions.md`: one owner,
  load class, exact trigger, byte/token delta, representative eval, and rollback.
- `just agent-context` is required for agent-instruction changes. Agents MUST refuse an
  over-budget `always` chain, unknown instruction file, missing route, duplicate owner,
  hollow file, or unexplained budget increase.
- External research, Prompt Tracker, MemPalace, private research, and public benchmark
  rules are owned by `.agents/research.md` and `.agents/memory.md`, not this floor.

## Directness and scope

- Lead with evidence; disagree when code, specs, OS contracts, or primary sources do.
- State uncertainty and missing proof; MUST NOT present inference as fact.
- Fix the root cause, keep scope bounded, and park adjacent cleanup.
- Ask one focused question only when a user-owned choice materially changes the result;
  otherwise take the smallest reversible path.

## Atomic problem-solving protocol (MUST)

Before selecting a design, answer or fix for any non-trivial design, diagnosis, review
or implementation, agents MUST decompose the problem into atomic reasoning units. This
is a decision input, not a retrospective explanation. Small obvious tasks MAY record it
concisely, but no boundary or gate failure may skip it.

1. **Decompose before deciding (MUST).** Split the problem into decision-bearing atoms
   small enough that one observable can falsify each atom without relying on the final
   conclusion.
2. **State the logical component (MUST).** Each atom MUST name its owner, boundary and
   inputs/outputs, failure mode, and observable contract.
3. **Validate independence (MUST).** Changing or falsifying one atom MUST NOT silently
   alter another. Hidden coupling MUST be promoted into its own atom or an explicit edge
   between atoms.
4. **Verify correctness (MUST).** Each atom MUST have direct evidence or a falsifiable
   test or negative control. Prose, comments, mocks, or another atom's pass are not proof
   of that atom.
5. **Synthesize only after proof (MUST).** Agents MUST NOT synthesize an answer, design
   or fix until every decision-bearing atom is passed, explicitly unknown, or named as a
   blocker. If the synthesis contradicts a passed atom, agents MUST stop and correct the
   model; they MUST NOT average away the contradiction.

Performance decompositions MUST separate census, work, queue/copy, clock, statistic and
artifact. Security decompositions MUST separate identity, authentication, authorization,
OS containment, lifecycle/revocation and evidence provenance.

Enforcement: `just atomic-protocol` validates the canonical stages and the narrower
workflow, testing and routing references. It MUST pass after changing any of those files.

## Engineering principles — non-negotiable

For every architecture, public-contract, process, IPC, permission, lifecycle or
performance design change, agents MUST apply first-principles systems engineering and
DRY before choosing a design or writing code. Familiar framework shapes, a larger
language rewrite, and a passing happy path are not evidence that a design is correct,
secure or fast. For smaller changes, agents MUST state `No boundary change` when that
distinction would otherwise be ambiguous.

1. **Start from facts, not analogies.** Apply the Atomic problem-solving protocol above.
   For Keld architecture and public-contract work, the atoms MUST cover ownership,
   process, memory, I/O, lifecycle, trust and failure facts: who owns each handle, who
   can mint each identity, what can crash independently, where copies/queues occur, and
   what observable contract proves the result. An unmeasured performance claim or an
   uncited platform assumption MUST NOT decide architecture.
2. **Reuse before rewrite.** Agents MUST search for and evaluate the existing shared
   abstraction, platform primitive, verified upstream facility and generated contract
   before adding a replacement. A rewrite is permitted only when the existing option
   cannot meet a named correctness, security, ownership or measured-performance
   requirement. An approved spec—or, for a bug fix governed by an existing spec, the
   PR—MUST record the rejected alternative and preserve a compatibility fallback whenever
   the published contract requires one.
3. **One rule, one owner, one source of truth.** Agents MUST NOT duplicate policy,
   schema, permission checks, wire parsing, lifecycle state, platform shims or helpers
   because a shared implementation is inconvenient. Fix or extend the owning
   abstraction with tests. Parallel copies, mirrored constants and diverging fallback
   paths are defects, not expedient implementation choices.
4. **Performance is an outcome, not a language property.** Rewriting an API in Rust,
   adding shared memory, or removing a runtime does not by itself prove improvement.
   Agents MUST establish semantic equivalence and use an attributed, reproducible
   benchmark before claiming or retaining a performance-motivated replacement. The
   baseline remains the simpler correct path until a measured end-to-end gain justifies
   added complexity.
5. **Reject violations at the boundary.** A design or PR that violates these rules MUST
   stop for correction; agents MUST NOT hide it behind a local workaround, flag, special
   case or broad permission. If the shared abstraction itself is wrong, propose its
   smallest root-cause fix in an approved spec. Human review may choose a different
   architecture, but it MUST record the new invariant rather than grant an undocumented
   exception.

## Repo map

`TARGET` is specified destination scope; `SKELETON` is a name-only surface. Current
status is `docs/architecture/01-overview.md` §1.

| Crate | Current role / destination |
|---|---|
| keld-core | Hello window + lifecycle session; TARGET event loop/window registry; nested `AGENTS.md` |
| keld-wv | System WebEngine backends; TARGET CEF; nested `AGENTS.md` |
| keld-ipc | kipc framing/codecs; TARGET channel registry + shm; nested `AGENTS.md` |
| keld-guard | Capabilities, manifest, scopes; nested `AGENTS.md` |
| keld-native | Guard-checked brokers; `fs` live, rest SKELETON |
| keld-runtime | Supervised Bun child-role runtime; nested `AGENTS.md` |
| keld-update | SKELETON; TARGET signed manifests + delta update |
| keld-pack | SKELETON; TARGET installers/signing/cross-compile |
| keld-compat | Electron conformance/lifecycle oracle; TARGET host emulation; nested `AGENTS.md` |
| keld-host | Shipping host binary; nested `AGENTS.md` |
| keld-cli | create/dev/doctor/mcp live; build/migrate/gen/ext reserved; nested `AGENTS.md` |
| packages/ | `@keld/electron` live; other `@keld/*` upcoming |

Crate `AGENTS.md` exists only for real extra invariants (`wv`, `ipc`, `guard`,
`compat`, `runtime`, `core`, `host`, `cli`). MUST NOT add hollow agent files.

## Verification floor

- `just ci` is the sole exact local gate inventory. New behavior MUST have tests.
- Before done, agents MUST run and report actual output for format, workspace clippy
  with warnings denied, and the full workspace test gate. Conditional gates come from
  `.agents/index.md`; MUST NOT report an unrun OS/path as passed.
- Tests, failures, CI, docs/rendering, and dependency changes MUST load their routed
  playbook before selecting a fix or proof.

## Rust, TypeScript, and naming

- Rust lints are workspace-owned. A new `allow` needs inline justification.
- `unsafe` is limited to `keld-wv` backends (and future `keld-ipc` shm), with
  `deny(unsafe_op_in_unsafe_fn)` and a `// SAFETY:` proof. Else requires human review.
- Libraries MUST NOT add `unwrap`/`expect`/`panic!`. Typed errors are hand-written
  `Display` + `KELD-*` fix guidance; test and `keld-cli` top-level invariants may expect.
- Hot kipc/event-loop/guard paths use callbacks/state machines: no async runtime or
  steady-state allocation. Async is cold-tooling only.
- Dependencies are std-first, minimal, workspace-pinned and always a review gate.
- Public Rust items are documented. TypeScript is strict; public APIs MUST NOT use
  `any`; generated packages are never hand-edited; `@keld/electron` never imports
  Electron at runtime.
- Names: crates `keld-*`, libs `keld_*`, npm `@keld/*`, protocol `KI*`. Config names are
  only `keld.config.ts`, `keld.permissions.jsonc`, `keld.build.ts`, `keld.compat.ts`.

## Security, performance, and review gates

- Default-deny is sacred: MUST NOT bypass `keld-guard`. Dev-permissive exists only under
  `keld dev` + recorder; release refuses it.
- Perf claims use attributed reproducible measurements; >5% regression needs a written
  waiver and benchmarks. Budgets: architecture 01 §5.
- Threat models remain in the `keld-guard` and `keld-update` crate documentation.
- PRs list these five review gates (or `none`); human sign-off is required when present:
  `unsafe`, public API, permission model, dependency addition, wire protocol.

## Working invariants

- Tests are contracts. Compat lands a conformance entry first; bugs land a regression
  test before the fix. Anti-flake/test shape belongs to `.agents/testing.md`.
- MUST NOT land `todo!()`/`unimplemented!()`/stubs. Keep one concern per PR.
- Before implementation: grep/reuse the codebase, read the governing spec and nearest
  crate rules, query relevant learnings, inspect the pin-to-`origin/main` delta and open
  work, then load `docs/agents/workflow.md` through `.agents/index.md` before the first
  edit.
- Keld protects four uniques only: prebuilt host; supervised Bun process family with
  zero ambient OS authority per strict-profile principal; kipc; generated host-enforced
  default-deny. MUST NOT invent a fifth.
- A change is architecture only if it changes handle ownership, crash ownership, or
  principal minting. Evidence from frameworks/platform loops is fact, not a template.
- Reuse before adding crates/helpers/files. If Phase 2 can ship without it, or it exists
  only to look complete, YAGNI forbids it. No 100-crate graph or unimplemented trait API.
- Fix causes, not symptoms. MUST NOT swallow faults, retry deterministic failures,
  sleep-sync, hard-code callers, duplicate helpers/policy, widen permissions, weaken
  tests, skip gates, add lint bypasses, or regenerate artifacts from a stale generator.
- When the correct fix is out of scope, stop and name the cause/fix. A disclosed blocker
  is acceptable; a workaround or unverified completion claim is not.
- Nested agent rules add constraints and never silently weaken root. `just agents-md`
  enforces the current nested allowlist and unsafe coverage.
- Workflow/Linear/worktree/OS/PR/CodeRabbit/CI/docs/research details are conditional and
  MUST be loaded through `.agents/index.md`; they MUST NOT be re-copied into this floor.
- Non-obvious reusable facts (>10 minutes saved) go to the relevant-area learning log
  after grep/dedupe; `docs/agents/learnings.md` is evidence, not default context.
- MUST NOT commit secrets or edit `.env*`. Destructive operations and merge authority
  remain explicit; following a workflow never broadens user authorization.
