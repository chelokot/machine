{ config, ... }:
{
  xdg.configFile."systemd/user/uresourced.service".source = config.lib.file.mkOutOfStoreSymlink "/dev/null";
}
