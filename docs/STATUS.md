# État réel de NIGHTFALL

Ce document distingue ce qui est **implémenté et vérifié automatiquement**, ce qui est **implémenté mais pas encore vérifié sur une vraie machine**, et ce qui est **non disponible**. Il est mis à jour à chaque étape. Une fonctionnalité n'est marquée « vérifiée » qu'après un test réel.

Légende :
- ✅ vérifié par des tests automatisés (CI Linux, Windows et macOS) ou par l'auto-test de bout en bout dans le vrai navigateur ;
- 🟡 implémenté, compilé, à valider manuellement sur ta machine ;
- ⛔ non disponible (licence payante, limite technique), avec la raison.

## Moteur et application

| Élément | État | Détail |
| --- | --- | --- |
| Compilation Windows (x64) et macOS (Apple Silicon) | ✅ | Workflow `CI`, job *Application* |
| Démarrage de l'application + chargement de l'interface | ✅ Windows, ✅ macOS | Test de démarrage : processus vivant après 25 s, profil créé, interface chargée (`interface prête`) |
| Style des vues CEF | Alloy | CEF n'accepte qu'**une seule** vue en style Chrome par fenêtre (erreur observée en CI : `Cannot add multiple Chrome style BrowserViews`). NIGHTFALL a besoin de plusieurs vues (interface + une par onglet), donc toutes sont en style Alloy. |
| Installateur Windows (Inno Setup), version portable .zip, image .dmg macOS | voir dernière CI | Artefacts téléchargeables. Non signés (budget 0 €). |

## Modules (tests unitaires, 82 tests sur 3 OS)

| Module | État | Couvert par les tests |
| --- | --- | --- |
| Cryptographie (XChaCha20-Poly1305, HKDF, HMAC) | ✅ | Chiffrement, falsification, unicité des nonces |
| NIGHTFALL LOCK (code, Argon2id, secret matériel, temporisation, Recovery Key) | ✅ | Création, déverrouillage, temporisation progressive, seuil de 20 échecs, compteur falsifié, profil copié sans secret matériel = inutilisable, purge cryptographique |
| Secret matériel TPM (Windows) / Trousseau (macOS) | ✅ DPAPI Windows, ✅ Trousseau macOS (test unitaire), 🟡 TPM | Les machines de test GitHub n'ont pas de puce TPM : NIGHTFALL y a basculé sur DPAPI, comme prévu. Le Trousseau est testé par un aller-retour réel. Le TPM reste à valider sur ta machine. |
| Base chiffrée SQLCipher (favoris, historique, réglages, téléchargements, sessions) | ✅ | Fichier illisible sans la clé, dossiers, recherche, filtres de date |
| Security Core : commutateurs interdits, anti-phishing, risques des extensions, format du Session Vault | ✅ | Typosquatting, homographes, sous-domaines trompeurs, IP + HTTP |
| Emergency : purge limitée au profil, règles d'Auto-Purge | ✅ | Fichiers témoins hors profil et liens symboliques intacts, signal isolé = pas de purge |
| Shield (adblock-rust) | ✅ | Blocage réseau, exceptions, liste blanche par site, filtrage cosmétique, statistiques |
| VideoGuard (SponsorBlock) | ✅ (logique) | Analyse des réponses, catégories, identifiants vidéo. 🟡 saut réel sur YouTube à vérifier |
| Coffre-fort de mots de passe | ✅ | Chiffrement, correspondance d'origine stricte, générateur sans biais |
| Performance (politique de mise en veille) | ✅ (logique) | Onglets protégés jamais déchargés. 🟡 effet réel à mesurer |
| Téléchargements (types dangereux, noms de fichiers, signatures) | ✅ / 🟡 | Assainissement testé ; vérification Authenticode et codesign à valider sur ta machine |
| Synchronisation facultative des favoris (dossier, chiffrée) | ✅ | Deux appareils, mauvaise phrase refusée |

## Auto-test de bout en bout (dans le vrai navigateur, en CI)

Feature `selftest`, réservée aux tests et jamais livrée (un contrôle de sécurité de la CI le vérifie). Le scénario : création du code → Recovery Key → page d'accueil → navigation réelle (HTTP puis HTTPS) vers `example.com` → historique enregistré → Shield bloque une requête → verrouillage + Session Vault écrit → mauvais code refusé → déverrouillage → ajout au coffre (aucun mot de passe en clair sur disque) → purge d'urgence des mots de passe → Security Center.

| Système | État |
| --- | --- |
| Windows | ✅ `SELFTEST OK` (secret de l'appareil : DPAPI, car les machines de test n'ont pas de TPM) |
| macOS | ✅ `SELFTEST OK`, avec deux adaptations propres à la machine de test (voir ci-dessous) |

Adaptations de l'auto-test macOS (feature `selftest` uniquement, jamais présentes dans les versions livrées) :
- le secret de l'appareil est un fichier local à la place du Trousseau, et Chromium utilise un Trousseau simulé (`use-mock-keychain`). Sur une machine de test, toute demande d'accès au Trousseau attend une autorisation que personne ne peut donner. Le Trousseau réel est vérifié séparément par un test unitaire ;
- le résolveur DNS du système est utilisé à la place de celui de Chromium (configuration réseau particulière des machines de test).

Comportement vérifié : une adresse en `http://` est automatiquement ouverte en `https://` quand le site le permet (mise à niveau HTTPS de Chromium).

**À savoir sur un vrai Mac :** au premier lancement, macOS peut demander si NIGHTFALL peut utiliser le Trousseau (pour le secret de l'appareil et pour le chiffrement des cookies de Chromium). Il faut accepter. Comme l'application n'est pas signée par un compte développeur Apple (budget 0 €), cette demande peut revenir après chaque mise à jour.

## À valider manuellement (ta machine)

Interface visuelle (rendu, animation, onglets verticaux), raccourcis clavier, onglets multiples et fenêtres, navigation privée, remplissage des mots de passe, téléchargements (pause et reprise), VideoGuard sur une vraie vidéo, verrouillage après inactivité et après mise en veille, désinstallation.

## Non disponible

| Fonction | Raison | Alternative |
| --- | --- | --- |
| Extensions Chrome | L'API d'extensions de CEF n'existe qu'en style Chrome, limité à une vue par fenêtre (voir plus haut). | Le gestionnaire affiche l'analyse des risques. L'installation d'extensions n'est pas fonctionnelle dans cette version. |
| Netflix, Disney+, Prime Video, Max, Spotify | Widevine : licence et certification payantes | Aucune. **Non annoncés comme compatibles.** |
| H.264 / AAC (Twitch, certaines vidéos) | Brevets + compilation spéciale de CEF | YouTube fonctionne (VP9, AV1, Opus) |
| Signature de code reconnue par Windows et Apple | Payante | Avertissements au premier lancement (voir INSTALL.md) |
| Benchmarks comparatifs | Exigent du matériel physique identique pour tous les navigateurs | Protocole et scripts prêts dans `bench/` |
