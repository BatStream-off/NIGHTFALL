# Audit M-0001 (tentative 2)

(Version condensée archivée par le Directeur ; contenu fidèle à l'audit rendu.)

- **Auditeur** : claude-opus-5-5, agent neuf
- **Objet** : commit f401264 (`agent/M-0001-registre-v2`), run CI 41844972643 (tous jobs verts)
- **Verdict** : **ACCEPTÉ** — 0 BLOQUANT, 0 MAJEUR, 9 MINEUR

## Vérifications
Script `/tmp/audit2-M-0001/check.py` + lecture : 198 lignes, 585 citations trouvées ;
120/120 tests, 20/20 étapes ; > 45 lignes de preuves lues, > 30 « Où » lus ; 44 branches
de raccourcis de `client.rs:760-813` présentes avec plateformes ; Couverture
arithmétiquement exacte ; M1–M3 et m1–m12 de l'audit 1 réellement corrigés ;
`preserve.py` : aucune disparition ; `scope` : respecté.

## Problèmes mineurs (à corriger dans une mission ultérieure)
- P1 : F-098, F-150, F-151, F-188, F-166 classées « prouvée » alors que la preuve n'en
  couvre qu'une partie (Couverture stricte : jusqu'à 24 prouvées / 81 partielles).
- P2 : F-133 — `vault_fill` remplit aussi le champ identifiant (`api.rs:1095-1099`).
- P3 : F-032 — `Cmd+[` / `Cmd+]` à marquer « (macOS) » (`client.rs:788-789`).
- P4 : F-031 — l'application quitte avec sa dernière fenêtre ; le Dock la relance
  (`windows.rs:656-671`, `mac.rs:43`).
- P5 : entrées manquantes : carte LOCK du Security Center (`security.js:111-116`), fond
  d'écran depuis la page Nouvel onglet (`newtab.js:245-258`), Échap dans la barre
  d'adresse (`shell.js:740-741`), « Actualiser l'analyse » (`extensions.js:20`),
  « Télécharger et installer » et « Notes de version » (`settings.js:500-514`).
- P6 : F-166 — « dernier écrit gagne » seulement dans le fichier partagé (`api.rs:1154-1165`).
- P7 : F-083 / F-037 — l'auto-test appelle `https.proceed`, il ne clique pas `#go`.
- P8 : F-113 — le texte d'interface ne mentionne pas l'exclusion des onglets sonores.
- P9 : L-16 — liste des effets non exhaustive (Ctrl+N, Ctrl+H/J…).

## Hors périmètre
- H1 (processus) : PR #24 (`agent/M-0001-registre-v2` → `main`) créée 14:10:42Z et
  fusionnée 14:10:52Z, avant la fin de la CI et avant l'audit.
- H2 : `tab.discard` et `password_reused_elsewhere` sans entrée d'interface.

L'Auditeur n'a rien modifié.
