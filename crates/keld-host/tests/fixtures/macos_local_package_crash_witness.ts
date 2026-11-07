/** Test-only causal witness. Never part of the framework's boot authority. */
import { rename, writeFile } from "node:fs/promises";
import { join } from "node:path";

interface LifecycleSession {
  onLifecycleEvent(
    listener: (event: "ready" | "last-window-closed") => void,
  ): () => void;
  onEnd(listener: (error: Error) => void): () => void;
}

/** Arm before the first app CALL; publish only after Host Ready and echo Reply. */
export function crashReadyWitness(
  session: LifecycleSession,
  root: string,
  nonce: string,
) {
  const ready = new Promise<void>((resolve, reject) => {
    session.onLifecycleEvent((event) => {
      if (event === "ready") resolve();
    });
    session.onEnd(reject);
  });
  // Keep rejection handled while the caller's echo request is still pending.
  void ready.catch(() => {});
  return async (echo: { message: string; count: number }): Promise<void> => {
    await ready;
    if (echo.message !== "keld" || echo.count !== 1) {
      throw new Error(
        "crash control received an unexpected authenticated echo reply",
      );
    }
    const final = join(root, `${process.pid}.json`);
    const temporary = `${final}.partial`;
    await writeFile(
      temporary,
      JSON.stringify({ nonce, pid: process.pid, ready: true, echo }) + "\n",
      {
        flag: "wx",
        mode: 0o600,
      },
    );
    await rename(temporary, final);
  };
}
