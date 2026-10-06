export const meta = {
  name: 'electron-compat-final-fix',
  description: 'Fix the 16 cross-unit residual defects the Opus re-check confirmed in the repaired Electron compat plan; each cluster agent returns field-level patch operations (Opus)',
  phases: [{ title: 'Fix', detail: 'one Opus agent per residual cluster', model: 'opus' }],
}
const S = args.scratch
const STR = { type: 'string' }, ARR = { type: 'array', items: STR }
const TICKET = { type: 'object', properties: { key: STR, title: STR, kind: STR, tier: STR, maturity_target: STR, milestone: STR, ready_state: STR, size: STR,
  what_to_build: STR, acceptance_criteria: ARR, negative_controls: ARR, blocked_by_keys: ARR, blocked_by_external: ARR, linear_owner: STR, review_gates: ARR, platforms: STR,
  electron_members_covered: ARR, current_behavior: STR, key_interfaces: STR, out_of_scope: STR, notes: STR, change_log: ARR },
  required: ['key', 'title', 'kind', 'milestone', 'ready_state', 'what_to_build', 'acceptance_criteria', 'negative_controls', 'blocked_by_keys', 'linear_owner', 'review_gates', 'change_log'] }
const OP = { type: 'object', properties: {
  key: STR, field: STR, op: { type: 'string', enum: ['set', 'add', 'remove', 'replace'] },
  value: STR, items: ARR, old: STR, new: STR, why: STR },
  required: ['key', 'field', 'op', 'why'] }
const SCHEMA = { type: 'object', properties: { cluster: STR, ops: { type: 'array', items: OP },
  new_tickets: { type: 'array', items: { type: 'object', properties: { unit: STR, ticket: TICKET }, required: ['unit', 'ticket'] } },
  supersede: { type: 'array', items: { type: 'object', properties: { key: STR, reason: STR, replaced_by: STR }, required: ['key', 'reason'] } },
  label_changes: { type: 'array', items: { type: 'object', properties: { key: STR, add: ARR, remove: ARR, why: STR }, required: ['key', 'why'] } },
  rejected: { type: 'array', items: { type: 'object', properties: { residual: STR, reason: STR }, required: ['residual', 'reason'] } } },
  required: ['cluster', 'ops', 'new_tickets', 'supersede', 'label_changes', 'rejected'] }
const RULES = `OUTPUT CONTRACT: return field-level patch operations, never whole tickets.
- String fields (title, what_to_build, ready_state, milestone, linear_owner, notes, current_behavior, key_interfaces, out_of_scope): op "set" with the complete new value, or op "replace" with an exact "old" substring that occurs once and its "new" text.
- List fields (acceptance_criteria, negative_controls, blocked_by_keys, blocked_by_external, review_gates, electron_members_covered): op "add" with items, op "remove" with exact existing items, or op "replace" with an exact old item and its new item.
- Every op carries "why", naming the residual it fixes. The orchestrator appends a change_log line from it, so do not add change_log ops.
- New tickets continue the unit's numbering and use the full ticket schema; superseded tickets go in supersede with replaced_by.
- Label changes on open decision tickets, such as a milestone relabel, go in label_changes.
RULES:
- Change only what the residual requires.
- Acceptance criteria stay binary and observable. Each key criterion keeps one negative control that names the single mutation that must fail it.
- No file paths or line numbers in ticket text.
- A feature keeps ready_state "needs-spec".
- ready_state is one of ready-for-agent | ready-for-human | needs-spec | needs-info | needs-triage.
- A milestone "parked" ticket is needs-triage.
- Every blocked_by key must exist (published or in the plan), must not be a closed decision, and must not create a cycle.
- The tracker-of-record rule (owner decision #517) applies: a ticket with no covering live Linear issue has its own GitHub issue as tracker of record, and its Linear reference is the nearest live issue from ${S}/alignment.json.
If a residual is wrong, put it in rejected with the evidence. Read before editing: ${S}/briefing.md, and the current tickets in ${S}/repair_final_pre.json (repaired[].repair.tickets). Read-only: never write to GitHub, Linear or the repository.`
const clusters = args.clusters
phase('Fix')
const out = (await parallel(clusters.map(c => () => agent(`You fix residual cluster ${c.cluster} of the repaired KELD Electron compatibility plan (gyldlab/keld#391).
AFFECTED KEYS: ${c.keys.join(', ')}. Read each key's current ticket in ${S}/repair_final_pre.json before proposing ops.
RESIDUAL DEFECTS: read the entry with "cluster" == "${c.cluster}" in ${S}/final_fix_clusters.json → residuals[]. They were independently re-checked, and each has a recommended fix you may refine but not weaken.
${RULES}`, { label: `fix:${c.cluster}`, phase: 'Fix', schema: SCHEMA, model: 'opus', effort: 'high' })))).filter(Boolean)
return { fixes: out }
