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
  onNativeQuitRequested,
} from "@keld/api/internal/app";

import {
  nativeFacadeBootSelected,
  linkError,
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
import type { WindowRef } from "@keld/api/internal/window-codec";

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
  let listenerFailed = false;
  for (const listener of snapshot.slice()) {
    try {
      Reflect.apply(listener, undefined, args);
    } catch {
      listenerFailed = true;
      // A user callback must not break lifecycle delivery.
    }
  }
  return listenerFailed;
}

/** Electron event names and application listeners have this single owner. */
export function emitNativeAppEvent(event: string, ...args: unknown[]): void {
  emit(event, ...args);
}

function ignoreIfUnawaited(promise: Promise<unknown>): void {
  void promise.catch(() => {});
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

function removeAppListener(event: string, listener: AppListener): void {
  const list = stateMapGet(listeners, event);
  if (!list) return;
  for (let i = list.length - 1; i >= 0; i -= 1) {
    if (list[i] === listener) {
      list.splice(i, 1);
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
      if (getWindowSnapshots().some((window) => !window.host_destroyed)) return;
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
  handoff?: OriginalWindowQuitScope;
}
let activeQuitAttempt: QuitAttempt | undefined;

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
  const attempt = activeQuitAttempt;
  if (!attempt?.handoff?.isCurrentPending(window, closeSeq)) return false;
  fulfillQuitAttempt(attempt);
  return true;
}

function requestQuitAfterBeforeEvent(
  terminal: (attempt: QuitAttempt) => Promise<void>,
): Promise<void> {
  if (activeQuitAttempt) return activeQuitAttempt.promise;
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
  };
  activeQuitAttempt = attempt;
  let prevented = false;
  const event = {
    get defaultPrevented(): boolean {
      return prevented;
    },
    preventDefault(): void {
      prevented = true;
    },
  };
  const listenerFailed = emit("before-quit", event);
  if (prevented || listenerFailed) {
    fulfillQuitAttempt(attempt);
    return promise;
  }
  const failed = (error: unknown): void => {
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
    void terminal(attempt).then(() => fulfillQuitAttempt(attempt), failed);
  } catch (error) {
    failed(error);
  }
  return promise;
}

function quit(): Promise<void> {
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
    try {
      const live = getWindowSnapshots().filter(
        (window) => !window.host_destroyed,
      );
      if (live.length === 1) {
        const scope = captureOriginalWindowQuitScope(live[0]!.window);
        if (scope) {
          attempt.handoff = scope;
          const pending = scope.dispatchedPendingClose();
          if (
            pending !== null &&
            joinOriginalQuitBeforeClose(scope.window, pending)
          )
            return Promise.resolve();
          return scope.requestClose().then((reply) => {
            if (
              reply.variant !== "CloseRequested" &&
              reply.variant !== "Merged" &&
              reply.variant !== "AlreadyClosing"
            )
              throw linkError(
                "KELD-IPC-003",
                "original single-window Quit handoff received an invalid close response",
              );
          });
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
    ignoreIfUnawaited(observeReady());
    return apiApp.quit();
  });
}

apiApp.onLastWindowClosed(onLastWindowClosed);
onNativeQuitRequested(() => {
  // Same framework Quit owner/D2 preflight, never a caller MenuItem click or
  // a forceful Host terminal request while the original document is live.
  void quit().catch((error: unknown) => {
    try {
      console.error(
        "KELD native menu Quit failed; inspect the current original role/link:",
        error,
      );
    } catch {
      /* Diagnostic replacement cannot retry, force Quit or change the actual cause. */
    }
  });
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
    list.push(listener);
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
