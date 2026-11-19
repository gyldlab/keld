/** Canonical GH565 refusal for a linked Electron member. */
import { stateDefineProperty, stateErrorConstructor } from "@keld/kipc";

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
