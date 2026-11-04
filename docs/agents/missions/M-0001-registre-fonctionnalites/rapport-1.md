# Rapport M-0001

(Rapport de l'Analyste, archivé par le Directeur ; seule la mise en forme a été retouchée, le contenu est celui rendu par l'agent.)

- **Agent** : Analyste — modèle : sonnet (Sonnet 5.5)
- **Branche / PR / run CI** : `agent/M-0001-registre-fonctionnalites` (commit `76ed70a`, poussée vers `origin`) / aucune PR (demandé) / CI non lancée
- **Résultat** : TERMINÉ

## Ce qui a été fait
- Rédaction complète de `docs/agents/FONCTIONNALITES.md` : 187 fonctionnalités (F-001 à F-187) réparties en 20 domaines (ceux de la fiche + « Profil et stockage » et « Outils de développement »), chaque ligne avec « Où » (fichier:symbole), preuve automatique ou « aucune », et vérification manuelle.
- Génération contrôlée par un script qui refusait toute citation non vérifiée : chaque symbole cité existe dans son fichier, chaque test cité est l'un des 120 `#[test]`, chaque étape d'auto-test existe dans `selftest.rs`, chaque étape de CI existe dans `ci.yml`.
- Toutes les combinaisons de touches de `on_pre_key_event` sont présentes sous la forme ``raccourci: `...` `` dans la ligne de la fonctionnalité concernée.
- Preuves partielles signalées explicitement dans la cellule (« le raccourci lui-même n'est pas testé », « informatif seulement » pour le presse-papiers, etc.).
- Section « Limites connues » (L-01 à L-20) séparée, section « Couverture » avec chiffres et liste des fonctionnalités sans preuve.

## Preuves
| Critère | Vérification | Preuve | Statut |
|---|---|---|---|
| 1. Lignes F-xxx uniques, groupées par domaine | Script sur le fichier final : 187 lignes, 187 ID distincts, 7 séparateurs `\|` par ligne | sortie « 187 lignes, ids uniques » | OK |
| 2. Citations « Où » exactes | Script : chaque symbole cité recherché (mot entier) dans chaque fichier cité | génération sans erreur | OK (voir risques) |
| 3. Preuves réelles ou « aucune » | Script : tests cités ⊂ liste des `#[test]`, étapes ⊂ `step("…")` de selftest.rs | 120/120 tests et 20/20 étapes cités au moins une fois | OK |
| 4. Raccourcis ``raccourci: `...` `` | Comparaison avec `on_pre_key_event` | toutes les touches (Windows et macOS) présentes | OK |
| 5. Section « Limites connues » distincte | Lecture | L-01 à L-20 | OK |
| 6. Section « Couverture » | Lecture | 187 fonctionnalités, 116 avec preuve, 71 sans (liste par domaine) | OK |
| 7a. `preserve.py origin/develop HEAD` | Lancé dans `/home/claude/wt-M-0001` | « aucune disparition non déclarée » | OK |
| 7b. `mission.py scope M-0001 --base origin/orga/multi-agents --ref HEAD` | Lancé après commit | « 1 fichier(s) modifié(s) … périmètre respecté » | OK |
| Poussée | `git push -u origin agent/M-0001-registre-fonctionnalites` | nouvelle branche, pas de push forcé | OK |

## Nouveautés d'interface technique
Aucune (seul `docs/agents/FONCTIONNALITES.md` modifié ; aucune ligne de code de l'application).

## Risques et limites connus
- 28 lignes avec preuve ne sont couvertes que partiellement (mention dans la cellule) ; beaucoup de raccourcis, menus et réglages n'ont aucun test dédié (71 sans preuve).
- Les contrôles de rendu (`ci/ui_render_test.js`) ne vérifient que « page non vide » ; cités avec cette mention.
- La vérification des « Où » par le script cherche le symbole comme mot dans le fichier : pour un mot courant (`check`, `search`), elle peut valider une citation peu précise.
- Les limites L-01 à L-17 sont des constats de lecture, pas des tests exécutés.
- Attribution du commit : l'agent a suivi la fiche (« Co-Authored-By: Claude Opus 5.5 ») alors que le modèle utilisé était Sonnet ; à harmoniser.

## Hors périmètre (constats à traiter dans d'autres missions)
1. `shortcut()` n'exige pas que NIGHTFALL soit déverrouillé : à confirmer par un test.
2. Les signaux d'Auto-Purge `BinarySignatureInvalid` et `InjectedModule` ne sont jamais produits, alors que l'interface les cite.
3. `PerfConfig.battery_saver` n'a ni interface ni effet.
4. Mise en veille des onglets : `editing_form` toujours faux.
5. `password_reused_elsewhere` du coffre n'est pas utilisé.
6. Anti-phishing : `check(url, false)`, donc le signal « connexion en HTTP » ne peut pas se déclencher.
7. Synchronisation : favoris seulement, sans dossiers (`folder: vec![]`), et les suppressions locales ne sont pas transmises.
8. Bouton « Installer / gérer » de la page Extensions : ouvre `chrome://extensions/`, qui ne fonctionne pas en Alloy.
9. « Enregistrer la page » ne télécharge que le HTML.
10. Pas de demande « Quitter la page ? ».

## Ce qui n'a pas été fait et pourquoi
- CI et application non lancées : le registre est établi par lecture du code ; les preuves citées sont celles qui existent, pas des exécutions.
- Pas de PR (demandé). Le script de génération est resté hors du dépôt (`/tmp/nfreg`).
