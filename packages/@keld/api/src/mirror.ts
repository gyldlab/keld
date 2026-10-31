/** Internal decode/apply/dispatch owner; not an application export (GH-531 §4.d). */
import {
  linkError,
  stateApply,
  stateSlots,
  type WorkerLink,
} from "../../kipc/src/transport.ts";

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
export function attachMirror<Fact>(
  link: WorkerLink,
  source: MirrorSource<Fact>,
): void {
  const channel = source.channel;
  const decode = source.decode;
  const apply = source.apply;
  const dispatch = source.dispatch;
  // Existing ring admission bounds pending + currently dispatched facts. These
  // private own slots never append/shift or expose storage to application code.
  const capacity = link.stateFactCapacity;
  const pending = stateSlots<Fact>(capacity);
  let head = 0;
  let count = 0;
  link.setStateApplier(channel, (payload) => {
    try {
      const fact = stateApply(decode, source, [payload]);
      stateApply(apply, source, [fact]);
      if (count >= capacity)
        throw linkError(
          "KELD-IPC-005",
          "mirror pending facts exceed retained ring records",
        );
      pending[(head + count) % capacity] = fact;
      count += 1;
    } catch {
      // WorkerLink owns first-terminal priority and earlier-answer delivery. Only
      // this source boundary is normalized; unrelated transport causes pass through.
      throw linkError(
        "KELD-IPC-005",
        "mirror decode or apply failed. Check the source codec and state contract before restarting the role.",
      );
    }
  });
  link.onEvent(channel, () => {
    if (count === 0) {
      throw linkError(
        "KELD-IPC-005",
        "mirror dispatch has no applied fact. Attach the mirror before subscribing to host facts.",
      );
    }
    // Clear before callbacks, including reentrant dispatch. Count owns presence;
    // undefined remains an exact Fact rather than an empty-slot sentinel.
    const fact = pending[head] as Fact;
    pending[head] = undefined;
    head = (head + 1) % capacity;
    count -= 1;
    stateApply(dispatch, source, [fact]);
  });
}
