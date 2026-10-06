"""Deterministic post-repair normalization: owner decision #517 (X06-D7) and triage-state hygiene.
Usage: python3 normalize_repair.py <repair_result.json> <out.json>"""
import json, sys, re
res=json.load(open(sys.argv[1])); adopted=json.load(open('adopted_decisions.json')); plan=json.load(open('publish_plan.json'))
CLOSED={i['key'] for i in plan['issues'] if i.get('close')}|set(adopted)|{'X06-D8','X06-D7'}
NOTE='#517 (X06-D7) decided 2026-10-07: when no live Linear issue covers the work, the GitHub issue is the tracker of record'
SUBS=[(r'\(X06-D7, open\)','(X06-D7, decided 2026-10-07)'),
      (r'Route through #517 \(X06-D7\)(; the repository owner @0monish decides)?\.', NOTE+'.'),
      (r'\broutes to #517\b','falls back to the #517 rule (GitHub tracker of record)'),
      (r'\broute to #517\b','fall back to the #517 rule (GitHub tracker of record)'),
      (r'\bBoth route to #517\b','Both fall back to the #517 rule (GitHub tracker of record)'),
      (r'under #517 option C','under the #517 alignment rule'),
      (r'\(1\) #517 names an owner;','(1) done 2026-10-07: #517 decided the tracker of record;'),
      (r'under option C of #517 \(X06-D7\)','under the #517 alignment rule (X06-D7, decided 2026-10-07)'),
      (r'Both fall back to the #517 rule \(GitHub tracker of record\) \(X06-D7, decided 2026-10-07\)\. The remaining owner question there is whether to archive Done issues \(B\) or upgrade the workspace \(A\)\. The repository owner @0monish can act on it\.',
       'Their GitHub issues are the tracker of record under #517 (X06-D7, decided 2026-10-07): the owner decided that new work is tracked on GitHub instead of creating Linear issues, and is referenced from the nearest live Linear issue.')]
def fix(s):
    if not isinstance(s,str): return s
    for a,b in SUBS: s=re.sub(a,b,s)
    return s
log=[]
for r in res['repaired']:
    rp=r['repair']
    for f in ('epic_body',): rp[f]=fix(rp.get(f,''))
    for f in ('never_list','out_of_scope','not_yet_specified','epic_change_log'): rp[f]=[fix(x) for x in rp.get(f) or []]
    for t in rp['tickets']:
        for f,v in list(t.items()):
            if isinstance(v,str): t[f]=fix(v)
            elif isinstance(v,list): t[f]=[fix(x) if isinstance(x,str) else x for x in v]
        drop=[b for b in t.get('blocked_by_keys') or [] if b in CLOSED]
        if drop:
            t['blocked_by_keys']=[b for b in t['blocked_by_keys'] if b not in CLOSED]
            t['change_log'].append(f"Dropped closed blocker(s) {', '.join(drop)}: {NOTE}."); log.append((t['key'],'drop',drop))
        rs=(t.get('ready_state') or '').lower(); kind=(t.get('kind') or '').lower()
        if rs in ('needs-owner','parked'):
            new='needs-spec' if kind=='feature' else 'ready-for-agent'
            why=(NOTE+', so a missing Linear owner no longer blocks the claim') if rs=='needs-owner' else "'parked' is a milestone, not a triage state"
            t['ready_state']=new; t['change_log'].append(f'ready_state {rs} → {new}: {why}.'); log.append((t['key'],rs,new))
json.dump(res,open(sys.argv[2],'w'),indent=1)
for x in log: print(*x)
left=[(t['key'],m.group(0)) for r in res['repaired'] for t in r['repair']['tickets'] for m in re.finditer(r'.{0,70}(#517|X06-D7).{0,70}',json.dumps(t))]
left+=[(r['unit']+'-EPIC',m.group(0)) for r in res['repaired'] for m in re.finditer(r'.{0,70}(#517|X06-D7).{0,70}',r['repair'].get('epic_body',''))]
print('residual #517/X06-D7 mentions for review:',len(left))
for k,s in left: print(' ',k,'::',s.replace('\\n',' ')[:170])
