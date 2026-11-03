'use strict';
const ENGINE_NAMES = { duckduckgo: 'DuckDuckGo', google: 'Google', brave: 'Brave Search', startpage: 'Startpage', qwant: 'Qwant', ecosia: 'Ecosia', bing: 'Bing' };
let S = null;
let P = null;

function toggle(label, desc, get, set) {
  const input = el('input', { type: 'checkbox' });
  input.checked = !!get();
  input.addEventListener('change', async () => { set(input.checked); await save(); });
  return el('div', { class: 'switch' }, el('div', {}, el('div', { text: label }), desc ? el('div', { class: 'muted small', text: desc }) : null), input);
}

async function save() {
  check(await api('settings.set', { settings: S, prefs: P }), 'Paramètres enregistrés');
}

async function load() {
  const r = await api('settings.get');
  if (!check(r)) return;
  S = r.settings;
  P = r.prefs;
  const root = $('#root');
  root.textContent = '';

  const engine = el('select', {}, r.engines.map((e) => el('option', { value: e, text: ENGINE_NAMES[e] || e })));
  engine.value = S.search_engine;
  engine.addEventListener('change', () => { S.search_engine = engine.value; save(); });

  const autoLock = el('select', {}, [0, 1, 5, 15, 30, 60].map((m) => el('option', { value: m, text: m ? `${m} min` : 'Jamais' })));
  autoLock.value = S.auto_lock_minutes;
  autoLock.addEventListener('change', () => { S.auto_lock_minutes = Number(autoLock.value); save(); });

  root.append(
    el('div', { class: 'card' }, el('h2', { text: 'Général' }),
      el('label', { text: 'Moteur de recherche' }), engine,
      toggle('Animation de démarrage', 'Joue l\'animation NIGHTFALL au lancement de l\'application (1,8 s).', () => P.splash_enabled, (v) => { P.splash_enabled = v; }),
      toggle('Onglets verticaux', 'Affiche les onglets dans une barre latérale.', () => S.vertical_tabs, (v) => { S.vertical_tabs = v; }),
      toggle('Restaurer la session', 'Rouvre les onglets de la dernière session après déverrouillage.', () => S.restore_session, (v) => { S.restore_session = v; }),
    ),
    el('div', { class: 'card' }, el('h2', { text: 'NIGHTFALL LOCK' }),
      el('label', { text: 'Verrouillage automatique après inactivité' }), autoLock,
      toggle('Verrouiller après une mise en veille', 'Détecte la reprise après veille et exige le code.', () => S.lock_on_sleep, (v) => { S.lock_on_sleep = v; }),
      el('p', { class: 'muted', text: 'Changer de code, générer une nouvelle Recovery Key : Security Center.' }),
    ),
    el('div', { class: 'card' }, el('h2', { text: 'Performance' }),
      toggle('Mise en veille des onglets inactifs', 'Sous pression mémoire, décharge les onglets inactifs depuis longtemps (jamais l\'onglet actif, épinglé ou privé).', () => S.perf.discard_enabled, (v) => { S.perf.discard_enabled = v; }),
      toggle('Économie de batterie', 'Réduit les tâches secondaires sur batterie.', () => S.perf.battery_saver, (v) => { S.perf.battery_saver = v; }),
    ),
    el('div', { class: 'card' }, el('h2', { text: 'Développement' }),
      toggle('Outils de développement (Ctrl+Maj+I)', 'Désactivés par défaut : ils peuvent lire les données des pages, y compris les sessions.', () => S.devtools_enabled, (v) => { S.devtools_enabled = v; }),
    ),
  );
}
load();
