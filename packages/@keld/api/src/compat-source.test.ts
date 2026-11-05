/** Pure native-fact mirror tests. These fixture values prove no Host authority. */
import { expect, test } from "bun:test";
import { LIFECYCLE_CHANNEL } from "@keld/kipc";
import {
  encodeCompatControlEvent,
  type CompatControlEvent,
} from "@keld/kipc/compat-control";
import { CompatSource } from "./compat-source.ts";
import { WindowSource } from "./window-source.ts";

function fixture() {
  const windows = new WindowSource(() => {});
  windows.apply({
    variant: "Created",
    window: { id: 7, incarnation: 258n },
    state: {
      bounds: { x: 0, y: 0, width: 640, height: 480 },
      focused: true,
      maximized: false,
      full_screen: false,
    },
    pending_close: null,
  });
  const commit = (generation: number) =>
    windows.apply({
      variant: "NavigationCommitted",
      window: { id: 7, incarnation: 258n },
      document_generation: generation,
      native_url: "keld-app://actual/index.html",
    });
  commit(2);
  const callbacks: number[] = [];
  const releases: unknown[] = [];
  const source = new CompatSource(
    LIFECYCLE_CHANNEL,
    windows,
    (event, current) => {
      expect(current()).toBe(true);
      if (event.variant === "RendererMessage") callbacks.push(event.message);
      return { variant: "Consumed" };
    },
    (release) => {
      releases.push(release);
    },
  );
  const event = (navigation = 2, message = 1): CompatControlEvent => ({
    variant: "RendererMessage",
    webview: 1,
    navigation,
    message,
    window_id: 7,
    window_incarnation: 258n,
    url: "keld-app://actual/index.html",
    origin: "keld-app://actual",
    topic: "rendererReq",
    payload: Uint8Array.of(0),
  });
  return { windows, source, event, callbacks, releases, commit };
}

test("exact native facts apply once; receipt waits for independent ring-release phase", () => {
  const f = fixture();
  const fact = f.source.decode(encodeCompatControlEvent(f.event()));
  f.source.apply(fact);
  expect(f.callbacks).toEqual([]);
  expect(f.source.isCurrent(1, 2)).toBe(true);
  f.source.dispatch(fact);
  expect(f.callbacks).toEqual([1]);
  expect(f.releases).toEqual([]);
  f.source.released(fact);
  expect(f.releases).toEqual([
    {
      variant: "ReleaseConsumed",
      webview: 1,
      navigation: 2,
      message: 1,
      disposition: { variant: "Consumed" },
    },
  ]);
});

test("retirement applied before deferred dispatch cannot revive A or revoke B", () => {
  const f = fixture();
  const a = f.source.decode(encodeCompatControlEvent(f.event()));
  f.source.apply(a);
  const retired = f.source.decode(
    encodeCompatControlEvent({
      variant: "DocumentRetired",
      webview: 1,
      navigation: 2,
    }),
  );
  f.source.apply(retired);
  f.source.dispatch(a);
  expect(f.callbacks).toEqual([]);
  const stale = f.source.decode(encodeCompatControlEvent(f.event()));
  f.source.apply(stale);
  f.source.dispatch(stale);
  expect(f.source.isCurrent(1, 2)).toBe(false);
  expect(f.callbacks).toEqual([]);
  f.commit(4);
  const b = f.source.decode(encodeCompatControlEvent(f.event(4, 2)));
  f.source.apply(b);
  f.source.apply(retired);
  f.source.dispatch(b);
  expect(f.source.isCurrent(1, 4)).toBe(true);
  expect(f.callbacks).toEqual([2]);
  f.source.end();
  expect(f.source.isCurrent(1, 4)).toBe(false);
});

test("wrong WindowRef half is rejected before callback; native view is not window id", () => {
  const f = fixture();
  for (const invalid of [
    { ...f.event(), window_id: 1 },
    { ...f.event(), window_incarnation: 259n },
  ]) {
    const fact = f.source.decode(encodeCompatControlEvent(invalid));
    expect(() => f.source.apply(fact)).toThrow("KELD-IPC-005");
  }
  expect(f.callbacks).toEqual([]);
  expect(f.releases).toEqual([]);
});

test("Host-bound window event capture retains the same real view and rejects ambiguity or retirement", () => {
  const f = fixture();
  const ref = { id: 7, incarnation: 258n };
  expect(f.source.captureForWindow(ref)).toBeUndefined();
  expect(
    f.source.captureForWindow({ id: 1, incarnation: 258n }),
  ).toBeUndefined();
  const a = f.source.decode(encodeCompatControlEvent(f.event()));
  f.source.apply(a);
  const selected = f.source.captureForWindow(ref);
  expect(selected).toBeDefined();
  expect(
    selected && { webview: selected.webview, navigation: selected.navigation },
  ).toEqual({
    webview: 1,
    navigation: 2,
  });
  expect(selected?.current()).toBe(true);
  expect(
    f.source.captureForWindow({ id: 7, incarnation: 259n }),
  ).toBeUndefined();
  // Two real view facts pointing to the same window are ambiguous: no guessed
  // recipient and no broadcast to the first one that happens to be enumerated.
  const duplicate = f.source.decode(
    encodeCompatControlEvent({ ...f.event(), webview: 2 }),
  );
  f.source.apply(duplicate);
  expect(f.source.captureForWindow(ref)).toBeUndefined();
  expect(selected?.current()).toBe(false);
  f.source.end();
  expect(selected?.current()).toBe(false);
});

test("fresh native bound fact selects the exact target without renderer DATA or consumer credit", () => {
  const f = fixture();
  // Independent postcard vector: appended event tag 2; view 1, navigation 2,
  // WindowRef 7/258. No renderer message, topic, payload, or nonce is present.
  const fact = f.source.decode(Uint8Array.of(2, 1, 2, 7, 0x82, 2));
  expect(fact.event).toEqual({
    variant: "DocumentBound",
    webview: 1,
    navigation: 2,
    window_id: 7,
    window_incarnation: 258n,
  });
  f.source.apply(fact);
  f.source.dispatch(fact);
  f.source.released(fact);
  const selected = f.source.captureForWindow({ id: 7, incarnation: 258n });
  expect(
    selected && { webview: selected.webview, navigation: selected.navigation },
  ).toEqual({
    webview: 1,
    navigation: 2,
  });
  expect(selected?.current()).toBe(true);
  expect(f.callbacks).toEqual([]);
  expect(f.releases).toEqual([]);
  const message = f.source.decode(encodeCompatControlEvent(f.event()));
  f.source.apply(message);
  f.source.dispatch(message);
  f.source.apply(fact); // Duplicate metadata must not reset the message high-water mark.
  f.source.dispatch(message);
  expect(f.callbacks).toEqual([1]);
});

test("bound facts cannot revive A, revoke B, or survive window/link retirement", () => {
  const f = fixture();
  const ref = { id: 7, incarnation: 258n };
  const a = f.source.decode(Uint8Array.of(2, 1, 2, 7, 0x82, 2));
  const retiredA = f.source.decode(Uint8Array.of(1, 1, 2));
  f.source.apply(a);
  const capturedA = f.source.captureForWindow(ref);
  f.source.apply(retiredA);
  f.source.apply(a);
  expect(capturedA?.current()).toBe(false);
  expect(f.source.captureForWindow(ref)).toBeUndefined();
  f.commit(4);
  f.source.apply(f.source.decode(Uint8Array.of(2, 1, 4, 7, 0x82, 2)));
  const capturedB = f.source.captureForWindow(ref);
  expect(capturedB?.navigation).toBe(4);
  f.source.apply(retiredA);
  f.source.apply(a);
  expect(capturedB?.current()).toBe(true);
  expect(capturedA?.current()).toBe(false);
  f.windows.apply({ variant: "Destroyed", window: ref });
  expect(capturedB?.current()).toBe(false);
  expect(f.source.captureForWindow(ref)).toBeUndefined();
  f.source.end();
  expect(capturedB?.current()).toBe(false);
});

test("bound observation rejects wrong native pairing, future generation and ambiguous target", () => {
  const f = fixture();
  for (const wire of [
    Uint8Array.of(2, 1, 2, 8, 0x82, 2), // wrong window ID
    Uint8Array.of(2, 1, 2, 7, 0x83, 2), // wrong incarnation
    Uint8Array.of(2, 1, 3, 7, 0x82, 2), // no native commit for generation 3
  ]) {
    expect(() => f.source.apply(f.source.decode(wire))).toThrow("KELD-IPC-005");
  }
  f.source.apply(f.source.decode(Uint8Array.of(2, 1, 2, 7, 0x82, 2)));
  const selected = f.source.captureForWindow({ id: 7, incarnation: 258n });
  f.source.apply(f.source.decode(Uint8Array.of(2, 2, 2, 7, 0x82, 2)));
  expect(selected?.current()).toBe(false);
  expect(
    f.source.captureForWindow({ id: 7, incarnation: 258n }),
  ).toBeUndefined();
  expect(f.callbacks).toEqual([]);
  expect(f.releases).toEqual([]);
});

test("bound observation needs a committed document and does not relax renderer URL checks", () => {
  const windows = new WindowSource(() => {});
  windows.apply({
    variant: "Created",
    window: { id: 7, incarnation: 258n },
    state: {
      bounds: { x: 0, y: 0, width: 640, height: 480 },
      focused: false,
      maximized: false,
      full_screen: false,
    },
    pending_close: null,
  });
  const source = new CompatSource(
    LIFECYCLE_CHANNEL,
    windows,
    () => ({ variant: "Consumed" }),
    () => {},
  );
  expect(() =>
    source.apply(source.decode(Uint8Array.of(2, 1, 2, 7, 0x82, 2))),
  ).toThrow("KELD-IPC-005");
  const f = fixture();
  f.source.apply(f.source.decode(Uint8Array.of(2, 1, 2, 7, 0x82, 2)));
  const forged = { ...f.event(), url: "keld-app://other/index.html" };
  expect(() =>
    f.source.apply(f.source.decode(encodeCompatControlEvent(forged))),
  ).toThrow("KELD-IPC-005");
  expect(f.callbacks).toEqual([]);
});
