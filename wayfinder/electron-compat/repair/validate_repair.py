"""Validate repair-pass output before it touches GitHub. Exit 1 on any blocking defect.
Usage: python3 validate_repair.py <repair_result.json>"""
import json, sys, re, collections
res=json.load(open(sys.argv[1])); nums=json.load(open('publish_state.json')); plan=json.load(open('publish_plan.json')); adopted=json.load(open('adopted_decisions.json'))
UNITS=['F01','F02','F03','F04','F05','F06','F07','F08','F09','X01','X02','X03','X04','X05']
OPEN={'F02-A3','F02-A6','F03-D6','F04-A3','F04-A5','PANEL-D22','PANEL-D23','PANEL-P1','PANEL-P2','PANEL-P3','X01-A5','X04-D3','X05-A4'}
CLOSED={i['key'] for i in plan['issues'] if i.get('close')}|set(adopted)|{'X06-D8','X06-D7'}
published={u:[i['key'] for i in plan['issues'] if i.get('parent_key')==f'{u}-EPIC'] for u in UNITS}
block=[]; warn=[]
rep={r['unit']:r['repair'] for r in res['repaired']}
for u in UNITS:
    if u not in rep: block.append(f'{u}: unit missing from repair output'); continue
    r=rep[u]; keys=[t['key'] for t in r['tickets']]; sup={s['key'] for s in r.get('supersede') or []}
    for k in published[u]:
        if k not in keys and k not in sup: block.append(f'{u}: published ticket {k} silently dropped')
    for k,c in collections.Counter(keys).items():
        if c>1: block.append(f'{u}: duplicate key {k}')
    for k in keys:
        if not re.fullmatch(rf'{u}-T\d+',k): block.append(f'{u}: bad key {k}')
all_keys={t['key'] for r in rep.values() for t in r['tickets']}
RANK={'first-proof':0,'next':1,'parked':2}
MS={t['key']:next((m for m in RANK if m in (t.get('milestone') or '').lower()),'parked') for r in rep.values() for t in r['tickets']}
MS.update({'F02-A3':'next','F02-A6':'parked','F03-D6':'next','F04-A3':'next','F04-A5':'next','PANEL-D22':'first-proof','PANEL-D23':'next','PANEL-P1':'first-proof','PANEL-P2':'first-proof','PANEL-P3':'first-proof','X01-A5':'first-proof','X04-D3':'first-proof','X05-A4':'next','X06-T1':'first-proof'})  # live decision milestone labels, 2026-10-07
for r in rep.values():
    for t in r['tickets']:
        for b in t.get('blocked_by_keys') or []:
            if b in MS and RANK[MS[b]]>RANK[MS[t['key']]]: block.append(f"{t['key']} ({MS[t['key']]}) blocked by {b} ({MS[b]}): milestone inversion")

known=set(nums)|all_keys
graph=collections.defaultdict(set)
RS={'ready-for-agent','ready-for-human','needs-spec','needs-info','needs-triage','triage'}
for u,r in rep.items():
    for t in r['tickets']:
        k=t['key']
        ms=(t.get('milestone') or '').lower()
        if not any(x in ms for x in ('first-proof','next','parked')): block.append(f'{k}: milestone {t.get("milestone")!r}')
        if (t.get('ready_state') or '').lower() not in RS: block.append(f'{k}: ready_state {t.get("ready_state")!r}')
        if (t.get('kind') or '').lower()=='feature' and (t.get('ready_state') or '').lower()=='ready-for-agent': block.append(f'{k}: feature marked ready-for-agent (needs a spec)')
        if not t.get('acceptance_criteria'): block.append(f'{k}: no acceptance criteria')
        if not t.get('negative_controls'): block.append(f'{k}: no negative controls')
        for b in t.get('blocked_by_keys') or []:
            if b==k: block.append(f'{k}: self edge')
            elif b in CLOSED: block.append(f'{k}: blocked by CLOSED decision {b}')
            elif b not in known: block.append(f'{k}: blocked by unknown key {b}')
            else: graph[k].add(b)
        txt=' '.join([t.get('what_to_build',''),' '.join(t.get('acceptance_criteria') or []),t.get('key_interfaces','') or ''])
        if re.search(r'\b(?:crates|packages|src|docs|tests?|scripts|\.github|\.agents)/[\w./-]+|\b[\w-]+\.(?:rs|ts|js|mjs|toml)\b:\d+',txt): warn.append(f'{k}: repo file path or line number in ticket text')
        if re.search(r'quirk[^.]{0,80}(allow|widen|nodeIntegration|contextIsolation)',txt,re.I): warn.append(f'{k}: quirk near authority wording — review')
# cycles
state={}
def dfs(v,stack):
    state[v]=1; stack.append(v)
    for w in graph.get(v,()):
        if state.get(w)==1: block.append('cycle: '+' -> '.join(stack[stack.index(w):]+[w]))
        elif not state.get(w): dfs(w,stack)
    stack.pop(); state[v]=2
for v in list(graph):
    if not state.get(v): dfs(v,[])
new=sorted(all_keys-set(nums)); sup=sorted({s['key'] for r in rep.values() for s in r.get('supersede') or []})
ms=collections.Counter((t.get('milestone') or '').lower() for r in rep.values() for t in r['tickets'])
unap=sum(len(r.get('unapplied_findings') or []) for r in rep.values())
print(f'units {len(rep)}/14 | tickets {len(all_keys)} (new {len(new)}: {new}) | superseded {len(sup)}: {sup}')
print('milestones:',dict(ms),'| unapplied findings:',unap)
print('check:',res.get('checkModel'),(res.get('check') or {}).get('verdict'),'| residual:',len((res.get('check') or {}).get('residual') or []),'| fixed units:',res.get('fixed_units'),'| re-check:',(res.get('recheck') or {}).get('verdict'),len((res.get('recheck') or {}).get('residual') or []))
print(f'BLOCKING ({len(block)}):'); [print('  ',b) for b in block]
print(f'WARN ({len(warn)}):'); [print('  ',w) for w in warn[:40]]
sys.exit(1 if block else 0)
