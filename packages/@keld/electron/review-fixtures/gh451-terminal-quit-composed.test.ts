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
  onMappedTerminal?: (source: "effect" | "status") => Promise<void>,
) {
  const trace: string[] = [];
  const sent: Request[] = [];
  const leases: CorrelationLease[] = [];
  const consumed: ResolvedReceipt[] = [];
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
  let currentGeneration = generation;
  let onConsumedCancel: (() => void) | undefined;
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
      lease?: CorrelationLease;
      attempted: boolean;
    }
  >();
  function map(
    result: LinkOutcome,
    source: "effect" | "status",
    statusError?: unknown,
  ): Disposition<Token> {
    if (result.kind === "unknown")
      return {
        kind: "unknown",
        original: result.original,
        error: source === "status" ? statusError : api.effectError,
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
    const expectedSource = source === "effect" ? "direct" : "status";
    if (!receipt) {
      const history = api.historicalTerminalReceipt;
      if (
        history?.source === expectedSource &&
        history.disposition === result &&
        (d.kind === "accepted" || d.kind === "competing")
      ) {
        // An active facade may lag transport retirement. Validate the
        // delivery below without forwarding an acknowledgement to the API.
        deliveries.set(d, { receipt: history, attempted: false });
        return d;
      }
    }
    const lease = ledger.original;
    if (
      !receipt ||
      !lease ||
      receipt.source !== expectedSource ||
      receipt.disposition !== result
    )
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
    census: () => ({
      generation: currentGeneration,
      ready: current,
      selected: current,
      windows,
    }),
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
      const disposition = map(await api.commit(), "effect");
      if (
        onMappedTerminal &&
        (disposition.kind === "accepted" || disposition.kind === "competing")
      )
        await onMappedTerminal("effect");
      return disposition;
    },
    async status(original) {
      const previousFailure = api.statusFailure;
      const result = await api.reconcile();
      const failure = api.statusFailure;
      const statusError =
        failure !== previousFailure ? failure?.error : undefined;
      const actual =
        result.kind === "unknown"
          ? result.original
          : (api.pendingReceipt ?? api.historicalTerminalReceipt)?.original;
      if (!actual || !sameOriginal(actual, original))
        return {
          original,
          disposition: { kind: "unknown", original, error: statusError },
        };
      const disposition = map(result, "status", statusError);
      if (
        onMappedTerminal &&
        (disposition.kind === "accepted" || disposition.kind === "competing")
      )
        await onMappedTerminal("status");
      return { original: actual, disposition };
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
      if (!mapped.lease) return { ready: true };
      // The lease can retire across the async handoff after a live mapping.
      // Only the exact terminal history can settle this delivery without Consume.
      if (
        !ledger.isReserved(mapped.lease) &&
        api.historicalTerminalReceipt === mapped.receipt
      )
        return { ready: true };
      if (!mapped.attempted) {
        if (api.pendingReceipt !== mapped.receipt)
          return { ready: false, error: new Error("receipt no longer exact") };
        mapped.attempted = true;
        consumed.push(mapped.receipt);
        api.consumeReceipt(mapped.receipt);
      }
      if (ledger.isReserved(mapped.lease) || ledger.status)
        return { ready: false, pending: true };
      if (delivery.disposition.kind !== "prepared") token = undefined;
      if (delivery.disposition.kind === "canceled") onConsumedCancel?.();
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
    consumed,
    ledger,
    late,
    reply(value: unknown, noLate = true) {
      replies.push({ run: () => Promise.resolve(value), noLate });
    },
    lose(error = new Error("missing terminal receipt")) {
      replies.push({ run: () => Promise.reject(error), noLate: false });
      return error;
    },
    holdFailure() {
      const { promise, reject } = Promise.withResolvers<unknown>();
      replies.push({ run: () => promise, noLate: false });
      return reject;
    },
    replaceRoleOnCancelConsumption() {
      onConsumedCancel = () => {
        currentGeneration = generation + 1n;
      };
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
  const originalQuit = f.coordinator.quit();
  let settlement = "pending";
  void originalQuit.then(
    () => {
      settlement = "fulfilled";
    },
    () => {
      settlement = "rejected";
    },
  );
  await awaitObserved(() => f.coordinator.heldDisposition?.kind === "prepared");
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(settlement).toBe("pending");
  expect(f.coordinator.quit()).toBe(originalQuit);
  expect(f.late).toEqual([]);
  expect(f.trace).toEqual(["before", "prepare"]);
  expect(f.ledger.original).toBeDefined();
  f.proveOriginalNoLate();
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.reconcile()).toBe("committed");
  await expect(originalQuit).resolves.toBeUndefined();
  expect(settlement).toBe("fulfilled");
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
  expect(f.fences).toBe(1);
});

for (const operation of ["prepare", "commit"] as const) {
  test(`composed: ${operation} Retained waits for receipt release without false failure`, async () => {
    const f = fixture();
    if (operation === "commit") f.reply({ kind: "prepared", token: "old" });
    f.reply({ kind: "retained" }, false);
    const publicQuit = f.coordinator.quit();
    await awaitObserved(
      () => f.coordinator.heldDisposition?.kind === "retained",
    );
    await expect(publicQuit).rejects.toThrow("KELD-COMPAT-012: retained");
    const expected: Request["operation"][] =
      operation === "prepare" ? ["prepare"] : ["prepare", "commit"];
    const held = f.ledger.original;
    expect(held).toBeDefined();
    for (let i = 0; i < 2; i++) {
      expect(f.coordinator.quit()).toBe(publicQuit);
      expect(await f.coordinator.reconcile()).toBe("unknown");
      expect(f.ledger.original).toBe(held);
      expect(f.sent.map((request) => request.operation)).toEqual(expected);
      expect(f.late).toEqual([]);
    }
    f.proveOriginalNoLate();
    expect(await f.coordinator.reconcile()).toBe("retained");
    expect(f.ledger.original).toBeUndefined();
    expect(f.late).toEqual([]);
    f.reply({ kind: "retained" });
    const retry = f.coordinator.quit();
    expect(retry).not.toBe(publicQuit);
    await expect(retry).rejects.toThrow("KELD-COMPAT-012: retained");
    expect(f.sent.map((request) => request.operation)).toEqual([
      ...expected,
      "prepare",
    ]);
    expect(f.fences).toBe(0);
  });
}

test("composed: recovered Invalidated with proof held produces no synthetic errors", async () => {
  const f = fixture();
  const lost = f.lose();
  const publicQuit = f.coordinator.quit();
  await expect(publicQuit).rejects.toBe(lost);
  const original = f.original();
  f.recorded({ kind: "invalidated" }, original);
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.coordinator.heldDisposition?.kind).toBe("invalidated");
  for (let i = 0; i < 2; i++) {
    expect(f.coordinator.quit()).toBe(publicQuit);
    expect(await f.coordinator.reconcile()).toBe("unknown");
    expect(f.sent).toEqual([
      { operation: "prepare" },
      { operation: "status", original },
    ]);
    expect(f.late).toEqual([]);
  }
  f.proveOriginalNoLate();
  expect(await f.coordinator.reconcile()).toBe("retained");
  expect(f.late).toEqual([]);
  await expect(publicQuit).rejects.toBe(lost);
  f.reply({ kind: "retained" });
  const retry = f.coordinator.quit();
  expect(retry).not.toBe(publicQuit);
  await expect(retry).rejects.toThrow("KELD-COMPAT-012: retained");
  expect(f.sent.map((request) => request.operation)).toEqual([
    "prepare",
    "status",
    "prepare",
  ]);
  expect(f.fences).toBe(0);
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
  expectTerminalCustodyUntilRetirement(f);
  expect(f.fences).toBe(1);
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
});

test("composed: competing terminal winner is never this attempt's acceptance", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "first" });
  f.reply({ kind: "competing-terminal" });
  expect(await f.coordinator.requestQuit()).toBe("competing");
  expect(f.api.terminalDisposition).toBe("competing-terminal");
  expectTerminalCustodyUntilRetirement(f);
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
  expectTerminalCustodyUntilRetirement(f);
  expect(f.fences).toBe(1);
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "commit"]);
});

function expectTerminalCustodyUntilRetirement(f: ReturnType<typeof fixture>) {
  const lease = f.leases.at(-1)!;
  expect(f.ledger.original).toBe(lease);
  expect(f.ledger.proveNoLate(lease)).toBe(false);
  expect(f.ledger.original).toBe(lease);
  expect(f.ledger.proveRetired(lease)).toBe(true);
  expect(f.ledger.isReserved(lease)).toBe(false);
}

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
  const lease = f.ledger.original!;
  const original = f.original();
  f.retire();
  expect(f.ledger.proveRetired(lease)).toBe(false);
  expect(f.api.historicalTerminalReceipt).toBeUndefined();
  await expect(publicQuit).rejects.toThrow("retired");
  expect(await first).toBe("unknown");
  deliver({ kind: "accepted" });
  await awaitObserved(() => f.coordinator.disposition === "committed");
  expect(f.api.terminalDisposition).toBe("accepted");
  expect(f.api.pendingReceipt).toBeUndefined();
  const history = f.api.historicalTerminalReceipt!;
  expect(history).toEqual({
    original,
    disposition: { kind: "accepted" },
    source: "direct",
  });
  expect(f.api.consumeReceipt(history)).toBe(false);
  expect(f.ledger.isReserved(lease)).toBe(false);
  expect(f.ledger.original).toBeUndefined();
  expect(f.consumed.map((receipt) => receipt.original.operation)).toEqual([
    "prepare",
  ]);
  expect(f.late).toEqual([]);
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
  const lease = f.ledger.original!;
  f.retire();
  expect(f.ledger.proveRetired(lease)).toBe(false);
  expect(f.api.historicalTerminalReceipt).toBeUndefined();
  expect(await recovery).toBe("unknown");
  const reportsAtRetirement = [...f.late];
  deliver({ kind: "recorded", original, disposition: { kind: "accepted" } });
  await awaitObserved(() => f.coordinator.disposition === "committed");
  expect(f.api.terminalDisposition).toBe("accepted");
  expect(f.api.pendingReceipt).toBeUndefined();
  const history = f.api.historicalTerminalReceipt!;
  expect(history).toEqual({
    original,
    disposition: { kind: "accepted" },
    source: "status",
  });
  expect(f.api.consumeReceipt(history)).toBe(false);
  expect(f.ledger.isReserved(lease)).toBe(false);
  expect(f.ledger.original).toBeUndefined();
  expect(f.consumed.map((receipt) => receipt.original.operation)).toEqual([
    "prepare",
  ]);
  expect(f.late).toEqual(reportsAtRetirement);
  await expect(publicQuit).rejects.toBe(lost);
  expect(f.fences).toBe(0);
  expect(f.sent.map((x) => x.operation)).toEqual([
    "prepare",
    "commit",
    "status",
  ]);
});

test("composed: confirmed Cancel holds one queued retry until independent no-late proof", async () => {
  const f = fixture();
  f.veto(true);
  f.reply({ kind: "prepared", token: "old" });
  let deliver!: (value: unknown) => void;
  f.reply(
    new Promise<unknown>((resolve) => {
      deliver = resolve;
    }),
    false,
  );
  const first = f.coordinator.quit();
  await awaitObserved(() => f.sent.at(-1)?.operation === "cancel");
  await expect(first).resolves.toBeUndefined();
  const retry = f.coordinator.quit();
  expect(retry).not.toBe(first);
  deliver({ kind: "cancelled" });
  await awaitObserved(() => f.coordinator.heldDisposition?.kind === "canceled");

  expect(f.coordinator.quit()).toBe(retry);
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.sent.map((x) => x.operation)).toEqual(["prepare", "cancel"]);
  expect(f.late).toEqual([]);

  f.proveOriginalNoLate();
  f.veto(false);
  f.reply({ kind: "prepared", token: "fresh" });
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.reconcile()).toBe("vetoed");
  await expect(retry).resolves.toBeUndefined();
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "cancel", token: "old" },
    { operation: "prepare" },
    { operation: "commit", token: "fresh" },
  ]);
});

test("composed: role change during Cancel consumption blocks queued callbacks", async () => {
  const f = fixture();
  f.veto(true);
  f.reply({ kind: "prepared", token: "old" });
  const cancel = Promise.withResolvers<unknown>();
  f.reply(cancel.promise);
  const first = f.coordinator.quit();
  await awaitObserved(() => f.sent.at(-1)?.operation === "cancel");
  const queued = f.coordinator.quit();
  expect(queued).not.toBe(first);
  f.replaceRoleOnCancelConsumption();
  cancel.resolve({ kind: "cancelled" });
  await expect(first).resolves.toBeUndefined();
  await expect(queued).rejects.toThrow("original role retired");
  expect(f.trace).toEqual(["before", "prepare", "will", "cancel"]);
});

test("composed: a no-send Status never re-reports prior failure custody", async () => {
  const f = fixture();
  const effect = f.lose();
  await expect(f.coordinator.quit()).rejects.toBe(effect);
  const original = f.original();
  const status = f.lose();
  expect(await f.coordinator.reconcile()).toBe("unknown");
  const held = f.ledger.status!;
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "status", original },
  ]);
  expect(f.api.statusError).toBe(status);
  expect(f.late).toHaveLength(1);
  expect(f.late[0]!.error).toBe(status);
  expect(f.late[0]!.custody).toEqual({ phase: "status", original, generation });
  f.ledger.proveNoLate(held);
  const next = f.lose();
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.late).toHaveLength(2);
  expect(f.late[0]!.error).toBe(status);
  expect(f.late[1]!.error).toBe(next);
  expect(f.sent).toHaveLength(3);
  f.ledger.proveNoLate(f.ledger.status!);
  f.lose(next); // A new dispatch may reject with the same object.
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.late).toHaveLength(3);
  expect(f.late[0]!.error).toBe(status);
  expect(f.late[1]!.error).toBe(next);
  expect(f.late[2]!.error).toBe(next);
  expect(f.sent).toHaveLength(4);
});

for (const source of ["effect", "status"] as const) {
  test(`composed: exact late ${source} error transfers custody once after retirement`, async () => {
    const f = fixture();
    f.reply({ kind: "prepared", token: "retired" });
    const firstFailure = source === "status" ? f.lose() : undefined;
    const rejectLate = f.holdFailure();
    const publicQuit = f.coordinator.quit();
    await awaitObserved(() => f.sent.at(-1)?.operation === "commit");
    const original = f.original();
    if (firstFailure) {
      await expect(publicQuit).rejects.toBe(firstFailure);
      void f.coordinator.reconcile();
      await awaitObserved(() => f.sent.at(-1)?.operation === "status");
    }
    f.retire();
    const error = new Error("held exact late failure");
    rejectLate(error);
    await awaitObserved(() => f.late.some((item) => item.error === error));
    expect(f.late.filter((item) => item.error === error)).toEqual([
      {
        error,
        custody: {
          phase: source === "status" ? "status" : "commit",
          original,
          generation,
        },
      },
    ]);
    if (firstFailure) await expect(publicQuit).rejects.toBe(firstFailure);
    else await expect(publicQuit).rejects.toThrow("original role retired");
    expect(f.coordinator.quit()).toBe(publicQuit);
    expect(await f.coordinator.reconcile()).toBe("unknown");
    expect(f.coordinator.disposition).toBe("unknown");
    expect(f.fences).toBe(0);
    expect(f.sent.map((request) => request.operation)).toEqual(
      source === "status"
        ? ["prepare", "commit", "status"]
        : ["prepare", "commit"],
    );
  });
}

test("composed: retirement during local refusal delivery preserves the typed error", async () => {
  const ledger = new DraftCorrelationLedger(1, 1);
  const f = fixture([], true, ledger);
  const ordinary = ledger.reserveOrdinary()!;
  expect(ordinary).toBeDefined();
  const publicQuit = f.coordinator.quit();
  f.retire();
  await expect(publicQuit).rejects.toThrow("original role retired");
  await awaitObserved(() => f.late.length === 1);
  expect(f.late[0]?.error).toBeInstanceOf(Error);
  expect((f.late[0]?.error as Error).message).toBe("draft refusal: capacity");
  expect(f.late[0]?.custody).toEqual({
    phase: "prepare",
    original: undefined,
    generation,
  });
  expect(f.coordinator.quit()).toBe(publicQuit);
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.sent).toEqual([]);
  expect(f.ledger.original).toBeUndefined();
  expect(f.ledger.isReserved(ordinary)).toBe(true);
  expect(f.late).toHaveLength(1);
});

test("composed: direct Invalidated is unknown until exact Prepare Status", async () => {
  const f = fixture();
  f.reply({ kind: "invalidated" }, false);
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  const original = f.original();
  const lease = f.ledger.original!;
  expect(await f.api.prepare()).toEqual({ kind: "unknown", original });
  expect(f.api.pendingReceipt).toBeUndefined();
  expect(f.consumed).toEqual([]);

  f.recorded({ kind: "invalidated" }, { ...original, correlation: 99 });
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.ledger.original).toBe(lease);
  expect(f.consumed).toEqual([]);

  f.recorded({ kind: "invalidated" }, original);
  expect(await f.coordinator.reconcile()).toBe("unknown");
  expect(f.consumed).toEqual([
    { original, disposition: { kind: "invalidated" }, source: "status" },
  ]);
  expect(f.api.conditionalAvailable).toBe(false);
  expect(f.ledger.original).toBe(lease);
  f.proveOriginalNoLate();
  expect(await f.coordinator.reconcile()).toBe("retained");
  expect(f.api.conditionalAvailable).toBe(true);
  expect(f.ledger.isReserved(lease)).toBe(false);
  expect(f.sent).toEqual([
    { operation: "prepare" },
    { operation: "status", original },
    { operation: "status", original },
  ]);

  f.reply({ kind: "prepared", token: "fresh" });
  f.reply({ kind: "accepted" });
  expect(await f.coordinator.requestQuit()).toBe("committed");
  expect(f.sent.slice(3)).toEqual([
    { operation: "prepare" },
    { operation: "commit", token: "fresh" },
  ]);
});

test("composed: definitive proof can precede facade retirement notice", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "old" });
  let deliver!: (value: unknown) => void;
  f.reply(
    new Promise<unknown>((resolve) => {
      deliver = resolve;
    }),
    false,
  );
  const first = f.coordinator.requestQuit();
  await awaitObserved(() => f.sent.at(-1)?.operation === "commit");
  const lease = f.ledger.original!;
  expect(f.ledger.proveRetired(lease)).toBe(false);
  deliver({ kind: "accepted" });
  expect(await first).toBe("committed");
  expect(f.api.historicalTerminalReceipt?.disposition).toEqual({
    kind: "accepted",
  });
  expect(f.api.pendingReceipt).toBeUndefined();
  expect(f.ledger.isReserved(lease)).toBe(false);
  expect(f.fences).toBe(0);
  expect(f.consumed.map((receipt) => receipt.original.operation)).toEqual([
    "prepare",
  ]);
  expect(f.late).toEqual([]);
});

for (const source of ["effect", "status"] as const) {
  test(`composed: retirement between ${source} mapping and consumption stays exact`, async () => {
    const mapped = Promise.withResolvers<"effect" | "status">();
    const handoff = Promise.withResolvers<void>();
    const f = fixture([], true, undefined, undefined, (observed) => {
      mapped.resolve(observed);
      return handoff.promise;
    });
    f.reply({ kind: "prepared", token: "old" });
    const lost = source === "status" ? f.lose() : undefined;
    if (!lost) f.reply({ kind: "accepted" }, false);
    const first = f.coordinator.quit();
    let recovered: ReturnType<typeof f.coordinator.reconcile> | undefined;
    if (lost) {
      await expect(first).rejects.toBe(lost);
      f.recorded({ kind: "accepted" }, f.original());
      recovered = f.coordinator.reconcile();
    }
    expect(await mapped.promise).toBe(source);
    const lease = f.ledger.original!;
    const receipt = f.api.pendingReceipt!;
    const before = [...f.late];
    try {
      expect(receipt.disposition.kind).toBe("accepted");
      expect(f.api.historicalTerminalReceipt).toBeUndefined();
      expect(f.api.terminalDisposition).toBe("accepted");
      expect(f.coordinator.disposition).not.toBe("committed");
      expect(f.fences).toBe(1);
      expect(f.ledger.proveRetired(lease)).toBe(true);
      expect(f.ledger.isReserved(lease)).toBe(false);
    } finally {
      handoff.resolve();
    }
    if (lost) {
      expect(await recovered).toBe("committed");
      await expect(first).rejects.toBe(lost);
    } else {
      await expect(first).resolves.toBeUndefined();
    }
    expect(f.coordinator.disposition).toBe("committed");
    expect(f.api.historicalTerminalReceipt).toBe(receipt);
    expect(f.api.pendingReceipt).toBeUndefined();
    expect(f.consumed.map((x) => x.original.operation)).toEqual(["prepare"]);
    expect(f.late).toEqual(before);
    expect(f.fences).toBe(1);
    expect(f.sent.map((x) => x.operation)).toEqual(
      source === "status"
        ? ["prepare", "commit", "status"]
        : ["prepare", "commit"],
    );
  });
}
