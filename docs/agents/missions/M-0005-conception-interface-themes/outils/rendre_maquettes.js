// Rend les maquettes HTML en PNG. Usage : NODE_PATH=$(npm root -g) node rendre_maquettes.js <dossier maquettes> <dossier sortie> [filtre]
const { chromium } = require('playwright');
const path = require('path');
const [DIR, OUT, FILTER] = process.argv.slice(2);
const SHOTS = [
  ['fenetre-windows', 'fenetre.html?os=win', 1280, 800],
  ['fenetre-windows-survol', 'fenetre.html?os=win&hover=.tab:nth-child(2),.wc span:nth-child(3),.shield', 1280, 800],
  ['fenetre-macos', 'fenetre.html?os=mac', 1280, 800],
  ['fenetre-privee', 'fenetre.html?os=win&private=1', 1280, 800],
  ['fenetre-onglets-verticaux', 'fenetre.html?os=win&mode=v', 1280, 800],
  ['fenetre-macos-onglets-verticaux', 'fenetre.html?os=mac&mode=v', 1280, 800],
  ['fenetre-theme-aube', 'fenetre.html?os=win&theme=aube', 1280, 800],
  ['fenetre-theme-braise', 'fenetre.html?os=mac&theme=braise', 1280, 800],
  ['header-detail-2x', 'fenetre.html?os=win', 1280, 84, 2],
  ['onglets-etats', 'onglets.html', 1440, 520],
  ['menus', 'menus.html', 1280, 760],
  ['panneaux', 'panneaux.html', 1280, 800],
  ['page-interne-apparence', 'page-apparence.html', 1280, 900],
  ['editeur-themes', 'editeur.html', 1280, 900],
  ['systeme-de-design', 'systeme.html', 1280, 1500],
  ['barre-de-titre-verrouillage', 'verrouillage.html', 1280, 800],
];
(async () => {
  const b = await chromium.launch();
  for (const [name, url, w, h, scale] of SHOTS) {
    if (FILTER && !name.includes(FILTER)) continue;
    const p = await b.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: scale || 1 });
    const errs = []; p.on('pageerror', (e) => errs.push(e.message)); p.on('console', (m) => m.type() === 'error' && errs.push(m.text()));
    await p.goto('file://' + path.resolve(DIR, url));
    await p.waitForTimeout(600);
    await p.screenshot({ path: path.join(OUT, name + '.png'), fullPage: h > 900 });
    console.log(name, errs.length ? 'ERREURS ' + errs.join(' | ') : 'ok');
    await p.close();
  }
  await b.close();
})();
