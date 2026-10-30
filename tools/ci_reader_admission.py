"""GH-1275 isolated review-signature format verifier, NOT a production CI gate.

The CLI verifies exact Git bytes and an SSH signature, but its caller chooses
the signer pin, baseline and head. That alone does NOT authenticate authority:
a PR author can forge all three. Only a future protected-base invoker may
supply externally pinned identities; the root-only adapter below hard-fails
until such independent authority is actually available.
"""

import argparse
import fnmatch
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess
import sys
import unicodedata

from ci_reader_git_integrity import index_or_graft_mismatch

PROOF_PREFIX = ".ci-reader-proofs/readers/"
PROOF_SCHEMA = "keld.ci-reader-proof/v1"
REVIEW_SCHEMA = "keld.ci-reader-review/v1"
POLICY_SCHEMA = "keld.ci-inputs/v1"
SIGN_NAMESPACE = "keld-ci-reader-admission-v1"
CODEOWNER = "/.ci-reader-proofs/ @gyldlab/keld-maintainers"
HEX40 = re.compile(r"[0-9a-f]{40}\Z")
HEX64 = re.compile(r"[0-9a-f]{64}\Z")

class AdmissionError(Exception):
    """A missing or falsified admission prerequisite; selection is forbidden."""

def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AdmissionError(detail)

def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def normalized(data: bytes) -> bytes:
    return data.replace(b"\r\n", b"\n")

def unique_pairs(items: list[tuple[str, object]]) -> dict:
    obj = {}
    for name, value in items:
        require(name not in obj, f"duplicate JSON field: {name}")
        obj[name] = value
    return obj

def json_object(content: bytes, label: str) -> dict:
    try:
        value = json.loads(
            content.decode("utf-8", "strict"),
            object_pairs_hook=unique_pairs,
            parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite number")),
        )
    except (UnicodeError, ValueError) as error:
        raise AdmissionError(f"{label}: malformed JSON: {error}") from error
    require(type(value) is dict, f"{label}: expected JSON object")
    return value

def shape(value: object, fields: set[str], label: str) -> dict:
    require(type(value) is dict, f"{label}: expected object")
    require(set(value) == fields, f"{label}: missing or unexpected field")
    return value

def text(value: object, label: str) -> str:
    require(type(value) is str and bool(value), f"{label}: expected nonempty string")
    return value

def digest_string(value: object, label: str) -> str:
    value = text(value, label)
    require(HEX64.fullmatch(value) is not None, f"{label}: malformed SHA-256")
    return value

def git(repo: Path, *args: str, allow_failure=False) -> bytes:
    try:
        result = subprocess.run(
            ["git", "--no-replace-objects", *args], cwd=repo, capture_output=True, check=False,
        )
    except OSError as error:
        raise AdmissionError(f"git unavailable: {error}") from error
    if not allow_failure:
        require(result.returncode == 0, f"Git command failed: {args[0]} (exit {result.returncode})")
    return result.stdout if result.returncode == 0 else b""

def checked_oid(value: str, name: str) -> str:
    require(HEX40.fullmatch(value or "") is not None, f"{name}: requires immutable full SHA-1 commit")
    return value

def safe_path(value: object) -> str:
    path = text(value, "source/proof path")
    require(
        not path.startswith("/") and not path.endswith("/") and
        "\\" not in path and "\0" not in path and
        all(part not in ("", ".", "..") for part in path.split("/")) and
        unicodedata.normalize("NFC", path) == path and
        all(ord(char) >= 32 for char in path),
        f"invalid source/proof path: {path!r}",
    )
    return path

def path_aliases(paths: set[str]) -> None:
    seen = {}
    for path in sorted(paths):
        folded = unicodedata.normalize("NFC", path).casefold()
        require(folded not in seen, f"case/Unicode path alias: {seen.get(folded)} vs {path}")
        seen[folded] = path

def tree(repo: Path, commit: str) -> dict[str, tuple[str, str]]:
    output = git(repo, "ls-tree", "-r", "-z", "--full-tree", commit)
    result = {}
    for row in output.split(b"\0"):
        if not row:
            continue
        try:
            info, raw_name = row.split(b"\t", 1)
            mode, kind, oid = info.decode("ascii").split(" ")
            name = safe_path(raw_name.decode("utf-8", "strict"))
        except (ValueError, UnicodeError) as error:
            raise AdmissionError(f"invalid Git tree path: {error}") from error
        require(name not in result, f"duplicate Git tree path: {name}")
        require(kind in ("blob", "commit"), f"invalid Git tree entry: {name}")
        result[name] = (mode, oid)
    path_aliases(set(result))
    return result

def git_blob(repo: Path, entry: tuple[str, str]) -> bytes:
    require(entry[0] in ("100644", "100755"), "Git mode is not a regular file (symlink/submodule)")
    return git(repo, "cat-file", "blob", entry[1])

def regular_worktree_bytes(root: Path, relative: str, entry: tuple[str, str],
                           *, kind="source") -> bytes:
    path = root
    for part in relative.split("/"):
        path = path / part
        try:
            info = path.lstat()
        except OSError as error:
            raise AdmissionError(f"missing tracked {kind}: {relative}: {error}") from error
        require(not stat.S_ISLNK(info.st_mode), f"symlink {kind} or ancestor: {relative}")
    require(stat.S_ISREG(info.st_mode), f"{kind} is not a regular file: {relative}")
    require(entry[0] in ("100644", "100755"), f"Git symlink or nonregular {kind}: {relative}")
    try:
        content = path.read_bytes()
    except OSError as error:
        raise AdmissionError(f"cannot read {kind}: {relative}: {error}") from error
    committed = git_blob(root, entry)
    require(normalized(content) == normalized(committed),
            f"uncommitted or missing {kind} bytes: {relative}")
    return normalized(content)

def external_bytes(repo: Path, path: Path, label: str) -> bytes:
    require(path.is_absolute(), f"{label}: independent authority path must be absolute")
    require(not path.is_symlink(), f"{label}: symlink external authority is refused")
    real = path.resolve()
    require(not real.is_relative_to(repo), f"{label}: PR-controlled authority path is forbidden")
    require(real.is_file(), f"{label}: missing independent authority input")
    return real.read_bytes()

def verify_signature(repo: Path, receipt_path: Path, signature_path: Path,
                     signers_path: Path, expected_signers_sha256: str) -> dict:
    require(bool(expected_signers_sha256), "independent signer pin unavailable; hard fail")
    digest_string(expected_signers_sha256, "independent signer pin")
    signers = external_bytes(repo, signers_path, "signer")
    require(sha256(signers) == expected_signers_sha256, "independent signer pin mismatch")
    receipt_bytes = external_bytes(repo, receipt_path, "review receipt")
    external_bytes(repo, signature_path, "review signature")
    receipt = json_object(receipt_bytes, "review receipt")
    shape(receipt, {
        "schema", "baseline", "head", "reviewer", "decision", "policy_sha256",
        "codeowners_sha256", "proofs", "approved_edges",
    }, "review receipt")
    require(receipt["schema"] == REVIEW_SCHEMA and receipt["decision"] == "approve",
            "review receipt does not approve this schema")
    reviewer = text(receipt["reviewer"], "reviewer principal")
    require(not any(c.isspace() for c in reviewer) and len(reviewer) <= 256,
            "invalid reviewer principal")
    try:
        verified = subprocess.run(
            ["ssh-keygen", "-Y", "verify", "-f", str(signers_path), "-I",
             reviewer, "-n", SIGN_NAMESPACE, "-s", str(signature_path)],
            input=receipt_bytes, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as error:
        raise AdmissionError(f"signature verifier unavailable: {error}") from error
    require(verified.returncode == 0, "independent review signature verification failed")
    return receipt

def parsed_policy(root: Path, entries: dict[str, tuple[str, str]]) -> tuple[dict, bytes]:
    path = "tools/ci-inputs.json"
    require(path in entries, "missing CI reader policy")
    content = regular_worktree_bytes(root, path, entries[path], kind="policy")
    policy = json_object(content, "CI reader policy")
    require(policy.get("schema") == POLICY_SCHEMA, "CI reader policy schema mismatch")
    require(type(policy.get("reader_sets")) is dict and type(policy.get("consumers")) is list,
            "missing reader scope")
    require(type(policy.get("known_inputs")) is list, "missing known-input policy")
    return policy, content

def reader_patterns(policy: dict) -> dict[str, list[str]]:
    result = {}
    for name, data in policy["reader_sets"].items():
        require(type(name) is str and name and type(data) is dict, "invalid reader set")
        patterns = data.get("patterns")
        require(type(patterns) is list and bool(patterns) and
                all(type(x) is str and x for x in patterns), "invalid reader-set pattern")
        result[name] = patterns
    require(bool(result), "no reader sets")
    return result

def consumers(policy: dict) -> dict[str, dict]:
    result = {}
    outputs = set()
    for item in policy["consumers"]:
        require(type(item) is dict, "invalid consumer")
        name = text(item.get("owner"), "consumer owner")
        require(name not in result, "duplicate consumer owner")
        require(type(item.get("outputs")) is list and bool(item["outputs"]) and
                all(type(v) is str and v for v in item["outputs"]), "invalid consumer outputs")
        for output in item["outputs"]:
            require(output not in outputs, "duplicate/shadowed consumer output")
            outputs.add(output)
        require(type(item.get("reader_set")) is str and
                item["reader_set"] in policy["reader_sets"], "consumer references missing reader set")
        for key in ("inputs", "documentation_inputs"):
            value = item.get(key, [] if key == "documentation_inputs" else None)
            require(type(value) is list and all(type(v) is str and v for v in value),
                    f"invalid {key}")
        result[name] = item
    require(bool(result), "no consumers")
    return result

def verify_scope(base: dict, current: dict) -> tuple[dict[str, list[str]], dict[str, dict], dict[str, dict]]:
    require(set(base) == set(current), "CI policy field inventory changed")
    base_sets, now_sets = reader_patterns(base), reader_patterns(current)
    require(base_sets == now_sets, "reader-set registration/pattern change requires separate review")
    for name in base_sets:
        before, after = base["reader_sets"][name], current["reader_sets"][name]
        require(set(before) == set(after), "reader-set field change")
        for key in set(before) - {"sha256"}:
            require(before[key] == after[key], "reader-set scope changed")
    before_consumers, now_consumers = consumers(base), consumers(current)
    require(set(before_consumers) == set(now_consumers), "consumer owner deleted or renamed")
    for name, before in before_consumers.items():
        after = now_consumers[name]
        require(set(before) == set(after), "consumer field inventory changed")
        for key in set(before) - {"inputs"}:
            require(before[key] == after[key], "consumer output/documentation scope changed")
        require(set(before["inputs"]).issubset(after["inputs"]), "consumer input edge removed")
    for key in set(base) - {"reader_sets", "consumers", "rust_source_census"}:
        require(base[key] == current[key], f"policy field {key} changed")
    require(type(base.get("rust_source_census")) is dict and
            type(current.get("rust_source_census")) is dict and
            base["rust_source_census"].get("patterns") == current["rust_source_census"].get("patterns"),
            "Rust source census changed")
    return now_sets, before_consumers, now_consumers

def watched(path: str, patterns: dict[str, list[str]]) -> list[str]:
    return sorted(name for name, values in patterns.items()
                  if any(fnmatch.fnmatchcase(path, pattern) for pattern in values))

def historical_added(repo: Path, baseline: str, head: str) -> set[str]:
    result = git(repo, "log", "--format=", "--name-only", "--no-renames",
                 "--diff-filter=A", "-m", "-z", f"{baseline}..{head}", "--")
    paths = set()
    for raw in result.split(b"\0"):
        if not raw:
            continue
        try:
            paths.add(safe_path(raw.decode("utf-8", "strict")))
        except UnicodeError as error:
            raise AdmissionError(f"invalid historical path: {error}") from error
    path_aliases(paths)
    return paths

def verify_unknown(root: Path, patterns: dict[str, list[str]], rust_patterns: list[str]) -> None:
    for kind, flags in (
        ("untracked", ["--others", "--exclude-standard"]),
        ("ignored", ["--others", "--ignored", "--exclude-standard"]),
    ):
        data = git(root, "ls-files", "-z", *flags, "--")
        for raw in data.split(b"\0"):
            if not raw:
                continue
            try:
                path = safe_path(raw.decode("utf-8", "strict"))
            except UnicodeError as error:
                raise AdmissionError(f"invalid {kind} path: {error}") from error
            if path.startswith(".ci-reader-proofs/") or watched(path, patterns) or any(
                fnmatch.fnmatchcase(path, pattern) for pattern in rust_patterns
            ):
                raise AdmissionError(f"{kind} source/proof path cannot certify CI: {path}")

def proof_filename(path: str) -> str:
    return PROOF_PREFIX + sha256(path.encode("utf-8")) + ".json"

def verify_proof(root: Path, filename: str, entries: dict[str, tuple[str, str]],
                 baseline: str, source: str, expected_state: str,
                 expected_sets: list[str]) -> tuple[dict, bytes]:
    require(filename in entries, f"missing {expected_state} tombstone/source proof: {source}")
    content = regular_worktree_bytes(root, filename, entries[filename], kind="proof")
    proof = json_object(content, "reader proof")
    shape(proof, {"schema", "baseline", "path", "reader_sets", "state", "sha256", "edges"},
          "reader proof")
    require(proof["schema"] == PROOF_SCHEMA and proof["baseline"] == baseline,
            "proof baseline/schema mismatch")
    require(safe_path(proof["path"]) == source and proof_filename(source) == filename,
            "proof path/name is not bound to the exact reader")
    require(proof["reader_sets"] == expected_sets, "proof reader-set membership mismatch")
    require(proof["state"] == expected_state, f"unexpected proof state: {source}")
    if expected_state == "retired":
        require(proof["sha256"] is None, "retirement tombstone must have null digest")
    else:
        digest_string(proof["sha256"], "reader source digest")
        require(source in entries, "source disappeared while verifying proof")
        actual = regular_worktree_bytes(root, source, entries[source])
        require(proof["sha256"] == sha256(actual), "source proof digest mismatch")
    require(type(proof["edges"]) is list, "invalid proof edge list")
    return proof, content

def admission(root: Path, baseline: str, head: str, receipt_path: Path,
              signature_path: Path, signers_path: Path, signer_pin: str) -> int:
    require(root.is_dir(), "Git repository directory missing")
    root = root.resolve(strict=True)
    checked_oid(baseline, "baseline")
    checked_oid(head, "head")
    require(git(root, "rev-parse", "--show-toplevel").decode().strip() == str(root),
            "Git repository root is not the requested candidate")
    require(git(root, "rev-parse", "--is-shallow-repository").strip() == b"false",
            "shallow Git history cannot prove added-then-retired readers")
    require(git(root, "rev-parse", "--show-object-format").decode().strip() == "sha1",
            "unsupported Git object format")
    require(git(root, "rev-parse", "HEAD").decode().strip() == head,
            "stale reviewed head: current HEAD is different")
    require(git(root, "cat-file", "-t", baseline, allow_failure=True).strip() == b"commit",
            "missing immutable Git baseline commit")
    require(git(root, "cat-file", "-t", head, allow_failure=True).strip() == b"commit",
            "missing immutable head commit")
    ancestor = subprocess.run(["git", "--no-replace-objects", "merge-base", "--is-ancestor", baseline, head],
                              cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    require(ancestor.returncode == 0, "baseline is not a proven ancestor of exact head")

    receipt = verify_signature(root, receipt_path, signature_path, signers_path, signer_pin)
    require(receipt["baseline"] == baseline, "review receipt baseline mismatch")
    require(receipt["head"] == head, "stale review receipt head")
    baseline_tree, current_tree = tree(root, baseline), tree(root, head)
    integrity_issue = index_or_graft_mismatch(root, current_tree, git)
    require(integrity_issue is None, integrity_issue or "Git index/history rejected")
    baseline_policy, _ = parsed_policy_at_commit(root, baseline_tree)
    policy, policy_bytes = parsed_policy(root, current_tree)
    require(digest_string(receipt["policy_sha256"], "reviewed policy digest") == sha256(policy_bytes),
            "reviewed policy bytes mismatch")
    sets, before_consumers, after_consumers = verify_scope(baseline_policy, policy)
    rust = policy["rust_source_census"]["patterns"]
    require(type(rust) is list and bool(rust) and all(type(x) is str and x for x in rust),
            "invalid Rust census patterns")
    verify_unknown(root, sets, rust)
    owner = ".github/CODEOWNERS"
    require(owner in current_tree, "CODEOWNERS missing")
    owner_bytes = regular_worktree_bytes(root, owner, current_tree[owner], kind="CODEOWNERS")
    owner_rules = [row for row in owner_bytes.decode("utf-8", "strict").splitlines()
                   if row.strip() and not row.lstrip().startswith("#")]
    require(owner_rules and owner_rules[-1].split() == CODEOWNER.split(),
            "CODEOWNERS proof rule must be final so no later rule overrides it")
    require(digest_string(receipt["codeowners_sha256"], "reviewed CODEOWNERS") == sha256(owner_bytes),
            "reviewed CODEOWNERS bytes mismatch")

    source_patterns = lambda p: bool(watched(p, sets) or any(
        fnmatch.fnmatchcase(p, item) for item in rust
    ))
    candidates = {p for p in current_tree if source_patterns(p)}
    bases = {p for p in baseline_tree if source_patterns(p)}
    historic = {p for p in historical_added(root, baseline, head) if source_patterns(p)}
    path_aliases(candidates | bases | historic)
    changes = {}
    for path in sorted(candidates | bases | historic):
        prior, live = baseline_tree.get(path), current_tree.get(path)
        if live is None:
            changes[path] = "retired"
        elif prior is None:
            changes[path] = "present"
        elif watched(path, sets):
            before = normalized(git_blob(root, prior))
            now = normalized(git_blob(root, live))
            if before != now or prior[0] != live[0]:
                changes[path] = "present"
        if live is not None:
            regular_worktree_bytes(root, path, live)

    actual_proof_files = {
        path for path in current_tree if path.startswith(".ci-reader-proofs/")
    }
    expected_proof_files = {proof_filename(path) for path in changes}
    for name in actual_proof_files:
        require(name.startswith(PROOF_PREFIX) and name.endswith(".json"),
                f"unknown proof path outside owned reader namespace: {name}")
    missing = expected_proof_files - actual_proof_files
    require(not missing, f"missing source proof or retirement tombstone: {sorted(missing)}")
    require(actual_proof_files == expected_proof_files,
            f"orphan or extra proof path: {sorted(actual_proof_files - expected_proof_files)}")

    receipt_entries = receipt["proofs"]
    require(type(receipt_entries) is list, "review receipt proofs must be a list")
    reviewed_proofs = {}
    for item in receipt_entries:
        shape(item, {"path", "sha256"}, "reviewed proof entry")
        name = safe_path(item["path"])
        require(name not in reviewed_proofs, "duplicate reviewed proof")
        reviewed_proofs[name] = digest_string(item["sha256"], "reviewed proof digest")
    require(set(reviewed_proofs) == expected_proof_files,
            "review receipt missing or extra proof approval")

    all_edges = []
    for path, state in sorted(changes.items()):
        name = proof_filename(path)
        applicable = watched(path, sets)
        proof, content = verify_proof(root, name, current_tree, baseline, path, state, applicable)
        require(sha256(content) == reviewed_proofs[name],
                "independently reviewed proof bytes mismatch")
        seen_edges = set()
        for edge in proof["edges"]:
            shape(edge, {"consumer", "input"}, "reader edge")
            consumer = text(edge["consumer"], "edge consumer")
            source_input = text(edge["input"], "edge input")
            require(consumer in after_consumers, "edge references unknown consumer")
            target = after_consumers[consumer]
            require(target["reader_set"] in applicable, "edge consumer does not own reader set")
            require(any(fnmatch.fnmatchcase(source_input, rule) for rule in target["inputs"] +
                        target.get("documentation_inputs", [])), "edge omitted from consumer inputs")
            identity = (consumer, source_input)
            require(identity not in seen_edges, "duplicate reader edge")
            seen_edges.add(identity)
            all_edges.append({"reader": path, "consumer": consumer, "input": source_input})
        if state == "present":
            # Every newly declared cross-tree input for this reader's consumer
            # must be explicitly attested; a byte-stamp alone cannot add an edge.
            for consumer, item in after_consumers.items():
                if item["reader_set"] in applicable:
                    newly_declared = set(item["inputs"]) - set(before_consumers[consumer]["inputs"])
                    for newly_added in newly_declared:
                        require((consumer, newly_added) in seen_edges,
                                f"edge omitted from reviewed reader proof: {path} -> {newly_added}")

    signed_edges = receipt["approved_edges"]
    require(type(signed_edges) is list, "review receipt approved edges must be a list")
    for edge in signed_edges:
        shape(edge, {"reader", "consumer", "input"}, "reviewed source edge")
        safe_path(edge["reader"])
        text(edge["consumer"], "reviewed edge owner")
        text(edge["input"], "reviewed edge input")
    edge_key = lambda item: (item["reader"], item["consumer"], item["input"])
    require(sorted(map(edge_key, signed_edges)) == sorted(map(edge_key, all_edges)),
            "independent edge review does not match declared source edges")
    return len(changes)

def parsed_policy_at_commit(root: Path, entries: dict[str, tuple[str, str]]) -> tuple[dict, bytes]:
    path = "tools/ci-inputs.json"
    require(path in entries, "immutable baseline policy missing")
    content = normalized(git_blob(root, entries[path]))
    policy = json_object(content, "baseline CI reader policy")
    require(policy.get("schema") == POLICY_SCHEMA, "baseline CI policy schema mismatch")
    return policy, content

def admit_shadow_contract(root: Path) -> dict[str, object]:
    """Fail closed: no authenticated, protected-base caller exists yet."""
    require(isinstance(root, Path), "candidate root must be a pathlib.Path")
    raise AdmissionError(
        "protected immutable baseline/head and independently pinned reviewer "
        "authority unavailable; CI admission is disabled"
    )

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--allowed-signers", type=Path, required=True)
    parser.add_argument("--allowed-signers-sha256")
    parser.add_argument("--fixture-verify-only", action="store_true")
    args = parser.parse_args()
    try:
        require(args.fixture_verify_only, "protected-base authority absent; CLI admission disabled")
        approved = admission(
            args.repo, args.baseline, args.head, args.receipt, args.signature,
            args.allowed_signers, args.allowed_signers_sha256,
        )
        print(f"CI-READER-ADMISSION: fixture verified (UNTRUSTED); {approved} reader/path changes")
        return 0
    except (AdmissionError, OSError, UnicodeError, ValueError) as error:
        print(f"CI-READER-ADMISSION: refused: {error}", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())
