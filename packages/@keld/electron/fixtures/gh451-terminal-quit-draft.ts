// GH451 W3 review-only fixture, based on #1581 db74c846 and draft #1432.
// Never imported by the facade or shipped. No protocol allocations, real Host
// calls, or public quit emission. The injected facts/replies stand in for the
// existing authenticated GH531 and lifecycle owners; they mint no authority.
export type WindowIdentity = Readonly<{
  id: number;
  incarnation: bigint;
  document: bigint;
}>;
export type Census = Readonly<{
  generation: bigint;
  ready: boolean;
  selected: boolean;
  // Existing Source snapshot, in creation order. It cannot prove Host zero.
  windows: readonly WindowIdentity[];
}>;
export type CloseResult =
  | { kind: "requested" | "merged"; sequence: bigint }
  | { kind: "refused" | "unknown" };
export type PrepareResult<T> =
  | { kind: "prepared"; token: T }
  | { kind: "retained" | "unknown" };
export type Outcome = "vetoed" | "retained" | "committed" | "unknown" | "retired";

export interface DraftHost<T> {
  census(): Census;
  requestClose(window: WindowIdentity, dispatched: (sequence: bigint) => void): Promise<CloseResult>;
  prepare(): Promise<PrepareResult<T>>;
  cancel(token: T): Promise<"canceled" | "unknown">;
  commit(token: T): Promise<"accepted" | "retained" | "unknown">;
}
export interface DraftListeners {
  beforeQuit(): boolean; // true = synchronous preventDefault
  willQuit(): boolean;
  lastWindowClosed(): void;
}

type Entry = {
  window: WindowIdentity;
  sequence?: bigint;
  // A held merged result can be preceded by the matching Veto receipt.
  earlyVeto: Set<bigint>;
  destroyed: boolean;
  acknowledged: boolean;
};
type Attempt = {
  phase: "before" | "closing" | "prepare" | "will" | "cancel" | "commit";
  entries: Entry[];
  dispatching: boolean;
  vetoed: boolean;
  refused: boolean;
  uncertain: boolean;
  result: Promise<Outcome>;
  resolve: (outcome: Outcome) => void;
};

function same(a: WindowIdentity, b: WindowIdentity): boolean {
  return a.id === b.id && a.incarnation === b.incarnation && a.document === b.document;
}

export class DraftQuitCoordinator<T> {
  private active?: Attempt;
  private stopped?: "committed" | "unknown" | "retired";
  constructor(
    private readonly host: DraftHost<T>,
    private readonly listeners: DraftListeners,
    private readonly generation: bigint,
  ) {}

  requestQuit(): Promise<Outcome> {
    if (this.stopped) return Promise.resolve(this.stopped);
    if (this.active) return this.active.result;
    let resolve!: Attempt["resolve"];
    const result = new Promise<Outcome>(r => { resolve = r; });
    const a: Attempt = {
      phase: "before", entries: [], dispatching: false, vetoed: false,
      refused: false, uncertain: false, result, resolve,
    };
    this.active = a;
    try {
      // Synchronous before-quit, including nested calls.
      if (this.listeners.beforeQuit()) {
        this.finish(a, "vetoed");
        return result;
      }
      if (this.active !== a) return result;
      const census = this.current();
      if (!census) {
        this.finish(a, "retained");
        return result;
      }
      a.phase = "closing";
      a.entries = [...census.windows].reverse().map(window => ({
        window, earlyVeto: new Set(), destroyed: false, acknowledged: false,
      }));
      a.dispatching = true;
      for (const e of a.entries) {
        // Complete the selected sweep despite a synchronous veto. Role
        // retirement, in contrast, revokes further use of that link.
        if (this.active !== a) break;
        try {
          const reply = this.host.requestClose(e.window, seq => {
            if (this.active !== a) return;
            this.bind(a, e, seq);
            this.advance(a);
          });
          void reply.then(r => {
            if (this.active !== a) return;
            if (r.kind === "requested" || r.kind === "merged") {
              this.bind(a, e, r.sequence);
              e.acknowledged = true;
            } else if (r.kind === "refused") {
              a.refused = true;
            } else {
              a.uncertain = true;
            }
            this.advance(a);
          }, () => {
            if (this.active !== a) return;
            a.uncertain = true;
            this.advance(a);
          });
        } catch {
          a.uncertain = true;
        }
      }
      a.dispatching = false;
      this.advance(a);
    } catch {
      // No proof of terminal admission. A throwing before listener has no
      // Host effect; an unexpected failure later is conservatively unknown.
      this.finish(a, a.phase === "before" ? "retained" : "unknown");
    }
    return result;
  }

  observeVetoApplied(generation: bigint, window: WindowIdentity, sequence: bigint): void {
    const a = this.active;
    if (generation !== this.generation || !a || a.phase !== "closing") return;
    const e = a.entries.find(x => same(x.window, window));
    if (!e || e.destroyed) return;
    if (e.sequence === sequence) a.vetoed = true;
    else if (e.sequence === undefined) e.earlyVeto.add(sequence);
    this.advance(a);
  }

  observeDestroyed(generation: bigint, window: WindowIdentity): void {
    const a = this.active;
    if (generation !== this.generation || !a || a.phase !== "closing") return;
    const e = a.entries.find(x => same(x.window, window));
    if (!e) return;
    e.destroyed = true;
    this.advance(a);
  }

  observeLastWindowClosed(generation: bigint): void {
    if (generation !== this.generation || this.stopped) return;
    const a = this.active;
    if (a && a.phase !== "before" && !a.vetoed) return;
    // Stale LastWindowClosed must not notify against a replacement window.
    const census = this.current();
    if (census && census.windows.length === 0) this.listeners.lastWindowClosed();
  }

  retire(generation: bigint): void {
    if (generation !== this.generation || this.stopped) return;
    // Role retirement after a sent Commit is not proof of nonexecution.
    const outcome = this.active?.phase === "commit" ? "unknown" : "retired";
    this.stopped = outcome;
    if (this.active) this.finish(this.active, outcome);
  }

  private current(): Census | undefined {
    const census = this.host.census();
    return census.ready && census.selected && census.generation === this.generation
      ? census : undefined;
  }

  private bind(a: Attempt, e: Entry, sequence: bigint): void {
    if (e.sequence !== undefined && e.sequence !== sequence) {
      a.uncertain = true;
      return;
    }
    e.sequence = sequence;
    if (e.earlyVeto.has(sequence)) a.vetoed = true;
    e.earlyVeto.clear();
  }

  private advance(a: Attempt): void {
    if (this.active !== a || a.phase !== "closing" || a.dispatching) return;
    if (a.uncertain) return this.finish(a, "unknown");
    if (a.vetoed) return this.finish(a, "vetoed");
    if (a.refused) return this.finish(a, "retained");
    // Neither the close RPC nor LastWindowClosed can stand in for Destroyed.
    if (!a.entries.every(e => e.destroyed && e.acknowledged)) return;
    try {
      const census = this.current();
      if (!census || census.windows.length !== 0) return this.finish(a, "retained");
      a.phase = "prepare";
      // Initiate Prepare in the same turn after the local rejection checks.
      // Only Host may prove no Opening/Closing reservation is outstanding.
      void this.host.prepare().then(
        prepared => this.afterPrepare(a, prepared),
        () => this.finish(a, "unknown"),
      );
    } catch {
      this.finish(a, "unknown");
    }
  }

  private afterPrepare(a: Attempt, prepared: PrepareResult<T>): void {
    if (this.active !== a) return;
    if (prepared.kind === "retained") return this.finish(a, "retained");
    if (prepared.kind !== "prepared") return this.finish(a, "unknown");
    try {
      const census = this.current();
      if (!census) return this.finish(a, "unknown");
      // If a replacement is already visible before the callback, give up
      // the reversible token without emitting will-quit.
      if (census.windows.length !== 0) {
        return this.cancel(a, prepared.token, "retained");
      }
      a.phase = "will";
      let veto: boolean;
      try { veto = this.listeners.willQuit(); }
      catch { veto = true; }
      if (this.active !== a) return;
      if (!this.current()) return this.finish(a, "unknown");
      if (veto) {
        return this.cancel(a, prepared.token, "vetoed");
      }
      a.phase = "commit";
      // Even if a Create was admitted and failed during will-quit, only Host
      // can detect irrevocable token invalidation at its existing admission cut.
      void this.host.commit(prepared.token).then(
        r => this.finish(a, r === "accepted" ? "committed" : r === "retained" ? "retained" : "unknown"),
        () => this.finish(a, "unknown"),
      );
    } catch {
      this.finish(a, "unknown");
    }
  }

  private cancel(a: Attempt, token: T, outcome: "retained" | "vetoed"): void {
    a.phase = "cancel";
    void this.host.cancel(token).then(
      r => this.finish(a, r === "canceled" ? outcome : "unknown"),
      () => this.finish(a, "unknown"),
    );
  }

  private finish(a: Attempt, outcome: Outcome): void {
    if (this.active !== a) return;
    this.active = undefined;
    if (outcome === "committed" || outcome === "unknown" || outcome === "retired") {
      this.stopped = outcome;
    }
    // Accepted is commit admission only. This fixture never emits public quit
    // or claims cleanup, process exit, link closure or AppKit success.
    a.resolve(outcome);
  }
}
