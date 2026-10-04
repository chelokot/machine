{
  config,
  lib,
  pkgs,
  dev,
  ...
}:
let
  name = "dev";
  image = "ghcr.io/chelokot/${name}:latest";
in
{
  xdg.configFile."containers/systemd/${name}.container".text =
    lib.generators.toINI { listsAsDuplicateKeys = true; }
      {
        Unit.Description = "Development container from ${image}";
        Container = {
          ContainerName = name;
          Image = image;
          Pull = "newer";
          UserNS = "keep-id";
          SecurityLabelDisable = true;
          Network = "host";
          PodmanArgs = [
            "--ipc=host"
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

  home.activation.devContainer = lib.hm.dag.entryAfter [ "linkGeneration" ] ''
    run /usr/bin/systemctl --user daemon-reload
    run /usr/bin/systemctl --user start ${name}.service
  '';

  systemd.user = {
    services."${name}-update" = {
      Unit.Description = "Pull the latest ${name} image and restart the container once nothing runs in it";
      Service = {
        Type = "oneshot";
        ExecStart = lib.getExe (
          pkgs.writeShellApplication {
            name = "${name}-update";
            text = ''
              /usr/bin/podman pull --quiet ${image} > /dev/null
              running="$(/usr/bin/podman container inspect --format '{{.Image}}' ${name})"
              latest="$(/usr/bin/podman image inspect --format '{{.Id}}' ${image})"
              cgroup="/sys/fs/cgroup$(/usr/bin/podman container inspect --format '{{.State.CgroupPath}}' ${name})"
              if [ "$running" != "$latest" ] && [ "$(wc -l < "$cgroup/cgroup.procs")" -eq 1 ]; then
                /usr/bin/systemctl --user restart ${name}.service
              fi
            '';
          }
        );
      };
    };
    timers."${name}-update" = {
      Unit.Description = "Hourly ${name} image update";
      Timer.OnCalendar = "hourly";
      Install.WantedBy = [ "timers.target" ];
    };
  };

  home.packages = [
    (pkgs.writeScriptBin "dev" (builtins.readFile "${dev}/host/dev"))
  ];
}
