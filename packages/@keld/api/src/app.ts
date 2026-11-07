import type { MenuRequestV1, MenuInstallReceipt } from "./menu.generated.ts";
/**
 * Backend-independent application lifecycle over the canonical app-link.
 *
 * This package owns the singleton lifecycle session. Compatibility layers
 * such as @keld/electron delegate to this object instead of opening another
 * socket or reader.
 */
import { dispatchApplicationCall } from "./channels.ts";
import { LifecycleLink } from "./link.ts";
import type {
  DialogReplyV1,
  MessageOptions,
  OpenDialogOptions,
  SaveDialogOptions,
} from "./dialog.generated.ts";
import {
  hostLaunchFacts,
  linkError,
  nativeFacadeBootSelected,
  runtimeProcessFactsSelected,
  stateFreeze,
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
import type { DesktopSnapshot, WindowSnapshot } from "./window-source.ts";
export type { DesktopSnapshot } from "./window-source.ts";
import { WindowCloseReceipts } from "./window-close-receipts.ts";
import {
  encodeCompatControlRequest,
  type CompatControlRequest,
} from "@keld/kipc/compat-control";
import type { CompatConsumer } from "./compat-source.ts";

import {
  encodeRuntimeProcessInit,
  snapshotRuntimeProcessFacts,
} from "@keld/kipc/runtime-process-facts";
let runtimeInitPayload: Uint8Array | undefined;
/** Internal cold preload entry: captures before any link attempt or user entry. */
export function preloadRuntimeProcessFacts(versions: unknown): Promise<void> {
  if (!runtimeProcessFactsSelected)
    return Promise.reject(
      linkError(
        "KELD-IPC-005",
        "Runtime facts require trusted Host launch selection.",
      ),
    );
  if (linkPromise !== undefined || runtimeInitPayload !== undefined)
    return Promise.reject(
      linkError(
        "KELD-IPC-005",
        "Runtime facts must precede the role's one link attempt.",
      ),
    );
  runtimeInitPayload = encodeRuntimeProcessInit(
    snapshotRuntimeProcessFacts(versions),
  );
  return ensureLink().then(() => undefined);
}

let compatConsumer: CompatConsumer | undefined;
/** Internal facade registration; no connection, sender or permission is created. */
export function registerCompatMainConsumer(consumer: CompatConsumer): void {
  if (compatConsumer !== undefined)
    throw linkError(
      "KELD-IPC-005",
      "compat main consumer is already registered",
    );
  compatConsumer = consumer;
}
/** Recheck the captured original target after the existing link's async readiness. */
export function sendCompatRequest(
  request: CompatControlRequest,
  current: () => boolean,
): Promise<void> {
  // Synchronous snapshot/refusal is preserved before any async link readiness.
  const payload = encodeCompatControlRequest(request);
  if (!current()) return Promise.resolve();
  const send = (link: LifecycleLink): Promise<void> =>
    current()
      ? link.callCompat(request, current, payload).then(() => undefined)
      : Promise.resolve();
  return connectedLink ? send(connectedLink) : ensureLink().then(send);
}

/** @internal Main-to-renderer send using only a native-observed current view.
 * This is the same link and final Host Guard as original IPC replies; unlike
 * event.reply, it selects the *current* window document at call admission.
 */
export function sendCurrentWindowEvent(
  window: Readonly<WindowRef>,
  topic: string,
  payload: Uint8Array,
): Promise<void> {
  const session = connectedLink;
  if (
    !nativeFacadeBootSelected ||
    !runtimeProcessFactsSelected ||
    !hostReady ||
    linkDead !== undefined ||
    !session?.windowAdmissionOpen
  ) {
    throw linkError(
      "KELD-CORE-043",
      "authenticated main-to-renderer window route unavailable",
    );
  }
  const current = (): boolean =>
    connectedLink === session &&
    hostReady &&
    linkDead === undefined &&
    session.windowAdmissionOpen;
  return session.sendToCurrentWindow(window, topic, payload, current);
}

/**
 * GH481 S1 is currently the qualified macOS *development* facade slice.
 * Only Core's macOS admitted dev lease produces the existing immutable
 * hostLaunchFacts captured in the canonical preload. Windows/Linux facade
 * roles receive the normal window subscription but no such record; their
 * previously usable Ready must not depend on an unimplemented desktop source.
 * Missing launch facts are not inferred from mutable process.env or from a
 * guessed display value. Installed/non-dev macOS remains outside S1 until its
 * own Host-owned selector and native proof are provided.
 */
const desktopS1Selected =
  nativeFacadeBootSelected && hostLaunchFacts !== undefined;

/** Shared scope for the facade's first-read refusal and public Ready commit. */
export function isDesktopS1Selected(): boolean {
  return desktopS1Selected;
}

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
type ObservedLifecycleEvent = "last-window-closed" | "quit-requested";
const lifecycleListeners = stateMap<
  number,
  { event: ObservedLifecycleEvent; listener: () => void }
>();
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
    if (desktopS1Selected) {
      let failure: Error | undefined;
      try {
        if (!connectedLink?.windowSource) {
          failure = linkError(
            "KELD-CORE-045",
            "The native window subscription has no admitted mirror source.",
          );
        } else if (!connectedLink.windowSource.desktop()) {
          failure = linkError(
            "KELD-CORE-049",
            "The native desktop snapshot is unavailable. Correct the Host display/appearance capture and relaunch.",
          );
        }
      } catch (error) {
        failure =
          error instanceof Error
            ? error
            : linkError("KELD-IPC-005", "desktop mirror has ended");
      }
      if (failure) {
        // Failure belongs to the same generation and is terminal. A later
        // valid capture cannot revive its earlier public Ready promise.
        failReadyWaiters(failure);
        connectedLink?.close();
        return;
      }
    }
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

function dispatchObservedLifecycle(
  event: ObservedLifecycleEvent,
  current?: () => boolean,
): void {
  const snapshot = stateMapValues(lifecycleListeners);
  for (let index = 0; index < snapshot.length; index += 1) {
    const observed = snapshot[index]!;
    if (observed.event !== event) continue;
    if (current && !current()) return;
    try {
      observed.listener();
    } catch {
      // Host events share the app-link read loop; isolate user callbacks.
    }
  }
}
function onLastWindowClosed(): void {
  dispatchObservedLifecycle("last-window-closed");
}
function onHostQuitRequested(session: LifecycleLink): void {
  dispatchObservedLifecycle(
    "quit-requested",
    () =>
      connectedLink === session && currentOriginalWindowSession() === session,
  );
}
function observeLifecycle(
  event: ObservedLifecycleEvent,
  listener: () => void,
): Unsubscribe {
  const id = nextListenerId;
  nextListenerId += 1;
  stateMapSet(lifecycleListeners, id, { event, listener });
  let active = true;
  return () => {
    if (!active) return;
    active = false;
    stateMapDelete(lifecycleListeners, id);
  };
}
/** @internal Framework observer on the existing lifecycle owner; not menu authority. */
export function onNativeQuitRequested(listener: () => void): Unsubscribe {
  return observeLifecycle("quit-requested", listener);
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
      linkError(
        "KELD-IPC-007",
        "KELD_APP_LINK is unset. Run under a Keld host so the host mints <endpoint>#<64 hex chars>.",
      ),
    );
  }
  linkPromise = LifecycleLink.connect(
    envLink,
    {
      onReady: onHostReady,
      onLastWindowClosed,
      onQuitRequested: onHostQuitRequested,
      onConnected: (session) => {
        connectedLink = session;
      },
      onWindowFact,
      onApplicationCall: dispatchApplicationCall,
      onCompatEvent: (event, current) =>
        compatConsumer?.(event, current) ?? { variant: "NoListener" },
      onCompatError: (error) => {
        try {
          console.error("KELD compat link failed:", error);
        } catch {
          /* diagnostic only */
        }
      },
      onLinkDead: failReadyWaiters,
    },
    runtimeInitPayload,
  ).catch((err: unknown) => {
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
    return observeLifecycle("last-window-closed", listener);
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

/** Package-private, read-only scope for the root-approved original single-window handoff. */
export interface OriginalWindowQuitScope {
  readonly window: Readonly<WindowRef>;
  isCurrentPending(window: WindowRef, closeSeq: bigint): boolean;
  dispatchedPendingClose(): bigint | null;
  requestClose(): Promise<WindowResponse>;
  failAfterHandoff(error: Error): void;
}

/** Select existing original-runtime metadata plus the actual current native Source. */
function currentOriginalWindowSession(): LifecycleLink | undefined {
  const session = connectedLink;
  if (
    !nativeFacadeBootSelected ||
    !runtimeProcessFactsSelected ||
    (hostLaunchFacts?.schema !== 5 && hostLaunchFacts?.schema !== 7) ||
    hostLaunchFacts.originalResourcePolicy === undefined ||
    !hostReady ||
    linkDead !== undefined ||
    !session?.windowAdmissionOpen
  )
    return undefined;
  if (!session.windowSource) return undefined;
  return session;
}

/** @internal Read-only observation of one actual finished original document.
 * Uses the existing qualified session and its Host mirror; no URL or grant mint. */
export function captureOriginalWindowRecoveryDocument(
  window: WindowRef,
): (() => boolean) | undefined {
  const session = currentOriginalWindowSession();
  const source = session?.windowSource;
  if (!session || !source) return undefined;
  const ref = stateFreeze({ id: window.id, incarnation: window.incarnation });
  let generation: number | undefined;
  try {
    generation = source.get(ref)?.document?.generation;
  } catch {
    return undefined;
  }
  if (generation === undefined) return undefined;
  const binding = session.captureCurrentWindowDocument(ref);
  if (!binding || binding.navigation !== generation) return undefined;
  const current = (): boolean => {
    if (
      currentOriginalWindowSession() !== session ||
      session.windowSource !== source ||
      !binding.current()
    )
      return false;
    try {
      const snapshot = source.get(ref);
      return (
        snapshot !== undefined &&
        !snapshot.host_destroyed &&
        snapshot.document?.current === true &&
        snapshot.document.finished &&
        snapshot.document.generation === generation
      );
    } catch {
      return false;
    }
  };
  return current() ? current : undefined;
}

/** Read-only scope for the same admitted original session and current window. */
export function captureOriginalWindowQuitScope(
  window: WindowRef,
): OriginalWindowQuitScope | undefined {
  const session = currentOriginalWindowSession();
  if (!session) return undefined;
  const source = session.windowSource;
  if (!source) return undefined;
  const ref = Object.freeze({ id: window.id, incarnation: window.incarnation });
  const current = (): WindowSnapshot | undefined => {
    if (
      connectedLink !== session ||
      linkDead !== undefined ||
      !session.windowAdmissionOpen
    )
      return undefined;
    try {
      const snapshot = source.get(ref);
      return snapshot &&
        !snapshot.host_destroyed &&
        snapshot.document?.current &&
        snapshot.document.finished
        ? snapshot
        : undefined;
    } catch {
      return undefined;
    }
  };
  if (!current()) return undefined;
  const isCurrentPending = (candidate: WindowRef, closeSeq: bigint): boolean =>
    candidate.id === ref.id &&
    candidate.incarnation === ref.incarnation &&
    current()?.pending_close === closeSeq;
  return {
    window: ref,
    isCurrentPending,
    dispatchedPendingClose(): bigint | null {
      // Mirror application alone is insufficient: this existing owner records
      // only genuine already-dispatched Host facts and their acknowledged sequence.
      const receipts = deliveredWindowCloses.pending();
      for (let index = 0; index < receipts.length; index++) {
        const receipt = receipts[index]!;
        if (isCurrentPending(receipt.window, receipt.closeSeq))
          return receipt.closeSeq;
      }
      return null;
    },
    requestClose(): Promise<WindowResponse> {
      if (!current())
        return Promise.reject(
          linkError(
            "KELD-CORE-043",
            "original Quit window scope retired before close admission",
          ),
        );
      return invokeWindow({ variant: "RequestClose", window: ref });
    },
    failAfterHandoff(error: Error): void {
      // First completion cannot be undone. Preserve its actual later cause and
      // fence the same original role/link instead of pretending that it succeeded.
      if (connectedLink === session) failReadyWaiters(error);
      session.retireWindowHandoffFailure(error);
      try {
        console.error(
          "KELD original Quit RequestClose failed after witnessed handoff; effect unknown; original role link retired:",
          error,
        );
      } catch {
        /* The actual link retirement above is independent of logging. */
      }
    },
  };
}

/** Current admitted desktop facts. Reads are local and issue zero app-link frames. */
export function getDesktopSnapshot(): DesktopSnapshot {
  if (linkDead) throw linkDead;
  if (!hostReady || !connectedLink?.windowSource)
    throw linkError(
      "KELD-CORE-040",
      "desktop facts require the admitted Host Ready and desktop replay barrier",
    );
  const facts = connectedLink.windowSource.desktop();
  if (!facts)
    throw linkError(
      "KELD-CORE-049",
      "The native desktop snapshot is unavailable. Correct the Host display/appearance capture and relaunch.",
    );
  return facts;
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

/** Synchronous Message consumes the existing Ready session without connecting
 * or awaiting it. The link owners retain closed/Quit and terminal admission. */
export function invokeMessageDialogBlocking(
  parent: WindowRef | null,
  options: MessageOptions,
): DialogReplyV1 {
  if (linkDead !== undefined) throw linkDead;
  const session = connectedLink;
  if (!hostReady || session === undefined) {
    throw linkError(
      "KELD-CORE-040",
      "native message dialog requires Host Ready on the existing app link",
    );
  }
  return session.callMessageDialogBlocking(parent, options);
}

/** Native Message request over the already-connected, authenticated app link.
 * Nonces and both call correlations belong to LifecycleLink/WorkerLink, not
 * Electron or a second socket; the Host decides actual native authority. */
export async function invokeMessageDialog(
  parent: WindowRef | null,
  options: MessageOptions,
): Promise<DialogReplyV1> {
  await app.whenReady();
  const link = await ensureLink();
  return link.callMessageDialog(parent, options);
}

/** Path-only native file chooser over the same verified lifecycle app-link.
 * The Host, not the Electron adapter, grants/denies the actual OS panel. */
export async function invokeFileDialog(
  variant: "Open" | "Save",
  parent: WindowRef | null,
  options: OpenDialogOptions | SaveDialogOptions,
): Promise<DialogReplyV1> {
  await app.whenReady();
  const link = await ensureLink();
  return link.callFileDialog(variant, parent, options);
}

/** One native application-menu installation over this role's existing link. */
export async function invokeMenuInstall(
  request: MenuRequestV1,
): Promise<MenuInstallReceipt> {
  await app.whenReady();
  const link = await ensureLink();
  return link.callMenu(request);
}
