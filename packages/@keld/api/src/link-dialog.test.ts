import { expect, test } from "bun:test";
import { LifecycleLink } from "./link.ts";
import type { MessageOptions } from "./dialog.generated.ts";

const options: MessageOptions = {
  message: "Confirm",
  kind: "Question",
  buttons: ["&Save", "&Discard Changes", "Cancel"],
  default_id: 0,
  cancel_id: 2,
  normalize_access_keys: true,
  title: null,
  detail: null,
  no_link: true,
  checkbox_label: null,
  checkbox_checked: null,
};

test("real LifecycleLink rejects wrong dialog variant and closed session with typed errors", async () => {
  // The real adapter runs against a controlled transport seam; this does not
  // prove a named pipe, native panel, or physical cleanup.
  let bytes = new Uint8Array([1, 0]); // Cancel ACK, not Message
  const transport = {
    callBlocking: () => bytes,
    call: () => Promise.resolve(bytes),
    close: () => {},
  };
  const link = Reflect.construct(LifecycleLink, [transport]) as LifecycleLink;
  try {
    link.callMessageDialogBlocking(null, options);
    throw new Error("expected refusal");
  } catch (error) {
    expect((error as { code?: string }).code).toBe("KELD-IPC-003");
  }
  await expect(link.callMessageDialog(null, options)).rejects.toMatchObject({
    code: "KELD-IPC-003",
  });
  bytes = new Uint8Array([0, 2, 2]); // Invalid boolean in Message reply.
  try {
    link.callMessageDialogBlocking(null, options);
    throw new Error("expected malformed reply refusal");
  } catch (error) {
    expect((error as { code?: string }).code).toBe("KELD-IPC-003");
  }
  await expect(link.callMessageDialog(null, options)).rejects.toMatchObject({
    code: "KELD-IPC-003",
  });
  link.close();
  try {
    link.callMessageDialogBlocking(null, options);
    throw new Error("expected refusal");
  } catch (error) {
    expect((error as { code?: string }).code).toBe("KELD-IPC-001");
  }
});
