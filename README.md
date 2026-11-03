# NIGHTFALL Browser

**Browse Beyond the Darkness.**

NIGHTFALL est un navigateur indépendant pour Windows et macOS, écrit principalement en Rust, avec le moteur Chromium embarqué via [CEF](https://github.com/chromiumembedded/cef) et les bindings Rust [`cef-rs`](https://github.com/tauri-apps/cef-rs).

> Statut : **en développement**. Aucune version stable n'est encore publiée. L'état réel de chaque fonctionnalité (testée ou non) est tenu à jour dans [docs/STATUS.md](docs/STATUS.md).

## Principes

- Cœur en Rust : sécurité, profils, sessions, filtrage, téléchargements, données.
- Interface entièrement personnalisée (HTML/CSS/JS servi en interne), sans IA.
- Aucune biométrie : NIGHTFALL LOCK fonctionne uniquement avec un code.
- Aucun serveur personnel ni abonnement requis.
- Les protections de Chromium (sandbox, Site Isolation) ne sont jamais désactivées.

## Organisation

| Dossier | Contenu |
| --- | --- |
| `crates/nf-core` | Chemins du profil, permissions, filtrage des journaux |
| `crates/nf-crypto` | XChaCha20-Poly1305, HKDF, HMAC |
| `crates/nf-hwkey` | Secret lié à l'appareil (TPM 2.0, Trousseau macOS) |
| `crates/nf-auth` | NIGHTFALL LOCK : code, temporisation, Recovery Key |
| `crates/nf-storage` | Base SQLCipher : favoris, historique, réglages, téléchargements, sessions |
| `crates/nf-security` | Security Core : anti-phishing, commutateurs interdits, Session Vault, extensions |
| `crates/nf-emergency` | Emergency Lockdown, purge, Auto-Purge |
| `crates/nf-shield` | Bloqueur (moteur adblock-rust de Brave, MPL-2.0) |
| `crates/nf-videoguard` | Saut des segments sponsorisés (API SponsorBlock) |
| `crates/nf-vault` | Gestionnaire de mots de passe chiffré |
| `crates/nf-perf` | Gestion de la mémoire et mise en veille des onglets |
| `crates/nf-downloads` | Contrôles de sécurité des téléchargements |
| `crates/nf-sync` | Synchronisation facultative des favoris, chiffrée de bout en bout |
| `app/` | Application CEF (fenêtres, onglets, interface) |
| `docs/` | Architecture, sécurité, licences, état d'avancement |

## Licence

Code de NIGHTFALL : [MPL-2.0](LICENSE). Les composants tiers gardent leur licence, voir [docs/LICENSES.md](docs/LICENSES.md).
