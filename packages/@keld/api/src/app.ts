/**
 * Backend-independent application lifecycle over the canonical app-link.
 *
 * This package owns the singleton lifecycle session. Compatibility layers
 * such as @keld/electron delegate to this object instead of opening another
 * socket or reader.
 */
import { dispatchApplicationCall } from "./channels.ts";
import { LifecycleLink } from "./link.ts";
import {
  linkError,
  stateMap,
  stateMapDelete,
  stateMapSet,
  stateMapValues,
} from "@keld/kipc";
import type {
  WindowEvent,
  WindowRef,
  WindowRequest,
  WindowResponse,
} from "./window-codec.ts";
import type { WindowSnapshot } from "./window-source.ts";
import { WindowCloseReceipts } from "./window-close-receipts.ts";

export type Unsubscribe = () => void;

export interface AppLifecycle {
  whenReady(): Promise<void>;
  isReady(): boolean;
  onLastWindowClosed(listener: () => void): Unsubscribe;
  quit(): Promise<void>;
}

type ReadyWaiter = {
  resolve: () => void;
  reject: (err: Error) => void;
};

let hostReady = false;
let nativeReadyQueued = false;

/** An authenticated raw Host Ready has arrived, even before public Ready. */
export function hasObservedHostReady(): boolean {
  return hostReady || nativeReadyQueued;
}
let linkDead: Error | undefined;
let readyWaiters: ReadyWaiter[] = [];
/**
 * The role's one link attempt (GH-527 §4.2: a realm opens one link). Never
 * reset: a failed connect, or a link that died before Ready, stays the answer,
 * so every later `whenReady` or `quit` rethrows that same typed error instead
 * of retrying into `KELD-IPC-005`.
 */
let linkPromise: Promise<LifecycleLink> | undefined;
let connectedLink: LifecycleLink | undefined;

let nextListenerId = 1;
const lastWindowClosedListeners = stateMap<number, () => void>();
const windowFactListeners = new Set<(fact: WindowEvent) => void>();
const deliveredWindowCloses = new WindowCloseReceipts();
const readyWindowCatchUps = new Set<() => void>();

function onWindowFact(fact: WindowEvent): void {
  deliveredWindowCloses.observe(fact);
  for (const listener of [...windowFactListeners]) {
    try {
      listener(fact);
    } catch (error) {
      // One application's event callback cannot stop later Host facts.
      // Even console.error can be replaced by app code: its failure cannot
      // escape the authenticated WorkerLink drain and kill the Bun role.
      try {
        console.error("KELD window listener error:", error);
      } catch {
        // Diagnostic delivery has no Host authority or lifecycle effect.
      }
    }
  }
}

/** Only delivered Host close records, never merely mirror-applied frames. */
export function getDispatchedPendingWindowCloses(): ReturnType<
  WindowCloseReceipts["pending"]
> {
  return hostReady ? deliveredWindowCloses.pending() : [];
}
function onHostReady(): void {
  if (hostReady || nativeReadyQueued || linkDead) return;
  nativeReadyQueued = true;
  // The canonical WorkerLink can drain Ready followed by Bounds/Focus in one
  // synchronous task. Delay only *public* Ready publication by one microtask,
  // never its genuine Host Init source: these later facts remain pre-Ready and
  // cannot flush a recovered close before application whenReady continuations.
  // WindowSource applies every record in-order in the existing bounded ring;
  // no extra fact queue, timer/polling, or second dispatch authority is added.
  queueMicrotask(() => {
    nativeReadyQueued = false;
    if (hostReady || linkDead) return;
    hostReady = true;
    const waiters = readyWaiters;
    readyWaiters = [];
    for (const waiter of waiters) waiter.resolve();
    // Late Electron imports get a one-shot authenticated mirror catch-up.
    for (const catchUp of [...readyWindowCatchUps]) {
      setImmediate(() => safelyCatchUpWindow(catchUp));
    }
  });
}

function safelyCatchUpWindow(catchUp: () => void): void {
  if (!hostReady || linkDead !== undefined) return;
  try {
    catchUp();
  } catch (error) {
    try {
      console.error("KELD authenticated window catch-up failed:", error);
    } catch {
      // User logging must never crash the source/Host lifecycle.
    }
  }
}

function onLastWindowClosed(): void {
  const snapshot = stateMapValues(lastWindowClosedListeners);
  for (let index = 0; index < snapshot.length; index += 1) {
    const listener = snapshot[index]!;
    try {
      listener();
    } catch {
      // Host events share the app-link read loop; isolate user callbacks.
    }
  }
}
function failReadyWaiters(err: Error): void {
  linkDead = err;
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) {
    try {
      waiter.reject(err);
    } catch {
      // One waiter must not prevent the session from being retired.
    }
  }
}

function ignoreIfUnawaited(promise: Promise<unknown>): void {
  void promise.catch(() => {});
}

function ensureLink(): Promise<LifecycleLink> {
  if (linkPromise) return linkPromise;
  const envLink = process.env.KELD_APP_LINK;
  if (!envLink) {
    return Promise.reject(
      new Error(
        "KELD-IPC-007: KELD_APP_LINK is unset. Run under a Keld host so the host mints <endpoint>#<64 hex chars>.",
      ),
    );
  }
  linkPromise = LifecycleLink.connect(envLink, {
    onReady: onHostReady,
    onLastWindowClosed,
    onConnected: (session) => {
      connectedLink = session;
    },
    onWindowFact,
    onApplicationCall: dispatchApplicationCall,
    onLinkDead: failReadyWaiters,
  }).catch((err: unknown) => {
    // The first connect failure is sticky: its typed error answers every
    // later `whenReady` and `quit`.
    if (err instanceof Error) linkDead ??= err;
    throw err;
  });
  ignoreIfUnawaited(linkPromise);
  return linkPromise;
}
/** The typed close a `whenReady` still waiting sees once a Quit closed the link. */
function closedByQuit(): Error {
  return Object.assign(
    new Error(
      "KELD-IPC-022: the app quit before the host was ready; the role's link is closed",
    ),
    { code: "KELD-IPC-022" },
  );
}

function sendQuit(): Promise<void> {
  // The already-Ready role owns one connected LifecycleLink. Close its public
  // admission *synchronously* before returning from app.quit(). Waiting on a
  // resolved `ensureLink().then(...)` inserts a microtask in which app code
  // can create a fresh BrowserWindow after a zero-window Quit preflight and
  // before the role's terminal link actually closes. That discards the new
  // window without its close/veto listener (real Mac RED oracle). Pre-Ready
  // and not-yet-connected roles retain the existing lazy single-link path.
  const requested =
    hostReady && connectedLink
      ? connectedLink.quit()
      : ensureLink().then((link) => link.quit());
  const done = requested.finally(() => {
    // The Quit closed the link, which suppresses onLinkDead: a whenReady
    // still waiting can never see Ready, so it rejects with the close.
    if (!hostReady && linkDead === undefined) failReadyWaiters(closedByQuit());
  });
  ignoreIfUnawaited(done);
  return done;
}

export const app: AppLifecycle = {
  whenReady(): Promise<void> {
    if (hostReady) return Promise.resolve();
    const ready = ensureLink().then(() => {
      if (hostReady) return;
      if (linkDead) return Promise.reject(linkDead);
      return new Promise<void>((resolve, reject) => {
        if (hostReady) {
          resolve();
          return;
        }
        if (linkDead) {
          reject(linkDead);
          return;
        }
        readyWaiters.push({ resolve, reject });
      });
    });
    ignoreIfUnawaited(ready);
    return ready;
  },
  isReady(): boolean {
    return hostReady;
  },

  onLastWindowClosed(listener: () => void): Unsubscribe {
    const id = nextListenerId;
    nextListenerId += 1;
    stateMapSet(lastWindowClosedListeners, id, listener);
    let active = true;
    return () => {
      if (!active) return;
      active = false;
      stateMapDelete(lastWindowClosedListeners, id);
    };
  },

  quit(): Promise<void> {
    return sendQuit();
  },
};

/** The same host-confirmed mirror exposed as a read-only facade snapshot. */
export function getWindowSnapshot(ref: WindowRef): WindowSnapshot | undefined {
  if (!hostReady) return undefined;
  return connectedLink?.windowSource?.get(ref);
}

/** Returns host identities, never locally allocated BrowserWindow ids. */
export function getWindowSnapshots(): readonly WindowSnapshot[] {
  if (!hostReady) return [];
  return connectedLink?.windowSource?.all() ?? [];
}

/**
 * A blocking native Destroy may return with authenticated LastWindowClosed
 * already applied, but its user listener still parked in WorkerLink's ring.
 * Publish that Host milestone only after the JS window emitted `closed`,
 * never inside the transport state applier or from a guessed window count.
 */
export function flushAppliedLastWindowClosedAfterDestroy(): void {
  const session = connectedLink;
  if (!session) return;
  if (session.consumeAppliedLastWindowClosed()) onLastWindowClosed();
}

/** Observe window facts only after their state was applied by the one WorkerLink. */
export function onWindowFactApplied(
  listener: (fact: WindowEvent) => void,
  onReadyCatchUp?: () => void,
): Unsubscribe {
  windowFactListeners.add(listener);
  if (onReadyCatchUp) {
    readyWindowCatchUps.add(onReadyCatchUp);
    if (hostReady) setImmediate(() => safelyCatchUpWindow(onReadyCatchUp));
  }
  return () => {
    windowFactListeners.delete(listener);
    if (onReadyCatchUp) readyWindowCatchUps.delete(onReadyCatchUp);
  };
}

/** Host-minted synchronous Create / Destroy, unavailable until the full Ready barrier. */
export function invokeWindowBlocking(request: WindowRequest): WindowResponse {
  if (!hostReady || !connectedLink?.windowSource) {
    throw linkError(
      "KELD-CORE-040",
      "native window access requires Host Ready and accepted replay",
    );
  }
  const session = connectedLink;
  const frozenRequest = snapshotHostCloseVerdict(request);
  const response = session.callWindowBlocking(frozenRequest);
  acceptHostCloseReply(frozenRequest, response, session);
  return response;
}

/** Copy the exact pair/sequence/verdict sent before a mutable caller can act. */
function snapshotHostCloseVerdict(request: WindowRequest): WindowRequest {
  if (request.variant !== "CloseReply") return request;
  const { id, incarnation } = request.window;
  const close_seq = request.close_seq;
  const verdict = request.verdict;
  return Object.freeze({
    variant: "CloseReply" as const,
    window: Object.freeze({ id, incarnation }),
    close_seq,
    verdict,
  });
}

/** Host reply, not a locally requested verdict, is the only clearing oracle. */
function acceptHostCloseReply(
  request: WindowRequest,
  response: WindowResponse,
  session: LifecycleLink,
): void {
  if (request.variant !== "CloseReply" || response.variant !== "ReplyApplied")
    return;
  if (
    session.windowSource?.acceptHostCloseReply(
      request.window,
      request.close_seq,
    )
  )
    deliveredWindowCloses.acceptHostCloseReply(
      request.window,
      request.close_seq,
    );
}

/** Asynchronous setters and close verdicts do not block the application's event loop. */
export function invokeWindow(request: WindowRequest): Promise<WindowResponse> {
  if (!hostReady || !connectedLink?.windowSource) {
    return Promise.reject(
      linkError(
        "KELD-CORE-040",
        "native window access requires Host Ready and accepted replay",
      ),
    );
  }
  const session = connectedLink;
  const frozenRequest = snapshotHostCloseVerdict(request);
  return session.callWindow(frozenRequest).then((response) => {
    acceptHostCloseReply(frozenRequest, response, session);
    return response;
  });
}

/** Package-private FS invocation; public exports expose only typed read/write. */
export async function invokeFs(payload: Uint8Array): Promise<Uint8Array> {
  await app.whenReady();
  const link = await ensureLink();
  return link.callFs(payload);
}

/** Package-private auth invocation on the existing role singleton link. */
export async function invokeAuth(
  payload: Uint8Array,
  deadlineMs: number,
): Promise<Uint8Array> {
  await app.whenReady();
  const link = await ensureLink();
  return link.callAuth(payload, deadlineMs);
}
