import { expect, test } from "bun:test";
import { resolve } from "node:path";

const transport = resolve(import.meta.dir, "../../kipc/src/transport.ts");
const facade = resolve(import.meta.dir, "app.ts");
const facts = {
  schema: 1,
  kind: "development",
  name: 'Exact Windows "name" λ',
  root: "\\\\?\\D:\\actual-test-stage λ",
};

test("Windows facade reads only delivered schema-one facts before Ready", () => {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:true",
      "--define=__KELD_HOST_LAUNCH_FACTS__:" + JSON.stringify(facts),
      "--preload=" + transport,
      "-e",
      "globalThis['__KELD_HOST_LAUNCH_FACTS__'] = { name: 'decoy' };" +
        "const { app } = await import(" +
        JSON.stringify(facade) +
        ");" +
        "const result = { name: app.getName(), property: app.name," +
        " root: app.getAppPath(), packaged: app.isPackaged, ready: app.isReady() };" +
        "for (const [key, read] of [['version',()=>app.getVersion()]," +
        " ['locale',()=>app.getLocale()],['path',()=>app.getPath('userData')]])" +
        " { try { result[key] = read(); } catch(error) { result[key] = error.code; } }" +
        "console.log(JSON.stringify(result));",
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    name: facts.name,
    property: facts.name,
    root: facts.root,
    packaged: false,
    ready: false,
    version: "KELD-COMPAT-040",
    locale: "KELD-COMPAT-040",
    path: "KELD-COMPAT-040",
  });
});

test("explicit Windows absence cannot be filled by application globals", () => {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:true",
      "--define=__KELD_HOST_LAUNCH_FACTS__:undefined",
      "-e",
      "globalThis['__KELD_HOST_LAUNCH_FACTS__'] = " +
        JSON.stringify(facts) +
        ";" +
        "const { app } = await import(" +
        JSON.stringify(facade) +
        ");" +
        "try { console.log(app.getName()); } catch(error) { console.log(error.code); }",
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(child.stdout.toString().trim()).toBe("KELD-COMPAT-040");
});

test("Windows facade consumes delivered schema-three metadata without fallback", () => {
  const input = {
    ...facts,
    schema: 3,
    version: "31.7.0-explicit",
    locale: "fr-CA",
    paths: {
      appData: "D:\\Observed\\Roaming",
      userData: "D:\\Observed\\Roaming\\Exact",
      documents: "\\\\server\\share\\Documents",
    },
  };
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:true",
      "--define=__KELD_HOST_LAUNCH_FACTS__:" + JSON.stringify(input),
      "--preload=" + transport,
      "-e",
      "globalThis['__KELD_HOST_LAUNCH_FACTS__'] = { version: 'decoy', locale: 'decoy' };" +
        "const { app } = await import(" +
        JSON.stringify(facade) +
        ");" +
        "console.log(JSON.stringify({ name: app.getName(), root: app.getAppPath()," +
        " version: app.getVersion(), locale: app.getLocale(), paths: {" +
        " appData: app.getPath('appData'), userData: app.getPath('userData'), documents: app.getPath('documents') } }));",
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    name: facts.name,
    root: facts.root,
    version: input.version,
    locale: input.locale,
    paths: input.paths,
  });
});

test("Windows schema-three optional absence preserves the application's existing catch behavior", () => {
  const input = { ...facts, schema: 3, paths: {} };
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:true",
      "--define=__KELD_HOST_LAUNCH_FACTS__:" + JSON.stringify(input),
      "--preload=" + transport,
      "-e",
      "const { app } = await import(" +
        JSON.stringify(facade) +
        ");" +
        "const result = {}; for (const [key, read] of [['version',()=>app.getVersion()]," +
        " ['locale',()=>app.getLocale()],['userData',()=>app.getPath('userData')]," +
        " ['documents',()=>app.getPath('documents')]])" +
        " { try { result[key] = read(); } catch(error) { result[key] = error.code; } }" +
        "console.log(JSON.stringify(result));",
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    version: "KELD-COMPAT-040",
    locale: "KELD-COMPAT-040",
    userData: "KELD-COMPAT-040",
    documents: "KELD-COMPAT-040",
  });
});
