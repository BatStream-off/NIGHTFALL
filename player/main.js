// Lecteur NIGHTFALL : application séparée, basée sur Electron for Content
// Security (castLabs, MIT), réservée aux sites vidéo protégés par Widevine.
//
// Verrouillage :
// - sandbox de Chromium pour tous les moteurs de rendu, contextIsolation, aucun
//   nodeIntegration, aucun préchargement, aucune extension, outils de
//   développement désactivés ;
// - seules les adresses https des services de services.js s'ouvrent ici ; toute
//   autre navigation (lien, redirection, nouvelle fenêtre) est rendue à NIGHTFALL ;
// - cookies et données dans le dossier propre au lecteur (partition persistante
//   « nightfall-lecteur »), séparés de ceux de NIGHTFALL ;
// - autorisations refusées sauf l'identifiant de contenu protégé et le plein écran ;
// - aucune télémétrie : pas de crashReporter, pas de service de correction en ligne.
'use strict';

const { app, components, BrowserWindow, Menu, session, dialog } = require('electron');
const { spawn } = require('child_process');
const fs = require('fs');
const { serviceFor } = require('./services');

const PARTITION = 'persist:nightfall-lecteur';
const SELFTEST = process.argv.includes('--nf-selftest');

app.enableSandbox();
app.setName('Lecteur NIGHTFALL');
// Pas de fenêtre de correction orthographique en ligne ni de requête « safe browsing » propre à Electron.
app.commandLine.appendSwitch('disable-features', 'SpellcheckService');

// ---- arguments : --nf-browser=<chemin de NIGHTFALL> <adresse>
function argValue(prefix) {
  const a = process.argv.find((x) => x.startsWith(prefix));
  return a ? a.slice(prefix.length) : null;
}
function urlArg(argv) {
  return argv.find((a) => /^https?:\/\//i.test(a)) || null;
}
const nightfallPath = argValue('--nf-browser=');

// Rendre une adresse à NIGHTFALL (jamais au navigateur par défaut du système,
// qui n'est peut-être pas NIGHTFALL). Seulement http(s).
const forwarded = []; // auto-test : adresses rendues à NIGHTFALL
function toNightfall(url) {
  if (!/^https?:\/\//i.test(url)) return;
  if (SELFTEST) { forwarded.push(url); return; }
  if (nightfallPath && fs.existsSync(nightfallPath)) {
    spawn(nightfallPath, [url], { detached: true, stdio: 'ignore' }).unref();
  } else {
    dialog.showMessageBox({ type: 'info', message: 'Ce site s\'ouvre dans NIGHTFALL.', detail: url, buttons: ['OK'] });
  }
}

let win = null;

function harden(contents) {
  contents.on('will-navigate', (e, url) => {
    if (!serviceFor(url)) { e.preventDefault(); toNightfall(url); }
  });
  contents.on('will-redirect', (e, url) => {
    if (e.isMainFrame && !serviceFor(url)) { e.preventDefault(); toNightfall(url); }
  });
  contents.setWindowOpenHandler(({ url }) => {
    if (serviceFor(url) && win) win.loadURL(url);
    else toNightfall(url);
    return { action: 'deny' };
  });
  contents.on('will-attach-webview', (e) => e.preventDefault());
  contents.on('devtools-opened', () => contents.closeDevTools());
}

function setupSession() {
  const ses = session.fromPartition(PARTITION);
  ses.setPermissionRequestHandler((_wc, permission, done) => done(permission === 'fullscreen' || permission === 'mediaKeySystem'));
  ses.setPermissionCheckHandler((_wc, permission) => permission === 'fullscreen' || permission === 'mediaKeySystem');
  ses.setSpellCheckerEnabled(false);
  ses.on('will-download', (e) => e.preventDefault());
  return ses;
}

function createWindow(url) {
  win = new BrowserWindow({
    width: 1280, height: 760, minWidth: 640, minHeight: 400,
    title: 'Lecteur NIGHTFALL', backgroundColor: '#05070c', autoHideMenuBar: true,
    webPreferences: {
      partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false,
      nodeIntegrationInSubFrames: false, webviewTag: false, devTools: SELFTEST, spellcheck: false,
      safeDialogs: true, navigateOnDragDrop: false,
    },
  });
  win.on('page-title-updated', (e, title) => { e.preventDefault(); win.setTitle(title ? title + ' — Lecteur NIGHTFALL' : 'Lecteur NIGHTFALL'); });
  win.loadURL(url);
}

function menu() {
  const mac = process.platform === 'darwin';
  Menu.setApplicationMenu(Menu.buildFromTemplate([
    ...(mac ? [{ label: 'Lecteur NIGHTFALL', submenu: [{ role: 'hide', label: 'Masquer' }, { type: 'separator' }, { role: 'quit', label: 'Quitter' }] }] : []),
    { label: 'Navigation', submenu: [
      { label: 'Retour', accelerator: mac ? 'Cmd+[' : 'Alt+Left', click: () => win && win.webContents.navigationHistory.goBack() },
      { label: 'Actualiser', accelerator: 'CmdOrCtrl+R', click: () => win && win.webContents.reload() },
      { label: 'Ouvrir cette page dans NIGHTFALL', click: () => win && toNightfall(win.webContents.getURL()) },
      ...(mac ? [] : [{ type: 'separator' }, { role: 'quit', label: 'Quitter' }]),
    ] },
    { label: 'Édition', submenu: [
      { role: 'undo', label: 'Annuler' }, { role: 'redo', label: 'Rétablir' }, { type: 'separator' },
      { role: 'cut', label: 'Couper' }, { role: 'copy', label: 'Copier' }, { role: 'paste', label: 'Coller' }, { role: 'selectAll', label: 'Tout sélectionner' },
    ] },
    { label: 'Affichage', submenu: [{ role: 'togglefullscreen', label: 'Plein écran' }, { role: 'resetZoom', label: 'Taille réelle' }, { role: 'zoomIn', label: 'Agrandir' }, { role: 'zoomOut', label: 'Réduire' }] },
  ]));
}

// ---- une seule instance : une seconde ouverture passe son adresse à la première
if (!SELFTEST && !app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', (_e, argv) => {
    const url = urlArg(argv);
    if (url && serviceFor(url) && win) { win.loadURL(url); if (win.isMinimized()) win.restore(); win.focus(); }
    else if (url) toNightfall(url);
  });
  app.on('web-contents-created', (_e, contents) => harden(contents));
  app.on('window-all-closed', () => app.quit());

  app.whenReady().then(async () => {
    menu();
    setupSession();
    try {
      await components.whenReady();
    } catch (e) {
      dialog.showErrorBox('Lecteur NIGHTFALL', 'Le module de lecture protégée (Widevine) n\'a pas pu être installé.\n\n' + String(e));
    }
    if (SELFTEST) return require('./selftest/run').run({ partition: PARTITION, status: components.status(), forwarded, serviceFor });
    const url = urlArg(process.argv);
    if (!url || !serviceFor(url)) {
      if (url) toNightfall(url);
      dialog.showMessageBoxSync({ type: 'info', message: 'Le lecteur NIGHTFALL s\'ouvre depuis NIGHTFALL, sur un site vidéo protégé.', buttons: ['OK'] });
      return app.quit();
    }
    createWindow(url);
  });
}

