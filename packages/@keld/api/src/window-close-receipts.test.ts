import { expect, test } from "bun:test";
import { WindowCloseReceipts } from "./window-close-receipts.ts";

const WINDOW = { id: 7, incarnation: 300n };
const STATE = {
  bounds: { x: 0, y: 0, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};

test("dispatched pending close retains a newer sequence across old Veto ack", () => {
  const receipts = new WindowCloseReceipts();
  receipts.observe({
    variant: "Created",
    window: WINDOW,
    state: STATE,
    pending_close: 7n,
  });
  const pending = () => receipts.pending().map(({ closeSeq }) => closeSeq);
  expect(pending()).toEqual([7n]);
  receipts.acceptHostCloseReply({ ...WINDOW, id: 8 }, 7n);
  receipts.acceptHostCloseReply({ ...WINDOW, incarnation: 301n }, 7n);
  receipts.acceptHostCloseReply(WINDOW, 6n);
  expect(pending()).toEqual([7n]);
  receipts.acceptHostCloseReply(WINDOW, 7n);
  expect(pending()).toEqual([]);

  receipts.observe({
    variant: "CloseRequested",
    window: WINDOW,
    close_seq: 8n,
  });
  receipts.acceptHostCloseReply(WINDOW, 7n);
  expect(pending()).toEqual([8n]);
  receipts.acceptHostCloseReply(WINDOW, 8n);
  expect(pending()).toEqual([]);
  receipts.observe({ variant: "Destroyed", window: WINDOW });
  expect(receipts.pending()).toEqual([]);
});

test("Veto ack arriving before retained Created/CloseRequested dispatch suppresses only older sequence", () => {
  const receipts = new WindowCloseReceipts();
  const pending = () => receipts.pending().map(({ closeSeq }) => closeSeq);
  // The authentic source already applied Created(pending7) but dispatch was
  // blocked on an unrelated sync CALL; the accepted Veto reply came first.
  receipts.acceptHostCloseReply(WINDOW, 7n);
  receipts.observe({
    variant: "Created",
    window: WINDOW,
    state: STATE,
    pending_close: 7n,
  });
  expect(pending()).toEqual([]);
  receipts.observe({
    variant: "CloseRequested",
    window: WINDOW,
    close_seq: 7n,
  });
  expect(pending()).toEqual([]);
  receipts.observe({
    variant: "CloseRequested",
    window: WINDOW,
    close_seq: 8n,
  });
  expect(pending()).toEqual([8n]);
  receipts.acceptHostCloseReply(WINDOW, 8n);
  expect(pending()).toEqual([]);
  receipts.observe({
    variant: "CloseRequested",
    window: WINDOW,
    close_seq: 7n,
  });
  expect(pending()).toEqual([]);
});

// Poisoning runs in a disposable process so no Bun runner/native client sees it.
for (const mode of [
  "control",
  "constructor",
  "get",
  "set",
  "delete",
  "values",
  "next",
  "freeze",
  "push",
  "array-iterator",
  "numeric-setter",
]) {
  test(`receipt ledger preserves authenticated catch-up under ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        `${import.meta.dir}/window-close-receipts.test-role.ts`,
        mode,
      ],
      { stdin: "ignore", stdout: "pipe", stderr: "pipe" },
    );
    try {
      const [code, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect({ code, stdout, stderr }).toEqual({
        code: 0,
        stdout: `PASS receipt-integrity ${mode}\n`,
        stderr: "",
      });
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  }, 10_000);
}
