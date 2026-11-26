import { expect, test } from "bun:test";
import {
  DraftCorrelationLedger,
  RetainedLinkDraft,
  type CorrelationLease,
  type Disposition,
  type Original,
  type Request,
  type RetainedPort,
} from "./candidate.ts";

// A scripted, independent observation of writes, correlations and local
// fencing. It deliberately does not emulate Core, a codec, a socket or OS.
class Port implements RetainedPort {
  negotiated = true;
  selected = true;
  current = true;
  live = true;
  fenceCount = 0;
  readonly ledger: DraftCorrelationLedger;
  readonly leases: CorrelationLease[] = [];
  fenceFailure?: Error;
  holdLate = false;
  readonly sent: Array<{ request: Request; correlation: number }> = [];
  readonly script: Array<() => Promise<unknown>> = [];
  constructor(max = 0xffff_ffff, next = 41) {
    this.ledger = new DraftCorrelationLedger(max, next);
  }
  isCurrent() {
    return this.current;
  }
  isLive() {
    return this.live;
  }
  send(lease: CorrelationLease, request: Request): Promise<unknown> {
    expect(this.ledger.isReserved(lease)).toBe(true);
    expect(
      request.operation === "status"
        ? this.ledger.status
        : this.ledger.original,
    ).toBe(lease);
    const correlation = lease.correlation;
    this.leases.push(lease);
    this.sent.push({ request, correlation });
    const next = this.script.shift();
    if (!next) throw new Error("unexpected effect or Status");
    const holdLate = this.holdLate;
    return next().then((value) => {
      // A delivered terminal reply in this scripted port excludes late
      // aliases unless the scenario explicitly holds that proof.
      if (!holdLate) this.ledger.proveNoLate(lease);
      return value;
    });
  }
  fenceAccepted() {
    this.fenceCount++;
    if (this.fenceFailure) throw this.fenceFailure;
    this.live = false;
  }
  reply(value: unknown) {
    this.script.push(() => Promise.resolve(value));
  }
  lose() {
    this.script.push(() => Promise.reject(new Error("lost ACK")));
  }
  recorded(disposition: unknown, original: unknown) {
    this.reply({ kind: "recorded", original, disposition });
  }
  ordinaryProbe() {
    if (!this.live || !this.current) throw new Error("same link closed");
    return "same port";
  }
  ops() {
    return this.sent.map((x) => x.request.operation);
  }
}

function ack(api: RetainedLinkDraft) {
  const receipt = api.pendingReceipt;
  expect(receipt).toBeDefined();
  expect(api.consumeReceipt(receipt!)).toBe(true);
  expect(api.pendingReceipt).toBeUndefined();
}

test("old and unselected peers send zero conditional or fallback operations", async () => {
  for (const gate of ["negotiated", "selected"] as const) {
    const port = new Port();
    port[gate] = false;
    const api = new RetainedLinkDraft(port);
    port[gate] = true; // captured eligibility cannot become true after construction
    expect(api.conditionalAvailable).toBe(false);
    expect(await api.prepare()).toEqual({
      kind: "refused",
      reason: "unsupported",
    });
    expect(await api.cancel()).toMatchObject({ kind: "refused" });
    expect(await api.commit()).toMatchObject({ kind: "refused" });
    expect(await api.reconcile()).toMatchObject({ kind: "refused" });
    expect(port.ops()).toEqual([]);
    expect(port.ledger.reservedCount).toBe(0);
    expect(port.ordinaryProbe()).toBe("same port");
  }
});

test("Prepare Retained keeps same port and permits new preparation", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "retained" });
  expect(await api.prepare()).toEqual({ kind: "retained" });
  ack(api);
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-1" });
  expect(await api.prepare()).toEqual({ kind: "prepared", token: "host-1" });
  expect(port.ops()).toEqual(["prepare", "prepare"]);
  expect(port.fenceCount).toBe(0);
});

test("confirmed Cancel and Commit Retained release attempt without closing link", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "host-1" });
  await api.prepare();
  ack(api);
  port.reply({ kind: "cancelled" });
  expect(await api.cancel()).toEqual({ kind: "cancelled" });
  ack(api);
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-2" });
  await api.prepare();
  ack(api);
  port.reply({ kind: "retained" });
  expect(await api.commit()).toEqual({ kind: "retained" });
  ack(api);
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-3" });
  expect(await api.prepare()).toEqual({ kind: "prepared", token: "host-3" });
  expect(port.sent.map((x) => x.request)).toEqual([
    { operation: "prepare" },
    { operation: "cancel", token: "host-1" },
    { operation: "prepare" },
    { operation: "commit", token: "host-2" },
    { operation: "prepare" },
  ]);
  expect(port.fenceCount).toBe(0);
});

test("only known Commit Accepted fences once, after the correlated reply", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "host-1" });
  await api.prepare();
  ack(api);
  let resolve!: (value: unknown) => void;
  port.script.push(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const commit = api.commit();
  expect(port.fenceCount).toBe(0);
  expect(await api.commit()).toEqual({ kind: "refused", reason: "busy" });
  expect(await api.cancel()).toEqual({ kind: "refused", reason: "busy" });
  expect(port.ops()).toEqual(["prepare", "commit"]);
  resolve({ kind: "accepted" });
  expect(await commit).toEqual({ kind: "accepted" });
  expect(port.fenceCount).toBe(1);
  expect(await api.prepare()).toMatchObject({ kind: "refused" });
  expect(port.ops()).toEqual(["prepare", "commit"]);
});

for (const effect of ["prepare", "cancel", "commit"] as const) {
  for (const failure of [
    "lost",
    "malformed",
    "wrong-operation",
    "generic-error",
  ] as const) {
    test(
      effect + " " + failure + " preserves original identity and blocks replay",
      async () => {
        const port = new Port();
        const api = new RetainedLinkDraft(port);
        if (effect !== "prepare") {
          port.reply({ kind: "prepared", token: "host-1" });
          await api.prepare();
          ack(api);
        }
        if (failure === "lost") port.lose();
        else if (failure === "malformed") port.reply(null);
        else if (failure === "wrong-operation")
          port.reply({ kind: effect === "commit" ? "cancelled" : "accepted" });
        else port.reply({ kind: "error", code: "generic" });
        const original: Original = {
          operation: effect,
          correlation: effect === "prepare" ? 41 : 42,
          token: effect === "prepare" ? undefined : "host-1",
        };
        expect(await api[effect]()).toEqual({ kind: "unknown", original });
        const prior = port.ops();
        expect(await api.prepare()).toEqual({ kind: "unknown", original });
        expect(await api.cancel()).toEqual({ kind: "unknown", original });
        expect(await api.commit()).toEqual({ kind: "unknown", original });
        expect(port.ops()).toEqual(prior);
        expect(port.fenceCount).toBe(0);
        const resolved: Disposition =
          effect === "prepare"
            ? { kind: "prepared", token: "host-recovered" }
            : effect === "cancel"
              ? { kind: "cancelled" }
              : { kind: "accepted" };
        port.recorded(resolved, original);
        expect(await api.reconcile()).toEqual(resolved);
        expect(port.sent.at(-1)).toEqual({
          request: { operation: "status", original },
          correlation: original.correlation + 1,
        });
        expect(port.ops()).toEqual([...prior, "status"]);
        expect(port.fenceCount).toBe(effect === "commit" ? 1 : 0);
        const recovered = api.pendingReceipt!;
        expect(recovered.source).toBe("status");
        expect(recovered.original).toEqual(original);
        expect(recovered.disposition).toEqual(resolved);
        if (failure === "lost") port.ledger.proveNoLate(port.leases.at(-2)!);
        ack(api);
        if (effect === "prepare") {
          port.reply({ kind: "cancelled" });
          await api.cancel();
          expect(port.sent.at(-1)?.request).toEqual({
            operation: "cancel",
            token: "host-recovered",
          });
        }
        if (effect === "cancel") {
          expect(port.ordinaryProbe()).toBe("same port");
          port.reply({ kind: "retained" });
          expect(await api.prepare()).toEqual({ kind: "retained" });
        }
      },
    );
  }
}

test("Status mismatch, indeterminate, lost and wrong-operation results remain unknown", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "host-1" });
  await api.prepare();
  ack(api);
  port.lose();
  const original: Original = {
    operation: "commit",
    correlation: 42,
    token: "host-1",
  };
  expect(await api.commit()).toEqual({ kind: "unknown", original });
  for (const wrong of [
    { ...original, correlation: 99 },
    { ...original, token: "other" },
    { ...original, operation: "cancel" },
  ]) {
    port.recorded({ kind: "accepted" }, wrong);
    expect(await api.reconcile()).toEqual({ kind: "unknown", original });
  }
  port.reply({ kind: "indeterminate" });
  expect(await api.reconcile()).toEqual({ kind: "unknown", original });
  port.lose();
  expect(await api.reconcile()).toEqual({ kind: "unknown", original });
  // Releasing an abandoned Status requires independent no-late evidence.
  port.ledger.proveNoLate(port.leases.at(-1)!);
  port.recorded({ kind: "cancelled" }, original);
  expect(await api.reconcile()).toEqual({ kind: "unknown", original });
  expect(port.fenceCount).toBe(0);
  port.recorded({ kind: "retained" }, original);
  expect(await api.reconcile()).toEqual({ kind: "retained" });
  port.ledger.proveNoLate(port.leases[1]!);
  ack(api);
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-2" });
  expect(await api.prepare()).toEqual({ kind: "prepared", token: "host-2" });
  expect(port.ops()).toEqual([
    "prepare",
    "commit",
    ...Array(7).fill("status"),
    "prepare",
  ]);
});

test("retired or ended original link cannot reconcile through successor", async () => {
  for (const failure of ["current", "live"] as const) {
    const originalPort = new Port();
    const successor = new Port();
    const api = new RetainedLinkDraft(originalPort);
    originalPort.lose();
    const unknown = await api.prepare();
    originalPort[failure] = false;
    expect(await api.reconcile()).toEqual(unknown);
    expect(await api.prepare()).toEqual(unknown);
    expect(originalPort.ops()).toEqual(["prepare"]);
    expect(successor.ops()).toEqual([]);
    expect(successor.ordinaryProbe()).toBe("same port");
    expect(originalPort.fenceCount).toBe(0);
  }
});

test("concurrent Status cannot start a second Status or effect", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.lose();
  await api.prepare();
  const original: Original = {
    operation: "prepare",
    correlation: 41,
    token: undefined,
  };
  let resolve!: (value: unknown) => void;
  port.script.push(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const pending = api.reconcile();
  expect(await api.reconcile()).toEqual({ kind: "refused", reason: "busy" });
  expect(await api.prepare()).toEqual({ kind: "refused", reason: "busy" });
  resolve({ kind: "recorded", original, disposition: { kind: "retained" } });
  expect(await pending).toEqual({ kind: "retained" });
  expect(port.ops()).toEqual(["prepare", "status"]);
});

test("synchronous send failure retains reserved original identity for Status", async () => {
  const port = new Port(); // no scripted receipt
  const api = new RetainedLinkDraft(port);
  const original: Original = {
    operation: "prepare",
    correlation: 41,
    token: undefined,
  };
  expect(await api.prepare()).toEqual({ kind: "unknown", original });
  expect(await api.prepare()).toEqual({ kind: "unknown", original });
  port.recorded({ kind: "retained" }, original);
  expect(await api.reconcile()).toEqual({ kind: "retained" });
  expect(port.ops()).toEqual(["prepare", "status"]);
  expect(api.consumeReceipt(api.pendingReceipt!)).toBe(false);
  expect(port.ledger.isReserved(port.leases[0]!)).toBe(true);
});

test("lost effect and Status errors retain exact distinct causes", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  const effectError = new Error("original timeout");
  const statusError = new Error("status timeout");
  port.script.push(() => Promise.reject(effectError));
  const unknown = await api.prepare();
  port.script.push(() => Promise.reject(statusError));
  expect(await api.reconcile()).toEqual(unknown);
  expect(api.effectError).toBe(effectError);
  expect(api.statusError).toBe(statusError);
  expect(port.ops()).toEqual(["prepare", "status"]);
});

test("late Prepared after role retirement cannot authorize will-quit", async () => {
  for (const status of [false, true]) {
    const port = new Port();
    const api = new RetainedLinkDraft(port);
    const original: Original = {
      operation: "prepare",
      correlation: 41,
      token: undefined,
    };
    if (status) {
      port.lose();
      await api.prepare();
    }
    let resolve!: (value: unknown) => void;
    port.script.push(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const pending = status ? api.reconcile() : api.prepare();
    port.current = false;
    const prepared = { kind: "prepared", token: "retired-host" };
    resolve(
      status ? { kind: "recorded", original, disposition: prepared } : prepared,
    );
    expect(await pending).toEqual({ kind: "unknown", original });
    expect(await api.commit()).toEqual({ kind: "unknown", original });
    expect(port.fenceCount).toBe(0);
    expect(port.ops()).toEqual(status ? ["prepare", "status"] : ["prepare"]);
  }
});

test("Status may not reuse unresolved effect correlation", async () => {
  const port = new Port(1, 1);
  const api = new RetainedLinkDraft(port);
  port.lose();
  const unknown = await api.prepare();
  // Wrap has no free identity. Reject before a Status write.
  expect(await api.reconcile()).toEqual(unknown);
  expect(await api.prepare()).toEqual(unknown);
  expect(port.fenceCount).toBe(0);
  expect(port.ops()).toEqual(["prepare"]);
  expect(port.ledger.reservedCount).toBe(1);
});

test("one active attempt survives Prepared receipt release and requires exact consumption", async () => {
  const port = new Port(2, 1);
  const api = new RetainedLinkDraft(port);
  const other = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "token-1" });
  expect(await api.prepare()).toEqual({ kind: "prepared", token: "token-1" });
  const receipt = api.pendingReceipt!;
  expect(receipt.source).toBe("direct");
  expect(api.consumeReceipt({ ...receipt })).toBe(false);
  expect(await api.commit()).toEqual({
    kind: "refused",
    reason: "receipt-pending",
  });
  expect(await other.prepare()).toEqual({ kind: "refused", reason: "busy" });
  expect(port.ops()).toEqual(["prepare"]);
  expect(api.consumeReceipt(receipt)).toBe(true);
  expect(port.ledger.reservedCount).toBe(0);
  expect(port.ledger.active).toBe(true);
  expect(other.conditionalAvailable).toBe(false);
  expect(await other.prepare()).toEqual({ kind: "refused", reason: "busy" });
  port.reply({ kind: "cancelled" });
  expect(await api.cancel()).toEqual({ kind: "cancelled" });
  ack(api);
  expect(port.ledger.active).toBe(false);
  expect(other.conditionalAvailable).toBe(true);
  port.reply({ kind: "retained" });
  expect(await other.prepare()).toEqual({ kind: "retained" });
  expect(port.ops()).toEqual(["prepare", "cancel", "prepare"]);
});

test("last no-late witness retires nonterminal attempt without polling the old wrapper", async () => {
  const port = new Port(2, 1);
  const first = new RetainedLinkDraft(port);
  const next = new RetainedLinkDraft(port);
  port.holdLate = true;
  port.reply({ kind: "retained" });
  expect(await first.prepare()).toEqual({ kind: "retained" });
  const lease = port.leases[0]!;
  expect(first.consumeReceipt(first.pendingReceipt!)).toBe(false);
  expect(next.conditionalAvailable).toBe(false);
  expect(port.ledger.active).toBe(true);
  expect(port.ledger.proveNoLate(lease)).toBe(true);
  expect(port.ledger.reservedCount).toBe(0);
  // No call to first may be needed to retire the shared attempt.
  expect(port.ledger.active).toBe(false);
  expect(next.conditionalAvailable).toBe(true);
  port.holdLate = false;
  port.reply({ kind: "prepared", token: "successor" });
  expect(await next.prepare()).toEqual({
    kind: "prepared",
    token: "successor",
  });
  expect(first.pendingReceipt).toBeUndefined();
  expect(port.ledger.active).toBe(true);
  expect(await first.prepare()).toEqual({ kind: "refused", reason: "busy" });
  ack(next);
  expect(port.ledger.active).toBe(true);
});

test("wrap and capacity preserve unresolved original until both consumption and no-late proof", async () => {
  const port = new Port(2, 2);
  const api = new RetainedLinkDraft(port);
  port.lose();
  const original: Original = {
    operation: "prepare",
    correlation: 2,
    token: undefined,
  };
  expect(await api.prepare()).toEqual({ kind: "unknown", original });
  const lease = port.leases[0]!;
  const ordinary = port.ledger.reserveOrdinary()!;
  expect(ordinary.correlation).toBe(1);
  expect(port.ledger.reserveOrdinary()).toBeUndefined();
  // Capacity and timeout are not nonexecution or retirement evidence.
  for (let i = 0; i < 8; i++) {
    expect(await api.reconcile()).toEqual({ kind: "unknown", original });
    expect(await api.prepare()).toEqual({ kind: "unknown", original });
    expect(port.ledger.reservedCount).toBe(2);
    expect(port.ledger.original).toBe(lease);
  }
  expect(port.ops()).toEqual(["prepare"]);
  expect(port.ledger.consume(ordinary)).toBe(false);
  expect(port.ledger.proveNoLate(ordinary)).toBe(true);
  port.recorded({ kind: "retained" }, original);
  expect(await api.reconcile()).toEqual({ kind: "retained" });
  expect(port.leases[1]!.correlation).toBe(1);
  const receipt = api.pendingReceipt!;
  expect(api.consumeReceipt(receipt)).toBe(false);
  expect(api.conditionalAvailable).toBe(false);
  expect(await api.prepare()).toEqual({
    kind: "refused",
    reason: "receipt-pending",
  });
  const pressure = port.ledger.reserveOrdinary()!;
  expect(pressure.correlation).toBe(1);
  expect(port.ledger.reserveOrdinary()).toBeUndefined();
  expect(port.ledger.proveNoLate(lease)).toBe(true);
  const reused = port.ledger.reserveOrdinary()!;
  expect(reused.correlation).toBe(2);
  expect(reused).not.toBe(lease);
  expect(port.ledger.proveNoLate(lease)).toBe(false);
  expect(port.ledger.consume(lease)).toBe(false);
  expect(api.consumeReceipt(receipt)).toBe(false);
  expect(port.ledger.isReserved(reused)).toBe(true);
  expect(port.ledger.consume(reused)).toBe(false);
  expect(port.ledger.proveNoLate(reused)).toBe(true);
  expect(port.ledger.proveNoLate(pressure)).toBe(false);
  expect(port.ledger.consume(pressure)).toBe(true);
  expect(api.conditionalAvailable).toBe(true);
  port.reply({ kind: "retained" });
  expect(await api.prepare()).toEqual({ kind: "retained" });
  ack(api);
  expect(port.ledger.reservedCount).toBe(0);
  expect(port.ledger.active).toBe(false);
});

test("no-late proof alone cannot retire an unconsumed effect", async () => {
  const port = new Port(1, 1);
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "retained" });
  expect(await api.prepare()).toEqual({ kind: "retained" });
  const receipt = api.pendingReceipt!;
  expect(port.ledger.reserveOrdinary()).toBeUndefined();
  expect(port.ledger.isReserved(port.leases[0]!)).toBe(true);
  expect(await api.prepare()).toEqual({
    kind: "refused",
    reason: "receipt-pending",
  });
  expect(api.consumeReceipt(receipt)).toBe(true);
  expect(port.ledger.active).toBe(false);
  expect(port.ledger.reserveOrdinary()?.correlation).toBe(1);
});

test("capacity before first send leaves no effect and no active attempt", async () => {
  const port = new Port(1, 1);
  const api = new RetainedLinkDraft(port);
  const lease = port.ledger.reserveOrdinary()!;
  expect(await api.prepare()).toEqual({ kind: "refused", reason: "capacity" });
  expect(port.ops()).toEqual([]);
  expect(port.ledger.active).toBe(false);
  expect(port.ledger.consume(lease)).toBe(false);
  expect(port.ledger.proveNoLate(lease)).toBe(true);
  port.reply({ kind: "retained" });
  expect(await api.prepare()).toEqual({ kind: "retained" });
  ack(api);
});

test("lost Status occupies its separate bounded slot until independent no-late proof", async () => {
  const port = new Port(2, 1);
  const api = new RetainedLinkDraft(port);
  port.lose();
  const unknown = await api.prepare();
  port.lose();
  expect(await api.reconcile()).toEqual(unknown);
  const original = port.leases[0]!;
  const status = port.leases[1]!;
  expect(original.correlation).not.toBe(status.correlation);
  expect(port.ledger.original).toBe(original);
  expect(port.ledger.status).toBe(status);
  for (let i = 0; i < 4; i++) expect(await api.reconcile()).toEqual(unknown);
  expect(port.ops()).toEqual(["prepare", "status"]);
  expect(port.ledger.reservedCount).toBe(2);
  expect(port.ledger.proveNoLate(status)).toBe(true);
  port.recorded({ kind: "retained" }, { operation: "prepare", correlation: 1 });
  expect(await api.reconcile()).toEqual({ kind: "retained" });
  expect(port.leases[2]!.correlation).toBe(2);
  expect(port.leases[2]).not.toBe(status);
  expect(port.ledger.proveNoLate(original)).toBe(false);
  ack(api);
  expect(port.ledger.reservedCount).toBe(0);
});

for (const recovery of [false, true]) {
  test(
    "known Accepted survives local fence failure, recovery=" + recovery,
    async () => {
      const port = new Port();
      const api = new RetainedLinkDraft(port);
      port.reply({ kind: "prepared", token: "token" });
      await api.prepare();
      ack(api);
      const failure = new Error("local fence failure");
      port.fenceFailure = failure;
      if (recovery) {
        port.lose();
        expect(await api.commit()).toMatchObject({ kind: "unknown" });
        port.recorded(
          { kind: "accepted" },
          { operation: "commit", correlation: 42, token: "token" },
        );
        expect(await api.reconcile()).toEqual({ kind: "accepted" });
      } else {
        port.reply({ kind: "accepted" });
        expect(await api.commit()).toEqual({ kind: "accepted" });
      }
      const receipt = api.pendingReceipt!;
      expect(receipt.disposition).toEqual({ kind: "accepted" });
      expect(receipt.source).toBe(recovery ? "status" : "direct");
      expect(api.fenceError).toBe(failure);
      expect(api.terminalDisposition).toBe("accepted");
      expect(api.conditionalAvailable).toBe(false);
      expect(port.fenceCount).toBe(1);
      expect(await api.commit()).toEqual({
        kind: "refused",
        reason: "wrong-phase",
      });
      expect(await api.prepare()).toEqual({
        kind: "refused",
        reason: "wrong-phase",
      });
      expect(await api.reconcile()).toEqual({
        kind: "refused",
        reason: "wrong-phase",
      });
      if (recovery) port.ledger.proveNoLate(port.leases[1]!);
      ack(api);
      expect(api.terminalDisposition).toBe("accepted");
      expect(port.fenceCount).toBe(1);
      expect(port.ops()).toEqual(
        recovery ? ["prepare", "commit", "status"] : ["prepare", "commit"],
      );
    },
  );
}

test("recovered invalidated Prepare and competing terminal are distinct dispositions", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.lose();
  expect(await api.prepare()).toMatchObject({ kind: "unknown" });
  port.recorded(
    { kind: "invalidated" },
    { operation: "prepare", correlation: 41 },
  );
  expect(await api.reconcile()).toEqual({ kind: "invalidated" });
  port.ledger.proveNoLate(port.leases[0]!);
  ack(api);
  expect(await api.commit()).toEqual({
    kind: "refused",
    reason: "wrong-phase",
  });
  port.reply({ kind: "prepared", token: "second" });
  await api.prepare();
  ack(api);
  port.reply({ kind: "competing-terminal" });
  expect(await api.commit()).toEqual({ kind: "competing-terminal" });
  ack(api);
  expect(api.terminalDisposition).toBe("competing-terminal");
  expect(api.conditionalAvailable).toBe(false);
  expect(port.fenceCount).toBe(0);
  expect(await api.prepare()).toEqual({
    kind: "refused",
    reason: "wrong-phase",
  });
  expect(port.ops()).toEqual(["prepare", "status", "prepare", "commit"]);
});

test("historical Accepted after retirement is known but never fences a successor", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "token" });
  await api.prepare();
  ack(api);
  let resolve!: (value: unknown) => void;
  port.script.push(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const pending = api.commit();
  port.current = false;
  port.live = false;
  resolve({ kind: "accepted" });
  expect(await pending).toEqual({ kind: "accepted" });
  expect(api.terminalDisposition).toBe("accepted");
  expect(port.fenceCount).toBe(0);
  ack(api);
});
