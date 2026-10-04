use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use machine::desktop::{FILES, capture_files};
use machine::tools::{CARGO, GO, UV, capture, go_package, undeclared};

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn build_go_binary(directory: &Path, module: &str) -> Vec<u8> {
    write(&directory.join("go.mod"), &format!("module {module}\n\ngo 1.21\n"));
    write(&directory.join("main.go"), "package main\n\nfunc main() {}\n");
    let output = Command::new("go")
        .args(["build", "-o", "binary", "."])
        .current_dir(directory)
        .env("GOCACHE", directory.join("cache"))
        .env("GOPATH", directory.join("gopath"))
        .env("GOTOOLCHAIN", "local")
        .env("GOFLAGS", "-mod=mod")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::read(directory.join("binary")).unwrap()
}

#[test]
fn reads_the_package_path_of_a_go_binary() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        go_package(&build_go_binary(directory.path(), "example.com/tools/hello")).as_deref(),
        Some("example.com/tools/hello")
    );
    assert_eq!(go_package(b"\x7fELF not a go binary"), None);
}

#[test]
fn captures_crates_io_crates_uv_tools_and_go_binaries() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let repo = root.path().join("repo");
    write(
        &home.join(".cargo/.crates2.json"),
        r#"{"installs":{
            "wasm-pack 0.13.1 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["wasm-pack"]},
            "ripgrep 14.1.1 (sparse+https://index.crates.io/)":{"bins":["rg"]},
            "local-tool 0.1.0 (path+file:///src/local-tool)":{"bins":["local-tool"]}
        }}"#,
    );
    fs::create_dir_all(home.join(".local/share/uv/tools/ruff")).unwrap();
    write(&home.join(".local/share/uv/tools/.lock"), "");
    let go_build = root.path().join("go-build");
    fs::create_dir_all(&go_build).unwrap();
    write(&home.join("go/bin/hello"), "");
    fs::write(home.join("go/bin/hello"), build_go_binary(&go_build, "example.com/tools/hello")).unwrap();
    write(&repo.join(CARGO), "# cargo install\nwasm-pack\n");

    capture(&repo, &home, &root.path().join("state")).unwrap();

    assert_eq!(fs::read_to_string(repo.join(CARGO)).unwrap(), "# cargo install\nripgrep\nwasm-pack\n");
    assert_eq!(fs::read_to_string(repo.join(UV)).unwrap(), "ruff\n");
    assert_eq!(fs::read_to_string(repo.join(GO)).unwrap(), "example.com/tools/hello\n");
}

#[test]
fn reports_only_programs_no_repository_declares() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    write(&home.join(".local/bin/act"), "binary");
    write(&home.join(".local/aws-cli/v2/current/bin/aws"), "binary");
    symlink(home.join(".local/aws-cli/v2/current/bin/aws"), home.join(".local/bin/aws")).unwrap();
    write(&home.join(".local/opt/godot-4.7.2/Godot"), "binary");
    symlink(home.join(".local/opt/godot-4.7.2/Godot"), home.join(".local/bin/godot")).unwrap();
    write(&home.join(".local/opt/blender-4.5/blender"), "binary");
    write(&home.join(".local/share/uv/tools/ruff/bin/ruff"), "binary");
    symlink(home.join(".local/share/uv/tools/ruff/bin/ruff"), home.join(".local/bin/ruff")).unwrap();
    write(&home.join(".local/bin/ttx"), "#!/usr/bin/python");
    write(&home.join(".local/bin/pyftsubset"), "#!/usr/bin/python");
    write(
        &home.join(".local/lib/python3.13/site-packages/fonttools-4.53.1.dist-info/RECORD"),
        "../../../bin/pyftsubset,sha256=a,1\n../../../bin/ttx,sha256=b,2\nfontTools/__init__.py,sha256=c,3\n",
    );
    fs::create_dir_all(home.join(".local/bin/__pycache__")).unwrap();
    write(&home.join(".cargo/bin/rustup"), "binary");
    symlink("rustup", home.join(".cargo/bin/cargo")).unwrap();
    write(&home.join(".elan/bin/elan"), "binary");
    fs::hard_link(home.join(".elan/bin/elan"), home.join(".elan/bin/lean")).unwrap();
    write(&home.join(".bun/bin/bun"), "binary");
    symlink(home.join(".bun/bin/bun"), home.join(".local/bin/bunx")).unwrap();
    write(&home.join(".cargo/bin/wasm-pack"), "binary");
    write(
        &home.join(".cargo/.crates2.json"),
        r#"{"installs":{"wasm-pack 0.13.1 (sparse+https://index.crates.io/)":{"bins":["wasm-pack"]}}}"#,
    );
    write(&home.join(".nix-profile/bin/machine"), "binary");
    fs::create_dir_all(home.join(".local/opt")).unwrap();
    symlink("/nix/store/0000-godot-4.6.3", home.join(".local/opt/godot-4.6.3")).unwrap();
    symlink("/nix/store/0000-home-manager-files/.local/bin/blender", home.join(".local/bin/blender")).unwrap();
    let path = [".local/bin", ".bun/bin", ".cargo/bin", ".elan/bin", ".nix-profile/bin", ".local/bin"]
        .map(|directory| home.join(directory).display().to_string())
        .join(":")
        + ":/usr/bin";

    let shown = |line: &str| line.replace(&home.display().to_string(), "~");
    assert_eq!(
        undeclared(home, &path).unwrap().iter().map(|line| shown(line)).collect::<Vec<_>>(),
        [
            "~/.bun/bin/bun (~/.local/bin/bunx)",
            "~/.cargo/bin/rustup (cargo)",
            "~/.elan/bin/elan (lean)",
            "~/.local/bin/act",
            "~/.local/bin/aws -> ~/.local/aws-cli/v2/current/bin/aws",
            "~/.local/opt/blender-4.5",
            "~/.local/opt/godot-4.7.2 (godot)",
            "pip --user fonttools (pyftsubset, ttx)",
        ]
    );
}

#[test]
fn captures_declared_files_that_exist_locally() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let files = root.path().join("repo").join(FILES);
    write(&files.join(".config/mimeapps.list"), "declared\n");
    write(&files.join(".config/zed/settings.json"), "declared\n");
    write(&home.join(".config/mimeapps.list"), "changed locally\n");
    write(&home.join(".config/unrelated"), "local\n");

    capture_files(&files, &home).unwrap();

    assert_eq!(fs::read_to_string(files.join(".config/mimeapps.list")).unwrap(), "changed locally\n");
    assert_eq!(fs::read_to_string(files.join(".config/zed/settings.json")).unwrap(), "declared\n");
    assert!(!files.join(".config/unrelated").exists());
}
