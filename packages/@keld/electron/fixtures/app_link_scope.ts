import { runInNewContext } from "node:vm";

const mainLink = process.env.KELD_APP_LINK;
if (!mainLink) throw new Error("main fixture requires KELD_APP_LINK");

// Electron's renderer has no Node `process` when nodeIntegration is disabled.
const rendererHasLink = runInNewContext(
  'typeof process !== "undefined" && process.env?.KELD_APP_LINK !== undefined',
  {},
);

// A preload has Node APIs, but Keld does not copy the host-minted app-link
// into its exposed renderer world. Model only that exposed environment view.
const preloadHasLink = runInNewContext('process.env?.KELD_APP_LINK !== undefined', {
  process: { env: {} },
});

console.log(`KEL101_MAIN_HAS_LINK=${mainLink.length > 0}`);
console.log(`KEL101_RENDERER_HAS_LINK=${rendererHasLink}`);
console.log(`KEL101_PRELOAD_HAS_LINK=${preloadHasLink}`);
