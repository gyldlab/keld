/**
 * Entry whose first dependency reads `process.type` before `electron` is
 * imported (GH-570 AC8). Run with `--preload=./process_facts_preload.ts` the
 * dependency sees the facts; run without it the dependency sees `undefined`
 * while the facade import still installs them afterwards.
 */
import { writeSync } from "node:fs";
import {
  earlyDefaultApp,
  earlyMode,
} from "./process_facts_early_dependency.ts";
import "electron";

const facade = process as typeof process & { type?: unknown };
writeSync(
  1,
  `GH570_EARLY mode=${earlyMode} defaultApp=${JSON.stringify(earlyDefaultApp)} facadeType=${String(facade.type)}\n`,
);
