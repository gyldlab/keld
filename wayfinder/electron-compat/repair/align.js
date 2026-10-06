export const meta = {
  name: 'electron-compat-linear-alignment',
  description: 'Align every repaired epic, ticket and open decision of the Electron compat map to the nearest live Linear issue (Opus, read-only Linear): tracker of record + Linear reference',
  phases: [{ title: 'Align', detail: 'one Opus agent per unit plus one for program-level decisions', model: 'opus' }],
}
const S = args.scratch, B = `${S}/briefing.md`
const STR = { type: 'string' }
const SCHEMA = { type: 'object', properties: { unit: STR, items: { type: 'array', items: { type: 'object', properties: {
  key: STR, kind: STR, tracker: { type: 'string', enum: ['linear', 'github'] }, linear_issue: STR, linear_status: STR, evidence: STR, reason: STR },
  required: ['key', 'kind', 'tracker', 'linear_issue', 'linear_status', 'evidence', 'reason'] } } }, required: ['unit', 'items'] }
const RULE = `CONTEXT: #517 (X06-D7) is DECIDED and closed — ticket owner fields that say MISSING, needs-owner, 'route through #517' or '#517 option C' predate that decision; treat them as leads only. OWNER DECISION (2026-10-07, binding): align each work item to the nearest EXISTING Linear issue (team KELD). If a live Linear issue's own description/acceptance explicitly covers this item's work and its non-goals do not exclude it, that Linear issue is the tracker of record (tracker "linear"). Otherwise the GitHub issue is the tracker of record (tracker "github") and NO Linear issue is created; you still name the nearest live Linear issue an agent working on this surface would open, so a reference link can be placed there.
RULES: (1) Read the candidate Linear issues NOW with the Linear tools (read-only: get_issue with includeRelations, list_comments when scope is unclear). Never write to Linear or GitHub. (2) "Live" = status not Done, Canceled or Duplicate; Backlog, Todo and In Progress count as live. Never return a non-live issue. (3) tracker "linear" needs a quoted sentence from that issue's description or a recorded scope comment that covers the item; a merely related issue is "github". Parent/program issues (KEL-127, KEL-177) are never the tracker of record for a work item; use them only as the fallback reference (KEL-127). (4) Decision, research and prototype tickets and epics are always tracker "github". (5) linear_status is the live status you fetched. (6) evidence quotes the covering sentence (tracker linear) or says why no live issue covers it and why the named reference is nearest (tracker github).`
const UNITS = ['F01','F02','F03','F04','F05','F06','F07','F08','F09','X01','X02','X03','X04','X05']
phase('Align')
const out = (await parallel(UNITS.map(u => () => agent(`You align unit ${u} of the KELD Electron compatibility map (gyldlab/keld#391) to Linear. Read ${B} first.
${RULE}
ITEMS: the epic ${u}-EPIC, every ticket in ${S}/repair_result.json → repaired[] where unit == "${u}" → repair.tickets (use title, what_to_build, linear_owner and blocked_by_external as leads only; verify against live Linear), and the open decision tickets of this unit listed in ${S}/open_decisions_by_group.json under "${u}" (skip keys that are closed per ${S}/adopted_decisions.json).
Return one item per key with kind = epic | ticket | decision.`, { label: `align:${u}`, phase: 'Align', schema: SCHEMA, model: 'opus', effort: 'high' }))
  .concat([() => agent(`You align the program-level open decision tickets of the KELD Electron compatibility map (gyldlab/keld#391) to Linear. Read ${B} first.
${RULE}
ITEMS: PANEL-P1 (link-drain gate for a worker-owned single-link Bun kipc transport), PANEL-P2 (close/quit veto oracle against Electron 44.4.5 on AppKit), PANEL-P3 (draw.io boot and authority trace under Bun), PANEL-D22 (Electron main-process re-entrancy during a Cocoa modal dialog), PANEL-D23 (quit sequencing against per-window close vetoes). Their bodies are in ${S}/publish_plan.json (issues[].key). All are kind "decision", tracker "github"; your job is the nearest live Linear reference for each, with evidence.`, { label: 'align:PANEL', phase: 'Align', schema: SCHEMA, model: 'opus', effort: 'high' })]))).filter(Boolean)
return { alignment: out }
