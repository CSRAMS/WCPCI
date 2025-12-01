{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    flakelight = {
      url = "github:nix-community/flakelight";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Crane has no inputs
    crane.url = "github:ipetkov/crane";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    garnix-lib = {
      url = "github:garnix-io/garnix-lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      flakelight,
      crane,
      garnix-lib,
      ...
    }@inputs:
    flakelight ./. {
      inherit inputs;
      nixDir = ./.;
      nixDirAliases.packages = [ "pkgs" ];
      systems = [ "x86_64-linux" ]; # TODO(Spoon): include darwin, aarch
      withOverlays = [ (final: prev: { craneLib = crane.mkLib final; }) ];

      lib =
        { lib, ... }:
        let
          inherit (lib) importTOML importJSON;
        in
        {
          # TODO(Spoon): use to set version of p.wrapper, containers, etc
          # - What if I didn't?
          rustVersion = (importTOML ./Cargo.toml).workspace.package.version;
        };

      nixosConfigurations.testing.modules = [
        ./nixos-testing.nix
        garnix-lib.nixosModules.garnix
        {
          garnix.server = {
            enable = true;
            persistence.enable = true;
            persistence.name = "wcpc-testing";
          };
        }
      ];

      flakelight.builtinFormatters = false;
      formatters = pkgs: {
        # TODO(Spoon): use `{ rustfmt, etc }:`

        "*.rs" = "${pkgs.rustfmt}/bin/rustfmt";
        "*.nix" = "${pkgs.nixfmt-rfc-style}/bin/nixfmt --strict";

        # TODO(Spoon): does this make the formatter not idempotent?
        # "*.gen.pdf" =
        #   let
        #     typst-pseudoformatter = pkgs.writeShellScript "typst-pseudoformatter" ''
        #       cd "$(${pkgs.coreutils}/bin/dirname "$1")" # TODO(Spoon): finish this
        #       ${pkgs.cabal2nix}/bin/cabal2nix . --hpack > "$1"
        #     '';
        #   in
        #   "${typst-pseudoformatter}";

        # == Langs:
        # - Prettier
        #   - HTML
        #   - CSS
        #   - SCSS
        #   - JS
        #   - MJS
        #   - TS
        #   - Astro
        #   - MD
        #   - JSON
        #   - YAML
        #   - YML
        # - Typst
        # - sql? pgformatter
        # - nginx.conf? nginx-config-formatter
        # - TOML? - taplo
      };

      checks = {
        manual-blocker =
          {
            runCommand,
            lib,
            ripgrep,
            ...
          }:
          runCommand "manual-blocker" { } ''
            cd ${./.}
            if ${lib.getExe ripgrep} --context 1 --pretty --ignore-case "BLOCK()ME"; then
              # RG succeeds = matches found
              echo "Found unfinished work!"
              exit 1
            else
              touch $out
            fi
          '';
        # TODO(Spoon): crane workspace tests
      };
    };
}

/*
  TODO(Spoon):
  - container for backend
  - rocket_config

  - Front NGINX
    - conf, substituted (with what? nginxRoot)
  - runner-nginx (?)
    - nginx conf, substituted
  - postgres?
    - dbInit
    - pg conf + hba conf
  - nixos-testing vm

  == Later Commits
  - Merge `.gitignore`s and clean up
  - Split runner
    - Justfile
    - `.cargo/config.toml` includes `runner = systemd-run ... Delegate=yes`
    - Container for runner
  - Redo runner - technically push-off-able
  - package for WC assets? - all assets are loaded from a given directory
  - Assert frontend/package.json version matches Cargo.toml
  - Rename to OxideJudge everywhere
  - Make README
    - Logo: Ferris as Lady Justice
    - Deploy docs
    - reference paper? - architechure documentation for contributors
    - garnix badge
    - link to staging server

  == Can be pushed off
  - make backend not do TLS
    - are there configurations where it's running atandalone?
    - No more OpenSSL???
  - mailmap?
  - container healthcheck?
  - remove sessions from database
  - include git rev in UI somewhere, add in wrapper to avoid backend rebuilds
    - or in container?
    - or in nixos or whatever runs the containers? - avoid unneeded rebuilds
    - or don't include?
  - mimimize container closure, try to remove bash, etc
    - something was pulling in X11 libs
  - validate config during build
  - <https://garnix.io/docs/actions>
    - Push images to OCI registry; have WCPC pull?
    - Could also trigger GH actions workflow on complete - nix is more portable tho
  - flake nixConfig use garnix cache?

  == WILL be pushed off
  - FPGA Runner
*/
