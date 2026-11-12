/** Private draft. TypeScript is the repository's pinned compiler, not app input.
 * Caller first admits names through Pack/Windows namespace owners and captures
 * immutable bytes under retained handles. This is syntax/closure validation,
 * not an OS sandbox or proof that arbitrary JavaScript cannot generate code.
 */
import * as ts from "typescript/unstable/ast";
import { API } from "typescript/unstable/async";
import { createVirtualFileSystem } from "typescript/unstable/fs";

export interface ParserProcess {
  /** Exact qualified tsgo executable, retained by the outer Job owner. */
  tsserverPath: string;
  /** Absolute normalized Windows virtual root, no actual disk reads allowed. */
  virtualRoot: string;
}

const TRANSPORT = "kipc-transport.js";
function reservedTransport(name: string): boolean {
  return ["kipc-transport.ts", "kipc-transport.js"].includes(
    name.split("/").at(-1)!.toLowerCase(),
  );
}
function transportAlias(entry: string, target: string): boolean {
  const parent = entry.includes("/")
    ? entry.slice(0, entry.lastIndexOf("/") + 1)
    : "";
  return (
    (entry.endsWith(".ts") && target === `${parent}kipc-transport.ts`) ||
    target === `${parent}kipc-transport.js`
  );
}
const MAX_FILES = 4096;
const MAX_FILE = 16 * 1024 * 1024;
const MAX_TOTAL = 256 * 1024 * 1024;

function relativeTarget(importer: string, specifier: string): string {
  if (!specifier.startsWith("./") && !specifier.startsWith("../")) {
    throw new Error("Only explicit relative ESM imports are supported");
  }
  const parts = importer.split("/").slice(0, -1);
  for (const part of specifier.split("/")) {
    if (part === ".") continue;
    if (part === "..") {
      if (!parts.length)
        throw new Error("Import escapes captured application root");
      parts.pop();
    } else {
      // This resolves already-admitted names, not Windows filename admission.
      if (!part || part.includes("\\") || part.includes(":"))
        throw new Error("Unsupported import spelling");
      parts.push(part);
    }
  }
  return parts.join("/");
}

// Resolution is limited to names already admitted by the native inventory.
// Extensionless imports may select one unique .ts or .js member; they never
// trigger Bun/TypeScript filesystem lookup or an implicit directory index.
function resolveInventoryMember(
  target: string,
  files: ReadonlyMap<string, string>,
): string {
  if (files.has(target)) return target;
  if (target.endsWith(".ts") || target.endsWith(".js"))
    throw new Error("Import is absent from captured inventory");
  const candidates = [`${target}.ts`, `${target}.js`].filter((name) =>
    files.has(name),
  );
  if (candidates.length !== 1)
    throw new Error("Import is absent from captured inventory");
  return candidates[0]!;
}

export interface ClosedApp {
  /** Defensive copies; callers cannot mutate input bytes after admission. */
  readonly files: ReadonlyMap<string, string>;
  readonly edges: ReadonlyMap<string, readonly string[]>;
}

// One standalone request per existing captured Job. Never reset after success or
// refusal: repeated native API sessions in one Bun process are unqualified.
let admissionStarted = false;
export async function admitClosedApp(
  inputs: ReadonlyMap<string, Uint8Array>,
  parser: ParserProcess,
  entry: string,
): Promise<ClosedApp> {
  if (admissionStarted)
    throw new Error(
      "Compiler runner admits exactly one request; start a new owned invocation",
    );
  admissionStarted = true;
  if (
    inputs.size === 0 ||
    inputs.size > MAX_FILES ||
    !inputs.has(entry) ||
    [...inputs.keys()].some(reservedTransport)
  ) {
    throw new Error(
      "Closed app requires its declared captured entry and reserves generated transport",
    );
  }
  const files = new Map<string, string>();
  let total = 0;
  for (const [name, bytes] of inputs) {
    // Canonical Windows admission remains upstream; these are compiler-cell checks.
    if (
      !(name.endsWith(".js") || name.endsWith(".ts")) ||
      name.startsWith("/") ||
      name.split("/").some((p) => !p || p === "." || p === "..") ||
      name.includes("\\") ||
      name.includes(":")
    ) {
      throw new Error("Expected admitted relative .js/.ts member name");
    }
    total += bytes.byteLength;
    if (bytes.byteLength > MAX_FILE || total > MAX_TOTAL)
      throw new Error("Captured app exceeds supported bounds");
    files.set(name, new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  }
  if (
    !/^[A-Za-z]:[\\/]/.test(parser.tsserverPath) ||
    !/^[A-Za-z]:\//.test(parser.virtualRoot) ||
    parser.virtualRoot.endsWith("/")
  ) {
    throw new Error(
      "Explicit qualified native parser and Windows virtual root required",
    );
  }
  const root = parser.virtualRoot;
  const configPath = `${root}/tsconfig.json`;
  const virtualFiles: Record<string, string> = Object.create(null);
  for (const [name, text] of files) virtualFiles[`${root}/${name}`] = text;
  virtualFiles[configPath] = JSON.stringify({
    compilerOptions: {
      allowJs: true,
      noLib: true,
      noResolve: true,
      noEmit: true,
      target: "esnext",
      module: "esnext",
      types: [],
    },
    files: [...files.keys()],
  });
  const virtual = createVirtualFileSystem(virtualFiles);
  // Official helper returns undefined for missing readFile/entries, which means
  // real-FS fallback. Override every callback with explicit deny-default values.
  // Spawn-mode async API uses this exact child's stdio, not named-pipe rendezvous.
  // close() closes stdin but does not await exit; outer Job retirement is mandatory.
  const api = new API({
    tsserverPath: parser.tsserverPath,
    cwd: root,
    fs: {
      readFile: (path) => virtual.readFile!(path) ?? null,
      fileExists: (path) => virtual.fileExists!(path) ?? false,
      directoryExists: (path) => virtual.directoryExists!(path) ?? false,
      getAccessibleEntries: (path) =>
        virtual.getAccessibleEntries!(path) ?? { files: [], directories: [] },
      realpath: (path) => path,
    },
  });
  try {
    const snapshot = await api.updateSnapshot({ openProjects: [configPath] });
    try {
      const project = snapshot.getProject(configPath);
      if (!project) throw new Error("Captured parser project missing");
      const program = project.program;
      if (
        (await program.getConfigFileParsingDiagnostics()).length ||
        (await program.getProgramDiagnostics()).length
      ) {
        throw new Error("Captured parser configuration refused");
      }
      const edges = new Map<string, readonly string[]>();
      for (const [name] of files) {
        const filePath = `${root}/${name}`;
        const source = await program.getSourceFile(filePath);
        if (!source || source.text !== files.get(name))
          throw new Error("Parser did not return captured source bytes");
        if ((await program.getSyntacticDiagnostics(filePath)).length)
          throw new Error("Malformed captured source");
        const imports: string[] = [];
        function visit(node: ts.Node): void {
          if (
            node.kind === ts.SyntaxKind.ImportKeyword ||
            ts.isImportEqualsDeclaration(node) ||
            ts.isExportAssignment(node)
          ) {
            throw new Error(
              "Dynamic imports and CommonJS forms are outside this cell",
            );
          }
          // Conservative support restriction, not a security proof against aliases.
          if (
            ts.isIdentifier(node) &&
            ["require", "eval", "Function"].includes(node.text)
          ) {
            throw new Error("Runtime code loading is outside this cell");
          }
          if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
            if (node.attributes)
              throw new Error("Import attributes are outside this cell");
            const specifier = node.moduleSpecifier;
            if (specifier) {
              if (!ts.isStringLiteral(specifier))
                throw new Error("Static string import required");
              const raw = relativeTarget(name, specifier.text);
              const target = transportAlias(entry, raw)
                ? raw
                : resolveInventoryMember(raw, files);
              imports.push(target);
            }
          }
          node.forEachChild(visit);
        }
        visit(source);
        edges.set(name, Object.freeze(imports));
      }
      return { files, edges };
    } finally {
      await snapshot.dispose();
    }
  } finally {
    await api.close();
  }
}

/** Optional compilation adapter. Every app resolution/load is intercepted.
 * Transport stays a separate runtime sidecar; its release/stamp owner is not
 * invoked or duplicated here. No caller-supplied plugin or disk fallback.
 */
export async function bundleCapturedApp(
  inputs: ReadonlyMap<string, Uint8Array>,
  parser: ParserProcess,
  entry: string,
): Promise<Uint8Array> {
  const admitted = await admitClosedApp(inputs, parser, entry);
  const result = await Bun.build({
    // Existing owned runner cwd, not inferred nonexistent virtual entry parent.
    root: process.cwd(),
    entrypoints: [entry],
    target: "bun",
    format: "esm",
    plugins: [
      {
        name: "keld-captured-app",
        setup(build) {
          const resolveCaptured = (args: {
            kind: string;
            path: string;
            importer: string;
          }) => {
            const raw =
              args.kind === "entry-point-build"
                ? args.path
                : relativeTarget(args.importer, args.path);
            if (transportAlias(entry, raw))
              return { path: `./${TRANSPORT}`, external: true };
            const target = resolveInventoryMember(raw, admitted.files);
            return { path: target, namespace: "keld-captured" };
          };
          build.onResolve({ filter: /.*/, namespace: "file" }, resolveCaptured);
          build.onResolve(
            { filter: /.*/, namespace: "keld-captured" },
            resolveCaptured,
          );
          build.onLoad({ filter: /.*/, namespace: "keld-captured" }, (args) => {
            const contents = admitted.files.get(args.path);
            if (contents === undefined)
              throw new Error("Compiler requested uncaptured bytes");
            return {
              contents,
              loader: args.path.endsWith(".ts") ? "ts" : "js",
            };
          });
        },
      },
    ],
  });
  if (!result.success || result.outputs.length !== 1)
    throw new Error("Expected one closed app output");
  if (result.outputs[0].size > MAX_TOTAL)
    throw new Error("Compiled app exceeds supported bounds");
  return new Uint8Array(await result.outputs[0].arrayBuffer());
}
