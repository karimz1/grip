#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use std::{ffi::OsString, io::IsTerminal, process::ExitCode};
const VERSION: &str = match option_env!("OFLH_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Usage(String),
    #[error(transparent)]
    Core(#[from] oflh_core::Error),
    #[error(transparent)]
    Terminal(#[from] std::io::Error),
    #[error("an interactive terminal is required; run oflh . in a terminal")]
    NotInteractive,
}

fn run() -> Result<(), CliError> {
    let mut path = None;
    let mut positional = false;
    for arg in std::env::args_os().skip(1) {
        if !positional {
            if arg == "--" {
                positional = true;
                continue;
            }
            if arg == "--version" || arg == "-version" {
                println!("oflh {VERSION}");
                return Ok(());
            }
            if arg == "--help" || arg == "-h" || arg == "-help" {
                println!(
                    "oflh — Open File Lock Handle. See what's using your files.\nSource: https://github.com/karimz1/open-file-lock-handle\n\nUsage: oflh [PATH]\n\n  oflh .\n  oflh ./build\n  oflh ./foo.dll\n\nNo PATH means the current directory.\n\nOptions:\n  --version  print version\n  --help     show help"
                );
                return Ok(());
            }
            if arg.to_string_lossy().starts_with('-') {
                return Err(CliError::Usage(format!(
                    "unknown option: {}",
                    arg.to_string_lossy()
                )));
            }
        }
        if path.replace(arg).is_some() {
            return Err(CliError::Usage(
                "expected one path; quote paths containing spaces".into(),
            ));
        }
        positional = true;
    }
    let target = oflh_core::Target::new(path.unwrap_or_else(|| OsString::from(".")))?;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CliError::NotInteractive);
    }
    let backend = oflh_platform::native()?;
    oflh_tui::run(target, VERSION.into(), backend)?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let code = if matches!(error, CliError::Usage(_)) {
                2
            } else {
                1
            };
            eprintln!("oflh: {}", oflh_core::safe(&error.to_string()));
            ExitCode::from(code)
        }
    }
}
