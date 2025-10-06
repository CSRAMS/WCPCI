{ lib, ... }:
let
  inherit (lib) mkOverride;
in
{
  nixpkgs.system = mkOverride 1250 "x86_64-linux"; # Lower than mkDefault, higher than mkOptionDefault

  services.openssh.enable = true;

  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAv0D8TnyJQh0w8FvXECe+iroAyHjK7LtpYCKV+QFxv8 spoon@spoonbaker-laptop"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKsVzdJra+x5aEuwTjL1FBOiMh9bftvs8QwsM1xyEbdd bean"
  ];
}
/*
  FIXME(Spoon):
  backend container directly exposed
  runner container (also exposed in vm?)

  make this system available as a VM under packages
*/
