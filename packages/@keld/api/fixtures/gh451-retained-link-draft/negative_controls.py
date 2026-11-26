"""Review-only mutation controls; never edits the original fixture or other trees."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent
bun = sys.argv[1]
sources = {name: (root / name).read_text()
           for name in ("candidate.ts", "reservation.ts")}
tests = (root / "candidate.test.ts").read_text()
controls = [
    (
        "skip peer negotiation",
        "this.#eligible = port.negotiated && port.selected;",
        "this.#eligible = true;",
        "old and unselected peers",
    ),
    (
        "close after Retained",
        'case "retained":\n      case "cancelled":',
        'case "retained":\n        this.#port.fenceAccepted();\n        // fall through\n      case "cancelled":',
        "Prepare Retained keeps same port",
    ),
    (
        "drop original unknown-outcome latch",
        'if (this.#phase.kind === "unknown")\n      return { kind: "unknown", original: this.#phase.original };',
        'if (this.#phase.kind === "unknown") this.#phase = { kind: "idle" };',
        "prepare lost preserves original identity",
    ),
    (
        "ignore Status original identity",
        '|| !sameOriginal(reply.original, original)',
        '',
        "Status mismatch",
    ),
    (
        "accept late direct Prepared from retired role",
        '(!this.#port.isCurrent() || !this.#port.isLive()))\n        return this.#unknown(original);',
        '!this.#port.isLive())\n        return this.#unknown(original);',
        "late Prepared after role retirement",
    ),
    (
        "convert generic failure to Retained",
        '// are not no-effect. Other cancellation dispositions need owner approval.\n  return undefined;',
        '// MUTANT: infer no effect.\n  return { kind: "retained" };',
        "prepare generic-error preserves original identity",
    ),
    (
        "accept structural receipt clone",
        'receipt !== this.#receipt',
        '!this.#receipt',
        "one active attempt survives Prepared receipt release",
    ),
    (
        "erase known acceptance on local fence failure",
        'this.#fenceError = error;',
        'throw error;',
        "known Accepted survives local fence failure, recovery=false",
    ),
    (
        "evict timed-out Status without no-late evidence",
        'this.#ledger.consume(statusLease);',
        'this.#ledger.consume(statusLease);\n      this.#ledger.proveNoLate(statusLease);',
        "lost Status occupies its separate bounded slot",
    ),
]
ledger_controls = [
    (
        "reuse original correlation for Status",
        'const lease = this.#allocate("status");',
        'const lease = original;',
        "Status may not reuse unresolved effect correlation",
    ),
    (
        "send without reserving original correlation",
        'const lease = this.#allocate("effect");',
        'const lease = Object.freeze({ correlation: this.#next, kind: "effect" as const });',
        "Prepare Retained keeps same port",
    ),
    (
        "drop active attempt exclusivity",
        'if (this.#active) return undefined;',
        '',
        "one active attempt survives Prepared receipt release",
    ),
    (
        "release without consumption",
        'if (!entry.consumed || !entry.noLate) return false;',
        'if (!entry.noLate) return false;',
        "no-late proof alone cannot retire",
    ),
    (
        "release without no-late proof",
        'if (!entry.consumed || !entry.noLate) return false;',
        'if (!entry.consumed) return false;',
        "wrap and capacity preserve unresolved original",
    ),
    (
        "accept stale lease for reused numeric identity",
        'return entry?.lease === lease ? entry : undefined;',
        'return entry;',
        "wrap and capacity preserve unresolved original",
    ),
]
cases = [("candidate.ts", *control) for control in controls]
cases += [("reservation.ts", *control) for control in ledger_controls]
def run_fixture(folder, selected):
    return subprocess.run(
        [bun, "test", str(folder / "candidate.test.ts"), "-t", re.escape(selected)],
        capture_output=True, text=True, timeout=20, check=False,
    )


def reported_test(output, outcome, selected):
    # Require exactly one named result of the expected kind. Do not accept a
    # compilation error, empty selection, or a failure from an unrelated test.
    results = re.findall(r"^\((pass|fail|skip)\) (.+)$", output, re.MULTILINE)
    if len(results) != 1 or results[0][0] != outcome:
        return None
    name = re.sub(r" \[\d+(?:\.\d+)?(?:ms|s)\]$", "", results[0][1])
    if re.match(re.escape(selected) + r"(?!\w)", name):
        return name
    return None


for filename, name, old, new, expected in cases:
    source = sources[filename]
    if old not in source:
        raise SystemExit(f"missing mutation preimage: {name}")
    with tempfile.TemporaryDirectory(prefix=".negative-", dir=root) as tmp:
        folder = Path(tmp)
        for name_in_tree, contents in sources.items():
            (folder / name_in_tree).write_text(contents)
        (folder / "candidate.test.ts").write_text(tests)
        baseline = run_fixture(folder, expected)
        output = baseline.stdout + "\n" + baseline.stderr
        baseline_name = reported_test(output, "pass", expected)
        if baseline.returncode != 0 or baseline_name is None:
            print(output)
            raise SystemExit(f"unmodified fixture did not pass expected test: {name}")
        (folder / filename).write_text(source.replace(old, new, 1))
        run = run_fixture(folder, expected)
        output = run.stdout + "\n" + run.stderr
        if run.returncode == 0 or reported_test(output, "fail", expected) != baseline_name:
            print(output)
            raise SystemExit(f"control did not falsify expected test: {name}")
        print(f"RED {name}: {expected} (exit {run.returncode})")
print(f"{len(cases)}/{len(cases)} negative controls detected")
