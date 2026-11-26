/**
 * Electron app compatibility facade over @keld/api (KEL-72 / KEL-142).
 *
 * Generic lifecycle/session ownership lives in @keld/api. This file owns only
 * Electron event names, listener behavior, the default quit on
 * window-all-closed when no user listener is registered, and the Electron
 * member names of the GH-452 session facts (`./app-session-facts.ts` owns their
 * fail-closed contract).
 */
import {
  app as apiApp,
  captureOriginalWindowQuitScope,
  type OriginalWindowQuitScope,
  getWindowSnapshots,
  hasObservedHostReady,
  observeWindowCreateActivity,
  onWindowFactApplied,
  onNativeQuitRequested,
} from "@keld/api/internal/app";

import {
  nativeFacadeBootSelected,
  linkError,
  stateApply,
  stateDefineProperty,
  stateMap,
  stateMapDelete,
  stateMapGet,
  stateMapSet,
} from "@keld/kipc";

import {
  defineSessionFactAccessors,
  readSingleInstanceVerdict,
  readSessionFact,
  readSessionPath,
} from "./app-session-facts.ts";

import { UnsupportedMemberError } from "./unsupported.ts";
import type {
  WindowRef,
  WindowResponse,
} from "@keld/api/internal/window-codec";

type AppListener = (...args: never[]) => void;

const listeners = stateMap<string, AppListener[]>();
// The canonical Host-preloaded transport captures the launch selection before
// application dependencies. Ready/Subscribe and Host ownership still authorize
// window access; this immutable hint only selects the fail-closed Quit policy.
let readyEventEmitted = false;
let readyObservation: Promise<void> | undefined;

function emit(event: string, ...args: unknown[]): boolean {
  const snapshot = stateMapGet(listeners, event);
  if (!snapshot) return false;
  // Snapshot listener membership before user callbacks can add or remove it.
  // Application code may replace Array.prototype.slice or Reflect.apply.
  const callbacks: AppListener[] = [];
  for (let index = 0; index < snapshot.length; index += 1)
    stateDefineProperty(callbacks, index, {
      __proto__: null,
      value: snapshot[index]!,
      writable: true,
      enumerable: true,
      configurable: true,
    } as PropertyDescriptor);
  let listenerFailed = false;
  for (let index = 0; index < callbacks.length; index += 1) {
    try {
      stateApply(
        callbacks[index]! as (...args: unknown[]) => void,
        undefined,
        args,
      );
    } catch {
      listenerFailed = true;
      // A user callback must not break lifecycle delivery.
    }
  }
  return listenerFailed;
}

/** Listener failures have the same canceling effect as preventDefault. */
function emitCancelable(eventName: string): boolean {
  let prevented = false;
  const event = {
    get defaultPrevented(): boolean {
      return prevented;
    },
    preventDefault(): void {
      prevented = true;
    },
  };
  // A listener cannot replace the next listener's veto method or spoof the
  // veto-state getter. Never use application-mutable global intrinsics here.
  stateDefineProperty(event, "preventDefault", {
    __proto__: null,
    writable: false,
    configurable: false,
  } as PropertyDescriptor);
  stateDefineProperty(event, "defaultPrevented", {
    __proto__: null,
    configurable: false,
  } as PropertyDescriptor);
  return emit(eventName, event) || prevented;
}

/** Electron event names and application listeners have this single owner. */
export function emitNativeAppEvent(event: string, ...args: unknown[]): void {
  emit(event, ...args);
}

function ignoreIfUnawaited(promise: Promise<unknown>): void {
  void stateApply(trustedQuitPromiseThen, promise, [undefined, () => {}]);
}

function observeReady(): Promise<void> {
  if (readyEventEmitted) return Promise.resolve();
  if (readyObservation) return readyObservation;
  // The API has already consumed the one-shot Host Ready when it is true
  // here. An observer installed later cannot retroactively emit Electron
  // `ready`, even though whenReady() must remain fulfilled.
  const hostWasReady = hasObservedHostReady();
  const ready = apiApp.whenReady();
  readyObservation = ready;
  ignoreIfUnawaited(ready);
  if (hostWasReady) {
    readyEventEmitted = true;
    return ready;
  }
  void ready.then(
    () => {
      if (readyEventEmitted) return;
      readyEventEmitted = true;
      emit("ready");
    },
    () => {
      if (readyObservation === ready) readyObservation = undefined;
    },
  );
  return ready;
}

function hasListeners(event: string): boolean {
  const list = stateMapGet(listeners, event);
  return list !== undefined && list.length > 0;
}

/** Read the authenticated mirror without mutable Array prototype methods. */
function liveWindowSnapshots() {
  const snapshots = getWindowSnapshots();
  const live: Array<(typeof snapshots)[number]> = [];
  for (let index = 0; index < snapshots.length; index += 1) {
    const window = snapshots[index];
    if (!window)
      throw new Error("KELD-COMPAT-012: native window census is incomplete");
    if (!window.host_destroyed)
      stateDefineProperty(live, live.length, {
        __proto__: null,
        value: window,
        writable: true,
        enumerable: true,
        configurable: true,
      } as PropertyDescriptor);
  }
  return live;
}

function removeAppListener(event: string, listener: AppListener): void {
  const list = stateMapGet(listeners, event);
  if (!list) return;
  for (let i = list.length - 1; i >= 0; i -= 1) {
    if (list[i] === listener) {
      // Preserve each remaining listener in order using the captured intrinsic.
      // Inherited Array.prototype.splice is application-mutable and may erase a
      // different listener's veto. Array length is our own data property.
      for (let slot = i; slot + 1 < list.length; slot += 1)
        stateDefineProperty(list, slot, {
          __proto__: null,
          value: list[slot + 1],
          writable: true,
          enumerable: true,
          configurable: true,
        } as PropertyDescriptor);
      list.length -= 1;
      break;
    }
  }
  if (list.length === 0) stateMapDelete(listeners, event);
}

// A `window-all-closed` listener that does not quit keeps the app alive, except at a
// Keld profile handover (an in-session update), where the host closes the window,
// delivers this event and ends the application session 10 s later if it has not
// requested quit by then (KEL-53 §4 "Profile handover", the quit deadline D).
function onLastWindowClosed(): void {
  const original = pendingOriginalQuitClose;
  if (original) {
    // The actual Host LastWindowClosed, with no surviving native window, is
    // the only suppression receipt. A delayed G1 event cannot consume G2.
    if (!original.scope.ownsCurrentGeneration()) {
      pendingOriginalQuitClose = undefined;
      // The API proved this LastWindowClosed belongs to its current link.
      // A stale original handoff cannot swallow the successor's event.
    } else {
      if (!apiApp.isReady()) return;
      try {
        const live = liveWindowSnapshots();
        if (live.length !== 0) {
          // An early Host callback while the original is still alive proves
          // nothing. Once only a replacement remains, this Quit's suppression
          // has no authority over that replacement's later final close.
          let originalLive = false;
          for (let i = 0; i < live.length; i += 1) {
            const current = live[i];
            if (
              current &&
              sameOriginalWindow(current.window, original.window)
            ) {
              originalLive = true;
            }
          }
          if (!originalLive) pendingOriginalQuitClose = undefined;
          return;
        }
      } catch {
        return;
      }
      pendingOriginalQuitClose = undefined;
      return;
    }
  }
  // Only an admitted Quit suppresses this notification. A `before-quit`
  // listener can close the final window synchronously and then veto Quit;
  // suppressing at preflight entry would lose its `window-all-closed` event.
  if (activeQuitAttempt?.closing) return;
  // GH531's Host wrote LastWindowClosed when the registry reached zero, but
  // a synchronous `closed` listener may construct a replacement BrowserWindow
  // before that event reaches this Bun role. Electron's native window list
  // checks the *current* membership after closed callbacks. Recheck the
  // authenticated Host mirror before notifying user `window-all-closed`
  // listeners as well as before the default quit. An unready/retired Source is
  // unknown, not proof of zero windows; fail closed instead of fabricating
  // application-wide closure while a dirty replacement may still be live.
  if (nativeFacadeBootSelected) {
    if (!apiApp.isReady()) return;
    try {
      if (liveWindowSnapshots().length !== 0) return;
    } catch {
      return;
    }
  }
  if (hasListeners("window-all-closed")) {
    emit("window-all-closed");
    return;
  }
  // A synchronous `closed` listener can create a replacement window after
  // the Host wrote LastWindowClosed but before this queued notification is
  // delivered. Recheck the *current* mirror using the same guarded facade
  // Quit; direct API Quit would destroy that replacement without veto.
  const quitting = app.quit();
  ignoreIfUnawaited(quitting);
}

// One facade attempt owns both D2 coalescing and the bounded original handoff.
interface QuitAttempt {
  readonly promise: Promise<void>;
  readonly resolve: () => void;
  readonly reject: (error: unknown) => void;
  settled: boolean;
  closing: boolean;
  handoff?: OriginalWindowQuitScope;
}
// The facade's private Host replies must not settle through an application
// replacement of Promise.prototype.then after the framework has loaded.
const trustedQuitPromiseThen = Promise.prototype.then;
let activeQuitAttempt: QuitAttempt | undefined;

// The nonterminal original close handoff settles its JS Promise at an
// authenticated CloseRequested, but that is neither a native Destroyed nor
// a Veto. Keep one session-bound suppression owner until the actual outcome.
interface OriginalQuitClose {
  readonly attempt: QuitAttempt;
  readonly scope: OriginalWindowQuitScope;
  readonly window: Readonly<WindowRef>;
  closeSeq: bigint | null;
}
let pendingOriginalQuitClose: OriginalQuitClose | undefined;

function sameOriginalWindow(
  left: Readonly<WindowRef>,
  right: Readonly<WindowRef>,
): boolean {
  return left.id === right.id && left.incarnation === right.incarnation;
}

/** @internal The existing close owner calls only after Host ReplyApplied(Veto). */
export function noteOriginalQuitVetoApplied(
  window: WindowRef,
  closeSeq: bigint,
): void {
  const claim = pendingOriginalQuitClose;
  if (
    claim &&
    sameOriginalWindow(window, claim.window) &&
    claim.closeSeq === closeSeq &&
    claim.scope.ownsCurrentGeneration()
  )
    pendingOriginalQuitClose = undefined;
}

/** @internal The existing BrowserWindow owner calls after actual Host Destroyed. */
export function noteOriginalQuitWindowDestroyed(window: WindowRef): void {
  const claim = pendingOriginalQuitClose;
  if (!claim) return;
  if (!claim.scope.ownsCurrentGeneration()) {
    pendingOriginalQuitClose = undefined;
    return;
  }
  try {
    const live = liveWindowSnapshots();
    if (sameOriginalWindow(window, claim.window)) {
      // An original window's Destroyed with a surviving replacement cannot
      // produce LastWindowClosed. Do not suppress the replacement's later close.
      if (live.length !== 0) pendingOriginalQuitClose = undefined;
      return;
    }
    // A different window was destroyed. Retain the claim only while its
    // original native window is still a member of this exact generation.
    let originalLive = false;
    for (let i = 0; i < live.length; i += 1) {
      const current = live[i];
      if (current && sameOriginalWindow(current.window, claim.window))
        originalLive = true;
    }
    if (!originalLive) pendingOriginalQuitClose = undefined;
  } catch {
    // An unproven Source cannot release a pending Quit suppression.
  }
}

function fulfillQuitAttempt(attempt: QuitAttempt): void {
  if (attempt.settled) return;
  attempt.settled = true;
  attempt.resolve();
  if (activeQuitAttempt === attempt) activeQuitAttempt = undefined;
}

/** @internal Called only in the existing authenticated native-close path, before user listeners. */
export function joinOriginalQuitBeforeClose(
  window: WindowRef,
  closeSeq: bigint,
): boolean {
  const claim = pendingOriginalQuitClose;
  if (claim && sameOriginalWindow(window, claim.window)) {
    if (
      !claim.scope.ownsCurrentGeneration() ||
      (claim.closeSeq !== null && claim.closeSeq !== closeSeq)
    ) {
      // The prior close was vetoed or retired; a later user close owns
      // its own LastWindowClosed notification.
      pendingOriginalQuitClose = undefined;
    } else if (claim.scope.isCurrentPending(window, closeSeq)) {
      claim.closeSeq = closeSeq;
    }
  }
  const attempt = activeQuitAttempt;
  if (!attempt?.handoff?.isCurrentPending(window, closeSeq)) return false;
  fulfillQuitAttempt(attempt);
  return true;
}

function requestQuitAfterBeforeEvent(
  terminal: (attempt: QuitAttempt) => Promise<void>,
): Promise<void> {
  if (activeQuitAttempt) return activeQuitAttempt.promise;
  const stillClosing = pendingOriginalQuitClose;
  if (stillClosing) {
    if (stillClosing.scope.ownsCurrentGeneration())
      return stillClosing.attempt.promise;
    pendingOriginalQuitClose = undefined;
  }
  let resolveAttempt!: () => void;
  let rejectAttempt!: (error: unknown) => void;
  const promise = new Promise<void>((resolve, reject) => {
    resolveAttempt = resolve;
    rejectAttempt = reject;
  });
  const attempt: QuitAttempt = {
    promise,
    resolve: resolveAttempt,
    reject: rejectAttempt,
    settled: false,
    closing: false,
  };
  activeQuitAttempt = attempt;
  if (emitCancelable("before-quit")) {
    fulfillQuitAttempt(attempt);
    return promise;
  }
  // GH531 criterion 34: suppress LastWindowClosed while the facade Quit
  // attempt is active, including a reentrant will-quit listener.
  attempt.closing = true;
  const failed = (error: unknown): void => {
    if (pendingOriginalQuitClose?.attempt === attempt)
      pendingOriginalQuitClose = undefined;
    if (attempt.settled) {
      // This request may fail after its genuine close fact already completed
      // the nonterminal handoff. A fresh active attempt must not be cleared.
      attempt.handoff?.failAfterHandoff(
        error instanceof Error ? error : new Error(String(error)),
      );
      return;
    }
    attempt.settled = true;
    attempt.reject(error);
    if (activeQuitAttempt === attempt) activeQuitAttempt = undefined;
  };
  try {
    // Same-turn terminal admission remains essential for the zero-window path.
    void stateApply(trustedQuitPromiseThen, terminal(attempt), [
      () => fulfillQuitAttempt(attempt),
      failed,
    ]);
  } catch (error) {
    failed(error);
  }
  return promise;
}

/** Role-local Create observations only refuse a Quit; they never authorize one. */
function createRacedWithQuit(epochAtStart: bigint): boolean {
  const activity = observeWindowCreateActivity();
  return activity.pending !== 0 || activity.epoch !== epochAtStart;
}

function quit(): Promise<void> {
  // Capture before user `before-quit` handlers: they may send an asynchronous
  // Create that Core already admits as Opening before its Source publishes it.
  const createEpochAtStart = nativeFacadeBootSelected
    ? observeWindowCreateActivity().epoch
    : undefined;
  return requestQuitAfterBeforeEvent((attempt) => {
    // GH451's full cancelable before-quit/window-close/will-quit choreography
    // has not passed its independent spec gate. Never translate Electron
    // app.quit() into the Host's *forceful* teardown while a native document
    // window still exists: that would bypass close.preventDefault() and may
    // discard unsaved edits. A typed refusal is safer than false compatibility.
    // During a native facade recovery/Subscribe, the mirror deliberately
    // returns [] until the authenticated Ready barrier. That means 'unknown',
    // not 'no live windows'. Refuse a pre-Ready facade Quit too. Legacy
    // renderer boots keep their existing Quit contract unchanged.
    if (nativeFacadeBootSelected && !apiApp.isReady()) {
      return Promise.reject(
        new Error(
          "KELD-COMPAT-012: native facade Quit is not authorized before Host Ready; the current window census has not been authenticated",
        ),
      );
    }
    if (
      createEpochAtStart !== undefined &&
      createRacedWithQuit(createEpochAtStart)
    ) {
      return Promise.reject(
        new Error(
          "KELD-COMPAT-012: a native window Create remains pending or was submitted during Quit preflight; retry after its Host result and close policy settle",
        ),
      );
    }
    try {
      const live = liveWindowSnapshots();
      if (live.length === 1) {
        const scope = captureOriginalWindowQuitScope(live[0]!.window);
        if (scope) {
          attempt.handoff = scope;
          pendingOriginalQuitClose = {
            attempt,
            scope,
            window: scope.window,
            closeSeq: null,
          };
          const pending = scope.dispatchedPendingClose();
          if (
            pending !== null &&
            joinOriginalQuitBeforeClose(scope.window, pending)
          )
            return Promise.resolve();
          return stateApply(trustedQuitPromiseThen, scope.requestClose(), [
            (reply: WindowResponse) => {
              if (
                reply.variant !== "CloseRequested" &&
                reply.variant !== "Merged" &&
                reply.variant !== "AlreadyClosing"
              )
                throw linkError(
                  "KELD-IPC-003",
                  "original single-window Quit handoff received an invalid close response",
                );
              if (
                reply.variant === "AlreadyClosing" &&
                pendingOriginalQuitClose?.attempt === attempt &&
                pendingOriginalQuitClose.closeSeq === null
              ) {
                throw linkError(
                  "KELD-CORE-054",
                  "original Quit cannot bind an already-closing window without its dispatched close sequence",
                );
              }
              if (reply.variant !== "AlreadyClosing") {
                const claim = pendingOriginalQuitClose;
                if (claim?.attempt === attempt && claim.closeSeq === null)
                  claim.closeSeq = reply.close_seq;
              }
            },
          ]) as Promise<void>;
        }
      }
      if (live.length !== 0) {
        return Promise.reject(
          new Error(
            "KELD-COMPAT-012: app.quit() with live native windows is not yet a cancelable Electron Quit. Close each window with its veto policy before retrying; GH451 owns the full quit choreography.",
          ),
        );
      }
    } catch (error) {
      return Promise.reject(error);
    }
    // The Host remains alive on will-quit veto, including after a previous
    // close removed every window. A later app.quit() must start a fresh D2
    // attempt and emit before-quit/will-quit again.
    if (emitCancelable("will-quit")) {
      // A listener may already have queued a retry microtask. Retire D2
      // synchronously after the veto, before that retry runs; no Host Quit
      // was admitted and the next call must receive a fresh before-quit.
      fulfillQuitAttempt(attempt);
      return Promise.resolve();
    }
    // A synchronous will-quit listener may construct another native window.
    // The prior zero-window census is no authority to force-close that window.
    if (
      nativeFacadeBootSelected &&
      (!apiApp.isReady() ||
        liveWindowSnapshots().length !== 0 ||
        (createEpochAtStart !== undefined &&
          createRacedWithQuit(createEpochAtStart)))
    ) {
      return Promise.reject(
        new Error(
          "KELD-COMPAT-012: a window appeared or Create was submitted during will-quit; retry through its cancelable close path",
        ),
      );
    }
    ignoreIfUnawaited(observeReady());
    // Host admission does not provide an Electron Event or a final exit code.
    // Keep the public "quit" event unsupported until both facts are available.
    return apiApp.quit();
  });
}

apiApp.onLastWindowClosed(onLastWindowClosed);
onWindowFactApplied((fact) => {
  const claim = pendingOriginalQuitClose;
  if (!claim || fact.variant === "DesktopSnapshot") return;
  // This callback observes the actual Source AFTER a Host fact is applied
  // but BEFORE a Destroyed tombstone is removed. A new document generation
  // in the same native WindowRef revokes the earlier Quit suppression.
  if (
    sameOriginalWindow(fact.window, claim.window) &&
    !claim.scope.ownsCurrentGeneration()
  )
    pendingOriginalQuitClose = undefined;
});
onNativeQuitRequested(() => {
  // Same framework Quit owner/D2 preflight, never a caller MenuItem click or
  // a forceful Host terminal request while the original document is live.
  void stateApply(trustedQuitPromiseThen, quit(), [
    undefined,
    (error: unknown) => {
      try {
        console.error(
          "KELD native menu Quit failed; inspect the current original role/link:",
          error,
        );
      } catch {
        /* Diagnostic replacement cannot retry, force Quit or change the actual cause. */
      }
    },
  ]);
});
// `name` and `isPackaged` are defined below as non-enumerable accessors, as
// Electron defines them; literal getters would be enumerable and make a spread
// or JSON.stringify of `app` throw.
export const app = defineSessionFactAccessors({
  /** GH453 no-argument subset; the Host acquired before this role ran. */
  requestSingleInstanceLock(...args: readonly unknown[]): boolean {
    if (args.length > 1 || args[0] !== undefined) {
      throw new UnsupportedMemberError(
        "method",
        "app.requestSingleInstanceLock(additionalData)",
        "GH-453",
      );
    }
    return readSingleInstanceVerdict();
  },

  whenReady(): Promise<void> {
    return observeReady();
  },

  isReady(): boolean {
    return apiApp.isReady();
  },

  quit,

  on(event: string, listener: AppListener): void {
    // A late registration must not join a queued facade Ready notification
    // if the underlying API already observed the Host event. Pre-Ready
    // listeners remain enrolled in the existing one-shot observer.
    if (event === "ready" && hasObservedHostReady()) return;
    const list = stateMapGet(listeners, event) ?? [];
    stateDefineProperty(list, list.length, {
      __proto__: null,
      value: listener,
      writable: true,
      enumerable: true,
      configurable: true,
    } as PropertyDescriptor);
    stateMapSet(listeners, event, list);
    if (event === "ready") ignoreIfUnawaited(observeReady());
  },

  removeListener: removeAppListener,
  off: removeAppListener,

  // GH-452 session facts (docs/specs/gh452-app-session-facts.md, T1). The Host
  // owns every value; until it delivers the payload each read fails closed with
  // KELD-COMPAT-040 naming the missing fact. Nothing here reads the environment.
  getPath(name: string): string {
    return readSessionPath(name);
  },

  getVersion(): string {
    return readSessionFact("version", "app.getVersion()");
  },

  getName(): string {
    return readSessionFact("name", "app.getName()");
  },

  getAppPath(): string {
    return readSessionFact("appPath", "app.getAppPath()");
  },

  /** Legal only after `ready` in Electron; without a payload it fails closed at any time. */
  getLocale(): string {
    return readSessionFact("locale", "app.getLocale()");
  },
});
