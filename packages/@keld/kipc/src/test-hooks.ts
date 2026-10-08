/**
 * Test-only entry to the GH-528 `WorkerLink` hooks (spec gh527 §7).
 *
 * In-repo only: `keld create` stages `transport.ts` as `src/kipc-transport.ts`
 * and never this file, and a release build of the transport
 * (`KELD_KIPC_RELEASE`, see `transport.ts`) has no hook code path at all, so
 * there is nothing here for an app to reach. The hooks only manipulate the
 * role's own transport and grant no authority.
 */
import type { WorkerLink, WorkerLinkOptions, WorkerLinkTestHooks } from "./transport.ts";
// Evaluates the transport, whose static block registers the seam.
import "./transport.ts";

export type { WorkerBlockingFault, WorkerLinkTestHooks } from "./transport.ts";

type TestSeam = (
  options: WorkerLinkOptions,
  hooks: WorkerLinkTestHooks,
) => Promise<{ link: WorkerLink; control: Int32Array }>;

/**
 * `WorkerLink.open` with the GH-528 test hooks, returning the link's control
 * words for assertions. Refuses unless the environment sets
 * `KELD_KIPC_TEST_HOOKS=1`, and refuses on a release build of the transport.
 */
export function openWorkerLinkForTest(
  options: WorkerLinkOptions,
  hooks: WorkerLinkTestHooks,
): Promise<{ link: WorkerLink; control: Int32Array }> {
  const seam = (globalThis as unknown as Record<symbol, TestSeam | undefined>)[
    Symbol.for("keld.kipc.worker-link-test-seam/v1")
  ];
  if (seam === undefined) {
    const error = new Error(
      "KELD-IPC-005: this build of the kipc transport has no test hooks (a release build strips them)",
    );
    Object.defineProperty(error, "code", { value: "KELD-IPC-005", enumerable: true });
    return Promise.reject(error);
  }
  return seam(options, hooks);
}
