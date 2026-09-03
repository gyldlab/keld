/**
 * App-main body appended to the wire-tested kipc client when `keld create`
 * renders `src/main.ts`. Bun opens the app-link socket itself and does the
 * HELLO handshake + echo Call/Reply — no shelling out to a second Rust
 * process. Full schema-driven codegen (`keld gen`, `@keld/schema`) is a later
 * slice. `AppLinkSession` holds one HELLO'd connection so further CALLs do not
 * handshake again. The process stays alive for the host-owned hello window
 * duration; `keld dev` reaps it after the window returns.
 */
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
    const response = await session.echo({ message: "keld", count: 1 });
    console.log(`ipc-echo ok: message=${JSON.stringify(response.message)} count=${response.count}`);
    console.log("{{name}}: main process ready (IPC echo ok)");
    // Stay alive while the host owns the hello window. The host supervisor
    // kills this child on window close (KEL-30 concurrent app-link).
    await new Promise(() => {});
  } finally {
    session.close();
  }
}
