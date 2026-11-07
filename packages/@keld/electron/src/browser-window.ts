/**
 * Electron-shaped first-window surface over the one authenticated Keld app link.
 * The Core registry is the sole owner of ids, native state and close decisions.
 * No renderer/Node grant, alternate transport, or optimistic native state exists.
 */
import { EventEmitter } from "node:events";
import {
  app as hostApp,
  captureOriginalWindowRecoveryDocument,
  flushAppliedLastWindowClosedAfterDestroy,
  getDispatchedPendingWindowCloses,
  getWindowSnapshot,
  getWindowSnapshots,
  invokeWindow,
  invokeWindowBlocking,
  sendCurrentWindowEvent,
  onWindowFactApplied,
} from "@keld/api/internal/app";
import type {
  Rect,
  WindowEvent,
  WindowRef,
  WindowRequest,
} from "@keld/api/internal/window-codec";
import {
  linkError,
  stateDefineProperty,
  stateFreeze,
  stateMap,
  stateMapDelete,
  stateMapGet,
  stateMapHas,
  stateMapSet,
  stateWeakMap,
  stateWeakMapGet,
  stateWeakMapSet,
} from "@keld/kipc";
import { emitNativeAppEvent, joinOriginalQuitBeforeClose } from "./app.ts";
import { encodeEndpointArguments } from "@keld/kipc/endpoint-value-codec";

/** Known draw.io preferences; other fields are reported, never made authoritative. */
export interface WebPreferences {
  preload?: string;
  additionalArguments?: readonly string[];
  webviewTag?: boolean;
  contextIsolation?: boolean;
  nodeIntegration?: boolean;
  nodeIntegrationInWorker?: boolean;
  nodeIntegrationInSubFrames?: boolean;
  webSecurity?: boolean;
  allowRunningInsecureContent?: boolean;
  disableBlinkFeatures?: string;
  spellcheck?: boolean;
  readonly [unknownField: string]: unknown;
}

export interface BrowserWindowOptions {
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  backgroundColor?: string;
  icon?: string;
  webPreferences?: WebPreferences;
  readonly [unknownField: string]: unknown;
}

/** A cancellable Host close request; consumers may veto, never forge its state. */
export interface CloseEvent {
  preventDefault(): void;
  readonly defaultPrevented: boolean;
}

/** The approved first-window event vocabulary (GH531 §4.f). */
export interface BrowserWindowEventMap {
  close: [event: CloseEvent];
  closed: [];
  move: [];
  maximize: [];
  unmaximize: [];
  resize: [];
  focus: [];
  blur: [];
  "enter-full-screen": [];
  "leave-full-screen": [];
}

/** The single Host-created content view's first-window events. */
export interface WebContentsEventMap {
  destroyed: [];
  "did-finish-load": [];
}

/** Public content lifetime surface; the Host alone owns native release. */
export interface WebContents {
  send(channel: string, ...args: unknown[]): void;
  /** Last actual native committed URL; empty before the first committed document. */
  getURL(): string;
  isDestroyed(): boolean;
  on<K extends keyof WebContentsEventMap>(
    event: K,
    listener: (...args: WebContentsEventMap[K]) => void,
  ): this;
  once<K extends keyof WebContentsEventMap>(
    event: K,
    listener: (...args: WebContentsEventMap[K]) => void,
  ): this;
  off<K extends keyof WebContentsEventMap>(
    event: K,
    listener: (...args: WebContentsEventMap[K]) => void,
  ): this;
}

/** Strict public listener signatures over the existing runtime EventEmitter. */
export interface BrowserWindow {
  on<K extends keyof BrowserWindowEventMap>(
    event: K,
    listener: (...args: BrowserWindowEventMap[K]) => void,
  ): this;
  once<K extends keyof BrowserWindowEventMap>(
    event: K,
    listener: (...args: BrowserWindowEventMap[K]) => void,
  ): this;
  off<K extends keyof BrowserWindowEventMap>(
    event: K,
    listener: (...args: BrowserWindowEventMap[K]) => void,
  ): this;
}

/** Only the Host's original identity pair can select a window. */
const windows = stateMap<number, BrowserWindow>();

function diagnostic(message: string): Error {
  return new Error(
    `KELD-COMPAT-010: ${message}. Use the Keld isolated webview defaults.`,
  );
}

function reportNativeCallbackError(message: string, error: unknown): void {
  // Application code can replace even console.error. A failed diagnostic may
  // never stop a Host close verdict or crash the deferred Destroy lifecycle.
  try {
    console.error(message, error);
  } catch {
    // Logging has no native authority.
  }
}

function checkOptions(options: BrowserWindowOptions): void {
  if (
    options === null ||
    typeof options !== "object" ||
    Array.isArray(options)
  ) {
    throw diagnostic("BrowserWindow options must be an object");
  }
  const prefs = options.webPreferences;
  if (prefs === undefined) return;
  if (prefs === null || typeof prefs !== "object" || Array.isArray(prefs)) {
    throw diagnostic("webPreferences must be an object");
  }
  for (const [name, unsafe, safe] of [
    ["nodeIntegration", true, false],
    ["nodeIntegrationInWorker", true, false],
    ["nodeIntegrationInSubFrames", true, false],
    ["contextIsolation", false, true],
    ["webSecurity", false, true],
    ["allowRunningInsecureContent", true, false],
  ] as const) {
    const value = prefs[name];
    if (value === undefined) continue;
    if (typeof value !== "boolean" || value === unsafe) {
      throw diagnostic(`webPreferences.${name} must be ${safe}`);
    }
  }
  if (prefs.webviewTag !== undefined && prefs.webviewTag !== false) {
    throw diagnostic("webPreferences.webviewTag must be false");
  }
}

/** Capture every used getter before Create; validation never rereads app objects. */
function captureWindowOptions(
  source: BrowserWindowOptions,
): BrowserWindowOptions {
  if (source === null || typeof source !== "object" || Array.isArray(source))
    throw diagnostic("BrowserWindow options must be an object");
  const captured: Record<string, unknown> = {};
  for (const name of [
    "width",
    "height",
    "x",
    "y",
    "backgroundColor",
    "icon",
  ] as const)
    stateDefineProperty(captured, name, {
      __proto__: null,
      value: source[name],
      enumerable: true,
    } as PropertyDescriptor);
  const original = source.webPreferences;
  if (original !== undefined) {
    if (
      original === null ||
      typeof original !== "object" ||
      Array.isArray(original)
    )
      throw diagnostic("webPreferences must be an object");
    const prefs: Record<string, unknown> = {};
    for (const name of [
      "preload",
      "webviewTag",
      "contextIsolation",
      "nodeIntegration",
      "nodeIntegrationInWorker",
      "nodeIntegrationInSubFrames",
      "webSecurity",
      "allowRunningInsecureContent",
      "disableBlinkFeatures",
      "spellcheck",
    ] as const) {
      stateDefineProperty(prefs, name, {
        __proto__: null,
        value: original[name],
        enumerable: true,
      } as PropertyDescriptor);
    }
    const args = original.additionalArguments;
    if (args !== undefined) {
      if (!Array.isArray(args))
        throw diagnostic(
          "additionalArguments requires at most sixteen strings",
        );
      const count = args.length;
      if (
        typeof count !== "number" ||
        count % 1 !== 0 ||
        count < 0 ||
        count > 16
      )
        throw diagnostic(
          "additionalArguments requires at most sixteen strings",
        );
      const copy: string[] = [];
      for (let index = 0; index < count; index += 1) {
        const value: unknown = args[index];
        if (typeof value !== "string")
          throw diagnostic("additionalArguments must contain strings");
        stateDefineProperty(copy, index, {
          __proto__: null,
          value,
          enumerable: true,
          writable: true,
          configurable: true,
        } as PropertyDescriptor);
      }
      stateDefineProperty(prefs, "additionalArguments", {
        __proto__: null,
        value: copy,
        enumerable: true,
      } as PropertyDescriptor);
    }
    stateDefineProperty(captured, "webPreferences", {
      __proto__: null,
      value: prefs,
      enumerable: true,
    } as PropertyDescriptor);
  }
  return captured as BrowserWindowOptions;
}

function ignoredOptionDiagnostics(options: BrowserWindowOptions): void {
  // Warnings are advisory only. Application code can replace console.warn
  // with a throwing function; it must never invalidate a committed native
  // Create or strand a live window without returning its constructor value.
  const warn = (message: string): void => {
    try {
      console.warn(message);
    } catch {
      // Diagnostic failure cannot grant or revoke Host window authority.
    }
  };
  for (const name of ["backgroundColor", "icon"] as const) {
    if (options[name] !== undefined) {
      warn(
        `KELD window: ${name} is accepted but not yet applied by the native adapter`,
      );
    }
  }
  for (const name of ["disableBlinkFeatures", "spellcheck"] as const) {
    if (options.webPreferences?.[name] !== undefined) {
      warn(
        `KELD window: webPreferences.${name} is accepted but not yet applied`,
      );
    }
  }
}

/** The first-proof content lifetime; loading and preload APIs are not faked. */
// The user-facing WebContents must never expose a method that can forge a
// native Host Destroyed tombstone. The sole release closure lives in this
// module-private WeakMap, not on the returned instance or its prototype.
const hostContentsRelease = stateWeakMap<
  BrowserWindowWebContents,
  () => void
>();

class BrowserWindowWebContents extends EventEmitter {
  #destroyed = false;
  readonly #windowRef: () => WindowRef;

  constructor(windowRef: () => WindowRef) {
    super();
    this.#windowRef = windowRef;
    stateWeakMapSet(hostContentsRelease, this, () =>
      this.#releaseAfterHostDestroyed(),
    );
  }

  isDestroyed(): boolean {
    return this.#destroyed;
  }
  send(channel: string, ...args: unknown[]): void {
    if (this.#destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    const payload = encodeEndpointArguments(args);
    // Capture the Host-selected document before yielding. Never substitute a
    // window id for a native webview id or retry an unknown native effect.
    const delivery = sendCurrentWindowEvent(
      this.#windowRef(),
      channel,
      payload,
    );
    void delivery.catch((error: unknown) =>
      reportNativeCallbackError("KELD webContents.send failed:", error),
    );
  }
  getURL(): string {
    if (this.#destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    const snapshot = getWindowSnapshot(this.#windowRef());
    if (!snapshot || snapshot.host_destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    return snapshot.document?.native_url ?? "";
  }

  /** Only the module-private Host release capability can flip this bit. */
  #releaseAfterHostDestroyed(): void {
    if (this.#destroyed) return;
    this.#destroyed = true;
    try {
      this.emit("destroyed");
    } catch (error) {
      // A JS listener throwing after a real native release must not swallow
      // BrowserWindow.closed or crash a deferred Destroy microtask.
      reportNativeCallbackError(
        "KELD webContents destroyed listener failed:",
        error,
      );
    }
  }
}

function releaseContentsAfterHostDestroyed(
  contents: BrowserWindowWebContents,
): void {
  const release = stateWeakMapGet(hostContentsRelease, contents);
  if (!release)
    throw diagnostic("Host WebContents release capability is missing");
  release();
}

/** Internal dialog parent extraction; deliberately absent from the package index. */
export let readDialogParentRef: (candidate: unknown) => WindowRef;
/** Internal exact Host pair lookup; absent from the public Electron entry. */
export let findIpcSenderContents: (ref: WindowRef) => WebContents | undefined;
/** Internal exact authenticated Veto receipt; absent from the public Electron entry. */
export let awaitOriginalIpcWindowCloseVeto: (
  window: WindowRef,
  current: () => boolean,
) => Promise<void>;
/** Internal conditional native teardown; its selector comes from the retained original event. */
export let destroyOriginalIpcWindowDocument: (
  request: Extract<WindowRequest, { variant: "DestroyOriginalDocument" }>,
  current: () => boolean,
) => Promise<void>;

/** @internal Consume only the automatic original startup load decision. */
export let consumeOriginalRecoveryStartup: (candidate: unknown) => boolean;

export class BrowserWindow extends EventEmitter {
  static {
    consumeOriginalRecoveryStartup = (candidate: unknown): boolean => {
      if (
        (typeof candidate !== "object" && typeof candidate !== "function") ||
        candidate === null ||
        !(#ref in candidate)
      )
        return false;
      const current = candidate.#originalRecoveryStartup;
      candidate.#originalRecoveryStartup = false;
      return (
        candidate.#recovered &&
        !candidate.#destroyed &&
        stateMapGet(windows, candidate.#ref.id) === candidate &&
        typeof current === "function" &&
        current()
      );
    };
    findIpcSenderContents = (ref: WindowRef): WebContents | undefined => {
      const admitted = getWindowSnapshot(ref);
      if (!admitted || admitted.host_destroyed) return undefined;
      let owner = stateMapGet(windows, ref.id);
      if (owner === undefined && hostApp.isReady()) {
        owner = BrowserWindow.getAllWindows().find(
          (window) => window.id === ref.id,
        );
      }
      if (
        owner === undefined ||
        owner.#ref.incarnation !== ref.incarnation ||
        owner.#destroyed
      )
        return undefined;
      const snapshot = getWindowSnapshot(ref);
      return snapshot && !snapshot.host_destroyed ? owner.#contents : undefined;
    };
    awaitOriginalIpcWindowCloseVeto = async (ref, current): Promise<void> => {
      const owner = stateMapGet(windows, ref.id);
      if (
        !owner ||
        owner.#ref.incarnation !== ref.incarnation ||
        owner.#destroyed ||
        !current()
      )
        throw linkError(
          "KELD-CORE-054",
          "original close has no current owning window",
        );
      for (;;) {
        if (!current() || owner.#destroyed)
          throw linkError(
            "KELD-CORE-054",
            "original document retired before close Veto settlement",
          );
        const receipt = owner.#lastVetoApplied;
        if (!receipt)
          throw linkError(
            "KELD-CORE-054",
            "original close has no authenticated Veto request",
          );
        await receipt.applied;
        if (!current() || owner.#destroyed)
          throw linkError(
            "KELD-CORE-054",
            "original document retired during close Veto settlement",
          );
        if (owner.#lastVetoApplied === receipt) return;
        // A different actual close attempt arrived while waiting. Await its
        // own correlated reply; do not resend or retry a failed operation.
      }
    };
    destroyOriginalIpcWindowDocument = async (
      request,
      current,
    ): Promise<void> => {
      const ref = request.window;
      const owner = stateMapGet(windows, ref.id);
      if (
        !owner ||
        owner.#ref.incarnation !== ref.incarnation ||
        owner.#destroyed ||
        !current()
      )
        throw linkError(
          "KELD-CORE-054",
          "original teardown has no current owning window",
        );
      await awaitOriginalIpcWindowCloseVeto(ref, current);
      if (!current())
        throw linkError(
          "KELD-CORE-054",
          "original document retired before conditional native teardown",
        );
      const response = await invokeWindow(request);
      // The genuine Destroyed dispatch may already have tombstoned this exact
      // owner and removed its Source record before the async reply resumes.
      const observedDestroyed =
        owner.#destroyed || getWindowSnapshot(ref)?.host_destroyed === true;
      if (response.variant !== "Destroyed" || !observedDestroyed)
        throw linkError(
          "KELD-IPC-003",
          "original teardown lacks its physical Destroyed fact and response",
        );
      // Native success retires the original document. Never demand current()
      // after success, nor replace the captured selector with a latest lookup.
      owner.#closed(false);
      flushAppliedLastWindowClosedAfterDestroy();
    };
    readDialogParentRef = (candidate: unknown): WindowRef => {
      if (
        (typeof candidate !== "object" && typeof candidate !== "function") ||
        candidate === null ||
        !(#ref in candidate)
      ) {
        throw new TypeError("dialog parent must be a live BrowserWindow");
      }
      candidate.#state();
      return candidate.#ref;
    };
  }

  // Only getAllWindows may enter this synchronous, callback-free constructor
  // path. Reading a private ref from caller options would let a Proxy forge
  // the otherwise secret Symbol and bypass the Host Create contract.
  static #replaying: WindowRef | undefined;
  // Dispatch-order barrier only: Host/mirror still own pending-close state.
  // A recovered Created(pending_close), or an ordinary CloseRequested before
  // Ready, must emit 'close' before any later window fact after Ready.
  static readonly #heldBeforeReadyCloses = stateMap<
    number,
    { readonly ref: WindowRef; readonly closeSeq: bigint }
  >();
  // A Created fact can dispatch before Ready while a later Bounds has already
  // been applied by the WorkerLink. Retain only the Host-published baseline,
  // never a locally invented/current authoritative window state.
  static readonly #receivedBounds = stateMap<
    number,
    { readonly incarnation: bigint; readonly bounds: Readonly<Rect> }
  >();
  static #terminalMirrorReported = false;
  readonly id!: number;
  readonly #ref!: WindowRef;
  readonly #contents = new BrowserWindowWebContents(() => this.#ref);
  #destroyed = false;
  #insideClosed = false;
  readonly #recovered!: boolean;
  // Undefined permits only an actual Adopted reply to arm a replay wrapper.
  // False is terminal: fresh creation, an ineligible decision, or any load.
  #originalRecoveryStartup: (() => boolean) | false | undefined;
  #lastCloseSeq: bigint | undefined;
  #lastVetoApplied:
    { readonly closeSeq: bigint; readonly applied: Promise<void> } | undefined;

  // Creation notifications belong to the Host-owned BrowserWindow wrapper,
  // not its WebContents. Mark before invoking reentrant application callbacks.
  #creationPublished = false;
  // Delta-only bookkeeping; public getters and CALLs use the Host mirror.
  #lastDispatchedBounds!: Readonly<Rect>;

  constructor(options: BrowserWindowOptions = {}) {
    super();
    // Only a successful new Created reply gives this constructor a bounded
    // cleanup obligation. Adopted/replayed documents already predate it, and
    // an IPC-022 Create outcome has no proven identity to destroy or retry.
    let created: WindowRef | undefined;
    let published = false;
    try {
      // Replay windows are materialized from trusted WindowSource records, not
      // from another Create request or a locally invented window identity.
      const replayed = BrowserWindow.#replaying;
      let adopted = false;
      let ref: WindowRef;
      if (replayed !== undefined) {
        ref = replayed;
      } else {
        options = captureWindowOptions(options);
        checkOptions(options);
        // Read *all* caller-controlled diagnostic getters before the irreversible
        // authenticated Host Create. A hostile getter cannot throw after the
        // Host returned Created and strand an unreturned native window.
        ignoredOptionDiagnostics(options);
        const geometry = {
          width: options.width ?? 800,
          height: options.height ?? 600,
          x: options.x ?? null,
          y: options.y ?? null,
        };
        const preload = options.webPreferences?.preload;
        const args = options.webPreferences?.additionalArguments;
        if (preload === undefined && args !== undefined && args.length !== 0)
          throw diagnostic(
            "additionalArguments requires an admitted preload selector",
          );
        if (
          preload !== undefined &&
          (typeof preload !== "string" || preload.length === 0)
        )
          throw diagnostic("preload must be a nonempty absolute selector");
        const request: WindowRequest =
          preload === undefined
            ? { variant: "Create", ...geometry }
            : {
                variant: "CreateWithPreload",
                ...geometry,
                preload_absolute: preload,
                additional_arguments: args ?? [],
              };
        const response = invokeWindowBlocking(request);
        if (response.variant !== "Created" && response.variant !== "Adopted") {
          throw diagnostic(
            `unexpected native Create response ${response.variant}`,
          );
        }
        ref = response.window;
        if (response.variant === "Created") created = ref;
        adopted = response.variant === "Adopted";
        const previous = stateMapGet(windows, ref.id);
        if (
          response.variant === "Adopted" &&
          previous &&
          previous.#ref.incarnation === ref.incarnation &&
          !previous.isDestroyed()
        ) {
          if (previous.#originalRecoveryStartup === undefined)
            previous.#originalRecoveryStartup =
              captureOriginalWindowRecoveryDocument(previous.#ref) ?? false;
          previous.#publishCreation();
          return previous;
        }
      }
      const confirmed = getWindowSnapshot(ref);
      if (!confirmed || confirmed.host_destroyed) {
        throw diagnostic(
          "native Create did not deliver a matching host-owned window fact",
        );
      }
      if (stateMapHas(windows, ref.id))
        throw diagnostic("Host reused an active window id unexpectedly");
      // TypeScript readonly is compile-time only. A writable public id can
      // redirect Map cleanup to a different Host window (including its listeners).
      stateDefineProperty(this, "id", {
        value: ref.id,
        enumerable: true,
        writable: false,
        configurable: false,
      });
      this.#ref = stateFreeze({ id: ref.id, incarnation: ref.incarnation });
      const delivered = stateMapGet(BrowserWindow.#receivedBounds, ref.id);
      this.#lastDispatchedBounds =
        delivered && delivered.incarnation === ref.incarnation
          ? delivered.bounds
          : confirmed.state.bounds;
      // An Adopted host window also predates this JS facade, even if application
      // code never called getAllWindows() to materialize its replay wrapper.
      this.#recovered = replayed !== undefined || adopted;
      this.#originalRecoveryStartup = adopted
        ? (captureOriginalWindowRecoveryDocument(this.#ref) ?? false)
        : replayed === undefined
          ? false
          : undefined;
      stateMapSet(windows, ref.id, this);
      published = true;
      if (replayed === undefined) this.#publishCreation();
    } catch (error) {
      if (created && !published && !stateMapHas(windows, created.id)) {
        try {
          const response = invokeWindowBlocking({
            variant: "Destroy",
            window: created,
          });
          if (
            response.variant !== "Destroyed" ||
            !getWindowSnapshot(created)?.host_destroyed
          ) {
            throw diagnostic(
              "failed constructor cleanup did not confirm native release",
            );
          }
        } catch (cleanupError) {
          throw new Error(
            "KELD-COMPAT-010: native Create succeeded, but wrapper setup and its Destroy cleanup failed. Native release is unconfirmed; do not retry Create. Inspect the Host generation and recover the surviving window.",
            { cause: { setup: error, cleanup: cleanupError } },
          );
        }
      }
      // Once published, the private registry owns the wrapper even if an
      // application creation callback throws. Never destroy that owned window.
      throw error;
    }
  }

  #publishCreation(): void {
    if (this.#creationPublished) return;
    // Mark before invoking application callbacks, including reentrant Create.
    this.#creationPublished = true;
    const event = { preventDefault(): void {} };
    emitNativeAppEvent("web-contents-created", event, this.#contents);
    emitNativeAppEvent("browser-window-created", event, this);
  }

  static getAllWindows(): BrowserWindow[] {
    const snapshots = getWindowSnapshots().filter(
      (value) => !value.host_destroyed,
    );
    const result = snapshots.map(({ window }) => {
      const previous = stateMapGet(windows, window.id);
      if (previous && previous.#ref.incarnation === window.incarnation)
        return previous;
      return BrowserWindow.#fromReplay(window);
    });
    BrowserWindow.#catchUpDispatchedCloseFromMirror();
    return result;
  }

  /** Reconcile only genuine mirror facts already applied before this module. */
  static #reconcilePendingSnapshot(
    ref: WindowRef,
    closeSeq: bigint | null,
  ): void {
    if (closeSeq === null) return;
    const previous = stateMapGet(windows, ref.id);
    if (
      previous &&
      previous.#ref.incarnation === ref.incarnation &&
      previous.#lastCloseSeq === closeSeq
    )
      return;
    const held = stateMapGet(BrowserWindow.#heldBeforeReadyCloses, ref.id);
    if (held?.ref.incarnation === ref.incarnation && held.closeSeq === closeSeq)
      return;
    BrowserWindow.#receiveNativeClose(ref, closeSeq, true);
  }

  static #catchUpDispatchedCloseFromMirror(): void {
    try {
      for (const receipt of getDispatchedPendingWindowCloses()) {
        // Applied facts may be ahead of the dispatch cursor during a blocking
        // CALL. Only a *dispatched* close receipt qualifies for catch-up.
        const snapshot = getWindowSnapshot(receipt.window);
        if (
          snapshot &&
          !snapshot.host_destroyed &&
          snapshot.pending_close === receipt.closeSeq
        ) {
          BrowserWindow.#reconcilePendingSnapshot(
            receipt.window,
            receipt.closeSeq,
          );
        }
      }
    } catch (error) {
      BrowserWindow.#reportDeadMirror(
        "KELD dispatched window close could not be read:",
        error,
      );
    }
  }

  static #reportDeadMirror(message: string, error: unknown): void {
    if (BrowserWindow.#terminalMirrorReported) return;
    BrowserWindow.#terminalMirrorReported = true;
    reportNativeCallbackError(message, error);
  }

  static #fromReplay(window: WindowRef): BrowserWindow {
    if (BrowserWindow.#replaying !== undefined) {
      throw diagnostic("recursive native window replay is not allowed");
    }
    BrowserWindow.#replaying = window;
    try {
      return new BrowserWindow();
    } finally {
      BrowserWindow.#replaying = undefined;
    }
  }

  static getFocusedWindow(): BrowserWindow | null {
    const snapshots = getWindowSnapshots();
    const focused = snapshots.find(
      (value) => !value.host_destroyed && value.state.focused,
    );
    return focused
      ? (BrowserWindow.getAllWindows().find(
          (win) => win.id === focused.window.id,
        ) ?? null)
      : null;
  }

  get webContents(): WebContents {
    if (this.#destroyed && !this.#insideClosed) {
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    }
    return this.#contents;
  }
  /** The existing window channel resolves only after the actual native Finished fact. */
  async loadURL(uri: string): Promise<void> {
    this.#originalRecoveryStartup = false;
    this.#state();
    const ref = this.#ref;
    const response = await invokeWindow({
      variant: "LoadUrl",
      window: ref,
      uri,
    });
    const current = getWindowSnapshot(ref);
    if (this.#destroyed || !current || current.host_destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    if (
      response.variant !== "Loaded" ||
      !current.document?.current ||
      !current.document.finished
    )
      throw diagnostic(
        "native LoadUrl settled without its current Finished fact",
      );
  }

  #state() {
    if (this.#destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    const value = getWindowSnapshot(this.#ref);
    if (!value || value.host_destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    return value.state;
  }

  /** Isolate each callback step inside one native event. A throwing `move`
   * cannot discard the same Bounds fact's `resize`, and a reentrant Destroy
   * suppresses notifications against an already-tombstoned window.
   */
  #emitNativeFact(event: string): void {
    if (this.#destroyed) return;
    try {
      this.emit(event);
    } catch (error) {
      reportNativeCallbackError(`KELD window ${event} callback failed:`, error);
    }
  }

  getSize(): number[] {
    const { width, height } = this.#state().bounds;
    return [width, height];
  }

  getPosition(): number[] {
    const { x, y } = this.#state().bounds;
    return [x, y];
  }

  isMaximized(): boolean {
    return this.#state().maximized;
  }
  isFullScreen(): boolean {
    return this.#state().full_screen;
  }
  isDestroyed(): boolean {
    return this.#destroyed;
  }

  #change(request: WindowRequest): void {
    if (this.#destroyed)
      throw new Error("KELD-COMPAT-011: Object has been destroyed");
    void invokeWindow(request).catch((error: unknown) => {
      reportNativeCallbackError("KELD native window request failed:", error);
    });
  }

  maximize(): void {
    this.#change({
      variant: "SetMaximized",
      window: this.#ref,
      maximized: true,
    });
  }

  setFullScreen(flag: boolean): void {
    if (typeof flag !== "boolean")
      throw diagnostic("setFullScreen expects a boolean");
    this.#change({
      variant: "SetFullScreen",
      window: this.#ref,
      full_screen: flag,
    });
  }

  /** A request only: the native window stays until the app's verdict permits closing. */
  close(): void {
    if (this.#destroyed) return;
    this.#change({ variant: "RequestClose", window: this.#ref });
  }

  /** Host confirms native release before this synchronous method reports closed. */
  destroy(): void {
    if (this.#destroyed) return;
    const response = invokeWindowBlocking({
      variant: "Destroy",
      window: this.#ref,
    });
    if (
      response.variant !== "Destroyed" ||
      !getWindowSnapshot(this.#ref)?.host_destroyed
    ) {
      throw diagnostic(
        "native Destroy did not confirm the actual window release",
      );
    }
    this.#closed(true);
    // One already-applied Host lifecycle milestone, after `closed` and before
    // the synchronous Electron destroy() returns (GH531 E1). The later ring
    // dispatch suppresses this exact consumed record without fabricating one.
    flushAppliedLastWindowClosedAfterDestroy();
  }

  #closed(fromDestroy: boolean): void {
    if (this.#destroyed) return;
    this.#destroyed = true;
    // The private Host pair, never a user-visible property, owns cleanup.
    if (stateMapGet(windows, this.#ref.id) === this)
      stateMapDelete(windows, this.#ref.id);
    if (!fromDestroy) releaseContentsAfterHostDestroyed(this.#contents);
    this.#insideClosed = true;
    try {
      this.emit("closed");
    } catch (error) {
      // A throwing application listener cannot interrupt physical release or
      // suppress the content tombstone; the host already destroyed the window.
      reportNativeCallbackError("KELD window closed callback failed:", error);
    } finally {
      this.#insideClosed = false;
      if (fromDestroy) {
        queueMicrotask(() => releaseContentsAfterHostDestroyed(this.#contents));
      }
    }
  }

  #nativeClose(close_seq: bigint): void {
    if (this.#destroyed || this.#lastCloseSeq === close_seq) return;
    this.#lastCloseSeq = close_seq;
    // The existing dispatch owner validated the actual Host pair/sequence.
    // Complete a qualified original handoff before a user listener can
    // synchronously destroy and enter the real last-window policy.
    joinOriginalQuitBeforeClose(this.#ref, close_seq);
    // A recovered wrapper may be materialized by a native fact before app code
    // has acquired it or registered its unsaved-document listener. Veto rather
    // than allowing an invisible/default close through that recovery interval.
    let prevented = this.#recovered && this.listenerCount("close") === 0;
    const event = {
      get defaultPrevented(): boolean {
        return prevented;
      },
      preventDefault(): void {
        prevented = true;
      },
    };
    try {
      this.emit("close", event);
    } catch (error) {
      // A throwing application callback must not cause data loss or wedge ClosePending.
      prevented = true;
      reportNativeCallbackError(
        "KELD window close callback failed; vetoed:",
        error,
      );
    }
    // The application may synchronously call win.destroy() from its `close`
    // callback. Native Released then wins and retires the pending sequence;
    // no delayed Allow/Veto for a physically destroyed Host window is legal.
    if (this.#destroyed) return;
    try {
      const snapshot = getWindowSnapshot(this.#ref);
      if (
        !snapshot ||
        snapshot.host_destroyed ||
        snapshot.pending_close !== close_seq
      )
        return;
    } catch (error) {
      BrowserWindow.#reportDeadMirror(
        "KELD close callback lost its Host generation:",
        error,
      );
      return;
    }
    const requested = invokeWindow({
      variant: "CloseReply",
      window: this.#ref,
      close_seq,
      verdict: prevented ? "Veto" : "Allow",
    });
    if (prevented) {
      const applied = requested.then((response) => {
        if (response.variant !== "ReplyApplied")
          throw linkError(
            "KELD-CORE-054",
            "original close Veto was not applied to its exact request",
          );
      });
      this.#lastVetoApplied = { closeSeq: close_seq, applied };
      void applied.catch((error: unknown) => {
        reportNativeCallbackError(
          "KELD native close Veto settlement failed:",
          error,
        );
      });
    } else {
      void requested.catch((error: unknown) => {
        reportNativeCallbackError("KELD native close verdict failed:", error);
      });
    }
  }

  /** Host facts are applied before this method; never mint a local identity. */
  static #dispatchNativeClose(ref: WindowRef, close_seq: bigint): void {
    let confirmed;
    try {
      confirmed = getWindowSnapshot(ref);
    } catch (error) {
      // The role/link has ended, so a late scheduled delivery has no legal
      // successor owner and must never send an Allow on its dead generation.
      BrowserWindow.#reportDeadMirror(
        "KELD close verdict abandoned after Host mirror failure:",
        error,
      );
      return;
    }
    if (
      !confirmed ||
      confirmed.host_destroyed ||
      confirmed.pending_close !== close_seq
    ) {
      return;
    }
    const previous = stateMapGet(windows, ref.id);
    // A different incarnation is an ownership mismatch, not permission to
    // replace the caller's JS listeners with a new, unrelated wrapper.
    if (previous && previous.#ref.incarnation !== ref.incarnation) return;
    const window = previous ?? BrowserWindow.#fromReplay(ref);
    window.#nativeClose(close_seq);
  }

  static #flushHeldBeforeReadyClose(ref: WindowRef): void {
    if (!hostApp.isReady()) return;
    const held = stateMapGet(BrowserWindow.#heldBeforeReadyCloses, ref.id);
    if (!held || held.ref.incarnation !== ref.incarnation) return;
    stateMapDelete(BrowserWindow.#heldBeforeReadyCloses, ref.id);
    BrowserWindow.#dispatchNativeClose(held.ref, held.closeSeq);
  }

  /** Replay can arrive before public Ready; give the successor its Ready turn. */
  static #receiveNativeClose(
    ref: WindowRef,
    close_seq: bigint,
    fromReplay: boolean,
  ): void {
    const ready = hostApp.isReady();
    const mustHold = fromReplay || !ready;
    if (mustHold) {
      stateMapSet(BrowserWindow.#heldBeforeReadyCloses, ref.id, {
        ref: { id: ref.id, incarnation: ref.incarnation },
        closeSeq: close_seq,
      });
    }
    if (!fromReplay && ready) {
      BrowserWindow.#flushHeldBeforeReadyClose(ref);
      BrowserWindow.#dispatchNativeClose(ref, close_seq);
      return;
    }
    void hostApp.whenReady().then(
      () => {
        // Ready continuations can attach a close/veto listener to the adopted
        // window before the gap's held close is delivered. A later native fact
        // flushes it synchronously first, preserving the Host's fact order.
        setImmediate(() => {
          try {
            if (mustHold) BrowserWindow.#flushHeldBeforeReadyClose(ref);
            else {
              BrowserWindow.#flushHeldBeforeReadyClose(ref);
              BrowserWindow.#dispatchNativeClose(ref, close_seq);
            }
          } catch (error) {
            // Expected shutdowns are handled by #dispatchNativeClose; do not
            // let an independent late callback crash the event loop.
            reportNativeCallbackError(
              "KELD deferred native close dispatch failed:",
              error,
            );
          }
        });
      },
      (error: unknown) => {
        // A dead link ends the Host's generation, so no stale Allow is sent.
        if (mustHold) {
          const held = stateMapGet(
            BrowserWindow.#heldBeforeReadyCloses,
            ref.id,
          );
          if (
            held &&
            held.ref.incarnation === ref.incarnation &&
            held.closeSeq === close_seq
          ) {
            stateMapDelete(BrowserWindow.#heldBeforeReadyCloses, ref.id);
          }
        }
        reportNativeCallbackError(
          "KELD recovery close lost its Host Ready barrier:",
          error,
        );
      },
    );
  }

  static {
    onWindowFactApplied(
      (fact: WindowEvent) => {
        // This is a global desktop fact. Close/Bounds listeners below own only
        // real native WindowRef pairs; do not fabricate an id or flush a close.
        if (
          fact.variant === "DesktopSnapshot" ||
          fact.variant === "DocumentRecovered"
        )
          return;
        if (fact.variant === "Created") {
          stateMapSet(BrowserWindow.#receivedBounds, fact.window.id, {
            incarnation: fact.window.incarnation,
            bounds: fact.state.bounds,
          });
        }
        if (fact.variant === "Created" && fact.pending_close !== null) {
          BrowserWindow.#receiveNativeClose(
            fact.window,
            fact.pending_close,
            true,
          );
          return;
        }
        // An already-applied Bounds/Focus/FullScreen/Destroyed fact may dispatch
        // before the deferred Ready continuation for an earlier replay Created.
        // Emit that replay close first, never reverse the host's causal order.
        BrowserWindow.#flushHeldBeforeReadyClose(fact.window);
        if (fact.variant === "CloseRequested") {
          BrowserWindow.#receiveNativeClose(fact.window, fact.close_seq, false);
          return;
        }
        if (fact.variant === "Destroyed") {
          stateMapDelete(BrowserWindow.#receivedBounds, fact.window.id);
        }
        if (fact.variant === "Bounds") {
          const earlier = stateMapGet(
            BrowserWindow.#receivedBounds,
            fact.window.id,
          );
          if (earlier?.incarnation === fact.window.incarnation) {
            // The role may receive Bounds before it has materialized a facade
            // wrapper. That dispatched Host value is the later wrapper's
            // baseline; a subsequent move cannot fabricate a resize of it.
            stateMapSet(BrowserWindow.#receivedBounds, fact.window.id, {
              incarnation: fact.window.incarnation,
              bounds: fact.bounds,
            });
          }
        }
        const window = stateMapGet(windows, fact.window.id);
        if (!window || window.#ref.incarnation !== fact.window.incarnation)
          return;
        switch (fact.variant) {
          case "NavigationFinished": {
            const current = getWindowSnapshot(window.#ref);
            if (
              !window.#destroyed &&
              current?.document?.current &&
              current.document.generation === fact.document_generation &&
              current.document.finished
            ) {
              try {
                window.#contents.emit("did-finish-load");
              } catch (error) {
                reportNativeCallbackError("KELD load listener failed:", error);
              }
            }
            break;
          }
          case "Bounds":
            {
              const prior = window.#lastDispatchedBounds;
              window.#lastDispatchedBounds = fact.bounds;
              stateMapSet(BrowserWindow.#receivedBounds, fact.window.id, {
                incarnation: fact.window.incarnation,
                bounds: fact.bounds,
              });
              if (prior.x !== fact.bounds.x || prior.y !== fact.bounds.y) {
                window.#emitNativeFact("move");
              }
              if (
                !window.#destroyed &&
                (prior.width !== fact.bounds.width ||
                  prior.height !== fact.bounds.height)
              ) {
                window.#emitNativeFact("resize");
              }
            }
            break;
          case "Focus":
            window.#emitNativeFact(fact.focused ? "focus" : "blur");
            break;
          case "Maximized":
            window.#emitNativeFact(fact.maximized ? "maximize" : "unmaximize");
            break;
          case "FullScreen":
            window.#emitNativeFact(
              fact.full_screen ? "enter-full-screen" : "leave-full-screen",
            );
            break;
          case "Destroyed":
            window.#closed(false);
            break;
          default:
            break;
        }
      },
      () => BrowserWindow.#catchUpDispatchedCloseFromMirror(),
    );
  }
}
