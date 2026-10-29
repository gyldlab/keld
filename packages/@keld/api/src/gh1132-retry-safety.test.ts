import { Database } from "bun:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";

import {
  ECHO_CHANNEL,
  FrameKind,
  decodeCallError,
} from "../../kipc/src/transport.ts";
import { resolveEchoCall, type EchoCallHandler } from "./echo-call.ts";

const PAYLOAD = Uint8Array.from([0, 0]);
const LOGICAL_ACTION = "one-logical-action";

function latch(): { wait: Promise<void>; release: () => void } {
  let release = () => {};
  const wait = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { wait, release };
}

function withSqlite(run: (db: Database) => Promise<void>): () => Promise<void> {
  return async () => {
    const directory = mkdtempSync(join(tmpdir(), "keld-gh1132-"));
    const db = new Database(join(directory, "actions.sqlite"));
    try {
      await run(db);
    } finally {
      db.close();
      rmSync(directory, { recursive: true, force: true });
    }
  };
}

describe("GH-1132: a status snapshot is not a replay fence", () => {
  test(
    "counterexample: API-001 can return before a deferred effect, so status-only resend commits twice",
    withSqlite(async (db) => {
      db.exec(
        "CREATE TABLE effects (id INTEGER PRIMARY KEY, logical_key TEXT NOT NULL)",
      );
      const insert = db.query("INSERT INTO effects (logical_key) VALUES (?1)");
      const count = db.query(
        "SELECT COUNT(*) AS n FROM effects WHERE logical_key = ?1",
      );
      const committed = (): number =>
        (count.get(LOGICAL_ACTION) as { n: number }).n;

      const delayed = latch();
      const outstanding: Promise<unknown>[] = [];
      const first: EchoCallHandler = async () => {
        outstanding.push(delayed.wait.then(() => insert.run(LOGICAL_ACTION)));
        throw new Error("handler failed but did not cancel its child task");
      };

      try {
        const firstAnswer = await resolveEchoCall(ECHO_CHANNEL, PAYLOAD, first);
        expect(firstAnswer.kind).toBe(FrameKind.Err);
        expect(decodeCallError(firstAnswer.payload).code).toBe("KELD-API-001");
        expect(committed()).toBe(0); // A real durable status query sees "absent".

        // This is the *old, unsafe app policy* — never recommended by GH-1132.
        const statusOnlyAllowsResend = committed() === 0;
        expect(statusOnlyAllowsResend).toBe(true);
        const retry = await resolveEchoCall(ECHO_CHANNEL, PAYLOAD, async () => {
          insert.run(LOGICAL_ACTION);
          return Uint8Array.of(0x41);
        });
        expect(retry.kind).toBe(FrameKind.Reply);

        delayed.release(); // Earlier work commits *after* the status query.
        await Promise.all(outstanding);
        expect(committed()).toBe(2); // Why the old spec rule is unsafe.
      } finally {
        delayed.release();
        await Promise.allSettled(outstanding);
      }
    }),
  );

  test(
    "safe app control: durable idempotency returns the recorded result in either completion order",
    withSqlite(async (db) => {
      db.exec(
        "CREATE TABLE effects (logical_key TEXT PRIMARY KEY, result INTEGER NOT NULL)",
      );
      const commit = db.query(
        "INSERT INTO effects (logical_key, result) VALUES (?1, ?2) ON CONFLICT(logical_key) DO NOTHING",
      );
      const read = db.query(
        "SELECT result FROM effects WHERE logical_key = ?1",
      );
      const count = db.query(
        "SELECT COUNT(*) AS n FROM effects WHERE logical_key = ?1",
      );
      const committed = (key: string): number =>
        (count.get(key) as { n: number }).n;

      // The application, not Keld, uses a stable key carried in *both* CALL
      // payloads. An atomic insertion is the complete effect in this fixture;
      // real external effects need their own idempotent receiver/protocol.
      const perform = (
        payload: Uint8Array,
        proposedResult: number,
      ): Uint8Array => {
        const key = new TextDecoder().decode(payload);
        commit.run(key, proposedResult);
        const row = read.get(key) as { result: number } | null;
        if (row === null) throw new Error("durable effect record disappeared");
        return Uint8Array.of(row.result);
      };

      for (const order of ["retry-first", "original-first"] as const) {
        const key = `${LOGICAL_ACTION}-${order}`;
        const payload = new TextEncoder().encode(key);
        const delayed = latch();
        const outstanding: Promise<unknown>[] = [];
        const first: EchoCallHandler = async (_channel, bytes) => {
          outstanding.push(delayed.wait.then(() => perform(bytes, 66)));
          throw new Error("first handler did not await its deferred effect");
        };

        try {
          const firstAnswer = await resolveEchoCall(
            ECHO_CHANNEL,
            payload,
            first,
          );
          expect(firstAnswer.kind).toBe(FrameKind.Err);
          expect(decodeCallError(firstAnswer.payload).code).toBe(
            "KELD-API-001",
          );
          expect(committed(key)).toBe(0);

          if (order === "original-first") {
            delayed.release();
            await Promise.all(outstanding);
          }

          // Competing retries propose a different result (65 versus 66).
          // Both MUST read the single durable winner's recorded value.
          const retry = await resolveEchoCall(
            ECHO_CHANNEL,
            payload,
            async (_channel, bytes) => perform(bytes, 65),
          );
          expect(retry.kind).toBe(FrameKind.Reply);

          delayed.release();
          await Promise.all(outstanding);
          const expected = order === "retry-first" ? 65 : 66;
          expect(retry.payload).toEqual(Uint8Array.of(expected));
          expect((read.get(key) as { result: number }).result).toBe(expected);
          expect(committed(key)).toBe(1);
        } finally {
          delayed.release();
          await Promise.allSettled(outstanding);
        }
      }
    }),
  );
});
