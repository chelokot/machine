{ lib, pkgs, ... }:
let
  dconfFiles = lib.filter (lib.hasSuffix ".ini") (lib.attrNames (builtins.readDir ./dconf));
in
{
  home.activation.dconf = lib.hm.dag.entryAfter [ "writeBoundary" ] (
    lib.concatMapStrings (file: ''
      run ${lib.getExe pkgs.dconf} load /${
        lib.replaceStrings [ "." ] [ "/" ] (lib.removeSuffix ".ini" file)
      }/ < ${./dconf + "/${file}"}
    '') dconfFiles
  );

  home.activation.chelotype = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    config="$HOME/.config/chelotype/config"
    run mkdir -p "$(dirname "$config")"
    run touch "$config"
    run ${lib.getExe pkgs.gnused} -i '/^startup_launch_target=/d' "$config"
    run sh -c 'echo startup_launch_target=toolbox:fedora-toolbox >> "$1"' _ "$config"
  '';

  home.packages = [
    (pkgs.writeShellScriptBin "flatpak" ''
      /usr/bin/flatpak "$@"
      status=$?
      if [ "$status" -eq 0 ]; then
        for argument in "$@"; do
          case "$argument" in
            install | uninstall | remove)
              setsid -f machine capture >/dev/null 2>&1
              break
              ;;
          esac
        done
      fi
      exit "$status"
    '')
    (pkgs.writeShellScriptBin "rpm-ostree" ''
      /usr/bin/rpm-ostree "$@"
      status=$?
      if [ "$status" -eq 0 ]; then
        machine record rpm-ostree "$@"
      fi
      exit "$status"
    '')
  ];
}
