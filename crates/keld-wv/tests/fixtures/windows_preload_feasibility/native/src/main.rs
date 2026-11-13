//! Standalone Windows feasibility harness; never a product Host or security contract.
#![deny(unsafe_op_in_unsafe_fn)]

mod config;
mod digest;
mod events;
mod native;
mod pump;
mod resources;
mod runner;
mod scenarios;
mod trace;

use std::path::PathBuf;
use std::process::ExitCode;

fn entry() -> Result<u8, String> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err("usage: keld-w1-native-probe.exe --config <absolute-json-path>".to_owned());
    }
    let path = PathBuf::from(args.next().ok_or("--config needs a path")?);
    if !path.is_absolute() || args.next().is_some() {
        return Err("supply one absolute config path".to_owned());
    }
    runner::run(config::Config::load(&path)?)
}

fn main() -> ExitCode {
    match entry() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("keld-w1-native-probe: {error}");
            ExitCode::FAILURE
        }
    }
}
