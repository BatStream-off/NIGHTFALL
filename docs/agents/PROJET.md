# NIGHTFALL : contexte commun pour les agents

À lire en entier avant toute mission, avec `docs/agents/ORGANISATION.md` (la charte)
et ta fiche de mission. En cas de contradiction, la charte et la fiche priment.

NIGHTFALL est un navigateur axé sur la vie privée, écrit en Rust sur Chromium (CEF 156
via la crate `cef` =156.3.0, « cef-rs »), pour Windows x64 et macOS arm64, avec une
interface entièrement en français. Dépôt : github.com/BatStream-off/NIGHTFALL (public).
Branche d'intégration : `develop`. Branche stable : `main`. Tu ne touches jamais ni
l'une ni l'autre : tu travailles sur ta branche `agent/...` et c'est le Directeur qui
intègre.

## Organisation du code
- `crates/*` : modules Rust purs avec tests unitaires (nf-core, nf-crypto, nf-auth
  (verrou par code PIN), nf-storage (base SQLCipher : réglages, favoris, historique,
  téléchargements, sessions), nf-security, nf-emergency, nf-shield (adblock-rust),
  nf-videoguard, nf-vault (mots de passe), nf-perf, nf-downloads, nf-sync, nf-hwkey,
  nf-update (mises à jour signées Ed25519)).
- `app/src/nf/` : le navigateur.
  - `mod.rs` : démarrage, `Settings` de CEF, ligne de commande, crochet de panique.
  - `app.rs` : `on_before_command_line_processing`, `on_context_initialized`.
  - `windows.rs` : fenêtres, onglets, mise en page. CEF Views : Window > [BrowserView
    latéral] + Panel colonne > [BrowserView du haut (interface), Panel de contenu
    (onglets)]. Les menus déroulants sont des BrowserViews en surimpression
    (`popup_open`, `popup_resize`, `popup_close`).
  - `client.rs` : gestionnaires CefClient (affichage, cycle de vie, chargement,
    requêtes, Shield, raccourcis `on_pre_key_event`, téléchargements, autorisations).
  - `api.rs` : API JSON interne des pages d'interface (`handle(caller, cmd, body)`).
  - `state.rs` : `Core`, `Settings` (serde, `#[serde(default)]`, stockés chiffrés),
    journal `state::log`. `ui_thread.rs` : `on_ui`, `on_ui_delayed`, `background`.
  - `selftest.rs` : test de bout en bout (feature `selftest`, CI seulement).
  - `assets.rs` : chaque fichier de `app/ui/` DOIT y être déclaré pour être servi.
  - Autres : `browsing.rs`, `browsing_data.rs`, `downloads.rs`, `https_first.rs`,
    `omnibox.rs`, `permissions.rs`, `prefs.rs`, `scheme.rs`, `security.rs`,
    `session.rs`, `timers.rs`, `updater.rs`, `idle.rs`, `mac.rs`.
- `app/ui/` : pages servies à `nightfall://app/` sous une CSP stricte (aucun script
  ou gestionnaire en ligne ; styles en ligne permis). `nf.css` = jetons de design,
  `nf.js` = utilitaires (`api(cmd, body)`, `el(...)`, `$`, `toast`, `openPage`...).
  `shell.html/css/js` = barre d'onglets et barre d'outils (`?mode=top`), onglets
  verticaux (`mode=side`), menus (`mode=popup&panel=...`). Pages internes : newtab,
  settings, security, history, bookmarks, downloads, passwords, extensions, about,
  insecure.
- Référence des liaisons CEF (signatures exactes), si présente sur la machine :
  `/home/claude/ref/cef-rs/cef/src/bindings/x86_64_pc_windows_msvc.rs`. Les en-têtes
  C++ de CEF font foi (`GetFoo` → `foo()`).

## Pièges connus (à lire avant de coder)
- Les méthodes par défaut des traits cef-rs renvoient 0/false/vide : surcharge tout
  ce dont tu as besoin (ex. `can_resize` d'une fenêtre).
- L'état des fenêtres et onglets est dans un `RefCell` local au thread (`WINDOWS`).
  Utilise `with_window(id, |ctx| ...)`. Dans cette fermeture, n'appelle jamais une
  méthode CEF qui peut rappeler NIGHTFALL de façon synchrone (fermer un navigateur,
  ajouter une vue qui crée un navigateur, focus...) : collecte les poignées, appelle
  CEF après (voir `close_tab`, `prepare_window_close`).
- Travail sur le thread UI depuis l'API : `on_ui(move || ...)`. Travail bloquant :
  `background(work, done)`.
- `Settings` : nouveaux champs avec une valeur par défaut sensée dans `impl Default`.
  Ne renomme ni ne supprime jamais un champ existant (les réglages enregistrés des
  utilisateurs doivent continuer à se charger).
- Interface : `[hidden]` est forcé en `display:none` ; déclare les nouveaux fichiers
  dans `assets.rs` ; passe par l'API, ne contacte jamais d'autre origine.
- Les types MIME servis par `scheme.rs` sont nus (pas de `; charset`).
- Sécurité (non négociable) : aucun commutateur Chromium qui affaiblit la sécurité
  (voir `crates/nf-security/src/switches.rs`), aucune télémétrie, aucun service payant
  ni clé d'API, aucune installation silencieuse d'extension, rien qui envoie les
  données de l'utilisateur. Ce qui est impossible avec CEF Alloy est dit honnêtement
  dans l'interface. Tu ne lis, n'affiches ni ne publies jamais de secret.

## Compiler et tester
- crates.io et le CDN de CEF ne sont pas joignables depuis cette machine : la
  compilation et les tests se font dans GitHub Actions (`.github/workflows/ci.yml`) :
  tests des modules sur 3 OS, compilation réelle Windows et macOS, test de démarrage
  avec capture d'écran vérifiée, auto-test de bout en bout (`app/src/nf/selftest.rs`,
  se termine par `SELFTEST OK`), contrôles de sécurité, préservation des
  fonctionnalités.
- Pour lancer la CI sur ta branche : `git push -u origin <ta branche agent/...>`, puis
  une seule PR **brouillon** vers `develop` :
  `gh api repos/BatStream-off/NIGHTFALL/pulls -X POST -f title="M-XXXX : ..." -f head=<branche> -f base=develop -F draft=true -f body="..."`.
  Chaque push relance la CI (25 à 35 min pour les jobs Application). Suivi :
  `gh run list -R BatStream-off/NIGHTFALL --branch <branche> -L 3`,
  `gh run view <id> -R BatStream-off/NIGHTFALL --json jobs`, journaux :
  `gh api repos/BatStream-off/NIGHTFALL/actions/jobs/<job_id>/logs`. Les artefacts ne
  sont pas téléchargeables d'ici. Les captures de la CI sont dans les journaux entre
  `NFSHOT-BEGIN`/`NFSHOT-END` (démarrage) et `NFSHOT2-BEGIN`/`NFSHOT2-END` (fin
  d'auto-test), en PNG base64 : décode-les et regarde-les avec l'outil Read.
- Si `git fetch` ne met pas à jour `origin/develop`, vérifie
  `git config --get-all remote.origin.fetch` (doit valoir
  `+refs/heads/*:refs/remotes/origin/*`).
- Erreurs Rust : `grep -n "error\[" -A12` dans le journal du job. Regroupe tes
  changements : chaque tour de CI est lent ; relis ton diff avant de pousser (imports
  de traits comme `use cef::{ImplBrowser, ImplFrame, ImplBrowserHost}`).
- Interface vérifiable localement dans Chromium sans tête : copie `app/ui` dans un
  dossier temporaire, remplace `'nightfall://app/api/'` par `'/api/'` dans le `nf.js`
  de la copie, sers-la avec `ci/ui_mock_server.py` (variables `NF_UI_DIR`, `NF_PORT`,
  `NF_UNLOCKED=1`) ; Playwright est installé globalement
  (`NODE_PATH=$(npm root -g) node script.js`). Utilise un port à toi, n'arrête jamais
  les processus des autres.
- Ajoute des vérifications d'auto-test pour ce que tu construis quand c'est testable
  sans humain (le scénario existant doit rester vert et finir sur https://example.com
  avec `SELFTEST OK`).

## Git
- Seulement ta branche `agent/...`, dans ta copie de travail. Jamais de push forcé,
  jamais de fusion, jamais de sortie du mode brouillon : ce sont des actions du
  Directeur (et le garde-fou les refuse).
- Messages de commit en français, terminés par les lignes d'attribution que le
  Directeur te donne dans ta fiche de mission.
