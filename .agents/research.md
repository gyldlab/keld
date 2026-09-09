# External research playbook

Load this playbook only for a material decision that may depend on current external
evidence.

`docs/research/` is a nested `0monish/keld-research` checkout, not a Keld index entry.
Research edits MUST commit inside it and push with `just research-push` (or nested git
push) in the same turn; on access failure warn plainly. MUST NOT stage it from Keld root.

Hello / installer / RSS competitor fixtures MUST live in
[`gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches) under
`{macos|windows|linux}/<framework>/...`, never under Keld `docs/`/`competitors/`,
at the benches root, or `/tmp`-only. Use the OS actually run. Push directly or open a
fork PR; warn and skip when neither is possible. Published measurements link the
OS-qualified fixture and immutable commit/tag.

## Where prompts live

Agents MUST NOT invent a parallel prompt taxonomy. Copy-ready packs and new research
prompts live in Prompt Tracker (`0monish/prompt-tracker`) under the existing category
tree. Website Deep Research pastes follow that repo’s `docs/05-deep-research-host.md`
and `prompts/SHARED/` chrome. This playbook still owns the escalation trigger below.

## Current-documentation receipt

Before deciding a material claim that depends on current external OS/platform-command,
SDK/API, runtime, or external-tool semantics, use this playbook's receipt. It does not
apply to a pure local refactor whose decision does not rely on external semantics.

1. **Discover.** When Context7 is available, resolve the relevant library and make a
   narrow query before deciding. Record the library ID, query, and retrieval date.
   Context7 is discovery, never the authority.
2. **Confirm.** Confirm every material claim with a current official primary source:
   owning vendor/platform documentation, a release or migration guide, an immutable
   upstream source tag, or a normative standard. Record the URL or immutable source
   reference, applicable version/tag, retrieval date, and exact supported claim.
3. **Handle gaps truthfully.** If Context7 is unavailable or its query fails, record the
   exact failure and continue only when primary confirmation exists. If no relevant
   Context7 library applies, record that reason as not-applicable; primary confirmation
   is still required. If no current, unambiguous primary source is available, leave the
   claim unknown or block the decision; local reproduction may demonstrate behavior but
   cannot turn an unsupported external claim into documented fact.
4. **Leave the record.** Use the `## Current-documentation receipt` template in
   [`coordination.md`](coordination.md) on the relevant Linear decision, OS handoff,
   or branch handoff. Do not place private source text or sensitive queries there.

## Escalation trigger

Agents MUST ask the user to run one copy-ready external-research prompt only when all
of these are true:

1. Local code, tests, specs, history, and available primary sources are insufficient,
   contradictory, inaccessible, or too stale.
2. The decision materially depends on current ecosystem facts, social sentiment,
   unpublished product changes, or cross-source synthesis.
3. The answer could change Keld's design, dependency, migration, roadmap, UX, or public
   claim.

Agents MUST NOT request external research for routine coding, stable API syntax, or a
question answered by local evidence, official documentation, a registry, upstream
releases, or source history. Name the missing evidence and the decision it blocks.

Perplexity, Google Deep Research, X, and Reddit produce leads, not truth. Consequential
claims MUST be verified against local reproduction or a primary source; otherwise
label them anecdotal or unverified. Separate evidence, contradiction, inference, and
uncertainty.

## Diagrams in private research

- A Mermaid diagram under `docs/research/`, or one that synthesizes external evidence,
  MUST also follow [`.agents/docs.md`](docs.md) and the render/report gate in
  [`.agents/testing.md`](testing.md). The nested commit/push rule still applies.
- A diagram is synthesis, not proof. Every decision-bearing node, edge, state transition
  and quantitative label MUST trace to a direct primary source or committed local/raw
  experiment artifact. Otherwise the label itself MUST say `inference`, `proposed` or
  `unknown`; a caption or color legend is not enough to downgrade the claim.
- Copied `turn…` citations, `sandbox:/mnt/data` paths, screenshots without provenance or
  environment context, and model-generated diagrams are leads only. They MUST NOT be
  promoted into a diagram's factual edge or number until the source ledger or executable
  artifact is restored.
- For unfamiliar Mermaid syntax, apply [the current-documentation receipt](#current-documentation-receipt)
  before authoring. Record the official [Mermaid documentation](https://mermaid.js.org/)
  page in the research source ledger; Context7 output is not the cited authority.
- The render report MUST name the exact stable renderer version and actual result. Keep
  generated SVG/PNG/PDF output temporary unless the rendered file is an intentionally
  reviewed research artifact; do not commit generated pictures merely to prove parsing.

## Copy-ready prompt pack

Canonical paste bodies live in Prompt Tracker (`0monish/prompt-tracker`, local clone
typically `keld-agent-prompts`), not in this playbook. Agents MUST copy from that
repo’s category tree (`prompts/NEW/<category>/`) and `prompts/SHARED/` chrome
(`deep-research-chrome.paste.md` for website Deep Research; `branch-linear-handoff.md`
for git/Linear). Website host policy: that repo’s `docs/05-deep-research-host.md`.
Do not keep a parallel prompt pack here.
