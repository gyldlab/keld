const { parseAppLink, WriteQueue, DrainSignal, FrameKind, HANDSHAKE_CHANNEL, withIoDeadline } =
  await import(process.env.KELD_RECOVERY_PROBE_TRANSPORT);
const old = parseAppLink(process.env.KELD_RECOVERY_PROBE_OLD);
const fresh = parseAppLink(process.env.KELD_RECOVERY_PROBE_FRESH);
const drain = new DrainSignal();
let resolveEnd, rejectEnd;
const refused = new Promise((resolve, reject) => { resolveEnd = resolve; rejectEnd = reject; });
void refused.catch(() => {});
const socket = await Bun.connect({ unix: fresh.endpoint, socket: {
  data() { rejectEnd(new Error("retired token received server reply bytes")); },
  drain() { drain.fire(); },
  end() { resolveEnd(); },
  close() { resolveEnd(); },
  error(_socket, error) {
    if (error.code === "ECONNRESET" || error.code === "ECONNABORTED") resolveEnd();
    else rejectEnd(error);
  },
} });
try {
  await withIoDeadline(new WriteQueue(socket, drain).writeFrame(FrameKind.Hello, 0, HANDSHAKE_CHANNEL, 0, old.token));
  await withIoDeadline(refused);
  console.log("RETIRED_TOKEN_SERVER_EOF_NO_REPLY");
} finally { socket.end(); }
