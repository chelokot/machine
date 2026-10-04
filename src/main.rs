use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use machine::desktop;
use machine::git::{Checkout, Repo};
use machine::record::{self, Action, Change, Manager};

#[derive(Parser)]
#[command(version, about = "Declarative, self-recording Fedora Silverblue workstation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Record a package manager invocation that already succeeded")]
    Record {
        manager: Manager,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(hide = true)]
    Apply { manager: Manager, action: Action, specs: Vec<String> },
    #[command(about = "Commit the current GNOME settings, extensions and flatpaks to the machine repository")]
    Capture,
    #[command(about = "Capture local changes, pull the machine repository and apply it with home-manager")]
    Sync,
    #[command(about = "Clone the machine repository and apply a host configuration for the first time")]
    Bootstrap { host: String },
}

struct Account {
    name: String,
    uid: u32,
    gid: u32,
    home: PathBuf,
}

fn sudo_account() -> Result<Option<Account>> {
    let Ok(name) = env::var("SUDO_USER") else { return Ok(None) };
    if !rustix::process::geteuid().is_root() {
        return Ok(None);
    }
    let passwd = fs::read_to_string("/etc/passwd")?;
    let fields: Vec<&str> = passwd
        .lines()
        .map(|line| line.split(':').collect::<Vec<&str>>())
        .find(|fields| fields.len() == 7 && fields[0] == name)
        .with_context(|| format!("{name} is not in /etc/passwd"))?;
    Ok(Some(Account {
        uid: fields[2].parse()?,
        gid: fields[3].parse()?,
        home: PathBuf::from(fields[5]),
        name,
    }))
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::Add => "add",
        Action::Remove => "remove",
    }
}

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("HOME").context("HOME is not set")?))
}

fn record(manager: Manager, args: &[String]) -> Result<()> {
    let Some(change) = record::parse(manager, args) else { return Ok(()) };
    if env::var("MACHINE_RECORD").is_ok_and(|value| value == "0") {
        return Ok(());
    }
    let account = sudo_account()?;
    if account.is_none() && rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["apply", manager.name(), action_name(change.action)])
        .args(&change.specs)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(account) = &account {
        let runtime = format!("/run/user/{}", account.uid);
        command
            .uid(account.uid)
            .gid(account.gid)
            .current_dir(&account.home)
            .env_clear()
            .envs(env::vars().filter(|(name, _)| name == "PATH" || name.starts_with("MACHINE_")))
            .env("HOME", &account.home)
            .env("USER", &account.name)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={runtime}/bus"));
    }
    command.spawn()?;
    eprintln!(
        "machine: recording {} {} ({}) -> {}",
        action_name(change.action),
        change.specs.join(" "),
        manager.name(),
        manager.repo().name()
    );
    Ok(())
}

fn apply(change: Change) -> Result<()> {
    let home = home()?;
    let result = record::apply(&change.manager.repo().target(&home), &change);
    if let Err(error) = &result {
        let log = home.join(".local/state/machine/record.log");
        fs::create_dir_all(log.parent().context("log path has no parent")?)?;
        let mut file = OpenOptions::new().create(true).append(true).open(log)?;
        writeln!(
            file,
            "{} {} {}: {error:#}",
            change.manager.name(),
            action_name(change.action),
            change.specs.join(" ")
        )?;
    }
    result
}

fn switch(root: &Path, host: &str) -> Result<()> {
    let flake = format!("{}#{}@{host}", root.display(), env::var("USER").context("USER is not set")?);
    let status = Command::new("nix")
        .args([
            "run",
            &format!("{}#home-manager", root.display()),
            "--",
            "switch",
            "-b",
            "backup",
            "--flake",
            &flake,
        ])
        .status()
        .context("nix is not available")?;
    if !status.success() {
        bail!("home-manager switch --flake {flake} failed");
    }
    Ok(())
}

fn state(home: &Path) -> PathBuf {
    home.join(".local/state/machine")
}

fn capture(checkout: &Checkout, home: &Path, host: &str) -> Result<bool> {
    desktop::capture(&checkout.root, &state(home))?;
    checkout.commit(&desktop::CAPTURED, &format!("Capture desktop state of {host}"))
}

fn host(home: &Path) -> Result<String> {
    let path = home.join(".config/machine/host");
    Ok(fs::read_to_string(&path)
        .with_context(|| format!("{} is missing, run machine bootstrap <host> first", path.display()))?
        .trim()
        .to_owned())
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Commands::Record { manager, args } => record(manager, &args),
        Commands::Apply { manager, action, specs } => apply(Change { manager, action, specs }),
        Commands::Capture => home().and_then(|home| {
            let checkout = Checkout::open(&Repo::Machine.target(&home))?;
            capture(&checkout, &home, &host(&home)?)?;
            checkout.pull()?;
            checkout.push()
        }),
        Commands::Sync => home().and_then(|home| {
            let host = host(&home)?;
            let checkout = Checkout::open(&Repo::Machine.target(&home))?;
            capture(&checkout, &home, &host)?;
            checkout.pull()?;
            checkout.push()?;
            switch(&checkout.root, &host)?;
            desktop::apply(&checkout.root, &state(&home))?;
            desktop::export_container_apps(&checkout.root, &home, &env::var("USER")?)
        }),
        Commands::Bootstrap { host } => home().and_then(|home| {
            let checkout = Checkout::open(&Repo::Machine.target(&home))?;
            checkout.pull()?;
            switch(&checkout.root, &host)?;
            desktop::apply(&checkout.root, &state(&home))?;
            desktop::export_container_apps(&checkout.root, &home, &env::var("USER")?)
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("machine: {error:#}");
            ExitCode::FAILURE
        }
    }
}
