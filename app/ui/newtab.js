'use strict';
const WALLS = ['city', 'nebula', 'midnight', 'graphite'];
let data = null;

function tick() {
  const d = new Date();
  $('#clock').textContent = d.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
  $('#date').textContent = d.toLocaleDateString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
}

function tile(title, url, onRemove) {
  const a = el('a', { class: 'tile', href: url, title: url },
    el('span', { class: 'ic', text: (title || host(url) || '?').charAt(0).toUpperCase() }),
    el('span', { class: 'nm', text: title || host(url) }));
  if (onRemove) a.append(el('button', { class: 'rm', title: 'Retirer ce raccourci', 'aria-label': 'Retirer ce raccourci', text: '✕', onclick: (e) => { e.preventDefault(); onRemove(); } }));
  return a;
}

async function saveShortcuts() {
  const s = await api('settings.get');
  if (!s.ok) return;
  s.settings.shortcuts = data.shortcuts;
  s.settings.wallpaper = data.wallpaper;
  await api('settings.set', { settings: s.settings });
}

function render() {
  document.body.dataset.wall = data.wallpaper || 'city';
  const sc = $('#shortcuts');
  sc.textContent = '';
  data.shortcuts.forEach((s, i) => sc.append(tile(s.title, s.url, () => { data.shortcuts.splice(i, 1); saveShortcuts(); render(); })));
  const top = $('#top');
  top.textContent = '';
  for (const t of data.topSites || []) top.append(tile(t.title, t.url));
  $('#top-group').hidden = !(data.topSites || []).length;
  $('#shortcuts-group').hidden = !data.shortcuts.length;
}

async function load() {
  data = await api('newtab.data');
  if (!data.ok) { data = { shortcuts: [], topSites: [], wallpaper: 'city' }; }
  render();
}

$('#search').addEventListener('submit', (e) => {
  e.preventDefault();
  const q = $('#q').value.trim();
  if (q) api('navigate', { input: q });
});
$('#add').addEventListener('click', () => {
  const url = prompt('Adresse du site (https://…)');
  if (!url) return;
  const title = prompt('Nom du raccourci', host(url)) || host(url);
  data.shortcuts.push({ title, url: url.includes('://') ? url : 'https://' + url });
  saveShortcuts();
  render();
});
$('#wall').addEventListener('click', () => {
  data.wallpaper = WALLS[(WALLS.indexOf(data.wallpaper) + 1) % WALLS.length];
  saveShortcuts();
  render();
});
$('#security').addEventListener('click', () => openPage('security'));
$('#settings').addEventListener('click', () => openPage('settings'));
tick();
setInterval(tick, 1000);
load();
