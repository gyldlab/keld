"""KEL-130/T1 predecessor validator, owned by the retained-filesystem contract.

The caller supplies authenticated Linear provenance and the approved landed T0 receipt
from its trusted coordination boundary, never from the candidate artifact itself.
This module checks their bindings; it neither authenticates an agent-written JSON file
nor invents a signature scheme. KEL-102/T3 must reuse this validator after fetching
the real Linear author, winning claim and current repository main.
"""

from dataclasses import dataclass
import hashlib
from pathlib import Path
import re
import subprocess


class Invalid(ValueError):
    """A required terminal-artifact predicate failed."""


def require(condition, message):
    if not condition:
        raise Invalid(message)


def text(value, name):
    require(isinstance(value, str) and bool(value.strip()), name + " must be nonempty text")


def digest(value, length, name):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{" + str(length) + "}", value),
            "invalid " + name)


@dataclass(frozen=True)
class Publication:
    """Trusted caller's independently authenticated Linear comment and claim facts.

    These facts are not accepted as self-attested candidate fields. The caller owns
    fetching the comment and its real author and proving repository-owner or standing
    delegate authority for this exact winning KEL-130 claim.
    """

    comment_id: str
    author_id: str
    winning_claim_id: str
    authorized_author_ids: frozenset[str]
    artifact_sha256: str


def git(repo, *arguments):
    result = subprocess.run(["git", "-C", str(repo), *arguments], capture_output=True,
                            timeout=30, check=False)
    require(result.returncode == 0, "Git proof failed: " + " ".join(arguments))
    return result.stdout


def validate(artifact_bytes, *, approved_t0, publication, repo, current_main, evidence_root):
    """Validate exact terminal bytes against external authority, Git and raw evidence.

    `approved_t0` is the already-authenticated passed landed T0 artifact. `current_main`
    is the caller's freshly fetched main SHA, not a value from the candidate. Native
    raw files must be retained under `evidence_root`. Success validates the declared
    evidence bindings; it does not replace independent review of native observables.
    """
    import json

    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON key: " + key)
            result[key] = value
        return result

    require(isinstance(artifact_bytes, bytes), "artifact must be exact bytes")
    try:
        artifact = json.loads(artifact_bytes, object_pairs_hook=unique)
    except (ValueError, UnicodeError) as error:
        raise Invalid("invalid artifact JSON") from error
    require(isinstance(artifact, dict), "artifact must be object")
    identity = {"schema": "keld.execution-artifact/v1", "node_id": "retained-filesystem",
                "issue_id": "KEL-130", "task_id": "KEL-130/T1", "status": "passed"}
    for field, expected in identity.items():
        require(artifact.get(field) == expected, "wrong " + field)
    require(isinstance(publication, Publication), "independent publication facts required")
    for name in ("comment_id", "author_id", "winning_claim_id"):
        text(getattr(publication, name), name)
    require(
        type(publication.authorized_author_ids) is frozenset,
        "authorized_author_ids must be a frozenset",
    )
    require(
        all(
            isinstance(author_id, str) and bool(author_id.strip())
            for author_id in publication.authorized_author_ids
        ),
        "authorized_author_ids must contain nonempty string IDs",
    )
    digest(publication.artifact_sha256, 64, "publication artifact SHA256")
    require(publication.author_id in publication.authorized_author_ids, "unauthorized publisher")
    require(artifact.get("publisher_id") == publication.author_id, "publisher identity mismatch")
    require(artifact.get("claim_id") == publication.winning_claim_id, "winning claim mismatch")
    require(hashlib.sha256(artifact_bytes).hexdigest() == publication.artifact_sha256,
            "candidate differs from authenticated publication")

    for field, expected in (("schema", "keld.execution-artifact/v1"),
                            ("node_id", "retained-filesystem-contract"),
                            ("issue_id", "KEL-130"), ("task_id", "KEL-130/T0"),
                            ("status", "passed")):
        require(approved_t0.get(field) == expected, "invalid approved T0 " + field)
    contract = artifact.get("contract")
    require(isinstance(contract, dict), "missing contract")
    for field, length in (("landed_head", 40), ("spec_blob", 40),
                          ("spec_sha256", 64), ("decision_digest", 64)):
        digest(approved_t0.get(field), length, "approved T0 " + field)
        require(contract.get(field) == approved_t0[field], "wrong contract " + field)
    spec_path = approved_t0.get("spec_path")
    text(spec_path, "T0 spec path")
    landed = artifact.get("landed_head")
    digest(landed, 40, "landed_head")
    digest(current_main, 40, "current_main")
    require(landed != approved_t0["landed_head"], "T0 is not a T1 implementation")
    git(repo, "merge-base", "--is-ancestor", approved_t0["landed_head"], landed)
    git(repo, "merge-base", "--is-ancestor", landed, current_main)
    actual_blob = git(repo, "rev-parse", approved_t0["landed_head"] + ":" + spec_path).decode().strip()
    require(actual_blob == contract["spec_blob"], "T0 spec blob missing from landed contract")
    spec = git(repo, "cat-file", "blob", actual_blob)
    require(hashlib.sha256(spec).hexdigest() == contract["spec_sha256"], "T0 spec bytes mismatch")
    payloads = re.findall(rb'^\{"schema":"keld.kel130-retained-filesystem-decisions/v1"[^\r\n]*', spec, re.M)
    require(len(payloads) == 1 and hashlib.sha256(payloads[0]).hexdigest() == contract["decision_digest"],
            "T0 canonical decision mismatch")

    tasks = artifact.get("tasks")
    require(isinstance(tasks, dict) and set(tasks) == {"T1a", "T1b", "T1c", "T1d"},
            "exact T1a-T1d task rows required")
    require(all(value == "passed" for value in tasks.values()), "incomplete T1 task")
    native = artifact.get("native_evidence")
    require(isinstance(native, list) and len(native) == 3, "three native OS rows required")
    seen_os, seen_devices = set(), set()
    root = Path(evidence_root).resolve(strict=True)
    for row in native:
        require(isinstance(row, dict), "native row must be object")
        system = row.get("os")
        require(system in {"macOS", "Windows", "Linux"} and system not in seen_os,
                "missing or duplicate native OS")
        seen_os.add(system)
        for name in ("device_id", "os_build", "observable"):
            text(row.get(name), name)
        require(row["device_id"] not in seen_devices, "duplicate native device identity")
        seen_devices.add(row["device_id"])
        require(row.get("native") is True, "row must identify actual native execution")
        require(row.get("source_head") == landed, "native source_head must equal landed_head")
        require(type(row.get("exit_code")) is int and row["exit_code"] == 0,
                "native command did not pass")
        command = row.get("command")
        require(isinstance(command, list) and command, "native command required")
        for argument in command:
            text(argument, "command argument")
        raw = row.get("raw_evidence")
        require(isinstance(raw, dict), "raw evidence required")
        text(raw.get("path"), "raw path")
        digest(raw.get("sha256"), 64, "raw evidence SHA256")
        path = Path(raw["path"])
        require(not path.is_absolute() and ".." not in path.parts, "raw path must be under evidence root")
        path = (root / path).resolve(strict=True)
        require(path.is_relative_to(root) and path.is_file(), "raw evidence escapes root")
        contents = path.read_bytes()
        require(contents and hashlib.sha256(contents).hexdigest() == raw["sha256"],
                "raw evidence bytes mismatch")
    return artifact
