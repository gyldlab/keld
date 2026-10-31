/**
 * The one typed refusal this package emits (GH-491 spec §4.2 D4).
 *
 * The code is registered in `docs/engineering/keld-error-codes.md`, outside
 * keld-compat's 001..009 range and never a `KELD-UPDATE-*` host code, and the
 * registry test scans this package's `src` for it. The `Error` carries the code
 * as a field, the shape `@keld/kipc` uses for rejected calls, so callers branch
 * on `code` instead of parsing `message`.
 */
import type { KeldCallError } from "../../kipc/src/transport.ts";

/** A `checkForUpdates()` that would need a feed is refused with this code. */
export const UPDATE_CHECK_REFUSED = "KELD-COMPAT-050";

/** Build the refusal: `"<code>: <detail>"` with the fix inside the message. */
export function updateCheckRefused(detail: string): KeldCallError {
  return Object.assign(new Error(`${UPDATE_CHECK_REFUSED}: ${detail}`), {
    code: UPDATE_CHECK_REFUSED,
  });
}
