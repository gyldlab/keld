/** Selected mirror only; native and Guard acceptance remain separate. */
import { expect, test } from "bun:test";
import { isCallError, linkError } from "@keld/kipc";
import { WindowSource } from "./window-source.ts";
import type { WindowEventWithDesktop, WindowRef } from "./window-codec.ts";
const first: WindowRef = { id: 7, incarnation: 300n };
const second: WindowRef = { id: 8, incarnation: 301n };
const state = {
  bounds: { x: 0, y: 0, width: 640, height: 480 },
  focused: true,
  maximized: false,
  full_screen: false,
};
const selected = (): WindowSource => new WindowSource(() => {}, true, true);
function apply(source: WindowSource, fact: unknown): void {
  source.apply(fact as WindowEventWithDesktop);
}
function created(source: WindowSource, window: WindowRef): void {
  apply(source, { variant: "Created", window, state, pending_close: null });
}
function bound(source: WindowSource, window: WindowRef, webview: number): void {
  apply(source, { variant: "OriginalRendererViewBound", window, webview });
}
type OriginalSnapshot = {
  webview?: number;
  document?: {
    generation: number;
    native_url: string;
    finished: boolean;
    current: boolean;
  };
};
function snapshot(
  source: WindowSource,
  window: WindowRef,
): OriginalSnapshot | undefined {
  return source.get(window) as OriginalSnapshot | undefined;
}
function invalid(action: () => void): void {
  try {
    action();
  } catch (error) {
    expect(isCallError(error) && error.code === "KELD-IPC-005").toBe(true);
    return;
  }
  throw new Error("expected source rejection");
}
test("selected source associates the exact host pair with its view", () => {
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  expect(snapshot(source, first)?.webview).toBe(5);
  expect(snapshot(source, first)?.document).toBeUndefined();
});
test("binding requires Created and rejects duplicate live physical views", () => {
  const before = selected();
  invalid(() => bound(before, first, 5));
  const duplicate = selected();
  created(duplicate, first);
  created(duplicate, second);
  bound(duplicate, first, 5);
  invalid(() => bound(duplicate, second, 5));
  const distinct = selected();
  created(distinct, first);
  created(distinct, second);
  bound(distinct, first, 5);
  bound(distinct, second, 6);
  expect(snapshot(distinct, second)?.webview).toBe(6);
});
test("a retired document cannot erase or revive a later generation", () => {
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 1,
    native_url: "file:///a",
  });
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///b",
  });
  source.retireNativeDocument(first, 1);
  expect(snapshot(source, first)?.document).toEqual({
    generation: 2,
    native_url: "file:///b",
    finished: false,
    current: true,
  });
  source.retireNativeDocument(first, 2);
  expect(source.getByWebview(5)?.window).toEqual(first);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///b",
  });
  expect(snapshot(source, first)?.document?.current).toBe(false);
});
test("desktop-only source still rejects a selected view binding", () => {
  const source = new WindowSource(() => {}, true);
  created(source, first);
  invalid(() => bound(source, first, 5));
});

test("binding rejects wrong incarnation, changed view and zero view", () => {
  const wrong = selected();
  created(wrong, first);
  invalid(() => bound(wrong, { id: 7, incarnation: 999n }, 5));
  const changed = selected();
  created(changed, first);
  bound(changed, first, 5);
  bound(changed, first, 5);
  invalid(() => bound(changed, first, 6));
  const zero = selected();
  created(zero, first);
  invalid(() => bound(zero, first, 0));
});

test("recovered document requires its binding and cannot replace an observed document", () => {
  const noBinding = selected();
  created(noBinding, first);
  invalid(() =>
    apply(noBinding, {
      variant: "DocumentRecovered",
      window: first,
      document_generation: 2,
      native_url: "file:///a",
    }),
  );
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "DocumentRecovered",
    window: first,
    document_generation: 2,
    native_url: "file:///a",
  });
  expect(snapshot(source, first)?.document).toEqual({
    generation: 2,
    native_url: "file:///a",
    finished: true,
    current: true,
  });
  invalid(() =>
    apply(source, {
      variant: "DocumentRecovered",
      window: first,
      document_generation: 3,
      native_url: "file:///b",
    }),
  );
});

test("same-generation URL conflicts and future finishes fail closed", () => {
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///a",
  });
  invalid(() =>
    apply(source, {
      variant: "NavigationCommitted",
      window: first,
      document_generation: 2,
      native_url: "file:///b",
    }),
  );
  const future = selected();
  created(future, first);
  bound(future, first, 5);
  apply(future, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///a",
  });
  invalid(() =>
    apply(future, {
      variant: "NavigationFinished",
      window: first,
      document_generation: 3,
    }),
  );
});

test("state changes and close acknowledgements preserve the bound identity", () => {
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///a",
  });
  apply(source, { variant: "CloseRequested", window: first, close_seq: 3n });
  expect(source.acceptHostCloseReply(first, 3n)).toBe(true);
  expect(snapshot(source, first)?.webview).toBe(5);
  expect(snapshot(source, first)?.document?.generation).toBe(2);
});

test("only the first current unit-pinch finish gets its exact dispatch identity", () => {
  const seen: boolean[] = [];
  let source!: WindowSource;
  source = new WindowSource(
    () => {},
    true,
    true,
    (fact) => {
      seen.push(source.isDispatchingUnitPinch(fact));
    },
  );
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///a",
  });
  const finish = {
    variant: "NavigationFinishedUnitPinchV1",
    window: first,
    document_generation: 2,
  } as const;
  apply(source, finish);
  source.dispatch(finish);
  source.dispatch(finish);
  expect(seen).toEqual([true]);
  expect(source.isDispatchingUnitPinch(finish)).toBe(false);
});

test("a deferred old finish cannot dispatch after a later commit", () => {
  const seen: number[] = [];
  const source = new WindowSource(
    () => {},
    true,
    true,
    (fact) => {
      seen.push(fact.document_generation);
    },
  );
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 1,
    native_url: "file:///a",
  });
  const oldFinish = {
    variant: "NavigationFinishedUnitPinchV1",
    window: first,
    document_generation: 1,
  } as const;
  apply(source, oldFinish);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 2,
    native_url: "file:///b",
  });
  source.dispatch(oldFinish);
  const currentFinish = {
    variant: "NavigationFinishedUnitPinchV1",
    window: first,
    document_generation: 2,
  } as const;
  apply(source, currentFinish);
  source.dispatch(currentFinish);
  expect(seen).toEqual([2]);
});

test("nested finish dispatch restores the outer exact callback identity", () => {
  const trace: string[] = [];
  const firstFinish = {
    variant: "NavigationFinishedUnitPinchV1",
    window: first,
    document_generation: 1,
  } as const;
  const secondFinish = {
    variant: "NavigationFinishedUnitPinchV1",
    window: second,
    document_generation: 1,
  } as const;
  let source!: WindowSource;
  source = new WindowSource(
    () => {},
    true,
    true,
    (fact) => {
      trace.push(`${fact.window.id}:${source.isDispatchingUnitPinch(fact)}`);
      if (fact === firstFinish) {
        source.dispatch(secondFinish);
        trace.push(`outer:${source.isDispatchingUnitPinch(firstFinish)}`);
      }
    },
  );
  created(source, first);
  created(source, second);
  bound(source, first, 5);
  bound(source, second, 6);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 1,
    native_url: "file:///a",
  });
  apply(source, {
    variant: "NavigationCommitted",
    window: second,
    document_generation: 1,
    native_url: "file:///b",
  });
  apply(source, firstFinish);
  apply(source, secondFinish);
  source.dispatch(firstFinish);
  expect(trace).toEqual(["7:true", "8:true", "outer:true"]);
  expect(source.isDispatchingUnitPinch(firstFinish)).toBe(false);
});

test("a later invalid fact does not suppress an earlier applied finish", () => {
  const seen: boolean[] = [];
  let source!: WindowSource;
  source = new WindowSource(
    () => {},
    true,
    true,
    (fact) => {
      seen.push(source.isDispatchingUnitPinch(fact));
    },
  );
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 1,
    native_url: "file:///a",
  });
  const finish = {
    variant: "NavigationFinishedUnitPinchV1",
    window: first,
    document_generation: 1,
  } as const;
  apply(source, finish);
  invalid(() => bound(source, first, 6));
  source.dispatch(finish);
  expect(seen).toEqual([true]);
  source.end(linkError("KELD-IPC-022", "role ended"));
  source.dispatch(finish);
  expect(seen).toEqual([true]);
});

test("a selected finish without an owning callback fails instead of disappearing", () => {
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  apply(source, {
    variant: "NavigationCommitted",
    window: first,
    document_generation: 1,
    native_url: "file:///a",
  });
  invalid(() =>
    apply(source, {
      variant: "NavigationFinished",
      window: first,
      document_generation: 1,
    }),
  );
});

test("view lookup returns only the matching live canonical Window row", () => {
  const source = selected();
  created(source, first);
  created(source, second);
  bound(source, first, 5);
  bound(source, second, 6);
  expect(source.getByWebview(5)?.window).toEqual(first);
  expect(source.getByWebview(6)?.window).toEqual(second);
  expect(source.getByWebview(7)).toBeUndefined();
  apply(source, { variant: "Destroyed", window: first });
  expect(source.getByWebview(5)).toBeUndefined();
  expect(source.getByWebview(6)?.window).toEqual(second);
});

test("view lookup ignores later application changes to Map iteration", () => {
  const source = selected();
  created(source, first);
  bound(source, first, 5);
  const original = Map.prototype.forEach;
  let resolved: ReturnType<WindowSource["getByWebview"]>;
  Map.prototype.forEach = (() => {
    throw new Error("application Map mutation");
  }) as typeof Map.prototype.forEach;
  try {
    resolved = source.getByWebview(5);
  } finally {
    Map.prototype.forEach = original;
  }
  expect(resolved?.window).toEqual(first);
});
