/**
 * GH-451 review-only, non-wired API-side retained-link fixture.
 * These are abstract decoded outcomes, not wire tags, error codes, public
 * exports, negotiation, a Host implementation or authority grants.
 */
import {
  DraftCorrelationLedger,
  type AttemptLease,
  type CorrelationLease,
} from "./reservation.ts";

export {
  DraftCorrelationLedger,
  type CorrelationLease,
} from "./reservation.ts";
export type Effect = "prepare" | "cancel" | "commit";
export type Token = string; // Opaque fixture match value issued by the peer.
export type Original = Readonly<{
  operation: Effect;
  correlation: number;
  token?: Token;
}>;
export type Request =
  | Readonly<{ operation: "prepare" }>
  | Readonly<{ operation: "cancel" | "commit"; token: Token }>
  | Readonly<{ operation: "status"; original: Original }>;
export type Disposition =
  | Readonly<{ kind: "prepared"; token: Token }>
  | Readonly<{ kind: "invalidated" }>
  | Readonly<{ kind: "retained" }>
  | Readonly<{ kind: "cancelled" }>
  | Readonly<{ kind: "accepted" }>
  | Readonly<{ kind: "competing-terminal" }>;
export type Outcome =
  | Disposition
  | Readonly<{
      kind: "refused";
      reason:
        | "unsupported"
        | "unavailable"
        | "busy"
        | "wrong-phase"
        | "receipt-pending"
        | "capacity";
    }>
  | Readonly<{ kind: "unknown"; original: Original }>;
export type ResolvedReceipt = Readonly<{
  original: Original;
  disposition: Disposition;
  source: "direct" | "status";
}>;
export interface RetainedPort {
  // These facts are assumed to have been validated and captured by the
  // trusted existing link owner; there is deliberately no negotiation here.
  readonly negotiated: boolean;
  readonly selected: boolean;
  /** Existing transport owner's allocator, shared with ordinary calls. */
  readonly ledger: DraftCorrelationLedger;
  isCurrent(): boolean;
  isLive(): boolean;
  /** Correlation already reserved before any write. No implicit retry. */
  send(reservation: CorrelationLease, request: Request): Promise<unknown>;
  // The actual link owner must fence its own link on known acceptance only.
  fenceAccepted(): void;
}
type Phase =
  | { kind: "idle" }
  | { kind: "prepared"; token: Token }
  | { kind: "busy" }
  | { kind: "unknown"; original: Original }
  | { kind: "accepted" }
  | { kind: "competing-terminal" };
function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object"
    ? (value as Record<string, unknown>)
    : undefined;
}
function sameOriginal(value: unknown, expected: Original): boolean {
  const r = record(value);
  return (
    r !== undefined &&
    r.operation === expected.operation &&
    r.correlation === expected.correlation &&
    r.token === expected.token
  );
}
function disposition(
  operation: Effect,
  value: unknown,
): Disposition | undefined {
  const r = record(value);
  if (!r) return undefined;
  if (
    operation === "prepare" &&
    r.kind === "prepared" &&
    typeof r.token === "string" &&
    r.token.length > 0
  ) {
    return { kind: "prepared", token: r.token };
  }
  if (operation === "prepare" && r.kind === "invalidated")
    return { kind: "invalidated" };
  if (r.kind === "competing-terminal") return { kind: "competing-terminal" };
  if (
    (operation === "prepare" || operation === "commit") &&
    r.kind === "retained"
  )
    return { kind: "retained" };
  if (operation === "cancel" && r.kind === "cancelled")
    return { kind: "cancelled" };
  if (operation === "commit" && r.kind === "accepted")
    return { kind: "accepted" };
  // In particular generic ERR, wrong operation, and missing/malformed ACK
  // are not no-effect. Other cancellation dispositions need owner approval.
  return undefined;
}

/** One local attempt over one captured existing port; never reconnects. */
export class RetainedLinkDraft {
  readonly #port: RetainedPort;
  readonly #ledger: DraftCorrelationLedger;
  readonly #eligible: boolean;
  #phase: Phase = { kind: "idle" };
  #attempt?: AttemptLease;
  #originalLease?: CorrelationLease;
  #receipt?: ResolvedReceipt;
  #effectError: unknown;
  #statusError: unknown;
  #fenceError: unknown;

  constructor(port: RetainedPort) {
    this.#port = port;
    this.#ledger = port.ledger;
    this.#eligible = port.negotiated && port.selected;
  }

  /** Preflight for a NEW conditional facade attempt; not a Status gate. */
  get conditionalAvailable(): boolean {
    this.#syncReleased();
    return (
      this.#eligible &&
      this.#phase.kind === "idle" &&
      !this.#ledger.active &&
      this.#port.isCurrent() &&
      this.#port.isLive()
    );
  }
  prepare(): Promise<Outcome> {
    return this.#effect("prepare");
  }
  cancel(): Promise<Outcome> {
    return this.#effect("cancel");
  }
  commit(): Promise<Outcome> {
    return this.#effect("commit");
  }
  /** Keep actual rejection objects distinct from the ambiguous disposition. */
  get effectError(): unknown {
    return this.#effectError;
  }
  get statusError(): unknown {
    return this.#statusError;
  }
  get fenceError(): unknown {
    return this.#fenceError;
  }
  get terminalDisposition(): "accepted" | "competing-terminal" | undefined {
    return this.#phase.kind === "accepted" ||
      this.#phase.kind === "competing-terminal"
      ? this.#phase.kind
      : undefined;
  }

  /** Exact object for W3 to acknowledge after consuming the disposition. */
  get pendingReceipt(): ResolvedReceipt | undefined {
    this.#syncReleased();
    return this.#receipt;
  }

  /** Acknowledgement alone never permits correlation reuse. */
  consumeReceipt(receipt: ResolvedReceipt): boolean {
    this.#syncReleased();
    const lease = this.#originalLease;
    if (!lease || receipt !== this.#receipt) return false;
    const released = this.#ledger.consume(lease);
    this.#syncReleased();
    return released;
  }

  #syncReleased(): void {
    if (this.#originalLease && !this.#ledger.isReserved(this.#originalLease)) {
      this.#originalLease = undefined;
      this.#receipt = undefined;
    }
    if (this.#phase.kind === "idle" && this.#attempt) {
      this.#ledger.endAttempt(this.#attempt);
      if (!this.#ledger.ownsAttempt(this.#attempt)) this.#attempt = undefined;
    }
  }

  #unknown(original: Original): Outcome {
    this.#phase = { kind: "unknown", original };
    return { kind: "unknown", original };
  }

  #settle(
    value: Disposition,
    original: Original,
    source: ResolvedReceipt["source"],
  ): Disposition {
    value = Object.freeze(value);
    this.#receipt = Object.freeze({ original, disposition: value, source });
    switch (value.kind) {
      case "prepared":
        this.#phase = { kind: "prepared", token: value.token };
        break;
      case "retained":
      case "cancelled":
      case "invalidated":
        this.#phase = { kind: "idle" };
        // Mark the known ended attempt even while receipt/control identities
        // remain reserved. The shared ledger finishes it on the final proof.
        this.#syncReleased();
        break;
      case "competing-terminal":
        this.#phase = { kind: "competing-terminal" };
        break;
      case "accepted":
        this.#phase = { kind: "accepted" };
        // A valid correlated Accepted is the terminal reservation receipt,
        // not a cleanup or OS success receipt. The receipt is known even
        // if local fencing throws; cleanup never waits for receipt consumption.
        // An old link's historical receipt cannot fence its successor.
        try {
          if (this.#port.isCurrent()) this.#port.fenceAccepted();
        } catch (error) {
          this.#fenceError = error;
        }
        break;
    }
    return value;
  }

  async #effect(operation: Effect): Promise<Outcome> {
    this.#syncReleased();
    if (!this.#eligible) return { kind: "refused", reason: "unsupported" };
    if (this.#phase.kind === "unknown")
      return { kind: "unknown", original: this.#phase.original };
    if (this.#phase.kind === "busy") return { kind: "refused", reason: "busy" };
    if (
      this.#phase.kind === "accepted" ||
      this.#phase.kind === "competing-terminal"
    )
      return { kind: "refused", reason: "wrong-phase" };
    if (this.#receipt || this.#originalLease || this.#ledger.status)
      return { kind: "refused", reason: "receipt-pending" };
    if (!this.#port.isCurrent() || !this.#port.isLive())
      return { kind: "refused", reason: "unavailable" };
    const phase = this.#phase;
    if (
      operation === "prepare"
        ? phase.kind !== "idle"
        : phase.kind !== "prepared"
    )
      return { kind: "refused", reason: "wrong-phase" };
    const token = phase.kind === "prepared" ? phase.token : undefined;
    const request: Request =
      operation === "prepare" ? { operation } : { operation, token: token! };
    if (operation === "prepare") {
      const attempt = this.#ledger.beginAttempt();
      if (!attempt) return { kind: "refused", reason: "busy" };
      this.#attempt = attempt;
    }
    const attempt = this.#attempt;
    if (!attempt) return { kind: "refused", reason: "wrong-phase" };
    const lease = this.#ledger.reserveEffect(attempt);
    if (!lease) {
      // No send was attempted. Release a newly claimed empty attempt only.
      this.#syncReleased();
      return { kind: "refused", reason: "capacity" };
    }
    this.#originalLease = lease;
    const original: Original = Object.freeze({
      operation,
      correlation: lease.correlation,
      token,
    });
    this.#effectError = undefined;
    this.#statusError = undefined;
    // Set before calling the port so reentrant callers cannot send twice.
    this.#phase = { kind: "busy" };
    try {
      const reply = await this.#port.send(lease, request);
      const outcome = disposition(operation, reply);
      // A late nonterminal response cannot authorize a retired role or
      // claim the ended link remains usable. A verified terminal response
      // is historical evidence, even if the original role has retired.
      if (
        outcome?.kind !== "accepted" &&
        outcome?.kind !== "competing-terminal" &&
        (!this.#port.isCurrent() || !this.#port.isLive())
      )
        return this.#unknown(original);
      return outcome
        ? this.#settle(outcome, original, "direct")
        : this.#unknown(original);
    } catch (error) {
      // Even a synchronous send failure cannot prove absence of a write.
      // The original identity already exists and remains reserved.
      this.#effectError = error;
      return this.#unknown(original);
    }
  }

  async reconcile(): Promise<Outcome> {
    this.#syncReleased();
    if (!this.#eligible) return { kind: "refused", reason: "unsupported" };
    if (this.#phase.kind === "busy") return { kind: "refused", reason: "busy" };
    if (this.#phase.kind !== "unknown")
      return { kind: "refused", reason: "wrong-phase" };
    const original = this.#phase.original;
    const attempt = this.#attempt;
    const originalLease = this.#originalLease;
    if (
      !attempt ||
      !originalLease ||
      !this.#port.isCurrent() ||
      !this.#port.isLive()
    )
      return this.#unknown(original);
    const statusLease = this.#ledger.reserveStatus(attempt, originalLease);
    // Capacity pressure or a still-live earlier Status identity is never
    // evidence of nonexecution. No extra effect or status write occurs.
    if (!statusLease) return this.#unknown(original);
    this.#phase = { kind: "busy" };
    this.#statusError = undefined;
    try {
      // New transport correlation only for Status, never a replay of effects.
      const raw = await this.#port.send(statusLease, {
        operation: "status",
        original,
      });
      const reply = record(raw);
      if (
        !reply ||
        reply.kind !== "recorded" ||
        !sameOriginal(reply.original, original)
      )
        return this.#unknown(original);
      const outcome = disposition(original.operation, reply.disposition);
      if (
        outcome?.kind !== "accepted" &&
        outcome?.kind !== "competing-terminal" &&
        (!this.#port.isCurrent() || !this.#port.isLive())
      )
        return this.#unknown(original);
      return outcome
        ? this.#settle(outcome, original, "status")
        : this.#unknown(original);
    } catch (error) {
      this.#statusError = error;
      return this.#unknown(original);
    } finally {
      // Status itself is consumed here. A timed-out Status still occupies
      // its slot until the transport separately proves no late alias.
      this.#ledger.consume(statusLease);
    }
  }
}
