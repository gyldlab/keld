import { expect, test } from "bun:test";
import { RetainedLinkDraft, type Request, type RetainedPort } from "./candidate.ts";

// A scripted, independent observation of writes, correlations and local
// fencing. It deliberately does not emulate Core, a codec, a socket or OS.
class Port implements RetainedPort {
  negotiated = true;
  selected = true;
  current = true;
  live = true;
  fenceCount = 0;
  correlation = 40;
  readonly sent: Array<{ request: Request; correlation: number }> = [];
  readonly script: Array<() => Promise<unknown>> = [];
  isCurrent() { return this.current; }
  isLive() { return this.live; }
  start(request: Request) {
    const correlation = ++this.correlation;
    this.sent.push({ request, correlation });
    const next = this.script.shift();
    if (!next) throw new Error("unexpected effect or Status");
    return { correlation, reply: next() };
  }
  fenceAccepted() { this.fenceCount++; this.live = false; }
  reply(value: unknown) { this.script.push(() => Promise.resolve(value)); }
  lose() { this.script.push(() => Promise.reject(new Error("lost ACK"))); }
  recorded(disposition: unknown, original: unknown) {
    this.reply({ kind: "recorded", original, disposition });
  }
  ordinaryProbe() {
    if (!this.live || !this.current) throw new Error("same link closed");
    return "same port";
  }
  ops() { return this.sent.map(x => x.request.operation); }
}

test("old and unselected peers send zero conditional or fallback operations", async () => {
  for (const gate of ["negotiated", "selected"] as const) {
    const port = new Port();
    port[gate] = false;
    const api = new RetainedLinkDraft(port);
    port[gate] = true; // captured eligibility cannot become true after construction
    expect(await api.prepare()).toEqual({ kind: "refused", reason: "unsupported" });
    expect(await api.cancel()).toMatchObject({ kind: "refused" });
    expect(await api.commit()).toMatchObject({ kind: "refused" });
    expect(await api.reconcile()).toMatchObject({ kind: "refused" });
    expect(port.ops()).toEqual([]);
    expect(port.ordinaryProbe()).toBe("same port");
  }
});

test("Prepare Retained keeps same port and permits new preparation", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "retained" });
  expect(await api.prepare()).toEqual({ kind: "retained" });
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
  port.reply({ kind: "cancelled" });
  expect(await api.cancel()).toEqual({ kind: "cancelled" });
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-2" });
  await api.prepare();
  port.reply({ kind: "retained" });
  expect(await api.commit()).toEqual({ kind: "retained" });
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-3" });
  expect(await api.prepare()).toEqual({ kind: "prepared", token: "host-3" });
  expect(port.sent.map(x => x.request)).toEqual([
    { operation: "prepare" }, { operation: "cancel", token: "host-1" },
    { operation: "prepare" }, { operation: "commit", token: "host-2" },
    { operation: "prepare" },
  ]);
  expect(port.fenceCount).toBe(0);
});

test("only known Commit Accepted fences once, after the correlated reply", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "host-1" });
  await api.prepare();
  let resolve!: (value: unknown) => void;
  port.script.push(() => new Promise(r => { resolve = r; }));
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
  for (const failure of ["lost", "malformed", "wrong-operation", "generic-error"] as const) {
    test(effect + " " + failure + " preserves original identity and blocks replay", async () => {
      const port = new Port();
      const api = new RetainedLinkDraft(port);
      if (effect !== "prepare") {
        port.reply({ kind: "prepared", token: "host-1" });
        await api.prepare();
      }
      if (failure === "lost") port.lose();
      else if (failure === "malformed") port.reply(null);
      else if (failure === "wrong-operation") port.reply({ kind: effect === "commit" ? "cancelled" : "accepted" });
      else port.reply({ kind: "error", code: "generic" });
      const original = { operation: effect, correlation: effect === "prepare" ? 41 : 42,
        token: effect === "prepare" ? undefined : "host-1" };
      expect(await api[effect]()).toEqual({ kind: "unknown", original });
      const prior = port.ops();
      expect(await api.prepare()).toEqual({ kind: "unknown", original });
      expect(await api.cancel()).toEqual({ kind: "unknown", original });
      expect(await api.commit()).toEqual({ kind: "unknown", original });
      expect(port.ops()).toEqual(prior);
      expect(port.fenceCount).toBe(0);
      const resolved = effect === "prepare"
        ? { kind: "prepared", token: "host-recovered" }
        : effect === "cancel" ? { kind: "cancelled" } : { kind: "accepted" };
      port.recorded(resolved, original);
      expect(await api.reconcile()).toEqual(resolved);
      expect(port.sent.at(-1)).toEqual({ request: { operation: "status", original },
        correlation: original.correlation + 1 });
      expect(port.ops()).toEqual([...prior, "status"]);
      expect(port.fenceCount).toBe(effect === "commit" ? 1 : 0);
      if (effect === "prepare") {
        port.reply({ kind: "cancelled" });
        await api.cancel();
        expect(port.sent.at(-1)?.request).toEqual({ operation: "cancel", token: "host-recovered" });
      }
      if (effect === "cancel") {
        expect(port.ordinaryProbe()).toBe("same port");
        port.reply({ kind: "retained" });
        expect(await api.prepare()).toEqual({ kind: "retained" });
      }
    });
  }
}

test("Status mismatch, indeterminate, lost and wrong-operation results remain unknown", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.reply({ kind: "prepared", token: "host-1" });
  await api.prepare();
  port.lose();
  const original = { operation: "commit", correlation: 42, token: "host-1" };
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
  port.recorded({ kind: "cancelled" }, original);
  expect(await api.reconcile()).toEqual({ kind: "unknown", original });
  expect(port.fenceCount).toBe(0);
  port.recorded({ kind: "retained" }, original);
  expect(await api.reconcile()).toEqual({ kind: "retained" });
  expect(port.ordinaryProbe()).toBe("same port");
  port.reply({ kind: "prepared", token: "host-2" });
  expect(await api.prepare()).toEqual({ kind: "prepared", token: "host-2" });
  expect(port.ops()).toEqual(["prepare", "commit", ...Array(7).fill("status"), "prepare"]);
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
  const original = { operation: "prepare", correlation: 41, token: undefined };
  let resolve!: (value: unknown) => void;
  port.script.push(() => new Promise(r => { resolve = r; }));
  const pending = api.reconcile();
  expect(await api.reconcile()).toEqual({ kind: "refused", reason: "busy" });
  expect(await api.prepare()).toEqual({ kind: "refused", reason: "busy" });
  resolve({ kind: "recorded", original, disposition: { kind: "retained" } });
  expect(await pending).toEqual({ kind: "retained" });
  expect(port.ops()).toEqual(["prepare", "status"]);
});

test("unidentified send failure blocks effect and Status instead of guessing correlation", async () => {
  const port = new Port(); // no scripted receipt
  const api = new RetainedLinkDraft(port);
  expect(await api.prepare()).toEqual({ kind: "unknown", original: undefined });
  expect(await api.reconcile()).toEqual({ kind: "unknown", original: undefined });
  expect(await api.prepare()).toEqual({ kind: "unknown", original: undefined });
  expect(port.ops()).toEqual(["prepare"]);
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
    const original = { operation: "prepare", correlation: 41, token: undefined };
    if (status) {
      port.lose();
      await api.prepare();
    }
    let resolve!: (value: unknown) => void;
    port.script.push(() => new Promise(r => { resolve = r; }));
    const pending = status ? api.reconcile() : api.prepare();
    port.current = false;
    const prepared = { kind: "prepared", token: "retired-host" };
    resolve(status ? { kind: "recorded", original, disposition: prepared } : prepared);
    expect(await pending).toEqual({ kind: "unknown", original });
    expect(await api.commit()).toEqual({ kind: "unknown", original });
    expect(port.fenceCount).toBe(0);
    expect(port.ops()).toEqual(status ? ["prepare", "status"] : ["prepare"]);
  }
});

test("Status may not reuse unresolved effect correlation", async () => {
  const port = new Port();
  const api = new RetainedLinkDraft(port);
  port.lose();
  const unknown = await api.prepare();
  port.correlation = 40; // next allocator result aliases the original
  port.recorded({ kind: "retained" }, { operation: "prepare", correlation: 41 });
  expect(await api.reconcile()).toEqual(unknown);
  expect(await api.prepare()).toEqual(unknown);
  expect(port.fenceCount).toBe(0);
});
