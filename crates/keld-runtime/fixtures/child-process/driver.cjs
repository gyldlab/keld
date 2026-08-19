// KEL-77 differential corpus — the parent half, and the measured subject.
//
// The pre-spec probe ran the full parent-runtime x child-runtime matrix; every
// divergence found tracked the PARENT (the side calling child_process), not the
// child. This driver therefore spawns `process.execPath`, so one invocation
// measures one runtime acting as both supervisor and supervised — which is also
// Keld's real deployment shape (architecture 06 §1.1: a Bun parent owning Bun
// children).
//
// Usage: `<runtime> driver.cjs <case-id>`
// Contract: exactly one JSON line on stdout, then a natural exit. Anything else
// is a case failure on the Rust side, never a missing record.
"use strict";

const { spawn } = require("node:child_process");
const path = require("node:path");

const CHILD = path.join(__dirname, "child.cjs");
const caseId = process.argv[2];

// Encode event arguments so `null` and `undefined` stay distinguishable — that
// difference is itself one of the measured observations.
const enc = (v) => (v === null ? "null" : v === undefined ? "undefined" : String(v));

function emit(extra) {
  clearTimeout(watchdog);
  process.stdout.write(`${JSON.stringify({ case: caseId, ...extra })}\n`);
}

// Spawn one child and collect a canonical event trace. `exe` is explicit so the
// spawn-failure case can point at a path that is not an executable at all; every
// other case passes `process.execPath` to measure this runtime on both sides.
function observe(exe, args, { onSpawn, onExit } = {}) {
  return new Promise((resolve) => {
    const events = [];
    const extra = {};
    const child = spawn(exe, args, { stdio: ["ignore", "pipe", "pipe"] });
    let stdoutLen = 0;
    let stderr = "";

    if (child.stdout) child.stdout.on("data", (d) => (stdoutLen += d.length));
    if (child.stderr) child.stderr.on("data", (d) => (stderr += d));

    child.on("error", (e) => events.push(`error(${enc(e.code)})`));
    child.on("exit", (code, signal) => {
      events.push(`exit(${enc(code)},${enc(signal)})`);
      if (onExit) onExit(child, extra);
    });
    child.on("close", (code, signal) => {
      events.push(`close(${enc(code)},${enc(signal)})`);
      resolve({
        events,
        stdoutLen,
        stderr: stderr.trim(),
        exitCode: enc(child.exitCode),
        signalCode: enc(child.signalCode),
        ...extra,
      });
    });

    if (onSpawn) onSpawn(child);
  });
}

// Kill switch, not synchronization: every case is driven by awaiting an observable
// condition, never a sleep. But a runtime regression that never emits the child's
// first byte would otherwise hang the case forever, and a hang is a worse signal
// than a failure. `unref()` keeps this timer from holding the loop open on its own.
const WATCHDOG_MS = 30_000;
const watchdog = setTimeout(() => {
  process.stderr.write(
    `watchdog: case ${String(caseId)} produced no observation within ${WATCHDOG_MS}ms\n`,
  );
  process.exit(75);
}, WATCHDOG_MS);
if (typeof watchdog.unref === "function") watchdog.unref();

async function main() {
  switch (caseId) {
    // 1 + 3: exit status values, and the documented 'close'-after-'exit' order.
    case "child-process.exit-code-propagation":
    case "child-process.close-after-exit":
      emit(await observe(process.execPath, [CHILD, "exit7"]));
      break;

    // 2: termination by signal. Killed only after READY is observed — ordered,
    // never timed, so there is nothing to flake.
    case "child-process.signal-termination":
      emit(
        await observe(process.execPath, [CHILD, "hang"], {
          onSpawn: (c) => c.stdout.once("data", () => c.kill("SIGTERM")),
        }),
      );
      break;

    // 4: spawn failure. The subject is a missing EXECUTABLE, so the failure comes
    // from spawn itself (ENOENT) — not from a real runtime failing to resolve a
    // script, which would merely be a normal exit(1) and would prove nothing.
    case "child-process.spawn-failure-order": {
      const missing = path.join(__dirname, "kel77-no-such-executable");
      emit(await observe(missing, []));
      break;
    }

    // 5: teardown. kill() a child that has already exited, and independently ask
    // the OS whether that pid still exists, so the record can prove the child was
    // truly gone rather than merely unreaped.
    case "child-process.kill-after-exit":
      emit(
        await observe(process.execPath, [CHILD, "exit7"], {
          onExit: (c, extra) => {
            extra.killReturn = enc(c.kill("SIGTERM"));
            extra.killed = enc(c.killed);
            try {
              process.kill(c.pid, 0);
              extra.rawKillErrno = "alive";
            } catch (e) {
              extra.rawKillErrno = enc(e.code);
            }
          },
        }),
      );
      break;

    // 6: cleanup. Both variants of the same write, so the specified path and the
    // unspecified one are measured in the same record.
    case "child-process.stdout-flush-on-abrupt-exit": {
      const abrupt = await observe(process.execPath, [CHILD, "abrupt"]);
      const drained = await observe(process.execPath, [CHILD, "drained"]);
      emit({
        events: abrupt.events,
        abruptBytes: abrupt.stdoutLen,
        drainedBytes: drained.stdoutLen,
        drainedEvents: drained.events,
      });
      break;
    }

    default:
      process.stderr.write(`unknown case: ${String(caseId)}\n`);
      process.exit(64);
  }
}

main().catch((e) => {
  process.stderr.write(`driver failure: ${e && e.stack ? e.stack : String(e)}\n`);
  process.exit(70);
});
