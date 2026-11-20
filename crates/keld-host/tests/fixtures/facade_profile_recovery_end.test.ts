/** Independent control-socket oracle for GH-78 recovery fixture terminal reasons.
 * A mocked link end is NOT Host/Guard/native acceptance. This test only
 * prevents the real native fixture from reporting abnormal close as LINK_EOF.
 */
import { expect, test } from "bun:test";
import { createServer } from "node:net";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const unixTest = process.platform === "win32" ? test.skip : test;
const fixture = readFileSync(
  join(import.meta.dir, "facade_profile_recovery.ts"),
  "utf8",
);
const controlledPrelude =
  [
    "const WINDOW_CHANNEL=61;",
    "const LIFECYCLE_CHANNEL=62;",
    "const nativeFacadeBootSelected=true;",
    "const getWindowSnapshots=()=>[];",
    "const fs={write:async()=>{},read:async()=>new Uint8Array([])};",
    "let terminalCallback;",
    "const fakeLink={onEvent:()=>{},onEnd:(callback)=>{terminalCallback=callback;}};",
    "const WorkerLink={open:async()=>fakeLink};",
    "const app={",
    " whenReady:async()=>{await WorkerLink.open({receive:{eventChannels:[WINDOW_CHANNEL]}});},",
    " quit:async()=>{setImmediate(()=>terminalCallback({code:process.env.B3_TERMINAL_CODE}));},",
    "};",
  ].join("\n") + "\n";

interface ControlledEnd {
  readonly exitCode: number;
  readonly records: string[];
  readonly stderr: string;
}

/** The real fixture uses a real Unix control socket, but the end cause is injected. */
async function observeTerminal(code: string): Promise<ControlledEnd> {
  const scratch = mkdtempSync(join(tmpdir(), "k78-"));
  const socketPath = join(scratch, "c.sock");
  const rolePath = join(scratch, "role.ts");
  writeFileSync(rolePath, controlledPrelude + fixture);
  const records: string[] = [];
  const server = createServer((connection) => {
    let pending = "";
    connection.on("data", (chunk: Buffer) => {
      pending += chunk.toString("utf8");
      let separator = pending.indexOf("\n");
      while (separator >= 0) {
        const line = pending.slice(0, separator).trim();
        pending = pending.slice(separator + 1);
        records.push(line);
        if (line.startsWith("HELLO ")) connection.write("ADMIT\n");
        if (line === "READY") connection.write("QUIT\n");
        separator = pending.indexOf("\n");
      }
    });
  });
  try {
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(socketPath, () => {
        server.off("error", reject);
        resolve();
      });
    });
    const child = Bun.spawn({
      cmd: [process.execPath, rolePath],
      cwd: scratch,
      env: {
        ...process.env,
        KELD_T1B_CONTROL: socketPath,
        KELD_APP_LINK: "test-only-not-authenticated",
        B3_TERMINAL_CODE: code,
      },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    const killSwitch = setTimeout(() => child.kill(), 7_000);
    try {
      const [exitCode, _stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      return { exitCode, records, stderr };
    } finally {
      clearTimeout(killSwitch);
      if (child.exitCode === null) child.kill();
    }
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
    rmSync(scratch, { recursive: true, force: true });
  }
}

unixTest(
  "expected ordinary WorkerLink close is the only clean recovery EOF",
  async () => {
    const result = await observeTerminal("KELD-IPC-022");
    expect(result.exitCode, result.stderr).toBe(0);
    expect(result.records.slice(-3)).toEqual([
      "QUIT_REPLY",
      "READY_COUNT 0 OPEN_COUNT 1",
      "LINK_EOF",
    ]);
  },
);

for (const abnormal of ["KELD-IPC-005", "KELD-IPC-006", "KELD-IPC-027"]) {
  unixTest(
    "abnormal WorkerLink end cannot pass as normal EOF: " + abnormal,
    async () => {
      const result = await observeTerminal(abnormal);
      expect(result.records).toContain("LINK_END_ERROR " + abnormal);
      expect(result.records).not.toContain("LINK_EOF");
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("unexpected WorkerLink terminal code");
    },
  );
}
