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

- [x] `FormView` généré depuis l'OpenAPI (types, enums, requis, writeOnly, sélecteur de relations, erreurs par champ, PATCH des seuls champs modifiés) — R7.1–R7.4, R7.7
- [x] Édition `$EDITOR` YAML + diff + PATCH, réouverture avec l'erreur en commentaire — R7.5
- [x] Delete avec multi-sélection (`Espace`) + `ConfirmView` (Non par défaut) — R7.6, R13.2
- [x] Suivi des tâches lancées (barre de statut, 1 s) + modal de progression détachable + notifications — R6.3, R6.4
- [x] Cancel (`Ctrl-K`) et purge (`:purge`) de tâches — R6.5, R6.6
- [x] Sync (`s`), publish (`P`), distribute (`D`, URL affichée en fin de tâche) ; mise à jour d'une distribution via `u`/`e` — R8.1–R8.3
- [x] Modify (`a` ajout depuis le contenu d'une version ou vers un dépôt choisi, `r` retrait), rollback (`R`), suppression de version — R8.4–R8.6
- [x] `:orphans` (estimation du nombre d'orphelins), `:reclaim` (pré-rempli avec la sélection), `:repair` — R8.7–R8.9
- [x] Masquage des actions en readonly (test unitaire sur les raccourcis + refus des touches) — R13.1

**Acceptation M2** : scénario file complet (remote → repo → sync → publish → distribute → modify → rollback → delete) réalisé uniquement dans pulply ; en `--readonly`, aucune requête mutante émise (vérifié par wiremock).

## M3 — Vues sur mesure + upload + diff + xray

- [x] Trait `PluginView` (colonnes + lignes d'usage du describe) + résolution plugin → générique ; colonnes absentes du schéma ignorées
- [x] Vues rpm (NEVRA, advisories, modulemd, arbres, groupes ; `dnf config-manager`), container (tags → manifest, manifests multi-arch avec nombre de manifests listés suivis via le describe, blobs ; `podman pull`), file, python (`pip --index-url`), ansible, deb, ostree, certguard (aperçu des certificats) — R11.1–R11.8. ostree n'est pas présent dans le conteneur de référence : sa vue n'est pas vérifiée contre un serveur réel. Options sync/publish rpm via les formulaires générés.
- [x] `FilePickerView` + upload par chunks (`U`, taille `upload_chunk_size`) + création du contenu via `upload` (formulaire urlencoded) + ajout au dépôt — R9. Types sans champ `upload` (container, rôles ansible, hugging_face, sources deb) non téléversables depuis la TUI.
- [x] `DiffView` (`x` : résumé, consécutif via `repository_version_added/removed`, arbitraire par différence d'ensembles ; détail limité à 200 unités par côté) — R10.1, R10.2
- [x] `XrayView` (`X` : distribution → publication → version → dépôt → remote + content guard ; depuis un dépôt : remote, dernière version, publications et distributions qui le servent) — R10.3

**Acceptation M3** : uploader un RPM et un fichier, les voir dans la vue rpm/file, faire le diff v(n-1)/v(n) et l'xray de la distribution.

## M4 — Admin, personnalisation, i18n, distribution

- [x] Vues RBAC & admin, `:me`, gestion des 403 — R12. Colonnes users/groups/roles/access policies/upstream pulps/task schedules/signing services ; collections enfants (membres, rôles) liées depuis le describe, création et retrait de membres ; `o` : `list_roles` / `add_role` / `remove_role` ; `m` : toute action POST de la ressource (reset, replicate, exports, imports, set_label…). Les permissions effectives de `:me` se limitent à ce que l'utilisateur peut lire (un non-admin ne lit ni son utilisateur ni ses rôles par défaut).
- [x] Skins (`dark`, `light`, par contexte) — R14.1. Skins embarqués dans `crates/pulply/skins/`, `inherit` pour n'en surcharger qu'une partie.
- [x] Hotkeys, alias, colonnes configurables — R14.2–R14.4
- [x] Plugins shell — R14.5. Variables aussi exportées en `PULPLY_<NOM>` ; au premier plan, retour à la TUI après Entrée.
- [x] Traduction FR complète — R15.1
- [x] Release : cargo-dist, `.deb`, `.rpm`, Homebrew, image OCI, crates.io — R16.3. Vérifié localement (`dist plan`, paquets deb/rpm, `cargo package --workspace`, image distroless de 6,5 Mo testée contre le conteneur) ; le workflow GitHub n'a pas encore tourné.
- [x] README (installation, config, raccourcis), captures

**Acceptation M4** : un utilisateur non admin voit uniquement les actions permises ; la release produit tous les artefacts.
