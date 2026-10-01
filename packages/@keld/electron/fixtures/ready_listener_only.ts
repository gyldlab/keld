import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

process.env.KELD_APP_LINK ??= `/tmp/keld-kel142-ready-listener.sock#${"a".repeat(64)}`;

let connectCalls = 0;
LifecycleLink.connect = (async (
  _link: string,
  handlers: Parameters<typeof LifecycleLink.connect>[1],
) => {
  connectCalls += 1;
  queueMicrotask(() => handlers.onReady());
  return {
    async quit() {},
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/app.ts");

const timeout = setTimeout(() => {
  writeSync(2, "KEL142_READY_LISTENER_ONLY_TIMEOUT\n");
  process.exit(1);
}, 1_000);

app.on("ready", () => {
  clearTimeout(timeout);
  marker("KEL142_READY_LISTENER_ONLY");
  marker(`KEL142_CONNECT_CALLS=${connectCalls}`);
  process.exit(0);
});
