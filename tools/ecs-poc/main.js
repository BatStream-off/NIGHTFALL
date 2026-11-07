// Tâche 5, étape 1 : preuve de faisabilité (CI seulement, jamais distribuée).
// Lit une vidéo Widevine de démonstration (Shaka Player) dans Electron for
// Content Security et dit si le déchiffrement a vraiment eu lieu.
// Fenêtre verrouillée comme le futur lecteur : sandbox, contextIsolation, pas
// de nodeIntegration, aucune autre navigation.
const { app, components, BrowserWindow } = require('electron');
const fs = require('fs');
const path = require('path');

const OUT = process.env.ECS_POC_OUT || path.join(__dirname, 'out');
const LABEL = process.env.ECS_POC_LABEL || 'test';
const TIMEOUT_MS = 90000;

function finish(result, code) {
  console.log('ECS-POC-RESULT ' + JSON.stringify({ label: LABEL, ...result }));
  app.exit(code);
}

app.whenReady().then(async () => {
  let status = null;
  try {
    await components.whenReady();
    status = components.status();
  } catch (e) {
    return finish({ ok: false, step: 'components', error: String(e) }, 2);
  }
  console.log('components:', JSON.stringify(status));
  const win = new BrowserWindow({
    width: 960, height: 600, show: true,
    webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false },
  });
  win.webContents.on('will-navigate', (e) => e.preventDefault());
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  // Electron ≥ 35 : le message est sur l'événement ; avant : 3e argument.
  win.webContents.on('console-message', (e, _level, message) => console.log('[page]', e.message ?? message));
  await win.loadFile(path.join(__dirname, 'test.html'));

  const started = Date.now();
  let last = null;
  while (Date.now() - started < TIMEOUT_MS) {
    await new Promise((r) => setTimeout(r, 1000));
    last = await win.webContents.executeJavaScript('window.nfPocState ? JSON.parse(JSON.stringify(window.nfPocState)) : null');
    if (last && (last.error || (last.keySystem === 'com.widevine.alpha' && last.currentTime > 5 && last.decodedFrames > 60))) break;
  }
  fs.mkdirSync(OUT, { recursive: true });
  const img = await win.webContents.capturePage();
  fs.writeFileSync(path.join(OUT, LABEL + '.png'), img.toPNG());
  const ok = !!(last && !last.error && last.keySystem === 'com.widevine.alpha' && last.currentTime > 5 && last.decodedFrames > 60);
  finish({ ok, components: status, state: last }, ok ? 0 : 1);
});
