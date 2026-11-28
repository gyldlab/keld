// Controlled authenticated role: existing WorkerLink and owning scaffold codec.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import {
  WorkerLink,
  ECHO_CHANNEL,
  FS_CHANNEL,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
} from "../../../../../../packages/@keld/kipc/src/transport.ts";
import {
  encodeEchoRequest,
  decodeEchoResponse,
} from "../../../../../keld-cli/templates/hello/src/kipc.ts";

const control = createConnection({
  host: "127.0.0.1",
  port: Number(process.argv[2]),
});
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
const lines = createInterface({ input: control });
const conditionalFixture = process.argv[3] === "gh451";
const link = await WorkerLink.open({
  link: process.env.KELD_APP_LINK!,
  receive: {
    eventChannels: conditionalFixture
      ? [LIFECYCLE_CHANNEL, WINDOW_CHANNEL]
      : [LIFECYCLE_CHANNEL],
    callReceivers: [],
  },
});
control.write("BOUND\n");
for await (const command of lines) {
  if (command === "CRASH") process.exit(17);
  if (command === "ECHO") {
    const response = decodeEchoResponse(
      await link.call(
        ECHO_CHANNEL,
        encodeEchoRequest({ message: "accepted-g2", count: 97 }),
        5000,
      ),
    );
    control.write(
      response.message === "accepted-g2" && response.count === 97
        ? "ECHO-OK\n"
        : "ECHO-BAD\n",
    );
  } else if (conditionalFixture && command.startsWith("GH451 ")) {
    // Only the isolated Rust test supplies these existing codec payloads.
    // A carrier Quit has no new response semantics; the Host test observes
    // its draft cut out of band. Never call quitAndCloseLink on this fixture.
    const [, operation, hex] = command.split(" ");
    const channel =
      operation === "FS"
        ? FS_CHANNEL
        : operation === "CREATE"
          ? WINDOW_CHANNEL
          : operation === "SUBSCRIBE" || operation === "CARRIER"
            ? LIFECYCLE_CHANNEL
            : undefined;
    if (channel === undefined || !hex || !/^(?:[0-9a-f]{2})+$/.test(hex))
      throw new Error("invalid GH451 fixture control");
    const payload = Uint8Array.from(Buffer.from(hex, "hex"));
    const answer = link.call(channel, payload, 20000).then(
      (reply) =>
        control.write(
          `${operation}-REPLY:${Buffer.from(reply).toString("hex")}\n`,
        ),
      (error) =>
        control.write(`${operation}-ERR:${error?.code ?? "unknown"}\n`),
    );
    control.write(`${operation}-QUEUED\n`);
    // The test controls pending FS/creation/terminal work independently.
    if (operation === "SUBSCRIBE") await answer;
    else void answer;
  } else if (command === "STOP") {
    link.close();
    process.exit(0);
  } else {
    throw new Error("unknown identity fixture command");
  }
}
