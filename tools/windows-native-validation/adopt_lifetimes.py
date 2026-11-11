"""Apply only reviewed CLI test lifetime annotations, using the existing checked adoption sequence."""
import difflib
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import types
import uuid


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


EXPECTED_AFTER = 'efdf64ea71b3fa9ad9e753673cfe12037e10deeb4d4004c2fddaabeb37fd3e20'

def main():
    if os.name != "nt" or len(sys.argv) != 1:
        raise ValueError("Run without arguments inside the owned Windows task")
    if EXPECTED_AFTER is None:
        raise ValueError("Verified native after-image required")
    repo = Path.cwd()
    if os.path.normcase(str(repo.resolve(strict=True))) != os.path.normcase(str(Path('D:/KeldBuild/keld/.keld-work/worktrees/gh-1405-windows-app-input-admission'))):
        raise ValueError("Exact owned repository required")
    bundle = Path(__file__).absolute().parent
    expected = {"preview_lifetimes.py": "a183f1dda86f5a13d8e044d0daff5e127c2cbbfa968e74a3ad765303952a5785"}
    helper = bundle / "preview_lifetimes.py"
    for item in [helper, *helper.parents]:
        info = item.lstat()
        if item.is_symlink() or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("Reparse transformer path")
    if not stat.S_ISREG(helper.stat().st_mode) or helper.stat().st_size > 64 * 1024:
        raise ValueError("Transformer bound/type")
    with helper.open("rb") as stream:
        payload = stream.read(64 * 1024 + 1)
    if digest(payload) != expected["preview_lifetimes.py"]:
        raise ValueError("Reviewed lifetime transformer differs")
    view = types.ModuleType("preview_lifetimes")
    exec(compile(payload, str(bundle / "preview_lifetimes.py"), "exec"), view.__dict__)
    name = "crates/keld-cli/src/build/local_baseline.rs"
    original = {name: view.read(repo / name, 64 * 1024)}
    if digest(original[name]) != view.EXPECTED:
        raise ValueError("Exact native before-image differs")
    after = {name: view.transform(original[name].decode()).encode()}
    if digest(after[name]) != EXPECTED_AFTER:
        raise ValueError("Exact reviewed after-image differs")
    plan = {name}
    patch = "".join(difflib.unified_diff(original[name].decode().splitlines(True),
        after[name].decode().splitlines(True), fromfile="a/" + name, tofile="b/" + name)).encode()
    parent = Path("D:/KeldBuild")
    for folder in [parent, *parent.parents]:
        info = folder.lstat()
        if folder.is_symlink() or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("Reparse evidence root")
    git = Path("C:/Program Files/Git/cmd/git.exe")
    for item in [git, *git.parents]:
        metadata = item.lstat()
        if item.is_symlink() or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("Reparse Git executable path")
    if not stat.S_ISREG(git.stat().st_mode) or git.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("Git executable bound exceeded")
    with git.open("rb") as executable:
        executable_bytes = executable.read(64 * 1024 * 1024 + 1)
    if len(executable_bytes) > 64 * 1024 * 1024 or digest(executable_bytes) != "3b64043b9e4dc225225f43ff9c1173b57493ff72e5b729accfae051eb160cca3":
        raise ValueError("Qualified Git executable differs")

    def run(*args, data=None):
        return subprocess.check_output([str(git), "-c", "gc.auto=0", "-c", "maintenance.auto=false", *args], cwd=repo, input=data)

    names = {line.split(b"\t", 2)[2].decode() for line in run("apply", "--numstat", "-", data=patch).splitlines()}
    if names != set(plan):
        raise ValueError("Patch target scope differs")
    run("apply", "--check", "--whitespace=nowarn", "-", data=patch)
    if any(view.read(repo / name, 64 * 1024) != raw for name, raw in original.items()):
        raise ValueError("Concurrent source mutation before adoption")
    evidence = parent / ("cli-lifetime-adoption-" + uuid.uuid4().hex)
    if evidence.resolve().is_relative_to(repo.resolve()):
        raise ValueError("Evidence must be outside checkout")
    evidence.mkdir()
    (evidence / "before-status").write_bytes(run("status", "--porcelain=v1", "-z"))
    (evidence / "before.patch").write_bytes(run("diff", "--no-ext-diff", "--binary"))
    for name, raw in original.items():
        if raw is not None:
            dest = evidence / "originals" / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(raw)
    receipt = {"reviewed_after_sha256": EXPECTED_AFTER, "patch_sha256": digest(patch), "files": {
        name: {"before": digest(original[name]) if original[name] is not None else None, "after": digest(after[name])}
        for name in plan}}
    (evidence / "inputs.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    if any(view.read(repo / name, 64 * 1024) != raw for name, raw in original.items()):
        raise ValueError("Concurrent source mutation after backup")
    (evidence / "PENDING").write_text("Exact one-file lifetime git apply; inspect outcome before any retry", encoding="utf-8")
    print("CLI_LIFETIME_ADOPTION_PENDING", evidence, flush=True)
    run("apply", "--whitespace=nowarn", "-", data=patch)
    if any(view.read(repo / name, 64 * 1024) != raw for name, raw in after.items()):
        raise ValueError("After-image mismatch: preserve PENDING and inspect, do not retry")
    (evidence / "COMPLETE").write_text("One exact lifetime after-image verified", encoding="utf-8")
    print("CLI_LIFETIME_ADOPTED_EXACT", evidence, flush=True)


if __name__ == "__main__":
    main()
