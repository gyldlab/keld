import { expect, test } from "bun:test";
import {
  MENU_ROLES,
  decodeMenuInstallReceipt,
  encodeMenuRequest,
  type MenuNodeV1,
} from "./menu.generated.ts";
import { encodeCallError, errorFromErrFrame, linkError } from "@keld/kipc";

test("GH480 peer ERR provenance identifies its diagnostic owner without claiming effect or dispatch", () => {
  const peer = errorFromErrFrame(
    encodeCallError("KELD-IPC-023", "Host effect unknown; do not replay"),
  ) as ReturnType<typeof linkError>;
  expect(peer.origin).toBe("peer");
  expect(peer.code).toBe("KELD-IPC-023");
  expect(peer.message).toContain("effect unknown");
  expect(Object.keys(peer)).not.toContain("origin");
  expect(Object.getOwnPropertyDescriptor(peer, "origin")?.writable).toBe(false);
  expect(
    linkError("KELD-IPC-007", "definitely not sent").origin,
  ).toBeUndefined();
});

const plain: MenuNodeV1 = {
  depth: 0,
  kind: "Normal",
  label: "X",
  enabled: true,
  checked: null,
  action: { variant: "NativeRole", role: "copy" },
};
test("GH480 menu wire fields have exact independent Postcard bytes", () => {
  // Vec count, u8 depth, kind enum, UTF-8 string, bool, Option None,
  // action enum NativeRole and role enum Copy (single source ordinal 8).
  expect([...encodeMenuRequest({ nodes: [plain] })]).toEqual([
    1, 0, 0, 1, 88, 1, 0, 0, 8,
  ]);
  expect([
    ...encodeMenuRequest({
      nodes: [
        {
          ...plain,
          kind: "Checkbox",
          checked: false,
          action: { variant: "UnsupportedClick" },
        },
      ],
    }),
  ]).toEqual([1, 0, 3, 1, 88, 1, 1, 0, 1]);
  expect(encodeMenuRequest({ nodes: [] })).toEqual(new Uint8Array([0]));
  expect(decodeMenuInstallReceipt(new Uint8Array([172, 2])).generation).toBe(
    300n,
  );
  for (const bytes of [[], [0], [1, 0], [128]])
    expect(() => decodeMenuInstallReceipt(new Uint8Array(bytes))).toThrow(
      "KELD-IPC-003",
    );
});
test("GH480 codec refuses invalid structure, kinds/check state/actions and actual envelope overflow", () => {
  for (const node of [
    { ...plain, depth: 1 },
    { ...plain, depth: 9 },
    { ...plain, label: "\ud800" },
    { ...plain, label: "x\0y" },
    { ...plain, checked: false },
    { ...plain, kind: "Checkbox", checked: null },
    { ...plain, kind: "Separator" },
    { ...plain, action: { variant: "NativeRole", role: "unknown" } },
  ])
    expect(() => encodeMenuRequest({ nodes: [node as MenuNodeV1] })).toThrow(
      "KELD-IPC-003",
    );
  expect(() =>
    encodeMenuRequest({ nodes: [plain, { ...plain, depth: 1 }] }),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeMenuRequest({ nodes: Array.from({ length: 257 }, () => plain) }),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeMenuRequest({
      nodes: Array.from({ length: 256 }, () => ({
        ...plain,
        label: "x".repeat(4096),
      })),
    }),
  ).toThrow("128 KiB");
  expect(MENU_ROLES.hideothers.modifiers).toBe(5);
  expect(MENU_ROLES.pasteandmatchstyle.modifiers).toBe(7);
  expect(MENU_ROLES.close.key).toBe("w");
});
