use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Manager {
    Dnf,
    Pipx,
    Npm,
    Bun,
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
    manifest: &'static str,
    subcommands: &'static [(&'static str, Action)],
    value_options: &'static [&'static str],
    skip_flags: &'static [&'static str],
    global_flags: &'static [&'static str],
}

const DNF_REMOVED: &str = "dnf-remove.txt";

impl Manager {
    pub fn name(self) -> &'static str {
        match self {
            Manager::Dnf => "dnf",
            Manager::Pipx => "pipx",
            Manager::Npm => "npm",
            Manager::Bun => "bun",
        }
    }

    fn rules(self) -> Rules {
        match self {
            Manager::Dnf => Rules {
                manifest: "dnf.txt",
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
            },
            Manager::Pipx => Rules {
                manifest: "pipx.txt",
                subcommands: &[("install", Action::Add), ("uninstall", Action::Remove)],
                value_options: &["--python", "--index-url", "-i", "--pip-args", "--suffix", "--preinstall", "--fetch-python"],
                skip_flags: &["--editable", "-e"],
                global_flags: &[],
            },
            Manager::Npm => Rules {
                manifest: "npm.txt",
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
            },
            Manager::Bun => Rules {
                manifest: "bun.txt",
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
            },
        }
    }
}

fn is_recordable(spec: &str) -> bool {
    let mut characters = spec.chars();
    let first_is_valid = characters.next().is_some_and(|first| first.is_ascii_alphanumeric() || first == '@');
    first_is_valid
        && spec
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._+@/:=<>~^*-".contains(character))
        && !spec.contains("://")
        && !spec.ends_with(".rpm")
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
    let specs: Vec<String> = rest.iter().filter(|spec| is_recordable(spec)).map(|spec| spec.to_string()).collect();
    (!specs.is_empty()).then_some(Change { manager, action, specs })
}

struct Manifest {
    header: Vec<String>,
    entries: Vec<String>,
}

impl Manifest {
    fn read(path: &Path) -> Result<Manifest> {
        let text = if path.exists() { fs::read_to_string(path)? } else { String::new() };
        let (header, entries): (Vec<&str>, Vec<&str>) = text.lines().filter(|line| !line.trim().is_empty()).partition(|line| line.starts_with('#'));
        Ok(Manifest {
            header: header.into_iter().map(str::to_owned).collect(),
            entries: entries.into_iter().map(|line| line.trim().to_owned()).collect(),
        })
    }

    fn contains(&self, spec: &str) -> bool {
        self.entries.iter().any(|entry| entry == spec)
    }

    fn write(mut self, path: &Path, add: &[&String], remove: &[&String]) -> Result<()> {
        self.entries.retain(|entry| !remove.contains(&entry));
        self.entries.extend(add.iter().map(|spec| spec.to_string()));
        self.entries.sort_by_key(|entry| entry.to_lowercase());
        self.entries.dedup();
        let text: String = self.header.iter().chain(&self.entries).map(|line| format!("{line}\n")).collect();
        Ok(fs::write(path, text)?)
    }
}

pub fn edit(packages: &Path, change: &Change) -> Result<()> {
    let manifest_path = packages.join(change.manager.rules().manifest);
    let manifest = Manifest::read(&manifest_path)?;
    let specs: Vec<&String> = change.specs.iter().collect();
    let undeclared: Vec<&String> = specs.iter().copied().filter(|spec| !manifest.contains(spec)).collect();
    match change.action {
        Action::Add => manifest.write(&manifest_path, &specs, &[])?,
        Action::Remove => manifest.write(&manifest_path, &[], &specs)?,
    }
    if change.manager != Manager::Dnf {
        return Ok(());
    }
    let removed_path = packages.join(DNF_REMOVED);
    let removed = Manifest::read(&removed_path)?;
    match change.action {
        Action::Add => removed.write(&removed_path, &[], &specs),
        Action::Remove => removed.write(&removed_path, &undeclared, &[]),
    }
}

pub struct Target {
    pub checkout: PathBuf,
    pub remote: String,
}

fn git(checkout: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .context("git is not available")?;
    if !output.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn apply(target: &Target, change: &Change) -> Result<()> {
    let parent = target.checkout.parent().context("checkout has no parent directory")?;
    fs::create_dir_all(parent)?;
    let lock = File::create(parent.join(".machine-record.lock"))?;
    lock.lock()?;
    if !target.checkout.join(".git").exists() {
        let status = Command::new("git").args(["clone", "--quiet", &target.remote]).arg(&target.checkout).status()?;
        if !status.success() {
            bail!("cloning {} failed", target.remote);
        }
        git(&target.checkout, &["config", "credential.https://github.com.helper", "!gh auth git-credential"])?;
    }
    git(&target.checkout, &["pull", "--quiet", "--rebase", "--autostash", "origin", "main"])?;
    edit(&target.checkout.join("packages"), change)?;
    git(&target.checkout, &["add", "packages"])?;
    if git(&target.checkout, &["status", "--porcelain", "packages"])?.trim().is_empty() {
        return Ok(());
    }
    let (verb, preposition) = match change.action {
        Action::Add => ("Add", "to"),
        Action::Remove => ("Remove", "from"),
    };
    let message = format!("{verb} {} {preposition} {} packages", change.specs.join(" "), change.manager.name());
    git(&target.checkout, &["commit", "--quiet", "-m", &message])?;
    git(&target.checkout, &["push", "--quiet", "origin", "HEAD:main"])?;
    Ok(())
}
