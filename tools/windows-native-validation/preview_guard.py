"""Complete Guard source mirror only. Fixed targets, no checkout application."""
import difflib,json,os,sys,uuid
from pathlib import Path
from preview_safety import read_plain,reject_alias_components,outside_repo,sha
TARGETS={'.agents/instruction-budget.tsv','crates/keld-guard/AGENTS.md','crates/keld-guard/Cargo.toml','crates/keld-guard/src/admit.rs','crates/keld-guard/src/probe.rs','crates/keld-guard/src/windows_authenticode.rs','crates/keld-guard/src/windows_profile/AGENTS.md','crates/keld-guard/src/windows_profile/grant.rs','crates/keld-guard/src/windows_profile/mod.rs','crates/keld-guard/src/windows_qualification_catalog/deployment_policy.rs','crates/keld-guard/src/windows_qualification_catalog.rs','crates/keld-guard/src/lib.rs','crates/keld-guard/src/windows_profile/generation.rs'}
def read_optional(path):
    reject_alias_components(path)
    for p in [path,*path.parents]:
        try:st=p.lstat()
        except FileNotFoundError:continue
        if p.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('Reparse input')
    try:return read_plain(path)
    except FileNotFoundError:return None

def transform(raw,entry):
    text=(raw or b'').decode('utf-8')
    if '\r' in text:raise ValueError('Expected exact LF')
    for i,op in enumerate(entry['operations']):
        before=op['before']
        if not before:
            if raw is not None or text:raise ValueError('New-file operation on existing source')
            text=op['after']
        else:
            if text.count(before)!=1:raise ValueError('Hunk '+str(i)+' context missing/nonunique')
            text=text.replace(before,op['after'],1)
    result=text.encode()
    if entry['after_sha256'] and sha(result)!=entry['after_sha256']:raise ValueError('Reviewed afterhash mismatch')
    return result

def main():
    if os.name!='nt':raise ValueError('Native Windows only')
    repo=Path(sys.argv[1]);reject_alias_components(repo)
    if not repo.is_absolute():raise ValueError('Absolute original source required')
    bundle=Path(__file__).parent
    plan=json.loads((bundle/'guard-source-plan.json').read_text())
    if set(plan)!=TARGETS:raise ValueError('Unexpected target set')
    originals={name:read_optional(repo/name) for name in plan}
    for name,entry in plan.items():
        actual=sha(originals[name]) if originals[name] is not None else None
        if actual!=entry['before_sha256']:raise ValueError('Beforehash mismatch '+name)
    previews={}
    for name,entry in plan.items():
        try:previews[name]=transform(originals[name],entry)
        except ValueError as error:raise ValueError(name+': '+str(error)) from error
    repo=repo.resolve(strict=True);parent=Path('D:/KeldBuild');root=parent/('guard-final-preview-'+uuid.uuid4().hex)
    for p in [parent,*parent.parents]:
        st=p.lstat()
        if p.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('Reparse output parent')
    if not outside_repo(repo,parent.resolve(strict=True)/root.name):raise ValueError('Output overlaps checkout')
    if any(read_optional(repo/n)!=raw for n,raw in originals.items()):raise ValueError('Concurrent mutation before preview')
    root.mkdir();manifest={};delta=[]
    for name,result in previews.items():
        target=root/name;target.parent.mkdir(parents=True,exist_ok=True)
        with target.open('xb') as f:f.write(result)
        original=originals[name]
        manifest[name]={'before':sha(original) if original is not None else None,'after':sha(result)}
        delta.extend(difflib.unified_diff((original or b'').decode().splitlines(True),result.decode().splitlines(True),fromfile='a/'+name if original is not None else '/dev/null',tofile='b/'+name))
    with (root/'native-guard.patch').open('x',encoding='utf-8',newline='\n') as f:f.write(''.join(delta))
    unchanged=all(read_optional(repo/n)==raw for n,raw in originals.items())
    receipt={'preview':str(root),'repository_unchanged':unchanged,'files':manifest,'status':'not applied; native patch review and explicit adoption still required'}
    with (root/'manifest.json').open('x',encoding='utf-8') as f:json.dump(receipt,f,indent=2)
    print(json.dumps(receipt),flush=True)
    if not unchanged:raise ValueError('Concurrent mutation invalidates preview; preserve evidence')
if __name__=='__main__':main()
