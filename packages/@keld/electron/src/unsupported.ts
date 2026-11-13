/**
 * `KELD-COMPAT-013`: the one typed refusal for an Electron main-process member that
 * `@keld/electron` links but does not implement yet (GH-565, F06 L0 rung;
 * `docs/specs/gh565-link-time-electron-bindings.md` §4.1 D1-D2).
 *
 * A binding exists so an ES module that imports it by name links. A member that no Keld
 * slice implements yet raises this error with Electron's documented return kind:
 * synchronous members throw, Promise members return a rejected Promise, properties throw
 * on read (and on write where Electron documents them writable), constructors throw.
 * Nothing here reaches the app link, so evaluating a refusing binding sends no kipc frame.
 * A rejected refusal that an application never handles ends a stock Bun process; the
 * Electron-style do-not-quit handler belongs to the lifecycle unit (#476 § Notes).
 */

/** The registered code every refusal carries (`docs/engineering/keld-error-codes.md`). */
export const UNSUPPORTED_MEMBER_CODE = "KELD-COMPAT-013";

/** The gyldlab/keld issue that owns a member's implementation, as `GH-<n>`. */
export type TrackingIssue = `GH-${number}`;

/** How the refused member is reached, which decides the sentence in the message. */
export type UnsupportedKind = "method" | "promise" | "property" | "constructor";

function describe(kind: UnsupportedKind, member: string): string {
  switch (kind) {
    case "constructor":
      return `new electron.${member}()`;
    case "property":
      return `electron.${member}`;
    default:
      return `electron.${member}()`;
  }
}

/** A linked but unimplemented Electron member was called, read or constructed. */
export class UnsupportedMemberError extends Error {
  readonly code: typeof UNSUPPORTED_MEMBER_CODE = UNSUPPORTED_MEMBER_CODE;
  /** `electron.`-relative path, for example `shell.openExternal`, `Menu` or `ClipboardItem.types`. */
  readonly member: string;
  /** The issue whose change replaces this refusal with the real member. */
  readonly ticket: TrackingIssue;
  readonly kind: UnsupportedKind;

  constructor(kind: UnsupportedKind, member: string, ticket: TrackingIssue) {
    super(
      `${UNSUPPORTED_MEMBER_CODE}: ${describe(kind, member)} is not implemented by @keld/electron yet; it is tracked on gyldlab/keld#${ticket.slice(3)}. Follow that issue for the first supported release and guard or remove this call site until then: the binding exists so the module links, and a silent stub is forbidden.`,
    );
    this.name = "UnsupportedMemberError";
    this.kind = kind;
    this.member = member;
    this.ticket = ticket;
  }
}

/**
 * True when `value` is a `KELD-COMPAT-013` refusal, by its registered fields rather than
 * by class identity, so a second module instance of this package still classifies it.
 */
export function isUnsupportedMemberError(
  value: unknown,
): value is UnsupportedMemberError {
  if (!(value instanceof Error)) return false;
  const { code, member, ticket } = value as Error & {
    code?: unknown;
    member?: unknown;
    ticket?: unknown;
  };
  return (
    code === UNSUPPORTED_MEMBER_CODE &&
    typeof member === "string" &&
    typeof ticket === "string"
  );
}

/** Throws the refusal for a synchronous member, a property read or a constructor. */
export function refuse(
  kind: Exclude<UnsupportedKind, "promise">,
  member: string,
  ticket: TrackingIssue,
): never {
  throw new UnsupportedMemberError(kind, member, ticket);
}

/** The refusal for a member whose pinned page says `Returns Promise<…>`: it rejects. */
export function refuseLater(
  member: string,
  ticket: TrackingIssue,
): Promise<never> {
  return Promise.reject(new UnsupportedMemberError("promise", member, ticket));
}

/** The documented shape of one member of a refusing module object. */
export type MemberKind = "method" | "promise" | "property" | "writableProperty";

/** One row of a binding's member table: its documented shape and its owning issue. */
export interface UnsupportedMember {
  readonly kind: MemberKind;
  readonly ticket: TrackingIssue;
}

/** A member documented without a `Promise` return: the call throws. */
export function method<T extends TrackingIssue>(
  ticket: T,
): { readonly kind: "method"; readonly ticket: T } {
  return { kind: "method", ticket };
}

/** A member documented as `Returns Promise<…>`: the call returns a rejected Promise. */
export function promise<T extends TrackingIssue>(
  ticket: T,
): { readonly kind: "promise"; readonly ticket: T } {
  return { kind: "promise", ticket };
}

/** A read-only property: reading it throws. */
export function property<T extends TrackingIssue>(
  ticket: T,
): { readonly kind: "property"; readonly ticket: T } {
  return { kind: "property", ticket };
}

/** A writable property: reading and assigning both throw. */
export function writableProperty<T extends TrackingIssue>(
  ticket: T,
): { readonly kind: "writableProperty"; readonly ticket: T } {
  return { kind: "writableProperty", ticket };
}

/**
 * The public type of a refusing module object: a method or Promise member is a function
 * that never returns normally, and a property has no readable value.
 */
export type UnsupportedSurface<T extends Record<string, UnsupportedMember>> = {
  readonly [K in keyof T]: T[K]["kind"] extends "promise"
    ? (...args: unknown[]) => Promise<never>
    : T[K]["kind"] extends "method"
      ? (...args: unknown[]) => never
      : never;
};

function named<F extends (...args: never[]) => unknown>(
  name: string,
  fn: F,
): F {
  Object.defineProperty(fn, "name", { value: name, configurable: true });
  return fn;
}

/**
 * Builds a frozen module object whose every member refuses as its row documents. The
 * object is frozen so no code path can turn a refusal into a silent success by
 * reassigning a member; an implementing slice replaces the row in `unsupported-bindings.ts`.
 */
export function unsupportedSurface<T extends Record<string, UnsupportedMember>>(
  binding: string,
  members: T,
): UnsupportedSurface<T> {
  const surface: Record<string, unknown> = {};
  for (const name of Object.keys(members)) {
    const spec: UnsupportedMember = members[name];
    const member = `${binding}.${name}`;
    const descriptor: PropertyDescriptor = {
      enumerable: true,
      configurable: false,
    };
    switch (spec.kind) {
      case "method":
        descriptor.value = named(name, (): never =>
          refuse("method", member, spec.ticket),
        );
        break;
      case "promise":
        descriptor.value = named(name, (): Promise<never> =>
          refuseLater(member, spec.ticket),
        );
        break;
      case "property":
        descriptor.get = (): never => refuse("property", member, spec.ticket);
        break;
      case "writableProperty":
        descriptor.get = (): never => refuse("property", member, spec.ticket);
        descriptor.set = (): void => {
          refuse("property", member, spec.ticket);
        };
        break;
    }
    Object.defineProperty(surface, name, descriptor);
  }
  return Object.freeze(surface) as UnsupportedSurface<T>;
}
