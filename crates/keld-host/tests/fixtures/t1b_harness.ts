
const KEL96_ECHO_CHANNEL = 1;
const KEL96_CONTROL = process.env.KELD_T1B_CONTROL;
const KEL96_LINK = process.env.KELD_APP_LINK;
if (!KEL96_CONTROL || !KEL96_LINK) {
  throw new Error("KEL96 fixture requires KELD_T1B_CONTROL and KELD_APP_LINK");
}

const textEncoder = new TextEncoder();
const controlDecoder = new TextDecoder();
const controlDrain = new DrainSignal();
let controlBuffer = "";
let resolveCommand: ((command: string) => void) | undefined;
const queuedCommands: string[] = [];

function receiveCommand(): Promise<string> {
  const queued = queuedCommands.shift();
  if (queued !== undefined) return Promise.resolve(queued);
  return new Promise((resolve) => {
    resolveCommand = resolve;
  });
}

function deliverCommand(line: string): void {
  const waiter = resolveCommand;
  if (waiter) {
    resolveCommand = undefined;
    waiter(line);
  } else {
    queuedCommands.push(line);
  }
}

const control = await Bun.connect({
  unix: KEL96_CONTROL,
  socket: {
    binaryType: "uint8array",
    data(_socket: unknown, data: Uint8Array) {
      controlBuffer += controlDecoder.decode(data, { stream: true });
      for (;;) {
        const newline = controlBuffer.indexOf("\n");
        if (newline < 0) break;
        const line = controlBuffer.slice(0, newline);
        controlBuffer = controlBuffer.slice(newline + 1);
        deliverCommand(line);
      }
    },
    drain() {
      controlDrain.fire();
    },
    error(_socket: unknown, error: Error) {
      throw error;
    },
  },
});

let controlWriteChain: Promise<void> = Promise.resolve();

function sendControl(line: string): Promise<void> {
  const run = controlWriteChain.then(() => writeControlLine(line));
  controlWriteChain = run.catch(() => undefined);
  return run;
}

async function writeControlLine(line: string): Promise<void> {
  const bytes = textEncoder.encode(`${line}\n`);
  let offset = 0;
  while (offset < bytes.length) {
    const written = control.write(bytes.subarray(offset));
    if (written < 0) throw new Error("KEL96 control socket closed");
    offset += written;
    if (written === 0) await controlDrain.wait();
  }
}

const { endpoint, token } = parseAppLink(KEL96_LINK);
const reader = new FrameReader();
const appDrain = new DrainSignal();
let linkClosed = false;
let orderlyQuit = false;
let resolveQuitReplySent: (() => void) | undefined;
const quitReplySent = new Promise<void>((resolve) => {
  resolveQuitReplySent = resolve;
});
let resolveCloseNotified: (() => void) | undefined;
const closeNotified = new Promise<void>((resolve) => {
  resolveCloseNotified = resolve;
});
const appSocket = await Bun.connect({
  unix: endpoint,
  socket: {
    binaryType: "uint8array",
    data(_socket: unknown, data: Uint8Array) {
      reader.push(data);
    },
    drain() {
      appDrain.fire();
    },
    error(_socket: unknown, error: Error) {
      reader.fail(error);
    },
    close() {
      linkClosed = true;
      reader.fail(new Error("KEL96 app link closed by host"));
      const notify = async (): Promise<void> => {
        if (orderlyQuit) await quitReplySent;
        await sendControl("LINK_EOF");
      };
      void notify()
        .catch(() => undefined)
        .finally(() => resolveCloseNotified?.());
    },
  },
});
const writes = new WriteQueue(appSocket, appDrain);
await withIoDeadline(writes.writeFrame(FrameKind.Hello, 0, 0, 0, token));
const helloReply = await withIoDeadline(reader.readFrame());
if (helloReply.header.kind !== FrameKind.Hello || helloReply.payload.length !== token.length) {
  throw new Error("KEL96 host did not complete the one authenticated HELLO");
}
for (let index = 0; index < token.length; index += 1) {
  if (helloReply.payload[index] !== token[index]) {
    throw new Error("KEL96 HELLO reply token mismatch");
  }
}
await sendControl(`HELLO ${process.pid} ${KEL96_LINK}`);
// Real-macOS acceptance pins the system tool instead of trusting a caller-controlled PATH.
const descendant = Bun.spawn(["/usr/bin/tail", "-f", "/dev/null"], {
  stdin: "ignore",
  stdout: "ignore",
  stderr: "ignore",
});
await sendControl(`DESCENDANT ${descendant.pid}`);

type PendingReply = {
  resolve: (payload: Uint8Array) => void;
  reject: (error: Error) => void;
};
const pending = new Map<number, PendingReply>();
let nextCorrelation = 1;
let resolveReady: (() => void) | undefined;
const ready = new Promise<void>((resolve) => {
  resolveReady = resolve;
});
let resolveLastWindowClosed: (() => void) | undefined;
const lastWindowClosed = new Promise<void>((resolve) => {
  resolveLastWindowClosed = resolve;
});

const readLoop = (async (): Promise<void> => {
  for (;;) {
    const frame = await reader.readFrame();
    if (frame.header.kind === FrameKind.Event && frame.header.channel === LIFECYCLE_CHANNEL) {
      if (frame.payload.length !== 1) throw new Error("KEL96 lifecycle Event is malformed");
      if (frame.payload[0] === 0) {
        resolveReady?.();
        resolveReady = undefined;
        continue;
      }
      if (frame.payload[0] === 1) {
        resolveLastWindowClosed?.();
        resolveLastWindowClosed = undefined;
        continue;
      }
      throw new Error(`KEL96 unknown lifecycle Event ${frame.payload[0]}`);
    }
    if (frame.header.kind === FrameKind.Reply) {
      const waiter = pending.get(frame.header.corr);
      if (!waiter) throw new Error(`KEL96 unexpected Reply ${frame.header.corr}`);
      pending.delete(frame.header.corr);
      waiter.resolve(frame.payload);
      continue;
    }
    if (frame.header.kind === FrameKind.Err) {
      const waiter = pending.get(frame.header.corr);
      if (!waiter) throw new Error(`KEL96 unexpected Err ${frame.header.corr}`);
      pending.delete(frame.header.corr);
      waiter.reject(errorFromErrFrame(frame.payload));
      continue;
    }
    throw new Error(
      `KEL96 unexpected frame kind=${frame.header.kind} channel=${frame.header.channel}`,
    );
  }
})().catch(async (error: Error) => {
  if (!linkClosed) {
    try {
      await sendControl(`ERROR ${error.message.replaceAll("\n", " ")}`);
    } catch {
      // The controller may already be gone; keep the original link failure terminal.
    }
  }
});
void readLoop;

async function invoke(channel: number, payload: Uint8Array): Promise<Uint8Array> {
  const correlation = nextCorrelation;
  nextCorrelation += 1;
  const reply = new Promise<Uint8Array>((resolve, reject) => {
    pending.set(correlation, { resolve, reject });
  });
  await withIoDeadline(
    writes.writeFrame(FrameKind.Call, 0, channel, correlation, payload),
  );
  return withIoDeadline(reply);
}

function postcardVarint(value: number): Uint8Array {
  const bytes: number[] = [];
  let remaining = value >>> 0;
  for (;;) {
    const byte = remaining & 0x7f;
    remaining >>>= 7;
    bytes.push(remaining === 0 ? byte : byte | 0x80);
    if (remaining === 0) return new Uint8Array(bytes);
  }
}

function echoPayload(message: string, count: number): Uint8Array {
  const text = textEncoder.encode(message);
  const length = postcardVarint(text.length);
  const encodedCount = postcardVarint(count);
  const payload = new Uint8Array(length.length + text.length + encodedCount.length);
  payload.set(length, 0);
  payload.set(text, length.length);
  payload.set(encodedCount, length.length + text.length);
  return payload;
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false;
  return left.every((byte, index) => byte === right[index]);
}

await ready;
await sendControl("READY");
for (const [index, message] of ["first", "second"].entries()) {
  const request = echoPayload(`KEL96-${message}`, index + 1);
  const response = await invoke(KEL96_ECHO_CHANNEL, request);
  if (!sameBytes(request, response)) throw new Error(`KEL96 ${message} echo mismatch`);
  await sendControl(`ECHO${index + 1}`);
}

const requested = await Promise.race([
  receiveCommand(),
  lastWindowClosed.then(async () => {
    await sendControl("LAST_WINDOW_CLOSED");
    return "QUIT";
  }),
]);
if (requested === "EXIT0") {
  process.exit(0);
}
if (requested !== "QUIT") {
  throw new Error(`KEL96 unknown controller command: ${requested}`);
}
orderlyQuit = true;
const response = await invoke(LIFECYCLE_CHANNEL, new Uint8Array([0]));
if (!sameBytes(response, new Uint8Array([0]))) {
  throw new Error("KEL96 correlated Quit response mismatch");
}
await sendControl("QUIT_REPLY");
resolveQuitReplySent?.();
await closeNotified;
process.exit(0);
