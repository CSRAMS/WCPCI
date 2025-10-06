{
  craneLib,
  lib,
  pkgs,
}:
let
  # TODO(Spoon): see if this includes artifacts ; if so, do only binary
  package = craneLib.buildPackage {
    strictDeps = true;
    pname = "backend";
    cargoExtraArgs = "-p backend";
    src = lib.fileset.toSource {
      root = ../../.;
      fileset = lib.fileset.unions [
        ../../Cargo.toml
        ../../Cargo.lock
        (lib.fileset.difference ./. ./default.nix)
      ];
    };

    meta.mainProgram = "backend";

    # TODO(Spoon): can I (should I) use `.dev` for these 2?
    buildInputs = [
      pkgs.openssl
      pkgs.libxml2
      pkgs.xmlsec
    ];
    nativeBuildInputs = [
      pkgs.pkg-config
      pkgs.xmlsec # For xmlsec1-config
      pkgs.rustPlatform.bindgenHook # This sets LIBCLANG_PATH and BINDGEN_EXTRA_CLANG_ARGS
      pkgs.sqlx-cli
    ];

    preBuild = "sqlx database setup --source ${./migrations}"; # I think the workspace means it can't find it, so we help it out
    DATABASE_URL = "sqlite://database.sqlite";
  };

in
package

# TODO(Spoon)
# - don't use `pkgs.*`
# - split out cargoArtifacts, use commonArgs
# - tests: (do these once per workspace? in pkgs/cargoArtifacts.nix)
#   - test/nextest
#   - clippy "--all-targets -- -D warnings" | Does --all-targets cover doctests?
#   - audit - only use Cargo.lock, nothing else | it already does this, I can pass whatever to it
#     - .cargo/audit.toml - ignore irrelevant advisories, only linux vulns
# - Generalize so it can be shared with runner
# - In flake.nix, auto-generate based on workspace .toml
#   - Set meta.mainProgram to pname
