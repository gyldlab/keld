/** GH553 local Windows divergence: registrations never become browser hooks. */
import { app, getDesktopSnapshot } from "@keld/api/internal/app";
import {
  hostLaunchFacts,
  linkError,
  stateFreeze,
  windowsDesktopSnapshotSelected,
  windowsOriginalDevelopmentSelected,
} from "@keld/kipc";
import { isProxy } from "node:util/types";

type Filter = { readonly urls?: readonly string[] };
type Listener = (...args: never[]) => unknown;
type RecordedMethod = "onHeadersReceived" | "onBeforeRequest";

const descriptor = Object.getOwnPropertyDescriptor;
const hasOwn = Object.hasOwn;
const ownKeys = Reflect.ownKeys;
const isArray = Array.isArray;
const diagnostic = console.warn.bind(console);

function refuse(member: string, detail: string): never {
  throw linkError(
    "KELD-COMPAT-013",
    `session.defaultSession${member} is unavailable. GH-553 owns this contract. ${detail}`,
  );
}

function registrationRefusal(name: RecordedMethod, detail: string): never {
  return refuse(`.webRequest.${name}`, detail);
}

function validateRegistration(name: RecordedMethod, args: unknown[]): void {
  if (args.length !== 1 && args.length !== 2)
    registrationRefusal(name, "Use listener|null or filter, listener|null.");
  const listener = args[args.length - 1];
  if (listener !== null && typeof listener !== "function")
    registrationRefusal(name, "Use a function or null for the listener.");
  if (args.length === 1) return;
  const filter = args[0];
  if (
    filter === null ||
    typeof filter !== "object" ||
    isProxy(filter) ||
    isArray(filter)
  )
    registrationRefusal(
      name,
      "Use a filter with optional own data urls:string[].",
    );
  const keys = ownKeys(filter);
  if (keys.length === 0) return;
  if (keys.length !== 1 || keys[0] !== "urls")
    registrationRefusal(name, "The filter only accepts urls.");
  const field = descriptor(filter, "urls");
  if (field === undefined || !hasOwn(field, "value"))
    registrationRefusal(name, "Use a data array for urls.");
  const urls: unknown = field.value;
  if (isProxy(urls) || !isArray(urls))
    registrationRefusal(name, "Use a data array for urls.");
  for (let index = 0; index < urls.length; index += 1) {
    const entry = descriptor(urls, `${index}`);
    if (
      entry === undefined ||
      !hasOwn(entry, "value") ||
      typeof entry.value !== "string"
    )
      registrationRefusal(
        name,
        "Use an array of own string values without getters or holes.",
      );
  }
}

function record(name: RecordedMethod, args: unknown[]): void {
  if (
    !windowsOriginalDevelopmentSelected ||
    !windowsDesktopSnapshotSelected ||
    !app.isReady() ||
    hostLaunchFacts?.originalResourcePolicy === undefined
  )
    registrationRefusal(
      name,
      "Run under the selected Ready Windows development Host with verified original resource policy and matching SDK.",
    );
  // The current live mirror is an availability check, never a policy decision.
  getDesktopSnapshot();
  validateRegistration(name, args);
  diagnostic(
    `Keld GH-553 session.defaultSession.webRequest.${name}: listener ignored; verified Host response CSP and resource confinement replace this hook.`,
  );
}

function onHeadersReceived(listener: Listener | null): void;
function onHeadersReceived(filter: Filter, listener: Listener | null): void;
function onHeadersReceived(...args: unknown[]): void {
  record("onHeadersReceived", args);
}

function onBeforeRequest(listener: Listener | null): void;
function onBeforeRequest(filter: Filter, listener: Listener | null): void;
function onBeforeRequest(...args: unknown[]): void {
  record("onBeforeRequest", args);
}

const webRequest = stateFreeze({
  onHeadersReceived,
  onBeforeRequest,
  onBeforeSendHeaders: () =>
    refuse(".webRequest.onBeforeSendHeaders", "This hook is unsupported."),
  onSendHeaders: () =>
    refuse(".webRequest.onSendHeaders", "This hook is unsupported."),
  onResponseStarted: () =>
    refuse(".webRequest.onResponseStarted", "This hook is unsupported."),
  onBeforeRedirect: () =>
    refuse(".webRequest.onBeforeRedirect", "This hook is unsupported."),
  onCompleted: () =>
    refuse(".webRequest.onCompleted", "This hook is unsupported."),
  onErrorOccurred: () =>
    refuse(".webRequest.onErrorOccurred", "This hook is unsupported."),
});
const defaultSession = stateFreeze({ webRequest });

/** One facade identity. It carries no native Session authority. */
export const session = stateFreeze({
  get defaultSession(): typeof defaultSession {
    if (
      !windowsOriginalDevelopmentSelected ||
      !windowsDesktopSnapshotSelected ||
      !app.isReady()
    )
      return refuse(
        "",
        "Wait for the selected Windows development Host to reach Ready.",
      );
    return defaultSession;
  },
});
