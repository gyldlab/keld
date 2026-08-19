// KEL-77 differential corpus — the child half. Package-agnostic: no Keld, no
// Electron, no VS Code, no npm dependency. CommonJS so Node and Bun both accept
// it without a package.json `type` field.
//
// Invoked as `<runtime> child.cjs <mode>` by driver.cjs. Every mode is a plain
// `node:`-namespaced API so the observation measures the runtime, not a shim.
"use strict";

const BIG_BYTES = 200000;
const mode = process.argv[2];

switch (mode) {
  // Deterministic exit status plus both stdio directions.
  case "exit7":
    process.stdout.write("OUT\n");
    process.stderr.write("ERR\n");
    process.exit(7);
    break;

  // Announce readiness, then stay alive until signalled. The driver waits for
  // this line rather than sleeping, so the kill is ordered, not timed.
  case "hang":
    process.stdout.write("READY\n");
    setInterval(() => {}, 1000);
    break;

  // Abrupt: process.exit() while an asynchronous stdout write is still pending.
  // Node documents this as lossy, so it is an unspecified path.
  case "abrupt":
    process.stdout.write("x".repeat(BIG_BYTES));
    process.exit(0);
    break;

  // Drained: the documented way to do the same thing — exit from the write
  // callback. This path IS specified and must deliver every byte.
  case "drained":
    process.stdout.write("x".repeat(BIG_BYTES), () => process.exit(0));
    break;

  default:
    process.stderr.write(`unknown child mode: ${String(mode)}\n`);
    process.exit(64);
}
