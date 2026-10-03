{
  description = "Declarative, self-recording Fedora Silverblue workstation";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fedora-toolbox = {
      url = "github:chelokot/fedora-toolbox";
      flake = false;
    };
    starship-show-on-command = {
      url = "github:chelokot/starship-show-on-command.fish";
      flake = false;
    };
    exposedcat-dotfiles = {
      url = "github:ExposedCat/dotfiles/0b9071e95f67f67dabb917d761a4fa2948c1148e";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      home-manager,
      ...
    }@inputs:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      lib = nixpkgs.lib;
      hosts = [
        "laptop"
        "server"
      ];
      machine = pkgs.rustPlatform.buildRustPackage {
        pname = "machine";
        version = (lib.importTOML ./Cargo.toml).package.version;
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./src
            ./tests
          ];
        };
        cargoLock.lockFile = ./Cargo.lock;
        nativeCheckInputs = [ pkgs.git ];
      };
      manifest =
        path: lib.filter (line: line != "" && !lib.hasPrefix "#" line) (lib.splitString "\n" (builtins.readFile path));
      home =
        host:
        home-manager.lib.homeManagerConfiguration {
          inherit pkgs;
          extraSpecialArgs = inputs // {
            inherit machine manifest host;
          };
          modules = [ ./home ];
        };
    in
    {
      packages.${system} = {
        default = machine;
        inherit machine;
        home-manager = home-manager.packages.${system}.default;
      };

      homeConfigurations = lib.genAttrs' hosts (host: lib.nameValuePair "chelokot@${host}" (home host));

      checks.${system} = {
        inherit machine;
        fish-prompt =
          pkgs.runCommand "fish-prompt-check"
            {
              nativeBuildInputs = [
                pkgs.fish
                pkgs.starship
              ];
              files = "${self.homeConfigurations."chelokot@laptop".activationPackage}/home-files";
            }
            ''
              export HOME=$TMPDIR XDG_CONFIG_HOME=$TMPDIR/.config COLUMNS=80
              cp -rL $files/.config $XDG_CONFIG_HOME
              fish --interactive --command fish_prompt > prompt 2> errors
              if [ -s errors ]; then cat errors; exit 1; fi
              grep -q . prompt
              cp prompt $out
            '';
        quadlet =
          pkgs.runCommand "quadlet-check"
            {
              nativeBuildInputs = [ pkgs.podman ];
              unit = "${self.homeConfigurations."chelokot@laptop".activationPackage}/home-files/.config/containers/systemd";
            }
            ''
              export HOME=$TMPDIR XDG_RUNTIME_DIR=$TMPDIR
              QUADLET_UNIT_DIRS=$unit ${pkgs.podman}/libexec/podman/quadlet --user --dryrun > $out 2> errors
              ! grep -E "unsupported|error" errors || (cat errors; exit 1)
            '';
      }
      // lib.genAttrs' hosts (
        host: lib.nameValuePair "home-${host}" self.homeConfigurations."chelokot@${host}".activationPackage
      );

      formatter.${system} = pkgs.nixfmt-rfc-style;
    };
}
