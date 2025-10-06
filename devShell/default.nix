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
    export ROCKET_TEMPLATE_DIR="$OXJ_ROOT/pkgs/frontend/dist"
    export ROCKET_CONFIG="$OXJ_ROOT/devShell/Rocket.toml"
    # TODO(Spoon): images

    # At the end, to make it override things
    [ -f "$OXJ_ROOT/devShell/local.env" ] && source "$OXJ_ROOT/devShell/local.env"
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

      # TODO(Spoon): all languages in Rocket.toml, or prune Rocket.toml
      ;
  };
  # Crane includes:
  # - rustc
  # - cargo
  # - clippy
  # - rustfmt
}

# BLOCKME: test
# TODO(Spoon): test: (incl. on MacOS)
# - clone repo
# - just setup
# - just dev
# - just lint
# - Edit frontend, ensure changes good
# - create contest, create problem, run submission
