// GH451 review-only composition of the two isolated TypeScript candidates.
// The scripted port and close facts are fixtures, not authenticated IPC or OS evidence.
import { expect, test } from "bun:test";
import {
  RetainedLinkDraft,
  type Request,
  type RetainedPort,
  type Token,
} from "../../api/fixtures/gh451-retained-link-draft/candidate";
import {
  DraftQuitCoordinator,
  type DraftHost,
  type WindowIdentity,
} from "./gh451-terminal-quit-draft";

const generation = 1n;
const window = { id: 1, incarnation: 11n, document: 101n };

function fixture(initial: readonly WindowIdentity[] = [], negotiated = true) {
  const trace: string[] = [];
  const sent: Request[] = [];
  const replies: Array<() => Promise<unknown>> = [];
  let correlation = 0;
  let live = true;
  let fenceCount = 0;
  let token: Token | undefined;
  let windows = [...initial];
  let veto = false;
  const port: RetainedPort = {
    negotiated,
    selected: true,
    isCurrent: () => true,
    isLive: () => live,
    start(request) {
      trace.push(request.operation);
      sent.push(request);
      const reply = replies.shift();
      if (!reply) throw new Error("unexpected draft request");
      return { correlation: ++correlation, reply: reply() };
    },
    fenceAccepted() {
      trace.push("fence");
      fenceCount++;
      live = false;
    },
  };
  const api = new RetainedLinkDraft(port);
  const host: DraftHost<Token> = {
    census: () => ({ generation, ready: true, selected: true, windows }),
    requestClose(w, dispatch) {
      trace.push("close:" + w.id);
      dispatch(7n);
      return Promise.resolve({ kind: "requested", sequence: 7n });
    },
    async prepare() {
      const result = await api.prepare();
      if (result.kind === "prepared") {
        token = result.token;
        return result;
      }
      return result.kind === "retained" ? result : { kind: "unknown" };
    },
    async cancel(t) {
      // Fixture adapter only: the approved API must bind the exact token itself.
      if (t !== token || token === undefined) return "unknown";
      const result = await api.cancel();
      if (result.kind !== "cancelled") return "unknown";
      token = undefined;
      return "canceled";
    },
    async commit(t) {
      if (t !== token || token === undefined) return "unknown";
      const result = await api.commit();
      if (result.kind !== "accepted" && result.kind !== "retained") return "unknown";
      token = undefined;
      return result.kind;
    },
  };
  const coordinator = new DraftQuitCoordinator(host, {
    beforeQuit() { trace.push("before"); return false; },
    willQuit() { trace.push("will"); return veto; },
    lastWindowClosed() { trace.push("last"); },
  }, generation);
  return {
    api, coordinator, trace, sent,
    reply(value: unknown) { replies.push(() => Promise.resolve(value)); },
    lose() { replies.push(() => Promise.reject(new Error("missing terminal receipt"))); },
    veto(value: boolean) { veto = value; },
    destroy() {
      windows = [];
      coordinator.observeDestroyed(generation, window);
    },
    get live() { return live; },
    get fences() { return fenceCount; },
  };
}

test("composed: close acknowledgement alone never prepares; actual fact precedes accepted fence", async () => {
  const f = fixture([window]);
  f.reply({ kind: "prepared", token: "first" });
  f.reply({ kind: "accepted" });
  const attempt = f.coordinator.requestQuit();
  await Promise.resolve();
  expect(f.trace).toEqual(["before", "close:1"]);
  expect(f.sent).toEqual([]);
  f.destroy();
  expect(await attempt).toBe("committed");
  expect(f.trace).toEqual(["before", "close:1", "prepare", "will", "commit", "fence"]);
  expect(f.sent).toEqual([{ operation: "prepare" }, { operation: "commit", token: "first" }]);
  expect(f.fences).toBe(1);
});

test("composed: will veto cancels the exact token and retains the original link for retry", async () => {
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
    { operation: "prepare" }, { operation: "cancel", token: "old" },
    { operation: "prepare" }, { operation: "commit", token: "new" },
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
    { operation: "prepare" }, { operation: "commit", token: "retired" },
    { operation: "prepare" }, { operation: "commit", token: "fresh" },
  ]);
  expect(f.fences).toBe(1);
});

test("composed: missing Commit receipt stops facade replay without fencing the link", async () => {
  const f = fixture();
  f.reply({ kind: "prepared", token: "unknown" });
  f.lose();
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  expect(await f.api.commit()).toMatchObject({ kind: "unknown" });
  expect(f.sent).toEqual([{ operation: "prepare" }, { operation: "commit", token: "unknown" }]);
  expect(f.fences).toBe(0);
  expect(f.live).toBe(true);
});

test("composed: unnegotiated peer never receives an effect or fallback", async () => {
  const f = fixture([], false);
  expect(await f.coordinator.requestQuit()).toBe("unknown");
  expect(f.trace).toEqual(["before"]);
  expect(f.sent).toEqual([]);
  expect(f.fences).toBe(0);
});
