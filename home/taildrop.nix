{
  systemd.user.services.taildrop = {
    Unit.Description = "Move files sent to this machine over Taildrop into ~/Downloads";
    Service = {
      ExecStart = "/usr/bin/tailscale file get --loop --conflict=rename %h/Downloads";
      Restart = "always";
      RestartSec = "30s";
    };
    Install.WantedBy = [ "default.target" ];
  };
}
