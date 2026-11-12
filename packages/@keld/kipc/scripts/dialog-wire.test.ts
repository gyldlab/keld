import { expect, test } from "bun:test";
import {
  decodeDialogReply,
  encodeDialogRequest,
} from "../../api/src/dialog.generated.ts";
import type { OpenDialogOptions } from "../../api/src/dialog.generated.ts";

test("Open/Save append exact Rust postcard variants without renumbering Message/Cancel", () => {
  const open = {
    variant: "Open" as const,
    nonce: 1n,
    parent: null,
    options: {
      default_path: null,
      filters: [],
      properties: ["OpenFile", "OpenDirectory"] as const,
    },
  };
  const save = {
    variant: "Save" as const,
    nonce: 1n,
    parent: null,
    options: {
      default_path: null,
      filters: [],
      security_scoped_bookmarks: false,
    },
  };
  expect([...encodeDialogRequest(open)]).toEqual([2, 1, 0, 0, 0, 2, 0, 1]);
  expect([...encodeDialogRequest(save)]).toEqual([3, 1, 0, 0, 0, 0]);
  expect(decodeDialogReply(Uint8Array.of(2, 1, 0))).toEqual({
    variant: "Open",
    canceled: true,
    file_paths: [],
  });
  expect(decodeDialogReply(Uint8Array.of(3, 1, 0))).toEqual({
    variant: "Save",
    canceled: true,
    file_path: "",
  });
  expect([...encodeDialogRequest({ variant: "Cancel", nonce: 1n })]).toEqual([
    1, 1,
  ]);
  expect(decodeDialogReply(Uint8Array.of(0, 2, 0))).toEqual({
    variant: "Message",
    response: 2,
    checkbox_checked: false,
  });
});

test("Open filters keep groups and the exact combined file/directory properties", () => {
  const source = {
    variant: "Open" as const,
    nonce: 12n,
    parent: null,
    options: {
      default_path: "file.drawio",
      filters: [
        { name: "draw.io", extensions: ["drawio", "xml"] },
        { name: "SVG", extensions: ["svg"] },
      ],
      properties: ["OpenFile", "OpenDirectory", "MultiSelections"] as const,
    },
  };
  const bytes = encodeDialogRequest(source);
  expect(bytes[0]).toBe(2);
  expect(bytes[1]).toBe(12);
  expect(bytes.byteLength).toBeLessThanOrEqual(16 * 1024 + 32);
  expect([...bytes].slice(-4)).toEqual([3, 0, 1, 2]);
  expect([...bytes].filter((value) => value === 3).length).toBeGreaterThan(0);
});

test("Open/Save rejects unsafe filters, bookmark authority and excess vectors before native call", () => {
  const openOptions = {
    default_path: null,
    filters: [{ name: "draw.io", extensions: ["drawio"] }],
    properties: ["OpenFile"] as string[],
  };
  const open = (options: unknown) =>
    encodeDialogRequest({
      variant: "Open",
      nonce: 1n,
      parent: null,
      // Deliberately adversarial dynamic inputs must reach the generated
      // runtime validator, not be rejected merely by TypeScript's static type.
      options: options as OpenDialogOptions,
    });
  for (const extensions of [
    [],
    ["*.drawio"],
    [".xml"],
    ["x/y"],
    ["drawio", "drawio"],
    ["a".repeat(129)],
    ["*", "xml"],
    ["bad."],
    ["a\u007f"],
  ]) {
    expect(() =>
      open({ ...openOptions, filters: [{ name: "File", extensions }] }),
    ).toThrow("KELD-IPC-003");
  }
  expect(() =>
    open({ ...openOptions, filters: Array(33).fill(openOptions.filters[0]) }),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    open({ ...openOptions, properties: ["OpenFile", "OpenFile"] }),
  ).toThrow("KELD-IPC-003");
  expect(() => open({ ...openOptions, properties: ["Other"] })).toThrow(
    "KELD-IPC-003",
  );
  expect(() =>
    open({ ...openOptions, default_path: "x".repeat(4097) }),
  ).toThrow("KELD-IPC-003");
  const save = (bookmark: boolean) =>
    encodeDialogRequest({
      variant: "Save",
      nonce: 1n,
      parent: null,
      options: {
        default_path: null,
        filters: [],
        security_scoped_bookmarks: bookmark,
      },
    });
  expect(() => save(true)).toThrow("KELD-IPC-003");
  expect(() => save(false)).not.toThrow();
});

test("native Open/Save replies reject empty successes, leaking canceled paths and unsafe POSIX paths", () => {
  const postcardString = (path: string): Uint8Array => {
    const data = new TextEncoder().encode(path);
    expect(data.byteLength).toBeLessThan(128);
    return Uint8Array.from([data.byteLength, ...data]);
  };
  const path = "/tmp/diagram.drawio";
  expect(
    decodeDialogReply(Uint8Array.from([2, 0, 1, ...postcardString(path)])),
  ).toEqual({ variant: "Open", canceled: false, file_paths: [path] });
  expect(
    decodeDialogReply(Uint8Array.from([3, 0, ...postcardString(path)])),
  ).toEqual({ variant: "Save", canceled: false, file_path: path });
  for (const invalid of [
    "",
    "./relative",
    "../escape",
    "C:\\Windows",
    "/tmp/a\0b",
    "/tmp/a\\b",
    "/a/../b",
  ]) {
    expect(() =>
      decodeDialogReply(Uint8Array.from([2, 0, 1, ...postcardString(invalid)])),
    ).toThrow("KELD-IPC-003");
    expect(() =>
      decodeDialogReply(Uint8Array.from([3, 0, ...postcardString(invalid)])),
    ).toThrow("KELD-IPC-003");
  }
  expect(() => decodeDialogReply(Uint8Array.of(2, 0, 0))).toThrow(
    "KELD-IPC-003",
  );
  expect(() =>
    decodeDialogReply(Uint8Array.from([2, 1, 1, ...postcardString(path)])),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    decodeDialogReply(Uint8Array.from([3, 1, ...postcardString(path)])),
  ).toThrow("KELD-IPC-003");
  expect(() => decodeDialogReply(Uint8Array.of(2, 1, 0, 0))).toThrow(
    "KELD-IPC-003",
  );
});

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
  const empty = { ...options, message: "x" };
  expect(() =>
    encodeDialogRequest({ ...request, options: { ...empty, default_id: 1 } }),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeDialogRequest({
      ...request,
      options: { ...empty, kind: "Invalid" as "Question" },
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

// Literal owned by the Rust dialog_wire parent-present test; its two identity
// halves differ and each uses a multi-byte postcard varint.
test("dialog parent ref preserves Rust field order", () => {
  const options = {
    message: "x",
    kind: "Question" as const,
    buttons: [],
    default_id: null,
    cancel_id: null,
    normalize_access_keys: false,
    title: null,
    detail: null,
    no_link: true,
    checkbox_label: null,
    checkbox_checked: null,
  };
  expect([
    ...encodeDialogRequest({
      variant: "Message",
      nonce: 1n,
      parent: { id: 300, incarnation: 513n },
      options,
    }),
  ]).toEqual([
    0, 1, 1, 0xac, 0x02, 0x81, 0x04, 1, 120, 3, 0, 0, 0, 0, 0, 0, 1, 0, 0,
  ]);
});
