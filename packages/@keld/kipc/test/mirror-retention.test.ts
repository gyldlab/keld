/** Literal Inspector3 rows are independent of the real mirror implementation. */
import { expect, test } from "bun:test";
import { MIRROR_FACT_MARKER, mirrorRetention } from "./mirror-retention.ts";

function graph() {
  return {
    version: 3,
    type: "Inspector",
    nodeClassNames: ["<root>", "Object", "Array"],
    // Root0, fact1, object-valued marker2, queue3, plain-object anchor4.
    nodes: [0, 0, 0, 0, 1, 32, 1, 0, 2, 16, 1, 0, 3, 16, 2, 0, 4, 16, 1, 0],
    edgeTypes: ["Internal", "Property", "Index", "Variable"],
    edgeNames: [MIRROR_FACT_MARKER, "fact"],
    edges: [0, 4, 0, 0, 4, 1, 1, 1, 1, 2, 1, 0, 3, 1, 2, 0, 3, 1, 2, 1],
  };
}

test("counts actual Array slots, including two slots referencing one marked fact", () => {
  expect(mirrorRetention(graph())).toEqual({ facts: 1, indexedEdges: 2 });
});
test("plain-object anchor keeps a marked fact without retaining an indexed slot", () => {
  const value = graph();
  value.edges = value.edges.slice(0, 12);
  expect(mirrorRetention(value)).toEqual({ facts: 1, indexedEdges: 0 });
});
test("Index edges from non-Array nodes and non-Index Array properties are excluded", () => {
  const value = graph();
  value.edges = [...value.edges.slice(0, 12), 4, 1, 2, 7, 3, 1, 1, 1];
  expect(mirrorRetention(value)).toEqual({ facts: 1, indexedEdges: 0 });
});
test("marker-looking variable names do not identify a Fact own property", () => {
  const value = graph();
  value.edges[10] = 3;
  expect(mirrorRetention(value)).toEqual({ facts: 0, indexedEdges: 0 });
});
test("a different property name does not silently identify a fact", () => {
  const value = graph();
  value.edgeNames[0] = "someOtherMarker";
  expect(mirrorRetention(value)).toEqual({ facts: 0, indexedEdges: 0 });
});

const invalid: readonly [
  string,
  (value: ReturnType<typeof graph>) => unknown,
][] = [
  ["unknown graph version", (value) => ({ ...value, version: 2 })],
  ["different graph layout", (value) => ({ ...value, type: "GCDebugging" })],
  [
    "unknown edge order",
    (value) => ({
      ...value,
      edgeTypes: ["Internal", "Index", "Property", "Variable"],
    }),
  ],
  [
    "unknown edge kind",
    (value) => {
      value.edges[2] = 4;
      return value;
    },
  ],
  ["incomplete node row", (value) => ({ ...value, nodes: [0, 0, 0] })],
  ["incomplete edge row", (value) => ({ ...value, edges: [0, 1, 0] })],
  [
    "duplicate node",
    (value) => {
      value.nodes[4] = 0;
      return value;
    },
  ],
  [
    "dangling source",
    (value) => {
      value.edges[0] = 99;
      return value;
    },
  ],
  [
    "dangling target",
    (value) => {
      value.edges[1] = 99;
      return value;
    },
  ],
  [
    "class index overflow",
    (value) => {
      value.nodes[2] = 3;
      return value;
    },
  ],
  [
    "missing root",
    (value) => {
      value.nodes[0] = 99;
      return value;
    },
  ],
  [
    "property name overflow",
    (value) => {
      value.edges[7] = 2;
      return value;
    },
  ],
  [
    "variable name overflow",
    (value) => {
      value.edges[10] = 3;
      value.edges[11] = 2;
      return value;
    },
  ],
  [
    "negative edge data",
    (value) => {
      value.edges[3] = -1;
      return value;
    },
  ],
  [
    "fractional node size",
    (value) => {
      value.nodes[1] = 0.5;
      return value;
    },
  ],
  [
    "non-finite node flags",
    (value) => {
      value.nodes[3] = Infinity;
      return value;
    },
  ],
  [
    "unknown node flag bits",
    (value) => {
      value.nodes[3] = 8;
      return value;
    },
  ],
  [
    "nonzero Internal data",
    (value) => {
      value.edges[3] = 1;
      return value;
    },
  ],
  [
    "unsafe node id",
    (value) => {
      value.nodes[0] = Number.MAX_SAFE_INTEGER + 1;
      return value;
    },
  ],
  ["invalid node class table", (value) => ({ ...value, nodeClassNames: [0] })],
  ["invalid edge name table", (value) => ({ ...value, edgeNames: [null] })],
  ["absent nodes", (value) => ({ ...value, nodes: undefined })],
  [
    "sparse node flags",
    (value) => {
      delete value.nodes[3];
      return value;
    },
  ],
  [
    "sparse name table",
    (value) => {
      delete value.edgeNames[0];
      return value;
    },
  ],
];
for (const [name, mutate] of invalid) {
  test(`rejects ${name} rather than returning zero`, () => {
    expect(() => mirrorRetention(mutate(graph()))).toThrow(
      "Unsupported mirror heap graph",
    );
  });
}
test("rejects non-graph input", () => {
  for (const value of [null, undefined, [], true, 3, "Inspector"]) {
    expect(() => mirrorRetention(value)).toThrow(
      "Unsupported mirror heap graph",
    );
  }
});
