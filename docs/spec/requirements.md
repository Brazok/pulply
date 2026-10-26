# pulply — Exigences

TUI Rust/ratatui pour visualiser et administrer [Pulp 3](https://pulpproject.org), sur le modèle de [k9s](https://k9scli.io) pour Kubernetes.

Format des critères : EARS (`QUAND … ALORS pulply DOIT …`). Chaque exigence porte un identifiant stable référencé par `design.md` et `tasks.md`.

## 1. Contexte et décisions validées

| Sujet | Décision |
|---|---|
| Utilisateurs | Tous profils : admin/SRE, dev/release engineer, diagnostic Katello. Limitation via readonly. |
| Plugins | Tous les plugins officiels. Vues génériques depuis l'OpenAPI du serveur + vues sur mesure pour les plugins prioritaires. |
| Niveau de gestion | CRUD complet + workflows (sync, publish, distribute, modify, rollback, orphans, reclaim). |
| Auth | Basic auth + `~/.netrc`. |
| Contextes | Import des profils `~/.config/pulp/cli.toml` + config propre `~/.config/pulply/config.toml`. |
| Navigation | k9s (mode commande `:`, filtre `/`, hotkeys) + sidebar arborescente repliable. |
| Écran d'accueil | Dashboard par défaut, configurable (config ou `-c <vue>`). |
| Rafraîchissement | Polling adaptatif, ETag si supporté, pause. |
| Tâches | Modal de progression détachable → barre de statut. |
| Édition | Formulaires générés depuis l'OpenAPI + édition YAML dans `$EDITOR`. |
| Sécurité | `--readonly` (global et par contexte) + confirmations sur actions destructives. |
| Upload | Oui, sélecteur de fichiers local, upload par chunks avec progression. |
| Avancé | Diff de versions, vue relations (xray), RBAC & admin. |
| Compatibilité | pulpcore ≥ 3.50 ; fonctions récentes activées selon la version détectée. |
| Tests | Conteneur `pulp/pulp` (pulp-oci-images) via docker compose. |
| Langue | i18n FR + EN, sélection par locale. |
| Personnalisation | Skins, hotkeys & alias, colonnes configurables, plugins shell. |
| Distribution | crates.io, GitHub Releases (Linux/macOS x86_64/arm64), RPM/DEB/Homebrew, image conteneur. |
| Livraison | Jalons verticaux M1 → M4, chacun utilisable. |
| Nom | `pulply` (binaire et crate). |

## 2. Hors périmètre

- Bascule de domains (multi-tenant) : non retenu. Le paramètre `domain` d'un profil est respecté pour construire les URLs, sans sélecteur dans l'UI.
- Auth mTLS, OAuth2, bearer/header personnalisé.
- Respect de `dry_run` de `cli.toml` (remplacé par `readonly` par contexte).
- Vue JSON brute et copie presse-papiers (l'édition YAML couvre l'inspection complète).
- Logs des workers Pulp (non exposés par l'API REST).
- pulpcore < 3.50.

## 3. Exigences

### R1 — Connexion et contextes

- **R1.1** Au démarrage, pulply DOIT charger `~/.config/pulply/config.toml` puis importer chaque section `[cli]` / `[cli-<nom>]` de `~/.config/pulp/cli.toml` comme contexte (`default` pour `[cli]`, `<nom>` sinon). Un contexte défini dans la config pulply avec le même nom l'emporte.
- **R1.2** pulply DOIT supporter par contexte : `base_url`, `api_root` (défaut `/pulp/`), `domain`, `username`, `password`, `verify_ssl` (booléen ou chemin de CA), `timeout`, `readonly`, `refresh`.
- **R1.3** QUAND aucun mot de passe n'est fourni ALORS pulply DOIT chercher `~/.netrc` (machine = hôte de `base_url`), puis la variable `PULPLY_PASSWORD`, puis demander le mot de passe dans un prompt masqué.
- **R1.4** QUAND l'utilisateur tape `:ctx` ALORS pulply DOIT lister les contextes (nom, URL, utilisateur, readonly, état de joignabilité) et basculer sur Entrée sans redémarrer.
- **R1.5** Options CLI : `--context <nom>`, `--config <chemin>`, `--readonly`, `-c/--command <vue>`, `--refresh <secondes>`, `-l/--log-level`, `--version`, `--help`.
- **R1.6** QUAND la connexion ou l'authentification échoue ALORS pulply DOIT afficher l'erreur (statut HTTP, message) dans un écran dédié avec les actions réessayer / changer de contexte / quitter, sans paniquer.
- **R1.7** QUAND la version de pulpcore détectée est < 3.50 ALORS pulply DOIT afficher un avertissement persistant dans l'en-tête et continuer en mode « au mieux ».
- **R1.8** pulply DOIT appliquer le `domain` comme pulp-cli : QUAND le serveur annonce les domaines activés (`domain_enabled` du statut) ALORS les URLs DOIVENT être préfixées par `{api_root}<domain>/api/v3/`, avec `default` si aucun domaine n'est configuré ; SINON le domaine DOIT être ignoré (`{api_root}api/v3/`).
- **R1.9** QUAND un `domain` différent de `default` est configuré mais que le serveur n'a pas les domaines activés ALORS pulply DOIT l'ignorer et le signaler par une notification à la connexion.

### R2 — Découverte des ressources

- **R2.1** À la connexion, pulply DOIT lire `GET {api_root}api/v3/status/` (toujours sans domaine, cette route n'étant pas rattachée à un domaine dans pulpcore) (versions des composants, workers, content apps, stockage, DB, Redis) et `GET {api_root}api/v3/docs/api.json`.
- **R2.2** pulply DOIT mettre en cache le schéma OpenAPI sur disque, avec pour clé `base_url` + versions des composants, et ne le recharger que si les versions changent ou sur `:refresh-schema`.
- **R2.3** pulply DOIT construire un registre de ressources à partir des chemins OpenAPI : catégorie (repositories, remotes, distributions, publications, content, acs, contentguards, exporters, importers…), plugin, type, opérations disponibles (list, read, create, update, partial_update, delete et actions comme sync, modify, add_role…).
- **R2.4** Tout plugin installé sur le serveur DOIT apparaître dans la sidebar et dans l'autocomplétion de `:` même sans vue sur mesure.
- **R2.5** pulply DOIT exposer des alias courts : `repo`, `remote`, `dist`, `pub`, `content`, `task`, `worker`, `user`, `group`, `role`, `orphan`, etc. ; avec qualifieur de plugin, ex. `:repo rpm`, `:content container/manifests`.

### R3 — Interface et navigation

- **R3.1** La disposition DOIT comporter : en-tête (contexte, utilisateur, version pulpcore, readonly, indicateur de refresh), sidebar repliable (`Tab`/`Ctrl-B`), zone principale, fil d'Ariane, barre de statut (tâches actives, notifications), barre d'aide des raccourcis contextuels.
- **R3.2** `:` DOIT ouvrir le mode commande avec autocomplétion et historique.
- **R3.3** `/` DOIT filtrer la liste courante côté client ; `/` suivi de `-q <expr>` ou `-f clé=valeur` DOIT passer le filtre à l'API (`name__icontains`, `q`, filtres OpenAPI du endpoint).
- **R3.4** `Entrée` DOIT descendre dans la ressource (repo → versions → contenu), `Échap` DOIT remonter d'un niveau, et l'historique de navigation DOIT être conservé (`[` / `]`).
- **R3.5** `?` DOIT afficher l'aide des raccourcis de la vue courante.
- **R3.6** Les listes DOIVENT gérer la pagination `limit/offset` de façon transparente (chargement incrémental au défilement), le tri par colonne (`Shift+lettre` comme k9s, `ordering` côté API si disponible) et afficher le total (`count`).
- **R3.7** `d` (describe) DOIT afficher le détail d'une ressource en champs lisibles, hrefs résolus en noms quand c'est possible.
- **R3.8** L'UI DOIT rester réactive (pas de blocage > 50 ms du thread de rendu) pendant les appels réseau.

### R4 — Dashboard

- **R4.1** Le dashboard DOIT afficher : santé (DB, Redis, workers en ligne / manquants, content apps), versions des composants, stockage (total / utilisé / libre), tâches en cours / en attente / échouées sur 24 h, compteurs par type de repository.
- **R4.2** QUAND un worker ou une content app manque ALORS le dashboard DOIT le signaler en couleur d'erreur.
- **R4.3** La vue de démarrage DOIT être configurable (`start_view` dans la config, `-c` en CLI).

### R5 — Rafraîchissement

- **R5.1** La vue active DOIT être rafraîchie toutes les `refresh` secondes (défaut 5 s, minimum 1 s).
- **R5.2** QUAND au moins une tâche suivie est `waiting` ou `running` ALORS les tâches suivies DOIVENT être interrogées toutes les secondes.
- **R5.3** QUAND le serveur supporte ETag / `If-None-Match` (pulpcore ≥ 3.122) ALORS pulply DOIT l'utiliser et réutiliser la donnée en cache sur une réponse 304.
- **R5.4** `Ctrl-R` DOIT forcer un rafraîchissement ; `p` DOIT mettre en pause / reprendre le polling (indicateur dans l'en-tête).
- **R5.5** Les vues non visibles NE DOIVENT PAS être interrogées.

### R6 — Tâches

- **R6.1** `:task` DOIT lister les tâches (état, nom, ressources concernées, worker, durée, création) avec filtres par état et par nom.
- **R6.2** Le détail d'une tâche DOIT afficher `progress_reports` (barres), `error` (trace), `created_resources` (navigables) et les sous-tâches / task group.
- **R6.3** QUAND une action utilisateur renvoie une tâche (202 + `task`) ALORS pulply DOIT ouvrir un modal de progression en direct.
- **R6.4** `Échap` sur le modal DOIT le détacher : la tâche continue d'être suivie dans la barre de statut, et une notification signale sa fin (succès / échec).
- **R6.5** `Ctrl-K` sur une tâche `waiting`/`running` DOIT l'annuler (`PATCH state=canceled`) après confirmation.
- **R6.6** `:task-purge` DOIT purger les tâches terminées antérieures à une date (`/tasks/purge/`) après confirmation.

### R7 — CRUD

- **R7.1** `c` DOIT ouvrir un formulaire de création généré depuis le schéma de requête OpenAPI (types, enums, champs requis, valeurs par défaut, descriptions en aide contextuelle).
- **R7.2** Les champs de relation (`*_href`, ex. `remote`, `repository`, `publication`, `content_guard`) DOIVENT proposer un sélecteur avec recherche sur la ressource cible.
- **R7.3** Les champs `writeOnly` (mots de passe, secrets) DOIVENT être masqués à la saisie.
- **R7.4** `u` DOIT ouvrir le formulaire d'édition pré-rempli ; seuls les champs modifiés sont envoyés (`PATCH`).
- **R7.5** `e` DOIT ouvrir les champs éditables en YAML dans `$EDITOR` (repli `vi`) ; à la fermeture, pulply DOIT valider le YAML, calculer le diff et envoyer un `PATCH` ; QUAND le YAML est invalide ou que le serveur refuse ALORS pulply DOIT proposer de rouvrir l'éditeur avec l'erreur en commentaire en tête de fichier.
- **R7.6** `Ctrl-D` DOIT supprimer la ressource sélectionnée (ou la multi-sélection `Espace`) après confirmation.
- **R7.7** Les erreurs de validation 400 DOIVENT être affichées par champ dans le formulaire.

### R8 — Workflows

- **R8.1** Sur un repository : `s` = sync (choix du remote, `mirror`, `sync_policy`, options propres au plugin issues de l'OpenAPI).
- **R8.2** Sur un repository ou une version : `P` = publish (pour les plugins à publications) avec les options du plugin.
- **R8.3** Sur un repository, une version ou une publication : `D` = distribute (créer ou mettre à jour une distribution : `base_path`, `name`, `content_guard`) ; afficher l'URL de distribution résultante.
- **R8.4** Dans le contenu d'une version : `a` = ajouter du contenu (sélecteur depuis le contenu existant), `r` = retirer la sélection, via `{repo}modify/` avec `base_version`.
- **R8.5** Sur une version : `R` = rollback (nouvelle version à partir de `base_version` = version choisie), après confirmation.
- **R8.6** Suppression d'une version de repository après confirmation.
- **R8.7** `:orphan` DOIT afficher le nombre d'orphelins estimé et lancer `/orphans/cleanup/` avec `orphan_protection_time` configurable, après confirmation.
- **R8.8** Reclaim space sur une sélection de repositories (`/repositories/reclaim_space/`, `keep_versions`), après confirmation.
- **R8.9** Repair (`/repair/`, `verify_checksums`) après confirmation.

### R9 — Upload

- **R9.1** `U` dans un repository ou une liste de contenu DOIT ouvrir un sélecteur de fichiers local (navigation, filtre, multi-sélection).
- **R9.2** pulply DOIT téléverser par chunks via l'API uploads (`POST /uploads/`, `PUT` avec `Content-Range`, `POST commit/` avec `sha256`), avec barre de progression par fichier et débit.
- **R9.3** Après l'upload, pulply DOIT créer l'unité de contenu via le endpoint `content/<plugin>/<type>/` (champs requis issus de l'OpenAPI, ex. `relative_path` pour file) et proposer de l'ajouter au repository courant.
- **R9.4** La taille de chunk DOIT être configurable (défaut 6 MiB).

### R10 — Fonctions avancées

- **R10.1** Diff de versions : `x` sur deux versions sélectionnées DOIT afficher le résumé (`content_summary` : ajoutés / retirés / présents par type) puis la liste détaillée des unités ajoutées et retirées, chargée à la demande.
- **R10.2** Pour deux versions consécutives, pulply DOIT utiliser les filtres `repository_version_added` / `repository_version_removed` ; sinon, la différence d'ensembles des hrefs.
- **R10.3** Xray : `X` sur un repository ou une distribution DOIT afficher l'arbre des relations distribution → publication → version → repository → remote (+ content guard), navigable.

### R11 — Vues sur mesure par plugin

Chaque vue sur mesure fournit des colonnes, un describe et des actions spécifiques ; le reste retombe sur la vue générique.

- **R11.1 rpm** : packages (NEVRA, arch, taille), advisories (id, type, sévérité, date), modulemd, distribution trees, package groups / environments, ACS ; options de sync (`sync_policy`, `skip_types`) et de publish (`checksum_type`, `repo_config`).
- **R11.2 container** : tags → manifest, manifests (media type, listes / multi-arch dépliables, taille), blobs, namespaces, repositories push vs sync ; URL `podman pull` affichée.
- **R11.3 file** : `relative_path`, `sha256`, taille ; publications avec `manifest`.
- **R11.4 python** : packages (nom, version, type de dist), URL d'index `pip`.
- **R11.5 ansible** : collections (namespace.name, version, signatures), roles, remotes Galaxy avec `requirements_file`.
- **R11.6 deb** : releases, components, architectures, packages ; options de publish (`structured`, `simple`).
- **R11.7 ostree** : refs, commits, configs.
- **R11.8 certguard** : guards x509 / RHSM avec aperçu du certificat CA.
- **R11.9** maven, gem, npm, hugging_face et tout plugin inconnu : vue générique uniquement.

### R12 — RBAC et administration

- **R12.1** Vues : users, groups (+ membres), roles (permissions), assignations de rôles (`list_roles` / `add_role` / `remove_role` sur une ressource), access policies (édition YAML, reset), signing services, exporters / importers (pulp, filesystem) avec lancement d'export / import, task schedules, upstream pulps (+ replicate), content guards, workers, content apps.
- **R12.2** `:me` DOIT afficher l'utilisateur courant et ses permissions effectives.
- **R12.3** QUAND l'API renvoie 403 ALORS pulply DOIT afficher « permission refusée » dans la vue au lieu d'une erreur générique, et masquer les actions correspondantes quand les permissions sont connues.

### R13 — Sécurité

- **R13.1** QUAND `--readonly` est passé ou que le contexte a `readonly = true` ALORS pulply NE DOIT envoyer aucune requête POST/PUT/PATCH/DELETE, et les actions de mutation DOIVENT être masquées de l'aide et refusées avec un message.
- **R13.2** Les actions destructives (delete, rollback, orphan cleanup, reclaim, repair, cancel, purge, remove content) DOIVENT demander une confirmation Oui/Non, `Non` étant sélectionné par défaut.

### R14 — Personnalisation

- **R14.1 Skins** : couleurs dans `~/.config/pulply/skins/<nom>.toml`, sélection globale ou par contexte ; skins `dark` et `light` fournis.
- **R14.2 Hotkeys** : `~/.config/pulply/hotkeys.toml` associe une touche à une commande `:` (ex. `Shift-1 = "repo rpm"`).
- **R14.3 Alias** : `~/.config/pulply/aliases.toml` (ex. `rr = "repo rpm"`).
- **R14.4 Colonnes** : sélection et ordre des colonnes par type de ressource (`views.toml`), y compris des chemins JSON arbitraires.
- **R14.5 Plugins shell** : `plugins.toml` définit touche, portée (types de ressources), commande et arguments, avec variables `$HREF`, `$NAME`, `$BASE_URL`, `$CONTEXT`, `$DIST_URL`, … ; option `background` ; suspension propre de la TUI pendant l'exécution.

- **R14.6 Édition depuis la TUI** : `:config` DOIT lister les fichiers de configuration (`config.toml`, fichiers annexes, skins intégrés et utilisateur, `cli.toml` de pulp-cli) avec leur état, et les ouvrir dans `$VISUAL`/`$EDITOR` ; un fichier absent DOIT partir d'un modèle contenant les valeurs par défaut. QUAND le fichier est enregistré ALORS pulply DOIT recharger la configuration et l'appliquer sans redémarrer (reconnexion si le contexte courant a changé) ; QUAND il est invalide ALORS l'ancien contenu DOIT être restauré et l'éditeur rouvert avec l'erreur.

### R15 — i18n

- **R15.1** Toutes les chaînes UI DOIVENT passer par le système de traduction ; FR et EN complets.
- **R15.2** Langue = `language` dans la config, sinon locale système (`LANG`/`LC_ALL`), sinon EN.

### R16 — Qualité, exploitation, distribution

- **R16.1** Logs dans un fichier unique `$XDG_STATE_HOME/pulply/pulply.log` (`~/.local/state/pulply/` sous Linux), niveau via `-l` ; jamais de mot de passe dans les logs. QUAND le fichier dépasse 10 Mo ALORS pulply DOIT le renommer en `pulply.log.1` (en supprimant le précédent) et repartir d'un fichier vide, l'espace total restant borné à 20 Mo. Les anciens fichiers journaliers `pulply.<date>.log` DOIVENT être supprimés au démarrage.
- **R16.2** Aucune panique ne doit laisser le terminal dans un état cassé (restauration via hook de panique).
- **R16.3** Artefacts : crates.io, GitHub Releases (linux-gnu, linux-musl, macOS, x86_64 + aarch64), `.rpm`, `.deb`, formule Homebrew, image OCI.
- **R16.4** Démarrage jusqu'au premier rendu < 300 ms avec schéma en cache (hors latence réseau).
