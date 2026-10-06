export const meta = {
  name: 'electron-compat-repair',
  description: 'Repair pass on Opus: rewrite each unit\'s published epic and tickets so every audit finding, refuter correction and adopted decision is applied; one Fable cross-check (Opus fallback); Opus fix round and re-check for residual defects',
  phases: [
    { title: 'Repair', detail: '14 unit repair agents (Opus)', model: 'opus' },
    { title: 'Cross-check', detail: 'one independent adversarial check (Fable, Opus fallback)', model: 'fable' },
    { title: 'Fix residuals', detail: 'targeted fixes per affected unit (Opus)', model: 'opus' },
    { title: 'Re-check', detail: 'verify the fixes (Opus)', model: 'opus' },
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
const CHECK_SCHEMA = { type: 'object', properties: { residual: { type: 'array', items: { type: 'object', properties: { keys: ARR, problem: STR, fix: STR, severity: STR }, required: ['keys', 'problem', 'fix', 'severity'] } }, edge_cycles: ARR, unresolved_keys: ARR, verdict: STR }, required: ['residual', 'verdict'] }
const UNITS = ['F01','F02','F03','F04','F05','F06','F07','F08','F09','X01','X02','X03','X04','X05']
const OPEN = 'F02-A3, F02-A6, F03-D6, F04-A3, F04-A5, PANEL-D22, PANEL-D23, PANEL-P1, PANEL-P2, PANEL-P3, X01-A5, X04-D3, X05-A4'
const RULES = `RULES:
1. Keep every existing key and its meaning stable; return ALL tickets of the unit (changed or not). New tickets continue the numbering (<unit>-T<next>). A ticket that should no longer exist goes in supersede (it will be closed as not planned), never silently dropped.
2. Split a ticket when the audit says it carries more than one concern or an architecture gate; narrow a ticket to its first-proof slice when the frontier says so and move the remainder to a new ticket with milestone "next" or "parked".
3. milestone ∈ first-proof | next | parked by the YAGNI test (can drawio-desktop on macOS under the explicit legacy profile ship install + activation + open → edit → save → close-with-unsaved-prompt correctly without this ticket?). Do not design parked work in detail.
4. One owner per atom: apply the audit's ownership splits exactly (state in the ticket which other key owns the adjacent atom). Add missing edges, delete edges that are not real gates. Every blocked_by_keys entry must exist: this unit's keys, other units' published keys, or an OPEN decision key (${OPEN}) — never a closed decision.
5. Invariants are not negotiable: no quirk or flag may widen authority; a missing handler is a deny; no runtime-minted grant shape without an approved spec; no second policy owner; no unattributed performance acceptance criterion (move it to a registered metric under X05 or delete it); a feature needs a spec (ready_state "needs-spec"); only scoreboard rows, specs and conformance entries may be "ready-for-agent".
6. Conformance first: an implementation ticket is either blocked by its family's conformance-entry ticket or its FIRST acceptance criterion is "a conformance entry citing the pinned Electron v44.4.5 sentence lands first". Red-until-implemented entries are KEL-74 cells recorded with an explicit expected status asserted by the runner and flipped to pass in the implementing change — never a silently red, skipped or retried test.
7. Ambiguity: rewrite any statement the audit flagged as fact-without-evidence with its true label (INFERENCE / UNKNOWN) or delete it. Name the live Linear status of every owner you cite (Linear is readable in this session; read-only). If an owner is Done, Backlog or absent, say so and name who can act. External predecessors name task-level artifacts (task id, landed SHA or comment id), not parent issues.
8. Acceptance criteria are binary observables; each key criterion has one negative control naming the single mutation that must fail it. No file paths or line numbers in ticket text.
9. epic_body is the corrected epic prose (Scope, Corpus demand, Maturity ladder, Linear owners consumed with live status, Design facts with correct labels, Platform lanes); never_list / out_of_scope / not_yet_specified must ENUMERATE every Electron entity of the family that no ticket covers, so every entity is accounted for.
Return only the structured object; each change_log line says what changed and which finding caused it.`
const repairPrompt = u => `You are the repair owner for unit ${u} of the PUBLISHED KELD Electron compatibility map (gyldlab/keld#391). Read ${B} and ${S}/DOCTRINE.md first; the doctrine is binding.
BOUNDED QUESTION: produce the corrected epic and the corrected full set of tracer tickets for unit ${u}, with every applicable finding applied in the ticket text itself.
EVIDENCE TARGET: your unit bundle ${S}/repair_inputs/${u}.json is the complete list of what you must address — published epic/tickets with issue numbers, critic findings filtered to ${u}, program-wide findings, refuter verdicts from both batches, decision packets, adopted decisions now CLOSED (their resolution text is binding; drop any dependency on them as open questions), panel decisions mentioning ${u}, the unit research, and X06 program-structure corrections. Verify facts against the Keld repo at <keld repo>, live Linear (read-only) and the pinned Electron v44.4.5 docs when a finding turns on a fact.
WRITES: none. STOP: every finding in the bundle is either applied (named in a change_log line) or listed in unapplied_findings with the evidence-backed reason it is wrong.
${RULES}`
const slimOf = list => list.map(r => ({ unit: r.unit, tickets: r.repair.tickets.map(t => ({ key: t.key, title: t.title, kind: t.kind, milestone: t.milestone, ready_state: t.ready_state, blocked_by_keys: t.blocked_by_keys, linear_owner: t.linear_owner, review_gates: t.review_gates, what_to_build: (t.what_to_build || '').slice(0, 600), acceptance_criteria: t.acceptance_criteria })), supersede: r.repair.supersede, unapplied: r.repair.unapplied_findings }))
const checkPrompt = slim => `You are the independent adversarial cross-checker for the REPAIRED KELD Electron compatibility plan. Read ${B} and ${S}/DOCTRINE.md. Default position: the repair left defects. EVIDENCE TARGET: the repaired tickets below, the audit they were meant to satisfy (${S}/batch2_result.json → critic), the published keys (${S}/publish_state.json), and the adopted closed decisions (${S}/adopted_decisions.json). WRITES: none.
Find only what is still wrong ACROSS units after repair: an atom with two owners; a blocked_by key that does not exist or is a closed decision; an edge cycle; an authority-widening quirk or default-allow; an implementation ticket labelled ready-for-agent without a spec; a first-proof ticket that the four-step draw.io workflow does not need, or a needed slice labelled next/parked; a finding marked unapplied whose stated reason is itself unevidenced. For each: keys, problem, the exact fix, severity (high|medium|low). Also list edge cycles and unresolved keys. verdict = "clean" or "defects remain".
REPAIRED PLAN:
${JSON.stringify(slim, null, 1).slice(0, 260000)}`

phase('Repair')
const runRepair = u => agent(repairPrompt(u), { label: `repair:${u}`, phase: 'Repair', schema: REPAIR_SCHEMA, model: 'opus', effort: 'high' }).then(r => r ? ({ unit: u, repair: r }) : null)
let repaired = (await parallel(UNITS.map(u => () => runRepair(u)))).filter(Boolean)
const missing = UNITS.filter(u => !repaired.some(r => r.unit === u))
if (missing.length) {
  log(`Retrying ${missing.length} failed unit(s): ${missing.join(', ')}`)
  repaired = repaired.concat((await parallel(missing.map(u => () => runRepair(u)))).filter(Boolean))
}
log(`Repaired units: ${repaired.length}/${UNITS.length}`)

phase('Cross-check')
let check = await agent(checkPrompt(slimOf(repaired)), { label: 'cross-check:fable', phase: 'Cross-check', schema: CHECK_SCHEMA, model: 'fable', effort: 'high' })
let checkModel = 'fable'
if (!check) { log('Fable cross-check unavailable; falling back to Opus'); checkModel = 'opus'; check = await agent(checkPrompt(slimOf(repaired)), { label: 'cross-check:opus-fallback', phase: 'Cross-check', schema: CHECK_SCHEMA, model: 'opus', effort: 'high' }) }

let fixes = [], recheck = null
const sev = (check && check.residual || []).filter(x => (x.severity || '').toLowerCase() !== 'low')
if (sev.length) {
  phase('Fix residuals')
  const byUnit = {}
  for (const x of sev) {
    const units = [...new Set((x.keys || []).map(k => (k.match(/^(F0\d|X0\d)-/) || [])[1]).filter(Boolean))]
    for (const u of units) (byUnit[u] = byUnit[u] || []).push(x)
  }
  log(`Residual medium/high defects: ${sev.length}; units affected: ${Object.keys(byUnit).join(', ') || 'none (program-level only)'}`)
  fixes = (await parallel(Object.entries(byUnit).map(([u, items]) => () => {
    const cur = repaired.find(r => r.unit === u)
    if (!cur) return null
    return agent(`You are fixing residual defects in the REPAIRED unit ${u} of the KELD Electron compatibility map. Read ${B} and ${S}/DOCTRINE.md. The independent cross-checker found these defects that touch ${u}:
${JSON.stringify(items, null, 1)}
Apply each fix to the repaired unit below and return the COMPLETE corrected unit in the same schema (all tickets, epic, supersede, unapplied_findings, change logs extended with one line per fix). If a defect is wrong, leave the text unchanged and add it to unapplied_findings with the evidence. Bundle for reference: ${S}/repair_inputs/${u}.json.
${RULES}
CURRENT REPAIRED UNIT:
${JSON.stringify(cur.repair, null, 1).slice(0, 200000)}`, { label: `fix:${u}`, phase: 'Fix residuals', schema: REPAIR_SCHEMA, model: 'opus', effort: 'high' }).then(r => r ? ({ unit: u, repair: r }) : null)
  }))).filter(Boolean)
  for (const f of fixes) { const i = repaired.findIndex(r => r.unit === f.unit); if (i >= 0) repaired[i] = f }
  phase('Re-check')
  recheck = await agent(checkPrompt(slimOf(repaired)), { label: 're-check:opus', phase: 'Re-check', schema: CHECK_SCHEMA, model: 'opus', effort: 'high' })
}
return { repaired, check, checkModel, fixed_units: fixes.map(f => f.unit), recheck }
