// Prelude supplied by existing bundle_public_api_entry imports ONLY public @keld/api.
const observation = "http://127.0.0.1:__PORT__";
const smokeCase = __CASE__ as
  | "default-deny"
  | "missing-client"
  | "invalid-hint"
  | "unknown-token"
  | "active-begin-quit";
const bodyLimit = 4096;
async function report(
  phase: string,
  fields: Record<string, unknown>,
): Promise<void> {
  const body = JSON.stringify({
    phase,
    smokeCase,
    pid: process.pid,
    ...fields,
  });
  if (new TextEncoder().encode(body).byteLength > bodyLimit)
    throw new Error("smoke observation exceeds fixture bound");
  const accepted = await fetch(observation + "/observe", {
    method: "POST",
    body,
  });
  if (!accepted.ok) throw new Error("ordinary smoke observer refused data");
}
async function outcome(
  call: () => Promise<unknown>,
): Promise<Record<string, unknown>> {
  try {
    await call();
    // Never serialize an unexpected success: credentials are not fixture data.
    return { ok: true };
  } catch (error: unknown) {
    if (!isCallError(error))
      throw new Error("KEL89_AUTH_SMOKE_UNTYPED_FAILURE");
    return { ok: false, code: error.code, message: error.message };
  }
}
await app.whenReady();
if (
  process.env.KELD_DEV_LEASE !== undefined ||
  process.env.KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION !== undefined
) {
  throw new Error("ordinary AppProcess inherited host-only authority");
}
// Same actual held-descendant shape as the existing public FS component fixture.
// No child.kill or child.exited wait in this application's public Quit path.
const descendant = Bun.spawn(["/usr/bin/tail", "-f", "/dev/null"], {
  stdin: "ignore",
  stdout: "ignore",
  stderr: "ignore",
});
const bootstrap = process.env.KELD_APP_LINK;
if (!bootstrap) throw new Error("smoke app has no host-minted link");
await report("app-ready", {
  descendantPid: descendant.pid,
  endpoint: bootstrap.slice(0, bootstrap.lastIndexOf("#")),
  executable: process.execPath,
  bunVersion: Bun.version,
});
let outcomes: Record<string, unknown>[];
let pendingBegin: Promise<Record<string, unknown>> | undefined;
if (smokeCase === "active-begin-quit") {
  // Leave the real public call pending. Only the independent Host socket census
  // establishes admission and a bound loopback owner; this marker does not.
  pendingBegin = outcome(() => auth.begin({ provider: "entra-common" }));
  await report("begin-pending", {});
  outcomes = [];
} else if (smokeCase === "unknown-token") {
  outcomes = [
    await outcome(() =>
      auth.token({
        accountId: "KEL89_SMOKE_UNKNOWN_A",
        scopes: ["smoke-scope"],
      }),
    ),
    await outcome(() =>
      auth.token({ accountId: "KEL89_SMOKE_UNKNOWN_B", scopes: [] }),
    ),
  ];
} else {
  outcomes = [
    await outcome(() =>
      auth.begin({
        provider: "entra-common",
        // Deny and missing-client must win over this validly encoded bad private hint.
        loginHint: "KEL89_SMOKE_HINT\n",
      }),
    ),
  ];
}
if (pendingBegin === undefined) await report("app-result", { outcomes });
const quit = await fetch(observation + "/quit");
if (!quit.ok || (await quit.text()) !== "quit")
  throw new Error("invalid ordinary fixture Quit command");
console.error("KELD_KEL89_PUBLIC_QUIT_REQUEST " + smokeCase);
await app.quit();
if (pendingBegin !== undefined) {
  const ended = await pendingBegin;
  if (
    ended.ok !== false ||
    (ended.code !== "KELD-IPC-022" && ended.code !== "KELD-IPC-023")
  ) {
    throw new Error("KEL89_ACTIVE_BEGIN_QUIT_WRONG_TERMINAL");
  }
  await report("begin-quit-terminal", { code: ended.code });
}
// Existing fixture behavior: only after the public correlated Quit reply.
process.exit(0);
