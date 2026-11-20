/** Host-fact census under post-preload application-realm prototype changes. */
import { WindowSource } from "./window-source.ts";

const state = {
  bounds: { x: 1, y: 2, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};
const source = new WindowSource(() => {});
const high = { id: 7, incarnation: 700n };
const low = { id: 3, incarnation: 300n };
const retired = { id: 5, incarnation: 500n };
for (const window of [high, retired, low]) {
  source.apply({ variant: "Created", window, state, pending_close: null });
}
const destroyed = { variant: "Destroyed" as const, window: retired };
source.apply(destroyed);
source.dispatch(destroyed);

if (!source.get(high) || !source.get(low) || source.get(retired)) {
  throw new Error("Source setup must retain both live Host facts and retire the tombstone");
}

const originalFilter = Array.prototype.filter;
const originalSort = Array.prototype.sort;
function census(mode: "filter" | "sort" | "both") {
  try {
    if (mode !== "sort") {
      Array.prototype.filter = (() => []) as typeof Array.prototype.filter;
    }
    if (mode !== "filter") {
      Array.prototype.sort = (() => []) as typeof Array.prototype.sort;
    }
    const snapshots = source.all();
    const ids: number[] = [];
    for (let i = 0; i < snapshots.length; i += 1) {
      ids[i] = snapshots[i]!.window.id;
    }
    return { ids, frozen: Object.isFrozen(snapshots) };
  } finally {
    Array.prototype.filter = originalFilter;
    Array.prototype.sort = originalSort;
  }
}

function inheritedIndexSetterCensus() {
  // Installed after the KIPC preload/import and after authenticated facts are
  // applied. A bare values[0] write would invoke this inherited setter.
  const prior = Object.getOwnPropertyDescriptor(Array.prototype, "0");
  let intercepted = 0;
  try {
    Object.defineProperty(Array.prototype, "0", {
      configurable: true,
      set(_value: unknown) {
        intercepted += 1;
      },
    });
    const snapshots = source.all();
    return {
      ids: [snapshots[0]?.window.id, snapshots[1]?.window.id],
      size: snapshots.length,
      ownZero: Object.prototype.hasOwnProperty.call(snapshots, "0"),
      frozen: Object.isFrozen(snapshots),
      intercepted,
    };
  } finally {
    if (prior) Object.defineProperty(Array.prototype, "0", prior);
    else Reflect.deleteProperty(Array.prototype, "0");
  }
}

process.stdout.write(JSON.stringify(
  process.argv[2] === "index-setter"
    ? inheritedIndexSetterCensus()
    : {
        filter: census("filter"),
        sort: census("sort"),
        both: census("both"),
        live: [source.get(low)?.window.id, source.get(high)?.window.id],
        tombstone: source.get(retired) === undefined,
      },
));
