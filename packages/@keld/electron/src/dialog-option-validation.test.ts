import { expect, test } from "bun:test";

import { encodeDialogRequest } from "../../api/src/dialog.generated.ts";
import { normalizeMessageOptions } from "./dialog-options.ts";

// This suite qualifies only the private synchronous option boundary. A real
// native click/cancellation and the public showMessageBox remain separate REDs.
test("pinned draw.io three-button options retain their exact native meaning", () => {
  const options = normalizeMessageOptions({
    type: "question",
    title: "Confirm",
    message: "Unsaved changes",
    buttons: ["Save", "Cancel", "Discard Changes"],
    defaultId: 0,
    cancelId: 1,
    noLink: true,
    normalizeAccessKeys: true,
    detail: undefined,
    checkboxLabel: undefined,
  });
  expect(options).toEqual({
    kind: "Question",
    title: "Confirm",
    message: "Unsaved changes",
    buttons: ["Save", "Cancel", "Discard Changes"],
    default_id: 0,
    cancel_id: 1,
    normalize_access_keys: true,
    detail: null,
    no_link: true,
    checkbox_label: null,
    checkbox_checked: null,
  });
  const frame = encodeDialogRequest({
    variant: "Message",
    nonce: 1n,
    parent: null,
    options,
  });
  expect(frame.byteLength).toBeGreaterThan(3);
  expect([...frame.slice(0, 3)]).toEqual([0, 1, 0]);
});

test("omission, present undefined and false stay distinct from invalid null", () => {
  expect(normalizeMessageOptions({ message: "hi" })).toEqual({
    kind: "None",
    title: null,
    message: "hi",
    buttons: [],
    default_id: null,
    cancel_id: null,
    normalize_access_keys: false,
    detail: null,
    no_link: false,
    checkbox_label: null,
    checkbox_checked: null,
  });
  expect(
    normalizeMessageOptions({
      message: "hi",
      type: undefined,
      buttons: undefined,
      noLink: undefined,
      checkboxLabel: undefined,
      checkboxChecked: undefined,
    }),
  ).toEqual(normalizeMessageOptions({ message: "hi" }));
  expect(
    normalizeMessageOptions({ message: "hi", noLink: false }).no_link,
  ).toBe(false);
  for (const key of ["title", "detail", "noLink", "type", "buttons"]) {
    expect(() =>
      normalizeMessageOptions({ message: "hi", [key]: null }),
    ).toThrow("KELD-COMPAT");
  }
});

test("unknown keys, accessors and inherited/untrusted input cannot create option authority", () => {
  let reads = 0;
  const unsupported = Object.defineProperty({ message: "hi" }, "unknown", {
    enumerable: true,
    value: undefined,
  });
  expect(() => normalizeMessageOptions(unsupported)).toThrow("KELD-COMPAT");
  const accessor = Object.defineProperty({}, "message", {
    enumerable: true,
    get() {
      reads++;
      return "hi";
    },
  });
  expect(() => normalizeMessageOptions(accessor)).toThrow("KELD-COMPAT");
  expect(reads).toBe(0);
  const inherited = Object.create({ message: "stolen" }) as object;
  expect(() => normalizeMessageOptions(inherited)).toThrow("KELD-COMPAT");
  for (const wrong of [null, undefined, [], 1, "x", false]) {
    expect(() => normalizeMessageOptions(wrong)).toThrow("KELD-COMPAT");
  }
});

test("bounded native message options reject invalid indices and button arrays", () => {
  for (const wrong of [0.2, -1, 2, NaN, Infinity, "0", true]) {
    expect(() =>
      normalizeMessageOptions({
        message: "hi",
        buttons: ["Yes", "No"],
        defaultId: wrong,
      }),
    ).toThrow("KELD-COMPAT");
  }
  for (const buttons of ["Yes", ["", 1], Array(17).fill("Yes")]) {
    expect(() => normalizeMessageOptions({ message: "hi", buttons })).toThrow(
      "KELD-COMPAT",
    );
  }
  expect(
    normalizeMessageOptions({ message: "hi", buttons: Array(16).fill("Yes") })
      .buttons.length,
  ).toBe(16);
  expect(() => normalizeMessageOptions({ message: "hi", cancelId: 1 })).toThrow(
    "KELD-COMPAT",
  );
  expect(() =>
    normalizeMessageOptions({ message: "hi", checkboxChecked: false }),
  ).toThrow("KELD-COMPAT");
  expect(
    normalizeMessageOptions({
      message: "hi",
      checkboxLabel: "Do not ask",
      checkboxChecked: false,
    }).checkbox_checked,
  ).toBe(false);
});

test("UTF-8 wire envelope bounds are checked without truncation or coercion", () => {
  const valid = normalizeMessageOptions({ message: "🌍".repeat(4000) });
  expect(
    encodeDialogRequest({
      variant: "Message",
      nonce: 2n,
      parent: null,
      options: valid,
    }).byteLength,
  ).toBeLessThanOrEqual(16 * 1024 + 32);
  for (const options of [
    { message: "🌍".repeat(4100) },
    { message: "hi", buttons: ["x".repeat(16 * 1024)] },
    { message: "x".repeat(9000), detail: "x".repeat(9000) },
    { message: false },
    { message: "hi", title: 1 },
  ]) {
    expect(() => normalizeMessageOptions(options)).toThrow("KELD-COMPAT");
  }
});

test("button iteration snapshots the own data length without invoking get traps", () => {
  let lengthReads = 0;
  let elementInspections = 0;
  const buttons = new Proxy(["Yes"], {
    get(target, key, receiver) {
      if (key === "length") {
        lengthReads++;
        return lengthReads === 1 ? 1 : 1000;
      }
      return Reflect.get(target, key, receiver);
    },
    getOwnPropertyDescriptor(target, key) {
      if (key !== "length") elementInspections++;
      return Reflect.getOwnPropertyDescriptor(target, key);
    },
  });
  const normalized = normalizeMessageOptions({ message: "hi", buttons });
  expect(normalized.buttons).toEqual(["Yes"]);
  expect(lengthReads).toBe(0);
  expect(elementInspections).toBe(1);
});

test("oversized buttons refuse before inspecting any element", () => {
  let elementInspections = 0;
  const buttons = new Proxy(Array(17).fill("Yes"), {
    getOwnPropertyDescriptor(target, key) {
      if (key !== "length") elementInspections++;
      return Reflect.getOwnPropertyDescriptor(target, key);
    },
  });
  expect(() => normalizeMessageOptions({ message: "hi", buttons })).toThrow(
    "KELD-COMPAT-010",
  );
  expect(elementInspections).toBe(0);
});

test("revoked and throwing inspection traps become canonical Message refusal", () => {
  const revokedRoot = Proxy.revocable({ message: "hi" }, {});
  revokedRoot.revoke();
  const revokedButtons = Proxy.revocable(["Yes"], {});
  revokedButtons.revoke();
  const throwingPrototype = new Proxy(["Yes"], {
    getPrototypeOf() {
      throw new Error("hostile prototype");
    },
  });
  const throwingLength = new Proxy(["Yes"], {
    getOwnPropertyDescriptor() {
      throw new Error("hostile descriptor");
    },
  });
  const throwingElement = new Proxy(["Yes"], {
    getOwnPropertyDescriptor(target, key) {
      if (key === "0") throw new Error("hostile element");
      return Reflect.getOwnPropertyDescriptor(target, key);
    },
  });
  expect(() => normalizeMessageOptions(revokedRoot.proxy)).toThrow(
    "KELD-COMPAT-010",
  );
  for (const buttons of [
    revokedButtons.proxy,
    throwingPrototype,
    throwingLength,
    throwingElement,
  ]) {
    expect(() => normalizeMessageOptions({ message: "hi", buttons })).toThrow(
      "KELD-COMPAT-010",
    );
  }
  const healthy = normalizeMessageOptions({
    message: "healthy",
    buttons: ["Yes", "No"],
  });
  expect(healthy.buttons).toEqual(["Yes", "No"]);
});

test("validation refusals preserve their specific actionable reason", () => {
  const cases: [unknown, string][] = [
    [[], "options must be an object"],
    [
      Object.create({ message: "hi" }),
      "custom-prototype options are unsupported",
    ],
    [{ message: "hi", buttons: "Yes" }, "buttons must be a plain array"],
    [
      { message: "hi", buttons: Array(17).fill("Yes") },
      "buttons must have a bounded own data length",
    ],
    [
      { message: "hi", buttons: Array(1) },
      "button labels must be own data values",
    ],
    [
      { message: "hi", buttons: [1] },
      "button label must be bounded UTF-8 text",
    ],
    [
      { message: "hi", buttons: ["x".repeat(16 * 1024 + 1)] },
      "button label must be bounded UTF-8 text",
    ],
  ];
  for (const [input, detail] of cases) {
    expect(() => normalizeMessageOptions(input)).toThrow(
      `KELD-COMPAT-010: unsupported Message dialog options: ${detail}.`,
    );
  }
});

test("inspection refusals never inspect an attacker-controlled thrown value", () => {
  let thrownReads = 0;
  const thrown = new Proxy({}, {
    get() {
      thrownReads++;
      throw new Error("must not inspect thrown value");
    },
    getPrototypeOf() {
      thrownReads++;
      throw new Error("must not classify thrown value");
    },
  });
  const roots = [
    new Proxy({ message: "hi" }, {
      getPrototypeOf() {
        throw thrown;
      },
    }),
    new Proxy({ message: "hi" }, {
      ownKeys() {
        throw thrown;
      },
    }),
  ];
  for (const input of roots) {
    expect(() => normalizeMessageOptions(input)).toThrow(
      "options could not be inspected safely",
    );
  }
  const buttons = new Proxy(["Yes"], {
    getOwnPropertyDescriptor() {
      throw thrown;
    },
  });
  expect(() => normalizeMessageOptions({ message: "hi", buttons })).toThrow(
    "buttons could not be inspected safely",
  );
  expect(thrownReads).toBe(0);
});
