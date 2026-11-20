// GH-1404 AC-11/AC-17 source-only Bun loader falsifiers.
//
// These tests exercise Bun 1.4.4 with the EXACT installed startup switches,
// but WITHOUT a signed KELD bundle, Core's initial graph census, or a Guardian.
// An external test process creates an unlisted code file only AFTER Bun prints
// READY. The marker is written by the loaded module itself. PASS here means
// a Bun-only pre-eval graph gate is ABSENT on that route; it does not prove
// installed KELD can be launched with an invalid pre-existing graph.
//
// Run:
//   KELD_AC11_BUN=/Users/.../.bun/bin/bun node --test tools/gh1404_ac11_bun_falsifiers.test.mjs
//
// Required installed result for AC-11/17: a typed graph/admission refusal
// BEFORE module/native code evaluates, with NO marker written.
import test from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync, copyFileSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const bun = process.env.KELD_AC11_BUN || join(homedir(), ".bun/bin/bun");
const version = spawnSync(bun, ["--version"], { encoding: "utf8", timeout: 3000 });
assert.equal(version.status, 0, "an installed Bun executable is required for these tests");
assert.equal(version.stdout.trim(), "1.4.4", "do not extrapolate the 1.4.4 proof to another Bun release");

const productionArgs = [
  "--config=/dev/null",
  "--tsconfig-override=/dev/null",
  "--no-env-file",
  "--no-install",
];

const quietEnvironment = () => ({
  HOME: homedir(),
  USER: process.env.USER || "keld-ac11",
  LOGNAME: process.env.LOGNAME || "keld-ac11",
  LANG: "C",
  PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
  BUN_RUNTIME_TRANSPILER_CACHE_PATH: "0",
});

function writeMember(root, name, source) {
  const parts = name.split("/");
  let parent = root;
  for (const part of parts.slice(0, -1)) {
    parent = join(parent, part);
    mkdirSync(parent, { recursive: true });
  }
  writeFileSync(join(root, name), source);
}

const markerModule = (token) =>
  `import { appendFileSync } from "node:fs";
appendFileSync("EXECUTED.txt", ${JSON.stringify(token + "\n")});
export const approved = false;\n`;
const markerCjs = (token) =>
  `require("node:fs").appendFileSync("EXECUTED.txt", ${JSON.stringify(token + "\n")});
module.exports = { approved: false };\n`;
const handshakeEntry = (body) => `
process.stdout.write("AC11_READY\\n");
process.stdin.once("data", async () => {
  try {
    ${body}
    process.stdout.write("AC11_DONE\\n");
  } catch (error) {
    process.stderr.write("AC11_ERROR:" + (error && error.message || error) + "\\n");
  } finally {
    process.stdin.pause();
  }
});\n`;

async function exercise({ name, body, before = {}, after = {}, flags = productionArgs, setup }) {
  const temporary = mkdtempSync(join(tmpdir(), "keld-gh1404-ac11-"));
  const root = join(temporary, "sealed-app-simulation");
  mkdirSync(root);
  try {
    writeMember(root, "main.mjs", handshakeEntry(
      typeof body === "function" ? body(temporary, root) : body,
    ));
    for (const [relative, source] of Object.entries(before)) {
      writeMember(root, relative, source);
    }
    const extra = setup?.(temporary, root);
    const child = spawn(bun, [...flags, "run", join(root, "main.mjs")], {
      cwd: root,
      stdio: ["pipe", "pipe", "pipe"],
      env: quietEnvironment(),
    });
    const result = await new Promise((resolve, reject) => {
      let stdout = "";
      let stderr = "";
      let signaled = false;
      const watchdog = setTimeout(() => {
        child.kill("SIGKILL");
        reject(new Error(`${name}: Bun did not complete within 8 seconds`));
      }, 8000);
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
        if (!signaled && stdout.includes("AC11_READY\n")) {
          signaled = true;
          try {
            for (const [relative, source] of Object.entries(after)) {
              writeMember(root, relative, source);
            }
            if (extra?.copyAfterReady) extra.copyAfterReady();
            child.stdin.end("go\n");
          } catch (error) {
            child.kill("SIGKILL");
            clearTimeout(watchdog);
            reject(error);
          }
        }
      });
      child.stderr.on("data", (chunk) => { stderr += chunk; });
      child.on("error", (error) => {
        clearTimeout(watchdog);
        reject(error);
      });
      child.on("close", (code, signal) => {
        clearTimeout(watchdog);
        resolve({ code, signal, stdout, stderr, signaled });
      });
    });
    return {
      ...result,
      marker: existsSync(join(root, "EXECUTED.txt"))
        ? readFileSync(join(root, "EXECUTED.txt"), "utf8")
        : "",
    };
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function assertPreEvalMissing(outcome, token) {
  assert.equal(outcome.signaled, true, JSON.stringify(outcome));
  assert.match(outcome.stdout, /AC11_DONE/, JSON.stringify(outcome));
  assert.equal(outcome.marker, token + "\n",
    `source-only pre-eval falsifier did not execute: ${JSON.stringify(outcome)}`);
}

test("static-import dependency of a late-loaded chunk executes before any graph check", async () => {
  const result = await exercise({
    name: "static",
    body: 'await import("./static-chunk.mjs");',
    before: { "static-chunk.mjs": 'import "./unlisted.mjs"; export const ok = true;\n' },
    after: { "unlisted.mjs": markerModule("static") },
  });
  assertPreEvalMissing(result, "static");
});

test("dynamic import of an unlisted local path evaluates the module", async () => {
  const result = await exercise({
    name: "dynamic",
    body: 'await import("./unlisted.mjs");',
    after: { "unlisted.mjs": markerModule("dynamic") },
  });
  assertPreEvalMissing(result, "dynamic");
});

test("absolute file URL outside the sealed app root evaluates without any app mutation", async () => {
  // No ancestor node_modules is needed. An external source file sits OUTSIDE
  // the installed graph even when its own app tree remains byte-identical.
  const result = await exercise({
    name: "external-file",
    body: (temporary) => `await import(${JSON.stringify(
      pathToFileURL(join(temporary, "unlisted-external.mjs")).href
    )});`,
    setup(temporary) {
      writeFileSync(join(temporary, "unlisted-external.mjs"), markerModule("external-file"));
    },
  });
  assertPreEvalMissing(result, "external-file");
});

test("CommonJS require loads an unlisted local file", async () => {
  const result = await exercise({
    name: "require",
    body: 'require("./unlisted.cjs");',
    after: { "unlisted.cjs": markerCjs("require") },
  });
  assertPreEvalMissing(result, "require");
});

test("node:module createRequire loads an unlisted local file", async () => {
  const result = await exercise({
    name: "createRequire",
    body: 'const { createRequire } = await import("node:module"); createRequire(import.meta.url)("./unlisted.cjs");',
    after: { "unlisted.cjs": markerCjs("createRequire") },
  });
  assertPreEvalMissing(result, "createRequire");
});

test("Bun resolves a bare package inserted into the app root after startup", async () => {
  const result = await exercise({
    name: "bare-package",
    body: 'await import("ac11-unlisted-marker");',
    after: {
      "node_modules/ac11-unlisted-marker/package.json": JSON.stringify({
        name: "ac11-unlisted-marker", version: "1.0.0", type: "module", main: "./index.mjs",
      }),
      "node_modules/ac11-unlisted-marker/index.mjs": markerModule("bare-package"),
    },
  });
  assertPreEvalMissing(result, "bare-package");
});

test("Worker entry inserted after startup executes before an app graph admission", async () => {
  const result = await exercise({
    name: "worker",
    body: `await new Promise((resolve, reject) => {
      const worker = new Worker(new URL("./unlisted-worker.mjs", import.meta.url), { type: "module" });
      worker.onmessage = () => { worker.terminate(); resolve(); };
      worker.onerror = (error) => { worker.terminate(); reject(error); };
    });`,
    after: {
      "unlisted-worker.mjs": markerModule("worker") + 'postMessage("completed");\n',
    },
  });
  assertPreEvalMissing(result, "worker");
});

test("package.json imports alias resolves an unlisted module after startup", async () => {
  const result = await exercise({
    name: "package-imports",
    body: 'await import("#unlisted");',
    before: {
      "package.json": JSON.stringify({ name: "ac11-source-falsifier", type: "module",
        imports: { "#unlisted": "./unlisted.mjs" } }),
    },
    after: { "unlisted.mjs": markerModule("package-imports") },
  });
  assertPreEvalMissing(result, "package-imports");
});

test("tsconfig paths override cannot remap an entry under exact production flags", async () => {
  const fixture = {
    name: "tsconfig",
    body: 'await import("@ac11/unlisted");',
    before: {
      "tsconfig.json": JSON.stringify({
        compilerOptions: { baseUrl: ".", paths: { "@ac11/unlisted": ["./unlisted.mjs"] } },
      }),
      // The alias destination exists at process start. An absent destination
      // would also fail *without* --tsconfig-override, masking the control.
      "unlisted.mjs": markerModule("tsconfig"),
    },
  };
  const blocked = await exercise(fixture);
  assert.equal(blocked.signaled, true, JSON.stringify(blocked));
  assert.doesNotMatch(blocked.stdout, /AC11_DONE/, "configured Bun unexpectedly loaded alias");
  assert.equal(blocked.marker, "", "tsconfig-override should suppress this path alias");
  const uncontrolled = await exercise({
    ...fixture,
    flags: productionArgs.filter((arg) => !arg.startsWith("--tsconfig-override=")),
  });
  assertPreEvalMissing(uncontrolled, "tsconfig");
});

test("Bun built-in specifiers are not external package marker controls", async () => {
  const result = await exercise({
    name: "builtin-control",
    body: 'const fs = await import("node:fs"); if (!fs.readFileSync) throw Error("missing built-in");',
    after: {
      "node_modules/fs/index.js": markerCjs("builtin-replacement"),
      "node_modules/fs/package.json": JSON.stringify({ name: "fs", main: "index.js" }),
    },
  });
  assert.equal(result.signaled, true);
  assert.match(result.stdout, /AC11_DONE/);
  assert.equal(result.marker, "", "node:fs must remain a Bun built-in");
});

test("process.dlopen loads an unlisted native constructor before JS gets its result", async (t) => {
  // C source is compiled into an OWNED temp dir, never checked in as an addon.
  // Its constructor writes only a harmless marker in the disposable cwd.
  const source = `
  #include <fcntl.h>
  #include <unistd.h>
  __attribute__((constructor)) static void ac11_marker(void) {
    int fd = open("EXECUTED.txt", O_CREAT | O_WRONLY | O_APPEND, 0600);
    if (fd >= 0) { (void)write(fd, "dlopen\\n", 7); (void)close(fd); }
  }
  __attribute__((visibility("default")))
  void* napi_register_module_v1(void* env, void* exports) {
    (void)env; return exports;
  }\n`;
  const result = await exercise({
    name: "dlopen",
    body: `try { process.dlopen({ exports: {} }, "./unlisted.node"); }
      catch (error) { process.stderr.write("AC11_DLOPEN_RESULT:" + error.message + "\\n"); }`,
    setup(temporary, root) {
      const cSource = join(temporary, "tiny-addon.c");
      const compiled = join(temporary, "tiny-addon.node");
      writeFileSync(cSource, source);
      const build = spawnSync("/usr/bin/clang", [
        "-arch", "arm64", "-dynamiclib", "-undefined", "dynamic_lookup",
        "-o", compiled, cSource,
      ], { encoding: "utf8", timeout: 6000 });
      assert.equal(build.status, 0, build.stderr || build.error?.message);
      return { copyAfterReady() { copyFileSync(compiled, join(root, "unlisted.node")); } };
    },
  });
  assertPreEvalMissing(result, "dlopen");
  t.diagnostic("Native constructor ran before any JS loader error; a post-dlopen catch is insufficient.");
});
