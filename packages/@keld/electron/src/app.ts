/**
 * Electron app compatibility facade over @keld/api (KEL-72 / KEL-142).
 *
 * Generic lifecycle/session ownership lives in @keld/api. This file owns only
 * Electron event names, listener behavior, and the default quit on
 * window-all-closed when no user listener is registered.
 */
import { app as apiApp } from "../../api/src/app.ts";

type AppListener = () => void;

const listeners = new Map<string, AppListener[]>();
let readyEventEmitted = false;
let readyObservation: Promise<void> | undefined;

function emit(event: string): void {
  const snapshot = listeners.get(event);
  if (!snapshot) return;
  for (const listener of snapshot.slice()) {
    try {
      listener();
    } catch {
      // A user callback must not break lifecycle delivery.
    }
  }
}

function ignoreIfUnawaited(promise: Promise<unknown>): void {
  void promise.catch(() => {});
}

function observeReady(): Promise<void> {
  if (readyEventEmitted) return Promise.resolve();
  if (readyObservation) return readyObservation;
  const ready = apiApp.whenReady();
  readyObservation = ready;
  ignoreIfUnawaited(ready);
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
  const list = listeners.get(event);
  return list !== undefined && list.length > 0;
}

function removeAppListener(event: string, listener: AppListener): void {
  const list = listeners.get(event);
  if (!list) return;
  for (let i = list.length - 1; i >= 0; i -= 1) {
    if (list[i] === listener) {
      list.splice(i, 1);
      break;
    }
  }
  if (list.length === 0) listeners.delete(event);
}

function onLastWindowClosed(): void {
  if (hasListeners("window-all-closed")) {
    emit("window-all-closed");
    return;
  }
  const quitting = apiApp.quit();
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
    ignoreIfUnawaited(observeReady());
    return apiApp.quit();
  },

  on(event: string, listener: AppListener): void {
    const list = listeners.get(event) ?? [];
    list.push(listener);
    listeners.set(event, list);
    if (event === "ready") ignoreIfUnawaited(observeReady());
  },

  removeListener: removeAppListener,
  off: removeAppListener,
};
