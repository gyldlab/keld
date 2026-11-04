import { writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
writeFileSync("W1_CONFIG_EXECUTION_MARKER", "config executed");
spawnSync(process.execPath, ["-e", "process.exit(37)"]);
export default { entry: "main.cjs" };
