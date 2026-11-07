'use strict';

// Original NIGHTFALL wallpapers (wallpapers/*.svg, made by tools/wallpapers.py).
// Unknown or legacy ids (city, nebula, midnight, graphite…) fall back to the first.
const WALLS = [
  { id: 'cretes', name: 'Crêtes' },
  { id: 'lac', name: 'Lac' },
  { id: 'ocean', name: 'Océan' },
  { id: 'foret', name: 'Forêt' },
  { id: 'dunes', name: 'Dunes' },
  { id: 'aube', name: 'Aube' },
  { id: 'graphite', name: 'Graphite' },
  { id: 'aurore', name: 'Aurore' },
  { id: 'hexagone', name: 'Hexagone' },
  { id: 'cite', name: 'Cité' },
  { id: 'horizon', name: 'Horizon' },
];
const DEFAULT_WALL = WALLS[0].id;
const COLORS = ['#0b0e16', '#141821', '#1b2433', '#14303a', '#1d2b22', '#2a1f33', '#33201f', '#3a3a3a', '#d9d4cc', '#e9eef5'];
const MAX_TILES = 16; // including the "add" tile
const POSITIONS = ['tl', 'tc', 'tr', 'ml', 'mc', 'mr', 'bl', 'bc', 'br'];
const POS_NAMES = { tl: 'En haut à gauche', tc: 'En haut au centre', tr: 'En haut à droite', ml: 'Au milieu à gauche', mc: 'Au centre', mr: 'Au milieu à droite', bl: 'En bas à gauche', bc: 'En bas au centre', br: 'En bas à droite' };

let data = { shortcuts: [], topSites: [], wallpaper: DEFAULT_WALL, stats: null };

// ---- Local preferences (layout and display options, hidden top sites)
function pref(key, fallback) {
  try {
    const v = localStorage.getItem('nf.newtab.' + key);
    return v == null ? fallback : JSON.parse(v);
  } catch (e) { return fallback; }
}
function setPref(key, value) {
  try { localStorage.setItem('nf.newtab.' + key, JSON.stringify(value)); } catch (e) { /* storage unavailable */ }
}

// Layout: where each widget sits and its options. Older pages only kept
// three show/hide switches: they become the starting values.
function defaultLayout() {
  return {
    clock: { show: pref('showClock', true), pos: 'tl', format: '24', seconds: false, date: false, size: 's', style: 'normal' },
    tiles: { show: pref('showTiles', true), pos: 'tc', count: 8, size: 'm', shape: 'soft', names: true, topSites: true },
    search: { show: true, pos: 'mc', width: 'm' },
    stats: { show: pref('showStats', true), pos: 'bl', compact: false },
    bg: { dim: 0, blur: 0 },
  };
}
function loadLayout() {
  const base = defaultLayout();
  const saved = pref('layout', null);
  if (!saved || typeof saved !== 'object') return base;
  for (const k of Object.keys(base)) if (saved[k] && typeof saved[k] === 'object') base[k] = { ...base[k], ...saved[k] };
  for (const k of ['clock', 'tiles', 'search', 'stats']) if (!POSITIONS.includes(base[k].pos)) base[k].pos = defaultLayout()[k].pos;
  return base;
}
let layout = loadLayout();
const prefs = { get tiles() { return layout.tiles.show; }, get stats() { return layout.stats.show; }, get clock() { return layout.clock.show; } };
function saveLayout() { setPref('layout', layout); }

// Background: a NIGHTFALL wallpaper id, « color:#rrggbb » or « image »
// (kept in the browser settings, the image in the encrypted database).
function bgKind(w) { return w && w.startsWith('color:') ? 'color' : w === 'image' ? 'image' : 'wall'; }
function wallId(id) { return WALLS.some((w) => w.id === id) ? id : DEFAULT_WALL; }
function wallUrl(id) { return 'wallpapers/' + id + '.svg'; }
function normWallpaper(w) {
  if (typeof w !== 'string') return DEFAULT_WALL;
  if (/^color:#[0-9a-f]{6}$/i.test(w) || w === 'image') return w;
  return wallId(w);
}

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
  const c = layout.clock;
  const now = new Date();
  const opts = { hour: '2-digit', minute: '2-digit', hour12: c.format === '12' };
  if (c.seconds) opts.second = '2-digit';
  $('#clock-time').textContent = now.toLocaleTimeString('fr-FR', opts);
  const date = c.date ? now.toLocaleDateString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' }) : '';
  $('#clock-date').textContent = date ? date.charAt(0).toUpperCase() + date.slice(1) : '';
  $('#clock-date').hidden = !date;
}

// ---- Background (cross-fade between two layers)
let wallLayer = 0;
let wallShown = null;
let customImage = '';
function bgCss(w) {
  if (bgKind(w) === 'color') return { image: 'none', color: w.slice(6) };
  if (bgKind(w) === 'image') return { image: customImage ? 'url("' + customImage + '")' : 'none', color: '#0b0e16' };
  return { image: 'url("' + wallUrl(w) + '")', color: '' };
}
function showWallpaper(id) {
  const key = id + (bgKind(id) === 'image' ? customImage.length : '');
  if (key === wallShown) { applyBgEffects(); return; }
  wallShown = key;
  const layers = [$('#wall-a'), $('#wall-b')];
  const next = layers[wallLayer = 1 - wallLayer];
  const prev = layers[1 - wallLayer];
  const css = bgCss(id);
  const swap = () => {
    if (wallShown !== key) return;
    next.style.backgroundImage = css.image;
    next.style.backgroundColor = css.color;
    next.classList.add('on');
    prev.classList.remove('on');
  };
  if (bgKind(id) === 'wall') {
    const img = new Image();
    img.onload = swap;
    img.onerror = swap;
    img.src = wallUrl(id);
  } else swap();
  applyBgEffects();
}
function applyBgEffects() {
  document.body.style.setProperty('--dim', String((Number(layout.bg.dim) || 0) / 100));
  const blur = Number(layout.bg.blur) || 0;
  for (const l of [$('#wall-a'), $('#wall-b')]) l.style.filter = blur ? 'blur(' + blur + 'px)' : '';
}

// ---- Shortcut tiles
// Every tile shares one quiet surface: the initial is enough to recognise
// a site, and a colour per site turned the row into a rainbow.
function initial(title, url) {
  const src = (title || host(url) || '?').replace(/^www\./i, '').trim();
  const ch = Array.from(src)[0] || '?';
  return ch.toLocaleUpperCase('fr-FR');
}

const X_ICON = '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="m1.5 1.5 7 7m0-7-7 7"/></svg>';

function tileEl(item, onRemove) {
  const name = item.title || host(item.url).replace(/^www\./, '');
  const sq = el('span', { class: 'sq' }, el('span', { class: 'ini', text: initial(item.title, item.url) }));
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

function renderTiles() {
  const list = $('#tiles');
  const t = layout.tiles;
  list.textContent = '';
  list.classList.toggle('no-names', !t.names);
  list.classList.toggle('square', t.shape === 'square');
  list.classList.toggle('round', t.shape === 'round');
  document.body.style.setProperty('--tile', { s: '56px', m: '72px', l: '88px' }[t.size] || '72px');
  list.style.setProperty('--per-row', String(Math.min(8, Math.max(4, Number(t.count) || 8))));
  if (!t.show) return;
  const hidden = pref('hiddenTopSites', []);
  const items = data.shortcuts.map((s, i) => ({ ...s, kind: 'shortcut', index: i }));
  if (t.topSites) {
    for (const top of data.topSites || []) {
      if (!top || !top.url || hidden.includes(top.url)) continue;
      if (items.some((x) => sameSite(x.url, top.url))) continue;
      items.push({ ...top, kind: 'top' });
    }
  }
  const max = Math.min(MAX_TILES, Math.max(4, Number(t.count) || 8)) - 1;
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

// ---- Persisting (shortcuts + background live in the browser settings)
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
  const h = form.offsetHeight;
  const left = Math.min(window.innerWidth - w - 16, Math.max(16, r.left + r.width / 2 - w / 2));
  const below = r.bottom + 10 + h < window.innerHeight;
  form.style.left = left + 'px';
  form.style.top = (below ? r.bottom + 10 : Math.max(16, r.top - h - 10)) + 'px';
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

// ---- Placing the widgets on the nine places
function placeWidgets() {
  for (const k of ['clock', 'tiles', 'search', 'stats']) {
    const w = document.querySelector(`.widget[data-w="${k}"]`);
    const cell = document.querySelector(`.cell[data-pos="${layout[k].pos}"]`);
    w.hidden = !layout[k].show;
    if (w.parentElement !== cell) cell.append(w);
  }
  // A fixed order inside a place: clock, search, shortcuts, statistics.
  for (const cell of document.querySelectorAll('.cell')) {
    const order = ['clock', 'search', 'tiles', 'stats'];
    [...cell.children].sort((a, b) => order.indexOf(a.dataset.w) - order.indexOf(b.dataset.w)).forEach((n) => cell.append(n));
  }
}

function applyPrefs() {
  const c = layout.clock;
  document.body.style.setProperty('--clock-size', { s: '26px', m: '48px', l: '84px', xl: '120px' }[c.size] || '26px');
  $('#clock').className = 'clock style-' + (c.style || 'normal');
  $('#stats').classList.toggle('compact', !!layout.stats.compact);
  document.body.style.setProperty('--search-w', { s: 'min(440px, 90vw)', m: 'min(620px, 90vw)', l: 'min(820px, 90vw)' }[layout.search.width] || 'min(620px, 90vw)');
  placeWidgets();
  tick();
  renderTiles();
}

// ---- Customize panel
function toggle(label, value, onChange) {
  const cb = el('input', { type: 'checkbox', class: 'tg', 'aria-label': label });
  cb.checked = !!value;
  cb.addEventListener('change', () => onChange(cb.checked));
  return el('label', { class: 'row' }, el('span', { text: label }), cb);
}
function selectRow(label, options, value, onChange) {
  const s = el('select', { 'aria-label': label }, options.map(([v, t]) => el('option', { value: v, text: t })));
  s.value = String(value);
  s.addEventListener('change', () => onChange(s.value));
  return el('label', { class: 'row' }, el('span', { text: label }), s);
}
function changed() { saveLayout(); applyPrefs(); }

function widgetOptions(key, title, extra) {
  const L = layout[key];
  const more = el('div', { class: 'more' });
  const expand = el('button', { type: 'button', class: 'ghost', text: 'Options', 'aria-expanded': 'false', style: 'height:30px;font-size:13px;padding:0 10px' });
  expand.addEventListener('click', () => { more.hidden = !more.hidden; expand.setAttribute('aria-expanded', String(!more.hidden)); });
  more.hidden = true;
  const pos = el('select', { 'aria-label': 'Position : ' + title }, POSITIONS.map((p) => el('option', { value: p, text: POS_NAMES[p] })));
  pos.value = L.pos;
  pos.addEventListener('change', () => { L.pos = pos.value; changed(); });
  const show = el('input', { type: 'checkbox', class: 'tg', 'aria-label': 'Afficher : ' + title });
  show.checked = L.show;
  show.addEventListener('change', () => { L.show = show.checked; changed(); });
  more.append(el('label', { class: 'row' }, el('span', { text: 'Position' }), pos), ...extra(L));
  return el('div', { class: 'wopt' }, el('div', { class: 'head' }, el('span', { class: 'grow', text: title }), expand, show), more);
}

function renderCustom() {
  // Background kind
  const kind = bgKind(data.wallpaper);
  for (const b of $('#bg-kind').children) b.setAttribute('aria-pressed', String(b.dataset.kind === kind));
  $('#bg-wall').hidden = kind !== 'wall';
  $('#bg-color').hidden = kind !== 'color';
  $('#bg-image').hidden = kind !== 'image';
  const box = $('#walls');
  box.textContent = '';
  for (const w of WALLS) {
    box.append(el('button', { type: 'button', 'data-id': w.id, 'aria-pressed': String(w.id === data.wallpaper), onclick: () => setWallpaper(w.id) },
      el('span', { class: 'tick' }, el('img', { src: wallUrl(w.id), alt: '', loading: 'lazy' })),
      el('span', { text: w.name })));
  }
  const sw = $('#swatches');
  sw.textContent = '';
  const cur = kind === 'color' ? data.wallpaper.slice(6).toLowerCase() : '';
  for (const c of COLORS) sw.append(el('button', { type: 'button', title: c, 'aria-label': 'Couleur ' + c, 'aria-pressed': String(c === cur), style: 'background:' + c, onclick: () => setWallpaper('color:' + c) }));
  const picker = el('input', { type: 'color', value: cur || '#1b2433', title: 'Autre couleur', 'aria-label': 'Autre couleur' });
  picker.addEventListener('input', () => setWallpaper('color:' + picker.value, true));
  picker.addEventListener('change', () => setWallpaper('color:' + picker.value));
  sw.append(picker);
  $('#img-preview').hidden = !customImage;
  if (customImage) $('#img-preview').src = customImage;
  $('#img-remove').hidden = !customImage;
  $('#bg-dim').value = layout.bg.dim; $('#bg-dim-v').textContent = layout.bg.dim + ' %';
  $('#bg-blur').value = layout.bg.blur; $('#bg-blur-v').textContent = layout.bg.blur + ' px';

  // Widgets
  const w = $('#wopts');
  w.textContent = '';
  w.append(
    widgetOptions('clock', 'Horloge', (L) => [
      selectRow('Format', [['24', '24 heures'], ['12', '12 heures']], L.format, (v) => { L.format = v; changed(); }),
      selectRow('Taille', [['s', 'Petite'], ['m', 'Moyenne'], ['l', 'Grande'], ['xl', 'Très grande']], L.size, (v) => { L.size = v; changed(); }),
      selectRow('Style', [['normal', 'Normal'], ['light', 'Fin'], ['bold', 'Gras']], L.style, (v) => { L.style = v; changed(); }),
      toggle('Secondes', L.seconds, (v) => { L.seconds = v; changed(); }),
      toggle('Date', L.date, (v) => { L.date = v; changed(); }),
    ]),
    widgetOptions('search', 'Recherche', (L) => [
      selectRow('Largeur', [['s', 'Étroite'], ['m', 'Normale'], ['l', 'Large']], L.width, (v) => { L.width = v; changed(); }),
    ]),
    widgetOptions('tiles', 'Raccourcis', (L) => [
      selectRow('Nombre', [[4, '4'], [6, '6'], [8, '8'], [12, '12 (2 rangées)'], [16, '16 (2 rangées)']], L.count, (v) => { L.count = Number(v); changed(); }),
      selectRow('Taille', [['s', 'Petits'], ['m', 'Moyens'], ['l', 'Grands']], L.size, (v) => { L.size = v; changed(); }),
      selectRow('Forme', [['soft', 'Arrondie'], ['square', 'Carrée'], ['round', 'Ronde']], L.shape, (v) => { L.shape = v; changed(); }),
      toggle('Noms des sites', L.names, (v) => { L.names = v; changed(); }),
      toggle('Sites les plus visités', L.topSites, (v) => { L.topSites = v; changed(); }),
    ]),
    widgetOptions('stats', 'Statistiques', (L) => [
      toggle('Version compacte', L.compact, (v) => { L.compact = v; changed(); }),
    ]),
  );
}

function setWallpaper(id, preview) {
  data.wallpaper = normWallpaper(id);
  showWallpaper(data.wallpaper);
  if (!preview) { renderCustom(); save(); }
}

$('#bg-kind').addEventListener('click', (e) => {
  const b = e.target.closest('button');
  if (!b) return;
  const k = b.dataset.kind;
  if (k === 'wall') setWallpaper(bgKind(data.wallpaper) === 'wall' ? data.wallpaper : DEFAULT_WALL);
  else if (k === 'color') setWallpaper(bgKind(data.wallpaper) === 'color' ? data.wallpaper : 'color:' + COLORS[2]);
  else if (customImage) setWallpaper('image');
  else { for (const x of $('#bg-kind').children) x.setAttribute('aria-pressed', String(x === b)); $('#bg-wall').hidden = true; $('#bg-color').hidden = true; $('#bg-image').hidden = false; }
});
$('#bg-dim').addEventListener('input', (e) => { layout.bg.dim = Number(e.target.value); $('#bg-dim-v').textContent = layout.bg.dim + ' %'; saveLayout(); applyBgEffects(); });
$('#bg-blur').addEventListener('input', (e) => { layout.bg.blur = Number(e.target.value); $('#bg-blur-v').textContent = layout.bg.blur + ' px'; saveLayout(); applyBgEffects(); });

// The user's image: resized in the page (2560 px wide at most, JPEG) and
// kept in NIGHTFALL's encrypted database, never sent anywhere.
$('#img-choose').addEventListener('click', () => $('#img-file').click());
$('#img-file').addEventListener('change', async (e) => {
  const file = e.target.files && e.target.files[0];
  e.target.value = '';
  if (!file) return;
  if (!/^image\/(jpeg|png|webp)$/.test(file.type)) { toast('Choisissez une image JPEG, PNG ou WebP', true); return; }
  try {
    const bmp = await createImageBitmap(file);
    const scale = Math.min(1, 2560 / bmp.width, 1600 / bmp.height);
    const c = document.createElement('canvas');
    c.width = Math.round(bmp.width * scale);
    c.height = Math.round(bmp.height * scale);
    c.getContext('2d').drawImage(bmp, 0, 0, c.width, c.height);
    let url = c.toDataURL('image/jpeg', 0.86);
    if (url.length > 2.9 * 1024 * 1024) url = c.toDataURL('image/jpeg', 0.7);
    const r = await api('newtab.image.set', { data: url });
    if (!r || !r.ok) { toast((r && r.error) || 'Image refusée', true); return; }
    customImage = url;
    setWallpaper('image');
  } catch (err) { toast('Impossible de lire cette image', true); }
});
$('#img-remove').addEventListener('click', async () => {
  await api('newtab.image.clear');
  customImage = '';
  setWallpaper(DEFAULT_WALL);
});

$('#gear').addEventListener('click', () => {
  const pop = $('#custom');
  if (openPop && openPop.node === pop) { closePop(true); return; }
  renderCustom();
  showPop(pop, $('#gear'));
  const cur = $('#custom [aria-pressed="true"]');
  if (cur) cur.focus();
});
$('#layout-reset').addEventListener('click', () => {
  const fresh = defaultLayout();
  for (const k of ['clock', 'tiles', 'search', 'stats']) { fresh[k].show = true; }
  fresh.bg = layout.bg;
  layout = fresh;
  changed();
  renderCustom();
});
$('#go-security').addEventListener('click', () => { closePop(false); openPage('security'); });
$('#go-settings').addEventListener('click', () => { closePop(false); openPage('settings'); });

// ---- Arranging: drag a widget onto one of the nine places
function setArranging(on) {
  document.body.classList.toggle('arranging', on);
  for (const w of document.querySelectorAll('.widget')) w.draggable = on;
  if (on) closePop(false);
}
$('#arrange').addEventListener('click', () => setArranging(true));
$('#arrange-done').addEventListener('click', () => setArranging(false));
let dragged = null;
document.addEventListener('dragstart', (e) => {
  const w = e.target.closest && e.target.closest('.widget');
  if (!w || !document.body.classList.contains('arranging')) return;
  dragged = w.dataset.w;
  w.classList.add('dragging');
  document.body.classList.add('dragging-widget');
  e.dataTransfer.effectAllowed = 'move';
  e.dataTransfer.setData('text/plain', dragged);
});
document.addEventListener('dragend', () => {
  for (const w of document.querySelectorAll('.widget.dragging')) w.classList.remove('dragging');
  for (const z of document.querySelectorAll('.drop-zones .over')) z.classList.remove('over');
  document.body.classList.remove('dragging-widget');
  dragged = null;
});
for (const z of document.querySelectorAll('.drop-zones div')) {
  z.addEventListener('dragover', (e) => { if (dragged) { e.preventDefault(); z.classList.add('over'); } });
  z.addEventListener('dragleave', () => z.classList.remove('over'));
  z.addEventListener('drop', (e) => {
    e.preventDefault();
    if (!dragged) return;
    layout[dragged].pos = z.dataset.pos;
    changed();
  });
}

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && document.body.classList.contains('arranging')) { e.preventDefault(); setArranging(false); return; }
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
      wallpaper: normWallpaper(r.wallpaper),
      stats: r.stats || null,
    };
  }
  if (bgKind(data.wallpaper) === 'image') {
    const im = await api('newtab.image.get');
    customImage = (im && im.ok && im.data) || '';
    if (!customImage) data.wallpaper = DEFAULT_WALL;
  }
  showWallpaper(data.wallpaper);
  renderStats();
  applyPrefs();
}

window.addEventListener('resize', () => placeAddForm());

applyPrefs();
setInterval(tick, 1000);
load();
