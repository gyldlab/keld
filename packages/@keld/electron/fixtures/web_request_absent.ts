/**
 * GH-470 red cells (corpus `electron-session-v1`): present-day observation of
 * draw.io's two `session.defaultSession.webRequest` registrations against the
 * `electron` alias. Import-only: no kipc, no whenReady (that needs a host).
 * The keld-compat target `tests/session_web_request.rs` asserts the markers.
 *
 * draw.io's own `import { session } from 'electron'` would be a link-time
 * SyntaxError against the facade today, so this probe reads the namespace and
 * the default export instead, then makes both registrations in draw.io's
 * argument shapes (drawio-desktop@2edf9fb src/main/electron.js:994 and :1010).
 *
 * Markers use `writeSync(1, …)` so a piped stdout cannot hide them behind
 * block buffering.
 */
import { writeSync } from "node:fs";
import * as electron from "electron";

type Listener = (
  details: unknown,
  callback: (response: Record<string, unknown>) => void,
) => void;
interface WebRequestLike {
  onBeforeRequest(filter: { urls: string[] }, listener: Listener): unknown;
  onHeadersReceived(listener: Listener): unknown;
}
interface SessionLike {
  defaultSession: { webRequest: WebRequestLike };
}

function marker(name: string, value: string): void {
  writeSync(1, `GH470_${name}=${value}\n`);
}

/** Records what the registration did: its return type, or the thrown error's name. */
function probe(name: string, register: () => unknown): void {
  try {
    const result = register();
    marker(name, `returned:${typeof result}`);
  } catch (error) {
    marker(name, `threw:${error instanceof Error ? error.name : typeof error}`);
  }
}

const named = (electron as unknown as { session?: SessionLike }).session;
const viaDefault = (
  electron as unknown as { default?: { session?: SessionLike } }
).default?.session;
marker("SESSION_NAMED", typeof named);
marker("SESSION_DEFAULT", typeof viaDefault);

// Today `named` is undefined; the cast keeps draw.io's call shape intact so the
// observation is the registration's own outcome, not a guard of this fixture.
const session = named as SessionLike;
// electron.js:1010 — onBeforeRequest with draw.io's file:// filter.
probe("ON_BEFORE_REQUEST", () =>
  session.defaultSession.webRequest.onBeforeRequest(
    { urls: ["file://*"] },
    (_details, callback) => callback({}),
  ),
);
// electron.js:994 — onHeadersReceived without a filter.
probe("ON_HEADERS_RECEIVED", () =>
  session.defaultSession.webRequest.onHeadersReceived((_details, callback) =>
    callback({}),
  ),
);
