import { expect, test } from "bun:test";
import { makeFixture } from "./original-recovery-startup.test-role.ts";

test("API: exact finished original document is observed without native I/O", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  const current = f.capture(ref);
  expect(typeof current).toBe("function");
  expect(current?.()).toBe(true);
  expect(f.calls).toEqual([]);
});

test("API: mutable global freeze cannot redirect the exact captured window", () => {
  const f = makeFixture(),
    original = f.hostWindow(),
    foreign = f.hostWindow();
  f.commit(original, 7);
  f.commit(foreign, 7);
  f.run(
    "Object.freeze=()=>({id:" +
      foreign.id +
      ",incarnation:" +
      foreign.incarnation +
      "n})",
  );
  const current = f.capture(original);
  expect(current?.()).toBe(true);
  f.retire(original, 7);
  expect(current?.()).toBe(false);
});

test("API: actual Created-only successor replay is ineligible", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  expect(f.capture(ref)).toBeUndefined();
  expect(f.calls).toEqual([]);
});

test("API: finished physical snapshot without current compat binding cannot retain startup", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7, true, false);
  expect(f.capture(ref)).toBeUndefined();
  f.bind(ref, 7);
  const current = f.capture(ref);
  expect(current?.()).toBe(true);
  f.compat.end();
  expect(current?.()).toBe(false);
  expect(f.capture(ref)).toBeUndefined();
});

test("API: missing Finished, retirement, incarnation and source termination refuse", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7, false);
  expect(f.capture(ref)).toBeUndefined();
  f.fact({
    variant: "NavigationFinished",
    window: ref,
    document_generation: 7,
  });
  const current = f.capture(ref);
  expect(current?.()).toBe(true);
  expect(
    f.capture({ ...ref, incarnation: ref.incarnation + 1n }),
  ).toBeUndefined();
  f.retire(ref, 7);
  expect(current?.()).toBe(false);
  expect(f.capture(ref)).toBeUndefined();
  f.endSource();
  expect(current?.()).toBe(false);
});

test("API: later same-URL document cannot inherit the captured predicate", () => {
  const f = makeFixture(),
    ref = f.hostWindow();
  f.commit(ref, 7);
  const old = f.capture(ref);
  expect(old?.()).toBe(true);
  f.commit(ref, 8);
  expect(old?.()).toBe(false);
  expect(f.capture(ref)?.()).toBe(true);
});

for (const change of [
  "nativeFacadeBootSelected=false",
  "runtimeProcessFactsSelected=false",
  "hostLaunchFacts={schema:6}",
  "hostLaunchFacts={schema:7}",
  "hostReady=false",
  "linkDead=new Error('old role revoked')",
  "session.windowAdmissionOpen=false",
  "connectedLink={windowSource:source,windowAdmissionOpen:true}",
  "session.windowSource={get:()=>({host_destroyed:false,document:{generation:7,current:true,finished:true}})}",
]) {
  test("API: captured identity rejects " + change, () => {
    const f = makeFixture(),
      ref = f.hostWindow();
    f.commit(ref, 7);
    const current = f.capture(ref);
    expect(current?.()).toBe(true);
    f.run(change);
    expect(current?.()).toBe(false);
  });
}
