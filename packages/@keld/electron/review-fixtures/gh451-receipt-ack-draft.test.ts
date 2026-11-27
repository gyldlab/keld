// Review-only facade receipt guards, not authenticated transport evidence.
import { expect, test } from "bun:test";
import { fixture, g, token, turn } from "./gh451-terminal-quit-draft-fixture";

test("AC11: missing Prepare consumption holds authorization until explicit release", async () => {
  const x = fixture();
  const consume = x.host.consumeDisposition!;
  delete x.host.consumeDisposition;
  const publicQuit = x.coordinator.quit();
  const t = token();
  const prepared = { kind: "prepared" as const, token: t };
  x.prepares[0]!.resolve(prepared);
  await turn();
  expect(x.trace).toEqual(["before", "prepare"]);
  expect(x.coordinator.heldDisposition).toBe(prepared);
  await expect(publicQuit).rejects.toThrow("missing receipt acknowledgement");
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.trace).toEqual(["before", "prepare"]);
  x.host.consumeDisposition = consume;
  const recovery = x.coordinator.reconcile();
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
  x.commits[0]!.reply.resolve("accepted");
  expect(await recovery).toBe("committed");
});

test("AC11: missing Retained consumption blocks a fresh conditional attempt", async () => {
  const x = fixture();
  const consume = x.host.consumeDisposition!;
  delete x.host.consumeDisposition;
  const publicQuit = x.coordinator.quit();
  const retained = { kind: "retained" as const };
  x.prepares[0]!.resolve(retained);
  await turn();
  expect(x.coordinator.heldDisposition).toBe(retained);
  await expect(publicQuit).rejects.toThrow("missing receipt acknowledgement");
  expect(x.coordinator.quit()).toBe(publicQuit);
  expect(x.prepares).toHaveLength(1);
  x.host.consumeDisposition = consume;
  expect(await x.coordinator.reconcile()).toBe("retained");
  const retry = x.coordinator.quit();
  expect(retry).not.toBe(publicQuit);
  expect(x.prepares).toHaveLength(2);
  x.prepares[1]!.resolve({ kind: "retained" });
  await expect(retry).rejects.toThrow("KELD-COMPAT-012");
});

test("AC11: known typed Retained rejects exactly once while release remains pending", async () => {
  const x = fixture();
  const error = new Error("exact typed Retained");
  let ready = false;
  x.f.consume = () =>
    ready ? { ready: true } : { ready: false, pending: true };
  const publicQuit = x.coordinator.quit();
  x.prepares[0]!.resolve({ kind: "retained", error });
  await expect(publicQuit).rejects.toBe(error);
  expect(x.coordinator.heldDisposition?.kind).toBe("retained");
  for (let i = 0; i < 2; i++) {
    expect(await x.coordinator.reconcile()).toBe("unknown");
    expect(x.coordinator.quit()).toBe(publicQuit);
    expect(x.late).toEqual([]);
  }
  ready = true;
  expect(await x.coordinator.reconcile()).toBe("retained");
  expect(x.late).toEqual([]);
  expect(x.prepares).toHaveLength(1);
});

test("AC11: unselected role cannot reconcile a held Retained receipt or wait on Quit", async () => {
  const x = fixture();
  const error = new Error("exact typed Retained");
  const retained = { kind: "retained" as const, error };
  let consumes = 0;
  x.f.consume = () => {
    consumes++;
    return { ready: false, pending: true };
  };
  const publicQuit = x.coordinator.quit();
  x.prepares[0]!.resolve(retained);
  await expect(publicQuit).rejects.toBe(error);
  expect(x.coordinator.heldDisposition).toBe(retained);
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(consumes).toBe(2); // selected role can try the local check

  x.f.selected = false;
  const reconciliation = x.coordinator.reconcile();
  expect(await Promise.race([reconciliation, Promise.resolve("pending")])).toBe(
    "unknown",
  );
  expect(x.coordinator.heldDisposition).toBe(retained);
  expect(x.coordinator.quit()).toBe(publicQuit);
  await expect(publicQuit).rejects.toBe(error);
  expect(consumes).toBe(2);
  expect(x.statuses).toHaveLength(0);
  expect(x.trace).toEqual(["before", "prepare"]);
});

test("AC11: recovered typed noncommit reports once independently of receipt release", async () => {
  const x = fixture();
  const original = { operation: "prepare" as const, correlation: 7 };
  const lost = new Error("lost Prepare");
  const error = new Error("exact Invalidated");
  const publicQuit = x.coordinator.quit();
  x.prepares[0]!.resolve({ kind: "unknown", original, error: lost });
  await expect(publicQuit).rejects.toBe(lost);
  let ready = false;
  x.f.consume = () =>
    ready ? { ready: true } : { ready: false, pending: true };
  const recovery = x.coordinator.reconcile();
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "invalidated", error },
  });
  expect(await recovery).toBe("unknown");
  const reports = [
    { error, custody: { phase: "prepare" as const, original, generation: 1n } },
  ];
  expect(x.late).toEqual(reports);
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.late).toEqual(reports);
  ready = true;
  expect(await x.coordinator.reconcile()).toBe("retained");
  expect(x.late).toEqual(reports);
  await expect(publicQuit).rejects.toBe(lost);
  expect(x.statuses).toHaveLength(1);
  expect(x.prepares).toHaveLength(1);
});

test("AC11: late typed refusal preserves the actual cause after retirement", async () => {
  const x = fixture();
  const publicQuit = x.coordinator.quit();
  x.coordinator.retire(1n);
  await expect(publicQuit).rejects.toThrow("original role retired");
  const error = new Error("exact typed refusal");
  x.prepares[0]!.resolve({ kind: "refused", error });
  await turn();
  expect(x.late).toEqual([
    {
      error,
      custody: { phase: "prepare", original: undefined, generation: 1n },
    },
  ]);
  expect(x.coordinator.quit()).toBe(publicQuit);
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.trace).toEqual(["before", "prepare"]);
});

test("AC11: known terminal acceptance does not require consumption callback", async () => {
  const x = fixture();
  const publicQuit = x.coordinator.quit();
  await x.prepared(0);
  delete x.host.consumeDisposition;
  x.commits[0]!.reply.resolve("accepted");
  expect(await publicQuit).toBeUndefined();
  expect(x.coordinator.disposition).toBe("committed");
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
  expect(x.late).toEqual([]);
});

test("AC11: a throwing terminal acknowledgement accessor cannot strand settlement", async () => {
  const x = fixture();
  const publicQuit = x.coordinator.quit();
  await x.prepared(0);
  const error = Error("terminal acknowledgement lookup failed");
  let lookups = 0;
  Object.defineProperty(x.host, "consumeDisposition", {
    get() {
      lookups++;
      throw error;
    },
  });
  x.commits[0]!.reply.resolve("accepted");
  await turn();
  expect(x.coordinator.disposition).toBe("committed");
  expect(x.late.map((item) => item.error)).toEqual([error]);
  expect(lookups).toBe(1);
  expect(await publicQuit).toBeUndefined();
  expect(x.coordinator.quit()).toBe(publicQuit);
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
});

test("AC11: terminal acknowledgement captures one callback with its original receiver", async () => {
  const x = fixture();
  const publicQuit = x.coordinator.quit();
  const t = await x.prepared(0);
  const disposition = { kind: "accepted" as const };
  let lookups = 0;
  let calls = 0;
  let receiver: unknown;
  let receipt: unknown;
  Object.defineProperty(x.host, "consumeDisposition", {
    get() {
      if (++lookups !== 1) return undefined;
      return function (
        this: unknown,
        delivery: Parameters<NonNullable<typeof x.host.consumeDisposition>>[0],
      ) {
        calls++;
        receiver = this;
        receipt = delivery;
        return { ready: true };
      };
    },
  });
  x.commits[0]!.reply.resolve(disposition);
  expect(await publicQuit).toBeUndefined();
  expect(x.coordinator.disposition).toBe("committed");
  expect(lookups).toBe(1);
  expect(calls).toBe(1);
  expect(receiver).toBe(x.host);
  expect(receipt).toEqual({
    operation: "commit",
    token: t,
    original: undefined,
    disposition,
  });
  expect(x.late).toEqual([]);
});

for (const operation of ["cancel", "commit"] as const) {
  test(`AC11: wrong-operation Invalidated on ${operation} never settles or consumes`, async () => {
    const x = fixture();
    x.f.will = () => operation === "cancel";
    const consumed: string[] = [];
    x.f.consume = (receipt) => {
      consumed.push(receipt.operation);
      return { ready: true };
    };
    const first = x.coordinator.quit();
    await x.prepared(0);
    const queued = operation === "cancel" ? x.coordinator.quit() : undefined;
    const replies = operation === "cancel" ? x.cancels : x.commits;
    replies[0]!.reply.resolve({ kind: "invalidated" });
    await turn();

    expect(x.coordinator.disposition).toBe("unknown");
    expect(x.coordinator.heldDisposition).toBeUndefined();
    expect(consumed).toEqual(["prepare"]);
    expect(x.coordinator.quit()).toBe(first);
    expect(x.prepares).toHaveLength(1);
    if (queued) {
      await expect(first).resolves.toBeUndefined();
      await expect(queued).rejects.toThrow("unknown");
    } else {
      await expect(first).rejects.toThrow("unknown");
    }
  });
}

test("AC11: Invalidated is not a direct Prepare response", async () => {
  const x = fixture();
  let consumed = 0;
  x.f.consume = () => {
    consumed++;
    return { ready: true };
  };
  const quit = x.coordinator.quit();
  x.prepares[0]!.resolve({ kind: "invalidated" });
  await turn();
  expect(x.coordinator.disposition).toBe("unknown");
  expect(x.coordinator.heldDisposition).toBeUndefined();
  expect(consumed).toBe(0);
  expect(x.coordinator.quit()).toBe(quit);
  expect(x.trace).toEqual(["before", "prepare"]);
  await expect(quit).rejects.toThrow("unknown");
});

test("AC11: unselected direct refusal preserves the pending public error", async () => {
  const x = fixture();
  const error = new Error("typed Prepare refusal");
  const quit = x.coordinator.quit();
  x.f.selected = false;
  x.prepares[0]!.resolve({ kind: "refused", error });

  await expect(quit).rejects.toBe(error);
  expect(await x.coordinator.outcome).toBe("unknown");
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.coordinator.quit()).toBe(quit);
  expect(x.trace).toEqual(["before", "prepare"]);
  expect(x.late).toEqual([]);
});

test("AC11: unselected exact Status preserves the late error with its original identity", async () => {
  const x = fixture();
  const original = { operation: "prepare" as const, correlation: 22 };
  const lost = new Error("lost Prepare response");
  const error = new Error("typed Invalidated status");
  const quit = x.coordinator.quit();
  x.prepares[0]!.resolve({ kind: "unknown", original, error: lost });
  await expect(quit).rejects.toBe(lost);
  const recovery = x.coordinator.reconcile();

  x.f.selected = false;
  x.statuses[0]!.reply.resolve({
    original,
    disposition: { kind: "invalidated", error },
  });
  expect(await recovery).toBe("unknown");
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.coordinator.quit()).toBe(quit);
  await expect(quit).rejects.toBe(lost);
  expect(x.trace).toEqual(["before", "prepare", "status:prepare"]);
  expect(x.late).toHaveLength(1);
  expect(x.late[0]!.error).toBe(error);
  expect(x.late[0]!.custody).toEqual({
    phase: "status",
    original,
    generation: g,
  });
});
