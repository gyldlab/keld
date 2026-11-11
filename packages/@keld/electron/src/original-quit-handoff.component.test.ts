// Actual source-owned Promise logic with pure scope/transport inputs. No Host,
// Ready, native document or CloseRequested EVENT is fabricated or authenticated.
// The real BrowserWindow/Host/native composition is a separate root-run gate.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const app = readFileSync(new URL("./app.ts", import.meta.url), "utf8");
const start = app.indexOf("interface QuitAttempt {");
const end = app.indexOf(
  "\napiApp.onLastWindowClosed(onLastWindowClosed);",
  start,
);
expect(start).toBeGreaterThan(0);
expect(end).toBeGreaterThan(start);
let code = app
  .slice(start, end)
  .replace(
    "export function joinOriginalQuitBeforeClose",
    "function joinOriginalQuitBeforeClose",
  );
const mutation = process.env.KELD_QUIT_HANDOFF_MUTATION;
if (mutation === "omit-handoff-resolve")
  code = code.replace("  attempt.resolve();", "");
if (mutation === "omit-scope-check")
  code = code.replace(
    "  if (!attempt?.handoff?.isCurrentPending(window, closeSeq)) return false;",
    "  if (!attempt?.handoff) return false;",
  );
if (mutation === "omit-late-fatal")
  code = code.replace(
    /attempt\.handoff\?\.failAfterHandoff\([\s\S]*?\);/,
    "void error;",
  );
if (mutation === "premature-detach")
  code = code.replace(
    "void terminal(attempt).then(",
    "activeQuitAttempt = undefined;\n    void terminal(attempt).then(",
  );
const js = new Bun.Transpiler({ loader: "ts" }).transformSync(code);

function setup() {
  const ctx = vm.createContext(Object.create(null), {
    codeGeneration: { strings: false, wasm: false },
  });
  vm.runInContext(
    `
let beforeCalls=0;
function emit() { beforeCalls++; return false; }
const trace=[], lateErrors=[];
let live=true, closeSeq=3n, finishRpc, failRpc, reported;
const reportedPromise=new Promise(resolve=>{reported=resolve;});
const rpc=new Promise((resolve,reject)=>{finishRpc=resolve;failRpc=reject;});
const selectedWindow={id:7,incarnation:258n};
const scope={window:selectedWindow,
 isCurrentPending(window,seq){return live && window.id===7 && window.incarnation===258n && seq===closeSeq;},
 failAfterHandoff(error){lateErrors.push(error);trace.push('retired-original-link');reported(error);}
};
const reason=new Error('KELD-IPC-006: actual deferred request failure input');
${js}
`,
    ctx,
    { timeout: 500 },
  );
  return {
    run(code: string): unknown {
      return vm.runInContext(code, ctx, { timeout: 500 });
    },
    read(code: string): unknown {
      return JSON.parse(
        vm.runInContext(`JSON.stringify(${code})`, ctx, { timeout: 500 }),
      );
    },
  };
}

test("pending handoff keeps exact D2 nested/concurrent Promise sharing", async () => {
  const f = setup();
  const first = f.run(
    "const first=requestQuitAfterBeforeEvent(attempt=>{attempt.handoff=scope;return rpc;}); first",
  );
  expect(f.run("requestQuitAfterBeforeEvent(()=>Promise.resolve())")).toBe(
    first,
  );
  expect(f.read("beforeCalls")).toBe(1);
  f.run("finishRpc()");
  await first;
});

test("witnessed handoff settles before callback while RPC reply is still pending", async () => {
  const f = setup();
  const first = f.run(
    "const first=requestQuitAfterBeforeEvent(attempt=>{attempt.handoff=scope;return rpc;}); first",
  );
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow,3n)")).toBe(true);
  const second = f.run(
    "const second=requestQuitAfterBeforeEvent(()=>Promise.resolve()); second",
  );
  expect(second).not.toBe(first);
  await first;
  expect(f.read("beforeCalls")).toBe(2);
  f.run("finishRpc()");
  await second;
});

test("unrelated WindowRef, incarnation or close sequence cannot settle an attempt", async () => {
  const f = setup();
  const first = f.run(
    "const first=requestQuitAfterBeforeEvent(attempt=>{attempt.handoff=scope;return rpc;}); first",
  );
  expect(f.run("joinOriginalQuitBeforeClose({id:8,incarnation:258n},3n)")).toBe(
    false,
  );
  expect(f.run("joinOriginalQuitBeforeClose({id:7,incarnation:259n},3n)")).toBe(
    false,
  );
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow,2n)")).toBe(false);
  expect(f.run("requestQuitAfterBeforeEvent(()=>Promise.resolve())")).toBe(
    first,
  );
  f.run("finishRpc()");
  await first;
});

test("retired scope input and absent handoff scope cannot settle", async () => {
  const f = setup();
  const first = f.run(
    "const first=requestQuitAfterBeforeEvent(attempt=>{attempt.handoff=scope;return rpc;}); live=false; first",
  );
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow,3n)")).toBe(false);
  expect(f.run("requestQuitAfterBeforeEvent(()=>Promise.resolve())")).toBe(
    first,
  );
  f.run("finishRpc()");
  await first;
  const second = f.run(
    "const second=requestQuitAfterBeforeEvent(()=>rpc); second",
  );
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow,3n)")).toBe(false);
  await second;
});

test("actual deferred rejection before handoff rejects with the same error and permits fresh attempt", async () => {
  const f = setup();
  const first = f.run(
    "const first=requestQuitAfterBeforeEvent(attempt=>{attempt.handoff=scope;return rpc;}); first",
  );
  f.run("failRpc(reason)");
  await expect(first).rejects.toBe(f.run("reason"));
  expect(f.read("lateErrors.length")).toBe(0);
  const next = f.run("requestQuitAfterBeforeEvent(()=>Promise.resolve())");
  expect(next).not.toBe(first);
  await next;
});

test("actual deferred ERR after handoff reaches fatal owner with identical cause and cannot clear a new attempt", async () => {
  const f = setup();
  const first = f.run(
    "const first=requestQuitAfterBeforeEvent(attempt=>{attempt.handoff=scope;return rpc;}); first",
  );
  f.run("joinOriginalQuitBeforeClose(selectedWindow,3n)");
  await first;
  const next = f.run(
    "let finishNext; const next=requestQuitAfterBeforeEvent(()=>new Promise(resolve=>{finishNext=resolve;})); next",
  );
  f.run("failRpc(reason)");
  expect(await f.run("reportedPromise")).toBe(f.run("reason"));
  expect(f.read("trace")).toEqual(["retired-original-link"]);
  expect(f.run("requestQuitAfterBeforeEvent(()=>Promise.resolve())")).toBe(
    next,
  );
  f.run("finishNext()");
  await next;
});

test("handoff hook is placed before real BrowserWindow close listener dispatch", () => {
  const source = readFileSync(
    new URL("./browser-window.ts", import.meta.url),
    "utf8",
  );
  const begin = source.indexOf("  #nativeClose(close_seq: bigint): void {");
  const hook = source.indexOf(
    "joinOriginalQuitBeforeClose(this.#ref, close_seq);",
    begin,
  );
  const user = source.indexOf('this.emit("close", event);', begin);
  expect(hook).toBeGreaterThan(begin);
  expect(user).toBeGreaterThan(hook);
});

test("actual fatal link method preserves cause and closes its owning link", () => {
  const link = readFileSync(
    new URL("../../api/src/link.ts", import.meta.url),
    "utf8",
  );
  const from = link.indexOf(
    "  retireWindowHandoffFailure(error: Error): void {",
  );
  const to = link.indexOf("\n  /**", from);
  let method = link.slice(from, to);
  if (mutation === "omit-fatal-close")
    method = method.replace("    this.#link.close();", "");
  const ownerCode = `class Owner { #dead; #closed=false; #link; constructor(link){this.#link=link;}
${method}
get error(){return this.#dead;} get closed(){return this.#closed;} }
let closes=0; const owner=new Owner({close(){closes++;}}); const cause=new Error('KELD-IPC-006: deferred'); owner.retireWindowHandoffFailure(cause);
`;
  const ctx = vm.createContext(Object.create(null), {
    codeGeneration: { strings: false, wasm: false },
  });
  vm.runInContext(
    new Bun.Transpiler({ loader: "ts" }).transformSync(ownerCode),
    ctx,
    { timeout: 500 },
  );
  expect(
    vm.runInContext("owner.error===cause && owner.closed && closes===1", ctx),
  ).toBe(true);
});

for (const native of ["0", "1"] as const) {
  test(`actual unqualified source keeps ${native === "1" ? "COMPAT012" : "IPC007"}`, () => {
    const env: Record<string, string | undefined> = {
      ...process.env,
      KELD_NATIVE_WINDOW_FACADE: native,
    };
    delete env.KELD_APP_LINK;
    delete env.BUN_OPTIONS;
    delete env.NODE_OPTIONS;
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        new URL(
          "../fixtures/original_quit_handoff_unqualified.ts",
          import.meta.url,
        ).pathname,
      ],
      env,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4_000,
    });
    expect(child.exitCode).toBe(0);
    expect(child.stderr.toString()).toBe("");
    expect(child.stdout.toString()).toContain(
      "ORIGINAL_HANDOFF_UNQUALIFIED_REFUSAL_OK",
    );
  });
}
