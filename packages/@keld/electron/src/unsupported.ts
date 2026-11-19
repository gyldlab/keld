/** Canonical GH565 refusal for a linked Electron member. */
import {
  stateDefineProperty,
  stateErrorConstructor,
  stateFreeze,
  stateHasOwn,
  stateIsNativeError,
  stateObjectEntries,
  stateOwnDescriptor,
} from "@keld/kipc";

export const UNSUPPORTED_MEMBER_CODE = "KELD-COMPAT-013";
export type TrackingIssue = `GH-${number}`;
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

function issueNumber(ticket: TrackingIssue): string {
  let number = "";
  for (let index = 3; index < ticket.length; index++) number += ticket[index];
  return number;
}

function defineOwnValue(target: object, name: string, value: unknown): void {
  const descriptor = {
    __proto__: null,
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  };
  stateDefineProperty(target, name, descriptor);
}

/** An Electron member whose native effect is outside the supported slice. */
export class UnsupportedMemberError extends stateErrorConstructor {
  declare readonly code: typeof UNSUPPORTED_MEMBER_CODE;
  declare readonly member: string;
  declare readonly ticket: TrackingIssue;
  declare readonly kind: UnsupportedKind;

  constructor(
    kind: UnsupportedKind,
    member: string,
    ticket: TrackingIssue,
    detail?: string,
  ) {
    super(
      `${UNSUPPORTED_MEMBER_CODE}: ${describe(kind, member)} is not implemented by @keld/electron yet; it is tracked on gyldlab/keld#${issueNumber(ticket)}. Follow that issue for the first supported release and guard or remove this call site until then: the binding exists so the module links, and a silent stub is forbidden.` +
        (detail === undefined ? "" : ` ${detail}`),
    );
    defineOwnValue(this, "name", "UnsupportedMemberError");
    defineOwnValue(this, "code", UNSUPPORTED_MEMBER_CODE);
    defineOwnValue(this, "member", member);
    defineOwnValue(this, "ticket", ticket);
    defineOwnValue(this, "kind", kind);
  }
}

function ownErrorValue(error: Error, key: string): unknown {
  const descriptor = stateOwnDescriptor(error, key);
  return descriptor && stateHasOwn(descriptor, "value")
    ? descriptor.value
    : undefined;
}

/** Recognize the registered fields across instances of the SDK. */
export function isUnsupportedMemberError(
  value: unknown,
): value is UnsupportedMemberError {
  if (!stateIsNativeError(value)) return false;
  return (
    ownErrorValue(value, "code") === UNSUPPORTED_MEMBER_CODE &&
    typeof ownErrorValue(value, "member") === "string" &&
    typeof ownErrorValue(value, "ticket") === "string"
  );
}

export function refuse(
  kind: Exclude<UnsupportedKind, "promise">,
  member: string,
  ticket: TrackingIssue,
): never {
  throw new UnsupportedMemberError(kind, member, ticket);
}

export async function refuseLater(
  member: string,
  ticket: TrackingIssue,
): Promise<never> {
  throw new UnsupportedMemberError("promise", member, ticket);
}

export type MemberKind = "method" | "promise" | "property" | "writableProperty";

export interface UnsupportedMember {
  readonly kind: MemberKind;
  readonly ticket: TrackingIssue;
}

export function method<T extends TrackingIssue>(ticket: T) {
  return { kind: "method", ticket } as const;
}

export function promise<T extends TrackingIssue>(ticket: T) {
  return { kind: "promise", ticket } as const;
}

export function property<T extends TrackingIssue>(ticket: T) {
  return { kind: "property", ticket } as const;
}

export function writableProperty<T extends TrackingIssue>(ticket: T) {
  return { kind: "writableProperty", ticket } as const;
}

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
  const descriptor = { __proto__: null, value: name, configurable: true };
  stateDefineProperty(fn, "name", descriptor);
  return fn;
}

/** Build one frozen module object with the documented refusal shapes. */
export function unsupportedSurface<T extends Record<string, UnsupportedMember>>(
  binding: string,
  members: T,
): UnsupportedSurface<T> {
  const surface: Record<string, unknown> = {};
  const entries = stateObjectEntries<UnsupportedMember>(members);
  for (let index = 0; index < entries.length; index++) {
    const entry = entries[index]!;
    const name = entry[0];
    const { kind, ticket } = entry[1];
    const member = `${binding}.${name}`;
    const descriptor = {
      __proto__: null,
      enumerable: true,
    } as PropertyDescriptor;
    switch (kind) {
      case "method":
        descriptor.value = named(name, () => refuse("method", member, ticket));
        break;
      case "promise":
        descriptor.value = named(name, () => refuseLater(member, ticket));
        break;
      case "property":
        descriptor.get = () => refuse("property", member, ticket);
        break;
      case "writableProperty":
        descriptor.get = () => refuse("property", member, ticket);
        descriptor.set = () => refuse("property", member, ticket);
        break;
    }
    stateDefineProperty(surface, name, descriptor);
  }
  return stateFreeze(surface) as UnsupportedSurface<T>;
}
