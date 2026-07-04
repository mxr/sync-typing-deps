use std::path::PathBuf;
use std::process::ExitCode;

use sync_typing_deps::run;

#[derive(Debug)]
struct Args {
    config: PathBuf,
    dir: PathBuf,
}

fn parse_args(mut iter: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut config: Option<PathBuf> = None;
    let mut dir: Option<PathBuf> = None;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                let val = iter.next().ok_or("--config requires a value")?;
                config = Some(PathBuf::from(val));
            }
            "--dir" | "-d" => {
                let val = iter.next().ok_or("--dir requires a value")?;
                dir = Some(PathBuf::from(val));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Args {
        config: config.unwrap_or_else(|| PathBuf::from(".pre-commit-config.yaml")),
        dir: dir.unwrap_or_else(|| PathBuf::from(".")),
    })
}

fn run_main(args: Args) -> ExitCode {
    match run(&args.dir, &args.config) {
        Ok(true) => {
            println!("updated {}", args.config.display());
            ExitCode::FAILURE // pre-commit convention: exit 1 when file modified
        }
        Ok(false) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(args) => run_main(args),
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("usage: sync-typing-deps [--config <path>] [--dir <path>]");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    // parse_args tests

    #[test]
    fn test_parse_args_defaults() {
        let a = parse_args(args(&[])).unwrap();
        assert_eq!(a.config, PathBuf::from(".pre-commit-config.yaml"));
        assert_eq!(a.dir, PathBuf::from("."));
    }

    #[test]
    fn test_parse_args_long_config() {
        let a = parse_args(args(&["--config", "my.yaml"])).unwrap();
        assert_eq!(a.config, PathBuf::from("my.yaml"));
    }

    #[test]
    fn test_parse_args_short_config() {
        let a = parse_args(args(&["-c", "my.yaml"])).unwrap();
        assert_eq!(a.config, PathBuf::from("my.yaml"));
    }

    #[test]
    fn test_parse_args_long_dir() {
        let a = parse_args(args(&["--dir", "/some/path"])).unwrap();
        assert_eq!(a.dir, PathBuf::from("/some/path"));
    }

    #[test]
    fn test_parse_args_short_dir() {
        let a = parse_args(args(&["-d", "/some/path"])).unwrap();
        assert_eq!(a.dir, PathBuf::from("/some/path"));
    }

    #[test]
    fn test_parse_args_config_missing_value() {
        assert_eq!(
            parse_args(args(&["--config"])).unwrap_err(),
            "--config requires a value"
        );
    }

    #[test]
    fn test_parse_args_dir_missing_value() {
        assert_eq!(
            parse_args(args(&["--dir"])).unwrap_err(),
            "--dir requires a value"
        );
    }

    #[test]
    fn test_parse_args_unknown_arg() {
        assert_eq!(
            parse_args(args(&["--unknown"])).unwrap_err(),
            "unknown argument: --unknown"
        );
    }

    // run_main tests

    fn write(dir: &TempDir, name: &str, content: &str) {
        fs::write(dir.path().join(name), content).unwrap();
    }

    #[test]
    fn test_run_main_ok_modified() {
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
        let code = run_main(Args {
            config: dir.path().join(".pre-commit-config.yaml"),
            dir: dir.path().to_path_buf(),
        });
        assert_eq!(code, ExitCode::FAILURE);
    }

    #[test]
    fn test_run_main_ok_no_change() {
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
        let code = run_main(Args {
            config: dir.path().join(".pre-commit-config.yaml"),
            dir: dir.path().to_path_buf(),
        });
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn test_run_main_error() {
        let dir = TempDir::new().unwrap();
        // setup.cfg as a directory triggers an IO error in find_deps
        fs::create_dir(dir.path().join("setup.cfg")).unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
        );
        let code = run_main(Args {
            config: dir.path().join(".pre-commit-config.yaml"),
            dir: dir.path().to_path_buf(),
        });
        assert_eq!(code, ExitCode::FAILURE);
    }
}
