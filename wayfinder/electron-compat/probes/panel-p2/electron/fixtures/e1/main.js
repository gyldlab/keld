'use strict';
const path = require('path');
const { app, BrowserWindow, dialog, ipcMain } = require('electron');
const { log, tomb, scheduleDismiss } = require('../lib');

app.setPath('userData', path.join(process.env.ORACLE_USERDATA, 'e1'));
const ANSWER = process.env.E1_ANSWER || 'discard'; // discard -> Return key; cancel -> Escape key
log('app:start', { electron: process.versions.electron, answer: ANSWER });

app.on('before-quit', () => log('app:before-quit'));
app.on('will-quit', () => log('app:will-quit'));
app.on('quit', () => log('app:quit'));
app.on('window-all-closed', () => { log('app:window-all-closed'); app.quit(); });

app.whenReady().then(() => {
  log('app:ready');
  const win = new BrowserWindow({ width: 420, height: 300, show: true,
    webPreferences: { nodeIntegration: true, contextIsolation: false } });
  const wc = win.webContents;
  let modified = false;
  let vetoed = false;

  ipcMain.on('dirty', (_e, v) => {
    modified = v; win.setDocumentEdited(v);
    log('ipc:dirty', { value: v, isDocumentEdited: win.isDocumentEdited() });
    setTimeout(() => { log('trigger:win.close'); win.close(); log('trigger:win.close:returned', tomb(win, wc)); }, 300);
  });

  win.on('close', (event) => {
    log('win:close', Object.assign({ isModified: modified, isDocumentEdited: win.isDocumentEdited() }, tomb(win, wc)));
    if (modified && !vetoed) {
      vetoed = true;
      event.preventDefault();
      log('win:close:preventDefault');
      scheduleDismiss(500, ANSWER === 'discard' ? 'return' : 'escape');
      log('dialog:before');
      const r = dialog.showMessageBoxSync(win, { type: 'question', message: 'Discard changes?',
        buttons: ['Cancel', 'Discard'], defaultId: 1, cancelId: 0 });
      log('dialog:after', Object.assign({ result: r }, tomb(win, wc)));
      if (r === 1) {
        modified = false;
        log('destroy:call');
        win.destroy();
        log('destroy:returned', tomb(win, wc));
        setImmediate(() => log('tick:setImmediate-after-destroy', tomb(win, wc)));
        setTimeout(() => log('tick:setTimeout50-after-destroy', tomb(win, wc)), 50);
      } else {
        log('close:cancelled-by-user');
        setTimeout(() => { modified = false; win.setDocumentEdited(false); log('trigger:win.close#2'); win.close(); }, 300);
      }
    }
  });
  win.on('closed', () => log('win:closed', tomb(win, wc)));
  wc.on('destroyed', () => log('wc:destroyed', tomb(win, wc)));
  win.loadFile(path.join(__dirname, 'index.html'));
});
