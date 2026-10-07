"""Consumer input contract used only through the shared CI router.

ci-inputs.json owns reviewed scopes and the reader source inventories behind them.
Changing any reader byte or adding/removing a reader invalidates that consumer's
exclusions. The fallback runs its gates until the scopes are reviewed and rebound.
`--check` fails when a recorded digest is stale; `--rebind` (`just ci-inputs-rebind`) renews
only the digest values after review.
This is deliberately a proof boundary, not a source-code dependency guesser.
"""

import fnmatch
import hashlib
import json
from pathlib import Path
import subprocess
import sys

SCHEMA = "keld.ci-inputs/v1"
MANDATORY = {"agent-context", "audit-docs", "product-status-check", "deny"}


def matches(path: str, patterns: list[str]) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)


def files(root: Path) -> list[str]:
    # Membership comes from Git plus untracked files, never a guessed directory
    # walk through build outputs. Deleted tracked readers remain in the census.
    data = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=root)
    return sorted(set(item.decode("utf-8", "strict") for item in data.split(b"\0") if item))


def ignored_reader_inputs(root: Path) -> list[str]:
    # Git-ignored dist/generated files can still be read by recursive consumers.
    # The source trees below are the reviewed consumer roots. node_modules is
    # excluded by Bun's source walkers; private research has its separate Mermaid
    # content router. Root target/workspace resources are handled by live gates.
    roots = ("crates", "packages", "tools", "docs", ".agents", ".github", ".config", ".cargo")
    data = subprocess.check_output(
        ["git", "ls-files", "--others", "--ignored", "--exclude-standard", "-z", "--", *roots,
         ":(exclude)**/node_modules/**", ":(exclude)docs/research/**"], cwd=root)
    inputs = []
    for item in data.split(b"\0"):
        if not item:
            continue
        path = item.decode("utf-8", "strict")
        if (any(path.startswith(prefix + "/") for prefix in roots)
                and "/node_modules/" not in path and not path.startswith("docs/research/")):
            inputs.append(path)
    return inputs


def fingerprint(root: Path, inventory: list[str], patterns: list[str]) -> str:
    # The policy artifact cannot hash itself. Its changes are explicitly unknown
    # below; every actual reader/helper under these source roots is bound.
    selected = [path for path in inventory if path != "tools/ci-inputs.json" and matches(path, patterns)]
    if not selected:
        raise ValueError("empty reader source inventory")
    digest = hashlib.sha256()
    for relative in selected:
        path = root / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"missing or indirect reader: {relative}")
        # These are text source files. Git's Windows CRLF checkout conversion
        # must not invalidate the same reviewed source on another OS.
        content = path.read_bytes().replace(b"\r\n", b"\n")
        digest.update(relative.encode("utf-8") + b"\0")
        digest.update(hashlib.sha256(content).digest())
    return digest.hexdigest()


def membership_fingerprint(inventory: list[str], patterns: list[str]) -> str:
    """Bind source membership independently from changes to existing file bytes."""
    paths = [path for path in inventory if matches(path, patterns)]
    return hashlib.sha256("\0".join(paths).encode("utf-8")).hexdigest()


def load(root: Path) -> dict:
    data = json.loads((root / "tools/ci-inputs.json").read_text(encoding="utf-8"))
    if (not isinstance(data, dict) or data.get("schema") != SCHEMA or not isinstance(data.get("consumers"), list)
            or not isinstance(data.get("reader_sets"), dict)):
        raise ValueError("invalid consumer input contract")
    return data


def bindings(root: Path, contract: dict, census: list[str]) -> list[tuple[str, str, str, list[str]]]:
    """Every recorded digest as (name, committed, live, patterns); the one comparison owner."""
    rows = []
    for name, reader_set in contract["reader_sets"].items():
        try:
            live = fingerprint(root, census, reader_set["patterns"])
        except (OSError, ValueError) as error:
            live = f"unreadable: {error}"
        rows.append((name, reader_set["sha256"], live, reader_set["patterns"]))
    census_binding = contract.get("rust_source_census")
    if census_binding:
        rows.append(("rust_source_census", census_binding["membership_sha256"],
                     membership_fingerprint(census, census_binding["patterns"]), census_binding["patterns"]))
    return rows


def stale(root: Path) -> list[str]:
    """Names whose committed digest differs from the live bytes. Binds nothing first."""
    return [name for name, recorded, live, _ in bindings(root, load(root), files(root)) if recorded != live]


def review_hint(root: Path, recorded: str, census: list[str], patterns: list[str]) -> list[str]:
    """Files to review: set members changed since the commit that recorded the digest."""
    try:
        commit = subprocess.check_output(
            ["git", "log", "-1", "--format=%H", f"-S{recorded}", "--", "tools/ci-inputs.json"],
            cwd=root, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        commit = ""
    members = [path for path in census if path != "tools/ci-inputs.json" and matches(path, patterns)]
    if not commit:
        return [f"? {path}" for path in members]
    lines = []
    old = {path for path in subprocess.check_output(
        ["git", "ls-tree", "-r", "--name-only", "-z", commit], cwd=root, text=True).split("\0") if path}
    old_members = {path for path in old if path != "tools/ci-inputs.json" and matches(path, patterns)}
    lines += [f"+ {path}" for path in sorted(set(members) - old_members)]
    lines += [f"- {path}" for path in sorted(old_members - set(members))]
    shared = old_members & set(members)
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", "-z", commit, "--"], cwd=root, text=True).split(chr(0))
    lines += [f"M {path}" for path in changed if path in shared]
    lines.append(f"(since digest recorded in {commit[:8]}; the digest was not necessarily correct there)")
    return lines


def check(root: Path) -> int:
    """Live freshness gate: committed digests against live bytes, never rebinding."""
    names = stale(root)
    if not names:
        print("ci-inputs: every reader digest and the rust source census match the live bytes")
        return 0
    print(f"KELD-CI-INPUTS-STALE: tools/ci-inputs.json is not bound to the live bytes for: {', '.join(names)}.\n"
          "The router fails safe by enabling their consumers, so CI runs every lane until this is fixed.\n"
          "Fix: review the reader changes, then run `just ci-inputs-rebind` and commit tools/ci-inputs.json.",
          file=sys.stderr)
    return 1


def rebind(root: Path) -> int:
    """Rewrite only the digest values in tools/ci-inputs.json using fingerprint()/membership_fingerprint()."""
    contract = load(root)
    census = files(root)
    path = root / "tools/ci-inputs.json"
    text = path.read_bytes().decode("utf-8")
    changed = 0
    for name, recorded, live, patterns in bindings(root, contract, census):
        if live.startswith("unreadable"):
            raise ValueError(f"{name}: {live}")
        if recorded == live:
            continue
        if text.count(recorded) != 1:
            raise ValueError(f"{name}: recorded digest is not a unique value in tools/ci-inputs.json")
        text = text.replace(recorded, live)
        changed += 1
        print(f"{name}: {recorded[:12]} -> {live[:12]}; review:")
        for line in review_hint(root, recorded, census, patterns):
            print(f"  {line}")
    if changed:
        path.write_bytes(text.encode("utf-8"))
    else:
        print("ci-inputs: nothing to rebind")
    return 0


def classify(root: Path, changed: list[str], *, comparison_unknown=False, paths_only=False) -> dict[str, bool]:
    selected = {"local_" + gate: True for gate in MANDATORY}
    selected.update({"input_rust": False, "input_ts": False, "input_all": False,
                     "input_router": False, "local_default": True})
    try:
        contract = load(root)
        census = files(root)
        fingerprints = {}
        rust_census = contract.get("rust_source_census")
        if rust_census:
            patterns = rust_census["patterns"]
            if membership_fingerprint(census, patterns) != rust_census["membership_sha256"]:
                selected["input_all"] = True
        # Untracked/deleted entries have no established prior consumer contract.
        tracked = set(subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
                      .decode("utf-8").split("\0"))
        uncertain = comparison_unknown or (not paths_only and (
            bool(ignored_reader_inputs(root)) or any(
                path not in tracked or not (root / path).is_file() or (root / path).is_symlink()
                for path in changed)))
        if any(not matches(path, contract["known_inputs"]) for path in changed):
            uncertain = True
        for consumer in contract["consumers"]:
            outputs = consumer["outputs"]
            allowed = {"input_rust", "input_ts", "input_all", "input_registry", "input_mermaid"}
            if not outputs or any(not (output.startswith("local_") or output in allowed) for output in outputs):
                raise ValueError("invalid consumer output")
            reader_set = contract["reader_sets"][consumer["reader_set"]]
            try:
                readers = tuple(reader_set["patterns"])
                if readers not in fingerprints:
                    fingerprints[readers] = fingerprint(root, census, list(readers))
                current = fingerprints[readers]
                drift = current != reader_set["sha256"]
            except (OSError, ValueError):
                drift = True
            if drift and "input_rust" in outputs:
                # A newly added Rust reader can introduce an edge outside Cargo
                # metadata (including a host/build reader). Its old package or
                # platform exclusions are no longer proven.
                selected["input_all"] = True
            relevant = uncertain or drift or any(
                matches(path, consumer["inputs"] + reader_set["patterns"]) for path in changed)
            for output in outputs:
                selected[output] = selected.get(output, False) or relevant
        package_inputs = contract.get("rust_package_inputs", {})
        if not isinstance(package_inputs, dict):
            raise ValueError("invalid Rust consumer package mapping")
        for package, inputs in package_inputs.items():
            if (not isinstance(package, str) or not package.startswith("keld-")
                    or not package.isascii() or not package.replace("-", "").isalnum()):
                raise ValueError("invalid Rust consumer package")
            if (not isinstance(inputs, list) or not inputs
                    or any(not isinstance(pattern, str) or not pattern for pattern in inputs)):
                raise ValueError("invalid Rust consumer input patterns")
            relevant = any(matches(path, inputs) for path in changed)
            selected["input_package_" + package] = relevant
            if relevant:
                selected["input_rust"] = True
                for gate in ("fmt-check", "clippy", "test", "doc"):
                    selected["local_" + gate] = True
        if "tools/ci-inputs.json" in changed:
            # A known router-policy edit exercises every job while retaining
            # the established single GUI-smoke owner of live Ubuntu GTK apt.
            unknown = selected["input_all"]
            selected = dict.fromkeys(selected, True)
            selected["input_all"] = unknown
        if uncertain or selected["input_all"]:
            selected = dict.fromkeys(selected, True)
    except (KeyError, TypeError, ValueError, OSError, subprocess.CalledProcessError) as error:
        raise ValueError(f"input contract unavailable: {error}") from error
    return selected


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] in ("--check", "--rebind"):
        root = Path(sys.argv[2] if len(sys.argv) > 2 else Path(__file__).resolve().parent.parent).resolve()
        return check(root) if sys.argv[1] == "--check" else rebind(root)
    root = Path(sys.argv[1]).resolve()
    changed = [item.decode("utf-8", "strict") for item in sys.stdin.buffer.read().split(b"\0") if item]
    result = classify(root, changed, comparison_unknown="--unknown" in sys.argv[2:],
                      paths_only="--paths-only" in sys.argv[2:])
    print("local_contract=v1")
    for key, value in sorted(result.items()):
        print(f"{key}={str(value).lower()}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"ci router: {error}; refusing incomplete input selection", file=sys.stderr)
        sys.exit(1)
