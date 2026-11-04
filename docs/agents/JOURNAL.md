# Journal du Directeur

Décisions, arbitrages, montées de modèle, refus d'audit et consultations du
propriétaire, du plus récent au plus ancien. Chaque entrée : date, mission, décision,
raison, preuves.

## 2026-11-04 — Mode de repli pour cette session
- **Constat** (tests avec des agents Haiku) : types `nf-*` inconnus en cours de session,
  crochet PreToolUse de Claude Code inactif (`echo NF_DIRECTOR` exécuté), même après
  copie dans `~/.claude`, dans le dossier projet de la session et rechargement du dépôt.
  Le crochet git pre-push, lui, a refusé `git push --dry-run origin HEAD:develop`.
- **Décision** : mode de repli de la charte §10 pour la fin de cette session.

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
