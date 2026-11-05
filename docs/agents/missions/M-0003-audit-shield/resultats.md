# M-0003 : résultats de l'audit de NIGHTFALL Shield

- **Auteur** : Optimiseur (Opus), branche `agent/M-0003-audit-shield`, PR brouillon #23.
- **Date** : 2026-11-04.
- **Nature** : audit seulement. Aucun fichier du navigateur (`app/`, `crates/nf-shield/src/`) n'a été modifié.
- **Règle de lecture** : chaque chiffre vient d'un run de CI cité (sorties brutes dans `mesures/`) ou d'une commande donnée ici.
  Ce qui n'a pas été mesuré est marqué **NON MESURÉ**.

## 0. Résumé

1. **Le moteur n'est pas le problème.** Avec les mêmes 8 listes, adblock-rust 0.13.5 (version résolue en CI) et le
   moteur réseau de uBlock Origin prennent la même décision sur 99,85 % des 227 054 requêtes vérifiables du jeu
   d'adblock-rust. On monte à 99,98 % une fois les `!#include` résolus. Sur l'exploration réelle, l'accord est de 100 % aux runs
   41845086446 et 41848082350, et de 99,86 % au run 41850060618 : les 6 écarts sont un artefact de mon harnais, qui
   ne transmet pas la méthode HTTP à uBO (§6.2). Remplacer adblock-rust n'apporterait aucun gain mesurable. Ses alternatives sont
   sous GPL (uBO, tsurlfilter) et écrites en JavaScript ou TypeScript.
2. **Les écarts viennent de l'intégration dans NIGHTFALL.** Sept constats sont mesurés ou exécutés :
   - la liste « uBlock filters » est chargée sans ses 10 `!#include` : 18 024 lignes de règles manquent, dont au
     plus environ 16 568 seraient réellement actives sur Chromium (§4.1) ;
   - les blocs `!#if` sont ignorés : 191 règles prévues pour d'autres environnements sont appliquées ;
   - aucune ressource `$redirect` : 501 règles annulent la requête au lieu de servir un substitut ;
   - `$removeparam` (216 règles), `$csp` (35), `$document` (3 695) et la partie document de `$all` (1 285) sont
     sans effet ;
   - `$popup` est rejeté par le moteur (3 764 règles) ;
   - 512 règles de scriptlet (15,7 %) appellent un des 37 scriptlets que NIGHTFALL ne fournit pas ;
   - l'interface affiche « 164 012 règles » alors que le moteur en accepte 159 585.
3. **Les performances sont saines, avec deux exceptions.** Une décision réseau prend en médiane 5,0 à 8,0 µs
   (p99 21 à 43 µs) selon la plateforme et le run (3 runs). Les deux points coûteux :
   - le moteur est reconstruit en 78 à 140 ms (médianes de `Shield::new`, 3 runs), de façon synchrone, deux fois au démarrage, et l'appel lié au
     déverrouillage tourne sur le fil d'E/S de CEF ;
   - chaque cadre reçoit un script de 28,6 ko (médiane), qui contient 531 sélecteurs génériques identiques sur
     toutes les pages.
4. **Recommandation : garder adblock-rust et corriger l'intégration** (§7). En tête :
   - résoudre `!#include` et `!#if`, valider les listes téléchargées ;
   - construire le moteur hors du fil d'E/S ; un cache du moteur sérialisé n'est envisageable qu'avec une
     authentification du fichier (risque de sécurité, §7 P2-0) ;
   - appliquer `$removeparam`, servir des ressources `$redirect` écrites par NIGHTFALL ;
   - compléter les scriptlets les plus utilisés ;
   - injecter scriptlets et CSS dans le processus de rendu.

## 1. Méthode et reproductibilité

### 1.1 Outils ajoutés (mesure seulement, jamais compilés dans le navigateur)

| Fichier | Rôle |
|---|---|
| `ci/shield_inputs.py` | Télécharge les 9 listes de `default_lists()` (mêmes URL) et 9 listes candidates, avec SHA-256. Résout les `!#include` comme uBO, dans le même dossier seulement, dans un fichier `.resolved.txt` à part. Télécharge aussi le jeu de requêtes `requests.json` d'adblock-rust (commit figé `09d297c`). |
| `ci/shield_crawl.js` | Playwright + Chromium sans affichage, **sans blocage**, sur les 30 adresses de `bench/pages.txt`. Enregistre chaque requête (page, cadre, URL, type) ainsi que les classes et id présents après chargement. |
| `ci/shield_ubo_oracle.mjs` | Fait juger les mêmes requêtes par le moteur réseau statique de uBlock Origin (commit `ead588a`), avec les mêmes 8 listes, `!#include` résolus et `!#if` évalués pour Chromium. uBO tourne depuis son propre dépôt, construit par son script `tools/make-nodejs.sh`. **Aucun code GPL n'est copié dans le dépôt** : c'est un outil de mesure (voir §8). |
| `crates/nf-shield/examples/shield_audit.rs` | Banc Rust. Utilise l'API de `Shield` et le moteur `adblock` exactement comme le navigateur. Un allocateur instrumenté compte le tas. Produit un rapport Markdown entre `NFAUDIT-BEGIN` et `NFAUDIT-END`. |
| `.github/workflows/ci.yml` | Jobs « Audit Shield — listes et exploration » (Linux) et « Audit Shield — mesures » (Linux, Windows, macOS, mêmes entrées via un artefact). Ils ne tournent que sur une branche `agent/M-0003*` ou au lancement manuel. |

### 1.2 Runs

| Run | Rôle | Jobs (id) |
|---|---|---|
| 41845086446 | Run principal : listes, couverture, temps, cosmétique, constats | entrées 126627902445 ; Linux 126630578235 ; Windows 126630578449 ; macOS 126630578626 |
| 41850060618 | Run du commit audité (`5f472da`) : 3e répétition complète, tous les jobs verts | entrées 126643863892 ; Linux 126646985444 ; Windows 126646985704 ; macOS 126646985940 |
| 41848082350 | Répétition complète (listes et exploration retéléchargées), base mémoire corrigée, scriptlets par liste, constats `$elemhide` / `$specifichide` corrigés | entrées 126637240424 ; Linux 126640107856 ; Windows 126640107745 ; macOS 126640107878 |

Les sorties brutes sont dans `mesures/run-<id>-<os>.md` (copie exacte du journal entre les marqueurs).
`mesures/run-<id>-entrees.txt` donne la taille et le SHA-256 de chaque liste.

Répétition (run 41848082350) : les listes EasyList et EasyPrivacy avaient changé entre les deux runs (SHA-256
différents), uBO filters et uBO privacy non. Les comptages des listes par défaut bougent de quelques règles
(164 009 → 164 011 règles). Les rejets (4 424), `!#include` (18 024) et scriptlets (3 253 / 2 741) sont
identiques, tout comme l'accord avec uBO sur le jeu adblock-rust (99,85 %). L'exploration a donné 3 998 requêtes
(9 pages en 403, comme aux deux autres runs), avec un accord de 100 % sur 3 800 requêtes vérifiables.
Run 41850060618 : mêmes comptages des listes à 1-2 règles près (164 012 règles, 4 424 rejets, 18 024 règles
incluses absentes, 2 741 / 3 253 scriptlets), accord de 99,85 % / 99,98 % sur le jeu adblock-rust ; exploration
de 4 573 requêtes (4 373 vérifiables), accord de 99,86 % (voir §6.2 pour les 6 écarts). Les temps du runner Linux
étaient 15 à 25 % plus lents au run 2 (`should_block` : 7,06 µs contre 5,79 µs de médiane ; `Shield::new` :
111 ms contre 92 ms) : c'est l'ordre de grandeur de la dispersion entre runners.

Contrôle de déterminisme : les sections 1 (listes) et 4 à 5 (couverture, règles déclenchées) du run 41845086446
sont **identiques à l'octet près** sur Linux, Windows et macOS (commande `diff` sur les sections, faite par
l'auteur). Seuls les temps varient d'une plateforme à l'autre.

Reproduire en local (avec accès réseau et crates.io) :

```
python3 ci/shield_inputs.py /tmp/in
node ci/shield_crawl.js bench/pages.txt /tmp/in      # facultatif (Playwright)
cargo run --release -p nf-shield --example shield_audit -- /tmp/in
```

### 1.3 Limites des mesures

- Les runners de CI sont des machines virtuelles partagées : 4 processeurs logiques sous Linux et Windows, 3 sous
  macOS arm64. Les temps sont des ordres de grandeur, comparables entre variantes **d'un même job**, pas des
  temps sur le poste d'un utilisateur.
- Le jeu `requests.json` est le jeu de référence d'adblock-rust. Il est ancien (environ 2019-2020) et ne distingue
  pas cadre principal et iframes : tous les `document` sont traités comme des navigations principales, que
  Shield ne vérifie pas.
- L'exploration réelle est petite : 30 pages, 3 501 requêtes. Elle tourne sans connexion à un compte et depuis des
  adresses de centre de données : 9 pages sur 30 ont répondu HTTP 403 aux trois runs (protection anti-robots), ce qui réduit
  fortement les requêtes de Reddit, NYT, Leboncoin, L'Équipe, Medium, eBay, Stack Overflow et Codepen.
- Les listes évoluent : les chiffres valent pour les SHA-256 du run cité.

## 2. Inventaire : ce que Shield fait aujourd'hui (critère 1)

| Domaine | Ce qui existe | Où | Constat |
|---|---|---|---|
| Moteur | adblock-rust (`adblock` 0.13, fonctions `embedded-domain-resolver`, `full-regex-handling`, `css-validation`) | `Cargo.toml:45`, `crates/nf-shield/src/lib.rs:12-14` | Version résolue en CI : 0.13.5 (`cargo tree`, journal du job 126630578235). |
| Listes | 9 listes : EasyList, EasyPrivacy, Liste FR, 5 listes uBO (activées, « de confiance »), Fanboy Annoyances (désactivée) | `lib.rs:66-87` | uBO `filters.txt` contient 10 `!#include` et `privacy.txt` 1, jamais résolus (§4.1). |
| Mise à jour | Téléchargement séquentiel des listes activées (délai 60 s). Rafraîchissement si absente ou âgée de plus de 12 h, vérifié toutes les 30 min et après déverrouillage | `lib.rs:371-405`, `app/src/nf/api.rs:790-813`, `app/src/nf/timers.rs:60-63` | Validation minimale : contient un saut de ligne et fait au moins 100 octets (`lib.rs:424`). Une page HTML de portail captif serait acceptée et remplacerait la liste. Pas de requête conditionnelle (ETag), `! Expires:` ignoré. Les 8 listes font 5 302 041 octets à chaque mise à jour (somme des tailles du run 41845086446). |
| Construction | `rebuild()` : `FilterSet` + `Engine::new_with_filter_set` + scriptlets | `lib.rs:184-206` | Synchrone. Appelée au démarrage (`app/src/nf/mod.rs:107`, configuration par défaut), puis à nouveau au déverrouillage (`api.rs:759-760`, sous le verrou `core()`, depuis le gestionnaire de schéma qui tourne sur le fil d'E/S de CEF). Également à chaque `shield.set` (`api.rs:1051-1060`). |
| Compteur « règles » | Lignes non vides qui ne commencent pas par `!` | `lib.rs:190`, `lib.rs:199`, `app/ui/security.js:49` | Surestime : il compte l'en-tête `[Adblock Plus 2.0]` et les règles rejetées par le moteur (§4). |
| Réseau | `on_before_resource_load` → `should_block` → `ReturnValue::CANCEL` | `app/src/nf/client.rs:674-706`, `lib.rs:243-261` | Seul un booléen est exploité : `redirect`, `rewritten_url` (`$removeparam`) et `important` sont ignorés. Les navigations principales ne sont jamais vérifiées (`lib.rs:244`, `client.rs:693`). La méthode est toujours « GET » (`lib.rs:250`). |
| Requêtes sans navigateur | Retour immédiat `CONTINUE` si `browser` est nul | `client.rs:681` ; aucun `RequestContextHandler::resource_request_handler` (`app/src/nf/prefs.rs:202-219`) | Selon la doc CEF, `browser` et `frame` sont nuls pour les requêtes des service workers : elles ne sont pas filtrées. **NON MESURÉ** en pratique (protocole au §6.4). |
| Types | Correspondance `cef_resource_type_t` → types adblock | `lib.rs:140-163` | Les workers sont typés `script`, prefetch et rapports CSP `other`. Les WebSockets ne passent probablement pas par ce rappel. **NON VÉRIFIÉ**. |
| Exceptions | `@@`, `$important`, `$badfilter`, `$generichide` (moteur) | moteur | Vérifié par exécution : `$generichide` coupe bien le CSS générique (§4.6). `$elemhide` et `$specifichide` (non reconnus par adblock-rust) sont rejetés. |
| Cosmétique propre au site et générique « complexe » | `url_cosmetic_resources` → CSS par paquets de 20 sélecteurs, injecté dans le script de page | `lib.rs:296-333` | Le script fait 28,6 ko en médiane par cadre (§5.4), dont 531 sélecteurs génériques identiques partout. |
| Cosmétique générique classes/id | `cosmetic.js` signale au cœur les classes et id vus, en passant par `console.debug` avec un jeton ; le cœur répond par `generic_css` | `crates/nf-shield/src/cosmetic.js:105-121`, `client.rs:147-162`, `client.rs:465-485` | La réponse est injectée dans le **cadre principal** (`browser.main_frame()`, `client.rs:476-484`) même quand le signalement vient d'une iframe : le masquage générique ne s'applique donc pas dans les iframes (lecture de code, **NON MESURÉ** à l'exécution). `generic_css` recalcule `url_cosmetic_resources` à chaque signalement (`lib.rs:341`). |
| Procédural | `has-text`, `min-text-length`, `upward`, `matches-attr`, `matches-css(-before/-after)`, `matches-path`, `xpath` ; actions `remove`, `style`, `remove-attr`, `remove-class` | `cosmetic.js:29-103` | Couvre les 10 opérateurs et 4 actions qu'adblock-rust produit (`adblock-rust/src/filters/cosmetic.rs:51-147`). Réévaluation complète de toutes les règles à chaque lot de mutations (40 ms, `cosmetic.js:136-145`). |
| Scriptlets | 21 scriptlets maison sous MPL et leurs alias, plus `nf-utils.fn`. Les `trusted-*` sont réservés aux listes de confiance | `crates/nf-shield/src/scriptlets.rs:27-51`, `lib.rs:192-194` | 84,3 % des règles de scriptlet des listes par défaut sont servies (§4.4). |
| Moment d'injection | `frame.execute_java_script` dans `on_load_start`, pour chaque cadre | `client.rs:404-421` | Envoi asynchrone vers le processus de rendu : rien ne garantit l'exécution avant les scripts de la page. L'auto-test vérifie le scriptlet après un `setTimeout` (`app/src/nf/selftest.rs:354-383`), il ne peut pas détecter une injection tardive. **NON MESURÉ** (§6.4). |
| Redirections `$redirect` | Aucune ressource de redirection | `scriptlets.rs:76-90` (scriptlets seulement) | Les 501 règles `$redirect` bloquent sans servir de substitut (exécuté, §4.6). |
| CSP `$csp` | Non appliqué | aucun appel à `get_csp_directives` | 35 règles. |
| `$removeparam` | Non appliqué | `lib.rs:254` (seul `should_block()` est lu) | 216 règles dans les listes par défaut. |
| Pop-ups | Bloque les fenêtres ouvertes sans geste de l'utilisateur | `client.rs:332-337`, `lib.rs:263-271` | Les règles `$popup` (3 764) sont rejetées par le moteur. |
| Réglages par site | Liste d'exceptions par domaine enregistrable (marche/arrêt) | `lib.rs:226-239`, `api.rs:478-486` | Aucun réglage plus fin : pas de « cosmétique seule », pas de mode strict, pas de désactivation des scriptlets. L'exception est vérifiée sur la page pour le réseau (`client.rs:699`), mais sur **l'adresse du cadre** pour l'injection (`client.rs:419`) : une iframe tierce garde ses scriptlets sur un site en exception (lecture de code). |
| Filtres personnels | Texte libre, jamais « de confiance » | `lib.rs:198-201`, test `trusted_scriptlets_need_a_trusted_list` | Conforme. |
| Statistiques | Total, pop-ups, compteur par site (sans limite de taille), compteur par onglet | `lib.rs:103-108`, `lib.rs:255-259`, `client.rs:20-29` | `per_site` grandit sans limite pendant la session : 653 entrées après le jeu de requêtes. Effet mémoire faible. |
| Interface | Panneau de la barre d'outils (compteur, marche/arrêt par site, recharge) ; Security Center (activation, pop-ups, listes, mise à jour, filtres personnels, exceptions, top 8 des sites) | `app/ui/shell.js:597-625`, `app/ui/security.js:26-83` | Pas de journal des requêtes bloquées (diagnostic des faux positifs impossible pour l'utilisateur), pas de sélecteur d'élément. |
| Tests | 10 tests unitaires dans `lib.rs`, 1 dans `scriptlets.rs`, étape d'auto-test de bout en bout | `lib.rs:430-546`, `selftest.rs:265-283`, `selftest.rs:354-399` | — |

## 3. Étude comparative (critère 2)

Fondée sur la lecture des copies `/home/claude/ref/` (adblock-rust `09d297c` = 0.13.5, uBlock `ead588a`,
tsurlfilter `de4c7af`).

| | adblock-rust (Brave) | uBlock Origin | AdGuard tsurlfilter / tswebextension | NIGHTFALL aujourd'hui |
|---|---|---|---|---|
| Licence | MPL-2.0 (`LICENSE`) — compatible | GPL-3.0 (`LICENSE.txt`) — étude seulement | GPL-3.0 (`packages/tsurlfilter/LICENSE`) ; `agtree` et `css-tokenizer` sous MIT | MPL-2.0 |
| Langage, forme | Bibliothèque Rust. Moteur compilé en FlatBuffers (`src/flatbuffers/fb_network_filter.fbs`), sérialisable (`src/engine.rs:436-466`) | Extension JS ; moteur réseau dans `src/js/static-net-filtering.js` (6 124 lignes) avec tries en WASM (`src/js/hntrie.js`, `src/js/biditrie.js`) ; image mémoire (« selfie », `static-net-filtering.js:539`) | Bibliothèque TS (`packages/tsurlfilter/src/engine/`), extension (`tswebextension`), convertisseur MV3 (`dnr-converter`) | adblock-rust intégré dans CEF (`crates/nf-shield`, `app/src/nf/client.rs`) |
| Indexation réseau | Jetons hachés par seaux, fusion de règles (`src/optimizer.rs`), regex compilées à la demande puis jetées (`src/regex_manager.rs`) | Seaux par jeton + dictionnaires d'hôtes en trie (`FilterHostnameDict`, `static-net-filtering.js:2573`) | Tables : domaines, hôtes, raccourcis en trie, balayage séquentiel (`engine/network-engine.ts`, `engine/lookup-tables/`) ; règles relues à la demande depuis le texte (`filterlist/rule-storage`) | celle d'adblock-rust |
| Options réseau | 38 noms reconnus (`src/filters/abstract_network.rs:189-312`). Absents : `popup`, `denyallow`, `header`, `permissions`, `urlskip`, `replace`, `strict1p/3p`, `ipaddress`, `elemhide`, `specifichide`… `$to` est analysé mais pas appliqué (CHANGELOG 0.13.3) | Le plus complet : `denyallow`, `header`, `permissions`, `popup`/`popunder`, `replace`, `urlskip`, `uritransform`, `ipaddress`, `strict1p/3p`, `cname`… (`src/js/static-filtering-parser.js`, `netOptionTokenDescriptors`) | Syntaxe AdGuard : `cookie`, `stealth`, `removeheader`, `jsonprune`, `hls`, `referrerpolicy`, `urltransform`, `permissions`, `replace`, `header`, `popup`… (`src/rules/network-rule.ts`, `NETWORK_RULE_OPTIONS`, dossier `src/modifiers/`) | celles d'adblock-rust, moins `redirect`, `removeparam`, `csp`, `document` et la méthode (non exploités) |
| Préprocesseur `!#if` / `!#include` | **Non** (aucune gestion dans `src/lists.rs`) : c'est à l'intégrateur de le faire | Oui (`src/js/static-filtering-parser.js:4340-4448`, `src/js/assets.js:422-440`) | Pas dans `packages/tsurlfilter` (aucune occurrence de `#include` dans ses sources) : confié au paquet externe `@adguard/filters-downloader` (dépendance déclarée dans `packages/adguard-api/package.json:49`), dont le code n'a **pas** été lu | **Non** |
| Cosmétique | Propre au site par hachage d'hôte, génériques classes/id à la demande, génériques complexes renvoyés à chaque page (`src/cosmetic_filter_cache.rs:233-366`) | Génériques « faibles » (classes/id) découverts par le script de contenu, génériques « forts » injectés en feuille de style utilisateur par `tabs.insertCSS` (`src/js/cosmetic-filtering.js:241-299`, `726`, `893`) | Moteur cosmétique (`src/engine/cosmetic-engine/`), CSS étendu (bibliothèque ExtendedCss), injection `#$#` et `#%#` | Tout passe par un script injecté par cadre, puis une balise `<style>` dans la page |
| Procédural | 10 opérateurs + 4 actions (`src/filters/cosmetic.rs:51-147`). Rejette `:watch-attr`, `:others`, `:matches-media`, `:shadow`, `:if`, `:matches-prop`, `:-abp-properties` | 26 opérateurs (`static-filtering-parser.js`, `proceduralOperatorTokens`) | ExtendedCss : `:contains`, `:has`, `:xpath`, `:upward`, `:matches-css`, `:matches-attr`, `:matches-property`, `:remove`… | les 10 + 4 d'adblock-rust (`cosmetic.js`) |
| Scriptlets | Mécanisme seulement (ressources fournies par l'intégrateur, permissions, `src/resources/resource_storage.rs:275-433`). Brave fournit ceux de uBO à part | 99 scriptlets publics, dont 34 `trusted-*` (comptage par expression régulière sur `src/js/resources/*.js`, commande au §3.1) | Bibliothèque `@adguard/scriptlets` (GPL), avec table de correspondance uBO/ABP (`packages/agtree/src/compatibility-tables/scriptlets/`, 90 fichiers, MIT) | 21 scriptlets + alias, écrits par NIGHTFALL |
| Redirections | Mécanisme seulement (`src/blocker.rs:228-290`, ressource renvoyée en `data:`) | 48 ressources (`src/js/redirect-resources.js`) | Ressources de redirection AdGuard (table `compatibility-tables/redirects/`, 54 fichiers) | aucune |
| Filtrage HTML | Non (`HtmlFilteringUnsupported`) | Oui, Firefox seulement (`src/js/html-filtering.js`) | Oui (`$$`, tswebextension) | Non |
| En-têtes, CSP | `get_csp_directives` (`src/engine.rs:287`) | `$csp`, `$header`, `$permissions` (`src/js/httpheader-filtering.js`) | `$csp`, `$header`, `$permissions`, `$removeheader`, `$cookie` | Non |
| Démasquage CNAME | Non | Firefox seulement (`src/js/background.js:61-65`) | Application de bureau / DNS | Non |
| Performances mesurées ici | Médiane 3,5 à 4,7 µs par décision (§5.3) | `matchRequest` dans Node : médiane 3,12 µs, p99 23,8 µs au run 41845086446 (job 126627902445) ; 5,98 / 41,9 µs au run 41848082350. Même jeu, chronométrage fait par le script, autre job que le banc Rust : comparaison indicative seulement | **NON MESURÉ** (TypeScript, non intégrable en Rust) | `should_block` : 5,4 à 8,0 µs |

### 3.1 Commandes de comptage citées

```
# Scriptlets publics de uBO : affiche « 99 34 » (99 noms distincts hors .fn, dont 34 « trusted- »).
# Les définitions passent par registerScriptlet(...) et par builtinScriptlets.push({...}) : les deux formes
# sont comptées. (La commande de la première version de ce document ne comptait que la seconde et affiche « 29 8 ».)
cd /home/claude/ref/uBlock/src/js/resources && python3 -I -c "
import re,glob
n=[]
for f in glob.glob('*.js'):
  s=open(f).read()
  for m in re.finditer(r'registerScriptlet\(\s*\w+\s*,\s*\{(.*?)\}\s*\)', s, re.S):
    x=re.search(r\"name:\s*'([^']+)'\",m.group(1))
    if x: n.append(x.group(1))
  for m in re.finditer(r'builtinScriptlets\.push\(\{(.*?)\n\}\)', s, re.S):
    x=re.search(r\"name:\s*'([^']+)'\",m.group(1))
    if x: n.append(x.group(1))
p=sorted(set(x for x in n if not x.endswith('.fn')))
print(len(p),sum(x.startswith('trusted-') for x in p))"
# Ressources de redirection de uBO : 48
grep -c "^    \[ '" /home/claude/ref/uBlock/src/js/redirect-resources.js
```

### 3.2 Listes étudiées

| Liste | Éditeur, licence | Dans NIGHTFALL | Dans uBO (`assets/assets.json`) | Remarque |
|---|---|---|---|---|
| EasyList, EasyPrivacy | EasyList ; double licence GPLv3 / CC BY-SA 3.0 (easylist.to/pages/licence.html, cité par le `README.md` du dépôt) | oui | oui (miroir `uAssets/thirdparties`) | NIGHTFALL charge la variante ABP. Elle contient la partie `easylist_specific_hide_abp.txt` (304 lignes) avec `#?#`, que adblock-rust traduit. |
| Liste FR | EasyList / Adblock Plus | oui | non (uBO propose AdGuard Français) | Rejets : 732 règles (5,2 %), surtout `$popup`. |
| uBlock filters (5 listes) | uAssets, GPLv3 | oui (de confiance) | oui | `!#include` non résolus, `!#if` ignorés. |
| Fanboy Annoyances | Fanboy, mêmes conditions qu'EasyList | désactivée | partiellement (variantes uBO) | 4 `!#include`. |
| AdGuard Base, Tracking Protection, Français | AdGuard, GPLv3 (`AdguardFilters/LICENSE`) | non | Base et Français proposés, désactivés | Mesures au §5.5. |
| uBO « privacy-removeparam » (AdGuard URL Tracking) | uAssets / AdGuard, GPLv3 | non | proposée, désactivée | Composée presque entièrement de `$removeparam`. |
| Bandeaux de cookies (uBO, EasyList Cookie) | GPLv3 / EasyList | non | proposées, désactivées | 2 162 et 2 468 scriptlets, presque tous absents chez NIGHTFALL. |
| Peter Lowe | pgl.yoyo.org ; **licence NON VÉRIFIÉE** (non trouvée par recherche web) | non | **activée par défaut** (`plowe-0`) | Format hosts. |
| URLhaus (malware-filter) | malware-filter | non | activée par défaut (`urlhaus-1`) | 108 991 règles `$all` : utile seulement si les navigations sont vérifiées. |

Les listes sont téléchargées par le navigateur de l'utilisateur. NIGHTFALL ne les distribue pas, ce qui évite de
mélanger leur licence au code MPL. Les inclure dans l'installateur reviendrait à redistribuer du contenu GPL ou
CC BY-SA : c'est une décision du propriétaire.

## 4. Syntaxes non prises en charge, comptées sur les listes actuelles (critère 3)

Source : run 41845086446, section 1 de `mesures/run-41845086446-linux.md` (identique sous Windows et macOS).
Analyse ligne par ligne avec `adblock::lists::parse_filter`, le parseur même du moteur, avec les options de chaque
liste.

### 4.1 Bilan des 8 listes activées

| Liste | Règles | Réseau acceptées | Cosmétiques acceptées | Rejetées | % |
|---|---|---|---|---|---|
| easylist | 77 619 | 49 895 | 24 742 | 2 982 | 3,8 % |
| easyprivacy | 56 475 | 56 436 | 36 | 3 | 0,0 % |
| liste_fr | 14 166 | 9 873 | 3 561 | 732 | 5,2 % |
| ubo_filters | 6 073 | 1 643 | 4 253 | 177 | 2,9 % |
| ubo_quick_fixes | 446 | 131 | 268 | 47 | 10,5 % |
| ubo_privacy | 1 786 | 1 184 | 309 | 293 | 16,4 % |
| ubo_unbreak | 2 572 | 1 846 | 601 | 125 | 4,9 % |
| ubo_badware | 4 872 | 4 665 | 142 | 65 | 1,3 % |
| **Total** | **164 009** | **125 673** | **33 912** | **4 424** | **2,70 %** |

- **`!#include` non résolus.** `ubo_filters` en contient 10 et `ubo_privacy` 1 (tous résolus avec succès par
  `ci/shield_inputs.py`). Une fois résolue, `ubo_filters` passe de 6 073 à 24 019 règles (6 042 réseau, 16 920
  cosmétiques). Au total, **18 024 lignes de règles** des listes activées ne sont jamais chargées par NIGHTFALL. Ce chiffre
  surestime le gain réel :
  - au moins 576 de ces lignes (648 − 72 pour `ubo_filters`) sont dans des blocs `!#if` faux pour Chromium : uBO
    ne les chargerait pas ;
  - 880 seraient rejetées par le moteur (1 057 − 177 rejets) ;
  - le gain utile est donc d'**au plus environ 16 568 règles** (18 024 − 576 − 880, en supposant les deux groupes
    disjoints ; s'ils se recouvrent, le gain est un peu plus grand). **NON MESURÉ** exactement.

  Mon résolveur (`ci/shield_inputs.py:62-82`) suit **toutes** les directives `!#include`, y compris celles d'un
  bloc `!#if` faux. uBO, lui, ignore les `!#include` exclus par un `!#if` (`ref/uBlock/src/js/assets.js:422-440`).
  Ici, les 10 fichiers inclus ont tous été résolus. Je n'ai pas vérifié si certaines inclusions sont elles-mêmes
  dans un `!#if` faux.
- **Compteur affiché** : « 164 012 règles » (`rules_loaded()`), pour 159 585 règles acceptées par le moteur.

### 4.2 Règles rejetées par le moteur, par motif

| Motif | Règles | Exemple |
|---|---|---|
| Option réseau inconnue | 4 260 | `/earn.php?z=$popup,subdocument` |
| Filtrage HTML `##^` | 71 | `abema.tv##^script:has-text(NREUM)` |
| `$redirect-rule` vide (exception générique) | 33 | `@@||google-analytics.com/analytics.js$redirect-rule,script,domain=mond.how` |
| Arguments de scriptlet illisibles | 12 | `bild.de##+js(rpnt, script, "adBlockWallEnabled":true, …)` |
| Exception cosmétique générique `#@#` | 11 | `#@#.reklama` |
| `$removeparam` vide | 11 | `||fastlane.rubiconproject.com^$removeparam,domain=aternos.org` |
| Opérateur `:-abp-properties()` | 8 | (EasyList) |
| `$removeparam` en expression régulière | 5 | `||reddit.com^$doc,removeparam=/web_only=/` |
| Divers (`:remove-attr(/regex/)`, `$csp` avec type, sélecteurs multiples procéduraux, `~document`, exception de `removeparam`) | 13 | — |

Options inconnues (règles rejetées) :

| Option | Règles | | Option | Règles |
|---|---|---|---|---|
| `$popup` | 3 764 | | `$reason` | 16 |
| `$urlskip` | 215 | | `$popunder` | 15 |
| `$denyallow` | 83 | | `$uritransform` | 12 |
| `$replace` | 40 | | `$permissions` | 11 |
| `$ipaddress` | 33 | | `$strict3p` / `$strict1p` | 10 / 9 |
| `$cname` | 27 | | `$header`, `$inline-script`, `$shide`, `$webrtc`, autres | 12 |

### 4.3 Règles acceptées mais sans effet (complet) dans NIGHTFALL

| Cas | Règles | Cause |
|---|---|---|
| `$document` (blocage de page) | 3 695 | navigations principales jamais vérifiées (`lib.rs:244`) |
| `$all`, partie « document » | 1 285 | idem (le reste de la règle s'applique) |
| `$redirect` | 501 | bloque sans substitut : aucune ressource fournie |
| `$to` | 270 | **appliqué plus largement que prévu** : adblock-rust 0.13.5 analyse `$to` sans l'utiliser au filtrage (`ref/adblock-rust/CHANGELOG.md:18`), donc la règle s'applique comme si `$to` était absent. Risque de faux positifs, pas absence d'effet |
| `$removeparam` | 216 | `rewritten_url` ignoré |
| `$redirect-rule` | 152 | sans effet sans ressource |
| `$csp` | 35 | `get_csp_directives` jamais appelé |
| `$method` sans GET | 12 | Shield passe toujours `GET` |

Ressources `$redirect` les plus demandées : `noopjs` (170), `google-ima.js` (106), `noop.js` (37), `noopmp3-0.1s`
(34), `1x1.gif` (27), `nooptext` (17), `fingerprint2.js` (16), `abp-resource:blank-mp4` (9), `noop.txt` (9),
`noopmp4-1s` (9), `googletagservices_gpt.js` (6), `2x2.png` (5)…

### 4.4 Scriptlets

Les listes activées contiennent 3 253 règles de scriptlet. NIGHTFALL en sert **2 741 (84,3 %)** ; aucune n'est
refusée pour une question de confiance. Les 512 autres appellent un des 37 noms absents. Les plus utilisés :
`href-sanitizer` (70), `set-local-storage-item` (61), `remove-cookie` (50), `nowebrtc` (47), `aost` (34),
`trusted-prevent-dom-bypass` (30), `noeval` (23), `rpnt` (23), `trusted-set-cookie` (19), `cookie-remover` (18),
`popads-dummy` (18), `trusted-replace-argument` (16), `nobab` (15), `nofab` (10), `set-cookie` (10). Ces 15 noms
totalisent 444 règles. Les fournir porterait la couverture à (2 741 + 444) / 3 253 = 97,9 %, calcul fait sur les
comptages du run.

### 4.5 `!#if`

adblock-rust ne gère pas les directives : toutes les règles des blocs `!#if` sont chargées. 191 règles des listes
activées se trouvent dans un bloc dont la condition est **fausse** pour un Chromium de bureau : `cap_html_filtering`
(43), `env_safari` (35), `env_mobile` (33), `ext_ubol` (28), `env_firefox` (21), etc. Évaluation par la fonction
`eval_cond` du banc, écrite d'après la sémantique documentée par uBO. Avec les `!#include` résolus,
`ubo_filters` seule en compte 648. Effet mesuré sur les décisions réseau : aucun. NIGHTFALL ne bloque jamais seul
après résolution (§6.2). Effet cosmétique et scriptlets : **NON MESURÉ**.

### 4.6 Constats exécutés (section 7 de `mesures/run-41845086446-linux.md`)

| Règle(s) | Résultat |
|---|---|
| `||ads.example^$script,redirect=noopjs` | requête annulée, pas de substitut |
| `$removeparam=utm_source` | non bloquée, et l'URL nettoyée est perdue (`should_block` ne renvoie qu'un booléen) |
| `!#if false` / `||iffalse.example^` / `!#endif` | **bloquée** malgré la condition fausse |
| `||popunder.example^$popup` | règle non chargée, mais comptée dans `rules_loaded` |
| `||badware.example^$document` | moteur : bloquer = vrai ; Shield sur la navigation : faux |
| `||collect.example^$method=post` | jamais bloquée (méthode toujours GET) |
| `news.example##+js(nonexistent-scriptlet)` | aucun appel injecté (perte silencieuse) |
| `[Adblock Plus 2.0]` + 1 `$popup` + 1 `#$#` + 1 règle valide | `rules_loaded` = 4 pour 1 règle chargée |
| `widget.example##.inner-ad` | appliquée dans l'iframe widget.example (l'injection se fait par cadre) |

Les constats `$elemhide`, `$generichide` et `$specifichide` du run 41845086446 étaient mal construits : la règle
générique `##.ad` passe par `generic_css`, pas par `page_script`. Ils sont corrigés à partir du run 41848082350 (§5.7).

## 5. Mesures (critère 4)

### 5.1 Construction du moteur au démarrage

Mesure faite avec `Shield::new` sur les 8 fichiers en cache, lecture disque comprise : c'est exactement ce que fait
`app/src/nf/mod.rs:107`. 5 répétitions, run 41845086446.

| Plateforme | `Shield::new` médiane (min–max) | idem, run 41848082350 | `set_config` (1 mesure) | Moteur V0 seul, médiane | Désérialisation, médiane (run 1 / run 2) | Taille sérialisée |
|---|---|---|---|---|---|---|
| Linux x86_64 | 92,4 ms (92,1–93,4) | 111,0 ms (110,6–111,1) | 92,5 ms | 87,3 ms | 1,5 / 1,6 ms | 7,3 Mio |
| Windows x86_64 | 140,4 ms (139,9–140,7) | 121,6 ms (120,2–123,5) | 138,6 ms | 133,8 ms | 2,6 / 3,0 ms | 7,3 Mio |
| macOS arm64 | 83,4 ms (80,2–94,3) | 95,8 ms (80,1–109,5) | 127,8 ms | 81,7 ms | 1,5 / 1,8 ms | 7,3 Mio |

Au démarrage avec un profil enregistré, le moteur est construit deux fois : une fois à l'ouverture
(`mod.rs:107`), une fois au déverrouillage (`api.rs:760`). La seconde construction s'exécute dans le gestionnaire
de schéma, donc sur le fil d'E/S de CEF (doc CEF de `cef_scheme_handler_factory_t` : « always be called on the IO
thread »), en tenant le verrou `core()`. **NON MESURÉ** : l'effet réel sur le chargement des pages restaurées
pendant ces 80 à 140 ms.

### 5.2 Mémoire

Tas compté par l'allocateur instrumenté du banc (`shield_audit.rs`, `Counting`).

| Mesure (Linux, run 41845086446) | Valeur |
|---|---|
| `Shield` construit (moteur + ressources + état), tas conservé | 7,4 Mio |
| Pic pendant la construction V0 (au-delà des textes déjà lus) | 18,7 Mio |
| Moteur sérialisé V0 / V1 (`!#include` résolus) / VP (proposition) | 7,3 / 8,7 / 26,0 Mio |

Le « tas conservé par variante » du run 41845086446 est **faux**, et donc pas cité : les copies des textes étaient
comptées dans la base puis libérées pendant la construction. Chiffres corrigés au §5.2 bis. La mémoire du processus de rendu (script injecté, CSS, `MutationObserver`)
est **NON MESURÉE**.

### 5.3 Temps par requête réseau

Jeu adblock-rust, 227 054 requêtes vérifiables, chaque appel chronométré seul, deuxième passe (run 41845086446).

| Plateforme | `Shield::should_block` médiane / p99 / p99,9 | `check_network_request` seul V0 médiane / p99 | `Request::new` médiane | V1 (`!#include`) médiane / p99 | VP médiane / p99 |
|---|---|---|---|---|---|
| Linux | 5,79 / 22,08 / 40,04 µs | 3,67 / 16,63 µs | 1,28 µs | 4,83 / 20,26 µs | 5,49 / 29,34 µs |
| Windows | 8,00 / 32,00 / 62,80 µs | 4,70 / 23,30 µs | 2,00 µs | 6,20 / 32,00 µs | 6,80 / 34,90 µs |
| macOS | 5,42 / 32,54 / 84,75 µs | 3,54 / 26,38 µs | 1,17 µs | 4,75 / 34,92 µs | 5,54 / 47,71 µs |

Répétition (run 41848082350), médiane / p99 de `should_block` : Linux 7,06 / 26,57 µs ; Windows 6,70 / 25,70 µs ;
macOS 6,00 / 43,08 µs.

`should_block` ajoute environ 2 µs au moteur seul. Cela vient de l'analyse de l'URL de la page par `site_of`
(0,33 à 0,60 µs, appelé une fois par `is_active_for`, puis une seconde fois en cas de blocage), des verrous et de
`Request::new`. La première passe, avec compilation des expressions régulières à la demande, ne diffère de la
seconde que dans la queue : p99,9 de 52 µs contre 40 µs sous Linux. Pour une page de 100 requêtes, le coût total
est de l'ordre de 0,5 à 0,8 ms de CPU sur le fil d'E/S.

### 5.4 Cosmétique

1 684 adresses de documents (hôtes distincts du jeu adblock-rust), run 41845086446.

| Plateforme | `url_cosmetic_resources` médiane / p99 | `page_script` médiane / p99 |
|---|---|---|
| Linux | 70,2 / 96,1 µs | 172,4 / 206,8 µs |
| Windows | 83,9 / 130,4 µs | 211,3 / 274,4 µs |
| macOS | 54,6 / 116,7 µs | 149,1 / 249,3 µs |

- Le libellé du banc dit « 2 appels à `url_cosmetic_resources` » : c'est inexact. `page_script` en fait un seul
  (`lib.rs:301`). Le reste, environ 100 µs, vient du tri, de l'assemblage des 531 sélecteurs, du JSON et de la
  concaténation du moteur `cosmetic.js`.
- Le script injecté fait 28 627 octets en médiane par cadre (p90 35 416, max 39 723). Une page sans aucune règle
  propre reçoit encore 531 sélecteurs génériques complexes (28 600 octets).
- `generic_css` (réponse au signalement des classes et id) prend, en médiane de 20 appels par page explorée,
  de 2 µs (pages sans règle ou en `generichide`) à 123 µs, et renvoie 0 à 8 sélecteurs (Linux).
- Le coût dans la page (analyse du script, `MutationObserver`, réévaluation des règles procédurales) est
  **NON MESURÉ**.

### 5.5 Variantes de listes : coût et gain

Run 41845086446, Linux. Construction en médiane de 5. Gains en requêtes bloquées en plus par rapport à V0.

| Variante | Construction | Sérialisé | Jeu adblock-rust : gagnées / perdues | Exploration : gagnées / perdues | Lecture |
|---|---|---|---|---|---|
| V0 actuel | 87,3 ms | 7,3 Mio | — (101 029 bloquées, 44,50 %) | — (797, 24,28 %) | |
| V1 = V0 avec `!#include` résolus | 117,2 ms | 8,7 Mio | +289 / −3 | 0 / 0 | correction, pas un ajout |
| V2 EasyList + EasyPrivacy seules | 64,3 ms | 5,8 Mio | +832 / −1 006 | +1 / −3 | les listes uBO apportent un gain net |
| V1 + Fanboy Annoyances | 157,2 ms | 11,7 Mio | +2 038 / −7 | +19 / 0 | surtout cosmétique (49 990 règles) |
| V1 + AdGuard Base | 187,7 ms | 12,5 Mio | +586 / −181 | +2 / −1 | ses exceptions débloquent 181 requêtes (correction ou perte : non établi) |
| V1 + AdGuard Tracking | 347,3 ms | 28,8 Mio | +2 158 / −194 | +29 / 0 | le plus gros gain anti-pistage, mais 3× plus coûteux à construire ; 13,8 Mo, au-delà de la limite de `download()` |
| V1 + AdGuard Français | 120,6 ms | 9,4 Mio | +289 / −239 | 0 / 0 | ≈ Liste FR, pas d'intérêt en plus |
| V1 + uBO URL tracking | 112,1 ms | 8,8 Mio | +289 / −3 (= V1) | 0 / 0 | utile seulement si `$removeparam` est appliqué |
| V1 + uBO cookies | 113,0 ms | 9,0 Mio | = V1 | 0 / 0 | inutile sans ses scriptlets (12 sur 2 162 servis) |
| V1 + EasyList Cookie | 132,2 ms | 10,7 Mio | +845 / −3 | +17 / 0 | |
| V1 + Peter Lowe | 112,3 ms | 8,8 Mio | +2 071 / −3 | +24 / 0 | dont +951 blocages de premières parties (46 % du gain) ; licence à vérifier |
| V1 + URLhaus | 228,5 ms | 23,5 Mio | = V1 | 0 / 0 | utile seulement si les navigations sont vérifiées ; 12,2 Mo, au-delà de la limite de 10 Mio de `download()` |
| VP = V1 + URL tracking + 2 listes cookies + Peter Lowe + URLhaus | 253,7 ms | 26,0 Mio | +2 620 / −3 | +41 / 0 | |

### 5.2 bis Mémoire par variante (run 41848082350, Linux, base mémoire corrigée)

| Variante | Tas conservé | Pic pendant la construction (textes compris) | Sérialisé |
|---|---|---|---|
| V0 actuel | 7,4 Mio | 23,8 Mio | 7,3 Mio |
| V1 `!#include` résolus | 9,0 Mio | 31,7 Mio | 8,7 Mio |
| V2 EasyList + EasyPrivacy | 5,8 Mio | 17,2 Mio | 5,8 Mio |
| V1 + Fanboy Annoyances | 11,9 Mio | 37,1 Mio | 11,7 Mio |
| V1 + AdGuard Tracking | 29,1 Mio | 96,3 Mio | 28,8 Mio |
| V1 + Peter Lowe | 9,1 Mio | 31,9 Mio | 8,8 Mio |
| V1 + URLhaus | 23,8 Mio | 74,2 Mio | 23,5 Mio |
| VP proposition | 26,3 Mio | 78,8 Mio | 26,0 Mio |

Le tas conservé d'un moteur est presque égal à sa taille sérialisée : le moteur garde son tampon FlatBuffers.
`Shield::new` conserve aussi 7,4 Mio, ce qui confirme la correction.

### 5.6 Scriptlets des listes candidates (run 41848082350)

| Liste | Scriptlets servis / total | Absents les plus utilisés |
|---|---|---|
| uBO cookies | 12 / 2 162 | `trusted-set-cookie` (950), `trusted-click-element` (897), `trusted-set-local-storage-item` (158), `set-cookie` (103) |
| EasyList Cookie | 59 / 2 468 | `set-cookie` (1 960), `set-local-storage-item` (418), `set-session-storage-item` (24) |
| Fanboy Annoyances | 135 / 2 846 | `set-cookie` (2 134), `set-local-storage-item` (484), `set-session-storage-item` (53) |
| AdGuard Base | 4 045 / 4 351 | `abort-on-stack-trace` (121), `set-cookie` (71), `google-ima.js` (32) |
| AdGuard Tracking | 234 / 435 | `set-local-storage-item` (155), `href-sanitizer` (16) |

Par liste activée : ubo_filters 2 258 / 2 427, ubo_unbreak 288 / 311, ubo_privacy 69 / 293, ubo_quick_fixes
85 / 168, easyprivacy 21 / 30, ubo_badware 20 / 24.

**Conséquence** : les listes anti-bandeaux de cookies reposent surtout sur `set-cookie`,
`trusted-set-cookie`, `trusted-click-element` et `set-local-storage-item`. Sans ces 4 scriptlets, les activer
n'apporterait presque rien. Fanboy Annoyances, déjà proposée dans l'interface, n'est servie qu'à 4,7 % pour ses
scriptlets.

### 5.7 Exceptions cosmétiques (constats corrigés, run 41848082350)

| Règles | Résultat |
|---|---|
| `##.ad` (témoin) | `generic_css` renvoie `.ad{display:none!important}` |
| `@@||news.example^$generichide` + `##.ad` | `""` : l'exception fonctionne |
| `@@||news.example^$elemhide` + `##.ad` | `.ad{…}` : **l'exception est ignorée** (`$elemhide` rejeté par le moteur) |
| `@@||news.example^$specifichide` + `news.example##.pub` | `.pub` toujours masqué : **l'exception est ignorée** |

Ces options sont rares dans les listes par défaut (1 `$shide`, §4.2) mais courantes dans les filtres personnels
copiés depuis la documentation de uBO.

## 6. Couverture et faux positifs (critère 5)

### 6.1 Protocole (reproductible, sans service payant)

1. **Entrées figées** : listes avec SHA-256, jeu `requests.json` à un commit fixe, exploration enregistrée **sans
   blocage** de `bench/pages.txt`. Toutes les variantes sont jugées sur les mêmes requêtes, comme dans les bancs
   de Brave.
2. **Couverture réseau** : part des requêtes vérifiables bloquées, au total, par type, tierces et premières
   parties, pour chaque variante.
3. **Référence indépendante** : verdicts du moteur de uBlock Origin avec les mêmes listes (et `!#include`, `!#if`
   traités comme uBO), joints ligne à ligne. Les désaccords sont listés avec la règle en cause des deux côtés.
4. **Indicateur de faux positifs** : blocages de premières parties (même domaine enregistrable que la page),
   attribués à leur règle par un moteur en mode débogage, et exceptions `@@` appliquées. Ce n'est qu'un
   indicateur : un blocage de premier niveau peut être voulu (pisteur hébergé par le site), et un faux positif
   peut venir d'un tiers.
5. **Effets non appliqués** (calculés par adblock-rust) : requêtes pour lesquelles le moteur désigne une ressource
   de substitution, URL qu'il nettoierait,
   documents qui recevraient une CSP.

### 6.2 Premiers résultats (run 41845086446, identiques sur les 3 plateformes)

| Jeu | Vérifiables | Bloquées par V0 | Accord V0 / uBO | Accord V1 / uBO | uBO seul (V0) | NIGHTFALL seul (V0) |
|---|---|---|---|---|---|---|
| adblock-rust | 227 054 | 101 029 (44,50 %) | 99,85 % | 99,98 % | 345 | 3 |
| exploration réelle | 3 283 | 797 (24,28 %) | 100,00 % | 100,00 % | 0 | 0 |

- Règles derrière « uBO seul » (V0) : `*$from=googleads.g.doubleclick.net` (110), `||pagefair.net^` (66),
  `||npttech.com/advertising.js^$script,3p` (47), `||amazon-adsystem.com^` (43)… Elles viennent pour la plupart
  des fichiers `!#include` : il en reste 56 après résolution.
- « NIGHTFALL seul » (V0) : `/somtag/loader/*` (3 requêtes). Mon hypothèse d'une règle d'un bloc `!#if` était
  fausse : avec les `!#include` résolus (V1), « NIGHTFALL seul » tombe à 0. Ces 3 requêtes sont donc débloquées
  par une exception présente dans les fichiers inclus, que NIGHTFALL ne charge pas.
- Exploration du run 41850060618 : 6 requêtes « NIGHTFALL seul », toutes vers googlevideo, par la règle
  `/\.googlevideo\.com\/videoplayback\?expire=…/$xhr,3p,method=get…`. C'est un **artefact de mon harnais** :
  `ci/shield_ubo_oracle.mjs:61` ne transmet pas la méthode HTTP à `matchRequest`, donc une règle `$method=` ne
  peut pas correspondre dans uBO, alors que Shield passe « GET ». La correction (ajouter la méthode aux détails
  envoyés à uBO) n'est pas faite, pour ne pas relancer la CI. Accord réel attendu : 100 %, **NON VÉRIFIÉ**.
- Blocages de premières parties dans l'exploration : 39 sur 797, surtout des points de collecte (`a1.api.bbc.com`,
  `developer.mozilla.org/pong/`, `collector.github.com`, `booking.com/c360/v1/track`, `cmp.lemonde.fr`). Aucun ne
  vise un script ou une feuille de style d'interface. Une seule exception `@@` a servi
  (`@@||cmp.lemonde.fr/js/lemonde.min.js$script,1p`).
- Effets non appliqués : sur le jeu adblock-rust, adblock-rust (et non uBO) désigne une ressource `$redirect` pour
  4 984 requêtes que NIGHTFALL annule (4 283 scripts) ; 26 sur l'exploration (tous des scripts). Calcul fait avec
  un moteur adblock-rust doté de ressources factices portant les noms demandés, pas avec uBO. URL nettoyées par
  `$removeparam` : 18 sous-requêtes avec V0, 45 avec la liste URL tracking, 47 navigations. 7 documents
  recevraient une `$csp`.
- Par type (exploration, V0) : `ping` 87 %, `sub_frame` 61,5 %, `xmlhttprequest` 38,3 %, `image` 21,2 %,
  `script` 16 %, `stylesheet` et `font` 0 %.

### 6.3 Ce que ce protocole ne mesure pas

- **Cosmétique dans la page** (éléments réellement masqués, cassures visuelles), scriptlets réellement efficaces,
  murs anti-bloqueur, publicités YouTube : il faut un navigateur NIGHTFALL réel. **NON MESURÉ**.
- **Pop-ups après un clic** (`$popup`) : il faut simuler un geste. **NON MESURÉ**.
- **Pages protégées contre les robots** : 9 pages sur 30 en HTTP 403 depuis la CI (trois runs).
- **Paramètres de pistage** des liens entrants (`utm_*`, `fbclid`) : l'exploration ouvre les pages directement.
  **NON MESURÉ**.

### 6.4 Protocole proposé pour ces trous (missions suivantes)

Ajouter à l'auto-test (`app/src/nf/selftest.rs`, hors du périmètre de cette mission) des pages locales servies
par la CI :

1. **Moment d'injection** : un `<script>` en ligne lit, pendant l'analyse de la page, une constante posée par
   `set-constant`. Le résultat dit si l'injection précède les scripts de la page.
2. **Iframe** : une page avec une iframe tierce qui porte une classe générique masquée. On vérifie le masquage
   dans l'iframe.
3. **Service worker** : une page enregistre un service worker qui appelle une adresse bloquée. On compte la
   requête dans le journal réseau.
4. **`$redirect`** : `noop.js` doit être servi et `onload` (pas `onerror`) doit se déclencher.
5. **`$removeparam`** : une navigation vers `?utm_source=x` doit arriver sans le paramètre.
6. **`$popup`** : `window.open` après un clic simulé vers un domaine visé.

## 7. Recommandation d'architecture (critère 6)

**Décision proposée : garder adblock-rust comme moteur unique, et réécrire autour de lui la couche
d'intégration.** Raisons :
- accord de 99,85 à 100 % avec uBO à listes égales (99,86 % au run 41850060618, à cause d'un artefact du harnais) ;
- 3,5 à 4,7 µs par décision ;
- licence MPL identique à celle de NIGHTFALL ;
- sérialisation rapide ;
- maintenu par Brave ;
- uBO et tsurlfilter sont sous GPL, en JS ou TS, donc non intégrables.

Ce qui manque relève de l'intégrateur (préprocesseur, ressources, CSP, méthodes, navigations) ; Brave le fait
lui-même dans son navigateur.

Classement : P1 = fiabilité ou couverture à faible risque ; P2 = gain net, effort moyen ; P3 = à décider plus tard.
Gains « mesurés » = chiffres de ce rapport ; gains « estimés » = calculs explicites à partir de ces chiffres.

| # | Proposition | Gain | Coût, risque | Licence |
|---|---|---|---|---|
| P1-1 | Résoudre `!#include` (même dossier, comme uBO, **en évaluant `!#if` d'abord**, avec P1-2) à la mise à jour | Mesuré : 18 024 lignes, au plus ≈ 16 568 règles utiles (§4.1) ; accord avec uBO de 99,85 % à 99,98 % ; +289 requêtes bloquées sur le jeu adblock-rust | +30 ms de construction (87 → 117 ms, Linux), +1,4 Mio sérialisé, +1,2 µs de médiane par requête ; risque faible (11 téléchargements de plus par mise à jour, 2,0 Mo de plus) | listes non redistribuées |
| P1-2 | Évaluer `!#if` (Chromium de bureau) avant `add_filter_list` | Mesuré : 191 règles d'autres environnements retirées (767 avec P1-1 : 191 − 72 + 648) ; effet sur le réseau nul ; risque de faux positif cosmétique retiré (non mesuré) | Petit module Rust écrit d'après la doc publique ; tests unitaires faciles | code original |
| P1-3 | Lever la limite de taille de `download()` : `ureq` 2 `into_string` refuse les réponses de plus de 10 Mio (`lib.rs:418-423`). URLhaus (12,2 Mo) et AdGuard Tracking (13,8 Mo) échoueraient donc aujourd'hui, ce que mes mesures, faites avec Python, ne montrent pas. Valider les listes téléchargées (en-tête ABP ou `! Title`, part minimale de règles acceptées) ; garder l'ancienne version en cas d'échec ; ETag / `If-Modified-Since` ; respecter `! Expires` (`read_list_metadata`, `adblock-rust/src/lists.rs:100`) | Robustesse (portail captif, page d'erreur) : **NON MESURÉ** ; bande passante : 5,3 Mo toutes les 12 h aujourd'hui, en grande partie évitables | Faible | — |
| P1-4 | Compteur de règles exact (règles acceptées par le moteur) | Mesuré : affichage 164 012 contre 159 585 réelles | Trivial | — |
| P1-5 | Construire le moteur hors du fil d'E/S et hors du verrou `core()` ; une seule construction au démarrage (pas de construction avec la configuration par défaut suivie d'une seconde au déverrouillage) | Estimé : supprime un blocage du fil d'E/S de 78 à 140 ms au déverrouillage et une construction sur deux au démarrage (gain non mesuré dans le navigateur) | Moyen : fil de travail, remplacement atomique du moteur déjà présent (`RwLock<Arc<Engine>>`) | — |
| P1-6 | Appliquer `$removeparam` : `request.set_url(rewritten_url)` dans `on_before_resource_load` (doc CEF : une URL modifiée vaut redirection), y compris sur les navigations principales (nettoyage seulement, jamais de blocage) ; puis activer la liste URL tracking | Mesuré : 216 règles aujourd'hui, 2 532 de plus avec la liste ; effet sur nos jeux faible (45 sous-requêtes, 47 navigations sur 242 945), effet réel **NON MESURÉ** | Risque de boucle de redirection, à protéger ; test d'auto-test 6.4-5 | listes non redistribuées |
| P1-7 | Ressources `$redirect` écrites par NIGHTFALL (`noop.js`, `noop.txt`, `noop.html`, `noop.css`, `noop.json`, `1x1.gif`, `2x2.png`, `noopmp3-0.1s`, `noopmp4-1s`, alias `noopjs`, `nooptext`, `abp-resource:blank-*`), servies par `get_resource_handler` au lieu d'annuler | Mesuré : 4 984 requêtes du jeu adblock-rust et 26 de l'exploration recevraient un substitut au lieu d'une erreur. Moins de cassures et de détection anti-bloqueur : **NON MESURÉ** | Moyen (CEF `ResourceHandler`) ; les ressources triviales en premier ; les substituts de bibliothèques (`google-ima.js`, 106 règles ; `googletagservices_gpt.js` ; `googletagmanager_gtm.js`) à écrire nous-mêmes d'après le comportement documenté, **jamais copiés de uBO (GPL)** | code original |
| P1-8 | Compléter les scriptlets, par usage : `href-sanitizer`, `set-local-storage-item`, `remove-cookie` (+ `cookie-remover`), `nowebrtc`, `abort-on-stack-trace` (`aost`), `trusted-prevent-dom-bypass`, `noeval`, `replace-node-text` (`rpnt`), `trusted-set-cookie`, `set-cookie`, `trusted-replace-argument`, `nobab`, `nofab`… | Estimé : de 84,3 % à 97,9 % des règles de scriptlet (+444 règles). Avec `set-cookie`, `trusted-set-cookie`, `trusted-click-element` et `set-session-storage-item` en plus, ouvre les listes anti-bandeaux de cookies (§5.6 : aujourd'hui 12 / 2 162 et 59 / 2 468) | Moyen ; chaque scriptlet a son test. La table de correspondance des noms de `agtree` (MIT) peut servir de référence | code original ; MIT compatible pour des données |
| P1-9 | Injecter scriptlets et CSS dans le processus de rendu (`CefRenderProcessHandler::on_context_created`), avec les données de la page transmises à l'avance, au lieu d'`execute_java_script` dans `on_load_start` | Garantit l'exécution avant les scripts de la page (aujourd'hui **NON MESURÉ**, test 6.4-1) | Élevé : architecture CEF multiprocessus, IPC. À confier à l'Architecte (Opus) | — |
| P2-0 | Cache du moteur sérialisé (`Engine::serialize` / `deserialize`) | Estimé : construction de 78–140 ms remplacée par la désérialisation (1,3–3,0 ms) **plus** la lecture d'un fichier de 7 à 9 Mio et `use_resources`, **non mesurés** | **Risque élevé, sécurité.** Pour une même version de crate, adblock-rust saute la vérification FlatBuffers (`ref/adblock-rust/src/engine.rs:441-444` et `469-479`) et la somme de contrôle est un seahash non cryptographique (`src/data_format/mod.rs:102-110`). La doc dit « A local rewrite of the stored file is outside this crate's threat model ». Un fichier réécrit dans le profil serait donc chargé sans contrôle de structure. La première construction a lieu avant déverrouillage (`app/src/nf/mod.rs:107`), donc sans clé. Conditions minimales : authentifier le fichier par un MAC à clé (clé du profil, donc seulement après déverrouillage) et reconstruire avant déverrouillage, ou ne pas mettre en cache. Hors du lot à faible risque ; **à confier à l'Architecte** | — |
| P2-1 | Réponse du masquage générique au **cadre** qui l'a demandé, pas au cadre principal (`client.rs:476-484`) ; règle d'exception par site appliquée au site de la page aussi pour l'injection | Couverture cosmétique des iframes : **NON MESURÉ** (test 6.4-2) | Faible à moyen (identifier le cadre) | — |
| P2-2 | Filtrer les requêtes sans navigateur (service workers) par `RequestContextHandler::resource_request_handler` | Trou de vie privée possible : **NON MESURÉ** (test 6.4-3) | Moyen ; risque de casser des applications web hors ligne | — |
| P2-3 | Vérifier les navigations principales pour `$document`/`$all` (listes badware, URLhaus), avec une page « bloquée par Shield » et un bouton « continuer » | Mesuré : 3 695 + 1 285 règles deviennent actives ; URLhaus (108 991 règles) devient utile | Moyen ; faux positifs sur des navigations : il faut l'interface de dérogation. Touche la sécurité, donc décision du propriétaire | listes non redistribuées |
| P2-4 | `$popup` : extraire ces règles avant le moteur, les compiler dans un second moteur interrogé seulement dans `on_before_popup` (source = page qui ouvre) | Mesuré : 3 764 règles aujourd'hui perdues ; gain réel **NON MESURÉ** | Moyen ; code original | — |
| P2-5 | Cosmétique moins coûteuse : générer une fois par moteur la CSS des 531 sélecteurs génériques complexes et la mettre en cache ; mettre en cache par hôte le résultat de `url_cosmetic_resources` (réutilisé par `generic_css`) ; ne réévaluer les règles procédurales que sur les nœuds ajoutés | Estimé : environ 100 µs sur 150–210 par cadre côté navigateur, et environ 70 µs par signalement générique ; coût dans la page **NON MESURÉ** | Faible à moyen | — |
| P2-6 | Ajouter Peter Lowe (activée par défaut chez uBO), et proposer en option EasyList Cookie et Fanboy Annoyances (après P1-8) | Mesuré : Peter Lowe +2 071 / −3 (jeu adblock-rust), +24 (exploration), pour un coût de construction négligeable. Mais 951 de ces 2 071 blocages en plus (46 %) sont des blocages de premières parties (8 699 → 9 650), et sur l'exploration ils passent de 39 à 46 : c'est le signal de risque de faux positifs du §6.1 | Moyen : vérifier les blocages de premières parties ajoutés (risque de faux positifs) avant activation par défaut ; **licence de Peter Lowe à vérifier** | listes non redistribuées |
| P2-7 | Réglages par site (« cosmétique désactivée », « scriptlets désactivés », mode strict comme Brave), journal des requêtes bloquées par onglet, sélecteur d'élément | Diagnostic des faux positifs par l'utilisateur : **NON MESURÉ** | Interface : missions de conception M-0005 / M-0006 | — |
| P3-1 | AdGuard Tracking Protection | Mesuré : +2 158 / −194 ; +29 sur l'exploration | Construction ×3 (347 ms au run 41845086446), 28,8 Mio sérialisé, pic 96,3 Mio (run 41848082350, textes compris) ; 13,8 Mo à télécharger, au-delà de la limite de `download()` (P1-3) : à réévaluer après P1-3 et P1-5 | GPL, non redistribuée |
| P3-2 | `$csp` (35 règles), filtrage HTML `##^` (71), `$replace`, `$urlskip` (215), `$denyallow` (83) | Faible en volume | CEF ne permet pas de modifier les en-têtes de réponse dans `on_resource_response` (doc : « The response object cannot be modified ») ; le filtrage HTML demanderait `get_resource_response_filter`. Coût élevé | — |
| P3-3 | Pas d'ajout par défaut d'AdGuard Base ni d'AdGuard Français | Mesuré : Base +586 / −181 sur le jeu adblock-rust (+2 / −1 sur l'exploration) ; ses exceptions débloquent 181 requêtes, ce qui peut aussi bien corriger des faux positifs que réduire la couverture : **NON ÉTABLI**. Français ≈ Liste FR (+289 / −239) | Gain faible pour un coût de construction +60 % (Base) | — |
| — | **Ne pas remplacer adblock-rust** | Aucun gain mesurable, risque élevé | — | — |

Ordre conseillé : P1-1 à P1-5 (une mission, faible risque ; P2-0 en est exclu, tout se vérifie par des tests du module et ce banc),
puis P1-6 à P1-8, puis P1-9 et P2-1 / P2-2 avec l'Architecte. Après chaque étape : rejouer ce banc (jobs
« Audit Shield ») et comparer aux chiffres du §5–6.

## 8. Points à signaler au Directeur

- `ci/shield_ubo_oracle.mjs` exécute uBlock Origin (GPL) en CI, depuis son dépôt cloné, comme outil de mesure.
  Rien n'est copié ni distribué. Le Directeur ou le propriétaire peut préférer retirer cette étape avant toute
  intégration.
- Les jobs « Audit Shield » ne tournent que sur la branche de cette mission ou au lancement manuel.
  `preserve.py` compte les jobs de CI : les retirer avant intégration serait une disparition à déclarer. Les
  garder sans effet sur les autres PR est possible tel quel.
- L'exemple `shield_audit.rs` est compilé par `cargo test` sur les 3 OS (vérifié : jobs « Modules » verts). Il
  n'est jamais inclus dans le navigateur.
- Hors périmètre : Clippy échoue déjà sur `develop`, dans `crates/nf-shield/src/lib.rs:151`
  (`clippy::manual_range_patterns`). L'échec est masqué par `continue-on-error` dans le job « Modules ».

## 9. Corrections après l'audit indépendant

Chaque point a été revérifié dans les sources ou les journaux avant d'être corrigé :

| Point | Correction |
|---|---|
| M1 | Cache du moteur sérialisé retiré du lot à faible risque : P2-0, risque élevé (sécurité), à confier à l'Architecte. P1-5 ne garde que la construction hors du fil d'E/S et la construction unique |
| m1 | Gain des `!#include` ramené à « au plus ≈ 16 568 règles utiles » (§4.1) |
| m2 | Artefact `$method` de l'oracle expliqué (§6.2) |
| m3 | 9 pages en 403 dans chacun des 3 runs |
| m4 | Commande du §3.1 remplacée : elle affiche « 99 34 » |
| m5 | tsurlfilter : préprocesseur dans `@adguard/filters-downloader`, non lu |
| m6 | Hypothèse `/somtag` corrigée ; pic de 96,3 Mio ; effets `$redirect` attribués à adblock-rust |
| m7 | Peter Lowe et AdGuard Base nuancés |
| m8 | Limite de 10 Mio de `download()` ajoutée (P1-3) ; `$to` reclassé comme risque de faux positifs |
| m9 | Run 41850060618 intégré (§1.2, §6.2, sorties dans `mesures/`) |

