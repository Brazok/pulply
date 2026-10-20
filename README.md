# pulply

Terminal UI to observe and manage [Pulp 3](https://pulpproject.org) servers, in the spirit of
[k9s](https://k9scli.io). Built with Rust and [ratatui](https://ratatui.rs).

Every resource type exposed by the server (all installed plugins) is discovered from its OpenAPI
schema and browsable from the sidebar or the `:` command line.

Status: milestone M1 (read/observe). See [`docs/spec/`](docs/spec/) for requirements, design and roadmap.

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

| Key | Action |
|---|---|
| `:` | command mode (`:repo rpm`, `:content container/tags`, `:dist`, `:task`, `:ctx`, `:q`); `Tab` completes |
| `/` | filter rows; `-q <expr>` and `-f key=value` are sent to the API |
| `Enter` / `Esc` | drill down (repository → versions → content) / back |
| `d` | describe (follow hrefs with `Enter`) |
| `Shift`+letter | sort by the column starting with that letter |
| `f` | cycle task state filter (task list) |
| `Tab` / `Ctrl-B` | focus / toggle sidebar |
| `Ctrl-R` / `p` | refresh now / pause polling |
| `[` / `]` | history back / forward |
| `?` | help |
| `q` / `Ctrl-C` | quit |

## Development

```sh
PULP_PORT=8080 docker compose -f deploy/compose.yml up -d   # admin / password
cargo test --workspace
```
