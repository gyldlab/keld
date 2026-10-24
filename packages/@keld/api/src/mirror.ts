/** Internal decode/apply/dispatch owner; not an application export (GH-531 §4.d). */
import { linkError, type WorkerLink } from "../../kipc/src/transport.ts";

/** Framework-owned facts; each production source requires its own heap/alias review. */
export interface MirrorSource<Fact> {
  /** An admitted host-event channel from the generated channel constants. */
  readonly channel: number;
  /** Decodes once in the state phase; unknown or malformed facts throw. */
  decode(payload: Uint8Array): Fact;
  /** Synchronous mirror mutation only, with no application callbacks. */
  apply(fact: Fact): void;
  /** Application callbacks, invoked only by the existing event dispatcher. */
  dispatch(fact: Fact): void;
}

/**
 * Install before requesting the host subscription/replay (GH-531 criterion 20).
 * The link retains these captured methods and their source receiver. Successfully
 * applied facts alone are retained, one per corresponding still-retained event;
 * the ring remains the sole frame admission, ordering and release owner.
 * Decoded heap bytes and aliases require source-specific qualification before use.
 */
export function attachMirror<Fact>(link: WorkerLink, source: MirrorSource<Fact>): void {
  const channel = source.channel;
  const decode = source.decode.bind(source);
  const apply = source.apply.bind(source);
  const dispatch = source.dispatch.bind(source);
  // Native shift removes the consumed reference without an ever-growing head index
  // or history. No released payload copy is retained, and no independent limit or
  // listener schedule is introduced. Undefined is a valid Fact, not an empty slot.
  const pending: Fact[] = [];
  link.setStateApplier(channel, (payload) => {
    try {
      const fact = decode(payload);
      apply(fact);
      pending.push(fact);
    } catch {
      // WorkerLink owns first-terminal priority and earlier-answer delivery. Only
      // this source boundary is normalized; unrelated transport causes pass through.
      throw linkError("KELD-IPC-005", "mirror decode or apply failed. Check the source codec and state contract before restarting the role.");
    }
  });
  link.onEvent(channel, () => {
    if (pending.length === 0) {
      throw linkError("KELD-IPC-005", "mirror dispatch has no applied fact. Attach the mirror before subscribing to host facts.");
    }
    // Presence is established by length, so an undefined Fact remains exact.
    const fact = pending.shift() as Fact;
    dispatch(fact);
  });
}
