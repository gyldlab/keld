"""Review-only mutation controls; never edits the original fixture or other trees."""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
ASSERTION_PREFIX = "error: GH451_MUTATION_ASSERT: "

controls = [
    (
        "skip peer negotiation",
        "this.#eligible = port.negotiated && port.selected;",
        "this.#eligible = true;",
        "old and unselected peers",
        "peer-eligibility",
    ),
    (
        "close after Retained",
        'case "retained":\n      case "cancelled":',
        'case "retained":\n        this.#port.fenceAccepted();\n        // fall through\n      case "cancelled":',
        "Prepare Retained keeps same port",
        "retained-port-live",
    ),
    (
        "drop original unknown-outcome latch",
        'if (this.#phase.kind === "unknown")\n      return { kind: "unknown", original: this.#phase.original };',
        'if (this.#phase.kind === "unknown") this.#phase = { kind: "idle" };',
        "prepare lost preserves original identity",
        "unknown-latch",
    ),
    (
        "ignore Status original identity",
        '||\n        !sameOriginal(reply.original, original)',
        '',
        "Status mismatch",
        "status-original",
    ),
    (
        "accept late direct Prepared from retired role",
        '(!this.#port.isCurrent() || !this.#port.isLive())\n      )\n        return this.#unknown(original);\n      return outcome\n        ? this.#settle(outcome, original, "direct")',
        '!this.#port.isLive()\n      )\n        return this.#unknown(original);\n      return outcome\n        ? this.#settle(outcome, original, "direct")',
        "late Prepared after role retirement",
        "retired-prepared",
    ),
    (
        "convert generic failure to Retained",
        '// are not no-effect. Other cancellation dispositions need owner approval.\n  return undefined;',
        '// MUTANT: infer no effect.\n  return { kind: "retained" };',
        "prepare generic-error preserves original identity",
        "unknown-initial-effect",
    ),
    (
        "accept structural receipt clone",
        'receipt !== this.#receipt',
        '!this.#receipt',
        "one active attempt survives Prepared receipt release",
        "receipt-identity",
    ),
    (
        "erase known acceptance on local fence failure",
        'this.#fenceError = error;',
        'throw error;',
        "known Accepted survives local fence failure, recovery=false",
        "accepted-despite-fence-error",
    ),
    (
        "evict timed-out Status without no-late evidence",
        'this.#ledger.consume(statusLease);',
        'this.#ledger.consume(statusLease);\n      this.#ledger.proveNoLate(statusLease);',
        "lost Status occupies its separate bounded slot",
        "status-lease-retained",
    ),
    (
        "restore a blocked preparation when capacity frees",
        '      if (phase.kind === "prepared")\n        this.#phase = { kind: "blocked-prepared", token: phase.token };',
        '',
        "capacity after Prepare blocks cancel even after a slot frees",
        "surviving-prepare",
    ),
]
ledger_controls = [
    (
        "reuse original correlation for Status",
        'const lease = this.#allocate("status");',
        'const lease = original;',
        "Status may not reuse unresolved effect correlation",
        "no-status-write",
    ),
    (
        "send without reserving original correlation",
        'const lease = this.#allocate("effect");',
        'const lease = Object.freeze({ correlation: this.#next, kind: "effect" as const });',
        "Prepare Retained keeps same port",
        "dispatch-reservation",
    ),
    (
        "drop active attempt exclusivity",
        'if (this.#active) return undefined;',
        '',
        "one active attempt survives Prepared receipt release",
        "attempt-exclusivity",
    ),
    (
        "release without consumption",
        'else if (!entry.consumed || !entry.noLate) return false;',
        'else if (!entry.noLate) return false;',
        "no-late proof alone cannot retire",
        "consumption-required",
    ),
    (
        "release without no-late proof",
        'else if (!entry.consumed || !entry.noLate) return false;',
        'else if (!entry.consumed) return false;',
        "wrap and capacity preserve unresolved original",
        "no-late-required",
    ),
    (
        "accept stale lease for reused numeric identity",
        'return entry?.lease === lease ? entry : undefined;',
        'return entry;',
        "wrap and capacity preserve unresolved original",
        "stale-lease",
    ),
    (
        "reclaim a terminal receipt before definitive retirement",
        '      if (!entry.retired) return false;',
        '      if (!entry.consumed || !entry.noLate) return false;',
        "known Accepted survives local fence failure, recovery=false",
        "retirement-required",
    ),
    (
        "forget a definitive retirement recorded before terminal delivery",
        '    entry.terminal = true;\n    // Definitive retirement may have been recorded while delivery was held.\n    this.#release(entry);',
        '    entry.terminal = true;',
        "retirement proof before historical Accepted delivery needs no Consume",
        "retirement-before-delivery",
    ),
]
cases = [("candidate.ts", *control) for control in controls]
cases += [("reservation.ts", *control) for control in ledger_controls]


def run_fixture(bun, folder, selected):
    return subprocess.run(
        [bun, "test", str(folder / "candidate.test.ts"), "-t", re.escape(selected)],
        capture_output=True, text=True, timeout=20, check=False,
        env={**os.environ, "NO_COLOR": "1", "FORCE_COLOR": "0"},
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


def expected_failure(result, baseline_name, selected, marker):
    output = result.stdout + "\n" + result.stderr
    # A named fail row also represents Bun's own timeout or a setup failure.
    # Credit only the single standalone diagnostic from the chosen assertion;
    # a copy of its source line in the failure excerpt does not count.
    diagnostics = re.findall(r"^(?:error|[A-Za-z]*Error): .+$", output, re.MULTILINE)
    return (
        result.returncode == 1
        and reported_test(output, "fail", selected) == baseline_name
        and diagnostics == [ASSERTION_PREFIX + marker]
        and not re.search(r"^\s*\^ this test timed out after ", output, re.MULTILINE)
    )


def main(bun):
    sources = {
        name: (ROOT / name).read_text(encoding="utf-8")
        for name in ("candidate.ts", "reservation.ts")
    }
    tests = (ROOT / "candidate.test.ts").read_text(encoding="utf-8")
    for filename, name, old, new, selected, marker in cases:
        source = sources[filename]
        if source.count(old) != 1:
            raise SystemExit(f"missing or ambiguous mutation preimage: {name}")
        with tempfile.TemporaryDirectory(prefix=".negative-", dir=ROOT) as tmp:
            folder = Path(tmp)
            for name_in_tree, contents in sources.items():
                (folder / name_in_tree).write_text(contents, encoding="utf-8")
            (folder / "candidate.test.ts").write_text(tests, encoding="utf-8")
            baseline = run_fixture(bun, folder, selected)
            output = baseline.stdout + "\n" + baseline.stderr
            baseline_name = reported_test(output, "pass", selected)
            if baseline.returncode != 0 or baseline_name is None:
                print(output)
                raise SystemExit(f"unmodified fixture did not pass expected test: {name}")
            (folder / filename).write_text(source.replace(old, new, 1), encoding="utf-8")
            run = run_fixture(bun, folder, selected)
            if not expected_failure(run, baseline_name, selected, marker):
                print(run.stdout + "\n" + run.stderr)
                raise SystemExit(
                    f"control did not falsify expected assertion: {name} ({marker})"
                )
            print(f"RED {name}: {marker} (exit {run.returncode})")
    print(f"{len(cases)}/{len(cases)} negative controls detected")


if __name__ == "__main__":
    main(sys.argv[1])
