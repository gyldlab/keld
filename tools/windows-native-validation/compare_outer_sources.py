"""Read-only three-file anchor census. No transformation, writer or subprocess."""
import hashlib,json,os,stat,sys
from pathlib import Path
PLAN_SHA='95d2164d4b87b827526dd2ef7a876dc695584ae9f13b663637ab096d51e1bd21'
NAMES=('local_archive.rs','local_archive_command.rs','local_images.rs')
def read(path,limit):
    for p in (path,*path.parents):
        s=p.lstat()
        if p.is_symlink() or getattr(s,'st_file_attributes',0)&0x400: raise ValueError('alias')
    if not stat.S_ISREG(path.lstat().st_mode): raise ValueError('regular_file')
    fd=os.open(path,os.O_RDONLY|getattr(os,'O_BINARY',0)|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
    with os.fdopen(fd,'rb') as f:
        if not stat.S_ISREG(os.fstat(f.fileno()).st_mode): raise ValueError('regular_handle')
        b=f.read(limit+1)
        if len(b)>limit: raise ValueError('bound')
        f.seek(0)
        if f.read(limit+1)!=b: raise ValueError('changed')
        return b

def compare(raw,variants):
    text=raw.decode('utf-8'); normalized=text.replace('\r\n','\n')
    return {'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),
        'variants':[{'before_sha256':v['before_sha256'],
          'anchors':[{'index':i,'raw_count':text.count(op['before']),
             'lf_count':normalized.count(op['before'].replace('\r\n','\n'))}
            for i,op in enumerate(v['operations'])]} for v in variants]}

def main():
    if len(sys.argv)!=3: raise ValueError('repository_and_plan_required')
    root,plan_path=map(Path,sys.argv[1:])
    if not root.is_absolute() or not plan_path.is_absolute(): raise ValueError('absolute_paths')
    plan_raw=read(plan_path,128*1024)
    if hashlib.sha256(plan_raw).hexdigest()!=PLAN_SHA: raise ValueError('plan_pin')
    plan=json.loads(plan_raw)
    rows=[]
    for name in NAMES:
        relative='crates/keld-cli/src/build/'+name
        rows.append({'path':relative,**compare(read(root/relative,128*1024),plan[relative]['variants'])})
    print(json.dumps({'source_written':False,'observations':rows}))
if __name__=='__main__': main()
