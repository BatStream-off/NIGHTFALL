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
function showChrome() {
  show('lock', false);
  show('chrome', true);
  show('private-badge', PRIVATE);
  api('tabs');
  refreshPageInfo();
}

function renderTabs() {
  const strip = $('#tabs');
  strip.textContent = '';
  for (const t of tabs) {
    const node = el('div', {
      class: 'tab' + (t.active ? ' active' : '') + (t.pinned ? ' pinned' : '') + (t.loading ? ' loading' : '') + (t.discarded ? ' discarded' : '') + (t.warning ? ' warning' : ''),
      title: t.title + (t.discarded ? ' (en veille)' : ''),
      draggable: 'true',
    },
      el('span', { class: 't-bg' }),
      el('span', { class: 't-edge' }),
      el('span', { class: 't-icon', text: t.crashed ? '!' : (t.title || '?').trim().charAt(0).toUpperCase() }),
      el('span', { class: 't-title', text: t.title || 'Nouvel onglet' }),
      el('button', { class: 't-close', title: 'Fermer l\'onglet', 'aria-label': 'Fermer l\'onglet', onclick: (e) => { e.stopPropagation(); api('tab.close', { id: t.id }); } }, svgIcon('close')),
    );
    node.addEventListener('mousedown', (e) => {
      if (e.button === 1) { e.preventDefault(); api('tab.close', { id: t.id }); }
      else if (e.button === 0) api('tab.activate', { id: t.id });
    });
    node.addEventListener('contextmenu', (e) => { e.preventDefault(); api(t.pinned ? 'tab.unpin' : 'tab.pin', { id: t.id }); });
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
  const a = tabs.find((t) => t.active);
  activeTab = a || null;
  if (a) {
    const addr = $('#address');
    if (document.activeElement !== addr) addr.value = a.url.startsWith('nightfall://app/newtab') ? '' : a.url;
    $('#b-back').disabled = !a.can_back;
    $('#b-fwd').disabled = !a.can_forward;
    $('#b-reload use').setAttribute('href', a.loading ? '#i-stop' : '#i-reload');
    $('#b-reload').title = a.loading ? 'Arrêter' : 'Actualiser';
    $('#shield-count').textContent = fmtCount(a.blocked);
    const icon = $('#sec-icon');
    icon.className = '';
    icon.textContent = '';
    if (a.url.startsWith('https://')) { icon.append(svgIcon('secure')); icon.className = 'secure'; icon.title = 'Connexion chiffrée (TLS vérifié par Chromium)'; }
    else if (a.url.startsWith('http://')) { icon.append(svgIcon('insecure')); icon.className = 'insecure'; icon.title = 'Connexion non chiffrée'; }
    else { icon.append(svgIcon('internal')); icon.className = 'internal'; icon.title = 'Page NIGHTFALL'; }
    setWarning(a.warning);
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

function fmtCount(n) { n = Number(n) || 0; return n > 999 ? Math.floor(n / 1000) + 'k' : String(n); }

function renderSide() {
  const box = $('#side-tabs');
  box.textContent = '';
  for (const t of tabs) {
    const node = el('div', { class: 'vtab' + (t.active ? ' active' : ''), title: t.url },
      el('span', { class: 't-title', text: (t.pinned ? '📌 ' : '') + (t.title || 'Nouvel onglet') }),
      el('button', { class: 't-close', title: 'Fermer l\'onglet', 'aria-label': 'Fermer l\'onglet', onclick: (e) => { e.stopPropagation(); api('tab.close', { id: t.id }); } }, svgIcon('close')),
    );
    node.addEventListener('click', () => api('tab.activate', { id: t.id }));
    box.append(node);
  }
}

function setWarning(text) {
  warning = text || null;
  const b = $('#banner');
  if (!warning) { if (!b.hidden) { b.hidden = true; resize(); } return; }
  b.className = 'danger';
  b.textContent = '';
  b.append(svgIcon('insecure'), el('span', { text: warning }), el('button', { text: 'Quitter ce site', onclick: () => api('nav.back') }));
  b.hidden = false;
  resize();
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
  $('#b-star').title = info.bookmarked ? 'Retirer des favoris (Ctrl+D)' : 'Ajouter aux favoris (Ctrl+D)';
  $('#b-shield').classList.toggle('on', !!info.shieldActive);
  $('#b-shield use').setAttribute('href', info.shieldActive || info.internal ? '#i-shield' : '#i-shield-off');
  $('#b-shield').title = info.internal ? 'NIGHTFALL Shield' : (info.shieldActive ? 'NIGHTFALL Shield : activé pour ce site' : 'NIGHTFALL Shield : désactivé pour ce site');
  $('#b-key').classList.toggle('on', info.passwords > 0);
}

// ------------------------------------------------------------------ permissions
function askPermission(ev) {
  const b = $('#banner');
  b.className = '';
  b.textContent = '';
  const done = (allow) => { api('permission.answer', { id: ev.id, allow }); b.hidden = true; resize(); };
  b.append(el('span', { text: `${host(ev.origin) || ev.origin} ${ev.what}.` }),
    el('button', { class: 'primary', text: 'Autoriser', onclick: () => done(true) }),
    el('button', { text: 'Refuser', onclick: () => done(false) }));
  b.hidden = false;
  resize();
}

// ------------------------------------------------------------------ popups (top view side)
// Size each dropdown opens with; the popup then fits itself (popup.resize).
const POPUP_SIZE = { menu: [340, 530], shield: [320, 330], passwords: [340, 380] };
const POPUP_BUTTON = { menu: '#b-menu', shield: '#b-shield', passwords: '#b-key' };
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

// Run an action from the popup, then close it (the action goes first: the
// popup view is destroyed on close).
async function act(fn) {
  try { await fn(); } finally { api('popup.close'); }
}

function fitPopup() {
  const inner = $('#popup-body').firstElementChild;
  if (!inner) return;
  const width = POPUP_SIZE[popupName()] ? POPUP_SIZE[popupName()][0] : 320;
  const height = Math.min(Math.ceil(inner.getBoundingClientRect().height) + 2, 560);
  api('popup.resize', { width, height });
}

function popupName() { return params.get('panel') || 'menu'; }

async function openPopupView() {
  document.documentElement.classList.add('popup');
  show('popup', true);
  const name = popupName();
  const box = $('#popup-body');
  const inner = el('div', { class: 'p-' + name });
  box.append(inner);
  if (name === 'shield') await shieldPanel(inner);
  else if (name === 'passwords') await keyPanel(inner);
  else menuPanel(inner);
  fitPopup();
  new ResizeObserver(() => fitPopup()).observe(inner);
  window.addEventListener('blur', () => { if (Date.now() - loadedAt > 300) api('popup.close'); });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') api('popup.close'); });
  api('tabs'); // up-to-date counters (blocked items) for this window
}

function menuPanel(p) {
  const item = (icon, label, fn, key, cls) => el('button', { class: 'm-item' + (cls ? ' ' + cls : ''), onclick: () => act(fn) },
    svgIcon(icon), el('span', { class: 'm-label', text: label }), key ? el('span', { class: 'm-key', text: key }) : null);
  const sep = () => el('div', { class: 'm-sep', role: 'separator' });
  p.setAttribute('role', 'menu');
  p.append(
    item('newtab', 'Nouvel onglet', () => api('tab.new'), shortcut('T')),
    item('window', 'Nouvelle fenêtre', () => api('window.new'), shortcut('N')),
    item('private', 'Fenêtre de navigation privée', () => api('window.private'), shortcut('N', true)),
    sep(),
    item('bookmarks', 'Favoris', () => openPage('bookmarks')),
    item('history', 'Historique', () => openPage('history'), shortcut('H')),
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
  );
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
$('#address').addEventListener('keydown', async (e) => {
  if (e.key === 'Enter') {
    const input = e.target.value;
    e.target.blur();
    await api('navigate', { input });
  } else if (e.key === 'Escape') {
    e.target.value = activeTab ? activeTab.url : '';
    e.target.blur();
  }
});
$('#address').addEventListener('focus', (e) => setTimeout(() => e.target.select(), 0));
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && MODE === 'top' && popupOpen) closePopup(); });
document.addEventListener('mousedown', () => api('ui.activity'));

// ------------------------------------------------------------------ events from the core
window.nfEvent = (ev) => {
  switch (ev.type) {
    case 'tabs':
      tabs = ev.tabs || [];
      if (MODE === 'side') { show('side', true); renderSide(); }
      else if (MODE === 'popup') { activeTab = tabs.find((t) => t.active) || null; if (shieldCounter) shieldCounter.textContent = fmtCount(activeTab ? activeTab.blocked : 0); }
      else { renderTabs(); refreshPageInfo(); }
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
      if (MODE === 'top' && (!ev.panel || ev.panel === popupOpen) && (ev.panel || Date.now() - popupOpenedAt > 250)) { popupOpen = null; setPressed(); }
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
