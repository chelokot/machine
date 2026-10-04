use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

use crate::desktop::merge;

pub const CARGO: &str = "packages/cargo.txt";
pub const UV: &str = "packages/uv.txt";
pub const GO: &str = "packages/go.txt";
pub const CAPTURED: [&str; 3] = [CARGO, UV, GO];
const CRATES_IO_SOURCES: [&str; 2] = ["(registry+https://github.com/rust-lang/crates.io-index)", "(sparse+https://index.crates.io/)"];
const NIX_STORE: &str = "/nix/store";
const GO_BUILDINFO_MAGIC: &[u8] = b"\xff Go buildinf:";
const GO_BUILDINFO_HEADER: usize = 32;
const GO_BUILDINFO_INLINE: u8 = 0x2;
const GO_MODINFO_SENTINEL: usize = 16;

fn entries(directory: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(directory) {
        Ok(listing) => {
            let mut paths = listing.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>()?;
            paths.sort();
            Ok(paths)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn file_name(path: &Path) -> Result<String> {
    Ok(path.file_name().context("path has no name")?.to_string_lossy().into_owned())
}

pub fn cargo_crates(home: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let path = home.join(".cargo/.crates2.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    let installs: serde_json::Map<String, serde_json::Value> = serde_json::from_value(serde_json::from_str::<serde_json::Value>(&text)?["installs"].clone())
        .with_context(|| format!("{} has no installs", path.display()))?;
    installs
        .iter()
        .filter(|(key, _)| CRATES_IO_SOURCES.iter().any(|source| key.ends_with(source)))
        .map(|(key, install)| {
            let name = key.split_whitespace().next().context("empty cargo install key")?.to_owned();
            let bins = serde_json::from_value(install["bins"].clone()).with_context(|| format!("{key} has no bins"))?;
            Ok((name, bins))
        })
        .collect()
}

pub fn uv_tools(home: &Path) -> Result<BTreeSet<String>> {
    entries(&home.join(".local/share/uv/tools"))?
        .iter()
        .filter(|path| path.is_dir())
        .map(|path| file_name(path))
        .collect()
}

fn uvarint(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0usize;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        value |= usize::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
    }
    None
}

pub fn go_package(binary: &[u8]) -> Option<String> {
    let start = binary.windows(GO_BUILDINFO_MAGIC.len()).position(|window| window == GO_BUILDINFO_MAGIC)?;
    let header = binary.get(start..start + GO_BUILDINFO_HEADER)?;
    if header[15] & GO_BUILDINFO_INLINE == 0 {
        return None;
    }
    let strings = &binary[start + GO_BUILDINFO_HEADER..];
    let (version_length, version_prefix) = uvarint(strings)?;
    let rest = strings.get(version_prefix + version_length..)?;
    let (modinfo_length, modinfo_prefix) = uvarint(rest)?;
    let end = (modinfo_prefix + modinfo_length).checked_sub(GO_MODINFO_SENTINEL)?;
    let modinfo = rest.get(modinfo_prefix + GO_MODINFO_SENTINEL..end)?;
    String::from_utf8_lossy(modinfo)
        .lines()
        .find_map(|line| line.strip_prefix("path\t").map(str::to_owned))
}

pub fn go_packages(home: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let mut packages = BTreeMap::new();
    for binary in entries(&home.join("go/bin"))? {
        if let Some(package) = go_package(&fs::read(&binary)?) {
            packages.insert(binary, package);
        }
    }
    Ok(packages)
}

pub fn capture(root: &Path, home: &Path, state: &Path) -> Result<()> {
    merge(&root.join(CARGO), &state.join("cargo.txt"), &cargo_crates(home)?.into_keys().collect())?;
    merge(&root.join(UV), &state.join("uv.txt"), &uv_tools(home)?)?;
    merge(&root.join(GO), &state.join("go.txt"), &go_packages(home)?.into_values().collect())
}

fn pip_user_scripts(home: &Path) -> Result<BTreeMap<String, String>> {
    let mut scripts = BTreeMap::new();
    for python in entries(&home.join(".local/lib"))? {
        for distribution in entries(&python.join("site-packages"))? {
            let directory = file_name(&distribution)?;
            let Some(stem) = directory.strip_suffix(".dist-info") else { continue };
            let name = stem.split_once('-').map_or(stem, |(name, _)| name);
            for line in fs::read_to_string(distribution.join("RECORD"))?.lines() {
                if let Some(script) = line.split(',').next().and_then(|path| path.strip_prefix("../../../bin/")) {
                    scripts.insert(script.to_owned(), name.to_owned());
                }
            }
        }
    }
    Ok(scripts)
}

fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normal.pop();
            }
            Component::CurDir => {}
            other => normal.push(other),
        }
    }
    normal
}

pub fn undeclared(home: &Path, search_path: &str) -> Result<Vec<String>> {
    let shown = |path: &Path| match path.strip_prefix(home) {
        Ok(relative) => format!("~/{}", relative.display()),
        Err(_) => path.display().to_string(),
    };
    let crate_bins: BTreeSet<PathBuf> = cargo_crates(home)?
        .into_values()
        .flatten()
        .map(|bin| home.join(".cargo/bin").join(bin))
        .collect();
    let go_bins = go_packages(home)?;
    let recorded_roots = [
        home.join(".local/share/uv/tools"),
        home.join(".local/share/pipx"),
        home.join(".bun/install/global"),
    ];
    let opt = home.join(".local/opt");
    let pip_scripts = pip_user_scripts(home)?;
    let local_bin = home.join(".local/bin");

    let mut directories: Vec<PathBuf> = Vec::new();
    for directory in search_path.split(':').map(PathBuf::from) {
        let managed = directory.starts_with(home.join(".nix-profile")) || directory.starts_with(home.join(".var"));
        if directory.starts_with(home) && !managed && !directories.contains(&directory) {
            directories.push(directory);
        }
    }

    let mut programs: BTreeMap<(u64, u64), (PathBuf, Vec<String>)> = BTreeMap::new();
    let mut links = Vec::new();
    let mut pip: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for directory in &directories {
        for path in entries(directory)? {
            let metadata = fs::symlink_metadata(&path)?;
            let name = file_name(&path)?;
            if metadata.is_dir() || crate_bins.contains(&path) || go_bins.contains_key(&path) {
                continue;
            }
            if metadata.is_symlink() {
                links.push(path);
                continue;
            }
            match pip_scripts.get(&name) {
                Some(distribution) if *directory == local_bin => pip.entry(distribution.clone()).or_default().push(name),
                _ => programs
                    .entry((metadata.dev(), metadata.ino()))
                    .or_insert_with(|| (path, Vec::new()))
                    .1
                    .push(name),
            }
        }
    }

    let mut lines = Vec::new();
    let mut opt_links: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for link in links {
        let directory = link.parent().context("link has no directory")?;
        let target = normalize(&directory.join(fs::read_link(&link)?));
        if target.starts_with(NIX_STORE) || recorded_roots.iter().any(|root| target.starts_with(root)) {
            continue;
        }
        let name = file_name(&link)?;
        let resolved = fs::metadata(&link)
            .ok()
            .and_then(|metadata| programs.get_mut(&(metadata.dev(), metadata.ino())));
        match (target.strip_prefix(&opt).ok().and_then(|inside| inside.components().next()), resolved) {
            (Some(installation), _) => opt_links.entry(opt.join(installation)).or_default().push(name),
            (None, Some((program, aliases))) if program.parent() == Some(directory) => aliases.push(name),
            (None, Some((_, aliases))) => aliases.push(shown(&link)),
            (None, None) => lines.push(format!("{} -> {}", shown(&link), shown(&target))),
        }
    }
    lines.extend(programs.into_values().map(|(program, aliases)| match &aliases[1..] {
        [] => shown(&program),
        others => format!("{} ({})", shown(&program), others.join(", ")),
    }));
    lines.sort();
    for installation in entries(&opt)? {
        if fs::read_link(&installation).is_ok_and(|target| target.starts_with(NIX_STORE)) {
            continue;
        }
        lines.push(match opt_links.get(&installation) {
            Some(links) => format!("{} ({})", shown(&installation), links.join(", ")),
            None => shown(&installation),
        });
    }
    for (distribution, scripts) in pip {
        lines.push(format!("pip --user {distribution} ({})", scripts.join(", ")));
    }
    Ok(lines)
}
