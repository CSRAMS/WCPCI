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

  npmDepsHash = "sha256-iU6HVXI5+GrkSs5x54xTKuG91uG1r44bgxrgeTot3Z4=";

  installPhase = "cp -r dist/ $out";

  meta = {
    description = "Frontend to WCPC";
  };
}
