# Modèle de sécurité de NIGHTFALL

NIGHTFALL ne promet aucune sécurité absolue. Ce document décrit ce qui est protégé, contre quoi, et les limites connues.

## Données au repos

| Donnée | Protection |
| --- | --- |
| Favoris, historique, réglages, métadonnées de téléchargements, session d'onglets | Base SQLCipher (AES-256) avec la clé de catégorie « database » |
| Mots de passe | Fichier `vault.bin` scellé (XChaCha20-Poly1305) avec la clé « vault » |
| Cookies, quand NIGHTFALL est verrouillé ou fermé | Session Vault : tous les cookies, y compris HttpOnly, sont lus par le processus principal, scellés avec la clé « cookies », puis **supprimés de Chromium**. Ils sont restaurés au déverrouillage. |
| Cookies pendant la navigation | Magasin de Chromium, chiffré par l'OS (DPAPI ou Trousseau) |
| Cache, stockage local des sites | Profil Chromium, accessible au seul compte utilisateur (droits 0700 sur macOS) |

## Hiérarchie de clés (NIGHTFALL LOCK)

```
clé maître (256 bits aléatoires)
 ├─ enveloppe code     : HKDF( Argon2id(code, sel) ‖ secret matériel )
 ├─ enveloppe recovery : HKDF( Recovery Key 160 bits, sel )
 └─ clés par catégorie : database, vault, cookies, sync (aléatoires, scellées par la clé maître)
```

- **Secret matériel** : clé RSA dans la puce TPM 2.0 sous Windows, dont la signature PKCS#1 v1.5 est déterministe et ne quitte jamais la puce. Sous macOS, graine dans le Trousseau, liée à l'application signée. Sans TPM, repli sur DPAPI, lié au compte Windows et non au matériel ; l'interface l'affiche. Conséquence : un profil copié sur une autre machine ne permet pas de deviner le code hors ligne.
- **Aucune biométrie.**
- **Temporisation** : 4 essais libres, puis 30 s, 1 min, 5 min, 15 min, et 1 h par essai. Après 20 échecs, seule la Recovery Key permet de déverrouiller. Le compteur est authentifié (HMAC dérivé du secret matériel) : supprimé ou modifié, il est considéré comme élevé (9 échecs).
- **Purge cryptographique** : détruire une clé de catégorie rend ses données illisibles, même si un fichier subsiste sur le disque.

## Anti-vol de sessions (token grabbing)

| Technique de vol connue | Réponse NIGHTFALL |
| --- | --- |
| Relancer le navigateur avec `--remote-debugging-port` pour lire les cookies via DevTools | NIGHTFALL **refuse de démarrer** avec ces options (liste dans `crates/nf-security/src/switches.rs`) |
| Lire la base de cookies sur disque | Elle est vide quand NIGHTFALL est verrouillé ou fermé (Session Vault) |
| Lire la mémoire du navigateur en cours d'exécution | Non bloquable face à un malware administrateur. Le verrouillage automatique réduit la fenêtre d'exposition. |
| Extension malveillante | Installation uniquement manuelle, analyse des permissions (cookies + tous les sites = risque critique), outils de développement désactivés par défaut |
| Fuite dans les journaux | Tous les journaux passent par `nf_core::redact::sanitize` |

**Limite** : aucun navigateur ne peut protéger totalement des sessions contre un logiciel malveillant déjà exécuté avec les droits de l'utilisateur ou des droits administrateur. Le chiffrement « App-Bound » de Chrome a été contourné de cette façon.

## Interface interne

- Les pages `nightfall://app/` sont intégrées à l'exécutable. Leur CSP interdit tout script externe ou inline, et elles ne peuvent pas être affichées dans une iframe.
- L'API interne n'accepte que les requêtes POST venant d'une page NIGHTFALL dans la frame principale. Le schéma n'étant pas CORS, une page web ne peut pas l'appeler.
- Une page web ne peut pas naviguer vers `nightfall://`. Seul NIGHTFALL lui-même ouvre ses pages internes.
- Afficher un mot de passe, purger des données ou changer le code exige de saisir à nouveau le code.

## Emergency System

- **Niveau 1, Lockdown** : masque les onglets, ferme les fenêtres privées, scelle les cookies et efface les clés de la mémoire.
- **Niveau 2, Purge** : détruit d'abord les clés, puis les fichiers. La purge est limitée au dossier du profil et ne suit jamais les liens symboliques ; un test CI le vérifie avec des fichiers témoins. Les téléchargements ne sont jamais touchés.
- **Niveau 3, Auto-Purge** : désactivée par défaut. Elle exige au moins 2 signaux critiques indépendants. Aucune page ni extension ne peut la déclencher.
- L'effacement physique sur SSD, dans les sauvegardes ou les dossiers synchronisés **n'est pas garanti**.

## Durcissement Chromium

Sandbox (processus séparés via `bootstrap.exe` sous Windows, helpers sandboxés sous macOS) et Site Isolation : réglages par défaut de Chromium, jamais désactivés. Un contrôle CI échoue si un commutateur affaiblissant apparaît dans `app/src`.

## Signaler une vulnérabilité

Ouvrez un *Security Advisory* privé sur GitHub (onglet *Security*). Ne publiez pas de détails exploitables dans une issue publique.
