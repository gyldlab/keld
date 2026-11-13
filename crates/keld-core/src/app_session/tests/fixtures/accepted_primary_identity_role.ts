// Controlled authenticated role: existing WorkerLink and owning scaffold codec.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import {
  WorkerLink,
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
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
const link = await WorkerLink.open({
  link: process.env.KELD_APP_LINK!,
  receive: { eventChannels: [LIFECYCLE_CHANNEL], callReceivers: [] },
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
  } else if (command === "STOP") {
    link.close();
    process.exit(0);
  } else {
    throw new Error("unknown identity fixture command");
  }
}
