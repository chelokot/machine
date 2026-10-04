use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::record::Manifest;

pub const DCONF: &str = "home/dconf";
pub const EXTENSIONS: &str = "home/gnome-extensions.txt";
pub const FLATPAKS: &str = "home/flatpaks.txt";
pub const CAPTURED: [&str; 3] = [DCONF, EXTENSIONS, FLATPAKS];
const CONTAINER_APPS: &str = "home/container-apps.txt";
const CONTAINER: &str = "fedora-toolbox";
const EXPORT_PREFIX: &str = "fedora-toolbox-";
const ICON_CANDIDATES: [&str; 5] = [
    "/usr/share/pixmaps/{}.png",
    "/usr/share/icons/hicolor/512x512/apps/{}.png",
    "/usr/share/icons/hicolor/256x256/apps/{}.png",
    "/usr/share/icons/hicolor/scalable/apps/{}.svg",
    "/usr/share/pixmaps/{}.svg",
];
const FLATPAK_REMOTES: &str = "home/flatpak-remotes.txt";
const IGNORED_KEYS: &str = "home/dconf/ignored-keys.txt";
const EXTENSIONS_SITE: &str = "https://extensions.gnome.org";

fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("{program} is not available"))?;
    if !output.status.success() {
        bail!("{program} {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn lines(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect()
}

pub fn filter_dump(dump: &str, ignored: &[String]) -> String {
    let is_ignored = |key: &str| {
        ignored
            .iter()
            .any(|pattern| pattern.strip_suffix('*').map_or(key == pattern, |prefix| key.starts_with(prefix)))
    };
    let mut sections: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in dump.lines().filter(|line| !line.trim().is_empty()) {
        if line.starts_with('[') {
            sections.push((line, Vec::new()));
        } else if let Some((_, lines)) = sections.last_mut()
            && !is_ignored(line.split_once('=').map_or(line, |(key, _)| key))
        {
            lines.push(line);
        }
    }
    sections
        .iter()
        .filter(|(_, lines)| !lines.is_empty())
        .map(|(header, lines)| format!("{header}\n{}\n", lines.join("\n")))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn merge(manifest: &Path, observed: &Path, local: &BTreeSet<String>) -> Result<()> {
    let declared = Manifest::read(manifest)?;
    let baseline: Option<BTreeSet<String>> = observed
        .exists()
        .then(|| Manifest::read(observed).map(|previous| previous.entries().iter().cloned().collect()))
        .transpose()?;
    let added: Vec<&String> = local
        .iter()
        .filter(|entry| {
            baseline
                .as_ref()
                .map_or(!declared.entries().contains(entry), |previous| !previous.contains(*entry))
        })
        .collect();
    let removed: Vec<&String> = baseline.iter().flatten().filter(|entry| !local.contains(*entry)).collect();
    declared.write(manifest, &added, &removed)?;
    remember(observed, local)
}

fn remember(observed: &Path, local: &BTreeSet<String>) -> Result<()> {
    fs::create_dir_all(observed.parent().context("state path has no parent")?)?;
    Ok(fs::write(observed, local.iter().map(|line| format!("{line}\n")).collect::<String>())?)
}

fn local_flatpaks() -> Result<BTreeSet<String>> {
    Ok(lines(&run("flatpak", &["list", "--app", "--columns=installation,origin,application"])?))
}

fn local_extensions() -> Result<BTreeSet<String>> {
    Ok(lines(&run("gnome-extensions", &["list", "--user"])?))
}

pub fn capture(root: &Path, state: &Path) -> Result<()> {
    let ignored = Manifest::read(&root.join(IGNORED_KEYS))?.entries().to_vec();
    for entry in fs::read_dir(root.join(DCONF))? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "ini") {
            continue;
        }
        let stem = path.file_stem().context("dconf file has no name")?.to_string_lossy();
        let dump = run("dconf", &["dump", &format!("/{}/", stem.replace('.', "/"))])?;
        fs::write(&path, filter_dump(&dump, &ignored))?;
    }
    merge(&root.join(EXTENSIONS), &state.join("gnome-extensions.txt"), &local_extensions()?)?;
    merge(&root.join(FLATPAKS), &state.join("flatpaks.txt"), &local_flatpaks()?)
}

pub fn apply(root: &Path, state: &Path) -> Result<()> {
    for remote in Manifest::read(&root.join(FLATPAK_REMOTES))?.entries() {
        let fields: Vec<&str> = remote.split_whitespace().collect();
        let [installation, name, url] = fields[..] else {
            bail!("{FLATPAK_REMOTES}: expected `installation name url`, got `{remote}`")
        };
        run("flatpak", &["remote-add", "--if-not-exists", &format!("--{installation}"), name, url])?;
    }
    let installed = local_flatpaks()?;
    for app in Manifest::read(&root.join(FLATPAKS))?.entries().iter().filter(|app| !installed.contains(*app)) {
        let fields: Vec<&str> = app.split_whitespace().collect();
        let [installation, origin, id] = fields[..] else {
            bail!("{FLATPAKS}: expected `installation origin app`, got `{app}`")
        };
        run("flatpak", &["install", "--noninteractive", "-y", &format!("--{installation}"), origin, id])?;
    }
    remember(&state.join("flatpaks.txt"), &local_flatpaks()?)?;

    let present = lines(&run("gnome-extensions", &["list"])?);
    let version = run("gnome-shell", &["--version"])?;
    let major = version
        .split_whitespace()
        .last()
        .and_then(|full| full.split('.').next())
        .context("unexpected gnome-shell --version output")?;
    let directory = tempfile::tempdir()?;
    for uuid in Manifest::read(&root.join(EXTENSIONS))?.entries().iter().filter(|uuid| !present.contains(*uuid)) {
        let info = run(
            "curl",
            &[
                "-fsSL",
                "--get",
                "--data-urlencode",
                &format!("uuid={uuid}"),
                "--data-urlencode",
                &format!("shell_version={major}"),
                &format!("{EXTENSIONS_SITE}/extension-info/"),
            ],
        )?;
        let download = serde_json::from_str::<serde_json::Value>(&info)?["download_url"]
            .as_str()
            .with_context(|| format!("{uuid} has no release for GNOME {major}"))?
            .to_owned();
        let archive = directory.path().join(format!("{uuid}.zip"));
        run("curl", &["-fsSL", "-o", &archive.to_string_lossy(), &format!("{EXTENSIONS_SITE}{download}")])?;
        run("gnome-extensions", &["install", "--force", &archive.to_string_lossy()])?;
    }
    remember(&state.join("gnome-extensions.txt"), &local_extensions()?)
}

pub fn export_container_apps(root: &Path, home: &Path, user: &str) -> Result<()> {
    let applications = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons");
    fs::create_dir_all(&applications)?;
    fs::create_dir_all(&icons)?;
    let apps = Manifest::read(&root.join(CONTAINER_APPS))?.entries().to_vec();
    let launcher = format!("podman exec --user {user} --workdir {} {CONTAINER} ", home.display());
    for app in &apps {
        let desktop_file = format!("/usr/share/applications/{app}.desktop");
        if run("podman", &["exec", CONTAINER, "test", "-f", &desktop_file]).is_err() {
            eprintln!("machine: {app} is not installed in {CONTAINER} yet, skipping its launcher");
            continue;
        }
        let entry = run("podman", &["exec", CONTAINER, "cat", &desktop_file])?;
        let candidates = ICON_CANDIDATES.map(|pattern| pattern.replace("{}", app)).join(" ");
        let found = run(
            "podman",
            &[
                "exec",
                CONTAINER,
                "sh",
                "-c",
                &format!("for path in {candidates}; do [ -f \"$path\" ] && echo \"$path\" && break; done"),
            ],
        )?;
        let icon = match found.trim() {
            "" => None,
            source => {
                let extension = Path::new(source).extension().context("icon has no extension")?.to_string_lossy();
                let target = icons.join(format!("{EXPORT_PREFIX}{app}.{extension}"));
                run("podman", &["cp", &format!("{CONTAINER}:{source}"), &target.to_string_lossy()])?;
                Some(target)
            }
        };
        let exported: String = entry
            .lines()
            .filter(|line| !line.starts_with("TryExec="))
            .map(|line| match (line.strip_prefix("Exec="), line.starts_with("Icon="), &icon) {
                (Some(command), _, _) => format!("Exec={launcher}{command}\n"),
                (None, true, Some(path)) => format!("Icon={}\n", path.display()),
                _ => format!("{line}\n"),
            })
            .collect();
        fs::write(applications.join(format!("{EXPORT_PREFIX}{app}.desktop")), exported)?;
    }
    for entry in fs::read_dir(&applications)? {
        let path = entry?.path();
        let name = path.file_name().context("desktop entry has no name")?.to_string_lossy().into_owned();
        let exported_app = name.strip_prefix(EXPORT_PREFIX).and_then(|rest| rest.strip_suffix(".desktop"));
        if exported_app.is_some_and(|app| !apps.iter().any(|wanted| wanted == app)) {
            fs::remove_file(path)?;
        }
    }
    run("update-desktop-database", &[&applications.to_string_lossy()]).map(drop)
}
