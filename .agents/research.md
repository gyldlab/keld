# External research playbook

Load this playbook only for a material decision that may depend on current external
evidence.

Edits under `docs/research/` MUST follow root `AGENTS.md` § Private research (nested
`keld-research` checkout; `just research-push` same turn; never stage into Keld).

Hello / installer / RSS competitor fixtures MUST live in
[`gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches) per root `AGENTS.md`
§ Public benches — never under Keld `docs/` or `competitors/`.

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

## Copy-ready prompt pack

### Perplexity — focused source discovery

> Investigate **[QUESTION]** for **[KELD DECISION]** during **[DATE RANGE]**. Prefer
> primary sources: official docs, release notes, repositories, issue trackers, talks,
> and maintainer posts. Cite every material claim with a direct URL, owner/author,
> publication date, access date, and short exact quote. Include sections named
> **Contradictions** and **Confidence and unverified claims**. Exclude SEO summaries
> except as leads, and do not recommend a decision from unverified claims.

### Google Deep Research — cross-source comparison

> Compare **[OPTIONS]** for **[KELD USE CASE]** during **[DATE RANGE]**, testing
> **[HYPOTHESES]**. Cover maintenance, security, performance, platform support, known
> failures, and migration cost. For every material claim cite a primary source with
> direct URL, publisher/author, publication date, access date, and exact supporting
> passage. Separate verified facts, vendor claims, practitioner reports, and inference.
> Include **Contradictions** and **Confidence and unverified claims** sections.

### X — maintainer and practitioner leads

> Search X for **[TOPIC/VERSION]** during **[DATE RANGE]**, prioritizing maintainers,
> release authors, and firsthand reproducible reports. Return each permalink, author
> and role, date, exact quote, linked primary artifact, and whether it is firsthand,
> hearsay, or promotion. Sample positive and negative reports; do not infer consensus
> from engagement. Include **Contradictions** and **Confidence and unverified claims**,
> and identify the official source or local reproduction needed to verify each lead.

### Reddit — failures and sentiment sampling

> Sample **[TOPIC/VERSION]** in **[COMMUNITIES]** during **[DATE RANGE]**. Focus on
> repeated concrete failures, migration pain, workarounds, and adoption or exit reasons.
> Return thread/comment permalinks, community, date, exact quote, environment/version,
> reproducibility details, and count of independent reports. Require primary-source
> citations for technical claims where available. Include contrary cases, sampling
> limits, **Contradictions**, and **Confidence and unverified claims**; mark every claim
> that still needs official-source verification or local reproduction.
