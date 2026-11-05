# M-0006 : Paramètres en centre de contrôle et Security Center intégré — dossier de conception

- Rôle : Architecte (Opus). Branche `agent/M-0006-conception-parametres-securite`.
- Base lue : `orga/refonte` (c550072), dont l'application est celle de `develop` (0.1.4) ;
  registre accepté lu sur `origin/main` (`docs/agents/FONCTIONNALITES.md`, M-0001 tentative 2) ;
  résultats VideoGuard lus sur `origin/agent/M-0004-audit-videoguard-v2` (§6) ; résultats
  Shield M-0003 (non acceptés à ce jour) cités seulement pour leur ligne P2-7.
- Aucune modification du navigateur. Livrables : ce fichier et `maquettes/` (HTML + PNG).
- Le style visuel final relève de M-0005 : les maquettes réutilisent les jetons de
  `app/ui/nf.css` et ne fixent que l'organisation, les contenus et les comportements.

## 0. Résumé pour le Directeur

1. **Inventaire complet** (§2) : 52 réglages et données enregistrés, 34 actions, 7 lieux
   d'interface. Chacun reçoit une place unique dans la nouvelle organisation (§4).
2. **Constats qui conditionnent la conception** (§3), vérifiés dans le code :
   - 4 doublons réels : changement de code et Recovery Key (Paramètres › LOCK **et** Security
     Center), anti-phishing (Paramètres › Confidentialité **et** Security Center), lien
     Shield/Emergency faisant aller-retour entre les deux pages.
   - **Risque d'écrasement** : `settings.js:save` envoie l'objet `Settings` entier ; une page
     ouverte avec une copie ancienne annule ce qu'une autre page a changé (`session_vault`,
     `phishing_warnings`, `wallpaper`, `shortcuts`…). `settings_set` (`api.rs:990-1019`) ne
     protège que 4 champs. Même défaut pour Shield : `security.js:saveShield` renvoie toute la
     configuration, y compris l'`allowlist` et les dates `updated` des listes.
   - **Risque de perte de réglages** : `ShieldConfig` (`crates/nf-shield/src/lib.rs:43-52`) et
     `VideoGuardConfig` (`crates/nf-videoguard/src/lib.rs:43-53`) n'ont pas `#[serde(default)]`
     au niveau de la structure. Ajouter un champ sans `#[serde(default)]` ferait échouer la
     lecture des réglages enregistrés, et `api.rs:756-760` (`if let Ok(Some(..))`) retomberait
     **en silence** sur les valeurs par défaut : listes, filtres personnels et exceptions perdus.
     Règle de conception n° 1 de toutes les missions qui suivent.
   - Réglage enregistré **sans interface** : `VideoGuardConfig.min_duration` ; sans effet :
     `PerfConfig.battery_saver` (L-08) ; données sans gestion : zoom par site (`site_zoom`).
3. **Organisation** (§4) : une entrée « Security Center » en tête de la barre latérale (état
   des protections, protections actives, alertes, événements récents), puis les 13 sections
   demandées, puis « À propos ». Les anciennes sections Démarrage, Langues, Mises à jour
   (→ Général), Mots de passe, NIGHTFALL LOCK, Emergency (→ Sécurité), Système (→ Performances
   et Options avancées), Réinitialiser (→ Options avancées) sont absorbées sans perte.
   `security.html` devient une redirection : menu, panneau Shield, Nouvel onglet et liens
   profonds (`#emergency`, `#lock`…) continuent de fonctionner.
4. **Manques** (§6) : 40 lignes Chrome/Brave évaluées ; « demander où enregistrer » existe
   déjà (F-109) ; Navigation sécurisée de Google, saisie automatique, sous-titres instantanés,
   traduction et protection d'empreinte façon Brave sont déclarés non faisables ou non
   recommandés, avec la raison.
5. **Par site** (§7) : vue unifiée par domaine enregistrable, mais chaque donnée garde sa portée
   réelle (origine exacte pour les autorisations, domaine pour Shield et VideoGuard, hôte pour le
   zoom) ; aucune migration destructive ; nouveaux champs tous `#[serde(default)]`, tests de
   chargement d'anciens JSON obligatoires.
6. **Risque important à signaler** (§8, R1) : les exceptions par site écrites dans Chromium par
   `set_content_setting` sont enregistrées par Chromium dans son fichier `Preferences` **en
   clair**, hors de la base chiffrée : la liste de sites deviendrait lisible sans le code.
   Aujourd'hui NIGHTFALL n'y écrit que des valeurs par défaut (sans adresse) et des réponses
   temporaires. Les nouveaux types par site (JavaScript, images, son, cookies…) exigent soit
   une écriture éphémère (déverrouillage → verrouillage/fermeture), soit un refus de ces
   types. Prototype exigé avant la mission concernée ; décision du propriétaire si le
   prototype échoue.
7. **Découpage** (§9) : 9 missions d'exécution, ordonnées, avec fichiers et critères.

## 1. Méthode

- Lecture intégrale de `app/ui/settings.js`, `settings.html`, `settings.css`, `security.js`,
  `security.html`, `app/src/nf/state.rs`, `permissions.rs`, `prefs.rs` (l. 1-140) ; lecture
  ciblée de `api.rs` (dispatch des commandes, `settings_get/set/reset`, `shield_set`,
  `page.open`, chargement au déverrouillage l. 740-790), `security.rs`, `browsing.rs`
  (`save_zoom`), `https_first.rs` (`allow`), `downloads.rs` (`on_before_download`), `app.rs`,
  `updater.rs`, `newtab.js`, `bookmarks.*`, `shell.js` (menu, panneau Shield) et des
  configurations `ShieldConfig`, `VideoGuardConfig`, `PerfConfig`, `AutoPurgeRules`.
- Recoupement avec le registre (lignes F-063, F-066, F-073 à F-119, F-137 à F-156, F-188 à
  F-197, limites L-01 à L-22).
- Fonctions CEF : liaisons cef-rs 156.3.0 (`/home/claude/ref/cef-rs/cef/src/bindings/x86_64_pc_windows_msvc.rs`,
  abrégé **B**, et `sys/src/bindings/x86_64_pc_windows_msvc.rs`, abrégé **S**) et en-têtes C++
  (`/home/claude/chromiumembedded/cef/include`, dépôt CEF du 2026-11-02, plus récent que 156 :
  les signatures sont vérifiées dans cef-rs 156, les en-têtes servent pour la documentation).
  Les noms de préférences Chromium cités ne sont **pas** vérifiés hors ligne : le mécanisme
  existant `prefs.rs:set_one` (`has_preference`, `can_set_preference`) les contrôle à
  l'exécution et affiche « Non appliqué » (F-190) ; chaque mission doit l'exiger.
- Maquettes : `maquettes/build.sh` assemble 4 pages, `maquettes/shoot.js` les rend avec
  Playwright (Chromium sans tête, fichiers locaux, aucun réseau).

## 2. Inventaire de l'existant

### 2.1 Lieux où l'on règle ou agit

| Lieu | Fichiers | Contenu |
|---|---|---|
| Paramètres (13 sections) | `settings.js:SECTIONS` (l. 582-596) | voir 2.2 |
| Security Center | `security.js` (6 cartes) | état, Shield, VideoGuard, sessions/phishing, LOCK, Emergency |
| Menu principal | `shell.js:menuPanel` | Security Center, Paramètres, Verrouiller, Verrouillage d'urgence… |
| Panneau Shield | `shell.js:shieldPanel` | interrupteur par site, lien Security Center |
| Bandeaux | `shell.js:askPermission`, `setWarning`, `insecure.html` | autorisation (Mémoriser), anti-phishing, HTTPS-First « Continuer » |
| Nouvel onglet (engrenage) | `newtab.js` | fond, raccourcis, affichage tuiles/stats/horloge (localStorage, L-14), liens Security Center / Paramètres |
| Favoris | `bookmarks.html:#sync-*` | dossier de synchronisation + phrase |

### 2.2 Table complète : chaque réglage et action → nouvelle place

Colonnes : élément ; stockage ou commande ; lieu actuel ; **nouvelle place** (section › groupe) ;
remarque. « SC » = Security Center intégré. Lignes du registre entre crochets.

**Réglages enregistrés (`Settings`, clé `app` de la base chiffrée)**

| # | Élément | Stockage | Actuel | Nouvelle place | Remarque |
|---|---|---|---|---|---|
| 1 | Moteur de recherche [F-096] | `search_engine` | P › Moteur de recherche | Moteurs de recherche › Moteur par défaut | |
| 2 | Moteur personnalisé [F-097] | `search_custom_name`, `search_custom_url` | idem | Moteurs de recherche › Gérer les moteurs | devient une entrée de la liste ; champs conservés (§7.4) |
| 3 | Onglets verticaux [F-092] | `vertical_tabs` | P › Apparence | Apparence et thèmes › Barre d'outils et onglets | |
| 4 | Verrouillage automatique [F-116] | `auto_lock_minutes` | P › LOCK | Sécurité › NIGHTFALL LOCK | |
| 5 | Verrouiller après mise en veille [F-116] | `lock_on_sleep` | P › LOCK | Sécurité › NIGHTFALL LOCK | |
| 6 | Session Vault [F-139] | `session_vault` | SC seulement | Sécurité › Protection des sessions | état aussi dans SC (lecture + lien) |
| 7 | Reprise de session (ancien) | `restore_session` | dérivé de `startup_mode` | Général › Au démarrage (invisible) | conservé pour les anciens profils (`state.rs:241`) |
| 8 | Outils de développement [F-115] | `devtools_enabled` | P › Système › Développement | Options avancées › Développement | alerte dans SC quand activé |
| 9 | Avertissements anti-phishing [F-102] | `phishing_warnings` | P › Conf. et sécurité **et** SC | Sécurité › Protections de navigation | **doublon supprimé** |
| 10 | Fond d'écran [F-090] | `wallpaper` | P › Apparence + engrenage Nouvel onglet | Apparence et thèmes › Page Nouvel onglet | l'engrenage reste un raccourci vers le même champ |
| 11 | Raccourcis de Nouvel onglet [F-074] | `shortcuts` | page Nouvel onglet | Apparence et thèmes › Page Nouvel onglet (lien) | gestion sur la page |
| 12 | Économiseur de mémoire [F-113] | `perf.discard_enabled`, `perf.discard_after_minutes` | P › Système | Performances › Mémoire | |
| 13 | Économiseur de batterie (L-08) | `perf.battery_saver` | aucun | Performances (non affiché tant qu'il n'a pas d'effet) | jamais affiché comme actif sans effet réel |
| 14 | Auto-Purge [F-142] | `auto_purge` | SC | Sécurité › Urgence | modifiable seulement par `autopurge.set` (min. 2 signaux) |
| 15 | Dossier de synchronisation [F-166] | `sync_folder` | page Favoris | Général › Synchronisation | bouton « Synchroniser » gardé sur la page Favoris |
| 16 | Démarrage [F-089] | `startup_mode`, `startup_pages` | P › Démarrage | Général › Au démarrage | + « Utiliser les pages ouvertes » (§6) |
| 17 | Bouton Accueil [F-091] | `show_home_button` | P › Apparence | Apparence et thèmes › Barre d'outils et onglets | |
| 18 | Zoom par défaut [F-095] | `default_zoom` | P › Apparence | Apparence et thèmes › Pages web | Navigation › Zoom renvoie ici |
| 19 | Taille de police [F-094] | `font_size` | P › Apparence | Apparence et thèmes › Pages web | Accessibilité renvoie ici |
| 20 | Cookies tiers [F-099] | `block_third_party_cookies` | P › Conf. et sécurité | Confidentialité › Cookies et données | sélecteur à 3 états (§7.4) |
| 21 | Global Privacy Control [F-100] | `global_privacy_control` | idem | Confidentialité › Suivi | |
| 22 | HTTPS-First [F-101] | `https_first` | idem | Sécurité › Connexions | |
| 23 | Protection IP WebRTC [F-103] | `webrtc_ip_protection` | idem | Confidentialité › Suivi | |
| 24 | DNS sécurisé [F-104] | `secure_dns`, `secure_dns_custom` | idem | Sécurité › Connexions | |
| 25 | Autorisations par défaut [F-105] | `permission_defaults` | P › Autorisations | Autorisations des sites › Par type | |
| 26 | Décisions mémorisées [F-106] | `site_permissions` | P › Autorisations | Autorisations des sites › Par site + fiche du site | |
| 27 | Réponses temporaires [F-107] | `permission_resets` | interne | interne (inchangé) | effacées au verrouillage |
| 28 | Dossier de téléchargement [F-109, F-162] | `download_dir` | P › Téléchargements | Téléchargements | |
| 29 | Demander où enregistrer [F-109, F-157] | `download_ask` | idem | Téléchargements | |
| 30 | Langues préférées [F-110] | `accept_languages` | P › Langues | Général › Langues | |
| 31 | Correcteur orthographique [F-111] | `spellcheck` | P › Langues | Général › Langues | indisponible sur Windows (L-05), dit tel quel |

**Préférences lisibles avant déverrouillage (`UiPrefs`, fichier `ui_prefs`)**

| # | Élément | Stockage | Actuel | Nouvelle place |
|---|---|---|---|---|
| 32 | Animation de démarrage [F-093] | `splash_enabled` | P › Apparence | Apparence et thèmes › Animations |
| 33 | Accélération matérielle [F-112, F-179] | `hardware_acceleration` | P › Système | Performances › Système (badge « Redémarrage nécessaire » conservé) |

**Shield (`ShieldConfig`, clé `shield`) et statistiques**

| # | Élément | Stockage | Actuel | Nouvelle place |
|---|---|---|---|---|
| 34 | Activer Shield [F-151] | `enabled` | SC | NIGHTFALL Shield › en tête (état dans SC) |
| 35 | Bloquer les pop-ups [F-152] | `block_popups` | SC | NIGHTFALL Shield › Protection ; exception par site dans la fiche du site |
| 36 | Listes : activation, date, mise à jour [F-147] | `lists`, `shield.update` | SC | NIGHTFALL Shield › Listes de filtres |
| 37 | Filtres personnalisés [F-150] | `custom_filters` | SC | NIGHTFALL Shield › Avancé |
| 38 | Exceptions (Shield désactivé) [F-151] | `allowlist` | SC + panneau Shield | NIGHTFALL Shield › Exceptions + fiche du site ; panneau inchangé |
| 39 | Total bloqué | clé `shield_total` | Nouvel onglet | SC (tuile) + Nouvel onglet |
| 40 | Statistiques de session, top 8 [F-153] | `ShieldStats` (mémoire) | SC | NIGHTFALL Shield › Statistiques ; résumé dans SC |

**VideoGuard (`VideoGuardConfig`, clé `videoguard`)**

| # | Élément | Stockage | Actuel | Nouvelle place |
|---|---|---|---|---|
| 41 | Activer VideoGuard [F-188] | `enabled` | SC | VideoGuard › en tête |
| 42 | Action par catégorie [F-154] | `actions` | SC | VideoGuard › Catégories |
| 43 | Durée minimale | `min_duration` | **aucune interface** | VideoGuard › Comportement (VG-17 : 0 à 30 s) |
| 44 | Accélérer les publicités vidéo [F-156] | `skip_video_ads` | SC | VideoGuard › Comportement |
| 45 | Attribution SponsorBlock | `ATTRIBUTION` | SC | VideoGuard › À propos des données + page À propos (VG-62) |

**Autres données enregistrées**

| # | Élément | Stockage | Actuel | Nouvelle place |
|---|---|---|---|---|
| 46 | Recherche auto. des mises à jour [F-191] | clé `updates.auto_check` | P › Mises à jour | Général › Mises à jour |
| 47 | Installation auto. au redémarrage [F-191] | clé `updates.auto_install` | idem | Général › Mises à jour |
| 48 | Zoom par site [F-038] | clé `site_zoom` (hôte → niveau) | aucune gestion | Navigation › Zoom par site (liste, retrait) + fiche du site |
| 49 | Exceptions HTTPS-First de la session [F-037] | `https_first::ALLOWED` (mémoire) | aucune | fiche du site (« Révoquer ») + événement |
| 50 | Affichage tuiles / stats / horloge [F-075] | localStorage `nf.newtab.*` | engrenage Nouvel onglet | inchangé (L-14) ; migration facultative (M-E8) |
| 51 | Sites les plus visités masqués | localStorage `nf.newtab.hiddenTopSites` | Nouvel onglet | inchangé |
| 52 | Code, Recovery Key, compteur d'échecs [F-117, F-123] | nf-auth | P › LOCK + SC | Sécurité › NIGHTFALL LOCK |

**Actions**

| # | Action | Commande | Actuel | Nouvelle place |
|---|---|---|---|---|
| A1 | Effacer les données de navigation [F-098] | `browsing.clear` | P › Conf. et sécurité | Confidentialité (en tête) |
| A2 | Retirer une décision / toutes [F-106] | `permissions.forget` | P › Autorisations | Autorisations des sites + fiche du site |
| A3 | Choisir / rétablir le dossier [F-109] | `downloads.chooseFolder`, `downloads.resetFolder` | P › Téléchargements | Téléchargements |
| A4 | Ouvrir la page Téléchargements | `page.open` | P › Téléchargements | Téléchargements (lien) |
| A5 | Paramètres proxy du système [F-114] | `system.proxySettings` | P › Système | Options avancées › Réseau |
| A6 | Changer de code [F-117] | `lock.changePin` | P › LOCK **et** SC | Sécurité › NIGHTFALL LOCK (**doublon supprimé**) |
| A7 | Nouvelle Recovery Key [F-117] | `lock.newRecovery` | P › LOCK **et** SC | Sécurité › NIGHTFALL LOCK (**doublon supprimé**) |
| A8 | Verrouillage d'urgence [F-140] | `emergency.lockdown` | SC + menu + raccourci | Sécurité › Urgence + bouton du SC ; menu et raccourci inchangés |
| A9 | Purge des données [F-141] | `emergency.purge` | SC | Sécurité › Urgence (code + « PURGER » inchangés) |
| A10 | Règles Auto-Purge [F-142] | `autopurge.set` | SC | Sécurité › Urgence |
| A11 | Mettre à jour les listes [F-147] | `shield.update` | SC | NIGHTFALL Shield + alerte du SC |
| A12 | Shield pour ce site [F-066] | `shield.toggleSite` | panneau Shield | panneau (inchangé) + fiche du site |
| A13 | Enregistrer Shield [F-150, F-151] | `shield.set` | SC | NIGHTFALL Shield (passe en modification ciblée, §5.2) |
| A14 | Enregistrer VideoGuard | `videoguard.set` | SC | VideoGuard |
| A15 | Réinitialiser les paramètres [F-118] | `settings.reset` | P › Réinitialiser | Options avancées › Réinitialiser |
| A16 | Rechercher / installer une mise à jour, notes de version [F-175, F-177, F-192] | `update.check`, `update.install`, `update.status` | P › Mises à jour | Général › Mises à jour (+ alerte SC) |
| A17 | Gestionnaire de mots de passe [F-108] | `page.open passwords` | P › Mots de passe | Sécurité › Mots de passe |
| A18 | Information « Enregistrement des mots de passe » (L-13) | texte | P › Mots de passe | Sécurité › Mots de passe |
| A19 | Raccourcis / stats / horloge de Nouvel onglet | `page.open newtab` | P › Apparence | Apparence et thèmes › Page Nouvel onglet |
| A20 | À propos, composants et licences [F-119] | `page.open about` | P › À propos | À propos de NIGHTFALL (bas de barre latérale) |
| A21 | Lien « NIGHTFALL Shield » (vers SC) | `page.open security` | P › Conf. et sécurité | supprimé : remplacé par la ligne « Blocage des traqueurs » → section Shield (même page) |
| A22 | Lien « Emergency System » (vers SC) | `page.open security#emergency` | P › LOCK | supprimé : Urgence est dans la même section Sécurité |
| A23 | État des modules [F-076] | `security.status` | SC | Security Center › Protections actives |
| A24 | Signaux de sécurité [F-130, F-143] | `signals` | SC (texte) | SC › alertes + Événements de sécurité |
| A25 | Sites les plus bloqués [F-153] | `stats.per_site` | SC | NIGHTFALL Shield › Statistiques |
| A26 | Recherche dans les paramètres, `/`, Échap, flèches [F-086, F-196] | `settings.js` | P | conservé ; indexe aussi Shield, VideoGuard, Sécurité, fiches de site |
| A27 | Liens profonds `#id` [F-087] | `hashchange`, `page.open` | P | conservé + table d'alias (§5.3) |
| A28 | « Non appliqué : … » [F-190] | `markPrefs` | P | conservé pour toute préférence Chromium, anciennes et nouvelles |
| A29 | Ouvrir le Security Center (menu, panneau Shield, Nouvel onglet) [F-063, F-066] | `openPage('security')` | 3 points d'entrée | `security.html` redirige vers `settings.html#security-center` |
| A30 | Synchroniser [F-166] | `sync.run` | page Favoris | page Favoris (action) ; configuration dans Général › Synchronisation |
| A31 | Bandeau d'autorisation « Mémoriser » [F-049] | `permission.answer` | barre d'outils | inchangé ; écrit dans le modèle §7 |
| A32 | « Continuer vers le site » HTTPS-First [F-037] | `https.proceed` | `insecure.html` | inchangé ; visible et révocable dans la fiche du site |
| A33 | « Quitter ce site » anti-phishing [F-193] | `nav.back` | bandeau | inchangé ; journalisé (événement) |
| A34 | Verrouiller [F-060, F-125] | `lock` | barre d'outils, menu | inchangé |

Recoupement : les 31 champs de `Settings` (`state.rs:38-106`), les 2 de `UiPrefs`, les 5 de
`ShieldConfig`, les 4 de `VideoGuardConfig`, les 3 de `PerfConfig`, les 3 d'`AutoPurgeRules`
(ligne 14), les 2 d'`UpdatePrefs` et les clés `site_zoom`, `shield_total` sont tous dans la
table ; les 13 sections de `settings.js` et les 6 cartes de `security.js` aussi.

## 3. Constats sur l'existant (lecture du code)

| # | Constat | Preuve | Effet sur la conception |
|---|---|---|---|
| C1 | Doublons Paramètres / Security Center | `settings.js:333` et `security.js:91` (phishing) ; `settings.js:463-488` et `security.js:96-104` (code, Recovery Key) | une seule place (§2.2 A6, A7, n° 9) |
| C2 | Enregistrement par objet entier, copie possiblement périmée | `settings.js:64-71` envoie `S` ; `api.rs:993-1006` remplace `c.settings` ; seuls `auto_purge`, `site_permissions`, `permission_resets`, `download_dir` sont protégés | commande `settings.patch` (§5.2) |
| C3 | Shield enregistré par objet entier | `security.js:26-33` reconstruit la configuration depuis l'instantané `ST` | `shield.patch` ciblé |
| C4 | `ShieldConfig`, `VideoGuardConfig` sans `#[serde(default)]` de structure ; échec de lecture silencieux | `nf-shield/src/lib.rs:43`, `nf-videoguard/src/lib.rs:43`, `api.rs:756-760` | règle §7.3 |
| C5 | `min_duration` sans interface | `nf-videoguard/src/lib.rs:48`, absent de `security.js` | ajouté (n° 43) |
| C6 | Zoom par site enregistré mais non gérable | `browsing.rs:92-117` | Navigation › Zoom par site |
| C7 | `page.open` n'accepte qu'un `hash` alphanumérique avec « - », 40 caractères | `api.rs:208` | nouvelle commande `settings.openSite` (§5.3) |
| C8 | Aucun journal d'événements de sécurité : seuls les signaux du démarrage existent | `security.rs:status_json`, `state.rs:Core.signals` | modèle §5.4 |
| C9 | Les décisions mémorisées sont appliquées par NIGHTFALL dans le gestionnaire d'autorisations, sans écrire le site dans Chromium ; seules les valeurs par défaut (sans adresse) et les réponses temporaires le sont | `permissions.rs:decide`, `apply_defaults`, `answer` | risque R1 pour les nouveaux types |
| C10 | Valeur par défaut `wallpaper: "city"` inconnue des pages (repli sur « Aurore ») | `state.rs:159`, `newtab.js:4-11` | hors périmètre, signalé |

## 4. Arborescence cible

```
Paramètres (settings.html)
├─ Security Center                      #security-center   (page d'accueil quand on vient du menu « Security Center »)
│   ├─ Alertes (listes périmées, mise à jour prête, signal au démarrage, pas de puce, outils de dév. activés)
│   ├─ État global : n protections actives / total, « Vérification de sécurité », « Verrouillage d'urgence »
│   ├─ Aujourd'hui : éléments bloqués, segments sautés, événements
│   ├─ Protections actives (10 lignes, chacune ouvre sa section) : LOCK, Shield, VideoGuard, Session Vault,
│   │   HTTPS-First, Anti-phishing, DNS sécurisé, Cookies/GPC/WebRTC, Sandbox (fixe), Auto-Purge
│   └─ Événements récents (3) → Sécurité › Événements de sécurité
├─ Général                              #general
│   ├─ Au démarrage (Nouvel onglet / reprendre / pages choisies + « Utiliser les pages ouvertes »)
│   ├─ Navigateur par défaut (nouveau, §6)
│   ├─ Langues (langues des pages, correcteur, langue de NIGHTFALL : français)
│   ├─ Importer et exporter (nouveau : favoris HTML, mots de passe CSV)
│   ├─ Synchronisation (dossier ; action sur la page Favoris)
│   └─ Mises à jour (état, rechercher, installer, notes, signée, 2 interrupteurs)
├─ Apparence et thèmes                  #apparence
│   ├─ Thème (éditeur de thèmes de M-0005 : prédéfinis, créer, dupliquer, aperçu)
│   ├─ Barre d'outils et onglets (bouton Accueil, onglets verticaux, barre de favoris — nouveau)
│   ├─ Page Nouvel onglet (fond, lien raccourcis/stats/horloge)
│   ├─ Pages web (taille de police, polices — nouveau, zoom par défaut)
│   └─ Animations (animation de démarrage, réduire les animations → Accessibilité)
├─ Confidentialité                      #confidentialite
│   ├─ Effacer les données de navigation ; Effacer à la fermeture (nouveau)
│   ├─ Cookies et données des sites (mode à 3 états, données par site — nouveau, lien exceptions)
│   ├─ Suivi (GPC, WebRTC, ligne d'état « Blocage des traqueurs » → Shield)
│   └─ Préchargement (nouveau, désactivé par défaut)
├─ Sécurité                             #securite
│   ├─ NIGHTFALL LOCK (délai, veille, changer de code, Recovery Key, protection du code : TPM/DPAPI/Trousseau)
│   ├─ Connexions (HTTPS-First, DNS sécurisé, Gérer les certificats — nouveau)
│   ├─ Protections de navigation (anti-phishing ; note honnête Navigation sécurisée indisponible)
│   ├─ Protection des sessions (Session Vault + limite)
│   ├─ Mots de passe (gestionnaire, enregistrement L-13, réutilisation L-10 — nouveau)
│   ├─ Événements de sécurité          #securite/evenements
│   └─ Urgence (niveau 1 lockdown, niveau 2 purge, niveau 3 Auto-Purge)
├─ NIGHTFALL Shield                     #shield
│   ├─ Activer, Pop-ups, niveau par défaut (Standard / Strict — nouveau, M-0003 P2-7)
│   ├─ Listes de filtres (activation, dates, mettre à jour)
│   ├─ Exceptions (sites, réglage fin → fiche du site)
│   ├─ Statistiques (session, total, top sites)
│   └─ Avancé (filtres personnalisés ; journal des requêtes bloquées — nouveau, M-0003)
├─ VideoGuard                           #videoguard          (contenu détaillé : M-0004 VG-60)
│   ├─ Activer ; Catégories (mode par catégorie) ; Comportement (durée min., avis, accélérer les pubs)
│   ├─ Navigation privée ; Exceptions par site (lecteurs intégrés) ; Statistiques
│   └─ Avancé (serveur, export/import) ; À propos des données (attribution, licence)
├─ Autorisations des sites              #sites
│   ├─ Sites (liste unifiée par domaine, recherche) → Fiche du site   #sites/<id>
│   ├─ Par type (défauts : caméra, micro, position, notifications, presse-papiers, téléchargements multiples,
│   │   fenêtres, polices + nouveaux types §6) ; Pop-ups → lien Shield
│   └─ Décisions mémorisées : tout retirer
├─ Téléchargements                      #telechargements (emplacement, demander, page Téléchargements)
├─ Navigation                           #navigation
│   ├─ Onglets (fermer la fenêtre avec le dernier onglet — comportement actuel affiché, avertir avant de fermer plusieurs onglets — nouveau)
│   ├─ Zoom (par défaut → Apparence ; zoom par site : liste et retrait)
│   └─ Lecture des médias (lecture automatique, son par site → fiche du site) — nouveau
├─ Moteurs de recherche                 #recherche
│   ├─ Moteur par défaut
│   ├─ Gérer les moteurs (liste, mots-clés, ajouter, modifier, supprimer) — nouveau
│   └─ Suggestions (locales : historique, favoris ; en ligne : désactivé par défaut) — nouveau
├─ Performances                         #performances (mémoire + sites toujours actifs, accélération matérielle, préchargement → Confidentialité)
├─ Accessibilité                        #accessibilite (police minimale, focus visible, réduire les animations, contraste élevé, lien police/zoom)
├─ Options avancées                     #avance (Réseau : proxy ; Développement : outils ; Réinitialiser)
└─ À propos de NIGHTFALL                #a-propos
```

Règles d'organisation :
1. **Un réglage, un endroit.** Les autres lieux n'affichent qu'un état avec un lien (le SC, la
   ligne « Blocage des traqueurs ») ou un raccourci vers le même champ (engrenage de Nouvel
   onglet, panneau Shield). Un contrôle affiché à deux endroits lit et écrit la même valeur
   par la même commande.
2. **Configuration dans les Paramètres, actions contextuelles là où l'on est** (Synchroniser
   sur la page Favoris, Shield pour ce site dans le panneau).
3. **Libellés courts, explication en une ligne, l'avancé replié** (« Avancé » en bas de section).
4. **Honnêteté** : ce qui n'existe pas dans CEF est dit dans la section concernée (Navigation
   sécurisée, correcteur Windows, langue d'interface, extensions).

Maquettes (organisation et contenus ; style final : M-0005) :
- `maquettes/1-security-center.png` : vue d'ensemble (alerte, état, protections actives, événements).
- `maquettes/2-section-confidentialite.png` : une section complète.
- `maquettes/3-autorisations-site.png` : fiche d'un site (protections, autorisations, cookies, zoom, portées).
- `maquettes/4-evenements.png` : événements de sécurité (filtres, durée de conservation, effacement).

## 5. Comportements et API internes

### 5.1 Pages
- `settings.js` devient modulaire : un fichier par section (`settings/general.js`…) déclarés
  dans `assets.rs` (piège connu). La recherche indexe toutes les lignes, y compris les sous-pages.
- `security.html` + `security.js` sont conservés comme **redirection** :
  `location.replace('settings.html#security-center')` (et `#emergency` → `#securite/urgence`).
  Le fichier `security.js` reste (registre F-076, F-140…) : son contenu est déplacé, et les
  symboles cités par le registre (`EMERGENCY LOCKDOWN`, `Tapez PURGER`, `Auto-Purge`,
  `Filtres personnalisés`…) se retrouvent dans les nouveaux fichiers : le Directeur met à jour
  la colonne « Où » du registre dans la même PR (accord nécessaire pour `preserve.py`).
- Le menu garde l'entrée « Security Center » (F-063) ; le panneau Shield remplace « Ouvrir le
  Security Center » par « Réglages de ce site » (fiche du site) et garde un lien vers le SC.

### 5.2 Enregistrement sans écrasement
- Nouvelle commande `settings.patch { changes: { "<champ>": valeur, … }, prefs: {…} }` : liste
  blanche des champs modifiables par cette voie ; applique, `sanitize()`, enregistre, applique
  à Chromium ; renvoie la valeur retenue de chaque champ (la page se met à jour). `settings.set`
  reste (compatibilité, Nouvel onglet) mais n'est plus utilisé par les Paramètres.
- `shield.patch` : `enabled`, `block_popups`, `list {id, enabled}`, `custom_filters`,
  `allow {site}`, `disallow {site}`, `site {site, …}` ; ne renvoie jamais les dates de listes.
- `videoguard.set` valide et borne (VG-60).
- Après chaque écriture, un événement `settings_changed {keys}` est poussé à toutes les vues
  ouvertes (`windows::push_event_all`), qui relisent : plus de copie périmée.

### 5.3 Liens profonds
- Table d'alias des anciens identifiants : `demarrage`, `langues`, `mises-a-jour` → `general` ;
  `apparence` inchangé ; `recherche` inchangé ; `confidentialite` inchangé ; `autorisations` →
  `sites` ; `mots-de-passe`, `lock` → `securite` ; `systeme` → `performances` ;
  `reinitialiser` → `avance` ; `a-propos` inchangé. Test : chaque ancien `#id` ouvre la bonne section.
- `settings.openSite { origin }` (nouveau) : valide l'adresse (http/https), calcule l'identifiant
  de la fiche et ouvre `settings.html#sites/<id>` ; `page.open` reste inchangé.

### 5.4 Événements de sécurité
- Données : `SecurityEvent { id, at, kind, severity (info|attention|critique), site: Option<String> (hôte seulement), detail: String, action: Option<String> }`.
- Sources (une ligne de code par point d'émission) : avertissement anti-phishing affiché et
  « Quitter ce site » (`client.rs:on_navigation`, F-193) ; page HTTPS-First affichée et
  « Continuer » (`https_first.rs`, `api.rs:https.proceed`) ; téléchargement dangereux ou
  signature invalide (`downloads.rs:post_process`) ; décision d'autorisation mémorisée ou
  retirée (`permissions.rs:answer`, `permissions.forget`) ; exception Shield ajoutée/retirée ;
  échecs du code avant déverrouillage (nombre fourni par nf-auth, écrit après déverrouillage) ;
  Recovery Key utilisée ; lockdown, purge, évaluation d'Auto-Purge ; signaux du démarrage ;
  échec de vérification d'une mise à jour signée.
- Jamais : adresse complète, requête, contenu ; rien en navigation privée ; rien avant le
  déverrouillage (file mémoire bornée, vidée dans la base au déverrouillage).
- Stockage : table `security_events` dans la base chiffrée (nf-storage, migration ajoutée),
  conservation 7 / 30 / 90 jours (défaut 30), 2 000 lignes au plus, effaçable ; incluse dans
  les catégories `browsing_data` de la purge et de l'Auto-Purge.
- API : `security.events { since, kind, limit }`, `security.events.clear`, réglage
  `security_events_days` dans `Settings`.

### 5.5 Vérification de sécurité (locale, aucun service)
Mise à jour disponible (service existant), listes Shield de plus de 3 jours, mots de passe
réutilisés (`password_reused_elsewhere`, L-10), autorisations accordées à des sites non visités
depuis 90 jours (historique), outils de développement activés, Auto-Purge, secret matériel absent.

## 6. Réglages manquants par rapport à Chrome et Brave

Effort : S (≤ 1 j), M (2-5 j), L (> 1 semaine). Risque : technique ou de régression.
« CEF » : fonction de cef-rs 156 (B = `cef/src/bindings/x86_64_pc_windows_msvc.rs`, ligne).

| # | Réglage | Chrome | Brave | Utile | Faisable / fonction CEF | Effort | Risque |
|---|---|---|---|---|---|---|---|
| 1 | Autorisation JavaScript par site et par défaut | oui | oui | oui | `ImplRequestContext::set_content_setting` (B:11273) avec `ContentSettingTypes::JAVASCRIPT` (S, valeur 2) | M | R1 (fichier Preferences en clair) ; à prouver par auto-test |
| 2 | Images par site | oui | oui | moyen | idem, `IMAGES` (1) | S après n°1 | R1 |
| 3 | Son par site (couper) | oui | oui | oui | `SOUND` (30) ou `ImplBrowserHost::set_audio_muted` (B:12858, déjà utilisé pour F-013) | S | faible si via `set_audio_muted` (aucune écriture dans Chromium) |
| 4 | Lecture automatique | non (drapeau) | oui | oui | `AUTOPLAY` (22) ; à défaut option de lancement `autoplay-policy` | S | à vérifier : Chromium peut ignorer AUTOPLAY hors Chrome style |
| 5 | Contenu non sécurisé (mixte) par site | oui | oui | faible | `MIXEDSCRIPT` (7) | S | sécurité : n'autoriser que par site, jamais par défaut |
| 6 | Capteurs, MIDI, USB/série/HID, gestionnaires de protocoles, détection d'inactivité, accès au stockage tiers, réseau local, verrouillage clavier/pointeur, fichiers | oui | oui | oui (bloquer) | invites : bits `CEF_PERMISSION_TYPE_SENSORS`, `MIDI_SYSEX`, `IDLE_DETECTION`, `REGISTER_PROTOCOL_HANDLER`, `STORAGE_ACCESS`, `LOCAL_NETWORK`, `KEYBOARD_LOCK`, `POINTER_LOCK`, `FILE_SYSTEM_ACCESS` (S) à ajouter à `permissions.rs:KINDS` ; défauts par `set_content_setting` sans adresse (déjà fait pour 8 types) | M | faible : même chemin que l'existant ; les « choosers » USB/série/HID passent par d'autres rappels, non vérifiés |
| 7 | Cookies : mode 3 états (tout autoriser / bloquer tiers / bloquer tiers en privé) | oui | oui | oui | préférence `profile.cookie_controls_mode` 0/1/2, déjà écrite (`prefs.rs:73`) | S | faible |
| 8 | Bloquer tous les cookies ; exceptions par site (autoriser / bloquer / effacer à la fermeture) | oui | oui | oui | défaut : `COOKIES` (0) sans adresse ; « effacer à la fermeture » fait par NIGHTFALL : `ImplCookieManager::delete_cookies` (B:8905) + `Storage.clearDataForOrigin` via `execute_dev_tools_method` (B:12743, déjà dans `browsing_data.rs:190`) | M | fermeture plus lente ; R1 si `COOKIES` par site |
| 9 | Effacer les données à la fermeture (historique, cookies, cache) | oui | oui | oui | `browsing_data::clear` existant, appelé dans `prepare_window_close` avant de quitter | M | ordre avec le Session Vault (sceller puis effacer) ; F-007 |
| 10 | Données des sites : voir et supprimer par site | oui | oui | oui | `visit_all_cookies` (B:8889) ; taille : DevTools `Storage.getUsageAndQuota` (B:12743) | M | faible |
| 11 | Réglages Shield par site (strict, cosmétique, scriptlets, pop-ups) | — | oui | oui | NIGHTFALL (nf-shield) ; aucune fonction CEF | M | compatibilité `ShieldConfig` (§7.3) ; correction M-0003 P2-1 (exception appliquée au cadre) |
| 12 | Journal des requêtes bloquées | — | non (uBO oui) | oui | NIGHTFALL (compteur existant `client.rs:blocked_count`) | M | mémoire bornée |
| 13 | Thème, couleurs, mode clair | oui | oui | oui | interface NIGHTFALL : M-0005. `set_chrome_color_scheme` (B:11281) ne concerne pas les vues Alloy de NIGHTFALL | — | M-0005 |
| 14 | Barre de favoris | oui | oui | oui | interface (`shell.*`, hauteur de la vue du haut, `windows.rs`) | M | mise en page des deux plateformes |
| 15 | « Utiliser les pages ouvertes » au démarrage | oui | oui | oui | NIGHTFALL (`tabs`) | S | aucun |
| 16 | Moteurs : liste, mots-clés, plusieurs personnalisés | oui | oui | oui | NIGHTFALL (`omnibox.rs`, `state.rs`) | M | compatibilité `search_custom_*` (§7.4) |
| 17 | Suggestions de recherche du moteur | oui | oui | moyen | requête réseau à chaque frappe vers le moteur : **désactivé par défaut**, consentement explicite | M | vie privée |
| 18 | Taille minimale de police | oui | oui | oui | `BrowserSettings.minimum_font_size` (B:701) ou préférence `webkit.webprefs.minimum_font_size` (non vérifiée) | S | faible |
| 19 | Polices (standard, serif, sans serif, chasse fixe) | oui | oui | moyen | préférences `webkit.webprefs.fonts.*` (non vérifiées) | S | faible |
| 20 | Curseur de navigation au clavier | oui | oui | moyen | non trouvé dans CEF ; à vérifier (préférence ou commande Chromium) | S/? | inconnu |
| 21 | Focus visible, réduire les animations, contraste élevé (interface) | oui | oui | oui | interface NIGHTFALL (jetons M-0005) | S | aucun |
| 22 | Lecteurs d'écran | auto | auto | oui | `ImplBrowserHost::set_accessibility_state` (B:12849) si besoin | S | à tester avec NVDA / VoiceOver |
| 23 | Sous-titres instantanés | oui | non | — | **non** : modèle SODA téléchargé chez Google, absent de CEF | — | — |
| 24 | Traduction des pages | oui | oui | — | **non** : service Google ; aucun équivalent local libre et gratuit retenu | — | — |
| 25 | Précharger les pages | oui | oui | moyen | préférence `net.network_prediction_options` (non vérifiée) ; défaut : désactivé | S | vie privée (désactivé) |
| 26 | Demander où enregistrer | oui | oui | — | **existe déjà** (`download_ask`, `downloads.rs:166`, F-109) | — | — |
| 27 | Ouvrir automatiquement certains types | oui | oui | non | non recommandé (sécurité) | — | — |
| 28 | Importer favoris (fichier HTML) et mots de passe (CSV exporté par l'autre navigateur) | oui | oui | oui | `ImplBrowserHost::run_file_dialog` (B:12690) ; analyse en Rust | M | CSV en clair : inviter à le supprimer ; jamais de lecture directe des profils des autres navigateurs (chiffrement lié à l'appareil, droits) |
| 29 | Exporter favoris HTML / mots de passe CSV (code exigé) | oui | oui | oui | idem | S | export en clair : avertissement |
| 30 | Navigation sécurisée (Google Safe Browsing) | oui | oui (proxy) | — | **non** : service absent de CEF, exige une clé d'API Google (interdit : aucune clé) ; dit dans Sécurité ; liste « Sites malveillants » de uBlock déjà active (F-147) | — | — |
| 31 | Navigateur par défaut | oui | oui | oui (navigateur principal) | aucune fonction CEF : Windows = enregistrement ProgID dans l'installateur Inno Setup + ouverture de `ms-settings:defaultapps` ; macOS = `LSSetDefaultHandlerForURLScheme` | M | installateur (F-180) |
| 32 | Avertir avant de fermer plusieurs onglets | non | oui | moyen | interface NIGHTFALL | S | F-007 |
| 33 | Confirmation « Quitter la page ? » (L-03) | oui | oui | oui | `on_before_unload_dialog` (B:20227) | M | fermeture d'onglet (bug signalé par le propriétaire) : à traiter avec M-0002 |
| 34 | Zoom par site : liste et retrait | oui | oui | oui | données existantes (`site_zoom`) | S | aucun |
| 35 | Sites toujours actifs (économiseur de mémoire) | oui | oui | oui | nf-perf (`tabs_to_discard`) | S | aucun |
| 36 | Économiseur d'énergie (L-08) | oui | oui | moyen | pas d'API CEF pour limiter la fréquence d'images des navigateurs fenêtrés : **non** pour l'instant ; ne pas afficher | — | — |
| 37 | Politique WebRTC à 4 valeurs | oui (stratégie) | oui | moyen | préférence `webrtc.ip_handling_policy` déjà écrite (`prefs.rs:75`) | S | faible |
| 38 | Proxy propre à NIGHTFALL | non | non | faible | préférence `proxy` (dictionnaire) via `set_preference` (B:10787), non vérifiée | M | réseau |
| 39 | Gérer les certificats | oui | oui | moyen | ouvrir le gestionnaire du système (`certmgr.msc`, Trousseau) comme `system.proxySettings` | S | aucun |
| 40 | Révocation des autorisations des sites inutilisés ; vérification de sécurité | oui | non | oui | NIGHTFALL (historique) §5.5 | M | aucun |
| 41 | Protection d'empreinte (farbling Brave) | non | oui | — | **non recommandé** : pas dans Chromium ; scripts injectés détectables et fragiles | — | — |
| 42 | Saisie automatique adresses / cartes | oui | oui | — | **non** : non fournie aux navigateurs de style Alloy (non vérifié en détail) | — | — |
| 43 | Raccourcis clavier configurables | non | non | moyen | NIGHTFALL (`client.rs:on_pre_key_event`) ; lié à VG-44 et au bug H1 | M | sécurité (verrouillage) |
| 44 | Tor, IPFS, portefeuille, récompenses | — | oui | non | hors objectifs | — | — |

## 7. Autorisations et exceptions par site

### 7.1 Portées (faits)
| Donnée | Clé | Portée | Source |
|---|---|---|---|
| Décision d'autorisation | `Settings.site_permissions` | origine exacte `https://a.b.example` | `permissions.rs:normalize_origin` |
| Shield désactivé | `ShieldConfig.allowlist` | domaine enregistrable (`psl`) | `nf-shield:site_of` |
| Zoom | `site_zoom` | hôte | `browsing.rs:host_of` |
| Exception HTTPS-First | mémoire | hôte, session | `https_first.rs:66-70` |

### 7.2 Modèle cible
Choix : **conserver chaque structure dans sa portée** et ajouter une vue unifiée, plutôt qu'un
nouveau magasin unique (option B ci-dessous).

| Option | Description | Avantages | Risques |
|---|---|---|---|
| A | Magasin unique `sites` par domaine, migration de tout | un seul modèle | élargit une autorisation d'origine à tout le domaine (sécurité) ; migration irréversible ; double écriture pour garder le retour arrière |
| **B (retenue)** | structures existantes inchangées + nouveaux champs à côté + agrégation en lecture | aucune migration, aucune donnée réécrite, retour à l'ancienne version sans perte des anciennes données | une agrégation à écrire et tester |

Nouveaux champs (tous `#[serde(default)]`) :
- `SitePermission.value: Option<String>` (`"allow" | "block" | "session"`) ; `allow` reste
  écrit (`allow = value != "block"`) pour qu'une ancienne version comprenne. Lecture : `value`
  s'il existe, sinon `allow`. Nouveaux `kind` : `javascript`, `images`, `sound`, `autoplay`,
  `cookies`, `insecure_content`, `sensors`, `midi`, `idle_detection`, `protocol_handlers`,
  `storage_access`, `local_network`, `keyboard_lock`, `pointer_lock`, `file_system`.
  `sanitize` refuse un `kind` inconnu sauf ceux-là ; une ancienne version qui lirait un `kind`
  inconnu l'ignore déjà dans `decide` (aucune correspondance).
- `ShieldConfig.site_overrides: BTreeMap<String /*domaine*/, SiteShield { level: Option<String> /*standard|strict*/, cosmetic: Option<bool>, scriptlets: Option<bool>, popups: Option<bool> }>` ;
  `ShieldConfig.default_level: String` (défaut `"standard"`). `allowlist` reste **la** source
  du « Shield désactivé » ; `is_active_for` inchangé.
- `VideoGuardConfig.disabled_sites: BTreeSet<String>` (domaine du site qui intègre le lecteur) ;
  chaîne : seulement si l'identifiant de chaîne est disponible sans dépendre du DOM (VG-41).
- `Settings.cookie_mode: String` (défaut dérivé de `block_third_party_cookies`), `clear_on_exit: Vec<String>`
  (catégories), `security_events_days: u32` (30), `zoom` inchangé.
- Exceptions HTTPS-First : restent de session (comme aujourd'hui) ; affichées et révocables.

Vue unifiée : `sites.list { query }` → `[{ site, origins[], permissions[], shield, videoguard, zoom, cookies, lastVisit }]`
groupées par domaine enregistrable ; `sites.get { site }` ; `sites.set { site|origin, key, value }`
qui écrit dans la bonne structure ; `sites.reset { site }` (retire décisions, exceptions,
zoom ; efface les données seulement si demandé). L'interface affiche la portée de chaque ligne.

### 7.3 Compatibilité avec les réglages enregistrés (exigences)
1. Ajouter `#[serde(default)]` au niveau de `ShieldConfig` et `VideoGuardConfig` (avec `impl Default`
   déjà présentes) **avant** tout nouveau champ ; tests : JSON d'une 0.1.4 (au moins
   `{"enabled":true,"block_popups":true,"lists":[…],"custom_filters":"x","allowlist":["a.fr"]}`
   et `nf-videoguard/src/lib.rs:282`) → chargés à l'identique.
2. Remplacer les `if let Ok(Some(..))` de `api.rs:753-760` par une lecture qui **journalise**
   l'échec et conserve la valeur brute (clé `<nom>.illisible`) au lieu de l'écraser à la
   prochaine sauvegarde.
3. Aucun champ renommé ni supprimé ; `search_custom_*`, `block_third_party_cookies`,
   `restore_session` gardés et tenus à jour à côté des nouveaux.
4. `settings_set`/`settings.patch` protègent aussi les nouveaux champs gérés par leurs propres
   commandes (`site_permissions`, `site_overrides`…).
5. `settings.reset` : comportement F-118 inchangé ; les nouveaux réglages par site suivent la
   même règle (autorisations oubliées, Shield et VideoGuard conservés).
6. Retour à une version antérieure : les champs inconnus sont ignorés (aucun
   `deny_unknown_fields`) mais perdus à la première sauvegarde : à écrire dans les notes de version.

### 7.4 Application dans Chromium
- Autorisations à invite (n° 6 du §6) : décidées par NIGHTFALL dans `on_show_permission_prompt`,
  sans écrire le site dans Chromium (comme aujourd'hui).
- Types sans invite (JavaScript, images, son, lecture auto, cookies, contenu mixte) : Chromium
  doit connaître la règle → `set_content_setting(url, url, type, valeur)` au déverrouillage,
  et retour à `DEFAULT` au verrouillage et à la fermeture, pour limiter la présence de la liste
  de sites dans `Preferences` (R1). Son : préférer `set_audio_muted` (aucune écriture).

## 8. Risques

| # | Risque | Niveau | Mesure |
|---|---|---|---|
| R1 | Exceptions par site écrites en clair par Chromium (`Preferences`) ; un arrêt brutal les laisse sur le disque | **important** (vie privée) | prototype : écrire, verrouiller, lire `Preferences` ; si la suppression n'est pas fiable, ne proposer que des défauts globaux pour ces types ; **décision du propriétaire** |
| R2 | Perte silencieuse des réglages Shield/VideoGuard en ajoutant un champ | important | §7.3, premier lot (M-E1) |
| R3 | Écrasement par copie périmée | moyen | `settings.patch`, événement `settings_changed` |
| R4 | Registre : symboles déplacés (`security.js`) font échouer `preserve.py` | moyen | mise à jour du registre par le Directeur dans la même PR |
| R5 | Content settings JAVASCRIPT/IMAGES/AUTOPLAY respectés par le style Alloy de CEF 156 : probable (moteur de rendu Chrome, `libcef/renderer/chrome`) mais **non vérifié** | moyen | auto-test qui bloque JavaScript sur un site et constate `noscript` |
| R6 | Effacer à la fermeture rallonge la fermeture (F-007) | faible | délai borné, journal |

## 9. Découpage en missions d'exécution

Ordre : M-E1 → M-E2 → (M-E3, M-E4, M-E5 en parallèle, fichiers distincts) → M-E6 → M-E7 → M-E8, M-E9.
Toutes : `preserve.py` vert, CI complète verte, auto-test fini sur example.com avec `SELFTEST OK`,
captures de l'interface simulée (`ci/ui_mock_server.py`) jointes, aucun réglage existant perdu.

| Mission | Contenu | Fichiers | Critères d'acceptation vérifiables |
|---|---|---|---|
| **M-E1** Fondations de compatibilité (Sonnet ; Opus en audit) | `#[serde(default)]` de structure (Shield, VideoGuard) ; lecture tolérante (§7.3-2) ; `settings.patch`, `shield.patch` ; événement `settings_changed` | `crates/nf-shield/src/lib.rs`, `crates/nf-videoguard/src/lib.rs`, `app/src/nf/api.rs`, `state.rs` | tests Rust : JSON 0.1.4 de Shield et VideoGuard chargés à l'identique ; JSON illisible → valeur conservée et ligne de journal ; `settings.patch` d'un champ ne modifie aucun autre champ (test sur tous les champs) ; champ hors liste blanche refusé ; auto-test : patch puis relecture |
| **M-E2** Nouvelle coque des Paramètres (Sonnet) | barre latérale cible, sections en fichiers séparés, déplacement **sans changement de comportement** de toutes les lignes §2.2, suppression des doublons, alias de liens profonds, `security.html` → redirection, menu et panneau Shield | `app/ui/settings*`, `app/ui/settings/*.js`, `security.*`, `shell.js`, `newtab.js`, `app/src/nf/assets.rs` | test Playwright : chaque ligne de §2.2 (nos 1-52, A1-A34) trouvée dans sa nouvelle section ; chaque ancien `#id` ouvre la bonne section ; `security.html#emergency` → Urgence ; recherche « code », « phishing » : une seule ligne chacune ; F-086, F-087, F-190, F-196 vérifiés ; registre mis à jour (colonne « Où ») |
| **M-E3** Security Center intégré (Opus : sécurité) | vue d'ensemble, alertes, vérification de sécurité (§5.5), événements (§5.4) avec table chiffrée, filtres, conservation, effacement, purge | `app/ui/settings/security-center.js`, `crates/nf-storage`, `app/src/nf/security.rs`, points d'émission (`client.rs`, `https_first.rs`, `downloads.rs`, `permissions.rs`, `api.rs`) | tests Rust : stockage, conservation, borne de 2 000, aucune URL complète (test sur une adresse avec requête) ; rien en privé ; purge `browsing_data` vide la table ; auto-test : avertissement HTTPS-First → 1 événement lu par `security.events` |
| **M-E4** Autorisations et fiche du site (Opus) | `sites.*`, `settings.openSite`, fiche du site, nouveaux types à invite (§6 n° 6), son via `set_audio_muted`, zoom par site | `app/src/nf/permissions.rs`, `api.rs`, `browsing.rs`, `app/ui/settings/sites.js` | tests Rust : agrégation (une origine et un domaine affichés avec leur portée) ; `SitePermission` ancien sans `value` → même décision ; `decide` inchangé pour les 8 anciens types (tests existants verts) ; nouveaux bits traduits ; auto-test : bloquer la position d'un site par `sites.set` → refus sans invite |
| **M-E5** Shield et VideoGuard par site (Sonnet ; dépend de M-0003/M-0004) | `site_overrides`, niveau strict, cosmétique/scriptlets/pop-ups par site, exceptions VideoGuard, section VideoGuard (VG-60), `min_duration` exposé | `crates/nf-shield`, `crates/nf-videoguard`, `client.rs`, `app/ui/settings/shield.js`, `videoguard.js` | tests Rust : cosmétique désactivée par site → aucune feuille injectée ; ancien JSON sans `site_overrides` → comportement identique ; `min_duration` borné 0-30 ; auto-test Shield existant vert |
| **M-E6** Prototype R1 puis types sans invite (Opus) | prototype : `set_content_setting(JAVASCRIPT)` par site, verrouillage, lecture de `Preferences` ; si fiable : JavaScript, images, lecture auto, cookies par site | `permissions.rs`, `prefs.rs`, `selftest.rs` | auto-test : JavaScript bloqué pour un site local → script non exécuté ; après verrouillage et fermeture, `Preferences` ne contient pas l'hôte (lecture du fichier dans l'auto-test) ; sinon rapport « non fiable » et arrêt pour décision |
| **M-E7** Confidentialité (Sonnet) | mode cookies à 3 états, effacer à la fermeture, données par site, préchargement, WebRTC à 4 valeurs | `prefs.rs`, `browsing_data.rs`, `windows.rs`, `app/ui/settings/privacy.js` | tests Rust des préférences (`preferences_follow_settings` étendu) ; auto-test : effacer à la fermeture des cookies → cookie absent au relancement simulé ; « Non appliqué » affiché si Chromium refuse |
| **M-E8** Général, Recherche, Accessibilité, Navigation (Sonnet) | « pages ouvertes », moteurs avec mots-clés (migration douce de `search_custom_*`), suggestions en ligne désactivées par défaut, police minimale, polices, accessibilité de l'interface, avertir avant de fermer plusieurs onglets, sites toujours actifs, certificats, migration facultative des préférences de Nouvel onglet | `state.rs`, `omnibox.rs`, `prefs.rs`, `nf-perf`, `app/ui/settings/*.js` | tests Rust : ancien `search_custom_url` → moteur de la liste, même URL de recherche ; mot-clé `w chat` → recherche Wikipédia ; aucune requête de suggestion sans consentement (test) |
| **M-E9** Import/export et navigateur par défaut (Sonnet) | import HTML de favoris, CSV de mots de passe, export, navigateur par défaut Windows/macOS | `nf-storage`, `nf-vault`, `api.rs`, `packaging/windows/nightfall.iss`, `mac.rs` | tests Rust : fichiers d'exemple de Chrome, Firefox, Brave importés ; CSV mal formé refusé sans import partiel ; export exige le code ; CI : installateur déclare le ProgID |

## 10. Décisions demandées au propriétaire (via le Directeur)
1. R1 : accepter que des exceptions par site (JavaScript, images, cookies…) soient écrites
   temporairement dans les fichiers de Chromium, ou limiter ces types à des réglages globaux.
2. Suggestions de recherche en ligne : les proposer (désactivées par défaut) ou jamais.
3. Navigateur par défaut et import/export CSV : à inclure dans la 0.1.5 ou plus tard.
4. Durée de conservation par défaut des événements de sécurité (proposé : 30 jours).

## 11. Hors périmètre (vu, non traité)
- `Settings::default().wallpaper = "city"` n'existe plus dans les pages (`newtab.js:4-11`) :
  repli sur « Aurore » ; valeur par défaut à aligner.
- `security.js` n'a aucune étiquette accessible sur ses cases (`sw`) ; repris de toute façon par M-E2/M-E3.
