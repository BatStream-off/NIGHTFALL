# M-0002 : Diagnostic des bugs et mesure des performances

- **Rôle** : Optimiseur
- **Modèle** : opus — raison : diagnostic du cycle de vie des navigateurs CEF, des threads et de l'état WINDOWS : code délicat où une erreur d'analyse coûterait cher
- **Statut** : ACCEPTÉE
- **Branche** : agent/M-0002-diagnostic-bugs-perf
- **Copie isolée (worktree)** : oui
- **Dépend de** : rien (phase d'audit de la refonte, aucune fusion)
- **Créée le** : 2026-11-04

## Objectif
Reproduire, diagnostiquer et mesurer chaque problème signalé par le propriétaire, et établir une mesure de référence des performances, sans corriger.

## Contexte
- Problèmes signalés : fermeture incorrecte des onglets (« il se ferme mal »), boutons
  défectueux (« impression que certains boutons sont bug »), chargement trop lent des
  pages, ralentissements et manque de fluidité, incohérences d'interaction, raccourcis
  clavier actifs quand NIGHTFALL est verrouillé (H1 de `M-0001/audit-1.md` : `shortcut()`
  et `on_pre_key_event` sans contrôle de verrouillage), problèmes éventuels de mémoire.
- Fermeture d'onglet : `windows.rs` `close_tab_inner`, `client.rs` `do_close` /
  `on_before_close`, et l'animation côté `app/ui/shell.js`.
- Suspects de lenteur à **mesurer** (pas à supposer) : vérification Shield sur chaque
  requête (`on_before_resource_load`), scripts injectés à chaque `on_load_start`
  (Shield, VideoGuard, cosmétique), HTTPS d'abord, DNS sécurisé, rendu de l'interface.
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
docs/agents/missions/M-0002-diagnostic-bugs-perf/**
app/src/nf/selftest.rs
app/src/nf/**/*.rs
ci/**
.github/workflows/ci.yml
<!-- /scope -->

## Critères d'acceptation
1. Pour chaque problème signalé : reproduction (ou échec de reproduction documenté),
   cause établie avec fichier:ligne, preuve (journal de CI, capture décodée, mesure),
   et correction proposée avec son risque. Constaté et supposé clairement séparés.
2. Boutons : inventaire de chaque bouton et entrée de menu de `shell` (barre, menus
   déroulants, menus contextuels) et de chaque page interne, avec un test automatique
   qui les active un par un (auto-test ou rendu Playwright) et le résultat de chacun.
3. H1 : test d'auto-test qui prouve (ou infirme) que les raccourcis agissent sur
   l'écran de verrouillage, sur Windows et macOS.
4. Performances de référence, mesurées en CI sur Windows et macOS, au moins 5
   répétitions, médiane et dispersion : temps de chargement de pages de référence
   (pages simples et pages lourdes accessibles en CI) avec Shield actif et inactif, avec
   et sans HTTPS d'abord et DNS sécurisé ; temps de démarrage ; mémoire après N
   onglets ; temps de réponse de l'interface (ouverture d'un menu, d'un onglet).
5. Toute instrumentation est derrière la feature `selftest` ou une feature de mesure
   dédiée, jamais active dans une version publiée ; `preserve.py` sans disparition.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Corriger ou modifier le comportement du navigateur : cette mission établit des faits
  et propose ; les corrections viendront dans des missions validées par le propriétaire.
- Laisser une instrumentation active hors des builds de test.

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Rapport d'audit détaillé `docs/agents/missions/M-0002-diagnostic-bugs-perf/resultats.md`, commité
et poussé sur la branche (sans PR, sauf PR **brouillon** nécessaire pour lancer la
CI). La branche de mesure reste non fusionnée ; le Directeur s'en servira pour mesurer « après ».
Rapport final au format `docs/agents/templates/RAPPORT.md` comme dernier message.
