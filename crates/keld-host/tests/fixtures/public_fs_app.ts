// Prepared application imports only the public API index in its supplied prelude.
type FsSampleOutcome =
  | { ok: true; bytes: number[] }
  | { ok: false; code: string; message: string };
type Control = { action: string; nonce: string };
const base = "http://127.0.0.1:__PORT__";
const target = __TARGET__;
const outside = __OUTSIDE__;
const content = new Uint8Array(__CONTENT__);
const automaticComponent = __AUTOMATIC_COMPONENT__;
const retireAfterQuit = __RETIRE_AFTER_QUIT__;
// Test-owned local socket capability, never shipping app authority.
const quitBodyReceipt: { port: number; nonce: string } | null = __QUIT_BODY_RECEIPT__;
type DescendantQuitInputs = { python: string; helper: string; control: string; token: string };
const descendantQuit = __DESCENDANT_QUIT__ as DescendantQuitInputs | null;
const naturalCase: "deny-read" | "links-limits" | "traversal-bounds" | null = __NATURAL_CASE__;
type NaturalInputs = {
  readSentinel: string;
  internalAlias: string;
  externalAlias: string;
  oversizedWrite: string;
  oversizedRead: string;
  fresh: string;
  maximumContent: number;
  componentsPass: string;
  componentsDeny: string;
  linksPass: string;
  linksDeny: string;
};
const naturalInputs: NaturalInputs | null = __NATURAL_INPUTS__;
type RecoveryInputs = { after: string; witness: string; afterContent: number[]; pendingHandler: boolean };
const recoveryInputs: RecoveryInputs | null = __RECOVERY_INPUTS__;
const nativeJoint = __NATIVE_JOINT__;
const sampleControlBudget = 4096;
let handlerCalls = 0;
function rejectedFsOutcome(error: unknown): FsSampleOutcome {
  if (!isCallError(error)) throw error;
  // Preserve the registered native code and full original fix text.
  return { ok: false, code: error.code, message: error.message };
}
function isControl(value: unknown): value is Control {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return Object.keys(record).sort().join(",") === "action,nonce" &&
    typeof record.action === "string" && typeof record.nonce === "string" &&
    /^[0-9a-f]{32}$/.test(record.nonce);
}
async function performFsOperation(action: "roundtrip" | "deny"): Promise<FsSampleOutcome> {
  try {
    if (action === "roundtrip") {
      await fs.write(target, content);
      return { ok: true, bytes: [...await fs.read(target)] };
    }
    await fs.write(outside, new Uint8Array([7, 8, 9]));
    return { ok: true, bytes: [] };
  } catch (error: unknown) {
    return rejectedFsOutcome(error);
  }
}
async function performRecoveryOperation(action: "recovery-before" | "recovery-after"): Promise<FsSampleOutcome> {
  if (!recoveryInputs) throw new Error("recovery fixture inputs absent");
  try {
    const marker = action === "recovery-after" ? 2 : 1;
    const destination = marker === 1 ? target : recoveryInputs.after;
    const bytes = marker === 1 ? content : new Uint8Array(recoveryInputs.afterContent);
    const completed = await fs.read(recoveryInputs.witness);
    await fs.write(recoveryInputs.witness, new Uint8Array([...completed, marker]));
    await fs.write(destination, bytes);
    return { ok: true, bytes: [...await fs.read(destination)] };
  } catch (error: unknown) {
    return rejectedFsOutcome(error);
  }
}
type NaturalAction =
  | "denied-read"
  | "internal-read" | "internal-write" | "internal-read-after-write"
  | "external-read" | "external-write"
  | "oversize-write" | "oversize-read"
  | "components-pass-read" | "components-deny-write"
  | "links-pass-read" | "links-deny-write"
  | "healthy-write" | "healthy-read";
async function performNaturalOperation(action: NaturalAction): Promise<FsSampleOutcome> {
  if (!naturalInputs) throw new Error("natural fixture inputs absent");
  try {
    switch (action) {
      case "denied-read":
        await fs.read(naturalInputs.readSentinel);
        // An unexpected allow is visible as success, but no sentinel data leaks.
        return { ok: true, bytes: [] };
      case "internal-read":
      case "internal-read-after-write":
        return { ok: true, bytes: [...await fs.read(naturalInputs.internalAlias)] };
      case "internal-write":
        await fs.write(naturalInputs.internalAlias, content);
        return { ok: true, bytes: [] };
      case "external-read":
        await fs.read(naturalInputs.externalAlias);
        return { ok: true, bytes: [] };
      case "external-write":
        await fs.write(naturalInputs.externalAlias, new Uint8Array([7, 8, 9]));
        return { ok: true, bytes: [] };
      case "oversize-write":
        await fs.write(naturalInputs.oversizedWrite,
          new Uint8Array(naturalInputs.maximumContent + 1).fill(0x79));
        return { ok: true, bytes: [] };
      case "oversize-read":
        await fs.read(naturalInputs.oversizedRead);
        // Never serialize an unexpectedly successful large read to the observer.
        return { ok: true, bytes: [] };
      case "components-pass-read":
        return { ok: true, bytes: [...await fs.read(naturalInputs.componentsPass)] };
      case "components-deny-write":
        await fs.write(naturalInputs.componentsDeny, content);
        return { ok: true, bytes: [] };
      case "links-pass-read":
        return { ok: true, bytes: [...await fs.read(naturalInputs.linksPass)] };
      case "links-deny-write":
        await fs.write(naturalInputs.linksDeny, content);
        return { ok: true, bytes: [] };
      case "healthy-write":
        await fs.write(naturalInputs.fresh, content);
        return { ok: true, bytes: [] };
      case "healthy-read":
        return { ok: true, bytes: [...await fs.read(naturalInputs.fresh)] };
    }
  } catch (error: unknown) {
    return rejectedFsOutcome(error);
  }
}
channels.handle(echoChannel, async request => {
  handlerCalls += 1;
  const control: unknown = JSON.parse(request.message);
  if (!isControl(control) || request.count !== handlerCalls) {
    throw new Error("public FS sample invalid action/nonce/count");
  }
  console.error("KELD_KEL140_PUBLIC_HANDLER " + JSON.stringify({
    call: handlerCalls, action: control.action, nonce: control.nonce, pid: process.pid,
  }));
  let outcome: FsSampleOutcome;
  if (recoveryInputs && request.count === 1 &&
      (control.action === "recovery-before" || control.action === "recovery-after")) {
    outcome = await performRecoveryOperation(control.action);
  } else if (nativeJoint && request.count === 1 && control.action === "native-hold") {
    try {
      await fs.write(target, content);
      throw new Error("native-held filesystem write returned before the expiry control");
    } catch (error: unknown) {
      if (!isCallError(error) || error.code !== "KELD-IPC-006") throw error;
      const report = { phase: "joint-native-expiry", pid: process.pid,
        nonce: control.nonce, code: error.code, message: error.message };
      const observed = await fetch(base + "/observe", { method: "POST",
        headers: { "Content-Type": "text/plain" }, body: JSON.stringify(report) });
      if (!observed.ok) throw new Error("native expiry observation refused");
      throw error;
    }
  } else if (recoveryInputs?.pendingHandler && request.count === 3 &&
      control.action === "recovery-pending") {
    const bytes = await fs.read(target);
    if (bytes.byteLength !== content.byteLength ||
        !bytes.every((byte, index) => byte === content[index])) {
      throw new Error("pending fixture lost its completed G1 public roundtrip");
    }
    // Handler-only hold after the independently completed G1 renderer calls.
    // The read above is genuine public FS; this is never a native-work hold.
    await fetch(base + "/recovery-pending/" + process.pid + "/" + control.nonce);
    throw new Error("old pending application handler was unexpectedly released");
  } else if (control.action === "roundtrip" && request.count === 1) {
    outcome = await performFsOperation("roundtrip");
  } else if (control.action === "deny" && request.count === 2) {
    outcome = await performFsOperation("deny");
  } else {
    // Unrelated handler failures retain resolveEchoCall's existing API001 rule.
    throw new Error("public FS sample unknown action");
  }
  const response = { message: JSON.stringify(outcome), count: request.count };
  if (echoChannel.encodeResponse(response).byteLength > sampleControlBudget) {
    throw new Error("sample outcome exceeds renderer control budget");
  }
  console.error("KELD_KEL140_PUBLIC_OUTCOME " + JSON.stringify({
    call: handlerCalls, nonce: control.nonce, outcome,
  }));
  return response;
});
if (recoveryInputs && !nativeJoint) {
  // Ordinary fixture data holds only this app's first public whenReady call.
  // It gives the observer time to test old credentials before fresh HELLO.
  const bootstrap = process.env.KELD_APP_LINK;
  if (!bootstrap) throw new Error("recovery app has no host-minted link");
  const boot = await fetch(base + "/observe", { method: "POST",
    headers: { "Content-Type": "text/plain" }, body: JSON.stringify({
      phase: "recovery-app-boot", pid: process.pid, appLink: bootstrap,
    }) });
  if (!boot.ok) throw new Error("recovery boot observation refused");
  const admission = await fetch(base + "/recovery-admit/" + process.pid);
  if (!admission.ok || await admission.text() !== "admit") {
    throw new Error("invalid recovery fixture admission command");
  }
}
await app.whenReady();
if (process.env.KELD_DEV_LEASE !== undefined ||
    process.env.KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION !== undefined) {
  throw new Error("public FS application inherited host-only authority");
}
const descendant = descendantQuit
  ? Bun.spawn([descendantQuit.python, descendantQuit.helper], {
      env: { KEL118_FIXTURE_CONTROL: descendantQuit.control,
        KEL118_FIXTURE_TOKEN: descendantQuit.token },
      stdin: "ignore", stdout: "inherit", stderr: "inherit",
      // Same non-detached Python-peer class actually measured by the root probe.
      // No child.kill or child.exited wait occurs in the public-Quit path.
    })
  : Bun.spawn(["/usr/bin/tail", "-f", "/dev/null"], {
      stdin: "ignore", stdout: "ignore", stderr: "ignore",
    });
const appLink = process.env.KELD_APP_LINK;
if (!appLink) throw new Error("public FS app has no host-minted link");
const ready = await fetch(base + "/observe", { method: "POST",
  headers: { "Content-Type": "text/plain" }, body: JSON.stringify({
    phase: "app-ready", pid: process.pid, descendantPid: descendant.pid,
    descendantKind: descendantQuit ? "python-peer-inherited-stdout" : "tail-ignored-stdio",
    endpoint: appLink.slice(0, appLink.lastIndexOf("#")),
    executable: process.execPath, bunVersion: Bun.version,
  }),
});
if (!ready.ok) throw new Error("public FS ready observation refused");
if (automaticComponent) {
  // A public-main component: no renderer invoke, OS input or Echo forwarding.
  const outcomes: { action: "roundtrip" | "deny"; outcome: FsSampleOutcome }[] = [];
  for (const action of ["roundtrip", "deny"] as const) {
    outcomes.push({ action, outcome: await performFsOperation(action) });
  }
  const result = { phase: "app-result", pid: process.pid, componentOnly: true, outcomes };
  console.error("KELD_KEL140_PUBLIC_APP_COMPONENT " + JSON.stringify(result));
  const observed = await fetch(base + "/observe", { method: "POST",
    headers: { "Content-Type": "text/plain" }, body: JSON.stringify(result) });
  if (!observed.ok) throw new Error("public app component observation refused");
  // Fixed fixture-only marker: Bun actually received HTTP response headers.
  // No response payload, URL, PID, path, credential, or policy is recorded.
  console.error("KELD_KEL140_APP_RESULT_HTTP_OK");
} else if (naturalCase) {
  // Closed public-main scenarios only: no renderer action or Echo forwarding.
  const actions: readonly NaturalAction[] = naturalCase === "deny-read"
    ? ["denied-read"]
    : naturalCase === "links-limits"
    ? ["internal-read", "internal-write", "internal-read-after-write",
      "external-read", "external-write", "oversize-write", "oversize-read",
      "healthy-write", "healthy-read"]
    : ["components-pass-read", "components-deny-write", "links-pass-read",
      "links-deny-write", "healthy-write", "healthy-read"];
  const outcomes: { action: NaturalAction; outcome: FsSampleOutcome }[] = [];
  for (const action of actions) {
    outcomes.push({ action, outcome: await performNaturalOperation(action) });
  }
  const result = { phase: "app-result", pid: process.pid, componentOnly: true,
    naturalCase, outcomes };
  const body = JSON.stringify(result);
  if (new TextEncoder().encode(body).byteLength > sampleControlBudget) {
    throw new Error("natural outcomes exceed existing sample control budget");
  }
  console.error("KELD_KEL140_PUBLIC_APP_COMPONENT " + body);
  const observed = await fetch(base + "/observe", { method: "POST",
    headers: { "Content-Type": "text/plain" }, body });
  if (!observed.ok) throw new Error("public natural-case observation refused");
}
// The standalone native test executes this exact real Bun Quit verifier.
// KELD_QUIT_BODY_FUNCTION_BEGIN
type QuitBodyReceiptInput = { port: number; nonce: string };
async function acceptActualQuitBody(
  quit: Response, receipt: QuitBodyReceiptInput | null,
): Promise<void> {
  if (!quit.ok) throw new Error("invalid sample quit command");
  console.error("KELD_KEL140_QUIT_HTTP_HEADERS_OK");
  if (await quit.text() !== "quit") throw new Error("invalid sample quit command");
  console.error("KELD_KEL140_QUIT_HTTP_BODY_OK");
  // Only the selected native acceptance fixture has this one-use capability.
  // It reaches an independent native socket AFTER the real HTTP body check
  // and BEFORE the KIPC lifecycle Quit call.
  if (receipt !== null) {
    const witness = await fetch("http://127.0.0.1:" + receipt.port +
      "/keld-body-verified/" + receipt.nonce);
    if (!witness.ok || await witness.text() !== "ack") {
      throw new Error("native Quit body receipt was not acknowledged");
    }
  }
}
// KELD_QUIT_BODY_FUNCTION_END

// The observer retains this stream without blocking other page observations.
const quit = await fetch(base + (recoveryInputs || nativeJoint
  ? "/recovery-quit/" + process.pid : "/quit"));
if (quitBodyReceipt !== null) {
  await acceptActualQuitBody(quit, quitBodyReceipt);
} else {
  // Keep the original no-receipt fixture path inline: no extra async hop.
  if (!quit.ok) throw new Error("invalid sample quit command");
  console.error("KELD_KEL140_QUIT_HTTP_HEADERS_OK");
  if (await quit.text() !== "quit") throw new Error("invalid sample quit command");
  console.error("KELD_KEL140_QUIT_HTTP_BODY_OK");
}
console.error("KELD_KEL140_PUBLIC_QUIT_REQUEST calls=" + handlerCalls);
const pendingQuit = app.quit();
if (retireAfterQuit) {
  // Reserve the ordinary public Quit first, without waiting for its REPLY.
  // Report actual outcomes before awaiting Quit; teardown owns process exit.
  const describeFailure = (action: "write" | "read", error: unknown) => ({
    action, status: "rejected",
    errorName: error instanceof Error ? error.name : null,
    message: error instanceof Error ? error.message : String(error),
    codePresent: typeof error === "object" && error !== null && "code" in error,
    callError: isCallError(error),
  });
  const outcomes = await Promise.all([
    fs.write(target, new Uint8Array([7, 8, 9])).then(
      () => ({ action: "write", status: "fulfilled", bytes: [] }),
      (error: unknown) => describeFailure("write", error),
    ),
    fs.read(target).then(
      bytes => ({ action: "read", status: "fulfilled", bytes: [...bytes] }),
      (error: unknown) => describeFailure("read", error),
    ),
  ]);
  const body = JSON.stringify({ phase: "app-retirement", pid: process.pid,
    componentOnly: true, outcomes });
  if (new TextEncoder().encode(body).byteLength > sampleControlBudget) {
    throw new Error("retirement outcomes exceed existing sample control budget");
  }
  console.error("KELD_KEL140_PUBLIC_FS_RETIRED " + body);
}
await pendingQuit;
process.exit(0);
