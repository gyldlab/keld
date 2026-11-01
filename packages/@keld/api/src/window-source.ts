/** Internal host-fact mirror (GH-531 §4.d); attached to the native facade link. */
import {
  WINDOW_CHANNEL,
  stateFreeze,
  stateDefineProperty,
  stateMap,
  stateMapGet,
  stateMapHas,
  stateMapSet,
  stateMapSize,
  stateMapValues,
  stateMapClear,
  linkError,
  type KeldCallError,
} from "@keld/kipc";
import type { MirrorSource } from "./mirror.ts";
import {
  decodeWindowEvent,
  validateDesktopEpoch,
  validateDesktopFacts,
  type DesktopFacts,
  type DesktopFact,
  type Rect,
  type WindowEvent,
  type WindowRef,
  type WindowState,
} from "./window-codec.ts";

/** Source-owned immutable host facts; Electron-facing tombstones are separate. */
export interface WindowSnapshot {
  readonly window: Readonly<WindowRef>;
  readonly state: Readonly<Omit<WindowState, "bounds">> & {
    readonly bounds: Readonly<Rect>;
  };
  readonly pending_close: bigint | null;
  readonly host_destroyed: boolean;
}

/** Owned immutable desktop observation; an absent value means unavailable. */
export interface DesktopSnapshot {
  readonly primary_id: number;
  readonly displays: readonly (Readonly<
    Omit<DesktopFact, "bounds" | "work_area">
  > & {
    readonly bounds: Readonly<Rect>;
    readonly work_area: Readonly<Rect>;
  })[];
  readonly should_use_dark_colors: boolean;
}

function desktopSnapshot(facts: DesktopFacts): DesktopSnapshot {
  // Never keep aliases from the decoded frame or a caller-owned direct fact.
  const displays: DesktopSnapshot["displays"][number][] = [];
  for (let index = 0; index < facts.displays.length; index += 1) {
    const display = facts.displays[index]!;
    stateDefineProperty(displays, index, {
      __proto__: null,
      value: stateFreeze({
        id: display.id,
        bounds: stateFreeze({
          x: display.bounds.x,
          y: display.bounds.y,
          width: display.bounds.width,
          height: display.bounds.height,
        }),
        work_area: stateFreeze({
          x: display.work_area.x,
          y: display.work_area.y,
          width: display.work_area.width,
          height: display.work_area.height,
        }),
        scale_factor: display.scale_factor,
      }),
      enumerable: true,
      configurable: true,
      writable: true,
    } as PropertyDescriptor);
  }
  return stateFreeze({
    primary_id: facts.primary_id,
    displays: stateFreeze(displays),
    should_use_dark_colors: facts.should_use_dark_colors,
  });
}

function sameRect(a: Readonly<Rect>, b: Readonly<Rect>): boolean {
  return (
    a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height
  );
}

function sameDesktop(
  a: DesktopSnapshot | undefined,
  b: DesktopFacts | null,
): boolean {
  if (b === null) return a === undefined;
  if (
    a === undefined ||
    a.primary_id !== b.primary_id ||
    a.should_use_dark_colors !== b.should_use_dark_colors ||
    a.displays.length !== b.displays.length
  )
    return false;
  for (let i = 0; i < b.displays.length; i += 1) {
    const left = a.displays[i]!;
    const right = b.displays[i]!;
    if (
      left.id !== right.id ||
      left.scale_factor !== right.scale_factor ||
      !sameRect(left.bounds, right.bounds) ||
      !sameRect(left.work_area, right.work_area)
    )
      return false;
  }
  return true;
}

/**
 * GH1149 review-only cumulative HOST-session admission proposal.
 * Must match the Rust registry's lifetime count after independent policy
 * approval; this cardinality alone is NOT a retained-heap byte budget.
 * Keep retired ID membership and never interpret this as a live-window quota.
 */
export const PROPOSED_MAX_CUMULATIVE_WINDOW_IDS = 1024;

function snapshot(
  window: WindowRef,
  state: WindowState,
  pending_close: bigint | null,
  host_destroyed: boolean,
): WindowSnapshot {
  // Copy every object: neither retained dispatch facts nor caller snapshots own
  // aliases into mutable map state, or into the transport's payload storage.
  return stateFreeze({
    window: stateFreeze({ id: window.id, incarnation: window.incarnation }),
    state: stateFreeze({
      bounds: stateFreeze({
        x: state.bounds.x,
        y: state.bounds.y,
        width: state.bounds.width,
        height: state.bounds.height,
      }),
      focused: state.focused,
      maximized: state.maximized,
      full_screen: state.full_screen,
    }),
    pending_close,
    host_destroyed,
  });
}

/**
 * No socket, identity minting, permission policy, optimistic write or event queue.
 * A largest decoded Created fact has four plain objects, fourteen own properties:
 * eleven scalar slots and three object references. Its separate mirror snapshot
 * has the same object/property/slot counts. All integers are wire-bounded; variant
 * strings are closed literals. These graphs retain no payload, buffer or view.
 * Diagnostics retain the first primitive code/full message, not an Error object;
 * their string backing and the callback's captures still need qualification.
 * These shapes and the proposed map-count bound are NOT a heap-byte budget.
 * Production attachment still requires independently qualified heap bytes,
 * callback/record aliases and lifetimes per approved GH1142/1149.
 */
export class WindowSource implements MirrorSource<WindowEvent> {
  readonly channel = WINDOW_CHANNEL;
  // Null keeps exact observed-ID membership and the historical census without
  // retaining the terminal snapshot's four-object graph after dispatch.
  readonly #windows = stateMap<number, WindowSnapshot | null>();
  #desktop: DesktopSnapshot | undefined;
  #desktopEpoch = 0n;
  #onFact: ((fact: WindowEvent) => void) | undefined;
  #failureCode: string | undefined;
  #failureMessage = "";

  constructor(onFact: (fact: WindowEvent) => void) {
    this.#onFact = onFact;
  }

  #diagnostic(code: string): KeldCallError {
    const error = linkError(code, "");
    // linkError accepts a detail and prepends code. Saved terminal messages are
    // already full messages, with no required prefix grammar; preserve exactly.
    error.message = this.#failureMessage;
    return error;
  }

  #requireLive(): void {
    if (this.#failureCode !== undefined)
      throw this.#diagnostic(this.#failureCode);
  }

  #reject(): KeldCallError {
    if (this.#failureCode !== undefined)
      return this.#diagnostic(this.#failureCode);
    const error = linkError(
      "KELD-IPC-005",
      "window mirror rejected a host fact. Check the host window pair and codec before restarting the role.",
    );
    this.#failureMessage = error.message;
    this.#failureCode = error.code;
    return error;
  }

  /** Decode once through the shared codec, freezing its bounded known graph. */
  decode(payload: Uint8Array): WindowEvent {
    this.#requireLive();
    try {
      const fact = decodeWindowEvent(payload);
      if (fact.variant === "DesktopSnapshot") {
        validateDesktopEpoch(fact.epoch);
        if (fact.snapshot !== null) {
          validateDesktopFacts(fact.snapshot);
          for (
            let index = 0;
            index < fact.snapshot.displays.length;
            index += 1
          ) {
            const display = fact.snapshot.displays[index]!;
            stateFreeze(display.bounds);
            stateFreeze(display.work_area);
            stateFreeze(display);
          }
          stateFreeze(fact.snapshot.displays);
          stateFreeze(fact.snapshot);
        }
      } else {
        stateFreeze(fact.window);
      }
      if (fact.variant === "Created") {
        stateFreeze(fact.state.bounds);
        stateFreeze(fact.state);
      } else if (fact.variant === "Bounds") {
        stateFreeze(fact.bounds);
      }
      return stateFreeze(fact);
    } catch {
      throw this.#reject();
    }
  }

  /** Apply only host-confirmed facts; Created first, exact pair, Destroyed last. */
  apply(fact: WindowEvent): void {
    this.#requireLive();
    try {
      if (fact.variant === "DesktopSnapshot") {
        validateDesktopEpoch(fact.epoch);
        if (fact.snapshot !== null) validateDesktopFacts(fact.snapshot);
        if (fact.epoch < this.#desktopEpoch) throw this.#reject();
        if (fact.epoch === this.#desktopEpoch) {
          if (!sameDesktop(this.#desktop, fact.snapshot)) throw this.#reject();
          return;
        }
        const next =
          fact.snapshot === null ? undefined : desktopSnapshot(fact.snapshot);
        this.#desktop = next;
        this.#desktopEpoch = fact.epoch;
        return;
      }
      if (fact.window.id === 0 || fact.window.incarnation === 0n)
        throw this.#reject();
      const previous = stateMapGet(this.#windows, fact.window.id);
      if (fact.variant === "Created") {
        if (
          stateMapHas(this.#windows, fact.window.id) ||
          stateMapSize(this.#windows) >= PROPOSED_MAX_CUMULATIVE_WINDOW_IDS
        )
          throw this.#reject();
        stateMapSet(
          this.#windows,
          fact.window.id,
          snapshot(fact.window, fact.state, fact.pending_close, false),
        );
        return;
      }
      if (
        !previous ||
        previous.window.incarnation !== fact.window.incarnation ||
        previous.host_destroyed
      )
        throw this.#reject();
      let state: WindowState = previous.state;
      let pending_close = previous.pending_close;
      let host_destroyed = false;
      switch (fact.variant) {
        case "Bounds":
          state = { ...state, bounds: fact.bounds };
          break;
        case "Focus":
          state = { ...state, focused: fact.focused };
          break;
        case "Maximized":
          state = { ...state, maximized: fact.maximized };
          break;
        case "FullScreen":
          state = { ...state, full_screen: fact.full_screen };
          break;
        case "CloseRequested":
          pending_close = fact.close_seq;
          break;
        case "Destroyed":
          pending_close = null;
          host_destroyed = true;
          break;
        default:
          throw this.#reject();
      }
      stateMapSet(
        this.#windows,
        fact.window.id,
        snapshot(fact.window, state, pending_close, host_destroyed),
      );
    } catch {
      throw this.#reject();
    }
  }

  /** A generation-owned read, zero transport traffic and no new mutable aliases. */
  desktop(): DesktopSnapshot | undefined {
    this.#requireLive();
    return this.#desktop;
  }

  /** Existing dispatch owns ordering and isolates listener exceptions. */
  dispatch(fact: WindowEvent): void {
    // Earlier applied records still drain after a later source fact fails.
    // After terminal end the transport has already drained earlier records:
    // never revive its former application callback/closure via a stale source.
    try {
      this.#onFact?.(fact);
    } finally {
      if (fact.variant === "Destroyed") {
        const current = stateMapGet(this.#windows, fact.window.id);
        if (
          current?.host_destroyed &&
          current.window.incarnation === fact.window.incarnation
        )
          stateMapSet(this.#windows, fact.window.id, null);
      }
    }
  }

  /**
   * Reflect an authenticated, correlated Host `CloseReply -> ReplyApplied`.
   * This is not an optimistic setter: the caller must have received that exact
   * Host response. A newer native close sequence wins; an old acknowledgement
   * can never clear a later `CloseRequested` or a different incarnation.
   */
  acceptHostCloseReply(window: WindowRef, closeSeq: bigint): boolean {
    // Bookkeeping follows an already received answer. A later Source failure
    // must not replace it or mutate either Source or the caller's receipt map.
    if (this.#failureCode !== undefined) return false;
    const previous = stateMapGet(this.#windows, window.id);
    if (
      !previous ||
      previous.window.incarnation !== window.incarnation ||
      previous.host_destroyed
    )
      return false;
    if (previous.pending_close === closeSeq)
      stateMapSet(
        this.#windows,
        window.id,
        snapshot(window, previous.state, null, false),
      );
    // An older ack still advances the dispatched receipt watermark; only the
    // exact pending sequence above may clear current Source state.
    return true;
  }

  /** A local read only; a wrong pair never observes a different incarnation. */
  get(window: WindowRef): WindowSnapshot | undefined {
    this.#requireLive();
    const value = stateMapGet(this.#windows, window.id);
    return value?.window.incarnation === window.incarnation ? value : undefined;
  }

  /** Immutable snapshots in host-id order; terminal ones remain until dispatch. */
  all(): readonly WindowSnapshot[] {
    this.#requireLive();
    return stateFreeze(
      stateMapValues(this.#windows)
        .filter((value): value is WindowSnapshot => value !== null)
        .sort((a, b) => a.window.id - b.window.id),
    );
  }

  /** Called by the future link owner only after its retained-record drain. */
  end(error: KeldCallError): void {
    if (this.#failureCode === undefined) {
      this.#failureMessage = error.message;
      this.#failureCode = error.code;
    }
    stateMapClear(this.#windows);
    this.#desktop = undefined;
    this.#desktopEpoch = 0n;
    // LifecycleLink still holds the terminated Source. It must not, in turn,
    // retain the captured application handler and its complete external graph.
    this.#onFact = undefined;
  }
}
