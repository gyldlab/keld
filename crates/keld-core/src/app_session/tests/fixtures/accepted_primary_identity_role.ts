// Controlled authenticated role: existing WorkerLink and owning scaffold codec.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import {
  WorkerLink,
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  DIALOG_CHANNEL,
} from "../../../../../../packages/@keld/kipc/src/transport.ts";
import { encodeDialogRequest } from "../../../../../../packages/@keld/api/src/dialog.generated.ts";
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
  } else if (
    command === "DIALOG_DENIED" ||
    command === "DIALOG_ALLOWED_PARENTLESS" ||
    command === "DIALOG_UNKNOWN_CANCEL"
  ) {
    // A denied manifest must refuse even malformed payload before any native
    // effect. The allowed control uses the real generated Message codec and
    // expects the typed, not-yet-supported parentless outcome. The last case
    // exercises Cancel decoding without creating a new native reservation.
    const payload =
      command === "DIALOG_DENIED"
        ? Uint8Array.of(0xff)
        : encodeDialogRequest(
            command === "DIALOG_UNKNOWN_CANCEL"
              ? { variant: "Cancel", nonce: 9n }
              : {
                  variant: "Message",
                  nonce: 1n,
                  parent: null,
                  options: {
                    message: "Unsaved changes",
                    kind: "Question",
                    buttons: ["Save", "Cancel", "Discard Changes"],
                    default_id: 0,
                    cancel_id: 1,
                    normalize_access_keys: true,
                    title: "Confirm",
                    detail: null,
                    no_link: true,
                    checkbox_label: null,
                    checkbox_checked: null,
                  },
                },
          );
    try {
      await link.call(DIALOG_CHANNEL, payload, 5000);
      control.write("DIALOG-UNEXPECTED-SUCCESS\n");
    } catch (error) {
      const code = (error as { code?: unknown } | null)?.code;
      control.write(
        `DIALOG-${typeof code === "string" ? code : "UNKNOWN-ERROR"}\n`,
      );
    }
  } else if (command === "STOP") {
    link.close();
    process.exit(0);
  } else {
    throw new Error("unknown identity fixture command");
  }
}
