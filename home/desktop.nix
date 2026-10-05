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

  home.activation.files = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    run cp --recursive --no-target-directory --no-preserve=mode ${./files} "$HOME"
  '';

  home.packages = [
    (pkgs.writeShellScriptBin "flatpak" ''
      if [ -e /run/.containerenv ]; then
        exec host-spawn flatpak "$@"
      fi
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
      if [ -e /run/.containerenv ]; then
        exec host-spawn rpm-ostree "$@"
      fi
      /usr/bin/rpm-ostree "$@"
      status=$?
      if [ "$status" -eq 0 ]; then
        machine record rpm-ostree "$@"
      fi
      exit "$status"
    '')
  ];
}
