# M-0001 : Registre initial des fonctionnalités à préserver

- **Rôle** : Analyste
- **Modèle** : sonnet — raison : inventaire étendu demandant de lire et recouper beaucoup de code (Rust, JS, CI) ; Haiku risquerait des oublis, or l'exhaustivité est l'objet même de la mission.
- **Statut** : EN COURS
- **Branche** : agent/M-0001-registre-fonctionnalites
- **Copie isolée (worktree)** : oui
- **Dépend de** : PR #19 (organisation multi-agents) — la branche part de `origin/orga/multi-agents`
- **Créée le** : 2026-11-04

## Objectif
Établir dans `docs/agents/FONCTIONNALITES.md` le registre complet des fonctionnalités
que NIGHTFALL possède aujourd'hui (branche `develop`, version 0.1.4), avec pour chacune
l'endroit où elle est réalisée et la preuve automatique qui montre qu'elle fonctionne
(ou l'absence de preuve). Ce registre sert à garantir qu'aucune fonctionnalité ne
sera perdue dans les versions suivantes.

## Contexte
- Le propriétaire exige : « Préserver absolument les fonctionnalités existantes » et
  « Ne jamais supprimer une fonctionnalité sans mon accord ».
- `tools/agents/preserve.py` suit déjà automatiquement les éléments techniques
  (commandes d'API, réglages, tests, étapes d'auto-test, fichiers d'interface,
  identifiants d'éléments). Le registre est la vue **fonctionnelle**, telle qu'un
  utilisateur la vit : « fermer un onglet », « rouvrir l'onglet fermé », « bloquer les
  publicités YouTube », « verrouiller après inactivité »…
- Sources à parcourir au minimum : `app/src/nf/*.rs` (surtout `api.rs`, `windows.rs`,
  `client.rs`, `selftest.rs`), `app/ui/*.js` et `*.html` (menus, boutons, pages,
  Paramètres), `crates/*`, `docs/STATUS.md`, `docs/RELEASE_NOTES.md`,
  `.github/workflows/ci.yml`, `ci/*`.
- Raccourcis clavier : `on_pre_key_event` dans `client.rs`.

## Fichiers autorisés
<!-- scope -->
docs/agents/FONCTIONNALITES.md
docs/agents/missions/M-0001-registre-fonctionnalites/**
<!-- /scope -->

## Critères d'acceptation
1. Chaque fonctionnalité visible par l'utilisateur a une ligne `| F-xxx | ... |` au
   format décrit en tête de `FONCTIONNALITES.md`, avec des identifiants F-001, F-002…
   uniques et groupés par domaine (fenêtres et onglets, navigation, barre d'adresse,
   menus, pages internes, Paramètres (chaque réglage), LOCK et code, coffre de mots de
   passe, Security Center et Emergency, Shield, VideoGuard, téléchargements, favoris,
   historique, sessions, navigation privée, mises à jour, performances, empaquetage
   et installation).
2. La colonne « Où » cite les fichiers (et la fonction quand c'est utile) réellement
   présents dans le code ; chaque citation est exacte.
3. La colonne « Preuve automatique » cite le test exact (`chemin::nom_du_test`, étape
   `step("...")` de l'auto-test, contrôle de la CI) ou écrit « aucune ». Aucune preuve
   inventée : si tu n'as pas trouvé le test, écris « aucune ».
4. Chaque raccourci clavier existant figure sous la forme ``raccourci: `...` ``.
5. Les limites connues et honnêtes (ex. pas de demande « Quitter la page ? »,
   extensions Chrome indisponibles) sont listées dans une section « Limites connues »
   distincte, sans être présentées comme des fonctionnalités.
6. Une section finale « Couverture » donne le nombre de fonctionnalités, combien ont
   une preuve automatique, et la liste de celles qui n'en ont aucune (utile pour la 0.1.5).
7. `python3 tools/agents/preserve.py origin/develop HEAD` ne signale aucune disparition
   et `python3 tools/agents/mission.py scope M-0001 --base origin/orga/multi-agents --ref HEAD`
   respecte le périmètre.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Modifier un autre fichier que ceux autorisés (aucune ligne de code de l'application).
- Présenter comme existante une fonctionnalité qui n'est pas réellement réalisée dans
  le code (une case d'interface sans effet réel doit être signalée dans « Limites
  connues » ou « Hors périmètre »).

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Commit(s) sur `agent/M-0001-registre-fonctionnalites`, poussés vers `origin` (pas de PR :
le Directeur l'ouvrira une fois la PR #19 intégrée). Rapport final au format
`docs/agents/templates/RAPPORT.md`.
