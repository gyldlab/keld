#!/usr/bin/env python3
"""Reject newly introduced wildcard imports in owner-local Rust test modules.

Only changed *source imports* are considered; untouched legacy imports are
not a retroactive build gate. This is part of the test-layout acceptance check.
"""
import argparse
import re
import subprocess
from pathlib import Path

IMPORT = re.compile(r"^[ \t]*use[ \t]+[^;]*;", re.M | re.S)
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


def active_glob_lines(content):
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
    masked = "".join(code)
    found = set()
    for match in IMPORT.finditer(masked):
        if "*" not in match.group():
            continue
        start_line = masked.count("\n", 0, match.start()) + 1
        end_line = start_line + match.group().count("\n")
        found.update(range(start_line, end_line + 1))
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


def violations(repo, base, head):
    # Three-dot ranges compare from the merge base, so base-branch changes made
    # after the fork are never attributed to the head as added lines.
    git(repo, "rev-parse", "--verify", base + "^{commit}")
    git(repo, "rev-parse", "--verify", head + "^{commit}")
    paths = git(repo, "diff", "--no-renames", "--name-only", "-z",
                "--diff-filter=AM", f"{base}...{head}", "--", "crates").decode("utf-8")
    results = []
    for path in filter(None, paths.split("\0")):
        if not is_owner_test(path):
            continue
        content = git(repo, "show", head + ":" + path).decode("utf-8")
        diff = git(repo, "diff", "--no-renames", "-U0", f"{base}...{head}", "--", path).decode("utf-8")
        for line in sorted(active_glob_lines(content) & added_lines(diff)):
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
