{
  # Packages
  stdenv,
  lib,
  libiconv,
  libclang,
  libxml2,
  openssl,
  sqlx-cli,
  coreutils,
  pkg-config,
  xmlsec,
  # Flake inputs
  crane,
  advisory-db,
  # Config
  version ? null,
  gitRev ? "",
}: let
  src = with lib.fileset;
    toSource {
      root = ../.;
      fileset = unions [
        ../src
        ../migrations
        ../Cargo.toml
        ../Cargo.lock
      ];
    };
  # TODO(Spoon) combine some args?
  cargoArtifacts =
    crane.buildDepsOnly {
      inherit src;
      strictDeps = true;

      buildInputs = [
        libxml2
        openssl
        xmlsec
        # These don't seem to be needed:
        # libiconv
        # libtool
        # libxslt
      ];

      # For samael
      # TODO(Spoon): clean up bindgen extra args, no ifd?
      LIBCLANG_PATH = "${libclang.lib}/lib";
      BINDGEN_EXTRA_CLANG_ARGS = "${builtins.readFile "${stdenv.cc}/nix-support/libc-crt1-cflags"} \
      ${builtins.readFile "${stdenv.cc}/nix-support/libc-cflags"} \
      ${builtins.readFile "${stdenv.cc}/nix-support/cc-cflags"} \
      ${builtins.readFile "${stdenv.cc}/nix-support/libcxx-cxxflags"} \
      -idirafter ${libiconv}/include \
      ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
      ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
  ";

      nativeBuildInputs = [
        pkg-config
        xmlsec
      ];
    };

  packages = {
    backend = crane.buildPackage {
      inherit src cargoArtifacts version;
      GIT_COMMIT_HASH = gitRev;

      # `out` can be used for tests/checks, `bin` is just the binary, which is all end users need
      doInstallCargoArtifacts = true;
      doCheck = false;
      outputs = ["out" "bin"];
      defaultOutput = "bin";
      postInstall = "mkdir $bin; cp -r $out/bin $bin/";

      # FIXME(Spoon): make it only compile wcpc, remove all inps except sqlx (cargo hakari? parse Cargo.toml & pass extra flags?)
      /*
      This may fix it:
      OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
      OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
      LIBXML2 = "${pkgs.libxml2.out}/lib/libxml2.so";
      */
      strictDeps = true;
      buildInputs = [
        libxml2
        xmlsec
        openssl
      ];
      nativeBuildInputs = [
        pkg-config
        xmlsec
        sqlx-cli
        coreutils
      ];

      # SQLx needs a database to check against
      preBuild = "sqlx database setup";
      DATABASE_URL = "sqlite://database.sqlite";
      passthru.tests = packages;
      meta = {
        description = "WCPC Backend";
        mainProgram = "wcpc";
      };
    };

    backend-fmt = crane.cargoFmt {inherit src;};
    # backend-advisory-audit = crane.cargoAudit {
    #   inherit src advisory-db;
    #   cargoAuditExtraArgs = "--ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2024-0436 --ignore RUSTSEC-2024-0363";
    # }; #FIXME(Spoon): disable ignore
    backend-test = crane.cargoTest {
      inherit src;
      cargoArtifacts = packages.backend.out;

      preBuild = "sqlx database setup";
      DATABASE_URL = "sqlite://database.sqlite";

      strictDeps = true;
      nativeBuildInputs = [pkg-config xmlsec sqlx-cli];
      buildInputs = [xmlsec openssl libxml2];
    };
    backend-clippy = crane.cargoClippy {
      inherit src;
      cargoArtifacts = packages.backend-test; # TODO(Spoon) does it need this, or just deps?

      preBuild = "sqlx database setup";
      DATABASE_URL = "sqlite://database.sqlite";

      strictDeps = true;
      nativeBuildInputs = [pkg-config xmlsec sqlx-cli];
      buildInputs = [xmlsec openssl libxml2];

      cargoClippyExtraArgs = "--all-targets -- -D warnings";
    };
  };
in
  packages.backend
