# Journal du Directeur

Décisions, arbitrages, montées de modèle, refus d'audit et consultations du
propriétaire, du plus récent au plus ancien. Chaque entrée : date, mission, décision,
raison, preuves.

## 2026-11-07 — Fusions dans develop (accord écrit du propriétaire)
- **Accord** : « Oui, fusionne les trois » (PR #42, #43, #44).
- #42 (texte du fond retiré du Nouvel onglet) fusionnée : 59d5440. #43 (popup Shield)
  fusionnée : e24499e. Toutes deux à CI verte.
- #44 (lecteur NIGHTFALL) : develop fusionnée dans la branche ; conflits sans perte
  (`ci/ui_mock_server.py`, `ci/ui_render_test.js`, `docs/RELEASE_NOTES.md`, ce journal :
  les deux ajouts gardés). Test de rendu complet repassé en local avant la CI ; fusion
  dans develop seulement après CI verte.

## 2026-11-07 — Lecteur NIGHTFALL 0.1.0 : construit, signé et testé (Windows + macOS)
- **Workflow** `.github/workflows/player.yml`, run 42377494926 (commit `9fab433`) : vert
  sur les deux plates-formes. Signature VMP de production EVS acceptée et vérifiée
  (« streaming, 1349 days left ») ; macOS signé ad hoc après EVS. Auto-test dans le
  paquet signé, 9/9 sur chaque plate-forme : liste des services, données séparées,
  Widevine installé (4.10.3112.0), page isolée, sandbox, lecture Widevine réelle,
  navigation et fenêtre hors liste rendues à NIGHTFALL, autorisations refusées.
  Manifeste Ed25519 avec préfixe de contexte signé et vérifié (clé jetable).
- **Contraintes EVS découvertes** (FAQ castLabs, « Binary signature denied ») : seules
  les fuses de la liste de castLabs sont acceptées, d'où
  `GrantFileProtocolExtraPrivileges` = true ; sur macOS, l'empreinte ASAR ne doit pas
  être écrite dans le binaire du framework (`asarIntegrityDigest: false`) :
  l'intégrité de app.asar reste vérifiée via `Info.plist`. Diagnostic : ECS + fuses
  accepté, ECS renommé avec empreinte refusé.
- **Clé de signature** : la vraie clé `NIGHTFALL_UPDATE_KEY` n'est pas utilisée sur les
  branches de travail (elle serait exposée à tout commit poussé) ; décision de
  publication à prendre avec le propriétaire.
- **Mesures** ajoutées à `docs/MESURES.md`.

## 2026-11-07 — Lecteur compagnon : étape 1 (EVS + ad hoc sur macOS) RÉUSSIE
- **Décision du propriétaire** : le lecteur compagnon (Electron for Content Security de
  castLabs, MIT) est avancé, alors que l'entrée du 2026-11-06 le reportait après la 1.0 ;
  plan validé (tâche 5). Compte EVS « NIGHTFALL » créé par le propriétaire lui-même
  (script fourni), secrets `EVS_ACCOUNT_NAME` et `EVS_PASSWD` posés le 2026-11-07.
- **Preuve** (run 42374056833, `.github/workflows/ecs-poc.yml`, branche
  `poc-ecs-evs-macos`, macOS arm64) : ECS 45.2.1 (SHA-256 vérifiée), CDM Widevine
  4.10.3112.0 ; EVS : « Existing signature invalid: Certificate is valid for development
  only » puis signature de production « streaming, 1349 days left » ; `codesign
  --sign -` (Signature=adhoc, TeamIdentifier=not set, « valid on disk ») ; `verify-pkg`
  **après** la signature Apple : « Signature is valid ». Lecture Shaka « Angel One »
  Widevine dans le paquet signé : keySystem com.widevine.alpha, 6,0 s, 149 images,
  2 licences, image déchiffrée visible sur la capture. Secrets masqués (« *** »).
- **Limite honnête** : le témoin (ECS tel que publié, signature VMP de développement)
  lit aussi la vidéo : le serveur de licences de démonstration n'exige pas de VMP de
  production. La preuve établit donc que la chaîne EVS + ad hoc produit une signature
  VMP valide, pas qu'un service commercial l'acceptera : seul le test réel du
  propriétaire le dira. Aucun service n'est annoncé compatible.

## 2026-11-07 — Popup Shield corrigée ; compte castLabs ; lecteur compagnon
- **Popup Shield** (demande du propriétaire, accord écrit « corrige le bouton dans la
  popup ») : l'interrupteur mesurait 16×16 px au lieu de 40×22 parce que la règle
  `input[type=checkbox]` de `nf.css` (spécificité 0,1,1, depuis a4ee186) l'emportait
  sur `.toggle` (0,1,0) ; le bouton rond débordait de la piste. Le contenu (353 px
  sur macOS) dépassait aussi la taille d'ouverture (330 px) : lien « Ouvrir le
  Security Center » coupé jusqu'à l'ajustement par `popup.resize`. Correctif :
  sélecteurs `input.toggle`, `POPUP_SIZE.shield` = [320, 356]. Test ajouté dans
  `ci/ui_render_test.js`, à la taille que la popup demande (les polices Linux de la
  CI donnent 372 px : la première version, à taille fixe, échouait sur Ubuntu).
  Échoue sur develop, passe avec le correctif (Playwright local). PR #43.
- **Lecteur compagnon (castLabs ECS + EVS)** : le propriétaire avance la décision du
  2026-11-06 (« reporté après la 1.0 ») et valide le plan de la tâche 5.
- **Compte castLabs EVS** : la création du compte et la saisie du mot de passe ne
  peuvent pas être faites par l'agent ; le Directeur fournit un script que le
  propriétaire lance lui-même.

## 2026-11-05 soir — M-0002 et M-0003 ACCEPTÉES (audit final)
- Corrections du Directeur (autorisées par le propriétaire) : M-0003 0dc8013, M-0002 7729abb.
- Audit final (Opus, agent neuf) : **ACCEPTÉ** pour les deux, aucun MAJEUR. Mineurs : errata
  du §10 / §8 à reporter dans le corps des textes (P1-5 hors du lot « faible risque », `$to`
  exceptions jetées, portail captif, fourchettes) — à reprendre dans les fiches L1 ; précision
  `DoClose` transmise à M-0008.
- Limite de 5 h atteinte à 17:00, reprise à 18:00 (M-0007, M-0008, audit final).

## 2026-11-05 — Décisions du propriétaire ; lot L0 lancé
- **Validé** : plan (ordre L0 → L6) ; lancement de L0 ; le Directeur corrige lui-même M-0002 et
  M-0003 (refusées deux fois) puis audit final ; protection de `main` (le proxy refuse ce réglage
  au Directeur : instructions données au propriétaire) ; effacement de la copie Chromium des
  autorisations mémorisées au verrouillage et à la fermeture ; uBlock Origin exécuté en CI comme
  outil de mesure seulement.
- **Non validé** : VideoGuard désactivé par défaut en navigation privée (comportement actuel conservé).
- **Design** : le propriétaire veut voir les maquettes avant de décider ; 8 maquettes envoyées.
- **M-0005** : corrections livrées (011d19d), second audit à faire. **M-0006** : audit 2 REFUSÉ
  (2 MAJEUR : remise à zéro des autorisations ≠ comportement identique → réappliquer au
  déverrouillage ; critères de la vue d'ensemble du Security Center absents et parallélisme
  M-E3/M-E4/M-E5 faux) → deux refus : propriétaire à consulter.
- **L0** : M-0007 (sécurité du verrouillage, réglages robustes, autorisations Chromium) et M-0008
  (onglets, menus, Accueil), Exécuteurs Opus en parallèle ; M-0007 intègre en premier.

## 2026-11-05 — M-0004 ACCEPTÉE ; M-0003 arrêtée (règle 6) ; M-0002 livrée
- **M-0004** : audit 2 (Opus, agent neuf) ACCEPTÉ, 5 MINEUR (§0 licence à aligner sur
  §9.3, clause §4 simplifiée, empreinte par la liste des catégories envoyées → paramètres
  constants et filtrage local, moyen de l'auto-test VG-25 à préciser, réserve licence à
  lever). Banc rejoué et éprouvé par mutation de `videoguard.js`.
- **M-0003** : audit 2 (agent neuf) **REFUSÉ** — 3 MAJEUR sur la justesse des risques :
  `$to` en réalité ignoré par adblock-rust (`fb_network_builder.rs:260-264`), donc perte
  de couverture et non faux positifs (erreur introduite par la correction) ; P1-6
  `$removeparam` : exceptions `@@…$removeparam` rejetées par adblock-rust
  (`network.rs:850-851`) → réécriture d'URL là où uBO s'abstient ; analyse de sécurité
  incohérente : listes en cache non authentifiées déjà chargées avant déverrouillage
  avec la permission TRUSTED. Mesures et outillage confirmés. **Deux refus successifs →
  arrêt et consultation du propriétaire (charte §5).** Recommandation du Directeur :
  le Directeur corrige lui-même ces trois points (précis et vérifiables), puis troisième
  audit par un agent neuf.
- **Constat de sécurité à prioriser** (M-0003 audit 2, M3) : intégrité des listes
  Shield en cache (texte clair, permission TRUSTED pour 5 listes, chargées avant
  déverrouillage).
- **M-0002** livrée (Opus) : 4 bugs reproduits avec cause ; audit lancé.
- **M-0006** lancée (Architecte Opus) ; **M-0005** en cours.

## 2026-11-05 — Reprise après la limite hebdomadaire ; M-0003 et M-0004
- Limite **hebdomadaire** atteinte le 2026-11-04 au soir (M-0002, M-0003, audit M-0004
  interrompus), levée le 2026-11-05 à 8:00 ; reprise par SendMessage.
- **M-0004** : audit 1 REFUSÉ (4 MAJEUR) ; MAJEUR 1 vérifié par le Directeur
  (`lock_now` ne touche ni VideoGuard ni les médias ; aucun `is_unlocked()` dans
  `on_navigation`). Tentative 2 (Opus) livrée 7bf3f63 ; audit 2 lancé.
- **Constat de sécurité ajouté aux priorités** : pendant le verrouillage, les onglets
  masqués continuent de tourner (lecture vidéo et son, requêtes VideoGuard).
- **M-0003** : audit REFUSÉ (1 MAJEUR : cache du moteur sérialisé présenté à tort comme
  sûr ; 9 MINEUR) ; mesures confirmées. **Arbitrage** : le producteur était déjà Opus
  (pas de modèle supérieur) ; les corrections, textuelles et précises, sont confiées
  au même agent producteur, puis un **nouvel** Auditeur vérifiera.
- **Décisions à soumettre au propriétaire** : exécution de uBlock Origin (GPL) cloné
  en CI comme outil de mesure ; conservation des jobs « Audit Shield ».

## 2026-11-04 — M-0001 ACCEPTÉE (tentative 2)
- Analyste Opus ; audit 2 (Opus, agent neuf) : ACCEPTÉ, 9 MINEUR (P1–P9), à corriger
  dans une mission ultérieure de mise à jour du registre.
- **Processus** : #22 (`orga/refonte` → `main`) et #24 (`agent/M-0001-registre-v2` →
  `main`) fusionnées depuis l'interface web avant audit (titre tronqué par GitHub,
  10 s entre ouverture et fusion). Documents seulement. Le propriétaire a été prié de
  ne plus fusionner et s'est vu proposer une protection de `main` (non activée sans
  son accord). `develop` non resynchronisé : c'est une fusion (règle 9).

## 2026-11-04 — Refonte : phase d'audit lancée
- **Demande** du propriétaire : `missions/DEMANDE-2026-11-04-refonte.md`. Nouvelles
  règles reportées dans la charte (§1, règles 9 à 12), dont : **aucune fusion sans son
  autorisation explicite**, résultats d'audit et plan présentés avant toute
  modification importante.
- **Faits établis** : Shield utilise déjà adblock-rust 0.13.5 (dernière version) ;
  VideoGuard est un client de l'API SponsorBlock sans code de l'extension ; uBlock,
  tsurlfilter et SponsorBlock sont sous GPL-3.0, NIGHTFALL sous MPL-2.0 : étude
  seulement (règle 12). `docs/LICENSES.md` indique LGPL-3.0 pour l'extension
  SponsorBlock, alors qu'elle est sous GPL-3.0 depuis le commit 7338af3 (à corriger).
- **Missions d'audit** M-0002 à M-0006 préparées (modèles et raisons dans les fiches) ;
  au plus 3 agents à la fois ; ordre : M-0001 (reprise), M-0002, M-0003, puis M-0004,
  M-0005, M-0006.
- **Limite de débit** atteinte à 14:50 (M-0001 tentative 2 interrompue, reprise après
  la levée de la limite par SendMessage).

## 2026-11-04 — M-0001 : audit REFUSÉ, montée à Opus
- **Audit 1** (Opus, agent neuf) : REFUSÉ, 3 MAJEUR (F-137 anti-phishing annoncé mais
  jamais déclenché, F-147 9 listes et non 6, Couverture comptant des preuves hors
  sujet), 12 MINEUR. Archivé : `missions/M-0001-registre-fonctionnalites/audit-1.md`.
- **Vérification du Directeur** : M1 confirmé (`security.rs:40` appelle
  `check(url, false)`), M2 confirmé (9 `list(...)` dans `default_lists()`), H1
  plausible (aucun contrôle de verrouillage dans `on_pre_key_event` ni dans `shortcut()`).
- **Décision** : tentative 2 confiée à un nouvel Analyste **Opus** (règle de montée de
  niveau), à partir de `origin/develop`, corrections vérifiées une à une.
- **H1 (sécurité)** : raccourcis actifs sur l'écran de verrouillage. Priorité de la
  0.1.5 : à reproduire par un test, puis corriger. Signalé au propriétaire.
- **Processus** : le propriétaire a fusionné lui-même #19 (orga → develop), #20
  (develop → main) et #21 (M-0001 tentative 1 → **main**, avant la fin de la CI et avant
  l'audit). Sans effet sur le navigateur (documents). `develop` resynchronisé avec `main`
  (14d6c60). Le propriétaire a été prié de laisser les fusions au Directeur ou
  d'attendre sa validation.
- **Attribution** : un agent Sonnet a signé « Claude Opus 5.5 » parce que la fiche le
  disait ; le modèle de fiche demande désormais le modèle réellement utilisé.

## 2026-11-04 — Mode de repli pour cette session
- **Constat** (tests avec des agents Haiku) : types `nf-*` inconnus en cours de session,
  crochet PreToolUse de Claude Code inactif (`echo NF_DIRECTOR` exécuté), même après
  copie dans `~/.claude`, dans le dossier projet de la session et rechargement du dépôt.
  Le crochet git pre-push, lui, a refusé `git push --dry-run origin HEAD:develop`.
- **Constat** : `isolation: worktree` de l'outil Agent refusé (« Permission to use Agent
  with isolation:* has been denied »). Ajout de `mission.py worktree`.
- **Décision** : mode de repli de la charte §10 pour la fin de cette session.

## 2026-11-04 — M-0001 lancée
- Analyste, modèle Sonnet (raison dans la fiche), copie `../wt-M-0001` depuis
  `origin/orga/multi-agents`.

## 2026-11-04 — Mise en place de l'organisation multi-agents
- **Décision** : création de l'organisation permanente (charte `ORGANISATION.md`,
  5 agents `nf-*`, garde-fous `tools/agents/`, contrôle de préservation en CI).
  Demandée et validée dans son principe par le propriétaire ; les modifications
  fonctionnelles de la 0.1.5 attendent sa validation du résultat.
- **Règles ajoutées par le propriétaire** : reportées telles quelles dans la charte §1.
- **Modèles vérifiés** en lançant un agent avec chacun : `claude-haiku-5-5`,
  `claude-sonnet-5-5`, `claude-opus-5-5` répondent. Fable 5.1 est proposé par l'outil
  mais n'a pas été demandé : non utilisé sans accord du propriétaire.
- **Parallélisme** : 3 agents actifs au maximum (limite de débit atteinte lors de la
  0.1.4 avec 3 agents).
- **Garde-fous testés** : `guard.py --self-test`, crochets git contre un dépôt local
  jetable (commit et push sur `develop` refusés sans marqueur Directeur, suppression
  refusée), `preserve.py` rejoué sur 0.1.2 → 0.1.3 (détecte les éléments disparus de
  la refonte) et 0.1.3 → 0.1.4 (aucune disparition).

## 2026-11-06 — Décisions du propriétaire (finalisation)
- security.html remplacée par une redirection vers le Security Center des Paramètres (accord écrit : « Oui, rediriger »).
- Inclus dans la 1.0 : import/export (favoris HTML, mots de passe CSV), navigateur par défaut, suggestions de recherche en ligne (désactivées par défaut), Snap Layouts Windows.
- VideoGuard : lecture seule, aucun envoi ni vote SponsorBlock.
- Événements de sécurité : conservation 30 jours.
- Netflix et services à DRM : lecteur compagnon sur Electron de castLabs (ECS + EVS gratuit) reporté après la 1.0.
