"""Group GitHub items by their nearest live Linear issue for save_issue(links=...) calls.
Usage: python3 link_plan.py <alignment.json> <repair_final.json>  -> link_plan.json"""
import json, sys, collections
al=json.load(open(sys.argv[1])); res=json.load(open(sys.argv[2])); nums=json.load(open('publish_state.json')); plan=json.load(open('publish_plan.json'))
title={i['key']:i['title'] for i in plan['issues']}
for r in res['repaired']:
    title[r['unit']+'-EPIC']=r['repair']['epic_title']
    for t in r['repair']['tickets']: title[t['key']]=t['title']
sup={s['key'] for r in res['repaired'] for s in r['repair'].get('supersede') or []}
by=collections.defaultdict(list); skipped=[]
for u in al['alignment']:
    for i in u['items']:
        k=i['key']
        if k not in nums or k in sup: skipped.append(k); continue
        L=(i.get('linear_issue') or 'KEL-127').split()[0].strip(',;')
        n=nums[k]['number']; short=title.get(k,k)
        short=short if len(short)<=72 else short[:71].rstrip()+'…'
        by[L].append({'url':f'https://github.com/gyldlab/keld/issues/{n}','title':f"GH #{n} · {k} · {short} · tracker: {'this Linear issue' if i['tracker']=='linear' else 'GitHub'}"})
json.dump(by,open('link_plan.json','w'),indent=1)
print({k:len(v) for k,v in sorted(by.items(),key=lambda x:-len(x[1]))}, '| skipped:',skipped)
