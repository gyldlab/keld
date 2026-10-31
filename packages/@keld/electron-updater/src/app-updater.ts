/**
 * The `AppUpdater` subset draw.io touches, mirroring electron-updater 6.8.9
 * `out/AppUpdater.js` and `out/BaseUpdater.js` wherever the shape can be
 * honoured without a host updater (GH-491 spec §3, §4). Line numbers below are
 * into that artifact.
 *
 * Nothing here contacts a feed, downloads, installs or quits. A check that would
 * need a feed is refused with one typed `error` event and a rejection; the
 * events upstream emits from a real check (`checking-for-update`,
 * `update-available`, `update-not-available`, `download-progress`,
 * `update-downloaded`) are never emitted.
 */
import { EventEmitter } from "node:events";

import { updateCheckRefused } from "./error.ts";
import type {
  AppAdapter,
  AppUpdaterEventMap,
  DownloadNotification,
  FeedOptions,
  Logger,
  UpdateCheckResult,
} from "./types.ts";

/** Upstream's `NoOpLogger` (`:720-733`): installed when `logger` is set to `null`. */
export class NoOpLogger implements Logger {
  info(): void {
    // ignore, as upstream does
  }
  warn(): void {
    // ignore, as upstream does
  }
  error(): void {
    // ignore, as upstream does
  }
}

const REFUSAL_DETAIL =
  "the Keld host has no updater path for a migrated app, so no feed was " +
  "contacted and no update exists. Keep updates disabled in the migrated app " +
  "(draw.io: DRAWIO_DISABLE_UPDATE=true in the declared role environment) until " +
  "the host update channel exists (F09-T5, KEL-53).";

export class AppUpdater extends EventEmitter<AppUpdaterEventMap> {
  /** Whether to automatically download an update when it is found (`:109`). */
  autoDownload = true;
  /** Whether to install a downloaded update on app quit (`:114`). */
  autoInstallOnAppQuit = true;
  /** Whether to run the app after a non-silent install (`:119`). */
  autoRunAppAfterInstall = true;

  protected _logger: Logger = console;
  protected readonly app: AppAdapter;

  /**
   * Upstream: `constructor(options, app?)` (`:103`, `:219-224`). The application
   * facts are injected explicitly here (spec §4.2 D3); `index.ts` constructs the
   * module `autoUpdater` with the Electron facade.
   */
  constructor(options: FeedOptions | null | undefined, app: AppAdapter) {
    super();
    // Upstream registers this listener before anything else (`:198-200`), so an
    // `error` emitted with no application listener is logged, not thrown by the
    // emitter.
    this.on("error", (error) => {
      this._logger.error(`Error: ${error.stack || error.message}`);
    });
    this.app = app;
    if (options != null) {
      this.setFeedURL(options);
    }
  }

  /** The stored logger object itself (`:62-64`); never a wrapper. */
  get logger(): Logger | null {
    return this._logger;
  }

  /** `null` installs a `NoOpLogger` (`:65-67`). */
  set logger(value: Logger | null) {
    this._logger = value == null ? new NoOpLogger() : value;
  }

  /** Upstream's deprecated literal (`:227-229`). */
  getFeedURL(): string | null | undefined {
    return "Deprecated. Do not use it.";
  }

  /**
   * ▲ Divergence (spec §3 criterion 5): upstream builds a provider client
   * (`:234-252`). Under Keld the feed is host-declared (architecture 04 §7), so
   * the application's feed is ignored: nothing is constructed, stored or logged,
   * and nothing can throw.
   */
  setFeedURL(_options: FeedOptions): void {
    // Intentionally empty.
  }

  /**
   * Upstream's packaged gate (`:253-256`, `:277-284`): not packaged resolves
   * `null` with no event. Packaged with no host updater is refused with one
   * `error` and the same promise rejecting, in upstream's catch order
   * (`:265-268`). No `checking-for-update`, no feed.
   */
  checkForUpdates(): Promise<UpdateCheckResult | null> {
    if (!this.#isUpdaterActive()) {
      return Promise.resolve(null);
    }
    const refusal = updateCheckRefused(REFUSAL_DETAIL);
    return Promise.resolve().then(() => {
      this.emit(
        "error",
        refusal,
        `Cannot check for updates: ${(refusal.stack || refusal).toString()}`,
      );
      throw refusal;
    });
  }

  /** Same two outcomes as `checkForUpdates` (`:286-300`); nothing is ever downloaded. */
  checkForUpdatesAndNotify(
    _downloadNotification?: DownloadNotification,
  ): Promise<UpdateCheckResult | null> {
    return this.checkForUpdates().then((it) => {
      if (!it?.downloadPromise) {
        this._logger.debug?.(
          "checkForUpdatesAndNotify called, downloadPromise is null",
        );
      }
      return it;
    });
  }

  /** Before any offer, upstream rejects "check first" (`:435-441`); here always. */
  downloadUpdate(): Promise<Array<string>> {
    const error = new Error("Please check update first");
    this.dispatchError(error);
    return Promise.reject(error);
  }

  /**
   * `BaseUpdater.quitAndInstall` (`out/BaseUpdater.js:13-26`) with nothing
   * staged: `install()` finds no installer path, dispatches one `error`
   * (`:42-53`) and returns `false`, so the application is never quit.
   */
  quitAndInstall(_isSilent = false, _isForceRunAfter = false): void {
    this._logger.info("Install on explicit quitAndInstall");
    this.dispatchError(
      new Error("No update filepath provided, can't quit and install"),
    );
  }

  /** `:476-478`. */
  protected dispatchError(e: Error): void {
    this.emit("error", e, (e.stack || e).toString());
  }

  /** `isUpdaterActive` (`:277-284`) without `forceDevUpdateConfig` (not in scope). */
  #isUpdaterActive(): boolean {
    if (this.app.isPackaged) {
      return true;
    }
    this._logger.info(
      "Skip checkForUpdates because application is not packed and dev update config is not forced",
    );
    return false;
  }
}
