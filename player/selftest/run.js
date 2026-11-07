// Auto-test du lecteur (CI : `--nf-selftest`), dans le paquet signé tel qu'il
// sera distribué. Vérifie la lecture Widevine et le verrouillage, puis écrit
// une ligne « PLAYER-SELFTEST {json} » et quitte (0 = tout est bon).
'use strict';

const { app, BrowserWindow, session } = require('electron');
const fs = require('fs');
const path = require('path');

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function run({ partition, status, forwarded, serviceFor }) {
  const out = process.env.NF_PLAYER_OUT || app.getPath('temp');
  const checks = {};
  const fail = [];
  const check = (name, ok, detail) => { checks[name] = { ok: !!ok, detail }; if (!ok) fail.push(name); };

  // 1. Liste des services : sous-domaines oui, imitations et http non.
  check('services', !!serviceFor('https://www.netflix.com/browse') && !!serviceFor('https://www.disneyplus.com/')
    && !serviceFor('https://evilnetflix.com/') && !serviceFor('http://www.netflix.com/') && !serviceFor('https://netflix.com.example.org/')
    && !serviceFor('https://user:pw@www.netflix.com/') && !serviceFor('https://www.netflix.com:8443/'), 'netflix.com, disneyplus.com acceptés ; imitations, http, port et identifiants refusés');

  // 2. Données séparées de NIGHTFALL : dossier propre au lecteur, partition persistante.
  const storage = session.fromPartition(partition).storagePath || '';
  check('donnees-separees', /Lecteur NIGHTFALL/.test(app.getPath('userData')) && /nightfall-lecteur/.test(storage),
    { userData: app.getPath('userData'), partition: storage });

  // 3. Widevine installé.
  const cdm = Object.values(status || {}).find((c) => /Widevine/i.test(c.title || ''));
  check('widevine-installe', !!cdm && /\d/.test(cdm.version || ''), cdm);

  const win = new BrowserWindow({
    width: 960, height: 600, show: true,
    webPreferences: { partition, sandbox: true, contextIsolation: true, nodeIntegration: false, devTools: false },
  });
  await win.loadFile(path.join(__dirname, 'test.html'));

  // 4. Page isolée : ni require ni process dans la page.
  const isolated = await win.webContents.executeJavaScript('typeof require === "undefined" && typeof process === "undefined"');
  check('page-isolee', isolated === true, 'require et process absents de la page');
  check('sandbox', win.webContents.getOSProcessId() > 0 && win.webContents.getWebPreferences().sandbox === true, 'sandbox du moteur de rendu');

  // 5. Lecture Widevine réelle (images décodées après licence).
  const started = Date.now();
  let st = null;
  while (Date.now() - started < 90000) {
    await wait(1000);
    st = await win.webContents.executeJavaScript('window.nfPocState ? JSON.parse(JSON.stringify(window.nfPocState)) : null');
    if (st && (st.error || (st.keySystem === 'com.widevine.alpha' && st.currentTime > 5 && st.decodedFrames > 60))) break;
  }
  check('lecture-widevine', !!st && !st.error && st.keySystem === 'com.widevine.alpha' && st.currentTime > 5 && st.decodedFrames > 60 && st.licenses > 0, st);
  fs.mkdirSync(out, { recursive: true });
  fs.writeFileSync(path.join(out, `lecteur-${process.platform}.png`), (await win.webContents.capturePage()).toPNG());

  // 6. Navigation hors liste : bloquée et rendue à NIGHTFALL.
  const before = win.webContents.getURL();
  await win.webContents.executeJavaScript('location.href = "https://example.com/hors-liste"; true');
  await wait(1500);
  check('navigation-rendue', win.webContents.getURL() === before && forwarded.includes('https://example.com/hors-liste'), { url: win.webContents.getURL(), forwarded });

  // 7. Nouvelle fenêtre hors liste : refusée et rendue à NIGHTFALL.
  await win.webContents.executeJavaScript('window.open("https://example.org/pop-up"); true');
  await wait(800);
  check('fenetre-refusee', BrowserWindow.getAllWindows().length === 1 && forwarded.includes('https://example.org/pop-up'), forwarded);

  // 8. Autorisations : seul le contenu protégé est permis.
  const perms = await win.webContents.executeJavaScript(
    'Promise.all(["geolocation","notifications","camera","microphone"].map((n) => navigator.permissions.query({ name: n }).then((r) => r.state, () => "erreur")))');
  check('autorisations', perms.every((p) => p !== 'granted'), perms);

  // Mémoire de tous les processus du lecteur pendant la lecture (Ko, ensemble de travail).
  const metrics = app.getAppMetrics();
  const memoryKB = metrics.reduce((n, m) => n + (m.memory ? m.memory.workingSetSize : 0), 0);
  const result = { ok: fail.length === 0, fail, checks, memoryKB, processes: metrics.length };
  fs.writeFileSync(path.join(out, `selftest-${process.platform}.json`), JSON.stringify(result, null, 1));
  console.log('PLAYER-SELFTEST ' + JSON.stringify(result));
  app.exit(fail.length === 0 ? 0 : 1);
}

module.exports = { run };
