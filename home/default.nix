{ machine, host, ... }:
{
  imports = [
    ./shell.nix
    ./git.nix
    ./desktop.nix
    ./container.nix
    ./sync.nix
  ];

  home = {
    username = "chelokot";
    homeDirectory = "/var/home/chelokot";
    stateVersion = "25.11";
    packages = [ machine ];
  };

  targets.genericLinux.enable = true;
  programs.home-manager.enable = true;
  xdg.configFile."machine/host".text = host;
}
