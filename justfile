_default:
    @"{{ just_executable() }}" --list --unsorted --justfile "{{ justfile() }}"

# Set up frontend, database, and environment variables
setup:
    cd ./pkgs/frontend && npm install
    cargo sqlx database setup --source ./pkgs/backend/migrations/
    [ -f ./devShell/secrets.toml ] || cp ./devShell/secrets.template.toml ./devShell/secrets.toml

# Watch the frontend folder and rebuild on changes
dev-frontend:
    cd ./pkgs/frontend && npm run watch

# Run the backend
dev-backend:
    cd ./pkgs/backend && cargo build # TODO(Spoon): change this to run once runner is split
    cd ./devShell && systemd-run --user --scope -p Delegate=yes ../target/debug/backend

# TODO(Spoon): once split, run runner (in podman or with cargo run? - probably cargo run)
# ^ Podman + nix works on MacOS, systemd-run wouldn't

# Run all dev tasks to get a full environment
dev:
    mprocs "just dev-backend" "just dev-frontend"

alias fmt := format
# Format everything
format:
    nix fmt

# Lint all rust code
lint:
    # Keep this in sync with Nix tests
    cargo clippy --all-targets -- -D warnings

# Update frontend & backend dependencies
update:
    cargo update # TODO: do we want flags to make it update more?
    cd ./pkgs/frontend && npm update --latest
    nix flake update

alias c := check
# Run flake checks
check:
    nix flake check --option allow-import-from-derivation false --keep-going --log-format multiline-with-logs
