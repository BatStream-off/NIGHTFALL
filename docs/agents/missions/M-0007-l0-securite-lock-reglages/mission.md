# M-0007 : L0-A — sécurité du verrouillage, réglages robustes, autorisations Chromium

- **Rôle** : Exécuteur
- **Modèle** : opus — raison : code sensible (verrouillage, cycle de vie CEF, données enregistrées des utilisateurs, vie privée)
- **Statut** : EN COURS
- **Branche** : agent/M-0007-l0-securite-lock-reglages
- **Copie isolée (worktree)** : oui
- **Dépend de** : rien. Intègre en premier ; M-0008 se rebase ensuite.
- **Créée le** : 2026-11-05

## Objectif
Corriger les failles et fragilités établies par les audits M-0002, M-0004 et M-0006 qui
touchent le verrouillage, la vie privée et la sécurité des réglages enregistrés.

## Contexte (faits audités)
- **H1** : raccourcis clavier actifs pendant le verrouillage, prouvé sur Windows et macOS (M-0002,
  lignes `DIAG h1`). Cause : ni `on_pre_key_event` (`client.rs:731-822`) ni `api::shortcut`
  (`api.rs:1175`) ne vérifient `is_unlocked()`, alors que `api::handle` filtre par `PRE_UNLOCK`.
  La correction doit laisser la saisie du PIN fonctionner (contrôle dans `shortcut()`), et
  garder `emergency.lockdown` possible.
- **Onglets masqués actifs pendant le verrouillage** : `lock_now` (`api.rs:816-829`),
  `on_locked`/`apply_mode` (`windows.rs:518-560, 622-630`) masquent sans arrêter ; vidéo et son
  continuent ; VideoGuard (`client.rs:204, 227-241, 426-430`) envoie des requêtes et saute des
  segments pendant le verrouillage (M-0004, D-19).
- **Réglages fragiles** : `ShieldConfig` (`crates/nf-shield/src/lib.rs:43`), `VideoGuardConfig`
  (`crates/nf-videoguard/src/lib.rs:43`), `PerfConfig` (`crates/nf-perf/src/lib.rs:51`),
  `AutoPurgeRules` (`crates/nf-emergency/src/rules.rs:67`) n'ont pas `#[serde(default)]` au niveau
  de la structure ; `PerfConfig` et `AutoPurgeRules` sont imbriquées dans `Settings`. `api.rs:752-766`
  (`if let Ok(Some(..))`) garde silencieusement les valeurs par défaut si la lecture échoue, puis la
  sauvegarde suivante écrase tout.
- **Autorisations mémorisées en clair** (décision du propriétaire : les effacer au verrouillage et
  à la fermeture) : NIGHTFALL répond par `cb.cont(ACCEPT/DENY)` (`permissions.rs:210, 287`) ; CEF
  transmet une réponse permanente (`permission_prompt.cc:142-151`), donc Chromium écrit l'origine
  en clair dans `<profil>/chromium/Default/Preferences`. Audit M-0006 : une simple remise à zéro
  changerait le comportement passif (`navigator.permissions.query`, `Notification.permission`) ;
  il faut donc **réappliquer** les décisions mémorisées au déverrouillage
  (`set_content_setting(origine, ALLOW/BLOCK)`), de sorte qu'elles ne soient sur disque que pendant
  que NIGHTFALL est déverrouillé.

## Fichiers autorisés
<!-- scope -->
docs/agents/missions/M-0007-l0-securite-lock-reglages/**
app/src/nf/**
crates/nf-shield/src/**
crates/nf-videoguard/src/**
crates/nf-perf/src/**
crates/nf-emergency/src/**
<!-- /scope -->

## Critères d'acceptation
1. **Raccourcis** : étape d'auto-test qui, NIGHTFALL verrouillé, envoie au moins les raccourcis de
   nouvel onglet, fermeture, nouvelle fenêtre, zoom, plein écran, impression et outils de
   développement, et vérifie qu'aucun n'a d'effet (nombre d'onglets et de fenêtres, zoom, plein
   écran inchangés) ; puis, déverrouillé, qu'ils fonctionnent. Saisie du PIN intacte (le scénario
   existant se déverrouille toujours).
2. **Onglets pendant le verrouillage** : au verrouillage, le son de tous les onglets est coupé et les
   médias en lecture sont mis en pause ; au déverrouillage le son est rétabli (les médias restent en
   pause). VideoGuard n'envoie aucune requête ni n'agit tant que NIGHTFALL est verrouillé. Auto-test
   avec une page locale qui joue un média : état coupé/en pause vérifié pendant le verrouillage.
3. **Réglages robustes** : `#[serde(default)]` au niveau de chaque structure enregistrée ou imbriquée
   dans `Settings` (au minimum les quatre citées, et toute autre trouvée), avec des `Default` sensés ;
   tests unitaires : ancien JSON de chacune avec des champs manquants → chargé sans perte des champs
   présents ; JSON `app` complet avec un `perf` à 3 champs → chargé sans perte. La lecture de
   `Settings` ne doit plus revenir aux valeurs par défaut sans le journaliser.
4. **Autorisations mémorisées** : au verrouillage et à la fermeture, la copie Chromium des décisions
   mémorisées est effacée (`set_content_setting` à DEFAULT pour chaque origine connue) ; au
   déverrouillage elle est réappliquée. Auto-test : mémoriser une décision sur une origine de test,
   verrouiller, lire le fichier `Preferences` (l'origine n'y figure plus après écriture des
   préférences), déverrouiller, vérifier que `navigator.permissions.query` rend de nouveau l'état
   mémorisé. Si un point n'est pas réalisable avec CEF 156, s'arrêter et le dire (BLOQUÉ) plutôt que
   de livrer une demi-mesure.
5. `python3 tools/agents/preserve.py origin/develop HEAD` sans disparition ; scénario d'auto-test
   existant toujours vert ; CI verte sur les deux plateformes.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Supprimer ou dégrader une fonctionnalité du registre (`git show origin/main:docs/agents/FONCTIONNALITES.md`).
- Affaiblir une protection de Chromium ; activer une instrumentation hors de la feature `selftest`.
- Modifier un fichier hors de la liste autorisée (le signaler dans le rapport).

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Branche poussée, une PR **brouillon** vers `develop` (titre « [BROUILLON – NE PAS FUSIONNER] M-0007 … »),
CI entièrement verte sur Windows et macOS (y compris `SELFTEST OK`), rapport au format
`docs/agents/templates/RAPPORT.md` avec, pour chaque critère, la preuve (run, job, ligne
de journal). La fusion se fera seulement après audit et autorisation du propriétaire.
