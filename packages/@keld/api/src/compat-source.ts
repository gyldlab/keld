/** Native-fact observation on the existing shared link; never native authority. */
import {
  linkError,
  stateMap,
  stateMapGet,
  stateMapSet,
  stateMapClear,
  stateMapSize,
  stateMapValues,
  stateFreeze,
} from "@keld/kipc";
import {
  decodeCompatControlEvent,
  type CompatControlEvent,
  type CompatControlRequest,
  type ConsumerDisposition,
} from "@keld/kipc/compat-control";
import type { MirrorSource } from "./mirror.ts";
import {
  WindowSource,
  PROPOSED_MAX_CUMULATIVE_WINDOW_IDS,
} from "./window-source.ts";
import type { WindowRef } from "./window-codec.ts";

type RendererMessage = Extract<
  CompatControlEvent,
  { variant: "RendererMessage" }
>;
type DocumentObservation = {
  webview: number;
  windowId: number;
  incarnation: bigint;
  navigation: number;
  lastDispatchedMessage: number;
};
/** One bounded decoded ring fact, including its endpoint-only consumer outcome. */
export interface CompatFact {
  readonly event: CompatControlEvent;
  disposition?: ConsumerDisposition;
  released?: boolean;
}
export type CompatConsumer = (
  event: CompatControlEvent,
  current: () => boolean,
) => ConsumerDisposition;

/**
 * Observes only authenticated Host facts. It mints no navigation/role counter,
 * credential or grant. Final native/world/role/Guard admission stays in Host.
 * One scalar record per native view is bounded by the existing window census.
 */
export class CompatSource implements MirrorSource<CompatFact> {
  readonly #windows: WindowSource;
  readonly #documents = stateMap<number, DocumentObservation>();
  #consumer: CompatConsumer | undefined;
  #release: ((request: CompatControlRequest) => void) | undefined;
  #ended = false;
  constructor(
    readonly channel: number,
    windows: WindowSource,
    consumer: CompatConsumer,
    release: (request: CompatControlRequest) => void,
  ) {
    this.#windows = windows;
    this.#consumer = consumer;
    this.#release = release;
  }
  decode(payload: Uint8Array): CompatFact {
    return { event: decodeCompatControlEvent(payload) };
  }
  apply(fact: CompatFact): void {
    if (this.#ended) return;
    const event = fact.event;
    const previous = stateMapGet(this.#documents, event.webview);
    if (event.variant === "DocumentRetired") {
      if (previous)
        this.#windows.retireNativeDocument(
          {
            id: previous.windowId,
            incarnation: previous.incarnation,
          },
          event.navigation,
        );
      return;
    }
    const window = this.#windows.get({
      id: event.window_id,
      incarnation: event.window_incarnation,
    });
    if (!window || window.host_destroyed || !window.document)
      throw linkError(
        "KELD-IPC-005",
        "compat Host fact has no matching committed WindowRef",
      );
    if (event.navigation < window.document.generation) return;
    if (
      event.navigation !== window.document.generation ||
      (event.variant === "RendererMessage" &&
        event.url !== window.document.native_url)
    )
      throw linkError(
        "KELD-IPC-005",
        "compat Host fact differs from native committed document",
      );
    if (previous) {
      if (
        previous.windowId !== event.window_id ||
        previous.incarnation !== event.window_incarnation
      )
        throw linkError(
          "KELD-IPC-005",
          "compat native view changed its WindowRef",
        );
      // These are comparisons of native-owned values, never a JS epoch mint.
      if (event.navigation <= previous.navigation) return;
      previous.navigation = event.navigation;
      previous.lastDispatchedMessage = 0;
    } else {
      if (stateMapSize(this.#documents) >= PROPOSED_MAX_CUMULATIVE_WINDOW_IDS)
        throw linkError(
          "KELD-IPC-005",
          "compat source exceeds the existing window census",
        );
      stateMapSet(this.#documents, event.webview, {
        webview: event.webview,
        windowId: event.window_id,
        incarnation: event.window_incarnation,
        navigation: event.navigation,
        lastDispatchedMessage: 0,
      });
    }
  }
  /** Local observation only; the Host must independently recheck before an effect. */
  isCurrent(webview: number, navigation: number): boolean {
    if (this.#ended) return false;
    const record = stateMapGet(this.#documents, webview);
    if (!record || record.navigation !== navigation) return false;
    try {
      const window = this.#windows.get({
        id: record.windowId,
        incarnation: record.incarnation,
      });
      return (
        window !== undefined &&
        !window.host_destroyed &&
        window.document?.current === true &&
        window.document.generation === navigation
      );
    } catch {
      return false;
    }
  }
  /** Current renderer target from native facts only, with no guessed view ID.
   * The same bounded native-view ledger is the sole owner of this lookup.
   * A missing or ambiguous binding cannot authorize main-to-renderer delivery.
   */
  captureForWindow(
    ref: Readonly<WindowRef>,
  ):
    | Readonly<{ webview: number; navigation: number; current(): boolean }>
    | undefined {
    if (this.#ended) return undefined;
    let selected: DocumentObservation | undefined;
    for (const record of stateMapValues(this.#documents)) {
      if (
        record.windowId !== ref.id ||
        record.incarnation !== ref.incarnation ||
        !this.isCurrent(record.webview, record.navigation)
      )
        continue;
      if (selected) return undefined;
      selected = record;
    }
    if (!selected) return undefined;
    const { webview, navigation, windowId, incarnation } = selected;
    const current = (): boolean => {
      // Recheck unique ownership after async handoff. A second current native
      // view for this window cannot inherit the already-captured delivery.
      const unique = this.captureForWindow({ id: windowId, incarnation });
      return unique?.webview === webview && unique.navigation === navigation;
    };
    return stateFreeze({ webview, navigation, current });
  }
  dispatch(fact: CompatFact): void {
    const event = fact.event;
    // Binding and retirement are Host metadata, not renderer DATA. They never
    // invoke an application consumer or advance its message sequence.
    if (event.variant !== "RendererMessage") return;
    const record = stateMapGet(this.#documents, event.webview);
    if (
      !this.isCurrent(event.webview, event.navigation) ||
      !record ||
      event.message <= record.lastDispatchedMessage
    ) {
      fact.disposition = { variant: "NoListener" };
      return;
    }
    record.lastDispatchedMessage = event.message;
    const webview = event.webview,
      navigation = event.navigation;
    try {
      fact.disposition = this.#consumer?.(event, () =>
        this.isCurrent(webview, navigation),
      ) ?? { variant: "NoListener" };
    } catch {
      fact.disposition = {
        variant: "Refused",
        error: {
          code: "KELD-IPC-005",
          message: "application main listener threw during dispatch",
        },
      };
    }
  }
  /** Called by the shared ring only after both physical read counters advance. */
  released(fact: CompatFact): void {
    if (fact.released) return;
    fact.released = true;
    if (this.#ended || fact.event.variant !== "RendererMessage") return;
    const event = fact.event;
    this.#release?.({
      variant: "ReleaseConsumed",
      webview: event.webview,
      navigation: event.navigation,
      message: event.message,
      disposition: fact.disposition ?? { variant: "NoListener" },
    });
  }
  end(): void {
    this.#ended = true;
    stateMapClear(this.#documents);
    this.#consumer = undefined;
    this.#release = undefined;
  }
}
