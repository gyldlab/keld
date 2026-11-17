/** KEL-140 caller: public outer expiry reported on the same declared Echo link. */
import {
  app,
  channels,
  echoChannel,
  fs,
  isCallError,
} from "../../../../../../packages/@keld/api/src/index.ts";

type Outcome =
  | { status: "fulfilled" }
  | { status: "rejected"; code: string; message: string };

function report(key: string, value: string | number | boolean): void {
  console.log(`KELD_WL ${key}=${value}`);
}

const ready = app.whenReady();
const controlText = Bun.stdin.text();
let handlerCalls = 0;
let settlements = 0;

// Register before awaiting stdin or Ready: an authenticated host may issue its
// declared CALL as soon as it supplies the existing EOF-terminated controls.
channels.handle(echoChannel, async (request) => {
  handlerCalls += 1;
  if (
    handlerCalls !== 1 ||
    request.message !== "outer-deadline" ||
    request.count !== 1
  ) {
    throw new Error("unexpected public outer-deadline fixture CALL");
  }
  const control: unknown = JSON.parse(await controlText);
  if (
    typeof control !== "object" ||
    control === null ||
    !("target" in control) ||
    typeof control.target !== "string" ||
    !("lateTarget" in control) ||
    typeof control.lateTarget !== "string"
  ) {
    throw new Error("invalid private public-deadline fixture control");
  }
  await ready;
  const outcome: Outcome = await fs
    .write(control.target, new Uint8Array([116, 50]))
    .then(
      () => {
        settlements += 1;
        return { status: "fulfilled" } as const;
      },
      (error: unknown) => {
        settlements += 1;
        if (!isCallError(error)) throw error;
        return {
          status: "rejected",
          code: error.code,
          message: error.message,
        } as const;
      },
    );
  report("handler-calls", handlerCalls);
  report("settlements", settlements);
  return {
    message: JSON.stringify({ outcome, settlements }),
    count: request.count,
  };
});

await ready;
// No Quit, reconnect, replay or explicit exit. The host's later retirement
// closes this same canonical Worker link; the existing custodian observes exit.
