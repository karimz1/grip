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

#[derive(Default)]
struct Arguments {
    path: Option<OsString>,
    view: oflh_tui::StartOptions,
    help: bool,
    version: bool,
}

fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments, CliError> {
    let mut result = Arguments::default();
    let mut positional = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if !positional {
            if argument == "--" {
                positional = true;
                continue;
            }
            if argument == "--help" || argument == "-h" || argument == "-help" {
                result.help = true;
                return Ok(result);
            }
            if argument == "--version" || argument == "-version" {
                result.version = true;
                return Ok(result);
            }
            if argument == "--ports" {
                result.view.ports = true;
                continue;
            }
            if argument == "--here" {
                result.view.ports = true;
                result.view.ports_path_only = true;
                continue;
            }
            if argument == "--port" {
                let port = arguments
                    .next()
                    .and_then(|value| value.to_str().and_then(|text| text.parse::<u16>().ok()))
                    .filter(|port| *port > 0)
                    .ok_or_else(|| {
                        CliError::Usage("--port requires a number from 1 to 65535".into())
                    })?;
                result.view.ports = true;
                result.view.port = Some(port);
                continue;
            }
            if argument.to_string_lossy().starts_with('-') {
                return Err(CliError::Usage(format!(
                    "unknown option: {}",
                    argument.to_string_lossy()
                )));
            }
        }
        if result.path.replace(argument).is_some() {
            return Err(CliError::Usage(
                "expected one path; quote paths containing spaces".into(),
            ));
        }
        positional = true;
    }
    Ok(result)
}

fn run() -> Result<(), CliError> {
    let arguments = parse_arguments(std::env::args_os().skip(1))?;
    if arguments.help {
        println!(
            "oflh — Open File Lock Handle. See what's using your files and ports.
Source: https://github.com/karimz1/open-file-lock-handle

Usage: oflh [OPTIONS] [PATH]

  oflh .                  inspect files in the current directory
  oflh --ports            inspect all visible local port bindings
  oflh --port 3000        find a TCP listener or UDP binding on port 3000
  oflh --here .           ports of processes referencing this directory

No PATH means the current directory. Put options before PATH.

Options:
  --ports       start in the Ports tab (TCP listeners and bound UDP)
  --port PORT   start in Ports with an exact local-port filter
  --here        limit Ports to processes referencing PATH
  --version     print version
  --help        show help"
        );
        return Ok(());
    }
    if arguments.version {
        println!("oflh {VERSION}");
        return Ok(());
    }
    let path = arguments.path;
    let target = oflh_core::Target::new(path.unwrap_or_else(|| OsString::from(".")))?;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CliError::NotInteractive);
    }
    let backend = oflh_platform::native()?;
    oflh_tui::run(target, VERSION.into(), backend, arguments.view)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn port_options_and_literal_paths() {
        let parse = |args: &[&str]| parse_arguments(args.iter().map(OsString::from));
        let args = parse(&["--port", "3000", "--here", "."]).unwrap();
        assert!(args.view.ports && args.view.ports_path_only);
        assert_eq!(args.view.port, Some(3000));
        assert_eq!(args.path, Some(OsString::from(".")));
        for args in [
            &["--port"][..],
            &["--port", "0"],
            &["--port", "65536"],
            &["--port", "abc"],
        ] {
            assert!(parse(args).is_err());
        }
        assert_eq!(
            parse(&["--", "--ports"]).unwrap().path,
            Some(OsString::from("--ports"))
        );
        assert!(!parse(&["3000"]).unwrap().view.ports);
    }
}
