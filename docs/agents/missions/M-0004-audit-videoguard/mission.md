# M-0004 : Audit complet de VideoGuard face à SponsorBlock

- **Rôle** : Analyste
- **Modèle** : opus (tentative 2) — raison : montée de niveau après audit REFUSÉ (tentative 1 : sonnet)
- **Statut** : EN COURS
- **Branche** : agent/M-0004-audit-videoguard-v2 (tentative 1 : agent/M-0004-audit-videoguard, 72740e4)
- **Copie isolée (worktree)** : oui
- **Dépend de** : rien (phase d'audit de la refonte, aucune fusion)
- **Créée le** : 2026-11-04

## Objectif
Établir ce que VideoGuard fait, ce qui manque ou est incomplet par rapport à SponsorBlock, et spécifier le module complet et son interface intégrée à NIGHTFALL.

## Contexte
- Fait établi : VideoGuard (`crates/nf-videoguard`, `videoguard.js`, réglages dans
  `security.js` / `settings.js`) est un client écrit pour NIGHTFALL de l'API publique
  SponsorBlock (préfixe de hachage), plus l'accélération des publicités vidéo YouTube.
  Il ne contient pas de code de l'extension SponsorBlock (GPL-3.0). Données
  SponsorBlock : CC BY-NC-SA 4.0 (attribution, non commercial). `docs/LICENSES.md`
  indique encore LGPL-3.0 pour l'extension : à vérifier et signaler.
- Points demandés : catégories de segments, options de configuration, comportements de
  saut (automatique, manuel, muet, chapitres, « highlight »), récupération et soumission
  de segments, synchronisation, plateformes vidéo prises en charge, fonctionnalités
  difficiles à retrouver ou manquantes.
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
docs/agents/missions/M-0004-audit-videoguard/**
<!-- /scope -->

## Critères d'acceptation
1. Tableau fonctionnalité par fonctionnalité : SponsorBlock (extension et API, chemins
   cités dans `/home/claude/ref/SponsorBlock*`) / VideoGuard aujourd'hui (fichier:ligne) /
   état (présent, partiel, absent) / faisabilité dans NIGHTFALL (CEF Alloy, sans
   extension) et effort.
2. Vérification des comportements actuels sur des cas concrets (logique des sauts,
   gestion des erreurs réseau, respect de la vie privée : préfixe de hachage, aucun
   identifiant envoyé), avec tests existants cités ou écrits en local.
3. Spécification du module cible : réglages (par catégorie : sauter, demander, muet,
   ignorer), interface moderne (barre de segments sur la vidéo, bandeaux, statistiques),
   soumission de segments (avec ce qu'elle exige : identifiant utilisateur, conditions
   d'utilisation), critères d'acceptation vérifiables.
4. Obligations de licence et d'attribution exactes, et ce qui est permis sans changer
   la licence MPL-2.0 de NIGHTFALL.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Corriger ou modifier le comportement du navigateur : cette mission établit des faits
  et propose ; les corrections viendront dans des missions validées par le propriétaire.
- Copier du code de l'extension SponsorBlock (GPL-3.0).

## Suppressions approuvées par le propriétaire
(aucune)

## Tentative 2 : corrections
La tentative 1 (`resultats.md`, `preuves/`) a été REFUSÉE (`audit-1.md`). La tentative 2
corrige le document existant : chaque point de l'audit est vérifié dans le code avant
d'être appliqué ; un point jugé faux est justifié dans le rapport. Reprendre aussi les
décisions demandées au propriétaire à la lumière des corrections.

## Attribution des commits
```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Wxk7V2hzmn6G9JuqKricoW
```

## Livrable
Rapport d'audit détaillé `docs/agents/missions/M-0004-audit-videoguard/resultats.md`, commité
et poussé sur la branche (sans PR, sauf PR **brouillon** nécessaire pour lancer la
CI). 
Rapport final au format `docs/agents/templates/RAPPORT.md` comme dernier message.
