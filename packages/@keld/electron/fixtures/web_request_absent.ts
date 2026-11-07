/**
 * GH-470 listener cells stay RED: GH-565 links session but defaultSession still
 * refuses under GH-553. Observe draw.io's two registration shapes and Host-link
 * attempts without opening sockets. The Rust target owns the independent oracle.
 * Keep this historical fixture filename for existing references.
 */
import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

let connects = 0;
LifecycleLink.connect = (async () => {
  connects += 1;
  throw new Error("GH470_UNEXPECTED_CONNECT");
}) as typeof LifecycleLink.connect;

// Install the recorder before module evaluation. The parent sets KELD_APP_LINK
// so an attempted Host call cannot stop early at the missing-env check.
const electron = await import("electron");

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

/** Observe raw error fields, without importing the production refusal predicate. */
function probe(name: string, register: () => unknown): void {
  try {
    const result = register();
    marker(name, `returned:${typeof result}`);
  } catch (error) {
    marker(name, `threw:${error instanceof Error ? error.name : typeof error}`);
    for (const field of ["code", "member", "ticket", "kind"] as const) {
      const value =
        error !== null && typeof error === "object"
          ? Reflect.get(error, field)
          : undefined;
      marker(
        `${name}_${field.toUpperCase()}`,
        `${typeof value}:${String(value)}`,
      );
    }
  }
}

const named = (electron as unknown as { session?: SessionLike }).session;
const viaDefault = (
  electron as unknown as { default?: { session?: SessionLike } }
).default?.session;
marker("SESSION_NAMED", typeof named);
marker("SESSION_DEFAULT", typeof viaDefault);

// Keep draw.io's call shapes: defaultSession refuses before either listener registers.
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

marker("HOST_CONNECTS", String(connects));
