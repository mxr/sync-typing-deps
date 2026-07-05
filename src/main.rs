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

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(args) => match run(&args.dir, &args.config) {
            Ok(true) => {
                println!("updated {}", args.config.display());
                ExitCode::FAILURE // pre-commit convention: exit 1 when file modified
            }
            Ok(false) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("usage: sync-typing-deps [--config <path>] [--dir <path>]");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn string_args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    // parse_args tests

    #[test]
    fn test_parse_args_defaults() {
        let a = parse_args(string_args(&[])).unwrap();
        assert_eq!(a.config, PathBuf::from(".pre-commit-config.yaml"));
        assert_eq!(a.dir, PathBuf::from("."));
    }

    #[rstest]
    #[case("--config")]
    #[case("-c")]
    fn test_parse_args_config_flag(#[case] flag: &str) {
        let a = parse_args(string_args(&[flag, "my.yaml"])).unwrap();
        assert_eq!(a.config, PathBuf::from("my.yaml"));
    }

    #[rstest]
    #[case("--dir")]
    #[case("-d")]
    fn test_parse_args_dir_flag(#[case] flag: &str) {
        let a = parse_args(string_args(&[flag, "/some/path"])).unwrap();
        assert_eq!(a.dir, PathBuf::from("/some/path"));
    }

    #[rstest]
    #[case("--config", "--config requires a value")]
    #[case("--dir", "--dir requires a value")]
    fn test_parse_args_missing_value(#[case] arg: &str, #[case] expected_err: &str) {
        assert_eq!(
            parse_args(string_args(&[arg])).unwrap_err(),
            expected_err
        );
    }

    #[test]
    fn test_parse_args_unknown_arg() {
        assert_eq!(
            parse_args(string_args(&["--unknown"])).unwrap_err(),
            "unknown argument: --unknown"
        );
    }
}
