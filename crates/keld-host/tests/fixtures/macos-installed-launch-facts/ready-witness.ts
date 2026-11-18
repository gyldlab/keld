/** Test-only causal witness; it is not framework boot data or an authority. */
import { rename, writeFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";

export interface ReadyWitnessConfig {
  readonly directory: string;
  readonly nonce: string;
}

/** Called only after the real app.whenReady() and all installed-fact assertions. */
export async function publishReadyWitness(
  config: ReadyWitnessConfig,
  facts: unknown,
): Promise<void> {
  if (!isAbsolute(config.directory) || !/^[0-9a-f]{64}$/.test(config.nonce)) {
    throw new Error("invalid test-only installed-facts witness configuration");
  }
  const final = join(config.directory, `${process.pid}.json`);
  const temporary = `${final}.partial`;
  await writeFile(
    temporary,
    JSON.stringify({
      nonce: config.nonce,
      pid: process.pid,
      ppid: process.ppid,
      ready: true,
      facts,
    }) + "\n",
    { flag: "wx", mode: 0o600 },
  );
  await rename(temporary, final);
}
