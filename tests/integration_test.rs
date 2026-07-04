use std::fs;
use std::process::Command;

use tempfile::TempDir;

use sync_typing_deps::run;

fn write(dir: &TempDir, name: &str, content: &str) {
    fs::write(dir.path().join(name), content).unwrap();
}

#[test]
fn test_run_inserts_and_sorts_deps() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "pyproject.toml",
        "[dependency-groups]\ndev = [\"z-dep\", \"a-dep\"]\n",
    );
    write(
        &dir,
        ".pre-commit-config.yaml",
        "# keep this comment\nrepos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
    );

    let updated = run(dir.path(), &dir.path().join(".pre-commit-config.yaml")).unwrap();
    assert!(updated);

    let content = fs::read_to_string(dir.path().join(".pre-commit-config.yaml")).unwrap();
    assert!(
        content.starts_with("# keep this comment\n"),
        "comment preserved"
    );
    let a_pos = content.find("a-dep").unwrap();
    let z_pos = content.find("z-dep").unwrap();
    assert!(a_pos < z_pos, "deps sorted");
}

#[test]
fn test_run_no_matching_hooks() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "pyproject.toml",
        "[dependency-groups]\ndev = [\"mypy>=1.0\"]\n",
    );
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/pre-commit-hooks\n  rev: v4.5.0\n  hooks:\n  - id: trailing-whitespace\n",
    );

    let updated = run(dir.path(), &dir.path().join(".pre-commit-config.yaml")).unwrap();
    assert!(!updated);
}

#[test]
fn test_run_no_dep_files() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - old\n",
    );

    let updated = run(dir.path(), &dir.path().join(".pre-commit-config.yaml")).unwrap();
    // No pyproject.toml/setup.cfg → deps=[] → replaces old dep with empty list → changed.
    assert!(updated);
    let content = fs::read_to_string(dir.path().join(".pre-commit-config.yaml")).unwrap();
    assert!(!content.contains("old"));
}

#[test]
fn test_run_find_deps_error() {
    // setup.cfg exists as a directory → find_deps fails → run propagates the error.
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("setup.cfg")).unwrap();
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
    );
    assert!(run(dir.path(), &dir.path().join(".pre-commit-config.yaml")).is_err());
}

#[test]
fn test_run_idempotent() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "pyproject.toml",
        "[dependency-groups]\ndev = [\"mypy>=1.0\"]\n",
    );
    // Config already has the correct sorted dep.
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - mypy>=1.0\n",
    );

    let updated = run(dir.path(), &dir.path().join(".pre-commit-config.yaml")).unwrap();
    assert!(!updated, "second run should be a no-op");
}

// Binary invocation tests – these cover fn main() paths.

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sync-typing-deps"))
}

#[test]
fn test_main_unknown_arg_exits_failure() {
    let status = bin().arg("--unknown").status().unwrap();
    assert!(!status.success());
}

#[test]
fn test_main_file_modified_exits_failure() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "pyproject.toml",
        "[dependency-groups]\ndev = [\"mypy>=1.0\"]\n",
    );
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
    );
    let status = bin()
        .arg("--dir")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".pre-commit-config.yaml"))
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn test_main_no_change_exits_success() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "pyproject.toml",
        "[dependency-groups]\ndev = [\"mypy>=1.0\"]\n",
    );
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - mypy>=1.0\n",
    );
    let status = bin()
        .arg("--dir")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".pre-commit-config.yaml"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn test_main_run_error_exits_failure() {
    let dir = TempDir::new().unwrap();
    // setup.cfg as a directory triggers an IO error in find_deps
    fs::create_dir(dir.path().join("setup.cfg")).unwrap();
    write(
        &dir,
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
    );
    let status = bin()
        .arg("--dir")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".pre-commit-config.yaml"))
        .status()
        .unwrap();
    assert!(!status.success());
}
