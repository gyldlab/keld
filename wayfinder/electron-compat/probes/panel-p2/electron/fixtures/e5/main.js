'use strict';
const path = require('path');
const { app, BrowserWindow, ipcMain } = require('electron');
const { log, tomb } = require('../lib');

app.setPath('userData', path.join(process.env.ORACLE_USERDATA, 'e5'));
// window-veto:     each window vetoes its first close(); quit#1 is aborted; quit#2 succeeds
// beforequit-veto: before-quit vetoes its first emission; quit#2 succeeds
// control:         no app.quit(); close both windows (each vetoes once) to see window-all-closed
const MODE = process.env.E5_MODE || 'window-veto';
log('app:start', { electron: process.versions.electron, mode: MODE });

let bqVetoed = false;
app.on('before-quit', (e) => {
  log('app:before-quit');
  if (MODE === 'beforequit-veto' && !bqVetoed) { bqVetoed = true; e.preventDefault(); log('app:before-quit:preventDefault'); }
});
app.on('will-quit', () => log('app:will-quit'));
app.on('quit', () => log('app:quit'));
app.on('window-all-closed', () => { log('app:window-all-closed'); if (MODE === 'control') app.quit(); });

app.whenReady().then(() => {
  log('app:ready');
  const wins = [];
  let dirtyN = 0;
  for (const id of ['w1', 'w2']) {
    const win = new BrowserWindow({ width: 300, height: 200, show: false,
      webPreferences: { nodeIntegration: true, contextIsolation: false } });
    const wc = win.webContents;
    const st = { id, win, wc, vetoed: MODE === 'beforequit-veto' }; // beforequit-veto: windows never veto
    wins.push(st);
    win.on('close', (event) => {
      log('win:close', Object.assign({ id, vetoed_before: st.vetoed }, tomb(win, wc)));
      if (!st.vetoed) { st.vetoed = true; event.preventDefault(); log('win:close:preventDefault', { id }); }
      else log('win:close:allowed', { id });
    });
    win.on('closed', () => log('win:closed', Object.assign({ id }, tomb(win, wc))));
    wc.on('ipc-message', () => {});
    win.loadFile(path.join(__dirname, 'index.html'));
  }
  ipcMain.on('dirty', () => {
    if (++dirtyN < 2) return;
    log('both-windows-dirty');
    const act = (label) => {
      if (MODE === 'control') {
        log('trigger:close-all:' + label);
        for (const s of wins) { log('trigger:win.close', { id: s.id, label }); s.win.close(); }
      } else {
        log('trigger:app.quit:' + label);
        app.quit();
        log('app.quit:returned:' + label);
      }
    };
    setTimeout(() => act('#1'), 300);
    setTimeout(() => act('#2'), 900);
    setTimeout(() => { log('watchdog:still-alive-after-quit#2'); app.exit(97); }, 4000);
  });
});
