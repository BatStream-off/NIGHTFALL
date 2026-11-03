Première version utilisable de NIGHTFALL : navigateur Chromium (CEF) écrit en Rust, pour Windows 10/11 (64 bits) et macOS (Apple Silicon).

**Pré-version.** Les binaires ne sont pas signés par un certificat payant (budget 0 €) : Windows SmartScreen et macOS Gatekeeper afficheront un avertissement au premier lancement. La marche à suivre est dans `docs/INSTALL.md`. Le fichier `SHA256SUMS.txt` permet de vérifier les téléchargements.

## Fichiers

- `NIGHTFALL-…-windows-x64-setup.exe` : installateur Windows (sans droits administrateur)
- `NIGHTFALL-…-windows-x64-portable.zip` : version portable Windows
- `NIGHTFALL-…-macos-arm64.dmg` : macOS Apple Silicon

## Vérifié automatiquement avant publication

Sur Windows et sur macOS, dans le vrai navigateur : création du code (6 à 12 chiffres), Recovery Key, navigation HTTPS, historique chiffré, blocage NIGHTFALL Shield, verrouillage avec Session Vault, refus d'un mauvais code, coffre de mots de passe chiffré, purge d'urgence, Security Center. Les 82 tests des modules passent aussi sur Linux, Windows et macOS.

## Limites connues

Le détail est dans `docs/STATUS.md`.

- Extensions Chrome non disponibles dans cette version (limite de CEF).
- Netflix, Disney+, Prime Video et les autres services avec DRM ne sont **pas compatibles** : Widevine exige une licence payante.
- H.264/AAC non pris en charge (brevets). YouTube fonctionne en VP9/AV1.
- Interface, raccourcis, verrouillage après mise en veille, TPM réel : à valider sur une vraie machine.
