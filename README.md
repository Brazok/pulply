# pulply

Terminal UI to observe and manage [Pulp 3](https://pulpproject.org) servers, in the spirit of
[k9s](https://k9scli.io). Built with Rust and [ratatui](https://ratatui.rs).

Every resource type exposed by the server (all installed plugins) is discovered from its OpenAPI
schema and browsable from the sidebar or the `:` command line.

Status: milestones M1 (observe) and M2 (create/edit/delete, workflows) done. See [`docs/spec/`](docs/spec/)
for requirements, design and roadmap.

## Build

```sh
cargo build --release
./target/release/pulply --help
```

Requires pulpcore ≥ 3.50.

## Configuration

pulply reads `~/.config/pulply/config.toml` and imports every profile of pulp-cli's
`~/.config/pulp/cli.toml` (`[cli]` → context `default`, `[cli-lab]` → `lab`).

```toml
language = "fr"            # optional, else system locale (fr / en)
start_view = "dashboard"   # or any command, e.g. "repo rpm", "task"
refresh = 5                # seconds
import_pulp_cli = true
default_context = "prod"

[contexts.prod]
base_url = "https://pulp.example.com"
username = "admin"         # password from ~/.netrc, PULPLY_PASSWORD, or prompted
readonly = true
verify_ssl = "/etc/pki/tls/certs/ca-bundle.crt"   # or true / false

[contexts.lab]
base_url = "http://localhost:8080"
username = "admin"
password = "password"
api_root = "/pulp/"        # default
domain = "acme"            # optional, for domain-enabled servers
```

CLI flags: `--context`, `--config`, `--readonly`, `-c/--command`, `--refresh`, `-l/--log-level`.
Logs go to `$XDG_STATE_HOME/pulply/pulply.<date>.log`.

## Keys

Browse:

| Key | Action |
|---|---|
| `:` | command mode (`:repo rpm`, `:content container/tags`, `:dist`, `:task`, `:ctx`, `:q`); `Tab` completes |
| `/` | filter rows; `-q <expr>` and `-f key=value` are sent to the API |
| `Enter` / `Esc` | drill down (repository → versions → content) / back |
| `d` | describe (follow hrefs with `Enter`) |
| `Shift`+letter | sort by the column starting with that letter (unless bound to an action below) |
| `f` | cycle task state filter (task list) |
| `Tab` / `Ctrl-B` | focus / toggle sidebar |
| `Ctrl-R` / `p` | refresh now / pause polling |
| `[` / `]` | history back / forward |
| `?` | help |
| `q` / `Ctrl-C` | quit |

Manage (hidden and refused with `--readonly` or `readonly = true`):

| Key | Action |
|---|---|
| `c` / `u` | create / edit with a form generated from the server schema (`Ctrl-S` submits) |
| `e` | edit as YAML in `$VISUAL`/`$EDITOR` (changed fields are PATCHed) |
| `Space` / `Ctrl-D` | mark rows / delete marked rows (or the current one), after confirmation |
| `s` / `P` / `D` | sync / publish / distribute a repository (`P`, `D` also on versions and publications) |
| `R` | rollback: new version with the content of the selected version |
| `a` / `r` | add content (from a version's content, or marked content to a chosen repository) / remove marked content |
| `Ctrl-K` | cancel task |
| `:purge` `:orphans` `:reclaim` `:repair` | global operations, after confirmation |

Tasks started from pulply open a progress window (`Esc` detaches it), are followed in the status
bar and notify on completion; a distribution's URL is shown once created.

## Development

```sh
PULP_PORT=8080 docker compose -f deploy/compose.yml up -d   # admin / password
cargo test --workspace
```
