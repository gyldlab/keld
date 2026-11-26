/**
 * GH-451 review-only, non-wired API-side retained-link fixture.
 * These are abstract decoded outcomes, not wire tags, error codes, public
 * exports, negotiation, a Host implementation or authority grants.
 */
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
  | Readonly<{ kind: "retained" }>
  | Readonly<{ kind: "cancelled" }>
  | Readonly<{ kind: "accepted" }>;
export type Outcome =
  | Disposition
  | Readonly<{ kind: "refused"; reason: "unsupported" | "unavailable" | "busy" | "wrong-phase" }>
  | Readonly<{ kind: "unknown"; original: Original | undefined }>;
export type Receipt = Readonly<{
  // The existing transport allocator, not this API, owns the correlation.
  correlation: number;
  reply: Promise<unknown>;
}>;
export interface RetainedPort {
  // These facts are assumed to have been validated and captured by the
  // trusted existing link owner; there is deliberately no negotiation here.
  readonly negotiated: boolean;
  readonly selected: boolean;
  isCurrent(): boolean;
  isLive(): boolean;
  start(request: Request): Receipt;
  // The actual link owner must fence its own link on known acceptance only.
  fenceAccepted(): void;
}
type Phase =
  | { kind: "idle" }
  | { kind: "prepared"; token: Token }
  | { kind: "busy" }
  | { kind: "unknown"; original: Original | undefined }
  | { kind: "accepted" };

function isCorrelation(value: number): boolean {
  return Number.isInteger(value) && value > 0 && value <= 0xffff_ffff;
}
function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object"
    ? value as Record<string, unknown>
    : undefined;
}
function sameOriginal(value: unknown, expected: Original): boolean {
  const r = record(value);
  return r !== undefined &&
    r.operation === expected.operation &&
    r.correlation === expected.correlation &&
    r.token === expected.token;
}
function disposition(operation: Effect, value: unknown): Disposition | undefined {
  const r = record(value);
  if (!r) return undefined;
  if (operation === "prepare" && r.kind === "prepared" &&
      typeof r.token === "string" && r.token.length > 0) {
    return { kind: "prepared", token: r.token };
  }
  if ((operation === "prepare" || operation === "commit") && r.kind === "retained")
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
  readonly #eligible: boolean;
  #phase: Phase = { kind: "idle" };
  #effectError: unknown;
  #statusError: unknown;

  constructor(port: RetainedPort) {
    this.#port = port;
    this.#eligible = port.negotiated && port.selected;
  }

  prepare(): Promise<Outcome> { return this.#effect("prepare"); }
  cancel(): Promise<Outcome> { return this.#effect("cancel"); }
  commit(): Promise<Outcome> { return this.#effect("commit"); }
  /** Keep actual rejection objects distinct from the ambiguous disposition. */
  get effectError(): unknown { return this.#effectError; }
  get statusError(): unknown { return this.#statusError; }

  #unknown(original: Original | undefined): Outcome {
    this.#phase = { kind: "unknown", original };
    return { kind: "unknown", original };
  }

  #settle(value: Disposition): Disposition {
    switch (value.kind) {
      case "prepared":
        this.#phase = { kind: "prepared", token: value.token };
        break;
      case "retained":
      case "cancelled":
        this.#phase = { kind: "idle" };
        break;
      case "accepted":
        this.#phase = { kind: "accepted" };
        // A valid correlated Accepted is the terminal reservation receipt,
        // not a cleanup or OS success receipt. No other branch closes.
        this.#port.fenceAccepted();
        break;
    }
    return value;
  }

  async #effect(operation: Effect): Promise<Outcome> {
    if (!this.#eligible) return { kind: "refused", reason: "unsupported" };
    if (this.#phase.kind === "unknown")
      return { kind: "unknown", original: this.#phase.original };
    if (this.#phase.kind === "busy") return { kind: "refused", reason: "busy" };
    if (!this.#port.isCurrent() || !this.#port.isLive())
      return { kind: "refused", reason: "unavailable" };
    const phase = this.#phase;
    if (operation === "prepare" ? phase.kind !== "idle" : phase.kind !== "prepared")
      return { kind: "refused", reason: "wrong-phase" };
    const token = phase.kind === "prepared" ? phase.token : undefined;
    const request: Request = operation === "prepare"
      ? { operation }
      : { operation, token: token! };
    this.#effectError = undefined;
    this.#statusError = undefined;
    // Set before calling the port so reentrant callers cannot send twice.
    this.#phase = { kind: "busy" };
    let original: Original | undefined;
    let reply: unknown;
    try {
      const receipt = this.#port.start(request);
      if (!isCorrelation(receipt.correlation)) {
        // A port violating the receipt contract cannot prove nonexecution.
        void receipt.reply.catch(() => {});
        return this.#unknown(undefined);
      }
      original = Object.freeze({ operation, correlation: receipt.correlation, token });
      reply = await receipt.reply;
    } catch (error) {
      // A throw before receipt cannot prove absence of a write either.
      this.#effectError = error;
      return this.#unknown(original);
    }
    const outcome = disposition(operation, reply);
    // A late nonterminal response cannot authorize a retired role or claim
    // the now-ended link is usable. Acceptance, when still current, may
    // precede EOF on the original link; it is not a cleanup receipt.
    if (!this.#port.isCurrent() ||
        (outcome?.kind !== "accepted" && !this.#port.isLive()))
      return this.#unknown(original);
    return outcome ? this.#settle(outcome) : this.#unknown(original);
  }

  async reconcile(): Promise<Outcome> {
    if (!this.#eligible) return { kind: "refused", reason: "unsupported" };
    if (this.#phase.kind === "busy") return { kind: "refused", reason: "busy" };
    if (this.#phase.kind !== "unknown") return { kind: "refused", reason: "wrong-phase" };
    const original = this.#phase.original;
    if (!original || !this.#port.isCurrent() || !this.#port.isLive())
      return this.#unknown(original);
    this.#phase = { kind: "busy" };
    let raw: unknown;
    try {
      // New transport correlation only for Status, never a replay of effects.
      const receipt = this.#port.start({ operation: "status", original });
      if (!isCorrelation(receipt.correlation) || receipt.correlation === original.correlation) {
        void receipt.reply.catch(() => {});
        return this.#unknown(original);
      }
      raw = await receipt.reply;
    } catch (error) {
      this.#statusError = error;
      return this.#unknown(original);
    }
    const reply = record(raw);
    if (!reply || reply.kind !== "recorded" || !sameOriginal(reply.original, original))
      return this.#unknown(original);
    const outcome = disposition(original.operation, reply.disposition);
    if (!this.#port.isCurrent() ||
        (outcome?.kind !== "accepted" && !this.#port.isLive()))
      return this.#unknown(original);
    return outcome ? this.#settle(outcome) : this.#unknown(original);
  }
}
