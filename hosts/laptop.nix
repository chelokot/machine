{
  imports = [ ../home/aws-sso.nix ];
  systemd.user.sessionVariables.GSK_RENDERER = "gl";
}
