{
  lib,
  buildNpmPackage,
  importNpmLock,
}:
let
  inherit (lib) importJSON;
in
buildNpmPackage {
  name = "oxidejudge-frontend";
  inherit (importJSON ./package.json) version;

  src = ./.;
  packageJSON = ./package.json;
  npmDeps = importNpmLock { npmRoot = ./.; };
  npmConfigHook = importNpmLock.npmConfigHook;
  installPhase = "cp -r dist/ $out";
}
