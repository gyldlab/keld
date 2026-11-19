import { expect, test } from "bun:test";
import { resolve } from "node:path";

const entry = resolve(import.meta.dir, "index.ts");
const sessionFacts = resolve(import.meta.dir, "app-session-facts.ts");
const facts = {
  schema: 8,
  kind: "development",
  name: 'draw.io "Windows" λ',
  root: "\\\\?\\D:\\verified stage",
  version: "31.7.0",
  locale: "fr-CA",
  paths: {
    appData: "D:\\Observed\\Roaming",
    userData: "D:\\Observed\\Roaming\\draw.io",
    documents: "\\\\server\\share\\Documents",
  },
  originalResourcePolicy: {
    contract: "gh473-original-editor-offline/v1",
    graphSha256: "sha256:" + "ab".repeat(32),
    policySha256: "sha256:" + "cd".repeat(32),
    windowKey: "editor",
    selector: "original-editor-offline",
  },
};

function observe(input: unknown, script: string, selected = true): unknown {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:" + selected,
      "--define=__KELD_RUNTIME_PROCESS_FACTS__:" + selected,
      "--define=__KELD_HOST_LAUNCH_FACTS__:" +
        (JSON.stringify(input) ?? "undefined"),
      "-e",
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = {name:"decoy",locale:"decoy"};
      const { app } = await import(${JSON.stringify(entry)});
      const readError = (read) => { try { return read(); } catch (e) { return [e.code,e.fact,e.reason]; } };
      console.log(JSON.stringify(${script}));`,
    ],
    env: {
      ...process.env,
      KELD_NATIVE_WINDOW_FACADE: selected ? "1" : "0",
      APPDATA: "D:\\decoy",
      LANG: "de-DE",
    },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4_000,
  });
  expect(child.exitCode, child.stderr.toString()).toBe(0);
  expect(child.stderr.toString()).toBe("");
  return JSON.parse(child.stdout.toString());
}

test("selected Electron readers use exact private facts before Host Ready", () => {
  expect(
    observe(
      facts,
      `({ name: app.name, getName: app.getName(), root: app.getAppPath(),
        version: app.getVersion(), locale: app.getLocale(), packed: app.isPackaged, ready: app.isReady(),
        paths: { appData: app.getPath("appData"), userData: app.getPath("userData"), documents: app.getPath("documents") },
        nameEnumerable: Object.keys(app).includes("name"),
        assignment: readError(() => { app.name = "mutated"; }),
        badPath: readError(() => app.getPath("unsupported")) })`,
    ),
  ).toEqual({
    name: facts.name,
    getName: facts.name,
    root: facts.root,
    version: facts.version,
    locale: facts.locale,
    packed: false,
    ready: false,
    paths: facts.paths,
    nameEnumerable: false,
    assignment: ["KELD-COMPAT-042", null, null],
    badPath: ["KELD-COMPAT-041", null, null],
  });
});

test("Windows helper projects only acquired or contended without acting on the lock", () => {
  const script = `([app.requestSingleInstanceLock(), app.requestSingleInstanceLock(undefined),
                   readError(() => app.requestSingleInstanceLock({ note: "unsupported" }))])`;
  for (const verdict of ["acquired", "contended"] as const) {
    const expected = verdict === "acquired";
    expect(observe({ ...facts, singleInstance: { verdict } }, script)).toEqual([
      expected,
      expected,
      ["KELD-COMPAT-013", null, null],
    ]);
  }
});

test("absent selected facts stay unavailable and selection leaves unselected app alone", () => {
  const readLock = `readError(() => app.requestSingleInstanceLock())`;
  expect(observe(facts, readLock)).toEqual([
    "KELD-COMPAT-040",
    "singleInstance",
    "undeclared",
  ]);
  expect(observe(undefined, readLock)).toEqual([
    "KELD-COMPAT-040",
    "singleInstance",
    "unavailable",
  ]);
  expect(
    observe(
      { ...facts, locale: undefined, paths: {} },
      `[readError(() => app.getPath("userData")), readError(() => app.getLocale())]`,
    ),
  ).toEqual([
    ["KELD-COMPAT-040", "path.userData", null],
    ["KELD-COMPAT-040", "locale", null],
  ]);
  expect(
    observe(
      undefined,
      `["getPath", "getVersion", "getLocale", "requestSingleInstanceLock", "name", "isPackaged"].map(key => Reflect.has(app,key))`,
      false,
    ),
  ).toEqual([false, false, false, false, false, false]);
});

test("selected app attachment ignores inherited setters and replaced own writers", () => {
  expect(
    observe(
      facts,
      `await (async () => {
        const { attachAppSessionFacts } = await import(${JSON.stringify(sessionFacts)});
        const define = Object.defineProperty;
        const assign = Object.assign;
        let hooks = 0;
        const poison = () => { hooks++; throw new Error("poisoned property write"); };
        define(Object.prototype, "getPath", { set: poison, configurable: true });
        let prototypeTarget, target;
        try {
          prototypeTarget = attachAppSessionFacts({});
          Object.assign = poison;
          Object.defineProperty = poison;
          target = attachAppSessionFacts({});
        }
        finally {
          delete Object.prototype.getPath;
          Object.defineProperty = define;
          Object.assign = assign;
        }
        return { path: target.getPath("userData"), own: Object.hasOwn(target,"getPath"),
          prototypeOwn: Object.hasOwn(prototypeTarget,"getPath"), hooks };
      })()`,
    ),
  ).toEqual({
    path: facts.paths.userData,
    own: true,
    prototypeOwn: true,
    hooks: 0,
  });
});

test("late selected and unselected facts imports use pre-app diagnostic primitives", () => {
  for (const selected of [true, false]) {
    expect(
      observe(
        selected ? facts : undefined,
        `await (async () => {
          const ActualError = Error;
          const stringify = JSON.stringify;
          const join = Array.prototype.join;
          let hooks = 0;
          const poison = () => { hooks++; throw new ActualError("late diagnostic hook"); };
          let target, error;
          try {
            globalThis.Error = poison;
            JSON.stringify = poison;
            Array.prototype.join = poison;
            const { attachAppSessionFacts } = await import(${JSON.stringify(sessionFacts + "?late")});
            target = attachAppSessionFacts({});
            if (Object.hasOwn(target, "getPath")) {
              try { target.getPath("bogus"); }
              catch (cause) { error = [cause instanceof ActualError, cause.name, cause.code, cause.fact, cause.message]; }
            }
          } finally {
            globalThis.Error = ActualError;
            JSON.stringify = stringify;
            Array.prototype.join = join;
          }
          return { own: Object.hasOwn(target, "getPath"), error, hooks };
        })()`,
        selected,
      ),
    ).toEqual(
      selected
        ? {
            own: true,
            error: [
              true,
              "AppSessionFactError",
              "KELD-COMPAT-041",
              null,
              'KELD-COMPAT-041: app.getPath received "bogus". Pass a documented path name from home, appData, assets, userData, sessionData, temp, exe, module, desktop, documents, downloads, music, pictures, videos, recent, logs, crashDumps.',
            ],
            hooks: 0,
          }
        : { own: false, hooks: 0 },
    );
  }
});
