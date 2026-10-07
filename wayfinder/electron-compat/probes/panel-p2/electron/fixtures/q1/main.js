'use strict';
const path = require('path');
const { app, BrowserWindow, ipcMain, autoUpdater } = require('electron');
const { log, tomb } = require('../lib');

app.setPath('userData', path.join(process.env.ORACLE_USERDATA, 'q1'));
// No update server: autoUpdater has no feed URL and no downloaded update. This probes only the
// window-closing half of quitAndInstall() (shared across platforms), not the install half.
const MODE = process.env.Q1_MODE || 'veto'; // veto: each window vetoes first close; noveto: windows allow
log('app:start', { electron: process.versions.electron, mode: MODE });

app.on('before-quit', () => log('app:before-quit'));
app.on('will-quit', () => log('app:will-quit'));
app.on('quit', () => log('app:quit'));
app.on('window-all-closed', () => log('app:window-all-closed'));
for (const ev of ['error', 'checking-for-update', 'update-available', 'update-not-available', 'update-downloaded', 'before-quit-for-update']) {
  autoUpdater.on(ev, (...a) => log('autoUpdater:' + ev, { arg: a[0] && a[0].message ? a[0].message : undefined }));
}

app.whenReady().then(() => {
  log('app:ready');
  const wins = [];
  let dirtyN = 0;
  for (const id of ['w1', 'w2']) {
    const win = new BrowserWindow({ width: 300, height: 200, show: false,
      webPreferences: { nodeIntegration: true, contextIsolation: false } });
    const wc = win.webContents;
    const st = { id, vetoed: MODE !== 'veto' };
    wins.push(st);
    win.on('close', (event) => {
      log('win:close', Object.assign({ id, vetoed_before: st.vetoed }, tomb(win, wc)));
      if (!st.vetoed) { st.vetoed = true; event.preventDefault(); log('win:close:preventDefault', { id }); }
      else log('win:close:allowed', { id });
    });
    win.on('closed', () => log('win:closed', Object.assign({ id }, tomb(win, wc))));
    win.loadFile(path.join(__dirname, 'index.html'));
  }
  ipcMain.on('dirty', () => {
    if (++dirtyN < 2) return;
    log('both-windows-dirty');
    const act = (label) => {
      log('trigger:autoUpdater.quitAndInstall:' + label);
      try { autoUpdater.quitAndInstall(); log('quitAndInstall:returned:' + label); }
      catch (e) { log('quitAndInstall:threw:' + label, { msg: e.message }); }
    };
    setTimeout(() => act('#1'), 300);
    setTimeout(() => act('#2'), 1200);
    setTimeout(() => { log('watchdog:exit'); app.exit(0); }, 3500);
  });
});
