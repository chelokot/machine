{
  config,
  machine,
  host,
  ...
}:
{
  imports = [
    ./shell.nix
    ./git.nix
    ./glab.nix
    ./desktop.nix
    ./container.nix
    ./sync.nix
    ./tools.nix
  ];

  home = {
    username = "chelokot";
    homeDirectory = "/var/home/chelokot";
    stateVersion = "25.11";
    packages = [ machine ];
  };

  targets.genericLinux = {
    enable = true;
    gpu.enable = false;
  };
  news.display = "silent";
  programs.home-manager.enable = true;
  home.shellAliases.home-manager = "home-manager --flake ${config.home.homeDirectory}/.local/share/machine#chelokot@${host}";
  xdg.configFile."machine/host".text = host;
}
