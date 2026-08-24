//! keld-host — the shipping host binary (pre-alpha).
//!
//! Destination (spec 01/06): app developers never compile this; `@keld/cli`
//! resolves a signed platform build that boots from the compiled form of
//! `keld.config.ts` and owns every OS resource for the app's lifetime.
//! Today: `--hello` opens the diagnostic hello window; any other invocation
//! prints the pre-alpha banner (no `@keld/cli`, no signed build, no config boot).

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--hello") {
        if let Some(flag) = keld_core::host_hello_unknown_arg(&args) {
            eprintln!(
                "KELD-CLI-044: unknown hello flag `{flag}`. \
                 Use `--hello` and optional `--title <name>`."
            );
            process::exit(2);
        }
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
        "keld-host {} (pre-alpha). Run with --hello for the window slice.",
        keld_core::VERSION
    );
}
