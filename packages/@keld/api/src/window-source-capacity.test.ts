/** GH1149 proposed Host/source census controls, not physical heap qualification. */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";
import { linkError } from "@keld/kipc";
import { encodeWindowEvent, type WindowEvent } from "./window-codec.ts";
import {
  PROPOSED_MAX_CUMULATIVE_WINDOW_IDS,
  WindowSource,
} from "./window-source.ts";

const state = {
  bounds: { x: 37, y: 91, width: 320, height: 240 },
  focused: false,
  maximized: false,
  full_screen: false,
};

test("proposed WindowSource and Rust Host census limits are the same bytes", () => {
  const registry = readFileSync(
    join(
      import.meta.dir,
      "../../../../crates/keld-core/src/app_session/window_registry.rs",
    ),
    "utf8",
  );
  const bound = registry.match(
    /PROPOSED_MAX_CUMULATIVE_WINDOW_IDS: usize = (\d+);/,
  );
  expect(bound?.[1]).toBe(String(PROPOSED_MAX_CUMULATIVE_WINDOW_IDS));
  expect(PROPOSED_MAX_CUMULATIVE_WINDOW_IDS).toBe(1024);
});

test("1024 distinct destroyed Host identities remain queryable; the 1025th invalidates", () => {
  const dispatched: WindowEvent[] = [];
  const source = new WindowSource((fact) => dispatched.push(fact));
  let first: WindowEvent | undefined;
  for (let id = 1; id <= PROPOSED_MAX_CUMULATIVE_WINDOW_IDS; id += 1) {
    const window = { id, incarnation: BigInt(id) };
    const created = source.decode(
      encodeWindowEvent({
        variant: "Created",
        window,
        state,
        pending_close: null,
      }),
    );
    source.apply(created);
    const destroyed = source.decode(
      encodeWindowEvent({ variant: "Destroyed", window }),
    );
    source.apply(destroyed);
    if (id === 1) first = created;
  }
  const historical = source.all();
  expect(historical.length).toBe(PROPOSED_MAX_CUMULATIVE_WINDOW_IDS);
  expect(historical.every((row) => row.host_destroyed)).toBe(true);
  expect(historical[0]?.window).toEqual({ id: 1, incarnation: 1n });
  expect(historical.at(-1)?.window.id).toBe(PROPOSED_MAX_CUMULATIVE_WINDOW_IDS);
  expect(Object.isFrozen(historical)).toBe(true);

  const malicious = source.decode(
    encodeWindowEvent({
      variant: "Created",
      window: {
        id: PROPOSED_MAX_CUMULATIVE_WINDOW_IDS + 1,
        incarnation: BigInt(PROPOSED_MAX_CUMULATIVE_WINDOW_IDS + 1),
      },
      state,
      pending_close: null,
    }),
  );
  expect(() => source.apply(malicious)).toThrow("KELD-IPC-005");
  // The Host emits zero native effect on its own corresponding rejection. A
  // forged extra EVENT makes this Source fail rather than silently lose a
  // tombstone or make room by evicting a destroyed identity.
  expect(() => source.get({ id: 1, incarnation: 1n })).toThrow("KELD-IPC-005");
  expect(() => source.all()).toThrow("KELD-IPC-005");
  expect(dispatched).toEqual([]);
  if (!first) throw new Error("initial captured Created fact missing");
  source.dispatch(first);
  expect(dispatched).toEqual([first]);

  source.end(linkError("KELD-IPC-022", "end after previous fact drain"));
  // An earlier caller can retain its own immutable copy; Source's map has
  // ended and no new getter can allocate or expose current mirror state.
  expect(historical.length).toBe(PROPOSED_MAX_CUMULATIVE_WINDOW_IDS);
  expect(() => source.all()).toThrow("KELD-IPC-005");
});

test("Source census is not an arbitrary limit on the Host-owned id value", () => {
  const source = new WindowSource(() => {});
  const window = { id: 0xffff_ffff, incarnation: 0xffff_ffff_ffff_ffffn };
  const created = source.decode(
    encodeWindowEvent({
      variant: "Created",
      window,
      state,
      pending_close: null,
    }),
  );
  source.apply(created);
  source.apply(
    source.decode(encodeWindowEvent({ variant: "Destroyed", window })),
  );
  expect(source.all()).toHaveLength(1);
  expect(source.get(window)?.host_destroyed).toBe(true);
});

test("retired sparse IDs reject all later facts and either Created incarnation", () => {
  for (const id of [7, 0xffff_ffff]) {
    const window = { id, incarnation: 900n };
    const later: WindowEvent[] = [
      { variant: "Created", window, state, pending_close: null },
      {
        variant: "Created",
        window: { id, incarnation: 901n },
        state,
        pending_close: null,
      },
      { variant: "Bounds", window, bounds: state.bounds },
      { variant: "Focus", window, focused: true },
      { variant: "Maximized", window, maximized: true },
      { variant: "FullScreen", window, full_screen: true },
      { variant: "CloseRequested", window, close_seq: 1n },
      { variant: "Destroyed", window },
    ];
    for (const invalid of later) {
      const source = new WindowSource(() => {});
      source.apply({ variant: "Created", window, state, pending_close: null });
      const terminal = source.decode(
        encodeWindowEvent({ variant: "Destroyed", window }),
      );
      source.apply(terminal);
      source.dispatch(terminal);
      expect(source.get(window)).toBeUndefined();
      expect(() =>
        source.apply(source.decode(encodeWindowEvent(invalid))),
      ).toThrow("KELD-IPC-005");
      expect(() => source.all()).toThrow("KELD-IPC-005");
    }
  }
});

test("retirement keeps cumulative1024 admission and remaining snapshots in host-id order", () => {
  const source = new WindowSource(() => {});
  for (let id = 1024; id >= 1; id -= 1) {
    const window = { id, incarnation: BigInt(id) };
    source.apply({ variant: "Created", window, state, pending_close: null });
    if (id === 3 || id === 1) continue;
    const terminal: WindowEvent = { variant: "Destroyed", window };
    source.apply(terminal);
    source.dispatch(terminal);
  }
  expect(source.all().map((row) => row.window.id)).toEqual([1, 3]);
  expect(Object.isFrozen(source.all())).toBe(true);
  expect(() =>
    source.apply({
      variant: "Created",
      window: { id: 1025, incarnation: 1025n },
      state,
      pending_close: null,
    }),
  ).toThrow("KELD-IPC-005");
});
