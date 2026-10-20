# pulply — Plan de livraison

Chaque jalon produit un binaire utilisable et testé contre `deploy/compose.yml`. Références : `requirements.md`.

## M1 — Socle + visualisation (lecture seule de fait)

- [x] Workspace `pulply-api` / `pulply`, CI (fmt, clippy, tests), hook de panique qui restaure le terminal — R16.2
- [x] `deploy/compose.yml` (image `pulp/pulp`, admin/password) + capture de la fixture `api.json`
- [x] Config : `config.toml`, import de `cli.toml` et de ses profils, netrc, `PULPLY_PASSWORD`, prompt — R1.1–R1.3
- [x] CLI `clap` : `--context`, `--config`, `--readonly`, `-c`, `--refresh`, `-l` — R1.5
- [x] `PulpClient` : basic auth, TLS, erreurs typées, garde readonly — R1.2, R13.1
- [x] Status + détection de version, avertissement < 3.50 — R1.7, R2.1 (ETag utilisé dès que le serveur en renvoie, sans table de capacités)
- [x] Registre OpenAPI : fetch, cache, classification, alias, résolution href/PRN → type — R2.1–R2.5
- [x] Shell TUI : en-tête, sidebar, fil d'Ariane, barre de statut, aide, mode commande avec autocomplétion, filtre `/` — R3.1–R3.5
- [x] `ListView` (pagination, tri, filtres API, repli sans `fields` sur erreur serveur) + `DetailView` (describe) — R3.6, R3.7
- [x] Navigation repo → versions → contenu
- [x] Dashboard — R4
- [x] Poller adaptatif, ETag, pause, refresh — R5
- [x] Vues tâches (liste, détail, progression, erreurs) — R6.1, R6.2
- [x] `:ctx` (bascule à chaud), écran d'erreur de connexion — R1.4, R1.6
- [x] Infrastructure i18n (`t!()` partout, `en.yml` et `fr.yml` complets) — R15
- [x] Logs fichier — R16.1

**Acceptation M1** : contre le conteneur, naviguer dans tous les types de ressources de tous les plugins installés, voir le dashboard et suivre une tâche lancée depuis `pulp-cli` en direct.

## M2 — CRUD + workflows

- [ ] `FormSchema` + `FormView` (types, enums, requis, writeOnly, sélecteur de relations, erreurs par champ) — R7.1–R7.4, R7.7
- [ ] Édition `$EDITOR` YAML + diff + PATCH — R7.5
- [ ] Delete avec multi-sélection + `ConfirmModal` — R7.6, R13.2
- [ ] `TaskTracker` + `TaskModal` détachable + notifications — R6.3, R6.4
- [ ] Cancel et purge de tâches — R6.5, R6.6
- [ ] Sync, publish, distribute (URL affichée) — R8.1–R8.3
- [ ] Modify (ajout / retrait), rollback, suppression de version — R8.4–R8.6
- [ ] Orphan cleanup, reclaim, repair — R8.7–R8.9
- [ ] Masquage des actions en readonly (tests snapshot) — R13.1

**Acceptation M2** : scénario file complet (remote → repo → sync → publish → distribute → modify → rollback → delete) réalisé uniquement dans pulply ; en `--readonly`, aucune requête mutante émise (vérifié par wiremock).

## M3 — Vues sur mesure + upload + diff + xray

- [ ] Trait `PluginView` + résolution plugin → générique
- [ ] Vues rpm, container, file, python, ansible, deb, ostree, certguard — R11.1–R11.8
- [ ] `FilePickerView` + uploader par chunks + création de contenu + ajout au repo — R9
- [ ] `DiffView` (résumé, consécutif, arbitraire) — R10.1, R10.2
- [ ] `XrayView` — R10.3

**Acceptation M3** : uploader un RPM et un fichier, les voir dans la vue rpm/file, faire le diff v(n-1)/v(n) et l'xray de la distribution.

## M4 — Admin, personnalisation, i18n, distribution

- [ ] Vues RBAC & admin, `:me`, gestion des 403 — R12
- [ ] Skins (`dark`, `light`, par contexte) — R14.1
- [ ] Hotkeys, alias, colonnes configurables — R14.2–R14.4
- [ ] Plugins shell — R14.5
- [ ] Traduction FR complète — R15.1
- [ ] Release : cargo-dist, `.deb`, `.rpm`, Homebrew, image OCI, crates.io — R16.3
- [ ] README (installation, config, raccourcis), captures

**Acceptation M4** : un utilisateur non admin voit uniquement les actions permises ; la release produit tous les artefacts.
