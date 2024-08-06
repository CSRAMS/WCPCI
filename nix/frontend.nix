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

  npmDepsHash = "sha256-/rQTGGEENHqLUpzQXwsUu0sw8a5d58+WfEtcTGR7gL0=";

  installPhase = "cp -r dist/ $out";

  meta = {
    description = "Frontend to WCPC";
  };
}
