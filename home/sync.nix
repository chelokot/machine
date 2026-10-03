{ config, machine, ... }:
{
  systemd.user = {
    services.machine-sync = {
      Unit.Description = "Capture desktop changes, pull chelokot/machine and apply it";
      Service = {
        Type = "oneshot";
        Environment = "PATH=${config.home.homeDirectory}/.local/bin:${config.home.profileDirectory}/bin:/nix/var/nix/profiles/default/bin:/usr/local/bin:/usr/bin";
        ExecStart = "${machine}/bin/machine sync";
      };
    };
    timers.machine-sync = {
      Unit.Description = "Hourly machine sync";
      Timer = {
        OnCalendar = "hourly";
        RandomizedDelaySec = "10m";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
