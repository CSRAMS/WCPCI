{
  frontend,
  backend,
  makeWrapper,
  runCommand,
  rocket_config,
}:
let
  inherit (backend.meta) mainProgram;
in
runCommand "wrapper"
  {
    nativeBuildInputs = [ makeWrapper ];
    meta = { inherit mainProgram; };
  }
  "makeWrapper ${backend}/bin/${mainProgram} $out/bin/${mainProgram} --set ROCKET_TEMPLATE_DIR ${frontend} --set ROCKET_CONFIG ${rocket_config}"
# TODO(Spoon): is there a better way to do this?
