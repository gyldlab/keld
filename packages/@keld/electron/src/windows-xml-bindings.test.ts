import { expect, test } from "bun:test";
import { resolve } from "node:path";
import {
  clipboard,
  ClipboardItem,
  nativeImage,
  shell,
} from "./unsupported-bindings.ts";
import {
  isUnsupportedMemberError,
  UnsupportedMemberError,
} from "./unsupported.ts";

// GH565 AC-3/4/5/8. The approved Windows census is independent of the table.
test.skipIf(process.platform !== "win32")(
  "Windows XML bindings preserve the pinned throw and rejected Promise shapes",
  async () => {
    expect(Object.keys(shell)).toEqual([
      "showItemInFolder",
      "openPath",
      "openExternal",
      "trashItem",
      "beep",
      "writeShortcutLink",
      "readShortcutLink",
    ]);
    expect(Object.keys(clipboard)).toEqual([
      "readText",
      "writeText",
      "read",
      "write",
      "has",
      "clear",
    ]);
    expect(Object.keys(nativeImage)).toEqual([
      "createEmpty",
      "createThumbnailFromPath",
      "createFromPath",
      "createFromBitmap",
      "createFromBuffer",
      "createFromDataURL",
      "createFromNamedImage",
      "createMenuSymbol",
    ]);
    expect(
      Object.isFrozen(shell) &&
        Object.isFrozen(clipboard) &&
        Object.isFrozen(nativeImage),
    ).toBe(true);
    expect(shell.openExternal.name).toBe("openExternal");
    for (const [call, member, ticket] of [
      [
        () => shell.openExternal("https://example.invalid"),
        "shell.openExternal",
        "GH-479",
      ],
      [() => clipboard.readText(), "clipboard.readText", "GH-559"],
      [() => clipboard.writeText("text"), "clipboard.writeText", "GH-559"],
      [() => clipboard.write([]), "clipboard.write", "GH-559"],
    ] as const) {
      const result = call();
      expect(result).toBeInstanceOf(Promise);
      try {
        await result;
        throw new Error("unsupported promise fulfilled");
      } catch (error) {
        expect(error).toBeInstanceOf(UnsupportedMemberError);
        expect(error).toMatchObject({
          code: "KELD-COMPAT-013",
          kind: "promise",
          member,
          ticket,
        });
      }
    }
    for (const [call, member, ticket] of [
      [
        () => nativeImage.createFromDataURL("data:,"),
        "nativeImage.createFromDataURL",
        "GH-559",
      ],
      [
        () => shell.showItemInFolder("invalid"),
        "shell.showItemInFolder",
        "GH-479",
      ],
      [() => clipboard.clear(), "clipboard.clear", "GH-476"],
    ] as const) {
      let refusal: unknown;
      try {
        call();
      } catch (error) {
        refusal = error;
      }
      expect(isUnsupportedMemberError(refusal)).toBe(true);
      expect(refusal).toMatchObject({
        code: "KELD-COMPAT-013",
        kind: "method",
        member,
        ticket,
      });
    }
  },
);

test("ClipboardItem AC8 snapshots each own enumerable payload once", async () => {
  const inherited = Object.create({ "text/plain": "inherited" });
  for (const args of [
    [],
    [null],
    [1],
    [[]],
    [{}],
    [inherited],
    [{ [Symbol("mime")]: "x" }],
  ]) {
    expect(() => Reflect.construct(ClipboardItem, args)).toThrow(TypeError);
  }
  let reads = 0;
  const hidden = {
    get "text/plain"() {
      reads++;
      return "payload";
    },
  };
  Object.defineProperty(hidden, "ignored", {
    get() {
      throw new Error("hidden read");
    },
  });
  const item = new ClipboardItem(hidden);
  expect(reads).toBe(1);
  expect(() => item.types).toThrow("KELD-COMPAT-013");
  await expect(item.getType("text/plain")).rejects.toThrow("KELD-COMPAT-013");
  expect(reads).toBe(1);
  const sourceFailure = new Error("enumerable payload read");
  expect(
    () =>
      new ClipboardItem({
        get "text/plain"() {
          throw sourceFailure;
        },
      }),
  ).toThrow(sourceFailure);
});

test("ClipboardItem AC8 mixed keys snapshot each enumerable getter once", () => {
  const symbol = Symbol("enumerable MIME sidecar");
  const reads: (string | symbol)[] = [];
  new ClipboardItem({
    get "text/plain"() {
      reads.push("text/plain");
      return "text";
    },
    get [symbol]() {
      reads.push(symbol);
      return "sidecar";
    },
  });
  expect(reads).toEqual(["text/plain", symbol]);
});

test("ClipboardItem AC8 propagates an enumerable symbol getter failure", () => {
  const symbol = Symbol("throwing MIME sidecar");
  const failure = new Error("enumerable symbol payload read");
  let calls = 0;
  let thrown: unknown;
  try {
    new ClipboardItem({
      "text/plain": "text",
      get [symbol]() {
        calls++;
        throw failure;
      },
    });
  } catch (error) {
    thrown = error;
  }
  expect(thrown).toBe(failure);
  expect(calls).toBe(1);
});

test("late four-binding import ignores poisoned hooks and opens zero Host links", () => {
  const fixture = resolve(
    import.meta.dir,
    "../fixtures/windows_xml_bindings_no_link.ts",
  );
  const child = Bun.spawnSync(
    [process.execPath, "--no-install", "--no-env-file", fixture],
    {
      env: { ...process.env, KELD_APP_LINK: `1#${"ab".repeat(32)}` },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4_000,
    },
  );
  expect(child.exitCode, child.stderr.toString()).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    connects: 0,
    hooks: 0,
    reads: 1,
    linkKinds: ["object", "function", "object", "object"],
    shell: ["KELD-COMPAT-013", "promise", "shell.openExternal", "GH-479"],
    clipboard: ["KELD-COMPAT-013", "promise", "clipboard.write", "GH-559"],
    nativeImage: [
      "KELD-COMPAT-013",
      "method",
      "nativeImage.createFromDataURL",
      "GH-559",
    ],
    invalid: true,
    goodType: true,
  });
});
