# Changelog

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
