# Installer NIGHTFALL

Les versions compilées sont produites automatiquement par GitHub Actions, sans aucun coût. Elles **ne sont pas signées** par un certificat payant : Windows et macOS afficheront donc un avertissement au premier lancement. C'est attendu, et voici comment procéder.

## Où trouver les fichiers

- **Versions publiées** : onglet *Releases* du dépôt GitHub.
- **Dernière compilation de développement** : onglet *Actions* → workflow *CI* → dernière exécution réussie → section *Artifacts* (`NIGHTFALL-Windows`, `NIGHTFALL-macOS`). Il faut être connecté à GitHub pour télécharger un artefact.

## Windows 10 / 11 (64 bits)

Deux formats :

- `NIGHTFALL-x.y.z-windows-x64-setup.exe` : installateur. Installation dans votre profil utilisateur, sans droits administrateur. Crée un raccourci dans le menu Démarrer (et sur le Bureau si vous le souhaitez).
- `NIGHTFALL-x.y.z-windows-x64-portable.zip` : version portable. Décompressez puis lancez `nightfall.exe`.

Au premier lancement, SmartScreen peut afficher « Windows a protégé votre ordinateur ». Cliquez sur **Informations complémentaires**, puis **Exécuter quand même**. Ce message apparaît parce que l'application n'est pas signée par un certificat payant, pas parce qu'elle est dangereuse. Vous pouvez vérifier que le fichier provient bien de la CI du dépôt.

Désinstallation : *Paramètres → Applications → NIGHTFALL Browser*. Le désinstalleur demande s'il faut aussi supprimer votre profil chiffré (`%LOCALAPPDATA%\NIGHTFALL`). Vos téléchargements ne sont jamais supprimés.

## macOS (Apple Silicon)

1. Ouvrez `NIGHTFALL-x.y.z-macos-arm64.dmg` et glissez **NIGHTFALL** dans **Applications**.
2. Au premier lancement, macOS indique que l'application ne peut pas être vérifiée. Ouvrez **Réglages Système → Confidentialité et sécurité**, puis cliquez sur **Ouvrir quand même** en face de NIGHTFALL.

L'application porte une signature « ad hoc » (gratuite), pas de signature Apple Developer ID, qui coûte 99 $ par an. Elle n'est donc pas notarisée.

Désinstallation : supprimez `NIGHTFALL.app` des Applications. Votre profil chiffré se trouve dans `~/Library/Application Support/NIGHTFALL`. Supprimez ce dossier si vous voulez aussi effacer vos données.

## Premier lancement

1. Animation NIGHTFALL (désactivable dans Paramètres).
2. Création de votre **code NIGHTFALL** : 6 à 12 chiffres. Les codes trop simples sont refusés.
3. Affichage unique de votre **Recovery Key**. Notez-la hors de l'ordinateur. Sans votre code ni cette clé, vos données NIGHTFALL sont définitivement illisibles : aucune porte dérobée n'existe.
4. NIGHTFALL télécharge ensuite les listes de filtres EasyList et EasyPrivacy pour NIGHTFALL Shield.

## Compiler soi-même

Prérequis : Rust stable. Sous Windows, il faut aussi Visual Studio Build Tools, Perl (fourni avec Git for Windows) et NASM. Sous macOS, il faut les outils Xcode en ligne de commande.

```sh
cargo test --workspace --exclude nightfall --exclude xtask   # tests des modules
cargo xtask bundle            # application de développement dans dist/
cargo xtask package           # version release (+ .dmg sur macOS)
```

La première compilation télécharge le runtime CEF (Chromium), qui pèse plusieurs centaines de Mo.
