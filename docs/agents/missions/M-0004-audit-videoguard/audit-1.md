# Audit M-0004 (tentative 1)

(Version condensée archivée par le Directeur ; contenu fidèle à l'audit rendu.)

- **Auditeur** : claude-opus-5-5, agent neuf — **Objet** : commit 72740e4 — **Verdict** : **REFUSÉ** (4 MAJEUR)

Banc relancé : sortie identique octet pour octet ; D-01 et D-02 prouvés (lecteur simulé) ;
169 références `fichier:ligne` contrôlées, > 50 relues ; aucune copie GPL (0 ligne commune
≥ 30 caractères) ; `preserve.py` sans disparition.

## MAJEUR
1. **VideoGuard reste actif pendant le verrouillage** (VG-25 et §6.5 disent l'inverse) :
   `lock_now` (`api.rs:816-829`) ne touche pas `c.videoguard` ; `on_locked`/`apply_mode`
   (`windows.rs:518-557, 623-630`) masquent les onglets sans pause ; `on_navigation`
   (`client.rs:227-241`) ne vérifie pas `is_unlocked()`. Ajouter D-19, corriger VG-25,
   critère : verrouiller pendant la lecture puis vidéo suivante → aucune requête.
   *(Vérifié par le Directeur.)*
2. **Deux défauts de saut non détectés** : (a) segments imbriqués A 10-40 / B 15-20 :
   saut à 40 puis retour à 20 (`videoguard.js:67-84`) ; (b) un segment commençant à 0
   n'est jamais réarmé (`t < s.start`, `videoguard.js:125`). Ajouter (VÉRIFIÉ) et
   critères de VG-11 / VG-14.
3. **Confusion mode utilisateur / type d'action du segment** : `mute` et `full` sont des
   types d'action ; l'utilisateur a des interrupteurs globaux `muteSegments` et
   `fullVideoSegments` (`SB/src/config.ts:22-23, 391-392`) ; `CategorySkipOption` n'a pas
   de sourdine (`SB/src/types.ts:32-38`) ; `full` aussi pour `sponsor` et `selfpromo`
   (`SBS/src/config.ts:24-27`). Corriger O1, S4, C6, VG-01, VG-02, VG-04.
4. **Licences** : attribution présente dans `app/ui/about.html:14` (affirmation contraire
   fausse) ; obligations CC BY-NC-SA 4.0 §3(a)(1) incomplètes (avis de droit d'auteur,
   avis d'exclusion de garantie, lien) ; définition NC (§1) à citer. Texte légal
   disponible via chromium.googlesource.com (`licenses/CC-BY-NC-SA-4.0`).

## MINEUR
Cas E du banc non concluant (segment à 0) ; D-03 scénario réel = « tout sur Ignorer » ;
D-12 faux en général (`parseParams.ts:32-33`) ; défaut non vu : réinjection pour la même
vidéo remet `state.skipped` à zéro (`videoguard.js:145`) ; VG-30 surestime la discrétion
(`window.__nfMed`, `browsing.rs:269`) ; VG-70 sans critère ; citations décalées
(`nf-sync/src/lib.rs:19`, `skipProfiles.ts:85-101`) ; résumé §0 trop affirmatif sur D-01.
