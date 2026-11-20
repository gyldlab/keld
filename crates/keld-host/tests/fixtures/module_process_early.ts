/** Imported dependency: capture facts before application entry evaluation. */
const proc = process as typeof process & Record<string, unknown>;
const tag = (value: unknown) =>
  value === undefined ? { kind: "undefined" } : { kind: typeof value, value };
export const gh569Early = {
  type: tag(proc.type),
  defaultApp: tag(proc.defaultApp),
  mas: tag(proc.mas),
  windowsStore: tag(proc.windowsStore),
  sandboxed: tag(proc.sandboxed),
  contextIsolated: tag(proc.contextIsolated),
  resourcesPath: tag(proc.resourcesPath),
  electron: tag(process.versions.electron),
  argv: [...process.argv],
};
