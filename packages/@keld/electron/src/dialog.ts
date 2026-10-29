/** Source-exact asynchronous Electron Message facade. The Host, not this
 * module, owns the verified permission, window identity and native panel. */
import { invokeMessageDialog } from "../../api/src/app.ts";
import type { MessageOptions } from "../../api/src/dialog.generated.ts";
import type { WindowRef } from "../../api/src/window-codec.ts";

import { readDialogParentRef } from "./browser-window.ts";
import { normalizeMessageOptions } from "./dialog-options.ts";

/** The native button index and checkbox state reported by AppKit, not a
 * guessed `defaultId` or a fabricated Cancel selection. */
export interface MessageBoxResult {
  response: number;
  checkboxChecked: boolean;
}

export type MessageBoxOptions = Readonly<{
  message: string;
  type?: "none" | "info" | "error" | "question" | "warning";
  title?: string;
  detail?: string;
  buttons?: readonly string[];
  defaultId?: number;
  cancelId?: number;
  noLink?: boolean;
  normalizeAccessKeys?: boolean;
  checkboxLabel?: string;
  checkboxChecked?: boolean;
}>;

function submit(
  parent: WindowRef | null,
  options: MessageOptions,
): Promise<MessageBoxResult> {
  return invokeMessageDialog(parent, options).then((reply) => {
    if (reply.variant !== "Message") {
      throw new Error(
        "KELD-IPC-003: native Message returned another operation's disposition",
      );
    }
    return {
      response: reply.response,
      checkboxChecked: reply.checkbox_checked,
    };
  });
}

/** The only public Message API; one-argument calls are allowed by Electron
 * grammar but the Host currently refuses unimplemented parentless presentation
 * with a typed error. It never opens another WorkerLink or silently succeeds. */
export function showMessageBox(
  options: MessageBoxOptions,
): Promise<MessageBoxResult>;
export function showMessageBox(
  parent: unknown,
  options: MessageBoxOptions,
): Promise<MessageBoxResult>;
export function showMessageBox(
  parentOrOptions: unknown,
  suppliedOptions?: MessageBoxOptions,
): Promise<MessageBoxResult> {
  const withParent = arguments.length === 2;
  if (arguments.length !== 1 && !withParent) {
    throw new TypeError(
      "KELD-COMPAT-010: showMessageBox takes options or BrowserWindow and options",
    );
  }
  // Private #ref branding precedes all option parsing; no untrusted `id`,
  // `isDestroyed`, getter or Proxy trap can mint a parent identity.
  // Electron treats an explicit null/undefined parent in the two-argument
  // overload as absent. Every other supplied value still needs the genuine
  // private BrowserWindow brand, with no caller property reads.
  const parent =
    withParent && parentOrOptions != null
      ? readDialogParentRef(parentOrOptions)
      : null;
  const options = normalizeMessageOptions(
    withParent ? suppliedOptions : parentOrOptions,
  );
  return submit(parent, options);
}

export const dialog = Object.freeze({ showMessageBox });
