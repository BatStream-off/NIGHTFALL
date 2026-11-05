// Rendu PNG des maquettes (Playwright, fichiers locaux, aucun réseau).
const { chromium } = require('playwright');
const path = require('path');
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  for (const f of ['1-security-center', '2-section-confidentialite', '3-autorisations-site', '4-evenements']) {
    await p.goto('file://' + path.join(__dirname, f + '.html'));
    await p.screenshot({ path: path.join(__dirname, f + '.png'), fullPage: true });
    console.log('ok', f);
  }
  await b.close();
})();
