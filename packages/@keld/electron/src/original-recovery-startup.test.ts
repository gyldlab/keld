import { expect, test } from "bun:test";
import { makeFixture } from "./original-recovery-startup.test-role.ts";

const INITIAL = "file:///sealed/editor/index.html?offline=1";
const LATER = "file:///sealed/editor/index.html?explicit=1";

test("facade: fresh first automatic load executes exactly once", async () => {
  const f = makeFixture(),
    win = f.construct();
  expect(f.consume(win)).toBe(false);
  await win.loadURL(INITIAL);
  expect(f.loadCalls().map((call) => call.uri)).toEqual([INITIAL]);
  expect(f.consume(win)).toBe(false);
});

test("facade: actual adopted finished document skips first only, without fake events", async () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  f.adoptNext(ref);
  const win = f.construct();
  let finished = 0;
  win.webContents.on("did-finish-load", () => finished++);
  expect(f.consume(win)).toBe(true);
  expect(f.consume(win)).toBe(false);
  expect(f.loadCalls()).toEqual([]);
  expect(finished).toBe(0);
  await win.loadURL(LATER);
  expect(f.loadCalls().map((call) => call.uri)).toEqual([LATER]);
  expect(finished).toBe(1);
  f.adoptNext(ref);
  expect(f.construct()).toBe(win);
  expect(f.consume(win)).toBe(false);
});

test("facade: recovered physical snapshot never emits a navigation completion", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  const replay = f.enumerate()[0]!;
  let finished = 0,
    closed = 0;
  replay.webContents.on("did-finish-load", () => finished++);
  replay.on("close", () => closed++);
  f.fact({
    variant: "DocumentRecovered",
    window: ref,
    document_generation: 7,
    native_url: "keld-original://qualified/index.html",
  });
  expect(f.source.get(ref)?.document?.finished).toBe(true);
  expect(finished).toBe(0);
  expect(closed).toBe(0);
  expect(f.capture(ref)).toBeUndefined();
  expect(f.consume(replay)).toBe(false);
});

test("facade: replay wrapper keeps identity but enumeration alone cannot arm", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  const replay = f.enumerate()[0]!;
  expect(f.consume(replay)).toBe(false);
  f.adoptNext(ref);
  expect(f.construct()).toBe(replay);
  expect(f.consume(replay)).toBe(false);
});

test("facade: actual Adopted arms the same previously enumerated untouched wrapper", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  const replay = f.enumerate()[0]!;
  f.adoptNext(ref);
  expect(f.construct()).toBe(replay);
  expect(f.consume(replay)).toBe(true);
  expect(f.consume(replay)).toBe(false);
});

for (const state of ["created-only", "unfinished", "retired", "unqualified"]) {
  test(
    "facade: " + state + " performs a real first load and never rearms",
    async () => {
      const f = makeFixture(),
        ref = f.hostWindow();
      if (state !== "created-only") f.commit(ref, 7, state !== "unfinished");
      if (state === "retired") f.retire(ref, 7);
      if (state === "unqualified") f.run("hostLaunchFacts={schema:6}");
      f.adoptNext(ref);
      const win = f.construct();
      expect(f.consume(win)).toBe(false);
      await win.loadURL(INITIAL);
      expect(f.loadCalls().map((call) => call.uri)).toEqual([INITIAL]);
      expect(f.consume(win)).toBe(false);
    },
  );
}

test("facade: successor document between Adopted and consumption cannot skip", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  f.adoptNext(ref);
  const win = f.construct();
  f.commit(ref, 8);
  expect(f.consume(win)).toBe(false);
});

test("facade: ineligible adoption cannot rearm when its old page later finishes", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7, false);
  f.adoptNext(ref);
  const win = f.construct();
  f.fact({
    variant: "NavigationFinished",
    window: ref,
    document_generation: 7,
  });
  f.adoptNext(ref);
  expect(f.construct()).toBe(win);
  expect(f.consume(win)).toBe(false);
});

test("facade: explicit pending load retires skip before its completion", async () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  f.adoptNext(ref);
  const win = f.construct();
  f.deferLoad();
  const pending = win.loadURL(LATER);
  expect(f.source.get(ref)?.document?.generation).toBe(7);
  expect(f.consume(win)).toBe(false);
  expect(f.loadCalls().map((call) => call.uri)).toEqual([LATER]);
  f.finishLoad();
  await pending;
  expect(f.consume(win)).toBe(false);
});

test("facade: forged/copy/proxy wrapper cannot consume the genuine owner", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  f.adoptNext(ref);
  const win = f.construct();
  for (const fake of [
    null,
    {},
    { id: win.id },
    { ...win },
    Object.create(win),
    new Proxy(win, {}),
  ])
    expect(f.consume(fake)).toBe(false);
  expect(f.consume(win)).toBe(true);
});

test("facade: destroyed window and ended source cannot skip", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  f.adoptNext(ref);
  const win = f.construct();
  win.destroy();
  expect(f.consume(win)).toBe(false);
  const g = makeFixture(),
    another = g.hostWindow();
  g.commit(another, 7);
  g.adoptNext(another);
  const other = g.construct();
  g.endSource();
  expect(g.consume(other)).toBe(false);
});
