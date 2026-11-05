# Rapport M-0001 (tentative 2)

(Version condensée archivée par le Directeur ; contenu fidèle au rapport rendu.)

- **Agent** : Analyste — claude-opus-5-5
- **Branche** : `agent/M-0001-registre-v2`, commit f401264 ; pas de PR demandée
- **Résultat** : TERMINÉ

## Fait
- Les 15 points de `audit-1.md` vérifiés dans le code puis appliqués (aucun jugé faux).
- Erreurs supplémentaires corrigées : F-118 (la réinitialisation oublie aussi les
  autorisations des sites, `api.rs:1023-1037`), F-113, F-155, F-182 (étapes de CI
  seulement en publication sur `main`).
- Classement explicite « prouvée / partielle / aucune » de chaque preuve, règle écrite
  en tête ; en-tête limité à ce qui a été vérifié.
- F-188 à F-198 ajoutées ; section « Renvois » (8 groupes de doublons, 4 lignes non
  visibles) sans suppression ; L-11 complétée, L-16 (H1) réécrite, L-21 et L-22 ajoutées.
- Couverture : 198 lignes = 29 prouvées, 76 partielles, 93 sans preuve.

## Preuves
Script `/tmp/m0001/check.py` (198 lignes, ID uniques, citations existantes, 120 tests,
20 étapes) ; `preserve.py origin/develop HEAD` : aucune disparition (187 → 198 ID) ;
`mission.py scope M-0001 --ref HEAD` : périmètre respecté.

## Limites
Classement « prouvée / partielle » fondé sur la lecture des tests ; rien d'exécuté dans
l'application ; CI non lancée par l'agent.

## Hors périmètre
H1 (raccourcis actifs pendant le verrouillage) ; L-21, L-22.
