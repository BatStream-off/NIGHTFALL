# Mesures de NIGHTFALL (lot L6)

Relevées par l'auto-test de bout en bout à chaque build (ligne `MESURES {…}` des
journaux de la CI, jobs « Application »). Machines de GitHub Actions, profil neuf,
listes Shield chargées. Ce sont des repères pour voir une régression d'une version à
l'autre, pas des chiffres de banc d'essai.

| Mesure | Windows (windows-latest) | macOS (macos-latest, arm64) |
|---|---|---|
| Première fenêtre affichée (depuis le lancement du processus) | 145 ms | 722 ms |
| Chargement d'example.com dans un nouvel onglet (onglet créé → chargement fini) | 517 ms | 607 ms |
| Processus de NIGHTFALL (navigateur, rendu, GPU, utilitaires) | 11 | 12 |
| Mémoire résidente totale de ces processus, 2 onglets ouverts | 595 Mio | 1 450 Mio |
| Règles Shield chargées | 181 659 | 181 659 |

Run du 06/11/2026, commit `3617a5b` (PR #38).

## Comment lire ces chiffres

- **Première fenêtre** : du début du processus à la création de la fenêtre, avant
  l'écran de code. Le reste du démarrage (Shield, réglages) se fait après le
  déverrouillage, en arrière-plan.
- **Mémoire** : somme de la mémoire résidente de tous les processus de NIGHTFALL
  (`nf_perf::process_tree_memory`). Sur macOS, la mémoire résidente compte aussi les
  pages partagées des bibliothèques du système et de Chromium ; elle n'est pas
  comparable directement au chiffre de Windows ni au « Moniteur d'activité ».
- Ces machines sont partagées et sans écran réel : les temps varient d'un run à
  l'autre (de 145 à 222 ms pour la première fenêtre sous Windows sur deux runs).

## Ce qui n'est pas encore mesuré

- Comparaison avec Chrome et Brave sur la même machine et les mêmes pages.
- Mémoire avec 10 ou 20 onglets, et effet de l'économiseur de mémoire.
- Fluidité de l'interface (images par seconde).
