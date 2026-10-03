# machine

Declarative, self-recording Fedora Silverblue workstation.

The goal: every machine is described by public, reproducible, content-addressed artifacts that are rebuilt on a schedule, and anything changed by hand flows back into this description automatically.

| Layer | Technology | State |
| --- | --- | --- |
| Dev container | [`chelokot/fedora-toolbox`](https://github.com/chelokot/fedora-toolbox) image, recorded by `machine record` | done |
| Host OS | bootc image on top of `quay.io/shadowblue/main(-nvidia)` with Nix built in | planned |
| User environment | Nix flake + home-manager (fish, starship, GNOME, Ptyxis, quadlets) | planned |
| Sync | `machine sync` timer pulling this repo and applying it | planned |

## `machine record`

Called by the package manager wrappers in the dev container after a successful command:

```sh
machine record dnf install -y htop
```

It recognises installs and removals for `dnf`, `pipx`, `npm -g` and `bun -g`, edits the matching `packages/*.txt` manifest in a checkout of the image repository, commits and pushes to `main` in the background. Under `sudo` the git work runs as the invoking user. Errors go to `~/.local/state/machine/record.log`.

The CLI ships as a static binary in `ghcr.io/chelokot/machine-cli`, signed with cosign (keyless) and carrying build provenance.
