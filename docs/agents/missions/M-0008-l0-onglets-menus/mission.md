# M-0008 : L0-B — fermeture des onglets, menus déroulants, bouton Accueil

- **Rôle** : Exécuteur
- **Modèle** : opus — raison : cycle de vie des navigateurs CEF et piège `DoClose` (risque de fermer toute la fenêtre)
- **Statut** : EN COURS
- **Branche** : agent/M-0008-l0-onglets-menus
- **Copie isolée (worktree)** : oui
- **Dépend de** : M-0007 intègre en premier ; cette branche se rebase sur `develop` ensuite si besoin.
- **Créée le** : 2026-11-05

## Objectif
Corriger les bugs d'interaction reproduits par M-0002 : la croix ne ferme pas l'onglet, les menus
déroulants fuient (navigateur et processus conservés), la course des menus sur macOS, le bouton
Accueil sans effet depuis une page web.

## Contexte (faits audités)
- **Croix** : 0 fermeture sur 60 vrais clics (Windows et macOS). L'appui (`mousedown`) active l'onglet
  (`shell.js:211-216`), ce qui provoque l'événement `tabs` et la reconstruction de toute la barre
  (`shell.js:197-199`, `windows.rs:847`) avant le relâchement ; le `click` est perdu. Le clic du
  milieu et `element.click()` ferment. Fermer un onglet d'arrière-plan active aussi cet onglet.
  Vérifier aussi le mode onglets verticaux (`shell.js:273-287`).
- **Fuite des menus** : chaque menu fermé garde son navigateur et son processus de rendu
  (`windows.rs:1412-1426` ; `crees=187 detruits=89`). **Piège** (audit M-0002) : `do_close` renvoie 0
  pour les navigateurs d'interface (`client.rs:287-299`) ; si CEF reçoit 0, il demande la fermeture
  de la fenêtre parente (`CanClose`, `windows.rs:227-229`) → toute la fenêtre NIGHTFALL se fermerait.
  Comparer avec `close_tab_inner` (vue détachée + `mark_closing` avant `close_browser`), qui libère
  correctement les onglets.
- **Course des menus (macOS)** : `popup.close` sans nom de panneau (`api.rs:246`, `shell.js:448`) ferme
  le menu suivant ; nommer le panneau fermé.
- **Accueil** : refusé depuis une page web (`windows.rs:1194-1211`, `state.rs:470-485`,
  `client.rs:261`) ; corriger sans laisser d'autorisation `INTERNAL_TABS` en attente
  (`navigate_active` `windows.rs:1198`, `windows.rs:812`) ; `navigate` et `omnibox.pick` passent aussi
  par `navigate_active`.

## Fichiers autorisés
<!-- scope -->
docs/agents/missions/M-0008-l0-onglets-menus/**
app/src/nf/**
app/ui/shell.js
app/ui/shell.css
app/ui/shell.html
<!-- /scope -->

## Critères d'acceptation
1. **Croix** : étape d'auto-test qui clique réellement (événements souris natifs via DevTools
   `Input.dispatchMouseEvent`, appui puis relâchement séparés) sur la croix d'un onglet actif et d'un
   onglet d'arrière-plan, en mode horizontal et vertical : l'onglet visé est fermé à chaque essai
   (au moins 5 essais par cas), et fermer un onglet d'arrière-plan ne change pas l'onglet actif.
2. **Fuite des menus** : après l'ouverture et la fermeture de 20 menus, le nombre de navigateurs
   d'interface vivants revient à sa valeur initiale (± 0), et la fenêtre reste ouverte avec ses
   onglets. Le risque `DoClose` est traité explicitement dans le code et testé.
3. **Course des menus** : ouvrir un menu, puis en ouvrir un autre : le second reste ouvert (auto-test).
4. **Accueil** : depuis une page web, le bouton Accueil ouvre la page Nouvel onglet ; aucune
   autorisation interne ne reste en attente (vérifié par l'auto-test).
5. `python3 tools/agents/preserve.py origin/develop HEAD` sans disparition ; scénario d'auto-test
   existant toujours vert ; CI verte sur les deux plateformes ; captures de fin d'auto-test regardées.

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
Branche poussée, une PR **brouillon** vers `develop` (titre « [BROUILLON – NE PAS FUSIONNER] M-0008 … »),
CI entièrement verte sur Windows et macOS (y compris `SELFTEST OK`), rapport au format
`docs/agents/templates/RAPPORT.md` avec, pour chaque critère, la preuve (run, job, ligne
de journal). La fusion se fera seulement après audit et autorisation du propriétaire.
