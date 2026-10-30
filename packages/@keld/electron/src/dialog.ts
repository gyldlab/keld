/** Source-exact asynchronous Electron Message facade. The Host, not this
 * module, owns the verified permission, window identity and native panel. */
import { invokeFileDialog, invokeMessageDialog } from "../../api/src/app.ts";
import type { MessageOptions } from "../../api/src/dialog.generated.ts";
import type { WindowRef } from "../../api/src/window-codec.ts";

import { readDialogParentRef } from "./browser-window.ts";
import { normalizeMessageOptions } from "./dialog-options.ts";
import {
  normalizeOpenDialogOptions,
  normalizeSaveDialogOptions,
} from "./file-dialog-options.ts";

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

/** Only the approved first-proof Open grammar. Unsupported source keys are
 * rejected at runtime even when a JS caller supplies `undefined`. */
export type OpenDialogOptions = Readonly<{
  defaultPath?: string;
  filters?: readonly Readonly<{
    name: string;
    extensions: readonly string[];
  }>[];
  properties?: readonly ("openFile" | "openDirectory" | "multiSelections")[];
  securityScopedBookmarks?: false;
}>;

export type SaveDialogOptions = Readonly<{
  defaultPath?: string;
  filters?: readonly Readonly<{
    name: string;
    extensions: readonly string[];
  }>[];
  securityScopedBookmarks?: false;
}>;

export interface OpenDialogResult {
  canceled: boolean;
  filePaths: string[];
}

export interface SaveDialogResult {
  canceled: boolean;
  filePath: string;
}

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

function fileParent(input: unknown): WindowRef | null {
  return input == null ? null : readDialogParentRef(input);
}

/** Async Open returns only OS-selected paths, never a file descriptor, handle,
 * bookmark, scope or an implicit `node:fs` permission. */
export function showOpenDialog(
  options: OpenDialogOptions,
): Promise<OpenDialogResult>;
export function showOpenDialog(
  parent: unknown,
  options: OpenDialogOptions,
): Promise<OpenDialogResult>;
export function showOpenDialog(
  parentOrOptions: unknown,
  input?: OpenDialogOptions,
): Promise<OpenDialogResult> {
  if (arguments.length !== 1 && arguments.length !== 2) {
    throw new TypeError(
      "KELD-COMPAT-010: showOpenDialog requires options or BrowserWindow plus options",
    );
  }
  const withParent = arguments.length === 2;
  const parent = withParent ? fileParent(parentOrOptions) : null;
  const options = normalizeOpenDialogOptions(
    withParent ? input : parentOrOptions,
  );
  return invokeFileDialog("Open", parent, options).then(
    (result): OpenDialogResult => {
      if (result.variant !== "Open") {
        throw new Error(
          "KELD-IPC-003: native Open returned a different operation's result",
        );
      }
      return { canceled: result.canceled, filePaths: result.file_paths };
    },
  );
}

/** Async Save returns the chosen destination only. It never writes a file or
 * creates an authority scope; true securityScopedBookmarks is unsupported. */
export function showSaveDialog(
  options: SaveDialogOptions,
): Promise<SaveDialogResult>;
export function showSaveDialog(
  parent: unknown,
  options: SaveDialogOptions,
): Promise<SaveDialogResult>;
export function showSaveDialog(
  parentOrOptions: unknown,
  input?: SaveDialogOptions,
): Promise<SaveDialogResult> {
  if (arguments.length !== 1 && arguments.length !== 2) {
    throw new TypeError(
      "KELD-COMPAT-010: showSaveDialog requires options or BrowserWindow plus options",
    );
  }
  const withParent = arguments.length === 2;
  const parent = withParent ? fileParent(parentOrOptions) : null;
  const options = normalizeSaveDialogOptions(
    withParent ? input : parentOrOptions,
  );
  return invokeFileDialog("Save", parent, options).then(
    (result): SaveDialogResult => {
      if (result.variant !== "Save") {
        throw new Error(
          "KELD-IPC-003: native Save returned a different operation's result",
        );
      }
      return { canceled: result.canceled, filePath: result.file_path };
    },
  );
}

export const dialog = Object.freeze({
  showMessageBox,
  showOpenDialog,
  showSaveDialog,
});
