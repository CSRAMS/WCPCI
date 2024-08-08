{
  buildNpmPackage,
  lib,
  version ? null,
}:
buildNpmPackage {
  name = "wcpc-frontend";
  inherit version;
  src = ../frontend;
  packageJSON = ../frontend/package.json;

  npmDepsHash = "sha256-BeMYOfv/5QmbiHGaa6F4oEz+fQ6Ou2OOBLrgnwSMkyo=";

  installPhase = "cp -r dist/ $out";

  meta = {
    description = "Frontend to WCPC";
  };
}
