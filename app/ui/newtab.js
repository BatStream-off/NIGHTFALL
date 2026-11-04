'use strict';

// Original NIGHTFALL wallpapers (wallpapers/*.svg). Unknown or legacy ids
// (city, nebula, midnight, graphite…) fall back to the first one.
const WALLS = [
  { id: 'aurore', name: 'Aurore' },
  { id: 'hexagone', name: 'Hexagone' },
  { id: 'cite', name: 'Cité' },
  { id: 'horizon', name: 'Horizon' },
];
const DEFAULT_WALL = WALLS[0].id;
const TILE_SLOT = 98; // tile width + gap, in px
const MAX_TILES = 8;  // including the "add" tile

let data = { shortcuts: [], topSites: [], wallpaper: DEFAULT_WALL, stats: null };

// ---- Local preferences (display toggles, hidden top sites)
function pref(key, fallback) {
  try {
    const v = localStorage.getItem('nf.newtab.' + key);
    return v == null ? fallback : JSON.parse(v);
  } catch (e) { return fallback; }
}
function setPref(key, value) {
  try { localStorage.setItem('nf.newtab.' + key, JSON.stringify(value)); } catch (e) { /* storage unavailable */ }
}
const prefs = {
  tiles: pref('showTiles', true),
  stats: pref('showStats', true),
  clock: pref('showClock', true),
};

function wallId(id) { return WALLS.some((w) => w.id === id) ? id : DEFAULT_WALL; }
function wallUrl(id) { return 'wallpapers/' + id + '.svg'; }

// ---- Formatting (French)
const nf0 = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 0 });
const nf1 = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 });
const plural = (n, one, many) => (n >= 2 ? many : one);

function fmtSize(bytes) {
  const mo = Math.max(0, Number(bytes) || 0) / (1024 * 1024);
  if (mo >= 1024) return [nf1.format(mo / 1024), 'Go'];
  return [mo >= 100 || mo === 0 ? nf0.format(mo) : nf1.format(mo), 'Mo'];
}

function fmtDuration(seconds) {
  const s = Math.max(0, Math.round(Number(seconds) || 0));
  if (s < 60) return [nf0.format(s), plural(s, 'seconde', 'secondes')];
  if (s < 3600) { const m = Math.floor(s / 60); return [nf0.format(m), plural(m, 'minute', 'minutes')]; }
  const h = s / 3600;
  const shown = h >= 10 ? Math.round(h) : Math.round(h * 10) / 10;
  return [nf1.format(shown), plural(shown, 'heure', 'heures')];
}

// ---- Clock
function tick() {
  $('#clock').textContent = new Date().toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
}

// ---- Wallpaper (cross-fade between two layers)
let wallLayer = 0;
let wallShown = null;
function showWallpaper(id) {
  if (id === wallShown) return;
  wallShown = id;
  const layers = [$('#wall-a'), $('#wall-b')];
  const next = layers[wallLayer = 1 - wallLayer];
  const prev = layers[1 - wallLayer];
  const img = new Image();
  const swap = () => {
    if (wallShown !== id) return;
    next.style.backgroundImage = 'url("' + wallUrl(id) + '")';
    next.classList.add('on');
    prev.classList.remove('on');
  };
  img.onload = swap;
  img.onerror = swap;
  img.src = wallUrl(id);
  const w = WALLS.find((x) => x.id === id);
  $('#credit').textContent = 'Fond : NIGHTFALL · ' + w.name;
}

// ---- Shortcut tiles
const HUES = [
  ['#2f8fe8', '#1f5fc8'], ['#2ab7c9', '#1a7f9a'], ['#6c5ce7', '#4a3bb8'], ['#3a77e8', '#2a4fb0'],
  ['#1fae8a', '#14806a'], ['#d9586e', '#a33a53'], ['#e08a3c', '#b4632a'], ['#8a5ce0', '#5f3db0'],
];
function tint(s) {
  let h = 0;
  for (const c of s) h = (h * 31 + c.codePointAt(0)) >>> 0;
  const [a, b] = HUES[h % HUES.length];
  return 'linear-gradient(145deg, ' + a + ', ' + b + ')';
}

function initial(title, url) {
  const src = (title || host(url) || '?').replace(/^www\./i, '').trim();
  const ch = Array.from(src)[0] || '?';
  return ch.toLocaleUpperCase('fr-FR');
}

const X_ICON = '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="m1.5 1.5 7 7m0-7-7 7"/></svg>';

function tileEl(item, onRemove) {
  const name = item.title || host(item.url).replace(/^www\./, '');
  const sq = el('span', { class: 'sq' }, el('span', { class: 'ini', text: initial(item.title, item.url) }));
  sq.firstChild.style.background = tint(host(item.url) || name);
  const li = el('li', { class: 'tile' },
    el('a', { href: item.url, title: name + '\n' + item.url }, sq, el('span', { class: 'nm', text: name })));
  const rm = el('button', { class: 'rm', type: 'button', title: 'Retirer', 'aria-label': 'Retirer ' + name, onclick: (e) => { e.preventDefault(); onRemove(); } });
  rm.innerHTML = X_ICON;
  li.append(rm);
  return li;
}

function sameSite(a, b) {
  return host(a).replace(/^www\./, '') === host(b).replace(/^www\./, '');
}

function slots() {
  // One row only: what fits between the clock and the settings button.
  const room = Math.max(0, window.innerWidth - 2 * 190);
  return Math.max(3, Math.min(MAX_TILES, Math.floor(room / TILE_SLOT)));
}

function renderTiles() {
  const list = $('#tiles');
  list.textContent = '';
  list.hidden = !prefs.tiles;
  if (!prefs.tiles) return;
  const hidden = pref('hiddenTopSites', []);
  const items = data.shortcuts.map((s, i) => ({ ...s, kind: 'shortcut', index: i }));
  for (const t of data.topSites || []) {
    if (!t || !t.url || hidden.includes(t.url)) continue;
    if (items.some((x) => sameSite(x.url, t.url))) continue;
    items.push({ ...t, kind: 'top' });
  }
  const max = slots() - 1;
  for (const item of items.slice(0, max)) {
    list.append(tileEl(item, () => {
      if (item.kind === 'shortcut') {
        data.shortcuts.splice(item.index, 1);
        save();
      } else {
        setPref('hiddenTopSites', [...pref('hiddenTopSites', []), item.url].slice(-50));
      }
      renderTiles();
      const next = $('#tiles .tile a') || $('#add-tile');
      if (next) next.focus();
    }));
  }
  const addIc = '<svg class="add-ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>';
  const sq = el('span', { class: 'sq' });
  sq.innerHTML = addIc;
  const add = el('button', { class: 'add', type: 'button', id: 'add-tile', 'aria-haspopup': 'dialog', 'aria-expanded': 'false', onclick: () => toggleAddForm() },
    sq, el('span', { class: 'nm', text: 'Ajouter' }));
  list.append(el('li', { class: 'tile' }, add));
}

// ---- Persisting (shortcuts + wallpaper live in the browser settings)
async function save() {
  const s = await api('settings.get');
  if (!s || !s.ok || !s.settings) return;
  s.settings.shortcuts = data.shortcuts;
  s.settings.wallpaper = data.wallpaper;
  const r = await api('settings.set', { settings: s.settings });
  if (!r || !r.ok) toast((r && r.error) || 'Impossible d’enregistrer', true);
}

// ---- Popovers
let openPop = null;
function closePop(restoreFocus) {
  if (!openPop) return;
  const { node, trigger } = openPop;
  node.hidden = true;
  if (trigger) trigger.setAttribute('aria-expanded', 'false');
  openPop = null;
  if (restoreFocus && trigger && document.body.contains(trigger)) trigger.focus();
}
function showPop(node, trigger) {
  closePop(false);
  node.hidden = false;
  trigger.setAttribute('aria-expanded', 'true');
  openPop = { node, trigger };
}

function placeAddForm() {
  const form = $('#addform');
  const t = $('#add-tile');
  if (!t || form.hidden) return;
  const r = t.getBoundingClientRect();
  const w = form.offsetWidth;
  const left = Math.min(window.innerWidth - w - 16, Math.max(16, r.left + r.width / 2 - w / 2));
  form.style.left = left + 'px';
  form.style.top = (r.bottom + 10) + 'px';
}

function toggleAddForm() {
  const form = $('#addform');
  if (openPop && openPop.node === form) { closePop(true); return; }
  form.reset();
  $('#add-err').hidden = true;
  showPop(form, $('#add-tile'));
  placeAddForm();
  $('#add-name').focus();
}

function normalizeUrl(raw) {
  let v = raw.trim();
  if (!v) return null;
  if (!/^[a-z][a-z0-9+.-]*:/i.test(v)) v = 'https://' + v;
  try {
    const u = new URL(v);
    if (!['http:', 'https:'].includes(u.protocol) || !u.hostname.includes('.') && u.hostname !== 'localhost') return null;
    return u.href;
  } catch (e) { return null; }
}

$('#addform').addEventListener('submit', (e) => {
  e.preventDefault();
  const url = normalizeUrl($('#add-url').value);
  const err = $('#add-err');
  if (!url) {
    err.textContent = 'Saisissez une adresse valide, par exemple exemple.fr';
    err.hidden = false;
    $('#add-url').focus();
    return;
  }
  const title = $('#add-name').value.trim() || host(url).replace(/^www\./, '');
  data.shortcuts.push({ title, url });
  // A site added by hand should show up even if it was hidden as a top site.
  setPref('hiddenTopSites', pref('hiddenTopSites', []).filter((u) => !sameSite(u, url)));
  save();
  closePop(false);
  renderTiles();
  const add = $('#add-tile');
  if (add) add.focus();
});
$('#add-cancel').addEventListener('click', () => closePop(true));

function renderCustom() {
  const box = $('#walls');
  box.textContent = '';
  for (const w of WALLS) {
    const b = el('button', { type: 'button', 'data-id': w.id, 'aria-pressed': String(w.id === data.wallpaper), onclick: () => setWallpaper(w.id) },
      el('span', { class: 'tick' }, el('img', { src: wallUrl(w.id), alt: '', loading: 'lazy' })),
      el('span', { text: w.name }));
    box.append(b);
  }
  $('#opt-tiles').checked = prefs.tiles;
  $('#opt-stats').checked = prefs.stats;
  $('#opt-clock').checked = prefs.clock;
}

function setWallpaper(id) {
  data.wallpaper = wallId(id);
  showWallpaper(data.wallpaper);
  for (const b of $('#walls').children) b.setAttribute('aria-pressed', String(b.dataset.id === data.wallpaper));
  save();
}

$('#gear').addEventListener('click', () => {
  const pop = $('#custom');
  if (openPop && openPop.node === pop) { closePop(true); return; }
  renderCustom();
  showPop(pop, $('#gear'));
  const cur = $('#walls button[aria-pressed="true"]');
  if (cur) cur.focus();
});

for (const [id, key, storeKey] of [['#opt-tiles', 'tiles', 'showTiles'], ['#opt-stats', 'stats', 'showStats'], ['#opt-clock', 'clock', 'showClock']]) {
  $(id).addEventListener('change', (e) => {
    prefs[key] = e.target.checked;
    setPref(storeKey, prefs[key]);
    applyPrefs();
  });
}
$('#go-security').addEventListener('click', () => { closePop(false); openPage('security'); });
$('#go-settings').addEventListener('click', () => { closePop(false); openPage('settings'); });

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && openPop) { e.preventDefault(); closePop(true); }
});
document.addEventListener('mousedown', (e) => {
  if (!openPop) return;
  if (openPop.node.contains(e.target) || openPop.trigger.contains(e.target)) return;
  closePop(false);
});

// ---- Stats
function renderStats() {
  const st = data.stats || {};
  const blocked = Math.max(0, Math.round(Number(st.blocked) || 0));
  $('#st-blocked').textContent = nf0.format(blocked);
  const [size, sizeUnit] = fmtSize(st.bytesSaved);
  $('#st-bytes').replaceChildren(size, el('small', { text: sizeUnit }));
  const [time, timeUnit] = fmtDuration(st.secondsSaved);
  $('#st-time').replaceChildren(time, el('small', { text: timeUnit }));
}

function applyPrefs() {
  $('#stats').hidden = !prefs.stats;
  $('#clock').style.visibility = prefs.clock ? '' : 'hidden';
  renderTiles();
}

// ---- Search
$('#search').addEventListener('submit', (e) => {
  e.preventDefault();
  const q = $('#q').value.trim();
  if (q) api('navigate', { input: q });
});

// ---- Startup
async function load() {
  const r = await api('newtab.data');
  if (r && r.ok) {
    data = {
      shortcuts: Array.isArray(r.shortcuts) ? r.shortcuts.filter((s) => s && s.url) : [],
      topSites: Array.isArray(r.topSites) ? r.topSites : [],
      wallpaper: wallId(r.wallpaper),
      stats: r.stats || null,
    };
  }
  showWallpaper(data.wallpaper);
  renderStats();
  applyPrefs();
}

let resizeTimer = 0;
let lastSlots = slots();
window.addEventListener('resize', () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    placeAddForm();
    const n = slots();
    if (n !== lastSlots) { lastSlots = n; if (!openPop) renderTiles(); }
  }, 100);
});

tick();
setInterval(tick, 1000);
load();
