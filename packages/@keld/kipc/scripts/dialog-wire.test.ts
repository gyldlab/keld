import { expect, test } from "bun:test";
import {
  decodeDialogReply,
  encodeDialogRequest,
} from "../../api/src/dialog.generated.ts";

test("dialog postcard bytes match the Native literal vectors", () => {
  expect([...encodeDialogRequest({ variant: "Cancel", nonce: 1n })]).toEqual([
    1, 1,
  ]);
  expect([
    ...encodeDialogRequest({
      variant: "Message",
      nonce: 1n,
      parent: null,
      options: {
        message: "x",
        kind: "Question",
        buttons: [],
        default_id: null,
        cancel_id: null,
        normalize_access_keys: false,
        title: null,
        detail: null,
        no_link: true,
        checkbox_label: null,
        checkbox_checked: null,
      },
    }),
  ]).toEqual([0, 1, 0, 1, 120, 3, 0, 0, 0, 0, 0, 0, 1, 0, 0]);
  expect(decodeDialogReply(new Uint8Array([0, 2, 0]))).toEqual({
    variant: "Message",
    response: 2,
    checkbox_checked: false,
  });
  expect(decodeDialogReply(new Uint8Array([1, 0]))).toEqual({
    variant: "Cancel",
    disposition: "Requested",
  });
  for (const bytes of [
    [1, 0, 1],
    [0, 2, 2],
    [2, 0],
    [1, 2],
  ])
    expect(() => decodeDialogReply(new Uint8Array(bytes))).toThrow(
      "KELD-IPC-003",
    );
  expect(() => encodeDialogRequest({ variant: "Cancel", nonce: 0n })).toThrow(
    "KELD-IPC-003",
  );
});

test("dialog encoder applies option bounds before transport", () => {
  const options = {
    message: "x".repeat(16372),
    kind: "Question" as const,
    buttons: [] as string[],
    default_id: null,
    cancel_id: null,
    normalize_access_keys: false,
    title: null,
    detail: null,
    no_link: true,
    checkbox_label: null,
    checkbox_checked: null,
  };
  const request = {
    variant: "Message" as const,
    nonce: 1n,
    parent: null,
    options,
  };
  expect(encodeDialogRequest(request).byteLength).toBe(16387);
  expect(() =>
    encodeDialogRequest({
      ...request,
      options: { ...options, message: options.message + "x" },
    }),
  ).toThrow("KELD-IPC-003");
  const small = { ...options, message: "x", buttons: Array(16).fill("") };
  expect(() =>
    encodeDialogRequest({ ...request, options: small }),
  ).not.toThrow();
  expect(() =>
    encodeDialogRequest({
      ...request,
      options: { ...small, buttons: Array(17).fill("") },
    }),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeDialogRequest({
      ...request,
      options: { ...small, checkbox_checked: false },
    }),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeDialogRequest({
      ...request,
      options: { ...small, detail: "embedded\0text", checkbox_label: "" },
    }),
  ).not.toThrow();
  expect(() =>
    encodeDialogRequest({
      ...request,
      options: { ...small, checkbox_label: "", checkbox_checked: false },
    }),
  ).toThrow("KELD-IPC-003");
});
