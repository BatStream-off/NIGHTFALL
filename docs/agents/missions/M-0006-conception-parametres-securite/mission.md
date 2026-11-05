# M-0006 : Conception des Paramètres (centre de contrôle) et du Security Center intégré

- **Rôle** : Architecte
- **Modèle** : opus — raison : réorganisation de tous les réglages existants sans en perdre un, intégration du Security Center et des autorisations par site : conception transversale à haut risque de régression
- **Statut** : PRÉPARÉE
- **Branche** : agent/M-0006-conception-parametres-securite
- **Copie isolée (worktree)** : oui
- **Dépend de** : rien (phase d'audit de la refonte, aucune fusion)
- **Créée le** : 2026-11-04

## Objectif
Concevoir la nouvelle organisation des Paramètres en centre de contrôle, avec le Security Center intégré, en recensant chaque réglage existant et sa nouvelle place, et spécifier ce qui manque par rapport à Chrome et Brave.

## Contexte
- Sections demandées : Général, Apparence et thèmes, Confidentialité, Sécurité,
  NIGHTFALL Shield, VideoGuard, Autorisations des sites, Téléchargements, Navigation,
  Moteurs de recherche, Performances, Accessibilité, Options avancées ; Security Center
  intégré (état des protections, autorisations et exceptions par site, événements de
  sécurité, protections actives, réglages avancés). Pas de doublons ni de réglages
  dispersés. Le Security Center actuel est jugé « trop laid et difficile à utiliser ».
- Existant : `app/ui/settings.*`, `security.*`, `permissions.rs`, `prefs.rs`,
  `state.rs` (`Settings`), Emergency / Auto-Purge, LOCK.
- Manques déjà repérés par rapport à Chrome/Brave (à vérifier) : autorisations par type
  et par site, cookies tiers et effacement à la fermeture, réglages Shield par site,
  thème, barre de favoris, pages au démarrage, gestion des moteurs avec mots-clés,
  accessibilité, préchargement, « demander où enregistrer », import depuis d'autres
  navigateurs. Navigation sécurisée de Google indisponible dans CEF : le dire.
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
docs/agents/missions/M-0006-conception-parametres-securite/**
<!-- /scope -->

## Critères d'acceptation
1. Table complète : chaque réglage et action existants (Paramètres, Security Center,
   menus, pages) → sa section et sa place dans la nouvelle organisation ; aucun oublié
   (recoupé avec `Settings` de `state.rs`, `settings.js`, `security.js` et le registre).
2. Arborescence des Paramètres et du Security Center intégré, avec maquettes HTML
   rendues en captures PNG (vue d'ensemble, une section, autorisations d'un site,
   événements de sécurité).
3. Liste des réglages manquants par rapport à Chrome et Brave, chacun avec : utile ou
   non, faisable dans CEF Alloy ou non (fonction CEF citée), effort, risque.
4. Spécification des autorisations et exceptions par site (Shield, VideoGuard,
   autorisations, cookies, JavaScript…) : modèle de données, compatibilité avec les
   réglages déjà enregistrés des utilisateurs.
5. Découpage en missions d'exécution avec critères d'acceptation vérifiables.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Corriger ou modifier le comportement du navigateur : cette mission établit des faits
  et propose ; les corrections viendront dans des missions validées par le propriétaire.

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Rapport d'audit détaillé `docs/agents/missions/M-0006-conception-parametres-securite/resultats.md`, commité
et poussé sur la branche (sans PR, sauf PR **brouillon** nécessaire pour lancer la
CI). 
Rapport final au format `docs/agents/templates/RAPPORT.md` comme dernier message.
