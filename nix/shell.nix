{pkgs ? import <nixpkgs> {}}: let
  lib = pkgs.lib;
  stdenv = pkgs.stdenv;
in
  pkgs.mkShell {
    name = "wcpc-shell";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
    OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
    LIBXML2 = "${pkgs.libxml2.out}/lib/libxml2.so";
    BINDGEN_EXTRA_CLANG_ARGS = "${builtins.readFile "${stdenv.cc}/nix-support/libc-crt1-cflags"} \
        ${builtins.readFile "${stdenv.cc}/nix-support/libc-cflags"} \
        ${builtins.readFile "${stdenv.cc}/nix-support/cc-cflags"} \
        ${builtins.readFile "${stdenv.cc}/nix-support/libcxx-cxxflags"} \
        -idirafter ${pkgs.libiconv}/include \
        ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
        ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
    ";
    nativeBuildInputs = with pkgs; [
      libiconv
      libtool
      libxml2
      libxslt
      llvmPackages.libclang
      openssl
      pkg-config
      xmlsec
    ];
    buildInputs = with pkgs; [
      rustc
      cargo
      clippy
      rustfmt
      rust-analyzer
      nodejs
      nodePackages.pnpm
      gcc
      libiconv
      libtool
      ghc
      typescript
      libxml2
      libxslt
      llvmPackages.libclang
      openssl
      pkg-config
      xmlsec
      sqlx-cli
      just
      mprocs
    ];
    shellHook = '''';
  }
