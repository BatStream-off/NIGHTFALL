# Audit {ID}

- **Auditeur** : modèle :
- **Objet audité** : branche, commit, PR, run CI
- **Verdict** : ACCEPTÉ | REFUSÉ

## Vérifications faites moi-même
Chaque critère d'acceptation de la fiche, vérifié sans s'appuyer sur le rapport.

| Critère | Ce que j'ai vérifié | Preuve | Conforme |
|---|---|---|---|

## Contrôles systématiques
- [ ] Diff relu en entier ; aucun fichier hors périmètre (`mission.py scope`)
- [ ] Aucune fonctionnalité perdue (`preserve.py`, et lecture du diff)
- [ ] CI complète verte sur le dernier commit audité (jobs nommés)
- [ ] Captures de la CI regardées quand l'interface change
- [ ] Sécurité : aucun commutateur affaiblissant, aucun envoi de données, aucun secret
- [ ] Pas de fonctionnalité simulée ni annoncée sans test
- [ ] Gestion d'erreurs et cas limites (fermeture, verrouillage, navigation privée, plateformes)
- [ ] Textes de l'interface en français, honnêtes

## Problèmes trouvés
Classés du plus grave au moins grave : BLOQUANT (refus), MAJEUR (refus sauf
arbitrage du Directeur), MINEUR (acceptable, à corriger plus tard).

| Gravité | Fichier:ligne | Problème | Scénario qui le prouve |
|---|---|---|---|

## Confrontation avec le rapport de l'agent
(Après le verdict : affirmations du rapport non prouvées ou contredites.)
