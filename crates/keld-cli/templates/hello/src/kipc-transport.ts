/**
 * In-repo re-export of the canonical KEL-136 transport.
 *
 * `keld create` does not copy this shim: template.rs embeds
 * `packages/@keld/kipc/src/transport.ts` as `src/kipc-transport.ts`.
 */
export * from "../../../../../packages/@keld/kipc/src/transport.ts";
