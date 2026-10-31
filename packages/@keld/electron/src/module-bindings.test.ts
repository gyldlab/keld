/**
 * GH-565 (F06-T8): link-time named bindings of the Electron v44.4.5 main-process modules
 * draw.io's main imports (`src/main/electron.js:5-6` at 2edf9fb9).
 *
 * Corpus `electron-main-bindings-v1` (`crates/keld-compat/fixtures/main-bindings-corpus/`)
 * maps one cell per binding to one exact case name below. Each cell case spawns a Bun
 * child that evaluates a one-line module importing that binding by name from `./index.ts`
 * (an isolated temp file, `file://` specifier), so the oracle is Bun's own ESM linker: a
 * missing export is `SyntaxError: Export named '<name>' not found in module '…'.`, exit 1
 * (Bun 1.4.4, measured 2026-10-31); the deliberately unexported `MenuItem` proves it live.
 * `ipcMain`, the twelfth name of that statement, is the GH-466 registration slice's export
 * (PR #1365, spec D5) and has no case here.
 *
 * The remaining cases hold the spec's other criteria
 * (`docs/specs/gh565-link-time-electron-bindings.md` §3): default-export parity, zero
 * Host traffic while every member is touched (`fixtures/bindings_no_link.ts`), and the
 * typed `KELD-COMPAT-013` refusal of every member with Electron's documented return kind.
 * `ORACLE` below is this file's own copy of the spec §4.3 table, transcribed from the
 * pinned pages' member headings and `Returns Promise<…>` lines and from the owning
 * tickets' `Electron members covered` lines; it is independent of the implementation's
 * table in `unsupported-bindings.ts`.
 */
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import * as electron from "./index";
import { isUnsupportedMemberError, type UnsupportedKind } from "./index";

/** The facade entry Electron code imports, as a specifier any directory can resolve. */
const INDEX_URL = pathToFileURL(join(import.meta.dir, "index.ts")).href;
const fixtures = join(import.meta.dir, "..", "fixtures");

/** Kill switch only: a linked or refused import settles far below this. */
const PROBE_KILL_MS = 10_000;

/** draw.io's import statement, `src/main/electron.js:5-6` at 2edf9fb9, in source order. */
const DRAWIO_IMPORT = [
  "Menu",
  "shell",
  "dialog",
  "session",
  "screen",
  "ClipboardItem",
  "clipboard",
  "nativeImage",
  "nativeTheme",
  "ipcMain",
  "app",
  "BrowserWindow",
] as const;

/**
 * The eleven names of that statement this facade exports. `ipcMain` is the GH-466
 * registration slice's (PR #1365; decision recorded on gyldlab/keld#466, 2026-10-31); the
 * PR that lands second restores the full twelve-name case.
 */
const LINKED_HERE = DRAWIO_IMPORT.filter((name) => name !== "ipcMain");

type MemberKind = "method" | "promise" | "property" | "writableProperty";

interface BindingOracle {
  /** Corpus operation prefix (`<prefix>.binding.named-import`). */
  readonly prefix: string;
  readonly name: (typeof DRAWIO_IMPORT)[number];
  readonly typeof: "object" | "function";
  /** Where the documented members live: the binding itself, or a class prototype. */
  readonly target: "own" | "prototype";
  /** Whether `new Binding({})` constructs (`ClipboardItem`) or refuses naming a ticket. */
  readonly construct?: { readonly refuses: `GH-${number}` | false };
  /** member → [documented kind, owning issue], from the pinned page and the ticket texts. */
  readonly members: Readonly<
    Record<string, readonly [MemberKind, `GH-${number}`]>
  >;
}

const ORACLE: readonly BindingOracle[] = [
  {
    // menu.md: `new Menu()` l.21; statics l.29-71. GH-480 covers buildFromTemplate and
    // setApplicationMenu; GH-556 covers Menu.constructor and getApplicationMenu;
    // sendActionToFirstResponder is "Not yet specified" on #476.
    prefix: "menu",
    name: "Menu",
    typeof: "function",
    target: "own",
    construct: { refuses: "GH-556" },
    members: {
      buildFromTemplate: ["method", "GH-480"],
      setApplicationMenu: ["method", "GH-480"],
      getApplicationMenu: ["method", "GH-556"],
      sendActionToFirstResponder: ["method", "GH-476"],
    },
  },
  {
    // shell.md l.24-111: openPath, openExternal and trashItem return Promise. GH-479 covers
    // the five portable members; the Windows shortcut pair is a GH-482 scoreboard row.
    prefix: "shell",
    name: "shell",
    typeof: "object",
    target: "own",
    members: {
      showItemInFolder: ["method", "GH-479"],
      openPath: ["promise", "GH-479"],
      openExternal: ["promise", "GH-479"],
      trashItem: ["promise", "GH-479"],
      beep: ["method", "GH-479"],
      writeShortcutLink: ["method", "GH-482"],
      readShortcutLink: ["method", "GH-482"],
    },
  },
  {
    // dialog.md l.19-429: the three async dialogs and showCertificateTrustDialog return
    // Promise. GH-478 covers the async trio, GH-564 showMessageBoxSync, GH-561 the rest.
    prefix: "dialog",
    name: "dialog",
    typeof: "object",
    target: "own",
    members: {
      showOpenDialogSync: ["method", "GH-561"],
      showOpenDialog: ["promise", "GH-478"],
      showSaveDialogSync: ["method", "GH-561"],
      showSaveDialog: ["promise", "GH-478"],
      showMessageBoxSync: ["method", "GH-564"],
      showMessageBox: ["promise", "GH-478"],
      showErrorBox: ["method", "GH-561"],
      showCertificateTrustDialog: ["promise", "GH-561"],
    },
  },
  {
    // session.md l.26-67: two factories (GH-474, one Session = the KEL-135 profile) and
    // the defaultSession property (GH-553 owns the object and its webRequest lane rules).
    prefix: "session",
    name: "session",
    typeof: "object",
    target: "own",
    members: {
      fromPartition: ["method", "GH-474"],
      fromPath: ["method", "GH-474"],
      defaultSession: ["property", "GH-553"],
    },
  },
  {
    // screen.md l.73-178: three events and nine methods, none returning Promise. GH-481
    // covers the two draw.io getters; GH-558 covers the rest and the display events.
    prefix: "screen",
    name: "screen",
    typeof: "object",
    target: "own",
    members: {
      getCursorScreenPoint: ["method", "GH-558"],
      getPrimaryDisplay: ["method", "GH-481"],
      getAllDisplays: ["method", "GH-481"],
      getDisplayNearestPoint: ["method", "GH-558"],
      getDisplayMatching: ["method", "GH-558"],
      screenToDipPoint: ["method", "GH-558"],
      dipToScreenPoint: ["method", "GH-558"],
      screenToDipRect: ["method", "GH-558"],
      dipToScreenRect: ["method", "GH-558"],
      on: ["method", "GH-558"],
      once: ["method", "GH-558"],
      off: ["method", "GH-558"],
      addListener: ["method", "GH-558"],
      removeListener: ["method", "GH-558"],
      removeAllListeners: ["method", "GH-558"],
    },
  },
  {
    // clipboard-item.md l.25-163: `new ClipboardItem(items)` takes a MIME-keyed record;
    // `types` is a read-only property and `getType` returns Promise. GH-559 covers both.
    prefix: "clipboard-item",
    name: "ClipboardItem",
    typeof: "function",
    target: "prototype",
    construct: { refuses: false },
    members: {
      types: ["property", "GH-559"],
      getType: ["promise", "GH-559"],
    },
  },
  {
    // clipboard.md l.96-218 (W3C promise shape): every method but clear returns Promise;
    // selection is a Linux read-only property. GH-559 covers draw.io's three members; the
    // rest are "Not yet specified" on #476.
    prefix: "clipboard",
    name: "clipboard",
    typeof: "object",
    target: "own",
    members: {
      readText: ["promise", "GH-559"],
      writeText: ["promise", "GH-559"],
      read: ["promise", "GH-476"],
      write: ["promise", "GH-559"],
      has: ["promise", "GH-476"],
      clear: ["method", "GH-476"],
      selection: ["property", "GH-476"],
    },
  },
  {
    // native-image.md l.131-251: eight module methods; only createThumbnailFromPath returns
    // Promise. GH-559 covers createFromDataURL; the rest are "Not yet specified" on #476.
    prefix: "native-image",
    name: "nativeImage",
    typeof: "object",
    target: "own",
    members: {
      createEmpty: ["method", "GH-476"],
      createThumbnailFromPath: ["promise", "GH-476"],
      createFromPath: ["method", "GH-476"],
      createFromBitmap: ["method", "GH-476"],
      createFromBuffer: ["method", "GH-476"],
      createFromDataURL: ["method", "GH-559"],
      createFromNamedImage: ["method", "GH-476"],
      createMenuSymbol: ["method", "GH-476"],
    },
  },
  {
    // native-theme.md l.11-88: the `updated` event, themeSource (writable) and seven
    // read-only properties. GH-481 covers shouldUseDarkColors; GH-558 covers the rest.
    prefix: "native-theme",
    name: "nativeTheme",
    typeof: "object",
    target: "own",
    members: {
      shouldUseDarkColors: ["property", "GH-481"],
      themeSource: ["writableProperty", "GH-558"],
      shouldUseHighContrastColors: ["property", "GH-558"],
      shouldUseDarkColorsForSystemIntegratedUI: ["property", "GH-558"],
      shouldUseInvertedColorScheme: ["property", "GH-558"],
      inForcedColorsMode: ["property", "GH-558"],
      prefersReducedTransparency: ["property", "GH-558"],
      shouldDifferentiateWithoutColor: ["property", "GH-558"],
      on: ["method", "GH-558"],
      once: ["method", "GH-558"],
      off: ["method", "GH-558"],
      addListener: ["method", "GH-558"],
      removeListener: ["method", "GH-558"],
      removeAllListeners: ["method", "GH-558"],
    },
  },
];

interface ProbeResult {
  code: number | null;
  stdout: string;
  stderr: string;
}

/**
 * Evaluates `import { <names> } from "<index.ts>"` in a fresh Bun process and prints the
 * `typeof` of every binding. The module graph is the child's own, so a missing export is
 * observed as Bun's link-time SyntaxError and never as a cached namespace lookup.
 */
function linkProbe(names: readonly string[]): ProbeResult {
  const dir = mkdtempSync(join(tmpdir(), "gh565-link-"));
  try {
    const probe = join(dir, "probe.ts");
    const typeofs = names.map((name) => `typeof ${name}`).join(", ");
    writeFileSync(
      probe,
      `import { ${names.join(", ")} } from ${JSON.stringify(INDEX_URL)};\n` +
        `console.log(JSON.stringify([${typeofs}]));\n`,
    );
    const child = Bun.spawnSync({
      cmd: [process.execPath, probe],
      cwd: dir,
      env: process.env,
      stdout: "pipe",
      stderr: "pipe",
      timeout: PROBE_KILL_MS,
    });
    return {
      code: child.exitCode,
      stdout: child.stdout.toString(),
      stderr: child.stderr.toString(),
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Bun's exact link failure for one missing named export of the facade entry. */
function exportNotFound(name: string): RegExp {
  return new RegExp(
    `SyntaxError: Export named '${name}' not found in module '[^']*index\\.ts'\\.`,
  );
}

/** `target[name]`, read the way an app reads it (inherited members included). */
function member(target: unknown, name: string): unknown {
  if (
    (typeof target !== "object" && typeof target !== "function") ||
    target === null
  ) {
    return undefined;
  }
  return Reflect.get(target, name);
}

/** The object whose own property names are the binding's documented members. */
function memberTarget(oracle: BindingOracle): object {
  const binding = member(electron, oracle.name) as object;
  return oracle.target === "prototype"
    ? (Reflect.get(binding, "prototype") as object)
    : binding;
}

/** The receiver a member is read from or called on: an instance for prototype members. */
function receiver(oracle: BindingOracle): object {
  const binding = member(electron, oracle.name);
  return oracle.target === "prototype"
    ? (Reflect.construct(binding as new (items: object) => object, [
        {},
      ]) as object)
    : (binding as object);
}

/** Runs `fn` and returns what it threw; failing the case when it returns normally. */
function thrownBy(fn: () => unknown): unknown {
  try {
    fn();
  } catch (error) {
    return error;
  }
  throw new Error("expected a throw, got a normal return");
}

/** Awaits a rejection and returns its reason; failing the case when the Promise settles. */
async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Error("expected a rejection, got a settled Promise");
}

interface ExpectedRefusal {
  kind: UnsupportedKind;
  member: string;
  ticket: `GH-${number}`;
}

/** The whole `KELD-COMPAT-013` contract on one caught value (spec §3 AC5-AC6). */
function expectRefusal(caught: unknown, expected: ExpectedRefusal): void {
  expect(isUnsupportedMemberError(caught)).toBe(true);
  if (!isUnsupportedMemberError(caught)) return;
  expect(caught).toBeInstanceOf(electron.UnsupportedMemberError);
  expect(caught.name).toBe("UnsupportedMemberError");
  expect(caught.code).toBe("KELD-COMPAT-013");
  expect(caught.kind).toBe(expected.kind);
  expect(caught.member).toBe(expected.member);
  expect(caught.ticket).toBe(expected.ticket);
  const subject =
    expected.kind === "constructor"
      ? `new electron.${expected.member}()`
      : expected.kind === "property"
        ? `electron.${expected.member} `
        : `electron.${expected.member}()`;
  expect(caught.message.startsWith(`KELD-COMPAT-013: ${subject}`)).toBe(true);
  expect(caught.message).toContain(
    `tracked on gyldlab/keld#${expected.ticket.slice(3)}.`,
  );
}

describe("link (electron-main-bindings-v1 cells)", () => {
  for (const oracle of ORACLE) {
    const article = oracle.typeof === "object" ? "an object" : "a function";
    test(`${oracle.prefix}.binding.named-import: a Bun child importing { ${oracle.name} } by name from the facade links and evaluates to ${article}, and the default export carries the same binding`, () => {
      const result = linkProbe([oracle.name]);
      expect(result.stderr).toBe("");
      expect(result.code).toBe(0);
      expect(JSON.parse(result.stdout)).toEqual([oracle.typeof]);
      const named = member(electron, oracle.name);
      expect(typeof named).toBe(oracle.typeof);
      expect(member(electron.default, oracle.name)).toBe(named);
    });
  }

  test("the eleven names of draw.io's import statement (electron.js:5-6) that this facade exports link in one module graph; ipcMain is PR #1365's", () => {
    const result = linkProbe(LINKED_HERE);
    expect(result.stderr).toBe("");
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual([
      "function",
      "object",
      "object",
      "object",
      "object",
      "function",
      "object",
      "object",
      "object",
      "object",
      "function",
    ]);
  });

  test("negative control: an unexported name fails the link with Bun's SyntaxError, and MenuItem stays unexported for the GH-480 transition gate", () => {
    const result = linkProbe(["MenuItem"]);
    expect(result.code).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr).toMatch(exportNotFound("MenuItem"));
    expect(member(electron, "MenuItem")).toBeUndefined();
    expect(member(electron.default, "MenuItem")).toBeUndefined();
  });

  test("the default export carries every binding draw.io imports that this facade exports, including app and BrowserWindow", () => {
    for (const name of LINKED_HERE) {
      const named = member(electron, name);
      expect(named).toBeDefined();
      expect(member(electron.default, name)).toBe(named);
    }
  });
});

describe("module evaluation and refusals open no app link", () => {
  test("touching every member of the nine bindings calls LifecycleLink.connect zero times", () => {
    let touched = 0;
    let threw = 0;
    let rejected = 0;
    for (const oracle of ORACLE) {
      for (const [kind] of Object.values(oracle.members)) {
        touched += 1;
        if (kind === "promise") rejected += 1;
        else threw += 1;
      }
      if (oracle.construct) {
        touched += 1;
        if (oracle.construct.refuses !== false) threw += 1;
      }
    }
    const child = Bun.spawnSync({
      cmd: [process.execPath, "./bindings_no_link.ts"],
      cwd: fixtures,
      env: { ...process.env, KELD_APP_LINK: `1#${"ab".repeat(32)}` },
      stdout: "pipe",
      stderr: "pipe",
      timeout: PROBE_KILL_MS,
    });
    expect(child.stderr.toString()).toBe("");
    expect(child.exitCode).toBe(0);
    expect(child.stdout.toString()).toBe(
      `GH565_NO_LINK connect=0 touched=${touched} threw=${threw} rejected=${rejected}\n`,
    );
  });
});

describe("every linked member refuses with KELD-COMPAT-013 (spec §4.3)", () => {
  for (const oracle of ORACLE) {
    for (const [name, [kind, ticket]] of Object.entries(oracle.members)) {
      const path = `${oracle.name}.${name}`;
      switch (kind) {
        case "method":
          test(`${path}() throws the typed refusal naming ${ticket}`, () => {
            const self = receiver(oracle);
            const fn = member(self, name);
            expect(typeof fn).toBe("function");
            expect((fn as { name: string }).name).toBe(name);
            const caught = thrownBy(() =>
              Reflect.apply(fn as (...args: unknown[]) => unknown, self, []),
            );
            expectRefusal(caught, { kind: "method", member: path, ticket });
          });
          break;
        case "promise":
          test(`${path}() returns a Promise that rejects with the typed refusal naming ${ticket}`, async () => {
            const self = receiver(oracle);
            const fn = member(self, name);
            expect(typeof fn).toBe("function");
            const result = Reflect.apply(
              fn as (...args: unknown[]) => unknown,
              self,
              [],
            );
            expect(result).toBeInstanceOf(Promise);
            const caught = await rejectionOf(result as Promise<unknown>);
            expectRefusal(caught, { kind: "promise", member: path, ticket });
          });
          break;
        case "property":
          test(`reading ${path} throws the typed refusal naming ${ticket}`, () => {
            const self = receiver(oracle);
            const caught = thrownBy(() => Reflect.get(self, name));
            expectRefusal(caught, { kind: "property", member: path, ticket });
          });
          break;
        case "writableProperty":
          test(`reading and assigning ${path} both throw the typed refusal naming ${ticket}`, () => {
            const self = receiver(oracle);
            expectRefusal(
              thrownBy(() => Reflect.get(self, name)),
              {
                kind: "property",
                member: path,
                ticket,
              },
            );
            expectRefusal(
              thrownBy(() => Reflect.set(self, name, "dark")),
              {
                kind: "property",
                member: path,
                ticket,
              },
            );
          });
          break;
      }
    }
  }
});

describe("census: each binding exposes exactly the pinned page's members (spec §3 AC7)", () => {
  for (const oracle of ORACLE) {
    test(`${oracle.name} own members equal the §4.3 table and cannot be reassigned`, () => {
      const binding = member(electron, oracle.name) as object;
      const target = memberTarget(oracle);
      const incidental =
        oracle.target === "prototype"
          ? ["constructor"]
          : typeof binding === "function"
            ? ["length", "name", "prototype"]
            : [];
      const names = Object.getOwnPropertyNames(target).filter(
        (name) => !incidental.includes(name),
      );
      expect([...names].sort()).toEqual(Object.keys(oracle.members).sort());
      if (typeof binding !== "function") {
        expect(Object.isFrozen(binding)).toBe(true);
      }
    });
  }
});

describe("constructors", () => {
  test("new Menu() throws the typed refusal naming GH-556", () => {
    const caught = thrownBy(() => new electron.Menu());
    expectRefusal(caught, {
      kind: "constructor",
      member: "Menu",
      ticket: "GH-556",
    });
  });

  test("new ClipboardItem({}) and a MIME-keyed record construct instances whose payload members refuse naming GH-559", async () => {
    const empty = new electron.ClipboardItem({});
    expect(empty).toBeInstanceOf(electron.ClipboardItem);
    const item = new electron.ClipboardItem({
      "text/plain": "hello",
      "text/html": "<b>hello</b>",
    });
    expect(item).toBeInstanceOf(electron.ClipboardItem);
    expectRefusal(
      thrownBy(() => item.types),
      { kind: "property", member: "ClipboardItem.types", ticket: "GH-559" },
    );
    const result = item.getType("text/plain");
    expect(result).toBeInstanceOf(Promise);
    expectRefusal(await rejectionOf(result), {
      kind: "promise",
      member: "ClipboardItem.getType",
      ticket: "GH-559",
    });
  });

  test("new ClipboardItem() without a MIME-keyed record is a TypeError, as Electron's argument check", () => {
    const Loose = electron.ClipboardItem as unknown as new (
      ...args: unknown[]
    ) => unknown;
    expect(() => new Loose()).toThrow(TypeError);
    expect(() => new Loose(null)).toThrow(TypeError);
    expect(() => new Loose("text/plain")).toThrow(TypeError);
  });
});

describe("UnsupportedMemberError classification", () => {
  test("isUnsupportedMemberError accepts only a KELD-COMPAT-013 refusal", () => {
    const refusal = thrownBy(() => electron.shell.beep());
    expect(isUnsupportedMemberError(refusal)).toBe(true);
    expect(
      isUnsupportedMemberError(new Error("KELD-COMPAT-013: not typed")),
    ).toBe(false);
    expect(
      isUnsupportedMemberError(
        Object.assign(new Error("other"), { code: "KELD-COMPAT-012" }),
      ),
    ).toBe(false);
    expect(isUnsupportedMemberError(undefined)).toBe(false);
    expect(electron.UNSUPPORTED_MEMBER_CODE).toBe("KELD-COMPAT-013");
  });
});
