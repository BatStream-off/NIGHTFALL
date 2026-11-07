// CI: render the interface pages in headless Chromium against a mock API and
// fail when a screen shows nothing (the bug of version 0.1.0: blank window).
const { chromium } = require('playwright');
const PORT_LOCKED = 8765;
const PORT_UNLOCKED = 8766;

async function brightShare(page) {
  const buf = await page.screenshot();
  return page.evaluate(async (b64) => {
    const img = new Image();
    img.src = 'data:image/png;base64,' + b64;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.width; c.height = img.height;
    const g = c.getContext('2d'); g.drawImage(img, 0, 0);
    const d = g.getImageData(0, 0, c.width, c.height).data;
    let bright = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 450) bright++;
    return bright / (d.length / 4);
  }, buf.toString('base64'));
}

(async () => {
  const b = await chromium.launch();
  let failed = false;
  const check = async (name, url, viewport, wait) => {
    const p = await b.newPage({ viewport });
    const errors = [];
    p.on('pageerror', (e) => errors.push(e.message));
    await p.goto(url);
    await p.waitForTimeout(wait);
    const share = await brightShare(p);
    const ok = share > 0.0005;
    console.log(`${ok ? 'OK  ' : 'VIDE'} ${name} : contenu visible ${(share * 100).toFixed(2)} %${errors.length ? ' ; erreurs : ' + errors.join(' | ') : ''}`);
    if (!ok || errors.length) failed = true;
    await p.screenshot({ path: `ui-${name}.png` });
    await p.close();
  };
  await check('verrouillage', `http://127.0.0.1:${PORT_LOCKED}/shell.html?mode=top&w=1&private=0`, { width: 1280, height: 820 }, 3500);
  await check('barre-d-outils', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=top&w=1&private=0`, { width: 1280, height: 88 }, 1500);
  await check('nouvel-onglet', `http://127.0.0.1:${PORT_UNLOCKED}/newtab.html`, { width: 1280, height: 760 }, 1000);
  await check('nouvel-onglet-petit', `http://127.0.0.1:${PORT_UNLOCKED}/newtab.html`, { width: 760, height: 560 }, 1000);
  await check('menu', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=menu`, { width: 340, height: 720 }, 1000);
  await check('menu-onglet', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=tabmenu-2-ro`, { width: 290, height: 380 }, 1000);
  await check('recherche-page', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=find`, { width: 400, height: 50 }, 1000);
  await check('parametres', `http://127.0.0.1:${PORT_UNLOCKED}/settings.html`, { width: 1280, height: 760 }, 1000);
  for (const s of ['general', 'confidentialite', 'securite', 'autorisations', 'navigation', 'recherche', 'performances', 'accessibilite', 'avance']) {
    await check('parametres-' + s, `http://127.0.0.1:${PORT_UNLOCKED}/settings.html#${s}`, { width: 1280, height: 760 }, 800);
  }
  await check('menu-fermeture', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=closeconfirm-3`, { width: 340, height: 150 }, 800);

  // Lecteur NIGHTFALL : bandeau sur un site vidéo protégé, puis accord avant tout
  // téléchargement (le serveur de test répond « non installé »).
  {
    const p = await b.newPage({ viewport: { width: 1280, height: 122 } });
    const errors = [], calls = [];
    p.on('pageerror', (e) => errors.push(e.message));
    p.on('request', (r) => { if (r.url().includes('/api/player.')) calls.push(r.url().split('/api/')[1]); });
    await p.goto(`http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=top&w=1&private=0`);
    await p.waitForTimeout(1200);
    await p.evaluate(() => { activeTab = { id: 7, url: 'https://www.netflix.com/browse' }; return playerBanner(activeTab); });
    const read = () => p.evaluate(() => {
      const b = document.querySelector('#banner');
      const r = b.getBoundingClientRect();
      const btns = [...b.querySelectorAll('button')].map((x) => ({ t: x.textContent, right: x.getBoundingClientRect().right }));
      return { hidden: b.hidden, cls: b.className, text: b.textContent, height: Math.round(r.height), width: innerWidth, btns };
    });
    const bar = await read();
    await p.screenshot({ path: 'ui-lecteur-bandeau.png' });
    await p.click('#banner button.primary');
    await p.waitForTimeout(400);
    const consent = await read();
    await p.screenshot({ path: 'ui-lecteur-accord.png' });
    await p.evaluate(() => { activeTab = { id: 8, url: 'https://example.com/' }; return playerBanner(activeTab); });
    await p.waitForTimeout(300);
    const other = await read();
    const problems = [];
    if (bar.hidden || !bar.text.includes('Ce site exige une protection vidéo. Ouvrir avec le lecteur NIGHTFALL ?')) problems.push('bandeau absent ou texte différent');
    if (bar.cls !== 'info' || bar.height !== 34) problems.push(`bandeau : classe ${bar.cls}, hauteur ${bar.height}`);
    if (bar.btns.length !== 3 || bar.btns.some((x) => x.right > bar.width)) problems.push('boutons du bandeau absents ou coupés');
    if (consent.hidden || !consent.text.includes('Télécharger le lecteur NIGHTFALL') || !consent.btns.some((x) => x.t === 'Télécharger et ouvrir')) problems.push('demande d\'accord absente');
    if (calls.includes('player.install')) problems.push('téléchargement lancé sans accord');
    if (!other.hidden) problems.push('bandeau resté sur un site ordinaire');
    if (errors.length) problems.push('erreurs : ' + errors.join(' | '));
    console.log(`${problems.length ? 'ÉCHEC' : 'OK  '} lecteur-bandeau : ${problems.length ? problems.join(' ; ') : 'bandeau, accord avant téléchargement, retrait hors liste'}`);
    if (problems.length) failed = true;
    await p.close();
  }
  await b.close();
  process.exit(failed ? 1 : 0);
})();
