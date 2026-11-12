// GH1404: source-only, unapproved draw.io migration review producer.
// Never launches installers, Bun, KELD, or native applications.
import { createHash } from "node:crypto";
import { existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve, join } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

export const pins = Object.freeze({
  "LICENSE": ["3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986", "f288702d2fa16d3cdf0035b15a9fcbc552cd88e7"],
  "package.json": ["c3612ea5534ab3a2be739b2ae40752d13cf497ee401af84e72a0c8c18ac7d077", "95a7e931fc43abc9b42dd3ef0007a1b9c1296c7f"],
  "package-lock.json": ["b99f2d8b1a166f84495252b4d0cb82da02ec4aa57aef005e50f6ff81cf669f26", "aab2e8562ea5fd1b46e6a9cbc2a432feb78de550"],
  "src/main/args.js": ["fe92324a1c43279ee1c6a7794a925a7fd4749a2b5e64da5ace491835f33ba838", "e5ab92e4468fc80d888f953945b60a24e20c8b06"],
  "src/main/electron-preload.js": ["1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98", "44f2f22f159a8a845ff7e573dd10b816383703ec"],
  "src/main/electron.js": ["55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d", "b57b5e2047710e829bfc25ab09f32b09ad2114d7"]
});
const pinnedAdapterSha = "b5ea68c99292bf42ca476e7348e5e05a5010a563324c65b29533bc5e64868085";
export const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const gitBlob = (bytes) => createHash("sha1").update(Buffer.from("blob " + bytes.length + "\0")).update(bytes).digest("hex");
export function assertPinned(name, bytes) {
  const pin = pins[name];
  if (!pin || sha256(bytes) !== pin[0] || gitBlob(bytes) !== pin[1]) throw new Error("GH1404 SOURCE PIN MISMATCH: " + name);
}
function readPinned(root, name) {
  const full = resolve(root, name);
  let current = full;
  while (current !== root && current !== dirname(current)) {
    if (lstatSync(current).isSymbolicLink()) throw new Error("GH1404 symlink refused: " + name);
    current = dirname(current);
  }
  if (current !== root || !lstatSync(full).isFile()) throw new Error("GH1404 non-regular source refused: " + name);
  const bytes = readFileSync(full);
  assertPinned(name, bytes);
  return bytes;
}
function newFile(path, bytes) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes, { flag: "wx" });
}
export async function produceReview(rootInput, adapterInput, outputInput) {
  if (!rootInput || !adapterInput || !outputInput) throw new Error("Usage: node source-review.mjs PRISTINE_ROOT EXACT_1506_ADAPTER OUTPUT_NEW_PATH");
  const root = resolve(rootInput), output = resolve(outputInput), adapter = resolve(adapterInput);
  if (existsSync(output)) throw new Error("GH1404 refuse existing output directory");
  const source = Object.fromEntries(Object.keys(pins).map((name) => [name, readPinned(root, name)]));
  if (!lstatSync(adapter).isFile() || sha256(readFileSync(adapter)) !== pinnedAdapterSha) throw new Error("GH1404 exact #1506 adapter pin mismatch");
  const module = await import(pathToFileURL(adapter).href);
  const patched = module.patchOriginalMainText(source["src/main/electron.js"].toString("utf8"));
  for (const marker of ["ipcMain.on('isModified-result'", "mainWindow.webContents.send('saveAndClose'", "app.on(\"open-file\"", "app.on('window-all-closed'", "ipcMain.on(\"rendererReq\"", "blessPath(a)", "win.webContents.send('args-obj'"]) {
    if (!patched.includes(marker)) throw new Error("GH1404 original lifecycle marker changed: " + marker);
  }
  if (!patched.includes("let argv = originalParserArgv(") || patched.includes("argv.unshift(null)") || patched.includes("process.argv =")) throw new Error("GH1404 unsafe argv patch");
  newFile(join(output, "src/main/electron.js"), patched);
  newFile(join(output, "src/main/electron-preload.js"), source["src/main/electron-preload.js"]);
  newFile(join(output, "src/main/args.js"), source["src/main/args.js"]);
  newFile(join(output, "package.json.UPSTREAM_NOT_MIGRATED"), source["package.json"]);
  newFile(join(output, "package-lock.json.UPSTREAM_NOT_MIGRATED"), source["package-lock.json"]);
  newFile(join(output, "LICENSE.UPSTREAM_NOT_DISTRIBUTION_APPROVAL"), source.LICENSE);
  newFile(join(output, "keld.permissions.MISSING_OWNER_APPROVAL.txt"), "No owner-selected Legacy Guard/CSP policy exists. No permissions were granted, and this is NOT an installable JSONC policy.\n");
  newFile(join(output, "DO-NOT-BUILD-OR-INSTALL.txt"), "GH1404 UNAPPROVED incomplete original source proposal. No current provider archives, full webapp, migrated lock, permission policy, CLI/Host pairing or independent license approval. DO NOT BUILD OR INSTALL.\n");
  const receipt = {
    schema: "gh1404-worker17-source-only-review/v1",
    status: "UNAPPROVED_INCOMPLETE_NOT_INSTALLABLE",
    upstream_desktop_commit: "2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e",
    upstream_webapp_gitlink: "24b76c2cbd55d88e354042e8d329a2e4708972bc",
    parser_proposal_pr: 1506,
    parser_proposal_commit: "1ff2c9f68f22e4cf0c8ef9ec7dde046f63cf69fa",
    parser_adapter_sha256: pinnedAdapterSha,
    proposed_main_sha256: sha256(Buffer.from(patched)),
    pristine_source_hashes: pins,
    approved_migrated_package_and_solver_lock: null,
    approved_permissions_policy: null,
    approved_provider_archives_and_unpacked_trees: null,
    approved_host_bun_cli_graph: null,
    qualified_full_webapp_and_original_source: null,
    approved_corresponding_source_distribution: null,
    approved_original_source_changes: null,
    approved_original_signed_artifact: null,
    independent_owner_approval_count: 0
  };
  newFile(join(output, "REVIEW-MANIFEST.UNAPPROVED.json"), JSON.stringify(receipt, null, 2) + "\n");
  return receipt;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const receipt = await produceReview(...process.argv.slice(2));
    console.log(JSON.stringify({ status: receipt.status, proposed_main_sha256: receipt.proposed_main_sha256, qualified_approvals: 0 }));
  } catch (error) {
    console.error(String(error));
    process.exitCode = 1;
  }
}
