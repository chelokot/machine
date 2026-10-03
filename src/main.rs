use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use machine::record::{self, Action, Change, Manager, Target};

const TOOLBOX_REMOTE: &str = "https://github.com/chelokot/fedora-toolbox.git";

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

fn toolbox_target(home: PathBuf) -> Target {
    Target {
        checkout: env::var_os("MACHINE_TOOLBOX_REPO").map_or_else(|| home.join(".local/share/fedora-toolbox"), PathBuf::from),
        remote: env::var("MACHINE_TOOLBOX_REMOTE").unwrap_or_else(|_| TOOLBOX_REMOTE.to_owned()),
    }
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::Add => "add",
        Action::Remove => "remove",
    }
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
        "machine: recording {} {} ({}) -> chelokot/fedora-toolbox",
        action_name(change.action),
        change.specs.join(" "),
        manager.name()
    );
    Ok(())
}

fn apply(change: Change) -> Result<()> {
    let home = PathBuf::from(env::var("HOME")?);
    let result = record::apply(&toolbox_target(home.clone()), &change);
    if let Err(error) = &result {
        let log = home.join(".local/state/machine/record.log");
        fs::create_dir_all(log.parent().context("log path has no parent")?)?;
        writeln!(
            OpenOptions::new().create(true).append(true).open(log)?,
            "{} {} {}: {error:#}",
            change.manager.name(),
            action_name(change.action),
            change.specs.join(" ")
        )?;
    }
    result
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Commands::Record { manager, args } => record(manager, &args),
        Commands::Apply { manager, action, specs } => apply(Change { manager, action, specs }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("machine: {error:#}");
            ExitCode::FAILURE
        }
    }
}
