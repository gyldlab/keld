"""Consumer input contract used only through the shared CI router.

tools/ci-inputs.json owns the reviewed consumer scopes: which changed inputs select
which gates, and which reader sources implement each consumer. The contract is
stateless (GH-1275): a changed reader source widens its consumers for the change that
touches it, and a crate path that is new or gone relative to the comparison base
selects every lane. No digest of reader bytes is recorded. On a protected base a
recorded digest was always fresh at merge time, so it never widened a later change;
it only produced rebind commits and merge conflicts (October 2026: 346 commits touched
this file, 318 of them digest-only).
This is deliberately a proof boundary, not a source-code dependency guesser.
"""

import fnmatch
import json
from pathlib import Path
import subprocess
import sys

SCHEMA = "keld.ci-inputs/v2"
RETIRED_DIGEST_KEYS = ("sha256", "membership_sha256")
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


def tree_paths(root: Path, revision: str) -> set[str]:
    """Every path tracked at `revision`; the comparison side of membership changes."""
    data = subprocess.check_output(["git", "ls-tree", "-r", "-z", "--name-only", revision], cwd=root)
    return {item.decode("utf-8", "strict") for item in data.split(b"\0") if item}


def load(root: Path) -> dict:
    data = json.loads((root / "tools/ci-inputs.json").read_text(encoding="utf-8"))
    if isinstance(data, dict) and data.get("schema") == "keld.ci-inputs/v1":
        raise ValueError(
            "CI-INPUTS-V1-RETIRED: schema v1 recorded reader digests, which v2 retired (GH-1275). Fix: set "
            f"\"schema\" to \"{SCHEMA}\" and delete every {', '.join(RETIRED_DIGEST_KEYS)} entry")
    if (not isinstance(data, dict) or data.get("schema") != SCHEMA or not isinstance(data.get("consumers"), list)
            or not isinstance(data.get("reader_sets"), dict)):
        raise ValueError(
            f"invalid consumer input contract: expected schema {SCHEMA} with consumers[] and reader_sets{{}}")
    for name, reader_set in data["reader_sets"].items():
        if not isinstance(reader_set, dict):
            raise ValueError(f"invalid reader set {name}")
        patterns_list(reader_set.get("patterns"), f"reader set {name} patterns", allow_empty=False)
        retired = [key for key in RETIRED_DIGEST_KEYS if key in reader_set]
        if retired:
            raise ValueError(f"reader set {name} still records {', '.join(retired)}; CI-INPUTS-V1-RETIRED: "
                             "v2 stores no digests, delete the entry (GH-1275)")
    census = data.get("rust_source_census")
    if census is not None:
        if not isinstance(census, dict):
            raise ValueError("invalid rust_source_census")
        patterns_list(census.get("patterns"), "rust_source_census patterns", allow_empty=False)
        retired = [key for key in RETIRED_DIGEST_KEYS if key in census]
        if retired:
            raise ValueError(f"rust_source_census still records {', '.join(retired)}; CI-INPUTS-V1-RETIRED: "
                             "membership is compared against the base tree, delete the entry (GH-1275)")
    return data


def patterns_list(value, label: str, *, allow_empty: bool) -> list[str]:
    if (not isinstance(value, list) or (not value and not allow_empty)
            or any(not isinstance(pattern, str) or not pattern for pattern in value)):
        raise ValueError(f"invalid {label}")
    return value


def documentation_only(changed: list[str], other: list[str], documentation: list[str]) -> bool:
    """True when this edge matched only reviewed documentation reads.

    A path that also matches a non-documentation input or reader pattern is
    never documentation-only, so a real source or build input keeps the
    caller's reverse-dependent expansion.
    """
    return (any(matches(path, documentation) for path in changed)
            and not any(matches(path, other) for path in changed))


def membership_changed(root: Path, changed: list[str], patterns: list[str], base: str | None,
                       census: list[str]) -> bool:
    """A changed path under the census patterns that the comparison base did not track.

    With a base revision (the router's range and worktree modes) the base tree is the
    oracle for every candidate; a removed path is handled by the caller's missing-file
    fallback. Path-only classification has no base and skips that fallback: a candidate
    that exists in the live census but not in HEAD is new, a candidate HEAD tracks that is
    missing from the working tree is removed, and a path that exists nowhere is routed by
    its input edges alone. Without a readable HEAD nothing proves membership, so any
    candidate fails closed.
    """
    candidates = [path for path in changed if matches(path, patterns)]
    if not candidates:
        return False
    if base:
        before = tree_paths(root, base)
        return any(path not in before for path in candidates)
    try:
        head = tree_paths(root, "HEAD")
    except (OSError, subprocess.CalledProcessError):
        return True
    live = set(census)
    return any((path in live and path not in head) or (path in head and not (root / path).is_file())
               for path in candidates)


REVIEW = ("ci-inputs: reader source changed in {sets}; every consumer behind it runs fully for this change. "
          "If the reader gained a new cross-tree read, add that input edge to consumers[].inputs of "
          "tools/ci-inputs.json in the same change; routing records nothing else.")


def changed_reader_sets(contract: dict, changed: list[str]) -> list[str]:
    """Reader sets with a member in the diff, in contract order; the CLI prints the review prompt."""
    return [name for name, reader_set in contract["reader_sets"].items()
            if any(matches(path, reader_set["patterns"]) for path in changed)]


def classify(root: Path, changed: list[str], *, comparison_unknown=False, paths_only=False,
             base: str | None = None) -> dict[str, bool]:
    selected = {"local_" + gate: True for gate in MANDATORY}
    selected.update({"input_rust": False, "input_ts": False, "input_all": False,
                     "input_router": False, "local_default": True})
    # Computed per edge below and reset after every fallback: a fallback can
    # only widen a selection, never turn on narrowing.
    narrowing: dict[str, bool] = {}
    try:
        contract = load(root)
        census = files(root)
        rust_census = contract.get("rust_source_census")
        if rust_census and membership_changed(root, changed, rust_census["patterns"], base, census):
            # A new crate source can be a new cross-tree reader with no edge
            # registered yet. Its package or platform exclusions are not proven.
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
            allowed = {"input_rust", "input_ts", "input_all", "input_registry", "input_mermaid", "input_hygiene"}
            if not outputs or any(not (output.startswith("local_") or output in allowed) for output in outputs):
                raise ValueError("invalid consumer output")
            reader_set = contract["reader_sets"][consumer["reader_set"]]
            drift = any(matches(path, reader_set["patterns"]) for path in changed)
            if drift and "input_rust" in outputs:
                # A changed Rust reader can introduce an edge outside Cargo
                # metadata (including a host/build reader). Its old package or
                # platform exclusions are no longer proven for this change.
                selected["input_all"] = True
            documentation = patterns_list(consumer.get("documentation_inputs", []),
                                          "consumer documentation inputs", allow_empty=True)
            other = consumer["inputs"] + reader_set["patterns"]
            relevant = uncertain or drift or any(matches(path, other + documentation) for path in changed)
            for output in outputs:
                selected[output] = selected.get(output, False) or relevant
            if "input_registry" in outputs:
                # `other` contains the reader patterns, so a reader change in the
                # diff is never documentation-only.
                narrowing["documentation_only_registry"] = documentation_only(changed, other, documentation)
        package_inputs = contract.get("rust_package_inputs", {})
        package_documentation = contract.get("rust_package_documentation_inputs", {})
        if not isinstance(package_inputs, dict) or not isinstance(package_documentation, dict):
            raise ValueError("invalid Rust consumer package mapping")
        for package in sorted(set(package_inputs) | set(package_documentation)):
            if (not isinstance(package, str) or not package.startswith("keld-")
                    or not package.isascii() or not package.replace("-", "").isalnum()):
                raise ValueError("invalid Rust consumer package")
            inputs = patterns_list(package_inputs.get(package, []), "Rust consumer input patterns",
                                   allow_empty=package not in package_inputs)
            documentation = patterns_list(package_documentation.get(package, []),
                                          "Rust consumer documentation patterns",
                                          allow_empty=package not in package_documentation)
            relevant = any(matches(path, inputs + documentation) for path in changed)
            selected["input_package_" + package] = relevant
            narrowing["documentation_only_package_" + package] = documentation_only(
                changed, inputs, documentation)
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
        widened = uncertain or selected["input_all"] or "tools/ci-inputs.json" in changed
        for key, value in narrowing.items():
            selected[key] = value and not widened
    except (KeyError, TypeError, ValueError, OSError, subprocess.CalledProcessError) as error:
        raise ValueError(f"input contract unavailable: {error}") from error
    return selected


def parse_options(options: list[str]) -> tuple[bool, bool, str | None]:
    """`--unknown`, `--paths-only` and `--base <revision>`; anything else is a usage error."""
    comparison_unknown = False
    paths_only = False
    base = None
    index = 0
    while index < len(options):
        option = options[index]
        if option == "--unknown":
            comparison_unknown = True
        elif option == "--paths-only":
            paths_only = True
        elif option == "--base":
            if index + 1 >= len(options) or not options[index + 1] or options[index + 1].startswith("--"):
                raise ValueError("--base requires a revision")
            base = options[index + 1]
            index += 1
        else:
            raise ValueError(f"unknown option {option}")
        index += 1
    return comparison_unknown, paths_only, base


def main() -> int:
    if len(sys.argv) < 2 or sys.argv[1].startswith("--"):
        raise ValueError("usage: ci_inputs.py <root> [--unknown] [--paths-only] [--base <revision>] < NUL-paths")
    root = Path(sys.argv[1]).resolve()
    comparison_unknown, paths_only, base = parse_options(sys.argv[2:])
    changed = [item.decode("utf-8", "strict") for item in sys.stdin.buffer.read().split(b"\0") if item]
    result = classify(root, changed, comparison_unknown=comparison_unknown, paths_only=paths_only, base=base)
    drifted = changed_reader_sets(load(root), changed)
    if drifted:
        print(REVIEW.format(sets=", ".join(drifted)), file=sys.stderr)
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
