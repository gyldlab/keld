"""Bounded Update source mirror only. Fixed targets, no checkout application."""
import difflib,json,os,sys,uuid
from pathlib import Path
from preview_safety import read_plain,reject_alias_components,outside_repo,sha
TARGETS={'crates/keld-update/src/windows_baseline/locate/v3.rs', 'crates/keld-update/src/windows_baseline/initialize/publication.rs', 'crates/keld-update/src/windows_baseline.rs', 'crates/keld-update/src/provenance.rs', 'crates/keld-update/src/windows_baseline/generation_source/workspace.rs', 'crates/keld-update/src/windows_baseline/initialize/v3.rs', 'crates/keld-update/Cargo.toml', 'crates/keld-update/src/windows_baseline/generation_source.rs', 'crates/keld-update/src/windows_generation_stage/role_data.rs', 'crates/keld-update/src/windows_generation_stage.rs', 'crates/keld-update/src/windows_baseline/initialize.rs', 'crates/keld-update/src/manifest.rs', 'crates/keld-update/src/windows_baseline/tests/per_user.rs', 'crates/keld-update/src/windows_generation_stage/installation_binding.rs', 'crates/keld-update/src/windows_generation_stage/resource_preparation.rs', 'crates/keld-update/src/windows_generation_stage/seal_directories.rs', 'crates/keld-update/src/records/profile_binding_v3.rs', 'crates/keld-update/src/baseline/windows_v3.rs', 'crates/keld-update/src/windows_generation_stage/grant_join.rs', 'crates/keld-update/src/windows_extraction/tests/generation_stage.rs', 'crates/keld-update/src/baseline.rs', 'crates/keld-update/src/windows_baseline/initialize/request_v3.rs', 'crates/keld-update/src/windows_baseline/load.rs', 'crates/keld-update/src/lib.rs', 'crates/keld-update/src/windows_baseline/locate.rs', 'crates/keld-update/src/windows_fs.rs', 'crates/keld-update/src/windows_generation_stage/read_plan.rs', 'crates/keld-update/src/records.rs'}
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
    scope=entry.get('scoped_forbidden')
    if scope:
        if text.count(scope['start'])!=1 or text.count(scope['end'])!=1:raise ValueError('Scope boundaries ambiguous')
        start=text.index(scope['start']);end=text.index(scope['end'],start)
        if any(token in text[start:end] for token in scope['tokens']):raise ValueError('Legacy trust reference in new neutral root body')
    result=text.encode()
    if entry['after_sha256'] and sha(result)!=entry['after_sha256']:raise ValueError('Reviewed afterhash mismatch')
    return result

def main():
    if os.name!='nt':raise ValueError('Native Windows only')
    repo=Path(sys.argv[1]);reject_alias_components(repo)
    if not repo.is_absolute():raise ValueError('Absolute original source required')
    bundle=Path(__file__).parent
    plan=json.loads((bundle/'update-source-plan.json').read_text())
    if set(plan)!=TARGETS:raise ValueError('Unexpected target set')
    originals={name:read_optional(repo/name) for name in plan}
    selected={}
    for name,entry in plan.items():
        raw=originals[name]
        actual=sha(raw) if raw is not None else None
        choices=[variant for variant in entry['variants'] if actual==variant['before_sha256']]
        if len(choices)!=1:raise ValueError('Beforehash missing/ambiguous '+name)
        text=(raw or b'').decode('utf-8')
        if any(token in text for token in entry.get('absent_before',[])):raise ValueError('Existing/ambiguous addition '+name)
        selected[name]=choices[0]
    plan=selected
    previews={}
    for name,entry in plan.items():
        try:previews[name]=transform(originals[name],entry)
        except ValueError as error:raise ValueError(name+': '+str(error)) from error
    repo=repo.resolve(strict=True);parent=Path('D:/KeldBuild');root=parent/('update-source-preview-'+uuid.uuid4().hex)
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
    with (root/'native-update.patch').open('x',encoding='utf-8',newline='\n') as f:f.write(''.join(delta))
    unchanged=all(read_optional(repo/n)==raw for n,raw in originals.items())
    receipt={'preview':str(root),'repository_unchanged':unchanged,'files':manifest,'status':'not applied; native patch review and explicit adoption still required'}
    with (root/'manifest.json').open('x',encoding='utf-8') as f:json.dump(receipt,f,indent=2)
    print(json.dumps(receipt),flush=True)
    if not unchanged:raise ValueError('Concurrent mutation invalidates preview; preserve evidence')
if __name__=='__main__':main()
