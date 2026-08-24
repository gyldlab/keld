# External research playbook

Load this playbook only for a material decision that may depend on current external
evidence.

Edits under `docs/research/` MUST follow root `AGENTS.md` § Private research (nested
`keld-research` checkout; `just research-push` same turn; never stage into Keld).

Hello / installer / RSS competitor fixtures MUST live in
[`gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches) under
`{macos|windows|linux}/<framework>/...` per root `AGENTS.md` § Public benches —
never under Keld `docs/` or `competitors/`, and never as OS-agnostic dumps at the
`keld-benches` repo root.

## Where prompts live

Agents MUST NOT invent a parallel prompt taxonomy. Copy-ready packs and new research
prompts live in Prompt Tracker (`0monish/prompt-tracker`) under the existing category
tree. Website Deep Research pastes follow that repo’s `docs/05-deep-research-host.md`
and `prompts/SHARED/` chrome. This playbook still owns the escalation trigger below.

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
  MUST also follow root `AGENTS.md` § Documentation diagrams and the render/report gate
  in [`.agents/testing.md`](testing.md). The nested research commit/push rule still
  applies to its Markdown source.
- A diagram is synthesis, not proof. Every decision-bearing node, edge, state transition
  and quantitative label MUST trace to a direct primary source or committed local/raw
  experiment artifact. Otherwise the label itself MUST say `inference`, `proposed` or
  `unknown`; a caption or color legend is not enough to downgrade the claim.
- Copied `turn…` citations, `sandbox:/mnt/data` paths, screenshots without provenance or
  environment context, and model-generated diagrams are leads only. They MUST NOT be
  promoted into a diagram's factual edge or number until the source ledger or executable
  artifact is restored.
- For unfamiliar Mermaid syntax, use Context7 when available to locate current material,
  then confirm against [official Mermaid documentation](https://mermaid.js.org/) before authoring.
  Record the official page used in the research source ledger; Context7 output is not
  the cited authority.
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
