{ lib, pkgs, ... }:
let
  check = pkgs.writeShellApplication {
    name = "machine-disk-check";
    runtimeInputs = [
      pkgs.gawk
      pkgs.libnotify
    ];
    text = ''
      unallocated="$(/usr/bin/btrfs filesystem usage -b /var/home 2>/dev/null | awk '/Device unallocated:/ { print $3 }')"
      metadata_free="$(/usr/bin/btrfs filesystem df -b /var/home | awk -F'[=,]' '/^Metadata/ { print $3 - $5 }')"
      if (( unallocated < 2 * 1024 ** 3 && metadata_free < 1024 ** 3 )); then
        urgency=critical
      elif (( unallocated < 20 * 1024 ** 3 && metadata_free < 4 * 1024 ** 3 )); then
        urgency=normal
      else
        exit 0
      fi
      notify-send --urgency="$urgency" --app-name=machine "Disk metadata is running low" \
        "Unallocated $((unallocated / 1024 ** 2)) MiB, metadata free $((metadata_free / 1024 ** 2)) MiB. The weekly btrfs-balance fixes this; to do it now: sudo btrfs balance start -dusage=50 /var/home"
    '';
  };
in
{
  systemd.user = {
    services.machine-disk-check = {
      Unit.Description = "Warn before btrfs runs out of space for metadata";
      Service = {
        Type = "oneshot";
        ExecStart = lib.getExe check;
      };
    };
    timers.machine-disk-check = {
      Unit.Description = "Check btrfs allocation every 30 minutes";
      Timer = {
        OnBootSec = "5m";
        OnUnitActiveSec = "30m";
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
