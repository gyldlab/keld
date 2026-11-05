/** GH553 recorded divergence; integrate only after the exact native qualification gate. */
import {
  app as apiApp,
  getDesktopSnapshot,
  isDesktopS1Selected,
} from "@keld/api/internal/app";
import { hostLaunchFacts } from "@keld/kipc";
import { isProxy } from "node:util/types";
import {
  method,
  refuse,
  UnsupportedMemberError,
  unsupportedSurface,
} from "./unsupported.ts";
import { remainingSession } from "./session-unsupported.ts";

/** Recorded filter shape. Its patterns grant nothing and are never retained. */
export interface WebRequestFilter {
  readonly urls?: readonly string[];
}
/** A listener accepted for the declared divergence and never retained/invoked. */
export type WebRequestListener = (...args: never[]) => unknown;

const diagnostic = console.warn.bind(console);
type RecordedMethod = "onHeadersReceived" | "onBeforeRequest";
function refusal(name: RecordedMethod, detail: string): never {
  const error = new UnsupportedMemberError(
    "method",
    `session.defaultSession.webRequest.${name}`,
    "GH-553",
  );
  error.message += ` ${detail}`;
  throw error;
}
function validateRegistration(name: RecordedMethod, args: unknown[]): void {
  if (args.length !== 1 && args.length !== 2)
    refusal(
      name,
      "Use listener|null or filter, listener|null; no callback emulation is available.",
    );
  const listener = args[args.length - 1];
  if (listener !== null && typeof listener !== "function")
    refusal(name, "The listener must be a function or null.");
  if (args.length === 1) return;
  const filter = args[0];
  if (
    filter === null ||
    typeof filter !== "object" ||
    isProxy(filter) ||
    Array.isArray(filter)
  )
    refusal(
      name,
      "The filter must contain only optional own data urls:string[].",
    );
  const keys = Reflect.ownKeys(filter);
  if (keys.some((key) => key !== "urls"))
    refusal(
      name,
      "The filter supports only urls; native policy remains authoritative.",
    );
  if (keys.length === 0) return;
  const field = Object.getOwnPropertyDescriptor(filter, "urls");
  if (
    !field?.hasOwnProperty("value") ||
    isProxy(field.value) ||
    !Array.isArray(field.value)
  )
    refusal(name, "The filter urls must be an own data array of strings.");
  for (let index = 0; index < field.value.length; index++) {
    const entry = Object.getOwnPropertyDescriptor(field.value, String(index));
    if (!entry?.hasOwnProperty("value") || typeof entry.value !== "string")
      refusal(
        name,
        "The filter urls must contain own string values, without getters or holes.",
      );
  }
}
function record(name: RecordedMethod, args: unknown[]): void {
  // This descriptor was minted from the SAME prepared native source before
  // Guardian startup and frozen by the canonical preload. It is data eligibility,
  // not a native grant, raw policy or proof inferred from generic launch metadata.
  if (
    !isDesktopS1Selected() ||
    !apiApp.isReady() ||
    hostLaunchFacts?.schema !== 5 ||
    hostLaunchFacts.originalResourcePolicy === undefined
  )
    refusal(
      name,
      "GH-473 verified response CSP/resource containment and the exact qualified Host/package composition are unavailable; use that admitted lane.",
    );
  // Existing live Source check is availability, never a second policy evaluator.
  // Preserve its actual terminal error instead of swallowing or retrying it.
  getDesktopSnapshot();
  validateRegistration(name, args);
  diagnostic(
    `Keld ▲ GH-553 session.defaultSession.webRequest.${name}: listener ignored; verified Host response CSP and native resource confinement replace this hook.`,
  );
}
const remainingWebRequest = unsupportedSurface(
  "session.defaultSession.webRequest",
  {
    onBeforeSendHeaders: method("GH-553"),
    onSendHeaders: method("GH-553"),
    onResponseStarted: method("GH-553"),
    onBeforeRedirect: method("GH-553"),
    onCompleted: method("GH-553"),
    onErrorOccurred: method("GH-553"),
  },
);
function onHeadersReceived(listener: WebRequestListener | null): void;
function onHeadersReceived(
  filter: WebRequestFilter,
  listener: WebRequestListener | null,
): void;
function onHeadersReceived(...args: unknown[]): void {
  record("onHeadersReceived", args);
}
function onBeforeRequest(listener: WebRequestListener | null): void;
function onBeforeRequest(
  filter: WebRequestFilter,
  listener: WebRequestListener | null,
): void;
function onBeforeRequest(...args: unknown[]): void {
  record("onBeforeRequest", args);
}
const selectedWebRequest = { onHeadersReceived, onBeforeRequest };
Object.defineProperties(
  selectedWebRequest,
  Object.getOwnPropertyDescriptors(remainingWebRequest),
);
const webRequest = Object.freeze(
  selectedWebRequest,
) as typeof selectedWebRequest & typeof remainingWebRequest;
const defaultSurface = { webRequest };
Object.defineProperties(
  defaultSurface,
  Object.getOwnPropertyDescriptors(remainingSession),
);
const defaultSession = Object.freeze(defaultSurface) as typeof defaultSurface &
  typeof remainingSession;
/** One after-Ready facade identity; it conveys no native session/storage authority. */
export type DefaultSession = typeof defaultSession;
const factories = unsupportedSurface("session", {
  fromPartition: method("GH-474"),
  fromPath: method("GH-474"),
});
const surface = {
  get defaultSession(): DefaultSession {
    if (!apiApp.isReady())
      return refuse("property", "session.defaultSession", "GH-553");
    return defaultSession;
  },
};
Object.defineProperties(surface, Object.getOwnPropertyDescriptors(factories));
/** Session module; only the two qualified recorded divergence registrations accept. */
export const session = Object.freeze(surface) as typeof surface &
  typeof factories;
