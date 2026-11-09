{ config, lib, ... }:
let
  name = "anthropic-proxy";
  image = "docker.io/ubuntu/squid@sha256:6a097f68bae708cedbabd6188d68c7e2e7a38cedd05a176e1cc0ba29e3bbe029";
  loylex = "100.77.232.19";
in
{
  xdg.configFile."${name}/squid.conf" = {
    text = ''
      http_port 3128
      acl loylex src ${loylex}
      acl anthropic dstdomain .anthropic.com .claude.ai .claude.com
      acl CONNECT method CONNECT
      acl https port 443
      http_access allow loylex anthropic CONNECT https
      http_access deny all
      cache deny all
      access_log stdio:/dev/stdout
      cache_log /dev/stderr
      pid_filename none
      pinger_enable off
      shutdown_lifetime 1 seconds
    '';
    onChange = "run /usr/bin/systemctl --user restart ${name}.service";
  };

  xdg.configFile."containers/systemd/${name}.container" = {
    text = lib.generators.toINI { listsAsDuplicateKeys = true; } {
      Unit.Description = "Squid relaying Loylex's Anthropic traffic through this machine";
      Container = {
        ContainerName = name;
        Image = image;
        Network = "host";
        SecurityLabelDisable = true;
        Volume = "${config.xdg.configHome}/${name}/squid.conf:/etc/squid/squid.conf:ro";
      };
      Service.Restart = "always";
      Install.WantedBy = "default.target";
    };
    onChange = ''
      run /usr/bin/systemctl --user daemon-reload
      run /usr/bin/systemctl --user restart ${name}.service
    '';
  };
}
