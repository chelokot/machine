{
  lib,
  pkgs,
  host,
  ...
}:
let
  name = "glab-token";
  tokenName = "glab-${host}";
in
{
  systemd.user = {
    services.${name} = {
      Unit.Description = "Log glab in to GitLab with a personal access token minted over SSH and revoke the previous one";
      Service = {
        Type = "oneshot";
        ExecStart = lib.getExe (
          pkgs.writeShellApplication {
            inherit name;
            runtimeInputs = with pkgs; [
              glab
              jq
            ];
            text = ''
              /usr/bin/ssh -o BatchMode=yes git@gitlab.com personal_access_token ${tokenName} api 30 \
                | sed -n 's/^Token: *//p' \
                | glab auth login --hostname gitlab.com --stdin
              current="$(glab api personal_access_tokens/self | jq .id)"
              glab api "personal_access_tokens?state=active&search=${tokenName}" \
                | jq --arg name ${tokenName} --argjson current "$current" '.[] | select(.name == $name and .id != $current) | .id' \
                | xargs -r -I{} glab api --method DELETE personal_access_tokens/{}
            '';
          }
        );
      };
    };
    timers.${name} = {
      Unit.Description = "Weekly ${name} renewal";
      Timer = {
        OnCalendar = "weekly";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
