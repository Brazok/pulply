# pulply — common tasks. `make` builds and checks everything; `make help` lists the targets.
#
# The web interface (crates/pulply/web) is built here explicitly; cargo then embeds it
# (`PULPLY_SKIP_WEB=1` keeps build.rs from rebuilding it a second time).

CARGO     ?= cargo
NPM       ?= npm
PODMAN    ?= podman
WEB       := crates/pulply/web
IMAGE     ?= pulply
LISTEN    ?= 127.0.0.1:8484
PULP_PORT ?= 8080
export PULPLY_SKIP_WEB := 1

.DEFAULT_GOAL := all
.PHONY: all help web-deps web web-test web-check build release test test-rust fmt fmt-check lint check \
        run serve web-dev pulp pulp-up pulp-down package deb rpm image clean distclean

all: check release ## Everything: formatting, lint, tests (Rust + web), release build

help: ## List the targets
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ---------------------------------------------------------------- web interface

$(WEB)/node_modules: $(WEB)/package.json $(WEB)/package-lock.json
	cd $(WEB) && $(NPM) ci --no-audit --no-fund
	@touch $@

web-deps: $(WEB)/node_modules ## Install the web interface dependencies

web: web-deps ## Build the web interface (typecheck + bundle into web/dist)
	cd $(WEB) && $(NPM) run build

web-test: web-deps ## Web interface unit tests
	cd $(WEB) && $(NPM) test

web-check: web-deps ## Typecheck the web interface
	cd $(WEB) && $(NPM) run typecheck

# ---------------------------------------------------------------- Rust

build: web ## Debug build (TUI + embedded web interface)
	$(CARGO) build --workspace

release: web ## Optimized build: target/release/pulply
	$(CARGO) build --release --locked -p pulply

test-rust: web ## Rust tests
	$(CARGO) test --workspace

test: test-rust web-test ## All tests

fmt: ## Format the Rust code
	$(CARGO) fmt --all

fmt-check: ## Check the Rust formatting
	$(CARGO) fmt --all --check

lint: web ## Clippy, warnings as errors
	$(CARGO) clippy --workspace --all-targets -- -D warnings

check: fmt-check lint web-check test ## Everything CI runs

# ---------------------------------------------------------------- run

run: build ## Terminal UI (ARGS="--context lab")
	$(CARGO) run -p pulply -- $(ARGS)

serve: build ## Web interface on $(LISTEN) (ARGS="--context lab --open")
	$(CARGO) run -p pulply -- serve --listen $(LISTEN) $(ARGS)

web-dev: web-deps ## Vite with hot reload, proxying to `make serve ARGS="--token dev"`
	cd $(WEB) && $(NPM) run dev

pulp-up: ## Local Pulp for development (admin / password) on $(PULP_PORT)
	PULP_PORT=$(PULP_PORT) $(PODMAN) compose -f deploy/compose.yml up -d

pulp-down: ## Stop the local Pulp
	$(PODMAN) compose -f deploy/compose.yml down

pulp: ## Follow the local Pulp logs (starts it if needed; TAIL=100 lines of history)
	@PULP_PORT=$(PULP_PORT) $(PODMAN) compose -f deploy/compose.yml up -d
	$(PODMAN) compose -f deploy/compose.yml logs --follow --tail=$(or $(TAIL),100)

# ---------------------------------------------------------------- packaging

deb: release ## .deb package (needs cargo-deb)
	$(CARGO) deb -p pulply --no-build

rpm: release ## .rpm package (needs cargo-generate-rpm)
	$(CARGO) generate-rpm -p crates/pulply

package: web ## Check the crates.io packages
	$(CARGO) package --workspace --allow-dirty

image: ## Container image $(IMAGE) (Containerfile)
	$(PODMAN) build -t $(IMAGE) -f Containerfile .

# ---------------------------------------------------------------- cleanup

clean: ## Remove build outputs
	$(CARGO) clean
	rm -rf $(WEB)/dist

distclean: clean ## Also remove the web dependencies
	rm -rf $(WEB)/node_modules
