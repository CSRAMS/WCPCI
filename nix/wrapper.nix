{
  frontend,
  backend,
  makeWrapper,
  runCommand,
  rocket_config,
  version ? null,
}:
runCommand "wcpc-wrapper" {
  nativeBuildInputs = [makeWrapper];
  inherit version;
  meta.mainProgram = "wcpc";
}
"makeWrapper ${backend}/bin/wcpc $out/bin/wcpc --set ROCKET_TEMPLATE_DIR ${frontend} --set ROCKET_CONFIG ${rocket_config}"
