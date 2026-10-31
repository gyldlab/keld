/**
 * Dependency shaped like electron-log 5.4.4 `src/index.js:5-8`: it selects its
 * mode from `process.type` while it evaluates, before the application imports
 * `electron`. It must not import `electron` itself, or it would observe the
 * facade's own installation and prove nothing about the pre-entry moment.
 */
const proc = process as typeof process & {
  type?: unknown;
  defaultApp?: unknown;
};

export const earlyMode =
  typeof process === "object" && proc.type === "browser" ? "main" : "unknown";
export const earlyDefaultApp = proc.defaultApp;
