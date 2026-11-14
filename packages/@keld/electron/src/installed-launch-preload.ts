/**
 * GH-1404 fixed installed pre-entry composition. This source is emitted intact
 * with its two owner modules under the same relative package layout.
 */
import { hostLaunchFacts } from "../../kipc/src/launch-facts.ts";
import { installMainProcessFacts } from "./process-facts.ts";

if (hostLaunchFacts?.kind !== "local-development") {
  throw Object.assign(
    new Error(
      "KELD-IPC-029: installed startup requires Host-validated local launch facts before application imports",
    ),
    { code: "KELD-IPC-029" },
  );
}

installMainProcessFacts(process, {
  resourcesPath: hostLaunchFacts.resourcesRoot,
});
