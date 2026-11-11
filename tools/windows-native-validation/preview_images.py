"""Pinned ordinary image source in-memory preview. No writes or subprocesses."""
import hashlib,json,os,stat,sys
from pathlib import Path
PLAN_SHA='c4573b3338b40da43617ca091704962c995fa4ab71320a8713067fee627580b9'
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

def transform(raw,plan):
    if hashlib.sha256(raw).hexdigest()!=plan['before_sha256']: raise ValueError('native_drift')
    text=raw.decode('utf-8')
    for operation in plan['operations']:
        before=operation['before']
        if text.count(before)!=1: raise ValueError('missing_or_duplicate_anchor')
        text=text.replace(before,operation['after'],1)
    return text.encode('utf-8')
def main():
    if len(sys.argv)!=3: raise ValueError('repository_and_plan_required')
    root,plan_path=map(Path,sys.argv[1:])
    if not root.is_absolute() or not plan_path.is_absolute(): raise ValueError('absolute_paths')
    encoded=read(plan_path,128*1024)
    if hashlib.sha256(encoded).hexdigest()!=PLAN_SHA: raise ValueError('plan_pin')
    plan=json.loads(encoded)
    relative='crates/keld-cli/src/build/local_images.rs'
    if plan['path']!=relative or len(plan['operations'])!=6: raise ValueError('closed_plan')
    raw=read(root/relative,128*1024);after=transform(raw,plan)
    if read(root/relative,128*1024)!=raw: raise ValueError('changed_during_preview')
    print(json.dumps({'source_written':False,'operation_count':6,'before_bytes':len(raw),
      'before_sha256':hashlib.sha256(raw).hexdigest(),'after_bytes':len(after),'after_sha256':hashlib.sha256(after).hexdigest()}))
if __name__=='__main__':main()
