{
  config,
  lib,
  pkgs,
  fedora-toolbox,
  ...
}:
let
  name = "fedora-toolbox";
in
{
  xdg.configFile."containers/systemd/${name}.container".text = lib.generators.toINI { listsAsDuplicateKeys = true; } {
    Unit.Description = "Development container from ghcr.io/chelokot/fedora-toolbox";
    Container = {
      ContainerName = name;
      Image = "ghcr.io/chelokot/fedora-toolbox:latest";
      AutoUpdate = "registry";
      Pull = "newer";
      UserNS = "keep-id";
      SecurityLabelDisable = true;
      Network = "host";
      Ipc = "host";
      PodmanArgs = [
        "--pid=host"
        "--privileged"
        "--ulimit=host"
      ];
      Volume = [
        "${config.home.homeDirectory}:${config.home.homeDirectory}:rslave"
        "/run/user/%U:/run/user/%U"
        "/tmp/.X11-unix:/tmp/.X11-unix"
        "/var/mnt:/var/mnt:rslave"
        "/nix:/nix"
      ];
      Environment = [
        "HOME=${config.home.homeDirectory}"
        "XDG_RUNTIME_DIR=/run/user/%U"
        "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus"
        "WAYLAND_DISPLAY=wayland-0"
        "DISPLAY=:0"
        "NIX_REMOTE=daemon"
      ];
      Exec = "sleep infinity";
    };
    Service.Restart = "always";
    Install.WantedBy = "default.target";
  };

  home.activation.devContainer = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    run /usr/bin/systemctl --user daemon-reload
    run /usr/bin/systemctl --user start ${name}.service
  '';

  systemd.user = {
    services."${name}-update" = {
      Unit.Description = "Pull the latest ${name} image and restart the container";
      Service = {
        Type = "oneshot";
        ExecStart = "/usr/bin/podman auto-update";
      };
    };
    timers."${name}-update" = {
      Unit.Description = "Daily ${name} image update";
      Timer = {
        OnCalendar = "04:00";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };

  home.packages = [ (pkgs.writeScriptBin "dev" (builtins.readFile "${fedora-toolbox}/host/fedora-toolbox-fast-shell")) ];
}
