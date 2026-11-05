'use strict';
// NIGHTFALL settings: sections in the sidebar, live search, grouped rows.
// Every option shown here is applied by the browser (see api.rs settings.*);
// what Chromium refuses is reported on the row concerned.

let S = null; // browser settings (state.rs Settings)
let P = null; // preferences readable before unlock (UiPrefs)
let INFO = {}; // the rest of settings.get

const WALLS = [['aurore', 'Aurore'], ['hexagone', 'Hexagone'], ['cite', 'Cité'], ['horizon', 'Horizon']];
const ZOOMS = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
const FONTS = [[9, 'Très petite'], [12, 'Petite'], [16, 'Moyenne (recommandé)'], [20, 'Grande'], [24, 'Très grande']];
const DNS = [
  ['system', 'Automatique (fournisseur du système)'],
  ['off', 'Désactivé'],
  ['cloudflare', 'Cloudflare (1.1.1.1)'],
  ['quad9', 'Quad9 (9.9.9.9)'],
  ['mullvad', 'Mullvad DNS'],
  ['nextdns', 'NextDNS (votre configuration)'],
  ['custom', 'Personnalisé (adresse DoH)'],
];
const RANGES = [['hour', 'Dernière heure'], ['day', 'Dernières 24 heures'], ['week', '7 derniers jours'], ['month', '4 dernières semaines'], ['all', 'Depuis toujours']];
const LANGS = [
  ['fr-FR', 'Français (France)'], ['fr', 'Français'], ['fr-BE', 'Français (Belgique)'], ['fr-CA', 'Français (Canada)'], ['fr-CH', 'Français (Suisse)'],
  ['en-US', 'Anglais (États-Unis)'], ['en-GB', 'Anglais (Royaume-Uni)'], ['en', 'Anglais'], ['de-DE', 'Allemand (Allemagne)'], ['de', 'Allemand'],
  ['es-ES', 'Espagnol (Espagne)'], ['es', 'Espagnol'], ['it-IT', 'Italien'], ['it', 'Italien (générique)'], ['nl-NL', 'Néerlandais (Pays-Bas)'], ['nl', 'Néerlandais'],
  ['pt-PT', 'Portugais (Portugal)'], ['pt-BR', 'Portugais (Brésil)'], ['ar', 'Arabe'], ['zh-CN', 'Chinois (simplifié)'], ['ja', 'Japonais'], ['ko', 'Coréen'],
  ['ru', 'Russe'], ['uk', 'Ukrainien'], ['pl', 'Polonais'], ['ro', 'Roumain'], ['tr', 'Turc'], ['sv', 'Suédois'], ['el', 'Grec'], ['he', 'Hébreu'],
];
const KIND_NAMES = {
  camera: ['Caméra', 'Visioconférence, photo de profil, lecture de QR codes'],
  microphone: ['Micro', 'Appels audio et vidéo, dictée'],
  geolocation: ['Position', 'Cartes, météo, magasins à proximité'],
  notifications: ['Notifications', 'Messages envoyés par les sites, même onglet fermé'],
  clipboard: ['Presse-papiers', 'Lecture de ce que vous avez copié'],
  downloads: ['Téléchargements multiples', 'Plusieurs fichiers téléchargés à la suite'],
  window_management: ['Gestion des fenêtres', 'Ouverture et placement de fenêtres sur vos écrans'],
  local_fonts: ['Polices installées', 'Liste des polices de votre ordinateur'],
};

// ------------------------------------------------------------------ helpers
const norm = (t) => String(t || '').normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
const icon = (id, cls) => {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('aria-hidden', 'true');
  if (cls) svg.setAttribute('class', cls);
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', '#' + id);
  svg.append(use);
  return svg;
};

let noteEl = null;
let noteTimer = 0;
function note(msg, isError) {
  if (!noteEl) { noteEl = el('div', { class: 'toast', role: 'status' }); document.body.append(noteEl); }
  noteEl.className = 'toast' + (isError ? ' error' : '');
  noteEl.textContent = msg;
  noteEl.hidden = false;
  clearTimeout(noteTimer);
  noteTimer = setTimeout(() => { noteEl.hidden = true; }, 3000);
}

async function save(msg) {
  const r = await api('settings.set', { settings: S, prefs: P });
  if (!r || !r.ok) { note((r && r.error) || 'Enregistrement impossible', true); return false; }
  if (Array.isArray(r.chromium)) { INFO.chromium = r.chromium; markPrefs(); }
  const refused = (r.chromium || []).filter((o) => !o.ok);
  note(msg || (refused.length ? 'Enregistré (certains réglages refusés par Chromium : voir ci-dessous)' : 'Paramètre enregistré'), refused.length > 0 && !msg);
  return true;
}

let rowSeq = 0;
/** A settings row: label and description on the left, control on the right. */
function row(o) {
  const id = 'r' + (++rowSeq);
  const txt = el('div', { class: 'txt' },
    el('div', { class: 'lbl', id: id + '-l', text: o.label }),
    o.desc ? el('div', { class: 'desc', id: id + '-d', text: o.desc }) : null,
    o.extra || null,
    el('div', { class: 'warn-line', hidden: true }));
  const r = el('div', { class: 'srow' + (o.stack ? ' stack' : '') + (o.onclick ? ' clickable' : '') + (o.cls ? ' ' + o.cls : '') }, txt);
  if (o.ctl) r.append(el('div', { class: 'ctl' }, o.ctl));
  if (o.below) r.append(o.below);
  if (o.onclick) {
    r.append(icon(o.chev || 's-open', 'chev'));
    r.tabIndex = 0;
    r.setAttribute('role', 'button');
    r.setAttribute('aria-labelledby', id + '-l');
    r.addEventListener('click', o.onclick);
    r.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); o.onclick(); } });
  }
  // Controls are named by the row's label for screen readers.
  for (const c of r.querySelectorAll('.ctl input, .ctl select')) {
    if (!c.hasAttribute('aria-label')) c.setAttribute('aria-labelledby', id + '-l');
    if (o.desc) c.setAttribute('aria-describedby', id + '-d');
  }
  r.dataset.search = norm([o.label, o.desc, o.keywords].join(' '));
  if (o.prefs) r.dataset.prefs = o.prefs.join(' ');
  return r;
}

function group(title, ...rows) {
  return el('div', { class: 'st-group' }, title ? el('h2', { text: title }) : null, el('div', { class: 'st-box' }, rows.flat().filter(Boolean)));
}

function sw(get, set, opts) {
  const input = el('input', { type: 'checkbox', class: 'sw', role: 'switch' });
  input.checked = !!get();
  if (opts && opts.disabled) input.disabled = true;
  input.addEventListener('change', async () => {
    set(input.checked);
    if (!(await save())) { set(!input.checked); input.checked = !input.checked; }
    if (opts && opts.after) opts.after(input.checked);
  });
  return input;
}

function toggle(label, desc, get, set, opts) {
  return row(Object.assign({ label, desc, ctl: sw(get, set, opts) }, opts || {}));
}

function select(options, value, onChange) {
  const s = el('select', {}, options.map(([v, t]) => el('option', { value: String(v), text: t })));
  s.value = String(value);
  s.addEventListener('change', () => onChange(s.value));
  return s;
}

function button(text, onclick, cls) {
  return el('button', { type: 'button', class: cls || '', text, onclick });
}

function iconButton(id, label, onclick, cls) {
  const b = el('button', { type: 'button', class: 'icon' + (cls ? ' ' + cls : ''), title: label, 'aria-label': label, onclick });
  b.append(icon(id));
  return b;
}

function openSection(page, hash) { return api('page.open', hash ? { page, hash } : { page }); }

/** Rows whose Chromium preference was refused show why. */
function markPrefs() {
  const refused = {};
  for (const o of INFO.chromium || []) if (!o.ok) refused[o.name] = o.error;
  for (const r of document.querySelectorAll('.srow[data-prefs]')) {
    const msgs = r.dataset.prefs.split(' ').filter((n) => refused[n]).map((n) => refused[n]);
    const w = r.querySelector('.warn-line');
    w.hidden = !msgs.length;
    w.textContent = msgs.length ? 'Non appliqué : ' + [...new Set(msgs)].join(' ; ') + '.' : '';
  }
}

// ------------------------------------------------------------------ dialogs
const dlg = $('#dlg');
function openDialog(title, body, actions) {
  dlg.textContent = '';
  dlg.append(el('div', { class: 'dh' }, el('h2', { id: 'dlg-title', text: title })), el('div', { class: 'db' }, body), el('div', { class: 'df' }, actions));
  dlg.showModal();
  const first = dlg.querySelector('select, input, button.primary, button');
  if (first) first.focus();
}
function closeDialog() { if (dlg.open) dlg.close(); }

function clearDataDialog() {
  const range = select(RANGES, 'hour', () => {});
  range.id = 'clear-range';
  const box = (id, label, desc, checked) => el('label', { class: 'check' }, el('input', { type: 'checkbox', id, checked }), el('span', {}, label, el('div', { class: 'desc', text: desc })));
  const body = el('div', {},
    el('label', { for: 'clear-range', text: 'Période' }), range,
    box('c-history', 'Historique de navigation', 'Les pages visitées pendant la période.', true),
    box('c-downloads', 'Historique des téléchargements', 'La liste seulement : les fichiers téléchargés restent sur votre ordinateur.', true),
    box('c-cookies', 'Cookies et données des sites', 'Cookies créés pendant la période et données des sites visités pendant la période (vous serez déconnecté de ces sites).', true),
    box('c-cache', 'Images et fichiers en cache', 'Le cache est vidé entièrement, quelle que soit la période (Chromium ne permet pas de le vider par date).', true));
  const go = button('Effacer les données', async () => {
    const pick = (id) => $('#' + id).checked;
    const req = { range: range.value, history: pick('c-history'), downloads: pick('c-downloads'), cookies: pick('c-cookies'), cache: pick('c-cache') };
    if (!req.history && !req.downloads && !req.cookies && !req.cache) { note('Choisissez au moins un type de données', true); return; }
    go.disabled = true;
    const r = await api('browsing.clear', req);
    go.disabled = false;
    if (!r || !r.ok) { note((r && r.error) || 'Effacement impossible', true); return; }
    closeDialog();
    const parts = [];
    if (req.history) parts.push(`${r.history} page(s) de l'historique`);
    if (req.downloads) parts.push(`${r.downloads} téléchargement(s) de la liste`);
    if (req.cookies) parts.push(`les cookies et les données de ${r.sites} site(s)`);
    if (req.cache) parts.push('le cache');
    note('Effacé : ' + parts.join(', ') + '.');
  }, 'primary');
  openDialog('Effacer les données de navigation', body, [button('Annuler', closeDialog), go]);
}

function confirmDialog(title, text, action, label, cls) {
  const go = button(label, async () => { go.disabled = true; await action(); go.disabled = false; closeDialog(); }, cls || 'primary');
  openDialog(title, el('p', { class: 'muted', text }), [button('Annuler', closeDialog), go]);
}

// ------------------------------------------------------------------ sections
function sStartup() {
  const pagesBox = el('div', {});
  const list = el('ul', { class: 'slist' });
  const add = el('input', { type: 'url', placeholder: 'https://exemple.fr', 'aria-label': 'Adresse de la page à ajouter' });
  const renderPages = () => {
    list.textContent = '';
    if (!S.startup_pages.length) list.append(el('li', { class: 'empty', text: 'Aucune page : ajoutez-en au moins une, sinon la page Nouvel onglet s\'ouvre.' }));
    S.startup_pages.forEach((u, i) => list.append(el('li', {}, el('span', { class: 'grow', text: u }),
      iconButton('s-x', 'Retirer ' + u, async () => { S.startup_pages.splice(i, 1); await save(); renderPages(); }))));
  };
  const addPage = async () => {
    let u = add.value.trim();
    if (!u) return;
    if (!/^https?:\/\//i.test(u)) u = 'https://' + u;
    try { new URL(u); } catch (e) { note('Adresse invalide', true); return; }
    S.startup_pages.push(u);
    add.value = '';
    await save('Page ajoutée');
    const g = await api('settings.get');
    if (g.ok) S.startup_pages = g.settings.startup_pages;
    renderPages();
  };
  add.addEventListener('keydown', (e) => { if (e.key === 'Enter') addPage(); });
  pagesBox.append(list, el('div', { class: 'inline-form', style: 'padding:10px 18px 14px' }, add, button('Ajouter', addPage)));
  renderPages();

  const mode = S.startup_mode || (S.restore_session ? 'restore' : 'newtab');
  const radio = (value, label, desc) => {
    const input = el('input', { type: 'radio', name: 'startup', value, checked: mode === value });
    input.addEventListener('change', async () => {
      if (!input.checked) return;
      S.startup_mode = value;
      S.restore_session = value === 'restore';
      pagesBox.hidden = value !== 'pages';
      await save();
    });
    const r = el('label', { class: 'radio' }, input, el('div', {}, el('div', { class: 'lbl', text: label }), desc ? el('div', { class: 'desc', text: desc }) : null));
    r.dataset.search = norm(label + ' ' + (desc || '') + ' démarrage lancement');
    return r;
  };
  pagesBox.hidden = mode !== 'pages';
  return [group('Au démarrage, après le déverrouillage',
    radio('newtab', 'Ouvrir la page Nouvel onglet'),
    radio('restore', 'Reprendre là où vous vous étiez arrêté', 'Rouvre les onglets de la session précédente.'),
    radio('pages', 'Ouvrir une page ou un ensemble de pages spécifiques'),
    pagesBox)];
}

function sAppearance() {
  const walls = el('div', { class: 'walls' });
  const current = WALLS.some(([id]) => id === S.wallpaper) ? S.wallpaper : WALLS[0][0];
  for (const [id, name] of WALLS) {
    const b = el('button', { type: 'button', 'aria-pressed': String(id === current), onclick: async () => {
      S.wallpaper = id;
      for (const x of walls.children) x.setAttribute('aria-pressed', String(x === b));
      await save('Fond d\'écran modifié');
    } }, el('img', { src: 'wallpapers/' + id + '.svg', alt: '' }), el('span', { text: name }));
    walls.append(b);
  }
  return [
    group('Page Nouvel onglet',
      row({ label: 'Fond d\'écran', desc: 'Image de fond de la page Nouvel onglet.', stack: true, below: walls, keywords: 'theme image arriere-plan' }),
      row({ label: 'Raccourcis, statistiques et horloge', desc: 'Se règlent sur la page Nouvel onglet (bouton engrenage).', onclick: () => openSection('newtab'), keywords: 'personnaliser' })),
    group('Barre d\'outils et onglets',
      toggle('Afficher le bouton Accueil', 'Bouton de la barre d\'outils qui ouvre la page Nouvel onglet.', () => S.show_home_button, (v) => { S.show_home_button = v; }, { keywords: 'maison home' }),
      toggle('Onglets verticaux', 'Affiche les onglets dans une barre latérale à gauche.', () => S.vertical_tabs, (v) => { S.vertical_tabs = v; }, { keywords: 'barre laterale' }),
      toggle('Animation de démarrage', 'Joue l\'animation NIGHTFALL au lancement (moins d\'une seconde).', () => P.splash_enabled, (v) => { P.splash_enabled = v; }, { keywords: 'splash' })),
    group('Pages web',
      row({ label: 'Taille de la police', desc: 'Taille du texte des pages qui ne fixent pas la leur.', prefs: ['webkit.webprefs.default_font_size', 'webkit.webprefs.default_fixed_font_size'], keywords: 'texte caracteres lisibilite',
        ctl: select(FONTS, S.font_size, async (v) => { S.font_size = Number(v); await save(); }) }),
      row({ label: 'Zoom de la page', desc: 'Zoom appliqué par défaut à toutes les pages.', keywords: 'agrandir taille',
        ctl: select(ZOOMS.map((z) => [z, z + ' %']), S.default_zoom, async (v) => { S.default_zoom = Number(v); await save(); }) })),
  ];
}

function sSearch() {
  const options = INFO.engines.map((e) => [e.id, e.name]).concat([['custom', S.search_custom_name ? S.search_custom_name + ' (personnalisé)' : 'Moteur personnalisé…']]);
  const name = el('input', { placeholder: 'Nom (ex. : Wikipédia)', 'aria-label': 'Nom du moteur personnalisé', value: S.search_custom_name });
  const url = el('input', { type: 'url', placeholder: 'https://fr.wikipedia.org/w/index.php?search=%s', 'aria-label': 'Adresse du moteur personnalisé (%s = recherche)', value: S.search_custom_url });
  const custom = el('div', { class: 'srow stack' },
    el('div', { class: 'txt' }, el('div', { class: 'lbl', text: 'Moteur personnalisé' }), el('div', { class: 'desc', text: 'Adresse https de la page de résultats, où %s est remplacé par votre recherche.' })),
    el('div', { class: 'inline-form' }, name, url, button('Utiliser', async () => {
      const u = url.value.trim();
      if (!/^https:\/\//i.test(u) || !u.includes('%s')) { note('L\'adresse doit commencer par https:// et contenir %s', true); return; }
      S.search_engine = 'custom';
      S.search_custom_name = name.value.trim() || host(u.replace('%s', ''));
      S.search_custom_url = u;
      sel.value = 'custom';
      await save('Moteur personnalisé enregistré');
    }, 'primary')));
  custom.dataset.search = norm('moteur personnalise adresse url');
  const sel = select(options, S.search_engine, async (v) => {
    custom.hidden = v !== 'custom';
    if (v === 'custom') { if (S.search_custom_url) { S.search_engine = 'custom'; await save(); } else url.focus(); return; }
    S.search_engine = v;
    await save();
  });
  custom.hidden = S.search_engine !== 'custom';
  custom.nfDep = () => sel.value === 'custom';
  return [group('',
    row({ label: 'Moteur de recherche utilisé dans la barre d\'adresse', desc: 'Ce que vous tapez et qui n\'est pas une adresse est envoyé à ce moteur.', ctl: sel, keywords: 'duckduckgo google brave qwant startpage ecosia bing' }),
    custom)];
}

function sPrivacy() {
  const dnsCustom = el('input', { 'aria-label': 'Identifiant NextDNS ou adresse DoH', value: S.secure_dns_custom });
  const dnsForm = el('div', { class: 'inline-form' }, dnsCustom, button('Enregistrer', async () => {
    S.secure_dns_custom = dnsCustom.value.trim();
    await save();
    const g = await api('settings.get');
    if (g.ok && g.settings.secure_dns !== S.secure_dns) { note(S.secure_dns === 'nextdns' ? 'Identifiant NextDNS invalide (lettres et chiffres)' : 'Adresse DoH invalide (https://…)', true); S.secure_dns = g.settings.secure_dns; dnsSel.value = S.secure_dns; dnsForm.hidden = true; }
  }));
  const dnsHint = () => {
    dnsForm.hidden = !['nextdns', 'custom'].includes(dnsSel.value);
    dnsCustom.placeholder = dnsSel.value === 'nextdns' ? 'Identifiant de configuration (ex. : a1b2c3)' : 'https://dns.exemple/dns-query';
  };
  const dnsSel = select(DNS, S.secure_dns, async (v) => {
    S.secure_dns = v;
    dnsHint();
    if (['nextdns', 'custom'].includes(v) && !dnsCustom.value.trim()) { dnsCustom.focus(); return; }
    S.secure_dns_custom = dnsCustom.value.trim();
    await save();
  });
  dnsHint();
  return [
    group('',
      row({ label: 'Effacer les données de navigation', desc: 'Historique, téléchargements, cookies, cache…', onclick: clearDataDialog, chev: 's-reset', keywords: 'supprimer vider nettoyer historique cookies cache' }),
      row({ label: 'NIGHTFALL Shield', desc: 'Blocage des publicités, traqueurs et pop-ups, listes de filtres, exceptions : Security Center.', onclick: () => openSection('security'), keywords: 'bloqueur publicite traqueurs adblock' })),
    group('Cookies et suivi',
      toggle('Bloquer les cookies tiers', 'Empêche les sites intégrés (publicités, réseaux sociaux) de vous suivre d\'un site à l\'autre. Quelques connexions intégrées peuvent ne plus fonctionner.', () => S.block_third_party_cookies, (v) => { S.block_third_party_cookies = v; }, { prefs: ['profile.cookie_controls_mode'], keywords: 'traçage pistage' }),
      toggle('Envoyer le signal Global Privacy Control', 'Indique aux sites que vous refusez la vente et le partage de vos données (en-tête Sec-GPC et navigator.globalPrivacyControl). Les sites ne sont pas tous tenus de le respecter.', () => S.global_privacy_control, (v) => { S.global_privacy_control = v; }, { keywords: 'gpc do not track dnt' })),
    group('Sécurité',
      toggle('HTTPS-First', 'Passe automatiquement les sites en HTTPS et vous avertit avant d\'ouvrir un site qui ne le prend pas en charge. Les adresses locales (box, imprimante) ne sont pas concernées. Désactivé, les adresses http:// s\'ouvrent sans chiffrement.', () => S.https_first, (v) => { S.https_first = v; }, { keywords: 'https only securise chiffre http' }),
      toggle('Avertissements anti-phishing', 'Analyse locale des adresses : homographes, imitations de vos sites et de services connus, sous-domaines trompeurs.', () => S.phishing_warnings, (v) => { S.phishing_warnings = v; }, { keywords: 'hameconnage fraude' }),
      toggle('Protection de l\'adresse IP (WebRTC)', 'Les sites ne voient pas votre adresse IP locale lors des appels audio ou vidéo dans le navigateur.', () => S.webrtc_ip_protection, (v) => { S.webrtc_ip_protection = v; }, { prefs: ['webrtc.ip_handling_policy'], keywords: 'fuite ip webrtc vpn' }),
      row({ label: 'DNS sécurisé', desc: 'Chiffre les requêtes DNS (DNS over HTTPS). Avec un fournisseur choisi, les sites ne s\'ouvrent que si ce fournisseur répond.', prefs: ['dns_over_https.mode', 'dns_over_https.templates'], keywords: 'doh dns over https cloudflare quad9 mullvad nextdns resolveur',
        ctl: dnsSel, below: dnsForm })),
  ];
}

function sPermissions() {
  const kinds = INFO.permissionKinds || [];
  const defaults = kinds.map((k) => {
    const [name, desc] = KIND_NAMES[k.id] || [k.label, ''];
    return row({ label: name, desc, keywords: 'autorisation permission ' + k.label,
      ctl: select([['ask', 'Les sites peuvent demander'], ['block', 'Ne pas autoriser']], S.permission_defaults[k.id] || 'ask', async (v) => {
        if (v === 'ask') delete S.permission_defaults[k.id]; else S.permission_defaults[k.id] = v;
        await save();
      }) });
  });
  const list = el('ul', { class: 'slist' });
  const clearAll = button('Tout retirer', async () => {
    if (check(await api('permissions.forget', { all: true }), 'Décisions retirées')) { S.site_permissions = []; renderList(); }
  });
  const renderList = () => {
    list.textContent = '';
    clearAll.hidden = !S.site_permissions.length;
    if (!S.site_permissions.length) {
      list.append(el('li', { class: 'empty', text: 'Aucune décision mémorisée. Cochez « Mémoriser » dans une demande d\'autorisation pour qu\'un site ne vous redemande plus.' }));
      return;
    }
    for (const p of S.site_permissions) {
      const name = (KIND_NAMES[p.kind] || [p.kind])[0];
      list.append(el('li', {},
        el('div', { class: 'grow' }, el('div', { text: host(p.origin) || p.origin }), el('div', { class: 'sub', text: name })),
        el('span', { class: 'badge ' + (p.allow ? 'ok' : 'danger'), text: p.allow ? 'Autorisé' : 'Bloqué' }),
        iconButton('s-x', `Retirer la décision ${name} pour ${host(p.origin)}`, async () => {
          if (check(await api('permissions.forget', { origin: p.origin, kind: p.kind }), 'Décision retirée')) {
            S.site_permissions = S.site_permissions.filter((x) => !(x.origin === p.origin && x.kind === p.kind));
            renderList();
          }
        })));
    }
  };
  renderList();
  const decisions = row({ label: 'Décisions mémorisées', desc: 'Sans « Mémoriser », votre réponse vaut jusqu\'au prochain verrouillage. Dans une fenêtre privée, rien n\'est mémorisé.', ctl: clearAll, keywords: 'sites autorises bloques exceptions' });
  return [
    group('Comportement par défaut', defaults),
    group('Par site', decisions, el('div', {}, list)),
  ];
}

function sPasswords() {
  return [group('',
    row({ label: 'Gestionnaire de mots de passe', desc: 'Vos identifiants, chiffrés avec votre clé NIGHTFALL. Afficher un mot de passe demande votre code.', onclick: () => openSection('passwords'), keywords: 'identifiants coffre' }),
    row({ label: 'Enregistrement des mots de passe', desc: 'NIGHTFALL n\'enregistre jamais un mot de passe sans votre action : sur une page de connexion, utilisez le bouton clé de la barre d\'outils. Le remplissage n\'a lieu que sur le site d\'origine.', keywords: 'proposer sauvegarder remplissage automatique' }))];
}

function sDownloads() {
  const path = el('div', { class: 'path', text: INFO.downloadDir || '' });
  const reset = button('Par défaut', async () => {
    if (check(await api('downloads.resetFolder'), 'Dossier par défaut rétabli')) { S.download_dir = null; await refreshFolder(); }
  });
  const refreshFolder = async () => {
    const g = await api('settings.get');
    if (!g.ok) return;
    S.download_dir = g.settings.download_dir;
    INFO.downloadDir = g.downloadDir;
    path.textContent = g.downloadDir;
    reset.hidden = !S.download_dir;
  };
  window.nfFolderChosen = (changed) => { if (changed) { note('Dossier de téléchargement modifié'); refreshFolder(); } };
  reset.hidden = !S.download_dir;
  return [group('',
    row({ label: 'Emplacement', extra: path, keywords: 'dossier repertoire telechargements',
      ctl: [reset, button('Modifier', async () => { check(await api('downloads.chooseFolder')); })] }),
    toggle('Toujours demander où enregistrer chaque fichier', 'Ouvre la fenêtre « Enregistrer sous » du système à chaque téléchargement.', () => S.download_ask, (v) => { S.download_ask = v; }, { keywords: 'enregistrer sous' }),
    row({ label: 'Téléchargements', desc: 'Liste des téléchargements et vérification des signatures.', onclick: () => openSection('downloads'), keywords: 'historique fichiers' }))];
}

function sLanguages() {
  const list = el('ul', { class: 'slist' });
  const langs = () => S.accept_languages.split(',').map((x) => x.trim()).filter(Boolean);
  const name = (t) => (LANGS.find(([id]) => id.toLowerCase() === t.toLowerCase()) || [t, t])[1];
  const set = async (arr) => { S.accept_languages = arr.join(','); await save(); render(); };
  const addSel = el('select', { 'aria-label': 'Langue à ajouter', style: 'width:auto' });
  const render = () => {
    const cur = langs();
    list.textContent = '';
    cur.forEach((t, i) => list.append(el('li', {},
      el('div', { class: 'grow' }, el('div', { text: name(t) }), el('div', { class: 'sub', text: t + (i === 0 ? ' · langue préférée' : '') })),
      Object.assign(iconButton('s-up', 'Monter ' + name(t), () => { const a = cur.slice(); [a[i - 1], a[i]] = [a[i], a[i - 1]]; set(a); }), { disabled: i === 0 }),
      Object.assign(iconButton('s-up', 'Descendre ' + name(t), () => { const a = cur.slice(); [a[i + 1], a[i]] = [a[i], a[i + 1]]; set(a); }, 'down'), { disabled: i === cur.length - 1 }),
      cur.length > 1 ? iconButton('s-x', 'Retirer ' + name(t), () => set(cur.filter((_, j) => j !== i))) : null)));
    addSel.textContent = '';
    addSel.append(el('option', { value: '', text: 'Ajouter une langue…' }), LANGS.filter(([id]) => !cur.some((c) => c.toLowerCase() === id.toLowerCase())).map(([id, n]) => el('option', { value: id, text: n + ' (' + id + ')' })));
  };
  addSel.addEventListener('change', () => { if (addSel.value) set(langs().concat([addSel.value])); });
  render();
  const spell = INFO.spellcheckAvailable
    ? toggle('Correcteur orthographique', 'Souligne les fautes dans les champs de texte avec le correcteur de macOS (rien n\'est envoyé en ligne).', () => S.spellcheck, (v) => { S.spellcheck = v; }, { prefs: ['browser.enable_spellchecking'], keywords: 'orthographe fautes' })
    : row({ label: 'Correcteur orthographique', desc: 'Indisponible sur Windows : Chromium y télécharge des dictionnaires depuis les serveurs de Google. NIGHTFALL le laisse désactivé pour ne rien envoyer.', cls: 'disabled', keywords: 'orthographe fautes', ctl: el('span', { class: 'badge', text: 'Désactivé' }) });
  return [
    group('Langues des pages web',
      row({ label: 'Langues préférées', desc: 'Envoyées aux sites, dans cet ordre, pour afficher les pages dans votre langue (en-tête Accept-Language).', ctl: addSel, prefs: ['intl.accept_languages'], keywords: 'langue anglais francais traduction' }),
      el('div', {}, list)),
    group('Interface et saisie',
      row({ label: 'Langue de NIGHTFALL', desc: 'L\'interface de NIGHTFALL est en français.', ctl: el('span', { class: 'badge', text: 'Français' }) }),
      spell),
  ];
}

function sSystem() {
  const restart = el('span', { class: 'badge warn', text: 'Redémarrage nécessaire' });
  const updateRestart = () => { restart.hidden = P.hardware_acceleration === INFO.hardwareAccelerationAtStart; };
  updateRestart();
  const after = el('select', { 'aria-label': 'Délai avant la mise en veille d\'un onglet' }, [15, 30, 60, 120].map((m) => el('option', { value: m, text: m < 60 ? `après ${m} min` : `après ${m / 60} h` })));
  after.value = String([15, 30, 60, 120].includes(S.perf.discard_after_minutes) ? S.perf.discard_after_minutes : 30);
  after.disabled = !S.perf.discard_enabled;
  after.addEventListener('change', async () => { S.perf.discard_after_minutes = Number(after.value); await save(); });
  return [
    group('',
      row({ label: 'Utiliser l\'accélération matérielle', desc: 'Utilise la carte graphique pour afficher les pages et les vidéos. À désactiver en cas d\'affichage défaillant. S\'applique au prochain lancement.', keywords: 'gpu carte graphique',
        ctl: [restart, sw(() => P.hardware_acceleration, (v) => { P.hardware_acceleration = v; }, { after: updateRestart })] }),
      row({ label: 'Économiseur de mémoire', desc: 'Quand la mémoire manque, met en veille les onglets inactifs depuis le délai choisi (5 min si la mémoire est presque épuisée), jamais l\'onglet actif, épinglé ou privé. Ils se rechargent d\'un clic.', keywords: 'ram performance veille onglets',
        ctl: [after, sw(() => S.perf.discard_enabled, (v) => { S.perf.discard_enabled = v; }, { after: (v) => { after.disabled = !v; } })] }),
      row({ label: 'Paramètres proxy', desc: 'NIGHTFALL utilise le proxy configuré dans votre système.', keywords: 'proxy reseau vpn',
        ctl: button('Ouvrir les paramètres du système', async () => { check(await api('system.proxySettings')); }) })),
    group('Développement',
      toggle('Outils de développement (Ctrl+Maj+I)', 'Désactivés par défaut : ils peuvent lire les données des pages, y compris les sessions.', () => S.devtools_enabled, (v) => { S.devtools_enabled = v; }, { keywords: 'devtools inspecteur console' })),
  ];
}

function sLock() {
  const autoLock = select([0, 1, 5, 15, 30, 60].map((m) => [m, m ? `Après ${m} min d'inactivité` : 'Jamais']), S.auto_lock_minutes, async (v) => { S.auto_lock_minutes = Number(v); await save(); });
  const pin = (ph) => el('input', { type: 'password', inputmode: 'numeric', autocomplete: 'off', placeholder: ph, 'aria-label': ph });
  const cur = pin('Code actuel'); const nw = pin('Nouveau code'); const nw2 = pin('Confirmer le nouveau code');
  const change = el('div', { class: 'inline-form' }, cur, nw, nw2, button('Changer', async () => {
    if (nw.value !== nw2.value) { note('Les deux nouveaux codes ne correspondent pas', true); return; }
    const r = await api('lock.changePin', { current: cur.value, new: nw.value });
    cur.value = ''; nw.value = ''; nw2.value = '';
    check(r, 'Code modifié');
  }, 'primary'));
  const rkPin = pin('Code actuel');
  const rkOut = el('div', {});
  const rk = el('div', { class: 'inline-form' }, rkPin, button('Générer', async () => {
    const r = await api('lock.newRecovery', { pin: rkPin.value });
    rkPin.value = '';
    if (!check(r)) return;
    rkOut.textContent = '';
    rkOut.append(el('div', { class: 'recovery', text: r.recoveryKey }), el('p', { class: 'muted small', style: 'margin:8px 0 0', text: 'Notez-la sur papier, hors de l\'ordinateur. L\'ancienne Recovery Key ne fonctionne plus.' }));
  }));
  return [
    group('Verrouillage',
      row({ label: 'Verrouillage automatique', desc: 'Verrouille NIGHTFALL après une période sans activité.', ctl: autoLock, keywords: 'inactivite delai' }),
      toggle('Verrouiller après une mise en veille', 'Détecte la reprise après veille de l\'ordinateur et exige le code.', () => S.lock_on_sleep, (v) => { S.lock_on_sleep = v; }, { keywords: 'sommeil veille' })),
    group('Code et récupération',
      row({ label: 'Changer de code', desc: '6 à 12 chiffres.', stack: true, below: change, keywords: 'pin mot de passe' }),
      row({ label: 'Nouvelle Recovery Key', desc: 'Génère une nouvelle clé de récupération ; l\'ancienne cessera de fonctionner.', stack: true, below: el('div', {}, rk, rkOut), keywords: 'cle de secours recuperation' })),
    group('Urgence',
      row({ label: 'NIGHTFALL Emergency System', desc: 'Verrouillage d\'urgence, purge des données, Auto-Purge : Security Center.', onclick: () => openSection('security', 'emergency'), keywords: 'lockdown purge urgence' })),
  ];
}

function sUpdates() {
  const status = el('div', { class: 'desc', text: 'Vérification de l\'état…' });
  const prog = el('div', { class: 'progress', hidden: true }, el('div', {}));
  const version = el('div', { class: 'lbl', text: 'NIGHTFALL ' + (INFO.version || '') });
  const checkBtn = button('Rechercher les mises à jour', async () => { checkBtn.disabled = true; const r = await api('update.check'); if (!r.ok) note(r.error || 'Service de mise à jour indisponible', true); refresh(); });
  let ready = false;
  const install = button('Redémarrer et mettre à jour', async () => {
    if (!ready) {
      // Not downloaded yet: update.install starts the download.
      const r = await api('update.install');
      note(r.ok ? 'Installation en cours…' : (r.error || 'Téléchargement impossible'), !r.ok && !/téléchargement/i.test(r.error || ''));
      refresh();
      return;
    }
    confirmDialog('Installer la mise à jour', 'NIGHTFALL va se fermer pour installer la mise à jour puis redémarrer. Vos onglets seront restaurés si « Reprendre là où vous vous étiez arrêté » est choisi au démarrage.', async () => {
      const r = await api('update.install');
      if (!r.ok) { note(r.error || 'Installation impossible', true); refresh(); }
    }, 'Redémarrer');
  }, 'primary');
  install.hidden = true;
  const notes = button('Notes de version', () => { if (notes.dataset.url) api('tab.new', { url: notes.dataset.url }); });
  notes.hidden = true;
  const signed = el('span', { class: 'badge ok', text: 'Mise à jour signée', hidden: true });
  const autoCheck = el('input', { type: 'checkbox', class: 'sw', role: 'switch', disabled: true });
  const autoInstall = el('input', { type: 'checkbox', class: 'sw', role: 'switch', disabled: true });
  const setPrefs = async () => {
    const r = await api('update.set', { autoCheck: autoCheck.checked, autoInstall: autoInstall.checked });
    if (!check(r, 'Préférence de mise à jour enregistrée')) refresh();
  };
  autoCheck.addEventListener('change', setPrefs);
  autoInstall.addEventListener('change', setPrefs);
  let timer = 0;
  const refresh = async () => {
    clearTimeout(timer);
    const r = await api('update.status');
    if (!r || !r.ok) {
      status.textContent = 'Le service de mise à jour n\'est pas disponible pour le moment' + (r && r.error && r.error.indexOf('commande inconnue') < 0 ? ' : ' + r.error : '.');
      checkBtn.disabled = true; autoCheck.disabled = true; autoInstall.disabled = true;
      return;
    }
    version.textContent = 'NIGHTFALL ' + (r.current || INFO.version || '');
    autoCheck.disabled = false; autoInstall.disabled = false;
    autoCheck.checked = !!r.autoCheck; autoInstall.checked = !!r.autoInstall;
    signed.hidden = !r.signed;
    checkBtn.disabled = !!(r.checking || r.downloading);
    ready = !!r.readyToInstall;
    install.hidden = !(r.readyToInstall || r.available) || r.checking || r.downloading || !!r.notInstallable;
    install.textContent = r.readyToInstall ? 'Redémarrer et mettre à jour' : 'Télécharger et installer';
    notes.hidden = !(r.available && /^https:\/\//.test(r.releaseUrl || ''));
    notes.dataset.url = r.releaseUrl || '';
    prog.hidden = !r.downloading;
    const pct = r.progress > 1 ? Math.round(r.progress) : Math.round((r.progress || 0) * 100);
    prog.firstChild.style.width = pct + '%';
    const last = r.lastCheck ? ' Dernière vérification : ' + fmtDate(r.lastCheck) + '.' : '';
    if (r.checking) status.textContent = 'Recherche des mises à jour…';
    else if (r.downloading) status.textContent = `Téléchargement de la version ${r.latest || ''} : ${pct} %`;
    else if (r.readyToInstall) status.textContent = `La version ${r.latest || ''} est prête : redémarrez NIGHTFALL pour l'installer.`;
    else if (r.available && r.notInstallable) status.textContent = `La version ${r.latest || ''} est disponible, mais ne peut pas être installée automatiquement : ${r.notInstallable}`;
    else if (r.available) status.textContent = `La version ${r.latest || ''} est disponible.` + last;
    else if (r.error) status.textContent = 'La dernière vérification a échoué : ' + r.error + '.' + last;
    else status.textContent = 'NIGHTFALL est à jour.' + last;
    if (r.checking || r.downloading) timer = setTimeout(refresh, 1500);
  };
  refresh();
  return [
    group('',
      el('div', { class: 'srow wrap' }, el('div', { class: 'upd' }, el('img', { src: 'emblem.png', alt: '' }), el('div', { class: 'grow' }, version, status, prog)), el('div', { class: 'ctl' }, signed, notes, install, checkBtn))),
    group('Automatique',
      row({ label: 'Rechercher automatiquement les mises à jour', desc: 'NIGHTFALL vérifie régulièrement si une nouvelle version est publiée.', ctl: autoCheck, keywords: 'automatique version' }),
      row({ label: 'Installer automatiquement au redémarrage', desc: 'Une mise à jour téléchargée s\'installe à la prochaine fermeture de NIGHTFALL.', ctl: autoInstall, keywords: 'automatique version' })),
  ];
}

function sReset() {
  return [group('',
    row({ label: 'Restaurer les paramètres par défaut', desc: 'Remet tous les paramètres de cette page à leur valeur d\'origine. Vos favoris, votre historique, vos mots de passe, vos raccourcis, les réglages de Shield et de VideoGuard et les règles d\'urgence sont conservés.', keywords: 'reinitialiser defaut usine',
      ctl: button('Réinitialiser…', () => confirmDialog('Réinitialiser les paramètres ?', 'Les paramètres reprennent leur valeur par défaut (démarrage, apparence, moteur de recherche, confidentialité, autorisations mémorisées, téléchargements, langues, système, verrouillage). Vos données sont conservées.', async () => {
        if (check(await api('settings.reset'), 'Paramètres réinitialisés')) await load();
      }, 'Réinitialiser', 'danger'), 'danger') }))];
}

function sAbout() {
  return [group('',
    el('div', { class: 'srow' }, el('div', { class: 'upd grow' }, el('img', { src: 'emblem.png', alt: '' }), el('div', {}, el('div', { class: 'lbl', text: 'NIGHTFALL Browser ' + (INFO.version || '') }), el('div', { class: 'desc', text: 'Browse Beyond the Darkness.' })))),
    row({ label: 'Mises à jour', desc: 'Version installée et recherche de mises à jour.', onclick: () => show('mises-a-jour'), chev: 's-update' }),
    row({ label: 'À propos, composants et licences', desc: 'Chromium (CEF), adblock-rust, polices, licences.', onclick: () => openSection('about'), keywords: 'licence credits open source' }))];
}

const SECTIONS = [
  { id: 'demarrage', title: 'Démarrage', icon: 's-start', build: sStartup },
  { id: 'apparence', title: 'Apparence', icon: 's-look', build: sAppearance },
  { id: 'recherche', title: 'Moteur de recherche', icon: 's-search', build: sSearch },
  { id: 'confidentialite', title: 'Confidentialité et sécurité', icon: 's-privacy', build: sPrivacy },
  { id: 'autorisations', title: 'Autorisations des sites', icon: 's-perm', build: sPermissions },
  { id: 'mots-de-passe', title: 'Mots de passe', icon: 's-key', build: sPasswords },
  { id: 'telechargements', title: 'Téléchargements', icon: 's-down', build: sDownloads },
  { id: 'langues', title: 'Langues', icon: 's-lang', build: sLanguages },
  { id: 'systeme', title: 'Système', icon: 's-system', build: sSystem },
  { id: 'lock', title: 'NIGHTFALL LOCK', icon: 's-lock', build: sLock, sep: true },
  { id: 'mises-a-jour', title: 'Mises à jour', icon: 's-update', build: sUpdates },
  { id: 'reinitialiser', title: 'Réinitialiser les paramètres', icon: 's-reset', build: sReset, sep: true },
  { id: 'a-propos', title: 'À propos de NIGHTFALL', icon: 's-about', build: sAbout },
];

// ------------------------------------------------------------------ navigation & search
let current = SECTIONS[0].id;

function show(id, focusHeading) {
  if (!SECTIONS.some((s) => s.id === id)) id = SECTIONS[0].id;
  current = id;
  if ($('#q').value) { $('#q').value = ''; filter(); }
  for (const sec of document.querySelectorAll('.st-section')) sec.hidden = sec.dataset.id !== id;
  for (const b of $('#nav').querySelectorAll('button')) b.setAttribute('aria-current', String(b.dataset.id === id));
  if (location.hash !== '#' + id) history.replaceState(null, '', '#' + id);
  $('.st-main').scrollTop = 0;
  const sec = SECTIONS.find((s) => s.id === id);
  document.title = sec.title + ' · Paramètres · NIGHTFALL';
  if (focusHeading) { const h = document.querySelector(`.st-section[data-id="${id}"] h1`); if (h) h.focus({ preventScroll: true }); }
}

function highlight(r, q) {
  const lbl = r.querySelector('.lbl');
  if (!lbl) return;
  if (lbl.dataset.text == null) lbl.dataset.text = lbl.textContent;
  const text = lbl.dataset.text;
  lbl.textContent = '';
  const i = q ? norm(text).indexOf(q) : -1;
  // norm() keeps one character per letter for French text (accents removed).
  if (i < 0 || norm(text).length !== text.length) { lbl.textContent = text; return; }
  lbl.append(text.slice(0, i), el('mark', { text: text.slice(i, i + q.length) }), text.slice(i + q.length));
}

function filter() {
  const q = norm($('#q').value.trim());
  const root = $('#root');
  root.classList.toggle('st-searching', !!q);
  let any = false;
  for (const sec of document.querySelectorAll('.st-section')) {
    const secTitle = norm(sec.dataset.title);
    let secAny = false;
    for (const g of sec.querySelectorAll('.st-group')) {
      let gAny = false;
      for (const r of g.querySelectorAll('[data-search]')) {
        // Rows shown only for one choice (custom engine) keep that rule.
        const hit = (!q || r.dataset.search.includes(q) || secTitle.includes(q)) && (!r.nfDep || r.nfDep());
        r.hidden = !hit;
        highlight(r, q);
        gAny = gAny || hit;
      }
      g.hidden = !gAny;
      secAny = secAny || gAny;
    }
    sec.hidden = q ? !secAny : sec.dataset.id !== current;
    any = any || secAny;
  }
  for (const b of $('#nav').querySelectorAll('button')) b.setAttribute('aria-current', String(!q && b.dataset.id === current));
  $('#none').hidden = !q || any;
  $('#none-q').textContent = $('#q').value.trim();
}

function render() {
  rowSeq = 0;
  const nav = $('#nav');
  const root = $('#root');
  nav.textContent = '';
  root.textContent = '';
  for (const sec of SECTIONS) {
    if (sec.sep) nav.append(el('div', { class: 'sep', role: 'presentation' }));
    const b = el('button', { type: 'button', 'data-id': sec.id, onclick: () => show(sec.id) }, icon(sec.icon), el('span', { text: sec.title }));
    nav.append(b);
    let content;
    try { content = sec.build(); } catch (e) { console.error('section ' + sec.id + ' : ' + e); content = [el('p', { class: 'muted', text: 'Section indisponible.' })]; }
    root.append(el('section', { class: 'st-section', 'data-id': sec.id, 'data-title': sec.title, hidden: true, 'aria-labelledby': 'h-' + sec.id },
      el('h1', { id: 'h-' + sec.id, tabindex: '-1', text: sec.title }), content));
  }
  markPrefs();
}

async function load() {
  const r = await api('settings.get');
  if (!r || !r.ok) { $('#root').append(el('p', { class: 'muted', text: 'Impossible de lire les paramètres : ' + ((r && r.error) || 'erreur') })); return; }
  S = r.settings;
  P = r.prefs;
  INFO = r;
  render();
  show((location.hash || '').slice(1) || current);
}

// Keyboard: "/" searches, Escape clears, arrows move in the sidebar.
document.addEventListener('keydown', (e) => {
  const typing = /^(INPUT|SELECT|TEXTAREA)$/.test(document.activeElement.tagName);
  if (e.key === '/' && !typing && !dlg.open) { e.preventDefault(); $('#q').focus(); }
});
$('#q').addEventListener('input', filter);
$('#q').addEventListener('keydown', (e) => { if (e.key === 'Escape' && $('#q').value) { e.stopPropagation(); $('#q').value = ''; filter(); } });
$('#nav').addEventListener('keydown', (e) => {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
  const items = [...$('#nav').querySelectorAll('button')];
  let i = items.indexOf(document.activeElement);
  if (e.key === 'ArrowDown') i = (i + 1) % items.length;
  else if (e.key === 'ArrowUp') i = (i - 1 + items.length) % items.length;
  else if (e.key === 'Home') i = 0;
  else i = items.length - 1;
  e.preventDefault();
  items[i].focus();
});
window.addEventListener('hashchange', () => show(location.hash.slice(1)));
load();
