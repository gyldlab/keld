import { expect, test } from "bun:test";
import { join } from "node:path";
for (const mode of [
  "push",
  "freeze",
  "get",
  "iterator",
  "integer",
  "bigint",
  "byteLength",
  "brand",
  "invocation",
  "inherited-slot",
  "public-copy-slot",
  "native-primitives",
  "failure-error",
  "failure-define",
  "failure-startsWith",
  "view-custody",
  "escape-values",
  "escape-clear",
  "escape-iterator",
  "lifecycle-length",
  "err-iterator",
  "err-startsWith",
  "err-subarray",
  "err-length",
  "err-textdecoder",
  "err-decode",
]) {
  const name =
    mode === "native-primitives"
      ? "snapshot operations reuse captured native targets"
      : mode === "public-copy-slot"
        ? "public Source.all copy ignores inherited numeric setters"
        : `state phase does not call replaced ${mode}`;
  test(name, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "window-state-phase.test-role.ts"),
        mode,
      ],
      {
        env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "0" },
        stdout: "pipe",
        stderr: "pipe",
        stdin: "ignore",
      },
    );
    const [exit, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect({ exit, stderr }).toEqual({ exit: 0, stderr: "" });
    expect(JSON.parse(stdout)).toEqual({
      mode,
      hits: 0,
      variant: mode.startsWith("failure-") ? "rejected" : "Created",
    });
  });
}

// Reuse the actual native fixture's early dependency and bundling owner. This
// distinguishes a trusted staged preload from capture caused by API import order.
test("staged canonical preload precedes an early application dependency", async () => {
  const {
    mkdtempSync,
    realpathSync,
    readFileSync,
    writeFileSync,
    copyFileSync,
    rmSync,
    readdirSync,
  } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { resolve } = await import("node:path");
  const repo = resolve(import.meta.dir, "../../../..");
  // A Windows TEMP path can use an 8.3 alias while module imports use the long
  // path. Canonicalize once so preload, entry and cwd name the same module owner.
  const stage = realpathSync.native(
    mkdtempSync(join(tmpdir(), "keld-state-phase-")),
  );
  async function run(args: string[]) {
    const child = Bun.spawn([process.execPath, ...args], {
      cwd: stage,
      stdout: "pipe",
      stderr: "pipe",
      stdin: "ignore",
    });
    const [exit, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    return { exit, stdout, stderr };
  }
  try {
    const probe = readFileSync(
      join(import.meta.dir, "../fixtures/window-state-probe.ts"),
      "utf8",
    );
    writeFileSync(join(stage, "state-phase-probe.ts"), probe);
    const build = readFileSync(
      join(import.meta.dir, "../fixtures/staged-api-bundle.ts"),
      "utf8",
    );
    writeFileSync(join(stage, "build.ts"), build);
    writeFileSync(
      join(stage, "entry.ts"),
      `
import {observeStateAppliers,assertStatePhase} from "./state-phase-probe.ts";
import {app} from ${JSON.stringify(join(repo, "packages/@keld/api/src/app.ts"))};
import {attachMirror} from ${JSON.stringify(join(repo, "packages/@keld/api/src/mirror.ts"))};
import {WindowSource} from ${JSON.stringify(join(repo, "packages/@keld/api/src/window-source.ts"))};
import {encodeWindowEvent} from ${JSON.stringify(join(repo, "packages/@keld/api/src/window-codec.ts"))};
import {WINDOW_CHANNEL} from ${JSON.stringify(join(repo, "packages/@keld/kipc/src/transport.ts"))};
class Link {
 stateFactCapacity=2;
 state!: (bytes:Uint8Array)=>void;
 dispatch!: ()=>void;
 setStateApplier(_channel:number,callback:(bytes:Uint8Array)=>void) {this.state=callback;}
 onEvent(_channel:number,callback:()=>void) {this.dispatch=callback;}
}
observeStateAppliers(Link,WINDOW_CHANNEL);
const link=new Link();
let fact;
attachMirror(link,new WindowSource(value=>{fact=value;}));
link.state(encodeWindowEvent({variant:"Created",window:{id:7,incarnation:300n},state:{bounds:{x:0,y:0,width:640,height:480},focused:false,maximized:false,full_screen:false},pending_close:null}));
link.dispatch();
assertStatePhase();
if(fact?.variant!=="Created" || typeof app.whenReady!=="function" || !import.meta.main || Bun.main!==import.meta.path) throw new Error("staged direct entry changed");
console.log("PASS staged same canonical capture");
`,
    );
    const bundled = await run([
      join(stage, "build.ts"),
      join(stage, "entry.ts"),
      join(stage, "main.ts"),
    ]);
    expect(bundled).toEqual({ exit: 0, stdout: "", stderr: "" });
    copyFileSync(
      join(repo, "packages/@keld/kipc/src/transport.ts"),
      join(stage, "kipc-transport.ts"),
    );
    // run() fixes cwd to stage; use the same portable module specifier as the
    // staged imports instead of a platform-specific absolute filesystem path.
    const preload = await run([
      "--preload=./kipc-transport.ts",
      "run",
      join(stage, "main.ts"),
    ]);
    expect(preload).toEqual({
      exit: 0,
      stdout: "PASS staged same canonical capture\n",
      stderr: "",
    });
    // Bun.build hoists the external transport before inlined dependencies;
    // omission is therefore a useful passing control for this bundled layout.
    const bundledWithoutPreload = await run(["run", join(stage, "main.ts")]);
    expect(bundledWithoutPreload).toEqual(preload);
    // A direct original entry can evaluate an earlier dependency first. Rewrite
    // only module locations in temporary copies, retaining one staged canonical
    // owner; an omitted preload must expose that distinct load-order defect.
    let direct = readFileSync(join(stage, "entry.ts"), "utf8");
    const apiRoot = join(repo, "packages/@keld/api/src");
    for (const file of readdirSync(apiRoot)) {
      if (!file.endsWith(".ts") || file.includes("test")) continue;
      writeFileSync(
        join(stage, file),
        readFileSync(join(apiRoot, file), "utf8").replaceAll(
          "@keld/kipc",
          "./kipc-transport.ts",
        ),
      );
      direct = direct.replaceAll(
        JSON.stringify(join(apiRoot, file)),
        JSON.stringify(`./${file}`),
      );
    }
    direct = direct.replaceAll(
      JSON.stringify(join(repo, "packages/@keld/kipc/src/transport.ts")),
      JSON.stringify("./kipc-transport.ts"),
    );
    writeFileSync(join(stage, "direct.ts"), direct);
    const directPreload = await run([
      "--preload=./kipc-transport.ts",
      "run",
      join(stage, "direct.ts"),
    ]);
    expect(directPreload).toEqual(preload);
    const omitted = await run(["run", join(stage, "direct.ts")]);
    expect(omitted.exit).not.toBe(0);
    expect(omitted.stderr).toContain("application hooks=");
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
});
