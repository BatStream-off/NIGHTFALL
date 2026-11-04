# M-0004 : audit de VideoGuard face à SponsorBlock et spécification du module cible

Auteur : Analyste (Sonnet 5.5), 2026-11-04. Aucune modification du navigateur. Ce document établit des faits et propose ; les décisions appartiennent au Directeur et au propriétaire.

Conventions :
- `fichier:ligne` sans préfixe = dépôt NIGHTFALL (commit `80f7924`, identique à `origin/develop` `2408f95` pour `crates/nf-videoguard`, `app/src/nf/client.rs` et `app/ui/security.js`, vérifié par `git diff`).
- `SB:` = copie de référence `/home/claude/ref/SponsorBlock` (extension, manifeste version 6.1.7, `manifest/manifest.json:4`, commit local `454c2d8`).
- `SBS:` = copie de référence `/home/claude/ref/SponsorBlockServer` (commit local `b691d94`).
- Statuts : VÉRIFIÉ (exécuté ou lu et confronté), LU (déduit de la lecture du code sans exécution), NON VÉRIFIÉ (écrit comme tel avec la raison).
- Rien de SponsorBlock n'a été copié : SB et SBS ont été lus pour en décrire le comportement en mots. Voir §9 pour la méthode de « salle blanche » recommandée.

---

## 0. Résumé pour le Directeur

1. **La prémisse « VideoGuard est basé sur du code repris de SponsorBlock » est inexacte.** VideoGuard est un client original de l'API publique : `crates/nf-videoguard/src/lib.rs:3-6` le dit, et une comparaison automatique (toute ligne de 30 caractères ou plus de `videoguard.js` et de `lib.rs` recherchée telle quelle dans `SB/src` et `SBS/src`) ne trouve **0 ligne commune** sur 150 + 286 lignes. Limite : cette méthode ne détecte pas une réécriture proche ; elle écarte seulement une copie littérale.
2. **VideoGuard couvre une fraction étroite de SponsorBlock** : 8 catégories sur 12 côté serveur, un seul type d'action (`skip`) sur 5 (`skip`, `mute`, `full`, `poi`, `chapter`), pas de saut en sourdine, pas de barre de segments, pas de vote ni de soumission, pas de statistiques, pas de profils par chaîne, une seule plateforme (YouTube, onglet principal).
3. **Points forts à préserver** : préfixe de hachage de 4 caractères (plus discret que l'extension qui en envoie 5 pour les segments, `SB/src/utils/segmentData.ts:62`), aucun identifiant ni cookie envoyé, requête faite par le cœur du navigateur et non par la page (`client.rs:227-241`), accélération des publicités vidéo (F-156), valeurs par défaut existantes.
4. **Défauts constatés** (§5) dont deux **vérifiés par exécution** : la vitesse de lecture choisie par l'utilisateur est écrasée (remise à 1) après une publicité (D-01) ; en mode « Proposer de sauter », le bouton disparaît au bout de 6 s et n'est plus jamais reproposé, même si le segment dure plusieurs minutes ou si l'utilisateur revient en arrière (D-02). Plusieurs autres sont établis par lecture (requête inutile quand aucune catégorie n'est active, absence de cache, navigation privée traitée comme la navigation normale, lecteurs intégrés sans segments, courses entre réponses, contournement probable d'un proxy système).
5. **Licences** : l'extension SponsorBlock est sous GPL-3.0-ou-ultérieure (`SB/README.md:76-78`), et **non LGPL** comme l'écrit `docs/LICENSES.md:17` (confirmé aussi par `docs/agents/JOURNAL.md:22-24`). Les données sont sous CC BY-NC-SA 4.0 « sauf permission explicite » (`SBS/README.MD:11`). L'attribution actuelle (`lib.rs:17-18`, affichée seulement dans le Centre de sécurité, `security.js:97`) est incomplète (pas de lien, pas de mention des modifications, absente de la page « À propos »). Le texte de la page « Database-and-API-License » n'a pas pu être consulté (§10).
6. **Trois décisions du propriétaire sont nécessaires avant toute construction** (§11) : soumission et vote (identifiant pseudonyme persistant, vie privée), comportement en navigation privée, et risque lié à la clause « non commercial ».

---

## 1. Méthode, sources et limites

| Source | Usage | Remarque |
|---|---|---|
| Code NIGHTFALL (liste §2) | lecture complète de `nf-videoguard` et de ses points d'intégration | tout est cité en `fichier:ligne` |
| `SB/src/**`, `SB/config.json.example`, `SB/manifest/` | comportement de l'extension | **`SB/maze-utils` est un sous-module absent de la copie** (dossier vide) : détection de la vidéo, de la publicité, du lecteur, génération de l'identifiant utilisateur et requêtes en arrière-plan n'ont pas pu être lues |
| `SBS/src/**`, `SBS/README.MD` | contrat réel de l'API (paramètres, erreurs, règles de soumission) | licence du serveur AGPL-3.0-only (`SBS/package.json:20`) |
| CI de `develop` (run 41825393127, job 126558718472 « Modules (ubuntu-latest) ») | exécution des tests Rust de `nf-videoguard` | 7 tests passés (journal à 12:00:00Z, cible `nf_videoguard`) |
| Banc local `preuves/videoguard_js_harness.js` | exécution du vrai `videoguard.js` dans Chromium sans tête avec un lecteur simulé | sortie dans `preuves/videoguard_js_harness.sortie.txt` |
| Documentation publique de l'API (wiki.sponsor.ajay.app) | contrat officiel | **non consultable depuis cette machine** (domaine non autorisé pour WebFetch ; recherche web sans résultat exploitable). Le contrat a donc été établi à partir du code du serveur (`SBS`), qui fait foi pour le comportement, pas pour les engagements de l'opérateur |

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
| Synchronisation | `crates/nf-sync/src/lib.rs:1,19` : uniquement les favoris (`nightfall-bookmarks.nfsync`) | VideoGuard n'est pas synchronisé |
| Tests d'auto-test CI | aucune occurrence de `videoguard`, `youtube` ou `sponsor` dans `app/src/nf/selftest.rs` (recherche faite) | pas de scénario de bout en bout |

### 2.2 Flux réel

1. Changement d'adresse du cadre principal → `on_navigation` (`client.rs:204`).
2. Si `nf_videoguard::video_id(url)` est `Some` et la configuration est active (`client.rs:228-230`) : `background(fetch_segments, ...)`.
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
| C2 | `exclusive_access` (« Accès exclusif », étiquette de vidéo entière, action `full`) | `SB/config.json.example` (`categorySupport.exclusive_access = ["full"]`), `SB/src/utils/videoLabels.ts:30` (`/api/videoLabels/<préfixe>`) | absent | absent | Oui (étiquette affichée dans le panneau et la barre), M |
| C3 | `hook` (accroche / salutations) | `SB/config.json.example`, `SB/src/config.ts` (couleur `hook`) | absent | absent | Oui, S |
| C4 | `poi_highlight` (point fort, action `poi` : aller au point fort) | `SB/config.json.example` ; bouton `SB/src/js-components/skipButtonControlBar.ts:15` ; raccourci `SB/src/config.ts:94` (`skipToHighlightKeybind`) | absent | absent | Oui, M |
| C5 | `chapter` (chapitres nommés par la communauté) | `SB/config.json.example` ; rendu `SB/src/js-components/previewBar.ts:464,1063` (`renderSegmentsAsChapters`) | absent | absent | Réserve : le rendu de chapitres dans la barre de YouTube dépend du DOM du lecteur ; alternative : liste dans le panneau natif. L |
| C6 | Types d'action `skip`, `mute`, `full`, `poi`, `chapter` | `SB/src/types.ts:52-58` ; compatibilité par catégorie `SB/config.json.example` (`categorySupport`) | seulement `skip` demandé (`lib.rs:176`) ; les autres sont ignorés à l'analyse (`lib.rs:152-154`, test `parse_filters_by_video_and_config`) | partiel | Oui, M |

### 3.2 Options de configuration

| # | Option | SponsorBlock | VideoGuard | État | Faisabilité / effort |
|---|---|---|---|---|---|
| O1 | Action par catégorie : Désactivé / Afficher seulement / Passage manuel / Saut auto | `SB/src/types.ts:32-38` (`CategorySkipOption`), `SB/src/config.ts:106` | `Skip`, `Show`, `Ignore` (`lib.rs:32-41`), liste déroulante `security.js:89-92` | partiel (pas de « afficher seulement », pas de sourdine) | Oui, S |
| O2 | Valeurs par défaut | `SB/src/config.ts:470-480` : `sponsor` auto, `poi_highlight` manuel, `exclusive_access` et `chapter` affichage seul | `lib.rs:63-67` : `sponsor`, `interaction`, `selfpromo` auto ; `intro`, `outro`, `preview` proposé ; reste ignoré | présent, différent | À préserver tel quel pour les 8 existantes (règle 7) |
| O3 | Durée minimale d'un segment | `SB/src/config.ts:43` (`minDuration`, défaut 0), `SB/src/utils/skipProfiles.ts:85-101` | `min_duration` (1 s, `lib.rs:47-48,70`), **absent de l'interface** (`security.js:85-98`) | partiel | Oui, S |
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
| S4 | Sourdine sur un segment | `SB/src/content.ts:876-886`, `1797-1802` | absent | absent | Oui, M |
| S5 | Segments qui se chevauchent | regroupement `SB/src/content.ts:1612-1660` | enchaînement un par un (vérifié, cas F : 11 → 20 → 30) ; deux avis successifs se remplacent | présent, rudimentaire | Oui, S |
| S6 | Fin de vidéo, boucle, listes de lecture, macOS | `SB/src/content.ts:1773-1785` (cas de bouclage signalés par des commentaires) | saute à `end` sans traitement particulier | NON VÉRIFIÉ dans CEF | Réserve : à tester dans CEF Windows et macOS, M |
| S7 | Pause d'une vidéo ou publicité en cours | `SB/src/content.ts:655-662` | ignoré tant qu'une publicité est affichée (`videoguard.js:65`) | présent | à préserver |
| S8 | Recul manuel dans la vidéo puis re-saut | `SB/src/content.ts` (planification recalculée) | `seeking` réarme les segments situés plus loin (`videoguard.js:122-126`) ; vérifié (cas D) | présent pour `skip`, absent pour `show` (D-02) | Oui, S |
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
| R10 | Synchronisation des réglages | `chrome.storage.sync` du navigateur (config sync dans `SB/src/config.ts`), exportation `SB/src/utils/exporter.ts` | aucune ; `nf-sync` ne gère que les favoris (`nf-sync/src/lib.rs:1`) | absent | Réserve : export/import JSON local, S ; intégration à `nf-sync` (format versionné `NFSYNC01`) à concevoir par l'Architecte, M |
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
| `old_saved_config_gets_video_ad_skipping` (281) | ancien réglage sans `skip_video_ads` → activé | `actions` vide : la liste des catégories actives devient vide (voir D-03) |

**Non couverts** : `fetch_segments` (aucun test réseau ni simulé : 404, 400, 429, délai, JSON invalide), `injection_script` avec données malicieuses, tout comportement de `videoguard.js` avant cette mission, tout le chemin `client.rs`, `api.rs`. Aucun auto-test CI de bout en bout.

### 4.2 Banc local sur le vrai `videoguard.js` (exécuté ici)

Fichiers : `preuves/videoguard_js_harness.js` (écrit pour la mission) et `preuves/videoguard_js_harness.sortie.txt`. Principe : Chromium sans tête (Playwright, Chromium 153), l'URL `https://www.youtube.com/watch?v=dQw4w9WgXcQ` est servie par interception réseau avec un lecteur simulé (`#movie_player` + `<video>` dont `currentTime`, `duration`, `playbackRate` sont remplacés), les événements `timeupdate` et `seeking` sont émis à la main. **Limite** : ce n'est pas le lecteur YouTube ni CEF ; la cadence réelle de `timeupdate` n'est pas simulée.

| Cas | Résultat observé | Conclusion |
|---|---|---|
| A. segment `skip` 10-20 s, lecture à 12 s | `currentTime` passe à 20 ; avis « Segment ignoré : Sponsor » + bouton « Revenir » | VÉRIFIÉ |
| B. clic sur « Revenir » puis lecture à 12,3 s | retour à 12 ; pas de re-saut | VÉRIFIÉ |
| D. retour manuel à 5 s puis lecture à 11 s | re-saut à 20 | VÉRIFIÉ |
| C. segment `show` 0-30 s | avis « Intro détecté » + « Passer », position inchangée ; **avis disparu à 6,3 s** | VÉRIFIÉ : le bouton « Passer » n'existe plus 6 s après l'entrée dans un segment de 30 s (D-02) |
| E. recul manuel puis nouvelle entrée dans le segment `show` | aucun nouvel avis | VÉRIFIÉ : une seule proposition par segment et par vidéo (D-02) |
| F. segments chevauchants 10-20 et 19-30 | 11 → 20 → 30 (deux événements) | VÉRIFIÉ |
| G. lecture à 19,8 s (fin - 0,2 s) | pas de saut | VÉRIFIÉ : tolérance de 0,3 s (`videoguard.js:69`) |
| H. navigation SPA vers une autre vidéo (`pushState`) avec les segments de l'ancienne | aucun saut | VÉRIFIÉ : la garde protège |
| I. vitesse 1,5 → publicité (`ad-showing`) → fin de publicité | pendant : vitesse 16, position 599,9 (fin) ; **après : vitesse 1** | VÉRIFIÉ : la vitesse choisie est perdue (D-01) |
| K. segment d'action `mute` fourni directement au script | aucune action | VÉRIFIÉ : le script n'implémente que `skip` et `show` |
| L. segment 30-31,2 s, ticks à 29,9 puis 30,15 | saut à 31,2 | VÉRIFIÉ (avec ticks simulés rapprochés) |

### 4.3 Gestion des erreurs réseau (lecture, non exécutée)

- 404 → aucun segment (`lib.rs:186`), correct : le serveur répond 404 quand aucune vidéo du préfixe n'a de segment (`SBS/src/routes/getSkipSegmentsByHash.ts:34`).
- Autres statuts (400, 429, 5xx), délai dépassé, JSON invalide → `VideoGuardError` consigné dans le journal (`client.rs:238`). Aucun nouvel essai, aucune information pour l'utilisateur, aucune distinction entre « pas de segment » et « service injoignable ».
- Une réponse dont **un seul** élément (même d'une autre vidéo du même préfixe) a un `segment` mal formé fait échouer toute l'analyse (`lib.rs:148`, `Vec<ApiVideo>` désérialisé d'un bloc). Risque faible car le serveur filtre déjà par `actionTypes`.

### 4.4 Vie privée

Établi par lecture :
- Envoyé : le préfixe de 4 hexadécimaux (65 536 groupes), la liste des catégories actives, `actionTypes`, un `User-Agent` constant `NIGHTFALL-VideoGuard/0.1` (`lib.rs:177`) ; aucun cookie (ureq n'en gère pas par défaut), aucun identifiant utilisateur. Le serveur voit forcément l'adresse IP et le moment.
- Le serveur ne reçoit pas l'identifiant complet : il renvoie toutes les vidéos du groupe et le filtrage est local (`lib.rs:150`, côté serveur `SBS/src/routes/getSkipSegmentsByHash.ts:30-33`).
- Comparaison : l'extension envoie 5 caractères pour les segments (`SB/src/utils/segmentData.ts:62`), 4 pour les catégories verrouillées (`SB/src/content.ts:1336`), un en-tête `X-CLIENT-NAME` (`segmentData.ts:69`) et, par défaut, le compteur de vues `viewedVideoSponsorTime` avec l'identifiant en clair (`SB/src/content.ts:1750`, `trackViewCount` activé par défaut, `config.ts` : `trackViewCount: true`). VideoGuard est donc plus discret.
- Écarts : navigation privée (D-04), absence de cache (D-05), contournement probable d'un proxy (D-08), détectabilité par la page (D-09).

---

## 5. Défauts et écarts constatés

Gravité : élevée, moyenne, faible. « Preuve » renvoie à §4.

| ID | Constat | Preuve | Statut | Gravité |
|---|---|---|---|---|
| D-01 | Après une publicité, la vitesse de lecture est remise à **1** au lieu de la valeur de l'utilisateur ; seule la sourdine est restaurée (`videoguard.js:99,110-111`) | banc, cas I | VÉRIFIÉ (lecteur simulé) | moyenne |
| D-02 | Mode « Proposer de sauter » : l'avis dure 6 s (`videoguard.js:51`), n'est jamais reproposé (`videoguard.js:77-79` ; le réarmement `122-126` ne touche que les clés `skip`) ; sur un segment long, plus aucun moyen de « Passer » | banc, cas C et E | VÉRIFIÉ | moyenne |
| D-03 | Si aucune catégorie n'est active (tout sur « Ignorer », ou `actions` vide après chargement d'un ancien réglage, `lib.rs:281-285`), la requête part quand même avec `categories=[]` : le serveur répond 400 « No valid categories provided » (`SBS/src/utils/parseSkipSegments.ts:22-23`), consigné à chaque navigation, et le préfixe est divulgué pour rien (`lib.rs:169-173`) | lecture de `fetch_segments` et du serveur | LU (non exécuté contre le serveur) | faible à moyenne |
| D-04 | Navigation privée : la requête est faite comme en navigation normale ; seul l'historique est conditionné par `private` (`client.rs:222` contre `227-241`) | lecture | LU | à arbitrer (§11) |
| D-05 | Aucun cache ni dédoublonnage : chaque changement d'adresse vers une vidéo relance une requête (`client.rs:129,227-241`) ; revenir sur une vidéo, ou un changement de paramètre d'URL, refait la requête | lecture | LU | faible (réseau, vie privée) |
| D-06 | Course : une réponse tardive d'une vidéo A injectée après navigation vers B remplace l'état (`videoguard.js:142-145`) sans que B ait ses segments ; l'injection d'une réponse arrivée avant le remplacement du document lors d'une navigation complète peut être perdue | lecture | LU ; effet réel NON VÉRIFIÉ (CEF absent) | faible à moyenne |
| D-07 | Lecteurs YouTube intégrés dans d'autres sites : aucun segment (déclenchement sur l'URL de l'onglet, `client.rs:228`), mais l'accélération des publicités s'y exécute (`client.rs:426-430`) | lecture | LU | moyenne |
| D-08 | La requête est faite par `ureq` avec `default-features = false` et `tls` (`Cargo.toml:49`) sans configuration de proxy (`lib.rs:174`) ; Chromium, lui, suit le proxy du système (`api.rs:1041`). Donc la requête VideoGuard pourrait **ne pas passer par le proxy ou le VPN configuré** et exposer l'adresse IP réelle | lecture ; comportement de ureq 2 sans la fonction de proxy d'environnement non testé | NON VÉRIFIÉ à l'exécution | moyenne (vie privée) |
| D-09 | Le script s'exécute dans le monde principal de la page (`client.rs:429`) et crée `window.__nightfallVideoGuard`, `window.__nightfallVideoGuardInit` (`videoguard.js:6,136,142`) et des éléments `#nf-vg-toast`, `#nf-vg-style` : YouTube peut détecter NIGHTFALL et peut appeler ces fonctions (effet limité à sa propre vidéo) | lecture | LU | moyenne (discrétion) |
| D-10 | L'avis est ajouté à `document.documentElement` (`videoguard.js:49`) : en plein écran, seul le sous-arbre de l'élément en plein écran est affiché (principe de la couche supérieure) ; l'avis serait invisible | raisonnement, non testé | NON VÉRIFIÉ | moyenne |
| D-11 | Précision du saut : fondée sur `timeupdate` (`videoguard.js:121`), dont la cadence va de 15 à 250 ms selon la spécification HTML ; jusqu'à environ 250 ms du segment peuvent être entendues. L'extension planifie le saut (`SB/src/content.ts:652-700`) | lecture ; cadence non mesurée dans CEF | NON VÉRIFIÉ (mesure) | faible |
| D-12 | `videoguard.set` accepte toute configuration sans contrôle (`api.rs:497-506`) : clés de catégories arbitraires envoyées telles quelles au serveur (réponse 400 qui bloque tout), `min_duration` quelconque ; `min_duration` n'a aucune commande dans l'interface (`security.js:85-98`) | lecture | LU | faible |
| D-13 | Changer un réglage n'agit ni sur l'onglet courant ni sur les options déjà injectées (`api.rs:497-506` ne déclenche rien ; `videoguard.js:136-139`) : l'accélération des publicités ne change qu'au rechargement | lecture | LU | faible |
| D-14 | Aucun retour utilisateur en cas d'échec (journal seulement, `client.rs:238`) ; aucun indicateur d'état | lecture | LU | faible |
| D-15 | Réglages dispersés : tout est dans le Centre de sécurité (`security.js:85-98`), rien dans Paramètres (`settings.js:569` ne cite que la réinitialisation) | lecture | LU | à traiter par M-0006 |
| D-16 | Pas de test automatisé du script de page avant cette mission, pas de test des chemins réseau, aucune étape d'auto-test (§4.1) | recherche dans `selftest.rs` | VÉRIFIÉ | moyenne |
| D-17 | Documentation : `docs/LICENSES.md:17` écrit LGPL-3.0 ; `docs/agents/FONCTIONNALITES.md:221` cite `client.rs:fetch_segments` qui n'existe pas (la fonction est `nf_videoguard::fetch_segments`, appelée en `client.rs:228-241`) ; `docs/STATUS.md:32` indique que le saut réel n'a jamais été vérifié | lecture | VÉRIFIÉ | faible |
| D-18 | L'accélération des publicités (hors SponsorBlock) peut contrevenir aux conditions d'utilisation de YouTube et dépend de noms de classes internes (`ad-showing`, `ytp-ad-skip-button`, `videoguard.js:96,107`), donc fragile aux changements de YouTube. Fonction à préserver (règle 7), mais à présenter honnêtement dans l'interface | lecture | LU | information |

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

- **VG-01** Les 12 catégories du serveur (`SBS/src/config.ts:22`) sont gérées, avec libellé français court, description utile en une phrase (rédigée par NIGHTFALL, pas copiée), et les actions possibles par catégorie (table inspirée du contrat serveur : sponsor/selfpromo/interaction/intro/outro/preview/hook/filler = sauter ou sourdine ; music_offtopic = sauter ; exclusive_access = étiquette ; poi_highlight = point fort ; chapter = chapitre). Une catégorie inconnue renvoyée par l'API est ignorée sans erreur.
  AC : test Rust avec une réponse contenant `category:"nouveau"` → ignorée ; test de table : chaque catégorie a au moins une action valide.
- **VG-02** Mode par catégorie : **Sauter automatiquement**, **Proposer**, **Couper le son** (si la catégorie le permet), **Afficher seulement**, **Ignorer**. Les 3 modes existants gardent leur sens (`Skip`, `Show`, `Ignore`, `lib.rs:32-41`) ; les nouveaux s'ajoutent à l'énumération (sérialisation `snake_case` inchangée pour l'existant).
  AC : un réglage enregistré avant la mission (JSON de `lib.rs:282`) se charge et se comporte comme avant (test de migration) ; chaque mode a un test de décision.
- **VG-03** Valeurs par défaut : celles de `lib.rs:63-67` pour les 8 catégories existantes ; `poi_highlight` = Proposer, `exclusive_access` = Afficher seulement, `chapter` et `hook` = Ignorer.
  AC : test `default_config_categories` conservé et étendu.
- **VG-04** `actionTypes` demandés = ceux nécessaires aux modes actifs (`skip`, plus `mute`, `full`, `poi`, `chapter` seulement si une catégorie les utilise).
  AC : test sur l'URL construite (sans réseau).
- **VG-05** Aucune requête n'est émise si aucune catégorie n'est active (corrige D-03) ou si VideoGuard est désactivé.
  AC : test Rust sur la fonction de construction de requête (retourne « rien à demander »).

**Décision de saut (cœur Rust, script de page)**

- **VG-10** Saut précis : déclenché à moins de 100 ms après le début du segment à vitesse 1 et 2, mesuré dans le banc ; un moteur à minuterie planifiée plutôt qu'à `timeupdate` seul (corrige D-11).
  AC : banc JS avec horloge simulée ; mesure en CI Windows/macOS si l'auto-test peut lire une vidéo locale (voir VG-71).
- **VG-11** Chevauchements fusionnés en un seul saut vers la fin la plus tardive, un seul avis.
  AC : cas F du banc → un seul saut 11 → 30, un avis.
- **VG-12** Sourdine : le son est coupé pendant le segment et restauré à la sortie, y compris si l'utilisateur change de position ; l'état de sourdine de l'utilisateur est respecté (s'il avait coupé le son, on ne le rétablit pas).
  AC : banc, 3 cas (entrée, sortie, recul manuel).
- **VG-13** Fin de vidéo, vidéo en boucle, liste de lecture : pas de bouclage ni de blocage lors d'un saut vers la fin.
  AC : banc pour la logique ; **test réel dans CEF Windows et macOS** (NON VÉRIFIÉ à ce jour, voir §10).
- **VG-14** « Revenir » ramène à la position d'avant le saut et ne re-saute pas ; un recul manuel avant le segment réarme tous les modes, y compris « Proposer » (corrige D-02).
  AC : banc, cas B, D et E modifiés (E doit re-proposer).
- **VG-15** « Proposer » : l'avis reste visible **tant que la lecture est dans le segment** (pas de disparition à 6 s), avec un bouton « Passer » ; il se ferme à la sortie du segment ou sur « Ignorer ce segment ».
  AC : banc, segment de 30 s, avis présent à 20 s.
- **VG-16** Pendant une publicité vidéo, aucun saut de segment ; à la fin de la publicité, la **vitesse et la sourdine choisies par l'utilisateur** sont restaurées (corrige D-01).
  AC : banc, cas I : vitesse après publicité = 1,5.
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
- **VG-25** Verrouillage : verrouillé, VideoGuard n'émet aucune requête et n'expose aucune donnée ; les réglages ne sont lisibles qu'une fois déverrouillé (comportement actuel : réglage chiffré chargé au déverrouillage, `api.rs:756-758`).
  AC : auto-test existant du verrouillage étendu.
- **VG-26** Réglage avancé « Serveur » (adresse HTTPS d'un miroir, défaut = serveur officiel) ; refus de `http://`.
  AC : test de validation.
- **VG-27** `viewedVideoSponsorTime` n'est **jamais** appelé (R8).
  AC : recherche de la chaîne dans le dépôt par l'Auditeur ; test de non-régression.

**Injection et discrétion**

- **VG-30** Aucune propriété globale ni identifiant d'élément à nom fixe visible de la page ; communication cœur → page par un jeton aléatoire par session ; le script s'exécute dans une fermeture.
  AC : un script de page de test énumère `window` et le DOM et ne trouve aucune chaîne `nightfall` ou `videoguard` (banc).
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
- **VG-62** Attribution et licences (§9) visibles dans Paramètres > VideoGuard et dans « À propos ».
  AC : capture ; recherche des liens.

**Tests et preuves**

- **VG-70** Le banc `preuves/videoguard_js_harness.js` devient un test permanent (dossier de tests de l'application ou `ci/`), exécuté en CI.
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
| NIGHTFALL verrouillé | aucune requête, aucun raccourci VideoGuard actif (VG-25, VG-44) |
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
| VG-A | Socle et correctifs : D-01, D-02, D-03, D-05, D-06, D-12, D-13 + tests Rust des chemins réseau + banc JS en CI (VG-70 à VG-72) | rien | M | Sonnet |
| VG-B | Moteur de décision en Rust (catégories complètes, sourdine, chevauchements, saut planifié, cache, ETag, erreurs, état) | VG-A | L | Opus (conception du fil d'exécution et de l'injection) |
| VG-C | Réseau : suivi du proxy et du verrouillage (VG-23, VG-25, VG-24) | décision 2 | M | Opus |
| VG-D | Interface native : indicateur, panneau, frise, avis (VG-40 à VG-43) | M-0005 (design), VG-B | L | Sonnet |
| VG-E | Paramètres > VideoGuard, export/import, statistiques (VG-50, VG-60 à VG-62) | M-0006, VG-B | M | Sonnet |
| VG-F | Lecteurs intégrés, YouTube Music, instances Invidious (VG-31, P3, P4) | VG-B | M | Sonnet |
| VG-G | Soumission et vote (§6.3) | décision 1, réponse de l'opérateur | L | Opus (sécurité, vie privée) |
| VG-H | Licences et attribution : `docs/LICENSES.md`, page « À propos », Paramètres (D-17, §9) | décision 3 | S | Haiku ou Sonnet |

Les lots A et H peuvent démarrer sans attendre la conception de l'interface.

---

## 9. Licences et attributions

### 9.1 Faits établis

| Élément | Licence | Source |
|---|---|---|
| Extension SponsorBlock | GPL-3.0 « ou toute version ultérieure » ; **LGPL-3.0 uniquement avant** un certain commit (`7338af3b…`) | `SB/README.md:76-78`, `SB/LICENSE` (texte GPL v3), `SB/LICENSE-HISTORY.txt:1-3` |
| Serveur SponsorBlock | AGPL-3.0-only | `SBS/package.json:20`, `SBS/README.MD:39-41` |
| Base de données et réponses de l'API | CC BY-NC-SA 4.0 « sauf permission explicite » de l'auteur | `SBS/README.MD:11` ; renvoi à une page « Database-and-API-License » (`SB/README.md:53`) **non consultée** |
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

### 9.3 Obligations d'attribution (à valider par le propriétaire avec le texte de la licence)

Le texte juridique de CC BY-NC-SA 4.0 n'a pas pu être consulté ici (domaine non autorisé). D'après la structure connue de cette licence, il faut : nommer le créateur et la source, indiquer la licence avec un lien, signaler les modifications éventuelles, ne pas suggérer l'approbation de l'auteur ; **NC** : pas d'usage commercial ; **SA** : une œuvre dérivée de la base partagée doit l'être sous la même licence. NIGHTFALL n'enregistre ni ne redistribue la base (aucune persistance proposée en VG-21), ce qui limite l'exposition à SA. **À confirmer** par une lecture du texte officiel.

État actuel : phrase courte « Segments fournis par SponsorBlock (sponsor.ajay.app), licence CC BY-NC-SA 4.0. » (`lib.rs:17-18`) affichée uniquement dans le Centre de sécurité (`security.js:97`) ; pas de lien, pas de page « À propos » concernée (recherche dans `app/ui/*.html` et `*.js` : aucune autre occurrence).

Cible : texte d'attribution avec liens (site, licence), présent dans Paramètres > VideoGuard et « À propos », et `docs/LICENSES.md` corrigé (GPL-3.0 pour l'extension, AGPL-3.0 pour le serveur, CC BY-NC-SA 4.0 pour les données, mention « permission explicite possible »).

### 9.4 Risque « non commercial »

NIGHTFALL est gratuit et sans budget, ce qui est compatible aujourd'hui. Si le projet devait un jour être monétisé (vente, abonnement, publicité, et peut-être dons selon l'interprétation), l'usage des données SponsorBlock deviendrait un problème. Le propriétaire peut demander une permission explicite à l'auteur de SponsorBlock (c'est la voie prévue par `SBS/README.MD:11`). Décision à lui.

---

## 10. Ce qui n'a pas pu être vérifié

1. Exécution locale des tests Rust (crates.io inaccessible) : on s'appuie sur la CI de `develop` (même code).
2. Comportement du script dans CEF et sur le vrai lecteur YouTube (plein écran, cadence de `timeupdate`, sélection du bon `<video>` : `document.querySelector('video')`, `videoguard.js` fonction `attach`, qui prend le premier élément vidéo de la page), onglet en arrière-plan, macOS.
3. Réponse réelle du serveur de production (aucune requête faite), ses règles de validation de requêtes éventuelles, ses limites de débit pour la lecture.
4. Wiki d'API officiel (`wiki.sponsor.ajay.app/w/API_Docs`) et page de licence de la base (non accessibles) ; texte de CC BY-NC-SA 4.0.
5. `SB/maze-utils` (sous-module absent) : détection de la vidéo et de la publicité, génération de l'identifiant utilisateur, requêtes en arrière-plan. La section sur l'identifiant (§6.3) repose donc sur les seules exigences du serveur (≥ 30 caractères).
6. Date du passage de l'extension à la GPL (commit introuvable localement).
7. Prise en compte du proxy par `ureq` (D-08) : déduction de la configuration, non testée.
8. Les estimations d'effort (S, M, L) ne reposent sur aucune mesure.
9. Le champ `payments` (clé de licence, chapitres autorisés) de la configuration de l'extension (`SB/src/config.ts:108-114`) n'a pas d'autre usage visible dans `SB/src` ; son sens (fonctionnalité payante éventuelle liée aux chapitres) n'a pas été éclairci. Raison de plus pour ne pas reprendre `chapter` sans vérifier ce que l'opérateur autorise à un client tiers.

## 11. Décisions demandées au propriétaire (via le Directeur)

1. **Soumission et vote** (VG-80 à VG-84) : accepter ou non un identifiant pseudonyme persistant et des envois explicites, sachant que cela touche la vie privée (règle 6 de la charte) et que l'opérateur peut refuser un client tiers.
2. **Navigation privée** : VideoGuard doit-il (a) rester actif comme aujourd'hui, (b) être désactivé par défaut avec un réglage, ou (c) demander à chaque fois ? Recommandation de l'Analyste : (b), car la requête révèle une adresse IP et un groupe de vidéos même avec le préfixe.
3. **Licence des données** : confirmer l'usage non commercial, valider le texte d'attribution, et décider s'il faut solliciter une permission explicite de l'auteur.
4. **Accélération des publicités** (F-156) : conservée par la règle 7 ; accepter qu'elle soit présentée honnêtement comme dépendante du fonctionnement interne de YouTube et susceptible de contrevenir à ses conditions d'utilisation (D-18).

## 12. Hors périmètre (vu, non traité)

- Le suivi des raccourcis clavier actifs pendant le verrouillage (M-0002).
- `docs/agents/FONCTIONNALITES.md:221` : référence de fonction inexistante pour F-154 (à corriger par M-0001).
- `docs/STATUS.md:32` : état « saut réel à vérifier » à mettre à jour après les tests.
