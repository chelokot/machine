{
  lib,
  pkgs,
  ...
}:
let
  name = "aws-sso-login";
  ssoSession = "orn";
in
{
  systemd.user = {
    services.${name} = {
      Unit.Description = "Log the AWS CLI in to the ${ssoSession} SSO session headlessly with credentials from the keyring";
      Service = {
        Type = "oneshot";
        StateDirectory = name;
        Restart = "on-failure";
        RestartSec = "5m";
        ExecStart = lib.getExe (
          pkgs.writeShellApplication {
            inherit name;
            runtimeInputs = with pkgs; [
              awscli2
              libsecret
            ];
            runtimeEnv.PLAYWRIGHT_BROWSERS_PATH = pkgs.playwright-driver.browsers;
            text = ''
              exec ${
                pkgs.python3.withPackages (python: [
                  python.playwright
                  python.pyotp
                ])
              }/bin/python ${./aws-sso-login.py} ${ssoSession}
            '';
          }
        );
      };
    };
    timers.${name} = {
      Unit.Description = "Six-hourly ${name} renewal";
      Timer = {
        OnCalendar = "00/6:00";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
