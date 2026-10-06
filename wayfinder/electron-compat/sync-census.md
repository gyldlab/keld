# Sync-semantics census (corpus demand × Electron v44.4.5 return types)

Generated 2026-10-06 from `corpus/*.usage.json` (regex member scan; inherited-name ambiguity means counts for generic names like `close`/`focus` are attributed to every class that declares them — treat as upper bounds). Classes: `sync-value` = synchronous non-void return (needs a facade mirror, prefetch, or blocking CALL); `sync-blocking` = `*Sync` API; `sync-void` = fire-and-forget (can be an async CALL without observable change unless ordering matters); `async-promise` = already async in Electron; `sync-property` = property read.

## drawio-desktop

| class | weighted call sites |
|---|---:|
| sync-void | 197 |
| sync-value | 77 |
| event | 60 |
| async-promise | 46 |
| sync-blocking | 1 |

| entity.member | class | returns | count | files |
|---|---|---|---:|---:|
| Tray.isDestroyed | sync-value | boolean | 6 | 2 |
| BrowserWindow.isDestroyed | sync-value | boolean | 6 | 2 |
| BaseWindow.isDestroyed | sync-value | boolean | 6 | 2 |
| WebContents.isDestroyed | sync-value | boolean | 6 | 2 |
| WebFrameMain.isDestroyed | sync-value | boolean | 6 | 2 |
| BrowserWindow.getFocusedWindow | sync-value | BrowserWindow|null | 6 | 1 |
| BaseWindow.getFocusedWindow | sync-value | BaseWindow|null | 6 | 1 |
| app.getVersion | sync-value | String | 4 | 1 |
| BrowserWindow.isFullScreen | sync-value | boolean | 3 | 1 |
| BaseWindow.isFullScreen | sync-value | boolean | 3 | 1 |
| app.getPath | sync-value | String | 3 | 1 |
| BrowserWindow.isMaximized | sync-value | boolean | 2 | 1 |
| NativeImage.resize | sync-value | NativeImage | 2 | 1 |
| NativeImage.toPNG | sync-value | Buffer | 2 | 1 |
| BaseWindow.isMaximized | sync-value | boolean | 2 | 1 |
| Menu.buildFromTemplate | sync-value | Menu | 1 | 1 |
| screen.getAllDisplays | sync-value | Display | 1 | 1 |
| screen.getPrimaryDisplay | sync-value | Display | 1 | 1 |
| BrowserWindow.getAllWindows | sync-value | BrowserWindow | 1 | 1 |
| BrowserWindow.getSize | sync-value | Integer | 1 | 1 |
| BrowserWindow.getPosition | sync-value | Integer | 1 | 1 |
| dialog.showMessageBoxSync | sync-blocking | Integer | 1 | 1 |
| NativeImage.toJPEG | sync-value | Buffer | 1 | 1 |
| NativeImage.getSize | sync-value | Size | 1 | 1 |
| BaseWindow.getAllWindows | sync-value | BaseWindow | 1 | 1 |
| BaseWindow.getSize | sync-value | Integer | 1 | 1 |
| BaseWindow.getPosition | sync-value | Integer | 1 | 1 |
| nativeImage.createFromDataURL | sync-value | NativeImage | 1 | 1 |
| app.getLocale | sync-value | String | 1 | 1 |
| app.requestSingleInstanceLock | sync-value | boolean | 1 | 1 |

## zettlr

| class | weighted call sites |
|---|---:|
| sync-void | 707 |
| async-promise | 309 |
| event | 216 |
| sync-value | 138 |
| sync-blocking | 6 |

| entity.member | class | returns | count | files |
|---|---|---|---:|---:|
| app.getPath | sync-value | String | 40 | 29 |
| screen.getPrimaryDisplay | sync-value | Display | 7 | 2 |
| app.getVersion | sync-value | String | 6 | 6 |
| ipcRenderer.sendSync | sync-blocking | any | 6 | 2 |
| NativeImage.isEmpty | sync-value | boolean | 5 | 3 |
| BrowserWindow.getAllWindows | sync-value | BrowserWindow | 5 | 3 |
| BaseWindow.getAllWindows | sync-value | BaseWindow | 5 | 3 |
| WebFrameMain.reload | sync-value | boolean | 5 | 4 |
| BrowserWindow.getFocusedWindow | sync-value | BrowserWindow|null | 5 | 2 |
| BaseWindow.getFocusedWindow | sync-value | BaseWindow|null | 5 | 2 |
| webUtils.getPathForFile | sync-value | String | 4 | 3 |
| app.getLocale | sync-value | String | 3 | 3 |
| Menu.buildFromTemplate | sync-value | Menu | 3 | 2 |
| Menu.getApplicationMenu | sync-value | Menu|null | 3 | 1 |
| BrowserWindow.getSize | sync-value | Integer | 3 | 2 |
| BaseWindow.getSize | sync-value | Integer | 3 | 2 |
| NativeImage.getSize | sync-value | Size | 3 | 2 |
| NativeImage.resize | sync-value | NativeImage | 3 | 2 |
| nativeImage.createFromPath | sync-value | NativeImage | 3 | 3 |
| BrowserWindow.isMaximized | sync-value | boolean | 3 | 1 |
| BaseWindow.isMaximized | sync-value | boolean | 3 | 1 |
| WebContents.getType | sync-value | String | 3 | 1 |
| app.getName | sync-value | String | 2 | 1 |
| Menu.getMenuItemById | sync-value | MenuItem|null | 2 | 1 |
| BrowserWindow.getBounds | sync-value | Rectangle | 2 | 1 |
| Tray.getBounds | sync-value | Rectangle | 2 | 1 |
| BaseWindow.getBounds | sync-value | Rectangle | 2 | 1 |
| app.requestSingleInstanceLock | sync-value | boolean | 1 | 1 |
| systemPreferences.subscribeNotification | sync-value | number | 1 | 1 |
| app.getRecentDocuments | sync-value | String | 1 | 1 |
| WebContents.getZoomFactor | sync-value | number | 1 | 1 |
| WebContents.getZoomLevel | sync-value | number | 1 | 1 |
| BrowserWindow.isFullScreen | sync-value | boolean | 1 | 1 |
| BaseWindow.isFullScreen | sync-value | boolean | 1 | 1 |
| nativeImage.createFromDataURL | sync-value | NativeImage | 1 | 1 |

## electron-quick-start

| class | weighted call sites |
|---|---:|
| async-promise | 3 |
| sync-value | 2 |
| event | 1 |
| sync-void | 1 |

| entity.member | class | returns | count | files |
|---|---|---|---:|---:|
| BaseWindow.getAllWindows | sync-value | BaseWindow | 1 | 1 |
| BrowserWindow.getAllWindows | sync-value | BrowserWindow | 1 | 1 |
