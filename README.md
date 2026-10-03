# machine

Declarative, self-recording Fedora Silverblue workstation.

Every layer of a machine is described by public, content-addressed artifacts that CI rebuilds on a schedule, and changes made by hand flow back into this repository automatically.

| Layer | Source of truth | Built by | Applied by |
| --- | --- | --- | --- |
| Host OS | `image/` (bootc, on top of `quay.io/shadowblue/main(-nvidia)`) | `image.yml`, daily | `bootc`/`rpm-ostree` staged updates |
| User environment | `flake.nix`, `home/` (home-manager) | `nix.yml`, `flake.lock` updated weekly | `machine sync`, hourly |
| Desktop state | `home/dconf/*.ini`, `home/flatpaks.txt`, `home/gnome-extensions.txt` | captured from the machine | `machine sync` |
| Dev container | [`chelokot/fedora-toolbox`](https://github.com/chelokot/fedora-toolbox) | daily | Podman Quadlet `AutoUpdate=registry` |

## Principles

- **Declared, not remembered.** Package lists, flatpaks, remotes, GNOME settings and extensions are plain text manifests. Nix and the image builds only read them.
- **Recorded, not retyped.** `dnf`/`pipx`/`npm -g`/`bun -g` in the container and `rpm-ostree`/`flatpak` on the host are wrapped: a successful change is committed and pushed by `machine record` or `machine capture`. GNOME settings changed in the UI are captured by the hourly sync. Capture is a three-way merge against the last state observed on that machine, so one machine never deletes what it simply has not installed yet.
- **Content-addressed and reproducible.** OCI digests for images, `flake.lock` and the Nix store for the user environment, `Cargo.lock` for the CLI.
- **Cattle containers.** The dev container is a Quadlet unit recreated from the image on every start and updated daily; all state lives in `$HOME`, so there is no container state to drift into an improper state. `dev` opens a shell in it, host commands (`xdg-open`, `flatpak`, `systemctl`, `podman`, ...) are bridged through `host-spawn` and the podman socket.
- **No secrets in git.** Nothing secret is captured (dconf is captured per tracked path with volatile keys filtered), gitleaks scans every push, images are signed with cosign keyless signatures from GitHub OIDC and carry build provenance, so no signing key exists anywhere.

## New machine

```sh
sudo bootc switch ghcr.io/chelokot/machine:nvidia   # or :main for AMD/Intel
systemctl reboot
```

After the reboot Nix is available (`/nix` is bind-mounted from `/var/lib/nix`):

```sh
mv ~/.gitconfig ~/.gitconfig.pre-machine            # git config now lives in home-manager
gh auth login                                       # pushes from record/capture/sync
nix run github:chelokot/machine -- bootstrap laptop # or server
```

`bootstrap` clones this repository to `~/.local/share/machine`, activates `homeConfigurations."chelokot@<host>"` (existing dotfiles are kept as `*.backup`), adds flatpak remotes, installs declared flatpaks and GNOME extensions, and starts the dev container.

## Day to day

| Command | What it does |
| --- | --- |
| `dev` | Shell in the `fedora-toolbox` container in the current directory |
| `sudo dnf install foo` (in `dev`) | Installs now, records `foo` into `chelokot/fedora-toolbox` |
| `rpm-ostree install foo` | Layers now, records `foo` into `image/packages.txt` |
| `flatpak install ...` | Installs now, captures the flatpak list |
| `machine capture` | Commits current GNOME settings, extensions and flatpaks |
| `machine sync` | Capture, pull, push, `home-manager switch`, install missing flatpaks and extensions |

Recording failures are logged to `~/.local/state/machine/record.log`; `MACHINE_RECORD=0` skips recording for one command.

## Verifying artifacts

```sh
cosign verify ghcr.io/chelokot/machine:nvidia \
  --certificate-identity-regexp 'https://github.com/chelokot/machine/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
gh attestation verify oci://ghcr.io/chelokot/machine:nvidia --owner chelokot
```
