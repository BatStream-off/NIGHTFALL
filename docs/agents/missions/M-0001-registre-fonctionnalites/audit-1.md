# Audit M-0001 (tentative 1)

(Audit archivé par le Directeur ; seule la mise en forme a été retouchée.)

- **Auditeur** : claude-opus-5-5 (agent neuf, rôle `nf-auditeur`, mode de repli)
- **Objet audité** : branche `agent/M-0001-registre-fonctionnalites`, commit 76ed70a (`docs/agents/FONCTIONNALITES.md` seul), PR #21 (base `main`, fusionnée par le propriétaire avant l'audit), run CI 41821544471
- **Verdict** : **REFUSÉ** (3 MAJEUR, 0 BLOQUANT)

Registre sérieux et en grande partie exact, mais trois points empêchent de s'en servir
comme référence : une protection anti-phishing annoncée mais jamais déclenchée (F-137),
une protection Shield manquante avec un nombre de listes faux (F-147), et une
« Couverture » qui compte comme prouvées des lignes dont la preuve ne teste pas la
fonctionnalité.

## Vérifications faites par l'Auditeur
Scripts : `/tmp/audit-M-0001/check.py`, `where.py`, `cov.py` (machine de la session).

| Critère | Vérification | Conforme |
|---|---|---|
| 1. Une ligne par fonctionnalité, ID uniques, par domaine | 187 lignes F-001→F-187 sans trou ni doublon, 6 colonnes, tous les domaines exigés. Inventaire indépendant : `api.rs` (137 commandes), `shell.js/html`, `settings.js` (13 sections), 9 pages internes, `on_pre_key_event`, 14 crates, `STATUS.md`, `RELEASE_NOTES.md` | Non (M2, m6) |
| 2. « Où » exacts | 540/540 citations existent (311 définitions, 129 commandes/chaînes, 27 ID HTML) ; échantillon > 60 lignes lu : correct sauf 8 imprécisions (m8) | Oui, réserves mineures |
| 3. Preuves exactes | 120/120 tests existent avec `#[test]`, 20/20 `step("…")`, contrôles CI présents ; échantillon > 40 lignes : 7 preuves ne testent pas la fonctionnalité (M3) | Non (M3) |
| 4. Raccourcis | 44 branches de `client.rs:762-818` toutes présentes ; restrictions de plateforme absentes (m7) ; raccourcis propres aux pages absents (m6) | Oui |
| 5. Limites connues | 20 entrées ; L-07…L-10, L-12 exactes ; L-11 incomplète (M1) ; L-16 sous-estimée (m5) | Partiel |
| 6. Couverture | 187/116/71 arithmétiquement exacts, mais 116 inclut des preuves hors sujet (M3) | Non |
| 7. preserve / scope | `preserve.py` : aucune disparition (aussi vs fa9a46d et origin/develop) ; `scope` avec la base de la fiche : périmètre respecté (avec 1b70417 : 3 fichiers du commit Directeur 6f03f95, normal) | Oui |

## Contrôles systématiques
- [x] Diff relu ; aucun fichier hors périmètre attribuable à l'Analyste ; code de l'application identique à la 0.1.4.
- [x] Aucune fonctionnalité perdue.
- [x] CI verte sur 76ed70a (Modules ×3, Application Windows et macOS avec auto-test, Sécurité, Préservation).
- [x] Captures : sans objet. Sécurité du livrable : document seul ; risque applicatif hors périmètre (H1).
- [ ] Pas de fonctionnalité annoncée sans réalité : non conforme (M1, M3, m1).
- [x] Textes en français.

## Problèmes trouvés

| Gravité | Ligne | Problème | Correction attendue | Preuve |
|---|---|---|---|---|
| MAJEUR M1 | F-137 (:194), L-11 (:315) | F-137 annonce des motifs « adresse IP » et « connexion en HTTP » que l'application ne produit jamais : `phishing_check` appelle `check(url, false)` et ces motifs exigent `has_password_field = true`. Le test cité `ip_and_http_login` passe `true`. | Retirer ces motifs de F-137 ; compléter L-11 (IP et HTTP jamais déclenchés depuis la navigation) | `app/src/nf/security.rs:40` ; `crates/nf-security/src/phishing.rs:184-191, 227, 290-295` |
| MAJEUR M2 | F-147 (:209) | « Six listes » : `default_lists()` en définit 9 ; manquent « uBlock – Réparations », « uBlock – Sites malveillants » (activées par défaut, la seconde est une protection de sécurité) et « Fanboy Annoyances » (désactivée par défaut). | 9 listes nommées avec leur état par défaut | `crates/nf-shield/src/lib.rs:76-86` |
| MAJEUR M3 | :66, :104, :121, :151, :197, :223, :264 ; Couverture :328-349 | Preuves qui ne testent pas la fonctionnalité, comptées comme prouvées : F-109 (`unique_paths` = nommage unique), F-156 (valeur par défaut du réglage seulement), F-072 (rendu de la barre déverrouillée), F-084 (le serveur de test n'envoie pas de CSP), F-140 (verrouillage simple, pas l'Emergency Lockdown), F-044 (étape informative qui ne peut pas échouer), F-172 (préférences communes, pas le contexte privé). | « aucune » ou une vraie preuve, puis recalculer la Couverture | `cov.py` |
| MINEUR m1 | F-178 (:280) | `tab.discard` n'est appelé par aucune page ni menu | Retirer « commande manuelle » ou le mettre en Limites | `grep tab.discard app/ui` vide |
| MINEUR m2 | F-161 (:233) | Seule l'adresse affichée est expurgée ; la base garde `item.url()` complet | Préciser | `app/src/nf/downloads.rs:46, 172-185` |
| MINEUR m3 | F-104 (:146) | Option par défaut « Automatique (fournisseur du système) » oubliée | Ajouter | `app/ui/settings.js:15` ; `state.rs:177` |
| MINEUR m4 | F-084 (:121) | « Sans accès réseau » inexact : CSP `img-src https: http:` | Corriger | `app/src/nf/scheme.rs:18` |
| MINEUR m5 | L-16 (:320) | Raccourcis actifs aussi sur l'écran de verrouillage (`keyboard_handler` fourni à toutes les vues, `shortcut()` sans contrôle de verrouillage) : Ctrl+P, F12/Ctrl+Maj+I si outils activés, titre de fenêtre après Ctrl+Tab | Décrire les effets (voir H1) | `client.rs:64, 820-824` ; `api.rs:1176` |
| MINEUR m6 | — | Absents : interrupteur général VideoGuard et ses bandeaux « Segment ignoré… / Revenir », « … détecté / Passer » (`security.js`, `videoguard.js:74-79`) ; « Non appliqué : … » des préférences refusées (`markPrefs`) ; deux réglages de mise à jour distincts et badge « Mise à jour signée » ; bouton « Quitter ce site » du bandeau anti-phishing ; désinstalleur Windows ; boîtes de dialogue Chromium en français (`app.rs:17`) ; raccourcis de page : Échap (recherche des Paramètres, menus, fenêtres de la page Nouvel onglet), ↑ ↓ Début Fin dans la barre latérale des Paramètres | Ajouter | fichiers cités |
| MINEUR m7 | :35-44, :54, :81, :157 | Plateformes non indiquées : Windows seul F11, F12, F6, Alt+D, Ctrl+F4, Alt+←/→, Ctrl+H ; macOS seul raccourcis Option et Ctrl+Cmd+F | Indiquer | `client.rs:762-818` |
| MINEUR m8 | F-048, F-057, F-164, F-042, F-156, F-153, F-185, F-120 | Citations imprécises : F-048 → `on_before_context_menu` (pas `strip_separators`) ; F-057/F-164 : l'étoile passe par `shortcut` → `bookmark.toggle` ; F-042 `page.viewSource` sans appelant ; F-156 → `videoguard.js` ; F-153 `security.js:requêtes et` ; F-185 `preserve.py:git` ; F-120 `windows_impl.rs:Backend` | Préciser | `where.py` |
| MINEUR m9 | — | Doublons F-112/F-179, F-115/F-186, F-130/F-143/F-187, F-113/F-178, F-116/F-127, F-050/F-152, F-044/F-174 ; non visibles par l'utilisateur F-145, F-183, F-184 ; F-185 vient de la PR #19. **Ne pas supprimer** (règle 7, `preserve.py` suit les ID) : renvoi « voir F-xxx » | Renvois | `preserve.py:97-100` |
| MINEUR m10 | :95, :97, :101, :104, :110, :127, :169, :241 | « (rendu non vide seulement) » répété (triplé en F-122) ; redondance F-164 | Nettoyer | lecture |
| MINEUR m11 | F-141, F-076, F-150, F-100, F-170, F-180 | Preuves existantes non citées : étape « déverrouillage et coffre » (F-141, F-076) ; « historique et Shield » (F-150) ; vérification GPC de l'auto-test (F-100). Précisions : F-170 instantané seulement en mode « Reprendre » ; F-180 installation dans `%LOCALAPPDATA%\Programs` | Compléter | `selftest.rs` ; `session.rs:184` ; `nightfall.iss:20` |
| MINEUR m12 | — | La page Téléchargements affiche toujours « Les fichiers sont enregistrés dans votre dossier Téléchargements », même si un autre dossier est choisi | Ajouter aux Limites | `app/ui/downloads.js` (`#dir`) |

## Hors périmètre (pour le Directeur)
- **H1 (sécurité, règle 6)** : d'après le code, les raccourcis fonctionnent quand NIGHTFALL est verrouillé (Ctrl+P imprime la page masquée, F12 / Ctrl+Maj+I ouvrent les outils de développement s'ils sont activés, Ctrl+Tab / Ctrl+Maj+T affichent le titre d'un onglet dans la barre de titre, Ctrl+W ferme des onglets). Contournement probable de la confidentialité de LOCK, non exécuté ici : à confirmer par un test et corriger dans une mission dédiée.
- **H2 (processus)** : la PR #21 visait `main` ; fusionnée à 11:31:29Z alors que la CI (démarrée 11:31:15Z) n'a fini (verte) qu'à 11:39:07Z.
- **H3** : la base 1b70417 fournie pour `scope` précède le commit Directeur 6f03f95 ; la base de la fiche donne le bon résultat.

## Confrontation
Aucun rapport transmis. Message de commit : « 187 fonctionnalités » et « tous les raccourcis » confirmés ; « citations vérifiées » exact pour l'existence, contredit sur la pertinence de 7 preuves (M3) et sur deux faits (M1, M2).

L'Auditeur n'a rien modifié (`git status` vide, aucune écriture d'API).
