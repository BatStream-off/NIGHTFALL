# M-0003 : Audit approfondi de NIGHTFALL Shield et étude comparative

- **Rôle** : Optimiseur
- **Modèle** : opus — raison : étude comparative technique (moteurs, syntaxes, cosmétique, performances) et choix d'architecture : raisonnement complexe et décision structurante
- **Statut** : ACCEPTÉE
- **Branche** : agent/M-0003-audit-shield
- **Copie isolée (worktree)** : oui
- **Dépend de** : rien (phase d'audit de la refonte, aucune fusion)
- **Créée le** : 2026-11-04

## Objectif
Auditer NIGHTFALL Shield, le comparer aux meilleurs projets open source, mesurer couverture, faux positifs, performances et mémoire, et proposer la meilleure architecture.

## Contexte
- Fait établi : Shield utilise **déjà** Brave adblock-rust 0.13.5 (`crates/nf-shield`,
  dépendance `adblock`), avec 9 listes (`default_lists()`), des scriptlets maison sous
  MPL (`crates/nf-shield/scriptlets/`), un moteur cosmétique procédural maison
  (`src/cosmetic.js`), des listes téléchargées à l'exécution et rafraîchies toutes les
  12 h. La question n'est donc pas « adopter adblock-rust » mais : comment mieux l'utiliser
  ou le compléter, et faut-il évoluer.
- Projets à étudier : adblock-rust, uBlock Origin, AdGuard tsurlfilter, EasyList /
  EasyPrivacy, AdGuard Filters, et les autres listes pertinentes (uBlock filters,
  Peter Lowe, listes régionales, anti-cookies…), avec leurs licences.
- Demandes du propriétaire : couverture publicitaire, qualité des filtres, anti-pistage,
  faux positifs, performances du moteur, compatibilité avec les sites modernes,
  exceptions, réglages par site.
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
docs/agents/missions/M-0003-audit-shield/**
crates/nf-shield/**
bench/**
ci/**
.github/workflows/ci.yml
<!-- /scope -->

## Critères d'acceptation
1. Inventaire précis de ce que Shield fait aujourd'hui (réseau, cosmétique générique et
   spécifique, procédural, scriptlets, redirections, CSP, `$removeparam`, exceptions,
   listes de confiance, réglages par site, interface) avec fichier:ligne.
2. Tableau comparatif adblock-rust / uBlock Origin / tsurlfilter (architecture, syntaxe
   prise en charge, cosmétique, scriptlets, redirections, performances, licences) fondé
   sur la lecture de leur code (chemins cités).
3. Liste des syntaxes et options des listes réellement utilisées qui ne sont pas
   prises en charge par NIGHTFALL aujourd'hui, comptée sur les listes actuelles
   (ex. combien de règles ignorées, par motif).
4. Mesures en CI (tests ou bancs dans `crates/nf-shield` / `bench`) : temps de
   compilation du moteur, mémoire, temps par requête (médiane, p99) sur un jeu de
   requêtes réaliste, avec la configuration actuelle et les variantes proposées.
5. Protocole d'évaluation de couverture et de faux positifs reproductible (jeu de pages
   ou de requêtes de test, sans service payant) et premiers résultats, ou explication
   précise de ce qui ne peut pas être mesuré en CI.
6. Recommandation d'architecture argumentée : ce qu'il faut garder, améliorer, ajouter
   (avec gain mesuré ou estimé, coût, risque, licence), classé par priorité.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Corriger ou modifier le comportement du navigateur : cette mission établit des faits
  et propose ; les corrections viendront dans des missions validées par le propriétaire.
- Copier du code GPL (uBlock, tsurlfilter) : idées et techniques seulement.
- Affaiblir une protection pour gagner en vitesse.

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Rapport d'audit détaillé `docs/agents/missions/M-0003-audit-shield/resultats.md`, commité
et poussé sur la branche (sans PR, sauf PR **brouillon** nécessaire pour lancer la
CI). 
Rapport final au format `docs/agents/templates/RAPPORT.md` comme dernier message.
