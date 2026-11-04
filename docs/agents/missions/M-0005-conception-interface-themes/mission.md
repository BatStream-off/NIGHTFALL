# M-0005 : Conception de la nouvelle interface, de l'identité visuelle et du système de thèmes

- **Rôle** : Architecte
- **Modèle** : opus — raison : conception structurante (identité, système de design, barre de titre intégrée sur deux plateformes, moteur de thèmes) : décisions difficiles à défaire
- **Statut** : PRÉPARÉE
- **Branche** : agent/M-0005-conception-interface-themes
- **Copie isolée (worktree)** : oui
- **Dépend de** : rien (phase d'audit de la refonte, aucune fusion)
- **Créée le** : 2026-11-04

## Objectif
Concevoir l'interface complète de NIGHTFALL (identité, système de design, header avec barre de titre intégrée, onglets, menus, panneaux, animations) et le système de thèmes avec son éditeur, avec maquettes et prototype de faisabilité.

## Contexte
- Demandes du propriétaire : design moderne, élégant, minimaliste et premium, identité
  propre (pas un navigateur générique des années 2010), header modernisé avec barre de
  titre intégrée, onglets/menus/panneaux repensés, tailles, espacements, polices et
  icônes harmonisés, animations modernes, libellés courts, informations avancées à la
  demande. Brave peut inspirer la disposition et l'ergonomie, sans ses logos ni icônes.
- Thèmes : créer, modifier les couleurs, personnaliser header, onglets, menus, panneaux,
  accent et arrière-plans, enregistrer / modifier / dupliquer / supprimer, aperçu avant
  application, thèmes prédéfinis, système de design centralisé couvrant tout le
  navigateur (y compris les pages internes).
- Existant : `app/ui/nf.css` (jetons), `shell.*`, pages internes, police Chakra Petch
  (OFL), emblème et fonds `app/ui/wallpapers/`. Fenêtres : CEF Views (`windows.rs`).
- Barre de titre : Windows (fenêtre sans cadre, boutons dessinés, zone de déplacement,
  redimensionnement par les bords, Snap) et macOS (boutons natifs dans la barre
  d'onglets). Vérifier dans les liaisons cef-rs et les en-têtes CEF ce qui existe
  (`is_frameless`, zones déplaçables, `get_titlebar_height`, etc.).
- Phase d'**audit** : aucune modification n'est intégrée. Le propriétaire valide les
  résultats et le plan avant toute modification importante (charte, règles 9 et 10).
- Demande du propriétaire : `docs/agents/missions/DEMANDE-2026-11-04-refonte.md`.
- Registre des fonctionnalités : `docs/agents/FONCTIONNALITES.md` (en cours de
  correction par M-0001 ; s'en servir avec prudence et vérifier dans le code).
- Copies de référence en lecture seule : `/home/claude/ref/` (adblock-rust, uBlock,
  tsurlfilter, easylist, AdguardFilters (partiel), SponsorBlock, SponsorBlockServer,
  cef-rs). Code GPL : étude seulement, jamais de copie (règle 12).

## Fichiers autorisés
<!-- scope -->
docs/agents/missions/M-0005-conception-interface-themes/**
app/**
.github/workflows/ci.yml
ci/**
<!-- /scope -->

## Critères d'acceptation
1. Audit de l'interface actuelle : incohérences de tailles, d'espacements, de polices,
   d'icônes, de couleurs, page par page, avec captures (rendu Playwright local et
   captures de la CI).
2. Système de design : jetons (couleurs, typographie, espacements, rayons, ombres,
   durées et courbes d'animation, tailles d'icônes et de contrôles), jeu d'icônes
   original, règles de composants (onglet, bouton, menu, panneau, champ, interrupteur).
3. Maquettes HTML du header (barre de titre intégrée), de la barre d'onglets, des
   menus, des panneaux et d'une page interne, rendues et jointes en captures PNG dans
   le dossier de la mission.
4. Barre de titre intégrée : prototype sur la branche, passé en CI sur Windows et macOS,
   avec captures et conclusion claire (fiable / fiable avec réserves / non fiable).
   S'il n'est pas fiable sur une plateforme, le dire : le Directeur consultera le
   propriétaire.
5. Architecture du moteur de thèmes (stockage chiffré dans les réglages, application à
   la barre et à toutes les pages internes sans rechargement, aperçu, thèmes prédéfinis,
   import/export éventuel) et maquette de l'éditeur.
6. Découpage en missions d'exécution (fichiers, ordre, critères d'acceptation), avec
   pour chaque fonctionnalité existante touchée la façon dont elle est préservée.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Corriger ou modifier le comportement du navigateur : cette mission établit des faits
  et propose ; les corrections viendront dans des missions validées par le propriétaire.
- Utiliser des logos, icônes ou images de Brave ou d'un autre produit.

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Rapport d'audit détaillé `docs/agents/missions/M-0005-conception-interface-themes/resultats.md`, commité
et poussé sur la branche (sans PR, sauf PR **brouillon** nécessaire pour lancer la
CI). Le prototype de barre de titre reste sur la branche, non fusionné.
Rapport final au format `docs/agents/templates/RAPPORT.md` comme dernier message.
