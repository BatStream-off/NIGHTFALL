# Licences et obligations de redistribution

Le code de NIGHTFALL est publié sous **MPL-2.0** (voir `LICENSE`). Le budget du projet est de 0 € : seuls des composants gratuits et légalement redistribuables sont utilisés. Ce document n'est pas un avis juridique.

| Composant | Licence | Utilisation | Obligation |
| --- | --- | --- | --- |
| Chromium | BSD-3-Clause + licences tierces | Moteur web, via CEF | Fournir les mentions de licence (page `chrome://credits` du runtime) |
| CEF | BSD-3-Clause | Intégration de Chromium | Mention de copyright |
| `cef-rs` (tauri-apps) | Apache-2.0 OR MIT | Bindings Rust ; `app/src/nf/mac.rs` est adapté de l'exemple `cefsimple` | Mention de copyright |
| adblock-rust (Brave) | MPL-2.0 | Moteur de NIGHTFALL Shield | Publier toute modification des fichiers MPL (aucune pour l'instant) |
| EasyList, EasyPrivacy, Liste FR, Fanboy | GPL-3.0 ou CC BY-SA 3.0 | Téléchargées par le navigateur de l'utilisateur, jamais intégrées au dépôt | Attribution (Security Center) |
| uBlock filters (listes uAssets) | GPL-3.0 | Téléchargées par le navigateur de l'utilisateur, jamais intégrées au dépôt | Attribution (Security Center) |
| uBlock Origin (code, scriptlets, ressources) | GPL-3.0-or-later | **Non utilisé** : les scriptlets des règles `##+js(...)` sont des implémentations originales de NIGHTFALL (MPL-2.0, `crates/nf-shield/scriptlets/`) qui reprennent seulement les noms et arguments documentés | Aucune |
| Chakra Petch (police) | SIL OFL 1.1 | Titres et chiffres de l'interface, sous-ensemble intégré (`app/ui/fonts/`) | Licence jointe (`OFL-ChakraPetch.txt`) |
| electron/rcedit | MIT | Outil de compilation en CI (icône de nightfall.exe), non distribué | Aucune |
| SponsorBlock (API et données) | CC BY-NC-SA 4.0 | Interrogée à l'exécution par NIGHTFALL VideoGuard | Attribution, **usage non commercial** |
| Extension SponsorBlock | LGPL-3.0 | Non utilisée (client réécrit en Rust) | Aucune |
| SQLCipher, OpenSSL (intégrés via rusqlite) | BSD-3-Clause / Apache-2.0 | Base de données chiffrée | Mentions de licence |
| Crates Rust | MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib, Unicode | Diverses | Vérifiées par `cargo-deny` (`deny.toml`) |

## Ce qui n'est pas inclus, faute de licence gratuite

| Élément | Raison | Alternative légale actuelle |
| --- | --- | --- |
| Widevine (Netflix, Disney+, Prime Video, Max, Spotify) | Licence Google et certification VMP payante (castLabs) | Aucune. Ces services **ne sont pas annoncés comme compatibles**. |
| H.264 / AAC logiciels | Brevets (Via LA) et compilation personnalisée de CEF | VP9, AV1, Opus, WebM (YouTube fonctionne). Twitch et certaines vidéos H.264 peuvent ne pas se lire. |
| Signature Windows et notarisation Apple | Payantes (Azure Artifact Signing 9,99 $/mois, Apple 99 $/an) | Binaires non signés (Windows) et signés ad hoc (macOS) |
| Google Safe Browsing commercial (Web Risk) | Payant au-delà d'un quota | Analyse anti-phishing locale |
