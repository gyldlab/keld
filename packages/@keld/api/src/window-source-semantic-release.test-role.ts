import assert from "node:assert/strict";
import { isProxy } from "node:util/types";
import { readFileSync, realpathSync, writeFileSync, unlinkSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, dirname, resolve } from "node:path";
const mapPrototype = Map.prototype,
  errorPrototype = Error.prototype,
  objectPrototype = Object.prototype,
  functionPrototype = Function.prototype,
  arrayPrototype = Array.prototype;
const native = {
  ownKeys: Reflect.ownKeys,
  desc: Object.getOwnPropertyDescriptor,
  proto: Object.getPrototypeOf,
  frozen: Object.isFrozen,
  create: Object.create,
  define: Object.defineProperty,
  setProto: Object.setPrototypeOf,
  same: Object.is,
  apply: Reflect.apply,
  arrayPush: Array.prototype.push,
  entries: Map.prototype.entries,
  mapGet: Map.prototype.get,
};
// The parent supplies its pinned compiler and canonical provider. The observed
// class is a disposable compiler-instrumented copy of this exact Source body.
const runtime = {
  version: Bun.version,
  revision: Bun.revision,
  platform: process.platform,
  arch: process.arch,
};
assert.deepEqual(process.argv.slice(7, 11), [
  runtime.version,
  runtime.revision,
  runtime.platform,
  runtime.arch,
]);
const mode = process.argv[3];
assert(mode === "baseline" || mode === "caller", "unrecognized witness mode");
const source = realpathSync(resolve(import.meta.dir, "./window-source.ts"));
const compiler = process.argv[4]!;
const sourceDir = dirname(source);
const js = source.endsWith(".js");
const codecPath = realpathSync(
  join(sourceDir, `window-codec.${js ? "js" : "ts"}`),
);
const mirror = realpathSync(join(sourceDir, `mirror.${js ? "js" : "ts"}`));
const transportPath = realpathSync(Bun.resolveSync("@keld/kipc", sourceDir));
assert.equal(
  transportPath,
  realpathSync(process.argv[5]!),
  "Source resolved a different provider",
);
const expected = js
  ? [
      "4f2a6ba84be8d16d5920f77391af8b4966ef349b55d01704f8b52ff1c0be2176",
      "b752bb8c7b875d57983a86695655653b1c103778d192c894789425c749756383",
      "24ac3fb3b3ad69dcce45fd74f20cf42daac0da4f114915c1e0c4be3bcc1f1265",
      "09475cffc8b1fe557d54694368d9dc3c3f07a81efa5d72d058f14ff902407e1f",
    ]
  : [
      "11e78e5753b659592444aeabd796766f404fbbe514bfd2c961160e9ae52ed595",
      "02316c26133883f65a3255801160e2dc8209cd83490e7a9f90fd8d3f8d5e53c7",
      "3e11e55bedb30a3644e7d280c118ca5712b468d532dc9c214f7cd72f1e618c9f",
      "8d435ce9e80d73a7a8b6b8de4f136ab3a48c85f20e17b3e3c0e36d399b37ac92",
    ];
const sha = (v: string) => createHash("sha256").update(v).digest("hex");
const values = [source, codecPath, mirror, transportPath].map((path) =>
  readFileSync(path, "utf8"),
);
const actualHashes = values.map(sha);
// The existing package test runs the producer from the API directory. Bun's
// source-path comment and the stamp over it differ from the root-CWD artifact;
// the reviewed executable bytes after those first two lines are identical.
const apiCwdTransport =
  "856c356e4a8de3f68e4b587d765e1f44940ec5cc1426c52cbaf57ecf92d6e956";
if (js && actualHashes[3] === apiCwdTransport) expected[3] = apiCwdTransport;
assert.deepEqual(actualHashes, expected, "unreviewed body or helper");
assert.equal(
  realpathSync(
    Bun.resolveSync(`./window-codec.${js ? "js" : "ts"}`, sourceDir),
  ),
  codecPath,
);
assert.equal(
  realpathSync(Bun.resolveSync("@keld/kipc", dirname(codecPath))),
  transportPath,
);
assert.equal(
  Bun.resolveSync("node:worker_threads", dirname(transportPath)),
  "node:worker_threads",
);
assert.equal(
  Bun.resolveSync("node:util/types", dirname(transportPath)),
  "node:util/types",
);
const { API } = await import(join(compiler, "dist/api/async/api.js"));
const { SyntaxKind } = await import(join(compiler, "dist/ast/index.js"));
const api = new API({ cwd: sourceDir });
let generated = "";
try {
  const snap = await api.updateSnapshot({ openFiles: [source] });
  try {
    const project = await snap.getDefaultProjectForFile(source);
    assert(project);
    const f = await project.program.getSourceFile(source);
    assert(f);
    const classes = f.statements.filter(
      (s: any) =>
        s.kind === SyntaxKind.ClassDeclaration &&
        s.name?.text === "WindowSource",
    );
    assert.equal(classes.length, 1);
    const c = classes[0];
    const fields: string[] = [],
      statics: string[] = [];
    for (const m of c.members)
      if (m.kind === SyntaxKind.PropertyDeclaration) {
        const name = m.name?.getText(f);
        assert(name);
        (m.modifiers?.some((x: any) => x.kind === SyntaxKind.StaticKeyword)
          ? statics
          : fields
        ).push(name);
      }
    const bindings: string[] = [];
    for (const s of f.statements) {
      if (s.kind === SyntaxKind.ImportDeclaration) {
        if (s.importClause?.isTypeOnly) continue;
        for (const x of s.importClause?.namedBindings?.elements ?? [])
          if (!x.isTypeOnly) bindings.push(x.name.text);
      } else if (s.kind === SyntaxKind.VariableStatement) {
        for (const x of s.declarationList.declarations)
          bindings.push(x.name.getText(f));
      } else if (
        [SyntaxKind.FunctionDeclaration, SyntaxKind.ClassDeclaration].includes(
          s.kind,
        )
      )
        bindings.push(s.name.text);
    }
    assert.deepEqual(fields, [
      "channel",
      "#windows",
      "#onFact",
      "#failureCode",
      "#failureMessage",
    ]);
    assert.deepEqual(statics, []);
    assert.deepEqual(bindings, [
      "WINDOW_CHANNEL",
      "stateFreeze",
      "stateMap",
      "stateMapGet",
      "stateMapHas",
      "stateMapSet",
      "stateMapSize",
      "stateMapValues",
      "stateMapClear",
      "linkError",
      "decodeWindowEvent",
      "PROPOSED_MAX_CUMULATIVE_WINDOW_IDS",
      "snapshot",
      "WindowSource",
    ]);
    const lhs = js
      ? 'globalThis[Symbol.for("keld.private-observer.prototype")]'
      : '(globalThis as any)[Symbol.for("keld.private-observer.prototype")]';
    const instance = js ? "instance" : "instance: WindowSource";
    const tagged =
      "\n/*__KELD_OBSERVER_BEGIN__*/\n  static {\n    " +
      lhs +
      "(\n      (" +
      instance +
      ") => [" +
      fields.map((n) => `[${JSON.stringify(n)}, instance.${n}]`).join(", ") +
      "],\n      () => ({" +
      bindings.map((n) => `${JSON.stringify(n)}: ${n}`).join(", ") +
      "}),\n      () => []\n    );\n  }\n/*__KELD_OBSERVER_END__*/\n";
    const offset = c.end - 1,
      text = values[0]!;
    assert.equal(text[offset], "}");
    generated = text.slice(0, offset) + tagged + text.slice(offset);
    assert.equal(
      generated.slice(0, offset) + generated.slice(offset + tagged.length),
      text,
    );
    assert.equal(
      sha(tagged),
      js
        ? "e175d7fd0966473df51c12fec729b14476f39d4dc43f9d71c7b6cd4712e91575"
        : "725c1ba14e5929489f6fbc1360cfdda978b3db5605832c90fd738bfc95f91767",
    );
  } finally {
    await snap.dispose();
  }
} finally {
  await api.close();
}
const token = process.argv[6]!;
assert(/^[0-9a-f-]{36}$/.test(token), "invalid observer token");
const observed = join(
  sourceDir,
  `.window-source-observer-${token}.${js ? "js" : "ts"}`,
);
let observerWritten = false;
let read: any,
  readModule: any,
  readStatics: any,
  module: any,
  transport: any,
  codec: any;
try {
  writeFileSync(observed, generated, { flag: "wx" });
  observerWritten = true;
  const symbols = Symbol.for("keld.private-observer.prototype");
  assert(!(symbols in globalThis));
  (globalThis as any)[symbols] = (a: any, b: any, c: any) => {
    assert.equal(read, undefined);
    read = a;
    readModule = b;
    readStatics = c;
  };
  try {
    module = await import(observed);
  } finally {
    delete (globalThis as any)[symbols];
  }
  const originalTransport = await import(transportPath),
    originalCodec = await import(codecPath);
  transport = originalTransport;
  codec = originalCodec;
  const actual = readModule();
  for (const name of [
    "WINDOW_CHANNEL",
    "stateFreeze",
    "stateMap",
    "stateMapGet",
    "stateMapHas",
    "stateMapSet",
    "stateMapSize",
    "stateMapValues",
    "stateMapClear",
    "linkError",
  ])
    assert.equal(actual[name], originalTransport[name]);
  assert.equal(actual.decodeWindowEvent, originalCodec.decodeWindowEvent);
  assert.deepEqual(readStatics(), []);
  assert.equal(typeof read, "function");
  assert.equal(typeof module.WindowSource, "function");
} finally {
  if (observerWritten) unlinkSync(observed);
}
const staticNames = readStatics().map((x: any) => x[0]);
function descriptorOwnValue(descriptor: any, key: string) {
  const d = native.desc(descriptor, key);
  return d && "value" in d ? d.value : undefined;
}
function descriptorHasOwn(descriptor: any, key: string) {
  return native.desc(descriptor, key) !== undefined;
}
function sameDescriptor(now: any, old: any) {
  if (!now) return false;
  const data = descriptorHasOwn(now, "value"),
    oldData = descriptorHasOwn(old, "value");
  if (
    data !== oldData ||
    descriptorOwnValue(now, "configurable") !==
      descriptorOwnValue(old, "configurable") ||
    descriptorOwnValue(now, "enumerable") !==
      descriptorOwnValue(old, "enumerable")
  )
    return false;
  if (data)
    return (
      descriptorOwnValue(now, "writable") ===
        descriptorOwnValue(old, "writable") &&
      native.same(
        descriptorOwnValue(now, "value"),
        descriptorOwnValue(old, "value"),
      )
    );
  return (
    native.same(
      descriptorOwnValue(now, "get"),
      descriptorOwnValue(old, "get"),
    ) &&
    native.same(descriptorOwnValue(now, "set"), descriptorOwnValue(old, "set"))
  );
}
function environmentValid(protoSnapshots: any[]) {
  for (let i = 0; i < protoSnapshots.length; i++) {
    const item = protoSnapshots[i];
    if (isProxy(item.value) || native.proto(item.value) !== item.parent) {
      return false;
    }
    const keys = native.ownKeys(item.value);
    if (keys.length !== item.descriptors.length) {
      return false;
    }
    for (let j = 0; j < keys.length; j++) {
      const prev = item.descriptors[j],
        key = keys[j];
      if (key !== prev.key) {
        return false;
      }
      if (!sameDescriptor(native.desc(item.value, key), prev.descriptor)) {
        return false;
      }
    }
  }
  return true;
}
function captureEnvironment() {
  const classPrototypeDesc = native.desc(module.WindowSource, "prototype");
  assert(
    classPrototypeDesc &&
      "value" in classPrototypeDesc &&
      !isProxy(classPrototypeDesc.value),
  );
  const sourcePrototype = classPrototypeDesc.value;
  const sourceProtoExpected = [
      "constructor",
      "decode",
      "apply",
      "dispatch",
      "acceptHostCloseReply",
      "get",
      "all",
      "end",
    ],
    classExpected = ["length", "name", "prototype"];
  function captureDescriptors(value: any) {
    assert(!isProxy(value));
    const keys = native.ownKeys(value),
      out = [];
    for (let i = 0; i < keys.length; i++) {
      const d = native.desc(value, keys[i]);
      assert(d);
      out.push({ key: keys[i], descriptor: d });
    }
    return out;
  }
  function exactNames(rows: any[], names: string[]) {
    if (rows.length !== names.length) return false;
    for (let i = 0; i < rows.length; i++) {
      if (typeof rows[i].key !== "string" || names.indexOf(rows[i].key) < 0)
        return false;
    }
    return true;
  }
  const rootProtos = [
    ["Source.prototype", sourcePrototype, objectPrototype],
    ["WindowSource", module.WindowSource, functionPrototype],
    ["Object.prototype", objectPrototype, null],
    ["Map.prototype", mapPrototype, objectPrototype],
    ["Function.prototype", functionPrototype, objectPrototype],
    ["Error.prototype", errorPrototype, objectPrototype],
    ["Array.prototype", arrayPrototype, objectPrototype],
  ];
  const sourceOwn = captureDescriptors(sourcePrototype),
    classOwn = captureDescriptors(module.WindowSource);
  assert(exactNames(sourceOwn, sourceProtoExpected));
  assert(exactNames(classOwn, classExpected));
  for (const [, value, parent] of rootProtos)
    assert(!isProxy(value) && native.proto(value) === parent);
  const protoSnapshots: any[] = [],
    pendingProtos: any[] = rootProtos.map(([name, value]) => ({ name, value })),
    visitedProtos = new Set();
  for (let cursor = 0; cursor < pendingProtos.length; cursor++) {
    assert(cursor < 1024, "prototype descriptor graph exceeds bound");
    const item = pendingProtos[cursor];
    if (visitedProtos.has(item.value)) continue;
    visitedProtos.add(item.value);
    assert(!isProxy(item.value));
    const parent = native.proto(item.value);
    assert(
      parent === null ||
        parent === objectPrototype ||
        parent === functionPrototype,
      "unsupported prototype node",
    );
    const ds = captureDescriptors(item.value);
    protoSnapshots.push({
      name: item.name,
      value: item.value,
      parent,
      descriptors: ds,
    });
    for (let j = 0; j < ds.length; j++) {
      const d = ds[j].descriptor;
      for (const value of [
        "value" in d ? d.value : undefined,
        "get" in d ? d.get : undefined,
        "set" in d ? d.set : undefined,
      ])
        if (
          value !== null &&
          (typeof value === "object" || typeof value === "function")
        ) {
          assert(!isProxy(value), "prototype descriptor points to Proxy");
          pendingProtos.push({
            name: String(item.name) + "." + String(ds[j].key),
            value,
          });
        }
    }
  }
  return { sourcePrototype, protoSnapshots };
}
function checkedSnapshot(value: any, path: string) {
  function record(candidate: any, names: string[]) {
    assert(
      candidate !== null &&
        typeof candidate === "object" &&
        !isProxy(candidate),
      path + ":proxy-or-not-object",
    );
    assert(
      native.proto(candidate) === objectPrototype && native.frozen(candidate),
      path + ":unsupported-record",
    );
    const keys = native.ownKeys(candidate);
    assert.equal(keys.length, names.length, path + ":keys");
    const out: any = native.create(null);
    for (const key of keys) {
      assert(
        typeof key === "string" && names.includes(key),
        path + ":unknown-key",
      );
      const d = native.desc(candidate, key);
      assert(d && descriptorHasOwn(d, "value"), path + ":accessor");
      native.define(out, key, {
        value: descriptorOwnValue(d, "value"),
        enumerable: true,
      });
    }
    for (const name of names) assert(native.desc(out, name), path + ":missing");
    return out;
  }
  const snapshot = record(value, [
    "window",
    "state",
    "pending_close",
    "host_destroyed",
  ]);
  const window = record(snapshot.window, ["id", "incarnation"]);
  const state = record(snapshot.state, [
    "bounds",
    "focused",
    "maximized",
    "full_screen",
  ]);
  const bounds = record(state.bounds, ["x", "y", "width", "height"]);
  assert(
    typeof window.id === "number" && typeof window.incarnation === "bigint",
    path + ":window-scalars",
  );
  assert(
    typeof state.focused === "boolean" &&
      typeof state.maximized === "boolean" &&
      typeof state.full_screen === "boolean",
    path + ":state-scalars",
  );
  assert(
    typeof bounds.x === "number" &&
      typeof bounds.y === "number" &&
      typeof bounds.width === "number" &&
      typeof bounds.height === "number",
    path + ":bounds-scalars",
  );
  assert(
    (snapshot.pending_close === null ||
      typeof snapshot.pending_close === "bigint") &&
      typeof snapshot.host_destroyed === "boolean",
    path + ":snapshot-scalars",
  );
  return {
    graph: [value, snapshot.window, snapshot.state, state.bounds],
    snapshot,
    window,
    state,
    bounds,
  };
}
// This immutable metadata ledger is a test-owned C root. Capture it once before
// Source, callback, or snapshots exist; never rebaseline between lifetime cuts.
const baselineEnvironment = captureEnvironment();
// The known callback is created outside stage, before any snapshot exists.
// Its only capture is a frozen primitive-only witness, a named test-owned C root.
function makeCaller() {
  const witness = Object.freeze({ tag: "caller-witness" });
  const callback = () => {
    assert(witness.tag);
  };
  return {
    callback,
    callbackWeak: new WeakRef(callback),
    witnessWeak: new WeakRef(witness),
  };
}
function stage() {
  const { sourcePrototype, protoSnapshots } = baselineEnvironment;
  const errors: string[] = [];
  native.setProto(errors, null);
  let oldGraph: object[] = [];
  let currentOK = false;
  const captured = makeCaller();
  const callback = captured.callback;
  assert(
    inspectEnvironment() && inspectCallback(callback),
    "unqualified callback or environment before Source construction",
  );
  const witness = captured.witnessWeak.deref();
  assert(
    witness &&
      !isProxy(witness) &&
      native.proto(witness) === objectPrototype &&
      native.frozen(witness),
  );
  assert.deepEqual(native.ownKeys(witness), ["tag"]);
  const tag = native.desc(witness, "tag");
  assert(tag && descriptorOwnValue(tag, "value") === "caller-witness");
  const source = new module.WindowSource(callback),
    ref = { id: 7, incarnation: 300n };
  assert(inspectSource(source));
  const beforeRows = read(source),
    before = inspectFields(beforeRows, "before");
  assert(inspectMap(before["#windows"]));
  assert.equal(
    [...native.apply(native.entries, before["#windows"], [])].length,
    0,
  );
  assert.equal(errors.length, 0);
  const created = source.decode(
    Uint8Array.from(Buffer.from(process.argv[2]!, "hex")),
  );
  assert(
    created.variant === "Created" &&
      created.window.id === 7 &&
      created.window.incarnation === 300n,
    "wrong Rust Created fixture",
  );
  source.apply(created);
  source.dispatch(created);
  const old = source.get(ref);
  assert(old);
  const oldSnapshots = [checkedSnapshot(old, "old-before").graph];
  for (let i = 0; i < 32; i++) {
    source.apply(
      source.decode(
        codec.encodeWindowEvent({
          variant: "Focus",
          window: ref,
          focused: i % 2 !== 0,
        }),
      ),
    );
    if (i < 31)
      oldSnapshots.push(
        checkedSnapshot(source.get(ref), "old-iteration").graph,
      );
  }
  source.apply(
    source.decode(
      codec.encodeWindowEvent({
        variant: "Created",
        window: { id: 3, incarnation: 900n },
        state: created.state,
        pending_close: null,
      }),
    ),
  );
  const external = source.get({ id: 3, incarnation: 900n });
  assert(external);
  checkedSnapshot(external, "external-before");
  oldGraph = oldSnapshots.flat();
  assert.equal(oldGraph.length, 128);
  const oldWeak = oldGraph.map((x) => new WeakRef(x));
  const initial = Object.fromEntries(read(source)),
    actualMap = initial["#windows"];
  assert.equal(
    actualMap,
    before["#windows"],
    "Source replaced its initial private Map",
  );
  const initialCurrent = native.apply(native.mapGet, actualMap, [7]);
  const currentWeak = checkedSnapshot(
    initialCurrent,
    "current-before",
  ).graph.map((value) => new WeakRef(value));
  assert.equal(
    [...oldWeak, ...currentWeak].filter((ref) => ref.deref() !== undefined)
      .length,
    132,
    "same-job WeakRef pre-collection control failed",
  );
  const callerRoot = mode === "caller" ? [old] : [];
  const candidateSource = source,
    probeMap = actualMap;
  function addError(message: string) {
    native.apply(native.arrayPush, errors, [message]);
  }
  function inspectEnvironment() {
    if (!environmentValid(protoSnapshots)) {
      addError("environment-changed");
      return false;
    }
    return true;
  }
  function descriptors(value: any) {
    const result = [];
    for (const key of native.ownKeys(value)) {
      const d = native.desc(value, key);
      if (!d) throw Error("missing own descriptor");
      result.push({ key, d });
    }
    return result;
  }
  function inspectSource(value: any) {
    if (isProxy(value)) {
      addError("Source:proxy");
      return false;
    }
    if (native.proto(value) !== sourcePrototype) {
      addError("Source:prototype");
      return false;
    }
    const ds = descriptors(value);
    for (const { key, d } of ds) {
      if (key !== "channel")
        addError(
          typeof key === "symbol"
            ? "Source:unknown-symbol"
            : "Source:unknown-own:" + key,
        );
      else if (
        !("value" in d) ||
        d.value !== transport.WINDOW_CHANNEL ||
        !d.writable ||
        !d.enumerable ||
        !d.configurable
      )
        addError("Source:channel-descriptor");
    }
    if (ds.filter(({ key }) => key === "channel").length !== 1)
      addError("Source:channel-missing");
    return errors.length === 0;
  }
  function inspectMap(value: any) {
    if (isProxy(value)) {
      addError("Map:proxy");
      return false;
    }
    if (native.proto(value) !== mapPrototype) {
      addError("Map:prototype");
      return false;
    }
    const ds = descriptors(value);
    for (const { key, d } of ds) {
      addError(
        typeof key === "symbol"
          ? "Map:unknown-symbol"
          : "Map:unknown-own:" + key,
      );
    }
    return ds.length === 0;
  }
  function inspectCallback(value: any) {
    if (isProxy(value)) {
      addError("Callback:proxy");
      return false;
    }
    if (native.proto(value) !== functionPrototype) {
      addError("Callback:prototype");
      return false;
    }
    const ds = descriptors(value),
      names = ds.map(({ key }) => key);
    for (const { key, d } of ds) {
      if (typeof key === "symbol" || !["length", "name"].includes(key))
        addError("Callback:unknown-own");
      if (!("value" in d)) addError("Callback:accessor");
      else if (key === "length" && typeof d.value !== "number")
        addError("Callback:length");
      else if (key === "name" && typeof d.value !== "string")
        addError("Callback:name");
    }
    if (ds.length !== 2 || !names.includes("length") || !names.includes("name"))
      addError("Callback:shape");
    return errors.length === 0;
  }
  function inspectFields(rows: any, where: string) {
    const expected = [
      "channel",
      "#windows",
      "#onFact",
      "#failureCode",
      "#failureMessage",
    ];
    const names = rows.map((r: any) => r[0]);
    if (JSON.stringify(names) !== JSON.stringify(expected)) {
      addError(where + ":unknown-fields");
    }
    const fields = Object.fromEntries(rows);
    if (where === "before") {
      if (fields["#onFact"] !== callback) addError("Source:callback-identity");
      else inspectCallback(callback);
      if (
        fields["#failureCode"] !== undefined ||
        fields["#failureMessage"] !== ""
      )
        addError("Source:initial-diagnostic");
    } else {
      if (fields["#onFact"] !== undefined) addError("Source:terminal-callback");
      if (
        typeof fields["#failureCode"] !== "string" ||
        typeof fields["#failureMessage"] !== "string"
      )
        addError("Source:terminal-diagnostic-not-primitive");
    }
    return fields;
  }
  let fields: any,
    observerIntegrity = false;
  if (inspectEnvironment() && inspectSource(candidateSource)) {
    const observedRows = read(candidateSource);
    observerIntegrity =
      JSON.stringify(observedRows.map((r: any) => r[0])) ===
        JSON.stringify([
          "channel",
          "#windows",
          "#onFact",
          "#failureCode",
          "#failureMessage",
        ]) && staticNames.length === 0;
    fields = inspectFields(observedRows, "before");
    if (fields["#windows"] !== actualMap) addError("Source:map-identity");
    inspectMap(probeMap);
  }
  if (fields && errors.length === 0) {
    const entries = [...native.apply(native.entries, actualMap, [])];
    if (
      entries.length !== 2 ||
      entries[0][0] !== 7 ||
      entries[1][0] !== 3 ||
      entries[1][1] !== external
    )
      addError("Map:entries");
    for (const [, v] of entries) {
      if (oldGraph.includes(v)) addError("Map:old-entry");
    }
    const current = native.apply(native.mapGet, actualMap, [7]);
    const currentParts = checkedSnapshot(current, "current");
    const externalParts = checkedSnapshot(external, "external");
    for (const part of [...currentParts.graph, ...externalParts.graph])
      if (oldGraph.includes(part)) addError("Source:old");
    const w = currentParts.window,
      state = currentParts.state,
      b = currentParts.bounds,
      snap = currentParts.snapshot;
    const ew = externalParts.window,
      eb = externalParts.bounds,
      es = externalParts.snapshot;
    currentOK =
      w.id === 7 &&
      w.incarnation === 300n &&
      state.focused === true &&
      state.maximized === false &&
      state.full_screen === true &&
      b.x === -3 &&
      b.y === 4 &&
      b.width === 640 &&
      b.height === 480 &&
      snap.pending_close === null &&
      snap.host_destroyed === false &&
      ew.id === 3 &&
      ew.incarnation === 900n &&
      eb.width === 640 &&
      es.pending_close === null &&
      es.host_destroyed === false;
    if (!currentOK) addError("Source:current");
  }
  const immediateAdmission =
    observerIntegrity && currentOK && errors.length === 0;
  assert(
    immediateAdmission,
    JSON.stringify({
      observerIntegrity,
      currentOK,
      errors,
    }),
  );
  return {
    source,
    sourceWeak: new WeakRef(source),
    external,
    oldWeak,
    currentWeak,
    callbackWeak: captured.callbackWeak,
    witnessWeak: captured.witnessWeak,
    callerRoot,
    prototypeGraphNodes: protoSnapshots.length,
  };
}
function readsDenied(source: any, code: string, message: string): boolean {
  try {
    source.all();
  } catch (failure) {
    return (
      transport.isCallError(failure) &&
      (failure as { code?: unknown }).code === code &&
      (failure as { message?: unknown }).message === message
    );
  }
  return false;
}
function endStage(result: any) {
  const { sourcePrototype, protoSnapshots } = baselineEnvironment,
    source = result.source;
  assert(environmentValid(protoSnapshots));
  function sourceShape() {
    if (isProxy(source) || native.proto(source) !== sourcePrototype)
      return false;
    const keys = native.ownKeys(source);
    if (keys.length !== 1 || keys[0] !== "channel") return false;
    const d = native.desc(source, "channel");
    return (
      !!d &&
      "value" in d &&
      d.value === transport.WINDOW_CHANNEL &&
      d.writable === true &&
      d.enumerable === true &&
      d.configurable === true
    );
  }
  function mapShape(map: any) {
    return (
      !isProxy(map) &&
      native.proto(map) === mapPrototype &&
      native.ownKeys(map).length === 0
    );
  }
  assert(sourceShape());
  const callback = result.callbackWeak.deref();
  assert(
    callback &&
      !isProxy(callback) &&
      native.proto(callback) === functionPrototype,
  );
  const callbackKeys = native.ownKeys(callback);
  assert.equal(callbackKeys.length, 2);
  assert(callbackKeys.includes("length") && callbackKeys.includes("name"));
  const callbackLength = native.desc(callback, "length"),
    callbackName = native.desc(callback, "name");
  assert(
    callbackLength &&
      "value" in callbackLength &&
      callbackLength.value === 0 &&
      callbackLength.writable === false &&
      callbackLength.enumerable === false &&
      callbackLength.configurable === true,
  );
  assert(
    callbackName &&
      "value" in callbackName &&
      callbackName.value === "callback" &&
      callbackName.writable === false &&
      callbackName.enumerable === false &&
      callbackName.configurable === true,
  );
  const preRows = read(source);
  assert.deepEqual(
    preRows.map((x: any) => x[0]),
    ["channel", "#windows", "#onFact", "#failureCode", "#failureMessage"],
  );
  const pre = Object.fromEntries(preRows);
  assert.equal(pre["#onFact"], callback);
  assert.equal(pre["#failureCode"], undefined);
  assert.equal(pre["#failureMessage"], "");
  const map = pre["#windows"];
  assert(mapShape(map));
  const entries = [...native.apply(native.entries, map, [])];
  assert.equal(entries.length, 2);
  assert.equal(entries[0][0], 7);
  assert.equal(entries[0][1], result.currentWeak[0].deref());
  assert.equal(entries[1][0], 3);
  assert.equal(entries[1][1], result.external);
  checkedSnapshot(entries[0][1], "end.current");
  checkedSnapshot(entries[1][1], "end.external");
  assert.equal(source.all().length, 2, "public live-window census changed");
  const error = transport.linkError(
    "KELD-IPC-022",
    "standalone source release regression ended",
  );
  assert(!isProxy(error) && native.proto(error) === errorPrototype);
  const code = native.desc(error, "code"),
    message = native.desc(error, "message");
  assert(code && "value" in code && typeof code.value === "string");
  assert(message && "value" in message && typeof message.value === "string");
  source.end(error);
  assert(environmentValid(protoSnapshots) && sourceShape());
  const postRows = read(source);
  assert.deepEqual(
    postRows.map((x: any) => x[0]),
    ["channel", "#windows", "#onFact", "#failureCode", "#failureMessage"],
  );
  const post = Object.fromEntries(postRows);
  assert.equal(post["#windows"], map);
  assert(mapShape(map));
  assert.equal([...native.apply(native.entries, map, [])].length, 0);
  assert.equal(post["#onFact"], undefined);
  assert.equal(post["#failureCode"], code.value);
  assert.equal(post["#failureMessage"], message.value);
  assert.equal(typeof post["#failureCode"], "string");
  assert.equal(typeof post["#failureMessage"], "string");
  assert(
    readsDenied(source, code.value, message.value),
    "terminal Source reads must be denied",
  );
  return { code: code.value, message: message.value };
}
const result = stage();
function nextJob<T>(run: () => T): Promise<T> {
  return new Promise((resolve, reject) =>
    setImmediate(() => {
      try {
        resolve(run());
      } catch (error) {
        reject(error);
      }
    }),
  );
}
const next = () =>
  nextJob(() => {
    Bun.gc(true);
  });
const live = (refs: WeakRef<object>[]) =>
  refs.filter((x) => x.deref() !== undefined).length;
const replacement = [];
for (let i = 0; i < 8; i++) {
  await next();
  const row = {
    old: live(result.oldWeak),
    current: live(result.currentWeak),
    callback: result.callbackWeak.deref() !== undefined,
    witness: result.witnessWeak.deref() !== undefined,
    sourceAlive: result.sourceWeak.deref() === result.source,
    externalAlive:
      result.external.window.id === 3 &&
      result.external.state.bounds.width === 640,
  };
  replacement.push(row);
  if (
    row.old === (mode === "caller" ? 4 : 0) &&
    row.current === 4 &&
    row.callback &&
    row.witness
  )
    break;
}
const repl = replacement.at(-1)!;
assert.equal(repl.old, mode === "caller" ? 4 : 0);
assert.equal(repl.current, 4);
assert(repl.callback && repl.witness && repl.sourceAlive && repl.externalAlive);
const terminal = await nextJob(() => endStage(result));
const ended = [];
for (let i = 0; i < 8; i++) {
  await next();
  const row = {
    old: live(result.oldWeak),
    current: live(result.currentWeak),
    callback: result.callbackWeak.deref() !== undefined,
    witness: result.witnessWeak.deref() !== undefined,
    sourceAlive: result.sourceWeak.deref() === result.source,
    externalAlive:
      result.external.window.id === 3 &&
      result.external.state.bounds.width === 640,
  };
  ended.push(row);
  if (
    row.old === (mode === "caller" ? 4 : 0) &&
    row.current === 0 &&
    !row.callback &&
    !row.witness
  )
    break;
}
const last = ended.at(-1)!;
assert.equal(last.old, mode === "caller" ? 4 : 0);
assert.equal(last.current, 0);
assert.equal(last.callback, false);
assert.equal(last.witness, false);
assert(last.sourceAlive && last.externalAlive);
assert(
  readsDenied(result.source, terminal.code, terminal.message),
  "terminal Source reads must remain denied after collection",
);
const afterCallerDrop = [];
if (mode === "caller") {
  result.callerRoot.length = 0;
  for (let i = 0; i < 8; i++) {
    await next();
    const row = {
      old: live(result.oldWeak),
      current: live(result.currentWeak),
      callback: result.callbackWeak.deref() !== undefined,
      witness: result.witnessWeak.deref() !== undefined,
      sourceAlive: result.sourceWeak.deref() === result.source,
      externalAlive:
        result.external.window.id === 3 &&
        result.external.state.bounds.width === 640,
    };
    afterCallerDrop.push(row);
    if (row.old === 0 && row.current === 0 && !row.callback && !row.witness)
      break;
  }
  assert.equal(afterCallerDrop.at(-1)?.old, 0);
}
console.log(
  JSON.stringify({
    runtime,
    mode,
    immediateSemantic: true,
    terminalSemantic: true,
    oldComponents: result.oldWeak.length,
    prototypeGraphNodes: result.prototypeGraphNodes,
    replacement: repl,
    end: last,
    afterCallerDrop: afterCallerDrop.at(-1) ?? null,
  }),
);
