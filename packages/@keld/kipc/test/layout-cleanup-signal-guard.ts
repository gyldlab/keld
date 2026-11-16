// Isolated subprocess-only observer. Suppress destructive numeric PID signals
// so the negative control cannot harm a recycled PID; preserve real signal-zero
// observations. Bun Subprocess cleanup remains owned by runLayout.
const originalKill = process.kill;
let destructiveSignals = 0;
process.kill = ((pid, signal) => {
  if (signal === 0) return Reflect.apply(originalKill, process, [pid, signal]);
  destructiveSignals += 1;
  return true;
}) as typeof process.kill;

process.on("exit", () => {
  process.kill = originalKill;
  console.log(`AUDIT_PID_SIGNALS=${destructiveSignals}`);
  if (destructiveSignals !== 0) process.exitCode = 1;
});
