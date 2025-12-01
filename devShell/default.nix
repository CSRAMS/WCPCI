{
  craneLib,
  backend,

  # Only for `packages = ` below
  pkgs,
}:
craneLib.devShell {
  inputsFrom = [ backend ];

  # TODO(Spoon): everything needed to compile backend

  shellHook = ''
    export OXJ_ROOT=$(jj root 2>/dev/null || git rev-parse --show-toplevel 2>/dev/null)
    # TODO(Spoon): ensure at least one of these succeeds
    # TODO(Spoon): test this without jj

    export DATABASE_URL="sqlite://$OXJ_ROOT/devShell/database.sqlite"
    export OXIDEJUDGE_TEMPLATE_DIR="$OXJ_ROOT/pkgs/frontend/dist"
    export OXIDEJUDGE_CONFIG="$OXJ_ROOT/devShell/config.toml"
    export OXIDEJUDGE_SECRETS="$OXJ_ROOT/devShell/secrets.toml"
    # TODO(Spoon): images
  '';

  packages = __attrValues {
    inherit (pkgs)
      # Used by justfile
      just
      nodejs
      sqlx-cli
      mprocs
      nix-output-monitor

      # TODO(Spoon): do we want these? it could be bloat - version with, version without?
      typescript-language-server
      vscode-langservers-extracted
      rust-analyzer

      libtool # Something needs `ltdl`, not sure what

      # TODO(Spoon): all languages in Rocket.toml, or prune config.toml
      #   - Might as well have node + rust in config.toml
      ;
  };
  # Crane includes:
  # - rustc
  # - cargo
  # - clippy
  # - rustfmt
}

# TODO(Spoon): test: (incl. on MacOS)
# - clone repo
# - just setup
# - just dev
# - just lint
# - Edit frontend, ensure changes good
# - create contest, create problem, run submission
