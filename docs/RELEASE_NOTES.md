**0.1.5**
- **Sécurité du verrouillage :**
  - les raccourcis clavier n'agissent plus quand NIGHTFALL est verrouillé (seul le verrouillage d'urgence reste disponible) ;
  - au verrouillage, le son de tous les onglets est coupé et les vidéos sont mises en pause ; VideoGuard ne fait plus rien tant que NIGHTFALL est verrouillé ;
  - les autorisations mémorisées (position, presse-papiers…) sont retirées du fichier de préférences de Chromium pendant le verrouillage et à la fermeture, puis rendues au déverrouillage : elles ne sont plus lisibles en clair sans votre code.
- **Réglages plus sûrs :** un réglage manquant ou ajouté par une version future ne peut plus faire perdre tous vos réglages.
- **Corrections :**
  - la croix ferme vraiment l'onglet (barre horizontale et onglets verticaux) ; fermer un onglet d'arrière-plan ne change plus l'onglet affiché ;
  - les menus déroulants ne restent plus en mémoire après fermeture ;
  - ouvrir un menu juste après en avoir fermé un autre fonctionne aussi sur macOS ;
  - le bouton Accueil fonctionne depuis une page web.

**0.1.4**
- **Mises à jour automatiques :**
  - NIGHTFALL recherche les nouvelles versions au démarrage puis toutes les 6 h ;
  - il les télécharge en arrière-plan et les installe à la fermeture, ou tout de suite avec « Redémarrer pour mettre à jour » dans le menu ;
  - chaque mise à jour est signée et vérifiée (signature, empreinte, version plus récente uniquement) ;
  - réglages dans Paramètres › Mises à jour.
- **Nouvelle page Paramètres façon Brave/Chrome :** menu latéral et recherche. Sections :
  - démarrage, apparence, moteur de recherche (DuckDuckGo, Brave Search, Qwant, Startpage, Ecosia, Google, Bing ou personnalisé) ;
  - confidentialité : cookies tiers, Global Privacy Control, HTTPS d'abord, protection WebRTC, DNS sécurisé, effacement des données par période ;
  - autorisations des sites mémorisées, téléchargements (dossier, toujours demander), langues, système (accélération matérielle) ;
  - NIGHTFALL LOCK, mises à jour, réinitialisation.
- **Fonctions du navigateur :**
  - plein écran vidéo (YouTube) et F11 ;
  - recherche dans la page ;
  - zoom mémorisé par site ;
  - menu clic droit sur les onglets et sur les pages ;
  - suggestions locales dans la barre d'adresse ;
  - rouvrir l'onglet fermé, couper le son d'un onglet ;
  - impression, enregistrement de la page, code source ;
  - icônes des sites dans les onglets ;
  - clic molette ou Ctrl/⌘ + clic pour ouvrir en arrière-plan ;
  - raccourcis clavier Windows et macOS complets.
- **Corrections :**
  - une page qui se fermait elle-même (connexion « avec Google ») fermait toute la fenêtre ;
  - les sites sans HTTPS ne pouvaient pas s'ouvrir ;
  - nouveaux messages « page plantée » et « page qui ne répond pas ».

**0.1.3**
- **Menus en vraies fenêtres déroulantes :** les menus (principal, Shield, mots de passe) s'ouvrent par-dessus la page au lieu de la pousser vers le bas. Ils se ferment avec Échap ou un clic ailleurs.
- **Barre du navigateur inspirée de Brave :**
  - le bouton « + » est collé aux onglets ;
  - le bouton Shield est placé dans la barre d'adresse ;
  - le menu principal est en liste, avec les raccourcis clavier ;
  - nouvelles icônes dessinées pour NIGHTFALL.
- **Nouvel onglet inspiré de Brave :**
  - quatre fonds d'écran originaux ;
  - raccourcis en tuiles ;
  - carte « Statistiques » (éléments bloqués, bande passante et temps gagnés : estimations) ;
  - panneau de personnalisation.

**0.1.2**
- **Fenêtre :** elle peut être agrandie, réduite, redimensionnée et fermée, avec les boutons standard de Windows et de macOS. Sa taille s'adapte à l'écran.
- **NIGHTFALL Shield, bien plus efficace :**
  - les listes uBlock filters sont ajoutées, dont les règles YouTube ;
  - les scriptlets (règles `##+js`) sont écrits pour NIGHTFALL ;
  - les règles procédurales sont prises en charge (`:has-text`, etc.) ;
  - le masquage générique par classe et identifiant est ajouté ;
  - l'injection se fait dès le début du chargement ;
  - un défaut est corrigé : une seule règle invalide annulait jusqu'à 200 règles de masquage.
- **VideoGuard :** si une publicité vidéo YouTube passe malgré Shield, elle est coupée et avancée jusqu'à la fin. Une indication s'affiche quand des segments sont repérés.
- **Nouvelle interface :** palette de l'emblème, coins coupés hexagonaux, police Chakra Petch, icônes vectorielles, nouveaux onglets, écran de verrouillage avec une ville nocturne et nouvel onglet repensé.

Première version utilisable de NIGHTFALL : navigateur Chromium (CEF) écrit en Rust, pour Windows 10/11 (64 bits) et macOS (Apple Silicon).

**Pré-version.** Les binaires ne sont pas signés par un certificat payant (budget 0 €) : Windows SmartScreen et macOS Gatekeeper afficheront un avertissement au premier lancement. La marche à suivre est dans `docs/INSTALL.md`. Le fichier `SHA256SUMS.txt` permet de vérifier les téléchargements.

## Fichiers

- `NIGHTFALL-…-windows-x64-setup.exe` : installateur Windows (sans droits administrateur)
- `NIGHTFALL-…-windows-x64-portable.zip` : version portable Windows
- `NIGHTFALL-…-macos-arm64.dmg` : macOS Apple Silicon

## Vérifié automatiquement avant publication

Sur Windows et sur macOS, une capture d'écran prise pendant les tests confirme que l'interface s'affiche réellement. Dans le vrai navigateur : création du code (6 à 12 chiffres), Recovery Key, navigation HTTPS, historique chiffré, blocage NIGHTFALL Shield, verrouillage avec Session Vault, refus d'un mauvais code, coffre de mots de passe chiffré, purge d'urgence, Security Center. Les 82 tests des modules passent aussi sur Linux, Windows et macOS.

## Limites connues

Le détail est dans `docs/STATUS.md`.

- Extensions Chrome non disponibles dans cette version (limite de CEF).
- Netflix, Disney+, Prime Video et les autres services avec DRM ne sont **pas compatibles** : Widevine exige une licence payante.
- H.264/AAC non pris en charge (brevets). YouTube fonctionne en VP9/AV1.
- Interface, raccourcis, verrouillage après mise en veille, TPM réel : à valider sur une vraie machine.
