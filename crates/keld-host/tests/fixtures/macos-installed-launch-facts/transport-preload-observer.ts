/**
 * Stage as src/kipc-transport.ts only in this acceptance fixture. The actual IPC
 * implementation remains the release-stamped @keld/kipc module below.
 *
 * Admission allows this regular-file sidecar to import an application dependency.
 * The fixed Host-owned facts preload must have run before that dependency.
 */
import "./early.ts";
export * from "@keld/kipc";
