// Linux authenticated-Window component actor. No WindowSource/public API attachment.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import {
  LIFECYCLE_CHANNEL, WINDOW_CHANNEL, WorkerLink, isCallError,
} from "../../../../../../packages/@keld/kipc/src/transport.ts";
import {
  decodeWindowEvent, decodeWindowResponse, encodeWindowRequest,
  type WindowEvent, type WindowRef, type WindowRequest, type WindowResponse,
} from "../../../../../../packages/@keld/api/src/window-codec.ts";

const control = createConnection({ host: "127.0.0.1", port: Number(process.argv[2]) });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
const commands = createInterface({ input: control });
const endpoint = process.env.KELD_APP_LINK;
if (endpoint === undefined) throw new Error("missing authenticated app link");
const link = await WorkerLink.open({
  link: endpoint,
  receive: { eventChannels: [LIFECYCLE_CHANNEL, WINDOW_CHANNEL], callReceivers: [] },
});

let facts = 0;
let firstCreated: Extract<WindowEvent, { variant: "Created" }> | undefined;
let latest: WindowEvent | undefined;
let window: WindowRef | undefined;
let readyBytes = "absent";
let readyResolve: () => void = () => undefined;
const ready = new Promise<void>((resolve) => { readyResolve = resolve; });
let maximizedResolve: () => void = () => undefined;
const maximized = new Promise<void>((resolve) => { maximizedResolve = resolve; });
const ended = new Promise<string>((resolve) => { link.onEnd((error) => resolve(error.code)); });
link.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
  readyBytes = Array.from(payload, (byte) => byte.toString(16).padStart(2, "0")).join("");
  readyResolve();
});
link.setStateApplier(WINDOW_CHANNEL, (payload) => {
  const fact = decodeWindowEvent(payload);
  facts += 1;
  latest = fact;
  if (fact.variant === "Created" && firstCreated === undefined) firstCreated = fact;
  if (fact.variant === "Maximized") maximizedResolve();
});

function ref(value: WindowRef | undefined): string {
  return value === undefined ? "absent" : `${value.id}:${value.incarnation}`;
}
function requireWindow(): WindowRef {
  if (window === undefined) throw new Error("Create did not return a window reference");
  return window;
}
function confirmedMaximized(): string {
  if (latest?.variant === "Created") return String(latest.state.maximized);
  if (latest?.variant === "Maximized") return String(latest.maximized);
  return "absent";
}
async function call(request: WindowRequest): Promise<WindowResponse> {
  return decodeWindowResponse(await link.call(WINDOW_CHANNEL, encodeWindowRequest(request), 5000));
}
async function refusal(request: WindowRequest): Promise<string> {
  try {
    await call(request);
    return "returned";
  } catch (error: unknown) {
    return isCallError(error) ? error.code : "untyped";
  }
}
function report(line: string): void { control.write(`${line}\n`); }

report("BOUND");
for await (const command of commands) {
  if (command === "PRE-SUB") {
    report(`PRE-SUB ${await refusal({ variant: "Create", width: 640, height: 480, x: null, y: null })}`);
  } else if (command === "SUBSCRIBE") {
    // The existing lifecycle enum is Subscribe=1, followed by this allocated u16
    // channel (5 fits one byte). Subscribed's exact unit payload is checked by Rust.
    const response = await link.call(LIFECYCLE_CHANNEL, Uint8Array.of(1, WINDOW_CHANNEL), 5000);
    report(`SUBSCRIBE ${Array.from(response).join(",")}`);
  } else if (command === "READY") {
    await ready;
    report(`READY ${readyBytes}`);
  } else if (command === "CREATE") {
    const response = await call({ variant: "Create", width: 640, height: 480, x: null, y: null });
    if (response.variant === "Created") window = response.window;
    const fact = firstCreated;
    const bounds = fact?.state.bounds;
    report(`CREATE ${response.variant} ${ref(window)} ${fact?.variant ?? "absent"} ${ref(fact?.window)} ` +
      `${bounds === undefined ? "absent" : `${bounds.x},${bounds.y},${bounds.width},${bounds.height}`} ` +
      `${fact === undefined ? "absent" : `${fact.state.focused},${fact.state.maximized},${fact.state.full_screen}`} ` +
      `${String(fact?.pending_close)} ${facts}`);
  } else if (command === "SET-MAX") {
    const response = await call({ variant: "SetMaximized", window: requireWindow(), maximized: true });
    report(`SET-MAX ${response.variant} ${facts} ${confirmedMaximized()}`);
  } else if (command === "MAX-FACT") {
    await maximized;
    report(`MAX-FACT ${latest?.variant} ${ref(latest?.window)} ${facts} ${confirmedMaximized()}`);
  } else if (command === "STALE") {
    const current = requireWindow();
    const code = await refusal({
      variant: "SetMaximized",
      window: { id: current.id, incarnation: current.incarnation + 1n },
      maximized: false,
    });
    report(`STALE ${code} ${facts}`);
  } else if (command === "HEALTHY") {
    const response = await call({ variant: "SetFullScreen", window: requireWindow(), full_screen: true });
    report(`HEALTHY ${response.variant} ${facts} ${confirmedMaximized()}`);
  } else if (command === "WAIT-END") {
    report(`ENDED ${await ended} ${facts}`);
  } else {
    throw new Error(`unknown authenticated Window command: ${command}`);
  }
}
