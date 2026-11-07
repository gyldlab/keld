/** GH481 candidate: real codec/source contracts; no link or native acceptance. */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";
import { isCallError, linkError } from "@keld/kipc";
import { encodeWindowEvent, type WindowEvent } from "./window-codec.ts";
import { WindowSource } from "./window-source.ts";

function facts(dark = false) {
  return {
    primary_id: 17,
    displays: [
      {
        id: 17,
        bounds: { x: 0, y: 0, width: 1440, height: 900 },
        work_area: { x: 0, y: 24, width: 1440, height: 876 },
        scale_factor: 2,
      },
      {
        id: 23,
        bounds: { x: -1280, y: 0, width: 1280, height: 800 },
        work_area: { x: -1280, y: 0, width: 1280, height: 800 },
        scale_factor: 1,
      },
    ],
    should_use_dark_colors: dark,
  };
}
function capture(epoch = 1n, value: ReturnType<typeof facts> | null = facts()) {
  return {
    variant: "DesktopSnapshot",
    epoch,
    snapshot: value,
  } satisfies WindowEvent;
}
function source(observed: WindowEvent[] = []) {
  return new WindowSource((event) => observed.push(event));
}
function decode(owner: WindowSource, event: WindowEvent) {
  return owner.decode(encodeWindowEvent(event));
}
function diagnostic(action: () => unknown) {
  try {
    action();
  } catch (error) {
    if (!isCallError(error)) throw error;
    return { code: error.code, message: error.message };
  }
  throw new Error("expected a typed source failure");
}
function refuses(owner: WindowSource, event: WindowEvent) {
  const failure = diagnostic(() => owner.apply(event));
  expect(failure.code).toBe("KELD-IPC-005");
  expect(diagnostic(() => owner.desktop())).toEqual(failure);
  expect(diagnostic(() => decode(owner, capture(99n)))).toEqual(failure);
  expect(diagnostic(() => owner.apply(capture(100n)))).toEqual(failure);
}

test("desktop wire literals preserve the tag, optional facts and full u64 epoch", () => {
  const single = facts();
  single.displays.pop();
  const expected = [
    ["desktop-captured", capture(300n, single)],
    ["desktop-unavailable", capture(301n, null)],
    ["desktop-epoch-maximum", capture(0xffff_ffff_ffff_ffffn, null)],
  ] as const;
  const rows = readFileSync(
    join(
      import.meta.dir,
      "../../../../crates/keld-ipc/tests/fixtures/desktop-v0.tsv",
    ),
    "utf8",
  )
    .trimEnd()
    .split("\n")
    .filter((line) => !line.startsWith("#"));
  expect(rows.length).toBe(expected.length);
  for (const [index, row] of rows.entries()) {
    const [name, hex] = row.split("\t");
    expect(name).toBe(expected[index]![0]);
    const bytes = Uint8Array.from(Buffer.from(hex!, "hex"));
    const owner = source();
    expect(owner.decode(bytes)).toEqual(expected[index]![1]);
    expect(encodeWindowEvent(expected[index]![1])).toEqual(bytes);
    if (name === "desktop-captured") {
      const invalidOption = bytes.slice();
      invalidOption[3] = 2;
      expect(diagnostic(() => source().decode(invalidOption)).code).toBe(
        "KELD-IPC-005",
      );
    }
    for (let end = 0; end < bytes.length; end += 1) {
      expect(
        diagnostic(() => source().decode(bytes.subarray(0, end))).code,
      ).toBe("KELD-IPC-005");
    }
    expect(
      diagnostic(() => source().decode(Uint8Array.of(...bytes, 0))).code,
    ).toBe("KELD-IPC-005");
  }
});

test("decode and apply are silent; only apply installs desktop facts", () => {
  const observed: WindowEvent[] = [];
  const owner = source(observed);
  expect(owner.desktop()).toBeUndefined();
  const event = decode(owner, capture());
  expect(owner.desktop()).toBeUndefined();
  expect(observed).toEqual([]);
  if (event.variant !== "DesktopSnapshot" || event.snapshot === null)
    throw new Error("expected a populated desktop capture");
  expect(event.epoch).toBe(1n);
  expect(event.snapshot).toEqual(facts());
  for (const value of [event, event.snapshot, event.snapshot.displays])
    expect(Object.isFrozen(value)).toBe(true);
  for (const display of event.snapshot.displays) {
    for (const value of [display, display.bounds, display.work_area])
      expect(Object.isFrozen(value)).toBe(true);
  }
  owner.apply(event);
  expect(owner.desktop()).toEqual(facts());
  expect(owner.all()).toEqual([]);
  expect(observed).toEqual([]);
  owner.dispatch(event);
  expect(observed).toEqual([event]);
});

test("apply owns a deep frozen copy and leaves caller aliases mutable", () => {
  const owner = source();
  const input = facts();
  owner.apply(capture(1n, input));
  const saved = owner.desktop();
  if (!saved) throw new Error("desktop capture missing");
  input.should_use_dark_colors = true;
  for (const display of input.displays) {
    display.bounds.x = 9000;
    display.work_area.width = 1;
    display.scale_factor = 7;
  }
  input.displays.pop();
  expect(saved).toEqual(facts());
  for (const value of [saved, saved.displays])
    expect(Object.isFrozen(value)).toBe(true);
  for (const display of saved.displays) {
    for (const value of [display, display.bounds, display.work_area])
      expect(Object.isFrozen(value)).toBe(true);
  }
  expect(Reflect.set(saved, "should_use_dark_colors", true)).toBe(false);
});

test("newer captures and invalidation replace facts without changing old readers", () => {
  const observed: WindowEvent[] = [];
  const owner = source(observed);
  owner.apply(decode(owner, capture()));
  const before = owner.desktop();
  owner.apply(decode(owner, capture(2n, facts(true))));
  expect(owner.desktop()).toEqual(facts(true));
  expect(before).toEqual(facts());
  const lost = decode(owner, capture(3n, null));
  owner.apply(lost);
  expect(owner.desktop()).toBeUndefined();
  expect(observed).toEqual([]);
  owner.dispatch(lost);
  expect(observed).toEqual([lost]);
  owner.apply(decode(owner, capture(4n)));
  expect(owner.desktop()).toEqual(facts());
});

test("identical epochs are idempotent for both present and unavailable facts", () => {
  const owner = source();
  owner.apply(decode(owner, capture(5n)));
  owner.apply(decode(owner, capture(5n)));
  expect(owner.desktop()).toEqual(facts());
  owner.apply(decode(owner, capture(6n, null)));
  owner.apply(decode(owner, capture(6n, null)));
  expect(owner.desktop()).toBeUndefined();
});
test("epoch zero is reserved before any capture has been accepted", () => {
  refuses(source(), capture(0n));
});

for (const epoch of [0n, 1n, 2n]) {
  for (const value of [facts(true), null]) {
    test(`desktop epoch ${epoch} ${value === null ? "null" : "facts"} cannot follow 2`, () => {
      const owner = source();
      owner.apply(decode(owner, capture(2n)));
      refuses(owner, capture(epoch, value));
    });
  }
}
test("invalidation retains the epoch floor without lossy number conversion", () => {
  const owner = source();
  const epoch = 0x20_0000_0000_0001n;
  owner.apply(decode(owner, capture(epoch)));
  owner.apply(decode(owner, capture(epoch + 1n, null)));
  expect(owner.desktop()).toBeUndefined();
  refuses(owner, capture(epoch));
});

const malformed: [string, (value: ReturnType<typeof facts>) => void][] = [
  [
    "missing primary",
    (value) => {
      value.primary_id = 999;
    },
  ],
  [
    "duplicate id",
    (value) => {
      value.displays[1]!.id = 17;
    },
  ],
  [
    "zero id",
    (value) => {
      value.displays[1]!.id = 0;
    },
  ],
  [
    "empty display set",
    (value) => {
      value.displays = [];
    },
  ],
  [
    "fractional work area",
    (value) => {
      value.displays[0]!.work_area.x = 0.5;
    },
  ],
  [
    "zero work area",
    (value) => {
      value.displays[0]!.work_area.height = 0;
    },
  ],
];
for (const scale of [0, -1, Number.NaN, Infinity, -Infinity])
  malformed.push([
    `invalid scale ${scale}`,
    (value) => {
      value.displays[0]!.scale_factor = scale;
    },
  ]);
for (const [name, mutate] of malformed) {
  test(`newer malformed desktop facts fail closed: ${name}`, () => {
    const owner = source();
    owner.apply(decode(owner, capture()));
    const input = facts();
    mutate(input);
    refuses(owner, capture(2n, input));
  });
}

test("the proposed display bound accepts 32 and rejects 33", () => {
  const owner = source();
  const input = facts();
  input.displays = Array.from({ length: 32 }, (_, index) => ({
    ...facts().displays[0]!,
    id: 17 + index,
  }));
  owner.apply(decode(owner, capture(1n, input)));
  expect(owner.desktop()?.displays).toHaveLength(32);
  input.displays.push({ ...facts().displays[0]!, id: 49 });
  refuses(owner, capture(2n, input));
});

test("retirement preserves its diagnostic and cannot revive facts or callbacks", () => {
  const observed: WindowEvent[] = [];
  const owner = source(observed);
  const event = decode(owner, capture());
  owner.apply(event);
  owner.dispatch(event);
  const reason = linkError("KELD-IPC-022", "role retired");
  owner.end(reason);
  const expected = { code: reason.code, message: reason.message };
  expect(diagnostic(() => owner.desktop())).toEqual(expected);
  expect(diagnostic(() => decode(owner, capture(2n)))).toEqual(expected);
  expect(diagnostic(() => owner.apply(capture(2n)))).toEqual(expected);
  owner.dispatch(event);
  expect(observed).toEqual([event]);
});

test("sources own independent desktop facts, watermarks and lifetimes", () => {
  const first = source();
  const second = source();
  const event = capture();
  first.apply(event);
  expect(second.desktop()).toBeUndefined();
  second.apply(event);
  expect(first.desktop()).not.toBe(second.desktop());
  expect(first.desktop()?.displays).not.toBe(second.desktop()?.displays);
  first.apply(decode(first, capture(8n, facts(true))));
  expect(second.desktop()).toEqual(facts());
  first.end(linkError("KELD-IPC-022", "role retired"));
  expect(second.desktop()).toEqual(facts());
  second.apply(decode(second, capture(2n, facts(true))));
  expect(second.desktop()).toEqual(facts(true));
});
