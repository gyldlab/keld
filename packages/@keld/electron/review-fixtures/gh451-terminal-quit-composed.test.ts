// GH451 review-only composition of the isolated API and facade candidates.
// Owned by the Electron package's review-only test suite.
// Independent scripted receipt/no-late witnesses, not real IPC or OS proof.
import { expect, test } from "bun:test";
import {
  DraftCorrelationLedger,
  RetainedLinkDraft,
  type CorrelationLease,
  type Original,
  type Outcome as LinkOutcome,
  type Request,
  type ResolvedReceipt,
  type RetainedPort,
  type Token,
} from "../../api/fixtures/gh451-retained-link-draft/candidate";
import {
  DraftQuitCoordinator,
  type Disposition,
  type DraftHost,
  type ErrorCustody,
  type WindowIdentity,
} from "./gh451-terminal-quit-draft";

const generation = 1n;
const window = { id: 1, incarnation: 11n, document: 101n };

function fixture(
  initial: readonly WindowIdentity[] = [],
  negotiated = true,
  ledger = new DraftCorrelationLedger(),
  onWill?: () => void,
) {
  const trace: string[] = [];
  const sent: Request[] = [];
  const leases: CorrelationLease[] = [];
  const replies: Array<{ run: () => Promise<unknown>; noLate: boolean }> = [];
  const late: Array<{ error: unknown; custody: ErrorCustody<Token> }> = [];
  let current = true;
  let live = true;
  let fenceCount = 0;
  let token: Token | undefined;
  let windows = [...initial];
  let veto = false;
  let fenceError: Error | undefined;
  let invalidated = false;
  const port: RetainedPort = {
    negotiated,
    selected: true,
    ledger,
    isCurrent: () => current,
    isLive: () => live,
    send(lease, request) {
      expect(ledger.isReserved(lease)).toBe(true);
      expect(
        request.operation === "status" ? ledger.status : ledger.original,
      ).toBe(lease);
      trace.push(request.operation);
      sent.push(request);
      leases.push(lease);
      const reply = replies.shift();
      if (!reply) throw new Error("unexpected draft request");
      return reply.run().then((value) => {
        // Explicit fixture input; a Status result alone does NOT prove
        // quiescence of the earlier effect's transport identity.
        if (reply.noLate) ledger.proveNoLate(lease);
        return value;
      });
    },
    fenceAccepted() {
      trace.push("fence");
      fenceCount++;
      if (fenceError) throw fenceError;
      live = false;
    },
  };
  const api = new RetainedLinkDraft(port);
  const deliveries = new WeakMap<
    object,
    {
      receipt: ResolvedReceipt;
      lease: CorrelationLease;
      attempted: boolean;
    }
  >();
  function map(
    result: LinkOutcome,
    source: "effect" | "status",
  ): Disposition<Token> {
    if (result.kind === "unknown")
      return {
        kind: "unknown",
        original: result.original,
        error:
          (source === "status" ? api.statusError : api.effectError) ??
          new Error("draft effect unknown"),
      };
    if (result.kind === "refused")
      return {
        kind: "refused",
        error: new Error("draft refusal: " + result.reason),
      };
    const d: Disposition<Token> =
      result.kind === "cancelled"
        ? { kind: "canceled" }
        : result.kind === "competing-terminal"
          ? { kind: "competing" }
          : result.kind === "accepted"
            ? { kind: "accepted", error: api.fenceError }
            : { ...result };
    const receipt = api.pendingReceipt;
    const lease = ledger.original;
    if (!receipt || !lease || receipt.disposition !== result)
      throw new Error("missing exact mapped receipt");
    deliveries.set(d, { receipt, lease, attempted: false });
    if (d.kind === "prepared") token = d.token;
    return d;
  }
  function sameOriginal(a: Original, b: Original) {
    return (
      a.operation === b.operation &&
      a.correlation === b.correlation &&
      a.token === b.token
    );
  }
  const host: DraftHost<Token> = {
    conditionalEligible: api.conditionalAvailable,
    census: () => ({ generation, ready: current, selected: current, windows }),
    requestClose(w, dispatch) {
      trace.push("close:" + w.id);
      dispatch(7n);
      return Promise.resolve({ kind: "requested", sequence: 7n });
    },
    async prepare() {
      const result = map(await api.prepare(), "effect");
      if (result.kind === "accepted" || result.kind === "canceled")
        throw new Error("invalid disposition for Prepare");
      return result;
    },
    async cancel(t) {
      if (t !== token || token === undefined) return { kind: "unknown" };
      return map(await api.cancel(), "effect");
    },
    async commit(t) {
      if (t !== token || token === undefined) return { kind: "unknown" };
      return map(await api.commit(), "effect");
    },
    async status(original) {
      const result = await api.reconcile();
      const actual =
        result.kind === "unknown"
          ? result.original
          : api.pendingReceipt?.original;
      if (!actual || !sameOriginal(actual, original))
        return {
          original,
          disposition: { kind: "unknown", original, error: api.statusError },
        };
      return { original: actual, disposition: map(result, "status") };
    },
    observedInvalidation(t) {
      return invalidated && t === token;
    },
    reportLateError(error, custody) {
      late.push({ error, custody });
    },
    consumeDisposition(delivery) {
      const mapped = deliveries.get(delivery.disposition);
      if (!mapped)
        return {
          ready: false,
          error: new Error("missing exact consumer receipt"),
        };
      if (
        mapped.receipt.original.operation !== delivery.operation ||
        mapped.receipt.original.token !== delivery.token ||
        (delivery.original &&
          !sameOriginal(mapped.receipt.original, delivery.original))
      ) {
        return {
          ready: false,
          error: new Error("consumer receipt identity mismatch"),
        };
      }
      if (!mapped.attempted) {
        if (api.pendingReceipt !== mapped.receipt)
          return { ready: false, error: new Error("receipt no longer exact") };
        mapped.attempted = true;
        api.consumeReceipt(mapped.receipt);
      }
      if (ledger.isReserved(mapped.lease) || ledger.status)
        return {
          ready: false,
          error: new Error("independent no-late proof pending"),
        };
      if (delivery.disposition.kind !== "prepared") token = undefined;
      return { ready: true };
    },
  };
  const coordinator = new DraftQuitCoordinator(
    host,
    {
      beforeQuit() {
        trace.push("before");
        return false;
      },
      willQuit() {
        trace.push("will");
        onWill?.();
        return veto;
      },
      lastWindowClosed() {
        trace.push("last");
      },
    },
    generation,
  );
  return {
    api,
    coordinator,
    trace,
    sent,
    leases,
    ledger,
    late,
    reply(value: unknown, noLate = true) {
      replies.push({ run: () => Promise.resolve(value), noLate });
    },
    lose(error = new Error("missing terminal receipt")) {
      replies.push({ run: () => Promise.reject(error), noLate: false });
      return error;
    },
    original() {
      const lease = ledger.original;
      if (!lease) throw new Error("no original reservation");
      const request = sent[leases.indexOf(lease)];
      if (!request || request.operation === "status")
        throw new Error("missing original effect");
      return {
        operation: request.operation,
        correlation: lease.correlation,
        token: request.operation === "prepare" ? undefined : request.token,
      };
    },
    recorded(disposition: unknown, original: Original) {
      replies.push({
        run: () => Promise.resolve({ kind: "recorded", original, disposition }),
        noLate: true,
      });
    },
    proveOriginalNoLate() {
      const original = ledger.original;
      if (!original) throw new Error("no original identity");
      ledger.proveNoLate(original);
    },
    veto(value: boolean) {
      veto = value;
    },
    invalidate() {
      invalidated = true;
    },
    retire() {
      current = false;
      live = false;
      coordinator.retire(generation);
    },
    failFence(error: Error) {
      fenceError = error;
    },
    destroy() {
      windows = [];
      coordinator.observeDestroyed(generation, window);
    },
    get live() {
      return live;
    },
    get fences() {
      return fenceCount;
    },
  };
}

test("composed: public handoff before physical receipt, accepted fence only after actual fact", async () => {
  const f = fixture([window]);
  f.reply({ kind: "prepared", token: "first" });
  f.reply({ kind: "accepted" });
  const publicQuit = f.coordinator.quit();
  const attempt = f.coordinator.requestQuit();
  await publicQuit;
  expect(f.coordinator.quit()).toBe(publicQuit);
  expect(f.trace).toEqual(["before", "close:1"]);
  expect(f.sent).toEqual([]);
  f.destroy();
  expect(await attempt).toBe("committed");
  expect(f.trace).toEqual([
    "before",
    "close:1",
    "prepare",
    "will",
    "commit",
    "fence",
  ]);
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "commit", token: "first" },
  ]);
  expect(f.fences).toBe(1);
});

test("composed: will veto cancels exact token and retains the original link for retry", async () => {
  const f = fixture();
  f.veto(true);
  f.reply({ kind: "prepared", token: "old" });
  f.reply({ kind: "cancelled" });
  expect(await f.coordinator.requestQuit()).toBe("vetoed");
  expect(f.live).toBe(true);
  expect(f.fences).toBe(0);
  f.veto(false);
  f.reply({ kind: "prepared", token: "new" });
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.requestQuit()).toBe("committed");
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "cancel", token: "old" },
    { operation: "prepare" },
    { operation: "commit", token: "new" },
  ]);
  expect(f.fences).toBe(1);
});

test("composed: retained Commit is nonterminal and permits fresh preparation", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "retired" });
  f.reply({ kind: "retained" });
  expect(await f.coordinator.requestQuit()).toBe("retained");
  expect(f.live).toBe(true);
  expect(f.fences).toBe(0);
  f.reply({ kind: "prepared", token: "fresh" });
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.requestQuit()).toBe("committed");
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "commit", token: "retired" },
    { operation: "prepare" },
    { operation: "commit", token: "fresh" },
  ]);
  expect(f.fences).toBe(1);
});

test("composed: missing Commit receipt stops replay without fencing the link", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "unknown" });
  f.lose();
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  expect(await f.api.commit()).toMatchObject({ kind: "unknown" });
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "commit", token: "unknown" },
  ]);
  expect(f.fences).toBe(0);
  expect(f.live).toBe(true);
});

for (const operation of ["cancel", "commit"] as const) {
  test(`composed: ${operation} allocation refusal preserves preparation without a new identity`, async () => {
    const ledger = new DraftCorrelationLedger(1);
    let ordinary: CorrelationLease | undefined;
    const f = fixture([], true, ledger, () => {
      ordinary = ledger.reserveOrdinary();
    });
    f.veto(operation === "cancel");
    f.reply({ kind: "prepared", token: "held" });
    const publicQuit = f.coordinator.quit();
    if (operation === "cancel")
      await expect(publicQuit).resolves.toBeUndefined();
    else await expect(publicQuit).rejects.toThrow("draft refusal: capacity");
    expect(await f.coordinator.requestQuit()).toBe("unknown");
    expect(ordinary).toBeDefined();
    expect(ledger.active).toBe(true);
    expect(ledger.original).toBeUndefined();
    expect(ledger.status).toBeUndefined();
    expect(f.api.pendingReceipt).toBeUndefined();
    expect(f.sent).toEqual([{ operation: "prepare" }]);
    expect(f.trace).toEqual(["before", "prepare", "will"]);
    // Reclaim only the ordinary slot, not the surviving preparation.
    expect(ledger.consume(ordinary!)).toBe(false);
    expect(ledger.proveNoLate(ordinary!)).toBe(true);
    expect(ledger.reservedCount).toBe(0);
    expect(await f.api.prepare()).toEqual({
      kind: "refused",
      reason: "wrong-phase",
    });
    expect(await f.coordinator.reconcile()).toBe("unknown");
    expect(await f.coordinator.requestQuit()).toBe("unknown");
    expect(ledger.active).toBe(true);
    expect(ledger.original).toBeUndefined();
    expect(ledger.status).toBeUndefined();
    expect(f.sent).toEqual([{ operation: "prepare" }]);
    expect(f.trace).toEqual(["before", "prepare", "will"]);
    expect(f.live).toBe(true);
    expect(f.fences).toBe(0);
  });
}

test("composed: old peer with a live window has no conditional close/effect/fallback", async () => {
  const f = fixture([window], false);
  expect(await f.coordinator.requestQuit()).toBe("retained");
  expect(f.trace).toEqual(["before"]);
  expect(f.sent).toEqual([]);
  expect(f.fences).toBe(0);
});

test("composed: lost Prepare reconciles exact original then invokes will once", async () => {
  const f = fixture();
  const error = f.lose();
  const publicQuit = f.coordinator.quit();
  await expect(publicQuit).rejects.toBe(error);
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  const original = f.original();
  f.proveOriginalNoLate(); // independent transport witness
  f.recorded({ kind: "prepared", token: "recovered" }, original);
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.reconcile()).toBe("committed");
  expect(f.trace).toEqual([
    "before",
    "prepare",
    "status",
    "will",
    "commit",
    "fence",
  ]);
  expect(f.sent[1]).toEqual({ operation: "status", original });
  expect(f.leases[1]!.correlation).not.toBe(original.correlation);
  expect(f.api.terminalDisposition).toBe("accepted");
  expect(f.fences).toBe(1);
  await expect(publicQuit).rejects.toBe(error);
});

test("composed: lost Cancel recovers cancellation and allows one fresh attempt", async () => {
  const f = fixture();
  f.veto(true);
  f.reply({ kind: "prepared", token: "veto" });
  f.lose();
  await f.coordinator.quit(); // local veto settlement, not Cancel receipt
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  const original = f.original();
  f.proveOriginalNoLate();
  f.recorded({ kind: "cancelled" }, original);
  expect(await f.coordinator.reconcile()).toBe("vetoed");
  expect(f.fences).toBe(0);
  expect(f.live).toBe(true);
  f.veto(false);
  f.reply({ kind: "prepared", token: "fresh" });
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.requestQuit()).toBe("committed");
  expect(f.trace).toEqual([
    "before",
    "prepare",
    "will",
    "cancel",
    "status",
    "before",
    "prepare",
    "will",
    "commit",
    "fence",
  ]);
});

test("composed: lost Commit reconciles retained without replay, then fresh attempt", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "old" });
  f.lose();
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  const original = f.original();
  f.proveOriginalNoLate();
  f.recorded({ kind: "retained" }, original);
  expect(await f.coordinator.reconcile()).toBe("retained");
  expect(f.live).toBe(true);
  f.reply({ kind: "prepared", token: "new" });
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.requestQuit()).toBe("committed");
  expect(f.sent.map((x) => x.operation)).toEqual([
    "prepare",
    "commit",
    "status",
    "prepare",
    "commit",
  ]);
  expect(f.fences).toBe(1);
});

test("composed: lost Commit reconciles accepted once with no duplicate effect", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "first" });
  f.lose();
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  const original = f.original();
  f.proveOriginalNoLate();
  f.recorded({ kind: "accepted" }, original);
  expect(await f.coordinator.reconcile()).toBe("committed");
  expect(await f.coordinator.reconcile()).toBe("committed");
  expect(f.sent.map((x) => x.operation)).toEqual([
    "prepare",
    "commit",
    "status",
  ]);
  expect(f.fences).toBe(1);
});

test("composed: stale original Status cannot authorize a new effect", async () => {
  const f = fixture();
  f.lose();
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  const original = f.original();
  f.recorded(
    { kind: "prepared", token: "wrong" },
    { ...original, correlation: 99 },
  );
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.trace).toEqual(["before", "prepare", "status"]);
  expect(f.ledger.original?.correlation).toBe(original.correlation);
  expect(f.fences).toBe(0);
});

test("composed: known Prepare held until independent no-late proof permits continuation", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "held" }, false);
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  expect(f.trace).toEqual(["before", "prepare"]);
  expect(f.ledger.original).toBeDefined();
  f.proveOriginalNoLate();
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.reconcile()).toBe("committed");
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
  expect(f.fences).toBe(1);
});

test("composed: known Accepted survives exact local fence failure", async () => {
  const f = fixture();
  const failure = new Error("local fence failure");
  f.failFence(failure);
  f.reply({ kind: "prepared", token: "first" });
  f.reply({ kind: "accepted" });
  const publicQuit = f.coordinator.quit();
  await expect(publicQuit).rejects.toBe(failure);
  expect(await f.coordinator.requestQuit()).toBe("committed");
  expect(f.api.terminalDisposition).toBe("accepted");
  expect(f.api.fenceError).toBe(failure);
  expect(f.fences).toBe(1);
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
});

test("composed: competing terminal winner is never this attempt's acceptance", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "first" });
  f.reply({ kind: "competing-terminal" });
  expect(await f.coordinator.requestQuit()).toBe("competing");
  expect(f.api.terminalDisposition).toBe("competing-terminal");
  expect(f.fences).toBe(0);
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
});

test("composed: Accepted with no-late proof held fulfills after local fence", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "accepted" });
  f.reply({ kind: "accepted" }, false);
  expect(await f.coordinator.quit()).toBeUndefined();
  expect(f.coordinator.disposition).toBe("committed");
  expect(f.api.terminalDisposition).toBe("accepted");
  expect(f.ledger.original).toBeDefined();
  expect(f.fences).toBe(1);
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
});

async function awaitObserved(predicate: () => boolean) {
  for (let i = 0; i < 32 && !predicate(); i++) await Promise.resolve();
  expect(predicate()).toBe(true);
}

test("composed: late exact Commit receipt after retirement is historical only", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "old" });
  let deliver!: (value: unknown) => void;
  f.reply(
    new Promise<unknown>((resolve) => {
      deliver = resolve;
    }),
    false,
  );
  const publicQuit = f.coordinator.quit();
  await awaitObserved(() => f.sent.at(-1)?.operation === "commit");
  const first = f.coordinator.outcome;
  f.retire();
  await expect(publicQuit).rejects.toThrow("retired");
  expect(await first).toBe("unknown");
  deliver({ kind: "accepted" });
  await awaitObserved(() => f.coordinator.disposition === "committed");
  expect(f.api.terminalDisposition).toBe("accepted");
  expect(await first).toBe("unknown");
  await expect(publicQuit).rejects.toThrow("retired");
  expect(f.fences).toBe(0);
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
});

test("composed: exact late Status after retirement preserves terminal history", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "old" });
  const lost = f.lose();
  const publicQuit = f.coordinator.quit();
  await expect(publicQuit).rejects.toBe(lost);
  const original = f.original();
  let deliver!: (value: unknown) => void;
  f.reply(
    new Promise<unknown>((resolve) => {
      deliver = resolve;
    }),
    false,
  );
  const recovery = f.coordinator.reconcile();
  await awaitObserved(() => f.sent.at(-1)?.operation === "status");
  f.retire();
  expect(await recovery).toBe("unknown");
  deliver({ kind: "recorded", original, disposition: { kind: "accepted" } });
  await awaitObserved(() => f.coordinator.disposition === "committed");
  expect(f.api.terminalDisposition).toBe("accepted");
  await expect(publicQuit).rejects.toBe(lost);
  expect(f.fences).toBe(0);
  expect(f.sent.map((x) => x.operation)).toEqual([
    "prepare",
    "commit",
    "status",
  ]);
});
