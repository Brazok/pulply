# Couverture de l'API Pulp `/pulp/api/v3/`

État d'implémentation, dans pulply, de chaque route exposée par le schéma OpenAPI de Pulp.

- **Source** : `crates/pulply-api/tests/fixtures/api.json.gz` (pulpcore 3.122.2 + ansible, certguard, container, deb, file, gem, hugging_face, maven, npm, python, rpm).
- **Périmètre** : chemins sous `/pulp/api/v3/` et chemins `{xxx_href}` (objets de l'API v3). Les 70 chemins hors v3 (Galaxy `/pulp_ansible/galaxy/…`, `/pypi/…`, `/npm/…`, `/pulp/maven/…`, `/token/`, `/ansible/collections/`) sont hors périmètre.
- **Méthode** : pulply ne code aucune route en dur, il construit un registre à partir du schéma (`crates/pulply-api/src/registry.rs`). Le statut ci-dessous reproduit les règles de ce registre et vérifie, dans `crates/pulply/src/`, quelle vue ou commande utilise chaque opération.

## Légende

| Symbole | Signification |
|---|---|
| ✅ | Implémenté : opération exposée dans l'UI |
| 🟡 | Partiel : découvert par le registre mais non exposé, ou fonctionnement non garanti |
| ➖ | Volontairement non utilisé (équivalent couvert autrement) |
| ❌ | Non implémenté |

## Résumé

1111 opérations (couples chemin + méthode) analysées.

| Statut | Opérations |
|---|---|
| ✅ | 1039 |
| 🟡 | 0 |
| ➖ | 72 |
| ❌ | 0 |

Couverture générique, valable pour tous les plugins :

| Opération OpenAPI | Statut | Où dans pulply |
|---|---|---|
| `GET <liste>/` (`*_list`) | ✅ | vue liste : filtres, tri serveur, pagination (`views/list.rs`) |
| `POST <liste>/` (`*_create`) | ✅ | formulaire de création `c` (`views/form.rs`) ; champs binaires = chemin local, envoi `multipart/form-data` si fichier ou si l'endpoint ne déclare pas de corps JSON ; upload par chunks `U` si le schéma a un champ `upload` |
| `GET {href}` (`*_read`) | ✅ | vue détail (`views/detail.rs`) |
| `PATCH {href}` | ✅ | édition `u` (formulaire) ou `e` ($EDITOR, `editor.rs`) ; annulation des tâches |
| `PUT {href}` | ➖ | jamais utilisé : pulply modifie par PATCH |
| `DELETE {href}` | ✅ | suppression confirmée (`views/actions.rs::delete`) |
| `POST {href}<verbe>/` | ✅ | menu d'actions `m` (`views/admin.rs`), formulaire construit depuis le schéma du corps ; touches dédiées pour `sync`, `modify`, rôles |
| `GET {href}list_roles/`, `my_permissions/` | ✅ | vue rôles, filtrage des actions par permission (`perms.rs`) |
| `GET {href}<sous-collection>/` | ✅ | collection imbriquée (versions, users, exports…) |
| Autres `GET {href}<verbe>/` renvoyant du JSON | ✅ | menu d'actions `m`, vue JSON en lecture seule (`views/raw.rs`), disponible aussi en mode lecture seule |
| `POST` global sans liste | ✅ | `:purge`, `:orphans`, `:reclaim`, `:repair`, et `:op <chemin>` pour toutes les autres (complétion Tab) ; réponse synchrone affichée |

## Routes non implémentées ou écartées

Toutes les routes jugées pertinentes sont implémentées. Restent les routes écartées volontairement (hors `PUT`, voir plus haut) :

| Statut | Méthode | Chemin | operationId | Remarque |
|---|---|---|---|---|
| ➖ | GET | `/pulp/api/v3/docs/api.yaml` | `docs_api.yaml_get` | doublon YAML de `docs/api.json` |
| ➖ | GET | `/pulp/api/v3/livez/` | `livez_read` | sonde de vivacité pour orchestrateurs ; `status/` suffit |
| ➖ | POST | `/pulp/api/v3/login/` | `login` | session par cookie inutile : pulply s'authentifie en Basic auth à chaque requête |
| ➖ | PATCH | `/pulp/api/v3/login/` | `login_update` | session par cookie inutile : pulply s'authentifie en Basic auth à chaque requête |
| ➖ | DELETE | `/pulp/api/v3/login/` | `logout` | session par cookie inutile : pulply s'authentifie en Basic auth à chaque requête |
| ➖ | DELETE | `/pulp/api/v3/orphans/` | `orphans_delete` | déprécié, remplacé par `orphans/cleanup/` (`:orphans`) |
| ➖ | GET | `{ansible_ansible_namespace_metadata_href}avatar/` | `content_ansible_namespaces_avatar` | `avatar` : redirection vers une image, rien à afficher dans un terminal |
| ➖ | GET | `{ansible_collection_import_href}` | `collection_import_read` | API Galaxy d'ansible (href `ansible_collection_import_href` hors ressources v3), non pertinente pour pulply |
| ➖ | GET | `{ansible_cross_repository_collection_version_index_href}api/v3/plugin/ansible/search/collection-versions/` | `api_v3_plugin_ansible_search_collection_versions_list` | API Galaxy d'ansible (href `ansible_cross_repository_collection_version_index_href` hors ressources v3), non pertinente pour pulply |
| ➖ | POST | `{ansible_cross_repository_collection_version_index_href}api/v3/plugin/ansible/search/collection-versions/` | `api_v3_plugin_ansible_search_collection_versions_rebuild` | API Galaxy d'ansible (href `ansible_cross_repository_collection_version_index_href` hors ressources v3), non pertinente pour pulply |
| ➖ | GET | `{ansible_role_href}api/v1/roles/` | `api_v1_roles_get` | API Galaxy v1 d'ansible greffée sur un href de rôle, non pertinente |
| ➖ | GET | `{ansible_role_href}versions/` | `api_v1_roles_versions_list` | API Galaxy v1 d'ansible greffée sur un href de rôle, non pertinente |
| ➖ | GET | `{maven_maven_repository_href}packages/flat/` | `repositories_maven_maven_packages_flat` | variante non paginée de `packages/` (exposé dans le menu `m`) |

## Opérations globales

| Statut | Chemin | Remarque |
|---|---|---|
| ✅ | `/pulp/api/v3/ansible/copy/` | `:op ansible/copy` |
| ✅ | `/pulp/api/v3/content/file/files/upload/` | `:op content/file/files/upload` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/content/maven/artifact/upload/` | `:op content/maven/artifact/upload` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/content/maven/metadata/upload/` | `:op content/maven/metadata/upload` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/content/npm/packages/upload/` | `:op content/npm/packages/upload` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/content/python/packages/upload/` | `:op content/python/packages/upload` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/content/rpm/packages/upload/` | `:op content/rpm/packages/upload` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/datarepair/7272/` | `:op datarepair/7272` |
| ✅ | `/pulp/api/v3/deb/copy/` | `:op deb/copy` |
| ✅ | `/pulp/api/v3/domains/migrate/` | `:op domains/migrate` |
| ✅ | `/pulp/api/v3/importers/core/pulp/import-check/` | `:op importers/core/pulp/import-check` |
| ✅ | `/pulp/api/v3/orphans/cleanup/` | commande `:orphans` |
| ✅ | `/pulp/api/v3/remotes/python/python/from_bandersnatch/` | `:op remotes/python/python/from_bandersnatch` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/repair/` | commande `:repair` |
| ✅ | `/pulp/api/v3/repositories/reclaim_space/` | commande `:reclaim` |
| ✅ | `/pulp/api/v3/rpm/comps/` | `:op rpm/comps` (multipart, champ fichier) |
| ✅ | `/pulp/api/v3/rpm/copy/` | `:op rpm/copy` |
| ✅ | `/pulp/api/v3/rpm/prune/` | `:op rpm/prune` |
| ✅ | `/pulp/api/v3/tasks/purge/` | commande `:purge` |

## Système

| Statut | Méthode | Chemin | Remarque |
|---|---|---|---|
| ✅ | GET | `/pulp/api/v3/docs/api.json` | chargement du schéma OpenAPI (registre) |
| ✅ | GET | `/pulp/api/v3/login/` | identité de l'utilisateur (`:me`, permissions) |
| ➖ | POST | `/pulp/api/v3/login/` | session par cookie inutile : pulply s'authentifie en Basic auth à chaque requête |
| ➖ | PATCH | `/pulp/api/v3/login/` | session par cookie inutile : pulply s'authentifie en Basic auth à chaque requête |
| ➖ | DELETE | `/pulp/api/v3/login/` | session par cookie inutile : pulply s'authentifie en Basic auth à chaque requête |
| ✅ | GET | `/pulp/api/v3/status/` | tableau de bord / connexion |

## Matrice par type de ressource

Colonnes : liste, création, lecture, PATCH, PUT, DELETE. La dernière colonne liste les actions et collections imbriquées (`{href}<verbe>/`).

### core (listes génériques)

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `content` | ✅ | · | · | · | · | · | · |
| `contentguards` | ✅ | · | · | · | · | · | · |
| `distributions` | ✅ | · | · | · | · | · | · |
| `publications` | ✅ | · | · | · | · | · | · |
| `remotes` | ✅ | · | · | · | · | · | · |
| `repositories` | ✅ | · | · | · | · | · | · |

### core et ressources hors catégories plugin

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `access_policies` | ✅ | · | ✅ | ✅ | ➖ | · | ✅ POST `reset` |
| `ansible/collections` | ✅ | · | ✅ | · | · | · | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `artifacts` | ✅ | ✅ | ✅ | · | · | ✅ | · |
| `domains` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `groups` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ GET `roles`<br>✅ POST `roles`<br>✅ GET `users`<br>✅ POST `users`<br>✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `groups/roles` | · | · | ✅ | · | · | ✅ | · |
| `groups/users` | · | · | · | · | · | ✅ | · |
| `pulp_ansible/tags` | ✅ | · | · | · | · | · | · |
| `pulp_container/namespaces` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `repository_versions` | ✅ | · | · | · | · | · | · |
| `roles` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | · |
| `signing-services` | ✅ | · | ✅ | · | · | · | · |
| `task-groups` | ✅ | · | ✅ | ✅ | · | · | · |
| `task-schedules` | ✅ | · | ✅ | · | · | · | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `tasks` | ✅ | · | ✅ | ✅ | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ GET `profile_artifacts`<br>✅ POST `remove_role` |
| `uploads` | ✅ | ✅ | ✅ | · | ➖ | ✅ | ✅ POST `add_role`<br>✅ POST `commit`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `upstream-pulps` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `replicate` |
| `users` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ GET `roles`<br>✅ POST `roles` |
| `users/roles` | · | · | ✅ | · | · | ✅ | · |
| `vuln_report` | ✅ | · | ✅ | · | · | ✅ | · |
| `workers` | ✅ | · | ✅ | · | · | · | · |

### acs

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `acs/deb/deb` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `refresh`<br>✅ POST `remove_role` |
| `acs/file/file` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `refresh`<br>✅ POST `remove_role` |
| `acs/rpm/rpm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `refresh`<br>✅ POST `remove_role` |

### content

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `content/ansible/collection_deprecations` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/ansible/collection_marks` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/ansible/collection_signatures` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/ansible/collection_versions` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/ansible/namespaces` | ✅ | · | ✅ | · | · | · | ➖ GET `avatar`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/ansible/roles` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label`<br>➖ GET `api/v1/roles`<br>➖ GET `versions` |
| `content/container/blobs` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/container/manifests` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/container/signatures` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/container/tags` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/core/openpgp_publickey` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/core/openpgp_publicsubkey` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/core/openpgp_signature` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/core/openpgp_userattribute` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/core/openpgp_userid` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/generic_contents` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/installer_file_indices` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/installer_packages` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/package_indices` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/package_release_components` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/packages` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/release_architectures` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/release_components` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/release_files` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/releases` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/source_indices` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/source_packages` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/deb/source_release_components` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/file/files` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/gem/gem` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/hugging_face/hugging-face` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/maven/artifact` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/maven/maven/index-page` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/maven/metadata` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/maven/package` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/npm/packages` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/python/packages` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/python/provenance` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/python/yanks` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/advisories` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/distribution_trees` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/modulemd_defaults` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/modulemd_obsoletes` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/modulemds` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/packagecategories` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/packageenvironments` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/packagegroups` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/packagelangpacks` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/packages` | ✅ | ✅ | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `content/rpm/repo_metadata_files` | ✅ | · | ✅ | · | · | · | ✅ POST `set_label`<br>✅ POST `unset_label` |

### contentguards

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `contentguards/certguard/rhsm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | · |
| `contentguards/certguard/x509` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | · |
| `contentguards/core/composite` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `contentguards/core/content_redirect` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `contentguards/core/envvar_header` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `contentguards/core/header` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `contentguards/core/rbac` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |

### distributions

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `distributions/ansible/ansible` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/container/container` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/container/pull-through` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/core/artifacts` | ✅ | · | ✅ | · | · | · | · |
| `distributions/core/openpgp` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/deb/apt` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/file/file` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/gem/gem` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/hugging_face/hugging-face` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/maven/maven` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/npm/npm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/python/pypi` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `distributions/rpm/rpm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |

### exporters

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `exporters/core/filesystem` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ GET `exports`<br>✅ POST `exports` |
| `exporters/core/filesystem/exports` | · | · | ✅ | · | · | ✅ | · |
| `exporters/core/pulp` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ GET `exports`<br>✅ POST `exports` |
| `exporters/core/pulp/exports` | · | · | ✅ | · | · | ✅ | · |

### importers

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `importers/core/pulp` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ GET `imports`<br>✅ POST `imports` |
| `importers/core/pulp/imports` | · | · | ✅ | · | · | ✅ | · |

### publications

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `publications/deb/apt` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `publications/deb/verbatim` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `publications/file/file` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `publications/gem/gem` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `publications/hugging_face/hugging-face` | ✅ | ✅ | ✅ | · | · | ✅ | · |
| `publications/python/pypi` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |
| `publications/rpm/rpm` | ✅ | ✅ | ✅ | · | · | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role` |

### remotes

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `remotes/ansible/collection` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/ansible/git` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/ansible/role` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/container/container` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/container/pull-through` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/deb/apt` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/file/file` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/file/git` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/gem/gem` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/hugging_face/hugging-face` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/maven/maven` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/npm/npm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/python/python` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/rpm/rpm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `remotes/rpm/uln` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |

### repositories

| Ressource | Liste | Création | Lecture | PATCH | PUT | DELETE | Actions / sous-collections |
|---|---|---|---|---|---|---|---|
| `repositories/ansible/ansible` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ POST `copy_collection_version`<br>✅ GET `list_roles`<br>✅ POST `mark`<br>✅ POST `modify`<br>✅ POST `move_collection_version`<br>✅ GET `my_permissions`<br>✅ POST `rebuild_metadata`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sign`<br>✅ POST `sync`<br>✅ POST `unmark`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/ansible/ansible/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `rebuild_metadata`<br>✅ POST `repair` |
| `repositories/container/container` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add`<br>✅ POST `add_role`<br>✅ POST `build_image`<br>✅ POST `copy_manifests`<br>✅ POST `copy_tags`<br>✅ GET `list_roles`<br>✅ GET `my_permissions`<br>✅ POST `remove`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sign`<br>✅ POST `sync`<br>✅ POST `tag`<br>✅ POST `unset_label`<br>✅ POST `untag`<br>✅ GET `versions` |
| `repositories/container/container-push` | ✅ | · | ✅ | ✅ | ➖ | · | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `migrate`<br>✅ GET `my_permissions`<br>✅ POST `remove_image`<br>✅ POST `remove_role`<br>✅ POST `remove_signatures`<br>✅ POST `sign`<br>✅ POST `tag`<br>✅ POST `untag`<br>✅ GET `versions` |
| `repositories/container/container-push/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/container/container/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/core/openpgp_keyring` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `unset_label` |
| `repositories/deb/apt` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/deb/apt/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/file/file` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/file/file/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/gem/gem` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/gem/gem/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/hugging_face/hugging-face` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `modify`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/hugging_face/hugging-face/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/maven/maven` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ POST `build_path_index`<br>✅ POST `compact_path_index`<br>✅ GET `list_roles`<br>✅ GET `metrics`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ GET `packages`<br>➖ GET `packages/flat`<br>✅ GET `path_index_status`<br>✅ POST `remove_role`<br>✅ POST `repair_index_pages`<br>✅ POST `repair_metadata`<br>✅ POST `repair_packages`<br>✅ POST `set_label`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/maven/maven/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/npm/npm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/npm/npm/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair` |
| `repositories/python/python` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `repair_metadata`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `blocklist_entries`<br>✅ POST `blocklist_entries`<br>✅ GET `versions` |
| `repositories/python/python/blocklist_entries` | · | · | ✅ | · | · | ✅ | · |
| `repositories/python/python/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair`<br>✅ POST `scan` |
| `repositories/rpm/rpm` | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | ✅ POST `add_role`<br>✅ GET `list_roles`<br>✅ POST `modify`<br>✅ GET `my_permissions`<br>✅ POST `remove_role`<br>✅ POST `set_label`<br>✅ POST `sync`<br>✅ POST `unset_label`<br>✅ GET `versions` |
| `repositories/rpm/rpm/versions` | · | · | ✅ | · | · | ✅ | ✅ POST `repair`<br>✅ POST `scan` |

`·` : opération absente de l'API pour ce type.

## Régénérer ce fichier

Le statut découle des règles du registre : si le schéma ou ces règles changent (nouveau plugin, nouvelle commande globale…), ce document doit être revu.
