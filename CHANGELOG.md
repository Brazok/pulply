# Changelog

## 0.2.0 - 2026-11-13

### Added

- `pulply serve`: web interface built with shadcn/ui and embedded in the binary. Dashboard, every
  resource type of every plugin (search, filters, sorting, pagination, bulk actions), schema-driven
  create/edit forms with resource pickers and file fields, sync / publish / distribute / rollback
  workflows, chunked uploads from the browser, add/remove content, live task progress, role
  assignments, global operations and plugin actions, command palette (`Ctrl-K`), light and dark
  themes, and a simple mode (the default) next to the advanced one. The server keeps the Pulp
  credentials and relays requests behind an access token (embedded in the page on loopback
  addresses, printed at startup otherwise), honoring `--readonly`.
  On a wildcard address (`--listen 0.0.0.0:8484`, `make serve-lan`) it prints a link with the
  token for each network interface.
- Web interface pages and maps:
  - **Map** tab on remotes, repositories, versions, publications and distributions: the chain from
    the remote to the URL clients use; for container pull-through caches, the remote, the
    distribution, the cached images and the `docker login` / `docker pull` commands.
  - **Disk usage** page: a scan run by `pulply serve` in the background (cancellable, with
    progress, last report kept on disk) breaking down the storage by plugin, content type and
    repository, orphans included.
  - **Maintenance** page: orphan cleanup (with the current orphan count), task purge (age presets),
    storage repair (quick check or full checksum verification) and space reclaim in one click,
    each with its last run; other maintenance operations (prune, datarepair, domain migration)
    listed apart from content operations.
  - **Administration** page and sidebar section (users, groups, roles, access policies…).
- `pulply serve` authenticates its reads to Pulp with a session cookie instead of Basic
  authentication (Pulp hashes the password on every Basic request), answers the registry and
  disk usage state with an `ETag`, sets security headers (CSP, `X-Frame-Options`, `nosniff`,
  `Referrer-Policy`, COOP) and stops gracefully on `SIGTERM`.
- `:config` lists the configuration files (`config.toml`, hotkeys, aliases, views, plugins, skins,
  pulp-cli's `cli.toml`) and edits them in `$EDITOR`; a missing file starts from the defaults. The
  configuration is reloaded on save, and an invalid edit is rolled back and reopened with the error.
- `:op <path>` runs any global operation of the server not covered by a dedicated command
  (`rpm/copy`, `deb/copy`, `ansible/copy`, `rpm/prune`, `rpm/comps`, `domains/migrate`,
  `importers/core/pulp/import-check`, `remotes/python/python/from_bandersnatch`, the synchronous
  `content/*/upload/`…), with a form built from its schema; a synchronous response is displayed.
- Forms accept files: binary fields (`file`, `config`) take a local path and the request is sent as
  `multipart/form-data`, as are endpoints declaring no JSON body (content and artifact creation,
  `build_image`).
- The action menu (`m`) also lists read-only GET actions returning JSON (Maven `metrics`,
  `packages`, `path_index_status`, task `profile_artifacts`), available in read-only mode too.
- `docs/api-coverage.md`: implementation status of every `/api/v3` route.

### Fixed

- Container pull-through distributions are shown and pulled under the path kept in their
  `pulp_container.pull_through` label (their `base_path` is a UUID since pulp_container 2.30), and
  registry references use the content origin's host rather than the one pulply calls the API with.

### Changed

- The local development Pulp (`deploy/compose.yml`) enables the registry token authentication, so
  `docker login` works and pull-through caches can be filled. An already running Pulp is recreated
  by `make pulp-up`; `PULP_TOKEN_AUTH_DISABLED=true` restores Basic authentication only.

## 0.1.2 - 2026-10-22

### Fixed

- `timeout = 0` disables the request timeout instead of making every request fail at once.

## 0.1.1 - 2026-10-21

### Changed

- Logs go to a single `pulply.log` rotated to `pulply.log.1` past 10 MB, instead of one file per day;
  former daily files are removed at startup.

### Fixed

- `status/` is read outside the domain, and the configured `domain` only scopes URLs when the server
  has domains enabled (`default` when unset), as pulp-cli does; an ignored domain is notified.

## 0.1.0 - 2026-10-21

First release.

### Observe

- Every resource type of every installed plugin, discovered from the server's OpenAPI schema and
  browsable from the sidebar or the `:` command line, with pagination, sorting, API filters and `/` filter.
- Dashboard, live task views (progress, errors), adaptive polling with ETag.
- Contexts from `~/.config/pulply/config.toml` and pulp-cli's `cli.toml`, hot switch with `:ctx`.

### Manage

- Forms generated from the OpenAPI schema, `$EDITOR` YAML editing, multi-select delete with confirmation.
- Sync, publish, distribute, modify, rollback; task cancel and purge; `:orphans`, `:reclaim`, `:repair`.
- `--readonly` mode: mutating actions hidden and refused.

### Plugin views and tools

- Dedicated views for rpm, container, file, python, ansible, deb, ostree and certguard.
- Chunked uploads, repository version diff (`x`) and relations view (`X`).

### Administration and customization

- RBAC and admin views, `:me`, role management, generic POST actions (`m`).
- Skins (`dark`, `light`, per context), hotkeys, aliases, configurable columns, shell plugins.
- English and French translations.

### Distribution

- Prebuilt binaries for Linux (gnu, musl) and macOS (x86_64, aarch64), shell installer,
  `.deb` and `.rpm` packages, crates.io.
