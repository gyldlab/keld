/**
 * GH502 realm-local duplicate evaluation diagnostic. This slot does not attest
 * package provenance or grant authority; installed graph admission owns those.
 * A normal cached import/require does not evaluate this module a second time.
 */
import { realpathSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";

const slot = Symbol.for("keld.electron.entry-identity/v1");
const observed = new URL(import.meta.url);
const canonical = pathToFileURL(realpathSync(fileURLToPath(observed)));
canonical.search = observed.search;
canonical.hash = observed.hash;
const location = canonical.href;
const previous = Object.getOwnPropertyDescriptor(globalThis, slot)?.value;

if (previous !== undefined) {
  const error = new Error(
    `KELD-COMPAT-051: a second Electron compatibility entry was evaluated in this role. ` +
      `First: ${previous}. Second: ${location}. ` +
      "Use one installed @keld/electron package for all electron imports.",
  ) as Error & { code: "KELD-COMPAT-051"; first: string; second: string };
  error.name = "DuplicateElectronEntryError";
  error.code = "KELD-COMPAT-051";
  error.first = previous;
  error.second = location;
  throw error;
}
Object.defineProperty(globalThis, slot, {
  value: location,
  writable: false,
  configurable: false,
  enumerable: false,
});
