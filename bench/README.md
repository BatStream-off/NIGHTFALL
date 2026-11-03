# Protocole de benchmark NIGHTFALL

Les résultats ne sont publiés que s'ils sont **mesurés** avec ce protocole. Aucun objectif n'est présenté comme un résultat. À ce jour, **aucune mesure comparative n'a encore été réalisée** : elles exigent du matériel physique Windows et macOS identique pour tous les navigateurs. Les runners CI sont des machines virtuelles partagées, ce qui ne donne pas de mesures fiables.

## Navigateurs comparés

NIGHTFALL (Shield activé, puis désactivé), Brave (protections par défaut), Google Chrome et Firefox (sans bloqueur), plus Safari sur macOS. Les versions exactes sont notées dans chaque résultat.

## Conditions

- Même machine, branchée sur secteur, luminosité fixe, aucune autre application ouverte, redémarrage avant chaque série.
- Profils neufs, extensions désactivées, même résolution de fenêtre (1280×820).
- Deux séries : pages **enregistrées puis rejouées localement**, pour neutraliser le réseau, et **sites réels**.
- 5 répétitions par mesure. On publie la médiane et l'écart interquartile.

## Scénarios

1, 5, 10, 20 et 30 onglets ouverts depuis `bench/pages.txt`, plus une série de sites lourds : tableaux de bord, WebGL, vidéo 4K, applications React, Vue et Angular.

## Mesures et outils

| Mesure | Windows | macOS |
| --- | --- | --- |
| RAM totale (tous les processus du navigateur) | `bench/measure_windows.ps1` (somme des Private Working Set) | `bench/measure_macos.sh` (somme des RSS via `ps`) |
| CPU au repos (5 min) et en navigation | idem, compteurs de processus | idem, `ps -o %cpu` |
| Images par seconde et fluidité | MotionMark 1.3 | MotionMark 1.3 |
| Réactivité | Speedometer 3 | Speedometer 3 |
| JavaScript | JetStream 2 | JetStream 2 |
| Temps de chargement | `performance.timing` / LCP via la console | idem |
| Énergie | rapport `powercfg /srumutil` | `powermetrics` (sudo) |
| Stabilité | session de 24 h, nombre de plantages | idem |

## Publication

Résultats bruts (CSV) et machine utilisée dans `bench/results/<date>-<os>/`, avec un résumé dans `docs/STATUS.md`.
