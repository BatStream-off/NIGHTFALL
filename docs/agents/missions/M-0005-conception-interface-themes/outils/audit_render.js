// Audit M-0005 : rendu de toutes les pages d'interface actuelles (Chromium
// sans affichage, API simulée) et relevé des styles réellement calculés.
// Usage : NODE_PATH=$(npm root -g) node audit_render.js <port verrouillé> <port déverrouillé> <dossier de sortie>
const { chromium } = require('playwright');
const fs = require('fs');
const [PL, PU, OUT] = process.argv.slice(2);
const U = (p) => `http://127.0.0.1:${PU}/${p}`;
const TABS = [
  { id: 1, title: 'Le Monde.fr - Actualités et Infos', url: 'https://www.lemonde.fr/', active: false, loading: false, pinned: false, blocked: 41, zoom: 100 },
  { id: 2, title: 'Crépuscule — Wikipédia', url: 'https://fr.wikipedia.org/wiki/Cr%C3%A9puscule', active: true, loading: false, pinned: false, blocked: 3, zoom: 100, canBack: true },
  { id: 3, title: 'GitHub · BatStream-off/NIGHTFALL', url: 'https://github.com/', active: false, loading: true, pinned: false, blocked: 0, zoom: 100 },
  { id: 4, title: 'Nouvel onglet', url: 'nightfall://app/newtab.html', active: false, loading: false, pinned: false, blocked: 0, zoom: 100 },
];
const PAGES = [
  ['nouvel-onglet', 'newtab.html', 1280, 760], ['parametres', 'settings.html', 1280, 1400], ['securite', 'security.html', 1280, 2600],
  ['historique', 'history.html', 1280, 760], ['favoris', 'bookmarks.html', 1280, 900], ['telechargements', 'downloads.html', 1280, 600],
  ['mots-de-passe', 'passwords.html', 1280, 900], ['extensions', 'extensions.html', 1280, 600], ['a-propos', 'about.html', 1280, 600],
  ['site-non-securise', 'insecure.html?url=http%3A%2F%2Fexemple.fr', 1280, 700],
];
const POPUPS = [['menu', 'menu', 320, 640], ['menu-onglet', 'tabmenu-2-ro', 290, 380], ['shield', 'shield', 340, 420], ['mots-de-passe-panneau', 'key', 340, 360], ['recherche', 'find', 420, 50], ['zoom', 'zoom', 300, 46]];

// Styles computed on every visible element: what sizes, radii, fonts,
// colours are really used (the audit counts the distinct values).
function survey() {
  const tally = {}; const add = (k, v) => { (tally[k] = tally[k] || {})[v] = (tally[k][v] || 0) + 1; };
  for (const e of document.querySelectorAll('body *')) {
    const r = e.getBoundingClientRect(); if (!r.width || !r.height) continue;
    const s = getComputedStyle(e); if (s.visibility === 'hidden' || s.display === 'none') continue;
    const txt = [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
    if (txt) { add('font-size', s.fontSize); add('font-family', s.fontFamily.split(',')[0].replace(/"/g, '')); add('font-weight', s.fontWeight); add('color', s.color); }
    if (s.borderTopLeftRadius !== '0px') add('radius', s.borderTopLeftRadius);
    if (e.matches('button, input, select, .btn')) add('control-height:' + e.tagName.toLowerCase(), Math.round(r.height) + 'px');
    if (e.matches('svg')) add('icon-size', Math.round(r.width) + 'px');
    if (s.backgroundColor !== 'rgba(0, 0, 0, 0)') add('background', s.backgroundColor);
    if (s.boxShadow !== 'none') add('shadow', s.boxShadow.slice(0, 60));
    if (s.transitionDuration !== '0s') add('transition', s.transitionDuration);
  }
  return tally;
}

(async () => {
  const b = await chromium.launch();
  const report = {};
  const shoot = async (name, url, w, h, opts = {}) => {
    const p = await b.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: opts.scale || 1 });
    const errors = []; p.on('pageerror', (e) => errors.push(e.message));
    if (opts.state) await p.route('**/api/state', (r) => r.fulfill({ contentType: 'application/json', body: JSON.stringify(opts.state) }));
    await p.goto(url); await p.waitForTimeout(opts.wait || 1200);
    if (opts.after) { await opts.after(p); await p.waitForTimeout(400); }
    await p.screenshot({ path: `${OUT}/${name}.png`, fullPage: !!opts.full });
    report[name] = { url, errors, styles: await p.evaluate(survey) };
    await p.close();
  };
  const tabsEv = (p) => p.evaluate((t) => window.nfEvent({ type: 'tabs', tabs: t }), TABS);
  await shoot('verrouillage-creation', `http://127.0.0.1:${PL}/shell.html?mode=top&w=2&private=0`, 1280, 820, { wait: 1500 });
  await shoot('verrouillage-code', `http://127.0.0.1:${PL}/shell.html?mode=top&w=2&private=0`, 1280, 820, { wait: 1500,
    state: { ok: true, configured: true, unlocked: false, awaitingRecovery: false, lockStatus: { failures: 0, wait: 0 }, splash: false, verticalTabs: false, windowId: 2, private: false, version: '0.1.4', tagline: 'Browse Beyond the Darkness.', signals: [], lastError: null } });
  await shoot('barre-haut', U('shell.html?mode=top&w=1&private=0'), 1280, 88, { after: tabsEv });
  await shoot('barre-haut-zoom2x', U('shell.html?mode=top&w=1&private=0'), 1280, 88, { after: tabsEv, scale: 2 });
  await shoot('barre-haut-privee', U('shell.html?mode=top&w=1&private=1'), 1280, 88, { after: tabsEv });
  await shoot('onglets-verticaux', U('shell.html?mode=side&w=1&private=0'), 260, 700, { after: tabsEv,
    state: { ok: true, configured: true, unlocked: true, verticalTabs: true, windowId: 1, private: false, version: '0.1.4', signals: [] } });
  for (const [name, panel, w, h] of POPUPS) await shoot('popup-' + name, U(`shell.html?mode=popup&w=1&private=0&panel=${panel}`), w, h, { after: tabsEv });
  for (const [name, page, w, h] of PAGES) await shoot('page-' + name, U(page), w, h, { full: true });
  // Whole window: top bar over the new tab page, as the user sees it.
  const p = await b.newPage({ viewport: { width: 1280, height: 820 } });
  await p.goto(U('about.html'));
  await p.setContent(`<html><body style="margin:0;background:#06080e"><iframe id=t src="${U('shell.html?mode=top&w=1&private=0')}" style="border:0;width:1280px;height:88px;display:block"></iframe><iframe src="${U('newtab.html')}" style="border:0;width:1280px;height:732px;display:block"></iframe></body></html>`);
  await p.waitForTimeout(1500);
  await p.frames()[1].evaluate((t) => window.nfEvent({ type: 'tabs', tabs: t }), TABS);
  await p.waitForTimeout(500);
  await p.screenshot({ path: `${OUT}/fenetre-complete.png` });
  await p.close();
  fs.writeFileSync(`${OUT}/releve-styles.json`, JSON.stringify(report, null, 1));
  await b.close();
})();
