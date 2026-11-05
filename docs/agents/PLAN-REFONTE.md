# Plan d'exécution de la refonte NIGHTFALL (proposition du Directeur, 2026-11-05)

Statut : **en attente de validation du propriétaire.** Rien n'est fusionné sans son accord.

## 1. Résultats des audits

| Mission | Objet | Verdict de l'audit indépendant |
|---|---|---|
| M-0001 | Registre des fonctionnalités (198, dont 29 prouvées, 76 partielles, 93 sans preuve) | ACCEPTÉ (tentative 2) |
| M-0002 | Diagnostic des bugs et mesures | REFUSÉ 2 fois (faits confirmés ; risque d'une correction et mesure mémoire insuffisants) |
| M-0003 | Audit de Shield et étude comparative | REFUSÉ 2 fois (mesures confirmées ; risques de 3 recommandations mal évalués) |
| M-0004 | Audit de VideoGuard face à SponsorBlock | ACCEPTÉ (tentative 2) |
| M-0005 | Interface, barre de titre intégrée, thèmes | REFUSÉ 1 fois, correction en cours |
| M-0006 | Paramètres et Security Center intégré | REFUSÉ 1 fois, second audit en cours |

### Faits établis et vérifiés
- **Sécurité (LOCK)**
  - Les raccourcis clavier agissent pendant le verrouillage, sur Windows et macOS : ils ouvrent et ferment des onglets et des fenêtres, changent le zoom et passent en plein écran.
  - Les onglets masqués continuent de tourner pendant le verrouillage : vidéo, son, et requêtes VideoGuard.
- **Vie privée**
  - Les décisions d'autorisation mémorisées sont écrites en clair par Chromium dans son fichier `Preferences`.
  - Les listes Shield en cache sont chargées sans authentification avant le déverrouillage, avec la permission « de confiance » pour 5 listes.
- **Compatibilité** : ajouter un champ à certaines structures de réglages (Shield, VideoGuard, Performances, Auto-Purge) ferait perdre **tous** les réglages enregistrés. C'est à corriger avant toute autre modification.
- **Bugs signalés**
  - La croix ne ferme pas l'onglet : 0 fermeture sur 60 vrais clics. L'appui de la souris reconstruit la barre avant le relâchement.
  - Chaque menu déroulant fermé laisse son navigateur et son processus en mémoire.
  - Sur macOS, fermer un menu ferme aussi le menu suivant.
  - « Accueil » ne fait rien depuis une page web.
  - Sur macOS, `page.info` bloque le fil réseau de CEF jusqu'à 2,16 s. Sur Windows c'est négligeable. Son effet sur le chargement des pages reste à mesurer.
- **Shield**
  - Le moteur de Brave (adblock-rust 0.13.5) est déjà utilisé et décide comme uBlock Origin à 99,85 %.
  - Les écarts viennent de l'intégration : `!#include` non résolus (environ 16 500 règles utiles manquantes), `!#if` ignoré, 15 % des scriptlets absents, `$redirect`, `$removeparam`, `$csp` et `$to` sans effet.
  - Le moteur est construit deux fois au démarrage, de façon synchrone.
- **VideoGuard**
  - Il couvre 8 catégories sur 12 et 1 type d'action sur 5.
  - 22 défauts recensés, dont des segments imbriqués mal gérés et la vitesse de lecture perdue après une publicité.
  - Aucun code de SponsorBlock n'est repris.
- **Interface**
  - L'interface actuelle utilise 18 tailles de police, 16 rayons d'arrondi, 11 hauteurs de bouton et environ 130 couleurs codées en dur.
  - La barre de titre intégrée est réalisable : un prototype passe la CI sur Windows et macOS, avec des réserves (Snap Layouts de Windows 11 absent, plusieurs cas non testés).

## 2. Plan par lots (ordre d'exécution)

Chaque lot suit la même chaîne :
1. une mission d'exécution sur une branche `agent/*` ;
2. une CI verte sur Windows et macOS ;
3. un audit indépendant Opus ;
4. un tableau des preuves ;
5. **votre autorisation de fusion** dans `develop` ;
6. après votre validation, une version publiée (pré-version signée et mise à jour automatique).

| Lot | Contenu | Version visée |
|---|---|---|
| **L0 — Sécurité et fiabilité** (priorité absolue) | Réglages enregistrés robustes (aucune perte possible) ; raccourcis bloqués pendant LOCK ; onglets mis en pause et coupés pendant LOCK ; croix des onglets ; fuite et course des menus ; Accueil ; `page.info` non bloquant (mesuré avant/après) ; copie Chromium des décisions mémorisées effacée au verrouillage et à la fermeture ; anti-phishing honnête | 0.1.5 |
| **L1 — Shield** | Résolution `!#include` avec `!#if` ; moteur construit une seule fois, hors du fil réseau ; scriptlets manquants les plus utilisés ; ressources `$redirect` ; compteur exact ; téléchargement borné des grandes listes ; intégrité des listes en cache ; réglages par site | 0.1.6 |
| **L2 — Système de design et chrome** | Identité « Crépuscule », jetons, icônes originales, header avec barre de titre intégrée, onglets, menus, panneaux, animations, contrastes AA | 0.2.0 |
| **L3 — Paramètres en centre de contrôle et Security Center intégré** | 13 sections, Security Center (état, exceptions par site, événements, protections, réglages avancés), réglages manquants par rapport à Chrome et Brave | 0.2.x |
| **L4 — Thèmes** | Moteur de thèmes centralisé, éditeur (créer, modifier, dupliquer, supprimer, aperçu), thèmes prédéfinis | 0.2.x |
| **L5 — VideoGuard complet** | 12 catégories, sourdine, étiquettes de vidéo entière, barre de segments, corrections des 22 défauts, inactif pendant LOCK, choix pour la navigation privée | 0.3.0 |
| **L6 — Performances et finalisation** | Mesures avant/après, fluidité, mémoire, registre des preuves complété (objectif : chaque fonctionnalité prouvée), tests manuels guidés, version finale | 1.0 |

## 3. Décisions demandées au propriétaire
1. Valider ce plan et l'ordre des lots.
2. M-0002 et M-0003, refusées deux fois : autoriser le Directeur à corriger lui-même les derniers points (textuels), puis un audit final.
3. Protection de la branche `main` sur GitHub (recommandée).
4. Identité visuelle « Crépuscule » (maquettes de M-0005), Snap Layouts de Windows 11 (accepter son absence ou prévoir une mission dédiée), thème de l'écran de verrouillage.
5. Vie privée : remettre à zéro la copie Chromium des décisions mémorisées au verrouillage et à la fermeture (recommandé).
6. VideoGuard : identifiant pour soumettre et voter des segments (oui/non) ; comportement en navigation privée (désactivé par défaut, recommandé) ; accélération des publicités YouTube conservée avec un avertissement honnête.
7. Licences : exécution de uBlock Origin (GPL) en CI comme outil de mesure seulement (recommandé : oui, rien n'est copié).
8. Règle 7 : retirer les contrôles en double (Security Center / Paramètres) et rediriger l'ancienne page Security Center.
