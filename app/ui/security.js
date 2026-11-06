'use strict';
let ST = null;
const PURGE = [
  ['sessions', 'Cookies, sessions et jetons locaux'],
  ['browsing_data', 'Historique, favoris, réglages, historique des téléchargements'],
  ['passwords', 'Mots de passe'],
  ['extensions', 'Extensions et leurs données'],
  ['shield_cache', 'Listes de filtres'],
  ['everything', 'TOUT, y compris les clés (réinitialisation complète)'],
];

function sw(label, desc, checked, onChange) {
  const input = el('input', { type: 'checkbox' });
  input.checked = !!checked;
  input.addEventListener('change', () => onChange(input.checked));
  return el('div', { class: 'switch' }, el('div', {}, el('div', { text: label }), desc ? el('div', { class: 'muted small', text: desc }) : null), input);
}

async function saveSettings(mut) {
  const s = await api('settings.get');
  if (!s.ok) return;
  mut(s.settings);
  check(await api('settings.set', { settings: s.settings }), 'Enregistré');
}

async function saveShield(mut) {
  const cfg = {
    enabled: ST.shield.enabled, block_popups: ST.shield.blockPopups, lists: ST.shield.lists,
    custom_filters: ST.shield.customFilters, allowlist: ST.shield.allowlist,
  };
  mut(cfg);
  if (check(await api('shield.set', { config: cfg }), 'Shield mis à jour')) load();
}

function card(title, ...children) { return el('div', { class: 'card' }, el('h2', { text: title }), ...children); }

async function load() {
  const r = await api('security.status');
  if (!check(r)) return;
  ST = r.status;
  const root = $('#root');
  root.textContent = '';

  // ---- Overview
  const b = (ok, on, off) => el('span', { class: 'badge ' + (ok ? 'ok' : 'warn'), text: ok ? on : off });
  root.append(card('État des modules',
    el('ul', { class: 'list' },
      el('li', {}, el('span', { class: 'grow', text: 'NIGHTFALL LOCK' }), b(ST.lock.hardwareBacked, ST.lock.backend, ST.lock.backend)),
      el('li', {}, el('span', { class: 'grow', text: 'NIGHTFALL Shield' }), b(ST.shield.enabled && ST.shield.rules > 0, `actif · ${ST.shield.rules} règles`, ST.shield.enabled ? 'listes non téléchargées' : 'désactivé')),
      el('li', {}, el('span', { class: 'grow', text: 'NIGHTFALL VideoGuard' }), b(ST.videoguard.config.enabled, 'actif', 'désactivé')),
      el('li', {}, el('span', { class: 'grow', text: 'Session Vault' }), b(ST.sessionVault, 'cookies chiffrés au verrouillage', 'désactivé')),
      el('li', {}, el('span', { class: 'grow', text: 'Anti-phishing (analyse locale)' }), b(ST.phishingWarnings, 'actif', 'désactivé')),
      el('li', {}, el('span', { class: 'grow', text: 'Sandbox et Site Isolation de Chromium' }), b(ST.chromium.sandbox, 'conservés', 'sandbox désactivée (build de test)')),
      el('li', {}, el('span', { class: 'grow', text: 'Débogage à distance' }), b(true, 'interdit', '')),
    ),
    ST.signals.length ? el('p', { class: 'muted', text: 'Signaux de sécurité : ' + ST.signals.join(' ; ') }) : null,
  ));

  // ---- Shield
  const lists = el('ul', { class: 'list' });
  ST.shield.lists.forEach((l, i) => {
    const cb = el('input', { type: 'checkbox' });
    cb.checked = l.enabled;
    cb.addEventListener('change', () => saveShield((c) => { c.lists[i].enabled = cb.checked; }));
    lists.append(el('li', {}, cb, el('div', { class: 'grow' }, el('div', { class: 'title', text: l.name }), el('div', { class: 'sub', text: l.updated ? 'Mise à jour : ' + fmtDate(l.updated) : 'Jamais téléchargée' }))));
  });
  const custom = el('textarea', { placeholder: '||publicite.exemple^\nexemple.com##.banniere' });
  custom.value = ST.shield.customFilters;
  const allow = el('ul', { class: 'list' });
  (ST.shield.allowlist || []).forEach((d) => allow.append(el('li', {}, el('span', { class: 'grow', text: d }), el('button', { text: 'Retirer', onclick: () => saveShield((c) => { c.allowlist = c.allowlist.filter((x) => x !== d); }) }))));
  if (!(ST.shield.allowlist || []).length) allow.append(el('li', { class: 'empty', text: 'Aucune exception' }));
  const top = Object.entries(ST.shield.stats.per_site || {}).sort((a, b2) => b2[1] - a[1]).slice(0, 8);
  root.append(card('NIGHTFALL Shield',
    sw('Activer Shield', 'Bloque publicités et traqueurs (moteur adblock-rust de Brave).', ST.shield.enabled, (v) => saveShield((c) => { c.enabled = v; })),
    sw('Bloquer les pop-ups', 'Bloque les fenêtres ouvertes sans action de votre part.', ST.shield.blockPopups, (v) => saveShield((c) => { c.block_popups = v; })),
    el('p', { text: `${ST.shield.stats.total_blocked} requêtes et ${ST.shield.stats.popups_blocked} pop-ups bloquées depuis le démarrage.` }),
    top.length ? el('ul', { class: 'list' }, top.map(([s, n]) => el('li', {}, el('span', { class: 'grow', text: s }), el('span', { class: 'badge', text: n })))) : null,
    el('h2', { text: 'Listes de filtres' }), lists,
    el('div', { class: 'row' }, el('button', { class: 'primary', text: 'Mettre à jour maintenant', onclick: async () => { if (check(await api('shield.update'), 'Téléchargement des listes en cours…')) setTimeout(load, 8000); } })),
    el('label', { text: 'Filtres personnalisés' }), custom,
    el('button', { text: 'Enregistrer les filtres', onclick: () => saveShield((c) => { c.custom_filters = custom.value; }) }),
    el('label', { text: 'Sites où Shield est désactivé' }), allow,
  ));

  // ---- VideoGuard
  const vg = ST.videoguard.config;
  const cats = el('ul', { class: 'list' });
  for (const [id, label] of ST.videoguard.categories) {
    const sel = el('select', { style: 'width:auto' }, [['skip', 'Sauter automatiquement'], ['show', 'Proposer de sauter'], ['ignore', 'Ignorer']].map(([v, t]) => el('option', { value: v, text: t })));
    sel.value = vg.actions[id] || 'ignore';
    sel.addEventListener('change', async () => { vg.actions[id] = sel.value; check(await api('videoguard.set', { config: vg }), 'VideoGuard mis à jour'); });
    cats.append(el('li', {}, el('span', { class: 'grow', text: label }), sel));
  }
  root.append(card('NIGHTFALL VideoGuard',
    sw('Activer VideoGuard', 'Saute les segments sponsorisés des vidéos YouTube, avec un bouton pour revenir en arrière.', vg.enabled, async (v) => { vg.enabled = v; check(await api('videoguard.set', { config: vg }), 'VideoGuard mis à jour'); }),
    sw('Couper le son des segments « sourdine »', 'Certains segments ne sont signalés que pour le son (musique sous licence, passage sponsorisé à l\'oral) : le son est coupé pendant ces passages.', vg.mute_segments !== false, async (v) => { vg.mute_segments = v; check(await api('videoguard.set', { config: vg }), 'VideoGuard mis à jour'); }),
    sw('Étiquette « vidéo entière »', 'Signale les vidéos entièrement sponsorisées ou en accès exclusif.', vg.full_video_labels !== false, async (v) => { vg.full_video_labels = v; check(await api('videoguard.set', { config: vg }), 'VideoGuard mis à jour'); }),
    sw('Accélérer les publicités vidéo', 'Si une publicité YouTube passe malgré Shield, VideoGuard la coupe et l\'avance jusqu\'à la fin.', vg.skip_video_ads !== false, async (v) => { vg.skip_video_ads = v; check(await api('videoguard.set', { config: vg }), 'VideoGuard mis à jour'); }),
    cats, el('p', { class: 'muted small', text: ST.videoguard.attribution + ' Seul un préfixe de hachage de l\'identifiant de la vidéo est envoyé.' }),
  ));

  // ---- Sessions & phishing
  root.append(card('Protection des sessions et anti-phishing',
    sw('Session Vault', 'Au verrouillage et à la fermeture, les cookies (y compris HttpOnly) sont chiffrés avec votre clé NIGHTFALL et retirés de Chromium, puis restaurés au déverrouillage.', ST.sessionVault, (v) => saveSettings((s) => { s.session_vault = v; })),
    sw('Avertissements anti-phishing', 'Analyse locale : homographes, sites imitant vos sites et des services connus, sous-domaines trompeurs.', ST.phishingWarnings, (v) => saveSettings((s) => { s.phishing_warnings = v; })),
    el('p', { class: 'muted small', text: 'Limite : aucun navigateur ne peut protéger totalement des sessions face à un logiciel malveillant déjà installé avec vos droits ou des droits administrateur.' }),
  ));

  // ---- Lock management
  const cur = el('input', { type: 'password', inputmode: 'numeric', placeholder: 'Code actuel' });
  const nw = el('input', { type: 'password', inputmode: 'numeric', placeholder: 'Nouveau code' });
  const rkPin = el('input', { type: 'password', inputmode: 'numeric', placeholder: 'Code actuel' });
  const rkOut = el('div', { class: 'mono' });
  root.append(card('NIGHTFALL LOCK',
    el('p', { class: 'muted', text: 'Protection du code : ' + ST.lock.backend + '.' }),
    el('label', { text: 'Changer de code' }), el('div', { class: 'row' }, cur, nw, el('button', { class: 'primary', text: 'Changer', onclick: async () => { if (check(await api('lock.changePin', { current: cur.value, new: nw.value }), 'Code modifié')) { cur.value = ''; nw.value = ''; } } })),
    el('label', { text: 'Nouvelle Recovery Key (l\'ancienne cessera de fonctionner)' }), el('div', { class: 'row' }, rkPin, el('button', { text: 'Générer', onclick: async () => { const r2 = await api('lock.newRecovery', { pin: rkPin.value }); rkPin.value = ''; if (check(r2)) rkOut.textContent = r2.recoveryKey + '  ← notez-la hors de l\'ordinateur'; } })), rkOut,
  ));

  // ---- Emergency
  const purgeBoxes = PURGE.map(([id, label]) => { const cb = el('input', { type: 'checkbox', value: id, style: 'width:auto' }); return el('label', { style: 'display:flex;gap:8px;align-items:center;text-transform:none;letter-spacing:0;font-size:14px;color:var(--text)' }, cb, label); });
  const purgePin = el('input', { type: 'password', inputmode: 'numeric', placeholder: 'Code NIGHTFALL' });
  const purgeConfirm = el('input', { placeholder: 'Tapez PURGER' });
  const ap = ST.autoPurge;
  const apCats = PURGE.filter(([id]) => id !== 'everything').map(([id, label]) => { const cb = el('input', { type: 'checkbox', style: 'width:auto' }); cb.checked = ap.categories.includes(id); cb.addEventListener('change', async () => { ap.categories = cb.checked ? [...new Set([...ap.categories, id])] : ap.categories.filter((c) => c !== id); check(await api('autopurge.set', { rules: ap }), 'Règles enregistrées'); }); return el('label', { style: 'display:flex;gap:8px;align-items:center;text-transform:none;letter-spacing:0;font-size:14px;color:var(--text)' }, cb, label); });
  root.append(card('NIGHTFALL Emergency System',
    el('p', { class: 'muted', text: 'Niveau 1 · Security Lockdown : masque immédiatement les onglets, ferme les fenêtres privées, chiffre les sessions et exige le code. Raccourci : Ctrl+Maj+F12 (Cmd+Maj+F12 sur Mac).' }),
    el('button', { class: 'danger', text: 'EMERGENCY LOCKDOWN', onclick: () => api('emergency.lockdown') }),
    el('h2', { text: 'Niveau 2 · Emergency Data Purge', style: 'margin-top:18px' }),
    el('p', { class: 'muted', text: 'Destruction cryptographique des clés puis suppression des fichiers du profil NIGHTFALL. Vos téléchargements et vos fichiers personnels ne sont jamais touchés. L\'effacement physique absolu sur SSD, dans les sauvegardes ou les dossiers synchronisés ne peut pas être garanti.' }),
    ...purgeBoxes,
    el('div', { class: 'row', style: 'margin-top:10px' }, purgePin, purgeConfirm, el('button', { class: 'danger', text: 'Purger', onclick: async () => {
      const categories = purgeBoxes.map((l) => l.querySelector('input')).filter((c) => c.checked).map((c) => c.value);
      const r2 = await api('emergency.purge', { pin: purgePin.value, confirm: purgeConfirm.value, categories });
      purgePin.value = '';
      if (check(r2, r2.restart ? 'Purge programmée : NIGHTFALL va se fermer et terminera la purge au prochain lancement.' : 'Purge effectuée')) load();
    } })),
    el('h2', { text: 'Niveau 3 · Auto-Purge', style: 'margin-top:18px' }),
    el('p', { class: 'muted', text: `Déclenchée uniquement si au moins ${Math.max(2, ap.min_independent_critical)} signaux critiques indépendants sont confirmés (signature invalide, magasin de clés altéré, lancement en mode débogage, injection de module, 10 échecs du code ou plus). Un signal isolé ne provoque qu'un verrouillage. Aucune page web ni extension ne peut la déclencher.` }),
    sw('Activer l\'Auto-Purge', 'Désactivée par défaut.', ap.enabled, async (v) => { ap.enabled = v; check(await api('autopurge.set', { rules: ap }), 'Règles enregistrées'); }),
    ...apCats,
  ));
  // Deep link from the settings: security.html#emergency.
  const cards = root.querySelectorAll('.card');
  cards[cards.length - 1].id = 'emergency';
  if (location.hash === '#emergency') cards[cards.length - 1].scrollIntoView();
}
load();
