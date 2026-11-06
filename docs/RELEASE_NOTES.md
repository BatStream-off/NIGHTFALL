**0.3.0**
- **Interface :** couleurs et dégradés superflus retirés, raccourcis du Nouvel onglet sur une surface commune, emoji remplacés par des icônes, Security Center plus lisible.
- **Paramètres en centre de contrôle :** Security Center puis 13 sections (Général, Apparence et thèmes, Confidentialité, Sécurité, NIGHTFALL Shield, VideoGuard, Autorisations des sites, Téléchargements, Navigation, Moteurs de recherche, Performances, Accessibilité, Options avancées). L'ancienne page Security Center séparée redirige vers les Paramètres.
- **Nouveaux réglages :**
  - moteurs par mot-clé (« @w chat ») et moteurs ajoutés par vous ; suggestions du moteur pendant la frappe, désactivées par défaut ;
  - import et export des favoris (HTML) et des mots de passe (CSV, export protégé par le code) ;
  - navigateur par défaut (Windows et macOS) : les liens des autres applications s'ouvrent dans NIGHTFALL, après le code s'il est verrouillé ;
  - effacer l'historique et les cookies à la fermeture ; préchargement des pages désactivé par défaut ;
  - taille de police minimale, réduction des animations, blocage de la lecture automatique, zooms par site ;
  - sites toujours actifs (jamais mis en veille), gestionnaire de certificats ;
  - autorisations : 9 types de plus (capteurs, MIDI, réseau local…) et fiche par site.
- **Événements de sécurité :** journal chiffré de 30 jours (sites sans HTTPS, phishing, téléchargements à risque, codes incorrects, verrouillage d'urgence), nom du site seulement.
- **Windows 11 :** Snap Layouts au survol du bouton Agrandir.
- **NIGHTFALL Shield :** réponses neutres pour les règles `$redirect` (moins de pages cassées) ; 8 scriptlets de plus.
- **Mesures :** démarrage, chargement et mémoire relevés à chaque build.

**0.2.0**
- **Nouveau design « Crépuscule » :**
  - barre de titre intégrée au header : sous Windows, boutons réduire, agrandir et fermer dans la barre d'onglets ; sur macOS, boutons natifs intégrés ;
  - couleurs, tailles, boutons, champs et cases harmonisés dans tout le navigateur ; police du système pour l'interface ;
  - contraste vérifié pour tous les textes.
- **Thèmes** (Paramètres › Apparence) :
  - 6 thèmes prêts : Crépuscule, Minuit, Aurore, Braise, Brume, Aube (clair) ;
  - éditeur : créer, modifier, dupliquer, supprimer vos thèmes, avec aperçu en direct ;
  - les couleurs illisibles sont refusées ; le thème s'applique sans redémarrer à toutes les barres, menus et pages NIGHTFALL.
- **Paramètres :**
  - Security Center intégré : état des protections, alertes, sites avec exceptions, accès aux réglages avancés ;
  - nouvelles sections NIGHTFALL Shield et VideoGuard.
- **Lancement plus rapide :** le moteur Shield est construit après le déverrouillage, en arrière-plan ; animation de démarrage raccourcie (0,85 s au lieu de 1,8 s) ; sur macOS, plus de blocage du réseau par l'interface (jusqu'à 2 s).
- **NIGHTFALL Shield :**
  - environ 16 000 règles uBlock qui manquaient (directives !#include et !#if) ;
  - listes jusqu'à 32 Mo ;
  - une liste modifiée sur le disque par autre chose que NIGHTFALL est refusée (empreinte gardée dans les réglages chiffrés).
- **VideoGuard :**
  - 11 catégories (accroche, moment fort, accès exclusif en plus) ;
  - son coupé pendant les segments « sourdine », étiquette « vidéo entière », bouton « Aller au moment fort », segments en couleur sur la barre de lecture ;
  - corrections : retour dans un sponsor imbriqué, segment au début non ignoré, « Passer » non reproposé, vitesse perdue après une publicité, choix « Revenir » oublié ;
  - vie privée : la requête ne révèle plus vos catégories choisies.

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
