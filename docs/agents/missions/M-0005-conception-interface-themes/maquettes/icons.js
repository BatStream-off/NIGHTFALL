// Jeu d'icônes NIGHTFALL proposé (original) : grille 24, trait 1,6 px à 16 px,
// extrémités et jonctions arrondies, aucun remplissage sauf états « actif ».
// Règles : formes construites sur des cercles et des angles de 45°, coins
// intérieurs de 2 px, l'hexagone de l'emblème réservé aux icônes « NIGHTFALL »
// (Shield, LOCK, pages internes).
const NF_ICONS = {
  back: '<path d="M19 12H5.5M11 6.5 5.5 12l5.5 5.5"/>',
  fwd: '<path d="M5 12h13.5M13 6.5l5.5 5.5-5.5 5.5"/>',
  reload: '<path d="M19 12.8A7 7 0 1 1 16.6 7"/><path d="M17.4 3.6v4.1h-4.1"/>',
  home: '<path d="M4.5 10.8 12 4.6l7.5 6.2"/><path d="M6.6 9.3v9.2c0 .6.4 1 1 1h2.9v-4.8h3v4.8h2.9c.6 0 1-.4 1-1V9.3"/>',
  star: '<path d="M12 4.3l2.4 4.9 5.4.8-3.9 3.8.9 5.4L12 16.7l-4.8 2.5.9-5.4-3.9-3.8 5.4-.8z"/>',
  shield: '<path d="M12 3.4 18.8 6.6v5.2c0 4.1-2.8 7.3-6.8 8.8-4-1.5-6.8-4.7-6.8-8.8V6.6z"/>',
  shieldok: '<path d="M12 3.4 18.8 6.6v5.2c0 4.1-2.8 7.3-6.8 8.8-4-1.5-6.8-4.7-6.8-8.8V6.6z"/><path d="m9.2 12 2 2 3.8-4"/>',
  hex: '<path d="M12 3.8 19.1 8v8L12 20.2 4.9 16V8z"/>',
  lock: '<rect x="5.5" y="10.5" width="13" height="9.5" rx="2.4"/><path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5"/>',
  secure: '<rect x="6.5" y="11" width="11" height="8.5" rx="2"/><path d="M9 11V8.9a3 3 0 0 1 6 0V11"/>',
  key: '<circle cx="8.2" cy="15.8" r="3.7"/><path d="M10.9 13.1 19 5M16.2 7.8l2.3 2.3"/>',
  menu: '<path d="M5 8h14M5 12h14M5 16h14"/>',
  more: '<circle cx="6" cy="12" r=".9"/><circle cx="12" cy="12" r=".9"/><circle cx="18" cy="12" r=".9"/>',
  plus: '<path d="M12 5.5v13M5.5 12h13"/>',
  close: '<path d="m7 7 10 10M17 7 7 17"/>',
  search: '<circle cx="10.8" cy="10.8" r="6"/><path d="m15.3 15.3 4.2 4.2"/>',
  sidebar: '<rect x="3.8" y="5" width="16.4" height="14" rx="2.6"/><path d="M9.5 5v14"/>',
  download: '<path d="M12 4.5v10.5M7.8 10.8 12 15l4.2-4.2M5.5 19.5h13"/>',
  history: '<path d="M5 12.5A7.2 7.2 0 1 0 7 7.3"/><path d="M6.8 3.9v3.8h3.8M12 8.4V12l2.6 1.8"/>',
  bookmarks: '<path d="M7.5 4.5h9c.6 0 1 .4 1 1v14l-5.5-3.6-5.5 3.6v-14c0-.6.4-1 1-1z"/>',
  settings: '<circle cx="12" cy="12" r="2.8"/><path d="M12 3.5v2.2M12 18.3v2.2M3.5 12h2.2M18.3 12h2.2M6 6l1.6 1.6M16.4 16.4 18 18M6 18l1.6-1.6M16.4 7.6 18 6"/>',
  private: '<path d="M18.8 14.6A7.5 7.5 0 0 1 9.4 5.2a7.5 7.5 0 1 0 9.4 9.4z"/>',
  extensions: '<rect x="4.5" y="4.5" width="6.2" height="6.2" rx="1.6"/><rect x="4.5" y="13.3" width="6.2" height="6.2" rx="1.6"/><rect x="13.3" y="13.3" width="6.2" height="6.2" rx="1.6"/><path d="M16.4 3.9l3.1 3.1-3.1 3.1-3.1-3.1z"/>',
  globe: '<circle cx="12" cy="12" r="7.8"/><path d="M4.2 12h15.6M12 4.2c2.3 2.2 3.4 4.8 3.4 7.8s-1.1 5.6-3.4 7.8c-2.3-2.2-3.4-4.8-3.4-7.8S9.7 6.4 12 4.2z"/>',
  sound: '<path d="M4.5 9.5h3l4.5-4v13l-4.5-4h-3z"/><path d="M15.5 9a4.2 4.2 0 0 1 0 6M17.8 6.6a7.6 7.6 0 0 1 0 10.8"/>',
  mute: '<path d="M4.5 9.5h3l4.5-4v13l-4.5-4h-3z"/><path d="m15.5 9.5 5 5M20.5 9.5l-5 5"/>',
  pin: '<path d="M9.5 4.5h5l-.8 5.2 3.3 3.3H7l3.3-3.3zM12 13v6.5"/>',
  palette: '<path d="M12 4a8 8 0 1 0 0 16c1.2 0 1.8-.8 1.5-1.8-.4-1.2.4-2.2 1.6-2.2H17a3 3 0 0 0 3-3c0-5-3.6-9-8-9z"/><circle cx="8" cy="11" r=".9"/><circle cx="11" cy="7.8" r=".9"/><circle cx="15" cy="8.6" r=".9"/>',
  theme: '<circle cx="12" cy="12" r="7.8"/><path d="M12 4.2v15.6a7.8 7.8 0 0 0 0-15.6z" fill="currentColor" stroke="none"/>',
  eye: '<path d="M3.5 12S6.5 6.5 12 6.5 20.5 12 20.5 12 17.5 17.5 12 17.5 3.5 12 3.5 12z"/><circle cx="12" cy="12" r="2.6"/>',
  copy: '<rect x="8.5" y="8.5" width="11" height="11" rx="2.2"/><path d="M15.5 8.5V6.7c0-1.2-1-2.2-2.2-2.2H6.7c-1.2 0-2.2 1-2.2 2.2v6.6c0 1.2 1 2.2 2.2 2.2h1.8"/>',
  trash: '<path d="M5 7h14M10 4.5h4M7 7l.8 11.2c.1 1 .9 1.8 1.9 1.8h4.6c1 0 1.8-.8 1.9-1.8L17 7"/>',
  check: '<path d="m5.5 12.5 4.2 4.2 8.8-9"/>',
  chev: '<path d="m9.5 6.5 5.5 5.5-5.5 5.5"/>',
  chevd: '<path d="m6.5 9.5 5.5 5.5 5.5-5.5"/>',
  video: '<rect x="3.8" y="6" width="16.4" height="12" rx="2.6"/><path d="m10.5 9.5 4 2.5-4 2.5z"/>',
  emergency: '<path d="M8.7 3.8h6.6l4.9 4.9v6.6l-4.9 4.9H8.7l-4.9-4.9V8.7z"/><path d="M12 8v4.6M12 15.9v.1"/>',
  update: '<path d="M12 4.5v9M8 10l4 4 4-4"/><path d="M5 17.5c0 1.1.9 2 2 2h10a2 2 0 0 0 2-2"/>',
  newtab: '<rect x="4" y="4.5" width="16" height="15" rx="3"/><path d="M12 8.8v6.4M8.8 12h6.4"/>',
  window: '<rect x="3.8" y="4.8" width="16.4" height="14.4" rx="2.6"/><path d="M3.8 9h16.4"/>',
  zoom: '<circle cx="10.8" cy="10.8" r="6"/><path d="m15.3 15.3 4.2 4.2M8.3 10.8h5M10.8 8.3v5"/>',
  print: '<path d="M7.5 9V4.5h9V9"/><rect x="4.5" y="9" width="15" height="7" rx="1.8"/><path d="M7.5 13.5h9v6h-9z"/>',
  fullscreen: '<path d="M4.5 9V4.5H9M15 4.5h4.5V9M19.5 15v4.5H15M9 19.5H4.5V15"/>',
  info: '<circle cx="12" cy="12" r="8.2"/><path d="M12 11v5M12 7.9v.1"/>',
  cookie: '<path d="M19.6 12.6A7.8 7.8 0 1 1 11.4 4.4a3 3 0 0 0 3.6 3.6 3 3 0 0 0 4.6 4.6z"/><path d="M9 10h.01M8.5 14.5h.01M13 15.5h.01"/>',
  camera: '<rect x="3.8" y="7" width="12" height="10" rx="2.4"/><path d="m15.8 10.5 4.4-2.5v8l-4.4-2.5"/>',
  split: '<rect x="3.8" y="5" width="16.4" height="14" rx="2.6"/><path d="M12 5v14"/>',
  reopen: '<path d="M5.5 12a6.5 6.5 0 1 0 2-4.7"/><path d="M5.2 4.3v3.9h3.9"/>',
  winmin: '<path d="M7 12h10"/>',
  winmax: '<rect x="7" y="7" width="10" height="10" rx="1.4"/>',
  winrestore: '<rect x="7" y="9" width="8" height="8" rx="1.2"/><path d="M9.5 9V8c0-.6.4-1 1-1h5.5c.6 0 1 .4 1 1v5.5c0 .6-.4 1-1 1H15"/>',
  winclose: '<path d="m7.5 7.5 9 9M16.5 7.5l-9 9"/>',
};
(function () {
  const NS = 'http://www.w3.org/2000/svg';
  const defs = Object.entries(NF_ICONS).map(([k, v]) => `<symbol id="i-${k}" viewBox="0 0 24 24">${v}</symbol>`).join('');
  document.body.insertAdjacentHTML('afterbegin', `<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>${defs}</defs></svg>`);
  for (const e of document.querySelectorAll('[data-i]')) {
    const s = document.createElementNS(NS, 'svg');
    s.setAttribute('class', 'i ' + (e.dataset.size || ''));
    const u = document.createElementNS(NS, 'use'); u.setAttribute('href', '#i-' + e.dataset.i); s.append(u);
    e.replaceWith(s);
  }
})();
