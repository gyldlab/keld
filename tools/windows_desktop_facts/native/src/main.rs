#![deny(unsafe_op_in_unsafe_fn)]
//! Standalone read-only Windows desktop feasibility probe for GH1186.
//! This is not a product sampler, observer, identity contract, or Ready admission.

mod args;
mod context;
mod display;
mod error;
mod identity;
mod metadata;
#[cfg(test)]
mod tests;
mod theme;

use args::{Args, Control};
use error::{ProbeError, ProbeResult};
use serde_json::{Value, json};

fn sample(args: Args) -> ProbeResult<Value> {
    match args.control {
        Some(Control::InvalidMonitor) => {
            display::invalid_monitor_control()?;
            Err(ProbeError::unexpected("GetMonitorInfoW"))
        }
        Some(Control::FailedDpi) => {
            display::failed_dpi_control()?;
            Err(ProbeError::unexpected("GetDpiForMonitor"))
        }
        Some(Control::HighContrastSize) => {
            theme::high_contrast(true)?;
            Err(ProbeError::unexpected(
                "SystemParametersInfoW(SPI_GETHIGHCONTRAST)",
            ))
        }
        None => Ok(json!({"desktop": display::sample(args.mode)?, "theme": theme::sample()?})),
    }
}

fn record_failure(output: &mut Value, error: ProbeError) -> std::process::ExitCode {
    output["status"] = json!("error");
    output["error"] = error.json();
    println!("{output}");
    std::process::ExitCode::from(if error.kind == "unexpected-control-success" {
        3
    } else {
        1
    })
}

fn main() -> std::process::ExitCode {
    let mut output = json!({
        "schemaVersion": 1, "probe": "keld-windows-desktop-facts-feasibility",
        "status": "error", "processId": std::process::id(),
        "cleanup": {"ownedWindowsCreated": 0, "ownedNativeHandles": 0, "globalSettingsWritten": false},
    });
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(error) => return record_failure(&mut output, error),
    };
    output["request"] =
        json!({"mode": args.mode.name(), "control": args.control.map(Control::name)});
    output["machine"] = match metadata::sample() {
        Ok(value) => value,
        Err(error) => return record_failure(&mut output, error),
    };
    output["dpiContext"] = match context::configure(args.mode) {
        Ok(value) => value,
        Err(error) => return record_failure(&mut output, error),
    };
    match sample(args) {
        Ok(value) => {
            output["sample"] = value;
            output["status"] = json!("ok");
            println!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => record_failure(&mut output, error),
    }
}
