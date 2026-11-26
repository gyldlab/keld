// GH451 W3 model-only: scripted facts, not authenticated Host evidence.
import { expect, test } from "bun:test";
import type { Outcome } from "./gh451-terminal-quit-draft";
import { fixture, g, w1, w2, turn } from "./gh451-terminal-quit-draft-fixture";

test("AC2: newest-first requests, physical receipts, then qualified Prepare and conditional Commit", async () => {
  const x = fixture([w1, w2]);
  const a = x.coordinator.requestQuit();
  expect(x.trace).toEqual(["before", "close:2", "close:1"]);
  expect(x.coordinator.requestQuit()).toBe(a);
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 4n });
  x.closes[1]!.reply.resolve({ kind: "requested", sequence: 3n });
  await turn();
  x.last(); // Premature notification is not a Destroyed receipt.
  expect(x.prepares).toHaveLength(0);
  x.destroy(w2);
  expect(x.prepares).toHaveLength(0);
  x.destroy(w1);
  x.last();
  expect(x.trace).toEqual(["before", "close:2", "close:1", "prepare"]);
  const t = await x.prepared(0);
  expect(x.trace).toEqual(["before", "close:2", "close:1", "prepare", "will", "commit"]);
  expect(x.commits[0]!.token).toBe(t);
  expect(x.coordinator.requestQuit()).toBe(a);
  x.commits[0]!.reply.resolve("accepted");
  expect(await a).toBe("committed");
  expect(await x.coordinator.requestQuit()).toBe("committed");
  expect(x.trace).toEqual(["before", "close:2", "close:1", "prepare", "will", "commit"]);
});

test("AC1/7: before veto and nested synchronous call coalesce without Host actions", async () => {
  const x = fixture([w1]);
  let nested: Promise<Outcome> | undefined;
  x.f.before = () => { nested = x.coordinator.requestQuit(); return true; };
  const a = x.coordinator.requestQuit();
  expect(nested).toBe(a);
  expect(x.trace).toEqual(["before"]);
  expect(await a).toBe("vetoed");
  expect(x.trace).toEqual(["before"]);
  expect(await x.coordinator.requestQuit()).toBe("vetoed");
  expect(x.trace).toEqual(["before", "before"]);
});

test("AC3/4: synchronous veto does not stop the sweep; asynchronous final release can start a second Quit", async () => {
  const x = fixture([w1, w2]);
  x.f.onRequest = (w, dispatch) => {
    if (w === w2) {
      dispatch(4n);
      x.coordinator.observeVetoApplied(g, w2, 4n);
    }
  };
  const a = x.coordinator.requestQuit();
  expect(x.trace).toEqual(["before", "close:2", "close:1"]);
  expect(await a).toBe("vetoed");
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 4n });
  x.closes[1]!.reply.resolve({ kind: "requested", sequence: 3n });
  await turn();
  x.destroy(w1);
  let retry: Promise<Outcome> | undefined;
  x.f.last = () => { retry = x.coordinator.requestQuit(); };
  x.destroy(w2);
  x.last();
  expect(x.trace).toEqual(["before", "close:2", "close:1", "last", "before", "prepare"]);
  expect(retry).not.toBe(a);
  await x.prepared(0);
  x.commits[0]!.reply.resolve("accepted");
  expect(await retry).toBe("committed");
});

test("AC3: veto in the last requested window still resets suppression", async () => {
  const x = fixture([w1, w2]);
  x.f.onRequest = (w, dispatch) => {
    dispatch(BigInt(w.id));
    if (w === w1) x.coordinator.observeVetoApplied(g, w, 1n);
  };
  expect(await x.coordinator.requestQuit()).toBe("vetoed");
  expect(x.trace).toEqual(["before", "close:2", "close:1"]);
  x.destroy(w2);
  x.destroy(w1);
  x.last();
  expect(x.trace.at(-1)).toBe("last");
  expect(x.prepares).toHaveLength(0);
});

test("W7 C8 candidate: held merged reply observes matching earlier Veto, never another sequence", async () => {
  const x = fixture([w1, w2]);
  x.f.onRequest = (w, dispatch) => {
    if (w === w2) {
      dispatch(3n);
      x.coordinator.observeVetoApplied(g, w2, 3n);
    } else {
      dispatch(7n);
    }
  };
  const a = x.coordinator.requestQuit();
  expect(await a).toBe("vetoed");
  // W1 is still on the earlier sweep's close sequence; W2 later saves.
  x.destroy(w2);
  x.f.onRequest = () => {};
  const b = x.coordinator.requestQuit();
  x.coordinator.observeVetoApplied(g, w1, 7n);
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 3n });
  x.closes[1]!.reply.resolve({ kind: "requested", sequence: 7n });
  x.closes[2]!.reply.resolve({ kind: "merged", sequence: 7n });
  expect(await b).toBe("vetoed");
  expect(x.trace).toEqual(["before", "close:2", "close:1", "before", "close:1"]);
  expect(x.prepares).toHaveLength(0);
});

test("W7 C8 candidate: held Merged and wrong-sequence Veto cannot authorize or cancel; real Destroyed permits Prepare", async () => {
  const x = fixture([w1, w2]);
  x.f.onRequest = (w, dispatch) => {
    dispatch(w === w2 ? 3n : 7n);
    if (w === w2) x.coordinator.observeVetoApplied(g, w2, 3n);
  };
  expect(await x.coordinator.requestQuit()).toBe("vetoed");
  x.destroy(w2);
  x.f.onRequest = () => {};
  const a = x.coordinator.requestQuit();
  x.coordinator.observeVetoApplied(g, w1, 6n);
  x.destroy(w1);
  expect(x.prepares).toHaveLength(0);
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 3n });
  x.closes[1]!.reply.resolve({ kind: "requested", sequence: 7n });
  await turn();
  expect(x.prepares).toHaveLength(0);
  expect(x.coordinator.requestQuit()).toBe(a);
  x.closes[2]!.reply.resolve({ kind: "merged", sequence: 7n });
  await turn();
  expect(x.prepares).toHaveLength(1);
  x.prepares[0]!.resolve({ kind: "retained" });
  expect(await a).toBe("retained");
});

test("AC8/9: hidden Opening refuses Prepare before any will callback, and later retry is distinct", async () => {
  const x = fixture();
  const a = x.coordinator.requestQuit();
  expect(x.trace).toEqual(["before", "prepare"]);
  x.prepares[0]!.resolve({ kind: "retained" }); // Host reports hidden Opening.
  expect(await a).toBe("retained");
  expect(x.trace).toEqual(["before", "prepare"]);
  const b = x.coordinator.requestQuit();
  expect(b).not.toBe(a);
  await x.prepared(1);
  x.commits[0]!.reply.resolve("accepted");
  expect(await b).toBe("committed");
});

test("AC8: replacement after destruction rejects stale LastWindowClosed and does not prepare", async () => {
  const x = fixture([w1]);
  const a = x.coordinator.requestQuit();
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 1n });
  await turn();
  x.f.windows.push(w2);
  x.destroy(w1);
  x.last();
  expect(x.prepares).toHaveLength(0);
  expect(await a).toBe("retained");
  expect(x.trace).toEqual(["before", "close:1"]);
});

test("AC8: replacement visible while Prepare response is held causes Cancel, not will-quit", async () => {
  const x = fixture();
  const a = x.coordinator.requestQuit();
  x.f.windows.push(w2);
  const t = await x.prepared(0);
  expect(x.trace).toEqual(["before", "prepare", "cancel"]);
  expect(x.cancels[0]!.token).toBe(t);
  x.cancels[0]!.reply.resolve("canceled");
  expect(await a).toBe("retained");
  expect(x.commits).toHaveLength(0);
});

for (const mode of ["veto", "throw"] as const) {
  test("AC5: will-quit " + mode + " cancels exact token and permits a fresh retry after receipt", async () => {
    const x = fixture();
    let nested: Promise<Outcome> | undefined;
    x.f.will = () => {
      nested = x.coordinator.requestQuit();
      if (mode === "throw") throw Error("listener error");
      return true;
    };
    const a = x.coordinator.requestQuit();
    const t = await x.prepared(0);
    expect(nested).toBe(a);
    expect(x.trace).toEqual(["before", "prepare", "will", "cancel"]);
    expect(x.cancels[0]!.token).toBe(t);
    expect(x.coordinator.requestQuit()).toBe(a);
    x.cancels[0]!.reply.resolve("canceled");
    expect(await a).toBe("vetoed");
    x.f.will = () => false;
    const b = x.coordinator.requestQuit();
    const t2 = await x.prepared(1);
    expect(t2).not.toBe(t);
    expect(x.commits[0]!.token).toBe(t2);
    x.commits[0]!.reply.resolve("accepted");
    expect(await b).toBe("committed");
    expect(x.trace).toEqual(["before", "prepare", "will", "cancel", "before", "prepare", "will", "commit"]);
  });
}

test("W7 C4c: transient Create during will-quit is Host-retained even if Source is zero again", async () => {
  const x = fixture();
  const a = x.coordinator.requestQuit();
  x.f.will = () => { x.f.windows.push(w2); x.f.windows.pop(); return false; };
  const oldToken = await x.prepared(0);
  expect(x.f.windows).toHaveLength(0);
  expect(x.commits[0]!.token).toBe(oldToken);
  x.commits[0]!.reply.resolve("retained"); // Explicit Host token invalidation.
  expect(await a).toBe("retained");
  const b = x.coordinator.requestQuit();
  const newToken = await x.prepared(1);
  expect(newToken).not.toBe(oldToken);
  expect(x.commits[1]!.token).toBe(newToken);
  x.commits[1]!.reply.resolve("accepted");
  expect(await b).toBe("committed");
});

for (const phase of ["prepare", "cancel", "commit"] as const) {
  test("W7 C19: lost " + phase + " acknowledgement blocks fresh attempts and never retries", async () => {
    const x = fixture();
    x.f.will = () => phase === "cancel";
    const a = x.coordinator.requestQuit();
    if (phase === "prepare") x.prepares[0]!.reject(Error("lost Prepare ACK"));
    else {
      await x.prepared(0);
      if (phase === "cancel") x.cancels[0]!.reply.reject(Error("lost Cancel ACK"));
      else x.commits[0]!.reply.reject(Error("lost Commit ACK"));
    }
    expect(await a).toBe("unknown");
    const trace = [...x.trace];
    const retry = x.coordinator.requestQuit();
    expect(x.trace).toEqual(trace);
    expect(await retry).toBe("unknown");
    expect(x.trace).toEqual(trace);
  });
}

test("C9: stale role or same WindowRef with different document cannot satisfy physical receipt", async () => {
  const x = fixture([w1]);
  const a = x.coordinator.requestQuit();
  x.closes[0]!.reply.resolve({ kind: "requested", sequence: 1n });
  await turn();
  x.f.windows = []; // Deliberately misleading mirror.
  x.coordinator.observeDestroyed(g + 1n, w1);
  x.coordinator.observeDestroyed(g, { ...w1, document: 999n });
  expect(x.prepares).toHaveLength(0);
  x.coordinator.observeDestroyed(g, w1);
  expect(x.prepares).toHaveLength(1);
  x.coordinator.retire(g);
  // A sent Prepare with no reply is not proof of nonexecution.
  expect(await a).toBe("unknown");
  await x.prepared(0);
  expect(x.trace).toEqual(["before", "close:1", "prepare"]);
});

test("C19: role retirement after sent Commit leaves effect unknown even if a late reply arrives", async () => {
  const x = fixture();
  const a = x.coordinator.requestQuit();
  await x.prepared(0);
  x.coordinator.retire(g);
  expect(await a).toBe("unknown");
  x.commits[0]!.reply.resolve("accepted");
  await turn();
  expect(await x.coordinator.requestQuit()).toBe("unknown");
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
});

test("ambiguous close sequence is unknown and cannot prepare", async () => {
  const x = fixture([w1]);
  const a = x.coordinator.requestQuit();
  x.closes[0]!.dispatched(3n);
  x.closes[0]!.reply.resolve({ kind: "merged", sequence: 4n });
  expect(await a).toBe("unknown");
  x.destroy(w1);
  expect(x.prepares).toHaveLength(0);
});

test("unselected or unready role cannot request close or prepare", async () => {
  const x = fixture([w1]);
  x.f.selected = false;
  expect(await x.coordinator.requestQuit()).toBe("retained");
  x.f.selected = true;
  x.f.ready = false;
  expect(await x.coordinator.requestQuit()).toBe("retained");
  expect(x.trace).toEqual(["before", "before"]);
});
