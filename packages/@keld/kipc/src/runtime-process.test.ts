import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { renderRuntimeProcessBinding } from "../scripts/echo-codegen.ts";
import {
  encodeRuntimeProcessInit,
  decodeRuntimeProcessInit,
  decodeRuntimeInitialized,
  snapshotRuntimeProcessFacts,
} from "./runtime-process.generated.ts";
import { encodeVarint, encodePostcardString } from "./transport.ts";
const rust = readFileSync(
  new URL("../../../../crates/keld-ipc/src/lifecycle.rs", import.meta.url),
  "utf8",
);
test("runtime Init uses owning append-only tags and immutable own-data snapshot", () => {
  const versions = { z: "2", a: "1" };
  const facts = snapshotRuntimeProcessFacts(versions);
  const bytes = encodeRuntimeProcessInit(facts);
  expect([...bytes]).toEqual([2, 2, 1, 97, 1, 49, 1, 122, 1, 50]);
  versions.a = "changed";
  expect(facts.versions.a).toBe("1");
  expect(Object.isFrozen(facts.versions)).toBe(true);
  expect(decodeRuntimeProcessInit(bytes)).toEqual({
    versions: { a: "1", z: "2" },
  });
  expect(() => decodeRuntimeInitialized(Uint8Array.of(3))).not.toThrow();
  for (const wrong of [[], [1], [3, 0], [4]])
    expect(() => decodeRuntimeInitialized(Uint8Array.from(wrong))).toThrow();
});
test("runtime metadata rejects accessors, duplicate keys, count and byte overages", () => {
  let reads = 0;
  const accessor = Object.defineProperty({}, "a", {
    enumerable: true,
    get() {
      reads++;
      return "1";
    },
  });
  expect(() => snapshotRuntimeProcessFacts(accessor)).toThrow();
  expect(reads).toBe(0);
  for (const bad of [
    {},
    { a: "" },
    { a: "x\0" },
    { ["x\0"]: "1" },
    Object.fromEntries(Array.from({ length: 33 }, (_, i) => [String(i), "1"])),
  ])
    expect(() => snapshotRuntimeProcessFacts(bad)).toThrow();
  expect(() =>
    encodeRuntimeProcessInit(
      snapshotRuntimeProcessFacts({ a: "x".repeat(8191) }),
    ),
  ).not.toThrow();
  expect(() => snapshotRuntimeProcessFacts({ a: "x".repeat(8192) })).toThrow();
  const dup = Buffer.concat([
    Uint8Array.of(2, 2),
    encodePostcardString("a"),
    encodePostcardString("1"),
    encodePostcardString("a"),
    encodePostcardString("2"),
  ]);
  expect(() => decodeRuntimeProcessInit(dup)).toThrow();
  expect(() =>
    decodeRuntimeProcessInit(
      Buffer.concat([Uint8Array.of(2), encodeVarint(33)]),
    ),
  ).toThrow();
  expect(() => decodeRuntimeProcessInit(new Uint8Array(8449))).toThrow();
});
test("runtime generator refuses changed fields and emits derived tags and limits", () => {
  expect(renderRuntimeProcessBinding(rust)).toContain(
    "MAX_RUNTIME_PROCESS_FACTS_PAYLOAD = 8448",
  );
  for (const changed of [
    rust.replace("versions: BTreeMap<String, String>", "versions: Vec<String>"),
    rust.replace(
      "MAX_RUNTIME_VERSION_ENTRIES: usize = 32",
      "MAX_RUNTIME_VERSION_ENTRIES: usize = 33",
    ),
    rust.replace("    Quit,\n", "    Removed,\n"),
  ])
    expect(() => renderRuntimeProcessBinding(changed)).toThrow();
});

test("GH563 generated lifecycle unit events preserve literal bytes and reject malformed intent", async () => {
  const { decodeLifecycleEvent } =
    await import("./runtime-process.generated.ts");
  expect(decodeLifecycleEvent(Uint8Array.of(0))).toBe("ready");
  expect(decodeLifecycleEvent(Uint8Array.of(1))).toBe("last-window-closed");
  expect(decodeLifecycleEvent(Uint8Array.of(2))).toBe("quit-requested");
  for (const bytes of [
    new Uint8Array(),
    Uint8Array.of(3),
    Uint8Array.of(2, 0),
    Uint8Array.of(0x82, 0),
  ])
    expect(() => decodeLifecycleEvent(bytes)).toThrow("KELD-IPC-003");
});
