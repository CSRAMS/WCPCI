{
  description = "Flake for wcpc web interface";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable"; # TODO(Spoon): Do we want to track stable?

    crane = {
      url = "github:ipetkov/crane";
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    garnix-lib = {
      url = "github:garnix-io/garnix-lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    advisory-db,
    garnix-lib,
  }: let
    forAllSystems = nixpkgs.lib.genAttrs [
      "aarch64-linux"
      "aarch64-darwin"
      "x86_64-darwin"
      "x86_64-linux"
    ];
    pkgsFor = system: import nixpkgs {inherit system;};

    gitRev = self.shortRev or self.dirtyShortRev or "";
    rawVersion = (nixpkgs.lib.importTOML ./Cargo.toml).package.version;
    version = rawVersion + "-" + gitRev;
    packages = system: let
      pkgs = pkgsFor system;
    in rec {
      backend = pkgs.callPackage ./nix/backend.nix {
        inherit advisory-db version gitRev;
        crane = crane.mkLib pkgs;
      };
      frontend = pkgs.callPackage ./nix/frontend.nix {inherit version;};
      wrapper = pkgs.callPackage ./nix/wrapper.nix {inherit version backend frontend rocket_config;};
      rocket_config = pkgs.callPackage ./nix-template/rocket_config.nix {openjdk = pkgs.jre_minimal.override {modules = ["java.base" "jdk.compiler"];};};

      container = pkgs.callPackage ./nix/container.nix {inherit wrapper;};
      container-stream = pkgs.runCommand "container-stream" {
        script = container.override {stream = true;};
        nativeBuildInputs = [pkgs.coreutils];
      } "mkdir -p $out/bin; ln -s $script $out/bin/container-stream";
      nixos-vm = (pkgs.nixos [{environment.systemPackages = [wrapper];} ./nix/testing-nixos-config.nix]).vm;

      default = wrapper;
    };
    checks = system: let
      pkgs = pkgsFor system;
    in
      {
        inherit (packages system) backend container nixos-vm; # TODO(Spoon): this might use a fair amount of disk space for dev
        nix-formatting = pkgs.runCommand "nix-check-formatting" {} "${pkgs.alejandra}/bin/alejandra --check ${self} && touch $out";
        # frontend-formatting = (packages system).frontend.overrideAttrs (old: {
        #   name = old.name + "check-formatting";
        #   buildPhase = "npm run format-check";
        #   installPhase = "touch $out";
        # });
        devshell = self.devShells.${system}.default;
        # TODO(Spoon): Frontend eslint eventually
      }
      // (packages system).backend.tests; # All backend tests
  in {
    packages = forAllSystems packages;
    checks = forAllSystems checks;
    formatter = forAllSystems (system: (pkgsFor system).alejandra);
    devShells = forAllSystems (system: {default = import ./nix/shell.nix {pkgs = pkgsFor system;};});
    templates.default = {
      path = ./nix-template;
      description = "Template for deploying WCPC (outside of WCU)";
      welcomeText = ''
        Deploy steps (see README.md):

        - Generate secrets in `secrets/`

        - Edit `rocket_config.nix`

        - Build and load the image: `nix run .#container-stream 2>/dev/null | sudo docker load`

        - Run the container: `sudo docker run --rm -d -v ./secrets:/secrets:ro -v wcpc_database:/database -p 443:443/tcp wcpc`
      '';
    };
    nixosConfigurations.testing = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        {environment.systemPackages = [self.packages.x86_64-linux.wrapper];}
        garnix-lib.nixosModules.garnix
        {
          garnix.server = {
            enable = true;
            persistence.enable = true;
            persistence.name = "wcpc-testing";
          };
        }
        ./nix/testing-nixos-config.nix
      ];
    };
  };
}
/*
TODO(Spoon):
Considerations for deployment:
- How will the certs (TLS, SAML) be renewed?
  - Outside container?
- Container healthcheck?
- port 80? - redirect (& acme challenge?)

Build:
- Hakari?
*/

