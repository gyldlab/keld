/**
 * App-main body appended to the wire-tested kipc client when `keld create`
 * renders `src/main.ts`. Bun opens the app-link itself (the transport Worker
 * owns the socket) and does the HELLO handshake + echo Call/Reply — no
 * shelling out to a second Rust process. Full schema-driven codegen
 * (`keld gen`, `@keld/schema`) is a later slice. `AppLinkSession` holds one
 * HELLO'd connection so further CALLs do not handshake again. The stock
 * lifecycle consumer waits for the host's final window event, then sends Quit
 * on that same connection, which closes the link on its Reply.
 */

/** Resolves on the host's LastWindowClosed; rejects when the link ends first. */
function lastWindowClosed(session: AppLinkSession): Promise<void> {
  return new Promise((resolve, reject) => {
    session.onLifecycleEvent((event) => {
      if (event === "last-window-closed") resolve();
    });
    session.onEnd(reject);
  });
}

if (import.meta.main) {
  const link = process.env.KELD_APP_LINK;
  if (!link) {
    console.error(
      "KELD-CLI-010: KELD_APP_LINK is unset — run the app with `keld dev`, not `bun` directly.",
    );
    process.exit(1);
  }

  const session = await AppLinkSession.connect(link);
  try {
    // Listen before the first call, so an early host EVENT is not missed.
    const windowsClosed = lastWindowClosed(session);
    const response = await session.echo({ message: "keld", count: 1 });
    console.log(`ipc-echo ok: message=${JSON.stringify(response.message)} count=${response.count}`);
    console.log("{{name}}: main process ready (IPC echo ok)");
    await windowsClosed;
    // Resolves only on the host's `LifecycleResponse::Quit`, then the link closes.
    await session.quit();
    process.exit(0);
  } finally {
    session.close();
  }
}
