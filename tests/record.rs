use std::fs;
use std::path::Path;
use std::process::Command;

use std::collections::BTreeSet;

use machine::desktop::{filter_dump, merge};
use machine::git::Target;
use machine::record::{Action, Change, Manager, apply, parse};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn change(manager: Manager, action: Action, specs: &[&str]) -> Change {
    Change {
        manager,
        action,
        specs: args(specs),
    }
}

#[test]
fn dnf_install_skips_option_values_and_files() {
    let parsed = parse(
        Manager::Dnf,
        &args(&["-y", "--repo", "fedora", "install", "htop", "./local.rpm", "/usr/bin/xxd", "https://x/y.rpm"]),
    );
    assert_eq!(parsed, Some(change(Manager::Dnf, Action::Add, &["htop"])));
}

#[test]
fn dnf_remove_and_ignored_invocations() {
    assert_eq!(
        parse(Manager::Dnf, &args(&["remove", "-y", "htop"])),
        Some(change(Manager::Dnf, Action::Remove, &["htop"]))
    );
    assert_eq!(parse(Manager::Dnf, &args(&["install", "--downloadonly", "htop"])), None);
    assert_eq!(parse(Manager::Dnf, &args(&["search", "htop"])), None);
    assert_eq!(
        parse(Manager::Dnf, &args(&["--setopt=install_weak_deps=False", "install", "htop"])),
        Some(change(Manager::Dnf, Action::Add, &["htop"]))
    );
}

#[test]
fn npm_and_bun_record_only_global_installs() {
    assert_eq!(parse(Manager::Npm, &args(&["install", "left-pad"])), None);
    assert_eq!(
        parse(Manager::Npm, &args(&["i", "-g", "prettier"])),
        Some(change(Manager::Npm, Action::Add, &["prettier"]))
    );
    assert_eq!(
        parse(Manager::Bun, &args(&["remove", "--global", "@openai/codex"])),
        Some(change(Manager::Bun, Action::Remove, &["@openai/codex"]))
    );
}

#[test]
fn pipx_skips_editable_installs() {
    assert_eq!(
        parse(Manager::Pipx, &args(&["install", "--python", "python3.12", "httpie"])),
        Some(change(Manager::Pipx, Action::Add, &["httpie"]))
    );
    assert_eq!(parse(Manager::Pipx, &args(&["install", "-e", "."])), None);
}

#[test]
fn rpm_ostree_layering() {
    assert_eq!(
        parse(Manager::RpmOstree, &args(&["install", "--apply-live", "netbird"])),
        Some(change(Manager::RpmOstree, Action::Add, &["netbird"]))
    );
    assert_eq!(
        parse(Manager::RpmOstree, &args(&["uninstall", "netbird"])),
        Some(change(Manager::RpmOstree, Action::Remove, &["netbird"]))
    );
    assert_eq!(parse(Manager::RpmOstree, &args(&["upgrade"])), None);
}

#[test]
fn merge_records_only_local_changes_since_last_observation() {
    let root = tempfile::tempdir().unwrap();
    let manifest = root.path().join("flatpaks.txt");
    let observed = root.path().join("state/flatpaks.txt");
    fs::write(&manifest, "system flathub org.gnome.Loupe\nuser chelotype com.chelokot.Chelotype\n").unwrap();
    let local = |entries: &[&str]| entries.iter().map(|entry| entry.to_string()).collect::<BTreeSet<String>>();

    merge(
        &manifest,
        &observed,
        &local(&["system flathub org.gnome.Loupe", "system flathub org.gnome.Weather"]),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&manifest).unwrap(),
        "system flathub org.gnome.Loupe\nsystem flathub org.gnome.Weather\nuser chelotype com.chelokot.Chelotype\n"
    );

    merge(&manifest, &observed, &local(&["system flathub org.gnome.Weather"])).unwrap();
    assert_eq!(
        fs::read_to_string(&manifest).unwrap(),
        "system flathub org.gnome.Weather\nuser chelotype com.chelokot.Chelotype\n"
    );
}

#[test]
fn dconf_dump_drops_ignored_keys_and_empty_sections() {
    let dump = "[/]\ncolor-scheme='prefer-dark'\nwindow-size=(1, 2)\n\n[window-state]\ninitial-size=(3, 4)\n\n[keybindings]\nmaximize=@as []\n";
    let ignored = vec!["window-size".to_owned(), "initial-size*".to_owned()];
    assert_eq!(
        filter_dump(dump, &ignored),
        "[/]\ncolor-scheme='prefer-dark'\n\n[keybindings]\nmaximize=@as []\n"
    );
}

fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn records_round_trip_through_remote() {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("remote.git");
    let seed = root.path().join("seed");
    git(root.path(), &["init", "--quiet", "--bare", "-b", "main", remote.to_str().unwrap()]);
    git(root.path(), &["clone", "--quiet", remote.to_str().unwrap(), seed.to_str().unwrap()]);
    fs::create_dir(seed.join("packages")).unwrap();
    fs::write(seed.join("packages/dnf.txt"), "# toolbox packages\ngit\nzip\n").unwrap();
    fs::write(seed.join("packages/dnf-remove.txt"), "").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "--quiet", "-m", "seed"]);
    git(&seed, &["push", "--quiet", "origin", "HEAD:main"]);

    for (name, value) in [
        ("GIT_AUTHOR_NAME", "test"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "test"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        unsafe { std::env::set_var(name, value) };
    }
    let target = Target {
        checkout: root.path().join("checkout"),
        remote: remote.to_str().unwrap().to_owned(),
    };
    let remote_file = |name: &str| git(&remote, &["show", &format!("main:packages/{name}")]);

    apply(&target, &change(Manager::Dnf, Action::Add, &["htop", "Bat"])).unwrap();
    assert_eq!(remote_file("dnf.txt"), "# toolbox packages\nBat\ngit\nhtop\nzip\n");
    apply(&target, &change(Manager::Dnf, Action::Remove, &["htop", "nano"])).unwrap();
    assert_eq!(remote_file("dnf.txt"), "# toolbox packages\nBat\ngit\nzip\n");
    assert_eq!(remote_file("dnf-remove.txt"), "nano\n");
    apply(&target, &change(Manager::Dnf, Action::Add, &["nano"])).unwrap();
    assert_eq!(remote_file("dnf-remove.txt"), "");
    apply(&target, &change(Manager::Dnf, Action::Add, &["nano"])).unwrap();

    let log = git(&remote, &["log", "--format=%s", "main"]);
    assert_eq!(
        log.lines().take(4).collect::<Vec<_>>(),
        [
            "Add nano to dnf packages",
            "Remove htop nano from dnf packages",
            "Add htop Bat to dnf packages",
            "seed"
        ]
    );
}
