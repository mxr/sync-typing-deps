use std::path::PathBuf;
use std::process::ExitCode;

use sync_typing_deps::run;

struct Args {
    config: PathBuf,
    dir: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut config: Option<PathBuf> = None;
    let mut dir: Option<PathBuf> = None;

    let mut iter = std::env::args().skip(1);
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
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("usage: sync-typing-deps [--config <path>] [--dir <path>]");
            return ExitCode::FAILURE;
        }
    };

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
