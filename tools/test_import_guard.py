#!/usr/bin/env python3
"""Reject newly introduced wildcard imports in owner-local Rust test modules.

Only changed *source imports* are considered; untouched legacy imports are
not a retroactive build gate. This is part of the test-layout acceptance check.
"""
import argparse
import re
import subprocess
from difflib import SequenceMatcher
from pathlib import Path

# Rust visibility may prefix a use item, including pub(in path).
IMPORT = re.compile(r"^[ \t]*(?:pub(?:\s*\([^)]*\)\s*|\s+))?use(?=\s|::|\{)[^;]*;", re.M | re.S)
RAW = re.compile(r"(?:br|r)(\#*)\"")
# A char or byte-char literal ('x', '\n', '\x41', '\u{1F600}', '\'', '"'; the `b`
# prefix is an ordinary code byte). It is recognised only when its closing quote
# follows, so lifetimes and loop labels ('a, 'outer:) stay in the normal state.
CHAR = re.compile(r"'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f_]{1,6}\}|[^xu\n])|[^'\\\n])'")


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def is_owner_test(path):
    parts = path.split("/")
    # crates/<crate>/src/<file>: crate-root tests.rs and <name>_tests.rs included.
    if len(parts) < 4 or parts[0] != "crates" or parts[2] != "src":
        return False
    name = parts[-1]
    return name.endswith(".rs") and (name == "tests.rs" or name.endswith("_tests.rs")
           or any(p == "tests" or p.endswith("_tests") for p in parts[3:-1]))


def masked_code(content):
    """Mask comments and Rust string literals, retaining source line numbers."""
    code = []
    i = 0
    state = "normal"
    depth = 0
    end_raw = ""
    while i < len(content):
        ch = content[i]
        if state == "normal":
            raw = RAW.match(content, i)
            if raw:
                end_raw = '"' + raw.group(1)
                code.extend(" " * len(raw.group()))
                i = raw.end()
                state = "raw"
                continue
            char = CHAR.match(content, i)
            if char:
                code.extend(" " * len(char.group()))
                i = char.end()
                continue
            if content.startswith("//", i):
                code.extend("  ")
                i += 2
                state = "line"
                continue
            if content.startswith("/*", i):
                code.extend("  ")
                i += 2
                depth = 1
                state = "block"
                continue
            if ch == '"':
                code.append(" ")
                state = "string"
                i += 1
                continue
            code.append(ch)
            i += 1
        elif state == "raw":
            if content.startswith(end_raw, i):
                code.extend(" " * len(end_raw))
                i += len(end_raw)
                state = "normal"
            else:
                code.append("\n" if ch == "\n" else " ")
                i += 1
        elif state == "string":
            if ch == "\\" and i + 1 < len(content):
                # Keep the newline of a `\`-newline line continuation so masked
                # text and source keep identical line numbers.
                code.append(" ")
                code.append("\n" if content[i + 1] == "\n" else " ")
                i += 2
            elif ch == '"':
                code.append(" ")
                i += 1
                state = "normal"
            else:
                code.append("\n" if ch == "\n" else " ")
                i += 1
        elif state == "line":
            if ch == "\n":
                code.append("\n")
                state = "normal"
            else:
                code.append(" ")
            i += 1
        else:
            if content.startswith("/*", i):
                depth += 1
                code.extend("  ")
                i += 2
            elif content.startswith("*/", i):
                depth -= 1
                code.extend("  ")
                i += 2
                if depth == 0:
                    state = "normal"
            else:
                code.append("\n" if ch == "\n" else " ")
                i += 1
    return "".join(code)


def glob_imports(masked):
    """Yield source line sets and trivia-free identities of glob use items."""
    imports = []
    for match in IMPORT.finditer(masked):
        if "*" not in match.group():
            continue
        start_line = masked.count("\n", 0, match.start()) + 1
        end_line = start_line + match.group().count("\n")
        imports.append((set(range(start_line, end_line + 1)),
                        re.sub(r"\s+", "", match.group())))
    return imports


def active_glob_lines(content, code_lines_only=False):
    masked = masked_code(content)
    found = set()
    for lines, _identity in glob_imports(masked):
        found.update(lines)
    if code_lines_only:
        # A comment-only insertion inside a legacy multiline import is not
        # a new source import, even though its line lies inside the match.
        lines = masked.splitlines()
        found = {line for line in found if lines[line - 1].strip()}
    return found


def added_lines(diff):
    current = None
    selected = set()
    for line in diff.splitlines():
        if line.startswith("@@ "):
            found = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)", line)
            if not found:
                raise ValueError("unexpected Git hunk header")
            current = int(found.group(1))
        elif current is not None:
            if line.startswith("+") and not line.startswith("+++"):
                selected.add(current)
                current += 1
            elif line.startswith(" "):
                current += 1
    return selected


def block_scopes(masked):
    """Track enclosing brace opening lines and headers in the current revision."""
    scopes = {1: ()}
    blocks = []
    start = 0
    line = 1
    for offset, ch in enumerate(masked):
        if ch == "\n":
            line += 1
            scopes[line] = tuple(blocks)
        elif ch == "{":
            header = re.sub(r"\s+", "", masked[start:offset])
            blocks.append((line, header))
            start = offset + 1
        elif ch == "}":
            if blocks:
                blocks.pop()
            start = offset + 1
        elif ch == ";":
            start = offset + 1
    return scopes


def unchanged_line_provenance(diff, old_count, new_count):
    """Map Git's unchanged before-lines to their positions after the patch."""
    mapped = {}
    old_next = new_next = 1
    for line in diff.splitlines():
        if not line.startswith("@@ "):
            continue
        hunk = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))?", line)
        if not hunk:
            raise ValueError("unexpected Git hunk header")
        old_start, old_span, new_start, new_span = hunk.groups()
        old_start, new_start = int(old_start), int(new_start)
        old_span = int(old_span) if old_span is not None else 1
        new_span = int(new_span) if new_span is not None else 1
        until = old_start if old_span == 0 else old_start - 1
        while old_next <= until:
            mapped[old_next] = new_next
            old_next += 1
            new_next += 1
        old_next = old_start + old_span + (old_span == 0)
        new_next = new_start + new_span + (new_span == 0)
    while old_next <= old_count and new_next <= new_count:
        mapped[old_next] = new_next
        old_next += 1
        new_next += 1
    return mapped


def trivia_only_changed_lines(diff, before, after):
    """Find changed lines whose code tokens match an old line in that hunk.

    Matching only inside a Git replacement hunk prevents a removed legacy
    import from excusing a new identical import in a different module.
    """
    old_raw, new_raw = before.splitlines(), after.splitlines()
    before_code, after_code = masked_code(before), masked_code(after)
    old_code, new_code = before_code.splitlines(), after_code.splitlines()
    old_scopes, new_scopes = block_scopes(before_code), block_scopes(after_code)
    retained_lines = unchanged_line_provenance(diff, len(old_code), len(new_code))
    old_glob = set().union(*(lines for lines, _ in glob_imports(before_code)))
    new_glob = set().union(*(lines for lines, _ in glob_imports(after_code)))
    old_changed, new_changed = [], []
    unchanged = set()

    def finish_hunk():
        # If an entire replacement hunk has the same source tokens after
        # masking comments and trivia, it cannot introduce a glob import.
        # This also covers a comment-only change on an enclosing brace line.
        old_hunk = "".join(re.sub(r"\s+", "", old_code[n - 1]) for n in old_changed)
        new_hunk = "".join(re.sub(r"\s+", "", new_code[n - 1]) for n in new_changed)
        if old_hunk == new_hunk:
            unchanged.update(n for n in new_changed if n in new_glob)
            old_changed.clear()
            new_changed.clear()
            return
        # Only lines containing actual glob-import code can need a trivia
        # exemption. Other changed comments never influence the guard.
        old_changed[:] = [n for n in old_changed if n in old_glob and old_code[n - 1].strip()]
        new_changed[:] = [n for n in new_changed if n in new_glob and new_code[n - 1].strip()]
        if len(old_changed) * len(new_changed) > 4096:
            old_changed.clear()
            new_changed.clear()
            return
        old_tokens = [re.sub(r"\s+", "", old_code[n - 1]) for n in old_changed]
        new_tokens = [re.sub(r"\s+", "", new_code[n - 1]) for n in new_changed]
        old_by_token = {}
        for old_index, token in enumerate(old_tokens):
            old_by_token.setdefault(token, []).append(old_index)
        candidates = []
        for new_index, new_token in enumerate(new_tokens):
            new_line = new_changed[new_index]
            for old_index in old_by_token.get(new_token, []):
                old_line = old_changed[old_index]
                old_chain, new_chain = old_scopes[old_line], new_scopes[new_line]
                if len(old_chain) != len(new_chain) or any(
                    retained_lines.get(old_open) != new_open or old_header != new_header
                    for (old_open, old_header), (new_open, new_header) in zip(
                        old_chain, new_chain)
                ):
                    continue
                left, right = old_raw[old_line - 1], new_raw[new_line - 1]
                score = (2.0 if left == right
                         else SequenceMatcher(None, left, right, autojunk=False).ratio())
                candidates.append((score, -abs(old_line - new_line),
                                   old_index, new_index))
        used_old, used_new = set(), set()
        for _score, _distance, old_index, new_index in sorted(candidates, reverse=True):
            if old_index in used_old or new_index in used_new:
                continue
            used_old.add(old_index)
            used_new.add(new_index)
            old_line, new_line = old_changed[old_index], new_changed[new_index]
            if old_raw[old_line - 1] != new_raw[new_line - 1]:
                unchanged.add(new_line)
        old_changed.clear()
        new_changed.clear()

    old_line = new_line = None
    for line in diff.splitlines():
        if line.startswith("@@ "):
            finish_hunk()
            header = re.match(r"@@ -(\d+)(?:,\d+)? \+(\d+)", line)
            if not header:
                raise ValueError("unexpected Git hunk header")
            old_line, new_line = map(int, header.groups())
        elif old_line is not None:
            if line.startswith("-") and not line.startswith("---"):
                old_changed.append(old_line)
                old_line += 1
            elif line.startswith("+") and not line.startswith("+++"):
                new_changed.append(new_line)
                new_line += 1
            elif line.startswith(" "):
                old_line += 1
                new_line += 1
    finish_hunk()
    return unchanged


def violations(repo, base, head):
    # Three-dot ranges compare from the merge base, so base-branch changes made
    # after the fork are never attributed to the head as added lines.
    git(repo, "rev-parse", "--verify", base + "^{commit}")
    git(repo, "rev-parse", "--verify", head + "^{commit}")
    merge_base = git(repo, "merge-base", base, head).decode("ascii").strip()
    paths = git(repo, "diff", "--no-renames", "--name-status", "-z",
                "--diff-filter=AM", f"{base}...{head}", "--", "crates").decode("utf-8").split("\0")
    results = []
    for status, path in zip(paths[::2], paths[1::2]):
        if not is_owner_test(path):
            continue
        content = git(repo, "show", head + ":" + path).decode("utf-8")
        diff = git(repo, "diff", "--no-renames", "-U0", f"{base}...{head}", "--", path).decode("utf-8")
        prior = git(repo, "show", merge_base + ":" + path).decode("utf-8") if status == "M" else ""
        masked = masked_code(content)
        changed = added_lines(diff) - trivia_only_changed_lines(diff, prior, content)
        code_lines = masked.splitlines()
        for lines, _identity in glob_imports(masked):
            for line in sorted(lines & changed):
                if code_lines[line - 1].strip():
                    results.append((path, line))
    return results


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    args = parser.parse_args()
    try:
        findings = violations(args.repo, args.base, args.head)
    except (subprocess.CalledProcessError, UnicodeError, ValueError) as exc:
        print(f"test-imports: comparison unavailable: {exc}")
        return 2
    for path, line in findings:
        print(f"{path}:{line}: newly added use super::*; import; list explicit names")
    if findings:
        return 1
    print("test-imports: no newly introduced owner-local Rust test glob imports")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
