/**
 * App-main body appended to the wire-tested kipc client when `keld create`
 * renders `src/main.ts`. Bun opens the app-link socket itself and does the
 * HELLO handshake + echo Call/Reply — no shelling out to a second Rust
 * process. Full schema-driven codegen (`keld gen`, `@keld/schema`) is a later
 * slice. `AppLinkSession` holds one HELLO'd connection so further CALLs do not
 * handshake again. The stock lifecycle consumer waits for the host's final
 * window event, sends Quit on that same connection, then exits after its Reply.
 */
import {
  LIFECYCLE_CHANNEL,
  lifecycleReplyWaiter,
} from "./kipc-transport.ts";

function decodeStockLifecycleEvent(payload: Uint8Array): "ready" | "last-window-closed" {
  if (payload.length !== 1) {
    throw kipcError("KELD-IPC-003", "lifecycle event must be one postcard enum byte");
  }
  if (payload[0] === 0) return "ready";
  if (payload[0] === 1) return "last-window-closed";
  throw kipcError("KELD-IPC-003", `unknown LifecycleEvent discriminant ${payload[0]}`);
}

function assertStockQuitReply(payload: Uint8Array): void {
  if (payload.length !== 1 || payload[0] !== 0) {
    throw kipcError("KELD-IPC-003", "Quit Reply must contain LifecycleResponse::Quit");
  }
}

async function quitAfterLastWindowClosed(session: AppLinkSession): Promise<void> {
  while (true) {
    const frame = await session.receiveWhileIdle(RECEIVE_POLICIES.lifecycleEventReceiver);
    if (frame.header.kind === FrameKind.Ping) {
      await session.writeFrame(
        FrameKind.Ping,
        frame.header.channel,
        frame.header.corr,
        new Uint8Array(),
      );
      continue;
    }
    if (decodeStockLifecycleEvent(frame.payload) === "ready") continue;

    const corr = session.allocCorr();
    await session.writeFrame(FrameKind.Call, LIFECYCLE_CHANNEL, corr, encodeVarint(0));
    const reply = await session.receive(lifecycleReplyWaiter(corr));
    if (reply.header.kind === FrameKind.Err) {
      throw kipcError("KELD-IPC-005", "host rejected the stock lifecycle Quit Call");
    }
    assertStockQuitReply(reply.payload);
    return;
  }
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
    const response = await session.echo({ message: "keld", count: 1 });
    console.log(`ipc-echo ok: message=${JSON.stringify(response.message)} count=${response.count}`);
    console.log("{{name}}: main process ready (IPC echo ok)");
    await quitAfterLastWindowClosed(session);
    session.close();
    process.exit(0);
  } finally {
    session.close();
  }
}
