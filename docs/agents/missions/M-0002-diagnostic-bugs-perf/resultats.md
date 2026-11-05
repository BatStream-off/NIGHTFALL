# M-0002 : résultats du diagnostic des bugs et des mesures de performance

- **Agent** : Optimiseur (Opus), 2026-11-04 / 2026-11-05.
- **Branche** : `agent/M-0002-diagnostic-bugs-perf` ; PR **brouillon** n° 25 (ne jamais fusionner).
- **Aucune correction** du navigateur : seulement de l'instrumentation, compilée uniquement
  avec la feature `selftest` et active seulement avec `NIGHTFALL_SELFTEST=diag`.

## 1. Preuves : runs et jobs de CI

| Tour | Commit | Run | Jobs utilisés |
|---|---|---|---|
| 1 | `e171f0d` | 41849659257 (tout vert) | Windows 126642245751, macOS 126642246729, Linux 126642245989 |
| 2 | `dbe503e` | 41985512387 (tout vert) | Windows 127105202980, macOS 127105203015, Linux 127105203164 |

Journaux : `gh api repos/BatStream-off/NIGHTFALL/actions/jobs/<job>/logs`. Les lignes
`DIAG …` viennent du journal de NIGHTFALL (étape « Diagnostic et mesures M-0002 »), les lignes
`MEM`, `DEMARRAGE`, `RESUME` de `ci/diag_runner.py`, les lignes `BOUTON`/`PAGE` de
`ci/ui_buttons_test.js` (job Linux). Sauf mention, les chiffres cités sont ceux du **tour 2** ;
le tour 1 donne les mêmes conclusions (macOS complet ; Windows interrompu à l'étape « boutons »
par un plantage de mon lanceur — sortie console cp1252 — corrigé au tour 2).

L'auto-test existant (`SELFTEST OK`) est resté vert sur Windows et macOS aux deux tours, avec
l'instrumentation compilée. `preserve.py origin/develop HEAD` : « aucune disparition non déclarée ».

## 2. Méthode

- **Navigateur réel** (`app/src/nf/diag.rs`, compilation **release** + `selftest`, comme une
  version publiée ; journal réseau et journal Chromium détaillé désactivés pendant la mesure) :
  - clics et touches injectés par le protocole DevTools interne (`Input.dispatchMouseEvent`,
    `Input.dispatchKeyEvent`), traités par la page comme des événements matériels ; le focus natif
    est d'abord donné à la vue cliquée, comme le fait un vrai clic ;
  - points d'accroche (sous `#[cfg(feature = "selftest")]`) : appels d'API reçus et leur durée
    sur le thread IO, événements poussés vers l'interface (nombre, octets), touches vues par
    `on_pre_key_event`, création/destruction des navigateurs, durée de chaque contrôle Shield et de
    la génération du script de page ;
  - mémoire et processus échantillonnés de l'extérieur (`psutil`, 3 mesures, médiane) ;
  - démarrage mesuré de l'extérieur (lancement → ligne « interface prête »), 5 profils neufs et
    6 lancements sur un profil existant (listes Shield en cache).
- **Pages internes** (`ci/ui_buttons_test.js`, Chromium sans affichage + maquette d'API
  `ci/ui_mock_server.py` enrichie de données d'exemple et d'une remise à zéro) : chaque élément
  activé seul dans une page rechargée ; on relève les commandes d'API, erreurs JS, dialogues,
  navigation et modification de la page.

Constaté = mesuré ou observé dans un journal cité. Supposé = déduit de la lecture du code,
signalé comme tel. Les références de code sont celles de `origin/develop` (non instrumenté).

## 3. Problèmes signalés

### 3.1 Fermeture incorrecte des onglets — REPRODUIT (Windows et macOS)

**Constaté.** Clic réel (appui puis relâchement) sur le bouton × d'un onglet, 5 essais × 3 durées
d'appui (40, 100, 160 ms) × onglet actif/inactif, par plateforme :

| Mode | Windows (fermés) | macOS (fermés) |
|---|---|---|
| Clic souris réel sur × (onglet actif) | **0/15** | **0/15** |
| Clic souris réel sur × (onglet inactif) | **0/15** | **0/15** |
| `element.click()` sur × (pas de `mousedown`) | 6/6 | 6/6 |
| Clic du milieu sur l'onglet | 6/6 | 6/6 |
| Menu contextuel « Fermer l'onglet » | fermé (ligne `DIAG bouton zone=tabmenu n=5`) | fermé |

Dans les 60 clics ratés, la seule commande reçue est `tab.activate` (2 à 5 ms après l'appui),
suivie d'un événement `tabs` poussé vers la barre 1 à 18 ms après l'appui, donc **avant** le
relâchement (lignes `DIAG fermeture …`, `RESUME fermeture …`). Pour un onglet inactif, l'onglet
actif change à chaque fois (`actif_conserve=0`). Reproduction identique sur la maquette
(Playwright local) : dès que la liste est reconstruite entre l'appui et le relâchement,
`tab.close` n'est jamais envoyé.

**Cause établie.**
1. `app/ui/shell.js:213-216` : le `mousedown` de l'onglet (y compris sur son ×) envoie
   `tab.activate` ; le × ne bloque que le `click` (`shell.js:211`).
2. `app/src/nf/windows.rs:798-848` : `activate_tab` pousse un événement `tabs` (`:847`) même si
   l'onglet est déjà actif.
3. `shell.js:197-199` : `renderTabs` vide et reconstruit toute la barre ; le bouton sous le
   pointeur est remplacé avant le `mouseup`, le `click` n'atteint jamais son gestionnaire.
4. Effet secondaire : si le clic aboutissait, fermer un onglet d'arrière-plan aurait d'abord
   activé cet onglet, puis `close_tab_inner` (`windows.rs:922,941`) activerait un voisin : l'onglet
   où l'on était serait perdu.

**Fermeture côté natif : saine (constaté).** Rafale de 6 onglets fermés par l'API : 6 navigateurs
détruits en 17 ms (Windows) / 73 ms (macOS, tour 1), 3 navigateurs vivants ensuite (barre,
vue latérale, onglet restant) ; `navigateur_detruit_ms` 4 à 11 ms par onglet.

**Correction proposée.** Dans `shell.js` : `mousedown` du × avec `stopPropagation()` (comme
`t-sound`, `shell.js:184`) ; ne pas reconstruire la barre à chaque événement (mise à jour par
onglet, clé `id`) ; côté natif, ne pas pousser `tabs` si l'onglet est déjà actif. Risque faible ;
à couvrir par le test `fermeture` de `diag.rs` (doit passer à 30/30).

### 3.2 Boutons défectueux — 4 causes constatées

Inventaire : **238 éléments** sur 30 écrans des pages internes (job Linux 127105203164,
`RESUME boutons elements=238 clique=215 desactive=4 masque=18 …`) + **53 éléments** de la barre et
des menus déroulants cliqués dans le vrai navigateur sur chaque plateforme (lignes `DIAG bouton`).

1. **Bouton × des onglets** : voir 3.1.
2. **Un menu déroulant en ferme un autre (macOS)** — constaté : menu ouvert puis clic sur Shield :
   Windows 5/5 Shield affiché, **macOS 0/5** (`menu_final=-`, commandes `popup.open,popup.close`) ;
   second clic sur Shield pour le fermer : Windows 5/5 fermé, **macOS 0/5 (il se rouvre)**.
   « Rechercher dans la page… » depuis le menu : la barre de recherche s'ouvre puis se referme
   aussitôt sur macOS (`api=find.open,popup.close effet=menu:menu>-`), reste ouverte sur Windows
   (`menu:menu>find`). Cause : quand un menu perd le focus il envoie `popup.close`
   (`shell.js:448`), et `popup.close` (`api.rs:246`) ferme **le menu ouvert à ce moment-là**, quel
   qu'il soit ; sur macOS cette fermeture arrive après l'ouverture du suivant. Correction : passer
   le nom du panneau à `popup.close` et l'ignorer s'il ne correspond pas. Risque faible.
3. **« Accueil » sans effet depuis une page web** — constaté sur les deux plateformes :
   `DIAG accueil_web … page_finale=example.com reussi=0`, bouton `#b-home` `effet=aucun`, et la
   ligne « navigation vers nightfall:// refusée depuis une page web ». Cause :
   `windows::navigate_active` (`windows.rs:1194-1211`) appelle `state::allow_internal_for_tab`
   (`state.rs:470`), qui n'est consommé qu'à la **création** d'un navigateur
   (`client.rs:261`) ; `on_before_browse` refuse donc la navigation (`client.rs:570-580`).
   Concerne aussi « Accueil » du menu et la page d'avertissement HTTP (`nav.home`). Supposé (non
   testé) : l'autorisation reste en attente et serait consommée si l'onglet est recréé (onglet mis
   en veille puis restauré, `windows.rs:812`), laissant une navigation interne autorisée à tort.
   Correction : autoriser la navigation sur le navigateur existant
   (`browsers()[id].allow_internal_nav = true`). Risque faible, toucher à la sécurité des pages
   internes : audit nécessaire.
4. **`confirm()` / `prompt()` sans gestionnaire** : 9 boutons en dépendent (historique « Tout
   effacer », favoris Nouveau dossier/Sous-dossier/Renommer/Supprimer, mots de passe
   Afficher/Supprimer). NIGHTFALL n'a pas de `JsDialogHandler` (`client.rs:42-93`). Constaté :
   `confirm()` reste en attente au moins 2,5 s (Windows et macOS, `DIAG dialogue confirm
   en_attente_apres_2500ms=1`), donc un dialogue est bien ouvert par CEF ; **non vérifié** : qu'il
   soit visible et utilisable par l'utilisateur (pas de capture prise). À contrôler à la main ou
   par capture avant toute conclusion.

Autres résultats (constatés) : tous les autres éléments cliqués dans le vrai navigateur envoient
la bonne commande et produisent l'effet attendu (barre : précédent, suivant, actualiser, favori,
Shield, mots de passe, menu, nouvel onglet, verrouiller ; menu : 15 entrées ; menu contextuel :
9/9 ; Shield 2/2 ; zoom 3/3 ; recherche 3/3, « précédent/suivant » envoyant `find.stop` car le
champ est vide). Panneau Mots de passe : « Générer » (`vault.generate`) et « Enregistrer »
(`vault.saveForPage`) vérifiés ; le 3ᵉ élément a été relu « Enregistrer » au lieu de « Gérer les
mots de passe » : **non vérifié** au clic réel (vérifié seulement par l'inventaire Playwright). Le clic droit sur un onglet
ouvre son menu (3/3 par plateforme). Dans les pages internes (maquette), aucune erreur JS
propre à NIGHTFALL ; les 9 éléments « sans effet visible » sont attendus (cases de sélection de
la purge, section déjà affichée, option déjà choisie, « Ajouter » avec champ vide). Les 5 erreurs
JS viennent de sites externes ouverts par des liens.

Non testé au clic réel : « Imprimer… », « Enregistrer la page », « Verrouillage d'urgence »
(dialogue système ou fin de session sur une machine de CI) ; les 5 dernières entrées du menu
(Security Center, Paramètres, Accueil, À propos, Verrouiller) étaient **hors de la zone visible**
du menu (`y=531…685`, `hauteur_vue=528/531` : écran de 768 px, défilement nécessaire), le clic
n'a donc rien touché ; leurs commandes (`page.open`, `lock`…) sont vérifiées par l'inventaire
Playwright et par le bouton cadenas de la barre.

### 3.3 Raccourcis clavier actifs quand NIGHTFALL est verrouillé (H1) — CONFIRMÉ

Écran de verrouillage affiché (`verrouille_avant=1`), touches envoyées à la vue de l'écran de
verrouillage ; identique sur Windows et macOS (lignes `DIAG h1`) :

| Raccourci | Action déclenchée | Effet constaté pendant le verrouillage |
|---|---|---|
| Ctrl/Cmd+T | tab.new | +1 onglet, onglet actif changé |
| Ctrl/Cmd+W | tab.close | −1 onglet |
| Ctrl+Tab, Ctrl/Cmd+2 | tab.next, tab.index.1 | onglet actif changé |
| Ctrl/Cmd+Maj+T | tab.reopen | onglet fermé rouvert |
| Ctrl/Cmd+R | tab.reload | rechargement (pas d'effet d'état mesurable) |
| Ctrl/Cmd+= | zoom.in | zoom 0 → 0,52 |
| Ctrl/Cmd+U | view.source | +1 onglet « source » de la page |
| Ctrl+H / Cmd+Y, Ctrl/Cmd+J, Ctrl/Cmd+Maj+O | page.history / downloads / bookmarks | +1 onglet chacun |
| F11 / Ctrl+Cmd+F | window.fullscreen | fenêtre en plein écran |
| Ctrl/Cmd+N, Ctrl/Cmd+Maj+N | window.new, window.private | +1 fenêtre chacun |
| Ctrl/Cmd+Maj+W (fenêtres créées) | window.close | −2 fenêtres |
| Ctrl/Cmd+F, Ctrl/Cmd+D, Ctrl/Cmd+L | find, bookmark.toggle, focus.address | action appelée, sans effet visible |
| F12 / Cmd+Opt+I | devtools | sans effet (outils désactivés par défaut) |

Le verrouillage reste en place (`verrouille_apres=1`) mais les onglets, fenêtres, zoom et plein
écran changent en arrière-plan, et le Session Vault scelle un état qui n'est plus celui du
verrouillage. Second chemin d'injection (`CefBrowserHost::SendKeyEvent`) : Windows,
`gestionnaire_appels=1 action=tab.new` (même faille) ; macOS, `gestionnaire_appels=0` (ce chemin
d'injection n'atteint pas `on_pre_key_event` sur macOS ; le résultat par DevTools reste valable).
Cause : `on_pre_key_event` (`client.rs:731-822`) puis `api::shortcut` (`api.rs:1175`) n'ont aucun
contrôle de verrouillage, alors que `api::handle` en a un (`api.rs:48`). Correction : refuser
toute action (sauf éventuellement `emergency.lockdown`) quand `!core().is_unlocked()`, dans
`shortcut()` ; risque faible ; le test `h1` de `diag.rs` sert de test de non-régression.

### 3.4 Chargement lent, ralentissements, manque de fluidité — cause principale établie

**Constaté : le thread IO de CEF est bloqué par `page.info`.**

| | Windows | macOS (tour 1) | macOS (tour 2) |
|---|---|---|---|
| appels `page.info` | 2 555 | 2 504 | 2 787 |
| temps cumulé sur le thread IO | 3,2 s | 379,8 s | 340,6 s |
| moyenne / max | 1,2 ms / 250 ms | 152 ms / 2 158 ms | 122 ms / 2 157 ms |

(lignes `DIAG api_cout commande=page.info`). Cause : à **chaque** événement `tabs` la barre appelle
`refreshPageInfo()` (`shell.js:790`, `:343`) ; `page_info` (`api.rs:1062`) appelle
`windows_active_url` (`api.rs:565-571`) qui, depuis le thread IO (l'API est servie par le
gestionnaire de schéma, `scheme.rs:126`), **attend le thread UI** jusqu'à 2 s
(`recv_timeout`). Pendant ce temps aucune requête réseau, aucun contrôle Shield, aucun autre appel
d'API n'est traité. Le maximum de 2,157 s = délai de 2 s atteint. Même schéma bloquant :
`downloads.list` (`api.rs:397-405`, max 2,1 s au tour 1 sur macOS, la page Téléchargements
l'appelle chaque seconde) et `settings.set` (`api.rs:986`, jusqu'à 3 s).

**Constaté : volume d'événements `tabs`.** 2 544 (Windows) / 2 768 (macOS) envois de la liste
complète des onglets en ≈ 20 min, ≈ 4 Mo de JavaScript exécuté dans les vues d'interface ; 6 à 22
envois par chargement de page (colonne « push » ci-dessous) ; chacun reconstruit toute la barre et
déclenche un `page.info`.

**Constaté : les protections ne sont pas la cause.** Temps passé dans Shield par page (thread IO) :
0,1 à 6 ms (Windows), jusqu'à 25 ms (bbc, macOS), pour des pages de 90 à 3 000 ms. Sur Windows
(mesures stables), défaut / sans Shield / sans HTTPS d'abord / DNS désactivé / DoH Quad9 / tout
désactivé sont dans la même plage pour chaque page ; sur macOS la dispersion de la machine de CI
(q1–q3 jusqu'à ×2) masque toute différence. Coût d'HTTPS d'abord sur `http://example.com` : non
mesurable (92 ms défaut contre 89 ms sans, Windows). Reconstruction du moteur Shield (164 145
règles) : 98–101 ms (Windows), 168–237 ms (macOS), sur le thread appelant.

Temps de chargement, médiane (q1–q3) en ms sur 5 essais, cache HTTP vidé avant chaque essai
(`total` = navigation demandée → `on_load_end`), colonnes Shield = temps Shield (ms) et
requêtes bloquées, médianes :

| Page | Config | Windows total | macOS total | Shield ms W / M | bloquées W / M | push `tabs` W / M |
|---|---|---|---|---|---|---|
| example (http) | défaut | 92 (84–101) | 223 (190–382) | 0,10 / 0,46 | 0 / 0 | 6 / 6 |
| example (http) | sans HTTPS d'abord | 89 (80–91) | 234 (206–244) | 0,08 / 1,14 | 0 / 0 | 6 / 6 |
| example (http) | tout désactivé | 94 (90–117) | 261 (163–297) | 0 / 0 | 0 / 0 | 6 / 6 |
| wikipedia | défaut | 240 (213–370) | 636 (431–2194) | 2,1 / 9 | 1 / 1 | 7 / 7 |
| wikipedia | sans Shield | 235 (208–294) | 506 (455–528) | 0,01 / 0,03 | 0 / 0 | 7 / 7 |
| wikipedia | DoH Quad9 | 226 (212–256) | 754 (568–918) | 2,0 / 10 | 1 / 1 | 7 / 7 |
| wikipedia | tout désactivé | 215 (213–228) | 454 (430–520) | 0,01 / 0,03 | 0 / 0 | 7 / 7 |
| lemonde | défaut | 538 (517–755) | 1171 (941–1912) | 2,4 / 6 | 3 / 3 | 7 / 7 |
| lemonde | sans Shield | 539 (508–641) | 1054 (952–1351) | 0,02 / 0,04 | 0 / 0 | 7 / 7 |
| lemonde | DNS désactivé | 538 (520–571) | 1030 (854–1203) | 2,5 / 9 | 3 / 3 | 7 / 7 |
| lemonde | tout désactivé | 511 (493–574) | 853 (711–1878) | 0,02 / 0,04 | 0 / 0 | 7 / 7 |
| bbc | défaut | 766 (734–785) | 3261 (2242–5226) | 5,8 / 23 | 3 / 20 | 9 / 15 |
| bbc | sans Shield | 917 (658–1058) | 4254 (2318–5608) | 0,03 / 0,17 | 0 / 0 | 9 / 22 |
| bbc | sans HTTPS d'abord | 744 (420–764) | 2539 (2088–3462) | 5,4 / 20 | 3 / 20 | 12 / 18 |
| bbc | tout désactivé | 1010 (960–1047) | 3301 (1870–3760) | 0,03 / 0,17 | 0 / 0 | 9 / 22 |

Tableau complet (6 configurations × 4 pages × 2 plateformes, FCP, LCP, TTFB) : lignes
`RESUME nav` des deux jobs. Limites : réseau de centre de données, connexions et DNS réutilisés
d'un essai à l'autre (seul le cache HTTP est vidé) ; sur macOS les blocages `page.info` se
superposent aux chargements (non séparés dans cette mission).

**Corrections proposées (non réalisées)** : (1) `page.info` sans aller-retour bloquant vers le
thread UI (URL active tenue dans un état partagé, ou réponse calculée sur le thread UI et poussée
avec l'événement `tabs`) — gain estimé : jusqu'à 120–150 ms de blocage du thread IO par événement
sur macOS, risque faible ; (2) même traitement pour `downloads.list` et `settings.set` ;
(3) ne plus appeler `refreshPageInfo` à chaque `tabs`, et mettre la barre à jour par différence ;
(4) regrouper les `update_tab` d'un même chargement (`windows.rs:1215-1227`). À mesurer après avec
le même scénario (branche à conserver).

### 3.5 Problèmes de mémoire — fuite constatée

**Constaté : chaque menu déroulant fermé laisse son navigateur et son processus de rendu.**

| Étape (tour 2) | Windows : navigateurs d'interface vivants / processus de rendu / RSS / privé | macOS : navigateurs d'interface vivants / processus de rendu / RSS |
|---|---|---|
| 1 onglet, après les tests de menus | 76 / 80 / 1 916 Mo / 2 147 Mo | 81 / 89 / 9 227 Mo |
| après 20 ouvertures-fermetures de menu | **96 / 100** / 3 009 Mo / 2 623 Mo | **101 / 107** / 10 744 Mo |

Exactement +1 navigateur d'interface et +1 processus de rendu par menu (lignes `DIAG
menus_ouverts_fermes`, `MEM … par_type`). Bilan final : 187 navigateurs créés, 89 détruits
(Windows) ; 191 / 88 (macOS). Les onglets, eux, sont bien libérés (`vivants_onglets` revient à 1
après fermeture de 20 onglets). Au démarrage (lock screen) : 7 processus, 356 Mo RSS / 126 Mo USS
(Windows), 759 Mo RSS (macOS).

Cause (établie par la mesure, mécanisme supposé) : `popup_close` (`windows.rs:1412-1426`) détruit
l'overlay **avant** `close_browser(1)` ; d'après la documentation de CEF, `DoClose` n'est plus
appelé quand la vue hôte est déjà détruite, et le navigateur n'est jamais fermé
(`on_before_close` jamais reçu). Correction proposée : fermer le navigateur d'abord, détruire
l'overlay dans `on_before_close` (ou attendre sa fermeture) ; risque moyen (cycle de vie CEF,
code délicat) ; critère : `vivants_ui` stable après 20 menus.

Mémoire par onglet (Windows, USS) : 1 → 10 → 20 onglets Wikipédia : 920 → 1 977 → 3 024 Mo
(≈ 105 Mo par onglet), retour à 903 Mo après fermeture (pas de fuite côté onglets).

**Autre point mesuré.** Vue latérale (onglets verticaux) créée même quand elle est désactivée
(`windows.rs:420`) : `vivants_laterale=1` en permanence. Coût non isolé.

### 3.6 Incohérences d'interaction (constatées en passant)

- Accès réentrants refusés (macOS, 16 et 18 occurrences par tour) à l'ouverture d'une page interne
  dans un nouvel onglet : `on_title_change` (`client.rs:186,196`) appelle `tab_url` /
  `active_tab_id` pendant que `add_tab_at` tient `WINDOWS` ; le titre n'est alors ni enregistré
  dans l'historique ni reporté sur la fenêtre. 0 occurrence sous Windows.
- Menu principal plus haut que la place disponible sur un écran de 768 px : 5 entrées et
  « Verrouillage d'urgence » demandent un défilement.

## 4. Performances de référence (critère 4)

| Mesure | Windows | macOS | Répétitions |
|---|---|---|---|
| Démarrage jusqu'à l'interface, profil neuf | 263 ms (258–298) | 2 174 ms (1 652–2 815) | 5 |
| Démarrage, profil existant (listes Shield chargées) | 354 ms (350–384) | 1 944 ms (1 600–3 106) ; tour 1 : 686 (666–810) | 6 |
| dont construction de Shield (164 145 règles) | 105 ms (103–110) | 210 ms (162–291) ; tour 1 : 90 | 6 |
| Ouverture du menu : clic → contenu affiché | 138 ms (137–142) | 392 ms (366–443) | 10 |
| Nouvel onglet : clic → page chargée | 150 ms (149–152) | 1 466 ms (349–2 550) | 10 |
| Changement d'onglet : clic → `tab.activate` / → liste poussée | 2 / 5 ms | 8 / 10 ms | 10 |
| Déverrouillage (Argon2id + ouverture des bases) | 200 ms | 481 ms (tour 1 : 310) | 1 |
| Mémoire au démarrage (7 processus) | 356 Mo RSS, 126 Mo USS | 759 Mo RSS | 5 |

Plages = q1–q3. La machine macOS de CI est nettement plus lente et plus dispersée d'un tour à
l'autre ; ses valeurs absolues sont à comparer avant/après **sur la même exécution**.

## 5. Causes écartées (mesurées)

- Contrôle Shield par requête : ≤ 6 ms par page (Windows), ≤ 25 ms (macOS, bbc), négligeable.
- Script de page Shield / scripts injectés : inclus dans les mesures `script_page_*` des lignes
  `DIAG nav`, sans écart de temps de chargement mesurable avec Shield désactivé (Windows).
- HTTPS d'abord, DNS sécurisé (automatique, désactivé, Quad9 strict) : aucune différence
  mesurable sur les médianes Windows.
- Fermeture native des onglets : rapide et complète (navigateurs détruits en < 80 ms).

## 6. Instrumentation (critère 5)

Fichiers : `app/src/nf/diag.rs` (nouveau, `#[cfg(feature = "selftest")]` dans `mod.rs`), points
d'accroche sous `#[cfg(feature = "selftest")]` dans `client.rs`, `windows.rs`, `api.rs`,
`scheme.rs`, `mod.rs`, `app.rs`, `selftest.rs` ; `ci/diag_runner.py`, `ci/ui_buttons_test.js`,
`ci/ui_mock_server.py` (données d'exemple, `__reset`) ; `ci.yml` : deux étapes
`continue-on-error`. Rien n'est compilé sans la feature `selftest` ; le job « Contrôles de
sécurité » (feature jamais par défaut, `NIGHTFALL_FEATURES` réservé à `selftest`) est vert.
Changements de comportement **dans les builds selftest seulement** : avec `NIGHTFALL_SELFTEST=diag*`,
pas de journal réseau et journal Chromium au niveau WARNING (comme une version publiée).

La branche reste non fusionnée : le Directeur peut relancer exactement les mêmes mesures « après »
en rebasant les corrections dessus (le scénario se lance seul dans la CI).

## 7. Limites

- Pas de capture d'écran du dialogue `confirm()` : sa visibilité n'est pas prouvée.
- Les clics passent par DevTools (pas par le système) ; le focus natif est simulé par
  `request_focus`. La reproduction du bug × est la même sur la maquette Playwright avec de vrais
  événements souris, ce qui appuie le diagnostic.
- Réseau de CI ; un seul jeu de pages (example.com, Wikipédia, Le Monde, BBC).
- La part exacte des blocages `page.info` dans le ressenti « pages lentes » n'est pas isolée :
  il faudra la mesure « après » correction.
