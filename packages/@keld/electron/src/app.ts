/**
 * Electron app compatibility facade over @keld/api (KEL-72 / KEL-142).
 *
 * Generic lifecycle/session ownership lives in @keld/api. This file owns only
 * Electron event names, listener behavior, and the default quit on
 * window-all-closed when no user listener is registered.
 */
import {
  app as apiApp,
  getWindowSnapshots,
  hasObservedHostReady,
} from "../../api/src/app.ts";

import {
  nativeFacadeBootSelected,
  stateMap,
  stateMapDelete,
  stateMapGet,
  stateMapSet,
} from "../../kipc/src/transport.ts";

type AppListener = (...args: never[]) => void;

const listeners = stateMap<string, AppListener[]>();
// The canonical Host-preloaded transport captures the launch selection before
// application dependencies. Ready/Subscribe and Host ownership still authorize
// window access; this immutable hint only selects the fail-closed Quit policy.
let readyEventEmitted = false;
let readyObservation: Promise<void> | undefined;

function emit(event: string, ...args: unknown[]): void {
  const snapshot = stateMapGet(listeners, event);
  if (!snapshot) return;
  for (const listener of snapshot.slice()) {
    try {
      Reflect.apply(listener, undefined, args);
    } catch {
      // A user callback must not break lifecycle delivery.
    }
  }
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

apiApp.onLastWindowClosed(onLastWindowClosed);
export const app = {
  whenReady(): Promise<void> {
    return observeReady();
  },

  isReady(): boolean {
    return apiApp.isReady();
  },

  quit(): Promise<void> {
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
      if (getWindowSnapshots().some((window) => !window.host_destroyed)) {
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
  },

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
};
