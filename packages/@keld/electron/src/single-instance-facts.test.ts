/** GH453 proposed no-argument subset: parser/facade evidence, never OS lock proof. */
import { expect, test } from "bun:test";
import { resolve } from "node:path";

const transport = resolve(import.meta.dir, "../../kipc/src/transport.ts");
const facade = resolve(import.meta.dir, "app.ts");
const base = {
  schema: 4,
  kind: "development",
  name: "independent display name",
  root: "/stage",
  paths: {},
};

function observe(setup: string, body: string) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "-e",
      `${setup}
       const transportPath = ${JSON.stringify(transport)};
       const facadePath = ${JSON.stringify(facade)};
       ${body}`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4_000,
  });
}

function decoded(child: ReturnType<typeof observe>) {
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  return JSON.parse(child.stdout.toString());
}

test("GH453 captured acquired and contended data produce distinct synchronous no-argument verdicts", () => {
  for (const [verdict, expected] of [
    ["acquired", true],
    ["contended", false],
  ] as const) {
    const child = observe(
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = ${JSON.stringify({ ...base, singleInstance: { verdict } })};`,
      `const { app } = await import(facadePath);
       console.log(JSON.stringify({ ready: app.isReady(), noarg: app.requestSingleInstanceLock(), explicitUndefined: app.requestSingleInstanceLock(undefined) }));`,
    );
    expect(decoded(child)).toEqual({
      ready: false,
      noarg: expected,
      explicitUndefined: expected,
    });
  }
});

test("GH453 missing declaration and unavailable legacy or absent carrier remain distinct typed failures", () => {
  for (const [value, reason] of [
    [base, "undeclared"],
    [{ ...base, schema: 3 }, "unavailable"],
    [{ ...base, schema: 2 }, "unavailable"],
    [
      { schema: 1, kind: "development", name: "n", root: "/stage" },
      "unavailable",
    ],
    [undefined, "unavailable"],
  ] as const) {
    const source = value === undefined ? "undefined" : JSON.stringify(value);
    const child = observe(
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = ${source};`,
      `const { app } = await import(facadePath);
       try { console.log(JSON.stringify({ returned: app.requestSingleInstanceLock() })); }
       catch (error) { console.log(JSON.stringify({ name: error.name, code: error.code, fact: error.fact, reason: error.reason, namedCall: error.message.includes("app.requestSingleInstanceLock()"), ticket: error.message.includes("GH-453"), fix: error.message.includes("singleInstance") })); }`,
    );
    expect(decoded(child)).toEqual({
      name: "AppSessionFactError",
      code: "KELD-COMPAT-040",
      fact: "singleInstance",
      reason,
      namedCall: true,
      ticket: true,
      fix: true,
    });
  }
});

test("GH453 additionalData and excess arguments refuse before reading a captured verdict", () => {
  for (const supplied of [
    "undefined",
    JSON.stringify({ ...base, singleInstance: { verdict: "acquired" } }),
  ]) {
    const child = observe(
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = ${supplied};`,
      `const { app } = await import(facadePath);
       const results = [];
       for (const args of [[null], [false], [0], [""], [{}], [undefined, undefined]]) {
         try { results.push({ returned: app.requestSingleInstanceLock(...args) }); }
         catch (error) { results.push({ code: error.code, ticket: error.ticket, kind: error.kind }); }
       }
       console.log(JSON.stringify(results));`,
    );
    expect(decoded(child)).toEqual(
      Array.from({ length: 6 }, () => ({
        code: "KELD-COMPAT-013",
        ticket: "GH-453",
        kind: "method",
      })),
    );
  }
});

test("GH453 private schema and nested verdict are closed before facade evaluation", () => {
  const malformed = [
    ...[1, 2, 3].map((schema) => ({
      ...base,
      schema,
      singleInstance: { verdict: "acquired" },
    })),
    { ...base, schema: 5, singleInstance: { verdict: "acquired" } },
    { ...base, paths: undefined, singleInstance: { verdict: "acquired" } },
    ...[
      null,
      [],
      {},
      true,
      "acquired",
      { verdict: null },
      { verdict: true },
      { verdict: "ACQUIRED" },
      { verdict: "other" },
      { verdict: "acquired", id: "org.app" },
    ].map((singleInstance) => ({ ...base, singleInstance })),
  ];
  for (const value of malformed) {
    const child = observe(
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = ${JSON.stringify(value)};`,
      `let code;
       try { await import(transportPath); } catch (error) { code = error.code; }
       console.log(JSON.stringify({ code }));`,
    );
    expect(decoded(child)).toEqual({ code: "KELD-IPC-029" });
  }
});

test("GH453 own getters proxies and inherited-only verdicts never provide a fact or invoke user code", () => {
  for (const nested of [
    `{ get verdict() { reads++; return "acquired"; } }`,
    `new Proxy({ verdict: "acquired" }, { get() { reads++; return "acquired"; } })`,
    `Object.create({ verdict: "acquired" })`,
  ]) {
    const child = observe(
      `let reads = 0;
       const source = ${JSON.stringify(base)};
       source.singleInstance = ${nested};
       globalThis["__KELD_HOST_LAUNCH_FACTS__"] = source;`,
      `let code;
       try { await import(transportPath); } catch (error) { code = error.code; }
       console.log(JSON.stringify({ code, reads }));`,
    );
    expect(decoded(child)).toEqual({ code: "KELD-IPC-029", reads: 0 });
  }
  const own = observe(
    `let reads = 0;
     const source = ${JSON.stringify(base)};
     Object.defineProperty(source, "singleInstance", { get() { reads++; return { verdict: "acquired" }; } });
     globalThis["__KELD_HOST_LAUNCH_FACTS__"] = source;`,
    `let code;
     try { await import(transportPath); } catch (error) { code = error.code; }
     console.log(JSON.stringify({ code, reads }));`,
  );
  expect(decoded(own)).toEqual({ code: "KELD-IPC-029", reads: 0 });
});

test("GH453 captured verdict owns a frozen null-prototype copy and absence cannot be forged later", () => {
  for (const present of [false, true]) {
    const source = {
      ...base,
      ...(present ? { singleInstance: { verdict: "contended" } } : {}),
    };
    const child = observe(
      `const source = ${JSON.stringify(source)};
       globalThis["__KELD_HOST_LAUNCH_FACTS__"] = source;`,
      `const { hostLaunchFacts } = await import(transportPath);
       if (source.singleInstance) source.singleInstance.verdict = "acquired";
       else source.singleInstance = { verdict: "acquired" };
       Object.prototype.singleInstance = { verdict: "acquired" };
       Object.prototype.verdict = "acquired";
       const nested = hostLaunchFacts.singleInstance;
       const mutation = nested === undefined ? null : Reflect.set(nested, "verdict", "acquired");
       const { app } = await import(facadePath);
       let result;
       try { result = app.requestSingleInstanceLock(); } catch (error) { result = error.reason; }
       console.log(JSON.stringify({ outerFrozen: Object.isFrozen(hostLaunchFacts), outerNull: Object.getPrototypeOf(hostLaunchFacts) === null, nestedFrozen: nested === undefined ? null : Object.isFrozen(nested), nestedNull: nested === undefined ? null : Object.getPrototypeOf(nested) === null, mutation, result }));`,
    );
    expect(decoded(child)).toEqual({
      outerFrozen: true,
      outerNull: true,
      nestedFrozen: present ? true : null,
      nestedNull: present ? true : null,
      mutation: present ? false : null,
      result: present ? false : "undeclared",
    });
  }
});

test("GH453 schema5 resource data preserves actual captured acquired and contended verdicts", () => {
  for (const [verdict, expected] of [
    ["acquired", true],
    ["contended", false],
  ] as const) {
    const value = {
      ...base,
      schema: 5,
      singleInstance: { verdict },
      originalResourcePolicy: {
        contract: "gh473-original-editor-offline/v1",
        graphSha256: `sha256:${"1".repeat(64)}`,
        policySha256: `sha256:${"2".repeat(64)}`,
        windowKey: "editor",
        selector: "original-editor-offline",
      },
    };
    const child = observe(
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = ${JSON.stringify(value)};`,
      `const { app } = await import(facadePath); console.log(JSON.stringify({ready:app.isReady(),verdict:app.requestSingleInstanceLock()}));`,
    );
    expect(decoded(child)).toEqual({ ready: false, verdict: expected });
  }
});
