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
  claim = pkgs.writeShellApplication {
    name = "${name}-claim";
    text = ''
      if [ -z "$(/usr/bin/podman ps --quiet --filter "name=^${name}$")" ]; then
        /usr/bin/podman rm --force --ignore ${name} > /dev/null
        /usr/bin/podman rename "${name}-$1" ${name}
      fi
    '';
  };
  release = pkgs.writeShellApplication {
    name = "${name}-release";
    text = ''
      current="$(/usr/bin/podman ps --all --filter "label=PODMAN_SYSTEMD_UNIT=${name}@$1.service" --format '{{.Names}}')"
      if [ -n "$current" ] && [ "$current" != "${name}-$1" ]; then
        /usr/bin/podman rename "$current" "${name}-$1"
      fi
    '';
  };
in
{
  xdg.configFile."containers/systemd/${name}@.container".text =
    lib.generators.toINI { listsAsDuplicateKeys = true; }
      {
        Unit.Description = "Development container from ${image}";
        Container = {
          ContainerName = "${name}-%i";
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
            "XDG_SESSION_TYPE=wayland"
            "WAYLAND_DISPLAY=wayland-0"
            "DISPLAY=:0"
            "NIX_REMOTE=daemon"
          ];
          Exec = "sleep infinity";
        };
        Service = {
          Restart = "always";
          SuccessExitStatus = 143;
          ExecStartPost = "${lib.getExe claim} %i";
          ExecStop = "${lib.getExe release} %i";
        };
        Install = {
          WantedBy = "default.target";
          DefaultInstance = "boot";
        };
      };

  home.activation.devContainer = lib.hm.dag.entryAfter [ "linkGeneration" ] ''
    run /usr/bin/systemctl --user daemon-reload
    if [ -z "$(/usr/bin/podman ps --quiet --filter "name=^${name}$")" ]; then
      run /usr/bin/systemctl --user start ${name}@boot.service
    fi
  '';

  systemd.user = {
    services."${name}-update" = {
      Unit.Description = "Move ${name} to the latest image in a new container and stop old ones once nothing runs in them";
      Service = {
        Type = "oneshot";
        ExecStart = lib.getExe (
          pkgs.writeShellApplication {
            name = "${name}-update";
            text = ''
              /usr/bin/podman pull --quiet ${image} > /dev/null
              latest="$(/usr/bin/podman image inspect --format '{{.Id}}' ${image})"
              active="$(/usr/bin/podman container inspect --format '{{index .Config.Labels "PODMAN_SYSTEMD_UNIT"}}' ${name})"
              if [ "$(/usr/bin/podman container inspect --format '{{.Image}}' ${name})" != "$latest" ] && [[ "$active" == ${name}@* ]]; then
                previous="''${active#${name}@}"
                previous="''${previous%.service}"
                next="$(date +%Y%m%d%H%M%S)"
                /usr/bin/systemctl --user start "${name}@$next.service"
                /usr/bin/podman rename ${name} "${name}-$previous"
                /usr/bin/podman rename "${name}-$next" ${name}
              fi
              /usr/bin/podman ps --filter label=PODMAN_SYSTEMD_UNIT --format '{{.Names}} {{index .Labels "PODMAN_SYSTEMD_UNIT"}}' |
                while read -r container unit; do
                  if [ "$container" = ${name} ] || [[ "$unit" != ${name}@* ]]; then
                    continue
                  fi
                  cgroup="/sys/fs/cgroup$(/usr/bin/podman container inspect --format '{{.State.CgroupPath}}' "$container")"
                  if [ "$(wc -l < "$cgroup/cgroup.procs")" -eq 1 ]; then
                    /usr/bin/systemctl --user stop "$unit"
                  fi
                done
              /usr/bin/podman images --quiet --filter dangling=true --filter containers=false --filter label=org.opencontainers.image.source=https://github.com/chelokot/${name} |
                xargs --no-run-if-empty /usr/bin/podman image rm > /dev/null
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

  home.packages =
    map (command: pkgs.writeScriptBin command (builtins.readFile "${dev}/host/${command}"))
      [
        "dev"
        "t3code"
      ];
}
