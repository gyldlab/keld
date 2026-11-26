// Source-bound negative controls. These model authenticated WindowSource facts
// but do not claim an installed Host, AppKit, or renderer-veto acceptance run.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("./app.ts", import.meta.url), "utf8");
const begin = source.indexOf("export function captureOriginalWindowQuitScope(");
const end = source.indexOf("/** Current admitted desktop facts.", begin);
expect(begin).toBeGreaterThan(0);
expect(end).toBeGreaterThan(begin);
let logicBody = source
  .slice(begin, end)
  .replace("export function ", "function ");
if (process.env.KELD_GH451_MUTANT === "drop-document-generation")
  logicBody = logicBody
    .replaceAll(
      "snapshot.document.generation === originalDocumentGeneration",
      "true",
    )
    .replaceAll(
      "snapshot.document?.generation === originalDocumentGeneration",
      "true",
    );
const logic = new Bun.Transpiler({ loader: "ts" }).transformSync(logicBody);

function fixture() {
  const context = vm.createContext(Object.create(null), {
    codeGeneration: { strings: false, wasm: false },
  });
  vm.runInContext(
    `
const ref = {id: 7, incarnation: 258n};
let documentGeneration = 7;
let documentCurrent = true;
let documentFinished = true;
let destroyed = false;
let hasSnapshot = true;
const windowSource = {
  get(candidate) {
    if (!hasSnapshot || candidate.id !== ref.id || candidate.incarnation !== ref.incarnation)
      return undefined;
    return {
      window: ref,
      pending_close: destroyed ? null : 3n,
      host_destroyed: destroyed,
      document: {
        generation: documentGeneration,
        current: documentCurrent,
        finished: documentFinished,
      },
    };
  },
};
const session = {windowSource, windowAdmissionOpen: true};
let connectedLink = session;
let linkDead;
const deliveredWindowCloses = {
  pending() {return [{window: ref, closeSeq: 3n}];},
};
function currentOriginalWindowSession() {return connectedLink;}
function invokeWindow() {return Promise.resolve({variant: "CloseRequested", close_seq: 3n});}
function failReadyWaiters() {}
function linkError(code, reason) {return Object.assign(new Error(reason), {code});}
${logic}
const first = captureOriginalWindowQuitScope(ref);
`,
    context,
    { timeout: 500 },
  );
  return {
    run<T = unknown>(expression: string): T {
      return vm.runInContext(expression, context, { timeout: 500 }) as T;
    },
  };
}

test("original Quit source pins the selected document generation on the same native WindowRef", async () => {
  const f = fixture();
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(true);
  expect(f.run<boolean>("first.isCurrentPending(ref, 3n)")).toBe(true);
  f.run("documentGeneration = 8");
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(false);
  expect(f.run<boolean>("first.isCurrentPending(ref, 3n)")).toBe(false);
  await expect(f.run<Promise<unknown>>("first.requestClose()")).rejects.toThrow(
    "original Quit window scope retired before close admission",
  );
  f.run("const second = captureOriginalWindowQuitScope(ref)");
  expect(f.run<boolean>("second.ownsCurrentGeneration()")).toBe(true);
  expect(f.run<boolean>("second.isCurrentPending(ref, 3n)")).toBe(true);
});

test("retiring the document or link invalidates Quit; a same-generation native Destroyed remains observable", () => {
  const f = fixture();
  f.run("documentCurrent = false");
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(false);
  f.run("documentCurrent = true; destroyed = true");
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(true);
  f.run("hasSnapshot = false");
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(true);
  f.run("connectedLink = {windowSource, windowAdmissionOpen:true}");
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(false);
});

test("a recovered document cannot borrow an old Quit suppression after native destruction", () => {
  const f = fixture();
  f.run("documentGeneration = 8; destroyed = true");
  expect(f.run<boolean>("first.ownsCurrentGeneration()")).toBe(false);
  f.run("hasSnapshot = false");
  // The live app's applied-Host-fact listener revokes the old claim before
  // this tombstone can disappear; a missing snapshot alone does not mint G2.
  expect(f.run<boolean>("first.isCurrentPending(ref, 3n)")).toBe(false);
});
