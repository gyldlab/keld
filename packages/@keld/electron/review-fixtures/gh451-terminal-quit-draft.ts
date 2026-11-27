// GH451 W3 nonshipping review fixture. No wire allocation or real Host authority.
// Owned by the Electron package's review-only test suite.
// The injected GH531 receipt and lifecycle owners are the only fact sources.
import type {
  WindowIdentity,
  Census,
  Effect,
  Original,
  PrepareResult,
  Disposition,
  CancelResult,
  CommitResult,
  Outcome,
  ErrorCustody,
  DraftHost,
  DraftListeners,
  Consumption,
} from "./gh451-terminal-quit-draft-contract";
export type * from "./gh451-terminal-quit-draft-contract";

type ReceiptConsumer<T> = NonNullable<DraftHost<T>["consumeDisposition"]>;

type Handle = {
  public: Promise<void>;
  resolve: () => void;
  reject: (error: unknown) => void;
  settled: boolean;
  outcome: Promise<Outcome>;
  observe: (value: Outcome) => void;
};
function handle(): Handle {
  let resolve!: Handle["resolve"],
    reject!: Handle["reject"],
    observe!: Handle["observe"];
  const publicPromise = new Promise<void>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  // Internal fixture observers may use only outcome; never change rejection
  // for an actual caller of quit().
  void publicPromise.catch(() => {});
  const outcome = new Promise<Outcome>((ok) => {
    observe = ok;
  });
  return {
    public: publicPromise,
    resolve,
    reject,
    settled: false,
    outcome,
    observe,
  };
}
type Entry = {
  window: WindowIdentity;
  sequence?: bigint;
  // Bounded projection of one undelivered exact GH531 receipt. A conflicting
  // second receipt before binding is ambiguous, never guessed or accumulated.
  earlyVeto?: bigint;
  destroyed: boolean;
  acknowledged: boolean;
};
type Pending<T> = {
  operation: Effect;
  token?: T;
  original?: Original<T>;
  held?: Disposition<T>;
  awaitingRelease?: boolean;
  // A known noncommit error is independent of private receipt reclamation.
  noncommitDelivered?: boolean;
};
type Recovery = {
  promise: Promise<Outcome>;
  resolve: (value: Outcome) => void;
};
type Attempt<T> = {
  h: Handle;
  phase: "before" | "closing" | "will" | "unknown" | Effect;
  entries: Entry[];
  dispatching: boolean;
  vetoed: boolean;
  willVeto: boolean;
  refused: boolean;
  uncertain: boolean;
  pending?: Pending<T>;
  recovery?: Recovery;
  queued?: Handle;
  cancelOutcome: "vetoed" | "retained";
};
function same(a: WindowIdentity, b: WindowIdentity): boolean {
  return (
    a.id === b.id &&
    a.incarnation === b.incarnation &&
    a.document === b.document
  );
}
function sameOriginal<T>(a: Original<T>, b: Original<T>): boolean {
  return (
    a.operation === b.operation &&
    a.correlation === b.correlation &&
    a.token === b.token
  );
}
function refusal(message: string): Error {
  return new Error("KELD-COMPAT-012: " + message);
}
function unknownError(): Error {
  return new Error("draft Quit effect unknown");
}

export class DraftQuitCoordinator<T> {
  private active?: Attempt<T>;
  private stopped?: { outcome: Outcome; h: Handle };
  // Only the one retired attempt whose already-issued response may still
  // be delivered by the exact private transport owner. Not retry authority.
  private retiredPending?: { attempt: Attempt<T>; pending: Pending<T> };
  private last?: Handle;
  private lastDisposition?: Outcome;
  private reportFailure?: {
    error: unknown;
    custody: ErrorCustody<T>;
    cause: unknown;
  };
  private readonly eligible: boolean;
  constructor(
    private readonly host: DraftHost<T>,
    private readonly listeners: DraftListeners,
    private readonly generation: bigint,
  ) {
    this.eligible = host.conditionalEligible === true;
  }

  /** Simulated public D2 interface. Never waits for physical close/Save. */
  quit(): Promise<void> {
    return this.begin(true).public;
  }

  /** Existing internal fixture probe; not a public API or an exit receipt. */
  requestQuit(): Promise<Outcome> {
    if (this.stopped) return Promise.resolve(this.stopped.outcome);
    return this.begin(false).outcome;
  }

  /** First observation remains immutable; disposition reflects later Status. */
  get outcome(): Promise<Outcome> | undefined {
    return this.last?.outcome;
  }
  get disposition(): Outcome | undefined {
    return this.lastDisposition;
  }
  get heldDisposition(): Disposition<T> | undefined {
    return this.active?.pending?.held;
  }
  get custodyFailure() {
    return this.reportFailure;
  }

  private begin(publicCall: boolean): Handle {
    if (this.stopped) return this.stopped.h;
    const a = this.active;
    if (a) {
      // Synchronous nested listeners share the original handle. One explicit
      // call after the veto callback returned may wait for confirmed Cancel.
      if (publicCall && a.phase === "cancel" && a.willVeto) {
        return (a.queued ??= handle());
      }
      return a.h;
    }
    if (this.reportFailure) {
      const h = handle();
      this.reject(h, this.reportFailure.error);
      h.observe("unknown");
      this.stopped = { outcome: "unknown", h };
      return h;
    }
    return this.start(handle());
  }

  private start(h: Handle): Handle {
    const a: Attempt<T> = {
      h,
      phase: "before",
      entries: [],
      dispatching: false,
      vetoed: false,
      willVeto: false,
      refused: false,
      uncertain: false,
      cancelOutcome: "retained",
    };
    this.active = a;
    this.last = h;
    this.lastDisposition = undefined;
    try {
      let veto: boolean;
      try {
        veto = this.listeners.beforeQuit();
      } catch {
        veto = true;
      }
      if (veto) {
        this.finish(a, "vetoed");
        return h;
      }
      if (this.active !== a) return h;
      const census = this.current();
      // Old/unselected peers never see the new close sweep or Prepare.
      if (!this.eligible || !census) {
        this.finish(a, "retained", refusal("conditional Quit not qualified"));
        return h;
      }
      a.phase = "closing";
      a.entries = [...census.windows].reverse().map((window) => ({
        window,
        destroyed: false,
        acknowledged: false,
      }));
      a.dispatching = true;
      for (const e of a.entries) {
        if (this.active !== a) break; // retirement, not veto, revokes dispatch
        try {
          const reply = this.host.requestClose(e.window, (seq) => {
            if (this.active !== a) return;
            if (this.bind(a, e, seq)) this.fulfill(h); // before user close listener
            this.advance(a);
          });
          void reply.then(
            (r) => {
              if (r.kind === "refused" || r.kind === "unknown") {
                this.closeFailure(
                  a,
                  e,
                  r.error ?? refusal("close handoff refused"),
                  r.kind === "unknown",
                );
              } else if (
                r.kind === "already-closing" &&
                e.sequence === undefined
              ) {
                this.closeFailure(
                  a,
                  e,
                  new Error("KELD-CORE-054: no bound close sequence"),
                  false,
                );
              } else if (this.active === a) {
                if (
                  r.kind === "already-closing" ||
                  this.bind(a, e, r.sequence)
                ) {
                  e.acknowledged = true;
                  this.fulfill(h);
                }
              } else if (
                r.kind !== "already-closing" &&
                e.sequence !== undefined
              ) {
                this.bind(a, e, r.sequence); // validate late reply, never touch a successor
              }
              this.advance(a);
            },
            (error) => {
              this.closeFailure(a, e, error, true);
              this.advance(a);
            },
          );
        } catch (error) {
          this.closeFailure(a, e, error, true);
        }
      }
      a.dispatching = false;
      this.advance(a);
    } catch (error) {
      this.finish(a, a.phase === "before" ? "retained" : "unknown", error);
    }
    return h;
  }

  observeVetoApplied(
    generation: bigint,
    window: WindowIdentity,
    sequence: bigint,
  ): void {
    const a = this.active;
    if (generation !== this.generation || !a || a.phase !== "closing") return;
    const e = a.entries.find((x) => same(x.window, window));
    if (!e) return;
    if (e.sequence === sequence) a.vetoed = true;
    else if (e.sequence === undefined) {
      if (e.earlyVeto !== undefined && e.earlyVeto !== sequence) {
        this.closeFailure(a, e, unknownError(), true);
      } else e.earlyVeto = sequence;
    }
    this.advance(a);
  }

  observeDestroyed(generation: bigint, window: WindowIdentity): void {
    const a = this.active;
    if (generation !== this.generation || !a || a.phase !== "closing") return;
    const e = a.entries.find((x) => same(x.window, window));
    if (!e) return;
    e.destroyed = true;
    this.advance(a);
  }

  observeLastWindowClosed(generation: bigint): void {
    if (generation !== this.generation || this.stopped) return;
    const a = this.active;
    if (a && a.phase !== "before" && !a.vetoed && !a.willVeto) return;
    const census = this.current();
    if (census && census.windows.length === 0)
      this.listeners.lastWindowClosed();
  }

  retire(generation: bigint): void {
    if (generation !== this.generation || this.stopped) return;
    const a = this.active;
    if (a) {
      // Retirement cannot prove that an unresolved effect did not commit.
      const outcome =
        a.pending || a.phase === "unknown" ? "unknown" : "retired";
      if (a.pending) this.retiredPending = { attempt: a, pending: a.pending };
      this.finish(
        a,
        outcome,
        a.pending?.noncommitDelivered
          ? undefined
          : refusal("original role retired"),
        true,
      );
    } else {
      const h = handle();
      this.reject(h, refusal("original role retired"));
      h.observe("retired");
      this.stopped = { outcome: "retired", h };
    }
  }

  private current(): Census | undefined {
    const census = this.host.census();
    return census.ready &&
      census.selected &&
      census.generation === this.generation
      ? census
      : undefined;
  }

  private bind(a: Attempt<T>, e: Entry, sequence: bigint): boolean {
    if (
      sequence <= 0n ||
      sequence > 0xffff_ffff_ffff_ffffn ||
      (e.sequence !== undefined && e.sequence !== sequence)
    ) {
      this.closeFailure(
        a,
        e,
        new Error("KELD-IPC-003: conflicting close sequence"),
        true,
      );
      return false;
    }
    e.sequence = sequence;
    if (e.earlyVeto === sequence) a.vetoed = true;
    e.earlyVeto = undefined;
    return true;
  }

  private closeFailure(
    a: Attempt<T>,
    e: Entry,
    error: unknown,
    ambiguous: boolean,
  ): void {
    if (this.active === a) {
      if (ambiguous) a.uncertain = true;
      else a.refused = true;
    }
    this.failPublic(a, error, {
      phase: "close",
      window: e.window,
      generation: this.generation,
    });
  }

  private advance(a: Attempt<T>): void {
    if (this.active !== a || a.phase !== "closing" || a.dispatching) return;
    if (a.uncertain) return this.finish(a, "unknown");
    if (a.vetoed) return this.finish(a, "vetoed");
    if (a.refused) return this.finish(a, "retained");
    if (!a.entries.every((e) => e.destroyed && e.acknowledged)) return;
    try {
      const census = this.current();
      if (!census || census.windows.length !== 0)
        return this.finish(a, "retained");
      this.effect(a, { operation: "prepare" });
    } catch (error) {
      this.finish(a, "unknown", error);
    }
  }

  private effect(a: Attempt<T>, p: Pending<T>): void {
    if (this.active !== a) return;
    a.pending = p;
    a.phase = p.operation;
    try {
      const promise: Promise<
        CancelResult<T> | CommitResult<T> | PrepareResult<T>
      > =
        p.operation === "prepare"
          ? this.host.prepare()
          : p.operation === "cancel"
            ? this.host.cancel(p.token!)
            : this.host.commit(p.token!);
      void promise.then(
        (value) => {
          try {
            this.accept(
              a,
              p,
              typeof value === "string" ? { kind: value } : value,
            );
          } catch (error) {
            this.uncertain(a, p, p.original, error);
          }
        },
        (error) => this.uncertain(a, p, undefined, error),
      );
    } catch (error) {
      this.uncertain(a, p, undefined, error);
    }
  }

  private accept(
    a: Attempt<T>,
    p: Pending<T>,
    d: Disposition<T>,
    source: "effect" | "status" | "held" = "effect",
  ): void {
    // A captured, validated terminal response from the exact original
    // operation may arrive after local retirement. Record only history;
    // never resettle the original public promise, run listeners or retry.
    if (
      this.retiredPending?.attempt === a &&
      this.retiredPending.pending === p &&
      this.stopped?.h === a.h &&
      this.stopped.outcome === "unknown"
    ) {
      if (
        d.kind === "competing" ||
        (d.kind === "accepted" && p.operation === "commit")
      ) {
        const outcome = d.kind === "accepted" ? "committed" : "competing";
        this.retiredPending = undefined;
        this.stopped = { outcome, h: a.h };
        if (this.last === a.h) this.lastDisposition = outcome;
        // Preserve original error settlement; terminal cleanup never
        // requires a consumption handshake on the retired link.
        if (d.error !== undefined)
          this.report(d.error, {
            phase: p.operation,
            original: p.original,
            generation: this.generation,
          });
      } else if (
        d.kind === "unknown" ||
        d.kind === "refused" ||
        d.kind === "retained" ||
        d.kind === "invalidated"
      ) {
        if (d.kind === "unknown") this.rememberOriginal(p, d.original);
        // Late nonterminal errors are historical custody, never retry authority.
        if (d.error !== undefined && !p.noncommitDelivered)
          this.report(d.error, {
            phase: source === "status" ? "status" : p.operation,
            original: p.original,
            generation: this.generation,
          });
      }
      return;
    }
    if (this.active !== a || a.pending !== p) return;
    if (d.kind === "unknown")
      return this.uncertain(a, p, d.original, d.error ?? unknownError());
    if (d.kind === "competing")
      return this.finish(
        a,
        "competing",
        d.error ?? refusal("another terminal winner committed"),
        false,
        (consumer) => this.consume(p, d, consumer),
      );
    // An impossible operation/result pair is not evidence of no effect.
    if (d.kind === "accepted") {
      if (p.operation !== "commit")
        return this.uncertain(a, p, undefined, unknownError());
      // Known acceptance survives an error in local fencing.
      return this.finish(a, "committed", d.error, false, (consumer) =>
        this.consume(p, d, consumer),
      );
    }
    if (!this.current())
      return this.uncertain(
        a,
        p,
        undefined,
        ("error" in d ? d.error : undefined) ??
          refusal("original role is no longer current"),
      );
    if (
      d.kind === "retained" ||
      d.kind === "invalidated" ||
      d.kind === "refused"
    ) {
      // Cancel needs actual cancellation/nonexecution, not a generic refusal.
      // An adapter's refused Status/Commit is not evidence of nonexecution.
      if (
        p.operation === "cancel" ||
        (d.kind === "invalidated" &&
          (p.operation !== "prepare" || source === "effect")) ||
        (d.kind === "refused" && (p.operation !== "prepare" || p.original))
      ) {
        return this.uncertain(a, p, undefined, d.error ?? unknownError());
      }
    }
    if (
      (d.kind === "canceled" && p.operation !== "cancel") ||
      (d.kind === "prepared" && p.operation !== "prepare")
    ) {
      return this.uncertain(a, p, undefined, unknownError());
    }
    // Store the exact known reply before acknowledging consumption. No
    // subsequent effect or fresh attempt until allocator custody is safe.
    if (d.kind !== "refused") {
      p.held = d;
      p.awaitingRelease = false;
      if (
        (d.kind === "retained" || d.kind === "invalidated") &&
        d.error !== undefined &&
        !p.noncommitDelivered
      ) {
        // A known typed noncommit error settles independently of private
        // receipt release. Mark it before the consumer can retire reentrantly.
        p.noncommitDelivered = true;
        this.failPublic(a, d.error, {
          phase: p.operation,
          original: p.original,
          generation: this.generation,
        });
        if (this.active !== a || a.pending !== p) return;
        if (!this.current()) {
          a.phase = "unknown";
          this.publish(a, "unknown");
          return;
        }
      }
      const consumed = this.consume(p, d);
      if (this.active !== a || a.pending !== p) return;
      if (!consumed.ready) {
        if (consumed.pending) {
          // Pending proof is not an effect failure. Keep private effects blocked.
          p.awaitingRelease = true;
          a.phase = p.operation;
          const recovery = a.recovery;
          a.recovery = undefined;
          recovery?.resolve("unknown"); // reconciliation is not yet complete
          if (
            (d.kind === "retained" || d.kind === "invalidated") &&
            !p.noncommitDelivered
          ) {
            // A definitive noncommit may settle the public error now, but the
            // attempt remains held. Mark delivery before any reentrant callback.
            p.noncommitDelivered = true;
            if (d.error !== undefined)
              this.failPublic(a, d.error, {
                phase: p.operation,
                original: p.original,
                generation: this.generation,
              });
            else this.reject(a.h, refusal("retained"));
          }
          return;
        }
        return this.uncertain(a, p, p.original, consumed.error);
      }
      p.held = undefined;
    }
    if (
      d.kind === "retained" ||
      d.kind === "invalidated" ||
      d.kind === "refused"
    ) {
      return this.finish(
        a,
        p.operation === "cancel" ? a.cancelOutcome : "retained",
        p.noncommitDelivered ? undefined : d.error,
      );
    }
    if (d.kind === "canceled") return this.finish(a, a.cancelOutcome);
    if (d.kind !== "prepared")
      return this.uncertain(a, p, undefined, unknownError());
    // Consume this exact Prepare authorization once; Status cannot repeat will.
    a.pending = undefined;
    try {
      const census = this.current();
      if (!census)
        return this.finish(a, "unknown", refusal("original role retired"));
      if (
        census.windows.length !== 0 ||
        this.host.observedInvalidation?.(d.token)
      ) {
        return this.cancel(a, d.token, "retained");
      }
      a.phase = "will";
      let veto: boolean;
      try {
        veto = this.listeners.willQuit();
      } catch {
        veto = true;
      }
      if (this.active !== a) return;
      if (!this.current())
        return this.finish(a, "unknown", refusal("original role retired"));
      if (veto) {
        a.willVeto = true;
        this.fulfill(a.h);
        return this.cancel(a, d.token, "vetoed");
      }
      this.effect(a, { operation: "commit", token: d.token });
    } catch (error) {
      this.finish(a, "unknown", error);
    }
  }

  private cancel(
    a: Attempt<T>,
    token: T,
    outcome: "vetoed" | "retained",
  ): void {
    a.cancelOutcome = outcome;
    this.effect(a, { operation: "cancel", token });
  }

  private consume(
    p: Pending<T>,
    d: Disposition<T>,
    consumer?: ReceiptConsumer<T>,
  ): Consumption {
    try {
      const receipt = {
        operation: p.operation,
        token: p.token,
        original: p.original,
        disposition: d,
      };
      return (
        (consumer
          ? Reflect.apply(consumer, this.host, [receipt])
          : this.host.consumeDisposition?.(receipt)) ?? {
          ready: false,
          error: new Error("missing receipt acknowledgement"),
        }
      );
    } catch (error) {
      return { ready: false, error };
    }
  }

  private uncertain(
    a: Attempt<T>,
    p: Pending<T>,
    original: Original<T> | undefined,
    error: unknown,
  ): void {
    if (
      this.retiredPending?.attempt === a &&
      this.retiredPending.pending === p
    ) {
      this.rememberOriginal(p, original);
      this.report(error, {
        phase: p.operation,
        original: p.original,
        generation: this.generation,
      });
      return;
    }
    if (this.active !== a || a.pending !== p) return;
    this.rememberOriginal(p, original);
    a.phase = "unknown";
    this.publish(a, "unknown");
    this.rejectQueued(a, error, "unknown");
    this.failPublic(a, error, {
      phase: p.operation,
      original: p.original,
      generation: this.generation,
    });
  }

  private rememberOriginal(p: Pending<T>, original?: Original<T>): void {
    if (
      original &&
      original.operation === p.operation &&
      original.token === p.token &&
      Number.isInteger(original.correlation) &&
      original.correlation > 0 &&
      original.correlation <= 0xffff_ffff &&
      (!p.original || sameOriginal(original, p.original))
    ) {
      p.original ??= Object.freeze({ ...original });
    }
  }

  /** One ordered read-only Status for the same original operation; never replay. */
  reconcile(): Promise<Outcome> {
    const a = this.active;
    if (!a)
      return Promise.resolve(
        this.stopped?.outcome ?? this.lastDisposition ?? "retained",
      );
    if (a.recovery) return a.recovery.promise;
    const p = a.pending;
    if (
      (a.phase !== "unknown" && !p?.awaitingRelease) ||
      !p ||
      !this.current() ||
      (!p.held && (!p.original || !this.host.status))
    ) {
      // No local release check or Status can run on an unqualified role.
      // Do not wait on an outcome this reconciliation cannot settle.
      return a.phase === "unknown" || p?.awaitingRelease
        ? Promise.resolve("unknown")
        : a.h.outcome;
    }
    const original = p.original;
    let resolve!: Recovery["resolve"];
    const promise = new Promise<Outcome>((ok) => {
      resolve = ok;
    });
    a.recovery = { promise, resolve };
    if (p.held) {
      // Already-known disposition: retry only the local release check.
      try {
        this.accept(a, p, p.held, "held");
      } catch (error) {
        this.uncertain(a, p, p.original, error);
      }
      return promise;
    }
    const failed = (error?: unknown): void => {
      if (
        this.retiredPending?.attempt === a &&
        this.retiredPending.pending === p
      ) {
        if (error !== undefined)
          this.report(error, {
            phase: "status",
            original,
            generation: this.generation,
          });
        return;
      }
      if (this.active !== a || a.pending !== p) return;
      this.publish(a, "unknown");
      if (error !== undefined)
        this.report(error, {
          phase: "status",
          original,
          generation: this.generation,
        });
    };
    try {
      void this.host.status!(original!).then((status) => {
        const retired =
          this.retiredPending?.attempt === a &&
          this.retiredPending.pending === p;
        if ((this.active !== a || a.pending !== p) && !retired) return;
        try {
          if (
            !status?.original ||
            !status.disposition ||
            !sameOriginal(status.original, original!)
          )
            return failed();
          if (retired) {
            this.accept(a, p, status.disposition, "status");
            return;
          }
          if (status.disposition.kind === "unknown")
            return failed(status.disposition.error);
          // Exact known terminal receipts remain historical evidence even if
          // the current census changes while Status is outstanding.
          if (
            status.disposition.kind !== "accepted" &&
            status.disposition.kind !== "competing" &&
            !this.current()
          )
            return failed(
              "error" in status.disposition
                ? status.disposition.error
                : undefined,
            );
          this.accept(a, p, status.disposition, "status");
        } catch (error) {
          failed(error);
        }
      }, failed);
    } catch (error) {
      failed(error);
    }
    return promise;
  }

  private fulfill(h: Handle): void {
    if (!h.settled) {
      h.settled = true;
      h.resolve();
    }
  }
  private reject(h: Handle, error: unknown): void {
    if (!h.settled) {
      h.settled = true;
      h.reject(error);
    }
  }
  private report(error: unknown, custody: ErrorCustody<T>): void {
    try {
      this.host.reportLateError(error, custody);
    } catch (cause) {
      // Preserve the first failed custody transfer without undoing commitment.
      this.reportFailure ??= { error, custody, cause };
    }
  }
  private failPublic(
    a: Attempt<T>,
    error: unknown,
    custody: ErrorCustody<T>,
  ): void {
    if (a.h.settled) this.report(error, custody);
    else this.reject(a.h, error);
  }
  private rejectQueued(a: Attempt<T>, error: unknown, outcome: Outcome): void {
    if (!a.queued) return;
    this.reject(a.queued, error);
    a.queued.observe(outcome);
    a.queued = undefined;
  }
  private publish(a: Attempt<T>, outcome: Outcome): void {
    if (this.last === a.h) this.lastDisposition = outcome;
    a.h.observe(outcome); // immutable first internal observation
    const recovery = a.recovery;
    a.recovery = undefined;
    recovery?.resolve(outcome);
  }
  private finish(
    a: Attempt<T>,
    outcome: Outcome,
    error?: unknown,
    retired = false,
    consume?: (consumer: ReceiptConsumer<T>) => Consumption,
  ): void {
    if (this.active !== a) return;
    const unknown = outcome === "unknown" && !retired;
    if (unknown) a.phase = "unknown";
    else {
      this.active = undefined;
      if (
        retired ||
        outcome === "committed" ||
        outcome === "competing" ||
        outcome === "retired"
      ) {
        this.stopped = { outcome, h: a.h };
      }
    }
    // Publish the authoritative result before any injected error-owner callback
    // can reenter or retire the link. A known terminal result is irreversible.
    this.publish(a, outcome);
    // Terminal history and cleanup do not depend on receipt reclamation.
    // Only report an acknowledgement result when a terminal adapter supplies one.
    let consumed: Consumption | undefined;
    if (consume) {
      try {
        const consumer = this.host.consumeDisposition;
        consumed = consumer ? consume(consumer) : undefined;
      } catch (lookupError) {
        // A throwing adapter getter must not strand a known terminal result.
        consumed = { ready: false, error: lookupError };
      }
    }
    if (consumed && !consumed.ready && !consumed.pending) {
      // An accepted reservation and completed local fence do not depend on
      // reclaiming a nonterminal correlation. Retain its custody; report
      // any local diagnostic without rejecting a successful terminal result.
      if (
        error === undefined &&
        outcome !== "committed" &&
        outcome !== "competing"
      )
        error = consumed.error;
      else
        this.report(consumed.error, {
          phase: a.pending?.operation ?? "close",
          original: a.pending?.original,
          generation: this.generation,
        });
    }
    if (error !== undefined)
      this.failPublic(a, error, {
        phase: a.pending?.operation ?? "close",
        original: a.pending?.original,
        generation: this.generation,
      });
    else if (outcome === "committed" || outcome === "vetoed") this.fulfill(a.h);
    else if (!a.h.settled)
      this.reject(
        a.h,
        outcome === "unknown" ? unknownError() : refusal(outcome),
      );
    if (unknown) {
      this.rejectQueued(a, error ?? unknownError(), outcome);
      return;
    }
    if (this.stopped || this.active || this.reportFailure) {
      this.rejectQueued(a, error ?? refusal(outcome), outcome);
      return;
    }
    const queued = a.queued;
    if (!queued) return;
    try {
      if (!this.current())
        return this.rejectQueued(
          a,
          refusal("original role retired"),
          "retired",
        );
    } catch (cause) {
      return this.rejectQueued(a, cause, "unknown");
    }
    if (this.stopped || this.active || this.reportFailure)
      return this.rejectQueued(a, refusal(outcome), outcome);
    a.queued = undefined;
    this.start(queued); // proved Cancel/Retained on the still-current role
  }
}
