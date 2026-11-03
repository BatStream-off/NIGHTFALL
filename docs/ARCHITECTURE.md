# Architecture

NIGHTFALL est un processus principal en Rust qui pilote Chromium via CEF (`cef` 156.3.0, bindings `cef-rs`). L'interface est faite de pages HTML/CSS/JS servies depuis l'exécutable sur le schéma interne `nightfall://app/`.

```
Fenêtre NIGHTFALL (CEF Views, boîte horizontale)
├─ side    : BrowserView shell.html?mode=side   (onglets verticaux, optionnel)
└─ colonne : Panel (boîte verticale)
   ├─ top     : BrowserView shell.html?mode=top  (onglets + barre d'outils, ou écran NIGHTFALL LOCK plein écran)
   └─ contenu : Panel (fill layout) : une BrowserView par onglet, seule l'active est visible
```

- Les vues web sont en **style Chrome sans barre d'outils Chrome** (`ChromeToolbarType::NONE`), condition pour l'API des extensions Chrome de CEF.
- Le **verrouillage** fait occuper toute la fenêtre à la vue `top` (animation de démarrage puis écran LOCK). Le contenu est masqué, et la base, le coffre et les cookies restent fermés.
- La **navigation privée** utilise un `RequestContext` sans chemin de cache (en mémoire), propre aux fenêtres privées.

## Processus principal (`app/src/nf/`)

| Fichier | Rôle |
| --- | --- |
| `mod.rs` | Démarrage : refus des options interdites, purge programmée, chargement du magasin de clés, initialisation de CEF |
| `app.rs` | `CefApp` : enregistrement du schéma, création de la première fenêtre |
| `windows.rs` | Fenêtres, onglets, mise en page, épinglage, mise en veille, événements vers l'interface (thread UI uniquement) |
| `client.rs` | Gestionnaires CEF : adresse/titre, popups, chargement, Shield réseau et cosmétique, raccourcis, anti-phishing, VideoGuard |
| `scheme.rs` | Schéma `nightfall://app/` : fichiers intégrés + API interne (POST, origine vérifiée) |
| `api.rs` | Commandes de l'interface : LOCK, onglets, favoris, historique, coffre, Security Center, urgence, synchronisation |
| `session.rs` | Session Vault (cookies scellés) et restauration des onglets |
| `downloads.rs` | Gestionnaire de téléchargements, signatures, marquage d'origine |
| `timers.rs` | Verrouillage après inactivité et après mise en veille, pression mémoire, instantané de session |
| `state.rs` | État partagé (`Core` derrière un mutex), registre des navigateurs |

## Crates Rust indépendantes de CEF (testées en CI sur Linux, Windows et macOS)

`nf-core`, `nf-crypto`, `nf-hwkey`, `nf-auth`, `nf-storage`, `nf-security`, `nf-emergency`, `nf-shield`, `nf-videoguard`, `nf-vault`, `nf-perf`, `nf-downloads`, `nf-sync` : voir le README pour le rôle de chacune.

## Construction et distribution

- `cargo xtask bundle` / `cargo xtask package` (dossier `xtask/`) : compile, puis assemble l'application avec le runtime CEF grâce aux utilitaires `cef::build_util`. Produit `NIGHTFALL.app` + `.dmg` sur macOS, et un dossier `NIGHTFALL/` sur Windows.
- Windows avec sandbox : `nightfall.exe` est le `bootstrap.exe` de CEF, qui charge `nightfall.dll` (`RunWinMain`).
- Installateur Windows : Inno Setup (`packaging/windows/nightfall.iss`), installation par utilisateur.
- CI (`.github/workflows/ci.yml`) : tests des modules sur 3 OS, clippy, cargo-deny, compilation + empaquetage + test de démarrage de l'application sur Windows et macOS, artefacts téléchargeables.
