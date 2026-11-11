/**
 * Public types of the inert `electron-updater` adapter (GH-491,
 * `docs/specs/gh491-inert-electron-updater-adapter.md` §4.3).
 *
 * Shapes mirror electron-updater 6.8.9 (`out/types.d.ts`, `out/AppUpdater.d.ts`,
 * `out/AppAdapter.d.ts`) and builder-util-runtime 9.7.0 (`out/updateInfo.d.ts`,
 * `out/ProgressCallbackTransform.d.ts`, `out/publishOptions.d.ts`) for the
 * members draw.io touches. Upstream `any` parameters are `unknown` here. Nothing
 * in this package ever produces an `UpdateInfo`; the types exist so listener
 * signatures typecheck.
 */

/**
 * `{ info(), warn(), error() }` plus an optional `debug()`: upstream accepts
 * electron-log, winston or any object of this shape.
 */
export interface Logger {
  info(message?: unknown): void;
  warn(message?: unknown): void;
  error(message?: unknown): void;
  debug?(message: string): void;
}

export interface UpdateFileInfo {
  url: string;
  readonly sha512: string;
  size?: number;
  blockMapSize?: number;
  readonly isAdminRightsRequired?: boolean;
}

export interface ReleaseNoteInfo {
  readonly version: string;
  readonly note: string | null;
}

export interface UpdateInfo {
  readonly version: string;
  readonly files: Array<UpdateFileInfo>;
  /** @deprecated */
  readonly path: string;
  /** @deprecated */
  readonly sha512: string;
  releaseName?: string | null;
  releaseNotes?: string | Array<ReleaseNoteInfo> | null;
  releaseDate: string;
  readonly stagingPercentage?: number;
  readonly minimumSystemVersion?: string;
}

export interface UpdateDownloadedEvent extends UpdateInfo {
  downloadedFile: string;
}

export interface ProgressInfo {
  total: number;
  delta: number;
  transferred: number;
  percent: number;
  bytesPerSecond: number;
}

export interface UpdateCheckResult {
  readonly isUpdateAvailable: boolean;
  readonly updateInfo: UpdateInfo;
  readonly downloadPromise?: Promise<Array<string>> | null;
  /** @deprecated */
  readonly versionInfo: UpdateInfo;
}

/** Electron `AuthInfo` subset carried by upstream's `login` event. */
export interface LoginAuthInfo {
  isProxy: boolean;
  scheme: string;
  host: string;
  port: number;
  realm: string;
}

export type LoginCallback = (username?: string, password?: string) => void;

/** Upstream `DownloadNotification`, the optional `checkForUpdatesAndNotify` argument. */
export interface DownloadNotification {
  body: string;
  title: string;
}

/** Upstream `AppUpdaterEvents`, as a Node `EventEmitter` argument-tuple map. */
export type AppUpdaterEventMap = {
  error: [error: Error, message?: string];
  login: [info: LoginAuthInfo, callback: LoginCallback];
  "checking-for-update": [];
  "update-not-available": [info: UpdateInfo];
  "update-available": [info: UpdateInfo];
  "update-downloaded": [event: UpdateDownloadedEvent];
  "download-progress": [info: ProgressInfo];
  "update-cancelled": [info: UpdateInfo];
  "appimage-filename-updated": [path: string];
};

export type UpdaterEvent = keyof AppUpdaterEventMap;

/** The provider names electron-builder publishes to (`PublishProvider`). */
export type PublishProvider =
  | "github"
  | "gitlab"
  | "s3"
  | "spaces"
  | "generic"
  | "custom"
  | "snapStore"
  | "keygen"
  | "bitbucket";

/**
 * A provider configuration for `setFeedURL` (upstream `AllPublishOptions` minus
 * the string form). Only `provider` is required; draw.io passes
 * `{ provider, repo, owner, channel? }` (`electron.js:2078-2083`). Other
 * providers' fields are accepted and ignored.
 */
export interface PublishConfiguration {
  readonly provider: PublishProvider;
  readonly channel?: string | null;
  readonly url?: string | null;
  readonly owner?: string | null;
  readonly repo?: string | null;
  readonly [option: string]: unknown;
}

/** `setFeedURL` accepts a generic server URL string or a provider configuration. */
export type FeedOptions = string | PublishConfiguration;

/**
 * The application facts the adapter reads: the two upstream `AppAdapter`
 * members this slice can observe (spec §4.2 D3).
 */
export interface AppAdapter {
  /** Electron `app.isPackaged`; upstream's gate compares it with `=== true`. */
  readonly isPackaged: boolean;
  /**
   * Electron `app.quit()`. The inert adapter never calls it; the member exists
   * so the no-quit contract of `quitAndInstall` is observable.
   */
  quit(): void | Promise<void>;
}
