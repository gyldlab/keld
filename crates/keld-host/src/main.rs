//! keld-host — the prebuilt host binary.
//!
//! App developers never compile this; `@keld/cli` resolves a signed platform
//! build. It boots from the compiled form of `keld.config.ts` and owns every
//! OS resource for the lifetime of the app.

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--hello") {
        let cwd = env::current_dir().ok();
        let title = keld_core::resolve_hello_title(&args, cwd.as_deref());
        if let Err(err) = keld_core::run_hello_window_titled(&title) {
            eprintln!("{err}");
            process::exit(1);
        }
        return;
    }

    // Phase 1 (ROADMAP): parse compiled config, start event loop, open window.
    eprintln!(
        "keld-host {} (pre-alpha). Run with --hello for the WKWebView window slice.",
        keld_core::VERSION
    );
}
