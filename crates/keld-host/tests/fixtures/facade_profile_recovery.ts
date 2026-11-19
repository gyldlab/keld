// Public API caller for one real rendererless Host, repeated in its G1/G2 roles.
import { createConnection } from "node:net";
import { once } from "node:events";
import { createInterface } from "node:readline";

let readyCount = 0;
let openCount = 0;
let receivesWindows = false;
let finishEnd: () => void = () => {
  throw new Error("end observer not installed");
};
const ended = new Promise<void>((resolve) => {
  finishEnd = resolve;
});
const open = WorkerLink.open;
WorkerLink.open = async (options) => {
  openCount += 1;
  if (openCount !== 1) throw new Error("second application link attempted");
  receivesWindows = options.receive.eventChannels.includes(WINDOW_CHANNEL);
  const actual = await open.call(WorkerLink, options);
  // Passive observer on the actual canonical link. Return that same object;
  // no fake transport, replacement callback or extra app connection is used.
  actual.onEvent(LIFECYCLE_CHANNEL, (payload) => {
    if (payload.length === 1 && payload[0] === 0) readyCount += 1;
  });
  actual.onEnd(() => finishEnd());
  return actual;
};

const control = createConnection(process.env.KELD_T1B_CONTROL!);
const commands = createInterface({ input: control });
const report = (line: string): void => {
  control.write(`${line}\n`);
};
await once(control, "connect");
report(`HELLO ${process.pid} ${process.env.KELD_APP_LINK}`);
report("DESCENDANT 0");
try {
  for await (const command of commands) {
    if (command === "ADMIT") {
      await app.whenReady();
      report("READY");
      report(
        `ROLE FACADE_HINT ${Number(nativeFacadeBootSelected)} RECEIVE_WINDOW ${Number(receivesWindows)} MIRROR_WINDOWS ${getWindowSnapshots().length}`,
      );
    } else if (command === "CHECK_READY") {
      report(`READY_COUNT ${readyCount} OPEN_COUNT ${openCount}`);
    } else if (command === "RUN1" || command === "RUN2") {
      const generation = command === "RUN1" ? 1 : 2;
      const bytes = new Uint8Array([generation, 0, 255]);
      let read: number[];
      try {
        await fs.write(__TARGET__, bytes);
        read = [...(await fs.read(__TARGET__))];
      } catch (error: unknown) {
        if (!isCallError(error)) throw error;
        report(`FS_ERROR ${generation} ${error.code}`);
        continue;
      }
      let denial = "unexpected-allow";
      try {
        await fs.write(__OUTSIDE__, new Uint8Array([9]));
      } catch (error: unknown) {
        if (!isCallError(error)) throw error;
        denial = error.code;
      }
      report(
        `FS ${generation} ${JSON.stringify(read)} ${denial} ${readyCount}`,
      );
    } else if (command === "QUIT") {
      await app.quit();
      report("QUIT_REPLY");
      await ended;
      report(`READY_COUNT ${readyCount} OPEN_COUNT ${openCount}`);
      report("LINK_EOF");
      break;
    } else {
      throw new Error(`unexpected fixture command ${command}`);
    }
  }
} finally {
  control.end();
}
