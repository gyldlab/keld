const fs = require("node:fs");
const { spawnSync } = require("node:child_process");
function preloadPath() {
  fs.writeFileSync("W1_EXECUTION_MARKER", "project code executed");
  spawnSync(process.execPath, ["-e", "process.exit(37)"]);
  return "./preload-a.js";
}
module.exports = { preloadPath };
