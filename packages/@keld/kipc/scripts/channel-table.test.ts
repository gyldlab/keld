/**
 * GH-508 channel table, TypeScript half (spec docs/specs/gh508-kipc-channel-table.md §3).
 *
 * Criterion 7: no numeric channel-id literal in production TypeScript or the macOS
 * bridge's injected scripts outside the generated region. Criterion 8: the region is
 * byte-identical to the generator's render of `channel_table.rs`. Criterion 9: the
 * generator fails closed and names the source line. Criterion 14: the source-text
 * parity expectations are gone. Oracles are literal expected text and seeded
 * single-mutation controls on the real files.
 */
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { relative, resolve } from "node:path";

import {
  CHANNEL_REGION_BEGIN,
  CHANNEL_REGION_END,
  assertChannelTableRegionFresh,
  channelRegionBounds,
  parseChannelTable,
  renderChannelTableRegion,
  replaceChannelTableRegion,
} from "./echo-codegen.ts";

const repositoryRoot = resolve(import.meta.dir, "../../../..");
const tablePath = resolve(repositoryRoot, "crates/keld-ipc/src/channel_table.rs");
const transportPath = resolve(repositoryRoot, "packages/@keld/kipc/src/transport.ts");
const transportTestPath = resolve(repositoryRoot, "packages/@keld/kipc/src/transport.test.ts");
const bridgePath = resolve(repositoryRoot, "crates/keld-wv/src/wkwebview/macos_bridge.rs");
const tableSource = readFileSync(tablePath, "utf8");
const transportSource = readFileSync(transportPath, "utf8");

const expectedRegion = `// @generated-begin channel-table: packages/@keld/kipc/scripts/echo-codegen.ts from
// crates/keld-ipc/src/channel_table.rs. Do not edit by hand; run bun run echo:generate.
/** Reserved \`HELLO\` channel (\`keld_ipc::channel_table::HANDSHAKE_CHANNEL\`). */
export const HANDSHAKE_CHANNEL = 0;
/** Channel \`echo\` (\`keld_ipc::channel_table::ECHO\`). */
export const ECHO_CHANNEL = 1;
/** Channel \`fs\` (\`keld_ipc::channel_table::FS\`). */
export const FS_CHANNEL = 2;
/** Channel \`lifecycle\` (\`keld_ipc::channel_table::LIFECYCLE\`). */
export const LIFECYCLE_CHANNEL = 3;
/** An allocated channel id (\`keld_ipc::channel_table::CHANNEL_TABLE\`). */
export type AllocatedChannel = typeof ECHO_CHANNEL | typeof FS_CHANNEL | typeof LIFECYCLE_CHANNEL;
/** Every allocated channel id, in table order. */
export const ALLOCATED_CHANNELS: readonly AllocatedChannel[] = Object.freeze([ECHO_CHANNEL, FS_CHANNEL, LIFECYCLE_CHANNEL]);
/** Channels whose receive class carries host \`EVENT\`s (\`ReceiveClass::carries_host_events\`). */
export const HOST_EVENT_CHANNELS: readonly AllocatedChannel[] = Object.freeze([LIFECYCLE_CHANNEL]);
// @generated-end channel-table`;

function replaceOnce(text: string, from: string, to: string): string {
  expect(text.split(from).length - 1).toBe(1);
  return text.replace(from, to);
}

// ---------------------------------------------------------------------------
// Criterion 7 scan.
// ---------------------------------------------------------------------------

/**
 * Numeric channel-id literal shapes: a `*channel*` binding assigned a number,
 * a `channel:` property set to a number, an equality comparison between a
 * `*channel*` operand and a number (either side), and a number as the channel
 * (third) argument of the transport's positional `writeFrame` / `encodeHeader`.
 * Ordering comparisons (`channel <= 0`, `channel > 0xffff`) are u16 bounds
 * checks, not ids.
 */
const CHANNEL_LITERAL_PATTERNS: readonly RegExp[] = [
  /\b\w*channel\w*\s*(?::\s*[\w<>|[\]\s]+)?=(?!=)\s*\(?\s*\d/i,
  /\b\w*channel\w*\s*:\s*\(?\s*\d/i,
  /\b\w*channel\w*\s*[!=]==?\s*\(?\s*\d/i,
  /\d\s*\)?\s*[!=]==?\s*[\w.]*channel\w*/i,
  /\b(?:writeFrame|encodeHeader)\(\s*[^,()]+,\s*[^,()]+,\s*\d/,
];

const PRODUCTION_EXTENSIONS = /\.(?:[cm]?ts|[cm]?js)$/;
const TEST_FILE = /\.test\.[cm]?[tj]s$/;

interface Source {
  path: string;
  source: string;
}

function walk(directory: string, into: Source[]): void {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === "node_modules") continue;
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) walk(path, into);
    else if (entry.isFile() && PRODUCTION_EXTENSIONS.test(entry.name) && !TEST_FILE.test(entry.name)) {
      into.push({ path: relative(repositoryRoot, path).replaceAll("\\", "/"), source: readFileSync(path, "utf8") });
    }
  }
}

function sourceRoots(): string[] {
  const roots: string[] = [];
  const packages = resolve(repositoryRoot, "packages");
  for (const entry of readdirSync(packages, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const children = entry.name.startsWith("@")
      ? readdirSync(resolve(packages, entry.name), { withFileTypes: true })
          .filter((child) => child.isDirectory())
          .map((child) => resolve(packages, entry.name, child.name))
      : [resolve(packages, entry.name)];
    roots.push(...children.map((child) => resolve(child, "src")));
  }
  const templates = resolve(repositoryRoot, "crates/keld-cli/templates");
  for (const entry of readdirSync(templates, { withFileTypes: true })) {
    if (entry.isDirectory()) roots.push(resolve(templates, entry.name, "src"));
  }
  return roots.filter((root) => existsSync(root));
}

function productionSources(): Source[] {
  const sources: Source[] = [];
  for (const root of sourceRoots()) walk(root, sources);
  return sources;
}

/** The two injected scripts, extracted from their Rust raw-string constants. */
function bridgeScripts(bridgeSource: string): Source[] {
  return ["PAGE_FACADE_SCRIPT", "ISOLATED_BRIDGE_SCRIPT"].map((name) => {
    const open = `const ${name}: &str = r#"`;
    const parts = bridgeSource.split(open);
    if (parts.length !== 2) throw new Error(`macos_bridge.rs must declare ${name} exactly once`);
    const end = parts[1].indexOf('\n"#;');
    if (end === -1) throw new Error(`${name} raw string is not terminated`);
    return { path: `crates/keld-wv/src/wkwebview/macos_bridge.rs#${name}`, source: parts[1].slice(0, end) };
  });
}

/** Blanks the generated region so only hand-written text is scanned. */
function withoutGeneratedRegion(candidate: Source): Source {
  const hasRegion = candidate.source.includes(CHANNEL_REGION_BEGIN);
  if (!hasRegion) return candidate;
  if (candidate.path !== "packages/@keld/kipc/src/transport.ts") {
    throw new Error(`${candidate.path} carries a channel-table region; only transport.ts may`);
  }
  const { lines, begin, end } = channelRegionBounds(candidate.source);
  return { ...candidate, source: lines.map((line, index) => (index >= begin && index <= end ? "" : line)).join("\n") };
}

function channelLiteralHits(sources: readonly Source[]): string[] {
  const hits: string[] = [];
  for (const candidate of sources.map(withoutGeneratedRegion)) {
    candidate.source.split("\n").forEach((line, index) => {
      if (CHANNEL_LITERAL_PATTERNS.some((pattern) => pattern.test(line))) {
        hits.push(`${candidate.path}:${index + 1}: ${line.trim()}`);
      }
    });
  }
  return hits;
}

function scanInputs(overrides: Record<string, string> = {}, bridgeSource?: string): Source[] {
  const sources = productionSources().map((candidate) =>
    candidate.path in overrides ? { ...candidate, source: overrides[candidate.path] } : candidate,
  );
  return [...sources, ...bridgeScripts(bridgeSource ?? readFileSync(bridgePath, "utf8"))];
}

describe("criterion 7: no hand-written channel id in production TypeScript", () => {
  test("scan input is the production TypeScript and both injected bridge scripts", () => {
    const paths = scanInputs().map((candidate) => candidate.path);
    for (const required of [
      "packages/@keld/kipc/src/transport.ts",
      "packages/@keld/api/src/channels.ts",
      "packages/@keld/electron/src/link.ts",
      "crates/keld-cli/templates/hello/src/kipc.ts",
      "crates/keld-wv/src/wkwebview/macos_bridge.rs#PAGE_FACADE_SCRIPT",
      "crates/keld-wv/src/wkwebview/macos_bridge.rs#ISOLATED_BRIDGE_SCRIPT",
    ]) {
      expect(paths).toContain(required);
    }
    expect(paths.some((path) => TEST_FILE.test(path))).toBe(false);
    for (const script of bridgeScripts(readFileSync(bridgePath, "utf8"))) {
      expect(script.source).toContain("channel");
    }
  });

  test("production TypeScript and the bridge scripts carry no numeric channel id", () => {
    expect(channelLiteralHits(scanInputs())).toEqual([]);
  });

  test("seeded literals outside the region fail the scan", () => {
    const transport = "packages/@keld/kipc/src/transport.ts";
    const movedOut = replaceOnce(
      transportSource,
      "export const ECHO_CHANNEL = 1;\n",
      "",
    ).replace(`${CHANNEL_REGION_END}\n`, `${CHANNEL_REGION_END}\nexport const ECHO_CHANNEL = 1;\n`);
    const movedLine = movedOut.split("\n").indexOf("export const ECHO_CHANNEL = 1;") + 1;
    expect(movedLine).toBeGreaterThan(movedOut.split("\n").indexOf(CHANNEL_REGION_END) + 0);
    expect(channelLiteralHits(scanInputs({ [transport]: movedOut }))).toEqual([
      `${transport}:${movedLine}: export const ECHO_CHANNEL = 1;`,
    ]);

    // The first of the two HELLO policies regains its literal channel 0.
    const helloLiteral = transportSource.replace("channel: HANDSHAKE_CHANNEL,", "channel: 0,");
    expect(helloLiteral).not.toBe(transportSource);
    expect(channelLiteralHits(scanInputs({ [transport]: helloLiteral }))).toHaveLength(1);

    const channels = "packages/@keld/api/src/channels.ts";
    const local = `${readFileSync(resolve(repositoryRoot, channels), "utf8")}\nconst DIALOG_CHANNEL = 4;\n`;
    expect(channelLiteralHits(scanInputs({ [channels]: local }))).toHaveLength(1);

    const bridge = readFileSync(bridgePath, "utf8");
    const pageLiteral = replaceOnce(
      bridge,
      "if (channel !== __KELD_ADMITTED_CHANNEL__) return fail(",
      "if (channel !== 1) return fail(",
    );
    const pageHits = channelLiteralHits(scanInputs({}, pageLiteral));
    expect(pageHits).toHaveLength(1);
    expect(pageHits[0]).toContain("macos_bridge.rs#PAGE_FACADE_SCRIPT:");
    const isolatedLiteral = replaceOnce(bridge, "call.channel !== __KELD_ADMITTED_CHANNEL__", "call.channel !== 1");
    expect(channelLiteralHits(scanInputs({}, isolatedLiteral))).toHaveLength(1);
  });

  test("bounds checks and named constants are not ids", () => {
    const clean: Source[] = [
      { path: "x.ts", source: "if (!Number.isInteger(channel) || channel <= 0 || channel > 0xffff) {}" },
      { path: "y.ts", source: "if (header.channel !== policy.channel && channel === ECHO_CHANNEL) {}" },
      { path: "z.ts", source: "export function f(channel: number): void {}" },
      { path: "w.ts", source: "writes.writeFrame(FrameKind.Hello, 0, HANDSHAKE_CHANNEL, 0, token);" },
    ];
    expect(channelLiteralHits(clean)).toEqual([]);
    for (const seeded of [
      "const fooChannel = 3;",
      "{ alsoChannel: 3 }",
      "if (1 === msg.channel) {}",
      "x.channel == 2",
      "const ECHO_CHANNEL: U16 = 1;",
      "if (header.channel !== (1)) {}",
      "await writes.writeFrame(FrameKind.Hello, 0, 0, 0, token);",
      "const header = encodeHeader(FrameKind.Call, 0, 1, corr, len);",
    ]) {
      expect(channelLiteralHits([{ path: "s.ts", source: seeded }])).toHaveLength(1);
    }
  });
});

// ---------------------------------------------------------------------------
// Criteria 8 and 11: generated drift.
// ---------------------------------------------------------------------------

describe("criterion 8: the region is the render of channel_table.rs", () => {
  test("renders every entry, in table order, deterministically", () => {
    expect(renderChannelTableRegion(tableSource)).toBe(expectedRegion);
    expect(renderChannelTableRegion(tableSource)).toBe(renderChannelTableRegion(tableSource));
    expect(parseChannelTable(tableSource)).toEqual({
      handshake: 0,
      entries: [
        { rustName: "ECHO", name: "echo", id: 1, receiveClass: "HostCall" },
        { rustName: "FS", name: "fs", id: 2, receiveClass: "GuardedCall" },
        { rustName: "LIFECYCLE", name: "lifecycle", id: 3, receiveClass: "HostCallWithEvents" },
      ],
      hostEventClasses: ["HostCallWithEvents"],
    });
  });

  test("committed transport.ts region is fresh", () => {
    expect(() => assertChannelTableRegionFresh(tableSource, transportSource)).not.toThrow();
    expect(transportSource).toContain(`${expectedRegion}\n`);
  });

  test("a hand edit inside the region or a Rust id change without regeneration is stale", () => {
    const handEdited = replaceOnce(transportSource, "export const LIFECYCLE_CHANNEL = 3;", "export const LIFECYCLE_CHANNEL = 4;");
    expect(() => assertChannelTableRegionFresh(tableSource, handEdited)).toThrow(
      "channel table region is stale; run bun run echo:generate",
    );
    const renumbered = replaceOnce(
      tableSource,
      'ChannelEntry::new(\n    "lifecycle",\n    3,\n',
      'ChannelEntry::new(\n    "lifecycle",\n    4,\n',
    );
    expect(() => assertChannelTableRegionFresh(renumbered, transportSource)).toThrow(
      "channel table region is stale; run bun run echo:generate",
    );
    expect(() => assertChannelTableRegionFresh(tableSource, `${transportSource}// extra\n`)).not.toThrow();
  });

  test("criterion 11 control: a fixture table with echo at 4 renders ECHO_CHANNEL = 4", () => {
    const fixture = replaceOnce(tableSource, 'ChannelEntry::new("echo", 1,', 'ChannelEntry::new("echo", 4,');
    expect(renderChannelTableRegion(fixture)).toContain("export const ECHO_CHANNEL = 4;");
    const regenerated = replaceChannelTableRegion(transportSource, renderChannelTableRegion(fixture));
    expect(() => assertChannelTableRegionFresh(fixture, regenerated)).not.toThrow();
    expect(() => assertChannelTableRegionFresh(tableSource, regenerated)).toThrow("stale");
  });

  test("host EVENT channels follow the Rust class rule, never a TypeScript mirror", () => {
    const rule = "        matches!(self, Self::HostCallWithEvents)\n";
    const widened = replaceOnce(tableSource, rule, "        matches!(self, Self::HostCallWithEvents | Self::GuardedCall)\n");
    expect(renderChannelTableRegion(widened)).toContain(
      "export const HOST_EVENT_CHANNELS: readonly AllocatedChannel[] = Object.freeze([FS_CHANNEL, LIFECYCLE_CHANNEL]);",
    );
    const narrowed = replaceOnce(tableSource, rule, "        matches!(self, Self::HostEvent)\n");
    expect(renderChannelTableRegion(narrowed)).toContain("Object.freeze([]);");
    expect(() => assertChannelTableRegionFresh(widened, transportSource)).toThrow("stale");
    const computed = replaceOnce(tableSource, rule, "        self as u8 == 1\n");
    expect(() => renderChannelTableRegion(computed)).toThrow("carries_host_events must be one");
    const missing = replaceOnce(tableSource, "    pub const fn carries_host_events(self) -> bool {", "    pub const fn host_events(self) -> bool {");
    expect(() => renderChannelTableRegion(missing)).toThrow("missing ReceiveClass::carries_host_events");
  });

  test("a missing, duplicated or inverted marker fails", () => {
    const withoutBegin = replaceOnce(transportSource, `${CHANNEL_REGION_BEGIN}:`, "// begin:");
    const withoutEnd = replaceOnce(transportSource, CHANNEL_REGION_END, "// end");
    const duplicated = `${transportSource}\n${expectedRegion}\n`;
    const inverted = `${CHANNEL_REGION_END}\n${withoutEnd.replace("// end", "")}`;
    for (const broken of [withoutBegin, withoutEnd, duplicated, inverted]) {
      expect(() => assertChannelTableRegionFresh(tableSource, broken)).toThrow("exactly one channel-table region");
    }
  });
});

// ---------------------------------------------------------------------------
// Criterion 9: the generator fails closed.
// ---------------------------------------------------------------------------

describe("criterion 9: the generator fails closed and names the source line", () => {
  const lineOf = (text: string, needle: string): number => text.slice(0, text.indexOf(needle)).split("\n").length;

  test("an id written as an expression or non-decimal literal names its line", () => {
    for (const id of ["1 + 0", "0x1", "1_u16", "ECHO_ID", "01", "70000"]) {
      const mutated = replaceOnce(tableSource, 'ChannelEntry::new("echo", 1,', `ChannelEntry::new("echo", ${id},`);
      expect(() => renderChannelTableRegion(mutated)).toThrow(
        `channel_table.rs source line ${lineOf(mutated, 'ChannelEntry::new("echo"')}: channel id must be a decimal u16 literal, found ${id}`,
      );
    }
  });

  test("unsupported entry shapes are refused, never guessed", () => {
    const refused: [string, string, string][] = [
      ["computed name", 'ChannelEntry::new("echo", 1,', "ChannelEntry::new(ECHO_NAME, 1,"],
      ["uppercase name", 'ChannelEntry::new("echo", 1,', 'ChannelEntry::new("Echo", 1,'],
      ["literal capability", "Authority::Guarded(&[FS_READ, FS_WRITE])", 'Authority::Guarded(&["fs.read"])'],
      ["unknown authority form", "    Authority::HostInternal,\n);", "    Authority::HostInternal.clone(),\n);"],
      ["non-call initializer", "pub const ECHO: ChannelEntry =\n    ChannelEntry::new(", "pub const ECHO: ChannelEntry = make(\n    ChannelEntry::new("],
      ["private entry", "pub const ECHO: ChannelEntry =", "const ECHO: ChannelEntry ="],
      ["three arguments", 'ChannelEntry::new("echo", 1, ReceiveClass::HostCall, Authority::HostInternal)', 'ChannelEntry::new("echo", 1, ReceiveClass::HostCall)'],
      ["unindented argument", 'ChannelEntry::new(\n    "fs",\n', 'ChannelEntry::new(\n"fs",\n'],
      ["handshake expression", "= ChannelId(0);", "= ChannelId(1 - 1);"],
    ];
    for (const [label, from, to] of refused) {
      const mutated = replaceOnce(tableSource, from, to);
      expect(() => renderChannelTableRegion(mutated), label).toThrow(/echo codegen: .*channel_table\.rs/);
    }
  });

  test("CHANNEL_TABLE must list every entry constant exactly once", () => {
    const list = "pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO, FS, LIFECYCLE];";
    for (const [label, replacement, message] of [
      ["unlisted entry", "pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO, FS];", "LIFECYCLE is not listed"],
      ["unknown name", "pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO, FS, LIFECYCLE, PROBE];", "PROBE, which is not a parsed"],
      ["listed twice", "pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO, FS, FS, LIFECYCLE];", "FS more than once"],
      ["computed list", "pub const CHANNEL_TABLE: &[ChannelEntry] = ENTRIES;", "unsupported CHANNEL_TABLE"],
      ["missing list", "", "missing CHANNEL_TABLE"],
    ] as const) {
      expect(() => renderChannelTableRegion(replaceOnce(tableSource, list, replacement)), label).toThrow(message);
    }
    const wrapped = replaceOnce(tableSource, list, "pub const CHANNEL_TABLE: &[ChannelEntry] = &[\n    ECHO,\n    FS,\n    LIFECYCLE,\n];");
    expect(renderChannelTableRegion(wrapped)).toBe(expectedRegion);
  });

  test("duplicate names and generated-constant collisions fail", () => {
    const duplicateName = replaceOnce(tableSource, 'ChannelEntry::new(\n    "fs",\n', 'ChannelEntry::new(\n    "echo",\n');
    expect(() => renderChannelTableRegion(duplicateName)).toThrow("duplicate channel entry name echo");
    const collision = replaceOnce(tableSource, 'ChannelEntry::new(\n    "fs",\n', 'ChannelEntry::new(\n    "handshake",\n');
    expect(() => renderChannelTableRegion(collision)).toThrow("duplicate HANDSHAKE_CHANNEL");
    const duplicateConstant = `${tableSource.slice(0, tableSource.indexOf("#[cfg(test)]"))}pub const ECHO: ChannelEntry =\n    ChannelEntry::new("echo", 1, ReceiveClass::HostCall, Authority::HostInternal);\n`;
    expect(() => renderChannelTableRegion(duplicateConstant)).toThrow("duplicate entry constant ECHO");
  });

  test("nothing can hide an entry from the parser", () => {
    const probe =
      'pub const PROBE: ChannelEntry =\n    ChannelEntry::new("probe", 4, ReceiveClass::HostCall, Authority::HostInternal);\n';
    const testGate = "#[cfg(test)]\nmod tests {";
    const hidden: [string, string, RegExp][] = [
      ["entry after the test module", `${tableSource}${probe}`, /nothing may follow the test module/],
      [
        "second cfg gate",
        replaceOnce(tableSource, "pub const CHANNEL_TABLE", "#[cfg(test)]\nuse crate::frame as _;\n\npub const CHANNEL_TABLE"),
        /only one `#\[cfg\(test\)\]`/,
      ],
      ["platform gate", replaceOnce(tableSource, "pub const LIFECYCLE", "#[cfg(windows)]\npub const LIFECYCLE"), /only one/],
      [
        "block comment",
        replaceOnce(tableSource, "/// Every allocated channel", "/* pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO]; */\n/// Every allocated channel"),
        /block comments are not admitted/,
      ],
      ["gate without the test module", replaceOnce(tableSource, testGate, "#[cfg(test)]\nmod fixtures {"), /must open `mod tests \{`/],
    ];
    for (const [label, source, message] of hidden) {
      expect(() => renderChannelTableRegion(source), label).toThrow(message);
    }
  });

  test("fixtures after the test-module gate never reach the generator", () => {
    expect(tableSource).toContain('#[cfg(test)]\nmod tests {');
    const testHalf = tableSource.slice(tableSource.indexOf("#[cfg(test)]"));
    expect(testHalf).toContain("ChannelEntry::new(");
    expect(() => renderChannelTableRegion(tableSource)).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// Criterion 14: the source-text parity test is retired.
// ---------------------------------------------------------------------------

describe("criterion 14: no source-text channel parity remains", () => {
  test("transport.test.ts no longer pins channel ids by Rust source text", () => {
    const transportTest = readFileSync(transportTestPath, "utf8");
    expect(transportTest).not.toMatch(/toContain\(\s*["'`]ChannelId\(/);
    expect(transportTest).not.toContain("crates/keld-ipc/src/echo.rs");
    expect(transportTest).not.toContain("crates/keld-ipc/src/lifecycle.rs");
  });
});
