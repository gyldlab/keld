/**
 * Refusing I/O probe, loaded with `bun --preload` before a fixture (GH-491 spec
 * §3 criterion 2). Every network, Worker, child-process and file-write entry
 * point a Bun role could use is replaced by a recorder that throws, so no I/O
 * can happen even under a mutated adapter. At exit it prints one line,
 * `GH491_PROBE <json>`, with the call counts and the list of entry points it
 * covers. `probe_self_check.ts` proves the probe sees real calls, so an inert
 * probe cannot pass the suite; a Bun release that drops one of these entry
 * points shrinks `covers`, which the suite also asserts.
 */
const counts = new Map<string, number>();
const covers: string[] = [];
/** Entry points that exist but cannot be replaced on this Bun; reported, not hidden. */
const uncovered: string[] = [];

function refuse(label: string): never {
  counts.set(label, (counts.get(label) ?? 0) + 1);
  throw new Error(`GH491 probe: ${label} refused`);
}

function patch(target: object, key: string, label: string): void {
  const descriptor = Object.getOwnPropertyDescriptor(target, key);
  if (descriptor === undefined) {
    // Inherited (prototype) members are replaced on the object itself.
    if (!(key in target)) return;
  }
  const refused = function refused(): never {
    return refuse(label);
  };
  if (descriptor === undefined || descriptor.configurable) {
    Object.defineProperty(target, key, {
      configurable: true,
      enumerable: true,
      writable: true,
      value: refused,
    });
  } else if (descriptor.writable) {
    (target as Record<string, unknown>)[key] = refused;
  } else {
    uncovered.push(label);
    return;
  }
  covers.push(label);
}

function patchAll(target: object, prefix: string, keys: string[]): void {
  for (const key of keys) patch(target, key, `${prefix}.${key}`);
}

// Bun's `require` returns the mutable module object; a later ESM named import
// of the same builtin observes these replacements (probe_self_check.ts).
const builtin = (name: string): object => require(name) as object;

patch(globalThis, "fetch", "fetch");
patch(globalThis, "Worker", "Worker");
patch(globalThis, "WebSocket", "WebSocket");
patchAll(Bun, "Bun", [
  "connect",
  "listen",
  "serve",
  "udpSocket",
  "spawn",
  "spawnSync",
  "write",
]);

const net = builtin("node:net");
patchAll(net, "net", ["connect", "createConnection"]);
const socketPrototype = (net as { Socket?: { prototype?: object } }).Socket
  ?.prototype;
if (socketPrototype) patch(socketPrototype, "connect", "net.Socket.connect");
patchAll(builtin("node:tls"), "tls", ["connect"]);
patchAll(builtin("node:http"), "http", ["request", "get"]);
patchAll(builtin("node:https"), "https", ["request", "get"]);
patchAll(builtin("node:dgram"), "dgram", ["createSocket"]);
patchAll(builtin("node:child_process"), "child_process", [
  "spawn",
  "exec",
  "execFile",
  "fork",
  "spawnSync",
  "execSync",
  "execFileSync",
]);
patchAll(builtin("node:worker_threads"), "worker_threads", ["Worker"]);
// Read-only opens remain available; deny creation/truncation before a handle exists.
const fs = require("node:fs") as typeof import("node:fs");
const writeFlags =
  fs.constants.O_WRONLY |
  fs.constants.O_RDWR |
  fs.constants.O_CREAT |
  fs.constants.O_TRUNC |
  fs.constants.O_APPEND;
function patchOpen(target: object, key: string, label: string): void {
  const original = Reflect.get(target, key) as (...args: unknown[]) => unknown;
  patch(target, key, label);
  if (!covers.includes(label)) return;
  Object.defineProperty(target, key, {
    configurable: true,
    enumerable: true,
    writable: true,
    value: (...args: unknown[]) => {
      const flags = args[1];
      if (
        (typeof flags === "number" && (flags & writeFlags) !== 0) ||
        (typeof flags === "string" && /[wa+]/.test(flags))
      )
        refuse(label);
      return Reflect.apply(original, target, args);
    },
  });
}
patchOpen(fs, "open", "fs.open");
patchOpen(fs, "openSync", "fs.openSync");
patchAll(fs, "fs", ["write", "writeSync", "writev", "writevSync"]);
patchOpen(builtin("node:fs/promises"), "open", "fs/promises.open");
patchAll(builtin("node:fs"), "fs", [
  "writeFile",
  "writeFileSync",
  "appendFile",
  "appendFileSync",
  "createWriteStream",
  "mkdir",
  "mkdirSync",
  "rm",
  "rmSync",
  "unlink",
  "unlinkSync",
  "rename",
  "renameSync",
  "copyFile",
  "copyFileSync",
]);
patchAll(builtin("node:fs/promises"), "fs/promises", [
  "writeFile",
  "appendFile",
  "mkdir",
  "rm",
  "unlink",
  "rename",
  "copyFile",
]);

process.on("exit", () => {
  process.stdout.write(
    `GH491_PROBE ${JSON.stringify({ counts: Object.fromEntries(counts), covers, uncovered })}\n`,
  );
});
