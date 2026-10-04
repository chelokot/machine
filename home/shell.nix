{
  config,
  lib,
  dev,
  starship-show-on-command,
  exposedcat-dotfiles,
  ...
}:
let
  colors = lib.pipe "${exposedcat-dotfiles}/fish/colors.fish" [
    builtins.readFile
    (lib.replaceStrings [ "set -Ux " ] [ "set -g " ])
    (lib.splitString "\n")
    (map (
      line:
      if lib.hasPrefix "set -g fish_color_command " line then
        "set -g fish_color_command 7ee787"
      else if lib.hasPrefix "set -g fish_color_error " line then
        "set -g fish_color_error ff6b81"
      else
        line
    ))
    (lib.concatStringsSep "\n")
  ];
in
{
  programs.fish = {
    enable = true;
    plugins = [
      {
        name = "starship-show-on-command";
        src = starship-show-on-command;
      }
    ];
    interactiveShellInit = ''
      set -g fish_greeting
      ${colors}
      ${builtins.readFile "${dev}/fish/config.fish"}
    '';
  };

  programs.starship = {
    enable = true;
    enableFishIntegration = false;
    settings = lib.importTOML "${dev}/starship.toml";
  };

  programs.bash = {
    enable = true;
    bashrcExtra = ''
      [ -f /etc/bashrc ] && . /etc/bashrc
    '';
    initExtra = ''
      if [[ $- == *i* && -z "''${BASH_EXECUTION_STRING:-}" && -z "''${MACHINE_NO_FISH:-}" ]]; then
        exec ${lib.getExe config.programs.fish.package}
      fi
    '';
  };

  home.sessionPath = [
    "$HOME/.local/bin"
    "$HOME/.bun/bin"
    "$HOME/.cargo/bin"
    "$HOME/.elan/bin"
    "$HOME/.opencode/bin"
    "$HOME/.var/app/com.anthropic.ClaudeDesktop/bin"
  ];
}
