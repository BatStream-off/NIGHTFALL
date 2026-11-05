# M-0005 : conception de l'interface, de l'identité et du système de thèmes

Architecte (Opus). Branche `agent/M-0005-conception-interface-themes`, PR brouillon #27
(ne pas fusionner). Phase d'audit : rien n'est intégré ; le prototype de barre de titre
reste sur la branche.

Sommaire : 1. Audit de l'existant · 2. Identité et système de design · 3. Header et
barre de titre intégrée (prototype, résultats CI) · 4. Onglets, menus, panneaux,
animations · 5. Moteur de thèmes et éditeur · 6. Découpage en missions · 7. Risques
et questions pour le propriétaire · Annexe : fichiers.

---

## 1. Audit de l'interface actuelle

### Méthode
- Rendu local de **toutes** les pages et vues d'interface de `HEAD~1` (avant prototype)
  dans Chromium sans affichage, avec une API simulée enrichie
  (`outils/audit_mock.py`, qui réutilise `ci/ui_mock_server.py`) et
  `outils/audit_render.js` : 24 captures dans `captures/audit/`.
- Relevé automatique des styles **calculés** sur chaque élément visible (tailles de
  police, rayons, hauteurs de contrôles, tailles d'icônes, couleurs, durées) :
  `captures/audit/releve-styles.json`.
- Captures réelles de la CI (Windows, macOS) : `captures/ci/`.
- Limite : sous Linux, « Segoe UI » est remplacée par la police de secours ; les
  métriques (tailles, rayons, hauteurs) ne dépendent pas de la police.

### Constat chiffré (toutes vues confondues)

| Mesure | Valeurs distinctes relevées | Cible proposée |
|---|---|---|
| Tailles de police | **18** (10, 11,5, 12, 12,5, 13, 13,5, 14, 14,5, 15, 17, 18, 19, 20, 22, 24, 26, 28, 40 px) | 8 (11, 12, 13, 14, 16, 20, 28, 40) |
| Rayons d'arrondi | **16** (3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 16, 18, 20, 27, 99 px, 50 %) + coins biseautés (`clip-path`) | 5 + pilule (4, 6, 8, 12, 16) |
| Hauteurs de boutons | **11** (20, 22, 28, 30, 32, 34, 38, 40, 44, 50, 106 px) | 5 (24, 28, 32, 36, 40) |
| Hauteurs de champs | **8** (18, 22, 32, 34, 38, 40, 48, 54 px) | mêmes 5 hauteurs |
| Tailles d'icônes | **10** (10, 13, 14, 16, 17, 18, 19, 20, 26, 30 px) | 3 (14, 16, 20) |
| Couleurs de fond | 18 ; couleurs de texte : 16 | jetons sémantiques (13) |
| Durées de transition | 8 (0,1 s à 0,6 s), aucune courbe commune | 4 durées, 3 courbes |
| Couleurs codées en dur | `shell.css` 44 hex + 19 rgba ; `nf.css` 22 + 13 ; `newtab.html` 17 ; `newtab.js` 16 | 0 hors des fichiers de thème |

Ces valeurs codées en dur sont aussi l'obstacle principal au moteur de thèmes : un
thème qui ne change que les variables actuelles laisserait plus de 130 couleurs fixes.

### Constat page par page

| Vue (capture) | Incohérences constatées |
|---|---|
| Barre du haut (`barre-haut.png`, `barre-haut-zoom2x.png`, `fenetre-complete.png`) | 88 px pour 2 bandes (38 + 50) ; onglet actif « collé » à la barre d'outils avec pieds arrondis (style 2012-2020) ; 5 tailles d'icônes dans la même barre (14 à 20 px) ; 7 rayons ; titre de la fenêtre système en plus de la bande d'onglets (barre de titre Windows séparée, F-001) ; séparateurs verticaux de 1 px entre onglets inactifs ; Shield (chiffre en Chakra Petch) et étoile alignés différemment. |
| Onglets verticaux (`onglets-verticaux.png`) | En-tête de 52 px ≠ 38 px de la bande horizontale ; titre « Onglets » en Chakra Petch 14 px alors que les onglets sont en 13 px système ; indicateur actif 2 px collé au bord. |
| Menus (`popup-menu.png`, `popup-menu-onglet.png`) | 20 entrées au même niveau dans le menu principal (F-063), libellés longs tronqués (« Fenêtre de navigation pri… ») ; boutons de zoom 30 px dans des lignes de 32 px ; rayon 12 px dessiné dans une vue rectangulaire opaque (les coins prennent la couleur de fond de la vue, voir 4.3). |
| Panneau Shield (`popup-shield.png`) | Grand chiffre 40 px Chakra Petch dans un encart noir, rayon 10 ; interrupteur 40 × 22 différent de celui des pages (même taille mais autre classe `.toggle` dupliquée) ; texte explicatif sur 2 lignes. |
| Verrouillage (`verrouillage-creation.png`, `verrouillage-code.png`, `captures/ci/macos-demarrage-verrouillage.png`) | Bouton principal **biseauté** (`clip-path`) à côté de champs arrondis à 8 px ; pavé numérique biseauté 50 px ; trois hauteurs de contrôles (44, 48, 50). Identité forte mais isolée : aucun autre écran n'emploie le biseau sauf `button.primary`. |
| Nouvel onglet (`page-nouvel-onglet.png`) | Rayons 12, 14, 16, 18, 27 px ; 7 tailles de police ; tuiles à initiale colorée sur fond translucide ; horloge en Chakra Petch 26 px en haut à gauche, statistiques en bas : la composition n'a pas d'axe ; durées 0,4 s et 0,6 s isolées. |
| Paramètres (`page-parametres.png`) | Seule page avec une navigation latérale et des cartes (`settings.css`) ; boutons radio **natifs gris** de Chromium ; 3 hauteurs de boutons (32, 34, 38) et 3 de champs (34, 38, 40) sur la même page. |
| Security Center (`page-securite.png`) | Mise en page cassée : les cases à cocher des listes de filtres sont **étirées** à la moitié de la largeur (règle globale `input { width: 100% }` appliquée aux `checkbox` dans `.list li`) ; cases natives de 13 px dans « Emergency » ; 7 sections empilées sur 4 000 px sans navigation ; mélange interrupteurs, cases, listes déroulantes natives. (Refonte complète confiée à M-0006.) |
| Historique, Favoris, Téléchargements, Mots de passe, Extensions (`page-*.png`) | Gabarit `.page` 920 px sans navigation, différent des Paramètres ; boutons `ghost` texte « Supprimer » répétés à chaque ligne ; aucune icône ; les dialogues utilisent `prompt()`/`confirm()` natifs (Favoris, Mots de passe, Historique). |
| À propos, Site non sécurisé | En-tête 28 px Chakra Petch + emblème 44 px : 3ᵉ style d'en-tête de page. |

### Points forts à garder
- Jeu d'icônes déjà original (grille 24, traits arrondis) : base du nouveau jeu.
- Palette de nuit cohérente avec l'emblème ; police Chakra Petch (OFL) distinctive.
- Pages servies sous CSP stricte, sans dépendance externe : le design reste local.

---

## 2. Identité « Crépuscule » et système de design

Maquette de référence : `captures/maquettes/systeme-de-design.png`
(source `maquettes/systeme.html`, jetons `maquettes/ds.css`).

**Idée.** NIGHTFALL = la tombée de la nuit. L'interface est une encre bleu-nuit très
sombre, calme et nette ; un seul élément lumineux sert de signature : la **ligne
d'horizon** (dégradé cyan → bleu → violet, comme la dernière lumière). Elle apparaît
avec parcimonie : indicateur de l'onglet actif (22 × 2 px), barre de progression,
séparateur de l'écran de verrouillage, horizon du Nouvel onglet. L'hexagone de
l'emblème est réservé à la marque (emblème, icône « page NIGHTFALL ») ; le biseau des
boutons est abandonné (incohérent avec le reste et peu lisible au focus).
Brave n'a inspiré que la disposition (onglets en haut, Shield dans la barre d'adresse,
panneau latéral) ; aucun logo, icône ni image d'un autre produit n'est utilisé.

**Jetons (3 niveaux, `ds.css`).**
1. Primitives `--p-*` : palette brute, jamais utilisée par un composant.
2. Sémantiques : couleurs `--c-*` (5 choisies par un thème : fond, surface, texte,
   accent, second accent ; le reste dérivé par `color-mix(in oklab, …)`, disponible
   dans Chromium ≥ 111 donc CEF 156) ; typographie `--t-11…40` ; espacements `--s-1…10`
   (grille de 4) ; rayons `--r-xs 4 / sm 6 / md 8 / lg 12 / xl 16 / pill` ; hauteurs
   `--h-xs 24 … xl 40` ; icônes `--i-sm 14 / md 16 / lg 20`, trait 1,6 ; élévation
   `--e-1…3` ; mouvement `--d-instant 80 / fast 140 / base 220 / slow 320 ms`, courbes
   `--ease`, `--ease-out`, `--ease-in`.
3. Composants : `--header-bg`, `--toolbar-bg`, `--tab-*`, `--omnibox-bg`, `--menu-*`,
   `--panel-bg`, `--page-bg`, `--card-bg` : ce que l'éditeur expose en « avancé ».

**Typographie.** Police système (Segoe UI Variable / SF Pro) pour toute l'interface ;
Chakra Petch seulement pour la marque, les titres de page (28) et les chiffres mis en
avant (compteurs Shield, horloge, statistiques). Interface 13 px, pages 14 px,
secondaire 12 px, étiquettes 11 px capitales espacées.

**Icônes.** Jeu original de 52 icônes (`maquettes/icons.js`) dérivé du jeu actuel :
grille 24, trait 1,6 px à 16 px (20 px dans les pages), extrémités arrondies,
remplissage seulement pour un état actif. Nouveaux : boutons de fenêtre, panneau
latéral, palette, thème, vidéo (VideoGuard), cookie, caméra, corbeille, plus.

**Règles de composants** (toutes illustrées dans `systeme-de-design.png`).
| Composant | Règle |
|---|---|
| Bouton | 32 px (28 petit, 40 grand), rayon 8, 4 variantes : principal (accent plein), normal (surélevé + filet), discret (transparent), danger ; focus clavier = anneau 2 px `--c-focus` séparé de 2 px ; pression = `scale(.98)` 80 ms. |
| Bouton icône (barres) | cible 32 × 32, icône 16, rayon 8 ; survol = fond `--c-hover` ; actif = `--c-accent-2` ; pastille 6 px pour signaler (téléchargement en cours…). |
| Champ | 36 px, rayon 8, fond `--c-bg`, filet `--c-line-strong` ; focus = bordure accent 2 + halo 3 px. |
| Interrupteur | 32 × 18, pastille 14, 220 ms `--ease-out` ; cases et boutons radio dessinés aux mêmes couleurs (plus de contrôle natif gris). |
| Pastille d'état | 22 px, pilule, point coloré : ok / attention / danger / neutre. |
| Onglet | voir 4.1. |
| Menu | voir 4.2 et 4.3. |
| Panneau / carte | rayon 12, filet `--c-line`, lignes de réglage ≥ 56 px, titre 13/500 + description 12. |

---

## 3. Header et barre de titre intégrée

### 3.1 Disposition (maquettes `fenetre-*.png`)
- **Hauteur totale 84 px** (au lieu de 88 + barre de titre système de ~31 px sur
  Windows) : bande d'onglets **40 px** qui sert de barre de titre + barre d'outils
  **44 px**.
- Windows : onglets à gauche, zone libre de déplacement, boutons Réduire / Agrandir /
  Fermer dessinés (46 × 40, Fermer rouge au survol) à droite.
- macOS : feux système à gauche (réserve de 80 px), centrés verticalement dans la bande.
- Barre d'outils : Précédent, Suivant, Actualiser (+ Accueil si activé) ; barre
  d'adresse (36 px, rayon 10) contenant l'état de connexion, l'adresse (domaine en
  clair, chemin estompé), le zoom si ≠ 100 %, l'étoile et Shield avec son compteur ;
  puis VideoGuard, Téléchargements (pastille quand actif), Mots de passe, séparateur,
  Verrouiller, Menu. Tous les boutons actuels sont conservés (F-032 à F-035, F-051 à F-062).
- Onglets verticaux (`fenetre-onglets-verticaux.png`, `fenetre-macos-onglets-verticaux.png`) :
  barre latérale 248 px avec en-tête 44 px (feux macOS ou emblème), onglets épinglés
  en grille, liste ; la barre d'outils 44 px porte les boutons de fenêtre sous Windows.
- Fenêtre privée (`fenetre-privee.png`) : même structure, header teinté violet et
  pastille « Privé ».
- Fenêtre inactive : textes du header estompés (`body.inactive`, prototype).

### 3.2 Ce que CEF permet (vérifié dans les liaisons cef-rs 156)
- `ImplWindowDelegate::is_frameless` (`cef/src/bindings/x86_64_pc_windows_msvc.rs:43337`) ;
  doc C : « The window will be resizable if can_resize() returns true. Use
  cef_window_t::set_draggable_regions() to specify draggable regions »
  (`sys/src/bindings/x86_64_pc_windows_msvc.rs:34053`).
- `with_standard_window_buttons` (« only supported on macOS », `:34061`) et
  `titlebar_height` / `get_titlebar_height` (« adjust the vertical position of the
  traffic light buttons in frameless windows », `:34067`).
- `ImplWindow::set_draggable_regions(&self, Option<&[DraggableRegion]>)` (`:44432`),
  coordonnées de fenêtre ; `DraggableRegion { bounds: Rect, draggable: c_int }` (`:939`).
- `ImplDragHandler::on_draggable_regions_changed(browser, frame, regions)` (`:19271`),
  alimenté par la propriété CSS `app-region: drag / no-drag` des pages.
- `ImplWindow::minimize / maximize / restore / is_maximized / is_fullscreen / is_active`,
  `ImplWindowDelegate::on_window_bounds_changed / on_window_activation_changed`.
- Côté CEF : seule une **ancienne** version de `window_view.cc` a pu être consultée
  (miroir third-party-mirror.googlesource.com, révision 1187115c, non CEF 156) :
  cadre sans bordure Windows = `remove_standard_frame`, bordure de redimensionnement
  de 4 px et coins de 16 px, test de la zone déplaçable **avant** les vues enfants.
  Si c'est toujours le cas en CEF 156, une vue superposée (menu) posée sur une zone
  déplaçable ne recevrait pas ses clics ; le prototype retire donc le menu de la zone
  (région `draggable: 0` ajoutée en dernier). **Non vérifié en CEF 156** : ni le
  comportement d'origine, ni l'effet de la correction (voir 3.4).

### 3.3 Prototype (branche, `app/src/nf/titlebar.rs` + `shell.*`)
- `NfWindowDelegate::is_frameless` = vrai sous Windows et macOS ; `titlebar_height`
  (macOS) = hauteur de la bande d'onglets (38) ou de l'en-tête latéral (52) ;
  `on_window_bounds_changed` / `on_window_activation_changed` / transition plein écran
  → événement `winstate` (agrandie, plein écran, active) vers l'interface.
- Gestionnaire `NfUiDrag` (vues d'interface seulement) : les régions de la vue du
  haut et de la vue latérale sont stockées, traduites en coordonnées de fenêtre
  (`convert_point_to_window`) et appliquées ; recalcul à chaque changement de mise en
  page (`apply_mode`), ouverture / redimensionnement / fermeture d'un menu (exclu de
  la zone). Aucune région venant d'un menu ni d'une page web n'est acceptée.
- API : `win.minimize`, `win.maximize` (bascule), `win.close`, autorisées aussi avant
  déverrouillage (la fenêtre sans cadre n'a pas d'autres boutons), refusées à tout
  appelant autre que la vue du haut. `state` renvoie `frameless` et `platform`.
- Interface : `app-region: drag` sur la bande d'onglets (et barre d'outils + en-tête
  latéral en mode vertical), `no-drag` sur onglets, boutons, champs ; bande de
  déplacement de 38 px au-dessus du démarrage et du verrouillage ; boutons de fenêtre
  Windows ; réserve de 78 px pour les feux macOS ; masqués en plein écran.
- Auto-test (`step_titlebar`) : zones reçues, point déplaçable et point non
  déplaçable trouvés ; sous Windows, réponses **réelles** de la procédure de fenêtre
  à `WM_NCHITTEST` (HTCAPTION sur la bande, HTCLIENT sur un onglet et dans la page,
  HTLEFT au bord gauche, HTBOTTOMRIGHT au coin) ; agrandir puis restaurer par
  l'API ; une région de plus est donnée à la fenêtre quand le menu est ouvert
  (seul le nombre de régions est vérifié, pas le clic sur un menu posé sur la bande).
- CI : souris réelle (`ci/titlebar_win.ps1`, SendInput/mouse_event) sur la fenêtre
  verrouillée sous Windows : hit-test, glisser pour déplacer, redimensionner par le
  bord droit, double-clic = agrandir / restaurer, boutons Agrandir / Restaurer /
  Réduire de l'interface, captures. macOS : événements souris synthétiques CoreGraphics
  (`ci/titlebar_mac.swift`) : glisser et double-clic sur la bande.

### 3.4 Résultats CI du prototype

Run **42023686973** (PR #27, commit `f0cae8c`) : **tous les jobs verts**, dont
Application Windows (job 127232475869) et Application macOS (job 127232475518),
auto-test terminé par `SELFTEST OK` sur les deux. (Run précédent 41990861781 :
macOS vert, Windows en échec de compilation sur le type `HWND` de cef-sys, corrigé.)

**Windows (Windows Server 2025, écran 1024 × 768)** — souris réelle sur la fenêtre :
```
NFTITLEBAR fenêtre … classe=Chrome_WidgetWin_1 style=0x16C70000 (WS_CAPTION=True)
NFTITLEBAR OK   bande de titre = HTCAPTION(2) réponse 2
NFTITLEBAR OK   bouton Fermer = HTCLIENT(1) réponse 1
NFTITLEBAR OK   bord gauche = HTLEFT(10) réponse 10
NFTITLEBAR OK   contenu = HTCLIENT(1) réponse 1
NFTITLEBAR OK   déplacement par glisser (-87,84)-(855,746) 942x662      (départ (41,29))
NFTITLEBAR OK   redimensionnement par le bord droit (-87,84)-(737,746) 824x662
NFTITLEBAR OK   double-clic : agrandie (-8,-8)-(1032,728) 1040x736
NFTITLEBAR OK   double-clic : restaurée
NFTITLEBAR OK   bouton Agrandir / bouton Restaurer / bouton Réduire / restauration après réduction
```
Auto-test (fenêtre déverrouillée, onglets) :
`"hitTest":{"drag":2,"noDrag":1,"content":1,"leftEdge":10,"bottomRight":17}`, puis
agrandie (`"maximized":true`, bords = HTCLIENT comme attendu), restaurée, et avec le
menu ouvert 72 régions au lieu de 71 : la région du menu est bien ajoutée, mais
aucun clic dans un menu posé sur la bande d'onglets n'a été testé.
Captures : `captures/ci/windows-demarrage-verrouillage.png`,
`windows-agrandie-double-clic.png` (agrandie : boutons entièrement visibles, le
débordement de 8 px habituel de Windows est compensé par Chromium),
`windows-apres-tests-souris.png`, `windows-fin-autotest.png` (onglets dans la barre de
titre, boutons de fenêtre à droite, menu ouvert).

**Run suivant 42025158585 (commit `d681812`, documentation seulement, même code)** :
Windows vert ; **macOS rouge** : `SELFTEST FAIL: effacement des données des sites
(DevTools Storage.clearDataForOrigin)`, étape antérieure à l'étape barre de titre et
qui ne touche aucun fichier modifié par le prototype ; le même code était vert au run
précédent. L'audit le classe comme échec intermittent préexistant ; je ne l'ai pas
vérifié moi-même sur `develop`. Dans ce run, le test souris macOS est resté OK
(déplacement, double-clic).

**macOS (runner GitHub, 1024 × 768)** — événements souris synthétiques CoreGraphics
acceptés par le runner :
```
NFTITLEBAR position initiale (41.0, 45.0, 942.0, 620.0)
NFTITLEBAR OK   déplacement par glisser (-70.0, 87.0, 942.0, 620.0)
NFTITLEBAR OK   double-clic : zoom (0.0, 31.0, 1024.0, 677.0)
```
Auto-test : `"draggable":3,"regions":64`, agrandir / restaurer par l'API, 65 régions
avec le menu ouvert. Captures : `captures/ci/macos-demarrage-verrouillage.png`
(fenêtre sans cadre, coins arrondis et ombre système conservés, feux centrés dans
la bande), `macos-apres-glisser-zoom.png`, `macos-fin-autotest.png` (feux à gauche
des onglets, aucune barre de titre système).

### 3.5 Conclusion

| Plateforme | Verdict |
|---|---|
| **macOS** | **fiable avec réserves** : seuls le déplacement et le double-clic sont testés, avec des événements synthétiques, et ce test n'est **pas bloquant** (`ci.yml`, `… \|\| echo "… non concluant"`). |
| **Windows** | **fiable avec réserves** : déplacement, redimensionnement par le bord droit, double-clic, boutons et réponses `WM_NCHITTEST` testés à la souris réelle ; le reste est déduit ou non testé (tableau ci-dessous). |

Aucune plateforme n'est « non fiable » à ce stade. Mais une partie des
comportements ci-dessous reste à prouver dans la mission B avant toute intégration.

| Comportement | Windows | macOS |
|---|---|---|
| Déplacer en glissant la bande | **testé** (souris réelle, bloquant) | **testé** (événements synthétiques, non bloquant) |
| Double-clic : agrandir / restaurer (zoom) | **testé** | **testé** (zoom seulement, non bloquant) |
| Redimensionner par un bord | **testé** (bord droit ; bord gauche et coin : hit-test seulement) | non testé (bords système) |
| Boutons Réduire / Agrandir / Restaurer | **testés** (boutons dessinés) | non testé (feux natifs, visibles sur la capture) |
| Fermer par le bouton | hit-test seulement (non cliqué : fermerait le test) | non testé |
| Snap par glisser vers un bord, Win+flèches | **non testé** (déduit de HTCAPTION) | sans objet |
| Menu Snap Layouts (survol d'Agrandir, Windows 11) | **absent** (Q1) | sans objet |
| Menu système Alt+Espace, clic droit sur la bande | non testé | sans objet |
| Ombre, coins arrondis | non testé (runner Windows Server 2025) | constatés sur capture |
| Agrandie : contenu non rogné | constaté sur capture (1024 × 768) | constaté sur capture |
| Onglets verticaux (bande = barre d'outils + en-tête latéral) | non testé | non testé |
| Fenêtre privée | non testé | non testé |
| Plein écran fenêtre (F-027) et page (F-028) : bande masquée, pas de zone fantôme | non testé | non testé |
| Taille minimale 640 × 420 (F-001) | non testé | non testé |
| DPI ≠ 100 % (125 %, 150 %, 200 %) | non testé (CI à 100 %) | non testé (CI sans Retina) |
| Écrans multiples, déplacement entre écrans de DPI différents | non testé | non testé |
| Windows 10 | non testé | sans objet |
| Clic dans un menu ouvert au-dessus de la bande | non testé (seul le nombre de régions est vérifié) | non testé |
| `app-region: drag` dans une page web sans effet sur la fenêtre | non testé (le code n'accepte que les vues top et side) | non testé |
| Écran de verrouillage : déplacement et boutons | **testé** (c'est l'écran des tests souris) | **testé** (déplacement, zoom) |

Tous les cas « non testé » deviennent des critères de la mission B (section 6).

---

## 4. Onglets, menus, panneaux, animations

### 4.1 Onglets (`captures/maquettes/onglets-etats.png`)
- Onglets **flottants** 32 px dans la bande de 40 px, rayon 8, largeur 216 px ;
  actif = surface surélevée + filet + indicateur horizon 22 × 2 px ; survol = fond
  `--c-hover` ; aucun séparateur vertical.
- Rétrécissement : 216 → 84 px (icône + ✕ au survol) → 40 px (icône seule, ✕ sur
  l'actif) : comportement F-017 conservé, seuils inchangés en logique ; épinglés 40 px
  fixes (F-012).
- États conservés : chargement (anneau), son / son coupé (F-013, F-014), déchargé
  (F-113, titre estompé), planté (F-030), privé, glisser-déposer (F-016 : l'onglet
  soulevé + trait d'insertion 2 px).
- Info-bulle riche au survol prolongé (600 ms) : titre, domaine, mémoire, éléments
  bloqués, son : « informations avancées à la demande ».

### 4.2 Menus (`captures/maquettes/menus.png`)
- Menu principal regroupé en 6 blocs courts (Créer · Zoom · Bibliothèque · Outils ·
  Réglages · Sécurité) avec libellés d'un ou deux mots ; « Plein écran » (F-027) reste
  une entrée du menu et un bouton de la ligne Zoom ; sous-menu « Plus d'outils »
  pour Imprimer, Enregistrer la page, Accueil, Outils de développement, Security
  Center, À propos. **Aucune entrée de F-063/F-064 ne disparaît** (Redémarrer pour
  mettre à jour reste en tête quand une mise à jour est prête).
- Panneau Shield : le chiffre, un interrupteur pour le site, détail « Avancé » replié.
- Menu d'onglet, suggestions, recherche, mots de passe : mêmes éléments `.mi` 32 px.

### 4.3 Contrainte CEF des vues superposées (importante pour le design)
Les menus sont des BrowserViews superposées (`add_overlay_view`). CEF n'accepte pour
un navigateur fenêtré qu'un fond **opaque** : `_cef_browser_settings_t::background_color`
(`sys/src/bindings/x86_64_pc_windows_msvc.rs:18261`) renvoie, pour un alpha nul, à
`CefSettings.background_color`, dont la documentation (`:18080`) dit : « If the alpha
component is fully transparent for a windowed browser then the default value of
opaque white be used ». Conséquence probable (non testée en vraie grandeur) : pas de
coins arrondis transparents ni d'ombre portée hors du rectangle. Les maquettes dessinent donc les menus en **rectangles nets avec filet de
1 px** (les éléments intérieurs gardent leur rayon de 6 px). Sur les captures CI, les coins du
menu actuel (rayon CSS 12 px) ne laissent pas voir la page : le coin prend la couleur
de fond de la vue. Alternative (à évaluer plus tard, pas prioritaire) :
menus natifs `Window::show_menu` (perte du style) ou fenêtre fille transparente
(non prévue par CEF Views).

### 4.4 Panneaux (`captures/maquettes/panneaux.png`)
- Bandeau d'autorisation 44 px sous la barre d'outils (F-071, F-105 : « Mémoriser »
  conservé).
- Panneau latéral droit 360 px (nouvelle vue CEF) : Téléchargements / Favoris /
  Historique en accès rapide ; les pages complètes restent.
- Notifications (toast) en bas au centre, 44 px.

### 4.5 Animations (`systeme-de-design.png`, section Mouvement)
Survol 80–140 ms (couleur seulement) ; ouverture de menu 220 ms (opacité + 4 px, dans
la vue du menu) ; sortie 140 ms ; ouverture / fermeture d'onglet 220 ms (largeur) ;
changement de thème 320 ms ; `prefers-reduced-motion` et un réglage NIGHTFALL
ramènent tout à 0.

### 4.6 Page interne type (`captures/maquettes/page-interne-apparence.png`)
Gabarit commun à toutes les pages internes : navigation latérale 248 px (pour les
Paramètres ; les autres pages reprennent l'en-tête et les cartes), titre 28 Chakra
Petch, sections en étiquettes capitales, cartes rayon 12, lignes de réglage 56 px.
La maquette montre la section **Apparence et thèmes** avec les 13 rubriques demandées
par le propriétaire (le contenu détaillé des autres sections relève de M-0006).

---

## 5. Moteur de thèmes et éditeur

Maquettes : `page-interne-apparence.png` (galerie), `editeur-themes.png` (éditeur),
`fenetre-theme-aube.png` et `fenetre-theme-braise.png` (thèmes appliqués).

### 5.1 Modèle de données (stocké chiffré dans `Settings`)
Nouveaux champs, tous `#[serde(default)]`, aucun champ existant renommé ni supprimé :
```rust
pub theme: String,              // id actif : "crepuscule" (défaut) | préréglage | "custom:<uuid>"
pub custom_themes: Vec<Theme>,  // thèmes de l'utilisateur (≤ 50)
pub reduce_motion: String,      // "system" | "on" | "off"
pub density: String,            // "compact" | "normal" | "comfortable"

pub struct Theme {              // nf-core (testable sans CEF)
    pub id: String, pub name: String, pub base: String, // "dark" | "light"
    pub based_on: String,       // préréglage d'origine
    pub colors: BTreeMap<String, String>, // clés autorisées : bg, surface, text, accent, accent2,
                                          // header, toolbar, tab_active, menu, panel, page…
    pub newtab: NewTabBackground,          // wallpaper id | color | gradient (2 couleurs)
    pub version: u32,
}
```
Le fond actuel `wallpaper` (F-090) reste la source pour le Nouvel onglet quand le
thème ne fixe pas de fond.

**Compatibilité (point bloquant).** Aujourd'hui, si la lecture de `Settings` échoue,
`api.rs` (ouverture des magasins, `st.setting::<state::Settings>("app")`, vers la
ligne 766) garde silencieusement **tous** les réglages par défaut, et la sauvegarde
suivante écrase les réglages de l'utilisateur. Un thème mal formé ne doit donc
jamais faire échouer la lecture de `Settings` :
- `Theme` et `NewTabBackground` portent `#[serde(default)]` **au niveau de la
  structure** (chaque champ absent prend sa valeur par défaut) ;
- `NewTabBackground` n'est **pas** une énumération serde stricte : c'est une
  structure `{ kind: String, wallpaper: String, colors: Vec<String> }` ; un `kind`
  inconnu (thème créé par une version plus récente) se replie sur le fond
  `wallpaper` actuel ; même règle pour `base` (inconnu → `dark`) ;
- `custom_themes` est lu par un désérialiseur dédié (`deserialize_with`) qui lit une
  liste de `serde_json::Value` et convertit **chaque** élément séparément : un thème
  invalide est ignoré (et journalisé), les autres sont gardés, jamais d'échec global ;
- champs inconnus ignorés (pas de `deny_unknown_fields`) ; `theme` qui désigne un
  thème absent → thème par défaut à l'affichage, sans réécrire la valeur ;
- `reduce_motion` et `density` : valeur inconnue → `system` / `normal`.

### 5.2 Génération et sécurité
- `nf_core::theme` : validation stricte (couleurs `#rgb`, `#rrggbb`, `#rrggbbaa`
  seulement, clés dans une liste blanche, nom ≤ 40 caractères, ≤ 50 thèmes),
  dérivation (survol, filets, texte 2 et 3 en oklab), contrôle de contraste (5.6) et
  `to_css(&Theme) -> String` qui n'émet que `:root{--c-…:#…}`. Aucune chaîne libre
  de l'utilisateur n'atteint le CSS : pas d'injection, pas d'`url()`.
- `scheme.rs` sert `nightfall://app/theme.css` (MIME `text/css` nu) généré depuis le
  thème actif (+ `?preview=1` pour l'aperçu), en-tête `Cache-Control: no-store`. La CSP
  réelle (`scheme.rs:18`) est `style-src 'self' 'unsafe-inline'` : `theme.css` est
  autorisé par `'self'` sans changement. `'unsafe-inline'` existe déjà pour les
  attributs `style=` des pages ; il ne protège donc pas d'une injection CSS, d'où la
  règle « aucune chaîne libre de l'utilisateur dans le CSS » ci-dessus.
- Chaque page interne et `shell.html` ajoutent `<link rel="stylesheet" href="theme.css">`
  après `nf.css` (à déclarer dans `assets.rs` comme ressource dynamique).

### 5.3 Application sans rechargement
- `theme.set` (API, thread UI) enregistre puis appelle `windows::push_event_all`
  `{type:"theme", v:N}` ; et `exec_in_tab` sur chaque onglet dont l'URL commence par
  `nightfall://app/` : `nfTheme(N)`.
- `nf.js` : `nfTheme(v)` remplace le `href` du lien par `theme.css?v=N` ; une classe
  `theme-switching` (320 ms) anime les couleurs. Aucun rechargement de page, l'état
  des formulaires est conservé.
- Natif : `Panel::set_background_color` / `Window::set_background_color` reçoivent la
  couleur de fond du thème (évite l'éclair sombre d'un thème clair) ; les nouvelles
  vues prennent `BrowserSettings.background_color` du thème.
- Écran de verrouillage : il s'affiche **avant** le déchiffrement des réglages ; il
  utilise donc le thème par défaut, sauf si le propriétaire accepte de copier
  l'identifiant du préréglage (pas les couleurs personnalisées) dans les préférences
  non chiffrées (question Q3).

### 5.4 Aperçu, préréglages, import / export
- Aperçu local : l'éditeur applique les variables sur sa maquette intégrée
  (fenêtre, page interne, menus) à chaque modification, sans rien enregistrer.
- « Essayer en direct » : `theme.preview` place un thème temporaire en mémoire
  (jamais écrit), servi par `theme.css?preview=1` à toutes les vues ; « Appliquer »
  l'enregistre, « Annuler », la fermeture de l'éditeur ou 2 minutes sans action le
  retirent.
- Préréglages (`maquettes/themes.css`) : Crépuscule (défaut), Minuit (noir OLED),
  Aube (clair), Aurore, Braise, Brume ; plus la teinte « Privé » appliquée par-dessus
  le thème dans les fenêtres privées. Non modifiables ; « Dupliquer » crée une copie
  personnalisée.
- Gestion : créer (depuis un préréglage), modifier, dupliquer, renommer, supprimer
  (confirmation, retour au défaut si actif) ; export / import JSON (`theme.export`
  ouvre la boîte « Enregistrer sous », `theme.import` lit un fichier choisi, ≤ 16 Ko,
  validé comme ci-dessus).

### 5.5 Éditeur (`editeur-themes.png`)
Colonne gauche : rubriques Couleurs / Header / Onglets / Menus / Panneaux / Fond ;
5 couleurs de base, puis
« Avancé (dérivé automatiquement) » pour chaque jeton de composant ; sélecteur de
couleur (teinte/saturation, hexadécimal, pipette) avec indicateur de contraste.
Droite : aperçu (Fenêtre / Page interne / Menus) où un clic sur un élément sélectionne
sa couleur. Pied : Annuler, Dupliquer, Appliquer.

---

### 5.6 Contraste (règle du moteur, appliquée aux maquettes)
- **Tout texte** ≥ 4,5:1 sur les 4 surfaces (`bg`, `surface`, `raised`, `overlay`) :
  `text`, `text-2` et `text-3` dérivés, `accent-2` (employé pour des liens et compteurs),
  couleurs d'état, et texte sur l'accent.
- **Indicateurs non textuels** (accent des interrupteurs, anneau de focus, indicateur
  d'onglet) ≥ 3:1 sur `bg`, `surface`, `raised`.
- `--c-on-accent` est **calculé** par le moteur : blanc ou encre (`#0b0e16`), celui qui
  dépasse 4,5 ; si aucun ne l'atteint, l'éditeur propose l'accent assombri ou éclairci
  le plus proche qui y parvient (le thème n'est pas enregistrable autrement).
- La première version de ce dossier ne respectait pas ces seuils (relevé de l'audit,
  confirmé : blanc sur l'accent 3,70 ; Aurore 2,53 ; Minuit 3,09 ; Brume 3,24 ; `text-3`
  ≈ 3,1 à 3,6, Aube 2,71 ; `accent-2` d'Aube 2,91 ; l'éditeur affichait « AA 6,1:1 »
  pour un ratio réel de 4,43). Jetons et préréglages corrigés (`maquettes/ds.css`,
  `maquettes/themes.css`) : accent Crépuscule `#2967ff`, Minuit `#3867ff`, texte sur
  accent automatique (encre pour Aurore, Braise, Brume), `text-3` relevé partout,
  couleurs d'état d'Aube assombries. Vérification reproductible :
  `python3 outils/contraste.py maquettes` (code de sortie 0) :

```
crepuscule  OK  text 14.15 ; text-2 5.28 ; text-3 4.64 ; accent-2 8.93 ; danger 5.37 ; warn 8.78 ; ok 8.35 ; on-accent/accent 4.69 ; accent (indicateur) 3.71
minuit      OK  text 15.17 ; text-2 5.11 ; text-3 4.63 ; accent-2 10.16 ; danger 5.74 ; warn 9.38 ; ok 8.92 ; on-accent/accent 4.61 ; accent (indicateur) 4.02
aube        OK  text 14.69 ; text-2 5.10 ; text-3 4.71 ; accent-2 4.85 ; danger 5.19 ; warn 5.05 ; ok 5.32 ; on-accent/accent 5.23 ; accent (indicateur) 4.62
aurore      OK  text 12.44 ; text-2 5.27 ; text-3 4.65 ; accent-2 10.02 ; danger 4.68 ; warn 7.64 ; ok 7.27 ; on-accent/accent 7.63 ; accent (indicateur) 6.61
braise      OK  text 13.99 ; text-2 5.85 ; text-3 4.61 ; accent-2 9.86 ; danger 5.39 ; warn 8.81 ; ok 8.38 ; on-accent/accent 6.47 ; accent (indicateur) 6.09
brume       OK  text 11.66 ; text-2 5.03 ; text-3 4.65 ; accent-2 9.55 ; danger 4.66 ; warn 7.61 ; ok 7.24 ; on-accent/accent 5.96 ; accent (indicateur) 4.86
```
  Conséquence assumée : `text-3` est désormais proche de `text-2` (la hiérarchie
  passe par la taille et la graisse, pas par un gris illisible).

### 5.7 Ce que les thèmes ne couvrent pas (éléments natifs)
Rendus par le système ou par Chromium hors de nos pages, ils ne suivent pas le thème
(au mieux `color-scheme` clair/sombre) : feux macOS et ombre de fenêtre ; menus
contextuels natifs des pages (clic droit, `NfContextMenu`) ; listes déroulantes
`<select>` ouvertes, sélecteurs de date et de couleur ; boîtes de dialogue
JavaScript (`alert`, `prompt`, `confirm`), d'impression, d'enregistrement et de
choix de fichier ; info-bulles `title` ; fenêtre des outils de développement ;
barre des tâches, Alt+Tab, Dock ; contenu des pages web. Les dialogues natifs des
pages internes seront remplacés (mission G).

### 5.8 Accessibilité (critères pour A à G)
- `@media (forced-colors: active)` (contraste élevé de Windows) : couleurs système
  (`Canvas`, `CanvasText`, `Highlight`, `ButtonText`), bordures visibles sur onglets,
  menus et boutons de fenêtre ; vérifié par capture Playwright avec
  `forcedColors: 'active'`.
- Clavier : tout est atteignable (onglets, barre d'outils, menus, boutons de fenêtre
  dessinés), focus toujours visible (anneau ≥ 3:1), F6 / Ctrl+L / Échap conservés,
  ordre de tabulation testé.
- Lecteur d'écran : rôles `tablist` / `tab` (`aria-selected`), `toolbar`, `menu` /
  `menuitem`, `switch` (`aria-checked`), libellés `aria-label` sur toutes les icônes.
  Les vues superposées CEF étant des documents séparés, vérifier avec NVDA et
  VoiceOver (vérification manuelle).
- Mouvement : `reduce_motion` (système / activé / désactivé) et
  `prefers-reduced-motion`.

## 6. Découpage en missions d'exécution

Ordre conseillé : A → (B, C en parallèle) → D → E → F. Chaque mission garde la CI
complète verte (auto-test `SELFTEST OK`, captures) et `preserve.py` sans perte.

| # | Mission | Fichiers | Dépend de | Critères d'acceptation | Fonctionnalités touchées et préservation |
|---|---|---|---|---|---|
| A | **Jetons et composants de base** (Sonnet) | `app/ui/nf.css`, `app/ui/settings.css`, `app/ui/fonts/` (inchangées), `ci/ui_render_test.js` | — | jetons de `ds.css` dans `nf.css` ; anciens noms gardés comme alias ; boutons, champs, interrupteurs, cases, radios, pastilles, cartes conformes à 2 ; correction des cases étirées du Security Center ; relevé automatique (méthode `audit_render.js`) : ≤ 8 tailles de police, ≤ 6 rayons, ≤ 5 hauteurs de contrôles par page | Aucune logique : toutes les pages gardent leurs contrôles et identifiants (F-073 à F-119) ; vérification visuelle CI. |
| B | **Barre de titre intégrée + header** (Opus : CEF, fenêtres) | `app/src/nf/titlebar.rs` (issu du prototype), `windows.rs`, `client.rs`, `api.rs`, `selftest.rs`, `app/ui/shell.*`, `app/Cargo.toml` (feature `Win32_UI_WindowsAndMessaging`), `ci/titlebar_*.{ps1,swift}`, `.github/workflows/ci.yml` | A | prototype repris tel quel puis header 40 + 44 px ; tests CI du prototype conservés et verts sur les deux plateformes ; Snap par glisser, double-clic, redimensionnement, boutons, plein écran (F-027, F-028), mode vertical (F-020), fenêtre privée (F-172 à F-174), verrouillage (F-072) | F-001 change de moyen (boutons dessinés sous Windows, feux natifs sous macOS) mais pas de fonction : agrandir, réduire, redimensionner, fermer, minimum 640 × 420 restent testés. F-003 : titre de fenêtre toujours défini (barre des tâches, Alt+Tab). |
| C | **Icônes et onglets** (Sonnet) | `app/ui/shell.html` (sprite), `shell.css`, `shell.js` (rendu des onglets) | A | jeu d'icônes 2 ; onglets 4.1 ; info-bulle d'onglet ; seuils de rétrécissement inchangés | F-004, F-006, F-012 à F-017, F-021, F-030 : mêmes gestes, mêmes commandes d'API ; tests `ui_render_test.js` barre-d-outils et menu-onglet. |
| D | **Menus, panneaux, animations** (Sonnet) | `shell.css`, `shell.js` (panneaux), nouvelle vue panneau latéral dans `windows.rs` (+ Opus si la vue latérale droite est retenue) | B, C | menus 4.2 avec toutes les entrées de F-063/F-064 ; panneau Shield 4.2 (F-066) ; animations 4.5 ; `prefers-reduced-motion` | F-063 à F-070, F-197 : entrées et raccourcis conservés, une entrée peut passer dans « Plus d'outils » mais reste accessible en 2 clics au plus. |
| E | **Moteur de thèmes** (Opus : réglages chiffrés, CSS généré) | `crates/nf-core/src/theme.rs` (+ tests), `app/src/nf/state.rs` (nouveaux champs), `api.rs` (`theme.*`), `scheme.rs` (`theme.css`), `assets.rs`, `windows.rs` (couleur native), `app/ui/nf.js`, toutes les pages (`<link>`), `selftest.rs` | A | tests unitaires de validation, dérivation, contraste, anti-injection ; anciens réglages chargés sans perte (test de désérialisation d'un `Settings` de 0.1.4) ; auto-test : changer de thème met à jour la barre et une page interne ouverte sans rechargement | F-088, F-090 (fond), F-118 (Réinitialiser : remet le thème par défaut, conserve les thèmes personnels) ; aucune donnée existante modifiée. |
| F | **Éditeur et galerie de thèmes** (Sonnet) | `app/ui/settings.*` (section Apparence), nouvelle page `themes.html/js` (déclarée dans `assets.rs`) | E, et l'organisation des Paramètres de M-0006 | galerie, préréglages, créer / modifier / dupliquer / supprimer, aperçu local et en direct, import / export, contraste ; tests Playwright de l'éditeur | F-090 à F-095 restent dans « Apparence » ; aucun réglage existant déplacé hors de la section sans renvoi. |
| G | **Pages internes au gabarit commun** (Sonnet, après M-0006) | `newtab.*`, `history.*`, `bookmarks.*`, `downloads.*`, `passwords.*`, `extensions.*`, `about.*`, `insecure.*` | A, E | gabarit 4.6 ; dialogues natifs `prompt/confirm` remplacés par des dialogues de la page ; Nouvel onglet 3.1 | F-073 à F-083, F-198 : mêmes actions, mêmes commandes d'API. |

**Critères complémentaires (après audit)**
- **A** : jetons conformes à 5.6 (contrôle automatique type `outils/contraste.py`
  sur `nf.css`) ; jetons de densité (`compact` / `normal` / `comfortable`) et règle CSS
  du mouvement réduit ; critères d'accessibilité 5.8 (forced-colors, focus).
- **B** :
  - le test souris macOS devient **bloquant** (le job échoue si le déplacement ou le
    double-clic échoue ; un refus d'événements synthétiques par le runner doit être
    détecté et signalé à part, pas confondu avec un succès) ;
  - nouveaux tests automatiques : clic dans un menu ouvert posé sur la bande
    (le menu reçoit le clic), page web avec `app-region: drag` (la fenêtre ne bouge
    pas), onglets verticaux et fenêtre privée (hit-test et déplacement), plein écran
    F-027 et F-028 (aucune zone déplaçable, boutons masqués), taille minimale
    640 × 420 (F-001), Fermer cliqué en fin de test ;
  - vérifications **manuelles** consignées (captures) : Windows 11 (Snap par glisser,
    Win+flèches, ombre, coins, Alt+Espace), Windows 10, DPI 125 / 150 / 200 %, deux
    écrans de DPI différents, macOS Retina et deux écrans ;
  - corrections du prototype : hauteur de barre de titre par fenêtre (pas
    globale), limitation de fréquence de `push_state` pendant un redimensionnement,
    code mort retiré.
- **E** : en plus des tests prévus, tests de désérialisation : thème partiel (champs
  manquants), `kind` / `base` inconnus (repli), liste avec un thème invalide parmi des
  valides (seul l'invalide est ignoré), réglages écrits par une version plus récente
  (champs inconnus) : dans tous les cas `Settings` se charge et aucun autre réglage
  n'est perdu ; champs `reduce_motion` et `density` (avec A pour le CSS et F pour
  l'interface).
- **F** : rubriques Couleurs, Header, **Onglets**, Menus, **Panneaux**, Fond ;
  indicateur de contraste qui affiche le ratio réellement calculé ; réglages
  « Densité » et « Animations ».

Parallélisme : A seule d'abord (fichiers communs) ; puis B et C touchent tous deux
`shell.*` → **séquencer B puis C** ou désigner B comme intégrateur. E et C sont
indépendantes. M-0006 (Paramètres, Security Center) doit fixer l'organisation des
Paramètres avant F et G.

---

## 7. Risques et questions pour le propriétaire

| Risque | Gravité | Mesure |
|---|---|---|
| Windows 11 : le menu de disposition **Snap Layouts** (survol du bouton Agrandir) n'apparaît pas avec des boutons dessinés en HTML (il faut répondre `HTMAXBUTTON` à `WM_NCHITTEST`, ce que CEF ne permet pas de déclarer). Snap par glisser vers les bords et Win+flèches restent disponibles (zone = HTCAPTION). | moyenne (confort) | Assumer, ou sous-classer la fenêtre native (`SetWindowSubclass`) pour répondre HTMAXBUTTON sur la zone du bouton : faisable mais code Win32 délicat ; à décider (Q1). |
| Ombre et coins arrondis Windows 11 d’une fenêtre sans cadre | à vérifier à la main | Non vérifiable en CI (runner Windows Server 2025) ; voir 3.5. |
| Menu ouvert au-dessus d'une zone déplaçable | moyenne (non vérifiée) | Région soustraite par le prototype ; seul le nombre de régions est vérifié ; test de clic exigé en B. |
| macOS : double-clic sur la bande | faible | Vérifié en CI (zoom) par événements synthétiques, test non bloquant (rendu bloquant en B). |
| Comportements déduits ou non testés (3.5) | moyenne | Critères de la mission B (tests automatiques et manuels). |
| Contraste des thèmes personnalisés | faible | Règle 5.6 bloquante dans l'éditeur. |
| Vues superposées opaques (pas de coins transparents ni d'ombre) | faible | Design en rectangles à filet (4.3). |
| Thème de l'écran de verrouillage | décision | Q3. |

Questions :
- **Q1** : accepter l'absence du menu Snap Layouts au survol du bouton Agrandir sous
  Windows 11, ou financer en temps une mission Win32 dédiée ?
- **Q2** : valider l'identité « Crépuscule » (ligne d'horizon, abandon du biseau des
  boutons, Chakra Petch limité à la marque et aux chiffres).
- **Q3** : l'écran de verrouillage peut-il prendre le préréglage choisi (identifiant
  seulement, stocké en clair) ou doit-il rester sur le thème par défaut ?
- **Q4** : panneau latéral droit (Téléchargements / Favoris / Historique) : à faire
  dans cette refonte ou plus tard ?

---

## Annexe : fichiers de la mission

- `outils/audit_mock.py`, `outils/audit_render.js` : audit reproductible.
- `outils/rendre_maquettes.js` : rend toutes les maquettes en PNG
  (`NODE_PATH=$(npm root -g) node outils/rendre_maquettes.js maquettes captures/maquettes`).
- `maquettes/` : `ds.css` (jetons et composants), `themes.css` (préréglages),
  `icons.js` (jeu d'icônes), `page.css`, `fenetre.html`, `onglets.html`, `menus.html`,
  `panneaux.html`, `page-apparence.html`, `editeur.html`, `systeme.html`,
  `verrouillage.html`.
- `captures/audit/` (24 + relevé JSON), `captures/maquettes/` (16), `captures/ci/`.
- Prototype : `app/src/nf/titlebar.rs`, modifications de `windows.rs`, `client.rs`,
  `api.rs`, `selftest.rs`, `mod.rs`, `app/Cargo.toml`, `app/ui/shell.{html,css,js}`,
  `ci/titlebar_win.ps1`, `ci/titlebar_mac.swift`, `.github/workflows/ci.yml`.
