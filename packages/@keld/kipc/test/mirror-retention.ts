/** Cold test observer for the native Array slots holding marked mirror facts.
 * Bun 1.4.2 pins this Inspector3 schema (older bun-types comments say2):
 * https://github.com/oven-sh/WebKit/blob/2e2aa2290fac856d6f451ceacb58f7f5b44dd057/Source/JavaScriptCore/heap/HeapSnapshotBuilder.cpp#L189-L269
 * Own-property/index edges are emitted by JSObject.cpp at that same revision.
 * This counts direct Array Index relations only, not all references or heap bytes.
 */
export const MIRROR_FACT_MARKER = "__keldHeapFactMarker_6fb6a4";

/** Primitive-only results; the observer never exports or writes a heap snapshot. */
export interface MirrorRetention {
  facts: number;
  indexedEdges: number;
}

function requireGraph(condition: unknown, detail: string): asserts condition {
  if (!condition) throw new Error(`Unsupported mirror heap graph: ${detail}`);
}
function nonnegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}
function stringTable(value: unknown, field: string): string[] {
  requireGraph(
    Array.isArray(value) &&
      Array.from(value).every((entry: unknown) => typeof entry === "string"),
    `${field} must be a string table`,
  );
  return value as string[];
}
function numericRows(value: unknown, field: string): number[] {
  requireGraph(
    Array.isArray(value) &&
      value.length % 4 === 0 &&
      Array.from(value).every(nonnegativeInteger),
    `${field} must contain complete nonnegative integer rows`,
  );
  return value as number[];
}

/** Refuse an unknown or malformed graph instead of interpreting it as zero slots. */
export function mirrorRetention(snapshot: unknown): MirrorRetention {
  requireGraph(
    snapshot !== null &&
      typeof snapshot === "object" &&
      !Array.isArray(snapshot),
    "snapshot must be an object",
  );
  const graph = snapshot as Record<string, unknown>;
  requireGraph(
    graph.version === 3 && graph.type === "Inspector",
    "expected version3 Inspector",
  );
  const edgeTypes = stringTable(graph.edgeTypes, "edgeTypes");
  requireGraph(
    edgeTypes.length === 4 &&
      edgeTypes.every(
        (kind, index) =>
          kind === ["Internal", "Property", "Index", "Variable"][index],
      ),
    "unknown edge kinds or order",
  );
  const nodeClassNames = stringTable(graph.nodeClassNames, "nodeClassNames");
  const edgeNames = stringTable(graph.edgeNames, "edgeNames");
  const nodes = numericRows(graph.nodes, "nodes");
  const edges = numericRows(graph.edges, "edges");
  const classes = new Map<number, string>();
  for (let at = 0; at < nodes.length; at += 4) {
    const [id, , classIndex, flags] = nodes.slice(at, at + 4);
    requireGraph(!classes.has(id), "duplicate node id");
    requireGraph(flags <= 7, "unknown node flag bits");
    requireGraph(
      classIndex < nodeClassNames.length,
      "class index is outside the name table",
    );
    classes.set(id, nodeClassNames[classIndex]);
  }
  requireGraph(classes.get(0) === "<root>", "missing graph root");
  const facts = new Set<number>();
  for (let at = 0; at < edges.length; at += 4) {
    const [from, to, kind, extra] = edges.slice(at, at + 4);
    requireGraph(classes.has(from) && classes.has(to), "dangling edge");
    requireGraph(kind < edgeTypes.length, "unknown edge kind index");
    requireGraph(kind !== 0 || extra === 0, "Internal edge data must be zero");
    if (kind === 1 || kind === 3)
      requireGraph(
        extra < edgeNames.length,
        "edge name is outside the name table",
      );
    if (kind === 1 && edgeNames[extra] === MIRROR_FACT_MARKER) facts.add(from);
  }
  let indexedEdges = 0;
  for (let at = 0; at < edges.length; at += 4) {
    const [from, to, kind] = edges.slice(at, at + 4);
    if (kind === 2 && classes.get(from) === "Array" && facts.has(to))
      indexedEdges += 1;
  }
  return { facts: facts.size, indexedEdges };
}
