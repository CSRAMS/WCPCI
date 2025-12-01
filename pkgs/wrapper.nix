{
  frontend,
  backend,
  makeWrapper,
  runCommand,
  oxidejudge_config,
}:
let
  inherit (backend.meta) mainProgram;
in
runCommand "oxidejudge-wrapped"
  {
    nativeBuildInputs = [ makeWrapper ];
    meta = { inherit mainProgram; };
  }
  "makeWrapper ${backend}/bin/${mainProgram} $out/bin/${mainProgram} --set OXIDEJUDGE_TEMPLATE_DIR ${frontend} --set OXIDEJUDGE_CONFIG ${oxidejudge_config}"
# TODO(Spoon): is there a better way to do this?
