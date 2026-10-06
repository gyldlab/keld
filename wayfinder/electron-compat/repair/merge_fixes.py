"""Apply final-fix patch ops to the normalized repair. Usage: python3 merge_fixes.py <repair_final_pre.json> <final_fix_result.json> -> repair_final.json, label_changes.json"""
import json, sys, collections
res=json.load(open(sys.argv[1])); fx=json.load(open(sys.argv[2]))
T={}; U={}
for r in res['repaired']:
    for t in r['repair']['tickets']: T[t['key']]=t; U[t['key']]=r
LISTS={'acceptance_criteria','negative_controls','blocked_by_keys','blocked_by_external','review_gates','electron_members_covered'}
STRS={'title','what_to_build','ready_state','milestone','linear_owner','notes','current_behavior','key_interfaces','out_of_scope','kind','tier','maturity_target','size','platforms'}
sets=collections.defaultdict(list); problems=[]; applied=0; labels=[]
for f in fx['fixes']:
    c=f['cluster']
    for nt in f.get('new_tickets') or []:
        t=nt['ticket']; u=nt['unit']
        if t['key'] in T: problems.append(f"{c}: new ticket {t['key']} already exists"); continue
        unit=next(r for r in res['repaired'] if r['unit']==u); unit['repair']['tickets'].append(t); T[t['key']]=t; U[t['key']]=unit
        t.setdefault('change_log',[]).append(f'Created by the final residual fix ({c}).')
    for op in f['ops']:
        k=op['key']; fld=op['field']; t=T.get(k)
        if t is None: problems.append(f"{c}: unknown key {k}"); continue
        why=op.get('why','')
        if fld in STRS:
            if op['op']=='set':
                sets[(k,fld)].append(c); t[fld]=op.get('value','')
            elif op['op']=='replace':
                cur=t.get(fld) or ''
                if cur.count(op.get('old',''))!=1: problems.append(f"{c}: {k}.{fld} replace anchor count {cur.count(op.get('old',''))}"); continue
                t[fld]=cur.replace(op['old'],op.get('new',''))
            else: problems.append(f"{c}: {k}.{fld} bad op {op['op']}"); continue
        elif fld in LISTS:
            cur=list(t.get(fld) or [])
            if op['op']=='add': cur+= [x for x in op.get('items') or [] if x not in cur]
            elif op['op']=='remove':
                miss=[x for x in op.get('items') or [] if x not in cur]
                if miss: problems.append(f"{c}: {k}.{fld} remove missing {[m[:60] for m in miss]}")
                cur=[x for x in cur if x not in (op.get('items') or [])]
            elif op['op']=='replace':
                if op.get('old') not in cur: problems.append(f"{c}: {k}.{fld} replace missing item {str(op.get('old'))[:60]}"); continue
                cur=[op.get('new') if x==op['old'] else x for x in cur]
            else: problems.append(f"{c}: {k}.{fld} bad op"); continue
            t[fld]=cur
        else: problems.append(f"{c}: {k} unknown field {fld}"); continue
        t.setdefault('change_log',[]).append(f"{fld} {op['op']}: {why} (final residual fix {c})"); applied+=1
    for s in f.get('supersede') or []:
        k=s['key']
        if k not in T: problems.append(f"{c}: supersede unknown {k}"); continue
        U[k]['repair'].setdefault('supersede',[]).append(s)
    labels+= [dict(l,cluster=c) for l in f.get('label_changes') or []]
    for rj in f.get('rejected') or []: print(f"REJECTED by {c}: {rj['residual'][:120]} :: {rj['reason'][:200]}")
for (k,fld),cs in sets.items():
    if len(cs)>1: problems.append(f"CONFLICT {k}.{fld} set by {cs}")
json.dump(res,open('repair_final.json','w'),indent=1); json.dump(labels,open('label_changes.json','w'),indent=1)
print('ops applied:',applied,'| new tickets:',sum(len(f.get('new_tickets') or []) for f in fx['fixes']),'| supersedes:',sum(len(f.get('supersede') or []) for f in fx['fixes']),'| label changes:',len(labels))
print('problems:',len(problems)); [print('  ',p) for p in problems]
