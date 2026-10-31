import { expect, test } from "bun:test";
import { resolve } from "node:path";

const transport = resolve(import.meta.dir, "../../kipc/src/transport.ts");
const facade = resolve(import.meta.dir, "app.ts");
const facts = {
  schema: 1,
  kind: "development",
  name: 'Quoted "name" λ',
  root: "/actual-test-stage",
};

function observe(value: unknown, supplied = true) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      ...(supplied
        ? [`--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(value)}`]
        : []),
      `--preload=${transport}`,
      "-e",
      `import { app } from ${JSON.stringify(facade)};
       const result = { name: app.getName(), property: app.name, root: app.getAppPath(), packaged: app.isPackaged };
       for (const [key, read] of [["version", () => app.getVersion()], ["path", () => app.getPath("userData")], ["appData", () => app.getPath("appData")], ["documents", () => app.getPath("documents")], ["locale", () => app.getLocale()]]) {
         try { result[key] = read(); } catch (error) { result[key] = error.code; }
       }
       console.log(JSON.stringify(result));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
}

test("GH452 canonical pre-entry launch facts supply name root and development mode without inventing other facts", () => {
  const child = observe(facts);
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    name: facts.name,
    property: facts.name,
    root: facts.root,
    packaged: false,
    version: "KELD-COMPAT-040",
    path: "KELD-COMPAT-040",
    appData: "KELD-COMPAT-040",
    documents: "KELD-COMPAT-040",
    locale: "KELD-COMPAT-040",
  });
});

test("GH452 schema-two values supply explicit version and native directories without inventing locale", () => {
  const value = {
    ...facts,
    schema: 2,
    version: "29.1.7-test+build.4",
    paths: {
      appData: "/Users/test/Library/Application Support",
      userData: `/Users/test/Library/Application Support/${facts.name}`,
      documents: "/Users/test/Documents",
    },
  };
  const child = observe(value);
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    name: facts.name,
    property: facts.name,
    root: facts.root,
    packaged: false,
    version: value.version,
    path: value.paths.userData,
    appData: value.paths.appData,
    documents: value.paths.documents,
    locale: "KELD-COMPAT-040",
  });
});

test("GH452 schema-two partial absence does not synthesize other values", () => {
  const child = observe({
    ...facts,
    schema: 2,
    paths: { documents: "/Users/test/Documents" },
  });
  expect(child.exitCode).toBe(0);
  const observed = JSON.parse(child.stdout.toString());
  expect(observed.documents).toBe("/Users/test/Documents");
  expect(observed.version).toBe("KELD-COMPAT-040");
  expect(observed.path).toBe("KELD-COMPAT-040");
  expect(observed.appData).toBe("KELD-COMPAT-040");
});

test("GH452 omitted launch facts stay absent instead of becoming unpackaged defaults", () => {
  const child = observe(undefined, false);
  expect(child.exitCode).not.toBe(0);
  expect(child.stderr.toString()).toContain("KELD-COMPAT-040");
  expect(child.stdout.toString()).toBe("");
});

test("GH452 schema-two paths must be an own data field without invoking inherited getters", () => {
  for (const accessor of [false, true]) {
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        "-e",
        `let reads = 0;
         const prototype = ${accessor ? "{ get paths() { reads++; return {}; } }" : "{ paths: {} }"};
         globalThis["__KELD_HOST_LAUNCH_FACTS__"] = Object.assign(Object.create(prototype), ${JSON.stringify({ ...facts, schema: 2 })});
         let code;
         try { await import(${JSON.stringify(transport)}); } catch(error) { code = error.code; }
         console.log(JSON.stringify({ code, reads }));`,
      ],
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4000,
    });
    expect(child.exitCode).toBe(0);
    expect(JSON.parse(child.stdout.toString())).toEqual({
      code: "KELD-IPC-029",
      reads: 0,
    });
  }
});

test("GH452 an explicit absent definition cannot be replaced by application globals without a preload", () => {
  const source = `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = ${JSON.stringify(facts)};
    const { app } = await import(${JSON.stringify(facade)});
    try { console.log(app.getName()); } catch (error) { console.log(error.code); }`;
  const run = (defineAbsent: boolean) =>
    Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        ...(defineAbsent
          ? ["--define=__KELD_HOST_LAUNCH_FACTS__:undefined"]
          : []),
        "-e",
        source,
      ],
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4000,
    });
  // The control deliberately omits both qualified launch inputs. It establishes
  // that the hostile assignment would matter without the Host's absent define.
  const control = run(false);
  expect(control.exitCode).toBe(0);
  expect(control.stdout.toString().trim()).toBe(facts.name);
  const protectedChild = run(true);
  expect(protectedChild.exitCode).toBe(0);
  expect(protectedChild.stderr.toString()).toBe("");
  expect(protectedChild.stdout.toString().trim()).toBe("KELD-COMPAT-040");
});

test("GH452 malformed present launch records fail before the app reads them", () => {
  for (const value of [
    null,
    [],
    {},
    { ...facts, schema: 2 },
    { ...facts, schema: 2, paths: null },
    { ...facts, schema: 2, paths: [] },
    { ...facts, schema: 2, paths: { documents: null } },
    { ...facts, schema: 2, paths: { documents: "relative" } },
    { ...facts, schema: 2, paths: { documents: "" } },
    { ...facts, schema: 2, paths: { documents: "/bad\0path" } },
    { ...facts, schema: 2, paths: { documents: "/" + "a".repeat(4096) } },
    { ...facts, schema: 2, paths: { unknown: "/path" } },
    { ...facts, schema: 2, paths: {}, version: null },
    { ...facts, schema: 2, paths: {}, version: "" },
    { ...facts, schema: 2, paths: {}, version: "x".repeat(4097) },
    { ...facts, kind: "installed" },
    { ...facts, root: "relative" },
    { ...facts, name: "" },
    { ...facts, grant: "all" },
  ]) {
    const child = observe(value);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
    expect(child.stdout.toString()).toBe("");
  }
});

test("GH452 captured record is frozen and does not reread application globals", () => {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(facts)}`,
      `--preload=${transport}`,
      "-e",
      `const captured = await import(${JSON.stringify(transport)});
       const before = captured.hostLaunchFacts;
       let refused = false;
       try { before.name = "forged"; } catch { refused = true; }
       globalThis["__KELD_HOST_LAUNCH_FACTS__"] = {name:"forged"};
       const { app } = await import(${JSON.stringify(facade)});
       console.log(JSON.stringify({refused, frozen:Object.isFrozen(before), name:app.getName()}));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(JSON.parse(child.stdout.toString())).toEqual({
    refused: true,
    frozen: true,
    name: facts.name,
  });
});

test("GH452 native directory snapshot is copied and deeply frozen", () => {
  const value = {
    ...facts,
    schema: 2,
    paths: { documents: "/Users/test/Documents" },
  };
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "-e",
      `const source = ${JSON.stringify(value)};
       globalThis["__KELD_HOST_LAUNCH_FACTS__"] = source;
       const { hostLaunchFacts } = await import(${JSON.stringify(transport)});
       source.paths.documents = "/forged";
       let refused = false;
       try { hostLaunchFacts.paths.documents = "/also-forged"; } catch { refused = true; }
       const { app } = await import(${JSON.stringify(facade)});
       console.log(JSON.stringify({ refused, copied: hostLaunchFacts.paths !== source.paths, frozen: Object.isFrozen(hostLaunchFacts.paths), documents: app.getPath("documents") }));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(JSON.parse(child.stdout.toString())).toEqual({
    refused: true,
    copied: true,
    frozen: true,
    documents: value.paths.documents,
  });
});

test("GH452 missing optional facts cannot be forged through Object.prototype after capture", () => {
  for (const value of [facts, { ...facts, schema: 2, paths: {} }]) {
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(value)}`,
        `--preload=${transport}`,
        "-e",
        `Object.prototype.version = "FORGED";
       Object.prototype.documents = "/forged";
       Object.prototype.paths = { documents: "/also-forged" };
       const { app } = await import(${JSON.stringify(facade)});
       const result = {};
       for (const [key, read] of [["version",()=>app.getVersion()],["documents",()=>app.getPath("documents")]]) {
         try { result[key] = read(); } catch(error) { result[key] = error.code; }
       }
       console.log(JSON.stringify(result));`,
      ],
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4000,
    });
    expect(child.exitCode).toBe(0);
    expect(JSON.parse(child.stdout.toString())).toEqual({
      version: "KELD-COMPAT-040",
      documents: "KELD-COMPAT-040",
    });
  }
});
