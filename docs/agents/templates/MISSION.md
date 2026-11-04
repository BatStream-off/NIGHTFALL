# {ID} : {TITRE}

- **Rôle** : {ROLE}
- **Modèle** : (opus | sonnet | haiku) — raison :
- **Statut** : PRÉPARÉE
- **Branche** : agent/{ID}-{SLUG}
- **Copie isolée (worktree)** : oui | non
- **Dépend de** : (missions qui doivent être intégrées avant)
- **Créée le** : {DATE}

## Objectif
(Ce que la mission doit produire, en une ou deux phrases.)

## Contexte
(Faits vérifiés utiles : symptômes rapportés, fichiers concernés, mesures existantes,
décisions déjà prises. Pas d'hypothèses présentées comme des faits.)

## Fichiers autorisés
Un motif par ligne entre les balises ; `mission.py scope` refuse tout autre fichier modifié.
<!-- scope -->
docs/agents/missions/{ID}-{SLUG}/**
<!-- /scope -->

## Critères d'acceptation
Chaque critère doit être vérifiable par l'Auditeur (test, journal de CI, capture, mesure).
1.

## Interdits
- Toute règle de `docs/agents/ORGANISATION.md` et de `docs/agents/PROJET.md`.
- Supprimer ou dégrader une fonctionnalité de `docs/agents/FONCTIONNALITES.md`.
- Modifier un fichier hors de la liste autorisée (le signaler dans le rapport à la place).

## Suppressions approuvées par le propriétaire
(aucune)

## Attribution des commits
(lignes fournies par le Directeur, avec le modèle réellement utilisé par l'agent :
`Co-Authored-By: Claude <Opus|Sonnet|Haiku> 5.5 <noreply@anthropic.com>`, puis la
ligne `Claude-Session` de la session en cours)

## Livrable
Rapport final au format `docs/agents/templates/RAPPORT.md`.
