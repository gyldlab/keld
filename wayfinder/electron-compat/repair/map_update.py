"""Rewrite map #391 from live GitHub state after the repair pass. Usage: python3 map_update.py [--dry-run]"""
import json, subprocess, sys, re
dry='--dry-run' in sys.argv; REPO='gyldlab/keld'; MAP=391
nums=json.load(open('publish_state.json')); adopted=json.load(open('adopted_decisions.json')); res=json.load(open('batch2_result.json'))
plan=json.load(open('publish_plan.json')); pk={i['key']:i for i in plan['issues']}
num2key={v['number']:k for k,v in nums.items()}
def gh(args, inp=None):
    p=subprocess.run(['gh','api']+args,input=inp,capture_output=True,text=True)
    if p.returncode: raise SystemExit(p.stderr[:300])
    return json.loads(p.stdout) if p.stdout.strip() else {}
rows=[]
for page in (1,2,3):
    j=gh([f'repos/{REPO}/issues?labels=electron-compat&state=all&per_page=100&page={page}']); rows+=[r for r in j if 'pull_request' not in r]
    if len(j)<100: break
by={r['number']:r for r in rows}
def link(num): r=by[num]; return f"[{r['title']}](https://github.com/{REPO}/issues/{num})"
def labels(r): return [l['name'] for l in r['labels']]
def wtype(r): return next((l.split(':')[1] for l in labels(r) if l.startswith('wayfinder:') and l!='wayfinder:map'),None)
def mile(r): return next((l.split(':')[1] for l in labels(r) if l.startswith('milestone:')),'—')
body=gh([f'repos/{REPO}/issues/{MAP}'])['body']
parts=re.split(r'(?m)^## ',body); head=parts[0]; sec={}; order=[]
for p in parts[1:]:
    t,_,rest=p.partition('\n'); sec[t.strip()]=rest; order.append(t.strip())
# ---- Notes: corrected operating loop + entry condition + pull order + capacity + artifacts
notes=sec['Notes']
loop_new=("- **Operating loop (corrected 2026-10-07).** Frontier = open, unblocked, unassigned sub-issues of this map and of its epics, pulled in milestone order "
 "`milestone:first-proof` → `milestone:next` → `milestone:parked`; `compat:tier-N` labels are the architecture 04 §4 taxonomy, not pull order. "
 "Decision, research and prototype tickets write only to scratchpads and the research branch and are claimed by GitHub self-assignment; resolve one decision ticket per session "
 "(research may run in parallel), record the resolution as a comment, close it, and add one line below. "
 "Any ticket that writes to the Keld repository, conformance entries included, is claimed with the `docs/agents/workflow.md` `## Agent claim` block on its owning Linear issue "
 "(earliest `createdAt` wins); the GitHub assignee only mirrors that claim. A repo-writing ticket with no aligned Linear issue uses its GitHub issue as the tracker of record. `needs-spec` tickets first produce the spec PR for human approval.")
notes=re.sub(r'(?m)^- \*\*Operating loop\.\*\*.*$',loop_new.replace('\\','\\\\'),notes)
extra=(f"- **Entry condition (decided 2026-10-07).** {link(nums['X06-D8']['number'])}: KEL-102/T3 is passed and landed (`66ccbbc`, terminal artifact `cde25f5e`), so it blocks nothing; "
 "compat lanes consume KEL-142 (Done); repo-landing work needs a tracker of record (see the next note), a conformance entry first, and an approved spec where behaviour is added.\n"
 f"- **Tracker of record (owner decision 2026-10-07, {link(nums['X06-D7']['number'])}).** Align each repo-writing ticket to the nearest live Linear issue whose scope covers it; that issue is the tracker of record and holds the claim. Otherwise the GitHub issue is the tracker of record (claim as an `## Agent claim` comment, earliest wins) and no Linear issue is created, because the workspace is at its Free-plan cap (275 non-archived issues against 250). Every GitHub ticket is linked from the nearest live Linear issue, titled with its number and tracker of record. Each ticket body names both. The repository instructions record this rule through #520 (`docs/agents/workflow.md` § Tracker issue and `.agents/coordination.md` § GitHub tracker issue: push-access comments only, first-match states, nothing private on the public issue); until that change merges, cite #517.\n"
 "- **Ticket format and labels (mattpocock `to-tickets` and `triage`, applied 2026-10-07).** Tracer tickets use the to-tickets body (Parent / What to build / Acceptance criteria / Blocked by) plus Keld sections (negative controls, ownership and gates, change log). "
 "Each carries one category role (`enhancement`) and exactly one state role: `ready-for-agent` (fully specified; the latest **Agent Brief** comment is the contract), `ready-for-human` (needs an OS/device observation or an owner judgment; its brief says why), "
 "`needs-info` (waits on an open decision ticket; its **Triage Notes** comment lists what is still needed), `triage` (needs triage) or `wontfix` (superseded, closed as not planned). "
 "`needs-spec` is a Keld modifier, not a state: the first deliverable is a `docs/agents/spec-template.md` spec PR stopped for human approval. Decision tickets keep the wayfinder roles (`wayfinder:research|grilling|prototype|task`) and are claimed by self-assignment. "
 "New tickets were published in dependency order with native blocked-by links; every AI triage comment starts with the triage-skill disclaimer.\n")
notes=notes.replace('- **Artifacts.**',extra+'- **Artifacts.**',1)
notes=re.sub(r'(?m)^- \*\*Artifacts\.\*\*.*$',("- **Artifacts.** Research branch `research/electron-compat-map` (never merged) holds the per-member compatibility matrix (`compat-matrix.tsv`), "
 "per-unit research notes with refuter verdicts, corpus demand scans, the perspective-panel rounds, the execution doctrine (`DOCTRINE.md`), the doctrine-audit results and adopted decisions (`batch2/`), "
 "and four judged single-file logic prototypes (`prototypes/`: window lifecycle, IPC bridge, migrate report, compat-matrix cell; open by double-click)."),notes)
sec['Notes']=notes
# ---- Decisions so far (all closed decision tickets, live)
closed=[r for r in rows if r['state']=='closed' and wtype(r)]
def gist(r):
    k=num2key.get(r['number'],'')
    if k in adopted: g=adopted[k]['resolution']
    elif k=='X06-D8': g='KEL-102/T3 landed; compat lanes consume KEL-142; repo-landing work needs a live Linear owner, conformance entry first, approved spec.'
    elif r.get('state_reason')=='not_planned': g='closed as not planned'
    else: g=(pk.get(k,{}).get('comments') or [''])[0]
    g=re.sub(r'(?s)^>.*?\n\n','',g); g=re.sub(r'(?m)^#+ .*$','',g); g=' '.join(g.split())
    m=re.match(r'(.{40,260}?[.;])(\s|$)',g); return (m.group(1) if m else g[:260])
sec['Decisions so far']='\n'+'\n'.join(f"- {link(r['number'])}: {gist(r)}" for r in sorted(closed,key=lambda r:r['number']))+'\n\n'
# ---- Open decision tickets (live)
opened=[r for r in rows if r['state']=='open' and wtype(r)]
sec['Open decision tickets (frontier and blocked)']='\n'+'\n'.join(f"- {link(r['number'])} — `{wtype(r)}`, `milestone:{mile(r)}`" for r in sorted(opened,key=lambda r:(['first-proof','next','parked'].index(mile(r)) if mile(r) in ('first-proof','next','parked') else 3, r['number'])))+'\n\n'
# ---- First-proof frontier (from the audit, live-filtered)
fr=[]
for x in res['critic'].get('first_proof_frontier') or []:
    k=x['key'].split()[0]
    if k in nums and by.get(nums[k]['number'],{}).get('state')=='open':
        fr.append(f"{len(fr)+1}. {link(nums[k]['number'])} — {x.get('why_now','').split('. ')[0].rstrip('.')}.")
frontier='\n'+'\n'.join(fr)+'\n\nOrdered by the doctrine audit (full rationale in its summary comment on this map); closed decisions were dropped from the list.\n\n'
# ---- Epics (live titles)
epics=[r for r in rows if 'epic' in labels(r) and r['number']!=MAP]
sec['Epics (implementation tracks; sub-issues carry the tracer bullets)']='\n'+'\n'.join(f"- {link(r['number'])}" for r in sorted(epics,key=lambda r:r['number']))+'\n\n'
# ---- Kill-condition factual fix
kc=sec['Kill conditions (test these first)']
kc=kc.replace("draw.io's single rendererReq channel is enumerable; Zettlr's generic window.ipc is not",
 "draw.io's main registers 25 distinct channel literals (27 registrations) while its preload forwards caller-chosen channel names, so grants are derived from the main-side registrations; Zettlr's generic window.ipc dispatcher is not statically enumerable")
sec['Kill conditions (test these first)']=kc
# ---- Out of scope: parked tracker-config adoption
oos=sec['Out of scope']
line="- Adopting the mattpocock skills tracker config inside the repo (`docs/agents/issue-tracker.md`, triage labels): parked. It is an agent-instruction change that needs the `.agents/instructions.md` protocol (budget-neutral router edit, change record, evals); the first proof does not need it.\n"
if 'mattpocock skills tracker config' not in oos: oos=line+oos
sec['Out of scope']=oos
# ---- reassemble: insert frontier after Open decision tickets
out=head
for t in order:
    out+='## '+t+'\n'+sec[t]
    if t=='Open decision tickets (frontier and blocked)': out+='## First-proof frontier (drawio-desktop on macOS, explicit legacy profile)\n'+frontier
out=re.sub(r'(?m)^## First-proof frontier[^\n]*\n(?:(?!^## ).*\n)*?(?=^## First-proof frontier)','',out)  # drop an older copy if re-run
print('map body chars:',len(out),'| decisions so far:',len(closed),'| open decisions:',len(opened),'| frontier items:',len(fr),'| epics:',len(epics))
if len(out)>65000: raise SystemExit('map body too long')
if dry: open('map_preview.md','w').write(out); print('preview written'); sys.exit(0)
gh(['-X','PATCH',f'repos/{REPO}/issues/{MAP}','--input','-'],inp=json.dumps({'body':out})); print('map #391 updated')
