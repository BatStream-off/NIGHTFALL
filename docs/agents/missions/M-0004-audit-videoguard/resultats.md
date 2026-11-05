# M-0004 : audit de VideoGuard face à SponsorBlock et spécification du module cible

Auteur : Analyste. Tentative 1 : Sonnet 5.5, 2026-11-04 (refusée, `audit-1.md`). Tentative 2 : Opus 5.5, 2026-11-05, corrections de chaque point de l'audit après vérification dans le code (récapitulatif au §13). Aucune modification du navigateur. Ce document établit des faits et propose ; les décisions appartiennent au Directeur et au propriétaire.

Conventions :
- `fichier:ligne` sans préfixe = dépôt NIGHTFALL (commit `80f7924`). Tentative 2 : `git diff 80f7924 adffa4f -- crates app` et `git diff adffa4f origin/develop -- crates/nf-videoguard app` sont vides (`origin/develop` = `2408f95`) : toutes les références au code restent valables.
- `SB:` = copie de référence `/home/claude/ref/SponsorBlock` (extension, manifeste version 6.1.7, `manifest/manifest.json:4`, commit local `454c2d8`).
- `SBS:` = copie de référence `/home/claude/ref/SponsorBlockServer` (commit local `b691d94`).
- Statuts : VÉRIFIÉ (exécuté ou lu et confronté), LU (déduit de la lecture du code sans exécution), NON VÉRIFIÉ (écrit comme tel avec la raison).
- Rien de SponsorBlock n'a été copié : SB et SBS ont été lus pour en décrire le comportement en mots. Voir §9 pour la méthode de « salle blanche » recommandée.

---

## 0. Résumé pour le Directeur

1. **La prémisse « VideoGuard est basé sur du code repris de SponsorBlock » est inexacte.** VideoGuard est un client original de l'API publique : `crates/nf-videoguard/src/lib.rs:3-6` le dit, et une comparaison automatique (toute ligne de 30 caractères ou plus de `videoguard.js` et de `lib.rs` recherchée telle quelle dans `SB/src` et `SBS/src`) ne trouve **0 ligne commune** sur 150 + 286 lignes. Limite : cette méthode ne détecte pas une réécriture proche ; elle écarte seulement une copie littérale.
2. **VideoGuard couvre une fraction étroite de SponsorBlock** : 8 catégories sur 12 côté serveur, un seul type d'action de segment (`skip`) sur 5 (`skip`, `mute`, `full`, `poi`, `chapter`), donc pas de sourdine des segments de type `mute` ni d'étiquette de vidéo entière, pas de barre de segments, pas de vote ni de soumission, pas de statistiques, pas de profils par chaîne, une seule plateforme (YouTube, onglet principal).
3. **Points forts à préserver** : préfixe de hachage de 4 caractères (plus discret que l'extension qui en envoie 5 pour les segments, `SB/src/utils/segmentData.ts:62`), aucun identifiant ni cookie envoyé, requête faite par le cœur du navigateur et non par la page (`client.rs:227-241`), accélération des publicités vidéo (F-156), valeurs par défaut existantes.
4. **Défaut le plus grave, établi par lecture : VideoGuard reste actif pendant le verrouillage** (D-19). Le verrouillage masque les onglets sans les arrêter ; un changement de vidéo dans un onglet masqué (lecture automatique, liste de lecture) déclenche quand même une requête à SponsorBlock, et le script de page continue de sauter et d'accélérer les publicités (`api.rs:816-829`, `windows.rs:622-630`, `client.rs:204,227-241`). La tentative 1 affirmait l'inverse (VG-25) : c'était faux.
5. **Défauts du script de page vérifiés par exécution sur un lecteur simulé** (banc, §4.2 ; pas sur le vrai lecteur YouTube ni dans CEF) : en mode « Proposer de sauter », l'avis disparaît au bout de 6 s et n'est jamais reproposé (D-02) ; deux segments imbriqués (10-40 et 15-20) font sauter à 40 puis revenir à 20 quand la lecture entre dans le segment intérieur (D-20) ; un segment qui commence à 0 n'est jamais réarmé (D-21) ; une nouvelle injection pour la même vidéo efface la mémoire des sauts, ce qui annule un « Revenir » (D-22) ; après une publicité, le script remet la vitesse à 1 (D-01) — que YouTube restaure ou non la vitesse de l'utilisateur de lui-même **n'a pas été vérifié** sur le vrai lecteur, donc l'effet réel de D-01 reste à confirmer. Plusieurs autres écarts sont établis par lecture seulement (requête inutile quand tout est sur « Ignorer », absence de cache, navigation privée traitée comme la navigation normale, lecteurs intégrés sans segments, courses entre réponses, contournement probable d'un proxy système).
6. **Licences** : l'extension SponsorBlock est sous GPL-3.0-ou-ultérieure (`SB/README.md:76-78`), et **non LGPL** comme l'écrit `docs/LICENSES.md:17`. Les données sont sous CC BY-NC-SA 4.0 « sauf permission explicite » (`SBS/README.MD:11`). Une attribution existe à deux endroits : Centre de sécurité (`lib.rs:17-18` via `security.js:97`) et page « À propos » (`app/ui/about.html:14`) ; toutes deux nomment SponsorBlock et la licence mais **sans lien vers la licence ni vers la source, sans avis de droit d'auteur ni renvoi à l'exclusion de garantie** demandés par la section 3(a)(1) de la licence (texte lu, §9.3). La page « Database-and-API-License » de SponsorBlock reste inaccessible d'ici.
7. **Décisions du propriétaire** (§11, reprises après correction) : soumission et vote, navigation privée, licence et texte d'attribution, présentation de l'accélération des publicités, et deux nouvelles : valeurs par défaut des interrupteurs « sourdine » et « vidéo entière », suspension de VideoGuard (et plus largement de la lecture) pendant le verrouillage.

---

## 1. Méthode, sources et limites

| Source | Usage | Remarque |
|---|---|---|
| Code NIGHTFALL (liste §2) | lecture complète de `nf-videoguard` et de ses points d'intégration | tout est cité en `fichier:ligne` |
| `SB/src/**`, `SB/config.json.example`, `SB/manifest/` | comportement de l'extension | **`SB/maze-utils` est un sous-module absent de la copie** (dossier vide) : détection de la vidéo, de la publicité, du lecteur, génération de l'identifiant utilisateur et requêtes en arrière-plan n'ont pas pu être lues |
| `SBS/src/**`, `SBS/README.MD` | contrat réel de l'API (paramètres, erreurs, règles de soumission) | licence du serveur AGPL-3.0-only (`SBS/package.json:20`) |
| CI de `develop` (run 41825393127, job 126558718472 « Modules (ubuntu-latest) ») | exécution des tests Rust de `nf-videoguard` | 7 tests passés (journal à 12:00:00Z, cible `nf_videoguard`) |
| Banc local `preuves/videoguard_js_harness.js` | exécution du vrai `videoguard.js` dans Chromium sans tête avec un lecteur simulé | sortie dans `preuves/videoguard_js_harness.sortie.txt` |
| Documentation publique de l'API (wiki.sponsor.ajay.app) | contrat officiel | **non consultable depuis cette machine** (domaine non autorisé pour WebFetch ; recherche web sans résultat exploitable ; tentative 2 : `wiki.sponsor.ajay.app` et la page GitHub « Database-and-API-License » refusés par le proxy). Le contrat a donc été établi à partir du code du serveur (`SBS`), qui fait foi pour le comportement, pas pour les engagements de l'opérateur |
| Texte légal CC BY-NC-SA 4.0 | obligations exactes (§9.3) | la source indiquée par l'audit (`chromium.googlesource.com`, `licenses/CC-BY-NC-SA-4.0`) **n'est pas accessible** d'ici : connexion refusée par le proxy (HTTP 403 au CONNECT), comme `creativecommons.org`, `raw.githubusercontent.com` (SPDX) et l'API GitHub pour `spdx/license-list-data`. Texte lu dans une copie locale : `/usr/share/texlive/texmf-dist/tex/latex/doclicense/license-texts/doclicense-CC-by-nc-sa-4.0-plaintext.tex` (paquet Debian `texlive-latex-extra`, paquet CTAN `doclicense`, 438 lignes, SHA-256 `e66c269d…2b7bbd`). Ce n'est pas la source officielle : à recouper avec `creativecommons.org/licenses/by-nc-sa/4.0/legalcode` par quelqu'un qui y a accès |

Ce que je n'ai **pas** pu faire : compiler ou exécuter les tests Rust localement (crates.io inaccessible, aucun registre local) ; interroger le serveur public `sponsor.ajay.app` (aucune requête réseau vers lui n'a été faite) ; lancer NIGHTFALL (CEF indisponible). Les comportements dans CEF réel (plein écran, onglet en arrière-plan, macOS) sont donc NON VÉRIFIÉS.

---

## 2. Inventaire de VideoGuard aujourd'hui

### 2.1 Fichiers et rôle

| Élément | Emplacement | Rôle |
|---|---|---|
| Module Rust | `crates/nf-videoguard/src/lib.rs` (286 lignes dont 69 de tests) | configuration, extraction de l'identifiant vidéo, préfixe de hachage, requête, analyse, génération des scripts |
| Script de page | `crates/nf-videoguard/src/videoguard.js` (150 lignes), inclus par `include_str!` (`lib.rs:216`) | saut, avis, accélération des publicités |
| Déclenchement des segments | `app/src/nf/client.rs:227-241` (dans `on_navigation`, appelé par `on_address_change`, `client.rs:129`) | si l'URL de l'onglet est une vidéo YouTube : requête en arrière-plan puis injection dans le cadre principal (`windows.rs:1505-1509`) |
| Déclenchement des options | `client.rs:426-430` (`on_load_start`) | injecte le script avec les options (accélération des publicités) dans chaque cadre dont l'URL est YouTube |
| API interne | `api.rs:497-506` (`videoguard.set`), chargement `api.rs:756-758` | enregistre la configuration sans validation |
| État et stockage | `state.rs:15`, `state.rs:346`, `state.rs:377` (réglage chiffré `videoguard`), valeur par défaut `mod.rs:123` | |
| Interface actuelle | `app/ui/security.js:85-98` (dans le Centre de sécurité) ; état `security.rs:84-88` ; ligne d'état `security.js:50` | interrupteur, accélération des publicités, une liste déroulante par catégorie, phrase d'attribution |
| Paramètres | aucun réglage VideoGuard dans `settings.js` ; une seule mention, le texte de réinitialisation (`settings.js:569`) | réglages dispersés (Centre de sécurité seulement) |
| Synchronisation | `crates/nf-sync/src/lib.rs:1-7,16-18` : uniquement les favoris (`FILE_NAME = "nightfall-bookmarks.nfsync"`, format `NFSYNC01`) | VideoGuard n'est pas synchronisé |
| Verrouillage | `api.rs:816-829` (`lock_now`), `windows.rs:622-630` (`on_locked`), `windows.rs:518-560` (`apply_mode`) | rien ne concerne VideoGuard : voir D-19 |
| Tests d'auto-test CI | aucune occurrence de `videoguard`, `youtube` ou `sponsor` dans `app/src/nf/selftest.rs` (recherche faite) | pas de scénario de bout en bout |

### 2.2 Flux réel

1. Changement d'adresse du cadre principal → `on_navigation` (`client.rs:204`).
2. Si `nf_videoguard::video_id(url)` est `Some` et la configuration est active (`client.rs:228-230`) : `background(fetch_segments, ...)`. Ni `on_navigation` ni la fonction de fin n'appellent `is_unlocked()` (`state.rs:352`) : la requête part aussi quand NIGHTFALL est verrouillé (D-19).
3. `fetch_segments` (`lib.rs:168-189`) envoie `GET https://sponsor.ajay.app/api/skipSegments/<4 hex>` avec `categories=[...actives]`, `actionTypes=["skip"]`, `User-Agent: NIGHTFALL-VideoGuard/0.1`, délai 10 s ; 404 = aucun segment ; autre erreur → journal seulement (`client.rs:238`).
4. `parse_response` (`lib.rs:147-165`) garde la vidéo demandée, les actions `skip` (ou vides), applique la configuration par catégorie et la durée minimale (1 s par défaut, `lib.rs:70`), trie par début.
5. Le script complet est injecté avec les segments (`lib.rs:193-196`), puis `timeupdate` déclenche saut ou proposition (`videoguard.js:64-82`, `121`).

---

## 3. Tableau comparatif fonctionnalité par fonctionnalité

Légende faisabilité dans NIGHTFALL (CEF Alloy, sans extension) : **Oui** = réalisable par script injecté et/ou cœur Rust ; **Réserve** = réalisable avec une limite ou une dépendance à la page YouTube ; **Non** = impossible ou déconseillé. Effort : S (moins d'une demi-journée d'agent), M (1 à 3 jours), L (plus). Les efforts sont des estimations, non mesurées.

### 3.1 Catégories et types d'action

| # | Fonctionnalité | SponsorBlock (réf.) | VideoGuard aujourd'hui | État | Faisabilité / effort |
|---|---|---|---|---|---|
| C1 | `sponsor`, `selfpromo`, `interaction`, `intro`, `outro`, `preview`, `filler`, `music_offtopic` | liste serveur `SBS/src/config.ts:22` ; client `SB/config.json.example` (`categoryList`) | les 8 : `lib.rs:21-30` | présent | Oui, S |
| C2 | `exclusive_access` (« Accès exclusif »), catégorie qui n'existe qu'en action `full` (étiquette de vidéo entière) | `SBS/src/config.ts:24-27` (`categorySupport.exclusive_access = ["full"]`), `SB/src/utils/videoLabels.ts:30` (`/api/videoLabels/<préfixe>`) | absent | absent | Oui (étiquette affichée dans le panneau et la barre), M |
| C3 | `hook` (accroche / salutations) | `SB/config.json.example`, `SB/src/config.ts` (couleur `hook`) | absent | absent | Oui, S |
| C4 | `poi_highlight` (point fort, action `poi` : aller au point fort) | `SB/config.json.example` ; bouton `SB/src/js-components/skipButtonControlBar.ts:15` ; raccourci `SB/src/config.ts:94` (`skipToHighlightKeybind`) | absent | absent | Oui, M |
| C5 | `chapter` (chapitres nommés par la communauté) | `SB/config.json.example` ; rendu `SB/src/js-components/previewBar.ts:464,1063` (`renderSegmentsAsChapters`) | absent | absent | Réserve : le rendu de chapitres dans la barre de YouTube dépend du DOM du lecteur ; alternative : liste dans le panneau natif. L |
| C6 | Types d'action **du segment** `skip`, `mute`, `full`, `poi`, `chapter`. Ce sont des propriétés de chaque segment, fixées par le contributeur, et non des choix de l'utilisateur. Compatibilité par catégorie (serveur) : `sponsor` et `selfpromo` = `skip`, `mute`, `full` ; `exclusive_access` = `full` ; `interaction`, `intro`, `outro`, `preview`, `hook`, `filler` = `skip`, `mute` ; `music_offtopic` = `skip` ; `poi_highlight` = `poi` ; `chapter` = `chapter` | `SB/src/types.ts:52-58` ; `SBS/src/config.ts:24-37` (`categorySupport`) | seulement `skip` demandé (`lib.rs:176`) ; les autres sont ignorés à l'analyse (`lib.rs:152-154`, test `parse_filters_by_video_and_config`) | partiel | Oui, M |
| C7 | Interrupteurs globaux de l'utilisateur pour les types d'action : `muteSegments` (appliquer les segments `mute`, défaut activé) et `fullVideoSegments` (afficher les étiquettes de vidéo entière, défaut activé), plus `manualSkipOnFullVideo` | `SB/src/config.ts:22-25` (déclaration), `391-394` (défauts) ; un segment `mute` est traité comme « Désactivé » si `muteSegments` est faux (`SB/src/utils/skipRule.ts:39-41`) ; étiquette conditionnée par `fullVideoSegments` (`SB/src/content.ts:1478-1479`) | absent | absent | Oui, S |

### 3.2 Options de configuration

| # | Option | SponsorBlock | VideoGuard | État | Faisabilité / effort |
|---|---|---|---|---|---|
| O1 | Mode par catégorie choisi par l'utilisateur : Désactivé / Afficher seulement / Passage manuel / Saut auto (plus « valeur par défaut » dans un profil). Il n'y a **pas** de mode « sourdine » : la sourdine vient du type d'action du segment (C6) et de l'interrupteur global `muteSegments` (C7) | `SB/src/types.ts:32-38` (`CategorySkipOption`), `SB/src/config.ts:106` | `Skip`, `Show`, `Ignore` (`lib.rs:32-41`), liste déroulante `security.js:89-92` | partiel (pas de « afficher seulement ») | Oui, S |
| O2 | Valeurs par défaut | `SB/src/config.ts:470-480` : `sponsor` auto, `poi_highlight` manuel, `exclusive_access` et `chapter` affichage seul | `lib.rs:63-67` : `sponsor`, `interaction`, `selfpromo` auto ; `intro`, `outro`, `preview` proposé ; reste ignoré | présent, différent | À préserver tel quel pour les 8 existantes (règle 7) |
| O3 | Durée minimale d'un segment | `SB/src/config.ts:43` (`minDuration`, défaut 0 en `412`), `SB/src/utils/skipProfiles.ts:85-98` (`hideTooShortSegments`) | `min_duration` (1 s, `lib.rs:47-48,70`), **absent de l'interface** (`security.js:85-98`) | partiel | Oui, S |
| O4 | Durée d'affichage de l'avis (4 s) | `SB/src/config.ts:44` | fixe 6 s (`videoguard.js:51`) | absent (non réglable) | Oui, S |
| O5 | Mode d'avis : complet, mini, estompé | `SB/src/types.ts:224-230`, `SB/src/config.ts:32` | un seul style | absent | Oui, S |
| O6 | Avis « à venir » avant un segment | `SB/src/config.ts:31` (`showUpcomingNotice`), `SB/src/render/UpcomingNotice.tsx` | absent | absent | Oui, M |
| O7 | Signal sonore au saut | `SB/src/config.ts:45`, `SB/src/content.ts:1807-1818` | absent | absent | Oui, S (son synthétisé, pas d'actif tiers) |
| O8 | Désactiver temporairement tout saut | `SB/src/config.ts:21` (`disableSkipping`), `SB/src/content.ts:1761` | interrupteur général seulement (`lib.rs:44`) | partiel | Oui, S |
| O9 | Options musique : saut automatique des parties non musicales ; seulement sur YouTube Music | `SB/src/config.ts:56-57`, `SB/src/content.ts:1959-1972` | `music_offtopic` ignoré par défaut ; pas d'option dédiée | absent | Oui, S |
| O10 | Profils par chaîne, règles avancées | `SB/src/config.ts:169-173`, `SB/src/utils/skipProfiles.ts`, `SB/src/utils/skipRule.type.ts:1-40` | absent | absent | Réserve : exige l'identifiant de chaîne, lu dans la page YouTube (fragile). Version simplifiée « exceptions par chaîne ou par site » : M. Règles avancées : L, non recommandé |
| O11 | Raccourcis clavier (passer, point fort, vote, chapitre suivant…) | `SB/src/config.ts:93-103`, `SB/src/content.ts:2676-2752` | absent | absent | Oui via le système de raccourcis de NIGHTFALL (`Settings.shortcuts`), M ; ne doit pas fonctionner verrouillé (demande du propriétaire, §7) |
| O12 | Couleurs des catégories | `SB/src/config.ts:117+` | sans objet (pas de barre) | absent | Oui, rattaché à l'éditeur de thèmes (M-0005), S |
| O13 | Adresse de serveur personnalisée | `SB/src/config.ts:42` | constante `API_BASE` (`lib.rs:16`) | absent | Oui, en réglage avancé, HTTPS uniquement, S |
| O14 | Accélération des publicités vidéo YouTube | n'existe pas dans SponsorBlock | `skip_video_ads` (`lib.rs:51-52`), `videoguard.js:94-113` | présent (F-156) | à préserver ; voir D-01 |
| O15 | Avertissement quand les segments n'ont pas pu être chargés | `SB/src/config.ts:74` (`showSegmentFailedToFetchWarning`) | journal seulement (`client.rs:238`) | absent | Oui, S |

### 3.3 Comportements de saut

| # | Comportement | SponsorBlock | VideoGuard | État | Faisabilité / effort |
|---|---|---|---|---|---|
| S1 | Saut automatique | planifié à l'avance, tampon 3 ms (`SB/src/content.ts:70`, `652-700`, `1760-1853`) | sur `timeupdate` (`videoguard.js:121`), tolérance de fin 0,3 s (`videoguard.js:69`) | présent, précision moindre (D-11) | Oui, M (planification précise) |
| S2 | Saut manuel (bouton) | `SB/src/content.ts:1759-1853` | proposition par avis de 6 s (`videoguard.js:77-82`) | partiel (D-02) | Oui, S |
| S3 | Retour arrière (« annuler le saut ») | `SB/src/content.ts:1888-1914` (`unskipSponsorTime`, `reskipSponsorTime`) | bouton « Revenir » (`videoguard.js:70-76`) | présent (vérifié, cas A et B) | à préserver |
| S4 | Sourdine pendant les segments **de type `mute`** (le mode de la catégorie décide si elle est automatique ou proposée ; l'interrupteur global `muteSegments` peut la désactiver, C7) | `SB/src/content.ts:876-886`, `1797-1802` ; `SB/src/utils/skipRule.ts:39-41` | absent (les segments `mute` ne sont ni demandés ni appliqués : `lib.rs:152-154,176`, banc cas K) | absent | Oui, M |
| S5 | Segments qui se chevauchent ou s'imbriquent | regroupement `SB/src/content.ts:1612-1660` | chevauchement : enchaînement un par un (vérifié, cas F : 11 → 20 → 30) ; deux avis successifs se remplacent. **Imbrication : défaut D-20** (A 10-40, B 15-20 : entrée à 16 → saut à 40 puis retour à 20, banc cas M2) | partiel, défaut | Oui, S |
| S6 | Fin de vidéo, boucle, listes de lecture, macOS | `SB/src/content.ts:1773-1785` (cas de bouclage signalés par des commentaires) | saute à `end` sans traitement particulier | NON VÉRIFIÉ dans CEF | Réserve : à tester dans CEF Windows et macOS, M |
| S7 | Pause d'une vidéo ou publicité en cours | `SB/src/content.ts:655-662` | ignoré tant qu'une publicité est affichée (`videoguard.js:65`) | présent | à préserver |
| S8 | Recul manuel dans la vidéo puis re-saut | `SB/src/content.ts` (planification recalculée) | `seeking` réarme les segments dont le début est **strictement** après la nouvelle position (`t < s.start`, `videoguard.js:122-126`) ; vérifié (cas D) | présent pour `skip` qui ne commencent pas à 0 ; absent pour `show` (D-02, cas E) ; absent pour un segment qui commence à 0 (D-21, cas N) ; annulé par une nouvelle injection pour la même vidéo (D-22, cas O) | Oui, S |
| S9 | Segment ignoré si la vidéo a changé | `SB/src/content.ts:887-912` | garde `currentVideoId() !== state.videoId` (`videoguard.js:65`) ; vérifié (cas H) | présent | à préserver |
| S10 | Étiquette « temps gagné » dans la barre de temps | `SB/src/config.ts:20`, `SB/src/content.ts:1468-1473` | absent | absent | Réserve (DOM YouTube), S |

### 3.4 Récupération, soumission, vote, synchronisation

| # | Fonctionnalité | SponsorBlock | VideoGuard | État | Faisabilité / effort |
|---|---|---|---|---|---|
| R1 | Récupération par préfixe de hachage | `SB/src/utils/segmentData.ts:62-71` (préfixe de **5** caractères pour les segments), contrat `SBS/src/routes/getSkipSegmentsByHash.ts:9-39`, longueurs 3 à 32 (`SBS/src/utils/hashPrefixTester.ts:3-6`) | préfixe de **4** (`lib.rs:124-126`, test `prefix_is_four_hex`) | présent, plus discret | à préserver |
| R2 | Cache des segments par vidéo | `SB/src/utils/segmentData.ts:11,27-29,89` | aucun (D-05) | absent | Oui, S |
| R3 | Requêtes conditionnelles (ETag) | le serveur émet `ETag` (`SBS/src/routes/getSkipSegmentsByHash.ts:26-29`) et autorise `If-None-Match` (`SBS/src/middleware/cors.ts:6`) | aucune | absent | Oui, S |
| R4 | Catégories verrouillées par les modérateurs | `SB/src/content.ts:1335-1356` (`/api/lockCategories/<préfixe 4>`) | absent | absent | Oui, mais utile seulement à la soumission, S |
| R5 | Segments masqués localement après un vote négatif | `SB/src/config.ts:28`, `SB/src/content.ts:1236-1250` | absent | absent | Oui, lié au vote, S |
| R6 | Vote (pour, contre, changer la catégorie) | `SB/src/content.ts:2317-2403` ; serveur `SBS/src/routes/voteOnSponsorTime.ts:298-306,436` (`type` 0, 1, 20 ; `category`) ; limite `SBS/src/config.ts:60-65` (15 votes par 15 min) | absent | absent | Réserve : exige l'identifiant utilisateur (§8), décision du propriétaire. L |
| R7 | Soumission de segments | `SB/src/content.ts:2462-2575` ; serveur `SBS/src/routes/postSkipSegments.ts:191-256,477-505` | absent | absent | Réserve : exige l'identifiant utilisateur, des règles serveur non maîtrisées (§8). L |
| R8 | Compteur de vues d'un segment | `SB/src/content.ts:1727-1757` → `POST /api/viewedVideoSponsorTime?UUID=&videoID=` (`SBS/src/routes/viewedVideoSponsorTime.ts:4-18`) | absent (volontaire ou non) | absent | **Non recommandé** : envoie l'identifiant de vidéo en clair, ce qui annule l'intérêt du préfixe de hachage |
| R9 | Statistiques locales (temps gagné, nombre de sauts) | `SB/src/config.ts:17,26` ; `SB/src/popup/YourWorkComponent.tsx` | absent | absent | Oui (stockage chiffré local, rien n'est envoyé), S |
| R10 | Synchronisation des réglages | `chrome.storage.sync` du navigateur (config sync dans `SB/src/config.ts`), exportation `SB/src/utils/exporter.ts` | aucune ; `nf-sync` ne gère que les favoris (`nf-sync/src/lib.rs:1-7,16-18`) | absent | Réserve : export/import JSON local, S ; intégration à `nf-sync` (format versionné `NFSYNC01`) à concevoir par l'Architecte, M |
| R11 | Brouillons de segments non envoyés | `SB/src/config.ts:167` | sans objet | absent | lié à R7 |

### 3.5 Interface

| # | Élément | SponsorBlock | VideoGuard | État | Faisabilité / effort |
|---|---|---|---|---|---|
| U1 | Barre de segments colorée sur la barre de progression | `SB/src/js-components/previewBar.ts` (1195 lignes) | absente | absent | Réserve : s'accroche au DOM du lecteur YouTube (sélecteurs fragiles). Solution robuste : frise dans un panneau natif NIGHTFALL ; frise sur le lecteur en « meilleur effort ». L |
| U2 | Avis de saut (annuler, voter, ne plus afficher) | `SB/src/components/SkipNoticeComponent.tsx` | avis minimal (`videoguard.js:31-52`) | partiel | Oui, M |
| U3 | Pastille de catégorie sous le titre (« étiquette de vidéo entière ») | `SB/src/render/CategoryPill.tsx:14` | absent | absent | Réserve (DOM YouTube) ; remplacer par un indicateur natif, S |
| U4 | Fenêtre contextuelle de l'extension (liste des segments, soumission, statistiques) | `SB/src/popup/` | absente | absent | Oui : panneau natif via les menus en surimpression existants (`popup_open`), M |
| U5 | Page d'options | `SB/src/options.ts`, `SB/public/options/` | cartes dans le Centre de sécurité (`security.js:85-98`) | partiel | Section « VideoGuard » dans Paramètres (M-0006), M |
| U6 | Boutons dans la barre du lecteur (info, supprimer, envoyer) | `SB/src/content.ts:1916-2070` | absents | absent | Réserve (DOM YouTube), non prioritaire |

### 3.6 Plateformes

| # | Plateforme | SponsorBlock | VideoGuard | État | Faisabilité / effort |
|---|---|---|---|---|---|
| P1 | YouTube (watch, shorts, embed, live, youtu.be, m., music.) | oui (`maze-utils`, **non lu**) | `video_id` (`lib.rs:102-121`), tests `video_ids` | présent (pages principales) | à préserver |
| P2 | YouTube intégré dans un site tiers | oui (extension active dans les cadres) | **pas de segments** : le déclenchement utilise l'URL de l'onglet (`client.rs:228`) ; seule l'accélération des publicités s'exécute dans le cadre (`client.rs:426-430`) | partiel (D-07) | Oui : requête par cadre à partir de l'URL du cadre, M |
| P3 | YouTube Music | `SB/src/content.ts:1959-1972` | l'hôte `music.youtube.com` est reconnu (`lib.rs:106`) | présent sans réglage dédié | S |
| P4 | Invidious et instances tierces | `SB/src/config.ts:41`, `SB/src/background.ts:29,135,154`, liste `SB/ci/invidiouslist.json` | absent | absent | Oui avec liste d'instances gérée par l'utilisateur, M ; priorité basse |
| P5 | Services prévus côté serveur : Spotify, PeerTube | `SBS/src/types/segments.model.ts:21-26` ; la soumission accepte YouTube et Spotify (`SBS/src/routes/postSkipSegments.ts:200-209`) | absent | absent | Non vérifié côté client (non trouvé dans `SB/src` hors serveur) ; hors périmètre |
| P6 | Twitch, Vimeo, Netflix, Dailymotion… | aucune base de segments (le serveur n'a que ces services) | non | impossible | **Non** : il n'existe pas de données. Ne jamais annoncer ces plateformes comme prises en charge (règle du cahier des charges) |

---

## 4. Vérification des comportements actuels sur des cas concrets

### 4.1 Tests Rust existants (exécutés par la CI, non relancés localement)

CI : run 41825393127 sur `develop` (`2408f95`), job 126558718472 : cible `nf_videoguard`, `test result: ok. 7 passed; 0 failed`.

| Test (`lib.rs`) | Ce qu'il établit | Ce qu'il ne couvre pas |
|---|---|---|
| `video_ids` (223) | extraction : watch + `t=`, `youtu.be`, `m.` + shorts ; refus de `/`, d'un faux domaine, d'un identifiant de 5 caractères | `/embed/`, `/live/`, `music.`, `youtube-nocookie`, majuscules d'hôte, identifiants avec caractères exotiques |
| `prefix_is_four_hex` (233) | 4 caractères hexadécimaux | valeur réelle (aucun vecteur de test SHA-256 fixé : un changement silencieux de l'algorithme passerait) |
| `parse_filters_by_video_and_config` (240) | tri par vidéo, `mute` ignoré, segment trop court ignoré, catégorie ignorée, ordre | segments chevauchants, `end` ≤ `start`, réponse mal formée, `segment` à un seul élément (échec global de l'analyse, `lib.rs:140`) |
| `default_config_categories` (256) | `sponsor` actif, `filler` inactif | |
| `script_embeds_segments` (264) | l'identifiant et le point d'entrée sont dans le script | exécution du script |
| `page_script_only_on_youtube` (271) | domaines, `youtube-nocookie`, `notyoutube.com` refusé, désactivation | |
| `old_saved_config_gets_video_ad_skipping` (281) | ancien réglage sans `skip_video_ads` → activé | le comportement des catégories après chargement. Le `"actions":{}` de ce test est un simple appareil de test : `actions` n'a pas de `#[serde(default)]` (`lib.rs:46`) et a toujours été rempli par `Default` depuis la création du module (`git log -S actions` : `79cddab`, `b27530e`) ; aucun réglage réel enregistré n'a donc une table vide |

**Non couverts** : `fetch_segments` (aucun test réseau ni simulé : 404, 400, 429, délai, JSON invalide), `injection_script` avec données malicieuses, tout comportement de `videoguard.js` avant cette mission, tout le chemin `client.rs`, `api.rs`. Aucun auto-test CI de bout en bout.

### 4.2 Banc local sur le vrai `videoguard.js` (exécuté ici)

Fichiers : `preuves/videoguard_js_harness.js` (écrit pour la mission) et `preuves/videoguard_js_harness.sortie.txt` (régénérée à la tentative 2 ; deux exécutions successives donnent une sortie identique). Principe : Chromium sans tête (Playwright, Chromium 153), l'URL `https://www.youtube.com/watch?v=dQw4w9WgXcQ` est servie par interception réseau avec un lecteur simulé (`#movie_player` + `<video>` dont `currentTime`, `duration`, `playbackRate` sont remplacés), les événements `timeupdate` et `seeking` sont émis à la main. **Limites** : ce n'est ni le lecteur YouTube ni CEF ; la cadence réelle de `timeupdate` n'est pas simulée ; `seeking` est émis de façon synchrone à l'affectation de `currentTime` (asynchrone dans un vrai navigateur ; sans effet sur les cas ci-dessous, où le gestionnaire de `seeking` ne modifie que des segments situés après la nouvelle position) ; le lecteur simulé ne reproduit aucun comportement propre à YouTube. « VÉRIFIÉ » signifie donc ici « vérifié sur le vrai `videoguard.js` avec un lecteur simulé ».

| Cas | Résultat observé | Conclusion |
|---|---|---|
| A. segment `skip` 10-20 s, lecture à 12 s | `currentTime` passe à 20 ; avis « Segment ignoré : Sponsor » + bouton « Revenir » | VÉRIFIÉ |
| B. clic sur « Revenir » puis lecture à 12,3 s | retour à 12 ; pas de re-saut | VÉRIFIÉ |
| D. retour manuel à 5 s puis lecture à 11 s | re-saut à 20 | VÉRIFIÉ |
| C. segment `show` 0-30 s | avis « Intro détecté » + « Passer », position inchangée ; **avis disparu à 6,3 s** | VÉRIFIÉ : le bouton « Passer » n'existe plus 6 s après l'entrée dans un segment de 30 s (D-02) |
| E. (refait à la tentative 2) segment `show` **10-40 s** : entrée à 12 s (avis), avis retiré, recul à 5 s (avant le début), lecture à 6 puis 12 s | premier avis présent ; **aucun second avis** | VÉRIFIÉ : une seule proposition par segment et par vidéo (D-02). Le cas E de la tentative 1 utilisait un segment 0-30 s, qu'aucun recul ne peut réarmer (D-21) : il ne distinguait pas les deux causes et n'était pas concluant. Le témoin est le cas D (même recul, segment `skip` réarmé) |
| F. segments chevauchants 10-20 et 19-30 | 11 → 20 → 30 (deux événements) | VÉRIFIÉ |
| G. lecture à 19,8 s (fin - 0,2 s) | pas de saut | VÉRIFIÉ : tolérance de 0,3 s (`videoguard.js:69`) |
| H. navigation SPA vers une autre vidéo (`pushState`) avec les segments de l'ancienne | aucun saut | VÉRIFIÉ : la garde protège |
| I. vitesse 1,5 → publicité (`ad-showing`) → fin de publicité | pendant : vitesse 16, position 599,9 (fin) ; **après : vitesse 1** | VÉRIFIÉ sur le lecteur simulé : le script écrit 1 (`videoguard.js:111`) au lieu de la vitesse d'avant (D-01). NON VÉRIFIÉ : si le vrai lecteur YouTube réapplique ensuite la vitesse choisie par l'utilisateur, l'effet visible peut être nul ou bref |
| K. segment d'action `mute` fourni directement au script | aucune action | VÉRIFIÉ : le script n'implémente que `skip` et `show` |
| L. segment 30-31,2 s, ticks à 29,9 puis 30,15 | saut à 31,2 | VÉRIFIÉ (avec ticks simulés rapprochés) |
| M1. segments imbriqués A `skip` 10-40, B `skip` 15-20, entrée à 12 s | saut à 40 | VÉRIFIÉ : correct quand l'entrée précède le début de B (la boucle garde `t` = 12, B ne correspond pas) |
| M2. mêmes segments, entrée à 16 s (lecture reprise, lien `&t=16`, déplacement manuel), puis lecture à 20,5 s | **position finale 20** (saut à 40 puis, dans la même boucle avec `t` = 16 périmé, saut de B vers 20) ; à 20,5 s A n'est plus sauté (déjà marqué) | VÉRIFIÉ : défaut D-20 ; l'utilisateur voit les 20 s restantes du segment A |
| N. segment `skip` 0-10 s : lecture à 1 s, puis retour à 0 et lecture à 1 s ; témoin : segment 5-10 s, même scénario | 0-10 : premier saut à 10, puis **position 1, pas de re-saut** ; témoin 5-10 : re-saut à 10 | VÉRIFIÉ : défaut D-21 (`t < s.start` est impossible pour `start` = 0, `videoguard.js:125`) ; touche aussi la relecture depuis le début |
| O. segment `skip` 10-20 : saut, « Revenir » (12), lecture à 12,3 (pas de re-saut), **nouvel appel de `__nightfallVideoGuard` avec les mêmes données**, lecture à 12,6 | **saut à 20** après la nouvelle injection | VÉRIFIÉ : défaut D-22 (`state.skipped = {}`, `videoguard.js:145`) |
| P. noms ajoutés à la page par VideoGuard | `window.__nightfallVideoGuard`, `window.__nightfallVideoGuardInit`, éléments `#nf-vg-style`, `#nf-vg-toast`, propriété `video.__nfVg` | VÉRIFIÉ (D-09) ; les autres scripts de NIGHTFALL ne sont pas chargés dans ce banc (voir VG-30) |

### 4.3 Gestion des erreurs réseau (lecture, non exécutée)

- 404 → aucun segment (`lib.rs:186`), correct : le serveur répond 404 quand aucune vidéo du préfixe n'a de segment (`SBS/src/routes/getSkipSegmentsByHash.ts:34`).
- Autres statuts (400, 429, 5xx), délai dépassé, JSON invalide → `VideoGuardError` consigné dans le journal (`client.rs:238`). Aucun nouvel essai, aucune information pour l'utilisateur, aucune distinction entre « pas de segment » et « service injoignable ».
- Une réponse dont **un seul** élément (même d'une autre vidéo du même préfixe) a un `segment` mal formé fait échouer toute l'analyse (`lib.rs:148`, `Vec<ApiVideo>` désérialisé d'un bloc). Risque faible car le serveur filtre déjà par `actionTypes`.

### 4.4 Vie privée

Établi par lecture :
- Envoyé : le préfixe de 4 hexadécimaux (65 536 groupes), la liste des catégories actives, `actionTypes`, un `User-Agent` constant `NIGHTFALL-VideoGuard/0.1` (`lib.rs:177`) ; aucun cookie (ureq n'en gère pas par défaut), aucun identifiant utilisateur. Le serveur voit forcément l'adresse IP et le moment.
- Le serveur ne reçoit pas l'identifiant complet : il renvoie toutes les vidéos du groupe et le filtrage est local (`lib.rs:150`, côté serveur `SBS/src/routes/getSkipSegmentsByHash.ts:30-33`).
- Comparaison : l'extension envoie 5 caractères pour les segments (`SB/src/utils/segmentData.ts:62`), 4 pour les catégories verrouillées (`SB/src/content.ts:1336`), un en-tête `X-CLIENT-NAME` (`segmentData.ts:69`) et, par défaut, le compteur de vues `viewedVideoSponsorTime` avec l'identifiant en clair (`SB/src/content.ts:1750`, `trackViewCount` activé par défaut, `config.ts` : `trackViewCount: true`). VideoGuard est donc plus discret.
- Écarts : requêtes pendant le verrouillage (D-19), navigation privée (D-04), absence de cache (D-05), contournement probable d'un proxy (D-08), détectabilité par la page (D-09).

### 4.5 Verrouillage (lecture, non exécuté)

Chemin lu, de bout en bout :
1. `lock_now` (`api.rs:816-829`) annule les autorisations temporaires, enregistre la session et les réglages, appelle `windows::on_locked()`, puis efface les clés (`c.unlocked = None`). Il ne touche pas `c.videoguard`, qui reste en mémoire avec `enabled = true`.
2. `on_locked` (`windows.rs:622-630`) ferme les menus, sort du plein écran et appelle `apply_mode_all(true)` ; le commentaire de la fonction le dit : « pages keep running but are invisible ». `apply_mode` (`windows.rs:518-560`) ne fait que masquer la vue de contenu (`set_visible(0)`). Aucune vidéo n'est mise en pause, aucun script n'est arrêté (recherche de `pause` et `set_audio_muted` dans `app/src/nf/*.rs` : seulement les téléchargements et la sourdine manuelle d'onglet).
3. Un changement d'adresse du cadre principal dans un onglet masqué (vidéo suivante en lecture automatique, liste de lecture, navigation de la page elle-même) appelle `on_address_change` → `on_navigation` (`client.rs:113-129`, `204`), qui ne teste pas `is_unlocked()` (`state.rs:352-354`) et lance `fetch_segments` (`client.rs:227-241`) puis injecte le résultat (`windows.rs:1505-1509`).
4. `on_load_start` injecte aussi le script de page d'options dans tout nouveau document YouTube (`client.rs:426-430`), sans test du verrouillage.
5. Seul effet du verrouillage sur ce chemin : `record_visit` ne s'exécute plus, car `core().storage()` vaut `None` (`client.rs:222-226`).

Conclusion : pendant le verrouillage, VideoGuard envoie des préfixes de hachage pour les vidéos qui s'enchaînent, continue de sauter des segments et d'accélérer les publicités dans des onglets invisibles (D-19). Effet réel dans CEF NON VÉRIFIÉ (pas de CEF ici) ; le Directeur indique l'avoir vérifié par lecture (`audit-1.md`, point MAJEUR 1).

---

## 5. Défauts et écarts constatés

Gravité : élevée, moyenne, faible. « Preuve » renvoie à §4.

| ID | Constat | Preuve | Statut | Gravité |
|---|---|---|---|---|
| D-01 | Après une publicité, le script remet la vitesse de lecture à **1** au lieu de la valeur d'avant ; seule la sourdine est mémorisée et restaurée (`videoguard.js:99,110-111`). Effet visible sur le vrai lecteur : dépend de ce que fait YouTube après la publicité (non vérifié) | banc, cas I | VÉRIFIÉ sur lecteur simulé ; effet réel NON VÉRIFIÉ | moyenne (à confirmer) |
| D-02 | Mode « Proposer de sauter » : l'avis dure 6 s (`videoguard.js:51`), n'est jamais reproposé (`videoguard.js:77-79` ; le réarmement `122-126` ne touche que les clés `skip`) ; sur un segment long, plus aucun moyen de « Passer » | banc, cas C et E | VÉRIFIÉ | moyenne |
| D-03 | Si l'utilisateur met les 8 catégories sur « Ignorer » sans désactiver VideoGuard (`security.js:89-91` enregistre chaque choix), la requête part quand même avec `categories=[]` (`lib.rs:169-176`) : le serveur répond 400 « No valid categories provided. » (`SBS/src/utils/parseSkipSegments.ts:23-24`, renvoyé en `getSkipSegmentsByHash.ts:17-18`), l'erreur est consignée à chaque vidéo et le préfixe est divulgué pour rien. (Tentative 1 : le scénario « `actions` vide après chargement d'un ancien réglage » était faux, voir §4.1.) | lecture de `fetch_segments` et du serveur | LU (non exécuté contre le serveur) | faible à moyenne |
| D-04 | Navigation privée : la requête est faite comme en navigation normale ; seul l'historique est conditionné par `private` (`client.rs:222` contre `227-241`) | lecture | LU | à arbitrer (§11) |
| D-05 | Aucun cache ni dédoublonnage : chaque changement d'adresse vers une vidéo relance une requête (`client.rs:129,227-241`) ; revenir sur une vidéo, ou un changement de paramètre d'URL, refait la requête | lecture | LU | faible (réseau, vie privée) |
| D-06 | Course : une réponse tardive d'une vidéo A injectée après navigation vers B remplace l'état (`videoguard.js:142-145`) sans que B ait ses segments ; l'injection d'une réponse arrivée avant le remplacement du document lors d'une navigation complète peut être perdue | lecture | LU ; effet réel NON VÉRIFIÉ (CEF absent) | faible à moyenne |
| D-07 | Lecteurs YouTube intégrés dans d'autres sites : aucun segment (déclenchement sur l'URL de l'onglet, `client.rs:228`), mais l'accélération des publicités s'y exécute (`client.rs:426-430`) | lecture | LU | moyenne |
| D-08 | La requête est faite par `ureq` avec `default-features = false` et `tls` (`Cargo.toml:49`) sans configuration de proxy (`lib.rs:174`) ; Chromium, lui, suit le proxy du système (`api.rs:1041`). Donc la requête VideoGuard pourrait **ne pas passer par le proxy ou le VPN configuré** et exposer l'adresse IP réelle | lecture ; comportement de ureq 2 sans la fonction de proxy d'environnement non testé | NON VÉRIFIÉ à l'exécution | moyenne (vie privée) |
| D-09 | Le script s'exécute dans le monde principal de la page (`client.rs:429`) et crée `window.__nightfallVideoGuard`, `window.__nightfallVideoGuardInit` (`videoguard.js:6,136,142`), la propriété `video.__nfVg` (`videoguard.js:119-120`) et des éléments `#nf-vg-toast`, `#nf-vg-style` : YouTube peut détecter VideoGuard et peut appeler ces fonctions (effet limité à sa propre vidéo). VideoGuard n'est pas seul en cause : le script d'indicateur de son injecté dans **toutes** les pages crée `window.__nfMed` (`browsing.rs:269`), ce qui suffit déjà à reconnaître NIGHTFALL | banc, cas P ; lecture | VÉRIFIÉ (noms) | moyenne (discrétion) |
| D-10 | L'avis est ajouté à `document.documentElement` (`videoguard.js:49`) : en plein écran, seul le sous-arbre de l'élément en plein écran est affiché (principe de la couche supérieure) ; l'avis serait invisible | raisonnement, non testé | NON VÉRIFIÉ | moyenne |
| D-11 | Précision du saut : fondée sur `timeupdate` (`videoguard.js:121`), dont la cadence va de 15 à 250 ms selon la spécification HTML ; jusqu'à environ 250 ms du segment peuvent être entendues. L'extension planifie le saut (`SB/src/content.ts:652-700`) | lecture ; cadence non mesurée dans CEF | NON VÉRIFIÉ (mesure) | faible |
| D-12 | `videoguard.set` accepte toute configuration bien typée sans autre contrôle (`api.rs:497-506`) : des clés de catégories arbitraires sont envoyées au serveur, `min_duration` est quelconque ; `min_duration` n'a aucune commande dans l'interface (`security.js:85-98`). Effet côté serveur (corrigé à la tentative 2) : le serveur **retire en silence** les éléments qui contiennent un caractère hors de `a-z`, `_`, `-`, `|` (`SBS/src/utils/parseParams.ts:29-34`) et ne compare pas les autres à sa liste de catégories ; une clé inconnue mais bien formée ne produit simplement aucun résultat. Le 400 n'arrive que si **aucun** élément valide ne reste (`parseSkipSegments.ts:23-24`) ou si le JSON est invalide (impossible ici, il est produit par `serde_json`) | lecture | LU | faible |
| D-13 | Changer un réglage n'agit ni sur l'onglet courant ni sur les options déjà injectées (`api.rs:497-506` ne déclenche rien ; `videoguard.js:136-139`) : l'accélération des publicités ne change qu'au rechargement | lecture | LU | faible |
| D-14 | Aucun retour utilisateur en cas d'échec (journal seulement, `client.rs:238`) ; aucun indicateur d'état | lecture | LU | faible |
| D-15 | Réglages dispersés : tout est dans le Centre de sécurité (`security.js:85-98`), rien dans Paramètres (`settings.js:569` ne cite que la réinitialisation) | lecture | LU | à traiter par M-0006 |
| D-16 | Pas de test automatisé du script de page avant cette mission, pas de test des chemins réseau, aucune étape d'auto-test (§4.1) | recherche dans `selftest.rs` | VÉRIFIÉ | moyenne |
| D-17 | Documentation : `docs/LICENSES.md:17` écrit LGPL-3.0 ; `docs/agents/FONCTIONNALITES.md:221` cite `client.rs:fetch_segments` qui n'existe pas (la fonction est `nf_videoguard::fetch_segments`, appelée en `client.rs:228-241`) ; `docs/STATUS.md:32` indique que le saut réel n'a jamais été vérifié | lecture | VÉRIFIÉ | faible |
| D-18 | L'accélération des publicités (hors SponsorBlock) peut contrevenir aux conditions d'utilisation de YouTube et dépend de noms de classes internes (`ad-showing`, `ytp-ad-skip-button`, `videoguard.js:96,107`), donc fragile aux changements de YouTube. Fonction à préserver (règle 7), mais à présenter honnêtement dans l'interface | lecture | LU | information |
| D-19 | **VideoGuard reste actif pendant le verrouillage** : aucune vérification de `is_unlocked()` dans `on_navigation` ni dans `on_load_start` ; `lock_now` ne touche pas `c.videoguard` ; les onglets sont seulement masqués et les pages continuent de tourner. Une vidéo suivante dans un onglet masqué déclenche une requête SponsorBlock ; le script de page continue de sauter et d'accélérer les publicités. Détail au §4.5 (`api.rs:816-829`, `windows.rs:518-560,622-630`, `client.rs:204,227-241,426-430`) | lecture | LU ; effet dans CEF NON VÉRIFIÉ | **élevée** (vie privée, contraire à l'attente du propriétaire : rien ne doit agir quand NIGHTFALL est verrouillé) |
| D-20 | Segments imbriqués : avec A 10-40 et B 15-20, une entrée dans B (position 15 à 19,7, par reprise de lecture, lien horodaté ou déplacement manuel) saute à 40 puis revient à 20 dans la même boucle, car `t` n'est pas relu après le premier saut (`videoguard.js:66-84`, `73`) ; A étant marqué sauté, ses 20 dernières secondes sont lues | banc, cas M2 (témoin M1) | VÉRIFIÉ (lecteur simulé) | moyenne |
| D-21 | Un segment qui commence à 0 n'est jamais réarmé : le réarmement exige `t < s.start` (`videoguard.js:125`), impossible pour `start` = 0. Retour au début ou relecture de la vidéo : l'intro n'est plus sautée | banc, cas N (témoin segment 5-10) | VÉRIFIÉ (lecteur simulé) | faible à moyenne (les intros commencent souvent à 0) |
| D-22 | Chaque appel de `__nightfallVideoGuard` remet `state.skipped` à zéro (`videoguard.js:145`), même pour la vidéo déjà chargée. Or `client.rs` refait la requête et l'injection à **chaque** changement d'adresse du cadre principal vers une vidéo (`client.rs:129,227-241`, D-05). Conséquence : après « Revenir », une nouvelle injection pour la même vidéo fait de nouveau sauter le segment que l'utilisateur voulait voir, et les avis « Proposer » réapparaissent. Quels changements d'adresse YouTube produit pour une même vidéo (paramètres ajoutés à l'URL…) : NON VÉRIFIÉ | banc, cas O ; lecture | VÉRIFIÉ (script) ; déclencheurs réels NON VÉRIFIÉS | faible à moyenne |

---

## 6. Spécification du module cible

### 6.1 Principes

1. **Données** : l'API publique de SponsorBlock uniquement ; aucune dépendance à l'extension ; code et textes originaux de NIGHTFALL (MPL-2.0). Voir §9.
2. **Séparation** : la logique de décision (quels segments, quelle action, quand, cache, erreurs) vit dans `crates/nf-videoguard` en Rust pur, **testable sans navigateur** ; le script de page ne fait qu'exécuter des ordres simples (aller à, couper le son, afficher). C'est une piste pour l'Architecte, qui décide.
3. **Interface** : l'essentiel de l'interface (liste des segments, état, statistiques, réglages) est **native NIGHTFALL** (menus en surimpression, page Paramètres), donc indépendante du DOM de YouTube. Les éléments dans le lecteur (frise colorée, avis) sont un confort « meilleur effort » qui se dégrade proprement si YouTube change.
4. **Discrétion** : aucun nom global ni identifiant DOM stable détectable par la page ; aucune donnée personnelle envoyée ; réseau via les mêmes règles que le reste du navigateur.
5. **Compatibilité** : les 8 catégories existantes, leurs valeurs par défaut, l'accélération des publicités et les réglages enregistrés (`#[serde(default)]`, aucun champ renommé ni supprimé, `PROJET.md` « Pièges connus ») sont préservés.

### 6.2 Exigences

Chaque exigence est numérotée `VG-xx` ; le critère d'acceptation (AC) est vérifiable par un Auditeur.

**Catégories et actions**

Deux notions distinctes (correction de la tentative 1, qui les confondait) : le **type d'action d'un segment** (`skip`, `mute`, `full`, `poi`, `chapter`) est une donnée du serveur, choisie par le contributeur ; le **mode d'une catégorie** est un choix de l'utilisateur qui dit quoi faire des segments de cette catégorie, quel que soit leur type.

- **VG-01** Les 12 catégories du serveur (`SBS/src/config.ts:22`) sont gérées, avec libellé français court et description utile en une phrase (rédigée par NIGHTFALL, pas copiée). La table des types d'action acceptés par catégorie suit le contrat du serveur (`SBS/src/config.ts:24-37`) : `sponsor`, `selfpromo` = `skip`, `mute`, `full` ; `exclusive_access` = `full` ; `interaction`, `intro`, `outro`, `preview`, `hook`, `filler` = `skip`, `mute` ; `music_offtopic` = `skip` ; `poi_highlight` = `poi` ; `chapter` = `chapter`. Une catégorie ou un type d'action inconnus renvoyés par l'API sont ignorés sans erreur.
  AC : test Rust avec une réponse contenant `category:"nouveau"` et `actionType:"nouveau"` → ignorés, les autres segments de la réponse sont gardés ; test de table : la table de NIGHTFALL est égale à celle ci-dessus (12 entrées, `sponsor` et `selfpromo` incluent `full`).
- **VG-02** Mode par catégorie, choisi par l'utilisateur : **Sauter automatiquement**, **Proposer**, **Afficher seulement**, **Ignorer** (équivalents fonctionnels de AutoSkip, ManualSkip, ShowOverlay, Disabled, `SB/src/types.ts:32-38`). Il n'y a pas de mode « Couper le son » : ce que fait le mode dépend du type du segment :
  - `skip` : sauter (auto) ou bouton « Passer » (proposer) ;
  - `mute` : couper le son pendant le segment (auto) ou bouton « Couper le son » (proposer) ;
  - `poi` : bouton « Aller au point fort » (proposer ; « auto » = aller au point fort au début de la lecture) ;
  - `full` : étiquette « Vidéo entière : <catégorie> » (aucun saut ; « auto » peut être refusé pour ce type, à décider par l'Architecte) ;
  - `chapter` : affichage du chapitre (aucun saut) ;
  - « Afficher seulement » : le segment apparaît dans la frise et le panneau, sans avis ni action ; « Ignorer » : ni requête pour cette catégorie, ni affichage.
  Deux interrupteurs globaux, comme dans l'extension (`SB/src/config.ts:22-23`, défauts `391-392`) : **Appliquer les segments « sourdine »** (si désactivé, un segment `mute` est traité comme « Ignorer », cf. `SB/src/utils/skipRule.ts:39-41`) et **Afficher les étiquettes de vidéo entière**. Valeurs par défaut : décision 5 (§11). Les 3 modes existants gardent leur sens et leur sérialisation (`skip`, `show`, `ignore`, `lib.rs:32-41`) ; le nouveau mode s'ajoute à l'énumération.
  AC : un réglage enregistré avant la mission (JSON de `lib.rs:282`, et un JSON contenant les 8 catégories avec `skip`/`show`/`ignore`) se charge et se comporte comme avant (test de migration) ; un test de décision pour chaque couple (mode × type de segment) ; un test où l'interrupteur « sourdine » est désactivé : un segment `mute` d'une catégorie en « Sauter automatiquement » ne produit aucune action.
- **VG-03** Valeurs par défaut : celles de `lib.rs:63-67` pour les 8 catégories existantes ; `poi_highlight` = Proposer, `exclusive_access` = Afficher seulement, `chapter` et `hook` = Ignorer.
  AC : test `default_config_categories` conservé et étendu.
- **VG-04** `categories` demandées = celles dont le mode n'est pas « Ignorer » ; `actionTypes` demandés = l'union des types acceptés (VG-01) par ces catégories, moins `mute` si l'interrupteur « sourdine » est désactivé et moins `full` si l'interrupteur « vidéo entière » est désactivé. Les étiquettes `full` peuvent aussi venir de `/api/videoLabels/<préfixe>` (`SB/src/utils/videoLabels.ts:30`) : choix de l'Architecte, avec le même préfixe de 4 caractères.
  AC : tests sur l'URL construite (sans réseau) : configuration par défaut actuelle → `actionTypes=["skip","mute","full"]` si les interrupteurs sont activés, `["skip"]` s'ils sont désactivés ; seulement `poi_highlight` actif → `["poi"]`.
- **VG-05** Aucune requête n'est émise si aucune catégorie n'est active (toutes sur « Ignorer », corrige D-03), si VideoGuard est désactivé, ou si NIGHTFALL est verrouillé (VG-25).
  AC : test Rust sur la fonction de construction de requête (retourne « rien à demander »).

**Décision de saut (cœur Rust, script de page)**

- **VG-10** Saut précis : déclenché à moins de 100 ms après le début du segment à vitesse 1 et 2, mesuré dans le banc ; un moteur à minuterie planifiée plutôt qu'à `timeupdate` seul (corrige D-11).
  AC : banc JS avec horloge simulée ; mesure en CI Windows/macOS si l'auto-test peut lire une vidéo locale (voir VG-71).
- **VG-11** Segments qui se chevauchent **ou s'imbriquent** (de type `skip`, à sauter automatiquement) fusionnés en un seul intervalle ; un saut mène toujours à la fin la plus tardive de l'intervalle fusionné, jamais en arrière, un seul avis. La décision se fait sur des intervalles fusionnés et la position n'est jamais relue d'une valeur périmée (corrige D-20).
  AC : banc : cas F → un seul saut 11 → 30, un avis ; cas M2 (A 10-40, B 15-20, entrée à 16) → position 40 après le saut, jamais 20 ; cas M1 (entrée à 12) → 40 ; test Rust de fusion (chevauchement, imbrication, segments adjacents, segment de longueur nulle).
- **VG-12** Sourdine (segments de type `mute`) : le son est coupé pendant le segment et restauré à la sortie, y compris si l'utilisateur change de position ; l'état de sourdine de l'utilisateur est respecté (s'il avait coupé le son, on ne le rétablit pas).
  AC : banc, 3 cas (entrée, sortie, recul manuel).
- **VG-13** Fin de vidéo, vidéo en boucle, liste de lecture : pas de bouclage ni de blocage lors d'un saut vers la fin.
  AC : banc pour la logique ; **test réel dans CEF Windows et macOS** (NON VÉRIFIÉ à ce jour, voir §10).
- **VG-14** « Revenir » ramène à la position d'avant le saut et ne re-saute pas ce segment tant que l'utilisateur ne s'est pas déplacé lui-même **avant ou au début** du segment ; un tel déplacement manuel réarme tous les modes, y compris « Proposer » (corrige D-02), et y compris pour un segment qui commence à 0 (corrige D-21 : le réarmement ne peut pas reposer sur `position < début`). Le déplacement provoqué par « Revenir » lui-même ne réarme pas. Une nouvelle réception de segments pour la vidéo déjà chargée conserve l'état des sauts (marques « sauté », « Revenir ») des segments inchangés (corrige D-22).
  AC : banc : cas B (pas de re-saut après « Revenir ») ; cas D (re-saut) ; cas E (doit re-proposer) ; cas N (segment 0-10 : après retour à 0 et lecture à 1 s, position 10) ; « Revenir » sur un segment 0-10 entré à 0 → pas de re-saut ; cas O (après nouvelle injection identique, position 12,6, pas de saut).
- **VG-15** « Proposer » : l'avis reste visible **tant que la lecture est dans le segment** (pas de disparition à 6 s), avec un bouton « Passer » ; il se ferme à la sortie du segment ou sur « Ignorer ce segment ».
  AC : banc, segment de 30 s, avis présent à 20 s.
- **VG-16** Pendant une publicité vidéo, aucun saut de segment ; à la fin de la publicité, la **vitesse et la sourdine choisies par l'utilisateur** sont restaurées (corrige D-01).
  AC : banc, cas I : vitesse après publicité = 1,5 ; test manuel sur le vrai YouTube dans CEF (vitesse 1,5, publicité, vitesse affichée par le lecteur après la publicité), capture à l'appui : NON VÉRIFIABLE ici.
- **VG-17** Durée minimale réglable (0 à 30 s, défaut 1 s conservé), durée d'affichage de l'avis (2 à 15 s, défaut 4 s ; l'ancienne valeur fixe de 6 s n'est plus imposée), signal sonore (désactivé par défaut), avis « à venir » (désactivé par défaut).
  AC : tests de plage ; bornes refusées par `videoguard.set` (VG-60).

**Réseau et vie privée**

- **VG-20** Préfixe de hachage de 4 caractères, jamais plus long (le serveur accepte 3 à 32, `hashPrefixTester.ts:3-6`) ; aucun identifiant utilisateur, cookie ni autre en-tête personnel ; `User-Agent` constant `NIGHTFALL-VideoGuard/<version>`.
  AC : test Rust sur l'URL et les en-têtes construits ; une capture réseau (journal d'un auto-test avec serveur simulé) ne montre rien d'autre.
- **VG-21** Cache mémoire par vidéo (clé = identifiant, durée de vie indicative 1 h) avec dédoublonnage des requêtes simultanées ; requêtes conditionnelles `If-None-Match` ; pas de persistance disque des segments.
  AC : test Rust avec un client simulé : 2 navigations vers la même vidéo = 1 requête.
- **VG-22** Erreurs : nouvel essai avec délai croissant (au plus 2 essais) sur délai dépassé, 429 et 5xx ; pas de nouvel essai sur 400 ; état d'erreur exposé à l'interface (« Segments indisponibles ») sans bruit (pas d'avis modal).
  AC : tests Rust pour 404, 400, 429, 500, délai, JSON invalide (simulés).
- **VG-23** La requête suit les mêmes réglages réseau que Chromium (proxy, VPN, DNS) ; si cela est impossible en Rust pur, la requête est faite par le réseau de CEF (par exemple depuis le processus du navigateur via `CefURLRequest`) ou VideoGuard signale « segments indisponibles » plutôt que de contourner le proxy. À trancher par l'Architecte (corrige D-08).
  AC : test manuel avec proxy local qui journalise ; NON VÉRIFIABLE sans CEF aujourd'hui.
- **VG-24** Navigation privée : réglage « En navigation privée » avec valeur par défaut décidée par le propriétaire (§11, décision 2). Si désactivé : aucune requête.
  AC : auto-test avec fenêtre privée simulée.
- **VG-25** Verrouillage (corrige D-19 ; **ce n'est pas le comportement actuel**, contrairement à ce qu'affirmait la tentative 1) : dès `lock_now`, VideoGuard est suspendu : aucune requête (test de `is_unlocked()` avant l'envoi **et** avant l'injection du résultat, pour une requête partie juste avant le verrouillage), aucune injection de script dans un nouveau document, et les scripts déjà présents cessent d'agir (plus de saut, d'avis ni d'accélération de publicité) jusqu'au déverrouillage. Au déverrouillage, VideoGuard reprend avec le réglage chiffré rechargé (`api.rs:756-758`) et redemande les segments de l'onglet actif. Le moyen (ordre de suspension au script, mise en pause des médias, autre) est choisi par l'Architecte ; la question plus large de la lecture des médias pendant le verrouillage est la décision 6 (§11).
  AC : auto-test (`selftest.rs`, étape « verrouillage », `selftest.rs:287-289`) étendu avec un serveur API simulé (VG-71) qui compte les requêtes : onglet sur une page vidéo simulée en lecture, `lock_now`, changement d'adresse de l'onglet vers une **autre** vidéo → **0 requête** reçue par le serveur simulé pendant le verrouillage et aucun saut observé ; après déverrouillage → 1 requête. Ligne de journal attendue de chaque étape, `SELFTEST OK` conservé.
- **VG-26** Réglage avancé « Serveur » (adresse HTTPS d'un miroir, défaut = serveur officiel) ; refus de `http://`.
  AC : test de validation.
- **VG-27** `viewedVideoSponsorTime` n'est **jamais** appelé (R8).
  AC : recherche de la chaîne dans le dépôt par l'Auditeur ; test de non-régression.

**Injection et discrétion**

- **VG-30** VideoGuard n'ajoute aucune propriété globale, propriété sur les éléments de la page (`__nfVg`) ni identifiant d'élément à nom fixe ; communication cœur → page par un jeton aléatoire par session ; le script s'exécute dans une fermeture. **Portée honnête** : cette exigence rend VideoGuard plus discret, elle ne rend pas NIGHTFALL indétectable : d'autres scripts injectés dans toutes les pages restent visibles, au moins `window.__nfMed` (`browsing.rs:269`), et, tant que tout passe par le monde principal de la page, un script de la page peut observer ce qui est injecté (§6.4). La discrétion de l'ensemble des scripts de NIGHTFALL relève d'une mission distincte (hors périmètre, §12).
  AC : banc : liste des noms de `window`, des identifiants du DOM et des propriétés propres de l'élément `<video>` relevée avec et sans VideoGuard (cas P) → **aucune différence** ; l'énumération ne cherche pas des chaînes précises (`nightfall`, `videoguard`), un nom neutre fixe serait aussi détectable.
- **VG-31** Les lecteurs YouTube intégrés dans des sites tiers reçoivent leurs segments (requête par cadre, identifiant lu depuis l'URL du cadre) (corrige D-07).
  AC : page de test locale avec un iframe dont l'URL est `/embed/<id>` simulée ; segments appliqués.
- **VG-32** Réponses hors ordre et changements rapides de vidéo : seule la réponse correspondant à l'identifiant **courant** est appliquée (corrige D-06) ; changement de réglage appliqué immédiatement à l'onglet ouvert (corrige D-13).
  AC : banc avec deux réponses inversées ; test d'API.
- **VG-33** Plein écran : les avis et la frise restent visibles (attachés au conteneur du lecteur et non à `documentElement`) ; sinon repli sur l'indicateur natif (corrige D-10).
  AC : test manuel avec capture dans CEF (NON VÉRIFIABLE sans CEF aujourd'hui).

**Interface**

- **VG-40** Indicateur VideoGuard dans la barre d'outils native, avec un état : actif et segments trouvés (nombre), actif sans segment, désactivé pour ce site, erreur réseau, mise en veille. Un clic ouvre le panneau natif (VG-41). Sobre, libellés courts, conforme au design de M-0005.
  AC : capture dans l'interface simulée (`ci/ui_mock_server.py`) pour chaque état.
- **VG-41** Panneau natif : liste des segments de la vidéo (catégorie, plage, durée, action prévue), frise horizontale proportionnelle colorée par catégorie (couleurs de l'éditeur de thèmes, avec contraste suffisant et information non portée par la seule couleur : forme ou libellé au survol), interrupteur « Désactiver pour cette vidéo / ce site / cette chaîne si disponible », statistiques locales (VG-50), lien vers Paramètres.
  AC : captures + test de la fonction qui construit les données du panneau.
- **VG-42** Frise sur le lecteur (meilleur effort) : segments colorés alignés sur la barre de progression de YouTube ; si les éléments attendus du DOM ne sont pas trouvés, aucune erreur, repli sur VG-41.
  AC : banc avec un DOM simulé complet et un DOM simulé sans barre.
- **VG-43** Avis dans le lecteur : une ligne (« Sponsor ignoré · Revenir »), animation courte, respect de `prefers-reduced-motion`, plus petits en mode « mini » ; texte entièrement en français ; accessible au clavier (focus, Échap pour fermer).
  AC : banc, captures, test d'accessibilité (rôle `status`, bouton atteignable au clavier).
- **VG-44** Raccourcis : « Passer le segment » et « Revenir » intégrés au système de raccourcis existant de NIGHTFALL, configurables, **sans effet lorsque NIGHTFALL est verrouillé** ; aucun conflit avec les raccourcis de YouTube ni ceux de `on_pre_key_event`.
  AC : auto-test de verrouillage ; table de conflits revue par l'Auditeur.

**Statistiques et réglages**

- **VG-50** Statistiques locales : nombre de segments sautés, temps gagné total et sur 7 jours, par catégorie ; stockées dans la base chiffrée (comme `videoguard`, `state.rs:377`) ; bouton « Réinitialiser » ; jamais transmises.
  AC : tests Rust (cumul, réinitialisation) ; recherche d'envoi réseau : aucun.
- **VG-60** Section « VideoGuard » dans Paramètres (M-0006), regroupant tous les réglages (interrupteur général, par catégorie avec explication courte, durée minimale, avis, son, accélération des publicités, navigation privée, exceptions par site, serveur avancé, statistiques, attribution). Le Centre de sécurité n'affiche plus qu'une ligne d'état avec lien (pas de doublon). `videoguard.set` valide ses entrées (catégories connues, bornes) et applique le changement à l'onglet courant (corrige D-12 et D-13).
  AC : capture de la section ; test d'API : valeur hors bornes refusée.
- **VG-61** Export et import des réglages VideoGuard en JSON (sans identifiant utilisateur), avec version de format. Inclusion éventuelle dans la synchronisation chiffrée (`nf-sync`) à décider par l'Architecte.
  AC : aller-retour test.
- **VG-62** Attribution et licences (§9.3, texte validé par la décision 3) visibles dans Paramètres > VideoGuard et dans « À propos » ; la phrase courte du Centre de sécurité (`lib.rs:17-18`) renvoie à la version complète.
  AC : captures des deux pages ; chaque page contient, dans un texte français : la source avec un lien vers `https://sponsor.ajay.app`, le nom de la licence avec un lien vers `https://creativecommons.org/licenses/by-nc-sa/4.0/`, la mention « en l'état, sans garantie », la mention du filtrage, la mention de non-affiliation ; un clic sur un lien ouvre un onglet et aucune requête n'est faite sans clic (journal réseau de l'interface simulée) ; `docs/LICENSES.md` n'indique plus LGPL-3.0 pour l'extension.

**Tests et preuves**

- **VG-70** Le banc `preuves/videoguard_js_harness.js` devient un test permanent (dossier de tests de l'application ou `ci/`), exécuté en CI, qui **compare** chaque résultat à une valeur attendue (celles des AC de VG-11, VG-14, VG-15, VG-16, VG-30, et pour les cas déjà corrects A, B, D, F, G, H, K, L, les valeurs de `preuves/videoguard_js_harness.sortie.txt`).
  AC : (1) un run CI vert contient une étape dédiée dont le journal montre une ligne par cas et une ligne de synthèse (par exemple `VIDEOGUARD-HARNESS OK 20/20`) ; (2) le banc sort avec un code non nul au premier écart ; (3) l'Auditeur vérifie localement qu'une altération volontaire de `videoguard.js` (par exemple tolérance de fin `0.3` → `0`, ou suppression du réarmement) fait échouer le banc ; (4) le banc ne contient aucun code de SponsorBlock (recherche littérale de lignes communes, comme au §0).
- **VG-71** Un serveur API simulé (adresse de base remplaçable par une fonction de test) alimente un auto-test de bout en bout : page locale simulant une vidéo, segments reçus, saut observé, ligne `SELFTEST OK` conservée. L'auto-test existant doit continuer à finir sur https://example.com.
- **VG-72** Tests Rust pour tous les chemins de `fetch_segments` (simulés) : 200, 404, 400, 429, 500, délai, JSON invalide.

### 6.3 Soumission de segments et vote (périmètre à valider par le propriétaire)

Ce qu'exige le serveur (lecture de `SBS`) :

| Exigence | Source | Conséquence pour NIGHTFALL |
|---|---|---|
| Un `userID` d'au moins 30 caractères, envoyé en clair, haché par le serveur | `SBS/src/routes/postSkipSegments.ts:209-213,518-520`, `SBS/src/config.ts:204` | NIGHTFALL doit créer un identifiant secret persistant (aléatoire, de 32 caractères ou plus) ; il relie toutes les contributions de l'utilisateur et est transmis avec l'adresse IP |
| Identifiant de vidéo valide, service `YouTube` (ou `Spotify`), segments non vides, secondes sans « : », description vide sauf chapitre (≤ 200 caractères) | `postSkipSegments.ts:191-256` | validation locale avant envoi |
| `videoDuration` et `userAgent` | `postSkipSegments.ts:477-505` | NIGHTFALL doit lire la durée de la vidéo dans la page |
| Permission par catégorie et par utilisateur, limitation des nouveaux utilisateurs, avertissements et bannissements | `postSkipSegments.ts:228-242,543-548,578-583,603` ; `SBS/src/utils/permissions.ts:30-56` | rejets (400, 403, 429) à afficher mot pour mot de façon compréhensible |
| Règles de validation de requêtes configurables par l'opérateur (par exemple sur le `userAgent`) | `SBS/src/utils/requestValidator.ts`, `postSkipSegments.ts:523-537` ; `SBS/src/config.ts:73` (vide par défaut) | **Un client tiers peut être refusé à la discrétion de l'opérateur** ; NON VÉRIFIÉ pour le serveur de production |
| Règle côté client de l'extension : aperçu obligatoire avant envoi sauf début/fin de vidéo, pas d'envoi sur direct ou première | `SB/src/content.ts:2465-2477` | à reproduire par courtoisie envers la communauté |
| Limite de 15 votes par 15 min | `SBS/src/config.ts:60-65` | |
| Conditions d'utilisation et lignes directrices | wiki SponsorBlock (non consulté) | un écran d'acceptation lié à la page des lignes directrices (`SB/config.json.example`, clé `wikiLinks.guidelines`) |

Exigences si le propriétaire valide :
- **VG-80** Contribution **désactivée par défaut**. L'identifiant n'est créé qu'à l'activation explicite, après un écran qui explique : pseudonyme persistant, public, visible côté serveur avec l'adresse IP, licence CC BY-NC-SA des contributions, lignes directrices.
- **VG-81** L'identifiant est stocké dans la base chiffrée, jamais journalisé, exportable de façon explicite, régénérable, supprimable ; jamais envoyé lors d'une simple lecture.
- **VG-82** Aucune soumission ni vote en navigation privée (valeur par défaut), ou avec confirmation à chaque fois.
- **VG-83** Brouillons locaux par vidéo ; aperçu obligatoire ; messages d'erreur du serveur affichés tels que reçus.
- **VG-84** Vote : pour, contre, changer la catégorie, annuler ; aucun appel à `viewedVideoSponsorTime`.
  AC : tests avec serveur simulé (formats de requêtes), revue de la confidentialité par l'Auditeur, test de non-envoi sans activation.

Effort : L. Avant de construire, il faut un échange avec l'opérateur de SponsorBlock ou un essai contrôlé pour savoir si un client tiers est accepté (NON VÉRIFIÉ).

### 6.4 Ce qui est impossible ou limité avec CEF Alloy (à dire honnêtement dans l'interface)

- Pas d'extension : tout passe par du JavaScript injecté dans le monde principal de la page (`execute_java_script`, `client.rs:419-430`). Une page peut donc voir et perturber ce script (D-09) ; l'existence de mondes isolés dans cef-rs 156 n'a pas été vérifiée.
- Les éléments dans le lecteur YouTube dépendent de son DOM interne et peuvent casser sans préavis ; le panneau natif ne dépend pas de YouTube.
- La communication page → cœur existe déjà par `console.debug` avec un jeton aléatoire (`client.rs:147-161`, `browsing.rs:266-275`) mais le jeton est lisible par la page, donc **aucune action sensible (envoi, vote) ne doit être déclenchée par ce canal sans geste explicite dans l'interface native**.
- Onglets en arrière-plan : Chromium réduit la fréquence des minuteries ; le saut dans un onglet caché (musique de fond) peut être retardé. NON VÉRIFIÉ.
- Aucune autre plateforme vidéo n'a de données SponsorBlock (P6).

### 6.5 Cas limites à tester

| Cas | Attendu |
|---|---|
| NIGHTFALL verrouillé | aujourd'hui : requêtes et sauts continuent (D-19). Attendu : aucune requête, aucun script actif, aucun raccourci VideoGuard actif ; une requête partie avant le verrouillage n'est pas injectée (VG-25, VG-44) |
| Navigation privée | selon la décision du propriétaire (VG-24) ; jamais de contribution (VG-82) |
| Windows et macOS | mêmes comportements ; test de boucle et fin de vidéo sur macOS (VG-13) |
| Fermeture de l'onglet pendant une requête | pas d'erreur ni de blocage : `exec_in_tab` ne fait rien si le navigateur n'existe plus (`windows.rs:1505-1509`) ; à couvrir par un test |
| Changement rapide de vidéo, deux onglets sur la même vidéo, restauration de session | VG-32 |
| Vidéo en direct, première, vidéo sans durée connue | pas de saut ; pas de soumission (VG-83) |
| Serveur injoignable, 429 | état d'erreur discret, nouvel essai limité (VG-22) |
| Réglages anciens | chargement sans perte (VG-02) |

---

## 7. Lien avec la demande du propriétaire

La demande (`DEMANDE-2026-11-04-refonte.md`, point 5) vise un module « complet, moderne et parfaitement intégré ». Correspondance : catégories et actions (VG-01 à VG-05), comportements de saut (VG-10 à VG-17), récupération et soumission (VG-20 à VG-27, §6.3), synchronisation (VG-61), plateformes (P1 à P6), fonctionnalités « difficiles à retrouver ou manquantes » (§3, lignes « absent »), interface et paramètres (VG-40 à VG-62). Le message du jour sur les raccourcis actifs pendant le verrouillage est traité par VG-25 et VG-44 pour VideoGuard (le correctif général relève de M-0002).

---

## 8. Découpage proposé (piste pour l'Architecte et le Directeur)

| Lot | Contenu | Dépend de | Effort | Modèle suggéré |
|---|---|---|---|---|
| VG-0 | Correctif prioritaire du verrouillage, partie simple : pas de requête ni d'injection quand `is_unlocked()` est faux (`on_navigation`, fin de requête, `on_load_start`) + étape d'auto-test (D-19, VG-25, VG-05) | rien (décision 6 seulement pour la suite) | S | Opus (verrouillage = sécurité) |
| VG-A | Socle et correctifs : D-01, D-02, D-03, D-05, D-06, D-12, D-13, D-20, D-21, D-22 + tests Rust des chemins réseau + banc JS en CI avec valeurs attendues (VG-70 à VG-72) | rien | M | Sonnet |
| VG-B | Moteur de décision en Rust (catégories complètes, modes × types d'action, sourdine, fusion des intervalles, saut planifié, cache, ETag, erreurs, état) | VG-A, décision 5 | L | Opus (conception du fil d'exécution et de l'injection) |
| VG-C | Réseau et verrouillage complet : suivi du proxy, suspension des scripts déjà injectés pendant le verrouillage, navigation privée (VG-23, VG-25, VG-24) | VG-0, décisions 2 et 6 | M | Opus |
| VG-D | Interface native : indicateur, panneau, frise, avis (VG-40 à VG-43) | M-0005 (design), VG-B | L | Sonnet |
| VG-E | Paramètres > VideoGuard, export/import, statistiques (VG-50, VG-60 à VG-62) | M-0006, VG-B | M | Sonnet |
| VG-F | Lecteurs intégrés, YouTube Music, instances Invidious (VG-31, P3, P4) | VG-B | M | Sonnet |
| VG-G | Soumission et vote (§6.3) | décision 1, réponse de l'opérateur | L | Opus (sécurité, vie privée) |
| VG-H | Licences et attribution : `docs/LICENSES.md`, page « À propos », Paramètres (D-17, §9) | décision 3 | S | Haiku ou Sonnet |

Les lots 0, A et H peuvent démarrer sans attendre la conception de l'interface ; le lot 0 est recommandé en premier, car D-19 est le seul défaut de gravité élevée.

---

## 9. Licences et attributions

### 9.1 Faits établis

| Élément | Licence | Source |
|---|---|---|
| Extension SponsorBlock | GPL-3.0 « ou toute version ultérieure » ; **LGPL-3.0 uniquement avant** un certain commit (`7338af3b…`) | `SB/README.md:76-78`, `SB/LICENSE` (texte GPL v3), `SB/LICENSE-HISTORY.txt:1-3` |
| Serveur SponsorBlock | AGPL-3.0-only | `SBS/package.json:20`, `SBS/README.MD:39-41` |
| Base de données et réponses de l'API | CC BY-NC-SA 4.0 « sauf permission explicite » de l'auteur, avec lien vers `https://creativecommons.org/licenses/by-nc-sa/4.0/` | `SBS/README.MD:11` ; renvoi à une page « Database-and-API-License » (`SB/README.md:53`) **non consultée** (refusée par le proxy). Aucun avis de droit d'auteur propre aux données n'a été trouvé dans les copies locales (le seul avis, « Copyright (c) 2019 Ajay Ramachandran », est dans l'ancienne licence MIT du code du serveur, `SBS/LICENSE.old:9`) |
| Icônes de l'extension | CC BY 3.0 (Flaticon) et iconmonstr | `SB/README.md:71-73` |
| Traductions, textes, couleurs de l'extension | couverts par la GPL du dépôt | `SB/README.md:76-78` (traductions : `SB/crowdin.yml`) |

Le commit `7338af3b…` est absent de la copie locale (`git log` échoue) : **la date du passage LGPL → GPL n'a pas pu être établie** ; la mention du Directeur (`JOURNAL.md:22-24`) l'affirme sans que j'aie pu le recouper. Conséquence : `docs/LICENSES.md:17` (« LGPL-3.0 ») est faux pour les versions actuelles (D-17).

### 9.2 Ce qui est permis sans changer la licence MPL-2.0 de NIGHTFALL

Cette section n'est pas un avis juridique ; le propriétaire décide (règle 12).

1. **Appeler l'API publique et afficher les segments** : aucun code GPL n'entre dans NIGHTFALL ; les données ne sont pas versées au dépôt. C'est ce que fait VideoGuard aujourd'hui.
2. **Étudier le code GPL** pour comprendre un comportement, puis écrire du code original d'après la description fonctionnelle. Les idées, les formats de l'API et les comportements ne sont pas protégés ; l'expression (code, structure détaillée, noms internes, textes, icônes, couleurs choisies, traductions) l'est.
3. **Non permis sans décision du propriétaire** : copier ou transcrire du code de l'extension (même adapté), reprendre ses fichiers de langue, ses icônes SVG ou PNG, ses feuilles de style ; mélanger du code GPL dans un fichier MPL obligerait à distribuer l'ensemble sous GPL.
4. **Procédure de salle blanche recommandée** pour les lots de construction : l'Exécuteur travaille **à partir de ce document uniquement** et n'ouvre pas `SB/src` ; l'Auditeur vérifie l'absence de lignes communes avec la méthode utilisée au §0 (recherche littérale), sachant qu'elle n'attrape que les copies directes.
5. **Noms et marques** : écrire « utilise l'API SponsorBlock » est factuel ; ne pas reprendre le logo ni nommer la fonction « SponsorBlock ». Le droit de marque de ce nom n'a pas été vérifié.

### 9.3 Obligations de CC BY-NC-SA 4.0 (texte lu)

**Source du texte** : la source indiquée par l'audit (`chromium.googlesource.com`, `licenses/CC-BY-NC-SA-4.0`) n'est **pas accessible** depuis cette machine (proxy : CONNECT refusé, 403), pas plus que `creativecommons.org`, SPDX sur `raw.githubusercontent.com` ou l'API GitHub. Le texte a été lu dans la copie locale décrite au §1 (paquet `doclicense` de TeX Live, fichier `doclicense-CC-by-nc-sa-4.0-plaintext.tex`, SHA-256 `e66c269d4819aaab34b49ef5220c4ddab6756f21bb5180761a4eb8561f2b7bbd`). Ci-dessous, « L. n » = ligne n de ce fichier. Cette copie n'a pas pu être comparée au texte officiel : **à recouper** par le Directeur ou le propriétaire. Ce qui suit n'est pas un avis juridique.

Définitions utiles (section 1) :
- **NonCommercial** (§1(k), L. 125-131) : « not primarily intended for or directed towards commercial advantage or monetary compensation » (pas principalement destiné à obtenir un avantage commercial ou une rémunération, ni orienté vers ce but).
- **Share** (§1(l), L. 133-139) : mettre le matériel à la disposition **du public** par un moyen qui exige l'autorisation (reproduction, diffusion, communication…). Les conditions d'attribution de la section 3 s'appliquent « If You Share the Licensed Material » (§3(a)(1), L. 244-245).
- **Adapted Material** (§1(a), L. 73-81) : matériel dérivé, modifié d'une façon qui exige l'autorisation. Les modifications techniques nécessaires à l'usage ne produisent jamais de matériel adapté (§2(a)(4), L. 174-184).

Obligations quand on partage (§3(a)(1), L. 244-271) :
- (A) conserver, **si le concédant les fournit** : (i) l'identification du ou des créateurs, de la façon raisonnable qu'il demande ; (ii) un avis de droit d'auteur ; (iii) un avis renvoyant à la licence ; (iv) un avis renvoyant à l'**exclusion de garantie** ; (v) un URI ou lien vers le matériel, dans la mesure du raisonnable ;
- (B) indiquer si le matériel a été modifié et conserver l'indication des modifications antérieures ;
- (C) indiquer que le matériel est sous cette licence et inclure le texte de la licence **ou** son URI ou lien.
- §3(a)(2) (L. 273-278) : ces conditions peuvent être remplies de toute manière raisonnable selon le support, par exemple par un lien vers une page qui contient les informations. §3(a)(3) (L. 279-281) : retirer ces mentions si le concédant le demande.
- Autres clauses pertinentes : aucune suggestion d'affiliation ou d'approbation (§2(a)(6), L. 207-212) ; marques non concédées (§2(b)(2), L. 224-225) ; **ShareAlike** seulement si l'on partage du matériel adapté (§3(b), L. 283-300) ; droits sui generis sur les bases de données : l'extraction d'une « partie substantielle » est permise pour un usage non commercial, et la base qui l'inclut devient du matériel adapté (§4, L. 303-324) ; exclusion de garantie, matériel fourni « en l'état » (§5, L. 327 et suivantes).

Application à VideoGuard (interprétation, à valider) :
1. NIGHTFALL ne stocke ni ne redistribue la base : chaque installation demande à l'API les segments d'un préfixe et les affiche à **son** utilisateur. Que cela constitue un « Share » par NIGHTFALL est discutable ; la prudence recommande de remplir quand même §3(a)(1), ce qui coûte une ligne de texte et deux liens.
2. Filtrer par catégorie, durée minimale et tri n'est vraisemblablement pas une adaptation au sens du §1(a) ; SA ne s'appliquerait que si NIGHTFALL publiait une base ou des segments modifiés (aucun projet en ce sens ; VG-21 interdit la persistance sur disque). La soumission (§6.3) produirait des contributions placées sous la licence de la base par l'opérateur : à expliquer à l'utilisateur (VG-80).
3. §4 : une requête par vidéo n'extrait pas une partie substantielle de la base ; un cache local en mémoire non plus. Télécharger ou embarquer la base (`sponsor.ajay.app/database`) changerait l'analyse.
4. **La licence des données ne s'étend pas au code** : le code de NIGHTFALL n'est pas du matériel adapté de la base, il reste sous MPL-2.0. Les clauses NC et SA visent l'usage et le partage des **données**.
5. NC : une distribution gratuite sans publicité est compatible. Vente, abonnement, publicité, offre groupée payante, et possiblement certains modèles de dons, ne le seraient pas sans permission de l'auteur (§9.4).

État actuel, corrigé à la tentative 2 (l'affirmation « absente de la page À propos » était fausse) :
- Centre de sécurité : « Segments fournis par SponsorBlock (sponsor.ajay.app), licence CC BY-NC-SA 4.0. » (`lib.rs:17-18`, affichée par `security.js:97`) ;
- À propos : « Segments vidéo : SponsorBlock (données CC BY-NC-SA 4.0). » (`app/ui/about.html:14`).
- Présent : identification de la source (i, sous la forme « SponsorBlock »), mention de la licence (iii, C partiel). Absent : lien vers la licence ou son texte (C), lien cliquable vers la source (v, l'adresse n'est qu'écrite, et seulement dans le Centre de sécurité), renvoi à l'exclusion de garantie (iv), indication des modifications (B, si on considère le filtrage comme une modification), mention de non-affiliation (§2(a)(6), recommandée). Avis de droit d'auteur (ii) : aucun n'est fourni pour les données dans les sources lues, donc rien à reprendre tant que la page « Database-and-API-License » n'a pas été lue.

Cible (texte original NIGHTFALL, à valider, décision 3) : dans Paramètres > VideoGuard et dans « À propos », par exemple « Segments vidéo : données de la communauté SponsorBlock (lien `https://sponsor.ajay.app`), sous licence Creative Commons BY-NC-SA 4.0 (lien `https://creativecommons.org/licenses/by-nc-sa/4.0/`), fournies en l'état, sans garantie. NIGHTFALL les filtre selon vos réglages. NIGHTFALL n'est ni affilié à SponsorBlock ni approuvé par lui. » Les liens s'ouvrent dans un onglet sur geste de l'utilisateur (aucune requête automatique vers une autre origine, `PROJET.md`). `docs/LICENSES.md:17` corrigé (GPL-3.0 pour l'extension, AGPL-3.0 pour le serveur, CC BY-NC-SA 4.0 pour les données, permission explicite possible). Avant publication : lire la page « Database-and-API-License » pour la forme d'attribution éventuellement demandée (§3(a)(1)(A)(i)).

### 9.4 Risque « non commercial »

NIGHTFALL est gratuit et sans budget, ce qui est compatible aujourd'hui avec la définition du §1(k). Si le projet devait un jour être monétisé (vente, abonnement, publicité, et peut-être dons selon l'interprétation de « primarily intended for… monetary compensation »), l'usage des données SponsorBlock deviendrait un problème. Le propriétaire peut demander une permission explicite à l'auteur de SponsorBlock (c'est la voie prévue par `SBS/README.MD:11`). Décision à lui.

---

## 10. Ce qui n'a pas pu être vérifié

1. Exécution locale des tests Rust (crates.io inaccessible) : on s'appuie sur la CI de `develop` (même code).
2. Comportement du script dans CEF et sur le vrai lecteur YouTube (plein écran, cadence de `timeupdate`, sélection du bon `<video>` : `document.querySelector('video')`, `videoguard.js` fonction `attach`, qui prend le premier élément vidéo de la page), onglet en arrière-plan, macOS.
3. Réponse réelle du serveur de production (aucune requête faite), ses règles de validation de requêtes éventuelles, ses limites de débit pour la lecture.
4. Wiki d'API officiel (`wiki.sponsor.ajay.app/w/API_Docs`) et page de licence de la base (non accessibles, refusés par le proxy à la tentative 2). Texte de CC BY-NC-SA 4.0 : lu dans une copie locale non officielle (§9.3) ; la source officielle et celle indiquée par l'audit (`chromium.googlesource.com`) sont inaccessibles, la conformité de la copie n'est donc pas vérifiée.
5. `SB/maze-utils` (sous-module absent) : détection de la vidéo et de la publicité, génération de l'identifiant utilisateur, requêtes en arrière-plan. La section sur l'identifiant (§6.3) repose donc sur les seules exigences du serveur (≥ 30 caractères).
6. Date du passage de l'extension à la GPL (commit introuvable localement).
7. Prise en compte du proxy par `ureq` (D-08) : déduction de la configuration, non testée.
8. Les estimations d'effort (S, M, L) ne reposent sur aucune mesure.
9. Le champ `payments` (clé de licence, chapitres autorisés) de la configuration de l'extension (`SB/src/config.ts:108-114`) n'a pas d'autre usage visible dans `SB/src` ; son sens (fonctionnalité payante éventuelle liée aux chapitres) n'a pas été éclairci. Raison de plus pour ne pas reprendre `chapter` sans vérifier ce que l'opérateur autorise à un client tiers.
10. D-19 (verrouillage) n'a pas été reproduit dans CEF : établi par lecture du chemin complet (§4.5). Les défauts D-20 à D-22 sont vérifiés sur le lecteur simulé, pas sur le vrai lecteur YouTube.
11. D-01 : comportement du vrai lecteur YouTube après une publicité (restaure-t-il la vitesse choisie ?) non observé.

## 11. Décisions demandées au propriétaire (via le Directeur)

1. **Soumission et vote** (VG-80 à VG-84) : accepter ou non un identifiant pseudonyme persistant et des envois explicites, sachant que cela touche la vie privée (règle 6 de la charte) et que l'opérateur peut refuser un client tiers.
2. **Navigation privée** : VideoGuard doit-il (a) rester actif comme aujourd'hui, (b) être désactivé par défaut avec un réglage, ou (c) demander à chaque fois ? Recommandation de l'Analyste : (b), car la requête révèle une adresse IP et un groupe de vidéos même avec le préfixe.
3. **Licence des données** (§9.3, texte désormais lu) : confirmer l'usage non commercial au sens du §1(k) ; valider le texte d'attribution proposé (source avec lien, licence avec lien, « en l'état, sans garantie », filtrage, non-affiliation) et ses deux emplacements (Paramètres > VideoGuard, À propos) ; décider s'il faut solliciter une permission explicite de l'auteur. Préalable conseillé : faire lire par quelqu'un qui y a accès le texte officiel de la licence et la page « Database-and-API-License ».
4. **Accélération des publicités** (F-156) : conservée par la règle 7 ; accepter qu'elle soit présentée honnêtement comme dépendante du fonctionnement interne de YouTube et susceptible de contrevenir à ses conditions d'utilisation (D-18).
5. **Nouveau — types d'action `mute` et `full`** (VG-02, VG-04) : aujourd'hui, les segments `mute` et les étiquettes de vidéo entière sont ignorés. Les prendre en charge ajoute un comportement (le son coupé pendant un segment). Valeurs par défaut des deux interrupteurs globaux : (a) activés, comme l'extension (`SB/src/config.ts:391-392`), ou (b) désactivés, pour ne rien changer au comportement actuel sans action de l'utilisateur. Recommandation de l'Analyste : (a) pour « vidéo entière » (simple étiquette, sans effet sur la lecture) et (b) pour « sourdine » jusqu'à ce que VG-12 soit vérifié dans CEF.
6. **Nouveau — verrouillage** (D-19, VG-25) : confirmer que VideoGuard doit être entièrement suspendu pendant le verrouillage (recommandé, conforme à la consigne sur les raccourcis actifs pendant le verrouillage) ; et décider, plus largement, si le verrouillage doit mettre en pause ou couper le son des médias des onglets masqués (aujourd'hui ils continuent de jouer, `windows.rs:622`). Cette seconde question dépasse VideoGuard : à traiter avec M-0002.

## 12. Hors périmètre (vu, non traité)

- Le suivi des raccourcis clavier actifs pendant le verrouillage (M-0002).
- `docs/agents/FONCTIONNALITES.md:221` : référence de fonction inexistante pour F-154 (à corriger par M-0001).
- `docs/STATUS.md:32` : état « saut réel à vérifier » à mettre à jour après les tests.
- Pendant le verrouillage, les pages des onglets masqués continuent de s'exécuter et de jouer du son (`windows.rs:622-630`) : question générale du verrouillage, au-delà de VideoGuard (décision 6, M-0002).
- Discrétion de l'ensemble des scripts injectés par NIGHTFALL (`window.__nfMed`, `browsing.rs:269`, scriptlets de Shield) : à évaluer dans une mission distincte ; VG-30 ne couvre que VideoGuard.

## 13. Tentative 2 : traitement des points de l'audit 1

Chaque point a été revérifié dans le code avant d'être appliqué ; aucun n'a été jugé faux.

| Point de l'audit | Vérification | Correction |
|---|---|---|
| MAJEUR 1 — VideoGuard actif pendant le verrouillage | lecture de `api.rs:816-829`, `windows.rs:518-560,622-630`, `client.rs:113-129,204-243,426-430`, `state.rs:352-354` | D-19 ajouté, §4.5, VG-25 réécrit avec critère (0 requête après verrouillage puis vidéo suivante), VG-05, §6.5, lot VG-0, décision 6 |
| MAJEUR 2a — segments imbriqués | banc, cas M1 et M2 (`videoguard.js:66-84`) | D-20 (VÉRIFIÉ), S5, VG-11 et son critère |
| MAJEUR 2b — segment à 0 jamais réarmé | banc, cas N et témoin (`videoguard.js:125`) | D-21 (VÉRIFIÉ), S8, VG-14 et son critère |
| MAJEUR 3 — confusion mode utilisateur / type d'action | `SB/src/config.ts:22-25,391-394`, `SB/src/types.ts:32-38,52-58`, `SB/src/utils/skipRule.ts:39-41`, `SBS/src/config.ts:24-37` | O1, S4, C2, C6, C7 (nouveau), VG-01, VG-02, VG-04, décision 5 |
| MAJEUR 4 — licences | `app/ui/about.html:14` ; texte de la licence lu dans une copie locale (source de l'audit inaccessible, §1) | §0 point 6, §9.1, §9.3 réécrit (§1(k), §1(l), §3(a)(1) complet, §2(a)(6), §3(b), §4, §5), décision 3 |
| MINEUR — cas E non concluant | banc refait (segment 10-40) | §4.2 cas E |
| MINEUR — D-03 scénario réel | `lib.rs:46`, `git log -S actions`, `security.js:89-91` | D-03, §4.1 |
| MINEUR — D-12 faux en général | `SBS/src/utils/parseParams.ts:29-34`, `parseSkipSegments.ts:23-24` | D-12 |
| MINEUR — réinjection pour la même vidéo | banc, cas O (`videoguard.js:145`) | D-22, VG-14 |
| MINEUR — VG-30 surestime la discrétion | `browsing.rs:269`, banc cas P | D-09, VG-30 (portée et critère), §12 |
| MINEUR — VG-70 sans critère | — | VG-70 avec 4 critères |
| MINEUR — citations décalées | `crates/nf-sync/src/lib.rs:1-18`, `SB/src/utils/skipProfiles.ts:85-98` | §2.1, R10, O3 |
| MINEUR — résumé §0 trop affirmatif sur D-01 | banc simulé seulement | §0 point 5, D-01, cas I, VG-16, §10 |
