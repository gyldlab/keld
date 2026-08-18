/**
 * App main process — supervised Bun child (KEL-29 hello template).
 * Speaks kipc directly (KEL-30): Bun opens the app-link socket itself and
 * does the HELLO handshake + echo Call/Reply — no shelling out to a second
 * Rust process. Full schema-driven codegen (`keld gen`, `@keld/schema`) is a
 * later slice; `./kipc.ts` is the hand-written, wire-exact v0 client.
 * `AppLinkSession` holds one HELLO'd connection so further CALLs do not
 * handshake again.
 *
 * `whenReady()`/`quit()` (KEL-72) are the first Electron-compat lifecycle
 * primitives: `whenReady()` blocks on a real host-backed signal (the dev
 * window's creation, or immediate under `run_dev_echo`'s windowless smoke
 * test), and `quit()` ends this app-process session.
 */
import { AppLinkSession } from "./kipc";

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

  await session.whenReady();
  console.log("{{name}}: app ready");

  await session.quit();
  console.log("{{name}}: app quit ok");
} finally {
  session.close();
}

console.log("{{name}}: main process ready (IPC echo ok)");
