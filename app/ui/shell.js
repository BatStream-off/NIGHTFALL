// NIGHTFALL interface view: splash, NIGHTFALL LOCK, tab strip, toolbar,
// vertical tabs (mode=side) and dropdown popups (mode=popup).
'use strict';

const params = new URLSearchParams(location.search);
const MODE = params.get('mode') || 'top';
const WIN = Number(params.get('w') || 0);
const PRIVATE = params.get('private') === '1';
const BASE_HEIGHT = () => (document.body.classList.contains('vertical') ? 52 : 88);

let st = null;
let tabs = [];
let activeTab = null;
let popupOpen = null;      // top view: name of the popup currently shown
let popupOpenedAt = 0;
const MAC = /Mac|iPhone|iPad/i.test(navigator.platform || '');
let warning = null;

function svgIcon(name) {
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  const use = document.createElementNS(NS, 'use');
  use.setAttribute('href', '#i-' + name);
  svg.append(use);
  return svg;
}

function show(id, on) { const e = document.getElementById(id); if (e) e.hidden = !on; }
function msg(text) { $('#lock-msg').textContent = text || ''; }

// ------------------------------------------------------------------ init
async function init() {
  if (PRIVATE) document.body.classList.add('private');
  st = await api('state');
  if (!st.ok) { msg('Erreur interne : ' + (st.error || '')); show('lock', true); return; }
  document.body.classList.toggle('vertical', !!st.verticalTabs);
  if (MODE === 'side') { show('side', st.unlocked); return; }
  if (MODE === 'popup') { if (st.unlocked) openPopupView(); else api('popup.close'); return; }
  if (!st.unlocked) {
    if (st.splash && WIN === 1 && !sessionStorage.getItem('nfSplash')) {
      sessionStorage.setItem('nfSplash', '1');
      await splash();
    }
    showLock();
  } else {
    showChrome();
  }
}

function splash() {
  return new Promise((resolve) => {
    show('splash', true);
    setTimeout(() => { show('splash', false); resolve(); }, 1800);
  });
}

// ------------------------------------------------------------------ lock
function setupKeypad() {
  const kp = $('#keypad');
  if (kp.childElementCount) return;
  ['1', '2', '3', '4', '5', '6', '7', '8', '9', '⌫', '0', '✓'].forEach((k) => {
    kp.append(el('button', { type: 'button', text: k, onclick: () => {
      const pin = $('#pin');
      if (k === '⌫') pin.value = pin.value.slice(0, -1);
      else if (k === '✓') $('#f-unlock').requestSubmit();
      else if (pin.value.length < 12) pin.value += k;
      pin.focus();
    } }));
  });
}

let countdownTimer = null;
function countdown(seconds) {
  clearInterval(countdownTimer);
  const btn = $('#unlock-btn');
  let left = seconds;
  const tick = () => {
    if (left <= 0) { clearInterval(countdownTimer); btn.disabled = false; msg(''); return; }
    btn.disabled = true;
    msg(`Trop d'essais. Patientez ${left} s.`);
    left--;
  };
  tick();
  countdownTimer = setInterval(tick, 1000);
}

function lockView(view) {
  for (const id of ['f-setup', 'f-recovery-show', 'f-unlock', 'f-recovery-use']) show(id, id === view);
  const focus = { 'f-setup': '#setup-pin', 'f-unlock': '#pin', 'f-recovery-use': '#rc-code', 'f-recovery-show': '#rk-confirm' }[view];
  if (focus) setTimeout(() => $(focus).focus(), 50);
}

async function showLock() {
  show('chrome', false);
  show('lock', true);
  st = await api('state');
  msg(st.lastError || '');
  $('#tagline').textContent = st.tagline || '';
  if (!st.configured) { lockView('f-setup'); return; }
  setupKeypad();
  lockView(st.lockStatus && st.lockStatus.recoveryRequired ? 'f-recovery-use' : 'f-unlock');
  if (st.lockStatus) {
    if (st.lockStatus.wait_seconds > 0) countdown(st.lockStatus.wait_seconds);
    if (!st.lockStatus.hardware_backed) $('#lock-hw').textContent = 'Attention : aucune puce de sécurité détectée, la protection du code est réduite.';
  }
}

$('#f-setup').addEventListener('submit', async (e) => {
  e.preventDefault();
  msg('Création du code…');
  const r = await api('setup', { pin: $('#setup-pin').value, confirm: $('#setup-confirm').value });
  $('#setup-pin').value = ''; $('#setup-confirm').value = '';
  if (!r.ok) { msg(r.error); return; }
  msg('');
  $('#rk').textContent = r.recoveryKey;
  lockView('f-recovery-show');
});

$('#rk-done').addEventListener('click', async () => {
  const first = ($('#rk').textContent || '').slice(0, 4);
  if ($('#rk-confirm').value.trim().toUpperCase() !== first) { msg('Recopiez les 4 premiers caractères pour confirmer.'); return; }
  $('#rk').textContent = '';
  const r = await api('recovery.confirm');
  if (r.ok) showChrome(); else msg(r.error);
});

$('#f-unlock').addEventListener('submit', async (e) => {
  e.preventDefault();
  const pin = $('#pin').value;
  if (!pin) return;
  msg('Vérification…');
  $('#unlock-btn').disabled = true;
  const r = await api('unlock', { pin });
  $('#pin').value = '';
  $('#unlock-btn').disabled = false;
  if (r.ok) { msg(''); showChrome(); return; }
  if (r.recoveryRequired) { msg(r.error); lockView('f-recovery-use'); return; }
  if (r.wait > 0) countdown(r.wait); else msg(r.error + (r.failures ? ` (${r.failures} échec${r.failures > 1 ? 's' : ''})` : ''));
});

$('#use-recovery').addEventListener('click', () => { msg(''); lockView('f-recovery-use'); });
$('#back-unlock').addEventListener('click', () => { msg(''); lockView('f-unlock'); });

$('#f-recovery-use').addEventListener('submit', async (e) => {
  e.preventDefault();
  if ($('#rc-pin').value !== $('#rc-confirm').value) { msg('Les deux codes ne correspondent pas.'); return; }
  msg('Vérification…');
  const r = await api('unlock.recovery', { code: $('#rc-code').value, newPin: $('#rc-pin').value });
  $('#rc-pin').value = ''; $('#rc-confirm').value = '';
  if (r.ok) { $('#rc-code').value = ''; msg(''); showChrome(); } else msg(r.error);
});

// ------------------------------------------------------------------ chrome
// Tooltips with the platform's shortcuts (Ctrl on Windows, ⌘ on macOS).
function shortcutTitles() {
  $('#b-back').title = 'Précédent (' + (MAC ? '⌘[' : 'Alt+←') + ')';
  $('#b-fwd').title = 'Suivant (' + (MAC ? '⌘]' : 'Alt+→') + ')';
  $('#new-tab').title = 'Nouvel onglet (' + shortcut('T') + ')';
  $('#b-lock').title = 'Verrouiller NIGHTFALL (' + shortcut('L', true) + ')';
}

function showChrome() {
  shortcutTitles();
  show('lock', false);
  show('chrome', true);
  show('private-badge', PRIVATE);
  api('tabs');
  refreshPageInfo();
}

// Icon of a tab: spinner while loading, the site's icon, or its initial.
function tabIcon(t) {
  if (t.crashed) return el('span', { class: 't-icon', text: '!' });
  if (t.favicon && !t.loading) return el('img', { class: 't-fav', src: t.favicon, alt: '' });
  if ((t.url || '').startsWith('nightfall://') && !t.loading) return el('img', { class: 't-fav', src: 'emblem.png', alt: '' });
  return el('span', { class: 't-icon', text: (t.title || '?').trim().charAt(0).toUpperCase() });
}

// "Playing sound" / muted button of a tab (click: mute or unmute).
function tabSound(t) {
  if (!t.audible && !t.muted) return null;
  return el('button', { class: 't-sound' + (t.muted ? ' muted' : ''), title: t.muted ? 'Réactiver le son de l\'onglet' : 'Couper le son de l\'onglet', 'aria-label': t.muted ? 'Réactiver le son' : 'Couper le son',
    onmousedown: (e) => e.stopPropagation(), onclick: (e) => { e.stopPropagation(); api('tab.mute', { id: t.id }); } }, svgIcon(t.muted ? 'sound-off' : 'sound'));
}

// Right-click on a tab: its menu, at the mouse position.
function openTabMenu(t, e) {
  const i = tabs.findIndex((x) => x.id === t.id);
  const flags = (t.pinned ? 'p' : '') + (t.muted ? 'm' : '') + (i >= 0 && i < tabs.length - 1 ? 'r' : '') + (tabs.length > 1 ? 'o' : '');
  const panel = 'tabmenu-' + t.id + (flags ? '-' + flags : '');
  popupOpen = panel;
  popupOpenedAt = Date.now();
  api('popup.open', { panel, anchor: { x: e.clientX, y: e.clientY - 6, width: 0, height: 0 }, width: 290, height: 360, alignLeft: true, toggle: false });
}

function renderTabs() {
  const strip = $('#tabs');
  strip.textContent = '';
  for (const t of tabs) {
    const node = el('div', {
      class: 'tab' + (t.active ? ' active' : '') + (t.pinned ? ' pinned' : '') + (t.loading ? ' loading' : '') + (t.discarded ? ' discarded' : '') + (t.warning ? ' warning' : '') + (t.audible || t.muted ? ' sound' : ''),
      title: t.title + (t.discarded ? ' (en veille)' : '') + (t.audible && !t.muted ? ' — lecture du son' : '') + (t.muted ? ' — son coupé' : ''),
      draggable: 'true',
    },
      el('span', { class: 't-bg' }),
      el('span', { class: 't-edge' }),
      tabIcon(t),
      el('span', { class: 't-title', text: t.title || 'Nouvel onglet' }),
      tabSound(t),
      el('button', { class: 't-close', title: 'Fermer l\'onglet (' + shortcut('W') + ')', 'aria-label': 'Fermer l\'onglet', onclick: (e) => { e.stopPropagation(); api('tab.close', { id: t.id }); } }, svgIcon('close')),
    );
    node.addEventListener('mousedown', (e) => {
      if (e.button === 1) { e.preventDefault(); api('tab.close', { id: t.id }); }
      else if (e.button === 0) api('tab.activate', { id: t.id });
    });
    node.addEventListener('dblclick', (e) => e.stopPropagation());
    node.addEventListener('contextmenu', (e) => { e.preventDefault(); openTabMenu(t, e); });
    node.addEventListener('dragstart', (e) => e.dataTransfer.setData('text/nf-tab', String(t.id)));
    node.addEventListener('dragover', (e) => e.preventDefault());
    node.addEventListener('drop', (e) => {
      e.preventDefault();
      const id = Number(e.dataTransfer.getData('text/nf-tab'));
      const index = tabs.findIndex((x) => x.id === t.id);
      if (id && id !== t.id) api('tab.move', { id, index });
    });
    strip.append(node);
  }
  fitTabs();
  const activeNode = strip.querySelector('.tab.active');
  if (activeNode) activeNode.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  const a = tabs.find((t) => t.active);
  activeTab = a || null;
  if (a) {
    const addr = $('#address');
    if (document.activeElement !== addr) addr.value = a.url.startsWith('nightfall://app/newtab') ? '' : a.url;
    $('#b-back').disabled = !a.can_back;
    $('#b-fwd').disabled = !a.can_forward;
    $('#b-reload use').setAttribute('href', a.loading ? '#i-stop' : '#i-reload');
    $('#b-reload').title = a.loading ? 'Arrêter' : 'Actualiser (' + shortcut('R') + ')';
    $('#shield-count').textContent = fmtCount(a.blocked);
    const icon = $('#sec-icon');
    icon.className = '';
    icon.textContent = '';
    if (a.url.startsWith('https://')) { icon.append(svgIcon('secure')); icon.className = 'secure'; icon.title = 'Connexion chiffrée (TLS vérifié par Chromium)'; }
    else if (a.url.startsWith('http://')) { icon.append(svgIcon('insecure')); icon.className = 'insecure'; icon.title = 'Connexion non chiffrée'; }
    else if (a.url.startsWith('nightfall://') || !a.url) { icon.append(svgIcon('internal')); icon.className = 'internal'; icon.title = 'Page NIGHTFALL'; }
    else { icon.append(svgIcon('globe')); icon.className = ''; icon.title = a.url.startsWith('view-source:') ? 'Code source de la page' : 'Page locale'; }
    const zoom = Number(a.zoom) || 100;
    const zb = $('#b-zoom');
    zb.hidden = zoom === 100 && popupOpen !== 'zoom';
    zb.querySelector('use').setAttribute('href', zoom < 100 ? '#i-zoom-out' : '#i-zoom-in');
    zb.title = 'Zoom : ' + zoom + ' %';
    pageBanner(a);
  }
}

// Brave-like: tabs share the strip, the "+" follows the last tab; narrow
// tabs only keep their icon.
function fitTabs() {
  for (const n of document.querySelectorAll('#tabs .tab:not(.pinned)')) {
    const w = n.getBoundingClientRect().width;
    n.classList.toggle('narrow', w < 60);
    n.classList.toggle('compact', w < 140);
  }
}
window.addEventListener('resize', () => { if (MODE === 'top') fitTabs(); });
// Many tabs: the strip scrolls with the mouse wheel.
$('#tabs').addEventListener('wheel', (e) => { const s = $('#tabs'); if (s.scrollWidth > s.clientWidth) { e.preventDefault(); s.scrollLeft += e.deltaY || e.deltaX; } }, { passive: false });

function fmtCount(n) { n = Number(n) || 0; return n > 999 ? Math.floor(n / 1000) + 'k' : String(n); }

function renderSide() {
  const box = $('#side-tabs');
  box.textContent = '';
  for (const t of tabs) {
    const node = el('div', { class: 'vtab' + (t.active ? ' active' : '') + (t.loading ? ' loading' : ''), title: t.url },
      tabIcon(t),
      el('span', { class: 't-title', text: (t.pinned ? '📌 ' : '') + (t.title || 'Nouvel onglet') }),
      tabSound(t),
      el('button', { class: 't-close', title: 'Fermer l\'onglet', 'aria-label': 'Fermer l\'onglet', onclick: (e) => { e.stopPropagation(); api('tab.close', { id: t.id }); } }, svgIcon('close')),
    );
    node.addEventListener('click', () => api('tab.activate', { id: t.id }));
    node.addEventListener('mousedown', (e) => { if (e.button === 1) { e.preventDefault(); api('tab.close', { id: t.id }); } });
    node.addEventListener('contextmenu', (e) => { e.preventDefault(); openTabMenu(t, e); });
    box.append(node);
  }
}

// One banner under the toolbar, by priority: permission prompt, page not
// responding, crashed page, phishing warning.
let bannerKind = null;
const unresponsive = new Set();

function showBanner(kind, danger, ...nodes) {
  const b = $('#banner');
  bannerKind = kind;
  b.className = danger ? 'danger' : '';
  b.textContent = '';
  b.append(...nodes);
  if (b.hidden) { b.hidden = false; resize(); }
}

function hideBanner(kinds) {
  const b = $('#banner');
  if (!bannerKind || !kinds.includes(bannerKind)) return;
  bannerKind = null;
  warning = null;
  if (!b.hidden) { b.hidden = true; resize(); }
}

function pageBanner(a) {
  if (bannerKind === 'permission') return;
  if (unresponsive.has(a.id)) {
    showBanner('unresponsive', true, svgIcon('insecure'), el('span', { text: 'Cette page ne répond pas.' }),
      el('button', { text: 'Attendre', onclick: () => { unresponsive.delete(a.id); api('tab.unresponsive', { id: a.id, action: 'wait' }); hideBanner(['unresponsive']); } }),
      el('button', { text: 'Arrêter la page', onclick: () => { unresponsive.delete(a.id); api('tab.unresponsive', { id: a.id, action: 'terminate' }); hideBanner(['unresponsive']); } }));
  } else if (a.crashed) {
    showBanner('crash', true, svgIcon('insecure'), el('span', { text: 'Cette page a planté : son processus s\'est arrêté.' }),
      el('button', { class: 'primary', text: 'Actualiser', onclick: () => api('tab.reload', { id: a.id }) }));
  } else if (a.warning) {
    setWarning(a.warning);
  } else {
    hideBanner(['warning', 'crash', 'unresponsive']);
  }
}

function setWarning(text) {
  if (!text) { hideBanner(['warning']); return; }
  if (bannerKind === 'permission') return;
  warning = text;
  showBanner('warning', true, svgIcon('insecure'), el('span', { text: warning }), el('button', { text: 'Quitter ce site', onclick: () => api('nav.back') }));
}

// The top view only grows for banners (permission prompt, warning).
// Dropdowns are separate overlay views (popup.*) and never push the page.
function resize() {
  let h = BASE_HEIGHT();
  if (!$('#banner').hidden) h += 34;
  api('ui.expand', { height: h });
}

async function refreshPageInfo() {
  const info = await api('page.info');
  if (!info.ok) return;
  $('#b-star').classList.toggle('on', !!info.bookmarked);
  $('#b-star').title = (info.bookmarked ? 'Retirer des favoris (' : 'Ajouter aux favoris (') + shortcut('D') + ')';
  $('#b-shield').classList.toggle('on', !!info.shieldActive);
  $('#b-shield use').setAttribute('href', info.shieldActive || info.internal ? '#i-shield' : '#i-shield-off');
  $('#b-shield').title = info.internal ? 'NIGHTFALL Shield' : (info.shieldActive ? 'NIGHTFALL Shield : activé pour ce site' : 'NIGHTFALL Shield : désactivé pour ce site');
  $('#b-key').classList.toggle('on', info.passwords > 0);
}

// ------------------------------------------------------------------ permissions
function askPermission(ev) {
  const done = (allow) => { api('permission.answer', { id: ev.id, allow }); hideBanner(['permission']); if (activeTab) pageBanner(activeTab); };
  showBanner('permission', false, el('span', { text: `${host(ev.origin) || ev.origin} ${ev.what}.` }),
    el('button', { class: 'primary', text: 'Autoriser', onclick: () => done(true) }),
    el('button', { text: 'Refuser', onclick: () => done(false) }));
}

// ------------------------------------------------------------------ popups (top view side)
// Size each dropdown opens with; the popup then fits itself (popup.resize).
const POPUP_SIZE = { menu: [340, 640], shield: [320, 330], passwords: [340, 380], zoom: [270, 60], find: [400, 50], tabmenu: [290, 360] };
const POPUP_BUTTON = { menu: '#b-menu', shield: '#b-shield', passwords: '#b-key', zoom: '#b-zoom' };
let pressedOnOpen = null;

function setPressed() {
  for (const [name, sel] of Object.entries(POPUP_BUTTON)) $(sel).classList.toggle('pressed', popupOpen === name);
}

function closePopup() {
  if (!popupOpen) return;
  popupOpen = null;
  setPressed();
  api('popup.close');
}

function togglePopup(name) {
  // A mousedown in this view blurs the popup, which then closes itself:
  // what matters is whether it was open when the press started.
  const wasOpen = pressedOnOpen === name;
  pressedOnOpen = null;
  if (wasOpen) { popupOpen = name; closePopup(); return; }
  const r = $(POPUP_BUTTON[name]).getBoundingClientRect();
  const [width, height] = POPUP_SIZE[name];
  popupOpen = name;
  popupOpenedAt = Date.now();
  setPressed();
  api('popup.open', { panel: name, anchor: { x: r.left, y: r.top, width: r.width, height: r.height }, width, height });
}

for (const [name, sel] of Object.entries(POPUP_BUTTON)) {
  $(sel).addEventListener('mousedown', (e) => { if (e.button === 0) pressedOnOpen = popupOpen === name ? name : null; });
  $(sel).addEventListener('click', () => togglePopup(name));
}

// ------------------------------------------------------------------ popups (popup view)
const loadedAt = Date.now();

function shortcut(key, shift) {
  if (MAC) return (shift ? '⇧' : '') + '⌘' + key;
  return 'Ctrl+' + (shift ? 'Maj+' : '') + key;
}
const FULLSCREEN_KEY = MAC ? '⌃⌘F' : 'F11';

// Run an action from the popup, then close it (the action goes first: the
// popup view is destroyed on close).
async function act(fn) {
  try { await fn(); } finally { api('popup.close'); }
}

function fitPopup() {
  const inner = $('#popup-body').firstElementChild;
  if (!inner) return;
  const kind = popupKind();
  // The suggestions keep the omnibox's width.
  const width = kind === 'suggest' ? window.innerWidth : (POPUP_SIZE[kind] ? POPUP_SIZE[kind][0] : 320);
  const height = Math.min(Math.ceil(inner.getBoundingClientRect().height) + 2, 700);
  api('popup.resize', { width, height });
}

function popupName() { return params.get('panel') || 'menu'; }
// "tabmenu-12-pm" -> "tabmenu"
function popupKind() { return popupName().split('-')[0]; }

async function openPopupView() {
  document.documentElement.classList.add('popup');
  show('popup', true);
  const name = popupName();
  const kind = popupKind();
  const box = $('#popup-body');
  const inner = el('div', { class: 'p-' + kind });
  box.append(inner);
  if (kind === 'shield') await shieldPanel(inner);
  else if (kind === 'passwords') await keyPanel(inner);
  else if (kind === 'find') findPanel(inner);
  else if (kind === 'zoom') zoomPanel(inner);
  else if (kind === 'suggest') await suggestPanel(inner);
  else if (kind === 'tabmenu') tabMenuPanel(inner, name);
  else menuPanel(inner);
  fitPopup();
  new ResizeObserver(() => fitPopup()).observe(inner);
  // The find bar stays open while the page is used (as in Chrome); the
  // suggestions never take the focus (closed by the address field).
  if (kind !== 'find' && kind !== 'suggest') window.addEventListener('blur', () => { if (Date.now() - loadedAt > 300) api('popup.close'); });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') api(kind === 'find' ? 'find.close' : 'popup.close'); });
  api('tabs'); // up-to-date counters (blocked items) for this window
}

// ---- find in page
let findState = null;
function findPanel(p) {
  const input = el('input', { class: 'f-input', placeholder: 'Rechercher dans la page', spellcheck: 'false', autocomplete: 'off', 'aria-label': 'Rechercher dans la page' });
  const count = el('span', { class: 'f-count', 'aria-live': 'polite' });
  const run = (next, back) => {
    const text = input.value;
    if (!text) { count.textContent = ''; input.classList.remove('none'); api('find.stop', { clear: true }); return; }
    api('find.run', { text, forward: !back, next: !!next });
  };
  let timer = null;
  input.addEventListener('input', () => { clearTimeout(timer); timer = setTimeout(() => run(false), 80); });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); run(true, e.shiftKey); }
  });
  const btn = (icon, title, fn) => el('button', { class: 'f-btn', title, 'aria-label': title, onclick: fn }, svgIcon(icon));
  p.append(el('div', { class: 'find-bar' }, svgIcon('search'), input, count,
    el('span', { class: 'f-sep' }),
    btn('up', 'Résultat précédent (Maj+Entrée)', () => run(true, true)),
    btn('down', 'Résultat suivant (Entrée)', () => run(true, false)),
    btn('close', 'Fermer (Échap)', () => api('find.close'))));
  findState = { input, count, run };
  setTimeout(() => input.focus(), 30);
}

function findResult(ev) {
  if (!findState) return;
  const { count, input } = findState;
  if (!input.value) { count.textContent = ''; return; }
  if (ev.count > 0) { count.textContent = `${Math.max(ev.active, 1)} sur ${ev.count}`; input.classList.remove('none'); }
  else if (ev.final) { count.textContent = 'Aucun résultat'; input.classList.add('none'); }
}

// ---- zoom
let zoomLabels = [];
function zoomRow(withReset) {
  const label = el('span', { class: 'z-val', text: activeTab ? (activeTab.zoom || 100) + ' %' : '100 %' });
  zoomLabels.push(label);
  return el('div', { class: 'zoom-row' },
    el('span', { class: 'z-name', text: 'Zoom' }),
    el('button', { class: 'z-btn', title: 'Zoom arrière (' + shortcut('-') + ')', 'aria-label': 'Zoom arrière', onclick: () => api('zoom.out') }, svgIcon('minus')),
    label,
    el('button', { class: 'z-btn', title: 'Zoom avant (' + shortcut('+') + ')', 'aria-label': 'Zoom avant', onclick: () => api('zoom.in') }, svgIcon('plus')),
    withReset ? el('button', { class: 'z-reset', text: 'Réinitialiser', title: 'Taille par défaut (' + shortcut('0') + ')', onclick: () => api('zoom.reset') })
      : el('button', { class: 'z-btn', title: 'Plein écran (' + FULLSCREEN_KEY + ')', 'aria-label': 'Plein écran', onclick: () => act(() => api('window.fullscreen')) }, svgIcon('fullscreen')));
}
function zoomPanel(p) { p.append(zoomRow(true)); }
function updateZoomLabels() {
  for (const l of zoomLabels) l.textContent = (activeTab ? (activeTab.zoom || 100) : 100) + ' %';
}

// ---- address bar suggestions (never focused: keys stay in the omnibox)
let suggestBox = null;
async function suggestPanel(p) {
  suggestBox = p;
  p.setAttribute('role', 'listbox');
  const r = await api('omnibox.state');
  if (r.ok && r.state) renderSuggestions(r.state);
}

const SUGGEST_ICON = { search: 'search', url: 'globe', bookmark: 'star', history: 'history' };
function renderSuggestions(ev) {
  if (!suggestBox) return;
  suggestBox.textContent = '';
  (ev.items || []).forEach((it, i) => {
    const row = el('div', { class: 's-item' + (i === ev.selected ? ' sel' : ''), role: 'option', 'aria-selected': i === ev.selected ? 'true' : 'false' },
      svgIcon(SUGGEST_ICON[it.type] || 'globe'),
      el('span', { class: 's-title', text: it.title }),
      it.type === 'history' || it.type === 'bookmark' ? el('span', { class: 's-url', text: it.url.replace(/^https?:\/\/(www\.)?/, '') }) : null);
    row.addEventListener('mousedown', (e) => { e.preventDefault(); api('omnibox.pick', { url: it.url }); });
    suggestBox.append(row);
  });
}

// ---- tab context menu ("tabmenu-<id>-<flags>")
function tabMenuPanel(p, name) {
  const [, idText, flags = ''] = name.split('-');
  const id = Number(idText);
  const has = (f) => flags.includes(f);
  const item = (icon, label, cmd, key, disabled) => {
    const b = el('button', { class: 'm-item', role: 'menuitem', onclick: () => act(() => api(cmd, { id })) },
      svgIcon(icon), el('span', { class: 'm-label', text: label }), key ? el('span', { class: 'm-key', text: key }) : null);
    if (disabled) b.disabled = true;
    return b;
  };
  const sep = () => el('div', { class: 'm-sep', role: 'separator' });
  p.setAttribute('role', 'menu');
  p.append(
    item('newtab', 'Nouvel onglet à droite', 'tab.newRight'),
    sep(),
    item('reload', 'Actualiser', 'tab.reload', shortcut('R')),
    item('duplicate', 'Dupliquer', 'tab.duplicate'),
    item('pin', has('p') ? 'Détacher' : 'Épingler', has('p') ? 'tab.unpin' : 'tab.pin'),
    item(has('m') ? 'sound' : 'sound-off', has('m') ? 'Réactiver le son de l\'onglet' : 'Couper le son de l\'onglet', 'tab.mute'),
    sep(),
    item('close', 'Fermer l\'onglet', 'tab.close', shortcut('W')),
    item('close', 'Fermer les autres onglets', 'tab.closeOthers', null, !has('o')),
    item('close', 'Fermer les onglets à droite', 'tab.closeRight', null, !has('r')),
    sep(),
    item('reopen', 'Rouvrir l\'onglet fermé', 'tab.reopen', shortcut('T', true), PRIVATE),
  );
}

function menuPanel(p) {
  // cls 'keep': the action replaces this menu by another dropdown itself
  // (closing the menu afterwards would close that one).
  const item = (icon, label, fn, key, cls) => el('button', { class: 'm-item' + (cls && cls !== 'keep' ? ' ' + cls : ''), onclick: () => (cls === 'keep' ? fn() : act(fn)) },
    svgIcon(icon), el('span', { class: 'm-label', text: label }), key ? el('span', { class: 'm-key', text: key }) : null);
  const sep = () => el('div', { class: 'm-sep', role: 'separator' });
  p.setAttribute('role', 'menu');
  p.append(...[
    item('newtab', 'Nouvel onglet', () => api('tab.new'), shortcut('T')),
    item('window', 'Nouvelle fenêtre', () => api('window.new'), shortcut('N')),
    item('private', 'Fenêtre de navigation privée', () => api('window.private'), shortcut('N', true)),
    PRIVATE ? null : item('reopen', 'Rouvrir l\'onglet fermé', () => api('tab.reopen'), shortcut('T', true)),
    sep(),
    zoomRow(false),
    sep(),
    item('search', 'Rechercher dans la page…', () => api('find.open'), shortcut('F'), 'keep'),
    item('print', 'Imprimer…', () => api('page.print'), shortcut('P')),
    item('save', 'Enregistrer la page', () => api('page.save'), shortcut('S')),
    item('fullscreen', 'Plein écran', () => api('window.fullscreen'), FULLSCREEN_KEY),
    sep(),
    item('bookmarks', 'Favoris', () => openPage('bookmarks'), shortcut('O', true)),
    item('history', 'Historique', () => openPage('history'), MAC ? '⌘Y' : shortcut('H')),
    item('download', 'Téléchargements', () => openPage('downloads'), shortcut('J')),
    item('key', 'Mots de passe', () => openPage('passwords')),
    sep(),
    item('extensions', 'Extensions', () => openPage('extensions')),
    item('shield', 'Security Center', () => openPage('security')),
    item('settings', 'Paramètres', () => openPage('settings')),
    sep(),
    item('home', 'Accueil', () => api('nav.home')),
    item('about', 'À propos de NIGHTFALL', () => openPage('about')),
    sep(),
    item('lock', 'Verrouiller NIGHTFALL', () => api('lock'), shortcut('L', true)),
    sep(),
    item('emergency', "Verrouillage d'urgence", () => api('emergency.lockdown'), shortcut('F12', true), 'danger'),
  ].filter(Boolean));
  for (const b of p.querySelectorAll('.m-item')) b.setAttribute('role', 'menuitem');
}

async function shieldPanel(p) {
  const info = await api('page.info');
  const site = (info && info.site) || '';
  const on = !!(info && info.shieldActive);
  const count = el('div', { class: 'num', text: activeTab ? fmtCount(activeTab.blocked) : '–' });
  const toggle = el('input', { type: 'checkbox', class: 'toggle', 'aria-label': 'Shield activé pour ce site' });
  toggle.checked = on;
  toggle.addEventListener('change', () => act(() => api('shield.toggleSite')));
  p.append(
    el('div', { class: 'sh-head' },
      el('span', { class: 'fav', text: site ? site.charAt(0).toUpperCase() : '' }, site ? null : svgIcon('internal')),
      el('div', { class: 'sh-site' },
        el('div', { class: 'sh-name', text: site || 'Page NIGHTFALL' }),
        el('div', { class: 'sh-sub', text: 'NIGHTFALL Shield' }))),
    el('div', { class: 'sh-count' + (on || !site ? '' : ' off') }, svgIcon(on || !site ? 'shield' : 'shield-off'), count,
      el('div', { class: 'cap', text: on || !site ? 'éléments bloqués sur cette page' : 'Shield est désactivé pour ce site' })),
    site ? el('label', { class: 'sh-toggle' },
      el('span', { class: 't' }, el('b', { text: 'Shield activé pour ce site' }),
        el('span', { text: on ? 'Pubs, traqueurs et scripts indésirables bloqués.' : 'Si un site ne fonctionne pas, désactiver Shield peut aider.' })),
      toggle) : el('div', { class: 'p-note', text: 'Les pages internes de NIGHTFALL ne contiennent aucun élément tiers.' }),
    el('div', { class: 'p-note', text: site ? 'La page sera actualisée après un changement.' : '' }),
    el('div', { class: 'm-sep' }),
    el('button', { class: 'm-item', onclick: () => act(() => openPage('security')) },
      svgIcon('shield'), el('span', { class: 'm-label', text: 'Ouvrir le Security Center' }), svgIcon('chevron')),
  );
  // keep the counter live while the panel is open
  shieldCounter = count;
}
let shieldCounter = null;

async function keyPanel(p) {
  const r = await api('vault.forPage');
  const origin = r && r.ok ? r.origin : null;
  p.append(el('div', { class: 'sh-head' },
    el('span', { class: 'fav' }, svgIcon('key')),
    el('div', { class: 'sh-site' }, el('div', { class: 'sh-name', text: 'Mots de passe' }),
      el('div', { class: 'sh-sub', text: origin ? host(origin) : 'Disponible uniquement sur les sites web' }))));
  if (origin) {
    const entries = r.entries || [];
    const list = el('div', { class: 'pw-list' });
    for (const c of entries) {
      const name = c.username || '(sans identifiant)';
      list.append(el('div', { class: 'pw-item' },
        el('span', { class: 'fav', text: name.charAt(0).toUpperCase() }),
        el('span', { class: 'u', text: name, title: name }),
        el('button', { class: 'fill', text: 'Remplir', onclick: () => act(async () => check(await api('vault.fill', { id: c.id }))) })));
    }
    if (!entries.length) list.append(el('div', { class: 'pw-empty', text: 'Aucun identifiant enregistré pour ce site.' }));
    const user = el('input', { placeholder: 'Identifiant', autocomplete: 'off', spellcheck: 'false' });
    const pass = el('input', { placeholder: 'Mot de passe', type: 'password', autocomplete: 'new-password' });
    p.append(list, el('div', { class: 'm-sep' }), el('div', { class: 'p-title', text: 'Enregistrer pour ce site' }),
      el('div', { class: 'pw-form' }, user,
        el('div', { class: 'pw-pass' }, pass,
          el('button', { class: 'gen', type: 'button', title: 'Générer un mot de passe robuste', onclick: async () => { const g = await api('vault.generate', {}); if (g.ok) { pass.value = g.password; pass.type = 'text'; } } },
            svgIcon('generate'), 'Générer')),
        el('div', { class: 'row' }, el('button', { class: 'primary', text: 'Enregistrer', onclick: async () => {
          if (check(await api('vault.saveForPage', { username: user.value, password: pass.value }), 'Identifiant enregistré (chiffré)')) api('popup.close');
        } }))));
  }
  p.append(el('div', { class: 'm-sep' }),
    el('button', { class: 'm-item', onclick: () => act(() => openPage('passwords')) },
      svgIcon('settings'), el('span', { class: 'm-label', text: 'Gérer les mots de passe' }), svgIcon('chevron')));
}

// ------------------------------------------------------------------ toolbar
$('#new-tab').addEventListener('click', () => api('tab.new'));
$('#side-new').addEventListener('click', () => api('tab.new'));
$('#b-back').addEventListener('click', () => api('nav.back'));
$('#b-fwd').addEventListener('click', () => api('nav.forward'));
$('#b-reload').addEventListener('click', () => api(activeTab && activeTab.loading ? 'nav.stop' : 'nav.reload'));
$('#b-home').addEventListener('click', () => api('nav.home'));
$('#b-star').addEventListener('click', async () => { await api('bookmark.toggle'); setTimeout(refreshPageInfo, 150); });
$('#b-lock').addEventListener('click', () => api('lock'));
// ---- address bar suggestions (top view side)
const sugg = { open: false, items: [], selected: -1, typed: '', timer: null, seq: 0 };

function closeSuggest() {
  sugg.items = [];
  sugg.selected = -1;
  if (!sugg.open) return;
  sugg.open = false;
  if (popupOpen === 'suggest') popupOpen = null;
  api('popup.close');
}

function showSuggest() {
  const box = $('#omnibox').getBoundingClientRect();
  const height = sugg.items.length * 38 + 14;
  if (!sugg.open) {
    sugg.open = true;
    popupOpen = 'suggest';
    popupOpenedAt = Date.now();
    setPressed();
    api('popup.open', { panel: 'suggest', anchor: { x: box.left, y: box.top, width: box.width, height: box.height - 2 }, width: Math.round(box.width), height, noActivate: true, alignLeft: true, toggle: false });
  } else {
    api('popup.resize', { width: Math.round(box.width), height });
  }
  api('omnibox.render', { items: sugg.items, selected: sugg.selected });
}

async function updateSuggestions() {
  const addr = $('#address');
  const text = sugg.typed;
  const seq = ++sugg.seq;
  if (!text.trim() || document.activeElement !== addr) { closeSuggest(); return; }
  const r = await api('omnibox.suggest', { text });
  if (seq !== sugg.seq || document.activeElement !== addr) return;
  sugg.items = (r.ok && r.items) || [];
  sugg.selected = sugg.items.length ? 0 : -1;
  if (!sugg.items.length) { closeSuggest(); return; }
  showSuggest();
}

function moveSuggestion(delta) {
  const n = sugg.items.length;
  if (!n) return;
  sugg.selected = (sugg.selected + delta + n) % n;
  const it = sugg.items[sugg.selected];
  $('#address').value = it.type === 'history' || it.type === 'bookmark' ? it.url : sugg.typed;
  api('omnibox.render', { items: sugg.items, selected: sugg.selected });
}

$('#address').addEventListener('input', (e) => {
  sugg.typed = e.target.value;
  clearTimeout(sugg.timer);
  sugg.timer = setTimeout(updateSuggestions, 40);
});

$('#address').addEventListener('keydown', async (e) => {
  if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && sugg.open) {
    e.preventDefault();
    moveSuggestion(e.key === 'ArrowDown' ? 1 : -1);
  } else if (e.key === 'Enter') {
    const it = sugg.open && sugg.selected > 0 ? sugg.items[sugg.selected] : null;
    const input = it ? it.url : e.target.value;
    clearTimeout(sugg.timer);
    sugg.seq++;
    closeSuggest();
    e.target.blur();
    await api('navigate', { input });
  } else if (e.key === 'Escape') {
    if (sugg.open) { e.preventDefault(); e.target.value = sugg.typed; closeSuggest(); return; }
    e.target.value = activeTab ? activeTab.url : '';
    e.target.blur();
  }
});
$('#address').addEventListener('focus', (e) => setTimeout(() => e.target.select(), 0));
$('#address').addEventListener('blur', () => { sugg.seq++; setTimeout(() => { if (document.activeElement !== $('#address')) closeSuggest(); }, 150); });

// Copy text to the clipboard (asked by the native side, see windows::copy_text).
window.nfSelectForCopy = (text) => {
  const old = document.getElementById('nf-copy');
  if (old) old.remove();
  const span = el('span', { id: 'nf-copy', style: 'position:fixed;left:-9999px;top:0;white-space:pre;user-select:text;-webkit-user-select:text', text });
  document.body.append(span);
  const r = document.createRange();
  r.selectNodeContents(span);
  const sel = window.getSelection();
  sel.removeAllRanges();
  sel.addRange(r);
};
window.nfCopyDone = () => {
  const span = document.getElementById('nf-copy');
  if (span) span.remove();
  window.getSelection().removeAllRanges();
};
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && MODE === 'top' && popupOpen) closePopup(); });
document.addEventListener('mousedown', () => api('ui.activity'));

// ------------------------------------------------------------------ events from the core
window.nfEvent = (ev) => {
  switch (ev.type) {
    case 'tabs':
      tabs = ev.tabs || [];
      if (MODE === 'side') { show('side', true); renderSide(); }
      else if (MODE === 'popup') { activeTab = tabs.find((t) => t.active) || null; if (shieldCounter) shieldCounter.textContent = fmtCount(activeTab ? activeTab.blocked : 0); updateZoomLabels(); }
      else { renderTabs(); refreshPageInfo(); }
      break;
    case 'find_result':
      if (MODE === 'popup') findResult(ev);
      break;
    case 'find_focus':
      if (MODE === 'popup' && findState) { findState.input.focus(); findState.input.select(); }
      break;
    case 'find_next':
      if (MODE === 'popup' && findState) findState.run(true, !!ev.back);
      break;
    case 'suggest':
      if (MODE === 'popup') renderSuggestions(ev);
      break;
    case 'omnibox_picked':
      if (MODE === 'top') { sugg.open = false; sugg.seq++; const a = $('#address'); a.value = ev.url || a.value; a.blur(); }
      break;
    case 'unresponsive':
      if (MODE === 'top') {
        if (ev.on) unresponsive.add(ev.tab); else unresponsive.delete(ev.tab);
        if (activeTab) pageBanner(activeTab);
      }
      break;
    case 'unlocked':
      // Unlocked elsewhere (another window): show the browser here too.
      if (MODE === 'top' && document.getElementById('chrome').hidden) showChrome();
      break;
    case 'locked':
    case 'lockdown':
      if (MODE === 'popup') { api('popup.close'); break; }
      closePopup();
      if (MODE === 'top') showLock();
      else show('side', false);
      break;
    case 'layout':
      location.reload();
      break;
    case 'warning':
      if (MODE === 'top' && activeTab && ev.tab === activeTab.id) setWarning(ev.message);
      break;
    case 'focus_address':
      if (MODE === 'top') { const a = $('#address'); a.focus(); a.select(); }
      break;
    case 'page_info':
      if (MODE === 'top') refreshPageInfo();
      break;
    case 'popup_closed':
      // Ignore a late notice for a popup replaced by the one just opened.
      if (MODE === 'top' && ev.panel === 'suggest') sugg.open = false;
      if (MODE === 'top' && (!ev.panel || ev.panel === popupOpen) && (ev.panel || Date.now() - popupOpenedAt > 250)) { popupOpen = null; setPressed(); if (activeTab) $('#b-zoom').hidden = (Number(activeTab.zoom) || 100) === 100; }
      break;
    case 'permission':
      if (MODE === 'top') askPermission(ev);
      break;
    case 'popup_blocked':
      if (MODE === 'top') toast('Pop-up bloquée par NIGHTFALL Shield');
      break;
    case 'shield_updated':
      if (MODE === 'top' && ev.errors && ev.errors.length) toast('Mise à jour des listes : ' + ev.errors.length + ' erreur(s)', true);
      break;
    default:
      break;
  }
};

init();
