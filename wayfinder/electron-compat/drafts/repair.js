export const meta = {
  name: 'electron-compat-repair',
  description: 'Repair pass: rewrite each unit\'s published tickets and epic so every doctrine-audit finding, refuter correction and adopted decision is applied in the issue body',
  phases: [
    { title: 'Repair', detail: 'one agent per unit applies audit findings, refuter corrections and adopted decisions' },
    { title: 'Cross-check', detail: 'one adversarial pass over all repaired units for residual duplicates, edges and invariant breaches' },
  ],
}
const S = args.scratch, B = `${S}/briefing.md`
const STR = { type: 'string' }, ARR = { type: 'array', items: STR }
const TICKET = { type: 'object', properties: { key: STR, title: STR, kind: STR, tier: STR, maturity_target: STR, milestone: STR, ready_state: STR, size: STR,
  what_to_build: STR, acceptance_criteria: ARR, negative_controls: ARR, blocked_by_keys: ARR, blocked_by_external: ARR, linear_owner: STR, review_gates: ARR, platforms: STR,
  electron_members_covered: ARR, current_behavior: STR, key_interfaces: STR, out_of_scope: STR, notes: STR, change_log: ARR },
  required: ['key', 'title', 'kind', 'milestone', 'ready_state', 'what_to_build', 'acceptance_criteria', 'negative_controls', 'blocked_by_keys', 'linear_owner', 'review_gates', 'change_log'] }
const REPAIR_SCHEMA = { type: 'object', properties: { unit: STR, epic_title: STR, epic_body: STR, never_list: ARR, out_of_scope: ARR, not_yet_specified: ARR,
  tickets: { type: 'array', items: TICKET },
  supersede: { type: 'array', items: { type: 'object', properties: { key: STR, reason: STR, replaced_by: STR }, required: ['key', 'reason'] } },
  unapplied_findings: { type: 'array', items: { type: 'object', properties: { finding: STR, reason: STR }, required: ['finding', 'reason'] } },
  epic_change_log: ARR }, required: ['unit', 'epic_title', 'epic_body', 'tickets', 'unapplied_findings', 'epic_change_log'] }
const CHECK_SCHEMA = { type: 'object', properties: { residual: { type: 'array', items: { type: 'object', properties: { keys: ARR, problem: STR, fix: STR, severity: STR }, required: ['keys', 'problem', 'fix'] } }, edge_cycles: ARR, unresolved_keys: ARR, verdict: STR }, required: ['residual', 'verdict'] }
const UNITS = ['F01','F02','F03','F04','F05','F06','F07','F08','F09','X01','X02','X03','X04','X05']
phase('Repair')
const repaired = (await parallel(UNITS.map(u => () => agent(`You are the repair owner for unit ${u} of the PUBLISHED KELD Electron compatibility map (gyldlab/keld#391). Read ${B} and ${S}/DOCTRINE.md first; the doctrine is binding.
BOUNDED QUESTION: produce the corrected epic and the corrected full set of tracer tickets for unit ${u}, with every applicable finding applied in the ticket text itself. EVIDENCE TARGET (read these, do not trust summaries):
- ${S}/swarm.json → units[] where unit == "${u}" → .tickets (the structured tickets and epic as published: keys ${u}-EPIC, ${u}-T<n>) and .research / .verdicts.
- ${S}/publish_plan.json (published bodies; issue numbers in ${S}/publish_state.json).
- ${S}/batch2_result.json → critic (take EVERY entry in coverage_gaps, duplicates, edge_problems, invariant_violations, quality_problems, yagni, ambiguity_laundering, hidden_coupling, first_proof_frontier whose key/keys/suggested_unit names ${u}); verdicts[] for unit ${u} (corrections); packets (decision packets for this unit's decision keys and any PANEL packet that names a ${u} ticket).
- ${S}/adopted_decisions.json — decisions now CLOSED as decided or owner-decided-under-delegation; treat their resolution text as binding and remove any dependency on them as open questions (keep an edge only where a ticket still needs a not-yet-existing artifact).
- Live Linear owner state (read-only) and the Keld repo at <keld repo> when a finding turns on a fact; pinned Electron v44.4.5 docs for semantics.
WRITES: none. STOP: every finding for ${u} is either applied (named in a change_log line) or listed in unapplied_findings with the evidence-backed reason it is wrong.
RULES:
1. Keep every existing key and its meaning stable; return ALL tickets of the unit (changed or not). New tickets continue the numbering (${u}-T<next>). A ticket that should no longer exist goes in supersede (it will be closed as not planned), never silently dropped.
2. Split a ticket when the audit says it carries more than one concern or an architecture gate; narrow a ticket to its first-proof slice when the frontier says so and move the remainder to a new ticket with milestone "next" or "parked".
3. milestone ∈ first-proof | next | parked by the YAGNI test (can drawio-desktop on macOS under the explicit legacy profile ship install + activation + open → edit → save → close-with-unsaved-prompt correctly without this ticket?). Do not design parked work in detail.
4. One owner per atom: apply the audit's ownership splits exactly (state in the ticket which other key owns the adjacent atom). Add the missing edges, delete edges that are not real gates; every blocked_by_keys entry must be a key that exists (this unit's keys, other units' published keys, open decision keys ${'F02-A3, F02-A6, F03-D6, F04-A3, F04-A5, PANEL-D22, PANEL-D23, PANEL-P1, PANEL-P2, PANEL-P3, X01-A5, X04-D3, X05-A4'}) — never a closed decision.
5. Invariants are not negotiable: no quirk or flag may widen authority; a missing handler is a deny; no runtime-minted grant shape without an approved spec; no second policy owner; no unattributed performance acceptance criterion (move it to a registered metric under X05 or delete it); a feature needs a spec (ready_state "needs-spec"), and only rows/specs/conformance entries may be "ready-for-agent".
6. Conformance first: an implementation ticket either is blocked by its family's conformance-entry ticket or its FIRST acceptance criterion is "a conformance entry citing the pinned Electron v44.4.5 sentence lands first". Red-until-implemented entries are KEL-74 cells recorded with an explicit expected status asserted by the runner and flipped to pass in the implementing change — never a silently red, skipped or retried test.
7. Ambiguity: rewrite any statement the audit flagged as fact-without-evidence with its true label (INFERENCE / UNKNOWN) or delete it. Name live Linear status for every owner you cite (fetched 2026-10-06); if an owner is Done/Backlog/absent say so and name who can act.
8. Acceptance criteria are binary observables; each key criterion has one negative control naming the single mutation that must fail it. No file paths or line numbers in ticket text.
9. epic_body: the corrected epic prose (Scope, Corpus demand, Maturity ladder, Linear owners consumed with live status, Design facts with correct labels, Platform lanes), plus never_list / out_of_scope / not_yet_specified arrays that ENUMERATE every Electron entity of this family that no ticket covers (the audit's coverage gaps), so all entities of the family are accounted for.
Return only the structured object; change_log lines say what changed and which finding caused it.`, { label: `repair:${u}`, phase: 'Repair', schema: REPAIR_SCHEMA, effort: 'high' }).then(r => r ? ({ unit: u, repair: r }) : null)))).filter(Boolean)
log(`Repaired units: ${repaired.length}/${UNITS.length}`)
phase('Cross-check')
const slim = repaired.map(r => ({ unit: r.unit, tickets: r.repair.tickets.map(t => ({ key: t.key, title: t.title, kind: t.kind, milestone: t.milestone, ready_state: t.ready_state, blocked_by_keys: t.blocked_by_keys, linear_owner: t.linear_owner, review_gates: t.review_gates, what_to_build: (t.what_to_build || '').slice(0, 700), acceptance_criteria: t.acceptance_criteria })), supersede: r.repair.supersede, unapplied: r.repair.unapplied_findings }))
const check = await agent(`You are the adversarial cross-checker for the REPAIRED KELD Electron compatibility plan. Read ${B} and ${S}/DOCTRINE.md. Default position: the repair left defects. EVIDENCE TARGET: the repaired tickets below, the audit they were meant to satisfy (${S}/batch2_result.json → critic) and the published keys (${S}/publish_state.json). WRITES: none.
Find only what is still wrong ACROSS units after repair: an atom with two owners; a blocked_by key that does not exist or is a closed decision; an edge cycle; an authority-widening quirk or default-allow; an implementation ticket labelled ready-for-agent without a spec; a first-proof ticket that the four-step draw.io workflow does not need, or a needed slice labelled next/parked; a finding marked unapplied whose stated reason is itself unevidenced. For each: keys, problem, the exact fix, severity. Also list edge cycles and unresolved keys. verdict = "clean" or "defects remain".
REPAIRED PLAN:
${JSON.stringify(slim, null, 1).slice(0, 260000)}`, { label: 'cross-check:repaired-plan', phase: 'Cross-check', schema: CHECK_SCHEMA, effort: 'high' })
return { repaired, check }
