// NIGHTFALL interface view: splash, NIGHTFALL LOCK, tab strip, toolbar.
'use strict';

const params = new URLSearchParams(location.search);
const MODE = params.get('mode') || 'top';
const WIN = Number(params.get('w') || 0);
const PRIVATE = params.get('private') === '1';
const BASE_HEIGHT = () => (document.body.classList.contains('vertical') ? 52 : 88);

let st = null;
let tabs = [];
let activeTab = null;
let panelOpen = null;
let warning = null;

function show(id, on) { const e = document.getElementById(id); if (e) e.hidden = !on; }
function msg(text) { $('#lock-msg').textContent = text || ''; }

// ------------------------------------------------------------------ init
async function init() {
  if (PRIVATE) document.body.classList.add('private');
  st = await api('state');
  if (!st.ok) { msg('Erreur interne : ' + (st.error || '')); show('lock', true); return; }
  document.body.classList.toggle('vertical', !!st.verticalTabs);
  if (MODE === 'side') { show('side', st.unlocked); return; }
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
      el('span', { class: 't-icon', text: t.crashed ? '!' : (t.title || '?').trim().charAt(0).toUpperCase() }),
      el('span', { class: 't-title', text: t.title || 'Nouvel onglet' }),
      el('button', { class: 't-close', title: 'Fermer', text: '×', onclick: (e) => { e.stopPropagation(); api('tab.close', { id: t.id }); } }),
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
  const a = tabs.find((t) => t.active);
  activeTab = a || null;
  if (a) {
    const addr = $('#address');
    if (document.activeElement !== addr) addr.value = a.url.startsWith('nightfall://app/newtab') ? '' : a.url;
    $('#b-back').disabled = !a.can_back;
    $('#b-fwd').disabled = !a.can_forward;
    $('#b-reload').textContent = a.loading ? '✕' : '↻';
    $('#b-reload').title = a.loading ? 'Arrêter' : 'Actualiser';
    $('#shield-count').textContent = a.blocked || 0;
    const icon = $('#sec-icon');
    icon.className = '';
    if (a.url.startsWith('https://')) { icon.textContent = '🔒'; icon.className = 'secure'; icon.title = 'Connexion chiffrée (TLS vérifié par Chromium)'; }
    else if (a.url.startsWith('http://')) { icon.textContent = '⚠'; icon.className = 'insecure'; icon.title = 'Connexion non chiffrée'; }
    else { icon.textContent = '◆'; icon.title = 'Page NIGHTFALL'; }
    setWarning(a.warning);
  }
}

function renderSide() {
  const box = $('#side-tabs');
  box.textContent = '';
  for (const t of tabs) {
    const node = el('div', { class: 'vtab' + (t.active ? ' active' : ''), title: t.url },
      el('span', { class: 't-title', text: (t.pinned ? '📌 ' : '') + (t.title || 'Nouvel onglet') }),
      el('button', { class: 't-close', text: '×', onclick: (e) => { e.stopPropagation(); api('tab.close', { id: t.id }); } }),
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
  b.append(el('span', { text: '⚠ ' + warning }), el('button', { text: 'Quitter ce site', onclick: () => api('nav.back') }));
  b.hidden = false;
  resize();
}

function resize() {
  let h = BASE_HEIGHT();
  if (!$('#banner').hidden) h += 34;
  if (panelOpen) h += 320;
  api('ui.expand', { height: h });
}

async function refreshPageInfo() {
  const info = await api('page.info');
  if (!info.ok) return;
  $('#b-star').textContent = info.bookmarked ? '★' : '☆';
  $('#b-star').classList.toggle('on', info.bookmarked);
  $('#b-shield').classList.toggle('on', info.shieldActive);
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

// ------------------------------------------------------------------ panels
function closePanel() { panelOpen = null; show('panel', false); resize(); }

function openPanel(name, build) {
  if (panelOpen === name) { closePanel(); return; }
  panelOpen = name;
  const p = $('#panel');
  p.textContent = '';
  build(p);
  p.hidden = false;
  resize();
}

function menuPanel(p) {
  const item = (label, fn, cls) => el('button', { class: cls || '', text: label, onclick: () => { closePanel(); fn(); } });
  p.append(el('div', { class: 'panel-head' }, el('h2', { text: 'NIGHTFALL' }), el('button', { class: 'ghost', text: 'Fermer', onclick: closePanel })),
    el('div', { class: 'menu-grid' },
      item('Nouvel onglet', () => api('tab.new')),
      item('Nouvelle fenêtre', () => api('window.new')),
      item('Fenêtre de navigation privée', () => api('window.private')),
      item('Favoris', () => openPage('bookmarks')),
      item('Historique', () => openPage('history')),
      item('Téléchargements', () => openPage('downloads')),
      item('Mots de passe', () => openPage('passwords')),
      item('Extensions', () => openPage('extensions')),
      item('Security Center', () => openPage('security')),
      item('Paramètres', () => openPage('settings')),
      item('À propos', () => openPage('about')),
      item('Verrouiller', () => api('lock')),
      item('EMERGENCY LOCKDOWN', () => api('emergency.lockdown'), 'danger'),
    ));
}

async function shieldPanel(p) {
  const info = await api('page.info');
  const site = info.site || '';
  p.append(el('div', { class: 'panel-head' }, el('h2', { text: 'NIGHTFALL Shield' }), el('button', { class: 'ghost', text: 'Fermer', onclick: closePanel })),
    el('p', { class: 'muted', text: site ? `Site : ${site}` : 'Page interne' }),
    el('p', { text: `${activeTab ? activeTab.blocked : 0} éléments bloqués sur cette page.` }),
    site ? el('button', { class: info.shieldActive ? '' : 'primary', text: info.shieldActive ? 'Désactiver Shield sur ce site' : 'Réactiver Shield sur ce site',
      onclick: async () => { await api('shield.toggleSite'); closePanel(); refreshPageInfo(); } }) : null,
    el('button', { class: 'ghost', text: 'Ouvrir le Security Center', onclick: () => { closePanel(); openPage('security'); } }));
}

async function keyPanel(p) {
  const r = await api('vault.forPage');
  p.append(el('div', { class: 'panel-head' }, el('h2', { text: 'Mots de passe' }), el('button', { class: 'ghost', text: 'Fermer', onclick: closePanel })));
  if (!r.ok || !r.origin) { p.append(el('p', { class: 'muted', text: 'Disponible uniquement sur les sites web.' })); return; }
  p.append(el('p', { class: 'muted', text: `Site : ${r.origin}` }));
  const list = el('ul', { class: 'list' });
  for (const c of r.entries) {
    list.append(el('li', {}, el('div', { class: 'grow' }, el('div', { class: 'title', text: c.username || '(sans identifiant)' })),
      el('button', { class: 'primary', text: 'Remplir', onclick: async () => { check(await api('vault.fill', { id: c.id })); closePanel(); } })));
  }
  if (!r.entries.length) list.append(el('li', { class: 'empty', text: 'Aucun identifiant enregistré pour ce site.' }));
  const user = el('input', { placeholder: 'Identifiant' });
  const pass = el('input', { placeholder: 'Mot de passe', type: 'password' });
  p.append(list, el('h2', { text: 'Enregistrer pour ce site' }), el('div', { class: 'row' }, user, pass,
    el('button', { text: 'Générer', onclick: async () => { const g = await api('vault.generate', {}); if (g.ok) { pass.value = g.password; pass.type = 'text'; } } }),
    el('button', { class: 'primary', text: 'Enregistrer', onclick: async () => {
      if (check(await api('vault.saveForPage', { username: user.value, password: pass.value }), 'Identifiant enregistré (chiffré)')) closePanel();
    } })));
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
$('#b-menu').addEventListener('click', () => openPanel('menu', menuPanel));
$('#b-shield').addEventListener('click', () => openPanel('shield', shieldPanel));
$('#b-key').addEventListener('click', () => openPanel('key', keyPanel));
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
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && panelOpen) closePanel(); });
document.addEventListener('mousedown', () => api('ui.activity'));

// ------------------------------------------------------------------ events from the core
window.nfEvent = (ev) => {
  switch (ev.type) {
    case 'tabs':
      tabs = ev.tabs || [];
      if (MODE === 'side') { show('side', true); renderSide(); } else { renderTabs(); refreshPageInfo(); }
      break;
    case 'unlocked':
      // Unlocked elsewhere (another window): show the browser here too.
      if (MODE === 'top' && document.getElementById('chrome').hidden) showChrome();
      break;
    case 'locked':
    case 'lockdown':
      closePanel();
      if (MODE === 'top') showLock();
      else show('side', false);
      break;
    case 'layout':
      location.reload();
      break;
    case 'warning':
      if (activeTab && ev.tab === activeTab.id) setWarning(ev.message);
      break;
    case 'focus_address':
      if (MODE === 'top') { const a = $('#address'); a.focus(); a.select(); }
      break;
    case 'page_info':
      refreshPageInfo();
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
