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

## Lecteur NIGHTFALL (sites vidéo protégés), version 0.1.0

Relevé par l'auto-test du lecteur dans le paquet signé (workflow « Lecteur
NIGHTFALL », run 42377494926 du 07/11/2026, commit `9fab433`, branche
`poc-ecs-evs-macos`), pendant la lecture d'une vidéo Widevine de démonstration
(Shaka Player, 960×600). Le lecteur n'est pas inclus dans l'installateur : ces
chiffres ne s'ajoutent que pour qui l'accepte, et sa mémoire seulement pendant
qu'il est ouvert.

| Mesure | Windows (windows-latest) | macOS (macos-latest, arm64) |
|---|---|---|
| Téléchargement du lecteur (zip) | 146 Mio | 114 Mio |
| Lecteur installé sur le disque | 358 Mio | 258 Mio |
| Installateur de NIGHTFALL 0.3.1, pour comparaison | 137 Mio (setup.exe) | 164 Mio (dmg) |
| Mémoire du lecteur pendant la lecture (ensemble de travail, tous processus) | 562 Mio | 620 Mio |
| Processus du lecteur | 6 | 6 |
| NIGHTFALL seul, 2 onglets (tableau ci-dessus, run du 06/11) | 595 Mio, 11 processus | 1 450 Mio, 12 processus |

Avec le lecteur ouvert, la mémoire totale est à peu près celle de NIGHTFALL plus
celle du lecteur (deux applications séparées, rien de partagé). Mesures faites
sur des runs différents : ce sont des ordres de grandeur, à confirmer sur une
même machine. La mémoire du lecteur vient d'`app.getAppMetrics()`
(workingSetSize), celle de NIGHTFALL de `nf_perf::process_tree_memory` (mémoire
résidente) : définitions proches, pas identiques.

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
