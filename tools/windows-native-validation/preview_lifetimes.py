"""Read-only CLI test type-lifetime preview; bodies remain byte-for-byte unchanged."""
import hashlib,json,os,stat,sys
from pathlib import Path
EXPECTED='6888246d8f02c34c0d7803d809e336214cfc4939fb2ebb25dbf8691153c8b21a'
ROOT='D:/KeldBuild/keld/.keld-work/worktrees/gh-1405-windows-app-input-admission'
OPERATIONS=[
 ("pub(super) struct AuthenticatedLocalBaseline {","pub(super) struct AuthenticatedLocalBaseline<'images> {",1),
 ("    _candidate: LocalArchiveCandidate,","    _candidate: LocalArchiveCandidate<'images>,",1),
 ("impl AuthenticatedLocalBaseline {","impl AuthenticatedLocalBaseline<'_> {",2),
 ("pub(super) fn authenticate(","pub(super) fn authenticate<'images>(",1),
 ("    candidate: LocalArchiveCandidate,","    candidate: LocalArchiveCandidate<'images>,",1),
 ("fn authenticate_inner(","fn authenticate_inner<'images>(",1),
 ("    mut candidate: LocalArchiveCandidate,","    mut candidate: LocalArchiveCandidate<'images>,",1),
 (") -> Result<AuthenticatedLocalBaseline, BuildError> {",") -> Result<AuthenticatedLocalBaseline<'images>, BuildError> {",2),
 ("    fn candidate(parent: &std::path::Path, name: &str, truncate: bool) -> LocalArchiveCandidate {","    fn candidate(parent: &std::path::Path, name: &str, truncate: bool) -> LocalArchiveCandidate<'static> {",1),
]
def transform(text):
    for before,after,count in OPERATIONS:
        if text.count(before)!=count: raise ValueError('missing_or_duplicate_declaration')
        text=text.replace(before,after)
    return text
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

def main():
    if len(sys.argv)!=2:raise ValueError('one_repository_root')
    root=Path(sys.argv[1])
    if not root.is_absolute() or os.path.normcase(str(root.resolve(strict=True)))!=os.path.normcase(str(Path(ROOT))):raise ValueError('exact_root_required')
    path=root/'crates/keld-cli/src/build/local_baseline.rs'
    raw=read(path,64*1024)
    if hashlib.sha256(raw).hexdigest()!=EXPECTED:raise ValueError('native_before_drift')
    result=transform(raw.decode()).encode()
    if read(path,64*1024)!=raw:raise ValueError('concurrent_mutation')
    print(json.dumps({'source_written':False,'before_sha256':EXPECTED,'before_bytes':len(raw),'after_sha256':hashlib.sha256(result).hexdigest(),'after_bytes':len(result),'declaration_edits':sum(n for _,_,n in OPERATIONS)}))
if __name__=='__main__':main()
