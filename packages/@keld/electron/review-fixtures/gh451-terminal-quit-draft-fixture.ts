// Shared deterministic scripted port for W3 review-only tests.
// Owned by the Electron package's review-only test suite.
import {
  DraftQuitCoordinator,
  type CloseResult,
  type CancelResult,
  type CommitResult,
  type ConsumedDisposition,
  type Consumption,
  type DraftHost,
  type ErrorCustody,
  type Original,
  type PrepareResult,
  type StatusResult,
  type WindowIdentity,
} from "./gh451-terminal-quit-draft";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}
export type Token = Readonly<{ opaque: symbol }>;
export const g = 1n;
export const w1 = { id: 1, incarnation: 11n, document: 101n };
export const w2 = { id: 2, incarnation: 22n, document: 202n };
export const token = (): Token => Object.freeze({ opaque: Symbol() });
export const turn = async () => {
  await Promise.resolve();
};

export function fixture(
  initial: readonly WindowIdentity[] = [],
  negotiated = true,
) {
  const trace: string[] = [];
  const f = {
    windows: [...initial],
    ready: true,
    selected: true,
    generation: g,
    before: () => false as boolean,
    will: () => false as boolean,
    last: () => {},
    onRequest: (_w: WindowIdentity, _dispatch: (s: bigint) => void) => {},
    invalidated: undefined as Token | undefined,
    onLateError: () => {},
    consume: (_receipt: ConsumedDisposition<Token>): Consumption => ({
      ready: true,
    }),
  };
  const closes: Array<{
    w: WindowIdentity;
    dispatched: (seq: bigint) => void;
    reply: ReturnType<typeof deferred<CloseResult>>;
  }> = [];
  const prepares: Array<ReturnType<typeof deferred<PrepareResult<Token>>>> = [];
  const cancels: Array<{
    token: Token;
    reply: ReturnType<typeof deferred<CancelResult<Token>>>;
  }> = [];
  const commits: Array<{
    token: Token;
    reply: ReturnType<typeof deferred<CommitResult<Token>>>;
  }> = [];
  const statuses: Array<{
    original: Original<Token>;
    reply: ReturnType<typeof deferred<StatusResult<Token>>>;
  }> = [];
  const late: Array<{ error: unknown; custody: ErrorCustody<Token> }> = [];
  const host: DraftHost<Token> = {
    conditionalEligible: negotiated,
    census: () => ({
      generation: f.generation,
      ready: f.ready,
      selected: f.selected,
      windows: f.windows,
    }),
    requestClose(w, dispatched) {
      trace.push("close:" + w.id);
      const reply = deferred<CloseResult>();
      closes.push({ w, dispatched, reply });
      f.onRequest(w, dispatched);
      return reply.promise;
    },
    prepare() {
      trace.push("prepare");
      const reply = deferred<PrepareResult<Token>>();
      prepares.push(reply);
      return reply.promise;
    },
    cancel(t) {
      trace.push("cancel");
      const reply = deferred<CancelResult<Token>>();
      cancels.push({ token: t, reply });
      return reply.promise;
    },
    commit(t) {
      trace.push("commit");
      const reply = deferred<CommitResult<Token>>();
      commits.push({ token: t, reply });
      return reply.promise;
    },
    status(original) {
      trace.push("status:" + original.operation);
      const reply = deferred<StatusResult<Token>>();
      statuses.push({ original, reply });
      return reply.promise;
    },
    observedInvalidation(t) {
      return f.invalidated === t;
    },
    consumeDisposition(receipt) {
      return f.consume(receipt);
    },
    reportLateError(error, custody) {
      late.push({ error, custody });
      f.onLateError();
    },
  };
  const coordinator = new DraftQuitCoordinator(
    host,
    {
      beforeQuit() {
        trace.push("before");
        return f.before();
      },
      willQuit() {
        trace.push("will");
        return f.will();
      },
      lastWindowClosed() {
        trace.push("last");
        f.last();
      },
    },
    g,
  );
  return {
    f,
    host,
    trace,
    closes,
    prepares,
    cancels,
    commits,
    statuses,
    late,
    coordinator,
    destroy(w: WindowIdentity, generation = g) {
      f.windows = f.windows.filter((x) => x !== w);
      coordinator.observeDestroyed(generation, w);
    },
    last() {
      coordinator.observeLastWindowClosed(g);
    },
    async prepared(index: number, t = token()) {
      prepares[index]!.resolve({ kind: "prepared", token: t });
      await turn();
      return t;
    },
  };
}
