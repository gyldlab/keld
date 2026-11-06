/**
 * Shared pieces of the scripted app-link hosts that kipc, `@keld/api` and
 * `@keld/electron` tests bind (GH-1114).
 */
import { randomBytes } from "node:crypto";

/**
 * A fresh Windows app-link endpoint (KEL-101). The transport accepts only the
 * host-minted `\\.\pipe\keld-<64 lowercase hex>` shape (`isWin32PipeEndpoint`
 * in `src/transport.ts`) and reads anything else as a diagnostic port. A pipe
 * name lives in the NPFS namespace, not a directory, so nothing is made or
 * removed.
 */
export function freshWin32AppLinkPipe(): string {
  return String.raw`\\.\pipe\keld-${randomBytes(32).toString("hex")}`;
}

/**
 * Settles `promise` with a plain `await` and returns its rejection, which
 * must be an `Error`; a resolution or any other rejection fails the caller.
 *
 * Host-side close checks use this rather than `expect(pending).rejects`. On
 * Windows (Bun 1.4.4) that matcher's wait defers delivery of a named-pipe
 * peer's close until the wait returns, once the host socket has received
 * data, so a read waiting for that close hits its kill switch; a plain
 * `await` sees the same close within ~2 ms (GH-1114).
 */
export async function rejectionOf(promise: Promise<unknown>): Promise<Error> {
  try {
    await promise;
  } catch (error) {
    if (error instanceof Error) return error;
    throw error;
  }
  throw new Error("expected a rejection; the promise resolved");
}
