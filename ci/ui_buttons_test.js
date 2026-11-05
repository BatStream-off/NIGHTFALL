// Mission M-0002: inventory of every button, menu entry and control of the
// interface pages, each one activated alone in a freshly loaded page, in
// headless Chromium against the mock API (ci/ui_mock_server.py).
//
// For each element: the API commands it sends, JavaScript errors, dialogs
// (confirm/prompt), navigation and whether the page changed. The native
// side is not here (no real tabs, windows or popups): ci/diag_runner.py
// clicks the toolbar and the dropdowns of the real browser.
//
// Prints one "BOUTON ..." line per element and a summary. Exit code 0 unless
// the test itself cannot run (it inventories, it does not judge).
const { chromium } = require('playwright');

const UNL = 'http://127.0.0.1:' + (process.env.NF_PORT_UNLOCKED || 8766);
const LCK = 'http://127.0.0.1:' + (process.env.NF_PORT_LOCKED || 8765);
const SEL = 'button, a[href], input[type=checkbox], input[type=radio], select, .clickable, [role=menuitem]';
const TAB = (id, active) => ({ id, title: 'Onglet ' + id, url: 'https://example.com/' + id, loading: false, pinned: false, active, discarded: false, crashed: false, can_back: true, can_forward: false, warning: null, blocked: 3, favicon: null, audible: false, muted: false, zoom: 100 });
const TABS = [TAB(1, true), TAB(2, false), TAB(3, false)];
const POP = (panel) => 'shell.html?mode=popup&w=1&private=0&panel=' + panel;

const PAGES = [
  { name: 'barre', base: UNL, path: 'shell.html?mode=top&w=1&private=0', vp: [1280, 88], tabs: true, scope: '#chrome' },
  { name: 'menu', base: UNL, path: POP('menu'), vp: [340, 720], scope: '#popup' },
  { name: 'menu-onglet', base: UNL, path: POP('tabmenu-2-ro'), vp: [290, 380], scope: '#popup' },
  { name: 'shield', base: UNL, path: POP('shield'), vp: [320, 330], scope: '#popup' },
  { name: 'mots-de-passe-menu', base: UNL, path: POP('passwords'), vp: [340, 380], scope: '#popup' },
  { name: 'zoom', base: UNL, path: POP('zoom'), vp: [270, 60], scope: '#popup' },
  { name: 'recherche', base: UNL, path: POP('find'), vp: [400, 50], scope: '#popup' },
  { name: 'nouvel-onglet', base: UNL, path: 'newtab.html', vp: [1280, 760] },
  { name: 'security', base: UNL, path: 'security.html', vp: [1280, 760] },
  { name: 'historique', base: UNL, path: 'history.html', vp: [1280, 760] },
  { name: 'favoris', base: UNL, path: 'bookmarks.html', vp: [1280, 760] },
  { name: 'telechargements', base: UNL, path: 'downloads.html', vp: [1280, 760] },
  { name: 'mots-de-passe', base: UNL, path: 'passwords.html', vp: [1280, 760] },
  { name: 'extensions', base: UNL, path: 'extensions.html', vp: [1280, 760] },
  { name: 'a-propos', base: UNL, path: 'about.html', vp: [1280, 760] },
  { name: 'avertissement-http', base: UNL, path: 'insecure.html?url=' + encodeURIComponent('http://example.com/'), vp: [1280, 760] },
];

async function open(browser, pg, log) {
  const p = await browser.newPage({ viewport: { width: pg.vp[0], height: pg.vp[1] } });
  p.on('pageerror', (e) => log.errors.push(e.message.slice(0, 120)));
  p.on('console', (m) => { if (m.type() === 'error') log.errors.push(m.text().slice(0, 120)); });
  p.on('dialog', async (d) => { log.dialogs.push(d.type()); await d.dismiss().catch(() => {}); });
  p.on('popup', () => log.popups++);
  await p.route('**/api/**', (route) => { log.api.push(route.request().url().split('/api/')[1]); route.continue(); });
  await p.goto(pg.base + '/' + pg.path);
  await p.waitForTimeout(pg.wait || 700);
  if (pg.tabs) {
    await p.evaluate((tabs) => window.nfEvent({ type: 'tabs', private: false, tabs }), TABS);
    await p.waitForTimeout(150);
  }
  return p;
}

function describe(e) {
  const lab = (e.querySelector && e.querySelector('.m-label')) || e;
  const text = (e.getAttribute('aria-label') || e.title || (lab.textContent || '').trim() || e.value || '').replace(/\s+/g, ' ').slice(0, 50);
  const cs = getComputedStyle(e);
  const r = e.getBoundingClientRect();
  return { text, tag: e.tagName.toLowerCase() + (e.type ? ':' + e.type : ''), visible: r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none', disabled: !!e.disabled };
}

// "a, b" scope: the elements inside a or b, in document order.
function scoped(scope) {
  return scope ? scope.split(',').map((s) => s.trim() + ' :is(' + SEL + ')').join(', ') : SEL;
}

async function list(p, scope) {
  return p.evaluate(({ sel, fn }) => {
    const describe = new Function('return ' + fn)();
    return [...document.querySelectorAll(sel)].map(describe);
  }, { sel: scoped(scope), fn: describe.toString() });
}

// Every activation starts from the mock's initial state.
async function resetMock() {
  for (const base of [UNL, LCK]) await fetch(base + '/api/__reset', { method: 'POST', body: '{}' }).catch(() => {});
}

async function activate(browser, pg, i, expected) {
  await resetMock();
  const log = { api: [], errors: [], dialogs: [], popups: 0 };
  const p = await open(browser, pg, log);
  const before = { api: log.api.length, url: p.url(), html: await p.evaluate(() => document.body.innerHTML.length + ':' + document.body.innerText.length) };
  const loc = p.locator(scoped(pg.scope)).nth(i);
  let state = 'clique';
  try {
    const d = await loc.evaluate((e, fn) => new Function('return ' + fn)()(e), describe.toString());
    if (d.text !== expected.text) state = 'introuvable';
    else if (!d.visible) state = 'masque';
    else if (d.disabled) state = 'desactive';
    if (state === 'clique') {
      if (d.tag.startsWith('select')) {
        const values = await loc.evaluate((s) => [...s.options].map((o) => o.value));
        const cur = await loc.inputValue();
        const next = values.find((v) => v !== cur);
        if (next !== undefined) await loc.selectOption(next); else state = 'une_seule_option';
      } else {
        await loc.click({ timeout: 2000 });
      }
    }
  } catch (e) {
    state = 'erreur_clic';
    log.errors.push(String(e.message || e).split('\n')[0].slice(0, 100));
  }
  await p.waitForTimeout(700);
  const after = { url: p.url(), html: await p.evaluate(() => document.body.innerHTML.length + ':' + document.body.innerText.length).catch(() => '?') };
  const api = log.api.slice(before.api).filter((c) => !['ui.activity', 'popup.resize', 'tabs', 'update.status', 'state'].includes(c));
  await p.close();
  return {
    state,
    api: [...new Set(api)].join(',') || '-',
    dialogs: log.dialogs.join(',') || '-',
    errors: log.errors.length,
    errorText: log.errors[0] || '',
    nav: after.url !== before.url ? 'oui' : 'non',
    dom: after.html !== before.html ? 'modifie' : 'inchange',
    popups: log.popups,
  };
}

(async () => {
  const browser = await chromium.launch();
  const pages = PAGES.slice();
  // Settings: one entry per section (settings.html#<id>).
  {
    const log = { api: [], errors: [], dialogs: [], popups: 0 };
    const p = await open(browser, { base: UNL, path: 'settings.html', vp: [1280, 760] }, log);
    const ids = await p.evaluate(() => [...document.querySelectorAll('#nav button')].map((b) => b.dataset.id));
    await p.close();
    ids.forEach((id, n) => pages.push({ name: 'parametres#' + id, base: UNL, path: 'settings.html#' + id, vp: [1280, 760], scope: (n === 0 ? '.st-side, .st-top, ' : '') + '.st-section[data-id="' + id + '"]' }));
  }
  // Lock screen last: its buttons change the mock's state.
  pages.push({ name: 'verrouillage', base: LCK, path: 'shell.html?mode=top&w=1&private=0', vp: [1280, 820], wait: 2600, scope: '#lock' });
  const totals = {};
  let count = 0;
  for (const pg of pages) {
    await resetMock();
    const log = { api: [], errors: [], dialogs: [], popups: 0 };
    const p = await open(browser, pg, log);
    const items = await list(p, pg.scope);
    await p.close();
    console.log(`PAGE ${pg.name} elements=${items.length} erreurs_au_chargement=${log.errors.length}${log.errors.length ? ' (' + log.errors[0] + ')' : ''}`);
    for (let i = 0; i < items.length; i++) {
      const r = await activate(browser, pg, i, items[i]);
      count++;
      totals[r.state] = (totals[r.state] || 0) + 1;
      const silent = r.state === 'clique' && r.api === '-' && r.dom === 'inchange' && r.nav === 'non' && r.dialogs === '-' && r.popups === 0;
      if (silent) totals.sans_effet_visible = (totals.sans_effet_visible || 0) + 1;
      if (r.errors) totals.avec_erreur_js = (totals.avec_erreur_js || 0) + 1;
      if (r.dialogs !== '-') totals.avec_dialogue = (totals.avec_dialogue || 0) + 1;
      console.log(`BOUTON page=${pg.name} n=${i} type=${items[i].tag} libelle="${items[i].text}" etat=${r.state} api=${r.api} dialogue=${r.dialogs} navigation=${r.nav} page=${r.dom} fenetres=${r.popups} erreurs_js=${r.errors}${r.errorText ? ' (' + r.errorText + ')' : ''}${silent ? ' SANS_EFFET_VISIBLE' : ''}`);
    }
  }
  console.log(`RESUME boutons elements=${count} ` + Object.entries(totals).map(([k, v]) => `${k}=${v}`).join(' '));
  await browser.close();
})().catch((e) => { console.error('ui_buttons_test : ' + e); process.exit(1); });
