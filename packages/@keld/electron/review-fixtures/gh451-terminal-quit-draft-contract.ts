// Nonshipping W3 contract between the scripted facade and existing owner adapters.
// Owned by the Electron package's review-only test suite.
export type WindowIdentity = Readonly<{
  id: number;
  incarnation: bigint;
  document: bigint;
}>;
export type Census = Readonly<{
  generation: bigint;
  ready: boolean;
  selected: boolean;
  windows: readonly WindowIdentity[];
}>;
export type Effect = "prepare" | "cancel" | "commit";
export type Original<T> = Readonly<{
  operation: Effect;
  correlation: number;
  token?: T;
}>;
type Failure<T> =
  | { kind: "unknown"; original?: Original<T>; error?: unknown }
  | {
      kind: "refused" | "retained" | "invalidated" | "competing";
      error?: unknown;
    };
export type CloseResult =
  | { kind: "requested"; sequence: bigint }
  | { kind: "merged"; sequence: bigint }
  | { kind: "already-closing" }
  | { kind: "refused"; error?: unknown }
  | { kind: "unknown"; error?: unknown };
export type PrepareResult<T> = { kind: "prepared"; token: T } | Failure<T>;
export type Disposition<T> =
  | PrepareResult<T>
  | { kind: "canceled" }
  | { kind: "accepted"; error?: unknown }; // acceptance may precede local fence failure
export type CancelResult<T> = "canceled" | "unknown" | Disposition<T>;
export type CommitResult<T> =
  "accepted" | "retained" | "unknown" | Disposition<T>;
export type StatusResult<T> = Readonly<{
  original: Original<T>;
  disposition: Disposition<T>;
}>;
export type Outcome =
  "vetoed" | "retained" | "committed" | "unknown" | "retired" | "competing";
export type ErrorCustody<T> = Readonly<{
  phase: "close" | Effect | "status";
  window?: WindowIdentity;
  original?: Original<T>;
  generation: bigint;
}>;
export type ConsumedDisposition<T> = Readonly<{
  operation: Effect;
  token?: T;
  original?: Original<T>;
  disposition: Disposition<T>;
}>;
export type Consumption = { ready: true } | { ready: false; error: unknown };
export interface DraftHost<T> {
  // Captured negotiated/selected qualification, never inferred from Ready.
  readonly conditionalEligible: boolean;
  census(): Census;
  requestClose(
    window: WindowIdentity,
    dispatched: (sequence: bigint) => void,
  ): Promise<CloseResult>;
  prepare(): Promise<PrepareResult<T>>;
  cancel(token: T): Promise<CancelResult<T>>;
  commit(token: T): Promise<CommitResult<T>>;
  status?(original: Original<T>): Promise<StatusResult<T>>;
  observedInvalidation?(token: T): boolean;
  // Explicit consumer acknowledgement; true requires receipt release/no-late proof.
  consumeDisposition?(receipt: ConsumedDisposition<T>): Consumption;
  // The existing owner retains the exact cause and scope, including legacy
  // close-handoff retirement. Conditional unknown must preserve Status custody.
  reportLateError(error: unknown, custody: ErrorCustody<T>): void;
}
export interface DraftListeners {
  beforeQuit(): boolean;
  willQuit(): boolean;
  lastWindowClosed(): void;
}
