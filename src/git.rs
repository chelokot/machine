use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repo {
    Dev,
    Machine,
}

impl Repo {
    pub fn name(self) -> &'static str {
        match self {
            Repo::Dev => "chelokot/dev",
            Repo::Machine => "chelokot/machine",
        }
    }

    pub fn target(self, home: &Path) -> Target {
        let (checkout_variable, remote_variable, directory) = match self {
            Repo::Dev => ("MACHINE_DEV_REPO", "MACHINE_DEV_REMOTE", "dev"),
            Repo::Machine => ("MACHINE_REPO", "MACHINE_REMOTE", "machine"),
        };
        Target {
            checkout: env::var_os(checkout_variable).map_or_else(|| home.join(".local/share").join(directory), PathBuf::from),
            remote: env::var(remote_variable).unwrap_or_else(|_| format!("https://github.com/{}.git", self.name())),
        }
    }
}

pub struct Target {
    pub checkout: PathBuf,
    pub remote: String,
}

pub struct Checkout {
    pub root: PathBuf,
    _lock: File,
}

impl Checkout {
    pub fn open(target: &Target) -> Result<Checkout> {
        let parent = target.checkout.parent().context("checkout has no parent directory")?;
        fs::create_dir_all(parent)?;
        let lock = File::create(parent.join(format!(".{}.lock", target.checkout.file_name().context("checkout has no name")?.display())))?;
        lock.lock()?;
        if !target.checkout.join(".git").exists() {
            let status = Command::new("git").args(["clone", "--quiet", &target.remote]).arg(&target.checkout).status()?;
            if !status.success() {
                bail!("cloning {} failed", target.remote);
            }
        }
        let checkout = Checkout {
            root: target.checkout.clone(),
            _lock: lock,
        };
        checkout.git(&["config", "credential.https://github.com.helper", "!gh auth git-credential"])?;
        Ok(checkout)
    }

    pub fn git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .context("git is not available")?;
        if !output.status.success() {
            bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn pull(&self) -> Result<()> {
        if let Err(error) = self.git(&["pull", "--quiet", "--rebase", "--autostash", "origin", "main"]) {
            self.git(&["rebase", "--abort"]).ok();
            return Err(error);
        }
        Ok(())
    }

    pub fn commit(&self, paths: &[&str], message: &str) -> Result<bool> {
        let mut add = vec!["add", "--"];
        add.extend(paths);
        self.git(&add)?;
        let mut status = vec!["status", "--porcelain", "--"];
        status.extend(paths);
        if self.git(&status)?.trim().is_empty() {
            return Ok(false);
        }
        self.git(&["commit", "--quiet", "-m", message])?;
        Ok(true)
    }

    pub fn push(&self) -> Result<()> {
        self.git(&["push", "--quiet", "origin", "HEAD:main"]).map(drop)
    }
}
