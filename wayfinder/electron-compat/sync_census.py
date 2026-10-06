"""Sync-semantics census: which Electron members demanded by the corpus apps return synchronously (non-void, non-Promise)?
Joins electron-api.json return types with the corpus usage scans. Output: sync-census.md + sync-census.json"""
import json, collections, re
api=json.load(open('electron-api.json'))
ret={}  # (entity, member) -> return type string or 'EVENT'/'PROPERTY'
kind={}
for e in api:
    for key in ('methods','instanceMethods','staticMethods'):
        for m in e.get(key) or []:
            r=m.get('returns')
            rt='void' if not r else (r.get('type') if isinstance(r,dict) else str(r))
            if isinstance(rt,list): rt='|'.join(x.get('type',str(x)) if isinstance(x,dict) else str(x) for x in rt)
            ret[(e['name'],m['name'])]=rt; kind[(e['name'],m['name'])]='method'
    for key in ('properties','instanceProperties','staticProperties'):
        for m in e.get(key) or []:
            ret[(e['name'],m['name'])]=f"PROPERTY:{m.get('type','')}"; kind[(e['name'],m['name'])]='property'
    for key in ('events','instanceEvents'):
        for m in e.get(key) or []:
            ret[(e['name'],m['name'])]='EVENT'; kind[(e['name'],m['name'])]='event'
def classify(rt, name):
    if rt=='EVENT': return 'event'
    if rt.startswith('PROPERTY'): return 'sync-property'
    if 'Promise' in rt: return 'async-promise'
    if rt in ('void','undefined','') : return 'sync-void'
    if name.endswith('Sync'): return 'sync-blocking'
    return 'sync-value'
rows=[]
for app in ('drawio-desktop','zettlr','electron-quick-start'):
    u=json.load(open(f'corpus/{app}.usage.json'))
    seen=set()
    for m in u['members_top']:
        k=(m['entity'],m['member'])
        if k in seen: continue
        seen.add(k)
        rt=ret.get(k,'?'); c=classify(rt,m['member'])
        rows.append({'app':app,'entity':k[0],'member':k[1],'count':m['count'],'files':m['file_count'],'returns':rt,'class':c})
# aggregate
agg=collections.defaultdict(lambda: collections.Counter())
for r in rows: agg[r['app']][r['class']]+=r['count']
out=['# Sync-semantics census (corpus demand × Electron v44.4.5 return types)','','Generated 2026-10-06 from `corpus/*.usage.json` (regex member scan; inherited-name ambiguity means counts for generic names like `close`/`focus` are attributed to every class that declares them — treat as upper bounds). Classes: `sync-value` = synchronous non-void return (needs a facade mirror, prefetch, or blocking CALL); `sync-blocking` = `*Sync` API; `sync-void` = fire-and-forget (can be an async CALL without observable change unless ordering matters); `async-promise` = already async in Electron; `sync-property` = property read.','']
for app in ('drawio-desktop','zettlr','electron-quick-start'):
    out.append(f'## {app}'); out.append('')
    out.append('| class | weighted call sites |'); out.append('|---|---:|')
    for c,v in agg[app].most_common(): out.append(f'| {c} | {v} |')
    out.append(''); out.append('| entity.member | class | returns | count | files |'); out.append('|---|---|---|---:|---:|')
    for r in sorted([r for r in rows if r['app']==app and r['class'] in ('sync-value','sync-blocking','sync-property')], key=lambda r:-r['count'])[:60]:
        out.append(f"| {r['entity']}.{r['member']} | {r['class']} | {r['returns'][:40]} | {r['count']} | {r['files']} |")
    out.append('')
open('sync-census.md','w').write('\n'.join(out)); json.dump(rows,open('sync-census.json','w'),indent=1)
print('\n'.join(out[:60]))
