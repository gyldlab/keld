/** Codec/public-shape evidence only; these tests do not simulate native auth success. */
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { decodeU64Varint, decodeVarint, encodeVarint } from "@keld/kipc";
import {
  AUTH_BEGIN_CALL_DEADLINE_MS,
  AUTH_TOKEN_CALL_DEADLINE_MS,
  decodeAuthResponse,
  encodeAuthRequest,
  type AuthRequest,
} from "./auth.generated.ts";
import {
  auth,
  isCallError,
  type AuthBeginRequest,
  type AuthTokenRequest,
  type AuthCallOptions,
} from "./index.ts";
import {
  assertAuthArtifactFresh,
  renderAuthBinding,
  renderFsBinding,
  renderEchoDeclarations,
} from "../../kipc/scripts/echo-codegen.ts";

test("Rust literal AuthRequest vectors and owned snapshots", () => {
  expect(
    Array.from(
      encodeAuthRequest({
        variant: "Begin",
        provider: "entra-common",
        scopes: null,
        login_hint: null,
      }),
    ),
  ).toEqual([
    0, 12, 101, 110, 116, 114, 97, 45, 99, 111, 109, 109, 111, 110, 0, 0,
  ]);
  expect(
    Array.from(
      encodeAuthRequest({
        variant: "Begin",
        provider: "p",
        scopes: ["s"],
        login_hint: "h",
      }),
    ),
  ).toEqual([0, 1, 112, 1, 1, 1, 115, 1, 1, 104]);
  expect(
    Array.from(
      encodeAuthRequest({
        variant: "Token",
        account_id: "a",
        scopes: null,
      }),
    ),
  ).toEqual([1, 1, 97, 0]);
  expect(
    Array.from(
      encodeAuthRequest({
        variant: "Token",
        account_id: "a",
        scopes: [],
      }),
    ),
  ).toEqual([1, 1, 97, 1, 0]);
  const scopes = ["s"];
  const bytes = encodeAuthRequest({
    variant: "Token",
    account_id: "a",
    scopes,
  });
  scopes[0] = "changed";
  expect(Array.from(bytes)).toEqual([1, 1, 97, 1, 1, 1, 115]);
  for (const value of [
    { variant: "Accounts" },
    { variant: "Begin", provider: "p", scopes: null, login_hint: "\ud800" },
    { variant: "Token", account_id: "a", scopes: [7] },
  ]) {
    expect(() => encodeAuthRequest(value as unknown as AuthRequest)).toThrow(
      "KELD-IPC-003",
    );
  }
});

test("Rust literal replies keep u64 expiry exact including beyond Number precision", () => {
  expect(decodeAuthResponse(new Uint8Array([0, 1, 97, 1, 1, 115]))).toEqual({
    variant: "Begin",
    account_id: "a",
    granted_scopes: ["s"],
  });
  const prefix = [1, 1, 116];
  const bearer = [6, 66, 101, 97, 114, 101, 114];
  const vectors: ReadonlyArray<readonly [readonly number[], bigint]> = [
    [[0], 0n],
    [[0xac, 0x02], 300n],
    [[0x80, 0x80, 0x80, 0x80, 0x10], 4_294_967_296n],
    [[0x81, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x10], 9_007_199_254_740_993n],
    [
      [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01],
      18_446_744_073_709_551_615n,
    ],
  ];
  for (const [expiry, exact] of vectors) {
    const bytes = new Uint8Array([...prefix, ...expiry, ...bearer]);
    expect(decodeAuthResponse(bytes)).toEqual({
      variant: "Token",
      access_token: "t",
      expires_at: exact,
      token_type: "Bearer",
    });
    expect(() =>
      decodeAuthResponse(new Uint8Array([...bytes, 1, 114])),
    ).toThrow("KELD-IPC-003");
  }
});

test("shared varint owner preserves u32 and rejects overflowing or truncated u64", () => {
  for (const [bytes, exact] of [
    [[0], 0],
    [[0xac, 2], 300],
    [[0xff, 0xff, 0xff, 0xff, 0x0f], 4_294_967_295],
  ] as const) {
    expect(decodeVarint(new Uint8Array(bytes), 0)).toEqual([
      exact,
      bytes.length,
    ]);
    expect(Array.from(encodeVarint(exact))).toEqual([...bytes]);
  }
  for (const bytes of [
    [],
    [0x80],
    [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2],
    [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0],
  ]) {
    expect(() => decodeU64Varint(new Uint8Array(bytes), 0)).toThrow(
      "KELD-IPC-003",
    );
  }
  // Pinned postcard 1.1.3 admits fitting noncanonical encodings; do not invent stricter wire policy.
  expect(decodeU64Varint(new Uint8Array([0x80, 0]), 0)).toEqual([0n, 2]);
  expect(() =>
    decodeVarint(new Uint8Array([0x80, 0x80, 0x80, 0x80, 0x10]), 0),
  ).toThrow("KELD-IPC-003");
  for (const bytes of [
    [2],
    [0, 1, 97, 0xff, 0xff, 0xff, 0xff, 0x0f],
    [0, 1, 0xff, 0],
    [1, 1, 116, 0x80],
  ]) {
    expect(() => decodeAuthResponse(new Uint8Array(bytes))).toThrow(
      "KELD-IPC-003",
    );
  }
});

test("generator freshness owns DTO/defaults while existing Echo and FS artifacts stay exact", () => {
  const root = resolve(import.meta.dir, "../../../..");
  const source = readFileSync(
    resolve(root, "crates/keld-native/src/auth/mod.rs"),
    "utf8",
  );
  const generated = readFileSync(resolve(import.meta.dir, "auth.generated.ts"));
  expect(() => assertAuthArtifactFresh(source, generated)).not.toThrow();
  expect(() =>
    assertAuthArtifactFresh(
      source,
      Buffer.concat([generated, Buffer.from("// mutation\n")]),
    ),
  ).toThrow("stale");
  expect(AUTH_BEGIN_CALL_DEADLINE_MS).toBe(330_000);
  expect(AUTH_TOKEN_CALL_DEADLINE_MS).toBe(60_000);
  expect(() =>
    renderAuthBinding(source.replace("expires_at: u64,", "expires_at: u32,")),
  ).toThrow("echo codegen:");
  expect(() =>
    renderAuthBinding(
      source.replace("    Token {\n", "    Accounts,\n    Token {\n"),
    ),
  ).toThrow("echo codegen:");
  const changedDeadline = source.replace(
    /(pub const AUTH_BEGIN_CALL_DEADLINE_MS: u32 = )[0-9_]+;/,
    (_line, prefix: string) => prefix + "331_000;",
  );
  expect(changedDeadline).not.toBe(source);
  expect(renderAuthBinding(changedDeadline)).toContain(
    "AUTH_BEGIN_CALL_DEADLINE_MS = 331000;",
  );
  expect(
    renderFsBinding(
      readFileSync(resolve(root, "crates/keld-native/src/fs.rs"), "utf8"),
    ),
  ).toBe(readFileSync(resolve(import.meta.dir, "fs.generated.ts"), "utf8"));
  expect(
    renderEchoDeclarations(
      readFileSync(resolve(root, "crates/keld-ipc/src/echo.rs"), "utf8"),
    ),
  ).toBe(readFileSync(resolve(import.meta.dir, "echo.generated.ts"), "utf8"));
});

test("public auth arguments reject through Promises before a link attempt", async () => {
  for (const call of [
    () => auth.begin(null as unknown as AuthBeginRequest),
    () => auth.begin({ provider: "p", loginHint: "\ud800" }),
    () => auth.token({ accountId: 7 } as unknown as AuthTokenRequest),
    () =>
      auth.token({
        accountId: "a",
        scopes: [7],
      } as unknown as AuthTokenRequest),
  ]) {
    let promise: Promise<unknown> | undefined;
    expect(() => {
      promise = call();
    }).not.toThrow();
    expect(promise).toBeInstanceOf(Promise);
    await expect(promise!).rejects.toThrow("KELD-IPC-003");
  }
  const api = await import("./index.ts");
  expect(Object.keys(api.auth).sort()).toEqual(["begin", "token"]);
  for (const privateName of [
    "invokeAuth",
    "LifecycleLink",
    "encodeAuthRequest",
    "decodeAuthResponse",
  ]) {
    expect(Object.hasOwn(api, privateName)).toBe(false);
  }
});

test("invalid caller options are typed IPC005 before readiness or bootstrap", async () => {
  const oldLink = process.env.KELD_APP_LINK;
  delete process.env.KELD_APP_LINK;
  try {
    // Reaching the unconfigured bootstrap would produce IPC007, not IPC005.
    for (const options of [
      null,
      1,
      "deadline",
      false,
      1n,
      Symbol("deadline"),
      [],
      { deadlineMs: null },
      { deadlineMs: 0 },
      { deadlineMs: -1 },
      { deadlineMs: Number.NaN },
      { deadlineMs: Number.POSITIVE_INFINITY },
      { deadlineMs: "330000" },
      { deadlineMs: 86_400_001 },
    ]) {
      for (const call of [
        () =>
          auth.begin(
            { provider: "entra-common" },
            options as unknown as AuthCallOptions,
          ),
        () =>
          auth.token({ accountId: "a" }, options as unknown as AuthCallOptions),
      ]) {
        let promise: Promise<unknown> | undefined;
        expect(() => {
          promise = call();
        }).not.toThrow();
        expect(promise).toBeInstanceOf(Promise);
        let error: unknown;
        try {
          await promise;
        } catch (caught) {
          error = caught;
        }
        expect(isCallError(error)).toBe(true);
        if (!isCallError(error))
          throw new Error("caller option error is not typed");
        expect(error.code).toBe("KELD-IPC-005");
        expect(error.message).not.toContain("KELD_APP_LINK");
      }
    }
  } finally {
    if (oldLink === undefined) delete process.env.KELD_APP_LINK;
    else process.env.KELD_APP_LINK = oldLink;
  }
});

test("caller option snapshot reads once and only missing or undefined defaults", async () => {
  let reads = 0;
  const options: AuthCallOptions = {
    get deadlineMs(): number {
      reads += 1;
      return 1;
    },
  };
  await expect(auth.begin({ provider: "\ud800" }, options)).rejects.toThrow(
    "KELD-IPC-003",
  );
  expect(reads).toBe(1);
  for (const options of [undefined, {}, { deadlineMs: undefined }]) {
    await expect(auth.begin({ provider: "\ud800" }, options)).rejects.toThrow(
      "KELD-IPC-003",
    );
    await expect(auth.token({ accountId: "\ud800" }, options)).rejects.toThrow(
      "KELD-IPC-003",
    );
  }
});
