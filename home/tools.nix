{
  config,
  lib,
  pkgs,
  ...
}:
let
  opt = {
    "blender-4.2.14-linux-x64" = pkgs.fetchzip {
      url = "https://download.blender.org/release/Blender4.2/blender-4.2.14-linux-x64.tar.xz";
      hash = "sha256-JQonAeb39Qjtx2WXrN1M8ehgux1IPr8aDINxAeak6Zw=";
    };
    "blender-4.5.14-linux-x64" = pkgs.fetchzip {
      url = "https://download.blender.org/release/Blender4.5/blender-4.5.14-linux-x64.tar.xz";
      hash = "sha256-H7U1mreuRgDDDxz8SixEGBlFMMl5++wyWNOmHCXjejM=";
    };
    "godot-4.6.3" = pkgs.fetchzip {
      url = "https://github.com/godotengine/godot-builds/releases/download/4.6.3-stable/Godot_v4.6.3-stable_linux.x86_64.zip";
      hash = "sha256-CQryrRkZmByCMACq1LkmQIZYcstgvnSkdgoceSSfw/M=";
      stripRoot = false;
    };
    "godot-4.7.2" = pkgs.fetchzip {
      url = "https://github.com/godotengine/godot-builds/releases/download/4.7.2-stable/Godot_v4.7.2-stable_linux.x86_64.zip";
      hash = "sha256-+FmJl4dfXUMUBjII7stmYRCfDR/G3SJpgi2/EeFuNek=";
      stripRoot = false;
    };
    REAPER = "${
      pkgs.fetchzip {
        url = "https://www.reaper.fm/files/7.x/reaper779_linux_x86_64.tar.xz";
        hash = "sha256-UHipv2Tq1tLn43a++RM2tiWq23qtO9x8ggxZ/iVALC0=";
      }
    }/REAPER";
  };
  local = name: config.lib.file.mkOutOfStoreSymlink "${config.home.homeDirectory}/.local/opt/${name}";
in
{
  home.packages = with pkgs; [
    act
    awscli2
    bun
    elan
    gh
    poetry
    rustup
    sfizz
    sops
  ];

  home.file =
    lib.mapAttrs' (name: source: lib.nameValuePair ".local/opt/${name}" { inherit source; }) opt
    // {
      ".local/bin/blender".source = local "blender-4.2.14-linux-x64/blender";
      ".local/bin/godot".source = local "godot-4.7.2/Godot_v4.7.2-stable_linux.x86_64";
    };
}
