"""Finish publishing the wayfinder plan: resolve remaining body placeholders, sub-issue links, blocked-by links,
resolution comments and closes. Idempotent via finish_state.json. Usage: python3 gh_finish.py"""
import json, subprocess, sys, time, os
plan=json.load(open('publish_plan.json')); nums=json.load(open('publish_state.json'))
REPO=plan['repo']; SP='finish_state.json'
st=json.load(open(SP)) if os.path.exists(SP) else {}
def save(): json.dump(st,open(SP,'w'),indent=1)
def gh(args, inp=None):
    for attempt in range(6):
        p=subprocess.run(['gh','api']+args,input=inp,capture_output=True,text=True)
        if p.returncode==0: return p.stdout
        err=(p.stderr+p.stdout)
        if any(x in err.lower() for x in ('rate limit','abuse','secondary')) or ' 403' in err or ' 429' in err:
            w=min(90,10*(attempt+1)); print(f'rate-limited, waiting {w}s',flush=True); time.sleep(w); continue
        raise RuntimeError(err[:400])
    raise RuntimeError('retries exhausted')
def sub(body):
    for k,v in nums.items(): body=body.replace('{{'+k+'}}',f"#{v['number']}")
    return body
issues=plan['issues']; by={i['key']:i for i in issues}
def do(tag, fn):
    if st.get(tag) is True: return
    try: fn(); st[tag]=True
    except RuntimeError as e:
        msg=str(e)
        st[tag]=('exists' if ('already' in msg.lower() or 'duplicate' in msg.lower() or '422' in msg) else 'failed: '+msg[:160])
        if st[tag]!='exists': print('FAIL',tag,msg[:160],flush=True)
    save()
# A: bodies with unresolved placeholders
for i in issues:
    k=i['key']; n=nums[k]['number']
    def patch(i=i,n=n): gh(['-X','PATCH',f'repos/{REPO}/issues/{n}','--input','-'],inp=json.dumps({'body':sub(i['body'])})); time.sleep(0.5)
    if '{{' in i['body']: do(f'body:{k}',patch)
print('bodies done',flush=True)
# B: sub-issue links
for i in issues:
    k=i['key']; pk=i.get('parent_key')
    if not pk: continue
    def link(k=k,pk=pk): gh(['-X','POST',f"repos/{REPO}/issues/{nums[pk]['number']}/sub_issues",'--input','-'],inp=json.dumps({'sub_issue_id':nums[k]['id']})); time.sleep(0.5)
    do(f'sub:{pk}>{k}',link)
print('sub-issues done',flush=True)
# C: dependencies
for i in issues:
    k=i['key']
    for b in i.get('blocked_by_keys',[]):
        if b not in nums: continue
        def dep(k=k,b=b): gh(['-X','POST',f"repos/{REPO}/issues/{nums[k]['number']}/dependencies/blocked_by",'--input','-'],inp=json.dumps({'issue_id':nums[b]['id']})); time.sleep(0.5)
        do(f'dep:{k}<{b}',dep)
print('dependencies done',flush=True)
# D: resolution comments + close
for i in issues:
    k=i['key']; n=nums[k]['number']
    for ci,c in enumerate(i.get('comments',[])):
        def com(c=c,n=n): gh(['-X','POST',f'repos/{REPO}/issues/{n}/comments','--input','-'],inp=json.dumps({'body':sub(c)})); time.sleep(0.6)
        do(f'comment:{k}:{ci}',com)
    if i.get('close'):
        def close(n=n): gh(['-X','PATCH',f'repos/{REPO}/issues/{n}','--input','-'],inp=json.dumps({'state':'closed','state_reason':'completed'})); time.sleep(0.4)
        do(f'closed:{k}',close)
import collections
c=collections.Counter((t.split(':')[0], 'ok' if v is True else ('exists' if v=='exists' else 'failed')) for t,v in st.items())
print('SUMMARY',dict(c),flush=True)
