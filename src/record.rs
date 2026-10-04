use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::git::{Checkout, Repo, Target};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Manager {
    Dnf,
    Pipx,
    Npm,
    Bun,
    RpmOstree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Action {
    Add,
    Remove,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    pub manager: Manager,
    pub action: Action,
    pub specs: Vec<String>,
}

struct Rules {
    repo: Repo,
    manifest: &'static str,
    removed: Option<&'static str>,
    subcommands: &'static [(&'static str, Action)],
    value_options: &'static [&'static str],
    skip_flags: &'static [&'static str],
    global_flags: &'static [&'static str],
    spec: fn(&str) -> Option<String>,
}

impl Manager {
    pub fn name(self) -> &'static str {
        match self {
            Manager::Dnf => "dnf",
            Manager::Pipx => "pipx",
            Manager::Npm => "npm",
            Manager::Bun => "bun",
            Manager::RpmOstree => "rpm-ostree",
        }
    }

    pub fn repo(self) -> Repo {
        self.rules().repo
    }

    fn rules(self) -> Rules {
        match self {
            Manager::Dnf => Rules {
                repo: Repo::Toolbox,
                manifest: "packages/dnf.txt",
                removed: Some("packages/dnf-remove.txt"),
                subcommands: &[
                    ("install", Action::Add),
                    ("in", Action::Add),
                    ("remove", Action::Remove),
                    ("rm", Action::Remove),
                    ("erase", Action::Remove),
                ],
                value_options: &[
                    "-c",
                    "--config",
                    "--repo",
                    "--repofrompath",
                    "--enablerepo",
                    "--disablerepo",
                    "--setopt",
                    "--setvar",
                    "--releasever",
                    "-x",
                    "--exclude",
                    "--from-repo",
                    "--comment",
                    "--forcearch",
                    "--destdir",
                    "--advisories",
                    "--advisory-severities",
                    "--bzs",
                    "--cves",
                ],
                skip_flags: &["--downloadonly", "--assumeno", "--installroot", "--use-host-config"],
                global_flags: &[],
                spec: package_spec,
            },
            Manager::Pipx => Rules {
                repo: Repo::Toolbox,
                manifest: "packages/pipx.txt",
                removed: None,
                subcommands: &[("install", Action::Add), ("uninstall", Action::Remove)],
                value_options: &["--python", "--index-url", "-i", "--pip-args", "--suffix", "--preinstall", "--fetch-python"],
                skip_flags: &["--editable", "-e"],
                global_flags: &[],
                spec: package_spec,
            },
            Manager::Npm => Rules {
                repo: Repo::Toolbox,
                manifest: "packages/npm.txt",
                removed: None,
                subcommands: &[
                    ("install", Action::Add),
                    ("i", Action::Add),
                    ("add", Action::Add),
                    ("uninstall", Action::Remove),
                    ("remove", Action::Remove),
                    ("rm", Action::Remove),
                    ("r", Action::Remove),
                    ("un", Action::Remove),
                ],
                value_options: &["--registry", "--tag", "--cache", "--userconfig", "--workspace", "-w"],
                skip_flags: &["--prefix", "--dry-run"],
                global_flags: &["-g", "--global", "--location=global"],
                spec: package_spec,
            },
            Manager::Bun => Rules {
                repo: Repo::Toolbox,
                manifest: "packages/bun.txt",
                removed: None,
                subcommands: &[
                    ("add", Action::Add),
                    ("install", Action::Add),
                    ("i", Action::Add),
                    ("remove", Action::Remove),
                    ("rm", Action::Remove),
                ],
                value_options: &["--registry", "--cwd", "--config", "-c", "--backend", "--cache-dir"],
                skip_flags: &["--dry-run", "--cwd"],
                global_flags: &["-g", "--global"],
                spec: package_spec,
            },
            Manager::RpmOstree => Rules {
                repo: Repo::Machine,
                manifest: "image/packages.txt",
                removed: None,
                subcommands: &[("install", Action::Add), ("uninstall", Action::Remove)],
                value_options: &["--os", "--sysroot", "--install", "--uninstall"],
                skip_flags: &["--dry-run", "-n"],
                global_flags: &[],
                spec: package_spec,
            },
        }
    }
}

fn package_spec(spec: &str) -> Option<String> {
    let first_is_valid = spec.chars().next().is_some_and(|first| first.is_ascii_alphanumeric() || first == '@');
    let valid = first_is_valid
        && spec
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._+@/:=<>~^*-".contains(character))
        && !spec.contains("://")
        && !spec.ends_with(".rpm");
    valid.then(|| spec.to_owned())
}

pub fn parse(manager: Manager, args: &[String]) -> Option<Change> {
    let rules = manager.rules();
    let mut positionals: Vec<&str> = Vec::new();
    let mut flags: Vec<&str> = Vec::new();
    let mut expects_value = false;
    for argument in args {
        if expects_value {
            expects_value = false;
        } else if let Some(stripped) = argument.strip_prefix('-') {
            let name = argument.split_once('=').map_or(argument.as_str(), |(name, _)| name);
            flags.extend([argument.as_str(), name]);
            expects_value = !stripped.contains('=') && rules.value_options.contains(&name);
        } else {
            positionals.push(argument);
        }
    }
    let has = |candidates: &[&str]| flags.iter().any(|flag| candidates.contains(flag));
    if has(rules.skip_flags) || (!rules.global_flags.is_empty() && !has(rules.global_flags)) {
        return None;
    }
    let (subcommand, rest) = positionals.split_first()?;
    let action = rules.subcommands.iter().find(|(name, _)| name == subcommand)?.1;
    let specs: Vec<String> = rest.iter().filter_map(|spec| (rules.spec)(spec)).collect();
    (!specs.is_empty()).then_some(Change { manager, action, specs })
}

pub struct Manifest {
    header: Vec<String>,
    entries: Vec<String>,
}

impl Manifest {
    pub fn read(path: &Path) -> Result<Manifest> {
        let text = if path.exists() { fs::read_to_string(path)? } else { String::new() };
        let (header, entries): (Vec<&str>, Vec<&str>) = text.lines().filter(|line| !line.trim().is_empty()).partition(|line| line.starts_with('#'));
        Ok(Manifest {
            header: header.into_iter().map(str::to_owned).collect(),
            entries: entries.into_iter().map(|line| line.trim().to_owned()).collect(),
        })
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn write(mut self, path: &Path, add: &[&String], remove: &[&String]) -> Result<()> {
        self.entries.retain(|entry| !remove.contains(&entry));
        self.entries.extend(add.iter().map(|spec| spec.to_string()));
        self.entries.sort_by_key(|entry| entry.to_lowercase());
        self.entries.dedup();
        let text: String = self.header.iter().chain(&self.entries).map(|line| format!("{line}\n")).collect();
        Ok(fs::write(path, text)?)
    }
}

pub fn edit(root: &Path, change: &Change) -> Result<()> {
    let rules = change.manager.rules();
    let manifest_path = root.join(rules.manifest);
    let manifest = Manifest::read(&manifest_path)?;
    let specs: Vec<&String> = change.specs.iter().collect();
    match change.action {
        Action::Add => manifest.write(&manifest_path, &specs, &[])?,
        Action::Remove => manifest.write(&manifest_path, &[], &specs)?,
    }
    match (change.action, rules.removed) {
        (Action::Add, Some(removed)) => {
            let removed_path = root.join(removed);
            Manifest::read(&removed_path)?.write(&removed_path, &[], &specs)
        }
        _ => Ok(()),
    }
}

pub fn apply(target: &Target, change: &Change) -> Result<()> {
    let checkout = Checkout::open(target)?;
    checkout.pull()?;
    edit(&checkout.root, change)?;
    let rules = change.manager.rules();
    let paths: Vec<&str> = [Some(rules.manifest), rules.removed].into_iter().flatten().collect();
    let (verb, preposition) = match change.action {
        Action::Add => ("Add", "to"),
        Action::Remove => ("Remove", "from"),
    };
    let message = format!("{verb} {} {preposition} {} packages", change.specs.join(" "), change.manager.name());
    if checkout.commit(&paths, &message)? {
        checkout.push()?;
    }
    Ok(())
}
