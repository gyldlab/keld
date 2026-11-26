// GH451 Q1 D2 and AC11 scripted proof, not authenticated/native evidence.
// Owned by the Electron package's review-only test suite.
import { expect, test } from "bun:test";
import {
  fixture,
  g,
  w1,
  w2,
  token,
  turn,
} from "./gh451-terminal-quit-draft-fixture";
import type { Original } from "./gh451-terminal-quit-draft";
import type { Token } from "./gh451-terminal-quit-draft-fixture";

test("D2: before veto/throw resolves, nested public Promise identity and fresh retry", async () => {
  const x = fixture([w1]);
  let nested: Promise<void> | undefined;
  x.f.before = () => {
    nested = x.coordinator.quit();
    throw Error("listener");
  };
  const p = x.coordinator.quit();
  expect(nested).toBe(p);
  expect(await p).toBeUndefined();
  expect(await x.coordinator.outcome).toBe("vetoed");
  x.f.before = () => true;
  const next = x.coordinator.quit();
  expect(next).not.toBe(p);
  expect(await next).toBeUndefined();
  expect(x.trace).toEqual(["before", "before"]);
});

test("D2: single-window genuine dispatch fulfills before close callback can await nested Quit", async () => {
  const x = fixture([w1]);
  let nested: Promise<void> | undefined;
  let nestedDone = false;
  x.f.onRequest = (_w, dispatch) => {
    dispatch(1n);
    nested = x.coordinator.quit();
    void nested.then(() => {
      nestedDone = true;
    });
  };
  const p = x.coordinator.quit();
  expect(nested).toBe(p);
  await turn();
  expect(nestedDone).toBe(true);
  expect(await p).toBeUndefined();
  expect(x.coordinator.quit()).toBe(p);
  expect(x.prepares).toHaveLength(0);
  expect(x.coordinator.disposition).toBeUndefined();
  x.coordinator.observeVetoApplied(g, w1, 1n);
  expect(await x.coordinator.outcome).toBe("vetoed");
});

test("D2: multiwindow first qualified handoff resolves, bare send does not, full sweep continues", async () => {
  const x = fixture([w1, w2]);
  const p = x.coordinator.quit();
  let done = false;
  void p.then(() => {
    done = true;
  });
  await turn();
  expect(done).toBe(false);
  expect(x.trace).toEqual(["before", "close:2", "close:1"]);
  x.closes[0]!.dispatched(4n);
  await turn();
  expect(done).toBe(true);
  expect(x.coordinator.quit()).toBe(p);
  expect(x.closes[1]!.w).toBe(w1);
  expect(x.prepares).toHaveLength(0);
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 4n });
  x.closes[1]!.reply.resolve({ kind: "requested", sequence: 3n });
  await turn();
  x.destroy(w2);
  expect(x.prepares).toHaveLength(0);
  x.destroy(w1);
  expect(x.prepares).toHaveLength(1);
  await x.prepared(0);
  x.commits[0]!.reply.resolve("accepted");
  expect(await x.coordinator.outcome).toBe("committed");
  expect(await p).toBeUndefined();
});

test("D2: exact typed failure before first handoff rejects; remaining selected request issued", async () => {
  const x = fixture([w1, w2]);
  const p = x.coordinator.quit();
  const typed = Error("KELD-IPC-007: actual close failure");
  x.closes[0]!.reply.resolve({ kind: "refused", error: typed });
  await expect(p).rejects.toBe(typed);
  expect(await x.coordinator.outcome).toBe("retained");
  expect(x.trace).toEqual(["before", "close:2", "close:1"]);
  x.closes[1]!.reply.resolve({ kind: "requested", sequence: 3n });
  await turn();
  expect(x.prepares).toHaveLength(0);
  expect(x.late).toHaveLength(0);
});

test("D2: AlreadyClosing without genuine bound pending receipt rejects; bound case fulfills", async () => {
  const a = fixture([w1]);
  const p = a.coordinator.quit();
  a.closes[0]!.reply.resolve({ kind: "already-closing" });
  await expect(p).rejects.toThrow("KELD-CORE-054");
  const b = fixture([w1]);
  const q = b.coordinator.quit();
  b.closes[0]!.dispatched(2n);
  b.closes[0]!.reply.resolve({ kind: "already-closing" });
  expect(await q).toBeUndefined();
  b.coordinator.observeVetoApplied(g, w1, 2n);
  expect(await b.coordinator.outcome).toBe("vetoed");
});

test("D2: exact late close error after veto and fresh attempt stays with original scope", async () => {
  const x = fixture([w1, w2]);
  const p = x.coordinator.quit();
  x.closes[0]!.dispatched(4n);
  expect(await p).toBeUndefined();
  x.coordinator.observeVetoApplied(g, w2, 4n);
  expect(await x.coordinator.outcome).toBe("vetoed");
  const retry = x.coordinator.quit();
  expect(retry).not.toBe(p);
  const late = Error("KELD-IPC-007: original close reply failed");
  x.closes[1]!.reply.reject(late);
  await turn();
  expect(x.late).toEqual([
    { error: late, custody: { phase: "close", window: w1, generation: g } },
  ]);
  expect(x.coordinator.quit()).toBe(retry);
  expect(await p).toBeUndefined();
  x.closes[2]!.dispatched(5n);
  expect(await retry).toBeUndefined();
  x.coordinator.observeVetoApplied(g, w2, 5n);
  expect(await x.coordinator.outcome).toBe("vetoed");
});

test("AC11: old or unselected nonempty peer sees no new close/prepare; before veto still cancels", async () => {
  for (const mode of ["old", "unselected"] as const) {
    const x = fixture([w1, w2], mode !== "old");
    if (mode === "unselected") x.f.selected = false;
    x.f.before = () => true;
    expect(await x.coordinator.quit()).toBeUndefined();
    x.f.before = () => false;
    const refused = x.coordinator.quit();
    expect(x.trace).toEqual(["before", "before"]);
    await expect(refused).rejects.toThrow("KELD-COMPAT-012");
    expect(x.closes).toHaveLength(0);
    expect(x.prepares).toHaveLength(0);
  }
});

test("D2: zero-window public settlement follows Accepted, not Prepare or will; typed refusal is exact", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  let done = false;
  void p.then(() => {
    done = true;
  });
  await x.prepared(0);
  expect(done).toBe(false);
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
  x.commits[0]!.reply.resolve("accepted");
  expect(await p).toBeUndefined();
  expect(x.coordinator.disposition).toBe("committed");
  const y = fixture();
  const q = y.coordinator.quit();
  const typed = Error("KELD-COMPAT-012: admitted Create");
  y.prepares[0]!.resolve({ kind: "retained", error: typed });
  await expect(q).rejects.toBe(typed);
  expect(y.coordinator.disposition).toBe("retained");
  expect(y.trace).toEqual(["before", "prepare"]);
});

test("D2: will veto/throw fulfills locally; one explicit queued retry waits for confirmed Cancel", async () => {
  for (const mode of ["veto", "throw"] as const) {
    const x = fixture();
    let nested: Promise<void> | undefined;
    x.f.will = () => {
      nested = x.coordinator.quit();
      if (mode === "throw") throw Error("listener");
      return true;
    };
    const p = x.coordinator.quit();
    const t = await x.prepared(0);
    expect(nested).toBe(p);
    expect(await p).toBeUndefined();
    expect(x.cancels[0]!.token).toBe(t);
    const next = x.coordinator.quit();
    expect(next).not.toBe(p);
    expect(x.coordinator.quit()).toBe(next);
    expect(x.trace).toEqual(["before", "prepare", "will", "cancel"]);
    x.f.will = () => false;
    x.cancels[0]!.reply.resolve("canceled");
    await turn();
    expect(x.trace).toEqual([
      "before",
      "prepare",
      "will",
      "cancel",
      "before",
      "prepare",
    ]);
    await x.prepared(1);
    x.commits[0]!.reply.resolve("accepted");
    expect(await next).toBeUndefined();
    expect(x.coordinator.disposition).toBe("committed");
  }
});

test("AC11: lost Prepare rejects exact error; Status resumes original once, no repeated before/will", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const error = Error("original Prepare receipt lost");
  const original: Original<Token> = { operation: "prepare", correlation: 7 };
  x.prepares[0]!.resolve({ kind: "unknown", original, error });
  await expect(p).rejects.toBe(error);
  expect(await x.coordinator.outcome).toBe("unknown");
  expect(x.coordinator.quit()).toBe(p);
  const recovery = x.coordinator.reconcile();
  expect(x.coordinator.reconcile()).toBe(recovery);
  expect(x.statuses[0]!.original).toEqual(original);
  const t = token();
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "prepared", token: t },
  });
  await turn();
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "status:prepare",
    "will",
    "commit",
  ]);
  expect(x.commits[0]!.token).toBe(t);
  expect(x.coordinator.reconcile()).toBe(recovery);
  x.commits[0]!.reply.resolve("accepted");
  expect(await recovery).toBe("committed");
  expect(x.coordinator.disposition).toBe("committed");
  expect(await x.coordinator.requestQuit()).toBe("committed");
  await expect(p).rejects.toBe(error);
});

test("AC11: mismatched Status cannot resume; exact invalidated Prepare never emits will", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const original: Original<Token> = { operation: "prepare", correlation: 9 };
  x.prepares[0]!.resolve({ kind: "unknown", original });
  await expect(p).rejects.toThrow("unknown");
  const bad = x.coordinator.reconcile();
  x.statuses[0]!.reply.resolve({
    original: { ...original, correlation: 10 },
    disposition: { kind: "prepared", token: token() },
  });
  await turn();
  expect(x.trace).toEqual(["before", "prepare", "status:prepare"]);
  expect(await bad).toBe("unknown");
  const good = x.coordinator.reconcile();
  expect(x.statuses[1]!.original).toEqual(original);
  x.statuses[1]!.reply.resolve({
    original,
    disposition: { kind: "invalidated" },
  });
  expect(await good).toBe("retained");
  expect(x.coordinator.disposition).toBe("retained");
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "status:prepare",
    "status:prepare",
  ]);
  const next = x.coordinator.quit();
  expect(next).not.toBe(p);
  x.prepares[1]!.resolve({ kind: "retained" });
  await expect(next).rejects.toThrow("KELD-COMPAT-012");
});

test("AC11: lost Cancel reports exact late error, rejects queued retry; Status cancellation permits explicit fresh call", async () => {
  const x = fixture();
  x.f.will = () => true;
  const p = x.coordinator.quit();
  const t = await x.prepared(0);
  expect(await p).toBeUndefined();
  const queued = x.coordinator.quit();
  const original: Original<Token> = {
    operation: "cancel",
    correlation: 11,
    token: t,
  };
  const error = Error("Cancel ACK lost");
  x.cancels[0]!.reply.resolve({ kind: "unknown", original, error });
  await expect(queued).rejects.toBe(error);
  expect(x.late).toEqual([
    { error, custody: { phase: "cancel", original, generation: g } },
  ]);
  const recovery = x.coordinator.reconcile();
  x.statuses[0]!.reply.resolve({ original, disposition: { kind: "canceled" } });
  expect(await recovery).toBe("vetoed");
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "will",
    "cancel",
    "status:cancel",
  ]);
  x.f.will = () => false;
  const next = x.coordinator.quit();
  await x.prepared(1);
  x.commits[0]!.reply.resolve("accepted");
  expect(await next).toBeUndefined();
  expect(await p).toBeUndefined();
});

test("AC11: lost Commit Status acceptance keeps known commitment despite local fence error", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const t = await x.prepared(0);
  const original: Original<Token> = {
    operation: "commit",
    correlation: 12,
    token: t,
  };
  const lost = Error("Commit ACK lost"),
    fence = Error("local fence failed");
  x.commits[0]!.reply.resolve({ kind: "unknown", original, error: lost });
  await expect(p).rejects.toBe(lost);
  const recovery = x.coordinator.reconcile();
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "accepted", error: fence },
  });
  expect(await recovery).toBe("committed");
  expect(x.coordinator.disposition).toBe("committed");
  expect(x.late).toEqual([
    { error: fence, custody: { phase: "commit", original, generation: g } },
  ]);
  expect(await x.coordinator.requestQuit()).toBe("committed");
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "will",
    "commit",
    "status:commit",
  ]);
  await expect(p).rejects.toBe(lost);
});

test("AC11: direct Accepted plus local fence failure rejects pending public Promise without undoing commitment", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  await x.prepared(0);
  const error = Error("local fence failed");
  x.commits[0]!.reply.resolve({ kind: "accepted", error });
  await expect(p).rejects.toBe(error);
  expect(x.coordinator.disposition).toBe("committed");
  expect(await x.coordinator.requestQuit()).toBe("committed");
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
});

test("AC11: another terminal winner cannot be reported as retained or trigger new effects", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const t = await x.prepared(0);
  const original: Original<Token> = {
    operation: "commit",
    correlation: 13,
    token: t,
  };
  x.commits[0]!.reply.resolve({ kind: "unknown", original });
  await expect(p).rejects.toThrow("unknown");
  const recovery = x.coordinator.reconcile();
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "competing" },
  });
  expect(await recovery).toBe("competing");
  expect(await x.coordinator.requestQuit()).toBe("competing");
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "will",
    "commit",
    "status:commit",
  ]);
});

test("AC11: exact competing Status remains historical evidence after census loss", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const t = await x.prepared(0);
  const original: Original<Token> = {
    operation: "commit",
    correlation: 21,
    token: t,
  };
  x.commits[0]!.reply.resolve({ kind: "unknown", original });
  await expect(p).rejects.toThrow("unknown");
  const recovery = x.coordinator.reconcile();
  let consumed = 0;
  x.f.consume = (receipt) => {
    expect(receipt.original).toEqual(original);
    expect(receipt.disposition.kind).toBe("competing");
    consumed++;
    return { ready: true };
  };
  x.f.ready = false;
  x.f.selected = false;
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "competing" },
  });
  expect(await recovery).toBe("competing");
  expect(x.coordinator.disposition).toBe("competing");
  expect(await x.coordinator.requestQuit()).toBe("competing");
  expect(consumed).toBe(1);
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "will",
    "commit",
    "status:commit",
  ]);
  await expect(p).rejects.toThrow("unknown");
});

test("AC11: wrong operation/token and retirement cannot manufacture a Status request", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const t = await x.prepared(0);
  x.commits[0]!.reply.resolve({
    kind: "unknown",
    original: { operation: "commit", correlation: 14, token: token() },
  });
  await expect(p).rejects.toThrow("unknown");
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.statuses).toHaveLength(0);
  expect(x.commits[0]!.token).toBe(t);
  x.coordinator.retire(g);
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
});

test("AC8: observed transient Create invalidation suppresses recovered will even with empty mirror", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const original: Original<Token> = { operation: "prepare", correlation: 15 };
  x.prepares[0]!.resolve({ kind: "unknown", original });
  await expect(p).rejects.toThrow("unknown");
  const recovery = x.coordinator.reconcile();
  const t = token();
  x.f.invalidated = t;
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "prepared", token: t },
  });
  await turn();
  expect(x.trace).toEqual(["before", "prepare", "status:prepare", "cancel"]);
  expect(x.cancels[0]!.token).toBe(t);
  x.cancels[0]!.reply.resolve("canceled");
  expect(await recovery).toBe("retained");
  expect(x.commits).toHaveLength(0);
});

test("AC11: late fence-error reporter can retire the original without overwriting Accepted", async () => {
  const x = fixture([w1]);
  const p = x.coordinator.quit();
  x.closes[0]!.dispatched(1n);
  expect(await p).toBeUndefined();
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 1n });
  await turn();
  x.destroy(w1);
  await x.prepared(0);
  const error = Error("accepted fence failed");
  x.f.onLateError = () => {
    x.coordinator.retire(g);
    throw Error("report failed");
  };
  x.commits[0]!.reply.resolve({ kind: "accepted", error });
  expect(await x.coordinator.outcome).toBe("committed");
  expect(x.coordinator.disposition).toBe("committed");
  expect(x.coordinator.custodyFailure?.error).toBe(error);
  expect(await x.coordinator.reconcile()).toBe("committed");
  expect(x.coordinator.quit()).toBe(p);
});

test("AC11: Status loss and generic refusal keep original custody, then exact retained releases it", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const t = await x.prepared(0);
  const original: Original<Token> = {
    operation: "commit",
    correlation: 16,
    token: t,
  };
  const lost = Error("commit reply missing"),
    statusError = Error("status reply missing");
  x.commits[0]!.reply.resolve({ kind: "unknown", original, error: lost });
  await expect(p).rejects.toBe(lost);
  const first = x.coordinator.reconcile();
  x.statuses[0]!.reply.reject(statusError);
  expect(await first).toBe("unknown");
  expect(x.late[0]).toEqual({
    error: statusError,
    custody: { phase: "status", original, generation: g },
  });
  const second = x.coordinator.reconcile();
  x.statuses[1]!.reply.resolve({ original, disposition: { kind: "refused" } });
  expect(await second).toBe("unknown");
  expect(x.coordinator.quit()).toBe(p);
  const third = x.coordinator.reconcile();
  expect(x.statuses[2]!.original).toEqual(original);
  x.statuses[2]!.reply.resolve({ original, disposition: { kind: "retained" } });
  expect(await third).toBe("retained");
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "will",
    "commit",
    "status:commit",
    "status:commit",
    "status:commit",
  ]);
  expect(await x.coordinator.reconcile()).toBe("retained");
  const next = x.coordinator.quit();
  expect(next).not.toBe(p);
  x.prepares[1]!.resolve({ kind: "retained" });
  await expect(next).rejects.toThrow("KELD-COMPAT-012");
});

test("AC11: consume exact recovered Prepared before any will or new effect; blocked release uses no new Status", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const original: Original<Token> = { operation: "prepare", correlation: 17 };
  const error = Error("lost Prepare"),
    capacity = Error("late alias not excluded");
  x.prepares[0]!.resolve({ kind: "unknown", original, error });
  await expect(p).rejects.toBe(error);
  const t = token();
  const prepared = { kind: "prepared" as const, token: t };
  let safe = false,
    consumes = 0;
  x.f.consume = (receipt) => {
    if (receipt.operation === "prepare") {
      consumes++;
      expect(receipt.original).toEqual(original);
      expect(receipt.disposition).toBe(prepared);
      expect(x.coordinator.heldDisposition).toBe(prepared);
      return safe ? { ready: true } : { ready: false, error: capacity };
    }
    return { ready: true };
  };
  const first = x.coordinator.reconcile();
  x.statuses[0]!.reply.resolve({ original, disposition: prepared });
  await turn();
  expect(x.trace).toEqual(["before", "prepare", "status:prepare"]);
  expect(await first).toBe("unknown");
  expect(x.coordinator.heldDisposition).toBe(prepared);
  expect(x.trace).toEqual(["before", "prepare", "status:prepare"]);
  expect(x.coordinator.quit()).toBe(p);
  safe = true;
  const second = x.coordinator.reconcile();
  expect(consumes).toBe(2);
  expect(x.statuses).toHaveLength(1);
  expect(x.trace).toEqual([
    "before",
    "prepare",
    "status:prepare",
    "will",
    "commit",
  ]);
  x.commits[0]!.reply.resolve("accepted");
  expect(await second).toBe("committed");
  await expect(p).rejects.toBe(error);
});

test("AC11: Accepted with completed fence does not wait for consume or reentry", async () => {
  const x = fixture();
  const p = x.coordinator.quit();
  const error = Error("accepted receipt still reserved");
  x.f.consume = (receipt) => {
    if (receipt.operation !== "commit") return { ready: true };
    expect(x.coordinator.disposition).toBe("committed");
    x.coordinator.retire(g);
    expect(x.coordinator.quit()).toBe(p);
    return { ready: false, error };
  };
  await x.prepared(0);
  x.commits[0]!.reply.resolve("accepted");
  expect(await p).toBeUndefined();
  expect(x.late.map((item) => item.error)).toEqual([error]);
  expect(x.coordinator.disposition).toBe("committed");
  expect(await x.coordinator.reconcile()).toBe("committed");
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
});
