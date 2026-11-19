/** Main-role process facts for the qualified original Windows entry (GH-570).
 * Its facade initializes these before Runtime captures the generation's versions.
 * Ordinary facade launches retain their established descriptor behavior.
 */
/** Reported `process.versions.electron`: the `@keld/electron` package version, not an Electron release. */
export const ELECTRON_VERSION_SHIM = "0.0.1";

/** Electron's `process.type` in the main process. */
export const MAIN_PROCESS_TYPE = "browser";

/** The `process`-shaped object the installer writes to; `process` satisfies it. */
export interface ProcessFactsTarget {
  readonly versions: object;
}

type FactValue = string | boolean;

interface Fact {
  /** Object that carries the property: the target or its `versions`. */
  readonly owner: object;
  /** Dotted name for diagnostics, for example `process.versions.electron`. */
  readonly label: string;
  readonly name: string;
  readonly value: FactValue;
}

interface Plan {
  readonly fact: Fact;
  /** `false` when the frozen fact is already in place. */
  readonly write: boolean;
}

function frozenDescriptor(value: FactValue): PropertyDescriptor {
  return { value, writable: false, enumerable: true, configurable: false };
}

function isInstalled(existing: PropertyDescriptor, value: FactValue): boolean {
  return (
    existing.value === value &&
    existing.writable === false &&
    existing.enumerable === true &&
    existing.configurable === false
  );
}

function show(existing: PropertyDescriptor): string {
  if (!("value" in existing)) return "an accessor";
  return typeof existing.value === "string"
    ? JSON.stringify(existing.value)
    : String(existing.value);
}

const FIX =
  "Remove that definition: @keld/electron's process-facts module is the only installer of main-role process.* facts, and a Host preload must import it instead of defining its own.";

/**
 * Decides whether `fact` must be written, is already in place, or belongs to a
 * second owner. Writes nothing, so a conflict on the last fact cannot leave the
 * first ones installed.
 */
function plan(fact: Fact): Plan {
  const existing = Object.getOwnPropertyDescriptor(fact.owner, fact.name);
  if (existing === undefined) return { fact, write: true };
  const sameValue = "value" in existing && existing.value === fact.value;
  if (!sameValue) {
    throw new Error(
      `KELD-COMPAT-014: ${fact.label} is already ${show(existing)}; a second owner defined it before @keld/electron. ${FIX}`,
    );
  }
  // A non-configurable property keeps its enumerability, so only an enumerable
  // one can still become Electron's frozen enumerable fact.
  if (existing.configurable !== true && existing.enumerable !== true) {
    throw new Error(
      `KELD-COMPAT-014: ${fact.label} is already ${show(existing)} but non-configurable and non-enumerable, so it cannot become Electron's frozen enumerable fact. ${FIX}`,
    );
  }
  return { fact, write: !isInstalled(existing, fact.value) };
}

/**
 * Installs the facade's main-role properties. A qualified original additionally
 * receives defaultApp and the frozen descriptors before runtime capture.
 *
 * Idempotent: a repeat call, or the facade import after a preload already
 * installed the facts, writes nothing. A same-value definition that is still
 * configurable is adopted and frozen.
 *
 * @throws Error `KELD-COMPAT-014` when a fact is already defined with another
 * value, as an accessor, or in a shape that cannot be frozen; nothing is written
 * in that case.
 */
export function installMainProcessFacts(
  target: ProcessFactsTarget,
  qualifiedOriginal: boolean,
): void {
  if (!qualifiedOriginal) {
    // Existing facade profiles retain their established property descriptors.
    (target.versions as { electron?: string }).electron = ELECTRON_VERSION_SHIM;
    Object.defineProperty(target, "type", {
      value: MAIN_PROCESS_TYPE,
      configurable: true,
      enumerable: true,
      writable: false,
    });
    return;
  }
  const facts: Fact[] = [
    {
      owner: target,
      label: "process.type",
      name: "type",
      value: MAIN_PROCESS_TYPE,
    },
    {
      owner: target,
      label: "process.defaultApp",
      name: "defaultApp",
      value: true,
    },
    {
      owner: target.versions,
      label: "process.versions.electron",
      name: "electron",
      value: ELECTRON_VERSION_SHIM,
    },
  ];
  const plans = facts.map(plan);
  for (const { fact, write } of plans) {
    if (write) {
      Object.defineProperty(
        fact.owner,
        fact.name,
        frozenDescriptor(fact.value),
      );
    }
  }
}
