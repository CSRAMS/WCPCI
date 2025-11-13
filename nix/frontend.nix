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

  npmDepsHash = "sha256-FpQ8jFe5cJ7amW6GXoQpfAKiqC4u00+OYBqqSeRe9fU=";

  installPhase = "cp -r dist/ $out";

  meta = {
    description = "Frontend to WCPC";
  };
}
