/**
 * GH-491 spec §3 criteria 2 and 10: the import-time and replayed-sequence I/O
 * contract, and the draw.io boot sequence in its three environments. Each case
 * runs a fixture child under `fixtures/probe_preload.ts`, which replaces every
 * network, Worker, child-process and file-write entry point with a recorder that
 * throws; `probe_self_check.ts` proves the probe sees real calls through the
 * import forms production code uses. The child's environment is built
 * explicitly: `DRAWIO_DISABLE_UPDATE` and `GH491_PACKAGED` are deleted, then set
 * per case, never inherited (#491 note: the macOS non-strict spawn inherits the
 * operator shell).
 */
import { join } from "node:path";
import { expect, test } from "bun:test";

const fixtures = join(import.meta.dir, "..", "fixtures");

/** Kill switch only; every fixture exits by itself once its sequence settles. */
const FIXTURE_KILL_MS = 10_000;

/** The entry points the probe must cover on this Bun; a shrink is a loud failure. */
const PROBE_COVERS = [
  "fetch",
  "Worker",
  "WebSocket",
  "Bun.connect",
  "Bun.listen",
  "Bun.serve",
  "Bun.udpSocket",
  "Bun.spawn",
  "Bun.spawnSync",
  "Bun.write",
  "net.connect",
  "net.createConnection",
  "net.Socket.connect",
  "tls.connect",
  "http.request",
  "http.get",
  "https.request",
  "https.get",
  "dgram.createSocket",
  "child_process.spawn",
  "child_process.exec",
  "child_process.execFile",
  "child_process.fork",
  "child_process.spawnSync",
  "child_process.execSync",
  "child_process.execFileSync",
  "worker_threads.Worker",
  "fs.writeFile",
  "fs.writeFileSync",
  "fs.appendFile",
  "fs.appendFileSync",
  "fs.createWriteStream",
  "fs.mkdir",
  "fs.mkdirSync",
  "fs.rm",
  "fs.rmSync",
  "fs.unlink",
  "fs.unlinkSync",
  "fs.rename",
  "fs.renameSync",
  "fs.copyFile",
  "fs.copyFileSync",
  "fs/promises.writeFile",
  "fs/promises.appendFile",
  "fs/promises.mkdir",
  "fs/promises.rm",
  "fs/promises.unlink",
  "fs/promises.rename",
  "fs/promises.copyFile",
];

interface Probe {
  counts: Record<string, number>;
  covers: string[];
  uncovered: string[];
}

interface Run {
  code: number | null;
  stderr: string;
  result: Record<string, unknown>;
  probe: Probe;
}

type EnvEdit = (env: Record<string, string | undefined>) => void;

function runFixture(script: string, edit: EnvEdit = () => {}): Run {
  const env: Record<string, string | undefined> = { ...process.env };
  delete env.DRAWIO_DISABLE_UPDATE;
  delete env.GH491_PACKAGED;
  edit(env);
  const child = Bun.spawnSync({
    cmd: [process.execPath, "--preload", "./probe_preload.ts", `./${script}`],
    cwd: fixtures,
    env,
    stdout: "pipe",
    stderr: "pipe",
    timeout: FIXTURE_KILL_MS,
    killSignal: "SIGKILL",
  });
  const stdout = child.stdout.toString();
  const line = (prefix: string): Record<string, unknown> => {
    const found = stdout
      .split("\n")
      .find((candidate) => candidate.startsWith(prefix));
    if (found === undefined) {
      throw new Error(`${script}: no ${prefix.trim()} line in\n${stdout}`);
    }
    return JSON.parse(found.slice(prefix.length)) as Record<string, unknown>;
  };
  return {
    code: child.exitCode,
    stderr: child.stderr.toString(),
    result: line("GH491_RESULT "),
    probe: line("GH491_PROBE ") as unknown as Probe,
  };
}

function expectCleanExit(run: Run): void {
  expect(run.stderr).toBe("");
  expect(run.code).toBe(0);
}

function expectNoIo(run: Run): void {
  expect(run.probe.counts).toEqual({});
  expect(run.probe.uncovered).toEqual([]);
  expect(run.probe.covers).toEqual(PROBE_COVERS);
}

const ZERO_EVENTS = {
  error: 0,
  "update-not-available": 0,
  "download-progress": 0,
  "update-downloaded": 0,
  "update-available": 0,
  "checking-for-update": 0,
  login: 0,
  "update-cancelled": 0,
  "appimage-filename-updated": 0,
};

// Negative control for the probe itself: an inert probe (no replacements)
// records nothing here, and the attempts report "no-throw" or an OS error.
test("electron-updater.probe.self-check: the refusing probe observes fetch, an ESM-imported net.connect, fs.writeFileSync and child_process.spawnSync, Bun.connect and a Worker constructor", () => {
  const run = runFixture("probe_self_check.ts");
  expectCleanExit(run);
  expect(run.probe.counts).toEqual({
    fetch: 1,
    "net.connect": 1,
    "fs.writeFileSync": 1,
    "child_process.spawnSync": 1,
    "Bun.connect": 1,
    Worker: 1,
  });
  expect(run.probe.uncovered).toEqual([]);
  expect(run.probe.covers).toEqual(PROBE_COVERS);
  expect(run.result.caught).toEqual({
    fetch: "GH491 probe: fetch refused",
    "net.connect": "GH491 probe: net.connect refused",
    "fs.writeFileSync": "GH491 probe: fs.writeFileSync refused",
    "child_process.spawnSync": "GH491 probe: child_process.spawnSync refused",
    "Bun.connect": "GH491 probe: Bun.connect refused",
    Worker: "GH491 probe: Worker refused",
  });
});

// Negative control: a top-level `Bun.connect(...)` in src/index.ts is recorded.
test("electron-updater.import.zero-io: importing the adapter the way draw.io does opens no socket, Worker, child process or file, and yields the destructurable EventEmitter", () => {
  const run = runFixture("import_only.ts");
  expectCleanExit(run);
  expectNoIo(run);
  expect(run.result).toEqual({
    destructuredIsEventEmitter: true,
    destructuredIsNamedExport: true,
    defaultHasAppUpdater: true,
    defaultHasNoOpLogger: true,
    electronVersionShim: "0.0.1",
  });
});

// Negative control: make setFeedURL throw; the replica records a synchronous
// throw (draw.io's safeUpdaterCall would open its Update Error dialog).
test("electron-updater.replay.env-disabled: with DRAWIO_DISABLE_UPDATE=true set explicitly, the draw.io sequence makes zero checkForUpdates calls, sees zero error events and zero throws, and the logger identity holds", () => {
  const run = runFixture("replay.ts", (env) => {
    env.DRAWIO_DISABLE_UPDATE = "true";
  });
  expectCleanExit(run);
  expectNoIo(run);
  expect(run.result).toMatchObject({
    disableUpdate: true,
    packagedFacts: false,
    loggerIdentity: true,
    levelsLanded: true,
    setFeedURLReturn: "undefined",
    checkForUpdatesCalls: 0,
    checkOutcome: "not-called",
    events: ZERO_EVENTS,
    errorCodes: [],
    rejections: [],
    syncThrows: [],
    quitCalls: 0,
    getFeedURL: "Deprecated. Do not use it.",
    logLines: [],
  });
});

// Negative control: remove the packaged gate; the facade-backed check refuses
// and the replica records a rejection.
test("electron-updater.replay.env-absent-unpackaged: with the variable absent and the Electron facade unpackaged, the boot check runs once, resolves null and emits nothing", () => {
  const run = runFixture("replay.ts");
  expectCleanExit(run);
  expectNoIo(run);
  expect(run.result).toMatchObject({
    disableUpdate: false,
    packagedFacts: false,
    loggerIdentity: true,
    levelsLanded: true,
    setFeedURLReturn: "undefined",
    checkForUpdatesCalls: 1,
    checkOutcome: "null",
    events: ZERO_EVENTS,
    errorCodes: [],
    rejections: [],
    syncThrows: [],
    quitCalls: 0,
    logLines: [
      "info Skip checkForUpdates because application is not packed and dev update config is not forced",
    ],
  });
});

// Negative controls: resolve null when packaged (no error, no rejection), or
// emit update-not-available (counted in events).
test("electron-updater.replay.env-absent-packaged-no-host-updater: with the variable absent, packaged facts and no host updater, the boot check runs once, emits exactly one error carrying KELD-COMPAT-050 and its rejection reaches draw.io's catch, with no other event and no quit", () => {
  const run = runFixture("replay.ts", (env) => {
    env.GH491_PACKAGED = "1";
  });
  expectCleanExit(run);
  expectNoIo(run);
  expect(run.result).toMatchObject({
    disableUpdate: false,
    packagedFacts: true,
    loggerIdentity: true,
    levelsLanded: true,
    setFeedURLReturn: "undefined",
    checkForUpdatesCalls: 1,
    checkOutcome: "rejected:KELD-COMPAT-050",
    events: { ...ZERO_EVENTS, error: 1 },
    errorCodes: ["KELD-COMPAT-050"],
    syncThrows: [],
    quitCalls: 0,
  });
  const rejections = run.result.rejections as Array<Record<string, unknown>>;
  expect(rejections).toHaveLength(1);
  expect(rejections[0]).toMatchObject({
    label: "checkForUpdates (boot)",
    code: "KELD-COMPAT-050",
  });
  expect(String(rejections[0]?.message).startsWith("KELD-COMPAT-050: ")).toBe(
    true,
  );
  const logLines = run.result.logLines as string[];
  expect(logLines).toHaveLength(1);
  expect(logLines[0]?.startsWith("error Error: Error: KELD-COMPAT-050: ")).toBe(
    true,
  );
});
