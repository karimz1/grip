#![forbid(unsafe_code)]
use std::{ffi::OsString, io::IsTerminal, process::ExitCode};
const VERSION: &str = match option_env!("OFLH_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
fn run() -> Result<(), (u8, String)> {
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
                return Err((2, format!("unknown option: {}", arg.to_string_lossy())));
            }
        }
        if path.replace(arg).is_some() {
            return Err((2, "expected one path; quote paths containing spaces".into()));
        }
        positional = true;
    }
    let target = oflh_core::Target::new(path.unwrap_or_else(|| OsString::from(".")))
        .map_err(|e| (1, e.to_string()))?;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err((
            1,
            "an interactive terminal is required; run oflh . in a terminal".into(),
        ));
    }
    let backend = oflh_platform::native().map_err(|e| (1, e.to_string()))?;
    oflh_tui::run(target, VERSION.into(), backend).map_err(|e| (1, e.to_string()))
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, error)) => {
            eprintln!("oflh: {}", oflh_core::safe(&error));
            ExitCode::from(code)
        }
    }
}
