"""Apply only the reviewed, hash-pinned 28-file Update mirror to its owned task."""
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


def main():
    if os.name != "nt" or len(sys.argv) != 1:
        raise ValueError("Run without arguments inside the owned Windows task")
    repo = Path.cwd()
    if repo.name != "gh-1405-windows-app-input-admission":
        raise ValueError("Unexpected task")
    bundle = Path(__file__).absolute().parent
    expected = {
        "preview_safety.py": "f862e00ebc1af7b4f3b82d89a076eb251d86781848eaadc2a1e6a4808278d252",
        "preview_update.py": "af61364573cf55b9d9f8672f1521311cb13955671f48713506ff5d1405601429",
        "update-source-plan.json": "0e1bcd9fece040cc53c8b4bd38dc8ca902add102f67f632d44723cee64b9bc8d",
    }
    captured = {name: (bundle / name).read_bytes() for name in expected}
    if any(digest(captured[name]) != pin for name, pin in expected.items()):
        raise ValueError("Reviewed payload differs")
    safety = types.ModuleType("preview_safety")
    exec(compile(captured["preview_safety.py"], str(bundle / "preview_safety.py"), "exec"), safety.__dict__)
    sys.modules["preview_safety"] = safety
    view = types.ModuleType("preview_update")
    exec(compile(captured["preview_update.py"], str(bundle / "preview_update.py"), "exec"), view.__dict__)
    safety.reject_alias_components(bundle)
    safety.reject_alias_components(repo)
    plan = json.loads(captured["update-source-plan.json"])
    if set(plan) != view.TARGETS or len(plan) != 28:
        raise ValueError("Unexpected target set")
    original = {name: view.read_optional(repo / name) for name in plan}
    after = {}
    for name, entry in plan.items():
        raw = original[name]
        pin = digest(raw) if raw is not None else None
        selected = [item for item in entry["variants"] if item["before_sha256"] == pin]
        if len(selected) != 1 or any(token in (raw or b"").decode() for token in entry.get("absent_before", [])):
            raise ValueError("Reviewed before-image differs: " + name)
        after[name] = view.transform(raw, selected[0])
    parent = Path("D:/KeldBuild")
    safety.reject_alias_components(parent)
    previews = [item for item in parent.glob("update-source-preview-*") if item.is_dir()]
    if len(previews) != 1:
        raise ValueError("Expected one retained Update preview, not a guessed UUID")
    preview = previews[0]
    if any(safety.read_plain(preview / name) != raw for name, raw in after.items()):
        raise ValueError("Preview differs from reviewed transformation")
    patch = "".join("".join(difflib.unified_diff(
        (original[name] or b"").decode().splitlines(True), after[name].decode().splitlines(True),
        fromfile="a/" + name if original[name] is not None else "/dev/null", tofile="b/" + name,
    )) for name in plan).encode()
    if safety.read_plain(preview / "native-update.patch") != patch:
        raise ValueError("Preview patch differs from exact reconstructed patch")
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
    if any(view.read_optional(repo / name) != raw for name, raw in original.items()):
        raise ValueError("Concurrent source mutation before adoption")
    evidence = parent / ("update-adoption-" + uuid.uuid4().hex)
    if not safety.outside_repo(repo.resolve(), evidence):
        raise ValueError("Evidence must be outside checkout")
    evidence.mkdir()
    (evidence / "before-status").write_bytes(run("status", "--porcelain=v1", "-z"))
    (evidence / "before.patch").write_bytes(run("diff", "--no-ext-diff", "--binary"))
    for name, raw in original.items():
        if raw is not None:
            dest = evidence / "originals" / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(raw)
    receipt = {"preview": str(preview), "patch_sha256": digest(patch), "files": {
        name: {"before": digest(original[name]) if original[name] is not None else None, "after": digest(after[name])}
        for name in plan}}
    (evidence / "inputs.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    if any(view.read_optional(repo / name) != raw for name, raw in original.items()):
        raise ValueError("Concurrent source mutation after backup")
    (evidence / "PENDING").write_text("Exact 28-file git apply; inspect outcome before any retry", encoding="utf-8")
    print("UPDATE_ADOPTION_PENDING", evidence, flush=True)
    run("apply", "--whitespace=nowarn", "-", data=patch)
    if any(safety.read_plain(repo / name) != raw for name, raw in after.items()):
        raise ValueError("After-image mismatch: preserve PENDING and inspect, do not retry")
    (evidence / "COMPLETE").write_text("28 exact after-images verified", encoding="utf-8")
    print("UPDATE_ADOPTED_28_EXACT", evidence, flush=True)


if __name__ == "__main__":
    main()
