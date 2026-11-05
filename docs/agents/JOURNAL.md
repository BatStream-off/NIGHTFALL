# Journal du Directeur

Décisions, arbitrages, montées de modèle, refus d'audit et consultations du
propriétaire, du plus récent au plus ancien. Chaque entrée : date, mission, décision,
raison, preuves.

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
