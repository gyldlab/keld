/**
 * GH-527 `WorkerLink` contract tests that need no host (spec gh527 §7).
 *
 * Oracles: the spec's bound constants, the §4.7 selection table, and a live
 * Unix listener that must see no connection when `open` refuses its options
 * (criteria 21 and 25: refused before the Worker spawns). Everything that needs
 * the real host writer is in `crates/keld-ipc/tests/worker_link.rs`, which
 * drives `test/worker-link-role.ts`; `bun test` shares one realm across files,
 * and a realm opens at most one WorkerLink, so no test here opens one.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";

import {
  DEFAULT_REPLY_BYTES,
  DEFAULT_RING_BYTES,
  DEFAULT_RING_RECORDS,
  ECHO_CHANNEL,
  FrameKind,
  LIFECYCLE_CHANNEL,
  MAX_ABANDONED_CALLS,
  MAX_BLOCKING_CALL_DEADLINE_MS,
  MAX_FRAME_LEN,
  MAX_RING_BYTES,
  RECEIVE_POLICIES,
  WORKER_HEARTBEAT_INTERVAL_MS,
  WORKER_LINK_CONTROL,
  WORKER_LINK_TEST_WORDS,
  WORKER_LIVENESS_WINDOW_MS,
  WorkerLink,
  echoReplyWaiter,
  isCallError,
  privilegedCallReceiver,
  replyWaiter,
  selectInboundPolicy,
  validateReceivedHeader,
  type FrameHeader,
  type InboundTable,
  type PendingCallEntry,
  type WorkerLinkOptions,
} from "./transport.ts";
import { openWorkerLinkForTest } from "./test-hooks.ts";

const TOKEN_HEX = "ab".repeat(32);

function header(kind: number, channel: number, corr: number, len = 4): FrameHeader {
  return { kind, flags: 0, channel, corr, len };
}

async function codeOfRejection(promise: Promise<unknown>): Promise<string> {
  try {
    await promise;
    return "resolved";
  } catch (err) {
    return isCallError(err) ? err.code : `untyped:${String(err)}`;
  }
}

describe("WorkerLink bounds (spec §4.3, §4.5)", () => {
  test("constants are the spec's values", () => {
    expect(DEFAULT_RING_BYTES).toBe(1 << 20);
    expect(DEFAULT_RING_RECORDS).toBe(16_384);
    expect(DEFAULT_REPLY_BYTES).toBe(1 << 16);
    expect(MAX_RING_BYTES).toBe(1 << 30);
    expect(MAX_BLOCKING_CALL_DEADLINE_MS).toBe(24 * 60 * 60 * 1000);
    expect(WORKER_HEARTBEAT_INTERVAL_MS).toBe(100);
    expect(WORKER_LIVENESS_WINDOW_MS).toBe(1_000);
    expect(MAX_ABANDONED_CALLS).toBe(256);
  });

  test("control block words are the §4.3 layout; words 2 and 15 stay reserved", () => {
    expect(WORKER_LINK_CONTROL).toEqual({
      SEQ: 0,
      STATE: 1,
      W_BYTES: 3,
      A_BYTES: 4,
      R_BYTES: 5,
      W_RECS: 6,
      R_RECS: 7,
      HEARTBEAT: 8,
      BLOCKING: 9,
      REPLY_READY: 10,
      REPLY_KIND: 11,
      REPLY_LEN: 12,
      REPLY_AT: 13,
      KICK: 14,
    });
    expect(Object.values(WORKER_LINK_CONTROL)).not.toContain(2);
    expect(Object.values(WORKER_LINK_CONTROL)).not.toContain(15);
  });
});

describe("WorkerLink.open refuses invalid bounds before the Worker spawns", () => {
  const lifecycle = { eventChannels: [LIFECYCLE_CHANNEL], callReceivers: [] };
  const cases: Array<[string, Partial<WorkerLinkOptions>]> = [
    ["a 3 MiB ring (not a power of two)", { ringBytes: 3 * 1024 * 1024 }],
    ["a ring below 64 KiB", { ringBytes: 1 << 15 }],
    ["a ring above MAX_RING_BYTES", { ringBytes: 2 ** 31 }],
    ["a fractional ring", { ringBytes: 65_536.5 }],
    ["zero ring records", { ringRecords: 0 }],
    ["fractional ring records", { ringRecords: 1.5 }],
    ["a reply slot below 4,096 bytes", { replyBytes: 4_095 }],
    ["a reply slot above MAX_FRAME_LEN", { replyBytes: MAX_FRAME_LEN + 1 }],
    ["a repeated EVENT channel", { receive: { eventChannels: [3, 3], callReceivers: [] } }],
    [
      "a CALL receiver on an EVENT channel",
      { receive: { eventChannels: [3], callReceivers: [{ policy: "privilegedCallReceiver", channel: 3 }] } },
    ],
    [
      "two echo receivers",
      { receive: { eventChannels: [], callReceivers: [{ policy: "echoReceiver" }, { policy: "echoReceiver" }] } },
    ],
    ["channel 0", { receive: { eventChannels: [0], callReceivers: [] } }],
    ["an EVENT channel that carries no host EVENTs", { receive: { eventChannels: [ECHO_CHANNEL], callReceivers: [] } }],
    [
      "an unknown receiver constructor",
      {
        receive: {
          eventChannels: [],
          callReceivers: [{ policy: "primaryAppReceiver" } as unknown as { policy: "echoReceiver" }],
        },
      },
    ],
    ["a missing receive table", { receive: undefined }],
  ];

  test("each invalid option is KELD-IPC-005 and nothing connects", async () => {
    const dir = mkdtempSync(join(tmpdir(), "keld-wl-"));
    const path = join(dir, "s.sock");
    let connections = 0;
    const listener = Bun.listen({
      unix: path,
      socket: {
        open() {
          connections += 1;
        },
        data() {},
      },
    });
    try {
      for (const [name, override] of cases) {
        const options = { link: `${path}#${TOKEN_HEX}`, receive: lifecycle, ...override } as WorkerLinkOptions;
        expect(`${name}: ${await codeOfRejection(WorkerLink.open(options))}`).toBe(`${name}: KELD-IPC-005`);
      }
      await new Promise((resolve) => setImmediate(resolve));
      expect(connections).toBe(0);
    } finally {
      listener.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("test hooks are refused unless KELD_KIPC_TEST_HOOKS=1", async () => {
    const saved = process.env.KELD_KIPC_TEST_HOOKS;
    delete process.env.KELD_KIPC_TEST_HOOKS;
    try {
      const words = new Int32Array(new SharedArrayBuffer(WORKER_LINK_TEST_WORDS.LENGTH * 4));
      const code = await codeOfRejection(
        openWorkerLinkForTest({ link: `/nonexistent#${TOKEN_HEX}`, receive: lifecycle }, { words }),
      );
      expect(code).toBe("KELD-IPC-005");
    } finally {
      if (saved !== undefined) process.env.KELD_KIPC_TEST_HOOKS = saved;
    }
  });
});

describe("WorkerLink.open refuses a transport bundled into an app entry (GH-527 §4.2)", () => {
  // A bundler that inlines the transport into the entry makes `import.meta.url`
  // the entry, so the Worker would load the whole app. The bundled copy runs
  // in its own process (a realm opens one link) against a live listener.
  test("the bundled open is KELD-IPC-005 and nothing connects", async () => {
    const dir = mkdtempSync(join(tmpdir(), "keld-wl-"));
    const path = join(dir, "s.sock");
    let connections = 0;
    const listener = Bun.listen({
      unix: path,
      socket: {
        open() {
          connections += 1;
        },
        data() {},
      },
    });
    try {
      const entry = join(dir, "entry.ts");
      await Bun.write(
        entry,
        `import { WorkerLink, isCallError } from ${JSON.stringify(join(import.meta.dir, "transport.ts"))};\n` +
          "try {\n" +
          "  await WorkerLink.open({ link: process.env.KELD_APP_LINK!, receive: { eventChannels: [3], callReceivers: [] } });\n" +
          '  console.log("opened");\n' +
          "} catch (err) {\n" +
          '  console.log(isCallError(err) ? err.code : "untyped");\n' +
          "}\n" +
          "process.exit(0);\n",
      );
      const built = await Bun.build({ entrypoints: [entry], target: "bun", format: "esm" });
      expect(built.success).toBe(true);
      const bundle = join(dir, "main.js");
      await Bun.write(bundle, built.outputs[0]!);
      const proc = Bun.spawn(["bun", bundle], {
        env: { ...process.env, KELD_APP_LINK: `${path}#${TOKEN_HEX}` },
        stdout: "pipe",
        stderr: "pipe",
      });
      const [stdout, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
      expect({ stdout: stdout.trim(), code }).toEqual({ stdout: "KELD-IPC-005", code: 0 });
      expect(connections).toBe(0);
    } finally {
      listener.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  }, 30_000);
});

describe("§4.7 per-frame policy selection", () => {
  const table: InboundTable = {
    eventChannels: new Set([LIFECYCLE_CHANNEL]),
    callPolicies: new Map([
      [ECHO_CHANNEL, RECEIVE_POLICIES.echoReceiver],
      [2, privilegedCallReceiver(2)],
    ]),
  };
  const pending = new Map<number, PendingCallEntry>([
    [10, { channel: ECHO_CHANNEL, blocking: false, abandoned: false }],
    [11, { channel: LIFECYCLE_CHANNEL, blocking: true, abandoned: false }],
    [12, { channel: LIFECYCLE_CHANNEL, blocking: false, abandoned: true }],
    [13, { channel: 2, blocking: false, abandoned: false }],
  ]);

  /** [frame, expected action or the KELD-IPC-005 detail its selected policy raises]. */
  const rows: Array<[string, FrameHeader, string]> = [
    ["PING on any channel is echoed", { ...header(FrameKind.Ping, 42, 9), len: 0 }, "ping"],
    ["REPLY for a pending echo call", header(FrameKind.Reply, ECHO_CHANNEL, 10), "append"],
    ["ERR for a pending echo call", header(FrameKind.Err, ECHO_CHANNEL, 10), "frame kind is not declared by the session policy"],
    ["REPLY for the blocking call", header(FrameKind.Reply, LIFECYCLE_CHANNEL, 11), "claim"],
    ["ERR for the blocking call", header(FrameKind.Err, LIFECYCLE_CHANNEL, 11), "claim"],
    ["REPLY for the blocking id on another channel", header(FrameKind.Reply, ECHO_CHANNEL, 11), "wrong channel for the session policy"],
    ["late REPLY for an abandoned id", header(FrameKind.Reply, LIFECYCLE_CHANNEL, 12), "discard"],
    ["REPLY for a pending privileged call", header(FrameKind.Reply, 2, 13), "append"],
    ["REPLY for an id that is not pending", header(FrameKind.Reply, ECHO_CHANNEL, 999), "frame kind is not declared by the session policy"],
    ["EVENT on a declared channel", header(FrameKind.Event, LIFECYCLE_CHANNEL, 0), "append"],
    ["correlated EVENT on a declared channel", header(FrameKind.Event, LIFECYCLE_CHANNEL, 4), "correlation must be 0 for this frame"],
    ["EVENT on an undeclared channel", header(FrameKind.Event, 2, 0), "frame kind is not declared by the session policy"],
    ["host echo CALL", header(FrameKind.Call, ECHO_CHANNEL, 77), "append"],
    ["host echo CALL with correlation 0", header(FrameKind.Call, ECHO_CHANNEL, 0), "correlation 0 is reserved"],
    ["host CALL on a declared privileged channel", header(FrameKind.Call, 2, 5), "append"],
    ["host CALL on an undeclared channel", header(FrameKind.Call, LIFECYCLE_CHANNEL, 5), "frame kind is not declared by the session policy"],
    ["HELLO after authentication", { ...header(FrameKind.Hello, 0, 0), len: 32 }, "frame kind is not declared by the session policy"],
    ["STREAM_OPEN", header(FrameKind.StreamOpen, LIFECYCLE_CHANNEL, 0), "frame kind is not declared by the session policy"],
    ["GRANT (no credit lane before T4)", header(FrameKind.Grant, LIFECYCLE_CHANNEL, 0), "frame kind is not declared by the session policy"],
  ];

  test("trusted state selects the policy; the unchanged validator decides", () => {
    for (const [name, frame, expected] of rows) {
      const { policy, action } = selectInboundPolicy(frame, table, pending);
      let outcome: string;
      try {
        validateReceivedHeader(policy, frame);
        outcome = action;
      } catch (err) {
        outcome = err instanceof Error ? err.message.replace("KELD-IPC-005: ", "") : String(err);
      }
      expect(`${name}: ${outcome}`).toBe(`${name}: ${expected}`);
    }
  });

  test("the reply waiter comes from the pending entry's channel, never the frame's", () => {
    expect(selectInboundPolicy(header(FrameKind.Reply, ECHO_CHANNEL, 10), table, pending).policy).toEqual(
      echoReplyWaiter(10),
    );
    expect(selectInboundPolicy(header(FrameKind.Reply, ECHO_CHANNEL, 11), table, pending).policy).toEqual(
      replyWaiter(LIFECYCLE_CHANNEL, 11),
    );
  });
});
